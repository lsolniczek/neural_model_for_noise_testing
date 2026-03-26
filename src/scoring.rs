/// Scoring / goal system.
///
/// Each goal defines scientifically-grounded target ranges for EEG band powers
/// (min, ideal, max) and FHN firing characteristics. The optimizer maximises
/// the score returned by `Goal::evaluate()`.
///
/// ## Scoring formula
/// Band score = triangular function peaked at `ideal`, zero at/beyond `min`/`max`.
/// This prevents gaming by ensuring values above the physiological maximum reduce
/// the score just as much as values below the minimum.
///
/// ## Scientific references
/// - Klimesch 1999: Alpha power and memory performance
/// - Cavanagh & Frank 2014: Frontal theta as working memory signal
/// - Ogilvie 2001: Sleep onset EEG dynamics
/// - Lomas et al. 2015: EEG during meditation
/// - Katahira et al. 2018: EEG correlates of flow state
/// - Engel & Fries 2010: Beta-band oscillations and active maintenance

use crate::neural::{BandPowers, FhnResult, JansenRitResult};
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
            "deep_work" | "deepwork" | "flow" => Some(GoalKind::DeepWork),
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
        ]
    }
}

/// Target range for a single EEG band.
///
/// Scoring is triangular: 0 at/below `min`, peaks at 1.0 at `ideal`,
/// back to 0 at/above `max`. Values outside [min, max] score 0.
#[derive(Debug, Clone, Copy)]
struct BandTarget {
    min: f64,
    ideal: f64,
    max: f64,
}

impl BandTarget {
    fn score(&self, power: f64) -> f64 {
        if power <= self.min || power >= self.max {
            0.0
        } else if power <= self.ideal {
            (power - self.min) / (self.ideal - self.min)
        } else {
            (self.max - power) / (self.max - self.ideal)
        }
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
            // NREM stage 1–2: theta dominant, alpha fading, delta beginning.
            // Models the transition into sleep — noise machines target this
            // phase, not deep slow-wave sleep.
            // Ref: Ogilvie 2001 (sleep onset EEG), Carskadon & Dement 2011.
            GoalKind::Sleep => Goal {
                kind,
                band_targets: BandTargets {
                    delta: BandTarget { min: 0.05, ideal: 0.22, max: 0.42 },
                    theta: BandTarget { min: 0.28, ideal: 0.46, max: 0.65 },
                    alpha: BandTarget { min: 0.00, ideal: 0.07, max: 0.20 },
                    beta:  BandTarget { min: 0.00, ideal: 0.01, max: 0.08 },
                    gamma: BandTarget { min: 0.00, ideal: 0.01, max: 0.04 },
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

        let brightness_mod = self.brightness_modifier(brightness);

        // Neural model does 90% of the work; brightness is a psychoacoustic complement.
        let total = 0.9 * neural_score + 0.1 * brightness_mod;

        total.clamp(0.0, 1.0)
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
                // Moderate darkness — brown/pink foundation is ideal
                // Too bright is distracting; too dark is soporific
                // Peak at brightness=0.35
                let x = (brightness - 0.35).abs();
                (1.0 - 1.5 * x).clamp(0.0, 1.0)
            }
        }
    }

    /// Score EEG band powers against targets using range-based triangular scoring.
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
    pub fn diagnose(&self, fhn: &FhnResult, jansen_rit: &JansenRitResult, brightness: f64) -> Diagnosis {
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
            let diff = (fhn.isi_cv - target_cv).abs();
            if diff < 0.08 {
                MetricStatus::Pass
            } else if diff < 0.18 {
                MetricStatus::Warn
            } else {
                MetricStatus::Fail
            }
        } else {
            MetricStatus::Pass
        };

        let score = self.evaluate_with_brightness(fhn, jansen_rit, brightness);

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

        // ISI regularity
        if let Some(target_cv) = targets.target_isi_cv {
            let cv_diff = (fhn.isi_cv - target_cv).abs();
            let cv_score = (-4.0 * cv_diff).exp(); // slightly softer penalty than before
            score += cv_score;
            components += 1.0;
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
