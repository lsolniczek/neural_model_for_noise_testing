/// Scoring / goal system.
///
/// Each goal defines scientifically-grounded target ranges for EEG band powers
/// (min, ideal, max) and FHN firing characteristics. The optimizer maximises
/// the score returned by `Goal::evaluate()`.
///
/// ## Scoring formula
/// Band score = Gaussian function peaked at `ideal`, with smooth roll-off toward
/// `min`/`max` (≈ 5% score at boundaries). This prevents gaming by ensuring values
/// above the physiological maximum reduce the score just as much as values below
/// the minimum, while providing continuous gradients for the optimizer.
///
/// ## Scientific references
/// - Klimesch 1999: Alpha power and memory performance
/// - Cavanagh & Frank 2014: Frontal theta as working memory signal
/// - Ogilvie 2001: Sleep onset EEG dynamics
/// - Lomas et al. 2015: EEG during meditation
/// - Katahira et al. 2018: EEG correlates of flow state
/// - Engel & Fries 2010: Beta-band oscillations and active maintenance
use crate::acoustic_score::{AcousticFeatureVector, AcousticScoreResult};
use crate::neural::{BandPowers, FhnResult, JansenRitResult, PerformanceVector};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalEvidenceLevel {
    PracticalModelHeuristic,
    ComponentSupportedButNotGoalValidated,
    RequiresHumanValidationForEfficacyClaim,
}

impl GoalEvidenceLevel {
    pub fn as_str(self) -> &'static str {
        match self {
            GoalEvidenceLevel::PracticalModelHeuristic => "practical_model_heuristic",
            GoalEvidenceLevel::ComponentSupportedButNotGoalValidated => {
                "component_supported_but_not_goal_validated"
            }
            GoalEvidenceLevel::RequiresHumanValidationForEfficacyClaim => {
                "requires_human_validation_for_efficacy_claim"
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GoalSemantics {
    pub goal: GoalKind,
    pub plain_language_purpose: &'static str,
    pub product_objective: &'static str,
    pub primary_neural_proxies: &'static [&'static str],
    pub primary_acoustic_proxies: &'static [&'static str],
    pub best_use_cases: &'static [&'static str],
    pub unsupported_claims: &'static [&'static str],
    pub evidence_level: GoalEvidenceLevel,
}

impl<'de> Deserialize<'de> for GoalSemantics {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct OwnedGoalSemantics {
            goal: GoalKind,
            plain_language_purpose: String,
            product_objective: String,
            primary_neural_proxies: Vec<String>,
            primary_acoustic_proxies: Vec<String>,
            best_use_cases: Vec<String>,
            unsupported_claims: Vec<String>,
            evidence_level: GoalEvidenceLevel,
        }

        fn strings_match(actual: &[String], expected: &[&str]) -> bool {
            actual
                .iter()
                .map(String::as_str)
                .eq(expected.iter().copied())
        }

        let value = OwnedGoalSemantics::deserialize(deserializer)?;
        let expected = value.goal.semantics();
        let matches = value.plain_language_purpose == expected.plain_language_purpose
            && value.product_objective == expected.product_objective
            && strings_match(
                &value.primary_neural_proxies,
                expected.primary_neural_proxies,
            )
            && strings_match(
                &value.primary_acoustic_proxies,
                expected.primary_acoustic_proxies,
            )
            && strings_match(&value.best_use_cases, expected.best_use_cases)
            && strings_match(&value.unsupported_claims, expected.unsupported_claims)
            && value.evidence_level == expected.evidence_level;
        if !matches {
            return Err(serde::de::Error::custom(format!(
                "goal semantics do not match the canonical {} contract",
                value.goal
            )));
        }
        Ok(expected)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalKind {
    DeepRelaxation,
    Focus,
    Sleep,
    Isolation,
    Meditation,
    DeepWork,
    /// Shield: Beta-dominant focused masking. High beta for concentration,
    /// minimal theta to prevent mind-wandering, stable moderate FHN.
    Shield,
    /// Flow: Alpha-dominant rhythmic state. Alpha-beta synchronization,
    /// coherent JR oscillations, relaxed sustained productivity.
    Flow,
    /// Ignition: Gamma-driven ADHD activation. 40 Hz binding,
    /// high FHN firing to push through activation threshold.
    Ignition,
}

impl fmt::Display for GoalKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GoalKind::DeepRelaxation => write!(f, "deep_relaxation"),
            GoalKind::Focus => write!(f, "focus"),
            GoalKind::Sleep => write!(f, "sleep"),
            GoalKind::Isolation => write!(f, "isolation"),
            GoalKind::Meditation => write!(f, "meditation"),
            GoalKind::DeepWork => write!(f, "deep_work"),
            GoalKind::Shield => write!(f, "shield"),
            GoalKind::Flow => write!(f, "flow"),
            GoalKind::Ignition => write!(f, "ignition"),
        }
    }
}

impl GoalKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "deep_relaxation" | "relaxation" | "relax" => Some(GoalKind::DeepRelaxation),
            "focus" | "concentration" => Some(GoalKind::Focus),
            "sleep" => Some(GoalKind::Sleep),
            "isolation" | "masking" => Some(GoalKind::Isolation),
            "meditation" | "meditate" => Some(GoalKind::Meditation),
            "deep_work" | "deepwork" => Some(GoalKind::DeepWork),
            "shield" => Some(GoalKind::Shield),
            "flow" => Some(GoalKind::Flow),
            "ignition" => Some(GoalKind::Ignition),
            _ => None,
        }
    }

    /// All goal kinds for iteration.
    pub fn all() -> &'static [GoalKind] {
        &[
            GoalKind::Focus,
            GoalKind::DeepWork,
            GoalKind::Sleep,
            GoalKind::DeepRelaxation,
            GoalKind::Meditation,
            GoalKind::Isolation,
            GoalKind::Shield,
            GoalKind::Flow,
            GoalKind::Ignition,
        ]
    }

    pub fn semantics(self) -> GoalSemantics {
        match self {
            GoalKind::Focus => GoalSemantics {
                goal: self,
                plain_language_purpose: "Active attention profile for cognitively demanding tasks.",
                product_objective: "Prioritize a focused, task-engaged neural profile for work sessions.",
                primary_neural_proxies: &["beta prominence", "moderate frontal-theta support", "stable mid-high firing rate"],
                primary_acoustic_proxies: &["none (legacy neural score primary)", "optional masking/privacy metrics only when acoustic scoring is enabled"],
                best_use_cases: &["single-user concentration sessions", "structured task blocks"],
                unsupported_claims: &["Does not prove human attention improvement.", "Does not prove clinical efficacy for ADHD or any disorder."],
                evidence_level: GoalEvidenceLevel::RequiresHumanValidationForEfficacyClaim,
            },
            GoalKind::DeepWork => GoalSemantics {
                goal: self,
                plain_language_purpose: "Sustained calm productivity profile.",
                product_objective: "Favor alpha-dominant sustained-work conditions over high-arousal vigilance.",
                primary_neural_proxies: &["alpha dominance", "supportive theta", "moderate firing regularity"],
                primary_acoustic_proxies: &["none (legacy neural score primary)"],
                best_use_cases: &["long writing/coding blocks", "low-interruption desk work"],
                unsupported_claims: &["Does not prove improved output quality or productivity.", "Does not prove reduced cognitive fatigue in humans."],
                evidence_level: GoalEvidenceLevel::RequiresHumanValidationForEfficacyClaim,
            },
            GoalKind::Sleep => GoalSemantics {
                goal: self,
                plain_language_purpose: "Sleep-onset-friendly low-arousal profile.",
                product_objective: "Bias presets toward transition-into-sleep proxy patterns, not active cognition.",
                primary_neural_proxies: &["theta-dominant with emerging delta", "suppressed beta/gamma", "low firing-rate regime"],
                primary_acoustic_proxies: &["low high-frequency fraction preference", "comfort-oriented spectral tilt priors"],
                best_use_cases: &["pre-sleep wind-down", "bedtime ambient masking"],
                unsupported_claims: &["Does not prove slow-wave enhancement.", "Does not prove sleep memory consolidation benefit.", "Does not prove treatment of insomnia or sleep disorders."],
                evidence_level: GoalEvidenceLevel::RequiresHumanValidationForEfficacyClaim,
            },
            GoalKind::DeepRelaxation => GoalSemantics {
                goal: self,
                plain_language_purpose: "Low-stress relaxation profile.",
                product_objective: "Encourage calm-state neural proxies and reduced high-frequency activation.",
                primary_neural_proxies: &["theta+alpha co-dominance", "suppressed beta/gamma", "low firing-rate regime"],
                primary_acoustic_proxies: &["comfort-oriented spectral tilt", "low HF energy preference"],
                best_use_cases: &["recovery breaks", "evening decompression"],
                unsupported_claims: &["Does not prove clinical anxiety reduction.", "Does not prove long-term stress biomarker improvement."],
                evidence_level: GoalEvidenceLevel::RequiresHumanValidationForEfficacyClaim,
            },
            GoalKind::Meditation => GoalSemantics {
                goal: self,
                plain_language_purpose: "Focused-meditative proxy profile.",
                product_objective: "Align with concentrative meditation-like oscillatory proxies.",
                primary_neural_proxies: &["theta/alpha co-dominance", "low beta/gamma", "balanced hemispheric tendency"],
                primary_acoustic_proxies: &["comfort-oriented acoustic profile"],
                best_use_cases: &["guided breath sessions", "quiet contemplative practice"],
                unsupported_claims: &["Does not prove meditative depth.", "Does not prove psychiatric or cognitive therapeutic benefit."],
                evidence_level: GoalEvidenceLevel::RequiresHumanValidationForEfficacyClaim,
            },
            GoalKind::Isolation => GoalSemantics {
                goal: self,
                plain_language_purpose: "Neutral masking/isolation profile.",
                product_objective: "Support acoustic privacy and speech masking without targeting a specific cognitive state.",
                primary_neural_proxies: &["flat band-distribution preference", "neutral cortical-state proxy"],
                primary_acoustic_proxies: &["speech privacy", "speech-band masking ratio", "comfort proxy"],
                best_use_cases: &["open office masking", "privacy-focused ambient bed"],
                unsupported_claims: &["Does not prove cognitive enhancement.", "Does not prove better learning, memory, or focus outcomes."],
                evidence_level: GoalEvidenceLevel::PracticalModelHeuristic,
            },
            GoalKind::Shield => GoalSemantics {
                goal: self,
                plain_language_purpose: "Masking-friendly sustained focus support profile.",
                product_objective: "Blend masking/privacy utility with a stable attention-support neural proxy.",
                primary_neural_proxies: &["alpha+beta stable balance", "low theta mind-wandering proxy", "regular mid-rate firing"],
                primary_acoustic_proxies: &["speech privacy", "speech-band masking ratio", "comfort proxy"],
                best_use_cases: &["distraction-heavy work environments", "focus sessions requiring masking"],
                unsupported_claims: &["Does not prove human focus improvement.", "Does not prove universal distraction resistance.", "Does not prove clinical efficacy."],
                evidence_level: GoalEvidenceLevel::ComponentSupportedButNotGoalValidated,
            },
            GoalKind::Flow => GoalSemantics {
                goal: self,
                plain_language_purpose: "Rhythmic relaxed-engagement profile.",
                product_objective: "Target a calm-but-engaged proxy state between deep relaxation and high-vigilance focus.",
                primary_neural_proxies: &["alpha-dominant with moderate beta", "rhythmic moderate firing", "low delta drowsiness"],
                primary_acoustic_proxies: &["none (legacy neural score primary)"],
                best_use_cases: &["creative sessions", "sustained medium-intensity cognitive work"],
                unsupported_claims: &["Does not prove psychological flow-state attainment.", "Does not prove performance gains on creative tasks."],
                evidence_level: GoalEvidenceLevel::RequiresHumanValidationForEfficacyClaim,
            },
            GoalKind::Ignition => GoalSemantics {
                goal: self,
                plain_language_purpose: "Exploratory activation profile.",
                product_objective: "Explore higher-activation ASSR/gamma-leaning proxy conditions.",
                primary_neural_proxies: &["elevated beta/gamma targets", "high firing-rate regime", "entrainment-weighted path"],
                primary_acoustic_proxies: &["none (legacy neural score primary)"],
                best_use_cases: &["controlled exploratory prototyping", "research-oriented activation comparisons"],
                unsupported_claims: &["Does not prove ADHD treatment efficacy.", "Does not prove therapeutic activation benefit.", "Does not prove safety or suitability for all users."],
                evidence_level: GoalEvidenceLevel::RequiresHumanValidationForEfficacyClaim,
            },
        }
    }
}

