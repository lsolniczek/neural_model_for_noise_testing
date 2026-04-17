/// Room impulse response (RIR) for pre-neural environment processing.
///
/// Applies frequency-dependent reverberation to audio BEFORE the gammatone
/// filterbank, matching the physical signal chain: source → room → ear → cochlea.
///
/// The RIR is a synthetic exponential decay with frequency-dependent RT60:
/// - Low frequencies (< 500 Hz): longer decay (rooms are more reverberant in bass)
/// - High frequencies (> 2 kHz): shorter decay (air absorption attenuates highs)
/// Per Kuttruff (2009) "Room Acoustics" 5th ed.
///
/// Evidence for pre-neural placement:
/// - Devore et al. (2009): auditory nerve encodes reverberant signal faithfully
/// - Fujihira & Shiraishi (2015): ASSR reduced under reverberation
/// - Bidelman & Krishnan (2010): brainstem FFR degraded by reverb
/// - Carney et al. (2015): computational models always place room before cochlea

use rustfft::{num_complex::Complex, FftPlanner};

/// Environment parameters for RIR generation.
/// RT60 values in seconds per frequency band.
#[derive(Debug, Clone, Copy)]
pub struct EnvironmentParams {
    /// RT60 at low frequencies (< 500 Hz).
    pub rt60_low: f64,
    /// RT60 at mid frequencies (500–2000 Hz).
    pub rt60_mid: f64,
    /// RT60 at high frequencies (> 2000 Hz).
    pub rt60_high: f64,
    /// Wet/dry mix ratio [0, 1]. 0 = fully dry, 1 = fully wet.
    pub wet_mix: f64,
}

impl EnvironmentParams {
    /// Return parameters for a given environment index (0–4).
    /// Environment 0 (Anechoic) returns zero RT60 → passthrough.
    ///
    /// RT60 values derived from:
    /// - Kuttruff (2009): typical room acoustics measurements
    /// - Sato & Bradley (2008): classroom/office RT60 ranges
    /// - Pätynen et al. (2014): concert hall / cathedral measurements
    pub fn from_index(env: u8) -> Self {
        match env {
            0 => EnvironmentParams {
                // AnechoicChamber: no reverb (passthrough)
                rt60_low: 0.0,
                rt60_mid: 0.0,
                rt60_high: 0.0,
                wet_mix: 0.0,
            },
            1 => EnvironmentParams {
                // FocusRoom: small treated room, short reverb
                rt60_low: 0.35,
                rt60_mid: 0.25,
                rt60_high: 0.15,
                wet_mix: 0.15,
            },
            2 => EnvironmentParams {
                // OpenLounge: open plan office / lounge
                rt60_low: 0.90,
                rt60_mid: 0.70,
                rt60_high: 0.40,
                wet_mix: 0.25,
            },
            3 => EnvironmentParams {
                // VastSpace: large hall
                rt60_low: 2.50,
                rt60_mid: 2.00,
                rt60_high: 1.20,
                wet_mix: 0.40,
            },
            _ => EnvironmentParams {
                // DeepSanctuary: cathedral-like, maximum reverb
                rt60_low: 5.00,
                rt60_mid: 4.00,
                rt60_high: 2.00,
                wet_mix: 0.55,
            },
        }
    }

    /// Returns true if this environment has no reverb (passthrough).
    pub fn is_anechoic(&self) -> bool {
        self.wet_mix < 1e-10
    }
}

