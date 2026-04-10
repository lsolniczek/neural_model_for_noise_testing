/// Simulation pipeline: Engine → Auditory → Neural → Score.
///
/// Wires together the noise engine, cochlear filterbank, and neural models
/// into a single evaluation function that the optimizer calls.

use crate::auditory::{GammatoneFilterbank, AssrTransfer, ThalamicGate};
use crate::brain_type::BrainType;
use crate::movement::MovementController;
use crate::neural::{FhnModel, FastInhibParams, PerformanceVector, simulate_bilateral};
use crate::preset::Preset;
use crate::scoring::Goal;
use noise_generator_core::NoiseEngine;

use rustfft::{num_complex::Complex, FftPlanner};

pub(crate) const SAMPLE_RATE: u32 = 48_000;
/// Decimation factor: 48 kHz → 1 kHz for neural models.
pub(crate) const DECIMATION_FACTOR: usize = 48;
/// Neural model sample rate after decimation.
pub(crate) const NEURAL_SR: f64 = SAMPLE_RATE as f64 / DECIMATION_FACTOR as f64;

/// Decimate a signal by averaging blocks of `factor` samples (boxcar anti-alias + downsample).
pub(crate) fn decimate(signal: &[f64], factor: usize) -> Vec<f64> {
    let out_len = signal.len() / factor;
    let inv = 1.0 / factor as f64;
    (0..out_len)
        .map(|i| {
            let start = i * factor;
            signal[start..start + factor].iter().sum::<f64>() * inv
        })
        .collect()
}

/// Deinterleave stereo buffer into separate L/R channels.
pub(crate) fn deinterleave(interleaved: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let num_frames = interleaved.len() / 2;
    let mut left = Vec::with_capacity(num_frames);
    let mut right = Vec::with_capacity(num_frames);
    for i in 0..num_frames {
        left.push(interleaved[i * 2]);
        right.push(interleaved[i * 2 + 1]);
    }
    (left, right)
}

pub struct SimulationConfig {
    /// Duration of audio to render per evaluation (seconds).
    pub duration_secs: f32,
    /// Initial seconds of neural output to discard before analysis.
    /// Allows differential-equation models to settle past startup transients.
    pub warmup_discard_secs: f32,
    /// Brain type profile for neural models.
    pub brain_type: BrainType,
    /// Enable ASSR transfer function between cochlea and cortex.
    pub assr_enabled: bool,
    /// Enable thalamic gate (arousal-dependent filtering).
    pub thalamic_gate_enabled: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        SimulationConfig {
            duration_secs: 12.0,
            warmup_discard_secs: 2.0,
            brain_type: BrainType::Normal,
            assr_enabled: false,
            thalamic_gate_enabled: false,
        }
    }
}

pub struct SimulationResult {
    pub score: f64,
    pub fhn_firing_rate: f64,
    pub fhn_isi_cv: f64,
    pub dominant_freq: f64,
    pub delta_power: f64,
    pub theta_power: f64,
    pub alpha_power: f64,
    pub beta_power: f64,
    pub gamma_power: f64,
    /// Spectral brightness [0, 1] — dark (brown) to bright (white).
    pub brightness: f64,
    /// Energy fraction per tonotopic band [Low, Low-mid, Mid-high, High].
    pub band_energy_fractions: [f64; 4],
    /// Left hemisphere dominant frequency (Hz).
    pub left_dominant_freq: f64,
    /// Right hemisphere dominant frequency (Hz).
    pub right_dominant_freq: f64,
    /// Alpha asymmetry index: (left_alpha - right_alpha) / (left_alpha + right_alpha).
    pub alpha_asymmetry: f64,
    /// Performance vector: entrainment, E/I stability, spectral centroid.
    pub performance: PerformanceVector,
}