/// Target range for a single EEG band.
///
/// Scoring uses a Gaussian curve centred on `ideal` with smooth roll-off.
/// Unlike the previous triangular function, values at/beyond the boundaries
/// receive a small nonzero score instead of hard zero, providing continuous
/// gradients for the optimizer and better matching biological homeostasis.
#[derive(Debug, Clone, Copy)]
struct BandTarget {
    min: f64,
    ideal: f64,
    max: f64,
}

impl BandTarget {
    fn score(&self, power: f64) -> f64 {
        let half_width = (self.max - self.min) / 2.0;
        if half_width < 1e-12 {
            return 0.0;
        }
        // Sigma chosen so that score at min/max ≈ 0.05 (smooth, not hard zero).
        // At distance = half_width from ideal, exp(-0.5 * (half_width/sigma)^2) ≈ 0.05
        // => sigma = half_width / sqrt(-2 * ln(0.05)) ≈ half_width / 2.448
        let sigma = half_width / 2.448;
        let dist = power - self.ideal;
        (-0.5 * (dist / sigma).powi(2)).exp()
    }

    fn expectation(&self) -> BandExpectation {
        BandExpectation::Range(self.min, self.ideal, self.max)
    }

    fn status(&self, power: f64) -> MetricStatus {
        if power >= self.min && power <= self.max {
            if (power - self.ideal).abs() <= (self.max - self.min) * 0.25 {
                MetricStatus::Pass
            } else {
                MetricStatus::Warn
            }
        } else {
            MetricStatus::Fail
        }
    }
}

/// Per-goal EEG band targets (min, ideal, max for each band).
struct BandTargets {
    delta: BandTarget,
    theta: BandTarget,
    alpha: BandTarget,
    beta: BandTarget,
    gamma: BandTarget,
}

/// Target FHN characteristics.
struct FhnTargets {
    /// Desired firing rate range (spikes/s).
    firing_rate_range: (f64, f64),
    /// Desired ISI CV. Physiological range: 0.20–0.60.
    /// None means "don't care".
    target_isi_cv: Option<f64>,
    /// Weight of the FHN component in the total score.
    weight: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AcousticFusionOutcome {
    pub acoustic_goal_score: f64,
    pub comfort_score: f64,
    pub fused_score: f64,
    pub nmm_weight: f64,
    pub acoustic_weight: f64,
}

pub struct Goal {
    kind: GoalKind,
    band_targets: BandTargets,
    fhn_targets: FhnTargets,
    /// Weight for the band power component (0–1). Must sum to 1.0 with fhn weight.
    band_weight: f64,
}

impl Goal {
    pub fn new(kind: GoalKind) -> Self {
        match kind {
            // ── Deep Relaxation ──────────────────────────────────────────────
            // Theta + alpha dominant, delta moderate, suppress beta/gamma.
            // Eyes-closed relaxation / body scan / pre-sleep unwinding.
            // Ref: Klimesch 1999 (alpha in relaxation), Niedermeyer 2005.
            GoalKind::DeepRelaxation => Goal {
                kind,
                band_targets: BandTargets {
                    delta: BandTarget {
                        min: 0.05,
                        ideal: 0.22,
                        max: 0.40,
                    },
                    theta: BandTarget {
                        min: 0.18,
                        ideal: 0.35,
                        max: 0.52,
                    },
                    alpha: BandTarget {
                        min: 0.20,
                        ideal: 0.36,
                        max: 0.52,
                    },
                    beta: BandTarget {
                        min: 0.00,
                        ideal: 0.03,
                        max: 0.14,
                    },
                    gamma: BandTarget {
                        min: 0.00,
                        ideal: 0.01,
                        max: 0.06,
                    },
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (1.0, 6.0),
                    target_isi_cv: Some(0.38), // Moderate irregularity — relaxed state
                    weight: 0.30,
                },
                band_weight: 0.70,
            },

            // ── Active Focus / Vigilance ─────────────────────────────────────
            // Beta prominent, alpha moderate, frontal theta present (cognitive
            // control), delta suppressed. Models active task engagement —
            // studying, monitoring, problem-solving under pressure.
            // Ref: Engel & Fries 2010 (beta maintenance), Cavanagh & Frank 2014.
            GoalKind::Focus => Goal {
                kind,
                band_targets: BandTargets {
                    delta: BandTarget {
                        min: 0.00,
                        ideal: 0.01,
                        max: 0.08,
                    },
                    theta: BandTarget {
                        min: 0.08,
                        ideal: 0.18,
                        max: 0.32,
                    },
                    alpha: BandTarget {
                        min: 0.18,
                        ideal: 0.33,
                        max: 0.50,
                    },
                    beta: BandTarget {
                        min: 0.25,
                        ideal: 0.42,
                        max: 0.60,
                    },
                    // NOTE: The Jansen-Rit model cannot produce >17 Hz oscillations,
                    // so gamma power (30-50 Hz) comes from the WilsonCowan oscillators
                    // in tonotopic bands 2-3 (see BandModelType::WilsonCowan in brain_type.rs).
                    gamma: BandTarget {
                        min: 0.02,
                        ideal: 0.06,
                        max: 0.15,
                    },
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (8.0, 20.0),
                    target_isi_cv: Some(0.30),
                    weight: 0.30,
                },
                band_weight: 0.70,
            },

            // ── Sleep Onset ──────────────────────────────────────────────────
            // NREM stage 1–2: theta dominant, delta emerging, alpha fading.
            // Models the transition into sleep — noise machines target this
            // phase, not deep slow-wave sleep.
            // Ideals sum ≈ 0.94 for achievable band scores on normalized powers.
            // Ref: Ogilvie 2001 (sleep onset EEG), Carskadon & Dement 2011.
            GoalKind::Sleep => Goal {
                kind,
                band_targets: BandTargets {
                    delta: BandTarget {
                        min: 0.08,
                        ideal: 0.30,
                        max: 0.50,
                    },
                    theta: BandTarget {
                        min: 0.28,
                        ideal: 0.48,
                        max: 0.68,
                    },
                    alpha: BandTarget {
                        min: 0.00,
                        ideal: 0.12,
                        max: 0.25,
                    },
                    beta: BandTarget {
                        min: 0.00,
                        ideal: 0.02,
                        max: 0.08,
                    },
                    gamma: BandTarget {
                        min: 0.00,
                        ideal: 0.02,
                        max: 0.06,
                    },
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (0.5, 4.0),
                    target_isi_cv: Some(0.42), // Bursting pattern during NREM
                    weight: 0.35,
                },
                band_weight: 0.65,
            },

            // ── Isolation / Masking ──────────────────────────────────────────
            // Flat spectral distribution — neutral cortical state.
            // Masking noise should not entrain any particular rhythm.
            GoalKind::Isolation => Goal {
                kind,
                band_targets: BandTargets {
                    delta: BandTarget {
                        min: 0.10,
                        ideal: 0.20,
                        max: 0.30,
                    },
                    theta: BandTarget {
                        min: 0.10,
                        ideal: 0.20,
                        max: 0.30,
                    },
                    alpha: BandTarget {
                        min: 0.10,
                        ideal: 0.20,
                        max: 0.30,
                    },
                    beta: BandTarget {
                        min: 0.10,
                        ideal: 0.20,
                        max: 0.30,
                    },
                    gamma: BandTarget {
                        min: 0.10,
                        ideal: 0.20,
                        max: 0.30,
                    },
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (2.0, 10.0),
                    target_isi_cv: None,
                    weight: 0.20,
                },
                band_weight: 0.80, // Fixed: was 0.40, causing score cap at ~0.64
            },

            // ── Focused-Attention Meditation ─────────────────────────────────
            // Theta + alpha co-dominant. Models breath-counting / concentrative
            // meditation (samatha, zazen, TM). Not open-monitoring (vipassana)
            // which shows more gamma.
            // Ref: Lomas et al. 2015 meta-analysis (theta/alpha in meditation).
            GoalKind::Meditation => Goal {
                kind,
                band_targets: BandTargets {
                    delta: BandTarget {
                        min: 0.02,
                        ideal: 0.08,
                        max: 0.22,
                    },
                    theta: BandTarget {
                        min: 0.25,
                        ideal: 0.40,
                        max: 0.56,
                    },
                    alpha: BandTarget {
                        min: 0.25,
                        ideal: 0.40,
                        max: 0.56,
                    },
                    beta: BandTarget {
                        min: 0.00,
                        ideal: 0.03,
                        max: 0.12,
                    },
                    gamma: BandTarget {
                        min: 0.00,
                        ideal: 0.02,
                        max: 0.08,
                    },
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (1.0, 6.0),
                    target_isi_cv: Some(0.28), // Rhythmic but not robotic
                    weight: 0.35,
                },
                band_weight: 0.65,
            },

            // ── Deep Work / Flow State ───────────────────────────────────────
            // Alpha dominant (relaxed sustained attention), theta supporting
            // (working memory / hippocampal-cortical dialogue), beta low-moderate
            // (engaged but not stressed), delta suppressed (not drowsy).
            // Models Cal Newport's "deep work" — flow state for cognitively
            // demanding tasks. Distinct from active focus (beta-heavy) and
            // meditation (theta-heavy).
            // Ref: Katahira et al. 2018 (alpha in flow), Ulrich et al. 2016.
            GoalKind::DeepWork => Goal {
                kind,
                band_targets: BandTargets {
                    delta: BandTarget {
                        min: 0.00,
                        ideal: 0.01,
                        max: 0.06,
                    },
                    theta: BandTarget {
                        min: 0.15,
                        ideal: 0.30,
                        max: 0.46,
                    },
                    alpha: BandTarget {
                        min: 0.35,
                        ideal: 0.52,
                        max: 0.70,
                    }, // dominant
                    beta: BandTarget {
                        min: 0.02,
                        ideal: 0.10,
                        max: 0.24,
                    },
                    gamma: BandTarget {
                        min: 0.00,
                        ideal: 0.02,
                        max: 0.08,
                    },
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (4.0, 12.0),
                    target_isi_cv: Some(0.30),
                    weight: 0.25,
                },
                band_weight: 0.75,
            },

            // ── Shield: Beta-Dominant Focused Masking ───────────────────────
            // High beta for task concentration, minimal theta to prevent
            // mind-wandering, stable moderate FHN firing.
            // Ref: Engel & Fries 2010 (beta maintenance hypothesis),
            //      Cavanagh & Frank 2014 (theta suppression in focused attention).
            // Shield: Alpha-dominant masking. "The Shield" — the brain recognizes
            // pink 1/f noise as "no information" and habituates rapidly. Alpha
            // dominant = cortex in idle-ready state; moderate beta = baseline
            // alertness without anxiety; minimal theta = no mind-wandering;
            // minimal delta = no drowsiness; minimal gamma = no external binding.
            //
            // This is NOT beta-dominant focus — it's the neurological state of
            // "ready but unstimulated" that enables the fastest habituation.
            //
            // Ref: Klimesch 1999 (alpha as cortical idle rhythm),
            //      Engel & Fries 2010 (beta as status-quo maintenance),
            //      Bastiaansen & Hagoort 2003 (theta suppression in masking).
            GoalKind::Shield => Goal {
                kind,
                band_targets: BandTargets {
                    delta: BandTarget {
                        min: 0.00,
                        ideal: 0.03,
                        max: 0.08,
                    },
                    theta: BandTarget {
                        min: 0.00,
                        ideal: 0.05,
                        max: 0.12,
                    },
                    alpha: BandTarget {
                        min: 0.35,
                        ideal: 0.50,
                        max: 0.65,
                    },
                    beta: BandTarget {
                        min: 0.20,
                        ideal: 0.30,
                        max: 0.40,
                    },
                    gamma: BandTarget {
                        min: 0.00,
                        ideal: 0.03,
                        max: 0.08,
                    },
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (5.0, 12.0),
                    target_isi_cv: Some(0.15), // Very regular — no novelty response
                    weight: 0.30,
                },
                band_weight: 0.70,
            },

            // ── Flow: Alpha-Dominant Rhythmic Synchronization ───────────────
            // Dominant alpha for relaxed alertness, moderate beta for task
            // engagement, coherent JR oscillations, rhythmic FHN firing.
            // The neurological flow state: creativity + calm.
            // Ref: Katahira et al. 2018 (alpha in flow),
            //      Csikszentmihalyi 1990 (flow state psychology).
            GoalKind::Flow => Goal {
                kind,
                band_targets: BandTargets {
                    delta: BandTarget {
                        min: 0.00,
                        ideal: 0.05,
                        max: 0.15,
                    },
                    theta: BandTarget {
                        min: 0.05,
                        ideal: 0.15,
                        max: 0.30,
                    },
                    alpha: BandTarget {
                        min: 0.30,
                        ideal: 0.45,
                        max: 0.60,
                    },
                    beta: BandTarget {
                        min: 0.15,
                        ideal: 0.30,
                        max: 0.45,
                    },
                    gamma: BandTarget {
                        min: 0.00,
                        ideal: 0.02,
                        max: 0.08,
                    },
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (3.0, 12.0),
                    target_isi_cv: Some(0.30), // Rhythmic oscillation
                    weight: 0.30,
                },
                band_weight: 0.70,
            },

            // ── Ignition: Gamma-Driven ADHD Activation ──────────────────────
            // 40 Hz gamma for cognitive binding (Iaccarino 2016), high beta
            // for activation, elevated FHN firing to push through the ADHD
            // activation threshold (reduced synaptic gain in ADHD model).
            // Ref: Iaccarino et al. 2016 (40 Hz entrainment),
            //      Söderlund et al. 2007 (stochastic resonance in ADHD).
            // ── Ignition: Stochastic Resonance + Gamma Binding for ADHD ────
            // Under-arousal → active cognitive readiness. Lower the
            // activation threshold via stochastic resonance (Söderlund 2007).
            // 40 Hz gamma binding (Iaccarino 2016). Suppress theta excess.
            // High FHN firing with LOW ISI CV = ordered, "locked-in" rhythm.
            GoalKind::Ignition => Goal {
                kind,
                band_targets: BandTargets {
                    delta: BandTarget {
                        min: 0.00,
                        ideal: 0.02,
                        max: 0.05,
                    },
                    theta: BandTarget {
                        min: 0.00,
                        ideal: 0.10,
                        max: 0.25,
                    },
                    alpha: BandTarget {
                        min: 0.10,
                        ideal: 0.20,
                        max: 0.35,
                    },
                    beta: BandTarget {
                        min: 0.35,
                        ideal: 0.50,
                        max: 0.65,
                    },
                    gamma: BandTarget {
                        min: 0.05,
                        ideal: 0.15,
                        max: 0.35,
                    },
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (12.0, 25.0),
                    target_isi_cv: Some(0.20), // Ordered rhythm — locked-in firing
                    weight: 0.30,
                },
                band_weight: 0.70,
            },
        }
    }

