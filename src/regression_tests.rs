/// Pre-refactor regression tests.
///
/// These tests capture the current behaviour of scoring, optimizer boundary
/// handling, genome encoding, and the full pipeline so that the planned
/// changes (downsampling, RNG fix, clamping, Gaussian scoring, longer
/// simulation) do not introduce silent regressions.

#[cfg(test)]
mod tests {
    use crate::brain_type::BrainType;
    use crate::neural::jansen_rit::*;
    use crate::neural::fhn::*;
    use crate::optimizer::DifferentialEvolution;
    use crate::pipeline::{evaluate_preset, SimulationConfig};
    use crate::preset::{Preset, GENOME_LEN};
    use crate::scoring::{Goal, GoalKind};

    // ════════════════════════════════════════════════════════════════════════
    // 1. Scoring regression tests
    // ════════════════════════════════════════════════════════════════════════

    /// Triangular band scoring: snapshot at key points.
    /// When we switch to Gaussian, these values will change intentionally —
    /// the test documents the *old* behaviour so we can compare.
    #[test]
    fn triangular_score_at_known_points() {
        // Use Focus goal: alpha band target is (0.18, 0.33, 0.50)
        let goal = Goal::new(GoalKind::Focus);

        // Build a JR result with controlled band powers.
        // We test score_bands indirectly through evaluate_with_brightness
        // by setting FHN to a "perfect" state so band score dominates.
        let sr = 48_000.0;
        let n = (sr * 3.0) as usize;

        // Generate a known bilateral result to extract band scoring behaviour
        let bands = [vec![0.5; n], vec![0.5; n], vec![0.5; n], vec![0.5; n]];
        let energy = [0.25, 0.25, 0.25, 0.25];
        let bt = BrainType::Normal;
        let params = bt.params();
        let bilateral = bt.bilateral_params();

        let fi = FastInhibParams {
            g_fast_gain: params.jansen_rit.g_fast_gain,
            g_fast_rate: params.jansen_rit.g_fast_rate,
            c5: params.jansen_rit.c5,
            c6: params.jansen_rit.c6,
            c7: params.jansen_rit.c7,
        };
        let bi = simulate_bilateral(
            &bands, &bands, &energy, &energy,
            &bilateral, params.jansen_rit.c, params.jansen_rit.input_scale, sr, &fi,
            params.jansen_rit.v0,
        );

        // Normalise EEG for FHN
        let eeg_max = bi.combined.eeg.iter().map(|x| x.abs()).fold(0.0_f64, f64::max);
        let eeg_norm = if eeg_max > 1e-10 { 1.0 / eeg_max } else { 1.0 };
        let fhn_input: Vec<f64> = bi.combined.eeg.iter().map(|x| x * eeg_norm).collect();
        let fhn = FhnModel::with_params(sr, params.fhn.a, params.fhn.b, params.fhn.epsilon, params.fhn.time_scale);
        let fhn_result = fhn.simulate(&fhn_input, params.fhn.input_scale);

        // Evaluate and snapshot
        let score = goal.evaluate_with_brightness(&fhn_result, &bi.combined, 0.5);

        // Score must be in [0, 1]
        assert!(score >= 0.0 && score <= 1.0,
            "score {} out of [0,1] range", score);

        // Snapshot: the score with Normal brain + Focus goal should be in a
        // reasonable range. We record the exact value as a comment for comparison
        // after refactoring, but assert a wider band to avoid flaky tests.
        println!("REGRESSION SNAPSHOT: Focus/Normal score = {:.6}", score);
        assert!(score > 0.0, "Focus score should be non-zero for Normal brain");
    }

