/// Auditory Steady-State Response (ASSR) Transfer Function.
///
/// Models the frequency-dependent cortical response to amplitude-modulated
/// sound. Based on empirical ASSR literature:
///   - Peak response at ~40 Hz (Galambos et al. 1981)
///   - Secondary peak near 10 Hz (alpha ASSR)
///   - Weak response below 4 Hz (subcortical filtering)
///   - Roll-off above 50 Hz
///
/// Ref: Picton TW et al. (2003). "Human auditory steady-state responses."
///      Int J Audiol 42(4):177-219.
/// Ref: Ross B et al. (2000). "A high-precision MEG study of human ASSR."
///      J Acoust Soc Am 108(2):679-691.
///
/// The transfer function is applied in the frequency domain to the decimated
/// band signals (1 kHz sample rate) between the cochlear filterbank and the
/// cortical neural models.
use rustfft::{num_complex::Complex, FftPlanner};
use serde::{Deserialize, Serialize};

use crate::preset::Preset;

/// Stage 2 diagnostic-only ASSR observability bundle.
///
/// These fields are not score inputs. They expose the model's current
/// assumptions about modulation-rate transmission and timing precision.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AssrDiagnostics {
    pub assr_enabled: bool,
    pub dominant_modulation_hz: Option<f64>,
    pub effective_amplitude_gain: Option<f64>,
    pub phase_consistency_heuristic: Option<f64>,
    pub implied_latency_jitter_ms_heuristic: Option<f64>,
    pub expected_plv_ceiling: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssrModulationSummary {
    pub dominant_modulation_hz: Option<f64>,
    pub effective_amplitude_gain: Option<f64>,
}

/// Expected PLV ceiling under a Gaussian latency-jitter model.
///
/// PLV_ceiling = exp(-0.5 * (2π f σ_t)^2), where σ_t is jitter std in seconds.
pub fn expected_plv_ceiling_from_latency_jitter(modulation_hz: f64, latency_jitter_ms: f64) -> f64 {
    if !modulation_hz.is_finite()
        || !latency_jitter_ms.is_finite()
        || modulation_hz <= 0.0
        || latency_jitter_ms < 0.0
    {
        return 0.0;
    }
    let sigma_t = latency_jitter_ms * 1e-3;
    let phase_sigma = 2.0 * std::f64::consts::PI * modulation_hz * sigma_t;
    (-0.5 * phase_sigma * phase_sigma).exp().clamp(0.0, 1.0)
}

/// Heuristic latency-jitter curve (Stage 2 diagnostics only).
///
/// Not a calibrated biomarker. This is an explicit modeling prior that can
/// be replaced once empirical calibration data exists.
pub fn implied_latency_jitter_ms_heuristic(modulation_hz: f64) -> f64 {
    if !modulation_hz.is_finite() || modulation_hz <= 0.0 {
        return 0.0;
    }
    let f = modulation_hz.max(0.5);
    // Low modulation rates are modeled as less temporally precise; around
    // 40 Hz the jitter prior tightens.
    (3.5 + 12.0 / (1.0 + (f / 28.0).powf(1.5))).max(0.0)
}

/// Diagnostics wrapper that reports the effective ASSR amplitude path plus
/// heuristic temporal-consistency terms for the current modulation rate.
pub fn diagnostics_for_modulation(
    modulation: AssrModulationSummary,
    assr_enabled: bool,
) -> AssrDiagnostics {
    let freq = match modulation.dominant_modulation_hz {
        Some(f) if f.is_finite() && f > 0.0 => f,
        _ => {
            return AssrDiagnostics {
                assr_enabled,
                dominant_modulation_hz: None,
                effective_amplitude_gain: None,
                phase_consistency_heuristic: None,
                implied_latency_jitter_ms_heuristic: None,
                expected_plv_ceiling: None,
            }
        }
    };

    let effective_amplitude_gain = if assr_enabled {
        modulation.effective_amplitude_gain.or(Some(1.0))
    } else {
        Some(1.0)
    };

    let jitter_ms = if assr_enabled {
        implied_latency_jitter_ms_heuristic(freq)
    } else {
        0.0
    };
    let expected_plv = expected_plv_ceiling_from_latency_jitter(freq, jitter_ms);
    let phase_consistency = if assr_enabled { expected_plv } else { 1.0 };

    AssrDiagnostics {
        assr_enabled,
        dominant_modulation_hz: Some(freq),
        effective_amplitude_gain,
        phase_consistency_heuristic: Some(phase_consistency),
        implied_latency_jitter_ms_heuristic: Some(jitter_ms),
        expected_plv_ceiling: Some(expected_plv),
    }
}

