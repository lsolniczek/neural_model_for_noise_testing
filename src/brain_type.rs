/// Brain type profiles for neural model parameterisation.
///
/// Different brain types adjust FHN and Jansen-Rit parameters to simulate
/// individual neurological variation (e.g. ADHD, aging, anxiety).

use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BrainType {
    /// Healthy adult — default model parameters.
    Normal,
    /// Meditation practitioner — stronger alpha, higher inhibition.
    HighAlpha,
    /// ADHD-like — weaker inhibition, faster/less stable dynamics.
    Adhd,
    /// Aging brain — slower time constants, stronger low-frequency tendency.
    Aging,
    /// Anxious profile — heightened excitability, elevated beta.
    Anxious,
}

impl fmt::Display for BrainType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BrainType::Normal => write!(f, "Normal"),
            BrainType::HighAlpha => write!(f, "HighAlpha"),
            BrainType::Adhd => write!(f, "ADHD"),
            BrainType::Aging => write!(f, "Aging"),
            BrainType::Anxious => write!(f, "Anxious"),
        }
    }
}

impl BrainType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "normal" | "default" | "healthy" => Some(BrainType::Normal),
            "high_alpha" | "highalpha" | "alpha" | "meditation" => Some(BrainType::HighAlpha),
            "adhd" => Some(BrainType::Adhd),
            "aging" | "aged" | "elderly" => Some(BrainType::Aging),
            "anxious" | "anxiety" => Some(BrainType::Anxious),
            _ => None,
        }
    }

    /// All brain types for iteration.
    pub fn all() -> &'static [BrainType] {
        &[
            BrainType::Normal,
            BrainType::HighAlpha,
            BrainType::Adhd,
            BrainType::Aging,
            BrainType::Anxious,
        ]
    }

    /// Get the neural parameter profile for this brain type.
    pub fn params(&self) -> NeuralParams {
        match self {
            // JR model: input_offset + nerve * input_scale determines the drive.
            // The standard JR model oscillates (alpha, ~10 Hz) for p ∈ [120, 320].
            // We keep the model in this oscillatory regime and use spectral brightness
            // (from the audio FFT) as a separate score modifier in the scoring stage.

            BrainType::Normal => NeuralParams {
                fhn: FhnParams {
                    a: 0.7,
                    b: 0.8,
                    epsilon: 0.08,
                    input_scale: 1.5,
                    time_scale: 300.0,
                },
                jansen_rit: JansenRitParams {
                    a_gain: 3.25,
                    b_gain: 22.0,
                    a_rate: 100.0,
                    b_rate: 50.0,
                    c: 135.0,
                    input_offset: 175.0,
                    input_scale: 60.0,
                    // Wendling 2002: balanced fast inhibitory loop
                    g_fast_gain: 10.0,
                    g_fast_rate: 500.0,
                    c5: 0.3 * 135.0,  // 40.5 — Pyr → FSI
                    c6: 0.1 * 135.0,  // 13.5 — Slow Inhib → FSI (disinhibition)
                    c7: 115.0,        // Universal: strengthened FSI→Pyr (was 108)
                    slow_inhib_ratio: 0.20,  // Universal: loosen GABA-B for beta access
                    v0: 6.0,
                },
            },

            BrainType::HighAlpha => NeuralParams {
                fhn: FhnParams {
                    a: 0.7,
                    b: 0.8,
                    epsilon: 0.06,
                    input_scale: 1.3,
                    time_scale: 300.0,
                },
                jansen_rit: JansenRitParams {
                    a_gain: 3.25,
                    b_gain: 25.0,
                    a_rate: 100.0,
                    b_rate: 50.0,
                    c: 135.0,
                    input_offset: 195.0,
                    input_scale: 40.0,
                    // Moderate fast inhibition — alpha dominance preserved
                    g_fast_gain: 8.0,
                    g_fast_rate: 500.0,
                    c5: 0.3 * 135.0,
                    c6: 0.1 * 135.0,
                    c7: 0.8 * 135.0,  // 108.0
                    slow_inhib_ratio: 0.20,
                    v0: 6.0,
                },
            },

            // ADHD: hypoaroused cortex — just above bifurcation boundary.
            // Spontaneous theta present, beta deficit without external drive.
            // With noise: higher bands pushed across threshold → stochastic resonance.
            // Wendling: weaker fast inhibition → reduced gamma capacity (Edden et al. 2012)
            BrainType::Adhd => NeuralParams {
                fhn: FhnParams {
                    a: 0.7,
                    b: 0.8,
                    epsilon: 0.10,
                    input_scale: 1.8,
                    time_scale: 300.0,
                },
                jansen_rit: JansenRitParams {
                    a_gain: 3.5,
                    b_gain: 18.0,
                    a_rate: 100.0,
                    b_rate: 50.0,
                    c: 135.0,
                    input_offset: 135.0,
                    input_scale: 80.0,
                    // Neuro-Plastic Shift: raise arousal floor so presets can reach the brain
                    g_fast_gain: 10.0,
                    g_fast_rate: 450.0,   // Slower GABA-A kinetics
                    c5: 0.3 * 135.0,
                    c6: 0.1 * 135.0,
                    c7: 112.0,            // Strengthened fast brakes (was 102)
                    slow_inhib_ratio: 0.15,
                    v0: 5.5,
                },
            },

            BrainType::Aging => NeuralParams {
                fhn: FhnParams {
                    a: 0.75,
                    b: 0.8,
                    epsilon: 0.06,
                    input_scale: 1.2,
                    time_scale: 300.0,
                },
                jansen_rit: JansenRitParams {
                    a_gain: 3.25,
                    b_gain: 22.0,
                    a_rate: 80.0,
                    b_rate: 40.0,
                    c: 120.0,
                    input_offset: 165.0,
                    input_scale: 50.0,
                    // Aging: slower GABA-A, reduced connectivity
                    g_fast_gain: 10.0,
                    g_fast_rate: 350.0,   // Slowed GABA-A kinetics
                    c5: 0.3 * 120.0,     // 36.0
                    c6: 0.1 * 120.0,     // 12.0
                    c7: 0.8 * 120.0,     // 96.0
                    slow_inhib_ratio: 0.20,
                    v0: 6.0,
                },
            },

            // Anxious: overdriven cortex, elevated beta, always oscillating
            // Wendling: hyperactive fast inhibitory loop → excessive beta
            BrainType::Anxious => NeuralParams {
                fhn: FhnParams {
                    a: 0.7,
                    b: 0.8,
                    epsilon: 0.10,
                    input_scale: 1.8,
                    time_scale: 300.0,
                },
                jansen_rit: JansenRitParams {
                    a_gain: 3.5,
                    b_gain: 19.0,
                    a_rate: 100.0,
                    b_rate: 50.0,
                    c: 145.0,
                    input_offset: 220.0,
                    input_scale: 70.0,
                    // Hyperactive fast inhibition → excessive beta/gamma
                    g_fast_gain: 15.0,    // Higher gain
                    g_fast_rate: 500.0,
                    c5: 0.3 * 145.0,     // 43.5
                    c6: 0.1 * 145.0,     // 14.5
                    c7: 0.8 * 145.0,     // 116.0
                    slow_inhib_ratio: 0.20,
                    v0: 6.0,
                },
            },
        }
    }

    /// Get tonotopic cortical parameters for this brain type.
    ///
    /// The primary differentiation mechanism is `band_offsets`: each band
    /// operates at a different point in the JR bifurcation diagram.
    ///   - Near lower Hopf (~120-150): delta/theta oscillation
    ///   - Center (~200-220): alpha oscillation
    ///   - Upper region (~260-310): faster/beta-like dynamics
    ///
    /// The rates are kept close to standard (100/50) with mild variations,
    /// since large rate changes can push the model out of oscillatory regime.
    pub fn tonotopic_params(&self) -> TonotopicParams {
        match self {
            // The JR oscillation frequency scales with sqrt(a_rate * b_rate)/(2π):
            //   (70, 35)  → ~7.9 Hz (theta)
            //   (85, 42)  → ~9.5 Hz (alpha-low)
            //   (100, 50) → ~11.3 Hz (alpha)
            //   (120, 60) → ~13.5 Hz (beta-low)
            //
            // Band offsets are graduated: lower bands sit closer to the Hopf
            // bifurcation (~120) so they only oscillate when driven by input.
            // Higher bands sit further inside the oscillatory regime.
            // This creates input-dependent band recruitment — weak input → theta;
            // strong input → theta + alpha + beta.
            BrainType::Normal => TonotopicParams {
                band_rates: [
                    (70.0, 35.0),   // Low → theta (ratio 2.0)
                    (85.0, 42.0),   // Low-mid → alpha-low (ratio 2.02)
                    (100.0, 50.0),  // Band 2: unused (WilsonCowan)
                    (100.0, 50.0),  // Band 3: unused (WilsonCowan)
                ],
                band_gains: [
                    (3.25, 22.0),
                    (3.25, 22.0),
                    (3.25, 22.0),
                    (3.25, 22.0),
                ],
                band_offsets: [150.0, 170.0, 150.0, 150.0],
                band_input_gains: [1.0; 4],
                band_output_weights: [1.0; 4],
                band_slow_inhib_ratios: [0.20; 4],
                band_c7: [115.0; 4],
                band_sigmoid_r: [0.62; 4],
                band_c1c2_scale: [1.0; 4],
                band_g_fast_rate: [500.0; 4],
                band_v0: [6.0; 4],
                band_model_types: [BandModelType::JansenRit, BandModelType::JansenRit, BandModelType::WilsonCowan(14.0), BandModelType::WilsonCowan(25.0)],
            },

            BrainType::HighAlpha => TonotopicParams {
                band_rates: [
                    (75.0, 38.0),
                    (90.0, 45.0),   // Closer to alpha
                    (100.0, 50.0),
                    (110.0, 55.0),  // Less beta push
                ],
                band_gains: [
                    (3.25, 24.0),
                    (3.25, 25.0),   // Strong inhibition → alpha lock
                    (3.25, 25.0),
                    (3.25, 23.0),
                ],
                // Mid bands deepest in oscillatory regime → strong alpha
                band_offsets: [160.0, 190.0, 210.0, 180.0],
                band_input_gains: [1.0; 4],
                band_output_weights: [1.0; 4],
                band_slow_inhib_ratios: [0.20; 4],
                band_c7: [108.0; 4],
                band_sigmoid_r: [0.62; 4],
                band_c1c2_scale: [1.0; 4],
                band_g_fast_rate: [500.0; 4],
                band_v0: [6.0; 4],
                band_model_types: [BandModelType::JansenRit; 4],
            },

            // ADHD: hypoaroused cortex — low band above bifurcation for
            // spontaneous theta, higher bands near/below threshold (beta deficit).
            // With noise: higher bands pushed across threshold → stochastic resonance.
            // (Barry et al. 2003: elevated theta/beta ratio)
            BrainType::Adhd => TonotopicParams {
                band_rates: [
                    (75.0, 37.0),   // Slightly faster low band
                    (90.0, 45.0),
                    (110.0, 55.0),  // Faster mid → more beta when activated
                    (120.0, 60.0),
                ],
                band_gains: [
                    (3.5, 18.0),    // Weaker inhibition
                    (3.5, 18.0),
                    (3.5, 19.0),
                    (3.5, 18.0),
                ],
                // Neuro-Plastic Shift: raised offsets so presets can activate fast bands
                // Still below Normal (150) — ADHD deficit preserved, but responsive
                band_offsets: [140.0, 125.0, 110.0, 100.0],
                band_input_gains: [1.0; 4],
                band_output_weights: [1.0; 4],
                band_slow_inhib_ratios: [0.18, 0.18, 0.12, 0.12],
                band_c7: [112.0, 112.0, 115.0, 115.0],
                band_sigmoid_r: [0.62; 4],
                band_c1c2_scale: [1.0; 4],
                band_g_fast_rate: [450.0; 4],
                band_v0: [5.5; 4],
                band_model_types: [BandModelType::JansenRit, BandModelType::JansenRit, BandModelType::WilsonCowan(14.0), BandModelType::WilsonCowan(25.0)],
            },

            BrainType::Aging => TonotopicParams {
                band_rates: [
                    (60.0, 30.0),   // Very slow → delta/theta
                    (70.0, 35.0),   // Slow → theta
                    (85.0, 42.0),   // Slower than normal
                    (100.0, 50.0),  // Normal (reduced from 120)
                ],
                band_gains: [
                    (3.25, 22.0),
                    (3.25, 22.0),
                    (3.25, 22.0),
                    (3.0, 21.0),
                ],
                // Moderate offsets; slower rates shift frequency down
                band_offsets: [155.0, 170.0, 185.0, 180.0],
                band_input_gains: [1.0; 4],
                band_output_weights: [1.0; 4],
                band_slow_inhib_ratios: [0.20; 4],
                band_c7: [96.0; 4],
                band_sigmoid_r: [0.62; 4],
                band_c1c2_scale: [1.0; 4],
                band_g_fast_rate: [350.0; 4],
                band_v0: [6.0; 4],
                band_model_types: [BandModelType::JansenRit; 4],
            },

            // Anxious: overdriven cortex — elevated beta, hyperarousal
            BrainType::Anxious => TonotopicParams {
                band_rates: [
                    (80.0, 40.0),
                    (100.0, 50.0),
                    (115.0, 57.0),  // Faster → beta bias
                    (120.0, 60.0),
                ],
                band_gains: [
                    (3.4, 20.0),
                    (3.5, 19.0),
                    (3.5, 19.0),
                    (3.5, 19.0),
                ],
                // Deep in oscillatory regime → persistent high-frequency activity
                band_offsets: [190.0, 210.0, 230.0, 240.0],
                band_input_gains: [1.0; 4],
                band_output_weights: [1.0; 4],
                band_slow_inhib_ratios: [0.20; 4],
                band_c7: [116.0; 4],
                band_sigmoid_r: [0.62; 4],
                band_c1c2_scale: [1.0; 4],
                band_g_fast_rate: [500.0; 4],
                band_v0: [6.0; 4],
                band_model_types: [BandModelType::JansenRit; 4],
            },
        }
    }

    /// Get bilateral cortical parameters for this brain type.
    ///
    /// Each hemisphere has its own tonotopic params per the AST hypothesis:
    ///   Left hemisphere  = fast (α/β): integration ~25-40ms
    ///   Right hemisphere = slow (δ/θ): integration ~150-300ms
    ///
    /// Brain-type-specific hemispheric signatures:
    ///   Normal:    balanced asymmetry
    ///   ADHD:      right-hemisphere theta excess, weaker callosal coupling
    ///   Anxious:   left-hemisphere beta excess, stronger coupling
    ///   Aging:     both slower, much weaker callosal transfer
    ///   HighAlpha: strong bilateral alpha synchrony, stronger coupling
    pub fn bilateral_params(&self) -> BilateralParams {
        match self {
            BrainType::Normal => BilateralParams {
                // Left hemisphere: fast — θ/α/SMR/β (hybrid JR+WC)
                left: TonotopicParams {
                    band_rates: [
                        (80.0, 40.0),    // θ ~9 Hz (ratio 2.0)
                        (100.0, 50.0),   // α ~11 Hz (ratio 2.0)
                        (100.0, 50.0),   // Band 2: unused (WilsonCowan)
                        (100.0, 50.0),   // Band 3: unused (WilsonCowan)
                    ],
                    band_gains: [(3.25, 22.0); 4],
                    band_offsets: [150.0, 175.0, 150.0, 150.0],
                    band_input_gains: [1.0, 1.0, 1.2, 2.0],
                    band_output_weights: [0.5, 0.7, 1.5, 2.0],
                    band_slow_inhib_ratios: [0.20; 4],
                    band_c7: [115.0; 4],
                    band_sigmoid_r: [0.62; 4],
                    band_c1c2_scale: [1.0; 4],
                    band_g_fast_rate: [500.0; 4],
                    band_v0: [6.0; 4],
                    band_model_types: [BandModelType::JansenRit, BandModelType::JansenRit, BandModelType::WilsonCowan(14.0), BandModelType::WilsonCowan(25.0)],
                },
                // Right hemisphere: slow — θ-high/α-low/α/α-high
                // Recalibrated from (60,30)/(70,35) "delta drag" to awake-relaxed range.
                // Original rates produced ~7 Hz delta as the floor, dragging the bilateral
                // combined EEG to ~4 Hz. New rates set the floor at high-theta (~8.7 Hz),
                // matching the AST hypothesis without crossing into pathological delta.
                right: TonotopicParams {
                    band_rates: [
                        (75.0, 37.0),    // θ-high ~8.7 Hz
                        (85.0, 42.0),    // α-low ~9.5 Hz
                        (95.0, 47.0),    // α ~10.6 Hz
                        (100.0, 50.0),   // α ~11.3 Hz
                    ],
                    band_gains: [(3.25, 22.0); 4],
                    // Pushed deeper into oscillatory regime to produce alpha at rest
                    // rather than falling into delta from Hopf bifurcation proximity
                    band_offsets: [160.0, 180.0, 195.0, 200.0],
                    band_input_gains: [1.0; 4],
                    band_output_weights: [1.0; 4],
                    // Right hemisphere: uniform stabilizer profile
                    band_slow_inhib_ratios: [0.22; 4],
                    band_c7: [115.0; 4],
                    band_sigmoid_r: [0.62; 4],
                    band_c1c2_scale: [1.0; 4],
                    band_g_fast_rate: [500.0; 4],
                    band_v0: [6.0; 4],
                    band_model_types: [BandModelType::JansenRit; 4],
                },
                // Increased from 0.10 to allow left hemisphere's faster rhythms
                // to synchronize with right — prevents "callosal isolation"
                callosal_coupling: 0.15,
                callosal_delay_s: 0.010,
                contralateral_ratio: 0.65,
                left_weight: 0.5,
            },

            BrainType::HighAlpha => BilateralParams {
                // Both hemispheres converge toward alpha (meditation training)
                left: TonotopicParams {
                    band_rates: [
                        (85.0, 42.0),
                        (95.0, 47.0),
                        (105.0, 52.0),
                        (115.0, 57.0),
                    ],
                    band_gains: [
                        (3.25, 24.0),
                        (3.25, 25.0),
                        (3.25, 25.0),
                        (3.25, 24.0),
                    ],
                    // Mid bands deepest → strong bilateral alpha synchrony
                    band_offsets: [165.0, 195.0, 210.0, 185.0],
                    band_input_gains: [1.0; 4],
                    band_output_weights: [1.0; 4],
                    band_slow_inhib_ratios: [0.20; 4],
                    band_c7: [108.0; 4],
                    band_sigmoid_r: [0.62; 4],
                    band_c1c2_scale: [1.0; 4],
                    band_g_fast_rate: [500.0; 4],
                    band_v0: [6.0; 4],
                    band_model_types: [BandModelType::JansenRit; 4],
                },
                right: TonotopicParams {
                    band_rates: [
                        (70.0, 35.0),
                        (85.0, 42.0),
                        (95.0, 47.0),
                        (105.0, 52.0),
                    ],
                    band_gains: [
                        (3.25, 24.0),
                        (3.25, 25.0),
                        (3.25, 25.0),
                        (3.25, 24.0),
                    ],
                    band_offsets: [155.0, 185.0, 200.0, 175.0],
                    band_input_gains: [1.0; 4],
                    band_output_weights: [1.0; 4],
                    band_slow_inhib_ratios: [0.20; 4],
                    band_c7: [108.0; 4],
                    band_sigmoid_r: [0.62; 4],
                    band_c1c2_scale: [1.0; 4],
                    band_g_fast_rate: [500.0; 4],
                    band_v0: [6.0; 4],
                    band_model_types: [BandModelType::JansenRit; 4],
                },
                // Stronger callosal coupling — bilateral synchrony from training
                callosal_coupling: 0.15,
                callosal_delay_s: 0.010,
                contralateral_ratio: 0.65,
                left_weight: 0.5,
            },

            BrainType::Adhd => BilateralParams {
                // Left: fast hemisphere — Neuro-Plastic Shift applied
                // Raised offsets + stronger C7 = responsive but still ADHD-impaired
                left: TonotopicParams {
                    band_rates: [
                        (85.0, 42.0),
                        (105.0, 52.0),
                        (120.0, 60.0),
                        (135.0, 67.0),
                    ],
                    band_gains: [(3.5, 18.0); 4],
                    band_offsets: [140.0, 120.0, 105.0, 95.0],
                    band_input_gains: [1.0; 4],
                    band_output_weights: [1.0; 4],
                    band_slow_inhib_ratios: [0.18, 0.18, 0.12, 0.12],
                    band_c7: [112.0, 112.0, 115.0, 115.0],
                    band_sigmoid_r: [0.62; 4],
                    band_c1c2_scale: [1.0; 4],
                    band_g_fast_rate: [450.0; 4],
                    band_v0: [5.5; 4],
                    band_model_types: [BandModelType::JansenRit, BandModelType::JansenRit, BandModelType::WilsonCowan(14.0), BandModelType::WilsonCowan(25.0)],
                },
                // Right: Lite Hybrid — theta-dominant but capable of fast response
                // Bands 0-1: JR (slow, theta/alpha characteristic of ADHD right hemisphere)
                // Bands 2-3: WC at lower targets than left (12/20 vs 14/25)
                // Output weights dampened to preserve L>R asymmetry
                right: TonotopicParams {
                    band_rates: [
                        (65.0, 32.0),    // θ-low ~8.0 Hz
                        (75.0, 37.0),    // θ-high ~8.7 Hz
                        (85.0, 42.0),    // Band 2: unused (WilsonCowan)
                        (95.0, 47.0),    // Band 3: unused (WilsonCowan)
                    ],
                    band_gains: [(3.5, 18.0); 4],
                    band_offsets: [135.0, 120.0, 105.0, 95.0],
                    band_input_gains: [1.0; 4],
                    band_output_weights: [1.0, 1.0, 0.6, 0.4],
                    band_slow_inhib_ratios: [0.18; 4],
                    band_c7: [112.0; 4],
                    band_sigmoid_r: [0.62; 4],
                    band_c1c2_scale: [1.0; 4],
                    band_g_fast_rate: [450.0; 4],
                    band_v0: [5.5; 4],
                    band_model_types: [BandModelType::JansenRit, BandModelType::JansenRit, BandModelType::WilsonCowan(12.0), BandModelType::WilsonCowan(20.0)],
                },
                // Strengthened callosal coupling (was 0.08) — hemispheres can communicate
                callosal_coupling: 0.12,
                callosal_delay_s: 0.010,
                contralateral_ratio: 0.65,
                left_weight: 0.5,
            },

            BrainType::Aging => BilateralParams {
                // Both hemispheres slower; asymmetry reduced
                left: TonotopicParams {
                    band_rates: [
                        (70.0, 35.0),
                        (85.0, 42.0),
                        (100.0, 50.0),
                        (115.0, 57.0),
                    ],
                    band_gains: [
                        (3.25, 22.0),
                        (3.25, 22.0),
                        (3.0, 21.0),
                        (3.0, 21.0),
                    ],
                    // Moderate, slightly reduced from Normal
                    band_offsets: [155.0, 170.0, 185.0, 185.0],
                    band_input_gains: [1.0; 4],
                    band_output_weights: [1.0; 4],
                    band_slow_inhib_ratios: [0.20; 4],
                    band_c7: [96.0; 4],
                    band_sigmoid_r: [0.62; 4],
                    band_c1c2_scale: [1.0; 4],
                    band_g_fast_rate: [350.0; 4],
                    band_v0: [6.0; 4],
                    band_model_types: [BandModelType::JansenRit; 4],
                },
                right: TonotopicParams {
                    band_rates: [
                        (60.0, 30.0),    // θ-low ~7 Hz (was δ ~5.7 Hz)
                        (70.0, 35.0),    // θ ~8 Hz (was δ/θ ~7 Hz)
                        (80.0, 40.0),    // α-low ~9 Hz (was θ ~8 Hz)
                        (85.0, 42.0),    // α-low ~9.5 Hz (unchanged)
                    ],
                    band_gains: [
                        (3.25, 22.0),
                        (3.25, 22.0),
                        (3.0, 21.0),
                        (3.0, 21.0),
                    ],
                    band_offsets: [155.0, 170.0, 180.0, 180.0],
                    band_input_gains: [1.0; 4],
                    band_output_weights: [1.0; 4],
                    band_slow_inhib_ratios: [0.20; 4],
                    band_c7: [96.0; 4],
                    band_sigmoid_r: [0.62; 4],
                    band_c1c2_scale: [1.0; 4],
                    band_g_fast_rate: [350.0; 4],
                    band_v0: [6.0; 4],
                    band_model_types: [BandModelType::JansenRit; 4],
                },
                // Weaker callosal transfer — white matter degradation
                callosal_coupling: 0.07,
                callosal_delay_s: 0.012,  // Slower transfer
                contralateral_ratio: 0.65,
                left_weight: 0.5,
            },

            BrainType::Anxious => BilateralParams {
                // Left: beta excess (hyperactive left hemisphere)
                left: TonotopicParams {
                    band_rates: [
                        (90.0, 45.0),
                        (110.0, 55.0),
                        (130.0, 65.0),   // Strong β bias
                        (145.0, 72.0),   // β-high
                    ],
                    band_gains: [(3.5, 19.0); 4],
                    // Deep in oscillatory regime → persistent beta overactivation
                    band_offsets: [195.0, 215.0, 235.0, 245.0],
                    band_input_gains: [1.0; 4],
                    band_output_weights: [1.0; 4],
                    band_slow_inhib_ratios: [0.20; 4],
                    band_c7: [116.0; 4],
                    band_sigmoid_r: [0.62; 4],
                    band_c1c2_scale: [1.0; 4],
                    band_g_fast_rate: [500.0; 4],
                    band_v0: [6.0; 4],
                    band_model_types: [BandModelType::JansenRit; 4],
                },
                // Right: elevated but less extreme
                right: TonotopicParams {
                    band_rates: [
                        (70.0, 35.0),
                        (85.0, 42.0),
                        (100.0, 50.0),
                        (115.0, 57.0),
                    ],
                    band_gains: [(3.5, 19.0); 4],
                    band_offsets: [185.0, 205.0, 220.0, 230.0],
                    band_input_gains: [1.0; 4],
                    band_output_weights: [1.0; 4],
                    band_slow_inhib_ratios: [0.20; 4],
                    band_c7: [116.0; 4],
                    band_sigmoid_r: [0.62; 4],
                    band_c1c2_scale: [1.0; 4],
                    band_g_fast_rate: [500.0; 4],
                    band_v0: [6.0; 4],
                    band_model_types: [BandModelType::JansenRit; 4],
                },
                // Stronger coupling — hyperconnected
                callosal_coupling: 0.14,
                callosal_delay_s: 0.010,
                contralateral_ratio: 0.65,
                left_weight: 0.5,
            },
        }
    }

    /// Short description of this brain type's characteristics.
    pub fn description(&self) -> &'static str {
        match self {
            BrainType::Normal => "Healthy adult baseline",
            BrainType::HighAlpha => "Strong alpha, meditation practitioner",
            BrainType::Adhd => "Weaker inhibition, higher excitability",
            BrainType::Aging => "Slower dynamics, reduced connectivity",
            BrainType::Anxious => "Heightened excitability, elevated drive",
        }
    }
}