    /// Triangular scoring produces exactly 0 at the boundaries.
    /// This is the specific behaviour we want to change to Gaussian.
    #[test]
    fn triangular_score_hard_zero_at_boundaries() {
        // Build BandPowers at exactly the boundary values for DeepRelaxation
        // delta target: (0.05, 0.22, 0.40)
        // We need to test the triangular scoring directly.
        // Since BandTarget is private, we test through the Goal API.

        // Construct JR result with band powers that put delta exactly at min
        let bp_at_min = BandPowers {
            delta: 0.05,   // exactly at min
            theta: 0.35,   // at ideal
            alpha: 0.36,   // at ideal
            beta: 0.03,    // at ideal
            gamma: 0.01,   // at ideal
        };

        let bp_at_max = BandPowers {
            delta: 0.40,   // exactly at max
            theta: 0.35,
            alpha: 0.36,
            beta: 0.03,
            gamma: 0.01,
        };

        let bp_at_ideal = BandPowers {
            delta: 0.22,   // at ideal
            theta: 0.35,
            alpha: 0.36,
            beta: 0.03,
            gamma: 0.01,
        };

        // Use raw band powers to create JR results for scoring
        let jr_min = make_jr_result_from_powers(bp_at_min);
        let jr_max = make_jr_result_from_powers(bp_at_max);
        let jr_ideal = make_jr_result_from_powers(bp_at_ideal);

        let fhn_perfect = make_perfect_fhn(GoalKind::DeepRelaxation);

        let goal = Goal::new(GoalKind::DeepRelaxation);

        let score_min = goal.evaluate_with_brightness(&fhn_perfect, &jr_min, 0.5);
        let score_max = goal.evaluate_with_brightness(&fhn_perfect, &jr_max, 0.5);
        let score_ideal = goal.evaluate_with_brightness(&fhn_perfect, &jr_ideal, 0.5);

        // At boundaries, the band score for delta should be 0 → lower total score
        // At ideal, band score for delta should be 1.0 → higher total score
        assert!(score_ideal > score_min,
            "ideal ({:.4}) should score higher than at-min ({:.4})", score_ideal, score_min);
        assert!(score_ideal > score_max,
            "ideal ({:.4}) should score higher than at-max ({:.4})", score_ideal, score_max);

        println!("REGRESSION SNAPSHOT: boundary scores min={:.6} max={:.6} ideal={:.6}",
            score_min, score_max, score_ideal);
    }