/// Generate a synthetic room impulse response.
///
/// The RIR is an exponentially decaying noise burst with frequency-dependent
/// decay rates. The decay rate at frequency f is:
///   decay_rate(f) = 6.91 / RT60(f)
/// where 6.91 = ln(10^3) (60 dB in natural log).
///
/// The RIR length is determined by the longest RT60 (low freq).
/// Per Kuttruff (2009): h(t) = noise(t) × exp(-decay_rate × t)
/// with frequency-dependent shaping via a 3-band EQ.
pub fn generate_rir(params: &EnvironmentParams, sample_rate: u32) -> Vec<f32> {
    if params.is_anechoic() {
        // Passthrough: single-sample impulse
        return vec![1.0];
    }

    // RIR length: longest RT60 (low freq) determines tail.
    // Cap at 6 seconds to avoid excessive computation while supporting
    // DeepSanctuary's 5s RT60.
    let max_rt60 = params.rt60_low.max(params.rt60_mid).max(params.rt60_high);
    let rir_len = ((max_rt60 * sample_rate as f64) as usize).min(sample_rate as usize * 6);
    if rir_len < 2 {
        return vec![1.0];
    }

    let mut rir = vec![0.0_f32; rir_len];
    let sr = sample_rate as f64;

    // Decay constants per band (6.91 = 3 × ln(10) for 60 dB decay)
    let decay_low = if params.rt60_low > 0.01 { 6.91 / params.rt60_low } else { 100.0 };
    let decay_mid = if params.rt60_mid > 0.01 { 6.91 / params.rt60_mid } else { 100.0 };
    let decay_high = if params.rt60_high > 0.01 { 6.91 / params.rt60_high } else { 100.0 };

    // Generate RIR in frequency domain for frequency-dependent decay.
    // For each time sample, compute the weighted decay across 3 bands.
    // Simple approach: use the mid-band decay for the overall envelope,
    // then apply frequency-dependent EQ via FFT.
    //
    // Step 1: Generate white noise impulse with mid-band exponential decay
    let mut rng_state: u64 = 0xDEAD_BEEF_CAFE_BABEu64;
    for i in 0..rir_len {
        let t = i as f64 / sr;
        // Exponential decay at mid-band rate
        let envelope = (-decay_mid * t).exp();
        // Simple xorshift noise
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        let noise = (rng_state as f64 / u64::MAX as f64) * 2.0 - 1.0;
        rir[i] = (envelope * noise) as f32;
    }

    // Step 2: Apply frequency-dependent decay shaping via FFT.
    // We multiply the RIR spectrum by a filter that adjusts the decay
    // rate per frequency: boost lows (slower decay) and cut highs (faster decay).
    let fft_len = rir_len.next_power_of_two();
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_len);
    let ifft = planner.plan_fft_inverse(fft_len);

    let mut spectrum: Vec<Complex<f64>> = rir
        .iter()
        .map(|&x| Complex::new(x as f64, 0.0))
        .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
        .take(fft_len)
        .collect();

    fft.process(&mut spectrum);

    // Apply frequency-dependent gain adjustment.
    // The ratio of actual decay to mid-band decay determines the boost/cut.
    // At low freq: decay is slower → need to boost (less attenuation)
    // At high freq: decay is faster → need to cut (more attenuation)
    let freq_resolution = sr / fft_len as f64;
    for k in 0..fft_len {
        let freq = if k <= fft_len / 2 {
            k as f64 * freq_resolution
        } else {
            (fft_len - k) as f64 * freq_resolution
        };

        // 3-band crossover: low < 500 Hz, mid 500-2000 Hz, high > 2000 Hz
        let target_decay = if freq < 500.0 {
            decay_low
        } else if freq < 2000.0 {
            // Linear interpolation between low and high
            let t = (freq - 500.0) / 1500.0;
            decay_low * (1.0 - t) + decay_high * t
        } else {
            decay_high
        };

        // Gain adjustment: ratio of target decay to mid-band decay.
        // Slower decay (lower rate) → boost; faster decay → cut.
        // The adjustment is applied as a spectral multiplier.
        // decay_ratio < 1 means slower decay than mid → boost
        // decay_ratio > 1 means faster decay → cut
        let decay_ratio = target_decay / decay_mid.max(1e-10);
        let gain = 1.0 / decay_ratio.max(0.1).min(10.0);
        spectrum[k] *= gain;
    }

    ifft.process(&mut spectrum);

    // Normalize by L2 energy so convolution preserves signal energy.
    // Peak normalization would make the RIR tail sum to enormous energy.
    let scale = 1.0 / fft_len as f64;
    let mut result = vec![0.0_f32; rir_len];
    let energy: f64 = spectrum[..rir_len]
        .iter()
        .map(|c| (c.re * scale).powi(2))
        .sum();
    let norm = if energy > 1e-10 { 1.0 / energy.sqrt() } else { 1.0 };

    for i in 0..rir_len {
        result[i] = (spectrum[i].re * scale * norm) as f32;
    }

    result
}

