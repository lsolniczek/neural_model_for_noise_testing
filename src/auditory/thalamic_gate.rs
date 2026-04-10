/// Thalamic Gate Model.
///
/// Models the medial geniculate nucleus (MGN) relay between the auditory
/// pathway and cortex. The thalamus modulates the cortical operating point:
///
///   - High arousal (alert): tonic firing → high input_offset → alpha limit cycle
///   - Low arousal (relaxed): burst firing → lower input_offset → near bifurcation
///     → theta/delta oscillations become possible
///
/// The gate works by adjusting the JR model's `input_offset` parameter based
/// on arousal derived from preset properties. This is the scientifically
/// correct mechanism: the thalamus doesn't filter signals — it modulates
/// the cortical operating point.
///
/// Ref: Hughes SW, Crunelli V (2005). "Thalamic mechanisms of EEG alpha rhythms."
/// Ref: Lopes da Silva FH (1991). "Neural mechanisms underlying brain waves."
/// Ref: Suffczynski P et al. (2004). "Dynamics of non-convulsive epileptic phenomena."

use crate::preset::Preset;

/// Thalamic gate that modulates the cortical operating point based on arousal.
pub struct ThalamicGate {
    enabled: bool,
    /// Arousal level in [0, 1]. 0 = deep relaxation, 1 = high alertness.
    arousal: f64,
}

/// Maximum reduction in input_offset at lowest arousal (pulses/s).
///
/// The effective operating point is: p = band_offset + mean_input * input_scale.
/// With mean_input ≈ 0.5 and input_scale = 100, the signal adds ~50 to the offset.
/// Normal brain band_offsets are ~150-175, so effective p ≈ 200-225 (deep alpha).
/// Bifurcation boundary is around p ≈ 130-140.
///
/// To reach bifurcation at lowest arousal: need offset shift ≈ -(200-130) = -70.
/// We use -65 as max to bring the model NEAR but not past bifurcation,
/// allowing the input modulation to push it across intermittently.
const MAX_OFFSET_REDUCTION: f64 = 65.0;

impl ThalamicGate {
    /// Create a thalamic gate with the given arousal level.
    pub fn new(arousal: f64) -> Self {
        ThalamicGate {
            enabled: true,
            arousal: arousal.clamp(0.0, 1.0),
        }
    }

    /// Create a disabled (passthrough) thalamic gate.
    pub fn disabled() -> Self {
        ThalamicGate {
            enabled: false,
            arousal: 0.5,
        }
    }

    /// Returns true if the gate is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the arousal level.
    pub fn arousal(&self) -> f64 {
        self.arousal
    }

    /// Compute the input_offset shift based on arousal.
    ///
    /// Returns a negative value (reduction in offset) at low arousal,
    /// zero at high arousal.
    ///
    /// The relationship is linear:
    ///   arousal = 1.0 → shift = 0.0 (no change, alert state)
    ///   arousal = 0.5 → shift = -17.5 (moderate relaxation)
    ///   arousal = 0.0 → shift = -35.0 (deep relaxation, near bifurcation)
    pub fn offset_shift(&self) -> f64 {
        if !self.enabled {
            return 0.0;
        }
        -MAX_OFFSET_REDUCTION * (1.0 - self.arousal)
    }