    pub fn kind(&self) -> GoalKind {
        self.kind
    }

    /// Evaluate a simulation result against this goal. Returns score in [0, 1].
    pub fn evaluate(&self, fhn: &FhnResult, jansen_rit: &JansenRitResult) -> f64 {
        self.evaluate_with_brightness(fhn, jansen_rit, 0.5)
    }

    /// Evaluate with spectral brightness modifier.
    ///
    /// Brightness ∈ [0, 1] captures the spectral character of the noise:
    ///   0.0 = very dark (brown noise), 0.5 = mid (pink), 1.0 = bright (white).
    pub fn evaluate_with_brightness(
        &self,
        fhn: &FhnResult,
        jansen_rit: &JansenRitResult,
        brightness: f64,
    ) -> f64 {
        let band_score = self.score_bands(&jansen_rit.band_powers);
        let fhn_score = self.score_fhn(fhn);

        let neural_score = self.band_weight * band_score + self.fhn_targets.weight * fhn_score;

        // Brightness modifier removed per Zwicker & Fastl (1999): brightness is
        // a perceptual construct derived from the same cochlear excitation that
        // feeds the neural model. With global band normalization (Priority 1a),
        // the neural model now captures spectral differences directly — adding
        // brightness as a separate term double-counts spectral information.
        let _ = brightness; // parameter kept for API compatibility

        neural_score.clamp(0.0, 1.0)
    }

    /// Evaluate with alpha asymmetry penalty.
    ///
    /// Per Davidson (2004) and Allen et al. (2004), hemispheric alpha asymmetry
    /// is a marker of cognitive state. Goals that want balanced processing
    /// (meditation, relaxation) penalize excessive asymmetry. Goals where
    /// lateralization is acceptable (focus, sleep) don't penalize.
    ///
    /// `alpha_asymmetry` ∈ [-1, 1]: 0 = balanced, ±1 = fully lateralized.
    pub fn evaluate_with_asymmetry(
        &self,
        fhn: &FhnResult,
        jansen_rit: &JansenRitResult,
        alpha_asymmetry: f64,
    ) -> f64 {
        let base_score = self.evaluate_with_brightness(fhn, jansen_rit, 0.5);

        let penalty = self.asymmetry_penalty(alpha_asymmetry);
        (base_score * (1.0 - penalty)).clamp(0.0, 1.0)
    }

    /// Evaluate with all corrections: asymmetry penalty + carrier PLV bonus
    /// + envelope-phase PLV bonus (CET 13c).
    ///
    /// Per Lachaux et al. (1999) and Helfrich et al. (2014), phase-locking
    /// to the *modulation frequency* (carrier PLV) indicates entrainment to
    /// the LFO. Goals that want carrier entrainment (Focus, Isolation,
    /// Ignition) get a bonus proportional to carrier PLV.
    ///
    /// Per Ding & Simon (2014) and Luo & Poeppel (2007), phase-locking to
    /// the *slow envelope* (envelope PLV) is the cortical envelope tracking
    /// metric. Relaxation-family goals (Sleep, Deep Relaxation, Meditation)
    /// — which currently have carrier PLV weight 0% because they don't want
    /// rigid entrainment to a tone — DO benefit from envelope PLV because
    /// that's the natural slow-rhythm tracking the brain does for organic
    /// sounds (wind, surf, breath).
    ///
    /// Both bonuses are additive on different perceptual axes; a preset can
    /// score on both simultaneously.
    pub fn evaluate_full(
        &self,
        fhn: &FhnResult,
        jansen_rit: &JansenRitResult,
        alpha_asymmetry: f64,
        plv: Option<f64>,
        envelope_plv: Option<f64>,
    ) -> f64 {
        let base_score = self.evaluate_with_asymmetry(fhn, jansen_rit, alpha_asymmetry);

        // Carrier PLV bonus: weighted by goal's carrier entrainment relevance.
        let plv_bonus = if let Some(plv_value) = plv {
            let weight = self.entrainment_weight();
            weight * plv_value * 0.10 // max 10% bonus at PLV=1.0
        } else {
            0.0
        };

        // Envelope-phase PLV bonus (CET 13c): weighted by goal's CET relevance.
        // Up to 10% bonus at envelope_plv=1.0 × envelope_weight.
        let env_plv_bonus = if let Some(env_value) = envelope_plv {
            let weight = self.envelope_entrainment_weight();
            weight * env_value * 0.10
        } else {
            0.0
        };

        (base_score + plv_bonus + env_plv_bonus).clamp(0.0, 1.0)
    }

    pub fn supports_acoustic_fusion(&self) -> bool {
        matches!(self.kind, GoalKind::Shield | GoalKind::Isolation)
    }

    pub fn evaluate_with_acoustic_fusion(
        &self,
        neural_score: f64,
        acoustic: &AcousticScoreResult,
    ) -> Option<AcousticFusionOutcome> {
        let (nmm_weight, acoustic_weight) = self.acoustic_fusion_weights()?;
        let acoustic_goal_score = self.acoustic_goal_score(acoustic)?;
        let comfort_score = self.acoustic_comfort_score(acoustic)?;
        let fused_score =
            (nmm_weight * neural_score + acoustic_weight * acoustic_goal_score).clamp(0.0, 1.0);

        Some(AcousticFusionOutcome {
            acoustic_goal_score,
            comfort_score,
            fused_score,
            nmm_weight,
            acoustic_weight,
        })
    }

    // ─────────────────────────────────────────────────────────────────────
    // Priority 28 Phase 2 — comfort-violation function for ε-constrained DE
    //
    // Maps the diagnostic comfort metrics (Phase 1) to a non-negative
    // scalar violation that the DE's ε-constrained comparator (Takahama
    // & Sakai 2009) consumes.
    //
    // **Standards / heuristic split** (re-evaluated 2026-05-01 after
    // external code review):
    //
    // | Term                     | Measurement                     | Threshold       |
    // | ------------------------ | ------------------------------- | --------------- |
    // | LUFS asymmetry           | per-channel BS.1770 (independent | 1/2 LU heuristic |
    // |                          | gating; standards-correct)      |                 |
    // | True-peak ceiling        | BS.1770-5 Annex 2 polyphase FIR | −1 dBFS         |
    // |                          | (standards-correct)             | (mastering)     |
    // | Spectral-tilt deviation  | Welch + 1/6-octave-bin (Welch   | per-goal target |
    // |                          | 1967; IEC 61260; robust)        | heuristic       |
    // | HF fraction              | single-FFT integration (simple, | 0.10/0.20       |
    // |                          | acceptable for relative use)    | heuristic       |
    // | PLR                      | derived from BS.1770 inputs     | 12/18 dB        |
    // |                          | (standards-grounded)            | heuristic       |
    // | Source-balance dB range  | volume-only proxy (HEURISTIC)   | 6/8 dB          |
    // |                          |                                 | heuristic       |
    //
    // The *measurement procedures* for LUFS, true-peak, tilt, and PLR
    // are all standards-derived (or standards-inspired). The *threshold
    // values* and the source-balance proxy are engineering priors —
    // tunable, calibrated by listening tests over time, not standards.
    //
    // Each term is bounded to a small max-penalty so the aggregate
    // violation stays roughly in [0, 0.80] for any input. The bounded
    // range matters because ε(t) decays through this scale; an
    // unbounded violation could push the population to always-feasible
    // or always-infeasible at chosen ε₀ percentiles.
    //
    // Missing metrics (`Option::None`) contribute zero — they are treated
    // as "not measurable on this preset" rather than "no violation".
    //
    // References: Takahama & Sakai 2009 (ε-constrained DE); ITU-R
    // BS.1770-5 (LUFS / true-peak); Welch 1967 / IEC 61260 (Welch PSD,
    // fractional-octave bandpass); Voss & Clarke 1975 (1/f); WHO 2009
    // / Basner 2022 (long-exposure fatigue context only — not for
    // numerical thresholds); Vickers 2010 / Pestana 2013 (PLR
    // background, not threshold values).
    // ─────────────────────────────────────────────────────────────────────

