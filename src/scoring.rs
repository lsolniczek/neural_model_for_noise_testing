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

use crate::neural::{BandPowers, FhnResult, JansenRitResult, PerformanceVector};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    beta:  BandTarget,
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
                    delta: BandTarget { min: 0.05, ideal: 0.22, max: 0.40 },
                    theta: BandTarget { min: 0.18, ideal: 0.35, max: 0.52 },
                    alpha: BandTarget { min: 0.20, ideal: 0.36, max: 0.52 },
                    beta:  BandTarget { min: 0.00, ideal: 0.03, max: 0.14 },
                    gamma: BandTarget { min: 0.00, ideal: 0.01, max: 0.06 },
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
                    delta: BandTarget { min: 0.00, ideal: 0.01, max: 0.08 },
                    theta: BandTarget { min: 0.08, ideal: 0.18, max: 0.32 },
                    alpha: BandTarget { min: 0.18, ideal: 0.33, max: 0.50 },
                    beta:  BandTarget { min: 0.25, ideal: 0.42, max: 0.60 },
                    // NOTE: The Jansen-Rit model cannot produce >17 Hz oscillations,
                    // so gamma power (30-50 Hz) comes from the WilsonCowan oscillators
                    // in tonotopic bands 2-3 (see BandModelType::WilsonCowan in brain_type.rs).
                    gamma: BandTarget { min: 0.02, ideal: 0.06, max: 0.15 },
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
                    delta: BandTarget { min: 0.08, ideal: 0.30, max: 0.50 },
                    theta: BandTarget { min: 0.28, ideal: 0.48, max: 0.68 },
                    alpha: BandTarget { min: 0.00, ideal: 0.12, max: 0.25 },
                    beta:  BandTarget { min: 0.00, ideal: 0.02, max: 0.08 },
                    gamma: BandTarget { min: 0.00, ideal: 0.02, max: 0.06 },
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
                    delta: BandTarget { min: 0.10, ideal: 0.20, max: 0.30 },
                    theta: BandTarget { min: 0.10, ideal: 0.20, max: 0.30 },
                    alpha: BandTarget { min: 0.10, ideal: 0.20, max: 0.30 },
                    beta:  BandTarget { min: 0.10, ideal: 0.20, max: 0.30 },
                    gamma: BandTarget { min: 0.10, ideal: 0.20, max: 0.30 },
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
                    delta: BandTarget { min: 0.02, ideal: 0.08, max: 0.22 },
                    theta: BandTarget { min: 0.25, ideal: 0.40, max: 0.56 },
                    alpha: BandTarget { min: 0.25, ideal: 0.40, max: 0.56 },
                    beta:  BandTarget { min: 0.00, ideal: 0.03, max: 0.12 },
                    gamma: BandTarget { min: 0.00, ideal: 0.02, max: 0.08 },
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
                    delta: BandTarget { min: 0.00, ideal: 0.01, max: 0.06 },
                    theta: BandTarget { min: 0.15, ideal: 0.30, max: 0.46 },
                    alpha: BandTarget { min: 0.35, ideal: 0.52, max: 0.70 }, // dominant
                    beta:  BandTarget { min: 0.02, ideal: 0.10, max: 0.24 },
                    gamma: BandTarget { min: 0.00, ideal: 0.02, max: 0.08 },
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
            GoalKind::Shield => Goal {
                kind,
                band_targets: BandTargets {
                    delta: BandTarget { min: 0.00, ideal: 0.05, max: 0.15 },
                    theta: BandTarget { min: 0.00, ideal: 0.03, max: 0.10 },
                    alpha: BandTarget { min: 0.15, ideal: 0.30, max: 0.50 },
                    beta:  BandTarget { min: 0.30, ideal: 0.50, max: 0.70 },
                    gamma: BandTarget { min: 0.00, ideal: 0.05, max: 0.15 },
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (5.0, 15.0),
                    target_isi_cv: Some(0.25), // Stable, low jitter
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
                    delta: BandTarget { min: 0.00, ideal: 0.05, max: 0.15 },
                    theta: BandTarget { min: 0.05, ideal: 0.15, max: 0.30 },
                    alpha: BandTarget { min: 0.30, ideal: 0.45, max: 0.60 },
                    beta:  BandTarget { min: 0.15, ideal: 0.30, max: 0.45 },
                    gamma: BandTarget { min: 0.00, ideal: 0.02, max: 0.08 },
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
                    delta: BandTarget { min: 0.00, ideal: 0.02, max: 0.05 },
                    theta: BandTarget { min: 0.00, ideal: 0.10, max: 0.25 },
                    alpha: BandTarget { min: 0.10, ideal: 0.20, max: 0.35 },
                    beta:  BandTarget { min: 0.35, ideal: 0.50, max: 0.65 },
                    gamma: BandTarget { min: 0.05, ideal: 0.15, max: 0.35 },
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

    /// Evaluate with all corrections: asymmetry penalty + PLV entrainment bonus.
    ///
    /// Per Lachaux et al. (1999) and Helfrich et al. (2014), phase-locking
    /// to the modulation frequency indicates genuine neural entrainment.
    /// Goals that want entrainment (Focus, Isolation) get a score bonus
    /// proportional to PLV. Goals that want natural rhythms (Sleep, Relaxation)
    /// don't benefit from entrainment.
    pub fn evaluate_full(
        &self,
        fhn: &FhnResult,
        jansen_rit: &JansenRitResult,
        alpha_asymmetry: f64,
        plv: Option<f64>,
    ) -> f64 {
        let base_score = self.evaluate_with_asymmetry(fhn, jansen_rit, alpha_asymmetry);

        // PLV bonus: weighted by goal's entrainment relevance
        let plv_bonus = if let Some(plv_value) = plv {
            let weight = self.entrainment_weight();
            weight * plv_value * 0.10 // max 10% bonus at PLV=1.0
        } else {
            0.0
        };

        (base_score + plv_bonus).clamp(0.0, 1.0)
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
                    + (norm.beta  - uniform).abs()
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
    pub fn diagnose(&self, fhn: &FhnResult, jansen_rit: &JansenRitResult, brightness: f64, alpha_asymmetry: f64, plv: Option<f64>, performance: Option<PerformanceVector>) -> Diagnosis {
        let norm = jansen_rit.band_powers.normalized();
        let t = &self.band_targets;

        let band_diagnoses = if self.kind == GoalKind::Isolation {
            let uniform = 0.2;
            vec![
                BandDiagnosis { name: "Delta", actual: norm.delta, expectation: BandExpectation::Flat(uniform), status: flat_status(norm.delta, uniform) },
                BandDiagnosis { name: "Theta", actual: norm.theta, expectation: BandExpectation::Flat(uniform), status: flat_status(norm.theta, uniform) },
                BandDiagnosis { name: "Alpha", actual: norm.alpha, expectation: BandExpectation::Flat(uniform), status: flat_status(norm.alpha, uniform) },
                BandDiagnosis { name: "Beta",  actual: norm.beta,  expectation: BandExpectation::Flat(uniform), status: flat_status(norm.beta,  uniform) },
                BandDiagnosis { name: "Gamma", actual: norm.gamma, expectation: BandExpectation::Flat(uniform), status: flat_status(norm.gamma, uniform) },
            ]
        } else {
            vec![
                BandDiagnosis { name: "Delta", actual: norm.delta, expectation: t.delta.expectation(), status: t.delta.status(norm.delta) },
                BandDiagnosis { name: "Theta", actual: norm.theta, expectation: t.theta.expectation(), status: t.theta.status(norm.theta) },
                BandDiagnosis { name: "Alpha", actual: norm.alpha, expectation: t.alpha.expectation(), status: t.alpha.status(norm.alpha) },
                BandDiagnosis { name: "Beta",  actual: norm.beta,  expectation: t.beta.expectation(),  status: t.beta.status(norm.beta)  },
                BandDiagnosis { name: "Gamma", actual: norm.gamma, expectation: t.gamma.expectation(), status: t.gamma.status(norm.gamma) },
            ]
        };

        let (min_rate, max_rate) = self.fhn_targets.firing_rate_range;
        let firing_rate_status = if fhn.firing_rate >= min_rate && fhn.firing_rate <= max_rate {
            MetricStatus::Pass
        } else if (fhn.firing_rate - min_rate).abs() < 2.0 || (fhn.firing_rate - max_rate).abs() < 2.0 {
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

        let score = self.evaluate_full(fhn, jansen_rit, alpha_asymmetry, plv);

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
    if diff < 0.05 { MetricStatus::Pass }
    else if diff < 0.10 { MetricStatus::Warn }
    else { MetricStatus::Fail }
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
            Verdict::Ok  => write!(f, "OK"),
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
        if f < 4.0 { "Delta" }
        else if f < 8.0 { "Theta" }
        else if f < 13.0 { "Alpha" }
        else if f < 30.0 { "Beta" }
        else { "Gamma" }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            band_powers: BandPowers { delta, theta, alpha, beta, gamma },
            dominant_freq: 10.0,
            fast_inhib_trace: vec![],
        }
    }

    // ---------------------------------------------------------------
    // BandTarget::score — Gaussian formula
    // ---------------------------------------------------------------

    #[test]
    fn band_score_at_ideal_is_one() {
        let t = BandTarget { min: 0.10, ideal: 0.30, max: 0.50 };
        let s = t.score(0.30);
        assert!((s - 1.0).abs() < 1e-10, "Score at ideal should be 1.0, got {s}");
    }

    #[test]
    fn band_score_at_boundaries_near_005() {
        // Centered ideal: boundaries should give ≈ 0.05
        let t = BandTarget { min: 0.10, ideal: 0.30, max: 0.50 };
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
        let t = BandTarget { min: 0.10, ideal: 0.30, max: 0.50 };
        let above = t.score(0.35);
        let below = t.score(0.25);
        assert!(
            (above - below).abs() < 1e-10,
            "Gaussian should be symmetric: above={above}, below={below}"
        );
    }

    #[test]
    fn band_score_decreases_away_from_ideal() {
        let t = BandTarget { min: 0.10, ideal: 0.30, max: 0.50 };
        let close = t.score(0.28);
        let far = t.score(0.15);
        assert!(close > far, "Closer to ideal should score higher: {close} vs {far}");
    }

    #[test]
    fn band_score_beyond_boundaries_near_zero() {
        let t = BandTarget { min: 0.10, ideal: 0.30, max: 0.50 };
        let beyond = t.score(0.70); // well beyond max
        assert!(beyond < 0.01, "Score well beyond boundary should be ~0, got {beyond}");
    }

    #[test]
    fn band_score_zero_half_width_returns_zero() {
        let t = BandTarget { min: 0.30, ideal: 0.30, max: 0.30 };
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
            let sum = t.delta.ideal + t.theta.ideal + t.alpha.ideal
                + t.beta.ideal + t.gamma.ideal;
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
        assert!(dark > bright, "Sleep should prefer dark: {dark:.3} vs {bright:.3}");
    }

    #[test]
    fn isolation_prefers_bright_sounds() {
        let goal = Goal::new(GoalKind::Isolation);
        let dark = goal.brightness_modifier(0.1);
        let bright = goal.brightness_modifier(0.9);
        assert!(bright > dark, "Isolation should prefer bright: {bright:.3} vs {dark:.3}");
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
        assert!(good > bad, "In-range FHN ({good:.3}) should beat out-of-range ({bad:.3})");
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
        assert!(s_nan > 0.3, "NaN CV with good rate should still score > 0.3, got {s_nan:.3}");
    }

    // ---------------------------------------------------------------
    // GoalKind utilities
    // ---------------------------------------------------------------

    #[test]
    fn goal_kind_all_returns_nine() {
        assert_eq!(GoalKind::all().len(), 9);
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
            let diag = goal.diagnose(&fhn, &jr, 0.5, 0.0, None, None);
            assert_eq!(diag.bands.len(), 5, "{kind} diagnosis should have 5 bands");
        }
    }

    #[test]
    fn diagnose_verdict_good_for_high_score() {
        let fhn = make_fhn(12.0, 0.30);
        let jr = make_jr(0.01, 0.18, 0.33, 0.42, 0.06); // Focus ideals

        let goal = Goal::new(GoalKind::Focus);
        let diag = goal.diagnose(&fhn, &jr, 0.55, 0.0, None, None);

        assert!(
            matches!(diag.verdict, Verdict::Good),
            "Focus with ideal inputs should get Good verdict, got {:?} (score={:.3})",
            diag.verdict, diag.score
        );
    }

    // ---------------------------------------------------------------
    // BandTarget status
    // ---------------------------------------------------------------

    #[test]
    fn band_status_pass_near_ideal() {
        let t = BandTarget { min: 0.10, ideal: 0.30, max: 0.50 };
        assert!(matches!(t.status(0.30), MetricStatus::Pass));
    }

    #[test]
    fn band_status_fail_outside_range() {
        let t = BandTarget { min: 0.10, ideal: 0.30, max: 0.50 };
        assert!(matches!(t.status(0.05), MetricStatus::Fail));
        assert!(matches!(t.status(0.60), MetricStatus::Fail));
    }
}