    /// Compute per-band offset shifts based on arousal.
    ///
    /// Per Steriade, McCormick & Sejnowski (1993), thalamic state changes
    /// are frequency-selective: during low arousal (burst mode), slow
    /// oscillations (<15 Hz) emerge while fast rhythms persist in tonic mode.
    ///
    /// Returns [band0_shift, band1_shift, band2_shift, band3_shift]:
    ///   - Band 0 (low, 50-200 Hz → delta/theta): full shift
    ///   - Band 1 (low-mid, 200-800 Hz → theta/alpha): 70% shift
    ///   - Band 2 (mid-high, 800-3kHz → alpha/beta): 20% shift
    ///   - Band 3 (high, 3-8kHz → beta/gamma): no shift (stays tonic)
    pub fn band_offset_shifts(&self) -> [f64; 4] {
        if !self.enabled {
            return [0.0; 4];
        }
        let base = -MAX_OFFSET_REDUCTION * (1.0 - self.arousal);
        [
            base * 1.0,   // Band 0: full shift — delta/theta target
            base * 0.7,   // Band 1: 70% shift — theta/alpha boundary
            base * 0.2,   // Band 2: 20% shift — mostly stays beta-responsive
            base * 0.0,   // Band 3: no shift — always tonic for gamma
        ]
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::Preset;

    // ═══════════════════════════════════════════════════════════════
    // Offset shift tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn high_arousal_no_shift() {
        let gate = ThalamicGate::new(1.0);
        assert!(
            gate.offset_shift().abs() < 1e-10,
            "High arousal should give zero shift, got {}",
            gate.offset_shift()
        );
    }

    #[test]
    fn low_arousal_max_shift() {
        let gate = ThalamicGate::new(0.0);
        assert!(
            (gate.offset_shift() - (-MAX_OFFSET_REDUCTION)).abs() < 1e-10,
            "Low arousal should give max shift (-{}), got {}",
            MAX_OFFSET_REDUCTION,
            gate.offset_shift()
        );
    }

    #[test]
    fn mid_arousal_half_shift() {
        let gate = ThalamicGate::new(0.5);
        let expected = -MAX_OFFSET_REDUCTION * 0.5;
        assert!(
            (gate.offset_shift() - expected).abs() < 1e-10,
            "Mid arousal should give half shift ({expected}), got {}",
            gate.offset_shift()
        );
    }

    #[test]
    fn shift_is_always_non_positive() {
        for a in 0..=100 {
            let gate = ThalamicGate::new(a as f64 / 100.0);
            assert!(
                gate.offset_shift() <= 0.0,
                "Shift should be <= 0, got {} at arousal {}",
                gate.offset_shift(),
                a as f64 / 100.0
            );
        }
    }

    #[test]
    fn shift_is_linear_with_arousal() {
        let g1 = ThalamicGate::new(0.25);
        let g2 = ThalamicGate::new(0.75);
        let diff = g2.offset_shift() - g1.offset_shift();
        let expected_diff = MAX_OFFSET_REDUCTION * 0.5; // 0.75 - 0.25 = 0.5 span
        assert!(
            (diff - expected_diff).abs() < 1e-10,
            "Shift should be linear: diff={diff}, expected={expected_diff}"
        );
    }

    #[test]
    fn disabled_gives_zero_shift() {
        let gate = ThalamicGate::disabled();
        assert_eq!(gate.offset_shift(), 0.0, "Disabled gate should give zero shift");
    }

