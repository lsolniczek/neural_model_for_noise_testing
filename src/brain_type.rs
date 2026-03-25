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
                },
                jansen_rit: JansenRitParams {
                    a_gain: 3.25,
                    b_gain: 22.0,
                    a_rate: 100.0,
                    b_rate: 50.0,
                    c: 135.0,
                    input_offset: 200.0,  // Center of oscillatory regime
                    input_scale: 60.0,    // Temporal modulation around center
                },
            },

            BrainType::HighAlpha => NeuralParams {
                fhn: FhnParams {
                    a: 0.7,
                    b: 0.8,
                    epsilon: 0.06,
                    input_scale: 1.3,
                },
                jansen_rit: JansenRitParams {
                    a_gain: 3.25,
                    b_gain: 25.0,   // Stronger inhibition → alpha dominance
                    a_rate: 100.0,
                    b_rate: 50.0,
                    c: 135.0,
                    input_offset: 220.0,  // Mid-alpha regime
                    input_scale: 40.0,    // Stable alpha
                },
            },

            BrainType::Adhd => NeuralParams {
                fhn: FhnParams {
                    a: 0.7,
                    b: 0.8,
                    epsilon: 0.10,
                    input_scale: 1.8,
                },
                jansen_rit: JansenRitParams {
                    a_gain: 3.5,    // Higher excitatory gain
                    b_gain: 18.0,   // Weaker inhibition
                    a_rate: 100.0,
                    b_rate: 50.0,
                    c: 135.0,
                    input_offset: 240.0,  // Higher drive
                    input_scale: 80.0,    // More reactive
                },
            },

            BrainType::Aging => NeuralParams {
                fhn: FhnParams {
                    a: 0.75,
                    b: 0.8,
                    epsilon: 0.06,
                    input_scale: 1.2,
                },
                jansen_rit: JansenRitParams {
                    a_gain: 3.25,
                    b_gain: 22.0,
                    a_rate: 80.0,   // Slower excitatory time constant
                    b_rate: 40.0,   // Slower inhibitory time constant
                    c: 120.0,       // Reduced connectivity
                    input_offset: 180.0,  // Lower drive → alpha/theta border
                    input_scale: 50.0,
                },
            },

            BrainType::Anxious => NeuralParams {
                fhn: FhnParams {
                    a: 0.7,
                    b: 0.8,
                    epsilon: 0.10,
                    input_scale: 1.8,
                },
                jansen_rit: JansenRitParams {
                    a_gain: 3.5,    // Higher excitability
                    b_gain: 19.0,   // Slightly weaker inhibition
                    a_rate: 100.0,
                    b_rate: 50.0,
                    c: 145.0,       // Higher connectivity
                    input_offset: 250.0,  // Higher baseline
                    input_scale: 70.0,
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
            BrainType::Normal => TonotopicParams {
                band_rates: [
                    (70.0, 35.0),   // Low → theta
                    (85.0, 42.0),   // Low-mid → alpha-low
                    (100.0, 50.0),  // Mid-high → alpha (standard)
                    (120.0, 60.0),  // High → beta-low
                ],
                band_gains: [
                    (3.25, 22.0),
                    (3.25, 22.0),
                    (3.25, 22.0),
                    (3.25, 22.0),
                ],
                band_offsets: [220.0; 4],  // All at center of oscillatory regime
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
                band_offsets: [220.0; 4],
            },

            BrainType::Adhd => TonotopicParams {
                band_rates: [
                    (75.0, 37.0),   // Slightly faster low band
                    (90.0, 45.0),
                    (110.0, 55.0),  // Faster mid → more beta
                    (120.0, 60.0),
                ],
                band_gains: [
                    (3.5, 18.0),    // Weaker inhibition
                    (3.5, 18.0),
                    (3.5, 19.0),
                    (3.5, 18.0),
                ],
                band_offsets: [240.0; 4],  // Higher drive
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
                band_offsets: [200.0; 4],
            },

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
                band_offsets: [250.0; 4],  // Higher baseline drive
            },
        }
    }

    /// Get bilateral cortical parameters for this brain type.
    ///
    /// Each hemisphere has its own tonotopic params per the AST hypothesis:
    ///   Left hemisphere  = fast (θ/γ): integration ~25-40ms
    ///   Right hemisphere = slow (δ/β): integration ~150-300ms
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
                // Left hemisphere: fast — θ/α/β-low/β-high
                left: TonotopicParams {
                    band_rates: [
                        (80.0, 40.0),    // θ ~9 Hz
                        (100.0, 50.0),   // α ~11 Hz
                        (120.0, 60.0),   // β-low ~14 Hz
                        (140.0, 70.0),   // β-high ~17 Hz
                    ],
                    band_gains: [(3.25, 22.0); 4],
                    band_offsets: [220.0; 4],
                },
                // Right hemisphere: slow — δ/θ/α-low/α
                right: TonotopicParams {
                    band_rates: [
                        (60.0, 30.0),    // δ ~7 Hz
                        (70.0, 35.0),    // θ ~8 Hz
                        (85.0, 42.0),    // α-low ~10 Hz
                        (100.0, 50.0),   // α ~11 Hz
                    ],
                    band_gains: [(3.25, 22.0); 4],
                    band_offsets: [220.0; 4],
                },
                callosal_coupling: 0.10,
                callosal_delay_s: 0.010,
                contralateral_ratio: 0.65,
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
                    band_offsets: [220.0; 4],
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
                    band_offsets: [220.0; 4],
                },
                // Stronger callosal coupling — bilateral synchrony from training
                callosal_coupling: 0.15,
                callosal_delay_s: 0.010,
                contralateral_ratio: 0.65,
            },

            BrainType::Adhd => BilateralParams {
                // Left: fast but unstable (weak inhibition)
                left: TonotopicParams {
                    band_rates: [
                        (85.0, 42.0),
                        (105.0, 52.0),
                        (120.0, 60.0),
                        (135.0, 67.0),
                    ],
                    band_gains: [(3.5, 18.0); 4],
                    band_offsets: [240.0; 4],
                },
                // Right: theta excess (documented ADHD signature)
                right: TonotopicParams {
                    band_rates: [
                        (55.0, 28.0),    // Strong delta → theta excess
                        (65.0, 32.0),    // θ
                        (80.0, 40.0),    // α-low (slower than normal)
                        (95.0, 47.0),    // α
                    ],
                    band_gains: [(3.5, 18.0); 4],
                    band_offsets: [240.0; 4],
                },
                // Weaker callosal coupling — reduced interhemispheric coherence
                callosal_coupling: 0.06,
                callosal_delay_s: 0.010,
                contralateral_ratio: 0.65,
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
                    band_offsets: [200.0; 4],
                },
                right: TonotopicParams {
                    band_rates: [
                        (50.0, 25.0),    // Very slow δ
                        (60.0, 30.0),    // δ/θ border
                        (70.0, 35.0),    // θ
                        (85.0, 42.0),    // α-low
                    ],
                    band_gains: [
                        (3.25, 22.0),
                        (3.25, 22.0),
                        (3.0, 21.0),
                        (3.0, 21.0),
                    ],
                    band_offsets: [200.0; 4],
                },
                // Much weaker callosal transfer — white matter degradation
                callosal_coupling: 0.05,
                callosal_delay_s: 0.012,  // Slower transfer
                contralateral_ratio: 0.65,
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
                    band_offsets: [250.0; 4],
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
                    band_offsets: [250.0; 4],
                },
                // Stronger coupling — hyperconnected
                callosal_coupling: 0.14,
                callosal_delay_s: 0.010,
                contralateral_ratio: 0.65,
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
}