/// Combined neural model parameters for a brain type.
#[derive(Debug, Clone)]
pub struct NeuralParams {
    pub fhn: FhnParams,
    pub jansen_rit: JansenRitParams,
}

/// FitzHugh-Nagumo model parameters.
#[derive(Debug, Clone)]
pub struct FhnParams {
    pub a: f64,
    pub b: f64,
    pub epsilon: f64,
    pub input_scale: f64,
    /// Time-scale factor: maps real time to model time.
    /// Higher values allow FHN to track faster oscillations.
    /// Default: 300.0; active profiles may use 400-600.
    pub time_scale: f64,
}

/// Jansen-Rit model parameters (extended with Wendling 2002 fast inhibitory population).
#[derive(Debug, Clone)]
pub struct JansenRitParams {
    /// Excitatory synaptic gain (mV). Default: 3.25
    pub a_gain: f64,
    /// Slow inhibitory synaptic gain (mV, GABA-B). Default: 22.0
    pub b_gain: f64,
    /// Excitatory time constant (1/s). Default: 100.0
    pub a_rate: f64,
    /// Slow inhibitory time constant (1/s). Default: 50.0
    pub b_rate: f64,
    /// Connectivity constant. Default: 135.0
    pub c: f64,
    /// Mean input pulse density (pulses/s). Default: 220.0
    pub input_offset: f64,
    /// Input modulation scale. Default: 100.0
    pub input_scale: f64,
    // ── Wendling 2002 fast inhibitory (GABA-A) parameters ───────────
    /// Fast inhibitory synaptic gain (mV). Default: 10.0. Set 0 for JR95 mode.
    pub g_fast_gain: f64,
    /// Fast inhibitory time constant (1/s). Default: 500.0
    pub g_fast_rate: f64,
    /// Pyramidal → fast inhibitory connectivity. Default: 0.3*C
    pub c5: f64,
    /// Slow inhibitory → fast inhibitory connectivity (disinhibition). Default: 0.1*C
    pub c6: f64,
    /// Fast inhibitory → pyramidal connectivity (gamma loop). Default: 0.8*C
    pub c7: f64,
    /// Slow inhibitory ratio for C3/C4. Default: 0.20 (Universal).
    /// Lower values loosen the GABA-B theta anchor, enabling beta access.
    pub slow_inhib_ratio: f64,
    /// Sigmoid firing threshold (mV). Default: 6.0.
    /// Lower values (e.g. 5.5) increase excitability for hypoaroused profiles.
    pub v0: f64,
}