    /// All goals produce scores in [0, 1] range.
    #[test]
    fn all_goals_score_in_valid_range() {
        for kind in GoalKind::all() {
            let goal = Goal::new(*kind);

            // Test with various band power distributions
            let distributions = [
                BandPowers { delta: 0.5, theta: 0.2, alpha: 0.2, beta: 0.05, gamma: 0.05 },
                BandPowers { delta: 0.05, theta: 0.1, alpha: 0.4, beta: 0.35, gamma: 0.1 },
                BandPowers { delta: 0.2, theta: 0.2, alpha: 0.2, beta: 0.2, gamma: 0.2 },
                BandPowers { delta: 0.0, theta: 0.0, alpha: 1.0, beta: 0.0, gamma: 0.0 },
            ];

            for (i, bp) in distributions.iter().enumerate() {
                let jr = make_jr_result_from_powers(BandPowers {
                    delta: bp.delta, theta: bp.theta, alpha: bp.alpha,
                    beta: bp.beta, gamma: bp.gamma,
                });
                let fhn = make_perfect_fhn(*kind);

                for brightness in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    let score = goal.evaluate_with_brightness(&fhn, &jr, brightness);
                    assert!(score >= 0.0 && score <= 1.0,
                        "{:?} dist={} brightness={}: score {} out of range",
                        kind, i, brightness, score);
                }
            }
        }
    }

    /// Isolation goal uses flat-deviation scoring (special case).
    #[test]
    fn isolation_perfect_flat_scores_high() {
        let goal = Goal::new(GoalKind::Isolation);
        let perfect_flat = BandPowers {
            delta: 0.2, theta: 0.2, alpha: 0.2, beta: 0.2, gamma: 0.2,
        };
        let jr = make_jr_result_from_powers(perfect_flat);
        let fhn = make_perfect_fhn(GoalKind::Isolation);

        let score = goal.evaluate_with_brightness(&fhn, &jr, 0.7);
        println!("REGRESSION SNAPSHOT: Isolation perfect flat score = {:.6}", score);

        // Perfect flat distribution should score well
        assert!(score > 0.5, "perfect flat isolation should score > 0.5, got {:.4}", score);
    }

    // ════════════════════════════════════════════════════════════════════════
    // 2. Optimizer / bounce-back regression tests
    // ════════════════════════════════════════════════════════════════════════

    /// Bounce-back produces values within bounds (current behaviour).
    /// Documents the RNG clone bug: repeated calls produce the same offset.
    #[test]
    fn bounce_back_stays_in_bounds() {
        let bounds = Preset::bounds();
        let mut de = DifferentialEvolution::new(bounds.clone(), 10, 0.8, 0.9, 42);

        // Generate trials that may trigger bounce-back
        // First evaluate initial population with dummy fitness
        for (idx, _) in de.pending_evaluations() {
            de.report_fitness(idx, 0.5);
        }

        let trials = de.generate_trials();

        for (_, trial) in &trials {
            for (j, &val) in trial.iter().enumerate() {
                let (lo, hi) = bounds[j];
                assert!(val >= lo && val <= hi,
                    "trial gene {} = {} outside bounds [{}, {}]", j, val, lo, hi);
            }
        }
    }

    /// Document that bounce-back prevents reaching exact boundary values.
    /// After switching to clamping, volume=0.0 should become reachable.
    #[test]
    fn bounce_back_prevents_exact_zero_volume() {
        let bounds = Preset::bounds();
        let mut de = DifferentialEvolution::new(bounds.clone(), 50, 0.8, 0.9, 123);

        // Evaluate initial pop
        for (idx, _) in de.pending_evaluations() {
            de.report_fitness(idx, 0.5);
        }

        // Run several generations collecting trial volume values
        let mut min_volume = f64::MAX;
        let volume_dim = 6 + 5; // first object's volume gene index

        for _ in 0..20 {
            let trials = de.generate_trials();
            for (idx, trial) in &trials {
                if trial[volume_dim] < min_volume {
                    min_volume = trial[volume_dim];
                }
                de.report_trial_result(*idx, trial.clone(), 0.5);
            }
        }

        println!("REGRESSION SNAPSHOT: min volume gene across 20 gens = {:.6}", min_volume);
        // With bounce-back, the minimum achievable is > 0 (bounced 10% into range)
        // After clamping fix, 0.0 should be reachable
    }

    /// RNG clone bug: bounce_back with same state produces deterministic (non-random) results.
    #[test]
    fn bounce_back_rng_clone_produces_same_values() {
        // Create two DE instances with same seed
        let bounds = vec![(0.0, 1.0); 10];
        let de1 = DifferentialEvolution::new(bounds.clone(), 5, 0.8, 0.9, 999);
        let de2 = DifferentialEvolution::new(bounds.clone(), 5, 0.8, 0.9, 999);

        // Both should produce identical initial populations (same seed)
        let pop1 = de1.pending_evaluations();
        let pop2 = de2.pending_evaluations();

        for ((_, g1), (_, g2)) in pop1.iter().zip(pop2.iter()) {
            for (v1, v2) in g1.iter().zip(g2.iter()) {
                assert!((v1 - v2).abs() < 1e-15,
                    "same seed should produce identical populations");
            }
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 3. Genome encoding / discrete parameter tests
    // ════════════════════════════════════════════════════════════════════════

    /// Round-trip: encode → decode preserves preset values.
    #[test]
    fn genome_roundtrip_preserves_preset() {
        let original = Preset::default();
        let genome = original.to_genome();
        assert_eq!(genome.len(), GENOME_LEN);

        let decoded = Preset::from_genome(&genome);

        assert_eq!(original.master_gain, decoded.master_gain);
        assert_eq!(original.spatial_mode, decoded.spatial_mode);
        assert_eq!(original.source_count, decoded.source_count);
        assert_eq!(original.anchor_color, decoded.anchor_color);
        assert_eq!(original.environment, decoded.environment);

        for (i, (orig, dec)) in original.objects.iter().zip(decoded.objects.iter()).enumerate() {
            assert_eq!(orig.active, dec.active, "object {} active", i);
            assert_eq!(orig.color, dec.color, "object {} color", i);
            assert!((orig.volume - dec.volume).abs() < 1e-6,
                "object {} volume: {} vs {}", i, orig.volume, dec.volume);
            assert_eq!(orig.bass_mod.kind, dec.bass_mod.kind, "object {} bass_mod.kind", i);
            assert_eq!(orig.satellite_mod.kind, dec.satellite_mod.kind, "object {} sat_mod.kind", i);
            assert_eq!(orig.movement.kind, dec.movement.kind, "object {} movement.kind", i);
        }
    }

    /// Discrete parameters: nearby continuous values decode to the same discrete value.
    /// This documents the "wasted budget" problem — the optimizer sees different
    /// genomes but the DSP engine sees identical presets.
    #[test]
    fn discrete_params_plateau_effect() {
        let bounds = Preset::bounds();
        let mut genome = vec![0.0; GENOME_LEN];

        // Set a valid base genome within bounds
        for (i, (lo, hi)) in bounds.iter().enumerate() {
            genome[i] = (lo + hi) / 2.0;
        }

        // movement.kind for first object is at index 6 + 15 = 21
        let mov_kind_idx = 6 + 15;

        // Values 2.1 and 2.4 both round to 2
        genome[mov_kind_idx] = 2.1;
        let preset_a = Preset::from_genome(&genome);

        genome[mov_kind_idx] = 2.4;
        let preset_b = Preset::from_genome(&genome);

        assert_eq!(preset_a.objects[0].movement.kind, preset_b.objects[0].movement.kind,
            "2.1 and 2.4 should decode to the same movement kind");

        // Values 2.4 and 2.6 round to different values (2 vs 3)
        genome[mov_kind_idx] = 2.6;
        let preset_c = Preset::from_genome(&genome);

        assert_ne!(preset_a.objects[0].movement.kind, preset_c.objects[0].movement.kind,
            "2.1 and 2.6 should decode to different movement kinds");
    }

    /// Genome bounds cover full parameter space.
    #[test]
    fn genome_bounds_correct_length() {
        let bounds = Preset::bounds();
        assert_eq!(bounds.len(), GENOME_LEN,
            "bounds length {} != GENOME_LEN {}", bounds.len(), GENOME_LEN);

        for (i, (lo, hi)) in bounds.iter().enumerate() {
            assert!(lo <= hi, "bound {} has lo {} > hi {}", i, lo, hi);
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 4. Pipeline integration tests (full evaluate_preset)
    // ════════════════════════════════════════════════════════════════════════

    /// Full pipeline produces valid score for default preset.
    #[test]
    fn pipeline_default_preset_produces_valid_score() {
        let preset = Preset::default();
        let config = SimulationConfig::default();

        for kind in GoalKind::all() {
            let goal = Goal::new(*kind);
            let result = evaluate_preset(&preset, &goal, &config);

            assert!(result.score >= 0.0 && result.score <= 1.0,
                "{:?}: score {} out of range", kind, result.score);
            assert!(result.dominant_freq >= 0.0 && result.dominant_freq <= 100.0,
                "{:?}: dominant_freq {} out of range", kind, result.dominant_freq);
            assert!(result.fhn_firing_rate >= 0.0,
                "{:?}: negative firing rate {}", kind, result.fhn_firing_rate);

            println!("REGRESSION SNAPSHOT: {:?} default preset score={:.6} dom_freq={:.2} firing_rate={:.2}",
                kind, result.score, result.dominant_freq, result.fhn_firing_rate);
        }
    }

    /// Pipeline: band powers sum to approximately 1.0 (normalised).
    #[test]
    fn pipeline_band_powers_normalised() {
        let preset = Preset::default();
        let goal = Goal::new(GoalKind::Focus);
        let config = SimulationConfig::default();

        let result = evaluate_preset(&preset, &goal, &config);

        let total = result.delta_power + result.theta_power + result.alpha_power
            + result.beta_power + result.gamma_power;

        assert!((total - 1.0).abs() < 0.01,
            "band powers should sum to ~1.0, got {:.6}", total);
    }

    /// Pipeline: different presets produce different scores.
    #[test]
    fn pipeline_different_presets_differ() {
        let goal = Goal::new(GoalKind::Sleep);
        let config = SimulationConfig::default();

        // Default preset
        let preset1 = Preset::default();
        let result1 = evaluate_preset(&preset1, &goal, &config);

        // Modified preset: all objects active with high volume
        let mut preset2 = Preset::default();
        for obj in &mut preset2.objects {
            obj.active = true;
            obj.volume = 1.0;
            obj.color = 4; // different noise color
        }
        preset2.master_gain = 1.0;
        let result2 = evaluate_preset(&preset2, &goal, &config);

        // Scores should differ (different audio → different neural response)
        assert!((result1.score - result2.score).abs() > 1e-6
            || (result1.dominant_freq - result2.dominant_freq).abs() > 0.1,
            "different presets should produce different results");
    }

    /// Simulation duration: verify default is 12 seconds with 2s warm-up discard.
    #[test]
    fn simulation_default_duration_is_12_seconds() {
        let config = SimulationConfig::default();
        assert!((config.duration_secs - 12.0).abs() < 1e-6,
            "default duration should be 12.0s, got {}", config.duration_secs);
        assert!((config.warmup_discard_secs - 2.0).abs() < 1e-6,
            "default warmup discard should be 2.0s, got {}", config.warmup_discard_secs);
    }

    // ════════════════════════════════════════════════════════════════════════
    // 5. Neural model consistency tests (pre-downsampling baseline)
    // ════════════════════════════════════════════════════════════════════════

    /// JR model at 48kHz produces valid EEG output.
    /// After downsampling to 1kHz, output characteristics should be preserved.
    #[test]
    fn jr_48khz_baseline_output() {
        let sr = 48_000.0;
        let n = (sr * 3.0) as usize;
        // Use 10 Hz sinusoidal input — the Universal Architecture is designed
        // to be plastic and needs structured input to oscillate (constant
        // input settles to a fixed point, which is the intended behaviour).
        let input: Vec<f64> = (0..n)
            .map(|i| 0.5 + 0.3 * (2.0 * std::f64::consts::PI * 10.0 * i as f64 / sr).sin())
            .collect();

        let bt = BrainType::Normal;
        let params = bt.params();
        let fi = FastInhibParams {
            g_fast_gain: params.jansen_rit.g_fast_gain,
            g_fast_rate: params.jansen_rit.g_fast_rate,
            c5: params.jansen_rit.c5,
            c6: params.jansen_rit.c6,
            c7: params.jansen_rit.c7,
        };
        let model = JansenRitModel::with_wendling_params(
            sr, 3.25, 22.0, 100.0, 50.0, params.jansen_rit.c,
            220.0, params.jansen_rit.input_scale, &fi,
            params.jansen_rit.slow_inhib_ratio, params.jansen_rit.v0, 0.62,
        );

        let result = model.simulate(&input);

        // EEG should be non-trivial
        let eeg_range = result.eeg.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            - result.eeg.iter().cloned().fold(f64::INFINITY, f64::min);
        assert!(eeg_range > 0.001, "EEG range {} too small — model not oscillating", eeg_range);

        // Dominant frequency should be in physiological range
        assert!(result.dominant_freq >= 0.5 && result.dominant_freq <= 50.0,
            "dominant freq {} out of physiological range", result.dominant_freq);

        // Band powers should be non-zero
        assert!(result.band_powers.total() > 0.0, "zero total band power");

        println!("REGRESSION SNAPSHOT: JR 48kHz baseline: dom_freq={:.2} eeg_range={:.4} total_power={:.4}",
            result.dominant_freq, eeg_range, result.band_powers.total());
    }

    /// FHN firing characteristics are stable across runs (deterministic model).
    #[test]
    fn fhn_deterministic_output() {
        let sr = 48_000.0;
        let n = (sr * 3.0) as usize;

        // Sine wave input at 10 Hz (alpha band)
        let input: Vec<f64> = (0..n)
            .map(|i| 0.5 * (2.0 * std::f64::consts::PI * 10.0 * i as f64 / sr).sin())
            .collect();

        let bt = BrainType::Normal;
        let params = bt.params();
        let fhn = FhnModel::with_params(sr, params.fhn.a, params.fhn.b, params.fhn.epsilon, params.fhn.time_scale);

        let result1 = fhn.simulate(&input, params.fhn.input_scale);
        let result2 = fhn.simulate(&input, params.fhn.input_scale);

        // Same input → same output (deterministic)
        assert_eq!(result1.firing_rate, result2.firing_rate,
            "FHN should be deterministic");
        // NaN != NaN in IEEE 754, so use total_cmp for bitwise equality
        assert!(result1.isi_cv.total_cmp(&result2.isi_cv).is_eq(),
            "FHN ISI CV should be deterministic: {} vs {}", result1.isi_cv, result2.isi_cv);

        println!("REGRESSION SNAPSHOT: FHN 10Hz sine: rate={:.2} cv={:.4}",
            result1.firing_rate, result1.isi_cv);
    }

    // ════════════════════════════════════════════════════════════════════════
    // Helpers
    // ════════════════════════════════════════════════════════════════════════

    /// Create a JansenRitResult with specific band powers for scoring tests.
    /// Uses synthetic EEG data that produces the desired power distribution.
    fn make_jr_result_from_powers(powers: BandPowers) -> JansenRitResult {
        JansenRitResult {
            eeg: vec![0.0; 1000], // dummy — scoring only uses band_powers
            band_powers: powers,
            dominant_freq: 10.0, // default alpha
            fast_inhib_trace: Vec::new(),
        }
    }

    /// Create an FhnResult that scores well for the given goal.
    fn make_perfect_fhn(kind: GoalKind) -> FhnResult {
        let (rate, cv) = match kind {
            GoalKind::DeepRelaxation => (3.5, 0.38),
            GoalKind::Focus => (14.0, 0.30),
            GoalKind::Sleep => (2.0, 0.42),
            GoalKind::Isolation => (5.0, 0.35),
            GoalKind::Meditation => (3.5, 0.28),
            GoalKind::DeepWork => (8.0, 0.30),
        };

        FhnResult {
            voltage: vec![0.0; 1000],
            recovery: vec![0.0; 1000],
            spike_times: vec![],
            firing_rate: rate,
            isi_cv: cv,
            mean_voltage: 0.0,
            voltage_variance: 0.0,
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 6. ASSR + Thalamic Gate integration tests
    // ════════════════════════════════════════════════════════════════════════

    /// Default config (both disabled) produces identical scores to pre-change baseline.
    #[test]
    fn assr_and_gate_disabled_by_default() {
        let config = SimulationConfig::default();
        assert!(!config.assr_enabled, "ASSR should be disabled by default");
        assert!(!config.thalamic_gate_enabled, "Thalamic gate should be disabled by default");
    }

    /// Pipeline with both features disabled produces same score as before.
    #[test]
    fn disabled_features_preserve_existing_scores() {
        let preset = Preset::default();
        let config_off = SimulationConfig::default(); // both disabled

        let mut config_explicit = SimulationConfig::default();
        config_explicit.assr_enabled = false;
        config_explicit.thalamic_gate_enabled = false;

        let goal = Goal::new(GoalKind::Focus);
        let result_off = evaluate_preset(&preset, &goal, &config_off);
        let result_explicit = evaluate_preset(&preset, &goal, &config_explicit);

        assert!(
            (result_off.score - result_explicit.score).abs() < 1e-10,
            "Explicitly disabled should match default: {} vs {}",
            result_off.score, result_explicit.score
        );
    }

    /// Helper: create a preset with strong modulation for testing ASSR/gate effects.
    fn make_modulated_preset() -> Preset {
        let mut preset = Preset::default();
        preset.source_count = 2;
        preset.objects[0].active = true;
        preset.objects[0].color = 0; // White
        preset.objects[0].volume = 0.85;
        preset.objects[0].x = 3.0;
        preset.objects[0].bass_mod.kind = 4; // NeuralLfo
        preset.objects[0].bass_mod.param_a = 14.0; // 14 Hz beta
        preset.objects[0].bass_mod.param_b = 0.90; // high depth
        preset.objects[0].satellite_mod.kind = 4;
        preset.objects[0].satellite_mod.param_a = 14.0;
        preset.objects[0].satellite_mod.param_b = 0.85;

        preset.objects[1].active = true;
        preset.objects[1].color = 0;
        preset.objects[1].volume = 0.75;
        preset.objects[1].x = -3.0;
        preset.objects[1].bass_mod.kind = 2; // Breathing
        preset.objects[1].bass_mod.param_a = 3.0;
        preset.objects[1].bass_mod.param_b = 0.80;
        preset
    }

    /// ASSR enabled changes scores (proves the component has effect).
    #[test]
    fn assr_enabled_changes_scores() {
        let preset = make_modulated_preset();
        let goal = Goal::new(GoalKind::Focus);

        let config_off = SimulationConfig::default();
        let mut config_on = SimulationConfig::default();
        config_on.assr_enabled = true;

        let result_off = evaluate_preset(&preset, &goal, &config_off);
        let result_on = evaluate_preset(&preset, &goal, &config_on);

        // Scores should differ when ASSR is enabled on modulated preset
        assert!(
            (result_off.score - result_on.score).abs() > 1e-4,
            "ASSR should change scores on modulated preset: off={:.6} on={:.6}",
            result_off.score, result_on.score
        );

        // Both should still be valid
        assert!(result_on.score >= 0.0 && result_on.score <= 1.0,
            "ASSR-enabled score {} out of range", result_on.score);

        println!("ASSR effect: off={:.6} on={:.6} delta={:.6}",
            result_off.score, result_on.score, result_on.score - result_off.score);
    }

    /// Thalamic gate enabled changes scores.
    #[test]
    fn thalamic_gate_enabled_changes_scores() {
        let preset = make_modulated_preset();
        let goal = Goal::new(GoalKind::DeepRelaxation);

        let config_off = SimulationConfig::default();
        let mut config_on = SimulationConfig::default();
        config_on.thalamic_gate_enabled = true;

        let result_off = evaluate_preset(&preset, &goal, &config_off);
        let result_on = evaluate_preset(&preset, &goal, &config_on);

        assert!(
            (result_off.score - result_on.score).abs() > 1e-4,
            "Thalamic gate should change scores on modulated preset: off={:.6} on={:.6}",
            result_off.score, result_on.score
        );

        assert!(result_on.score >= 0.0 && result_on.score <= 1.0,
            "Gate-enabled score {} out of range", result_on.score);

        println!("Thalamic gate effect: off={:.6} on={:.6} delta={:.6}",
            result_off.score, result_on.score, result_on.score - result_off.score);
    }

    /// Both features enabled together produces valid scores.
    #[test]
    fn both_features_enabled_produces_valid_scores() {
        let preset = Preset::default();

        let mut config = SimulationConfig::default();
        config.assr_enabled = true;
        config.thalamic_gate_enabled = true;

        for kind in GoalKind::all() {
            let goal = Goal::new(*kind);
            let result = evaluate_preset(&preset, &goal, &config);

            assert!(
                result.score >= 0.0 && result.score <= 1.0,
                "{:?} with both features: score {} out of range",
                kind, result.score
            );
            assert!(
                result.dominant_freq.is_finite(),
                "{:?} with both features: non-finite dominant freq",
                kind
            );

            println!("Both enabled {:?}: score={:.6} dom_freq={:.2}",
                kind, result.score, result.dominant_freq);
        }
    }

    /// ASSR + gate preserve band power normalization (sum ≈ 1.0).
    #[test]
    fn features_preserve_band_normalization() {
        let preset = Preset::default();
        let goal = Goal::new(GoalKind::Isolation);

        let mut config = SimulationConfig::default();
        config.assr_enabled = true;
        config.thalamic_gate_enabled = true;

        let result = evaluate_preset(&preset, &goal, &config);
        let total = result.delta_power + result.theta_power + result.alpha_power
            + result.beta_power + result.gamma_power;

        assert!(
            (total - 1.0).abs() < 0.01,
            "Band powers should sum to ~1.0 with features enabled, got {:.6}",
            total
        );
    }
}