    #[test]
    fn normal_brain_shifted_reaches_bifurcation() {
        // Normal brain band_offset ~150 + mean_input(0.5)*input_scale(100) = ~200
        // At arousal 0.0, shift = -65 → effective offset = 85 → p ≈ 85+50 = 135
        // This is near ADHD's bifurcation boundary
        let gate = ThalamicGate::new(0.0);
        let normal_offset = 150.0; // typical band_offset
        let effective_p = (normal_offset + gate.offset_shift()) + 0.5 * 100.0;
        assert!(
            effective_p >= 120.0 && effective_p <= 145.0,
            "Relaxed Normal effective p should approach bifurcation (~130-140), got {effective_p}"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // Arousal computation tests
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn arousal_from_dark_reverberant_preset() {
        let mut preset = Preset::default();
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
        preset.objects[0].active = true;
        preset.objects[0].color = 0; // White
        preset.objects[0].reverb_send = 0.10;
        preset.objects[0].bass_mod.kind = 4; // NeuralLfo
        preset.objects[0].bass_mod.param_a = 25.0;
        preset.objects[0].satellite_mod.kind = 4;
        preset.objects[0].satellite_mod.param_a = 40.0;
        preset.objects[0].movement.kind = 2;
        preset.objects[0].movement.speed = 4.0;

        let arousal = ThalamicGate::compute_arousal(&preset, 0.9); // bright
        assert!(
            arousal > 0.60,
            "Bright dry active preset should give high arousal, got {arousal}"
        );
    }

    #[test]
    fn arousal_always_in_valid_range() {
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

    #[test]
    fn dark_preset_produces_meaningful_offset_shift() {
        let mut preset = Preset::default();
        preset.objects[0].active = true;
        preset.objects[0].color = 2;
        preset.objects[0].reverb_send = 0.85;
        preset.objects[0].bass_mod.kind = 2;
        preset.objects[0].movement.kind = 0;

        let arousal = ThalamicGate::compute_arousal(&preset, 0.1);
        let gate = ThalamicGate::new(arousal);
        let shift = gate.offset_shift();

        // Low arousal should produce at least -30 shift
        assert!(
            shift < -30.0,
            "Dark preset should shift offset by > 30, got {shift}"
        );

        println!("Dark preset: arousal={arousal:.3}, offset_shift={shift:.1}");
    }

    // ═══════════════════════════════════════════════════════════════
    // Band-dependent shift tests
    // Per Steriade, McCormick & Sejnowski (1993): thalamic burst mode
    // is frequency-selective — slow bands shift, fast bands stay tonic.
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn band_shifts_low_arousal_decreasing_by_band() {
        let gate = ThalamicGate::new(0.0); // fully relaxed
        let shifts = gate.band_offset_shifts();
        // Band 0 gets full shift, band 3 gets none
        assert!(shifts[0] < shifts[1], "Band 0 shift ({}) should be more negative than band 1 ({})", shifts[0], shifts[1]);
        assert!(shifts[1] < shifts[2], "Band 1 shift ({}) should be more negative than band 2 ({})", shifts[1], shifts[2]);
        assert!(shifts[2] < shifts[3] || shifts[2].abs() < 1e-10 && shifts[3].abs() < 1e-10,
            "Band 2 shift ({}) should be more negative than band 3 ({})", shifts[2], shifts[3]);
    }

    #[test]
    fn band_shifts_band3_always_zero() {
        for a in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let gate = ThalamicGate::new(a);
            let shifts = gate.band_offset_shifts();
            assert!(
                shifts[3].abs() < 1e-10,
                "Band 3 (gamma) should never shift, got {} at arousal {a}",
                shifts[3]
            );
        }
    }

    #[test]
    fn band_shifts_high_arousal_all_zero() {
        let gate = ThalamicGate::new(1.0);
        let shifts = gate.band_offset_shifts();
        for (b, &s) in shifts.iter().enumerate() {
            assert!(
                s.abs() < 1e-10,
                "High arousal: band {b} shift should be 0, got {s}"
            );
        }
    }

    #[test]
    fn band_shifts_disabled_all_zero() {
        let gate = ThalamicGate::disabled();
        let shifts = gate.band_offset_shifts();
        for (b, &s) in shifts.iter().enumerate() {
            assert_eq!(s, 0.0, "Disabled: band {b} shift should be 0");
        }
    }

    #[test]
    fn band_shifts_band0_matches_uniform_shift() {
        // Band 0 should get the same shift as the uniform offset_shift()
        let gate = ThalamicGate::new(0.3);
        let uniform = gate.offset_shift();
        let per_band = gate.band_offset_shifts();
        assert!(
            (per_band[0] - uniform).abs() < 1e-10,
            "Band 0 ({}) should match uniform shift ({})",
            per_band[0], uniform
        );
    }

    #[test]
    fn band_shifts_proportions_correct() {
        let gate = ThalamicGate::new(0.0);
        let shifts = gate.band_offset_shifts();
        let band0 = shifts[0];
        // Band 1 = 70% of band 0
        assert!((shifts[1] / band0 - 0.7).abs() < 0.01,
            "Band 1 should be 70% of band 0: {} / {} = {}", shifts[1], band0, shifts[1] / band0);
        // Band 2 = 20% of band 0
        assert!((shifts[2] / band0 - 0.2).abs() < 0.01,
            "Band 2 should be 20% of band 0: {} / {} = {}", shifts[2], band0, shifts[2] / band0);
    }
}