/// Which neural mass model to use for a given tonotopic band.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BandModelType {
    /// Jansen-Rit / Wendling — rich biological texture, optimal for delta/theta/alpha (0.5-13 Hz).
    JansenRit,
    /// Wilson-Cowan — directly tunable frequency, optimal for SMR/beta/gamma (13-100 Hz).
    /// The f64 is the target oscillation frequency in Hz.
    WilsonCowan(f64),
}

/// Per-band parameters for the tonotopic cortical model.
///
/// The key mechanism: each band operates at a different point in the JR
/// bifurcation diagram via its input_offset. Near the lower Hopf boundary
/// (~120-150), the model produces slower oscillations (delta/theta). At the
/// center (~200-250), it produces alpha. Near the upper boundary (~280-320),
/// it can produce mixed/faster dynamics.
#[derive(Debug, Clone)]
pub struct TonotopicParams {
    /// (a_rate, b_rate) per band — excitatory and inhibitory time constants.
    pub band_rates: [(f64, f64); 4],
    /// (a_gain, b_gain) per band — synaptic gains.
    pub band_gains: [(f64, f64); 4],
    /// Input offset per band — places each band at a different operating point
    /// in the JR bifurcation diagram. Lower → slower oscillation (delta/theta),
    /// higher → faster or mixed (beta).
    pub band_offsets: [f64; 4],
    /// Per-band input gain multiplier. Biases energy toward high-frequency
    /// channels so fast neurons can "out-shout" the slow GABA-B loop.
    /// Default: [1.0, 1.0, 1.0, 1.0] (neutral).
    pub band_input_gains: [f64; 4],
    /// Per-band output weight multiplier. Adjusts how much each band's EEG
    /// contributes to the combined signal (applied ON TOP of audio energy
    /// fractions). This is the "spectral leverage" — it lets the brain type
    /// bias which frequency bands dominate the combined output.
    /// Default: [1.0, 1.0, 1.0, 1.0] (neutral — energy fractions only).
    pub band_output_weights: [f64; 4],
    /// Per-band slow inhibition ratio (C3=C4 = ratio×C).
    /// Stabilizer bands (0-1): higher ratio preserves alpha/theta.
    /// Accelerator bands (2-3): lower ratio unlocks beta/gamma.
    /// Default: [0.20, 0.20, 0.20, 0.20] (uniform).
    pub band_slow_inhib_ratios: [f64; 4],
    /// Per-band fast inhibitory connectivity C7.
    /// Higher C7 gives the GABA-A loop more "torque" for fast oscillations.
    /// Default: same as global C7 for all bands.
    pub band_c7: [f64; 4],
    /// Per-band sigmoid steepness R.
    /// Steeper R prevents Hopf collapse at high frequencies.
    /// Default: [0.62, 0.62, 0.62, 0.62] (uniform).
    pub band_sigmoid_r: [f64; 4],
    /// Per-band C1/C2 scaling factor ("Lean Loop").
    /// Reduces primary excitatory loop coupling for faster cycling.
    /// Default: [1.0, 1.0, 1.0, 1.0] (standard coupling).
    pub band_c1c2_scale: [f64; 4],
    /// Per-band fast inhibitory kinetic rate (GABA-A response speed).
    /// Higher = faster brakes = shorter cycle period.
    /// Default: same as global g_fast_rate (500.0 for Normal).
    pub band_g_fast_rate: [f64; 4],
    /// Per-band sigmoid threshold V0.
    /// Lower V0 pulls the population back to the sigmoid's linear center,
    /// preventing saturation from high-frequency input.
    /// Default: same as global v0 (6.0 for Normal).
    pub band_v0: [f64; 4],
    /// Which neural model to use per band.
    /// Bands 0-1: JansenRit (delta/theta/alpha).
    /// Bands 2-3: WilsonCowan(target_hz) for SMR/beta/gamma.
    pub band_model_types: [BandModelType; 4],
}