/// ASSR transfer function that attenuates modulation frequencies based on
/// their empirical cortical penetration strength.
pub struct AssrTransfer {
    enabled: bool,
    /// Primary ASSR peak frequency (Hz). Default 40.0.
    peak_freq: f64,
    /// Width of the primary peak (log-Gaussian sigma). Default ~1.2.
    sigma_primary: f64,
    /// Secondary peak frequency (alpha ASSR). Default 10.0.
    secondary_freq: f64,
    /// Width of secondary peak. Default ~0.8.
    sigma_secondary: f64,
    /// Relative strength of secondary peak. Default ~0.45.
    secondary_strength: f64,
    /// Minimum gain floor (even very low frequencies get some throughput).
    min_gain: f64,
}

impl AssrTransfer {
    /// Create a new ASSR transfer function with empirically-derived defaults.
    pub fn new() -> Self {
        AssrTransfer {
            enabled: true,
            peak_freq: 40.0,
            sigma_primary: 1.2,
            secondary_freq: 10.0,
            sigma_secondary: 0.8,
            secondary_strength: 0.45,
            min_gain: 0.05,
        }
    }

    /// Create a disabled (passthrough) ASSR transfer.
    pub fn disabled() -> Self {
        AssrTransfer {
            enabled: false,
            ..Self::new()
        }
    }

    /// Compute the ASSR gain for a given modulation frequency.
    ///
    /// Returns a value in [min_gain, 1.0] representing the fraction of
    /// modulation energy at this frequency that reaches the cortex.
    pub fn gain(&self, freq_hz: f64) -> f64 {
        if !self.enabled || freq_hz <= 0.0 {
            return 1.0;
        }

        let ln_f = freq_hz.ln();

        // Primary peak: log-Gaussian centered at peak_freq
        let ln_peak = self.peak_freq.ln();
        let primary = (-0.5 * ((ln_f - ln_peak) / self.sigma_primary).powi(2)).exp();

        // Secondary peak: log-Gaussian centered at secondary_freq
        let ln_sec = self.secondary_freq.ln();
        let secondary = self.secondary_strength
            * (-0.5 * ((ln_f - ln_sec) / self.sigma_secondary).powi(2)).exp();

        // Combined: max of primary and secondary, floored at min_gain
        let raw = primary.max(secondary);
        raw.max(self.min_gain).min(1.0)
    }

    /// Apply the ASSR transfer function to 4 tonotopic band signals in-place.
    ///
    /// Operates in the frequency domain: FFT each band, multiply each bin
    /// by the ASSR gain at that bin's frequency, inverse FFT.
    ///
    /// `sample_rate` is the neural sample rate (typically 1000 Hz).
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

            // Zero-pad to power-of-two length
            let mut buf: Vec<Complex<f64>> = band
                .iter()
                .map(|&v| Complex::new(v, 0.0))
                .chain(std::iter::repeat(Complex::new(0.0, 0.0)).take(fft_len - n))
                .collect();

            fft_fwd.process(&mut buf);

            // Apply frequency-dependent ASSR gain
            let freq_res = sample_rate / fft_len as f64;
            for (i, bin) in buf.iter_mut().enumerate() {
                let freq = if i <= fft_len / 2 {
                    i as f64 * freq_res
                } else {
                    (fft_len - i) as f64 * freq_res // mirror for negative freqs
                };

                // Skip DC bin
                if i == 0 {
                    continue;
                }

                let g = self.gain(freq);
                *bin *= g;
            }

            fft_inv.process(&mut buf);

