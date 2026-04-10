/// Thalamic Gate Model.
///
/// Models the medial geniculate nucleus (MGN) relay between the auditory
/// pathway and cortex. The thalamus acts as a state-dependent filter:
///
///   - High arousal (alert): tonic firing mode → passes fast rhythms (beta/gamma)
///   - Low arousal (relaxed/drowsy): burst firing mode → passes slow rhythms (theta/delta)
///
/// Arousal is derived from measurable preset properties:
///   - Spectral darkness (dark noise → low arousal)
///   - Reverb level (high reverb → diffuse, calming → low arousal)
///   - Modulation gentleness (slow, smooth modulation → low arousal)
///
/// Ref: Hughes SW, Crunelli V (2005). "Thalamic mechanisms of EEG alpha rhythms."
/// Ref: Lopes da Silva FH (1991). "Neural mechanisms underlying brain waves."
/// Ref: Suffczynski P et al. (2004). "Dynamics of non-convulsive epileptic phenomena."

use rustfft::{num_complex::Complex, FftPlanner};

use crate::preset::Preset;

/// Thalamic gate that filters signals based on arousal state.
pub struct ThalamicGate {
    enabled: bool,
    /// Arousal level in [0, 1]. 0 = deep relaxation, 1 = high alertness.
    arousal: f64,
    /// Crossover frequency (Hz) where high-pass and low-pass meet.
    crossover_freq: f64,
    /// Sigmoid steepness for the high/low-pass filters.
    steepness: f64,
}

impl ThalamicGate {
    /// Create a thalamic gate with the given arousal level.
    pub fn new(arousal: f64) -> Self {
        ThalamicGate {
            enabled: true,
            arousal: arousal.clamp(0.0, 1.0),
            crossover_freq: 10.0, // alpha frequency — natural thalamic crossover
            steepness: 5.0,
        }
    }

    /// Create a disabled (passthrough) thalamic gate.
    pub fn disabled() -> Self {
        ThalamicGate {
            enabled: false,
            arousal: 0.5,
            crossover_freq: 10.0,
            steepness: 5.0,
        }
    }

    /// Compute the thalamic gate gain for a given frequency and arousal.
    ///
    /// At high arousal: fast rhythms pass, slow rhythms attenuated.
    /// At low arousal: slow rhythms pass, fast rhythms attenuated.
    /// At mid arousal (0.5): approximately uniform passthrough.
    pub fn gate_gain(&self, freq_hz: f64, arousal: f64) -> f64 {
        if !self.enabled || freq_hz <= 0.0 {
            return 1.0;
        }

        // Sigmoid-based high-pass: passes frequencies above crossover
        let high_pass = 1.0 / (1.0 + (-(freq_hz - self.crossover_freq) / self.steepness).exp());

        // Low-pass: complement of high-pass
        let low_pass = 1.0 - high_pass;

        // Blend based on arousal
        let gain = arousal * high_pass + (1.0 - arousal) * low_pass;

        gain.clamp(0.0, 1.0)
    }

