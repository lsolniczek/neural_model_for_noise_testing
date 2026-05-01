/// One-off analysis of a specific preset against all goals.
#[cfg(test)]
mod tests {
    use crate::brain_type::BrainType;
    use crate::movement::MovementConfig;
    use crate::pipeline::{evaluate_preset, SimulationConfig};
    use crate::preset::{ModConfig, ObjectConfig, Preset, MAX_OBJECTS};
    use crate::scoring::{Goal, GoalKind};

    fn build_preset(ssn_lfo_freq: f32, ssn_lfo_depth: f32) -> Preset {
        let mut objects: Vec<ObjectConfig> =
            (0..MAX_OBJECTS).map(|_| ObjectConfig::default()).collect();

        objects[0] = ObjectConfig {
            active: true,
            color: 2, // brown vol 0.5
            x: 0.0,
            y: 0.0,
            z: 0.0,
            volume: 0.5,
            reverb_send: 0.0,
            spread: 0.0,
            bass_mod: ModConfig {
                kind: 0,
                param_a: 0.0,
                param_b: 0.0,
                param_c: 0.0,
            },
            satellite_mod: ModConfig {
                kind: 0,
                param_a: 0.0,
                param_b: 0.0,
                param_c: 0.0,
            },
            movement: MovementConfig::default(),
            tint_freq: 0.0,
            tint_db: 0.0,
            source_kind: 0,
            tone_freq: 200.0,
            tone_amplitude: 0.0,
        };

        objects[1] = ObjectConfig {
            active: true,
            color: 4, // grey vol 0.7, static at x:1, depth 0.8
            x: 1.0,
            y: 0.0,
            z: 1.0,
            volume: 0.7,
            reverb_send: 0.0,
            spread: 0.0,
            bass_mod: ModConfig {
                kind: 4,
                param_a: 30.0,
                param_b: 0.8,
                param_c: 0.0,
            },
            satellite_mod: ModConfig {
                kind: 4,
                param_a: 30.0,
                param_b: 0.8,
                param_c: 0.0,
            },
            movement: MovementConfig::default(),
            tint_freq: 0.0,
            tint_db: 0.0,
            source_kind: 0,
            tone_freq: 200.0,
            tone_amplitude: 0.0,
        };

        // Object 4: second grey, mirrored at x:-1, depth 0.8
        objects[4] = ObjectConfig {
            active: true,
            color: 4, // grey vol 0.7, static at x:-1
            x: -1.0,
            y: 0.0,
            z: 1.0,
            volume: 0.7,
            reverb_send: 0.0,
            spread: 0.0,
            bass_mod: ModConfig {
                kind: 4,
                param_a: 30.0,
                param_b: 0.8,
                param_c: 0.0,
            },
            satellite_mod: ModConfig {
                kind: 4,
                param_a: 30.0,
                param_b: 0.8,
                param_c: 0.0,
            },
            movement: MovementConfig::default(),
            tint_freq: 0.0,
            tint_db: 0.0,
            source_kind: 0,
            tone_freq: 200.0,
            tone_amplitude: 0.0,
        };

        objects[2] = ObjectConfig {
            active: true,
            color: 6, // SSN vol 1.0, neuralLfo bass + stochastic sat
            x: 0.0,
            y: 0.0,
            z: 1.0,
            volume: 1.0,
            reverb_send: 0.0,
            spread: 0.0,
            bass_mod: ModConfig {
                kind: 4,
                param_a: ssn_lfo_freq,
                param_b: ssn_lfo_depth,
                param_c: 0.0,
            },
            satellite_mod: ModConfig {
                kind: 3,
                param_a: 4.0,
                param_b: 150.0,
                param_c: 0.85,
            },
            movement: MovementConfig {
                kind: 5,
                radius: 2.0,
                speed: 0.4,
                phase: 0.0,
                depth_min: 0.5,
                depth_max: 5.0,
                reverb_min: 0.0,
                reverb_max: 0.0,
            },
            tint_freq: 0.0,
            tint_db: 0.0,
            source_kind: 0,
            tone_freq: 200.0,
            tone_amplitude: 0.0,
        };

        objects[3] = ObjectConfig {
            active: true,
            color: 1, // pink vol 0.5, flat mods
            x: 0.0,
            y: 0.0,
            z: 2.0,
            volume: 0.5,
            reverb_send: 0.0,
            spread: 0.0,
            bass_mod: ModConfig {
                kind: 0,
                param_a: 0.0,
                param_b: 0.0,
                param_c: 0.0,
            },
            satellite_mod: ModConfig {
                kind: 0,
                param_a: 0.0,
                param_b: 0.0,
                param_c: 0.0,
            },
            movement: MovementConfig::default(),
            tint_freq: 0.0,
            tint_db: 0.0,
            source_kind: 0,
            tone_freq: 200.0,
            tone_amplitude: 0.0,
        };

        let mut preset = Preset {
            master_gain: 0.8,
            spatial_mode: 1,
            source_count: 4,
            anchor_color: 0,
            anchor_volume: 0.0,
            environment: 1, // FocusRoom
            objects,
        };
        preset.clamp();
        preset
    }