    /// Compute the aggregated comfort violation for this goal.
    ///
    /// Returns 0.0 when every measured comfort metric is within its
    /// goal-specific tolerance, otherwise a non-negative scalar bounded
    /// by the sum of the per-term caps (≈ 0.85 across all seven terms:
    /// LUFS asymmetry, true-peak, spectral tilt, HF fraction, PLR,
    /// source balance, min-source). Higher = more uncomfortable; the
    /// ε-constrained DE prefers lower violation.
    pub fn comfort_violation(&self, features: &AcousticFeatureVector) -> f64 {
        let mut violation = 0.0_f64;

        // §28a — LUFS asymmetry.
        // Inputs are standards-correct per-channel BS.1770 readings (see
        // `lufs_left`, `lufs_right` doc). The threshold (3 LU for balanced
        // goals, 4 LU for active/attention goals — calibrated 2026-05-01
        // from the empirical p90 of curated presets) is a HEURISTIC
        // engineering prior, tunable. Linear ramp from zero at threshold
        // to LUFS_ASYM_CAP at 2× threshold.
        if let Some(asym) = features.lufs_asymmetry_lu {
            let threshold = self.lufs_asymmetry_threshold_lu();
            violation += linear_violation(asym, threshold, threshold * 2.0, LUFS_ASYM_CAP);
        }

        // §28a — true-peak ceiling.
        // Input is BS.1770-5 Annex 2 compliant (4× polyphase FIR, ≥60 dB
        // stopband). The −1 dBFS ceiling is a mastering convention, not
        // a BS.1770 threshold per se. Excess ramps to TRUE_PEAK_CAP at
        // +2 dBFS.
        if let Some(tp) = features.true_peak_dbfs {
            violation += linear_violation(
                tp,
                TRUE_PEAK_THRESHOLD_DBFS,
                TRUE_PEAK_CAP_DBFS,
                TRUE_PEAK_CAP,
            );
        }

        // §28c — spectral-tilt deviation from the goal's preferred slope.
        // Input is robust (Welch + 1/6-octave-bin regression). The
        // per-goal target slopes (−6 / −3 / −1.5 dB/oct) are HEURISTIC
        // engineering priors loosely informed by the 1/f literature
        // (Voss & Clarke 1975) and WHO/Basner long-exposure context, not
        // standards-derived numerical thresholds.
        if let Some(tilt) = features.spectral_tilt_db_per_oct {
            let target = self.spectral_tilt_target_db_per_oct();
            let dev = (tilt - target).abs();
            violation += linear_violation(
                dev,
                SPECTRAL_TILT_TOLERANCE_DB,
                SPECTRAL_TILT_CAP_DB,
                SPECTRAL_TILT_CAP,
            );
        }

        // §28c — HF-fraction guardrail.
        // Simple integration metric. The 0.10 / 0.20 thresholds are
        // HEURISTIC engineering priors — informed by WHO/Basner fatigue
        // context but not directly standards-derived.
        if let Some(hf) = features.hf_fraction_above_8khz {
            let threshold = self.hf_fraction_threshold();
            violation += linear_violation(hf, threshold, threshold * 2.0, HF_FRACTION_CAP);
        }

        // §28d — Peak-to-Loudness Ratio cap.
        // Input is `true_peak_dbfs − lufs_integrated`, both
        // standards-grounded. The 12 / 18 dB thresholds are HEURISTIC
        // engineering priors — Vickers 2010 / Pestana 2013 motivate
        // PLR as a sustained-listening metric, but the specific values
        // come from product judgment, not a standard. Skipped for
        // Ignition because sharp transients are intentional there.
        if let Some(plr) = features.plr_db {
            if !matches!(self.kind, GoalKind::Ignition) {
                violation += linear_violation(plr, PLR_THRESHOLD_DB, PLR_CAP_DB, PLR_CAP);
            }
        }

        // §28b — per-source loudness equity.
        // HEURISTIC end-to-end: the input itself is a *volume-only
        // proxy* (does not see color / tint / modulation / spread /
        // reverb). The 12 / 15 dB thresholds (calibrated 2026-05-01
        // from the empirical p90 of 50 curated presets) are tuning
        // priors that loosened the original 6 / 8 dB rule from
        // `feedback_balanced_cocoon` to match the actual range of the
        // user's curated set. Treat this as a soft house-rule
        // constraint, not a perceptual-loudness measurement.
        if let Some(range_db) = features.source_balance_db_range {
            let threshold = self.source_balance_threshold_db();
            violation += linear_violation(range_db, threshold, threshold * 2.0, SOURCE_BALANCE_CAP);
        }

        // §28b (companion to source_balance) — minimum active-source
        // count. HEURISTIC. Without this, a cocoon-style goal can
        // trivially satisfy `source_balance_db_range` by collapsing
        // to a single source (1 source ⇒ 0 dB range ⇒ no equity
        // violation), but a 1-source preset is not a cocoon. The
        // per-goal minimum encodes the design intent that Shield /
        // Isolation are multi-source by definition.
        if let Some(count) = features.active_source_count {
            let min_required = self.min_active_sources();
            if count < min_required {
                let deficit = (min_required - count) as f64;
                let frac = (deficit / min_required.max(1) as f64).min(1.0);
                violation += MIN_SOURCES_CAP * frac;
            }
        }

        violation
    }

    /// §28b — minimum number of effectively-active sources expected for
    /// each goal. Below this count, the source-equity constraint is
    /// trivially satisfied (mathematically 0 dB range with 1 source) but
    /// the cocoon-design intent is broken.
    fn min_active_sources(&self) -> u32 {
        match self.kind {
            // Cocoon goals — multi-source spatial design is the whole point.
            GoalKind::Shield | GoalKind::Isolation => 3,
            // Relaxation / flow — typically 2+ sources for envelope diversity
            // and habituation; not strictly required, but expected.
            GoalKind::Sleep | GoalKind::DeepRelaxation | GoalKind::Meditation | GoalKind::Flow => 2,
            // Active-attention goals — a single carefully-tuned source is
            // a valid product choice (e.g., focused beat-binding).
            GoalKind::Focus | GoalKind::DeepWork | GoalKind::Ignition => 1,
        }
    }

    /// §28b — goal-aware threshold for the per-source dB range.
    ///
    /// **Calibrated 2026-05-01** against the curated `presets/` set.
    /// The original 6 / 8 dB values came from the user-facing
    /// `feedback_balanced_cocoon` rule ("active sources within ~6 dB"),
    /// but empirical p90 of curated presets was 14 dB (Shield), 16 dB
    /// (Flow), 13 dB (Isolation), 7 dB (DeepWork), 15 dB (Ignition).
    /// The big gap between the stated rule and the practiced behaviour
    /// is partly because the volume-only proxy here doesn't account
    /// for color / tint / reverb / spread (which partially equalise
    /// perceptual loudness across sources at different volumes).
    /// Loosened to 12 / 15 dB to keep ~90% of curated presets feasible.
    fn source_balance_threshold_db(&self) -> f64 {
        match self.kind {
            GoalKind::Focus | GoalKind::DeepWork | GoalKind::Ignition => 15.0,
            _ => 12.0,
        }
    }

    /// LUFS asymmetry tolerance in LU. Balanced goals require tighter
    /// binaural symmetry than goals where lateralisation is acceptable.
    ///
    /// **Calibrated 2026-05-01** from the empirical p90 of 50 curated
    /// presets (`calibrate-comfort` over `presets/`). Pre-calibration
    /// values were 1 LU / 2 LU; the curated p90 was 2.94 LU for Shield
    /// (the strictest cocoon goal) and ≤ 1 LU for the active-attention
    /// goals. Rounded up to 3 / 4 to leave headroom around p90.
    fn lufs_asymmetry_threshold_lu(&self) -> f64 {
        match self.kind {
            GoalKind::Focus | GoalKind::DeepWork | GoalKind::Ignition => 4.0,
            _ => 3.0,
        }
    }

    /// Goal-specific preferred spectral slope (dB/oct).
    /// Sleep family: brown-leaning (−6 dB/oct); flow family: pink (−3);
    /// active/attention family: between pink and white (−1.5).
    fn spectral_tilt_target_db_per_oct(&self) -> f64 {
        match self.kind {
            GoalKind::Sleep | GoalKind::DeepRelaxation | GoalKind::Meditation => -6.0,
            GoalKind::Flow | GoalKind::DeepWork | GoalKind::Shield => -3.0,
            GoalKind::Focus | GoalKind::Isolation | GoalKind::Ignition => -1.5,
        }
    }

    /// HF-fraction (energy >8 kHz / energy 20 Hz–20 kHz) guardrail.
    /// Tighter cap on relax/sleep goals where HF content is the dominant
    /// fatigue driver in long-exposure literature.
    fn hf_fraction_threshold(&self) -> f64 {
        match self.kind {
            GoalKind::Sleep | GoalKind::DeepRelaxation | GoalKind::Meditation => 0.10,
            _ => 0.20,
        }
    }
}

// ── Per-term violation caps (Priority 28 Phase 2) ────────────────────────
//
// Each cap controls how much a single comfort dimension can contribute to
// the aggregated violation. The sum of caps bounds the maximum possible
// violation at ≈ 0.65 (LUFS_ASYM + TRUE_PEAK + SPECTRAL_TILT + HF + PLR =
// 0.20 + 0.10 + 0.15 + 0.10 + 0.10), which sets the natural scale for the
// ε schedule. These constants are tunable; Phase 2b (main.rs wiring) will
// surface them through CLI/config so empirical tuning is possible without
// changing scoring.rs.

const LUFS_ASYM_CAP: f64 = 0.20;
const TRUE_PEAK_CAP: f64 = 0.10;
const SPECTRAL_TILT_CAP: f64 = 0.15;
const HF_FRACTION_CAP: f64 = 0.10;
const PLR_CAP: f64 = 0.10;
const SOURCE_BALANCE_CAP: f64 = 0.15;
const MIN_SOURCES_CAP: f64 = 0.20;

const TRUE_PEAK_THRESHOLD_DBFS: f64 = -1.0;
const TRUE_PEAK_CAP_DBFS: f64 = 2.0;
// Calibrated 2026-05-01 from the empirical p90 of 50 curated presets
// (see `calibrate-comfort` subcommand). Pre-calibration values were
// 1.5 / 4.0 dB/oct (tilt) and 12 / 18 dB (PLR); the curated p90s
// were 5 dB/oct and 14–18 dB respectively. New values leave headroom
// around p90 so most hand-tuned presets stay inside the feasible region.
const SPECTRAL_TILT_TOLERANCE_DB: f64 = 5.0;
const SPECTRAL_TILT_CAP_DB: f64 = 8.0;
const PLR_THRESHOLD_DB: f64 = 16.0;
const PLR_CAP_DB: f64 = 22.0;

/// Linear violation ramp: 0 below `threshold`, scales linearly to
/// `max_penalty` at `cap_at`, clamped to `max_penalty` for values
/// beyond `cap_at`. Returns `max_penalty` when the input is non-finite.
fn linear_violation(value: f64, threshold: f64, cap_at: f64, max_penalty: f64) -> f64 {
    if !value.is_finite() {
        return max_penalty;
    }
    if value <= threshold {
        return 0.0;
    }
    let span = (cap_at - threshold).max(1e-12);
    let frac = ((value - threshold) / span).clamp(0.0, 1.0);
    max_penalty * frac
}

impl Goal {
    /// CET 13c — How much this goal values envelope-phase tracking (slow
    /// 2–9 Hz cortical entrainment to the auditory envelope).
    ///
    /// Inverted from `entrainment_weight()`: relaxation goals benefit from
    /// envelope tracking (slow natural rhythms), beta/gamma carrier-driven
    /// goals don't (their reward channel is already saturated by carrier PLV).
    fn envelope_entrainment_weight(&self) -> f64 {
        match self.kind {
            // Strong CET goals — slow natural rhythms are the whole point
            GoalKind::Sleep => 0.8,
            GoalKind::DeepRelaxation => 0.7,
            GoalKind::Meditation => 0.6,
            // Mixed — some envelope tracking benefit
            GoalKind::Flow => 0.4,
            GoalKind::DeepWork => 0.2,
            // Carrier-driven goals — envelope tracking is irrelevant
            GoalKind::Focus => 0.0,
            GoalKind::Isolation => 0.0,
            GoalKind::Shield => 0.0,
            GoalKind::Ignition => 0.0,
        }
    }

    /// How much this goal values entrainment (phase-locking to stimulus).
    fn entrainment_weight(&self) -> f64 {
        match self.kind {
            // Active entrainment goals: benefit strongly from PLV
            GoalKind::Focus => 1.0,
            GoalKind::Isolation => 0.8,
            GoalKind::DeepWork => 0.6,
            // Mild benefit
            GoalKind::Meditation => 0.3,
            // Natural rhythm goals: don't benefit from external entrainment
            GoalKind::DeepRelaxation => 0.0,
            GoalKind::Sleep => 0.0,
            // Shield: moderate entrainment benefit
            GoalKind::Shield => 0.7,
            // Flow: mild entrainment (natural rhythm more important)
            GoalKind::Flow => 0.3,
            // Ignition: strong entrainment (gamma binding)
            GoalKind::Ignition => 1.0,
        }
    }