    /// Derive arousal level from preset properties and spectral brightness.
    ///
    /// Dark, reverberant, gently-modulated presets → low arousal.
    /// Bright, dry, aggressively-modulated presets → high arousal.
    pub fn compute_arousal(preset: &Preset, brightness: f64) -> f64 {
        let active_objects: Vec<_> = preset.objects.iter().filter(|o| o.active).collect();
        if active_objects.is_empty() {
            return 0.5;
        }

        // 1. Spectral darkness: bright sound → high arousal
        let brightness_factor = brightness; // [0, 1]

        // 2. Reverb: high reverb → diffuse, calming → low arousal
        let avg_reverb: f64 = active_objects.iter().map(|o| o.reverb_send as f64).sum::<f64>()
            / active_objects.len() as f64;
        let reverb_factor = 1.0 - avg_reverb; // high reverb → low factor → low arousal

        // 3. Modulation speed: fast modulation → high arousal
        //    Check bass_mod and satellite_mod frequencies
        let mut mod_speed_sum = 0.0;
        let mut mod_count = 0;
        for obj in &active_objects {
            // NeuralLfo (kind 4): param_a is frequency
            if obj.bass_mod.kind == 4 {
                mod_speed_sum += (obj.bass_mod.param_a as f64).min(40.0) / 40.0;
                mod_count += 1;
            }
            if obj.satellite_mod.kind == 4 {
                mod_speed_sum += (obj.satellite_mod.param_a as f64).min(40.0) / 40.0;
                mod_count += 1;
            }
            // Stochastic (kind 3): param_a is spike rate
            if obj.bass_mod.kind == 3 {
                mod_speed_sum += (obj.bass_mod.param_a as f64).min(10.0) / 10.0;
                mod_count += 1;
            }
            if obj.satellite_mod.kind == 3 {
                mod_speed_sum += (obj.satellite_mod.param_a as f64).min(10.0) / 10.0;
                mod_count += 1;
            }
            // Breathing (kind 2) and SineLfo (kind 1): slow → low arousal
            if obj.bass_mod.kind == 1 || obj.bass_mod.kind == 2 {
                mod_speed_sum += 0.1; // very slow modulators
                mod_count += 1;
            }
        }
        let mod_factor = if mod_count > 0 {
            mod_speed_sum / mod_count as f64
        } else {
            0.3 // no modulation → moderate-low arousal
        };

        // 4. Movement: fast movement → high arousal
        let avg_movement: f64 = active_objects
            .iter()
            .map(|o| {
                if o.movement.kind == 0 {
                    0.0
                } else {
                    (o.movement.speed as f64).min(5.0) / 5.0
                }
            })
            .sum::<f64>()
            / active_objects.len() as f64;

        // Weighted combination
        let arousal = 0.30 * brightness_factor
            + 0.25 * reverb_factor
            + 0.25 * mod_factor
            + 0.20 * avg_movement;

        arousal.clamp(0.0, 1.0)
    }