/// Compute spectral brightness from audio via FFT.
///
/// Returns a value in [0, 1] where 0 = very dark (all energy < 200 Hz)
/// and 1 = very bright (all energy > 4 kHz). Based on the spectral centroid
/// mapped through the audible range on a log scale.
pub(crate) fn spectral_brightness(audio: &[f32], sample_rate: f64) -> f64 {
    let n = audio.len();
    let fft_len = n.next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);

    let mut buf: Vec<Complex<f64>> = (0..fft_len)
        .map(|i| {
            if i < n {
                Complex::new(audio[i] as f64, 0.0)
            } else {
                Complex::new(0.0, 0.0)
            }
        })
        .collect();
    fft.process(&mut buf);

    let freq_res = sample_rate / fft_len as f64;
    let mut weighted_sum = 0.0_f64;
    let mut total_power = 0.0_f64;

    // Only consider 20 Hz – 20 kHz (audible range)
    let min_bin = (20.0 / freq_res).ceil() as usize;
    let max_bin = ((20000.0 / freq_res).floor() as usize).min(fft_len / 2);

    for bin in min_bin..max_bin {
        let freq = bin as f64 * freq_res;
        let power = buf[bin].norm_sqr();
        weighted_sum += freq * power;
        total_power += power;
    }

    let centroid = if total_power > 0.0 {
        weighted_sum / total_power
    } else {
        500.0
    };

    // Map centroid to [0, 1] on a log scale over 100 Hz – 10 kHz
    let log_low = 100.0_f64.ln();
    let log_high = 10000.0_f64.ln();
    let brightness = ((centroid.max(100.0).ln() - log_low) / (log_high - log_low)).clamp(0.0, 1.0);

    brightness
}

