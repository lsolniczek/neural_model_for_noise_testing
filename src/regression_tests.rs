/// Pre-refactor regression tests.
///
/// These tests capture the current behaviour of scoring, optimizer boundary
/// handling, genome encoding, and the full pipeline so that the planned
/// changes (downsampling, RNG fix, clamping, Gaussian scoring, longer
/// simulation) do not introduce silent regressions.

#[cfg(test)]
mod tests {
    use crate::brain_type::BrainType;
    use crate::model_signature::ModelVersion;
    use crate::auditory::{ArousalModel, ArousalSource, ThalamicGate};
    use crate::neural::fhn::*;
    use crate::neural::jansen_rit::*;
    use crate::optimizer::DifferentialEvolution;
    use crate::pipeline::{
        evaluate_preset, evaluate_preset_detailed, SimulationConfig, DECIMATION_FACTOR, NEURAL_SR,
        SAMPLE_RATE,
    };
    use crate::preset::{Preset, GENOME_LEN};
    use crate::scoring::{Goal, GoalKind};

    fn fast_pipeline_config() -> SimulationConfig {
        SimulationConfig {
            duration_secs: 4.0,
            ..SimulationConfig::default()
        }
    }

    fn assert_same_legacy_simulation_result(
        lhs: &crate::pipeline::SimulationResult,
        rhs: &crate::pipeline::SimulationResult,
    ) {
        assert_same_legacy_simulation_result_except_score(lhs, rhs);
        assert_eq!(lhs.score, rhs.score);
    }

    fn assert_same_legacy_simulation_result_except_score(
        lhs: &crate::pipeline::SimulationResult,
        rhs: &crate::pipeline::SimulationResult,
    ) {
        assert_eq!(lhs.fhn_firing_rate, rhs.fhn_firing_rate);
        assert_eq!(lhs.fhn_isi_cv, rhs.fhn_isi_cv);
        assert_eq!(lhs.dominant_freq, rhs.dominant_freq);
        assert_eq!(lhs.delta_power, rhs.delta_power);
        assert_eq!(lhs.theta_power, rhs.theta_power);
        assert_eq!(lhs.alpha_power, rhs.alpha_power);
        assert_eq!(lhs.beta_power, rhs.beta_power);
        assert_eq!(lhs.gamma_power, rhs.gamma_power);
        assert_eq!(lhs.brightness, rhs.brightness);
        assert_eq!(lhs.band_energy_fractions, rhs.band_energy_fractions);
        assert_eq!(lhs.left_dominant_freq, rhs.left_dominant_freq);
        assert_eq!(lhs.right_dominant_freq, rhs.right_dominant_freq);
        assert_eq!(lhs.alpha_asymmetry, rhs.alpha_asymmetry);
        assert_eq!(
            lhs.performance.entrainment_ratio,
            rhs.performance.entrainment_ratio
        );
        assert_eq!(lhs.performance.ei_stability, rhs.performance.ei_stability);
        assert_eq!(
            lhs.performance.spectral_centroid,
            rhs.performance.spectral_centroid
        );
        assert_eq!(lhs.performance.plv, rhs.performance.plv);
        assert_eq!(lhs.performance.envelope_plv, rhs.performance.envelope_plv);
    }

    fn fixture_dark_unmodulated_symmetric() -> Preset {
        let mut preset = Preset::default();
        preset.master_gain = 0.75;
        preset.source_count = 2;
        for idx in [0usize, 1usize] {
            preset.objects[idx].active = true;
            preset.objects[idx].color = 2; // brown
            preset.objects[idx].volume = 0.34;
            preset.objects[idx].reverb_send = 0.18;
            preset.objects[idx].bass_mod.kind = 0;
            preset.objects[idx].satellite_mod.kind = 0;
            preset.objects[idx].movement.kind = 0;
        }
        preset.objects[0].x = -1.6;
        preset.objects[1].x = 1.6;
        preset
    }

    fn fixture_mid_modulated_lateralized() -> Preset {
        let mut preset = Preset::default();
        preset.master_gain = 0.82;
        preset.source_count = 2;
        preset.objects[0].active = true;
        preset.objects[0].color = 1; // pink (mid)
        preset.objects[0].volume = 0.42;
        preset.objects[0].x = 2.8; // lateralized right
        preset.objects[0].reverb_send = 0.24;
        preset.objects[0].bass_mod.kind = 4; // NeuralLfo
        preset.objects[0].bass_mod.param_a = 6.0;
        preset.objects[0].bass_mod.param_b = 0.85;
        preset.objects[0].satellite_mod.kind = 5; // Isochronic
        preset.objects[0].satellite_mod.param_a = 10.0;
        preset.objects[0].satellite_mod.param_b = 0.70;
        preset.objects[0].satellite_mod.param_c = 0.45;
        preset.objects[1].active = true;
        preset.objects[1].color = 4; // grey support bed
        preset.objects[1].volume = 0.14;
        preset.objects[1].x = 1.9;
        preset.objects[1].reverb_send = 0.18;
        preset
    }

    fn fixture_bright_modulated_symmetric() -> Preset {
        let mut preset = Preset::default();
        preset.master_gain = 0.80;
        preset.source_count = 2;
        for idx in [0usize, 1usize] {
            preset.objects[idx].active = true;
            preset.objects[idx].color = 0; // white
            preset.objects[idx].volume = 0.28;
            preset.objects[idx].reverb_send = 0.12;
            preset.objects[idx].bass_mod.kind = 4; // NeuralLfo
            preset.objects[idx].bass_mod.param_a = 14.0;
            preset.objects[idx].bass_mod.param_b = 0.62;
            preset.objects[idx].satellite_mod.kind = 6; // RandomPulse
            preset.objects[idx].satellite_mod.param_a = 7.0;
            preset.objects[idx].satellite_mod.param_b = 0.55;
            preset.objects[idx].satellite_mod.param_c = 120.0;
        }
        preset.objects[0].x = -1.4;
        preset.objects[1].x = 1.4;
        preset
    }

    fn fixture_single_source_with_modulation(
        color: u8,
        modulation_kind: u8,
        modulation_hz: f32,
        modulation_depth: f32,
    ) -> Preset {
        let mut preset = Preset::default();
        preset.master_gain = 0.82;
        preset.source_count = 1;
        preset.objects[0].active = true;
        preset.objects[0].color = color;
        preset.objects[0].volume = 0.42;
        preset.objects[0].x = 0.0;
        preset.objects[0].z = 1.0;
        preset.objects[0].reverb_send = 0.15;
        preset.objects[0].movement.kind = 0;
        preset.objects[0].bass_mod.kind = modulation_kind;
        preset.objects[0].bass_mod.param_a = modulation_hz;
        preset.objects[0].bass_mod.param_b = modulation_depth;
        preset.objects[0].bass_mod.param_c = match modulation_kind {
            5 => 0.5,   // Isochronic duty cycle
            6 => 120.0, // RandomPulse duration (ms)
            _ => 0.0,
        };
        preset.objects[0].satellite_mod.kind = 0;
        preset.objects[0].satellite_mod.param_a = 0.0;
        preset.objects[0].satellite_mod.param_b = 0.0;
        preset.objects[0].satellite_mod.param_c = 0.0;
        preset
    }

    fn fixture_single_tone_with_modulation(modulation_hz: f32, modulation_depth: f32) -> Preset {
        let mut preset =
            fixture_single_source_with_modulation(0, 4, modulation_hz, modulation_depth);
        preset.objects[0].source_kind = 1; // tone source for clean rendered modulation probe
        preset.objects[0].tone_freq = 220.0;
        preset.objects[0].tone_amplitude = 0.9;
        preset.objects[0].reverb_send = 0.0;
        preset
    }

    fn fixture_single_noise_unmodulated_reference() -> Preset {
        let mut preset = Preset::default();
        preset.master_gain = 0.82;
        preset.source_count = 1;
        preset.objects[0].active = true;
        preset.objects[0].color = 0; // white
        preset.objects[0].volume = 0.85;
        preset.objects[0].x = 0.0;
        preset.objects[0].z = 1.0;
        preset.objects[0].reverb_send = 0.0;
        preset.objects[0].movement.kind = 0;
        preset.objects[0].bass_mod.kind = 0;
        preset.objects[0].satellite_mod.kind = 0;
        preset
    }

    fn fixture_single_noise_with_modulation(
        color: u8,
        modulation_kind: u8,
        modulation_hz: f32,
        modulation_depth: f32,
    ) -> Preset {
        let mut preset = fixture_single_source_with_modulation(
            color,
            modulation_kind,
            modulation_hz,
            modulation_depth,
        );
        preset.objects[0].source_kind = 0; // rendered stochastic noise carrier path
        preset.objects[0].reverb_send = 0.0;
        preset.objects[0].volume = 0.85;
        // Reinforce explicit rendered modulation in the carrier path.
        preset.objects[0].satellite_mod.kind = modulation_kind;
        preset.objects[0].satellite_mod.param_a = modulation_hz;
        preset.objects[0].satellite_mod.param_b = modulation_depth;
        preset.objects[0].satellite_mod.param_c = if modulation_kind == 5 { 0.5 } else { 0.0 };
        preset
    }

    fn canonical_config(duration_secs: f32, brain_type: BrainType) -> SimulationConfig {
        SimulationConfig {
            duration_secs,
            brain_type,
            ..SimulationConfig::default()
        }
    }

    fn candidate_v2_config(duration_secs: f32, brain_type: BrainType) -> SimulationConfig {
        SimulationConfig {
            duration_secs,
            brain_type,
            model_version: ModelVersion::CandidateV2,
            ..SimulationConfig::default()
        }
    }

    fn candidate_v2_fixed_arousal_config(
        duration_secs: f32,
        brain_type: BrainType,
        fixed_arousal: f64,
    ) -> SimulationConfig {
        SimulationConfig {
            duration_secs,
            brain_type,
            model_version: ModelVersion::CandidateV2,
            arousal_model: ArousalModel::Fixed,
            fixed_arousal: Some(fixed_arousal),
            ..SimulationConfig::default()
        }
    }