    /// Apply the thalamic gate to 4 tonotopic band signals in-place.
    ///
    /// Uses FFT: multiply each frequency bin by the arousal-dependent gate gain.
    pub fn apply(&self, bands: &mut [Vec<f64>; 4], sample_rate: f64) {
        if !self.enabled {
            return;
        }

        let mut planner = FftPlanner::<f64>::new();

        for band in bands.iter_mut() {
            let n = band.len();
            if n == 0 {
                continue;
            }

            let fft_len = n.next_power_of_two();
            let fft_fwd = planner.plan_fft_forward(fft_len);
            let fft_inv = planner.plan_fft_inverse(fft_len);

            let mut buf: Vec<Complex<f64>> = band
                .iter()
                .map(|&v| Complex::new(v, 0.0))
                .chain(std::iter::repeat(Complex::new(0.0, 0.0)).take(fft_len - n))
                .collect();

            fft_fwd.process(&mut buf);

            let freq_res = sample_rate / fft_len as f64;
            for (i, bin) in buf.iter_mut().enumerate() {
                if i == 0 {
                    continue; // skip DC
                }
                let freq = if i <= fft_len / 2 {
                    i as f64 * freq_res
                } else {
                    (fft_len - i) as f64 * freq_res
                };

                let g = self.gate_gain(freq, self.arousal);
                *bin *= g;
            }

            fft_inv.process(&mut buf);

            let inv_n = 1.0 / fft_len as f64;
            for i in 0..n {
                band[i] = buf[i].re * inv_n;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::Preset;
    use std::f64::consts::PI;

    // ═══════════════════════════════════════════════════════════════
    // Gate gain curve tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn high_arousal_passes_fast_rhythms() {
        let gate = ThalamicGate::new(1.0);
        let g_fast = gate.gate_gain(30.0, 1.0);
        let g_slow = gate.gate_gain(3.0, 1.0);
        assert!(
            g_fast > g_slow,
            "High arousal: fast ({g_fast}) should exceed slow ({g_slow})"
        );
        assert!(g_fast > 0.8, "High arousal: 30 Hz gain should be >0.8, got {g_fast}");
    }

    #[test]
    fn low_arousal_passes_slow_rhythms() {
        let gate = ThalamicGate::new(0.0);
        let g_fast = gate.gate_gain(30.0, 0.0);
        let g_slow = gate.gate_gain(3.0, 0.0);
        assert!(
            g_slow > g_fast,
            "Low arousal: slow ({g_slow}) should exceed fast ({g_fast})"
        );
        assert!(g_slow > 0.8, "Low arousal: 3 Hz gain should be >0.8, got {g_slow}");
    }

    #[test]
    fn mid_arousal_balanced() {
        let gate = ThalamicGate::new(0.5);
        let g_fast = gate.gate_gain(30.0, 0.5);
        let g_slow = gate.gate_gain(3.0, 0.5);
        // At mid arousal, both should be moderate (0.3-0.7)
        assert!(
            (g_fast - g_slow).abs() < 0.3,
            "Mid arousal: gains should be similar: fast={g_fast}, slow={g_slow}"
        );
    }

    #[test]
    fn gate_gain_always_bounded() {
        for arousal in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let gate = ThalamicGate::new(arousal);
            for f in 1..=100 {
                let g = gate.gate_gain(f as f64, arousal);
                assert!(
                    g >= 0.0 && g <= 1.0,
                    "Gate gain at {f} Hz, arousal {arousal}: {g} out of [0,1]"
                );
            }
        }
    }

    #[test]
    fn crossover_at_10hz_gives_half() {
        // At the crossover frequency, high_pass ≈ 0.5, low_pass ≈ 0.5
        // So at any arousal, gain should be ~0.5
        let gate = ThalamicGate::new(0.5);
        let g = gate.gate_gain(10.0, 0.5);
        assert!(
            (g - 0.5).abs() < 0.05,
            "At crossover (10 Hz) and mid arousal, gain should be ~0.5, got {g}"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // Disabled/passthrough tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn disabled_gate_gain_is_unity() {
        let gate = ThalamicGate::disabled();
        for f in [1.0, 5.0, 10.0, 40.0] {
            let g = gate.gate_gain(f, 0.5);
            assert_eq!(g, 1.0, "Disabled gate at {f} Hz should be 1.0");
        }
    }

    #[test]
    fn disabled_apply_is_identity() {
        let gate = ThalamicGate::disabled();
        let original = [
            vec![0.5, 0.6, 0.7, 0.8],
            vec![0.1, 0.2, 0.3, 0.4],
            vec![0.9, 0.8, 0.7, 0.6],
            vec![0.3, 0.4, 0.5, 0.6],
        ];
        let mut bands = original.clone();
        gate.apply(&mut bands, 1000.0);
        for (i, (orig, result)) in original.iter().zip(bands.iter()).enumerate() {
            for (j, (&o, &r)) in orig.iter().zip(result.iter()).enumerate() {
                assert_eq!(o, r, "Disabled gate changed band {i} sample {j}");
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Arousal computation tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn arousal_from_dark_reverberant_preset() {
        let mut preset = Preset::default();
        // Dark, high reverb, slow modulation = low arousal
        preset.objects[0].active = true;
        preset.objects[0].color = 2; // Brown
        preset.objects[0].reverb_send = 0.85;
        preset.objects[0].bass_mod.kind = 2; // Breathing (slow)
        preset.objects[0].movement.kind = 0; // Static

        let arousal = ThalamicGate::compute_arousal(&preset, 0.1); // dark
        assert!(
            arousal < 0.35,
            "Dark reverberant preset should give low arousal, got {arousal}"
        );
    }

    #[test]
    fn arousal_from_bright_dry_preset() {
        let mut preset = Preset::default();
        // Bright, low reverb, fast modulation = high arousal
        preset.objects[0].active = true;
        preset.objects[0].color = 0; // White
        preset.objects[0].reverb_send = 0.10;
        preset.objects[0].bass_mod.kind = 4; // NeuralLfo
        preset.objects[0].bass_mod.param_a = 25.0; // fast
        preset.objects[0].satellite_mod.kind = 4;
        preset.objects[0].satellite_mod.param_a = 40.0; // very fast
        preset.objects[0].movement.kind = 2; // Figure-8
        preset.objects[0].movement.speed = 4.0; // Fast

        let arousal = ThalamicGate::compute_arousal(&preset, 0.9); // bright
        assert!(
            arousal > 0.60,
            "Bright dry active preset should give high arousal, got {arousal}"
        );
    }

    #[test]
    fn arousal_always_in_valid_range() {
        // Test with various preset configurations
        for brightness in [0.0, 0.3, 0.5, 0.8, 1.0] {
            let preset = Preset::default();
            let arousal = ThalamicGate::compute_arousal(&preset, brightness);
            assert!(
                arousal >= 0.0 && arousal <= 1.0,
                "Arousal {arousal} out of [0,1] at brightness {brightness}"
            );
        }
    }

    #[test]
    fn arousal_empty_preset() {
        let mut preset = Preset::default();
        for obj in &mut preset.objects {
            obj.active = false;
        }
        let arousal = ThalamicGate::compute_arousal(&preset, 0.5);
        assert_eq!(arousal, 0.5, "Empty preset should give neutral arousal");
    }

    // ═══════════════════════════════════════════════════════════════
    // Signal-level tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn apply_preserves_signal_length() {
        let gate = ThalamicGate::new(0.5);
        let n = 1000;
        let mut bands = [vec![0.5; n], vec![0.5; n], vec![0.5; n], vec![0.5; n]];
        gate.apply(&mut bands, 1000.0);
        for (i, band) in bands.iter().enumerate() {
            assert_eq!(band.len(), n, "Band {i} length changed");
        }
    }

    #[test]
    fn high_arousal_attenuates_slow_signal() {
        let gate = ThalamicGate::new(1.0); // high arousal
        let sr = 1000.0;
        let n = 2000;
        // 3 Hz signal (delta) should be attenuated at high arousal
        let signal: Vec<f64> = (0..n)
            .map(|i| 0.5 + 0.4 * (2.0 * PI * 3.0 * i as f64 / sr).sin())
            .collect();
        let original_var: f64 = {
            let mean = signal.iter().sum::<f64>() / n as f64;
            signal.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64
        };

        let mut bands = [signal, vec![0.5; n], vec![0.5; n], vec![0.5; n]];
        gate.apply(&mut bands, sr);

        let processed_var: f64 = {
            let mean = bands[0].iter().sum::<f64>() / n as f64;
            bands[0].iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64
        };

        assert!(
            processed_var < original_var * 0.5,
            "High arousal should attenuate 3 Hz: var {original_var:.6} → {processed_var:.6}"
        );
    }

    #[test]
    fn low_arousal_attenuates_fast_signal() {
        let gate = ThalamicGate::new(0.0); // low arousal
        let sr = 1000.0;
        let n = 2000;
        // 30 Hz signal (beta) should be attenuated at low arousal
        let signal: Vec<f64> = (0..n)
            .map(|i| 0.5 + 0.4 * (2.0 * PI * 30.0 * i as f64 / sr).sin())
            .collect();
        let original_var: f64 = {
            let mean = signal.iter().sum::<f64>() / n as f64;
            signal.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64
        };

        let mut bands = [signal, vec![0.5; n], vec![0.5; n], vec![0.5; n]];
        gate.apply(&mut bands, sr);

        let processed_var: f64 = {
            let mean = bands[0].iter().sum::<f64>() / n as f64;
            bands[0].iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64
        };

        assert!(
            processed_var < original_var * 0.5,
            "Low arousal should attenuate 30 Hz: var {original_var:.6} → {processed_var:.6}"
        );
    }

    #[test]
    fn apply_outputs_are_finite() {
        let gate = ThalamicGate::new(0.3);
        let n = 1000;
        let mut bands = [
            (0..n).map(|i| (i as f64 * 0.01).sin()).collect(),
            (0..n).map(|i| (i as f64 * 0.03).sin()).collect(),
            (0..n).map(|i| (i as f64 * 0.05).sin()).collect(),
            (0..n).map(|i| (i as f64 * 0.07).sin()).collect(),
        ];
        gate.apply(&mut bands, 1000.0);
        for (bi, band) in bands.iter().enumerate() {
            for (si, &v) in band.iter().enumerate() {
                assert!(v.is_finite(), "Band {bi} sample {si} is not finite: {v}");
            }
        }
    }

    #[test]
    fn apply_empty_bands_no_panic() {
        let gate = ThalamicGate::new(0.5);
        let mut bands = [vec![], vec![], vec![], vec![]];
        gate.apply(&mut bands, 1000.0);
    }
}