/// Apply room impulse response to a mono audio channel via FFT convolution.
///
/// Returns the convolved signal, same length as input (truncated, not zero-padded).
/// The wet_mix parameter controls how much of the reverb tail is blended with the dry signal.
pub fn apply_rir(signal: &[f32], rir: &[f32], wet_mix: f64) -> Vec<f32> {
    if rir.len() <= 1 || wet_mix < 1e-10 {
        // Passthrough: no reverb or single-sample impulse
        return signal.to_vec();
    }

    let sig_len = signal.len();
    let fft_len = (sig_len + rir.len() - 1).next_power_of_two();

    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(fft_len);
    let ifft = planner.plan_fft_inverse(fft_len);

    // FFT of signal
    let mut sig_fft: Vec<Complex<f64>> = signal
        .iter()
        .map(|&x| Complex::new(x as f64, 0.0))
        .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
        .take(fft_len)
        .collect();
    fft.process(&mut sig_fft);

    // FFT of RIR
    let mut rir_fft: Vec<Complex<f64>> = rir
        .iter()
        .map(|&x| Complex::new(x as f64, 0.0))
        .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
        .take(fft_len)
        .collect();
    fft.process(&mut rir_fft);

    // Multiply in frequency domain
    for i in 0..fft_len {
        sig_fft[i] *= rir_fft[i];
    }

    // IFFT
    ifft.process(&mut sig_fft);

    let scale = 1.0 / fft_len as f64;
    let dry_mix = 1.0 - wet_mix;

    // Blend wet (convolved) with dry (original), truncate to original length
    let mut result = Vec::with_capacity(sig_len);
    for i in 0..sig_len {
        let wet = (sig_fft[i].re * scale) as f32;
        let dry = signal[i];
        result.push((dry_mix as f32 * dry + wet_mix as f32 * wet));
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: u32 = 48_000;

    #[test]
    fn anechoic_is_passthrough() {
        let params = EnvironmentParams::from_index(0);
        assert!(params.is_anechoic());
        let rir = generate_rir(&params, SR);
        assert_eq!(rir.len(), 1);
        assert_eq!(rir[0], 1.0);
    }

    #[test]
    fn anechoic_apply_preserves_signal() {
        let params = EnvironmentParams::from_index(0);
        let rir = generate_rir(&params, SR);
        let signal: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin()).collect();
        let result = apply_rir(&signal, &rir, params.wet_mix);
        // Should be bitwise identical for anechoic
        for (i, (&a, &b)) in signal.iter().zip(result.iter()).enumerate() {
            assert_eq!(a, b, "Anechoic should preserve signal at sample {i}");
        }
    }

    #[test]
    fn reverberant_changes_signal() {
        let params = EnvironmentParams::from_index(3); // VastSpace
        let rir = generate_rir(&params, SR);
        let signal: Vec<f32> = (0..48000)
            .map(|i| (i as f32 * 0.01).sin() * 0.5)
            .collect();
        let result = apply_rir(&signal, &rir, params.wet_mix);

        // Signal should be different after reverb
        let diff: f64 = signal
            .iter()
            .zip(result.iter())
            .map(|(&a, &b)| (a as f64 - b as f64).abs())
            .sum::<f64>()
            / signal.len() as f64;
        assert!(diff > 1e-4, "VastSpace reverb should change the signal, MAD={diff}");
    }

    #[test]
    fn rir_length_scales_with_rt60() {
        let focus = generate_rir(&EnvironmentParams::from_index(1), SR);
        let vast = generate_rir(&EnvironmentParams::from_index(3), SR);
        let deep = generate_rir(&EnvironmentParams::from_index(4), SR);

        assert!(focus.len() < vast.len(),
            "FocusRoom RIR should be shorter than VastSpace");
        assert!(vast.len() < deep.len(),
            "VastSpace RIR should be shorter than DeepSanctuary");
    }

    #[test]
    fn rir_is_finite() {
        for env in 0..5 {
            let params = EnvironmentParams::from_index(env);
            let rir = generate_rir(&params, SR);
            for (i, &v) in rir.iter().enumerate() {
                assert!(v.is_finite(), "RIR[{i}] non-finite for env={env}: {v}");
            }
        }
    }

    #[test]
    fn convolution_preserves_energy() {
        // Reverb should not amplify or destroy total energy significantly.
        // Allow ±50% because wet/dry mixing redistributes but shouldn't
        // create or destroy energy dramatically.
        let params = EnvironmentParams::from_index(2); // OpenLounge
        let rir = generate_rir(&params, SR);
        let signal: Vec<f32> = (0..48000)
            .map(|i| (i as f32 * 0.002).sin() * 0.3)
            .collect();

        let dry_energy: f64 = signal.iter().map(|&x| (x as f64).powi(2)).sum();
        let result = apply_rir(&signal, &rir, params.wet_mix);
        let wet_energy: f64 = result.iter().map(|&x| (x as f64).powi(2)).sum();

        let ratio = wet_energy / dry_energy.max(1e-10);
        assert!(
            ratio > 0.5 && ratio < 2.0,
            "Energy ratio should be ~1.0, got {ratio:.3} for OpenLounge"
        );
    }

    #[test]
    fn output_length_matches_input() {
        for env in 1..5 {
            let params = EnvironmentParams::from_index(env);
            let rir = generate_rir(&params, SR);
            let signal: Vec<f32> = vec![0.5; 24000];
            let result = apply_rir(&signal, &rir, params.wet_mix);
            assert_eq!(
                result.len(),
                signal.len(),
                "Output length should match input for env={env}"
            );
        }
    }

    #[test]
    fn wet_mix_zero_is_passthrough() {
        let params = EnvironmentParams::from_index(3);
        let rir = generate_rir(&params, SR);
        let signal: Vec<f32> = (0..1000).map(|i| (i as f32 * 0.01).sin()).collect();
        let result = apply_rir(&signal, &rir, 0.0); // wet_mix = 0
        for (i, (&a, &b)) in signal.iter().zip(result.iter()).enumerate() {
            assert_eq!(a, b, "wet_mix=0 should preserve signal at sample {i}");
        }
    }

    #[test]
    fn deeper_environment_has_more_effect() {
        let signal: Vec<f32> = (0..48000)
            .map(|i| (i as f32 * 0.005).sin() * 0.4)
            .collect();

        let mut diffs = Vec::new();
        for env in 1..5 {
            let params = EnvironmentParams::from_index(env);
            let rir = generate_rir(&params, SR);
            let result = apply_rir(&signal, &rir, params.wet_mix);
            let diff: f64 = signal
                .iter()
                .zip(result.iter())
                .map(|(&a, &b)| (a as f64 - b as f64).abs())
                .sum::<f64>()
                / signal.len() as f64;
            diffs.push(diff);
        }

        // Each deeper environment should produce more change
        for i in 1..diffs.len() {
            assert!(
                diffs[i] >= diffs[i - 1] * 0.8, // allow 20% tolerance
                "Deeper environment should produce more effect: env{} diff={:.6} vs env{} diff={:.6}",
                i, diffs[i - 1], i + 1, diffs[i]
            );
        }
    }
}