/// Jansen-Rit model parameters.
#[derive(Debug, Clone)]
pub struct JansenRitParams {
    /// Excitatory synaptic gain (mV). Default: 3.25
    pub a_gain: f64,
    /// Inhibitory synaptic gain (mV). Default: 22.0
    pub b_gain: f64,
    /// Excitatory time constant (1/s). Default: 100.0
    pub a_rate: f64,
    /// Inhibitory time constant (1/s). Default: 50.0
    pub b_rate: f64,
    /// Connectivity constant. Default: 135.0
    pub c: f64,
    /// Mean input pulse density (pulses/s). Default: 220.0
    pub input_offset: f64,
    /// Input modulation scale. Default: 100.0
    pub input_scale: f64,
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
}

/// Bilateral cortical parameters.
///
/// Models the asymmetric sampling in time (AST) hypothesis (Poeppel, 2003):
///   - Left hemisphere: shorter integration windows → theta/gamma preference
///   - Right hemisphere: longer integration windows → delta/beta preference
///
/// Each hemisphere gets 65% contralateral + 35% ipsilateral auditory input
/// (Gutschalk et al., 2015), coupled through the corpus callosum with ~10ms
/// delay and ~10% coupling strength (relative to intracortical connectivity).
#[derive(Debug, Clone)]
pub struct BilateralParams {
    /// Left hemisphere tonotopic params (processes mainly R ear — contralateral).
    /// Faster time constants per AST: theta/gamma bias.
    pub left: TonotopicParams,
    /// Right hemisphere tonotopic params (processes mainly L ear — contralateral).
    /// Slower time constants per AST: delta/beta bias.
    pub right: TonotopicParams,
    /// Callosal coupling strength as fraction of intracortical C.
    /// ~0.10 = anatomical baseline (10% of convergent input is callosal).
    pub callosal_coupling: f64,
    /// Interhemispheric transfer delay in seconds.
    /// ~0.010 = 10ms (N1 ERP contralateral-ipsilateral latency difference).
    pub callosal_delay_s: f64,
    /// Contralateral input fraction (0.65 = 65% contra, 35% ipsi).
    pub contralateral_ratio: f64,
}