    #[test]
    #[ignore = "exploratory preset sweep; run manually with cargo test analyze_preset::tests -- --ignored --nocapture"]
    fn sweep_ssn_lfo_freq_and_depth() {
        let config = SimulationConfig::default();
        let goal = Goal::new(GoalKind::Isolation);

        println!("\n{}", "=".repeat(72));
        println!("  SSN neuralLFO sweep — Isolation score");
        println!("  freq\\depth   0.2     0.4     0.6     0.8");
        println!("{}", "=".repeat(72));

        for &freq in &[14.0_f32, 20.0, 28.0, 36.0] {
            print!("  {:5.0} Hz   ", freq);
            for &depth in &[0.2_f32, 0.4, 0.6, 0.8] {
                let preset = build_preset(freq, depth);
                let result = evaluate_preset(&preset, &goal, &config);
                print!("{:.4}  ", result.score);
            }
            println!();
        }
        println!();
    }

    #[test]
    #[ignore = "exploratory preset sweep; run manually with cargo test analyze_preset::tests -- --ignored --nocapture"]
    fn sweep_grey_lfo_depth() {
        let config = SimulationConfig::default();
        let goal = Goal::new(GoalKind::Isolation);

        println!("\n{}", "=".repeat(72));
        println!("  Grey neuralLFO depth sweep @ 30Hz — Isolation");
        println!("  depth   score    δ       θ       α       β       γ");
        println!("{}", "=".repeat(72));

        for &depth in &[0.2_f32, 0.4, 0.6, 0.8, 1.0] {
            // Temporarily build preset with variable grey depth
            let mut objects: Vec<ObjectConfig> =
                (0..MAX_OBJECTS).map(|_| ObjectConfig::default()).collect();
            objects[0] = ObjectConfig {
                active: true,
                color: 2,
                x: 0.0,
                y: 0.0,
                z: 0.0,
                volume: 0.5,
                reverb_send: 0.0,
                spread: 0.0,
                bass_mod: ModConfig {
                    kind: 0,
                    param_a: 0.0,
                    param_b: 0.0,
                    param_c: 0.0,
                },
                satellite_mod: ModConfig {
                    kind: 0,
                    param_a: 0.0,
                    param_b: 0.0,
                    param_c: 0.0,
                },
                movement: MovementConfig::default(),
                tint_freq: 0.0,
                tint_db: 0.0,
                source_kind: 0,
                tone_freq: 200.0,
                tone_amplitude: 0.0,
            };
            let grey_mod = ModConfig {
                kind: 4,
                param_a: 30.0,
                param_b: depth,
                param_c: 0.0,
            };
            objects[1] = ObjectConfig {
                active: true,
                color: 4,
                x: 1.0,
                y: 0.0,
                z: 1.0,
                volume: 0.7,
                reverb_send: 0.0,
                spread: 0.0,
                bass_mod: grey_mod.clone(),
                satellite_mod: grey_mod.clone(),
                movement: MovementConfig::default(),
                tint_freq: 0.0,
                tint_db: 0.0,
                source_kind: 0,
                tone_freq: 200.0,
                tone_amplitude: 0.0,
            };
            objects[4] = ObjectConfig {
                active: true,
                color: 4,
                x: -1.0,
                y: 0.0,
                z: 1.0,
                volume: 0.7,
                reverb_send: 0.0,
                spread: 0.0,
                bass_mod: grey_mod.clone(),
                satellite_mod: grey_mod.clone(),
                movement: MovementConfig::default(),
                tint_freq: 0.0,
                tint_db: 0.0,
                source_kind: 0,
                tone_freq: 200.0,
                tone_amplitude: 0.0,
            };
            objects[2] = ObjectConfig {
                active: true,
                color: 6,
                x: 0.0,
                y: 0.0,
                z: 1.0,
                volume: 1.0,
                reverb_send: 0.0,
                spread: 0.0,
                bass_mod: ModConfig {
                    kind: 4,
                    param_a: 14.0,
                    param_b: 0.2,
                    param_c: 0.0,
                },
                satellite_mod: ModConfig {
                    kind: 3,
                    param_a: 4.0,
                    param_b: 150.0,
                    param_c: 0.85,
                },
                movement: MovementConfig {
                    kind: 5,
                    radius: 2.0,
                    speed: 0.4,
                    phase: 0.0,
                    depth_min: 0.5,
                    depth_max: 5.0,
                    reverb_min: 0.0,
                    reverb_max: 0.0,
                },
                tint_freq: 0.0,
                tint_db: 0.0,
                source_kind: 0,
                tone_freq: 200.0,
                tone_amplitude: 0.0,
            };
            objects[3] = ObjectConfig {
                active: true,
                color: 1,
                x: 0.0,
                y: 0.0,
                z: 2.0,
                volume: 0.5,
                reverb_send: 0.0,
                spread: 0.0,
                bass_mod: ModConfig {
                    kind: 0,
                    param_a: 0.0,
                    param_b: 0.0,
                    param_c: 0.0,
                },
                satellite_mod: ModConfig {
                    kind: 0,
                    param_a: 0.0,
                    param_b: 0.0,
                    param_c: 0.0,
                },
                movement: MovementConfig::default(),
                tint_freq: 0.0,
                tint_db: 0.0,
                source_kind: 0,
                tone_freq: 200.0,
                tone_amplitude: 0.0,
            };
            let mut preset = Preset {
                master_gain: 0.8,
                spatial_mode: 1,
                source_count: 4,
                anchor_color: 0,
                anchor_volume: 0.0,
                environment: 1,
                objects,
            };
            preset.clamp();

            let r = evaluate_preset(&preset, &goal, &config);
            println!(
                "  {:.1}     {:.4}   {:.3}   {:.3}   {:.3}   {:.3}   {:.4}",
                depth,
                r.score,
                r.delta_power,
                r.theta_power,
                r.alpha_power,
                r.beta_power,
                r.gamma_power
            );
        }
    }