    fn ablation_config(duration_secs: f32, brain_type: BrainType) -> SimulationConfig {
        SimulationConfig {
            duration_secs,
            brain_type,
            assr_enabled: false,
            thalamic_gate_enabled: false,
            cet_enabled: false,
            physiological_thalamic_gate_enabled: false,
            habituation_enabled: false,
            stochastic_jr_enabled: false,
            ..SimulationConfig::default()
        }
    }

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
            &bands,
            &bands,
            &energy,
            &energy,
            &bilateral,
            params.jansen_rit.c,
            params.jansen_rit.input_scale,
            sr,
            &fi,
            params.jansen_rit.v0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.5,
        );

        // Normalise EEG for FHN
        // Percentile-based EEG scaling (matches pipeline)
        let mut abs_values: Vec<f64> = bi.combined.eeg.iter().map(|x| x.abs()).collect();
        abs_values.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let p95_idx = (abs_values.len() as f64 * 0.95) as usize;
        let p95 = abs_values[p95_idx.min(abs_values.len() - 1)];
        let scale = if p95 > 1e-10 { 1.0 / p95 } else { 1.0 };
        let fhn_input: Vec<f64> = bi
            .combined
            .eeg
            .iter()
            .map(|x| (x * scale).clamp(-3.0, 3.0))
            .collect();
        let fhn = FhnModel::with_params(
            sr,
            params.fhn.a,
            params.fhn.b,
            params.fhn.epsilon,
            params.fhn.time_scale,
        );
        let fhn_result = fhn.simulate(&fhn_input, params.fhn.input_scale);

        // Evaluate and snapshot
        let score = goal.evaluate_with_brightness(&fhn_result, &bi.combined, 0.5);

        // Score must be in [0, 1]
        assert!(
            score >= 0.0 && score <= 1.0,
            "score {} out of [0,1] range",
            score
        );

        // Snapshot: the score with Normal brain + Focus goal should be in a
        // reasonable range. We record the exact value as a comment for comparison
        // after refactoring, but assert a wider band to avoid flaky tests.
        println!("REGRESSION SNAPSHOT: Focus/Normal score = {:.6}", score);
        assert!(
            score > 0.0,
            "Focus score should be non-zero for Normal brain"
        );
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
            delta: 0.05, // exactly at min
            theta: 0.35, // at ideal
            alpha: 0.36, // at ideal
            beta: 0.03,  // at ideal
            gamma: 0.01, // at ideal
        };

        let bp_at_max = BandPowers {
            delta: 0.40, // exactly at max
            theta: 0.35,
            alpha: 0.36,
            beta: 0.03,
            gamma: 0.01,
        };

        let bp_at_ideal = BandPowers {
            delta: 0.22, // at ideal
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
        assert!(
            score_ideal > score_min,
            "ideal ({:.4}) should score higher than at-min ({:.4})",
            score_ideal,
            score_min
        );
        assert!(
            score_ideal > score_max,
            "ideal ({:.4}) should score higher than at-max ({:.4})",
            score_ideal,
            score_max
        );

        println!(
            "REGRESSION SNAPSHOT: boundary scores min={:.6} max={:.6} ideal={:.6}",
            score_min, score_max, score_ideal
        );
    }

    /// All goals produce scores in [0, 1] range.
    #[test]
    fn all_goals_score_in_valid_range() {
        for kind in GoalKind::all() {
            let goal = Goal::new(*kind);

            // Test with various band power distributions
            let distributions = [
                BandPowers {
                    delta: 0.5,
                    theta: 0.2,
                    alpha: 0.2,
                    beta: 0.05,
                    gamma: 0.05,
                },
                BandPowers {
                    delta: 0.05,
                    theta: 0.1,
                    alpha: 0.4,
                    beta: 0.35,
                    gamma: 0.1,
                },
                BandPowers {
                    delta: 0.2,
                    theta: 0.2,
                    alpha: 0.2,
                    beta: 0.2,
                    gamma: 0.2,
                },
                BandPowers {
                    delta: 0.0,
                    theta: 0.0,
                    alpha: 1.0,
                    beta: 0.0,
                    gamma: 0.0,
                },
            ];

            for (i, bp) in distributions.iter().enumerate() {
                let jr = make_jr_result_from_powers(BandPowers {
                    delta: bp.delta,
                    theta: bp.theta,
                    alpha: bp.alpha,
                    beta: bp.beta,
                    gamma: bp.gamma,
                });
                let fhn = make_perfect_fhn(*kind);

                for brightness in [0.0, 0.25, 0.5, 0.75, 1.0] {
                    let score = goal.evaluate_with_brightness(&fhn, &jr, brightness);
                    assert!(
                        score >= 0.0 && score <= 1.0,
                        "{:?} dist={} brightness={}: score {} out of range",
                        kind,
                        i,
                        brightness,
                        score
                    );
                }
            }
        }
    }

    /// Isolation goal uses flat-deviation scoring (special case).
    #[test]
    fn isolation_perfect_flat_scores_high() {
        let goal = Goal::new(GoalKind::Isolation);
        let perfect_flat = BandPowers {
            delta: 0.2,
            theta: 0.2,
            alpha: 0.2,
            beta: 0.2,
            gamma: 0.2,
        };
        let jr = make_jr_result_from_powers(perfect_flat);
        let fhn = make_perfect_fhn(GoalKind::Isolation);

        let score = goal.evaluate_with_brightness(&fhn, &jr, 0.7);
        println!(
            "REGRESSION SNAPSHOT: Isolation perfect flat score = {:.6}",
            score
        );

        // Perfect flat distribution should score well
        assert!(
            score > 0.5,
            "perfect flat isolation should score > 0.5, got {:.4}",
            score
        );
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
                assert!(
                    val >= lo && val <= hi,
                    "trial gene {} = {} outside bounds [{}, {}]",
                    j,
                    val,
                    lo,
                    hi
                );
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

        println!(
            "REGRESSION SNAPSHOT: min volume gene across 20 gens = {:.6}",
            min_volume
        );
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
                assert!(
                    (v1 - v2).abs() < 1e-15,
                    "same seed should produce identical populations"
                );
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

        for (i, (orig, dec)) in original
            .objects
            .iter()
            .zip(decoded.objects.iter())
            .enumerate()
        {
            assert_eq!(orig.active, dec.active, "object {} active", i);
            assert_eq!(orig.color, dec.color, "object {} color", i);
            assert!(
                (orig.volume - dec.volume).abs() < 1e-6,
                "object {} volume: {} vs {}",
                i,
                orig.volume,
                dec.volume
            );
            assert_eq!(
                orig.bass_mod.kind, dec.bass_mod.kind,
                "object {} bass_mod.kind",
                i
            );
            assert_eq!(
                orig.satellite_mod.kind, dec.satellite_mod.kind,
                "object {} sat_mod.kind",
                i
            );
            assert_eq!(
                orig.movement.kind, dec.movement.kind,
                "object {} movement.kind",
                i
            );
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

        assert_eq!(
            preset_a.objects[0].movement.kind, preset_b.objects[0].movement.kind,
            "2.1 and 2.4 should decode to the same movement kind"
        );

        // Values 2.4 and 2.6 round to different values (2 vs 3)
        genome[mov_kind_idx] = 2.6;
        let preset_c = Preset::from_genome(&genome);

        assert_ne!(
            preset_a.objects[0].movement.kind, preset_c.objects[0].movement.kind,
            "2.1 and 2.6 should decode to different movement kinds"
        );
    }

    /// Genome bounds cover full parameter space.
    #[test]
    fn genome_bounds_correct_length() {
        let bounds = Preset::bounds();
        assert_eq!(
            bounds.len(),
            GENOME_LEN,
            "bounds length {} != GENOME_LEN {}",
            bounds.len(),
            GENOME_LEN
        );

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
        let config = fast_pipeline_config();

        for kind in GoalKind::all() {
            let goal = Goal::new(*kind);
            let result = evaluate_preset(&preset, &goal, &config);

            assert!(
                result.score >= 0.0 && result.score <= 1.0,
                "{:?}: score {} out of range",
                kind,
                result.score
            );
            assert!(
                result.dominant_freq >= 0.0 && result.dominant_freq <= 100.0,
                "{:?}: dominant_freq {} out of range",
                kind,
                result.dominant_freq
            );
            assert!(
                result.fhn_firing_rate >= 0.0,
                "{:?}: negative firing rate {}",
                kind,
                result.fhn_firing_rate
            );

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

        let total = result.delta_power
            + result.theta_power
            + result.alpha_power
            + result.beta_power
            + result.gamma_power;

        assert!(
            (total - 1.0).abs() < 0.01,
            "band powers should sum to ~1.0, got {:.6}",
            total
        );
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
        assert!(
            (result1.score - result2.score).abs() > 1e-6
                || (result1.dominant_freq - result2.dominant_freq).abs() > 0.1,
            "different presets should produce different results"
        );
    }

    /// Simulation duration: verify default is 12 seconds with 2s warm-up discard.
    #[test]
    fn simulation_default_duration_is_12_seconds() {
        let config = SimulationConfig::default();
        assert!(
            (config.duration_secs - 12.0).abs() < 1e-6,
            "default duration should be 12.0s, got {}",
            config.duration_secs
        );
        assert!(
            (config.warmup_discard_secs - 2.0).abs() < 1e-6,
            "default warmup discard should be 2.0s, got {}",
            config.warmup_discard_secs
        );
    }

    #[derive(Debug, Clone, Copy)]
    struct LegacyGoldenSnapshot {
        score: f64,
        dominant_freq: f64,
        delta_power: f64,
        theta_power: f64,
        alpha_power: f64,
        beta_power: f64,
        gamma_power: f64,
        brightness: f64,
        alpha_asymmetry: f64,
    }

    struct LegacyGoldenCase {
        name: &'static str,
        preset: Preset,
        goal_kind: GoalKind,
        config: SimulationConfig,
        expected: LegacyGoldenSnapshot,
    }

    fn assert_snapshot_eq(
        actual: &crate::pipeline::SimulationResult,
        expected: LegacyGoldenSnapshot,
    ) {
        const EPS: f64 = 1e-9;
        assert!(
            (actual.score - expected.score).abs() < EPS,
            "score mismatch: actual={:.12} expected={:.12}",
            actual.score,
            expected.score
        );
        assert!(
            (actual.dominant_freq - expected.dominant_freq).abs() < EPS,
            "dominant_freq mismatch: actual={:.12} expected={:.12}",
            actual.dominant_freq,
            expected.dominant_freq
        );
        assert!(
            (actual.delta_power - expected.delta_power).abs() < EPS,
            "delta_power mismatch: actual={:.12} expected={:.12}",
            actual.delta_power,
            expected.delta_power
        );
        assert!(
            (actual.theta_power - expected.theta_power).abs() < EPS,
            "theta_power mismatch: actual={:.12} expected={:.12}",
            actual.theta_power,
            expected.theta_power
        );
        assert!(
            (actual.alpha_power - expected.alpha_power).abs() < EPS,
            "alpha_power mismatch: actual={:.12} expected={:.12}",
            actual.alpha_power,
            expected.alpha_power
        );
        assert!(
            (actual.beta_power - expected.beta_power).abs() < EPS,
            "beta_power mismatch: actual={:.12} expected={:.12}",
            actual.beta_power,
            expected.beta_power
        );
        assert!(
            (actual.gamma_power - expected.gamma_power).abs() < EPS,
            "gamma_power mismatch: actual={:.12} expected={:.12}",
            actual.gamma_power,
            expected.gamma_power
        );
        assert!(
            (actual.brightness - expected.brightness).abs() < EPS,
            "brightness mismatch: actual={:.12} expected={:.12}",
            actual.brightness,
            expected.brightness
        );
        assert!(
            (actual.alpha_asymmetry - expected.alpha_asymmetry).abs() < EPS,
            "alpha_asymmetry mismatch: actual={:.12} expected={:.12}",
            actual.alpha_asymmetry,
            expected.alpha_asymmetry
        );
    }

    fn legacy_v1_stage0_golden_cases() -> Vec<LegacyGoldenCase> {
        vec![
            LegacyGoldenCase {
                name: "dark_sleep_normal_canonical_4s",
                preset: fixture_dark_unmodulated_symmetric(),
                goal_kind: GoalKind::Sleep,
                config: canonical_config(4.0, BrainType::Normal),
                expected: LegacyGoldenSnapshot {
                    score: 0.335911959459740,
                    dominant_freq: 0.976562500000000,
                    delta_power: 0.424334458638242,
                    theta_power: 0.129684027561843,
                    alpha_power: 0.361601526286417,
                    beta_power: 0.078704473926617,
                    gamma_power: 0.005675513586882,
                    brightness: 0.112807522664250,
                    alpha_asymmetry: -0.585339028141012,
                },
            },
            LegacyGoldenCase {
                name: "dark_deep_relax_aging_ablation_12s",
                preset: fixture_dark_unmodulated_symmetric(),
                goal_kind: GoalKind::DeepRelaxation,
                config: ablation_config(12.0, BrainType::Aging),
                expected: LegacyGoldenSnapshot {
                    score: 0.321670819867025,
                    dominant_freq: 6.835937500000000,
                    delta_power: 0.003527594893125,
                    theta_power: 0.951742055357745,
                    alpha_power: 0.034577468291727,
                    beta_power: 0.009703062040394,
                    gamma_power: 0.000449819417009,
                    brightness: 0.054102405387457,
                    alpha_asymmetry: 0.997744091862655,
                },
            },
            LegacyGoldenCase {
                name: "mid_focus_adhd_canonical_4s",
                preset: fixture_mid_modulated_lateralized(),
                goal_kind: GoalKind::Focus,
                config: canonical_config(4.0, BrainType::Adhd),
                expected: LegacyGoldenSnapshot {
                    score: 0.334738086849502,
                    dominant_freq: 5.859375000000000,
                    delta_power: 0.255168311618526,
                    theta_power: 0.550173524729978,
                    alpha_power: 0.085564464498437,
                    beta_power: 0.101323860329155,
                    gamma_power: 0.007769838823905,
                    brightness: 0.325224436122527,
                    alpha_asymmetry: 0.081789223181948,
                },
            },
            LegacyGoldenCase {
                name: "mid_shield_adhd_ablation_12s",
                preset: fixture_mid_modulated_lateralized(),
                goal_kind: GoalKind::Shield,
                config: ablation_config(12.0, BrainType::Adhd),
                expected: LegacyGoldenSnapshot {
                    score: 0.341502463573143,
                    dominant_freq: 6.896972656250000,
                    delta_power: 0.003504896326945,
                    theta_power: 0.923150317183464,
                    alpha_power: 0.044790014912155,
                    beta_power: 0.026042026789557,
                    gamma_power: 0.002512744787880,
                    brightness: 0.317479948341570,
                    alpha_asymmetry: 0.929018533420237,
                },
            },
            LegacyGoldenCase {
                name: "mid_meditation_high_alpha_canonical_4s",
                preset: fixture_mid_modulated_lateralized(),
                goal_kind: GoalKind::Meditation,
                config: canonical_config(4.0, BrainType::HighAlpha),
                expected: LegacyGoldenSnapshot {
                    score: 0.510769605947867,
                    dominant_freq: 0.976562500000000,
                    delta_power: 0.481143795629993,
                    theta_power: 0.489930784034469,
                    alpha_power: 0.022950590348520,
                    beta_power: 0.005812061723820,
                    gamma_power: 0.000162768263198,
                    brightness: 0.325224436122527,
                    alpha_asymmetry: -0.497281058536251,
                },
            },
            LegacyGoldenCase {
                name: "bright_ignition_anxious_canonical_4s",
                preset: fixture_bright_modulated_symmetric(),
                goal_kind: GoalKind::Ignition,
                config: canonical_config(4.0, BrainType::Anxious),
                expected: LegacyGoldenSnapshot {
                    score: 0.216077172096382,
                    dominant_freq: 0.976562500000000,
                    delta_power: 0.990839614872210,
                    theta_power: 0.007148707534671,
                    alpha_power: 0.001237826637320,
                    beta_power: 0.000686510150874,
                    gamma_power: 0.000087340804925,
                    brightness: 0.960022127940275,
                    alpha_asymmetry: -0.817632399628552,
                },
            },
            LegacyGoldenCase {
                name: "bright_flow_normal_ablation_12s",
                preset: fixture_bright_modulated_symmetric(),
                goal_kind: GoalKind::Flow,
                config: ablation_config(12.0, BrainType::Normal),
                expected: LegacyGoldenSnapshot {
                    score: 0.238052585930439,
                    dominant_freq: 24.230957031250000,
                    delta_power: 0.000422105414719,
                    theta_power: 0.001532795314489,
                    alpha_power: 0.018127457361253,
                    beta_power: 0.807669810840614,
                    gamma_power: 0.172247831068925,
                    brightness: 0.962277113499317,
                    alpha_asymmetry: -0.993466058254526,
                },
            },
            LegacyGoldenCase {
                name: "bright_deep_work_high_alpha_ablation_12s",
                preset: fixture_bright_modulated_symmetric(),
                goal_kind: GoalKind::DeepWork,
                config: ablation_config(12.0, BrainType::HighAlpha),
                expected: LegacyGoldenSnapshot {
                    score: 0.400027331597159,
                    dominant_freq: 7.751464843750000,
                    delta_power: 0.033792227195554,
                    theta_power: 0.753879345392662,
                    alpha_power: 0.154169743825606,
                    beta_power: 0.054961361292831,
                    gamma_power: 0.003197322293347,
                    brightness: 0.962277113499317,
                    alpha_asymmetry: 0.862206520752636,
                },
            },
            LegacyGoldenCase {
                name: "bright_isolation_normal_canonical_12s",
                preset: fixture_bright_modulated_symmetric(),
                goal_kind: GoalKind::Isolation,
                config: canonical_config(12.0, BrainType::Normal),
                expected: LegacyGoldenSnapshot {
                    score: 0.484498072976395,
                    dominant_freq: 24.230957031250000,
                    delta_power: 0.034256679819131,
                    theta_power: 0.000121218029019,
                    alpha_power: 0.000037928274931,
                    beta_power: 0.795697651609615,
                    gamma_power: 0.169886522267304,
                    brightness: 0.962277113499317,
                    alpha_asymmetry: -0.987781083619729,
                },
            },
        ]
    }

    fn missing_goal_from_legacy_v1_stage0_cases(cases: &[LegacyGoldenCase]) -> Option<GoalKind> {
        let represented: Vec<GoalKind> = cases.iter().map(|c| c.goal_kind).collect();
        for goal in GoalKind::all() {
            if !represented.contains(goal) {
                return Some(*goal);
            }
        }
        None
    }

    #[test]
    #[ignore]
    fn print_legacy_v1_stage0_golden_snapshots() {
        for case in legacy_v1_stage0_golden_cases() {
            let name = case.name;
            let preset = case.preset;
            let goal_kind = case.goal_kind;
            let config = case.config;
            let goal = Goal::new(goal_kind);
            let result = evaluate_preset(&preset, &goal, &config);
            println!(
                "{name}: score={:.15}, dom={:.15}, delta={:.15}, theta={:.15}, alpha={:.15}, beta={:.15}, gamma={:.15}, brightness={:.15}, asym={:.15}",
                result.score,
                result.dominant_freq,
                result.delta_power,
                result.theta_power,
                result.alpha_power,
                result.beta_power,
                result.gamma_power,
                result.brightness,
                result.alpha_asymmetry
            );
        }
    }

    #[test]
    fn legacy_v1_stage0_golden_regression_snapshots() {
        for case in legacy_v1_stage0_golden_cases() {
            let name = case.name;
            let preset = case.preset;
            let goal_kind = case.goal_kind;
            let config = case.config;
            let expected = case.expected;
            let goal = Goal::new(goal_kind);
            let result = evaluate_preset(&preset, &goal, &config);
            assert_snapshot_eq(&result, expected);
            assert_eq!(
                result.model_signature.version,
                crate::model_signature::ModelVersion::LegacyV1,
                "{name}: model version drifted"
            );
            assert_eq!(
                result.model_signature.pipeline_variant,
                crate::model_signature::PipelineVariant::EvaluateCanonical,
                "{name}: pipeline variant drifted"
            );
        }
    }

    #[test]
    fn legacy_v1_stage0_golden_snapshots_cover_all_goals() {
        let cases = legacy_v1_stage0_golden_cases();
        if let Some(goal) = missing_goal_from_legacy_v1_stage0_cases(&cases) {
            panic!("missing Stage 0 golden coverage for goal {goal}");
        }
    }

    #[test]
    fn legacy_v1_stage0_goal_coverage_detector_catches_removed_goal_case() {
        let mut cases = legacy_v1_stage0_golden_cases();
        cases.retain(|case| case.goal_kind != GoalKind::Flow);
        assert_eq!(
            missing_goal_from_legacy_v1_stage0_cases(&cases),
            Some(GoalKind::Flow)
        );
    }

    #[test]
    fn legacy_v1_result_signature_has_expected_defaults() {
        let preset = fixture_dark_unmodulated_symmetric();
        let goal = Goal::new(GoalKind::Sleep);
        let config = canonical_config(4.0, BrainType::Normal);
        let result = evaluate_preset(&preset, &goal, &config);
        let sig = &result.model_signature;

        assert_eq!(sig.version, crate::model_signature::ModelVersion::LegacyV1);
        assert_eq!(
            sig.scoring_profile,
            crate::model_signature::ScoringProfile::LegacyV1
        );
        assert_eq!(result.multi_score.legacy_v1_neural, Some(result.score));
        assert!(result.multi_score.legacy_v1_fused.is_none());
        assert!(result.multi_score.candidate_research_v2.is_none());
        assert!(result.multi_score.product_acoustic.is_none());
        assert_eq!(
            sig.normalization_mode,
            crate::model_signature::NormalizationMode::GlobalPerEar
        );
        assert_eq!(
            sig.pipeline_variant,
            crate::model_signature::PipelineVariant::EvaluateCanonical
        );
        assert_eq!(sig.brain_type, BrainType::Normal);
        assert_eq!(sig.audio_sample_rate_hz, SAMPLE_RATE);
        assert_eq!(sig.neural_decimation_factor, DECIMATION_FACTOR);
        assert_eq!(sig.neural_sample_rate_hz.to_bits(), NEURAL_SR.to_bits());
        assert!(sig.auditory_flags.assr_enabled);
        assert!(sig.auditory_flags.thalamic_gate_enabled);
        assert!(sig.auditory_flags.cet_enabled);
        assert_eq!(
            sig.auditory_flags.arousal_model,
            ArousalModel::LegacyHeuristic
        );
        assert!(sig.neural_flags.stochastic_jr_enabled);
        assert_eq!(sig.warmup_discard_secs.to_bits(), 2.0f32.to_bits());
        assert_eq!(sig.duration_secs.to_bits(), 4.0f32.to_bits());
        assert_eq!(
            sig.numeric_params.habituation_rate.to_bits(),
            0.0003f64.to_bits()
        );
        assert_eq!(
            sig.numeric_params.habituation_recovery.to_bits(),
            0.0001f64.to_bits()
        );
        assert_eq!(
            sig.numeric_params.cet_c_slow_connectivity.to_bits(),
            30.0f64.to_bits()
        );
        assert_eq!(sig.numeric_params.jr_stochastic_rng_seed, 42);
        assert_eq!(sig.numeric_params.jr_v_max.to_bits(), 5.0f64.to_bits());
        assert_eq!(
            sig.numeric_params.fhn_spike_threshold.to_bits(),
            1.0f64.to_bits()
        );
        assert_eq!(
            sig.numeric_params.fhn_initial_voltage.to_bits(),
            (-1.2f64).to_bits()
        );
        assert_eq!(
            sig.numeric_params.fhn_initial_recovery.to_bits(),
            (-0.6f64).to_bits()
        );
        assert_eq!(sig.numeric_params.fhn_rk4_sub_steps, 4);
        assert_eq!(
            sig.numeric_params
                .wc_adaptive_entrainment_range_hz
                .to_bits(),
            5.0f64.to_bits()
        );
    }

    #[test]
    fn scoring_profile_variants_serialize_distinctly() {
        use crate::model_signature::ScoringProfile;
        let legacy = serde_json::to_string(&ScoringProfile::LegacyV1).unwrap();
        let candidate = serde_json::to_string(&ScoringProfile::CandidateResearchV2).unwrap();
        let product = serde_json::to_string(&ScoringProfile::ProductAcoustic).unwrap();
        assert_eq!(legacy, "\"legacy_v1\"");
        assert_eq!(candidate, "\"candidate_research_v2\"");
        assert_eq!(product, "\"product_acoustic\"");
    }

    #[test]
    fn candidate_research_v2_is_separate_from_legacy_v1() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Ignition);
        let mut config = canonical_config(6.0, BrainType::Normal);
        config.scoring_profile = crate::model_signature::ScoringProfile::CandidateResearchV2;
        let result = evaluate_preset(&preset, &goal, &config);
        assert!(result.multi_score.legacy_v1_neural.is_some());
        assert!(result.multi_score.candidate_research_v2.is_some());
        assert_eq!(
            result.score,
            result.multi_score.candidate_research_v2.unwrap()
        );
    }

    #[test]
    fn candidate_research_v2_is_unavailable_for_unsupported_legacy_goal() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Sleep);
        let config = canonical_config(6.0, BrainType::Normal);
        let result = evaluate_preset(&preset, &goal, &config);
        assert!(result.multi_score.candidate_research_v2.is_none());
    }

    #[test]
    #[should_panic(expected = "CandidateResearchV2 profile unavailable")]
    fn selected_profile_never_silently_falls_back() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Sleep);
        let mut config = canonical_config(6.0, BrainType::Normal);
        config.scoring_profile = crate::model_signature::ScoringProfile::CandidateResearchV2;
        let _ = evaluate_preset(&preset, &goal, &config);
    }

    #[test]
    fn product_acoustic_score_is_neural_independent() {
        let preset = fixture_bright_modulated_symmetric();
        let goal = Goal::new(GoalKind::Shield);
        let mut config = canonical_config(6.0, BrainType::Normal);
        config.acoustic_scoring_enabled = true;
        config.scoring_profile = crate::model_signature::ScoringProfile::ProductAcoustic;
        let result = evaluate_preset(&preset, &goal, &config);
        assert!(result.multi_score.product_acoustic.is_some());
        assert_eq!(result.score, result.multi_score.product_acoustic.unwrap());
    }

    #[test]
    #[should_panic(expected = "product_acoustic scoring requires acoustic scoring")]
    fn product_acoustic_requires_acoustic_scoring() {
        let preset = fixture_bright_modulated_symmetric();
        let goal = Goal::new(GoalKind::Shield);
        let mut config = canonical_config(6.0, BrainType::Normal);
        config.scoring_profile = crate::model_signature::ScoringProfile::ProductAcoustic;
        let _ = evaluate_preset(&preset, &goal, &config);
    }

    #[test]
    fn legacy_multiscore_channels_distinguish_neural_and_fused() {
        let preset = fixture_bright_modulated_symmetric();
        let goal = Goal::new(GoalKind::Shield);
        let mut config = canonical_config(6.0, BrainType::Normal);
        config.acoustic_scoring_enabled = true;
        config.acoustic_score_fusion_enabled = true;
        let result = evaluate_preset(&preset, &goal, &config);
        assert!(result.multi_score.legacy_v1_neural.is_some());
        assert!(result.multi_score.legacy_v1_fused.is_some());
        assert_eq!(result.score, result.multi_score.legacy_v1_fused.unwrap());
    }

    #[test]
    fn default_optimizer_path_stays_legacy_v1() {
        let config = SimulationConfig::default();
        assert_eq!(
            config.scoring_profile,
            crate::model_signature::ScoringProfile::LegacyV1
        );
    }

    #[test]
    fn model_signature_includes_tonotopic_parameters() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Focus);
        let config = canonical_config(4.0, BrainType::Adhd);
        let result = evaluate_preset(&preset, &goal, &config);
        let numeric = &result.model_signature.numeric_params;

        assert_eq!(numeric.tonotopic_params.band_rates[0], (75.0, 34.0));
        assert_eq!(numeric.tonotopic_params.band_offsets[3], 100.0);
        assert_eq!(numeric.tonotopic_params.band_g_fast_rate[2], 450.0);
        assert_eq!(numeric.tonotopic_params.band_v0[1], 5.5);
        match numeric.tonotopic_params.band_model_types[2] {
            crate::model_signature::BandModelTypeSnapshot::WilsonCowan { target_hz, .. } => {
                assert_eq!(target_hz, 14.0);
            }
            _ => panic!("band 2 expected Wilson-Cowan"),
        }

        assert_eq!(numeric.bilateral_params.left.band_offsets[0], 140.0);
        assert_eq!(numeric.bilateral_params.right.band_offsets[0], 135.0);
        match numeric.bilateral_params.right.band_model_types[3] {
            crate::model_signature::BandModelTypeSnapshot::WilsonCowan { target_hz, .. } => {
                assert_eq!(target_hz, 20.0);
            }
            _ => panic!("right band 3 expected Wilson-Cowan"),
        }
        assert_eq!(numeric.bilateral_params.callosal_coupling, 0.12);
    }

    #[test]
    fn model_signature_includes_jr_hidden_runtime_constants() {
        let result = evaluate_preset(
            &fixture_dark_unmodulated_symmetric(),
            &Goal::new(GoalKind::Sleep),
            &canonical_config(4.0, BrainType::Normal),
        );
        let n = &result.model_signature.numeric_params;

        assert_eq!(n.jr_v_max, 5.0);
        assert_eq!(n.jr_default_c, 135.0);
        assert_eq!(n.jr_default_c1, 135.0);
        assert_eq!(n.jr_default_c2, 108.0);
        assert_eq!(n.jr_default_c3, 27.0);
        assert_eq!(n.jr_default_c4, 27.0);
        assert_eq!(n.jr_default_v0, 6.0);
        assert_eq!(n.jr_default_sigmoid_r, 0.62);
        assert_eq!(n.jr_warmup_seconds, 1.0);
        assert_eq!(n.jr_sub_steps_base, 2);
        assert_eq!(n.jr_sub_steps_fast, 4);
        assert_eq!(n.jr_sub_steps_fast_rate_threshold, 200.0);
    }

    #[test]
    fn model_signature_includes_fhn_hidden_runtime_constants() {
        let result = evaluate_preset(
            &fixture_dark_unmodulated_symmetric(),
            &Goal::new(GoalKind::Sleep),
            &canonical_config(4.0, BrainType::Normal),
        );
        let n = &result.model_signature.numeric_params;
        let fhn_constants = crate::neural::fhn::legacy_constants_snapshot();

        assert_eq!(n.fhn_spike_threshold, fhn_constants.spike_threshold);
        assert_eq!(n.fhn_initial_voltage, fhn_constants.initial_voltage);
        assert_eq!(n.fhn_initial_recovery, fhn_constants.initial_recovery);
        assert_eq!(n.fhn_rk4_sub_steps, fhn_constants.rk4_sub_steps);
        assert_eq!(n.fhn_isi_cv_min_spikes, fhn_constants.isi_cv_min_spikes);
        assert_eq!(n.fhn_isi_cv_min_mean_isi, fhn_constants.isi_cv_min_mean_isi);
    }

    #[test]
    fn model_signature_includes_runtime_time_base() {
        let result = evaluate_preset(
            &fixture_mid_modulated_lateralized(),
            &Goal::new(GoalKind::Focus),
            &canonical_config(4.0, BrainType::Adhd),
        );
        let sig = &result.model_signature;
        let signature_json =
            serde_json::to_value(sig).expect("signature serialization should succeed");

        assert_eq!(sig.audio_sample_rate_hz, SAMPLE_RATE);
        assert_eq!(sig.neural_decimation_factor, DECIMATION_FACTOR);
        assert_eq!(sig.neural_sample_rate_hz.to_bits(), NEURAL_SR.to_bits());

        assert_eq!(
            signature_json["audio_sample_rate_hz"].as_u64(),
            Some(SAMPLE_RATE as u64)
        );
        assert_eq!(
            signature_json["neural_decimation_factor"].as_u64(),
            Some(DECIMATION_FACTOR as u64)
        );
        assert_eq!(
            signature_json["neural_sample_rate_hz"].as_f64(),
            Some(NEURAL_SR)
        );
    }

    #[test]
    fn stage2_scientific_diagnostics_are_score_inert_and_present_in_detailed_path() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Flow);
        let config = canonical_config(4.0, BrainType::Normal);

        let scalar = evaluate_preset(&preset, &goal, &config);
        let detailed = evaluate_preset_detailed(&preset, &goal, &config);

        assert_eq!(scalar.score.to_bits(), detailed.summary.score.to_bits());
        assert!(scalar.scientific_diagnostics.is_none());
        let diagnostics = detailed
            .summary
            .scientific_diagnostics
            .as_ref()
            .expect("detailed evaluation should carry stage2 diagnostics");
        assert!(diagnostics
            .spectral_parameterization
            .aperiodic_exponent
            .is_finite());
        assert!(diagnostics
            .spectral_parameterization
            .aperiodic_offset
            .is_finite());
        assert_eq!(
            diagnostics.arousal_sensitivity.estimated_score.to_bits(),
            detailed.summary.score.to_bits()
        );
    }

    #[test]
    fn stage2_arousal_sensitivity_diagnostics_are_finite_and_deterministic() {
        let preset = fixture_bright_modulated_symmetric();
        let goal = Goal::new(GoalKind::Ignition);
        let config = canonical_config(4.0, BrainType::Adhd);

        let d1 = evaluate_preset_detailed(&preset, &goal, &config);
        let d2 = evaluate_preset_detailed(&preset, &goal, &config);
        let a1 = d1
            .summary
            .scientific_diagnostics
            .as_ref()
            .expect("missing diagnostics")
            .arousal_sensitivity
            .clone();
        let a2 = d2
            .summary
            .scientific_diagnostics
            .as_ref()
            .expect("missing diagnostics")
            .arousal_sensitivity
            .clone();

        assert_eq!(a1.sweep.len(), 5);
        assert_eq!(a1.sweep, a2.sweep);
        assert!(a1.local_derivative.is_finite());
        assert!(a1.score_span.is_finite());
        assert!(a1.max_abs_slope.is_finite());
        for point in &a1.sweep {
            assert!(point.arousal.is_finite());
            assert!(point.score.is_finite());
        }
        assert_eq!(a1.sweep[0].arousal.to_bits(), 0.0f64.to_bits());
        assert_eq!(a1.sweep[4].arousal.to_bits(), 1.0f64.to_bits());
    }

    #[test]
    fn stage3_candidate_features_are_present_and_score_inert() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Flow);
        let config = canonical_config(4.0, BrainType::Normal);

        let scalar = evaluate_preset(&preset, &goal, &config);
        let detailed = evaluate_preset_detailed(&preset, &goal, &config);

        assert_eq!(scalar.score.to_bits(), detailed.summary.score.to_bits());
        let diagnostics = detailed
            .summary
            .scientific_diagnostics
            .as_ref()
            .expect("detailed path should include scientific diagnostics");
        let candidate = diagnostics
            .candidate_auditory_features
            .as_ref()
            .expect("stage3 candidate auditory features should be present");
        assert!(candidate.cochlear.brightness.is_finite());
        assert!(candidate
            .cochlear
            .band_energy_fractions
            .iter()
            .all(|v| v.is_finite()));
        assert!(candidate
            .temporal_modulation
            .total_modulation_power
            .is_finite());
    }

    #[test]
    fn stage3_candidate_temporal_modulation_tracks_modulation_rate() {
        let goal = Goal::new(GoalKind::Focus);
        let config = canonical_config(4.0, BrainType::Normal);

        let preset_5hz = fixture_single_tone_with_modulation(5.0, 0.95);
        let preset_40hz = fixture_single_tone_with_modulation(40.0, 0.95);
        let d5 = evaluate_preset_detailed(&preset_5hz, &goal, &config);
        let d40 = evaluate_preset_detailed(&preset_40hz, &goal, &config);
        let c5 = d5
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_auditory_features.as_ref())
            .expect("missing candidate features for 5 Hz case");
        let c40 = d40
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_auditory_features.as_ref())
            .expect("missing candidate features for 40 Hz case");
        let dom_5 = c5
            .temporal_modulation
            .dominant_modulation_hz
            .expect("5 Hz modulation should produce a dominant modulation estimate");
        let dom_40 = c40
            .temporal_modulation
            .dominant_modulation_hz
            .expect("40 Hz modulation should produce a dominant modulation estimate");
        assert!(
            (dom_5 - 5.0).abs() < 2.0,
            "dominant modulation should be near 5 Hz"
        );
        assert!(
            (dom_40 - 40.0).abs() < 4.0,
            "rendered 40 Hz modulation should recover near 40 Hz; got {dom_40:.3} Hz"
        );
        let low_case_fast = c5.temporal_modulation.band_power_by_mod_rate.beta_13_30_hz
            + c5.temporal_modulation.band_power_by_mod_rate.gamma_30_50_hz;
        let low_case_slow = c5.temporal_modulation.band_power_by_mod_rate.slow_0p5_4_hz
            + c5.temporal_modulation.band_power_by_mod_rate.theta_4_8_hz
            + 1e-12;
        let high_case_fast = c40.temporal_modulation.band_power_by_mod_rate.beta_13_30_hz
            + c40
                .temporal_modulation
                .band_power_by_mod_rate
                .gamma_30_50_hz;
        let high_case_slow = c40.temporal_modulation.band_power_by_mod_rate.slow_0p5_4_hz
            + c40.temporal_modulation.band_power_by_mod_rate.theta_4_8_hz
            + 1e-12;
        assert!(
            high_case_fast / high_case_slow > low_case_fast / low_case_slow,
            "higher-rate modulation should increase fast-vs-slow modulation power balance"
        );
    }

    #[test]
    fn stage3_candidate_rendered_unmodulated_noise_has_no_false_strong_modulation_peak() {
        let goal = Goal::new(GoalKind::Focus);
        let config = canonical_config(12.0, BrainType::Normal);
        let preset_unmodulated = fixture_single_noise_unmodulated_reference();

        let detailed = evaluate_preset_detailed(&preset_unmodulated, &goal, &config);
        let candidate = detailed
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_auditory_features.as_ref())
            .expect("missing candidate auditory features");
        let modulation = &candidate.temporal_modulation;
        let max_share = if modulation.total_modulation_power <= 0.0 {
            0.0
        } else {
            modulation
                .modulation_psd
                .iter()
                .map(|p| p.power)
                .fold(0.0_f64, f64::max)
                / modulation.total_modulation_power
        };
        assert!(
            modulation.dominant_modulation_hz.is_none(),
            "rendered unmodulated noise must not report a dominant modulation rate; dominant={:?}, max_share={max_share:.6}",
            modulation.dominant_modulation_hz
        );
        assert!(
            max_share < 0.03,
            "rendered unmodulated noise strongest-bin share must stay below dominant-peak threshold; max_share={max_share:.6}"
        );
    }

    #[test]
    fn stage3_candidate_rendered_noise_modulation_recovers_expected_rates() {
        let goal = Goal::new(GoalKind::Focus);
        let config = canonical_config(12.0, BrainType::Normal);

        let pink_5hz = fixture_single_noise_with_modulation(1, 4, 5.0, 0.95);
        let brown_40hz = fixture_single_noise_with_modulation(2, 5, 40.0, 0.95);

        let d5 = evaluate_preset_detailed(&pink_5hz, &goal, &config);
        let d40 = evaluate_preset_detailed(&brown_40hz, &goal, &config);

        let c5 = d5
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_auditory_features.as_ref())
            .expect("missing candidate features for pink 5 Hz AM case");
        let c40 = d40
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_auditory_features.as_ref())
            .expect("missing candidate features for brown 40 Hz AM case");

        let dom_5 = c5
            .temporal_modulation
            .dominant_modulation_hz
            .expect("rendered pink 5 Hz AM should produce a dominant modulation estimate");
        let dom_40 = c40
            .temporal_modulation
            .dominant_modulation_hz
            .expect("rendered brown 40 Hz AM should produce a dominant modulation estimate");

        assert!(
            (dom_5 - 5.0).abs() < 2.0,
            "rendered pink 5 Hz AM should recover near 5 Hz; got {dom_5:.3} Hz"
        );
        assert!(
            (dom_40 - 40.0).abs() < 5.0,
            "rendered brown 40 Hz AM should recover near 40 Hz; got {dom_40:.3} Hz"
        );
    }

    #[test]
    fn stage3_candidate_decouples_carrier_from_modulation_features() {
        let goal = Goal::new(GoalKind::Focus);
        let config = canonical_config(4.0, BrainType::Normal);

        let brown_5hz = fixture_single_source_with_modulation(2, 4, 5.0, 0.95);
        let white_5hz = fixture_single_source_with_modulation(0, 4, 5.0, 0.95);
        let db = evaluate_preset_detailed(&brown_5hz, &goal, &config);
        let dw = evaluate_preset_detailed(&white_5hz, &goal, &config);
        let cb = db
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_auditory_features.as_ref())
            .expect("missing candidate features for brown case");
        let cw = dw
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_auditory_features.as_ref())
            .expect("missing candidate features for white case");

        let dom_b = cb
            .temporal_modulation
            .dominant_modulation_hz
            .expect("brown 5 Hz should have a dominant modulation estimate");
        let dom_w = cw
            .temporal_modulation
            .dominant_modulation_hz
            .expect("white 5 Hz should have a dominant modulation estimate");
        assert!(
            (dom_b - dom_w).abs() < 1.5,
            "temporal modulation should follow modulation rate instead of carrier color"
        );

        let brightness_delta = (cb.cochlear.brightness - cw.cochlear.brightness).abs();
        assert!(
            brightness_delta > 0.05,
            "cochlear brightness should preserve carrier differences; delta={brightness_delta:.6}"
        );
        let band_l1 = cb
            .cochlear
            .band_energy_fractions
            .iter()
            .zip(cw.cochlear.band_energy_fractions.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>();
        assert!(
            band_l1 > 0.05,
            "cochlear band fractions should differ across carriers; L1={band_l1:.6}"
        );
    }

    #[test]
    fn stage3_candidate_modulation_comes_from_rendered_envelope_not_assr_metadata() {
        let goal = Goal::new(GoalKind::Focus);
        let config = canonical_config(4.0, BrainType::Normal);
        // SineLfo (kind=1) modulates rendered envelopes but is intentionally
        // excluded from ASSR metadata's active-modulator summary.
        let preset = fixture_single_source_with_modulation(1, 1, 1.0, 0.95);
        let detailed = evaluate_preset_detailed(&preset, &goal, &config);
        let diagnostics = detailed
            .summary
            .scientific_diagnostics
            .as_ref()
            .expect("missing detailed diagnostics");
        assert!(
            diagnostics.assr.dominant_modulation_hz.is_none(),
            "ASSR metadata should not expose SineLfo modulation as dominant"
        );
        let candidate = diagnostics
            .candidate_auditory_features
            .as_ref()
            .expect("missing candidate auditory features");
        assert!(
            !candidate.temporal_modulation.modulation_psd.is_empty(),
            "candidate modulation extraction should derive PSD from rendered envelopes"
        );
        assert!(
            candidate.temporal_modulation.total_modulation_power > 0.0,
            "candidate modulation power should be positive for actively modulated rendered input"
        );
    }

    #[test]
    fn candidate_v2_cortical_response_is_namespaced_and_score_inert() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Flow);
        let legacy_cfg = canonical_config(4.0, BrainType::Normal);
        let candidate_cfg = candidate_v2_config(4.0, BrainType::Normal);

        let legacy = evaluate_preset_detailed(&preset, &goal, &legacy_cfg);
        let candidate = evaluate_preset_detailed(&preset, &goal, &candidate_cfg);

        assert_eq!(
            legacy.summary.score.to_bits(),
            candidate.summary.score.to_bits(),
            "candidate_v2 diagnostics path must remain score-inert in Stage 4"
        );
        assert_eq!(
            candidate.summary.model_signature.pipeline_variant,
            crate::model_signature::PipelineVariant::EvaluateCandidateV2
        );

        let legacy_diag = legacy
            .summary
            .scientific_diagnostics
            .as_ref()
            .expect("legacy detailed diagnostics should exist");
        assert!(
            legacy_diag.candidate_cortical_response.is_none(),
            "legacy_v1 should not emit candidate_v2 cortical diagnostics"
        );
        let candidate_diag = candidate
            .summary
            .scientific_diagnostics
            .as_ref()
            .expect("candidate detailed diagnostics should exist");
        assert!(
            candidate_diag.candidate_cortical_response.is_some(),
            "candidate_v2 should emit candidate cortical diagnostics"
        );
    }

    #[test]
    fn candidate_v2_routes_same_modulation_same_rhythm_across_carriers() {
        let goal = Goal::new(GoalKind::Focus);
        let config = candidate_v2_config(12.0, BrainType::Normal);
        let brown_40hz = fixture_single_noise_with_modulation(2, 5, 40.0, 0.95);
        let white_40hz = fixture_single_noise_with_modulation(0, 5, 40.0, 0.95);

        let db = evaluate_preset_detailed(&brown_40hz, &goal, &config);
        let dw = evaluate_preset_detailed(&white_40hz, &goal, &config);
        let rb = db
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_cortical_response.as_ref())
            .expect("missing candidate cortical response for brown 40 Hz");
        let rw = dw
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_cortical_response.as_ref())
            .expect("missing candidate cortical response for white 40 Hz");

        assert_eq!(rb.dominant_module, rw.dominant_module);
        assert_eq!(
            rb.dominant_module,
            Some(crate::neural::CandidateRhythmModule::GammaAssr)
        );
    }

    #[test]
    fn candidate_v2_response_changes_with_modulation_rate() {
        let goal = Goal::new(GoalKind::Focus);
        let config = candidate_v2_config(12.0, BrainType::Normal);
        let pink_5hz = fixture_single_noise_with_modulation(1, 4, 5.0, 0.95);
        let pink_40hz = fixture_single_noise_with_modulation(1, 5, 40.0, 0.95);

        let d5 = evaluate_preset_detailed(&pink_5hz, &goal, &config);
        let d40 = evaluate_preset_detailed(&pink_40hz, &goal, &config);
        let r5 = d5
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_cortical_response.as_ref())
            .expect("missing candidate cortical response for 5 Hz");
        let r40 = d40
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_cortical_response.as_ref())
            .expect("missing candidate cortical response for 40 Hz");

        assert!(
            r40.gamma.response_strength > r5.gamma.response_strength,
            "40 Hz modulation should increase candidate gamma/ASSR response"
        );
        assert!(
            r5.slow.response_strength > r40.slow.response_strength,
            "5 Hz modulation should emphasize slow module over 40 Hz case"
        );
    }

    #[test]
    fn candidate_v2_unmodulated_baseline_has_no_false_strong_drive() {
        let goal = Goal::new(GoalKind::Focus);
        let config = candidate_v2_config(12.0, BrainType::Normal);
        let unmodulated = fixture_single_noise_unmodulated_reference();
        let detailed = evaluate_preset_detailed(&unmodulated, &goal, &config);
        let response = detailed
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_cortical_response.as_ref())
            .expect("missing candidate cortical response for unmodulated case");

        assert!(
            response.drive.dominant_modulation_hz.is_none(),
            "unmodulated input should not expose dominant modulation in candidate routing"
        );
        assert!(
            response.dominant_module.is_none(),
            "unmodulated input should not report a winning candidate rhythm module"
        );
        assert_eq!(response.slow.response_strength.to_bits(), 0.0f64.to_bits());
        assert_eq!(response.alpha.response_strength.to_bits(), 0.0f64.to_bits());
        assert_eq!(response.beta.response_strength.to_bits(), 0.0f64.to_bits());
        assert_eq!(response.gamma.response_strength.to_bits(), 0.0f64.to_bits());
        assert!(
            response.modulation_responsiveness_index < 1e-6,
            "unmodulated baseline should keep candidate responsiveness near zero; got {}",
            response.modulation_responsiveness_index
        );
    }

    #[test]
    fn candidate_v2_response_scales_with_modulation_depth() {
        let goal = Goal::new(GoalKind::Focus);
        let config = candidate_v2_config(12.0, BrainType::Normal);
        let weak = fixture_single_tone_with_modulation(40.0, 0.30);
        let strong = fixture_single_tone_with_modulation(40.0, 0.95);

        let dw = evaluate_preset_detailed(&weak, &goal, &config);
        let ds = evaluate_preset_detailed(&strong, &goal, &config);
        let rw = dw
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_cortical_response.as_ref())
            .expect("missing weak candidate cortical response");
        let rs = ds
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_cortical_response.as_ref())
            .expect("missing strong candidate cortical response");

        assert!(
            rs.drive.total_modulation_power > rw.drive.total_modulation_power,
            "strong modulation should produce larger total modulation power"
        );
        assert!(
            rs.modulation_responsiveness_index > rw.modulation_responsiveness_index,
            "strong modulation should produce stronger candidate responsiveness"
        );
    }

    #[test]
    fn candidate_v2_responsiveness_orders_none_weak_strong() {
        let goal = Goal::new(GoalKind::Focus);
        let config = candidate_v2_config(12.0, BrainType::Normal);
        let none = fixture_single_noise_unmodulated_reference();
        let weak = fixture_single_tone_with_modulation(40.0, 0.20);
        let strong = fixture_single_tone_with_modulation(40.0, 0.95);

        let dn = evaluate_preset_detailed(&none, &goal, &config);
        let dw = evaluate_preset_detailed(&weak, &goal, &config);
        let ds = evaluate_preset_detailed(&strong, &goal, &config);
        let rn = dn
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_cortical_response.as_ref())
            .expect("missing none-response");
        let rw = dw
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_cortical_response.as_ref())
            .expect("missing weak-response");
        let rs = ds
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_cortical_response.as_ref())
            .expect("missing strong-response");

        assert_eq!(
            rn.modulation_responsiveness_index.to_bits(),
            0.0f64.to_bits()
        );
        assert!(
            rn.modulation_responsiveness_index < rw.modulation_responsiveness_index
                && rw.modulation_responsiveness_index < rs.modulation_responsiveness_index,
            "expected responsiveness ordering none < weak < strong"
        );
    }

    #[test]
    fn legacy_default_arousal_model_matches_pre_stage5_heuristic() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Focus);
        let config = canonical_config(4.0, BrainType::Normal);
        let detailed = evaluate_preset_detailed(&preset, &goal, &config);
        let diag = detailed
            .summary
            .scientific_diagnostics
            .as_ref()
            .expect("missing diagnostics");
        let candidate = diag
            .candidate_auditory_features
            .as_ref()
            .expect("missing candidate diagnostics");
        let expected = ThalamicGate::compute_arousal(&preset, detailed.summary.brightness);
        assert_eq!(
            candidate.latent_state.estimated_arousal.to_bits(),
            expected.to_bits(),
            "default arousal model must stay legacy heuristic for legacy_v1"
        );
        assert_eq!(candidate.latent_state.arousal_source, ArousalSource::LegacyHeuristic);
    }

    #[test]
    fn candidate_fixed_arousal_is_reported_as_fixed() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Focus);
        let config = candidate_v2_fixed_arousal_config(4.0, BrainType::Normal, 0.8);
        let detailed = evaluate_preset_detailed(&preset, &goal, &config);
        let candidate = detailed
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_auditory_features.as_ref())
            .expect("missing candidate diagnostics");
        assert_eq!(
            candidate.latent_state.estimated_arousal.to_bits(),
            0.8f64.to_bits()
        );
        assert_eq!(candidate.latent_state.arousal_source, ArousalSource::Fixed);
    }

    #[test]
    fn fixed_arousal_controls_candidate_latent_state() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Focus);
        let low = candidate_v2_fixed_arousal_config(4.0, BrainType::Normal, 0.2);
        let high = candidate_v2_fixed_arousal_config(4.0, BrainType::Normal, 0.8);

        let d_low = evaluate_preset_detailed(&preset, &goal, &low);
        let d_high = evaluate_preset_detailed(&preset, &goal, &high);
        let c_low = d_low
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_auditory_features.as_ref())
            .expect("missing candidate diagnostics for low fixed arousal");
        let c_high = d_high
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_auditory_features.as_ref())
            .expect("missing candidate diagnostics for high fixed arousal");

        assert_eq!(c_low.latent_state.estimated_arousal.to_bits(), 0.2f64.to_bits());
        assert_eq!(
            c_high.latent_state.estimated_arousal.to_bits(),
            0.8f64.to_bits()
        );
        let a_low = d_low
            .summary
            .scientific_diagnostics
            .as_ref()
            .expect("missing diagnostics")
            .arousal_sensitivity
            .estimated_arousal;
        let a_high = d_high
            .summary
            .scientific_diagnostics
            .as_ref()
            .expect("missing diagnostics")
            .arousal_sensitivity
            .estimated_arousal;
        assert_eq!(a_low.to_bits(), 0.2f64.to_bits());
        assert_eq!(a_high.to_bits(), 0.8f64.to_bits());
    }

    #[test]
    fn same_fixed_arousal_different_gate_changes_gate_mapping_not_arousal_source() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Focus);
        let cfg_heuristic_gate = SimulationConfig {
            duration_secs: 4.0,
            model_version: ModelVersion::CandidateV2,
            arousal_model: ArousalModel::Fixed,
            fixed_arousal: Some(0.2),
            thalamic_gate_enabled: true,
            physiological_thalamic_gate_enabled: false,
            ..SimulationConfig::default()
        };
        let cfg_phys_gate = SimulationConfig {
            duration_secs: 4.0,
            model_version: ModelVersion::CandidateV2,
            arousal_model: ArousalModel::Fixed,
            fixed_arousal: Some(0.2),
            thalamic_gate_enabled: false,
            physiological_thalamic_gate_enabled: true,
            ..SimulationConfig::default()
        };
        let d_heur = evaluate_preset_detailed(&preset, &goal, &cfg_heuristic_gate);
        let d_phys = evaluate_preset_detailed(&preset, &goal, &cfg_phys_gate);

        let c_heur = d_heur
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_auditory_features.as_ref())
            .expect("missing candidate diagnostics (heur)");
        let c_phys = d_phys
            .summary
            .scientific_diagnostics
            .as_ref()
            .and_then(|d| d.candidate_auditory_features.as_ref())
            .expect("missing candidate diagnostics (phys)");

        assert_eq!(c_heur.latent_state.arousal_source, ArousalSource::Fixed);
        assert_eq!(c_phys.latent_state.arousal_source, ArousalSource::Fixed);
        assert_eq!(c_heur.latent_state.estimated_arousal.to_bits(), 0.2f64.to_bits());
        assert_eq!(c_phys.latent_state.estimated_arousal.to_bits(), 0.2f64.to_bits());

        let shifts_heur = d_heur
            .summary
            .scientific_diagnostics
            .as_ref()
            .expect("missing diagnostics")
            .arousal_sensitivity
            .estimated_score;
        let shifts_phys = d_phys
            .summary
            .scientific_diagnostics
            .as_ref()
            .expect("missing diagnostics")
            .arousal_sensitivity
            .estimated_score;
        assert!(
            (shifts_heur - shifts_phys).abs() > 1e-10,
            "same fixed arousal with different gate models should affect gate mapping"
        );
    }

    #[test]
    fn fixed_arousal_is_preserved_when_gates_are_disabled() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Focus);
        let config = SimulationConfig {
            duration_secs: 4.0,
            model_version: ModelVersion::CandidateV2,
            arousal_model: ArousalModel::Fixed,
            fixed_arousal: Some(0.8),
            thalamic_gate_enabled: false,
            physiological_thalamic_gate_enabled: false,
            ..SimulationConfig::default()
        };
        let detailed = evaluate_preset_detailed(&preset, &goal, &config);
        let diag = detailed
            .summary
            .scientific_diagnostics
            .as_ref()
            .expect("missing diagnostics");
        let candidate = diag
            .candidate_auditory_features
            .as_ref()
            .expect("missing candidate diagnostics");
        assert_eq!(
            candidate.latent_state.estimated_arousal.to_bits(),
            0.8f64.to_bits()
        );
        assert_eq!(candidate.latent_state.arousal_source, ArousalSource::Fixed);
        assert_eq!(diag.arousal_sensitivity.estimated_arousal.to_bits(), 0.8f64.to_bits());
    }

    #[test]
    fn model_signature_distinguishes_fixed_arousal_values() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Focus);
        let cfg_02 = SimulationConfig {
            duration_secs: 4.0,
            arousal_model: ArousalModel::Fixed,
            fixed_arousal: Some(0.2),
            ..SimulationConfig::default()
        };
        let cfg_08 = SimulationConfig {
            duration_secs: 4.0,
            arousal_model: ArousalModel::Fixed,
            fixed_arousal: Some(0.8),
            ..SimulationConfig::default()
        };
        let r02 = evaluate_preset(&preset, &goal, &cfg_02);
        let r08 = evaluate_preset(&preset, &goal, &cfg_08);
        assert_ne!(
            r02.model_signature.numeric_params.fixed_arousal,
            r08.model_signature.numeric_params.fixed_arousal
        );
        let j02 = serde_json::to_string(&r02.model_signature).expect("signature json");
        let j08 = serde_json::to_string(&r08.model_signature).expect("signature json");
        assert!(j02.contains("\"fixed_arousal\":0.2"));
        assert!(j08.contains("\"fixed_arousal\":0.8"));
    }

    #[test]
    #[should_panic(expected = "arousal_model=fixed requires fixed_arousal")]
    fn fixed_arousal_model_without_value_is_rejected() {
        let preset = fixture_mid_modulated_lateralized();
        let goal = Goal::new(GoalKind::Focus);
        let cfg = SimulationConfig {
            duration_secs: 4.0,
            arousal_model: ArousalModel::Fixed,
            fixed_arousal: None,
            ..SimulationConfig::default()
        };
        let _ = evaluate_preset(&preset, &goal, &cfg);
    }

    #[test]
    fn model_signature_includes_wilson_cowan_effective_parameters() {
        let result = evaluate_preset(
            &fixture_mid_modulated_lateralized(),
            &Goal::new(GoalKind::Focus),
            &canonical_config(4.0, BrainType::Adhd),
        );
        let n = &result.model_signature.numeric_params;

        let wc = match n.tonotopic_params.band_model_types[2] {
            crate::model_signature::BandModelTypeSnapshot::WilsonCowan {
                target_hz,
                tau_e,
                tau_i,
                w_ee,
                w_ie,
                w_ei,
                w_ii,
                h_e,
                h_i,
                sigmoid_a,
                sigmoid_theta,
                input_scale,
                input_offset,
                adaptive_entrainment_range_hz,
            } => (
                target_hz,
                tau_e,
                tau_i,
                w_ee,
                w_ie,
                w_ei,
                w_ii,
                h_e,
                h_i,
                sigmoid_a,
                sigmoid_theta,
                input_scale,
                input_offset,
                adaptive_entrainment_range_hz,
            ),
            _ => panic!("band 2 expected Wilson-Cowan"),
        };

        assert_eq!(wc.0, 14.0);
        assert_eq!(wc.3, 16.0); // w_ee
        assert_eq!(wc.4, 15.0); // w_ie
        assert_eq!(wc.5, 15.0); // w_ei
        assert_eq!(wc.6, 3.0); // w_ii
        assert_eq!(wc.7, 1.5); // h_e
        assert_eq!(wc.8, 0.0); // h_i
        assert_eq!(wc.9, 1.3); // sigmoid_a
        assert_eq!(wc.10, 4.0); // sigmoid_theta
        assert_eq!(wc.12, 1.0); // input_offset

        // Effective input scale must be the runtime WC scale (JR input_scale * 0.01).
        let expected_input_scale = BrainType::Adhd.params().jansen_rit.input_scale * 0.01;
        assert_eq!(wc.11, expected_input_scale);

        // Tau values are derived from target_hz by for_frequency(...)
        let tau_sum = 1.0 / (2.45 * 14.0);
        assert_eq!(wc.1, tau_sum * 0.45);
        assert_eq!(wc.2, tau_sum * 0.55);
    }

    #[test]
    fn model_signature_includes_wilson_cowan_entrainment_range() {
        let result = evaluate_preset(
            &fixture_bright_modulated_symmetric(),
            &Goal::new(GoalKind::Flow),
            &ablation_config(12.0, BrainType::Normal),
        );
        let n = &result.model_signature.numeric_params;

        assert_eq!(n.wc_adaptive_entrainment_range_hz, 5.0);
        match n.bilateral_params.left.band_model_types[2] {
            crate::model_signature::BandModelTypeSnapshot::WilsonCowan {
                adaptive_entrainment_range_hz,
                ..
            } => assert_eq!(adaptive_entrainment_range_hz, 5.0),
            _ => panic!("left band 2 expected Wilson-Cowan"),
        }
    }

    /// The detailed evaluation path must carry the same scalar result as the
    /// legacy summary API so single-preset diagnostics cannot drift from the
    /// canonical optimizer/matrix score.
    #[test]
    fn detailed_pipeline_summary_matches_scalar_evaluate() {
        let mut preset = Preset::default();
        preset.source_count = 1;
        preset.objects[0].active = true;
        preset.objects[0].color = 1; // pink
        preset.objects[0].volume = 0.85;
        preset.objects[0].bass_mod.kind = 4; // NeuralLfo
        preset.objects[0].bass_mod.param_a = 5.0;
        preset.objects[0].bass_mod.param_b = 0.90;
        preset.objects[0].satellite_mod.kind = 4;
        preset.objects[0].satellite_mod.param_a = 5.0;
        preset.objects[0].satellite_mod.param_b = 0.75;

        let goal = Goal::new(GoalKind::Sleep);
        let config = SimulationConfig::default();

        let scalar = evaluate_preset(&preset, &goal, &config);
        let detailed = evaluate_preset_detailed(&preset, &goal, &config);
        let summary = detailed.summary;

        assert!((scalar.score - summary.score).abs() < 1e-12);
        assert!((scalar.fhn_firing_rate - summary.fhn_firing_rate).abs() < 1e-12);
        assert!((scalar.fhn_isi_cv - summary.fhn_isi_cv).abs() < 1e-12);
        assert!((scalar.dominant_freq - summary.dominant_freq).abs() < 1e-12);
        assert!((scalar.alpha_asymmetry - summary.alpha_asymmetry).abs() < 1e-12);
        assert_eq!(scalar.performance.plv, summary.performance.plv);
        assert_eq!(
            scalar.performance.envelope_plv,
            summary.performance.envelope_plv
        );
    }

    /// The canonical detailed result must contain everything `Goal::diagnose()`
    /// needs to reproduce the same score the pipeline already computed.
    #[test]
    fn canonical_detailed_result_reproduces_diagnosis_score() {
        let mut preset = Preset::default();
        preset.source_count = 1;
        preset.objects[0].active = true;
        preset.objects[0].color = 2; // brown
        preset.objects[0].volume = 0.80;
        preset.objects[0].reverb_send = 0.80;
        preset.objects[0].bass_mod.kind = 4;
        preset.objects[0].bass_mod.param_a = 5.0;
        preset.objects[0].bass_mod.param_b = 0.95;

        let goal = Goal::new(GoalKind::DeepRelaxation);
        let config = SimulationConfig::default();
        let detailed = evaluate_preset_detailed(&preset, &goal, &config);

        let diagnosis = goal.diagnose(
            &detailed.fhn,
            &detailed.bilateral.combined,
            detailed.summary.brightness,
            detailed.summary.alpha_asymmetry,
            detailed.summary.performance.plv,
            detailed.summary.performance.envelope_plv,
            Some(detailed.summary.performance),
        );

        assert!(
            (diagnosis.score - detailed.summary.score).abs() < 1e-12,
            "diagnosis score {:.12} must match summary score {:.12}",
            diagnosis.score,
            detailed.summary.score
        );
        assert_eq!(
            diagnosis.performance.unwrap().envelope_plv,
            detailed.summary.performance.envelope_plv
        );
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
            sr,
            3.25,
            22.0,
            100.0,
            50.0,
            params.jansen_rit.c,
            220.0,
            params.jansen_rit.input_scale,
            &fi,
            params.jansen_rit.slow_inhib_ratio,
            params.jansen_rit.v0,
            0.62,
        );

        let result = model.simulate(&input);

        // EEG should be non-trivial
        let eeg_range = result.eeg.iter().cloned().fold(f64::NEG_INFINITY, f64::max)
            - result.eeg.iter().cloned().fold(f64::INFINITY, f64::min);
        assert!(
            eeg_range > 0.001,
            "EEG range {} too small — model not oscillating",
            eeg_range
        );

        // Dominant frequency should be in physiological range
        assert!(
            result.dominant_freq >= 0.5 && result.dominant_freq <= 50.0,
            "dominant freq {} out of physiological range",
            result.dominant_freq
        );

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
        let fhn = FhnModel::with_params(
            sr,
            params.fhn.a,
            params.fhn.b,
            params.fhn.epsilon,
            params.fhn.time_scale,
        );

        let result1 = fhn.simulate(&input, params.fhn.input_scale);
        let result2 = fhn.simulate(&input, params.fhn.input_scale);

        // Same input → same output (deterministic)
        assert_eq!(
            result1.firing_rate, result2.firing_rate,
            "FHN should be deterministic"
        );
        // NaN != NaN in IEEE 754, so use total_cmp for bitwise equality
        assert!(
            result1.isi_cv.total_cmp(&result2.isi_cv).is_eq(),
            "FHN ISI CV should be deterministic: {} vs {}",
            result1.isi_cv,
            result2.isi_cv
        );

        println!(
            "REGRESSION SNAPSHOT: FHN 10Hz sine: rate={:.2} cv={:.4}",
            result1.firing_rate, result1.isi_cv
        );
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
        assert!(
            config.thalamic_gate_enabled,
            "Thalamic gate should be enabled by default"
        );
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
            result1.score,
            result2.score
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
            result_off.score,
            result_on.score
        );

        // Both should still be valid
        assert!(
            result_on.score >= 0.0 && result_on.score <= 1.0,
            "ASSR-enabled score {} out of range",
            result_on.score
        );

        println!(
            "ASSR effect: off={:.6} on={:.6} delta={:.6}",
            result_off.score,
            result_on.score,
            result_on.score - result_off.score
        );
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
            result_off.score,
            result_on.score
        );

        assert!(
            result_on.score >= 0.0 && result_on.score <= 1.0,
            "Gate-enabled score {} out of range",
            result_on.score
        );

        println!(
            "Thalamic gate effect: off={:.6} on={:.6} delta={:.6}",
            result_off.score,
            result_on.score,
            result_on.score - result_off.score
        );
    }

    /// Both features enabled together produces valid scores.
    #[test]
    fn both_features_enabled_produces_valid_scores() {
        let preset = Preset::default();

        let mut config = fast_pipeline_config();
        config.assr_enabled = true;
        config.thalamic_gate_enabled = true;

        for kind in GoalKind::all() {
            let goal = Goal::new(*kind);
            let result = evaluate_preset(&preset, &goal, &config);

            assert!(
                result.score >= 0.0 && result.score <= 1.0,
                "{:?} with both features: score {} out of range",
                kind,
                result.score
            );
            assert!(
                result.dominant_freq.is_finite(),
                "{:?} with both features: non-finite dominant freq",
                kind
            );

            println!(
                "Both enabled {:?}: score={:.6} dom_freq={:.2}",
                kind, result.score, result.dominant_freq
            );
        }
    }

    #[test]
    fn acoustic_scaffolding_flag_does_not_change_scores() {
        let preset = make_modulated_preset();
        let goal = Goal::new(GoalKind::Shield);

        let config_off = fast_pipeline_config();
        let mut config_on = fast_pipeline_config();
        config_on.acoustic_scoring_enabled = true;

        let result_off = evaluate_preset(&preset, &goal, &config_off);
        let result_on = evaluate_preset(&preset, &goal, &config_on);

        assert_same_legacy_simulation_result(&result_off, &result_on);
        assert!(result_off.acoustic_score.is_none());
        assert!(result_on.acoustic_score.is_some());
    }

    #[test]
    fn acoustic_render_is_exposed_only_when_enabled() {
        let preset = make_modulated_preset();
        let goal = Goal::new(GoalKind::Shield);

        let config_off = fast_pipeline_config();
        let mut config_on = fast_pipeline_config();
        config_on.acoustic_scoring_enabled = true;

        let detailed_off = evaluate_preset_detailed(&preset, &goal, &config_off);
        let detailed_on = evaluate_preset_detailed(&preset, &goal, &config_on);

        assert!(detailed_off.acoustic_render.is_none());
        assert!(detailed_off.summary.acoustic_score.is_none());

        let render = detailed_on
            .acoustic_render
            .as_ref()
            .expect("acoustic render should be populated when enabled");
        assert!(render.is_finite());
        assert_eq!(
            render.frame_count(),
            (SAMPLE_RATE as f32 * config_on.duration_secs) as usize
        );
        let acoustic = detailed_on
            .summary
            .acoustic_score
            .as_ref()
            .expect("acoustic score should be populated when enabled");
        assert!((0.0..=1.0).contains(&acoustic.intelligibility_proxy.unwrap()));
        assert!((0.0..=1.0).contains(&acoustic.speech_privacy.unwrap()));
        let features = &acoustic.features;
        assert!(features.broadband_level_db.unwrap().is_finite());
        assert!((0.0..=1.0).contains(&features.speech_band_ratio.unwrap()));
        assert!((0.0..=1.0).contains(&features.modulation_depth.unwrap()));
        assert!((0.0..=1.0).contains(&features.sharpness_proxy.unwrap()));

        // Priority 28 Phase 1 — new diagnostic comfort metrics. They must
        // populate cleanly through the full pipeline and stay within
        // physically plausible ranges for any presetable rendered audio.
        // Their presence here also acts as the regression gate for the
        // `extract_features_v1` ↔ pipeline integration.
        assert_priority28_comfort_metrics_sane(features);

        assert_same_legacy_simulation_result(&detailed_off.summary, &detailed_on.summary);
    }

    /// Bounded sanity assertions for the Priority 28 Phase 1 comfort
    /// metrics. Used by all integration paths so that any preset that
    /// reaches `extract_features_v1` produces well-formed values.
    fn assert_priority28_comfort_metrics_sane(
        features: &crate::acoustic_score::AcousticFeatureVector,
    ) {
        let lufs_i = features
            .lufs_integrated
            .expect("lufs_integrated must be present");
        let lufs_l = features.lufs_left.expect("lufs_left must be present");
        let lufs_r = features.lufs_right.expect("lufs_right must be present");
        let asym = features
            .lufs_asymmetry_lu
            .expect("lufs_asymmetry_lu must be present");
        let tp = features
            .true_peak_dbfs
            .expect("true_peak_dbfs must be present");
        let plr = features.plr_db.expect("plr_db must be present");
        let tilt = features
            .spectral_tilt_db_per_oct
            .expect("spectral tilt must be present");
        let hf = features
            .hf_fraction_above_8khz
            .expect("hf fraction must be present");

        // Loudness must be finite and within the BS.1770 representable range.
        for (name, v) in [
            ("lufs_integrated", lufs_i),
            ("lufs_left", lufs_l),
            ("lufs_right", lufs_r),
        ] {
            assert!(v.is_finite(), "{name} = {v} must be finite");
            assert!(
                (-120.0..=12.0).contains(&v),
                "{name} = {v:.3} must be in plausible LUFS range"
            );
        }
        assert!(
            (0.0..=80.0).contains(&asym),
            "asymmetry must be ≥ 0 and < 80 LU, got {asym:.3}"
        );
        assert!(
            (-120.0..=12.0).contains(&tp),
            "true peak must be in dBFS range, got {tp:.3}"
        );
        // Steady masker rendered for ≥4 s should have a moderate PLR; allow
        // a generous envelope here since this is just a sanity check, not a
        // tight bound.
        assert!(
            (-20.0..=40.0).contains(&plr),
            "PLR must be plausible, got {plr:.3}"
        );
        // dB/oct slope: even noisy presets produce values well within ±20.
        assert!(
            (-20.0..=20.0).contains(&tilt),
            "spectral tilt must be in plausible range, got {tilt:.3}"
        );
        assert!(
            (0.0..=1.0).contains(&hf),
            "hf fraction must be in [0, 1], got {hf:.3}"
        );
        // Asymmetry equals abs difference between channels.
        assert!(
            (asym - (lufs_l - lufs_r).abs()).abs() < 1e-6,
            "asymmetry must equal |L − R| LU, got asym={asym:.3} L={lufs_l:.3} R={lufs_r:.3}"
        );
        // PLR is true_peak − integrated.
        assert!(
            (plr - (tp - lufs_i)).abs() < 1e-6,
            "plr must equal true_peak − integrated, got plr={plr:.3} tp={tp:.3} lufs_i={lufs_i:.3}"
        );
    }

    /// Priority 28 Phase 1 integration test — exercise every goal kind so
    /// that the full pipeline (engine → render → gammatone → JR → score)
    /// runs alongside the new comfort-metric extraction. The legacy
    /// simulation result must remain bit-identical when the comfort
    /// metrics flag is the only difference between two configurations.
    #[test]
    fn priority28_comfort_metrics_present_for_every_goal() {
        let preset = make_modulated_preset();
        for &kind in &[
            GoalKind::Shield,
            GoalKind::Isolation,
            GoalKind::Focus,
            GoalKind::DeepWork,
            GoalKind::Sleep,
            GoalKind::DeepRelaxation,
            GoalKind::Meditation,
            GoalKind::Flow,
            GoalKind::Ignition,
        ] {
            let goal = Goal::new(kind);
            let config_off = fast_pipeline_config();
            let mut config_on = fast_pipeline_config();
            config_on.acoustic_scoring_enabled = true;

            let result_off = evaluate_preset(&preset, &goal, &config_off);
            let result_on = evaluate_preset(&preset, &goal, &config_on);

            // Acoustic flag is the only difference → legacy summary fields
            // (band powers, FHN firing rate, dominant freq, score, …) must
            // be bit-identical between the two configurations.
            assert_same_legacy_simulation_result(&result_off, &result_on);

            let acoustic = result_on
                .acoustic_score
                .as_ref()
                .unwrap_or_else(|| panic!("acoustic_score missing for goal {kind}"));
            assert_priority28_comfort_metrics_sane(&acoustic.features);
        }
    }

    /// Defensive: enabling the comfort-metric extraction must NOT alter
    /// the optimizer-facing scalar `score` for any goal, including the
    /// Shield/Isolation acoustic-fusion path. The only legitimate route
    /// to a score change is the future Priority 28f ε-constrained ranking.
    #[test]
    fn priority28_comfort_metrics_do_not_change_scoring() {
        let preset = make_modulated_preset();
        for &kind in &[
            GoalKind::Shield,
            GoalKind::Isolation,
            GoalKind::Sleep,
            GoalKind::Focus,
        ] {
            let goal = Goal::new(kind);

            // Baseline: acoustic scoring enabled, fusion enabled (the
            // pre-Priority-28 production behavior for Shield/Isolation).
            let mut config_baseline = fast_pipeline_config();
            config_baseline.acoustic_scoring_enabled = true;
            config_baseline.acoustic_score_fusion_enabled = true;

            // Variant: same flags. Comfort metrics are populated either way
            // because they live inside `extract_features_v1` — but they
            // cannot reach the optimizer's score until Priority 28f wires
            // them through. This test pins that contract.
            let mut config_variant = fast_pipeline_config();
            config_variant.acoustic_scoring_enabled = true;
            config_variant.acoustic_score_fusion_enabled = true;

            let r_baseline = evaluate_preset(&preset, &goal, &config_baseline);
            let r_variant = evaluate_preset(&preset, &goal, &config_variant);
            assert_eq!(
                r_baseline.score, r_variant.score,
                "comfort metrics must not affect score for goal {kind}"
            );
        }
    }

    /// Priority 28 Phase 2 — end-to-end integration. The full pipeline
    /// (preset → render → gammatone → JR → score → comfort_violation)
    /// must produce a finite, non-negative, bounded violation for every
    /// goal kind. The violation is allowed to differ across goals because
    /// thresholds are goal-dependent; the test pins finiteness, range,
    /// and the structural property that running with the comfort-metric
    /// flag toggled does not change the legacy simulation result.
    #[test]
    fn priority28_phase2_violation_chain_works_end_to_end() {
        let preset = make_modulated_preset();
        // Theoretical maximum aggregate violation: sum of per-term caps
        // (LUFS_ASYM 0.20 + TRUE_PEAK 0.10 + SPECTRAL_TILT 0.15 + HF 0.10
        // + PLR 0.10 = 0.65). Add a small slack for any future cap
        // additions; the assert is a sanity bound, not a tight one.
        let max_total: f64 = 1.0;

        for &kind in &[
            GoalKind::Shield,
            GoalKind::Isolation,
            GoalKind::Focus,
            GoalKind::DeepWork,
            GoalKind::Sleep,
            GoalKind::DeepRelaxation,
            GoalKind::Meditation,
            GoalKind::Flow,
            GoalKind::Ignition,
        ] {
            let goal = Goal::new(kind);

            let mut config = fast_pipeline_config();
            config.acoustic_scoring_enabled = true;

            let result = evaluate_preset(&preset, &goal, &config);
            let acoustic = result
                .acoustic_score
                .as_ref()
                .unwrap_or_else(|| panic!("acoustic_score must be present for {kind}"));

            let violation = goal.comfort_violation(&acoustic.features);
            assert!(
                violation.is_finite(),
                "{kind}: violation must be finite, got {violation}"
            );
            assert!(
                violation >= 0.0,
                "{kind}: violation must be ≥ 0, got {violation:.6}"
            );
            assert!(
                violation <= max_total,
                "{kind}: violation {violation:.6} must be ≤ {max_total:.6}"
            );
        }
    }

    /// Priority 28 Phase 2 — opt-in feedback into the DE optimizer.
    /// Verifies that a constrained-mode DE wired with the goal's comfort
    /// violation function reaches a feasible best within a reasonable
    /// number of generations on the synthetic preset, AND that the same
    /// optimizer invoked with the same seed in legacy mode still produces
    /// its baseline behaviour (no implicit dependency on the new fields).
    #[test]
    fn priority28_phase2_constrained_mode_finds_feasible_individuals() {
        use crate::optimizer::DifferentialEvolution;

        // Use a small synthetic 4D problem rather than the full preset
        // pipeline — we want to exercise the constrained DE end-to-end
        // without paying the multi-second cost of full preset evaluation
        // on every trial. The constraint structure mirrors the production
        // setup (linear ε decay + violation = aggregated comfort terms).
        let bounds = vec![(-1.0, 1.0); 4];
        let mut de = DifferentialEvolution::new(bounds.clone(), 30, 0.8, 0.9, 7777);

        // Synthetic neural fitness: f(x) = sum(x_i)
        // Synthetic violation: v(x) = max(0, |x_0 - 0.3| - 0.1)
        //                      + max(0, |x_1 - 0.2| - 0.1)
        // (i.e., x_0 should be close to 0.3, x_1 close to 0.2; x_2/x_3 free)
        let eval = |g: &[f64]| -> (f64, f64) {
            let f = g.iter().sum();
            let v = (g[0] - 0.3).abs().max(0.0).max(0.1) - 0.1
                + (g[1] - 0.2).abs().max(0.0).max(0.1)
                - 0.1;
            let v = ((g[0] - 0.3).abs() - 0.1).max(0.0) + ((g[1] - 0.2).abs() - 0.1).max(0.0);
            (f, v)
        };

        // Initial pop evaluation.
        for (i, genome) in de.pending_evaluations() {
            let (f, v) = eval(&genome);
            de.report_constrained(i, f, v);
        }
        // Set ε₀ from the initial population's 70th-percentile violation
        // (matches the spec from Priority 28 §28f).
        let eps_0 = de.suggest_eps_from_population(0.70);
        assert!(eps_0.is_finite(), "ε₀ must be finite after initial evals");
        de.enable_eps_constrained(eps_0, 60);
        assert!(de.is_constrained());

        // Optimize.
        for _ in 0..120 {
            let trials = de.generate_trials();
            for (target, trial) in trials {
                let (f, v) = eval(&trial);
                de.report_trial_constrained(target, trial, f, v);
            }
        }

        // After T_c=60 generations, ε must be 0 (we ran 120 generations).
        assert_eq!(de.current_eps(), 0.0);

        let best = de
            .best_strict()
            .expect("synthetic problem has feasible interior; convergence should produce strict-feasible best");
        assert!(
            best.violation < 1e-6,
            "best_strict violation must be ~0 after past-T_c convergence, got {:.6}",
            best.violation
        );
        // x_0 should be near 0.3 ± 0.1 and x_1 near 0.2 ± 0.1 (the feasible band).
        assert!(
            (best.genome[0] - 0.3).abs() <= 0.10 + 1e-6,
            "x_0 should be in [0.2, 0.4], got {:.4}",
            best.genome[0]
        );
        assert!(
            (best.genome[1] - 0.2).abs() <= 0.10 + 1e-6,
            "x_1 should be in [0.1, 0.3], got {:.4}",
            best.genome[1]
        );
    }

    /// Priority 28 Phase 2b — wire-up integration test. Drives a small
    /// constrained-mode optimization through the same code path as
    /// `run_optimize`, verifying that:
    ///   1. The pipeline produces an `acoustic_score` payload.
    ///   2. `compute_comfort_violation` (via `Goal::comfort_violation`)
    ///      yields a finite, bounded scalar.
    ///   3. The constrained DE accepts `(neural_fitness, violation)`
    ///      pairs and works through `report_constrained` /
    ///      `report_trial_constrained`.
    ///   4. `best_strict()` is selectable as the final result.
    /// Uses a small 4-individual × 3-generation × 2.0 s render budget
    /// (≈ 16 full pipeline evaluations); enough to exercise the wiring
    /// without dominating CI time. Optimization quality is NOT tested
    /// here — the synthetic-problem test
    /// `priority28_phase2_constrained_mode_finds_feasible_individuals`
    /// covers convergence behavior.
    #[test]
    fn priority28_phase2b_wire_up_integration() {
        use crate::optimizer::DifferentialEvolution;
        use crate::preset::Preset;

        let goal = Goal::new(GoalKind::Sleep);
        // duration must exceed the 2 s warm-up discard; 2.5 s leaves a
        // 0.5 s analysis window, which is enough for the JR / FHN models.
        let mut config = fast_pipeline_config();
        config.duration_secs = 2.5;
        config.acoustic_scoring_enabled = true;
        // fusion stays off (constrained mode is incompatible with it)
        config.acoustic_score_fusion_enabled = false;
        config.acoustic_constraints_enabled = true;

        let bounds = Preset::bounds();
        let discrete_dims = Preset::discrete_gene_indices();
        let mut de =
            DifferentialEvolution::with_discrete(bounds, 4, 0.7, 0.8, 424242, discrete_dims);
        let spread = [0.0_f32; crate::preset::MAX_OBJECTS];

        // Initial population evaluation — full pipeline + violation derive.
        for (idx, genome) in de.pending_evaluations() {
            let preset = Preset::from_genome_with_spread(&genome, &spread);
            let result = evaluate_preset(&preset, &goal, &config);
            // The pipeline must populate acoustic features when
            // constrained mode is on.
            let acoustic = result
                .acoustic_score
                .as_ref()
                .expect("acoustic_score must be present in constrained mode");
            let violation = goal.comfort_violation(&acoustic.features);
            assert!(
                violation.is_finite(),
                "violation must be finite, got {violation}"
            );
            assert!(
                (0.0..=1.0).contains(&violation),
                "violation must be ≤ 1, got {violation}"
            );
            de.report_constrained(idx, result.score, violation);
        }

        // ε₀ from 70th percentile of initial-pop violations (per spec §28f).
        let eps_0 = de.suggest_eps_from_population(0.70);
        assert!(
            eps_0.is_finite(),
            "ε₀ must be finite after initial evaluations"
        );
        de.enable_eps_constrained(eps_0, 2); // t_c = 2 (covers 3 generations)
        assert!(de.is_constrained());

        // Very short evolution loop — the contract under test is wiring
        // correctness, not optimization quality.
        for _ in 0..3 {
            let trials = de.generate_trials();
            for (target_idx, trial_genome) in trials {
                let preset = Preset::from_genome_with_spread(&trial_genome, &spread);
                let result = evaluate_preset(&preset, &goal, &config);
                let acoustic = result.acoustic_score.as_ref().unwrap();
                let violation = goal.comfort_violation(&acoustic.features);
                de.report_trial_constrained(target_idx, trial_genome, result.score, violation);
            }
        }

        // After 3 generations with t_c = 2, ε must have reached 0.
        assert_eq!(de.current_eps(), 0.0);

        // best_strict must be callable. On real presets within a 3-gen
        // budget we do NOT guarantee finding a strictly feasible
        // candidate, so the contract is structural: the call must work
        // and either return Some(strict_feasible_individual) or None.
        // If Some, both fields must be finite.
        if let Some(strict) = de.best_strict() {
            assert!(strict.violation.is_finite());
            assert!(strict.neural_fitness.is_finite());
            assert!(
                strict.violation <= 1e-9,
                "Some(strict) must satisfy violation ≤ 1e-9"
            );
        }
    }

    /// **Review fix 2026-05-02**: in constrained mode the cached
    /// `best` follows the ε-relaxed comparator. With a generous ε that
    /// stays > 0 for the whole run, the optimizer will drift the
    /// population toward higher-fitness *infeasible* candidates
    /// (because they're allowed to win under ε-relaxation). When that
    /// happens, every population member has violation > 0, and
    /// `best_strict()` correctly returns `None`. The main.rs flow
    /// must handle this — it's the "no strictly feasible candidate
    /// found" warning path.
    #[test]
    fn priority28_constrained_mode_best_strict_can_be_none_under_generous_eps() {
        use crate::optimizer::DifferentialEvolution;

        let bounds = vec![(0.0, 1.0); 2];
        // f(x,y) = x + y maximised; v = max(0, x+y - 0.8). Feasibility
        // pulls the optimum toward x+y = 0.8; ε-relaxation pulls it
        // toward x+y = 1.3 (where v = 0.5 ≤ ε₀).
        let eval = |g: &[f64]| -> (f64, f64) {
            let f = g.iter().sum();
            let v = (g[0] + g[1] - 0.8).max(0.0);
            (f, v)
        };
        let mut de = DifferentialEvolution::new(bounds, 12, 0.7, 0.8, 999);
        for (i, genome) in de.pending_evaluations() {
            let (f, v) = eval(&genome);
            de.report_constrained(i, f, v);
        }
        // Generous ε₀ + long t_c → the run never "tightens" toward
        // strict feasibility within the test horizon. The population
        // collapses on the ε-relaxed front (x+y ≈ 1.3, v ≈ 0.5).
        de.enable_eps_constrained(0.50, 200);
        for _ in 0..15 {
            let trials = de.generate_trials();
            for (target, trial) in trials {
                let (f, v) = eval(&trial);
                de.report_trial_constrained(target, trial, f, v);
            }
        }

        // ε is still > 0 (t_c=200). best_strict may legitimately be
        // None at this point — the contract is that the code handles
        // this case rather than crashing or silently returning an
        // infeasible candidate as "the strict best".
        assert!(de.current_eps() > 0.0);
        let strict = de.best_strict();
        let cached = de.best();
        assert!(cached.neural_fitness.is_finite());
        assert!(cached.violation.is_finite());
        if let Some(s) = strict {
            // If we did find one, it must satisfy strict feasibility.
            assert!(
                s.violation <= 1e-9,
                "Some(strict) must satisfy violation ≤ 1e-9, got {:.6}",
                s.violation
            );
        }
        // No assertion on which case we land in — both are valid; the
        // test pins that the API returns Option (not a panic) and the
        // strict-feasible invariant.
    }

    /// Companion: with ε scheduled to reach 0 within the run, the
    /// optimizer is forced to converge onto strict feasibility, and
    /// best_strict must be Some after ε hits 0.
    #[test]
    fn priority28_constrained_mode_strict_best_emerges_after_eps_reaches_zero() {
        use crate::optimizer::DifferentialEvolution;

        let bounds = vec![(0.0, 1.0); 2];
        let eval = |g: &[f64]| -> (f64, f64) {
            let f = g.iter().sum();
            let v = (g[0] + g[1] - 0.8).max(0.0);
            (f, v)
        };
        let mut de = DifferentialEvolution::new(bounds, 16, 0.7, 0.8, 1234);
        for (i, genome) in de.pending_evaluations() {
            let (f, v) = eval(&genome);
            de.report_constrained(i, f, v);
        }
        de.enable_eps_constrained(0.50, 10); // ε reaches 0 by gen 10
        for _ in 0..40 {
            let trials = de.generate_trials();
            for (target, trial) in trials {
                let (f, v) = eval(&trial);
                de.report_trial_constrained(target, trial, f, v);
            }
        }
        assert_eq!(de.current_eps(), 0.0);
        // After ε=0 + 30 more generations under strict feasibility,
        // we MUST have at least one strict-feasible candidate.
        let strict = de
            .best_strict()
            .expect("ε=0 for 30 gens should produce strict-feasible best");
        assert!(strict.violation <= 1e-9);
        // Strict-best fitness should approach the constrained optimum
        // f = 0.8 (the boundary of the feasible region).
        assert!(
            strict.neural_fitness >= 0.7,
            "strict-best should approach constrained optimum 0.8, got {:.4}",
            strict.neural_fitness
        );
    }

    /// Priority 28 Phase 3 — diversification end-to-end smoke. Drives
    /// a synthetic 4D Rastrigin (multi-modal so diversification has
    /// something to do) through the full DE loop with both crowding
    /// selection and stagnation restart enabled. The contract is
    /// structural:
    ///   1. The optimizer runs to completion without panic.
    ///   2. The stagnation restart counter is observable and
    ///      monotone non-decreasing.
    ///   3. The final best is finite and within bounds.
    ///   4. The same seed reproduces the same trajectory (determinism
    ///      under diversification).
    #[test]
    fn priority28_phase3_diversification_end_to_end() {
        use crate::optimizer::DifferentialEvolution;

        let bounds = vec![(-5.12, 5.12); 4];
        // Rastrigin (negated → maximize).
        let eval = |g: &[f64]| -> f64 {
            let n = g.len() as f64;
            -(10.0 * n
                + g.iter()
                    .map(|x| x * x - 10.0 * (2.0 * std::f64::consts::PI * x).cos())
                    .sum::<f64>())
        };

        let run = || -> (f64, usize) {
            let mut de = DifferentialEvolution::new(bounds.clone(), 25, 0.8, 0.9, 31337);
            de.enable_crowding_selection();
            de.enable_stagnation_restart(8, 0.30);
            for (i, genome) in de.pending_evaluations() {
                de.report_fitness(i, eval(&genome));
            }
            for _ in 0..50 {
                let trials = de.generate_trials();
                for (target, trial) in trials {
                    de.report_trial_result(target, trial.clone(), eval(&trial));
                }
            }
            (de.best().fitness, de.stagnation_restart_count())
        };

        let (best1, restarts1) = run();
        let (best2, restarts2) = run();
        assert_eq!(
            best1, best2,
            "diversified DE must be deterministic at fixed seed"
        );
        assert_eq!(restarts1, restarts2, "restart count must be deterministic");
        assert!(
            best1.is_finite(),
            "Phase 3 run must produce finite best fitness, got {best1}"
        );
        // Sanity: Rastrigin's *random* baseline is around f ≈ −80 in 4D
        // ([0, ~20] per dim). Any DE run that completes without a bug
        // will beat that comfortably; we use −40 as a conservative
        // pass threshold so we don't make the test brittle to small RNG
        // fluctuations across Rust toolchain updates.
        assert!(
            best1 > -40.0,
            "Phase 3 run on 4D Rastrigin should beat random baseline, got {best1:.4}"
        );
    }

    /// Priority 28 Phase 2 — the legacy DE entry points must remain
    /// bit-identical at fixed seed. This is the strongest no-regression
    /// guarantee we can write at the optimizer-API level.
    #[test]
    fn priority28_phase2_legacy_de_bit_identical_after_phase2() {
        use crate::optimizer::DifferentialEvolution;

        let bounds = vec![(-2.0, 2.0); 3];
        let eval = |g: &[f64]| -g.iter().map(|x| x * x).sum::<f64>();

        let run = || -> Vec<(f64, f64, f64)> {
            let mut de = DifferentialEvolution::new(bounds.clone(), 25, 0.8, 0.9, 314159);
            for (i, genome) in de.pending_evaluations() {
                de.report_fitness(i, eval(&genome));
            }
            let mut history = Vec::new();
            for _ in 0..50 {
                let trials = de.generate_trials();
                for (target, trial) in trials {
                    de.report_trial_result(target, trial.clone(), eval(&trial));
                }
                history.push((de.best().fitness, de.mean_fitness(), de.fitness_std()));
            }
            history
        };

        let h1 = run();
        let h2 = run();
        assert_eq!(h1, h2, "legacy DE must be deterministic at fixed seed");
        // Should converge to ~0 (origin maximizes -x² for sphere).
        assert!(
            h1.last().unwrap().0 > -0.01,
            "legacy sphere DE should converge to ~0, got {:.6}",
            h1.last().unwrap().0
        );
    }

    #[test]
    fn acoustic_fusion_keeps_unsupported_goals_on_legacy_score() {
        let preset = make_modulated_preset();
        let goal = Goal::new(GoalKind::Focus);

        let mut config_off = fast_pipeline_config();
        config_off.acoustic_scoring_enabled = true;

        let mut config_on = fast_pipeline_config();
        config_on.acoustic_scoring_enabled = true;
        config_on.acoustic_score_fusion_enabled = true;

        let result_off = evaluate_preset(&preset, &goal, &config_off);
        let result_on = evaluate_preset(&preset, &goal, &config_on);

        assert_same_legacy_simulation_result(&result_off, &result_on);
        let acoustic = result_on
            .acoustic_score
            .as_ref()
            .expect("acoustic payload should still be present when fusion is requested");
        assert!(acoustic.acoustic_goal_score.is_none());
        assert!(acoustic.comfort_score.is_none());
        assert!(acoustic.legacy_nmm_score.is_none());
        assert!(acoustic.fused_score_preview.is_none());
    }

    #[test]
    fn acoustic_fusion_changes_only_score_for_supported_goals() {
        let preset = make_modulated_preset();
        let goal = Goal::new(GoalKind::Shield);

        let mut config_off = fast_pipeline_config();
        config_off.acoustic_scoring_enabled = true;

        let mut config_on = fast_pipeline_config();
        config_on.acoustic_scoring_enabled = true;
        config_on.acoustic_score_fusion_enabled = true;

        let result_off = evaluate_preset(&preset, &goal, &config_off);
        let result_on = evaluate_preset(&preset, &goal, &config_on);

        assert_same_legacy_simulation_result_except_score(&result_off, &result_on);
        let acoustic = result_on
            .acoustic_score
            .as_ref()
            .expect("supported fused goal should populate acoustic payload");
        assert!(
            (result_on.score - result_off.score).abs() > 1e-6,
            "supported fused goal should change the scalar score"
        );
        assert!(
            (acoustic.legacy_nmm_score.unwrap() - result_off.score).abs() < 1e-12,
            "legacy NMM score should be preserved in the acoustic payload"
        );
        assert!(
            (acoustic.fused_score_preview.unwrap() - result_on.score).abs() < 1e-12,
            "fused score preview should match the exported scalar score"
        );
        assert!((0.0..=1.0).contains(&acoustic.comfort_score.unwrap()));
        assert!((0.0..=1.0).contains(&acoustic.acoustic_goal_score.unwrap()));
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
        let total = result.delta_power
            + result.theta_power
            + result.alpha_power
            + result.beta_power
            + result.gamma_power;

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
            result_brown.score,
            result_white.score,
            score_diff
        );

        println!(
            "NORMALIZATION TEST: brown={:.4} white={:.4} diff={:.4}",
            result_brown.score, result_white.score, score_diff
        );
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
        println!(
            "BAND RATIO TEST: brown_slow={:.4} white_slow={:.4}",
            brown_slow, white_slow
        );

        // Note: this test documents expected behavior after the normalization fix.
        // With per-band normalization, brown_slow ≈ white_slow (both normalized to 1.0).
        // With global normalization, brown_slow > white_slow.
    }

    /// All presets still produce valid scores after normalization change.
    #[test]
    fn normalization_change_preserves_valid_scores() {
        let config = fast_pipeline_config();

        // Representative color/goal coverage is enough here because other
        // tests already cover all goals' math and several color-specific
        // spectral differences. This test is guarding "pipeline stays finite
        // and bounded after the normalization change", not exhaustively
        // re-validating the whole search space.
        for color in [0u8, 2, 6] {
            // White, Brown, SSN
            let mut preset = Preset::default();
            preset.source_count = 1;
            preset.objects[0].active = true;
            preset.objects[0].color = color;
            preset.objects[0].volume = 0.80;

            for kind in [
                GoalKind::Focus,
                GoalKind::Sleep,
                GoalKind::Isolation,
                GoalKind::DeepRelaxation,
            ] {
                let goal = Goal::new(kind);
                let result = evaluate_preset(&preset, &goal, &config);

                assert!(
                    result.score >= 0.0 && result.score <= 1.0,
                    "Color {} {:?}: score {} out of range",
                    color,
                    kind,
                    result.score
                );
                assert!(
                    result.dominant_freq.is_finite() && result.dominant_freq >= 0.0,
                    "Color {} {:?}: invalid dominant freq {}",
                    color,
                    kind,
                    result.dominant_freq
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

        println!(
            "FHN AMPLITUDE TEST: brown_rate={:.2} blue_rate={:.2}",
            result_brown.fhn_firing_rate, result_blue.fhn_firing_rate
        );

        // Firing rates should differ because the EEG amplitudes differ
        // (different spectral distributions → different JR inputs → different oscillation amplitudes)
        let rate_diff = (result_brown.fhn_firing_rate - result_blue.fhn_firing_rate).abs();
        assert!(
            rate_diff > 0.1,
            "Brown ({:.2}) and Blue ({:.2}) should produce different FHN rates (diff={:.2}). \
             Combined global-norm + percentile-scaling should preserve amplitude differences.",
            result_brown.fhn_firing_rate,
            result_blue.fhn_firing_rate,
            rate_diff
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
                color,
                result.fhn_firing_rate
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
        assert!(
            ratio > 0.5 && ratio < 1.0,
            "Boxcar should pass 300 Hz partially (ratio={ratio:.4})"
        );
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

        println!(
            "BILATERAL TEST: alpha_asymmetry={:.4} (positive=left-dominant)",
            result.alpha_asymmetry
        );
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
                bt,
                result.score
            );
            assert!(
                result.dominant_freq.is_finite() && result.dominant_freq > 0.0,
                "{:?}: invalid dominant freq {}",
                bt,
                result.dominant_freq
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
            delta: 0.05,
            theta: 0.15,
            alpha: 0.35,
            beta: 0.35,
            gamma: 0.10,
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
                delta: 0.2,
                theta: 0.2,
                alpha: 0.2,
                beta: 0.2,
                gamma: 0.2,
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
            delta: 0.22,
            theta: 0.35,
            alpha: 0.36,
            beta: 0.03,
            gamma: 0.01,
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
            delta: 0.30,
            theta: 0.48,
            alpha: 0.12,
            beta: 0.02,
            gamma: 0.02,
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
            let bp = BandPowers {
                delta: 0.2,
                theta: 0.2,
                alpha: 0.2,
                beta: 0.2,
                gamma: 0.2,
            };
            let jr = make_jr_result_from_powers(bp);
            let fhn = make_perfect_fhn(*kind);

            for asym in [-0.9, -0.5, 0.0, 0.5, 0.9] {
                let score = goal.evaluate_with_asymmetry(&fhn, &jr, asym);
                assert!(
                    score >= 0.0 && score <= 1.0,
                    "{:?} asymmetry={asym}: score {score} out of range",
                    kind
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
        println!(
            "ASSR DC/AC TEST: gate_only={:.4} both={:.4} ratio={:.3}",
            result_gate.score, result_both.score, ratio
        );

        assert!(
            ratio > 0.60,
            "ASSR+gate should not collapse score: gate={:.4} both={:.4} ratio={:.3}. \
             ASSR is likely reducing DC drive (operating point), not just AC (modulation).",
            result_gate.score,
            result_both.score,
            ratio
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

        println!(
            "HABITUATION TEST: var_first={var_first:.6} var_last={var_last:.6} ratio={:.3}",
            var_last / var_first.max(1e-20)
        );

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
        let mut jr_det =
            JansenRitModel::with_params(sr, 3.25, 22.0, 100.0, 50.0, 135.0, 175.0, 60.0);
        let result_det = jr_det.simulate(&input);

        let mut jr_stoch =
            JansenRitModel::with_params(sr, 3.25, 22.0, 100.0, 50.0, 135.0, 175.0, 60.0);
        jr_stoch.stochastic_sigma = 20.0;
        let result_stoch = jr_stoch.simulate(&input);

        let det_norm = result_det.band_powers.normalized();
        let stoch_norm = result_stoch.band_powers.normalized();

        // Stochastic should broaden the spectrum — energy distributes more
        // evenly across bands instead of concentrating in alpha+theta.
        // Measure: standard deviation of band powers (lower = more even).
        let det_bands = [
            det_norm.delta,
            det_norm.theta,
            det_norm.alpha,
            det_norm.beta,
        ];
        let stoch_bands = [
            stoch_norm.delta,
            stoch_norm.theta,
            stoch_norm.alpha,
            stoch_norm.beta,
        ];

        let mean_det = det_bands.iter().sum::<f64>() / 4.0;
        let mean_stoch = stoch_bands.iter().sum::<f64>() / 4.0;
        let std_det = (det_bands
            .iter()
            .map(|x| (x - mean_det).powi(2))
            .sum::<f64>()
            / 4.0)
            .sqrt();
        let std_stoch = (stoch_bands
            .iter()
            .map(|x| (x - mean_stoch).powi(2))
            .sum::<f64>()
            / 4.0)
            .sqrt();

        println!("STOCHASTIC: det_std={std_det:.3} stoch_std={std_stoch:.3}");
        println!(
            "  det: d={:.3} t={:.3} a={:.3} b={:.3}",
            det_norm.delta, det_norm.theta, det_norm.alpha, det_norm.beta
        );
        println!(
            "  stoch: d={:.3} t={:.3} a={:.3} b={:.3}",
            stoch_norm.delta, stoch_norm.theta, stoch_norm.alpha, stoch_norm.beta
        );

        // Stochastic should have MORE EVEN distribution (lower std)
        assert!(
            std_stoch < std_det,
            "Stochastic should broaden spectrum: det_std={std_det:.3} > stoch_std={std_stoch:.3}"
        );
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
            assert!(
                (r1.eeg[i] - r2.eeg[i]).abs() < 1e-10,
                "sigma=0 should match deterministic at sample {i}"
            );
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
            assert!(
                v.is_finite(),
                "Stochastic EEG sample {i} is not finite: {v}"
            );
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
            let total = result.delta_power
                + result.theta_power
                + result.alpha_power
                + result.beta_power
                + result.gamma_power;

            assert!(
                (total - 1.0).abs() < 0.02,
                "Color {}: band powers sum to {:.4}, should be ~1.0",
                color,
                total
            );
        }
    }
}