    /// Compute asymmetry penalty [0, max_penalty] for this goal.
    /// Returns 0.0 for goals that don't care about asymmetry.
    fn asymmetry_penalty(&self, alpha_asymmetry: f64) -> f64 {
        let abs_asym = alpha_asymmetry.abs();

        // Per-goal asymmetry tolerance and max penalty
        let (threshold, max_penalty) = match self.kind {
            // Meditation/relaxation: want balanced hemispheres
            GoalKind::Meditation => (0.2, 0.15),
            GoalKind::DeepRelaxation => (0.3, 0.12),
            // Isolation: neutral masking, moderate balance preferred
            GoalKind::Isolation => (0.4, 0.08),
            // Focus/DeepWork: allow task-oriented lateralization
            GoalKind::Focus => (0.5, 0.05),
            GoalKind::DeepWork => (0.5, 0.05),
            // Sleep: asymmetry irrelevant
            GoalKind::Sleep => (1.0, 0.0),
            // Shield: moderate tolerance (focused masking)
            GoalKind::Shield => (0.4, 0.08),
            // Flow: want balanced (relaxed state)
            GoalKind::Flow => (0.3, 0.12),
            // Ignition: allow lateralization (ADHD activation)
            GoalKind::Ignition => (0.6, 0.03),
        };

        if abs_asym <= threshold {
            0.0
        } else {
            // Linear ramp from 0 at threshold to max_penalty at |asymmetry|=1.0
            let excess = (abs_asym - threshold) / (1.0 - threshold);
            max_penalty * excess.min(1.0)
        }
    }

    /// Compute a [0, 1] score modifier based on spectral brightness for this goal.
    fn brightness_modifier(&self, brightness: f64) -> f64 {
        match self.kind {
            GoalKind::Isolation => {
                // White noise masks more frequencies — linear increase with brightness
                (0.3 + 0.7 * brightness).clamp(0.0, 1.0)
            }
            GoalKind::Sleep => {
                // Dark sounds promote sleep onset; bright sounds are arousing
                (1.0 - 0.8 * brightness).clamp(0.0, 1.0)
            }
            GoalKind::Focus => {
                // Inverted-U: moderate-to-bright is optimal (pink/white noise)
                // Peak at brightness=0.55, falls off toward dark
                let x = (brightness - 0.55).abs();
                (1.0 - 1.8 * x).clamp(0.0, 1.0)
            }
            GoalKind::DeepRelaxation => {
                // Lower brightness preferred — natural 1/f spectra
                (0.9 - 0.6 * brightness).clamp(0.0, 1.0)
            }
            GoalKind::Meditation => {
                // Low-to-moderate brightness — natural sounds, not harsh white
                (0.85 - 0.5 * brightness).clamp(0.0, 1.0)
            }
            GoalKind::DeepWork => {
                let x = (brightness - 0.35).abs();
                (1.0 - 1.5 * x).clamp(0.0, 1.0)
            }
            GoalKind::Shield => (0.3 + 0.7 * brightness).clamp(0.0, 1.0),
            GoalKind::Flow => {
                let x = (brightness - 0.45).abs();
                (1.0 - 1.5 * x).clamp(0.0, 1.0)
            }
            GoalKind::Ignition => (0.3 + 0.7 * brightness).clamp(0.0, 1.0),
        }
    }

    fn acoustic_fusion_weights(&self) -> Option<(f64, f64)> {
        match self.kind {
            GoalKind::Shield => Some((0.82, 0.18)),
            GoalKind::Isolation => Some((0.78, 0.22)),
            _ => None,
        }
    }

    fn acoustic_goal_score(&self, acoustic: &AcousticScoreResult) -> Option<f64> {
        let speech_privacy = acoustic.speech_privacy?;
        let speech_band_ratio = acoustic.features.speech_band_ratio?;
        let comfort_score = self.acoustic_comfort_score(acoustic)?;

        let score = match self.kind {
            GoalKind::Shield => {
                0.60 * speech_privacy + 0.20 * speech_band_ratio + 0.20 * comfort_score
            }
            GoalKind::Isolation => {
                0.65 * speech_privacy + 0.15 * speech_band_ratio + 0.20 * comfort_score
            }
            _ => return None,
        };

        Some(score.clamp(0.0, 1.0))
    }

    fn acoustic_comfort_score(&self, acoustic: &AcousticScoreResult) -> Option<f64> {
        let sharpness = acoustic.features.sharpness_proxy?;
        let modulation_depth = acoustic.features.modulation_depth?;
        Some((1.0 - (0.75 * sharpness + 0.25 * modulation_depth)).clamp(0.0, 1.0))
    }

    /// Score EEG band powers against targets using Gaussian scoring.
    fn score_bands(&self, powers: &BandPowers) -> f64 {
        let norm = powers.normalized();
        let t = &self.band_targets;

        // For isolation, use a flat-deviation scoring instead
        if self.kind == GoalKind::Isolation {
            let uniform = 0.2;
            let flatness = 1.0
                - ((norm.delta - uniform).abs()
                    + (norm.theta - uniform).abs()
                    + (norm.alpha - uniform).abs()
                    + (norm.beta - uniform).abs()
                    + (norm.gamma - uniform).abs())
                    / 2.0;
            return flatness.clamp(0.0, 1.0);
        }

        // Triangular score per band, simple average
        let scores = [
            t.delta.score(norm.delta),
            t.theta.score(norm.theta),
            t.alpha.score(norm.alpha),
            t.beta.score(norm.beta),
            t.gamma.score(norm.gamma),
        ];

        scores.iter().sum::<f64>() / scores.len() as f64
    }

    /// Produce a detailed diagnostic breakdown of how a result matches this goal.
    pub fn diagnose(
        &self,
        fhn: &FhnResult,
        jansen_rit: &JansenRitResult,
        brightness: f64,
        alpha_asymmetry: f64,
        plv: Option<f64>,
        envelope_plv: Option<f64>,
        performance: Option<PerformanceVector>,
    ) -> Diagnosis {
        let norm = jansen_rit.band_powers.normalized();
        let t = &self.band_targets;

        let band_diagnoses = if self.kind == GoalKind::Isolation {
            let uniform = 0.2;
            vec![
                BandDiagnosis {
                    name: "Delta",
                    actual: norm.delta,
                    expectation: BandExpectation::Flat(uniform),
                    status: flat_status(norm.delta, uniform),
                },
                BandDiagnosis {
                    name: "Theta",
                    actual: norm.theta,
                    expectation: BandExpectation::Flat(uniform),
                    status: flat_status(norm.theta, uniform),
                },
                BandDiagnosis {
                    name: "Alpha",
                    actual: norm.alpha,
                    expectation: BandExpectation::Flat(uniform),
                    status: flat_status(norm.alpha, uniform),
                },
                BandDiagnosis {
                    name: "Beta",
                    actual: norm.beta,
                    expectation: BandExpectation::Flat(uniform),
                    status: flat_status(norm.beta, uniform),
                },
                BandDiagnosis {
                    name: "Gamma",
                    actual: norm.gamma,
                    expectation: BandExpectation::Flat(uniform),
                    status: flat_status(norm.gamma, uniform),
                },
            ]
        } else {
            vec![
                BandDiagnosis {
                    name: "Delta",
                    actual: norm.delta,
                    expectation: t.delta.expectation(),
                    status: t.delta.status(norm.delta),
                },
                BandDiagnosis {
                    name: "Theta",
                    actual: norm.theta,
                    expectation: t.theta.expectation(),
                    status: t.theta.status(norm.theta),
                },
                BandDiagnosis {
                    name: "Alpha",
                    actual: norm.alpha,
                    expectation: t.alpha.expectation(),
                    status: t.alpha.status(norm.alpha),
                },
                BandDiagnosis {
                    name: "Beta",
                    actual: norm.beta,
                    expectation: t.beta.expectation(),
                    status: t.beta.status(norm.beta),
                },
                BandDiagnosis {
                    name: "Gamma",
                    actual: norm.gamma,
                    expectation: t.gamma.expectation(),
                    status: t.gamma.status(norm.gamma),
                },
            ]
        };

        let (min_rate, max_rate) = self.fhn_targets.firing_rate_range;
        let firing_rate_status = if fhn.firing_rate >= min_rate && fhn.firing_rate <= max_rate {
            MetricStatus::Pass
        } else if (fhn.firing_rate - min_rate).abs() < 2.0
            || (fhn.firing_rate - max_rate).abs() < 2.0
        {
            MetricStatus::Warn
        } else {
            MetricStatus::Fail
        };

        let isi_status = if let Some(target_cv) = self.fhn_targets.target_isi_cv {
            if fhn.isi_cv.is_nan() {
                MetricStatus::Fail // insufficient spikes for ISI analysis
            } else {
                let diff = (fhn.isi_cv - target_cv).abs();
                if diff < 0.08 {
                    MetricStatus::Pass
                } else if diff < 0.18 {
                    MetricStatus::Warn
                } else {
                    MetricStatus::Fail
                }
            }
        } else {
            MetricStatus::Pass
        };

        let score = self.evaluate_full(fhn, jansen_rit, alpha_asymmetry, plv, envelope_plv);

        let verdict = if score >= 0.75 {
            Verdict::Good
        } else if score >= 0.50 {
            Verdict::Ok
        } else {
            Verdict::Poor
        };

        Diagnosis {
            score,
            bands: band_diagnoses,
            firing_rate: fhn.firing_rate,
            firing_rate_range: (min_rate, max_rate),
            firing_rate_status,
            isi_cv: fhn.isi_cv,
            target_isi_cv: self.fhn_targets.target_isi_cv,
            isi_status,
            dominant_freq: jansen_rit.dominant_freq,
            verdict,
            performance,
        }
    }

    /// Score FHN firing characteristics.
    fn score_fhn(&self, fhn: &FhnResult) -> f64 {
        let targets = &self.fhn_targets;
        let mut score = 0.0;
        let mut components = 0.0;

        // Firing rate: 1.0 inside range, exponential decay outside
        let (min_rate, max_rate) = targets.firing_rate_range;
        let rate_score = if fhn.firing_rate >= min_rate && fhn.firing_rate <= max_rate {
            1.0
        } else if fhn.firing_rate < min_rate {
            (-2.0 * (min_rate - fhn.firing_rate) / min_rate.max(1.0)).exp()
        } else {
            (-2.0 * (fhn.firing_rate - max_rate) / max_rate.max(1.0)).exp()
        };
        score += rate_score;
        components += 1.0;

        // ISI regularity (skip if ISI CV is NaN — insufficient spikes)
        if let Some(target_cv) = targets.target_isi_cv {
            if fhn.isi_cv.is_nan() {
                // No meaningful ISI data → no credit for this component
                components += 1.0;
            } else {
                let cv_diff = (fhn.isi_cv - target_cv).abs();
                let cv_score = (-4.0 * cv_diff).exp();
                score += cv_score;
                components += 1.0;
            }
        }

        if components > 0.0 {
            score / components
        } else {
            0.5
        }
    }
}

fn flat_status(actual: f64, target: f64) -> MetricStatus {
    let diff = (actual - target).abs();
    if diff < 0.05 {
        MetricStatus::Pass
    } else if diff < 0.10 {
        MetricStatus::Warn
    } else {
        MetricStatus::Fail
    }
}

// ── Diagnosis types ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
pub enum MetricStatus {
    Pass,
    Warn,
    Fail,
}

impl fmt::Display for MetricStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetricStatus::Pass => write!(f, "PASS"),
            MetricStatus::Warn => write!(f, "WARN"),
            MetricStatus::Fail => write!(f, "FAIL"),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BandExpectation {
    /// Range-based target: (min, ideal, max)
    Range(f64, f64, f64),
    /// Flat distribution target for isolation
    Flat(f64),
    // Legacy variants kept for main.rs pattern matching compatibility
    High,
    Low,
    Neutral,
}

impl fmt::Display for BandExpectation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BandExpectation::Range(min, _, max) => write!(f, "{:.2}–{:.2}", min, max),
            BandExpectation::Flat(t) => write!(f, "~{:.2}", t),
            BandExpectation::High => write!(f, "HIGH"),
            BandExpectation::Low => write!(f, "LOW"),
            BandExpectation::Neutral => write!(f, "---"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BandDiagnosis {
    pub name: &'static str,
    pub actual: f64,
    pub expectation: BandExpectation,
    pub status: MetricStatus,
}

#[derive(Debug, Clone, Copy)]
pub enum Verdict {
    Good,
    Ok,
    Poor,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Good => write!(f, "GOOD"),
            Verdict::Ok => write!(f, "OK"),
            Verdict::Poor => write!(f, "POOR"),
        }
    }
}

pub struct Diagnosis {
    pub score: f64,
    pub bands: Vec<BandDiagnosis>,
    pub firing_rate: f64,
    pub firing_rate_range: (f64, f64),
    pub firing_rate_status: MetricStatus,
    pub isi_cv: f64,
    pub target_isi_cv: Option<f64>,
    pub isi_status: MetricStatus,
    pub dominant_freq: f64,
    pub verdict: Verdict,
    pub performance: Option<PerformanceVector>,
}