    #[test]
    #[ignore = "exploratory preset sweep; run manually with cargo test analyze_preset::tests -- --ignored --nocapture"]
    fn analyze_sleep_sanctuary_all_goals() {
        let preset = build_preset(14.0, 0.2); // current best
        let config = SimulationConfig::default();

        println!("\n{}", "=".repeat(70));
        println!("  PRESET ANALYSIS — current best state");
        println!("{}\n", "=".repeat(70));

        for kind in GoalKind::all() {
            let goal = Goal::new(*kind);
            let result = evaluate_preset(&preset, &goal, &config);
            println!("── {:?}  score={:.4}  dom={:.2}Hz  δ={:.3} θ={:.3} α={:.3} β={:.3} γ={:.4}  bright={:.3}",
                kind, result.score, result.dominant_freq,
                result.delta_power, result.theta_power, result.alpha_power,
                result.beta_power, result.gamma_power, result.brightness);
        }
    }

    #[test]
    #[ignore = "exploratory preset sweep; run manually with cargo test analyze_preset::tests -- --ignored --nocapture"]
    fn analyze_sleep_sanctuary_sleep_detail() {
        let preset = build_preset(14.0, 0.2);
        for bt in BrainType::all() {
            let config = SimulationConfig {
                brain_type: *bt,
                ..SimulationConfig::default()
            };
            let goal = Goal::new(GoalKind::Sleep);
            let result = evaluate_preset(&preset, &goal, &config);
            println!(
                "Sleep + {:?}: score={:.4}  dom={:.2}Hz  firing={:.2}Hz  ISI_CV={:.4}",
                bt, result.score, result.dominant_freq, result.fhn_firing_rate, result.fhn_isi_cv
            );
        }
    }
}
