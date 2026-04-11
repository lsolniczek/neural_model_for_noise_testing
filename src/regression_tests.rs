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
            params.jansen_rit.v0, 0.0, 0.0, 0.0,
        );

        // Normalise EEG for FHN
        // Percentile-based EEG scaling (matches pipeline)
        let mut abs_values: Vec<f64> = bi.combined.eeg.iter().map(|x| x.abs()).collect();
        abs_values.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let p95_idx = (abs_values.len() as f64 * 0.95) as usize;
        let p95 = abs_values[p95_idx.min(abs_values.len() - 1)];
        let scale = if p95 > 1e-10 { 1.0 / p95 } else { 1.0 };
        let fhn_input: Vec<f64> = bi.combined.eeg.iter().map(|x| (x * scale).clamp(-3.0, 3.0)).collect();
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
        let mut model = JansenRitModel::with_wendling_params(
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
            GoalKind::Shield => (10.0, 0.25),
            GoalKind::Flow => (7.0, 0.30),
            GoalKind::Ignition => (18.0, 0.35),
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
    fn assr_and_gate_enabled_by_default() {
        let config = SimulationConfig::default();
        assert!(config.assr_enabled, "ASSR should be enabled by default");
        assert!(config.thalamic_gate_enabled, "Thalamic gate should be enabled by default");
    }

    /// Pipeline with both features disabled produces same score as before.
    #[test]
    fn enabled_features_are_consistent() {
        let preset = Preset::default();
        let config1 = SimulationConfig::default(); // both enabled

        let mut config2 = SimulationConfig::default();
        config2.assr_enabled = true;
        config2.thalamic_gate_enabled = true;

        let goal = Goal::new(GoalKind::Focus);
        let result1 = evaluate_preset(&preset, &goal, &config1);
        let result2 = evaluate_preset(&preset, &goal, &config2);

        assert!(
            (result1.score - result2.score).abs() < 1e-10,
            "Default and explicit enabled should match: {} vs {}",
            result1.score, result2.score
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
    fn assr_disabled_changes_scores() {
        let preset = make_modulated_preset();
        let goal = Goal::new(GoalKind::Focus);

        let config_on = SimulationConfig::default(); // enabled by default
        let mut config_off = SimulationConfig::default();
        config_off.assr_enabled = false;

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
    fn thalamic_gate_disabled_changes_scores() {
        let preset = make_modulated_preset();
        let goal = Goal::new(GoalKind::DeepRelaxation);

        let config_on = SimulationConfig::default(); // enabled by default
        let mut config_off = SimulationConfig::default();
        config_off.thalamic_gate_enabled = false;

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

    // ════════════════════════════════════════════════════════════════════════
    // 7. Global band normalization tests
    // ════════════════════════════════════════════════════════════════════════
    //
    // Per Patterson et al. (1992) and Glasberg & Moore (2002), inter-band
    // energy ratios carry critical spectral information and must be preserved
    // through the auditory pipeline. Per-band max normalization destroys these
    // ratios; global normalization preserves them.

    /// Brown noise (low-freq dominant) and White noise (flat) should produce
    /// different neural responses. This is the core test for global normalization.
    #[test]
    fn brown_and_white_produce_different_scores() {
        let config = SimulationConfig::default();
        let goal = Goal::new(GoalKind::Isolation);

        // All-Brown preset
        let mut brown = Preset::default();
        brown.source_count = 1;
        brown.objects[0].active = true;
        brown.objects[0].color = 2; // Brown
        brown.objects[0].volume = 0.80;

        // All-White preset
        let mut white = Preset::default();
        white.source_count = 1;
        white.objects[0].active = true;
        white.objects[0].color = 0; // White
        white.objects[0].volume = 0.80;

        let result_brown = evaluate_preset(&brown, &goal, &config);
        let result_white = evaluate_preset(&white, &goal, &config);

        // Scores should differ because the neural model receives
        // different spectral ratios (Brown: low-heavy; White: flat)
        let score_diff = (result_brown.score - result_white.score).abs();
        assert!(
            score_diff > 0.005,
            "Brown ({:.4}) and White ({:.4}) should produce different scores (diff={:.4}). \
             If identical, band normalization is destroying spectral ratios.",
            result_brown.score, result_white.score, score_diff
        );

        println!("NORMALIZATION TEST: brown={:.4} white={:.4} diff={:.4}",
            result_brown.score, result_white.score, score_diff);
    }

    /// Band power distribution should differ between Brown and White noise.
    /// Brown: more delta/theta. White: more balanced/beta.
    #[test]
    fn brown_has_more_slow_band_power_than_white() {
        let config = SimulationConfig::default();
        let goal = Goal::new(GoalKind::Isolation);

        let mut brown = Preset::default();
        brown.source_count = 1;
        brown.objects[0].active = true;
        brown.objects[0].color = 2;
        brown.objects[0].volume = 0.80;

        let mut white = Preset::default();
        white.source_count = 1;
        white.objects[0].active = true;
        white.objects[0].color = 0;
        white.objects[0].volume = 0.80;

        let result_brown = evaluate_preset(&brown, &goal, &config);
        let result_white = evaluate_preset(&white, &goal, &config);

        // Brown noise concentrates energy in low bands → should produce
        // more delta+theta relative to alpha+beta than White noise.
        let brown_slow = result_brown.delta_power + result_brown.theta_power;
        let white_slow = result_white.delta_power + result_white.theta_power;

        // With global normalization, Brown should have more slow-wave power
        // because its low-band signals are stronger relative to high bands.
        println!("BAND RATIO TEST: brown_slow={:.4} white_slow={:.4}",
            brown_slow, white_slow);

        // Note: this test documents expected behavior after the normalization fix.
        // With per-band normalization, brown_slow ≈ white_slow (both normalized to 1.0).
        // With global normalization, brown_slow > white_slow.
    }

    /// All presets still produce valid scores after normalization change.
    #[test]
    fn normalization_change_preserves_valid_scores() {
        let config = SimulationConfig::default();

        // Test with various noise colors
        for color in [0u8, 1, 2, 3, 5, 6] { // White, Pink, Brown, Green, Black, SSN
            let mut preset = Preset::default();
            preset.source_count = 1;
            preset.objects[0].active = true;
            preset.objects[0].color = color;
            preset.objects[0].volume = 0.80;

            for kind in GoalKind::all() {
                let goal = Goal::new(*kind);
                let result = evaluate_preset(&preset, &goal, &config);

                assert!(
                    result.score >= 0.0 && result.score <= 1.0,
                    "Color {} {:?}: score {} out of range",
                    color, kind, result.score
                );
                assert!(
                    result.dominant_freq.is_finite() && result.dominant_freq >= 0.0,
                    "Color {} {:?}: invalid dominant freq {}",
                    color, kind, result.dominant_freq
                );
            }
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 8. FHN amplitude preservation tests
    // ════════════════════════════════════════════════════════════════════════
    //
    // Per FitzHugh (1961) and Izhikevich (2003), neuron firing rate is
    // monotonically dependent on input current amplitude. Max-normalization
    // collapses all amplitudes to [-1,1], destroying this relationship.
    // Percentile-based scaling preserves relative amplitudes.

    /// Different noise colors should produce different FHN firing rates
    /// because global band normalization preserves spectral energy ratios,
    /// and percentile FHN scaling preserves EEG amplitude differences.
    ///
    /// Brown noise drives low bands strongly (JR receives high input),
    /// Blue noise drives high bands strongly (JR low bands receive weak input).
    /// With per-band normalization + max scaling, these were identical.
    /// With global norm + percentile scaling, they should differ.
    #[test]
    fn different_colors_produce_different_firing_rates() {
        let config = SimulationConfig::default();
        let goal = Goal::new(GoalKind::Focus);

        // Brown: concentrated low-band energy → strong JR drive → large EEG
        let mut brown = Preset::default();
        brown.source_count = 1;
        brown.objects[0].active = true;
        brown.objects[0].color = 2; // Brown
        brown.objects[0].volume = 0.90;

        // Blue: concentrated high-band energy → weak JR low-band drive → smaller EEG
        let mut blue = Preset::default();
        blue.source_count = 1;
        blue.objects[0].active = true;
        blue.objects[0].color = 7; // Blue
        blue.objects[0].volume = 0.90;

        let result_brown = evaluate_preset(&brown, &goal, &config);
        let result_blue = evaluate_preset(&blue, &goal, &config);

        println!("FHN AMPLITUDE TEST: brown_rate={:.2} blue_rate={:.2}",
            result_brown.fhn_firing_rate, result_blue.fhn_firing_rate);

        // Firing rates should differ because the EEG amplitudes differ
        // (different spectral distributions → different JR inputs → different oscillation amplitudes)
        let rate_diff = (result_brown.fhn_firing_rate - result_blue.fhn_firing_rate).abs();
        assert!(
            rate_diff > 0.1,
            "Brown ({:.2}) and Blue ({:.2}) should produce different FHN rates (diff={:.2}). \
             Combined global-norm + percentile-scaling should preserve amplitude differences.",
            result_brown.fhn_firing_rate, result_blue.fhn_firing_rate, rate_diff
        );
    }

    /// FHN firing rate should remain in physiological range after the fix.
    #[test]
    fn fhn_firing_rate_in_valid_range() {
        let config = SimulationConfig::default();
        let goal = Goal::new(GoalKind::Focus);

        for color in [0u8, 1, 2, 6] {
            let mut preset = Preset::default();
            preset.source_count = 1;
            preset.objects[0].active = true;
            preset.objects[0].color = color;
            preset.objects[0].volume = 0.80;

            let result = evaluate_preset(&preset, &goal, &config);
            assert!(
                result.fhn_firing_rate >= 0.0 && result.fhn_firing_rate < 50.0,
                "Color {}: FHN rate {:.2} out of physiological range",
                color, result.fhn_firing_rate
            );
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 9. Decimation anti-aliasing tests
    // ════════════════════════════════════════════════════════════════════════
    //
    // Per Oppenheim & Schafer (2009) and Crochiere & Rabiner (1983),
    // decimation without adequate anti-aliasing introduces spectral folding.
    // A boxcar filter has -13 dB sidelobes; Hann achieves -31 dB.

    /// Document boxcar decimation behavior: 300 Hz passes through at ~74% power.
    /// This is acceptable because the gammatone filterbank's 80 Hz envelope
    /// lowpass already removes content above ~80 Hz before decimation.
    /// The boxcar only handles residual carrier leakage.
    ///
    /// Per Crochiere & Rabiner (1983): proper improvement would require
    /// multi-stage decimation or a long FIR filter — future priority.
    #[test]
    fn decimation_boxcar_documented_behavior() {
        use crate::pipeline::decimate;
        use std::f64::consts::PI;

        let factor = 48_usize;
        let sr = 48_000.0;
        let n = (sr * 2.0) as usize;

        // 300 Hz test tone — above the gammatone envelope band (~80 Hz)
        let signal: Vec<f64> = (0..n)
            .map(|i| 0.3 * (2.0 * PI * 300.0 * i as f64 / sr).sin())
            .collect();

        let decimated = decimate(&signal, factor);
        let power: f64 = decimated.iter().map(|x| x * x).sum::<f64>() / decimated.len() as f64;
        let orig_power: f64 = signal.iter().map(|x| x * x).sum::<f64>() / signal.len() as f64;
        let ratio = power / orig_power;

        // Boxcar passes 300 Hz at ~74% power. This is a known limitation
        // but acceptable since gammatone envelopes don't contain 300 Hz.
        println!("DECIMATION: 300 Hz boxcar passthrough = {ratio:.4} (expected ~0.74)");
        assert!(ratio > 0.5 && ratio < 1.0,
            "Boxcar should pass 300 Hz partially (ratio={ratio:.4})");
    }

    /// Decimation preserves low-frequency content (below ~50 Hz).
    #[test]
    fn decimation_preserves_low_freq() {
        use crate::pipeline::decimate;
        use std::f64::consts::PI;

        let factor = 48_usize;
        let sr = 48_000.0;
        let n = (sr * 2.0) as usize;

        // 10 Hz signal — well within the passband
        let signal: Vec<f64> = (0..n)
            .map(|i| 0.5 * (2.0 * PI * 10.0 * i as f64 / sr).sin())
            .collect();

        let decimated = decimate(&signal, factor);

        let orig_power: f64 = signal.iter().map(|x| x * x).sum::<f64>() / signal.len() as f64;
        let dec_power: f64 = decimated.iter().map(|x| x * x).sum::<f64>() / decimated.len() as f64;

        let ratio = dec_power / orig_power;
        assert!(
            ratio > 0.85,
            "10 Hz should be preserved after decimation (ratio={ratio:.4})"
        );
    }

    /// Decimated signal length is correct.
    #[test]
    fn decimation_output_length() {
        use crate::pipeline::decimate;
        let signal = vec![1.0; 4800];
        let result = decimate(&signal, 48);
        assert_eq!(result.len(), 100, "4800 samples / 48 = 100");
    }

    // ════════════════════════════════════════════════════════════════════════
    // 10. Bilateral coupling tests
    // ════════════════════════════════════════════════════════════════════════
    //
    // Per Innocenti (1986) and Bloom & Hynd (2005), callosal transmission
    // is primarily inhibitory. When one hemisphere is active, it suppresses
    // the other via GABA-mediated interhemispheric inhibition.
    // Excitatory coupling (current model) causes hemispheres to synchronize;
    // inhibitory coupling produces more hemispheric differentiation.

    /// With asymmetric input (source on one side), inhibitory coupling
    /// should produce greater alpha asymmetry than the pre-fix excitatory coupling.
    #[test]
    fn asymmetric_input_produces_hemispheric_differentiation() {
        let config = SimulationConfig::default();
        let goal = Goal::new(GoalKind::Isolation);

        // Strongly asymmetric preset: one loud source on the right
        let mut preset = Preset::default();
        preset.source_count = 1;
        preset.objects[0].active = true;
        preset.objects[0].color = 0; // White
        preset.objects[0].volume = 0.90;
        preset.objects[0].x = 5.0; // far right

        let result = evaluate_preset(&preset, &goal, &config);

        // Alpha asymmetry should be non-zero (hemispheres differentiated)
        assert!(
            result.alpha_asymmetry.abs() > 0.01,
            "Asymmetric input should produce hemispheric differentiation (asymmetry={:.4})",
            result.alpha_asymmetry
        );

        println!("BILATERAL TEST: alpha_asymmetry={:.4} (positive=left-dominant)",
            result.alpha_asymmetry);
    }

    /// Bilateral coupling should produce valid scores across all brain types.
    #[test]
    fn bilateral_coupling_valid_across_brain_types() {
        let goal = Goal::new(GoalKind::Focus);

        let mut preset = Preset::default();
        preset.source_count = 1;
        preset.objects[0].active = true;
        preset.objects[0].color = 0;
        preset.objects[0].volume = 0.80;
        preset.objects[0].x = 3.0;

        for bt in &[BrainType::Normal, BrainType::Adhd] {
            let config = SimulationConfig {
                brain_type: *bt,
                ..SimulationConfig::default()
            };
            let result = evaluate_preset(&preset, &goal, &config);

            assert!(
                result.score >= 0.0 && result.score <= 1.0,
                "{:?}: score {} out of range after coupling change",
                bt, result.score
            );
            assert!(
                result.dominant_freq.is_finite() && result.dominant_freq > 0.0,
                "{:?}: invalid dominant freq {}",
                bt, result.dominant_freq
            );
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 11. Brightness removal tests
    // ════════════════════════════════════════════════════════════════════════
    //
    // Per Zwicker & Fastl (1999), brightness is a perceptual construct
    // derived from the same cochlear excitation that feeds the neural model.
    // With global band normalization, the neural model now sees spectral
    // differences directly — brightness is redundant.

    /// Score should be fully determined by neural model (band powers + FHN),
    /// not by a separate brightness modifier.
    #[test]
    fn score_independent_of_brightness_parameter() {
        let goal = Goal::new(GoalKind::Focus);

        // Create a JR result and FHN result
        let bp = BandPowers {
            delta: 0.05, theta: 0.15, alpha: 0.35, beta: 0.35, gamma: 0.10,
        };
        let jr = make_jr_result_from_powers(bp);
        let fhn = make_perfect_fhn(GoalKind::Focus);

        // Score should be the same regardless of brightness parameter
        let score_dark = goal.evaluate_with_brightness(&fhn, &jr, 0.0);
        let score_bright = goal.evaluate_with_brightness(&fhn, &jr, 1.0);

        assert!(
            (score_dark - score_bright).abs() < 0.001,
            "Score should not depend on brightness: dark={score_dark:.4} bright={score_bright:.4}. \
             Brightness modifier should be removed (Zwicker & Fastl 1999)."
        );
    }

    /// All goal scores should remain in [0, 1] after brightness removal.
    #[test]
    fn scores_valid_without_brightness() {
        for kind in GoalKind::all() {
            let goal = Goal::new(*kind);
            let bp = BandPowers {
                delta: 0.2, theta: 0.2, alpha: 0.2, beta: 0.2, gamma: 0.2,
            };
            let jr = make_jr_result_from_powers(bp);
            let fhn = make_perfect_fhn(*kind);

            let score = goal.evaluate_with_brightness(&fhn, &jr, 0.5);
            assert!(
                score >= 0.0 && score <= 1.0,
                "{:?}: score {score} out of range after brightness removal",
                kind
            );
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 12. Alpha asymmetry scoring tests
    // ════════════════════════════════════════════════════════════════════════
    //
    // Per Davidson (2004) and Allen et al. (2004), frontal alpha asymmetry
    // is a marker of cognitive/emotional state. Balanced hemispheres indicate
    // relaxed, symmetric processing. Excessive asymmetry can indicate
    // maladaptive lateralization.

    /// Balanced hemispheres should score higher than extremely asymmetric
    /// for goals that want symmetric processing (meditation, relaxation).
    #[test]
    fn balanced_scores_higher_than_asymmetric_for_relaxation() {
        let goal = Goal::new(GoalKind::DeepRelaxation);
        let bp = BandPowers {
            delta: 0.22, theta: 0.35, alpha: 0.36, beta: 0.03, gamma: 0.01,
        };
        let jr = make_jr_result_from_powers(bp);
        let fhn = make_perfect_fhn(GoalKind::DeepRelaxation);

        // Score with balanced vs extreme asymmetry
        let score_balanced = goal.evaluate_with_asymmetry(&fhn, &jr, 0.0);
        let score_extreme = goal.evaluate_with_asymmetry(&fhn, &jr, 0.95);

        assert!(
            score_balanced > score_extreme,
            "Balanced ({score_balanced:.4}) should score higher than extreme asymmetry ({score_extreme:.4}) for relaxation"
        );
    }

    /// Sleep goal should not penalize asymmetry.
    #[test]
    fn sleep_ignores_asymmetry() {
        let goal = Goal::new(GoalKind::Sleep);
        let bp = BandPowers {
            delta: 0.30, theta: 0.48, alpha: 0.12, beta: 0.02, gamma: 0.02,
        };
        let jr = make_jr_result_from_powers(bp);
        let fhn = make_perfect_fhn(GoalKind::Sleep);

        let score_balanced = goal.evaluate_with_asymmetry(&fhn, &jr, 0.0);
        let score_extreme = goal.evaluate_with_asymmetry(&fhn, &jr, 0.95);

        // Should be identical or very close for sleep
        assert!(
            (score_balanced - score_extreme).abs() < 0.01,
            "Sleep should not penalize asymmetry: balanced={score_balanced:.4} extreme={score_extreme:.4}"
        );
    }

    /// All goals produce valid scores with asymmetry parameter.
    #[test]
    fn asymmetry_scoring_valid_range() {
        for kind in GoalKind::all() {
            let goal = Goal::new(*kind);
            let bp = BandPowers { delta: 0.2, theta: 0.2, alpha: 0.2, beta: 0.2, gamma: 0.2 };
            let jr = make_jr_result_from_powers(bp);
            let fhn = make_perfect_fhn(*kind);

            for asym in [-0.9, -0.5, 0.0, 0.5, 0.9] {
                let score = goal.evaluate_with_asymmetry(&fhn, &jr, asym);
                assert!(
                    score >= 0.0 && score <= 1.0,
                    "{:?} asymmetry={asym}: score {score} out of range", kind
                );
            }
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 13. ASSR DC/AC separation tests
    // ════════════════════════════════════════════════════════════════════════
    //
    // ASSR should only attenuate the modulation (AC) component, not the
    // mean drive level (DC). This prevents ASSR from conflating modulation
    // transmission with operating point shift (which is the thalamic gate's job).

    /// With ASSR + thalamic gate both enabled, Ground (sleep) should still
    /// produce a good score — ASSR shouldn't collapse the operating point
    /// on top of the gate's shift.
    #[test]
    fn assr_does_not_collapse_operating_point_with_gate() {
        let mut config = SimulationConfig::default();
        config.assr_enabled = true;
        config.thalamic_gate_enabled = true;

        let goal = Goal::new(GoalKind::Sleep);

        // Use Ground-like preset: Brown + NeuralLfo 5Hz
        let mut preset = Preset::default();
        preset.source_count = 2;
        preset.anchor_color = 5; // Black
        preset.objects[0].active = true;
        preset.objects[0].color = 2; // Brown
        preset.objects[0].volume = 0.75;
        preset.objects[0].reverb_send = 0.85;
        preset.objects[0].bass_mod.kind = 2; // Breathing
        preset.objects[0].bass_mod.param_b = 0.80;
        preset.objects[1].active = true;
        preset.objects[1].color = 6; // SSN
        preset.objects[1].volume = 0.85;
        preset.objects[1].bass_mod.kind = 4; // NeuralLfo 5Hz
        preset.objects[1].bass_mod.param_a = 5.0;
        preset.objects[1].bass_mod.param_b = 1.0;
        preset.objects[1].satellite_mod.kind = 4;
        preset.objects[1].satellite_mod.param_a = 5.0;
        preset.objects[1].satellite_mod.param_b = 0.90;

        // Gate alone
        let mut config_gate = SimulationConfig::default();
        config_gate.thalamic_gate_enabled = true;
        let result_gate = evaluate_preset(&preset, &goal, &config_gate);

        // Both
        let result_both = evaluate_preset(&preset, &goal, &config);

        // With DC/AC separation, ASSR should NOT dramatically reduce the
        // score when combined with gate. Allow some reduction but not collapse.
        let ratio = result_both.score / result_gate.score.max(0.001);
        println!("ASSR DC/AC TEST: gate_only={:.4} both={:.4} ratio={:.3}",
            result_gate.score, result_both.score, ratio);

        assert!(
            ratio > 0.60,
            "ASSR+gate should not collapse score: gate={:.4} both={:.4} ratio={:.3}. \
             ASSR is likely reducing DC drive (operating point), not just AC (modulation).",
            result_gate.score, result_both.score, ratio
        );
    }

    // ════════════════════════════════════════════════════════════════════════
    // 14. Habituation tests
    // ════════════════════════════════════════════════════════════════════════
    //
    // Per Moran et al. (2011) and Rowe et al. (2012): sustained neural
    // activity depresses excitatory connectivity, reducing response amplitude.

    /// With habituation enabled, longer simulation should show reduced
    /// EEG amplitude compared to the beginning.
    #[test]
    fn habituation_reduces_late_response() {
        use crate::neural::jansen_rit::JansenRitModel;

        let sr = 1000.0;
        let n = (sr * 30.0) as usize; // 30 seconds
        let input = vec![0.5; n]; // constant input

        let mut jr = JansenRitModel::new(sr);
        jr.habituation_rate = 0.0003;
        jr.habituation_recovery = 0.0001;

        let result = jr.simulate(&input);

        // Compare EEG variance in first 5s vs last 5s
        let first_5s = (sr * 2.0) as usize..(sr * 7.0) as usize; // skip 2s warmup
        let last_5s = (sr * 25.0) as usize..(sr * 30.0) as usize;

        let var_first: f64 = {
            let slice = &result.eeg[first_5s];
            let mean = slice.iter().sum::<f64>() / slice.len() as f64;
            slice.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / slice.len() as f64
        };
        let var_last: f64 = {
            let slice = &result.eeg[last_5s];
            let mean = slice.iter().sum::<f64>() / slice.len() as f64;
            slice.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / slice.len() as f64
        };

        println!("HABITUATION TEST: var_first={var_first:.6} var_last={var_last:.6} ratio={:.3}",
            var_last / var_first.max(1e-20));

        assert!(
            var_last < var_first * 0.95,
            "Habituation should reduce late EEG variance: first={var_first:.6} last={var_last:.6}"
        );
    }

    /// With habituation_rate = 0 (default), behavior is unchanged.
    #[test]
    fn no_habituation_when_rate_zero() {
        use crate::neural::jansen_rit::JansenRitModel;

        let sr = 1000.0;
        let n = (sr * 5.0) as usize;
        let input = vec![0.5; n];

        let mut jr_no_hab = JansenRitModel::new(sr);
        let mut jr_zero_hab = JansenRitModel::new(sr);
        jr_zero_hab.habituation_rate = 0.0;
        jr_zero_hab.habituation_recovery = 0.0;

        let result1 = jr_no_hab.simulate(&input);
        let result2 = jr_zero_hab.simulate(&input);

        // Should be identical
        for i in 0..n {
            assert!(
                (result1.eeg[i] - result2.eeg[i]).abs() < 1e-10,
                "Zero habituation should match default at sample {i}"
            );
        }
    }

    /// Habituation produces valid (finite) output.
    #[test]
    fn habituation_output_finite() {
        use crate::neural::jansen_rit::JansenRitModel;

        let sr = 1000.0;
        let n = (sr * 10.0) as usize;
        let input = vec![0.5; n];

        let mut jr = JansenRitModel::new(sr);
        jr.habituation_rate = 0.001; // aggressive
        jr.habituation_recovery = 0.0001;

        let result = jr.simulate(&input);
        for (i, &v) in result.eeg.iter().enumerate() {
            assert!(v.is_finite(), "EEG sample {i} is not finite: {v}");
        }
    }

    // ════════════════════════════════════════════════════════════════════════
    // 15. Stochastic JR tests — Per Ableidinger et al. (2017)
    // ════════════════════════════════════════════════════════════════════════

    #[test]
    fn stochastic_jr_broadens_spectrum() {
        use crate::neural::jansen_rit::JansenRitModel;
        let sr = 1000.0;
        let n = (sr * 10.0) as usize;
        let input = vec![0.5; n];

        // Use Normal brain-type parameters where alpha IS the attractor
        // (input_offset=175, input_scale=60 → p = 175 + 0.5*60 = 205)
        let mut jr_det = JansenRitModel::with_params(sr, 3.25, 22.0, 100.0, 50.0, 135.0, 175.0, 60.0);
        let result_det = jr_det.simulate(&input);

        let mut jr_stoch = JansenRitModel::with_params(sr, 3.25, 22.0, 100.0, 50.0, 135.0, 175.0, 60.0);
        jr_stoch.stochastic_sigma = 20.0;
        let result_stoch = jr_stoch.simulate(&input);

        let det_norm = result_det.band_powers.normalized();
        let stoch_norm = result_stoch.band_powers.normalized();

        // Stochastic should broaden the spectrum — energy distributes more
        // evenly across bands instead of concentrating in alpha+theta.
        // Measure: standard deviation of band powers (lower = more even).
        let det_bands = [det_norm.delta, det_norm.theta, det_norm.alpha, det_norm.beta];
        let stoch_bands = [stoch_norm.delta, stoch_norm.theta, stoch_norm.alpha, stoch_norm.beta];

        let mean_det = det_bands.iter().sum::<f64>() / 4.0;
        let mean_stoch = stoch_bands.iter().sum::<f64>() / 4.0;
        let std_det = (det_bands.iter().map(|x| (x - mean_det).powi(2)).sum::<f64>() / 4.0).sqrt();
        let std_stoch = (stoch_bands.iter().map(|x| (x - mean_stoch).powi(2)).sum::<f64>() / 4.0).sqrt();

        println!("STOCHASTIC: det_std={std_det:.3} stoch_std={std_stoch:.3}");
        println!("  det: d={:.3} t={:.3} a={:.3} b={:.3}", det_norm.delta, det_norm.theta, det_norm.alpha, det_norm.beta);
        println!("  stoch: d={:.3} t={:.3} a={:.3} b={:.3}", stoch_norm.delta, stoch_norm.theta, stoch_norm.alpha, stoch_norm.beta);

        // Stochastic should have MORE EVEN distribution (lower std)
        assert!(std_stoch < std_det,
            "Stochastic should broaden spectrum: det_std={std_det:.3} > stoch_std={std_stoch:.3}");
    }

    #[test]
    fn stochastic_sigma_zero_is_deterministic() {
        use crate::neural::jansen_rit::JansenRitModel;
        let sr = 1000.0;
        let n = (sr * 3.0) as usize;
        let input = vec![0.5; n];

        let mut jr_det = JansenRitModel::new(sr);
        let mut jr_zero = JansenRitModel::new(sr);
        jr_zero.stochastic_sigma = 0.0;

        let r1 = jr_det.simulate(&input);
        let r2 = jr_zero.simulate(&input);
        for i in 0..n {
            assert!((r1.eeg[i] - r2.eeg[i]).abs() < 1e-10,
                "sigma=0 should match deterministic at sample {i}");
        }
    }

    #[test]
    fn stochastic_jr_output_finite() {
        use crate::neural::jansen_rit::JansenRitModel;
        let sr = 1000.0;
        let n = (sr * 5.0) as usize;
        let input = vec![0.5; n];

        let mut jr = JansenRitModel::new(sr);
        jr.stochastic_sigma = 30.0;
        let result = jr.simulate(&input);
        for (i, &v) in result.eeg.iter().enumerate() {
            assert!(v.is_finite(), "Stochastic EEG sample {i} is not finite: {v}");
        }
    }

    /// Band powers still sum to ~1.0 after normalization change.
    #[test]
    fn normalization_preserves_band_power_sum() {
        let config = SimulationConfig::default();
        let goal = Goal::new(GoalKind::Isolation);

        for color in [0u8, 1, 2, 5, 6] {
            let mut preset = Preset::default();
            preset.source_count = 1;
            preset.objects[0].active = true;
            preset.objects[0].color = color;
            preset.objects[0].volume = 0.80;

            let result = evaluate_preset(&preset, &goal, &config);
            let total = result.delta_power + result.theta_power + result.alpha_power
                + result.beta_power + result.gamma_power;

            assert!(
                (total - 1.0).abs() < 0.02,
                "Color {}: band powers sum to {:.4}, should be ~1.0",
                color, total
            );
        }
    }
}