/// Evaluate a preset against a goal.
///
/// This is the core function the optimizer calls for each candidate.
pub fn evaluate_preset(preset: &Preset, goal: &Goal, config: &SimulationConfig) -> SimulationResult {
    let num_frames = (SAMPLE_RATE as f32 * config.duration_secs) as u32;
    let sr = SAMPLE_RATE as f64;

    // 1. Create engine and apply preset
    let engine = NoiseEngine::new(SAMPLE_RATE, 0.8);
    preset.apply_to_engine(&engine);

    // 2. Set up movement controller
    let mut movement = MovementController::from_preset(preset);

    // 3. Render audio with movement updates.
    //    When objects move, we render in small chunks (~50ms) and update
    //    positions between chunks so the HRTF pipeline reflects motion.
    //    For static presets we render in one shot for efficiency.
    let warmup_frames = (SAMPLE_RATE as f32 * 1.0) as u32;
    let chunk_frames = (SAMPLE_RATE as f32 * 0.05) as u32; // 50ms chunks

    if movement.has_movement() {
        // Warmup with movement ticking
        let warmup_chunks = warmup_frames / chunk_frames;
        let dt = chunk_frames as f64 / SAMPLE_RATE as f64;
        for _ in 0..warmup_chunks {
            movement.tick(dt, &engine);
            let _ = engine.render_audio(chunk_frames);
        }
    } else {
        let _ = engine.render_audio(warmup_frames);
    }

    let audio = if movement.has_movement() {
        let dt = chunk_frames as f64 / SAMPLE_RATE as f64;
        let mut all_audio = Vec::with_capacity((num_frames * 2) as usize);
        let mut rendered = 0_u32;
        while rendered < num_frames {
            let this_chunk = chunk_frames.min(num_frames - rendered);
            movement.tick(dt, &engine);
            all_audio.extend_from_slice(&engine.render_audio(this_chunk));
            rendered += this_chunk;
        }
        all_audio
    } else {
        engine.render_audio(num_frames)
    };

    // 3. Deinterleave to L/R
    let (left, right) = deinterleave(&audio);

    // 4. Cochlear model: tonotopic band-grouped processing
    //    Groups 32 gammatone channels into 4 frequency bands, preserving
    //    spectral energy distribution for the cortical model.
    let mut filterbank_l = GammatoneFilterbank::new(sr);
    let mut filterbank_r = GammatoneFilterbank::new(sr);

    let bands_l = filterbank_l.process_to_band_groups(&left);
    let bands_r = filterbank_r.process_to_band_groups(&right);

    // 5. Normalise each ear's band signals to [0, 1] using GLOBAL max.
    //    Per Patterson et al. (1992) and Glasberg & Moore (2002), inter-band
    //    energy ratios carry critical spectral information. Global normalization
    //    preserves these ratios: Brown noise keeps dominant low-band energy,
    //    White noise keeps flat distribution across bands.
    let mut left_bands: [Vec<f64>; 4] = [
        vec![0.0; bands_l.signals[0].len()],
        vec![0.0; bands_l.signals[1].len()],
        vec![0.0; bands_l.signals[2].len()],
        vec![0.0; bands_l.signals[3].len()],
    ];
    let mut right_bands: [Vec<f64>; 4] = [
        vec![0.0; bands_r.signals[0].len()],
        vec![0.0; bands_r.signals[1].len()],
        vec![0.0; bands_r.signals[2].len()],
        vec![0.0; bands_r.signals[3].len()],
    ];

    // Find global max across ALL bands for each ear
    let global_max_l = (0..4)
        .map(|b| bands_l.signals[b].iter().cloned().fold(0.0_f64, f64::max))
        .fold(0.0_f64, f64::max);
    let global_max_r = (0..4)
        .map(|b| bands_r.signals[b].iter().cloned().fold(0.0_f64, f64::max))
        .fold(0.0_f64, f64::max);

    let norm_l = if global_max_l > 1e-10 { 1.0 / global_max_l } else { 1.0 };
    let norm_r = if global_max_r > 1e-10 { 1.0 / global_max_r } else { 1.0 };

    for b in 0..4 {
        left_bands[b] = bands_l.signals[b].iter().map(|x| x * norm_l).collect();
        right_bands[b] = bands_r.signals[b].iter().map(|x| x * norm_r).collect();
    }

    // Average energy fractions for display (the bilateral model uses per-ear fractions)
    let mut energy_fractions = [0.0_f64; 4];
    for b in 0..4 {
        energy_fractions[b] = (bands_l.energy_fractions[b] + bands_r.energy_fractions[b]) * 0.5;
    }
    let ef_sum: f64 = energy_fractions.iter().sum();
    if ef_sum > 1e-30 {
        for ef in &mut energy_fractions {
            *ef /= ef_sum;
        }
    }

    // 5b. Spectral brightness from audio FFT (psychoacoustic complement)
    let brightness = spectral_brightness(&left, sr);

    // 5c. Decimate band signals from 48 kHz → 1 kHz for neural models.
    //     Neural models operate at 0.5–50 Hz; feeding 48 kHz wastes ~98% of
    //     compute on samples that carry no additional information.
    // Discard initial warm-up samples from decimated signals so the neural
    // models only analyse the settled portion of the auditory response.
    let discard_samples = (config.warmup_discard_secs as f64 * NEURAL_SR) as usize;

    let trim = |signal: &[f64]| -> Vec<f64> {
        let dec = decimate(signal, DECIMATION_FACTOR);
        let skip = discard_samples.min(dec.len());
        dec[skip..].to_vec()
    };

    let mut left_bands_dec: [Vec<f64>; 4] = [
        trim(&left_bands[0]),
        trim(&left_bands[1]),
        trim(&left_bands[2]),
        trim(&left_bands[3]),
    ];
    let mut right_bands_dec: [Vec<f64>; 4] = [
        trim(&right_bands[0]),
        trim(&right_bands[1]),
        trim(&right_bands[2]),
        trim(&right_bands[3]),
    ];

    // 5d. (Optional) ASSR: compute input_scale modifier from preset's modulation frequencies.
    //     Modulation at 40 Hz reaches cortex strongly → full input_scale.
    //     Modulation at 5 Hz barely reaches cortex → reduced input_scale.
    let assr_scale_modifier = if config.assr_enabled {
        let assr = AssrTransfer::new();
        assr.compute_input_scale_modifier(preset)
    } else {
        1.0
    };

    // 5e. (Optional) Thalamic gate — modulates cortical operating point.
    //     Dark, reverberant, gentle presets → low arousal → lower input_offset
    //     → JR model shifts toward bifurcation → theta/delta possible.
    // 5e. (Optional) Thalamic gate — compute per-band offset shifts.
    //     Per Steriade et al. (1993): thalamic burst mode is frequency-selective.
    //     Low bands (delta/theta) shift fully toward bifurcation.
    //     High bands (beta/gamma) stay in tonic mode for fast rhythms.
    let thalamic_band_shifts = if config.thalamic_gate_enabled {
        let arousal = ThalamicGate::compute_arousal(preset, brightness);
        let gate = ThalamicGate::new(arousal);
        gate.band_offset_shifts()
    } else {
        [0.0; 4]
    };

    // 6. Bilateral cortical model: 2×4 parallel Jansen-Rit models
    //    Left hemisphere (fast, α/β) ← mainly right ear (contralateral)
    //    Right hemisphere (slow, δ/θ) ← mainly left ear (contralateral)
    //    Coupled through corpus callosum with ~10ms delay, ~10% strength.
    let neural_params = config.brain_type.params();
    let mut bilateral = config.brain_type.bilateral_params();
    // Apply thalamic gate: per-band offset shifts toward bifurcation at low arousal.
    for b in 0..4 {
        if thalamic_band_shifts[b].abs() > 1e-10 {
            bilateral.left.band_offsets[b] += thalamic_band_shifts[b];
            bilateral.right.band_offsets[b] += thalamic_band_shifts[b];
        }
    }

    let fast_inhib = FastInhibParams {
        g_fast_gain: neural_params.jansen_rit.g_fast_gain,
        g_fast_rate: neural_params.jansen_rit.g_fast_rate,
        c5: neural_params.jansen_rit.c5,
        c6: neural_params.jansen_rit.c6,
        c7: neural_params.jansen_rit.c7,
    };

    let effective_input_scale = neural_params.jansen_rit.input_scale * assr_scale_modifier;

    let bi_result = simulate_bilateral(
        &left_bands_dec,
        &right_bands_dec,
        &bands_l.energy_fractions,
        &bands_r.energy_fractions,
        &bilateral,
        neural_params.jansen_rit.c,
        effective_input_scale,
        NEURAL_SR,
        &fast_inhib,
        neural_params.jansen_rit.v0,
    );

    let jr_result = &bi_result.combined;

    // 7. FHN: single-neuron driven by combined bilateral EEG oscillations.
    //    Uses the same decimated sample rate as the JR output.
    let fhn = FhnModel::with_params(
        NEURAL_SR,
        neural_params.fhn.a,
        neural_params.fhn.b,
        neural_params.fhn.epsilon,
        neural_params.fhn.time_scale,
    );

    // Scale EEG for FHN input using percentile-based normalization.
    // Per FitzHugh (1961) and Izhikevich (2003), neuron firing rate depends
    // monotonically on input current amplitude. Max-normalization destroys
    // this by collapsing all amplitudes to [-1,1]. Percentile scaling
    // preserves relative amplitude: strong EEG → higher current → more spikes.
    let fhn_input: Vec<f64> = {
        let mut abs_values: Vec<f64> = jr_result.eeg.iter().map(|x| x.abs()).collect();
        abs_values.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let p95_idx = (abs_values.len() as f64 * 0.95) as usize;
        let p95 = abs_values[p95_idx.min(abs_values.len() - 1)];
        let scale = if p95 > 1e-10 { 1.0 / p95 } else { 1.0 };
        jr_result.eeg.iter().map(|x| (x * scale).clamp(-3.0, 3.0)).collect()
    };
    let fhn_result = fhn.simulate(&fhn_input, neural_params.fhn.input_scale);

    // 8. Performance Vector — diagnostic metrics for preset evaluation.
    //    Extract NeuralLFO target frequency from the preset (kind=4, param_a=freq).
    let target_lfo_freq = preset.objects.iter()
        .flat_map(|obj| [&obj.bass_mod, &obj.satellite_mod])
        .filter(|m| m.kind == 4 && m.param_a > 0.5) // NeuralLfo with freq > 0.5 Hz
        .map(|m| m.param_a as f64)
        .next(); // Use first active NeuralLFO frequency

    // Detrend EEG for spectral analysis
    let eeg_mean = jr_result.eeg.iter().sum::<f64>() / jr_result.eeg.len() as f64;
    let eeg_detrended: Vec<f64> = jr_result.eeg.iter().map(|x| x - eeg_mean).collect();

    let performance = PerformanceVector::compute(
        &eeg_detrended,
        &jr_result.fast_inhib_trace,
        NEURAL_SR,
        target_lfo_freq,
    );

    // 9. Score (with reduced brightness modifier — neural model now does most work)
    let score = goal.evaluate_with_brightness(&fhn_result, jr_result, brightness);
    let norm_bands = jr_result.band_powers.normalized();

    SimulationResult {
        score,
        fhn_firing_rate: fhn_result.firing_rate,
        fhn_isi_cv: fhn_result.isi_cv,
        dominant_freq: jr_result.dominant_freq,
        delta_power: norm_bands.delta,
        theta_power: norm_bands.theta,
        alpha_power: norm_bands.alpha,
        beta_power: norm_bands.beta,
        gamma_power: norm_bands.gamma,
        brightness,
        band_energy_fractions: energy_fractions,
        left_dominant_freq: bi_result.left_dominant_freq,
        right_dominant_freq: bi_result.right_dominant_freq,
        alpha_asymmetry: bi_result.alpha_asymmetry,
        performance,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    // ---------------------------------------------------------------
    // Constants
    // ---------------------------------------------------------------

    #[test]
    fn neural_sr_is_1000() {
        assert_eq!(NEURAL_SR, 1000.0);
        assert_eq!(SAMPLE_RATE as f64 / DECIMATION_FACTOR as f64, 1000.0);
    }

    // ---------------------------------------------------------------
    // decimate
    // ---------------------------------------------------------------

    #[test]
    fn decimate_constant_signal_unchanged() {
        let signal = vec![3.0; 480]; // 480 samples / 48 = 10 output
        let dec = decimate(&signal, 48);
        assert_eq!(dec.len(), 10);
        for &v in &dec {
            assert!((v - 3.0).abs() < 1e-12, "Constant signal should decimate to same value");
        }
    }

    #[test]
    fn decimate_averages_blocks() {
        // Block of [0, 1, 2, 3] averaged = 1.5
        let signal: Vec<f64> = (0..8).map(|i| i as f64).collect();
        let dec = decimate(&signal, 4);
        assert_eq!(dec.len(), 2);
        assert!((dec[0] - 1.5).abs() < 1e-12); // (0+1+2+3)/4
        assert!((dec[1] - 5.5).abs() < 1e-12); // (4+5+6+7)/4
    }

    #[test]
    fn decimate_output_length() {
        let signal = vec![0.0; 4800];
        let dec = decimate(&signal, 48);
        assert_eq!(dec.len(), 100); // 4800 / 48
    }

    #[test]
    fn decimate_discards_remainder() {
        // 100 samples / 48 = 2 full blocks (96 samples), 4 remainder discarded
        let signal = vec![1.0; 100];
        let dec = decimate(&signal, 48);
        assert_eq!(dec.len(), 2);
    }

    #[test]
    fn decimate_preserves_low_frequency() {
        // A 10 Hz sine at 48 kHz, decimated to 1 kHz, should still be ~10 Hz
        let n = 48_000; // 1 second
        let signal: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 10.0 * i as f64 / 48_000.0).sin())
            .collect();
        let dec = decimate(&signal, 48);
        assert_eq!(dec.len(), 1000);

        // The decimated signal should still oscillate at ~10 Hz
        // Check it crosses zero multiple times (10 Hz → ~20 crossings/sec)
        let mut crossings = 0;
        for w in dec.windows(2) {
            if w[0] * w[1] < 0.0 {
                crossings += 1;
            }
        }
        assert!(
            crossings >= 15 && crossings <= 25,
            "10 Hz sine should have ~20 zero crossings after decimation, got {crossings}"
        );
    }

    // ---------------------------------------------------------------
    // deinterleave
    // ---------------------------------------------------------------

    #[test]
    fn deinterleave_splits_correctly() {
        let interleaved = vec![1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let (left, right) = deinterleave(&interleaved);
        assert_eq!(left, vec![1.0, 3.0, 5.0]);
        assert_eq!(right, vec![2.0, 4.0, 6.0]);
    }

    #[test]
    fn deinterleave_empty() {
        let (left, right) = deinterleave(&[]);
        assert!(left.is_empty());
        assert!(right.is_empty());
    }

    #[test]
    fn deinterleave_output_length() {
        let interleaved = vec![0.0_f32; 200];
        let (left, right) = deinterleave(&interleaved);
        assert_eq!(left.len(), 100);
        assert_eq!(right.len(), 100);
    }

    // ---------------------------------------------------------------
    // spectral_brightness
    // ---------------------------------------------------------------

    #[test]
    fn brightness_dark_for_low_freq_sine() {
        // 100 Hz sine → centroid ≈ 100 Hz → brightness ≈ 0.0
        let n = 48_000;
        let sr = 48_000.0;
        let audio: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 100.0 * i as f64 / sr).sin() as f32)
            .collect();
        let b = spectral_brightness(&audio, sr);
        assert!(b < 0.15, "100 Hz sine should be dark, got brightness={b:.3}");
    }

    #[test]
    fn brightness_bright_for_high_freq_sine() {
        // 8000 Hz sine → centroid ≈ 8000 Hz → brightness ≈ 0.95
        let n = 48_000;
        let sr = 48_000.0;
        let audio: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 8000.0 * i as f64 / sr).sin() as f32)
            .collect();
        let b = spectral_brightness(&audio, sr);
        assert!(b > 0.80, "8 kHz sine should be bright, got brightness={b:.3}");
    }

    #[test]
    fn brightness_in_zero_to_one() {
        let sr = 48_000.0;
        // Test with various signals
        for &freq in &[50.0, 500.0, 5000.0, 15000.0] {
            let n = 48_000;
            let audio: Vec<f32> = (0..n)
                .map(|i| (2.0 * PI * freq * i as f64 / sr).sin() as f32)
                .collect();
            let b = spectral_brightness(&audio, sr);
            assert!(
                b >= 0.0 && b <= 1.0,
                "Brightness should be [0,1] for {freq} Hz, got {b:.3}"
            );
        }
    }

    #[test]
    fn brightness_higher_for_higher_freq() {
        let n = 48_000;
        let sr = 48_000.0;
        let low: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 200.0 * i as f64 / sr).sin() as f32)
            .collect();
        let high: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 5000.0 * i as f64 / sr).sin() as f32)
            .collect();
        let b_low = spectral_brightness(&low, sr);
        let b_high = spectral_brightness(&high, sr);
        assert!(
            b_high > b_low,
            "Higher freq should be brighter: {b_low:.3} vs {b_high:.3}"
        );
    }

    #[test]
    fn brightness_silence_returns_mid_range() {
        let audio = vec![0.0_f32; 48_000];
        let b = spectral_brightness(&audio, 48_000.0);
        // Default centroid 500 Hz → brightness ≈ 0.35
        assert!(
            b >= 0.0 && b <= 1.0,
            "Silence brightness should be in [0,1], got {b:.3}"
        );
    }

    // ---------------------------------------------------------------
    // SimulationConfig defaults
    // ---------------------------------------------------------------

    #[test]
    fn default_config_values() {
        let config = SimulationConfig::default();
        assert_eq!(config.duration_secs, 12.0);
        assert_eq!(config.warmup_discard_secs, 2.0);
        assert_eq!(config.brain_type, BrainType::Normal);
    }

    #[test]
    fn warmup_discard_samples_count() {
        let config = SimulationConfig::default();
        let discard = (config.warmup_discard_secs as f64 * NEURAL_SR) as usize;
        assert_eq!(discard, 2000); // 2s × 1000 Hz
    }

    // ---------------------------------------------------------------
    // Pipeline data flow: signal lengths
    // ---------------------------------------------------------------

    #[test]
    fn decimation_then_trim_length() {
        // 12 seconds at 48 kHz = 576000 samples
        // Decimated by 48 = 12000 samples at 1 kHz
        // Discard 2000 (2s warmup) = 10000 samples
        let n = 576_000;
        let signal = vec![0.5; n];
        let dec = decimate(&signal, DECIMATION_FACTOR);
        assert_eq!(dec.len(), 12_000);

        let discard = 2000_usize;
        let trimmed = &dec[discard..];
        assert_eq!(trimmed.len(), 10_000);
    }
}