impl Diagnosis {
    /// Which EEG band the dominant frequency falls into.
    pub fn dominant_band_name(&self) -> &'static str {
        let f = self.dominant_freq;
        if f < 4.0 {
            "Delta"
        } else if f < 8.0 {
            "Theta"
        } else if f < 13.0 {
            "Alpha"
        } else if f < 30.0 {
            "Beta"
        } else {
            "Gamma"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::acoustic_score::{AcousticFeatureVector, AcousticScoreResult};
    use crate::neural::{BandPowers, FhnResult, JansenRitResult};

    /// Build a synthetic FhnResult with given firing_rate and isi_cv.
    fn make_fhn(firing_rate: f64, isi_cv: f64) -> FhnResult {
        FhnResult {
            voltage: vec![],
            recovery: vec![],
            spike_times: vec![],
            firing_rate,
            isi_cv,
            mean_voltage: 0.0,
            voltage_variance: 0.0,
        }
    }

    /// Build a synthetic JansenRitResult with given band powers.
    fn make_jr(delta: f64, theta: f64, alpha: f64, beta: f64, gamma: f64) -> JansenRitResult {
        JansenRitResult {
            eeg: vec![0.0; 100],
            band_powers: BandPowers {
                delta,
                theta,
                alpha,
                beta,
                gamma,
            },
            dominant_freq: 10.0,
            fast_inhib_trace: vec![],
        }
    }

    fn make_acoustic_result(
        speech_privacy: f64,
        speech_band_ratio: f64,
        modulation_depth: f64,
        sharpness_proxy: f64,
    ) -> AcousticScoreResult {
        AcousticScoreResult {
            features: AcousticFeatureVector {
                broadband_level_db: Some(-18.0),
                speech_band_ratio: Some(speech_band_ratio),
                modulation_depth: Some(modulation_depth),
                sharpness_proxy: Some(sharpness_proxy),
                ..AcousticFeatureVector::default()
            },
            intelligibility_proxy: Some(1.0 - speech_privacy),
            speech_privacy: Some(speech_privacy),
            ..AcousticScoreResult::default()
        }
    }

    // ---------------------------------------------------------------
    // BandTarget::score — Gaussian formula
    // ---------------------------------------------------------------

    #[test]
    fn band_score_at_ideal_is_one() {
        let t = BandTarget {
            min: 0.10,
            ideal: 0.30,
            max: 0.50,
        };
        let s = t.score(0.30);
        assert!(
            (s - 1.0).abs() < 1e-10,
            "Score at ideal should be 1.0, got {s}"
        );
    }

    #[test]
    fn band_score_at_boundaries_near_005() {
        // Centered ideal: boundaries should give ≈ 0.05
        let t = BandTarget {
            min: 0.10,
            ideal: 0.30,
            max: 0.50,
        };
        let s_min = t.score(0.10);
        let s_max = t.score(0.50);
        assert!(
            (s_min - 0.05).abs() < 0.01,
            "Score at min should be ~0.05, got {s_min:.4}"
        );
        assert!(
            (s_max - 0.05).abs() < 0.01,
            "Score at max should be ~0.05, got {s_max:.4}"
        );
    }

    #[test]
    fn band_score_symmetric_around_ideal() {
        let t = BandTarget {
            min: 0.10,
            ideal: 0.30,
            max: 0.50,
        };
        let above = t.score(0.35);
        let below = t.score(0.25);
        assert!(
            (above - below).abs() < 1e-10,
            "Gaussian should be symmetric: above={above}, below={below}"
        );
    }

    #[test]
    fn band_score_decreases_away_from_ideal() {
        let t = BandTarget {
            min: 0.10,
            ideal: 0.30,
            max: 0.50,
        };
        let close = t.score(0.28);
        let far = t.score(0.15);
        assert!(
            close > far,
            "Closer to ideal should score higher: {close} vs {far}"
        );
    }

    #[test]
    fn band_score_beyond_boundaries_near_zero() {
        let t = BandTarget {
            min: 0.10,
            ideal: 0.30,
            max: 0.50,
        };
        let beyond = t.score(0.70); // well beyond max
        assert!(
            beyond < 0.01,
            "Score well beyond boundary should be ~0, got {beyond}"
        );
    }

    #[test]
    fn band_score_zero_half_width_returns_zero() {
        let t = BandTarget {
            min: 0.30,
            ideal: 0.30,
            max: 0.30,
        };
        let s = t.score(0.30);
        assert_eq!(s, 0.0, "Zero-width target should return 0.0");
    }

    // ---------------------------------------------------------------
    // Weight balance
    // ---------------------------------------------------------------

    #[test]
    fn all_goals_weights_sum_to_one() {
        for &kind in GoalKind::all() {
            let goal = Goal::new(kind);
            let sum = goal.band_weight + goal.fhn_targets.weight;
            assert!(
                (sum - 1.0).abs() < 1e-10,
                "{kind}: band_weight + fhn_weight = {sum} (expected 1.0)"
            );
        }
    }

    // ---------------------------------------------------------------
    // Ideal values sum close to 1.0 for normalized band scoring
    // ---------------------------------------------------------------

    #[test]
    fn all_goals_ideal_sum_near_one() {
        for &kind in GoalKind::all() {
            let goal = Goal::new(kind);
            let t = &goal.band_targets;
            let sum = t.delta.ideal + t.theta.ideal + t.alpha.ideal + t.beta.ideal + t.gamma.ideal;
            assert!(
                sum >= 0.90 && sum <= 1.10,
                "{kind}: ideal sum = {sum:.3} (expected 0.90–1.10 for achievable max score)"
            );
        }
    }

    // ---------------------------------------------------------------
    // Score range [0, 1]
    // ---------------------------------------------------------------

    #[test]
    fn score_in_valid_range_for_all_goals() {
        let fhn = make_fhn(5.0, 0.35);
        let jr = make_jr(0.2, 0.2, 0.2, 0.2, 0.2); // flat

        for &kind in GoalKind::all() {
            let goal = Goal::new(kind);
            let score = goal.evaluate_with_brightness(&fhn, &jr, 0.5);
            assert!(
                score >= 0.0 && score <= 1.0,
                "{kind}: score = {score} out of [0, 1]"
            );
        }
    }

    #[test]
    fn score_in_range_with_extreme_inputs() {
        // Zero band powers
        let fhn_zero = make_fhn(0.0, f64::NAN);
        let jr_zero = make_jr(0.0, 0.0, 0.0, 0.0, 0.0);

        // Very high firing rate
        let fhn_high = make_fhn(100.0, 0.01);
        let jr_high = make_jr(1.0, 0.0, 0.0, 0.0, 0.0);

        for &kind in GoalKind::all() {
            let goal = Goal::new(kind);

            let s1 = goal.evaluate_with_brightness(&fhn_zero, &jr_zero, 0.0);
            assert!(s1 >= 0.0 && s1 <= 1.0, "{kind} zero: {s1}");

            let s2 = goal.evaluate_with_brightness(&fhn_high, &jr_high, 1.0);
            assert!(s2 >= 0.0 && s2 <= 1.0, "{kind} extreme: {s2}");
        }
    }

    #[test]
    fn shield_acoustic_fusion_prefers_private_comfortable_masking() {
        let goal = Goal::new(GoalKind::Shield);
        let weak = make_acoustic_result(0.20, 0.10, 0.80, 0.90);
        let strong = make_acoustic_result(0.85, 0.75, 0.15, 0.20);

        let weak_fused = goal
            .evaluate_with_acoustic_fusion(0.42, &weak)
            .expect("shield should support acoustic fusion");
        let strong_fused = goal
            .evaluate_with_acoustic_fusion(0.42, &strong)
            .expect("shield should support acoustic fusion");

        assert!(strong_fused.acoustic_goal_score > weak_fused.acoustic_goal_score);
        assert!(strong_fused.comfort_score > weak_fused.comfort_score);
        assert!(strong_fused.fused_score > weak_fused.fused_score);
        assert!((0.0..=1.0).contains(&strong_fused.fused_score));
    }

    #[test]
    fn non_shield_goals_do_not_apply_acoustic_fusion_yet() {
        let goal = Goal::new(GoalKind::Focus);
        let acoustic = make_acoustic_result(0.80, 0.60, 0.20, 0.20);

        assert!(!goal.supports_acoustic_fusion());
        assert!(goal
            .evaluate_with_acoustic_fusion(0.50, &acoustic)
            .is_none());
    }

    // ---------------------------------------------------------------
    // Perfect band powers → high score
    // ---------------------------------------------------------------

    #[test]
    fn ideal_band_powers_score_high() {
        // Use Focus ideals: δ=0.01, θ=0.18, α=0.33, β=0.42, γ=0.06
        let fhn = make_fhn(12.0, 0.30); // within Focus FHN range
        let jr = make_jr(0.01, 0.18, 0.33, 0.42, 0.06); // Focus ideals

        let goal = Goal::new(GoalKind::Focus);
        let score = goal.evaluate_with_brightness(&fhn, &jr, 0.55); // optimal brightness

        assert!(
            score > 0.80,
            "Focus with ideal bands + FHN should score > 0.80, got {score:.3}"
        );
    }

    // ---------------------------------------------------------------
    // Isolation: flat spectrum scores high
    // ---------------------------------------------------------------

    #[test]
    fn isolation_flat_spectrum_scores_high() {
        let fhn = make_fhn(5.0, 0.35);
        let jr = make_jr(0.2, 0.2, 0.2, 0.2, 0.2); // perfectly flat

        let goal = Goal::new(GoalKind::Isolation);
        let score = goal.evaluate_with_brightness(&fhn, &jr, 0.8);

        assert!(
            score > 0.70,
            "Isolation with flat spectrum should score > 0.70, got {score:.3}"
        );
    }

    #[test]
    fn isolation_concentrated_spectrum_scores_lower() {
        let fhn = make_fhn(5.0, 0.35);
        let jr_flat = make_jr(0.2, 0.2, 0.2, 0.2, 0.2);
        let jr_concentrated = make_jr(1.0, 0.0, 0.0, 0.0, 0.0);

        let goal = Goal::new(GoalKind::Isolation);
        let flat_score = goal.evaluate_with_brightness(&fhn, &jr_flat, 0.5);
        let conc_score = goal.evaluate_with_brightness(&fhn, &jr_concentrated, 0.5);

        assert!(
            flat_score > conc_score,
            "Flat spectrum ({flat_score:.3}) should beat concentrated ({conc_score:.3})"
        );
    }

    // ---------------------------------------------------------------
    // Brightness modifier
    // ---------------------------------------------------------------

    #[test]
    fn sleep_prefers_dark_sounds() {
        let goal = Goal::new(GoalKind::Sleep);
        let dark = goal.brightness_modifier(0.1);
        let bright = goal.brightness_modifier(0.9);
        assert!(
            dark > bright,
            "Sleep should prefer dark: {dark:.3} vs {bright:.3}"
        );
    }

    #[test]
    fn isolation_prefers_bright_sounds() {
        let goal = Goal::new(GoalKind::Isolation);
        let dark = goal.brightness_modifier(0.1);
        let bright = goal.brightness_modifier(0.9);
        assert!(
            bright > dark,
            "Isolation should prefer bright: {bright:.3} vs {dark:.3}"
        );
    }

    #[test]
    fn brightness_modifier_in_zero_to_one() {
        for &kind in GoalKind::all() {
            let goal = Goal::new(kind);
            for &b in &[0.0, 0.25, 0.5, 0.75, 1.0] {
                let m = goal.brightness_modifier(b);
                assert!(
                    m >= 0.0 && m <= 1.0,
                    "{kind} brightness={b}: modifier = {m}"
                );
            }
        }
    }

    // ---------------------------------------------------------------
    // FHN scoring
    // ---------------------------------------------------------------

    #[test]
    fn fhn_in_range_scores_high() {
        let goal = Goal::new(GoalKind::Focus);
        // Focus: rate 8–20, CV 0.30
        let fhn = make_fhn(12.0, 0.30);
        let s = goal.score_fhn(&fhn);
        assert!(s > 0.9, "FHN in range should score > 0.9, got {s:.3}");
    }

    #[test]
    fn fhn_out_of_range_scores_lower() {
        let goal = Goal::new(GoalKind::Focus);
        let fhn_good = make_fhn(12.0, 0.30);
        let fhn_bad = make_fhn(0.5, 0.80); // way below range, wrong CV

        let good = goal.score_fhn(&fhn_good);
        let bad = goal.score_fhn(&fhn_bad);
        assert!(
            good > bad,
            "In-range FHN ({good:.3}) should beat out-of-range ({bad:.3})"
        );
    }

    #[test]
    fn fhn_nan_isi_cv_gives_zero_cv_credit() {
        let goal = Goal::new(GoalKind::Focus);
        let fhn_nan = make_fhn(12.0, f64::NAN); // good rate, NaN CV
        let fhn_good = make_fhn(12.0, 0.30); // good rate, good CV

        let s_nan = goal.score_fhn(&fhn_nan);
        let s_good = goal.score_fhn(&fhn_good);

        // NaN CV gives 0 credit for CV component → lower total
        assert!(
            s_good > s_nan,
            "Good CV ({s_good:.3}) should beat NaN CV ({s_nan:.3})"
        );
        // But rate component still scores well
        assert!(
            s_nan > 0.3,
            "NaN CV with good rate should still score > 0.3, got {s_nan:.3}"
        );
    }

    // ---------------------------------------------------------------
    // GoalKind utilities
    // ---------------------------------------------------------------

    #[test]
    fn goal_kind_all_returns_nine() {
        assert_eq!(GoalKind::all().len(), 9);
    }

    #[test]
    fn every_goal_has_semantics() {
        for &kind in GoalKind::all() {
            let semantics = kind.semantics();
            assert_eq!(semantics.goal, kind);
        }
    }

    #[test]
    fn semantics_required_fields_are_non_empty() {
        for &kind in GoalKind::all() {
            let s = kind.semantics();
            assert!(!s.plain_language_purpose.trim().is_empty());
            assert!(!s.product_objective.trim().is_empty());
            assert!(!s.primary_neural_proxies.is_empty());
            assert!(!s.primary_acoustic_proxies.is_empty());
            assert!(!s.best_use_cases.is_empty());
            assert!(!s.unsupported_claims.is_empty());
        }
    }

    #[test]
    fn sleep_semantics_disclaims_slow_wave_and_memory() {
        let s = GoalKind::Sleep.semantics();
        let all_claims = s.unsupported_claims.join(" | ").to_lowercase();
        assert!(all_claims.contains("slow-wave"));
        assert!(all_claims.contains("memory"));
    }

    #[test]
    fn shield_and_isolation_separate_masking_from_cognitive_claims() {
        let shield = GoalKind::Shield.semantics();
        let isolation = GoalKind::Isolation.semantics();
        assert!(shield.product_objective.to_lowercase().contains("mask"));
        assert!(isolation.product_objective.to_lowercase().contains("privacy"));
        assert!(shield
            .unsupported_claims
            .join(" | ")
            .to_lowercase()
            .contains("focus improvement"));
        assert!(isolation
            .unsupported_claims
            .join(" | ")
            .to_lowercase()
            .contains("cognitive enhancement"));
    }

    #[test]
    fn goal_kind_from_str_round_trip() {
        for &kind in GoalKind::all() {
            let s = kind.to_string();
            let parsed = GoalKind::from_str(&s);
            assert_eq!(parsed, Some(kind), "Round-trip failed for {kind}");
        }
    }

    // ---------------------------------------------------------------
    // Diagnosis
    // ---------------------------------------------------------------

    #[test]
    fn diagnose_produces_five_bands() {
        let fhn = make_fhn(5.0, 0.35);
        let jr = make_jr(0.2, 0.2, 0.2, 0.2, 0.2);

        for &kind in GoalKind::all() {
            let goal = Goal::new(kind);
            let diag = goal.diagnose(&fhn, &jr, 0.5, 0.0, None, None, None);
            assert_eq!(diag.bands.len(), 5, "{kind} diagnosis should have 5 bands");
        }
    }

    #[test]
    fn diagnose_verdict_good_for_high_score() {
        let fhn = make_fhn(12.0, 0.30);
        let jr = make_jr(0.01, 0.18, 0.33, 0.42, 0.06); // Focus ideals

        let goal = Goal::new(GoalKind::Focus);
        let diag = goal.diagnose(&fhn, &jr, 0.55, 0.0, None, None, None);

        assert!(
            matches!(diag.verdict, Verdict::Good),
            "Focus with ideal inputs should get Good verdict, got {:?} (score={:.3})",
            diag.verdict,
            diag.score
        );
    }

    // ---------------------------------------------------------------
    // BandTarget status
    // ---------------------------------------------------------------

    #[test]
    fn band_status_pass_near_ideal() {
        let t = BandTarget {
            min: 0.10,
            ideal: 0.30,
            max: 0.50,
        };
        assert!(matches!(t.status(0.30), MetricStatus::Pass));
    }

    #[test]
    fn band_status_fail_outside_range() {
        let t = BandTarget {
            min: 0.10,
            ideal: 0.30,
            max: 0.50,
        };
        assert!(matches!(t.status(0.05), MetricStatus::Fail));
        assert!(matches!(t.status(0.60), MetricStatus::Fail));
    }

    // ───────────────────────────────────────────────────────────────────
    // Priority 28 Phase 2 — comfort_violation tests
    // ───────────────────────────────────────────────────────────────────

    /// Build an `AcousticFeatureVector` populated with values comfortably
    /// inside every comfort threshold for any goal.
    fn within_threshold_features() -> AcousticFeatureVector {
        AcousticFeatureVector {
            broadband_level_db: Some(-18.0),
            speech_band_ratio: Some(0.25),
            modulation_depth: Some(0.10),
            sharpness_proxy: Some(0.30),
            // Phase 1 comfort metrics — all well within tolerance
            lufs_integrated: Some(-23.0),
            lufs_left: Some(-23.0),
            lufs_right: Some(-23.0),
            lufs_asymmetry_lu: Some(0.2),
            true_peak_dbfs: Some(-3.0),
            plr_db: Some(8.0),
            spectral_tilt_db_per_oct: Some(-3.0),
            hf_fraction_above_8khz: Some(0.05),
            // §28b — within Shield's 12 dB threshold (post-2026-05-01 calibration)
            source_balance_db_range: Some(3.0),
            // §28b — at or above every goal's minimum (Shield/Iso need ≥ 3)
            active_source_count: Some(4),
        }
    }

    #[test]
    fn comfort_violation_zero_inside_thresholds_for_all_goals() {
        // For each goal, choose features that exactly hit the goal's
        // preferred tilt and stay inside the goal's tolerances.
        let baseline = within_threshold_features();
        for &kind in GoalKind::all() {
            let goal = Goal::new(kind);
            let mut features = baseline.clone();
            // Set tilt to this goal's target so the tilt-deviation term is 0.
            features.spectral_tilt_db_per_oct = Some(goal.spectral_tilt_target_db_per_oct());
            let v = goal.comfort_violation(&features);
            assert!(
                v < 1e-12,
                "{kind}: expected 0 violation inside thresholds, got {v:.6}"
            );
        }
    }

    #[test]
    fn comfort_violation_lufs_asymmetry_triggers_above_goal_threshold() {
        // After 2026-05-01 calibration: Sleep tolerates 3 LU; Focus
        // tolerates 4 LU. A 3.5 LU asymmetry triggers Sleep but not Focus.
        let mut f = within_threshold_features();
        f.lufs_asymmetry_lu = Some(3.5);
        let sleep = Goal::new(GoalKind::Sleep);
        let mut f_sleep = f.clone();
        f_sleep.spectral_tilt_db_per_oct = Some(sleep.spectral_tilt_target_db_per_oct());
        assert!(
            sleep.comfort_violation(&f_sleep) > 0.0,
            "Sleep should violate at 3.5 LU asymmetry (threshold 3 LU)"
        );

        let focus = Goal::new(GoalKind::Focus);
        let mut f_focus = f.clone();
        f_focus.spectral_tilt_db_per_oct = Some(focus.spectral_tilt_target_db_per_oct());
        assert!(
            focus.comfort_violation(&f_focus) < 1e-12,
            "Focus should not violate at 3.5 LU asymmetry (threshold 4 LU)"
        );
    }

    #[test]
    fn comfort_violation_lufs_asymmetry_capped() {
        // Asymmetry of 100 LU should saturate the term at LUFS_ASYM_CAP
        // (it must not blow up or escape the per-term bound).
        let mut f = within_threshold_features();
        f.lufs_asymmetry_lu = Some(100.0);
        let sleep = Goal::new(GoalKind::Sleep);
        f.spectral_tilt_db_per_oct = Some(sleep.spectral_tilt_target_db_per_oct());
        let v = sleep.comfort_violation(&f);
        assert!(
            (v - LUFS_ASYM_CAP).abs() < 1e-10,
            "asymmetry violation should saturate at {LUFS_ASYM_CAP}, got {v:.6}"
        );
    }

    #[test]
    fn comfort_violation_tilt_deviation_zero_at_target() {
        for &kind in GoalKind::all() {
            let goal = Goal::new(kind);
            let mut f = within_threshold_features();
            f.spectral_tilt_db_per_oct = Some(goal.spectral_tilt_target_db_per_oct());
            // All other metrics are inside threshold → total violation is 0.
            assert_eq!(
                goal.comfort_violation(&f),
                0.0,
                "tilt at target → no violation for {kind}"
            );
        }
    }

    #[test]
    fn comfort_violation_tilt_deviation_monotone() {
        // After 2026-05-01 calibration: tolerance 5 dB/oct, cap_at 8 dB/oct.
        let goal = Goal::new(GoalKind::Sleep); // target -6 dB/oct
        let mut close = within_threshold_features();
        close.spectral_tilt_db_per_oct = Some(-6.0 - SPECTRAL_TILT_TOLERANCE_DB); // at tolerance edge
        let mut far = within_threshold_features();
        far.spectral_tilt_db_per_oct = Some(-6.0 - SPECTRAL_TILT_CAP_DB); // at cap

        let v_close = goal.comfort_violation(&close);
        let v_far = goal.comfort_violation(&far);
        // Within tolerance band → 0 violation.
        assert!(v_close < 1e-12);
        // At cap → equals SPECTRAL_TILT_CAP (no other metric violates here).
        assert!(
            (v_far - SPECTRAL_TILT_CAP).abs() < 1e-10,
            "tilt at cap should saturate, got {v_far:.6}"
        );
    }

    #[test]
    fn comfort_violation_plr_skipped_for_ignition() {
        let mut f = within_threshold_features();
        f.plr_db = Some(20.0); // way above threshold
        let ignition = Goal::new(GoalKind::Ignition);
        f.spectral_tilt_db_per_oct = Some(ignition.spectral_tilt_target_db_per_oct());
        let v = ignition.comfort_violation(&f);
        assert!(
            v < 1e-12,
            "Ignition should ignore high PLR, got violation {v:.6}"
        );
        // Same PLR violates Focus (which expects steady masking).
        let focus = Goal::new(GoalKind::Focus);
        let mut f_focus = f.clone();
        f_focus.spectral_tilt_db_per_oct = Some(focus.spectral_tilt_target_db_per_oct());
        let v_focus = focus.comfort_violation(&f_focus);
        assert!(v_focus > 0.0, "Focus should violate at PLR=20 dB");
    }

    #[test]
    fn comfort_violation_hf_threshold_tighter_for_relax_goals() {
        // 0.15 HF fraction violates relax goals (threshold 0.10) but not
        // others (threshold 0.20).
        let mut f = within_threshold_features();
        f.hf_fraction_above_8khz = Some(0.15);
        for &kind in &[
            GoalKind::Sleep,
            GoalKind::DeepRelaxation,
            GoalKind::Meditation,
        ] {
            let g = Goal::new(kind);
            let mut ff = f.clone();
            ff.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
            assert!(
                g.comfort_violation(&ff) > 0.0,
                "{kind} should violate at HF=0.15"
            );
        }
        for &kind in &[
            GoalKind::Focus,
            GoalKind::DeepWork,
            GoalKind::Flow,
            GoalKind::Shield,
        ] {
            let g = Goal::new(kind);
            let mut ff = f.clone();
            ff.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
            assert!(
                g.comfort_violation(&ff) < 1e-12,
                "{kind} should not violate at HF=0.15"
            );
        }
    }

    #[test]
    fn comfort_violation_aggregate_bounded_by_sum_of_caps() {
        // Worst-case input: every metric saturates its term. Total must
        // never exceed the sum of caps.
        let f = AcousticFeatureVector {
            broadband_level_db: Some(0.0),
            speech_band_ratio: Some(0.5),
            modulation_depth: Some(0.5),
            sharpness_proxy: Some(0.9),
            lufs_integrated: Some(-23.0),
            lufs_left: Some(-20.0),
            lufs_right: Some(-50.0), // huge asymmetry
            lufs_asymmetry_lu: Some(30.0),
            true_peak_dbfs: Some(10.0),           // far above ceiling
            plr_db: Some(50.0),                   // way above cap
            spectral_tilt_db_per_oct: Some(10.0), // far from any target
            hf_fraction_above_8khz: Some(1.0),    // saturated
            source_balance_db_range: Some(40.0),  // way past any goal threshold
            active_source_count: Some(0),         // zero sources → max min-source penalty
        };
        let max_total = LUFS_ASYM_CAP
            + TRUE_PEAK_CAP
            + SPECTRAL_TILT_CAP
            + HF_FRACTION_CAP
            + PLR_CAP
            + SOURCE_BALANCE_CAP
            + MIN_SOURCES_CAP;
        for &kind in GoalKind::all() {
            let g = Goal::new(kind);
            let v = g.comfort_violation(&f);
            assert!(v.is_finite(), "{kind}: violation must be finite, got {v}");
            assert!(v >= 0.0, "{kind}: violation must be ≥ 0, got {v}");
            assert!(
                v <= max_total + 1e-10,
                "{kind}: violation {v:.4} should be ≤ {max_total:.4}"
            );
        }
    }

    #[test]
    fn comfort_violation_missing_metrics_contribute_zero() {
        // All Option fields = None → violation must be 0.0 regardless of goal.
        let f = AcousticFeatureVector::default();
        for &kind in GoalKind::all() {
            assert_eq!(
                Goal::new(kind).comfort_violation(&f),
                0.0,
                "missing metrics should produce 0 violation for {kind}"
            );
        }
    }

    #[test]
    fn comfort_violation_nan_input_is_capped_not_propagated() {
        let mut f = within_threshold_features();
        f.lufs_asymmetry_lu = Some(f64::NAN);
        let g = Goal::new(GoalKind::Sleep);
        f.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
        let v = g.comfort_violation(&f);
        assert!(
            v.is_finite(),
            "violation must remain finite under NaN input"
        );
        // NaN is treated as worst-case → asymmetry term saturates.
        assert!(v >= LUFS_ASYM_CAP - 1e-10, "NaN should saturate the term");
    }

    // ── §28b — per-source loudness equity ──────────────────────────

    #[test]
    fn comfort_violation_source_balance_zero_at_threshold() {
        let mut f = within_threshold_features();
        // Shield threshold = 6 dB. Exactly at threshold → 0 violation.
        f.source_balance_db_range = Some(6.0);
        let g = Goal::new(GoalKind::Shield);
        f.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
        assert_eq!(g.comfort_violation(&f), 0.0);
    }

    #[test]
    fn comfort_violation_source_balance_triggers_above_threshold_for_shield() {
        // After 2026-05-01 calibration: Shield threshold 12 dB, cap_at 24 dB.
        // 18 dB range → halfway between threshold and cap → ~half cap.
        let mut f = within_threshold_features();
        f.source_balance_db_range = Some(18.0);
        let g = Goal::new(GoalKind::Shield);
        f.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
        let v = g.comfort_violation(&f);
        assert!(
            (v - 0.5 * SOURCE_BALANCE_CAP).abs() < 1e-10,
            "expected ~½ cap at 18 dB (Shield threshold 12, cap 24), got {v:.6}"
        );
    }

    #[test]
    fn comfort_violation_source_balance_saturates_at_cap() {
        let mut f = within_threshold_features();
        f.source_balance_db_range = Some(50.0); // way past cap_at = 12 dB
        let g = Goal::new(GoalKind::Shield);
        f.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
        let v = g.comfort_violation(&f);
        assert!(
            (v - SOURCE_BALANCE_CAP).abs() < 1e-10,
            "extreme imbalance should saturate at cap, got {v:.6}"
        );
    }

    #[test]
    fn comfort_violation_source_balance_threshold_looser_for_active_goals() {
        // After 2026-05-01 calibration: Shield threshold 12 dB, Focus 15 dB.
        // 13 dB triggers Shield but not Focus.
        let mut f = within_threshold_features();
        f.source_balance_db_range = Some(13.0);

        let shield = Goal::new(GoalKind::Shield);
        let mut f_shield = f.clone();
        f_shield.spectral_tilt_db_per_oct = Some(shield.spectral_tilt_target_db_per_oct());
        assert!(
            shield.comfort_violation(&f_shield) > 0.0,
            "Shield should violate at 13 dB (threshold 12)"
        );

        let focus = Goal::new(GoalKind::Focus);
        let mut f_focus = f.clone();
        f_focus.spectral_tilt_db_per_oct = Some(focus.spectral_tilt_target_db_per_oct());
        assert!(
            focus.comfort_violation(&f_focus) < 1e-12,
            "Focus tolerates 13 dB (its threshold is 15 dB)"
        );
    }

    /// After 2026-05-01 calibration, Shield's source-balance threshold
    /// is 12 dB and cap_at is 24 dB. A 13.3 dB imbalance (the value
    /// from the original optimization run that motivated §28b) is no
    /// longer saturating — it sits ~1.3 dB into the ramp. To check
    /// saturation we now need ≥ 24 dB, which is the calibrated p90 of
    /// curated presets (the loudest hand-tuned imbalance was 24.93 dB
    /// for `normal_set_shield_v5_optimized.json`).
    #[test]
    fn comfort_violation_dominant_source_preset_saturates_term() {
        let mut f = within_threshold_features();
        f.source_balance_db_range = Some(30.0); // beyond Shield's 24 dB cap_at
        let g = Goal::new(GoalKind::Shield);
        f.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
        let v = g.comfort_violation(&f);
        assert!(
            v >= SOURCE_BALANCE_CAP - 1e-10,
            "30 dB should saturate Shield's source-balance term, got {v:.6}"
        );

        // And the 13.3 dB case should now sit *inside* the ramp, not
        // saturate. This pins the calibration loosening explicitly.
        f.source_balance_db_range = Some(13.3);
        let v_partial = g.comfort_violation(&f);
        assert!(
            v_partial > 0.0 && v_partial < SOURCE_BALANCE_CAP - 1e-9,
            "13.3 dB (just past 12 dB threshold) should be a partial penalty, got {v_partial:.6}"
        );
    }

    // ── Min-active-sources term (companion to source_balance) ──────

    #[test]
    fn comfort_violation_min_sources_zero_when_count_meets_threshold() {
        for &kind in GoalKind::all() {
            let g = Goal::new(kind);
            let mut f = within_threshold_features();
            f.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
            // exactly the goal's minimum
            f.active_source_count = Some(g.min_active_sources());
            assert_eq!(
                g.comfort_violation(&f),
                0.0,
                "{kind}: count == min_active_sources should give 0 violation"
            );
            // Above the minimum is also fine
            f.active_source_count = Some(g.min_active_sources() + 5);
            assert_eq!(
                g.comfort_violation(&f),
                0.0,
                "{kind}: count > min_active_sources should give 0 violation"
            );
        }
    }

    #[test]
    fn comfort_violation_min_sources_triggers_for_cocoon_goals_when_collapsed() {
        // The motivating case: optimizer collapses to 1 source, which
        // trivially passes source_balance (1 source ⇒ 0 dB range) but
        // breaks the cocoon design intent. Shield/Isolation must penalise
        // this; active goals should NOT (1 source is valid for them).
        for &kind in &[GoalKind::Shield, GoalKind::Isolation] {
            let g = Goal::new(kind);
            let mut f = within_threshold_features();
            f.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
            f.active_source_count = Some(1);
            f.source_balance_db_range = Some(0.0); // 1 source ⇒ 0 dB
            let v = g.comfort_violation(&f);
            assert!(
                v > 0.0,
                "{kind}: 1 source should violate min-sources floor (min={})",
                g.min_active_sources()
            );
        }
        for &kind in &[GoalKind::Focus, GoalKind::DeepWork, GoalKind::Ignition] {
            let g = Goal::new(kind);
            let mut f = within_threshold_features();
            f.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
            f.active_source_count = Some(1);
            f.source_balance_db_range = Some(0.0);
            assert_eq!(
                g.comfort_violation(&f),
                0.0,
                "{kind}: 1 source is acceptable (min=1)"
            );
        }
    }

    #[test]
    fn comfort_violation_min_sources_full_cap_at_zero_count() {
        // Zero active sources for Shield (min=3) should give the full
        // MIN_SOURCES_CAP penalty contribution.
        let g = Goal::new(GoalKind::Shield);
        let mut f = within_threshold_features();
        f.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
        f.active_source_count = Some(0);
        f.source_balance_db_range = Some(0.0);
        let v = g.comfort_violation(&f);
        assert!(
            (v - MIN_SOURCES_CAP).abs() < 1e-10,
            "0 sources for Shield (min=3) should saturate min-sources term, got {v:.6}"
        );
    }

    #[test]
    fn comfort_violation_min_sources_partial_when_count_below_min() {
        // Shield min=3. count=2 means deficit=1, fraction=1/3 of cap.
        let g = Goal::new(GoalKind::Shield);
        let mut f = within_threshold_features();
        f.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
        f.source_balance_db_range = Some(0.0);

        f.active_source_count = Some(2);
        let v2 = g.comfort_violation(&f);
        let expected2 = MIN_SOURCES_CAP * (1.0 / 3.0);
        assert!(
            (v2 - expected2).abs() < 1e-10,
            "Shield count=2 (min=3): expected {expected2:.6}, got {v2:.6}"
        );

        // count=1 ⇒ deficit=2, fraction=2/3.
        f.active_source_count = Some(1);
        let v1 = g.comfort_violation(&f);
        let expected1 = MIN_SOURCES_CAP * (2.0 / 3.0);
        assert!(
            (v1 - expected1).abs() < 1e-10,
            "Shield count=1 (min=3): expected {expected1:.6}, got {v1:.6}"
        );
    }

    #[test]
    fn comfort_violation_min_sources_missing_contributes_zero() {
        // Field absent → no penalty regardless of goal.
        let mut f = within_threshold_features();
        f.active_source_count = None;
        for &kind in GoalKind::all() {
            let g = Goal::new(kind);
            let mut ff = f.clone();
            ff.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
            assert_eq!(
                g.comfort_violation(&ff),
                0.0,
                "{kind}: missing active_source_count should produce 0 violation"
            );
        }
    }

    #[test]
    fn comfort_violation_source_balance_missing_contributes_zero() {
        // No source_balance_db_range field → no penalty regardless of threshold.
        let mut f = within_threshold_features();
        f.source_balance_db_range = None;
        for &kind in GoalKind::all() {
            let g = Goal::new(kind);
            let mut ff = f.clone();
            ff.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());
            assert_eq!(
                g.comfort_violation(&ff),
                0.0,
                "missing source_balance must produce 0 violation for {kind}"
            );
        }
    }

    #[test]
    fn comfort_violation_true_peak_ramps_above_ceiling() {
        let mut f = within_threshold_features();
        let g = Goal::new(GoalKind::Flow);
        f.spectral_tilt_db_per_oct = Some(g.spectral_tilt_target_db_per_oct());

        f.true_peak_dbfs = Some(-2.0); // below ceiling
        assert_eq!(g.comfort_violation(&f), 0.0);

        f.true_peak_dbfs = Some(-1.0); // exactly at ceiling
        assert_eq!(g.comfort_violation(&f), 0.0);

        f.true_peak_dbfs = Some(0.0); // 1 dB above ceiling → partial penalty
        let v_partial = g.comfort_violation(&f);
        assert!(v_partial > 0.0 && v_partial < TRUE_PEAK_CAP);

        f.true_peak_dbfs = Some(2.0); // at cap
        let v_full = g.comfort_violation(&f);
        assert!((v_full - TRUE_PEAK_CAP).abs() < 1e-10);
    }
}