/// Bilateral cortical parameters.
///
/// Models the asymmetric sampling in time (AST) hypothesis (Poeppel, 2003):
///   - Left hemisphere: shorter integration windows → alpha/beta preference
///   - Right hemisphere: longer integration windows → delta/theta preference
///
/// Each hemisphere gets 65% contralateral + 35% ipsilateral auditory input
/// (Gutschalk et al., 2015), coupled through the corpus callosum with ~10ms
/// delay and ~10% coupling strength (relative to intracortical connectivity).
#[derive(Debug, Clone)]
pub struct BilateralParams {
    /// Left hemisphere tonotopic params (processes mainly R ear — contralateral).
    /// Faster time constants per AST: alpha/beta bias.
    pub left: TonotopicParams,
    /// Right hemisphere tonotopic params (processes mainly L ear — contralateral).
    /// Slower time constants per AST: delta/theta bias.
    pub right: TonotopicParams,
    /// Callosal coupling strength as fraction of intracortical C.
    /// ~0.10 = anatomical baseline (10% of convergent input is callosal).
    pub callosal_coupling: f64,
    /// Interhemispheric transfer delay in seconds.
    /// ~0.010 = 10ms (N1 ERP contralateral-ipsilateral latency difference).
    pub callosal_delay_s: f64,
    /// Contralateral input fraction (0.65 = 65% contra, 35% ipsi).
    pub contralateral_ratio: f64,
    /// Left hemisphere weight in combined EEG (right = 1 - left_weight).
    /// Default: 0.5 (symmetric). AST-biased: 0.55 (left-fast for beta access).
    pub left_weight: f64,
}