            // Normalize inverse FFT and copy back
            let inv_n = 1.0 / fft_len as f64;
            for i in 0..n {
                band[i] = buf[i].re * inv_n;
            }
        }
    }

    fn active_modulators(preset: &Preset) -> Vec<(f64, f64)> {
        let mut mods = Vec::new();
        for obj in &preset.objects {
            if !obj.active {
                continue;
            }
            for modcfg in [&obj.bass_mod, &obj.satellite_mod] {
                if (modcfg.kind == 4 || modcfg.kind == 5) && modcfg.param_a > 0.5 {
                    let freq = modcfg.param_a as f64;
                    let depth = modcfg.param_b as f64;
                    let vol = obj.volume as f64;
                    let weight = depth * vol;
                    if freq.is_finite() && freq > 0.0 && weight.is_finite() && weight > 0.0 {
                        mods.push((freq, weight));
                    }
                }
            }
        }
        mods
    }

    pub fn summarize_preset_modulation(&self, preset: &Preset) -> AssrModulationSummary {
        let modulators = Self::active_modulators(preset);
        if modulators.is_empty() {
            return AssrModulationSummary {
                dominant_modulation_hz: None,
                effective_amplitude_gain: None,
            };
        }

        let mut dominant = None;
        let mut dominant_weight = f64::NEG_INFINITY;
        let mut weighted_gain_sum = 0.0_f64;
        let mut weight_sum = 0.0_f64;

        for (freq, weight) in modulators {
            if weight > dominant_weight {
                dominant_weight = weight;
                dominant = Some(freq);
            }
            weighted_gain_sum += self.gain(freq) * weight;
            weight_sum += weight;
        }

        let effective = if weight_sum > 1e-10 {
            Some((weighted_gain_sum / weight_sum).clamp(0.0, 1.0))
        } else {
            None
        };

        AssrModulationSummary {
            dominant_modulation_hz: dominant,
            effective_amplitude_gain: effective,
        }
    }

    /// Compute an input_scale modifier based on the preset's modulation frequencies.
    ///
    /// Scans all active NeuralLfo modulators, computes ASSR gain at each frequency,
    /// weights by modulation depth, and returns a multiplier for input_scale.
    ///
    /// Returns a value in [min_gain, 1.0]:
    ///   - Preset with 40 Hz NeuralLfo → modifier ~1.0 (full entrainment)
    ///   - Preset with 5 Hz NeuralLfo → modifier ~0.15 (weak entrainment)
    ///   - Preset with no NeuralLfo → modifier = 1.0 (no change)
    ///
    /// The modifier scales how strongly amplitude modulation drives the cortical
    /// model, reflecting the auditory pathway's frequency-dependent transmission.
    pub fn compute_input_scale_modifier(&self, preset: &Preset) -> f64 {
        self.summarize_preset_modulation(preset)
            .effective_amplitude_gain
            .unwrap_or(1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::Preset;
    use std::f64::consts::PI;

    const TOLERANCE: f64 = 0.05;

    // ═══════════════════════════════════════════════════════════════
    // Gain curve shape tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn gain_at_40hz_is_near_unity() {
        let assr = AssrTransfer::new();
        let g = assr.gain(40.0);
        assert!(
            (g - 1.0).abs() < TOLERANCE,
            "Gain at 40 Hz should be ~1.0, got {g}"
        );
    }

    #[test]
    fn gain_at_10hz_moderate() {
        let assr = AssrTransfer::new();
        let g = assr.gain(10.0);
        assert!(
            g > 0.35 && g < 0.55,
            "Gain at 10 Hz should be 0.35-0.55 (secondary peak), got {g}"
        );
    }

    #[test]
    fn gain_at_20hz() {
        let assr = AssrTransfer::new();
        let g = assr.gain(20.0);
        assert!(
            g > 0.30 && g < 0.90,
            "Gain at 20 Hz should be 0.30-0.90 (between two peaks), got {g}"
        );
    }

    #[test]
    fn gain_at_4hz_weak() {
        let assr = AssrTransfer::new();
        let g = assr.gain(4.0);
        assert!(
            g > 0.05 && g < 0.30,
            "Gain at 4 Hz should be 0.05-0.30 (weak ASSR), got {g}"
        );
    }

    #[test]
    fn gain_at_1hz_very_weak() {
        let assr = AssrTransfer::new();
        let g = assr.gain(1.0);
        assert!(
            g >= 0.05 && g < 0.15,
            "Gain at 1 Hz should be near min_gain (~0.05-0.15), got {g}"
        );
    }

    #[test]
    fn gain_monotonic_from_1_to_40hz() {
        let assr = AssrTransfer::new();
        // Between the secondary peak (10 Hz) and primary peak (40 Hz),
        // there may be a dip. But from 15 Hz to 40 Hz should be monotonic.
        let mut prev = assr.gain(15.0);
        for f in (16..=40).map(|f| f as f64) {
            let g = assr.gain(f);
            assert!(
                g >= prev - 0.01, // allow tiny float noise
                "Gain should increase from 15-40 Hz: at {f} Hz, {g} < {prev}"
            );
            prev = g;
        }
    }

    #[test]
    fn gain_above_40hz_decreases() {
        let assr = AssrTransfer::new();
        let g40 = assr.gain(40.0);
        let g60 = assr.gain(60.0);
        let g80 = assr.gain(80.0);
        assert!(
            g60 < g40,
            "Gain at 60 Hz ({g60}) should be less than at 40 Hz ({g40})"
        );
        assert!(
            g80 < g60,
            "Gain at 80 Hz ({g80}) should be less than at 60 Hz ({g60})"
        );
    }

    #[test]
    fn gain_always_in_valid_range() {
        let assr = AssrTransfer::new();
        for f in 1..=100 {
            let g = assr.gain(f as f64);
            assert!(
                g >= 0.0 && g <= 1.0,
                "Gain at {f} Hz = {g}, should be in [0, 1]"
            );
        }
    }

    #[test]
    fn gain_secondary_peak_visible() {
        // The secondary peak at ~10 Hz should create a local maximum
        // compared to neighboring frequencies (5 Hz and 15 Hz dip)
        let assr = AssrTransfer::new();
        let g5 = assr.gain(5.0);
        let g10 = assr.gain(10.0);
        assert!(
            g10 > g5,
            "Secondary peak: gain at 10 Hz ({g10}) should exceed 5 Hz ({g5})"
        );
    }

    #[test]
    fn diagnostics_separate_amplitude_and_phase_fields() {
        let d = diagnostics_for_modulation(
            AssrModulationSummary {
                dominant_modulation_hz: Some(40.0),
                effective_amplitude_gain: Some(AssrTransfer::new().gain(40.0)),
            },
            true,
        );
        assert_eq!(d.dominant_modulation_hz, Some(40.0));
        assert!(d.effective_amplitude_gain.is_some());
        assert!(d.phase_consistency_heuristic.is_some());
        assert!(d.implied_latency_jitter_ms_heuristic.is_some());
        assert!(d.expected_plv_ceiling.is_some());
    }

    #[test]
    fn diagnostics_40hz_has_higher_amplitude_gain_than_low_rate() {
        let low = diagnostics_for_modulation(
            AssrModulationSummary {
                dominant_modulation_hz: Some(4.0),
                effective_amplitude_gain: Some(AssrTransfer::new().gain(4.0)),
            },
            true,
        );
        let high = diagnostics_for_modulation(
            AssrModulationSummary {
                dominant_modulation_hz: Some(40.0),
                effective_amplitude_gain: Some(AssrTransfer::new().gain(40.0)),
            },
            true,
        );
        assert!(
            high.effective_amplitude_gain.unwrap_or(0.0)
                > low.effective_amplitude_gain.unwrap_or(0.0),
            "expected 40 Hz gain ({:?}) > 4 Hz gain ({:?})",
            high.effective_amplitude_gain,
            low.effective_amplitude_gain
        );
    }

    #[test]
    fn plv_ceiling_decreases_with_frequency_for_fixed_jitter() {
        let low = expected_plv_ceiling_from_latency_jitter(5.0, 5.0);
        let high = expected_plv_ceiling_from_latency_jitter(40.0, 5.0);
        assert!(low > high, "expected PLV ceiling to drop with frequency");
    }

    #[test]
    fn plv_ceiling_decreases_with_higher_jitter() {
        let low_jitter = expected_plv_ceiling_from_latency_jitter(40.0, 2.0);
        let high_jitter = expected_plv_ceiling_from_latency_jitter(40.0, 10.0);
        assert!(
            low_jitter > high_jitter,
            "expected PLV ceiling to drop as jitter increases"
        );
    }

    #[test]
    fn diagnostics_are_deterministic_and_finite() {
        let summary = AssrModulationSummary {
            dominant_modulation_hz: Some(18.0),
            effective_amplitude_gain: Some(AssrTransfer::new().gain(18.0)),
        };
        let a = diagnostics_for_modulation(summary, true);
        let b = diagnostics_for_modulation(summary, true);
        assert_eq!(a, b);
        assert!(a.effective_amplitude_gain.unwrap().is_finite());
        assert!(a.phase_consistency_heuristic.unwrap().is_finite());
        assert!(a.implied_latency_jitter_ms_heuristic.unwrap().is_finite());
        assert!(a.expected_plv_ceiling.unwrap().is_finite());
    }

    #[test]
    fn diagnostics_effective_gain_matches_single_modulator_modifier() {
        let assr = AssrTransfer::new();
        let mut preset = Preset::default();
        preset.objects[0].active = true;
        preset.objects[0].volume = 1.0;
        preset.objects[0].bass_mod.kind = 4;
        preset.objects[0].bass_mod.param_a = 40.0;
        preset.objects[0].bass_mod.param_b = 0.8;

        let summary = assr.summarize_preset_modulation(&preset);
        let diag = diagnostics_for_modulation(summary, true);
        assert_eq!(diag.dominant_modulation_hz, Some(40.0));
        assert_eq!(
            diag.effective_amplitude_gain.unwrap().to_bits(),
            assr.compute_input_scale_modifier(&preset).to_bits()
        );
    }

    #[test]
    fn diagnostics_effective_gain_matches_weighted_multi_modulator_modifier() {
        let assr = AssrTransfer::new();
        let mut preset = Preset::default();
        preset.objects[0].active = true;
        preset.objects[0].volume = 0.8;
        preset.objects[0].bass_mod.kind = 4;
        preset.objects[0].bass_mod.param_a = 40.0;
        preset.objects[0].bass_mod.param_b = 0.7;
        preset.objects[1].active = true;
        preset.objects[1].volume = 0.6;
        preset.objects[1].satellite_mod.kind = 5;
        preset.objects[1].satellite_mod.param_a = 8.0;
        preset.objects[1].satellite_mod.param_b = 0.9;

        let summary = assr.summarize_preset_modulation(&preset);
        let diag = diagnostics_for_modulation(summary, true);
        assert_eq!(diag.dominant_modulation_hz, Some(40.0));
        assert_eq!(
            diag.effective_amplitude_gain.unwrap().to_bits(),
            assr.compute_input_scale_modifier(&preset).to_bits(),
            "diagnostic effective gain must match the pipeline-applied weighted modifier"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // Disabled/passthrough tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn disabled_gain_is_unity() {
        let assr = AssrTransfer::disabled();
        for f in [1.0, 5.0, 10.0, 40.0, 80.0] {
            let g = assr.gain(f);
            assert_eq!(g, 1.0, "Disabled ASSR gain at {f} Hz should be 1.0");
        }
    }

    #[test]
    fn disabled_apply_is_identity() {
        let assr = AssrTransfer::disabled();
        let original = [
            vec![0.5, 0.6, 0.7, 0.8],
            vec![0.1, 0.2, 0.3, 0.4],
            vec![0.9, 0.8, 0.7, 0.6],
            vec![0.3, 0.4, 0.5, 0.6],
        ];
        let mut bands = original.clone();
        assr.apply(&mut bands, 1000.0);
        for (i, (orig, result)) in original.iter().zip(bands.iter()).enumerate() {
            for (j, (&o, &r)) in orig.iter().zip(result.iter()).enumerate() {
                assert_eq!(o, r, "Disabled ASSR should not change band {i} sample {j}");
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Signal-level tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn apply_preserves_signal_length() {
        let assr = AssrTransfer::new();
        let n = 1000;
        let mut bands = [vec![0.5; n], vec![0.5; n], vec![0.5; n], vec![0.5; n]];
        assr.apply(&mut bands, 1000.0);
        for (i, band) in bands.iter().enumerate() {
            assert_eq!(
                band.len(),
                n,
                "Band {i} length changed from {n} to {}",
                band.len()
            );
        }
    }

    #[test]
    fn apply_attenuates_slow_modulation() {
        // Create a 5 Hz modulated signal (theta range) — should be attenuated
        let assr = AssrTransfer::new();
        let sr = 1000.0;
        let n = 2000; // 2 seconds
        let freq = 5.0;
        let signal: Vec<f64> = (0..n)
            .map(|i| 0.5 + 0.5 * (2.0 * PI * freq * i as f64 / sr).sin())
            .collect();

        let original_power: f64 = signal.iter().map(|x| x * x).sum::<f64>() / n as f64;

        let mut bands = [signal, vec![0.5; n], vec![0.5; n], vec![0.5; n]];
        assr.apply(&mut bands, sr);

        let processed_power: f64 = bands[0].iter().map(|x| x * x).sum::<f64>() / n as f64;

        // 5 Hz modulation should be attenuated (gain ~0.15-0.25 at 5 Hz)
        // Power scales as gain^2, so expect significant reduction
        assert!(
            processed_power < original_power * 0.9,
            "5 Hz modulation should be attenuated: original power {original_power:.4}, processed {processed_power:.4}"
        );
    }

    #[test]
    fn apply_preserves_fast_modulation() {
        // Create a 40 Hz modulated signal — should pass through mostly intact
        let assr = AssrTransfer::new();
        let sr = 1000.0;
        let n = 2000;
        let freq = 40.0;
        let signal: Vec<f64> = (0..n)
            .map(|i| 0.5 + 0.3 * (2.0 * PI * freq * i as f64 / sr).sin())
            .collect();

        let original_power: f64 = signal.iter().map(|x| x * x).sum::<f64>() / n as f64;

        let mut bands = [signal, vec![0.5; n], vec![0.5; n], vec![0.5; n]];
        assr.apply(&mut bands, sr);

        let processed_power: f64 = bands[0].iter().map(|x| x * x).sum::<f64>() / n as f64;

        // 40 Hz should pass nearly unchanged (gain ~1.0)
        let ratio = processed_power / original_power;
        assert!(
            ratio > 0.85,
            "40 Hz modulation should pass through: power ratio {ratio:.4} (expected > 0.85)"
        );
    }

    #[test]
    fn apply_outputs_are_finite() {
        let assr = AssrTransfer::new();
        let sr = 1000.0;
        let n = 1000;
        let mut bands = [
            (0..n).map(|i| (i as f64 * 0.01).sin()).collect(),
            (0..n).map(|i| (i as f64 * 0.03).sin()).collect(),
            (0..n).map(|i| (i as f64 * 0.05).sin()).collect(),
            (0..n).map(|i| (i as f64 * 0.07).sin()).collect(),
        ];
        assr.apply(&mut bands, sr);
        for (bi, band) in bands.iter().enumerate() {
            for (si, &v) in band.iter().enumerate() {
                assert!(v.is_finite(), "Band {bi} sample {si} is not finite: {v}");
            }
        }
    }

    #[test]
    fn apply_empty_bands_no_panic() {
        let assr = AssrTransfer::new();
        let mut bands = [vec![], vec![], vec![], vec![]];
        assr.apply(&mut bands, 1000.0); // should not panic
    }

    // ═══════════════════════════════════════════════════════════════
    // input_scale modifier tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn modifier_40hz_neurallfo_near_unity() {
        let assr = AssrTransfer::new();
        let mut preset = Preset::default();
        preset.objects[0].active = true;
        preset.objects[0].volume = 0.80;
        preset.objects[0].bass_mod.kind = 4; // NeuralLfo
        preset.objects[0].bass_mod.param_a = 40.0;
        preset.objects[0].bass_mod.param_b = 0.90;

        let modifier = assr.compute_input_scale_modifier(&preset);
        assert!(
            modifier > 0.90,
            "40 Hz NeuralLfo should give modifier ~1.0, got {modifier}"
        );
    }

    #[test]
    fn modifier_5hz_neurallfo_weak() {
        let assr = AssrTransfer::new();
        let mut preset = Preset::default();
        preset.objects[0].active = true;
        preset.objects[0].volume = 0.80;
        preset.objects[0].bass_mod.kind = 4;
        preset.objects[0].bass_mod.param_a = 5.0;
        preset.objects[0].bass_mod.param_b = 0.90;

        let modifier = assr.compute_input_scale_modifier(&preset);
        assert!(
            modifier < 0.35,
            "5 Hz NeuralLfo should give weak modifier (<0.35), got {modifier}"
        );
    }

    #[test]
    fn modifier_no_neurallfo_is_unity() {
        let assr = AssrTransfer::new();
        let mut preset = Preset::default();
        preset.objects[0].active = true;
        preset.objects[0].bass_mod.kind = 2; // Breathing, not NeuralLfo
        preset.objects[0].satellite_mod.kind = 0; // Flat

        let modifier = assr.compute_input_scale_modifier(&preset);
        assert_eq!(modifier, 1.0, "No NeuralLfo should give modifier 1.0");
    }

    #[test]
    fn modifier_mixed_freqs_weighted_average() {
        let assr = AssrTransfer::new();
        let mut preset = Preset::default();
        // Obj 0: 40 Hz at high depth → high ASSR gain
        preset.objects[0].active = true;
        preset.objects[0].volume = 0.80;
        preset.objects[0].bass_mod.kind = 4;
        preset.objects[0].bass_mod.param_a = 40.0;
        preset.objects[0].bass_mod.param_b = 0.90;
        // Obj 1: 5 Hz at high depth → low ASSR gain
        preset.objects[1].active = true;
        preset.objects[1].volume = 0.80;
        preset.objects[1].bass_mod.kind = 4;
        preset.objects[1].bass_mod.param_a = 5.0;
        preset.objects[1].bass_mod.param_b = 0.90;

        let modifier = assr.compute_input_scale_modifier(&preset);
        let pure_40 = assr.gain(40.0);
        let pure_5 = assr.gain(5.0);
        // Should be between the two extremes
        assert!(
            modifier > pure_5 && modifier < pure_40,
            "Mixed preset: modifier {modifier} should be between {pure_5} and {pure_40}"
        );
    }

    #[test]
    fn modifier_disabled_is_unity() {
        let assr = AssrTransfer::disabled();
        let mut preset = Preset::default();
        preset.objects[0].active = true;
        preset.objects[0].bass_mod.kind = 4;
        preset.objects[0].bass_mod.param_a = 5.0;
        preset.objects[0].bass_mod.param_b = 0.90;

        let modifier = assr.compute_input_scale_modifier(&preset);
        assert_eq!(modifier, 1.0, "Disabled ASSR modifier should be 1.0");
    }

    #[test]
    fn modifier_in_valid_range() {
        let assr = AssrTransfer::new();
        // Test with various frequencies
        for freq in [1.0, 5.0, 10.0, 14.0, 25.0, 40.0] {
            let mut preset = Preset::default();
            preset.objects[0].active = true;
            preset.objects[0].volume = 0.80;
            preset.objects[0].bass_mod.kind = 4;
            preset.objects[0].bass_mod.param_a = freq as f32;
            preset.objects[0].bass_mod.param_b = 0.80;

            let modifier = assr.compute_input_scale_modifier(&preset);
            assert!(
                modifier >= 0.0 && modifier <= 1.0,
                "Modifier at {freq} Hz = {modifier}, should be in [0, 1]"
            );
        }
    }
}
