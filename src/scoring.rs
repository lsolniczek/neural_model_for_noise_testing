/// Scoring / goal system.
///
/// Each goal defines target weights for EEG band powers and FHN firing
/// characteristics. The optimizer maximises the score returned by
/// `Goal::evaluate()`.

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
}

impl fmt::Display for GoalKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GoalKind::DeepRelaxation => write!(f, "deep_relaxation"),
            GoalKind::Focus => write!(f, "focus"),
            GoalKind::Sleep => write!(f, "sleep"),
            GoalKind::Isolation => write!(f, "isolation"),
            GoalKind::Meditation => write!(f, "meditation"),
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
            _ => None,
        }
    }

    /// All goal kinds for iteration.
    pub fn all() -> &'static [GoalKind] {
        &[
            GoalKind::Focus,
            GoalKind::Sleep,
            GoalKind::DeepRelaxation,
            GoalKind::Meditation,
            GoalKind::Isolation,
        ]
    }
}

/// Target band power weights. Positive = maximise, negative = minimise.
struct BandTargets {
    delta: f64,
    theta: f64,
    alpha: f64,
    beta: f64,
    gamma: f64,
}

/// Target FHN characteristics.
struct FhnTargets {
    /// Desired firing rate range (spikes/s). Score penalty outside this range.
    firing_rate_range: (f64, f64),
    /// Desired ISI regularity. Low CV = regular, high = irregular.
    /// None means "don't care".
    target_isi_cv: Option<f64>,
    /// Weight of the FHN component in the total score.
    weight: f64,
}

pub struct Goal {
    kind: GoalKind,
    band_targets: BandTargets,
    fhn_targets: FhnTargets,
    /// Weight for the band power component (0–1).
    band_weight: f64,
}

impl Goal {
    pub fn new(kind: GoalKind) -> Self {
        match kind {
            GoalKind::DeepRelaxation => Goal {
                kind,
                band_targets: BandTargets {
                    delta: 0.3,
                    theta: 0.5,
                    alpha: 0.2,
                    beta: -0.5,
                    gamma: -0.3,
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (0.5, 5.0), // Low, slow firing
                    target_isi_cv: Some(0.1),       // Very regular
                    weight: 0.3,
                },
                band_weight: 0.7,
            },

            GoalKind::Focus => Goal {
                kind,
                band_targets: BandTargets {
                    delta: -0.3,
                    theta: -0.1,
                    alpha: 0.3,
                    beta: 0.6,
                    gamma: 0.2,
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (5.0, 20.0), // Moderate, steady
                    target_isi_cv: Some(0.15),       // Regular
                    weight: 0.3,
                },
                band_weight: 0.7,
            },

            GoalKind::Sleep => Goal {
                kind,
                band_targets: BandTargets {
                    delta: 0.7,
                    theta: 0.3,
                    alpha: -0.3,
                    beta: -0.6,
                    gamma: -0.4,
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (0.1, 3.0), // Very low firing
                    target_isi_cv: Some(0.05),      // Extremely regular
                    weight: 0.35,
                },
                band_weight: 0.65,
            },

            GoalKind::Isolation => Goal {
                kind,
                band_targets: BandTargets {
                    delta: 0.1,
                    theta: 0.1,
                    alpha: 0.1,
                    beta: 0.1,
                    gamma: 0.1,
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (2.0, 8.0), // Moderate baseline
                    target_isi_cv: None,            // Don't care about regularity
                    weight: 0.2,
                },
                band_weight: 0.4, // Lower weight — isolation cares more about flatness
            },

            GoalKind::Meditation => Goal {
                kind,
                band_targets: BandTargets {
                    delta: 0.1,
                    theta: 0.6,
                    alpha: 0.5,
                    beta: -0.4,
                    gamma: -0.2,
                },
                fhn_targets: FhnTargets {
                    firing_rate_range: (1.0, 6.0),
                    target_isi_cv: Some(0.08), // Very regular, rhythmic
                    weight: 0.35,
                },
                band_weight: 0.65,
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
    ///
    /// Scientific basis for brightness effects:
    /// - **Isolation/masking**: white noise (bright) masks more frequencies uniformly.
    ///   Broadband masking effectiveness scales with spectral coverage.
    /// - **Sleep**: low-frequency-dominant sounds (dark) promote delta/theta.
    ///   High-frequency noise is more arousing and sleep-disrupting.
    /// - **Focus**: moderate brightness (pink) is optimal. Too dark = drowsy,
    ///   too bright = fatiguing. Inverted-U relationship.
    /// - **Relaxation/Meditation**: lower brightness preferred (1/f-like spectra
    ///   match natural auditory environment).
    pub fn evaluate_with_brightness(
        &self,
        fhn: &FhnResult,
        jansen_rit: &JansenRitResult,
        brightness: f64,
    ) -> f64 {
        let band_score = self.score_bands(&jansen_rit.band_powers);
        let fhn_score = self.score_fhn(fhn);

        let neural_score = self.band_weight * band_score + self.fhn_targets.weight * fhn_score;

        // Apply brightness modifier based on psychoacoustic science
        let brightness_mod = self.brightness_modifier(brightness);

        // Neural model (tonotopic JR) does the heavy lifting; brightness is
        // a psychoacoustic complement (10% weight).
        let total = 0.9 * neural_score + 0.1 * brightness_mod;

        total.clamp(0.0, 1.0)
    }

    /// Compute a [0, 1] score modifier based on spectral brightness for this goal.
    fn brightness_modifier(&self, brightness: f64) -> f64 {
        match self.kind {
            GoalKind::Isolation => {
                // White noise is best for masking — linear increase with brightness
                // plus a baseline (even dark noise provides some masking)
                (0.3 + 0.7 * brightness).clamp(0.0, 1.0)
            }
            GoalKind::Sleep => {
                // Dark sounds promote sleep; bright sounds are arousing
                // Score decreases with brightness
                (1.0 - 0.8 * brightness).clamp(0.0, 1.0)
            }
            GoalKind::Focus => {
                // Inverted-U: moderate brightness is optimal (pink noise ~0.5)
                // Peak at brightness=0.45, falls off on both sides
                let x = (brightness - 0.45).abs();
                (1.0 - 2.0 * x).clamp(0.0, 1.0)
            }
            GoalKind::DeepRelaxation => {
                // Lower brightness preferred (natural 1/f spectra)
                // Gradual decrease with brightness
                (0.9 - 0.6 * brightness).clamp(0.0, 1.0)
            }
            GoalKind::Meditation => {
                // Low-to-moderate brightness preferred
                // Natural sounds, not harsh white noise
                (0.85 - 0.5 * brightness).clamp(0.0, 1.0)
            }
        }
    }

    /// Score EEG band powers against targets.
    fn score_bands(&self, powers: &BandPowers) -> f64 {
        let norm = powers.normalized();
        let targets = &self.band_targets;

        // For isolation, we want flat distribution — penalise deviation from uniform
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

        // Weighted sum: positive targets boost score when that band is high,
        // negative targets boost score when that band is low.
        let mut score = 0.0;
        let mut weight_sum = 0.0;

        let pairs = [
            (norm.delta, targets.delta),
            (norm.theta, targets.theta),
            (norm.alpha, targets.alpha),
            (norm.beta, targets.beta),
            (norm.gamma, targets.gamma),
        ];

        for (power, target) in &pairs {
            let w = target.abs();
            if w < 1e-10 {
                continue;
            }
            weight_sum += w;

            if *target > 0.0 {
                // Want this band high
                score += w * power;
            } else {
                // Want this band low
                score += w * (1.0 - power);
            }
        }

        if weight_sum > 0.0 {
            score / weight_sum
        } else {
            0.5
        }
    }

    /// Produce a detailed diagnostic breakdown of how a result matches this goal.
    pub fn diagnose(&self, fhn: &FhnResult, jansen_rit: &JansenRitResult, brightness: f64) -> Diagnosis {
        let norm = jansen_rit.band_powers.normalized();
        let targets = &self.band_targets;

        let band_diagnoses = if self.kind == GoalKind::Isolation {
            // Isolation: each band should be ~0.2
            let uniform = 0.2;
            vec![
                BandDiagnosis::new("Delta", 0.0, norm.delta, BandExpectation::Flat(uniform)),
                BandDiagnosis::new("Theta", 0.0, norm.theta, BandExpectation::Flat(uniform)),
                BandDiagnosis::new("Alpha", 0.0, norm.alpha, BandExpectation::Flat(uniform)),
                BandDiagnosis::new("Beta",  0.0, norm.beta,  BandExpectation::Flat(uniform)),
                BandDiagnosis::new("Gamma", 0.0, norm.gamma, BandExpectation::Flat(uniform)),
            ]
        } else {
            vec![
                BandDiagnosis::new("Delta", targets.delta, norm.delta, BandExpectation::from_target(targets.delta)),
                BandDiagnosis::new("Theta", targets.theta, norm.theta, BandExpectation::from_target(targets.theta)),
                BandDiagnosis::new("Alpha", targets.alpha, norm.alpha, BandExpectation::from_target(targets.alpha)),
                BandDiagnosis::new("Beta",  targets.beta,  norm.beta,  BandExpectation::from_target(targets.beta)),
                BandDiagnosis::new("Gamma", targets.gamma, norm.gamma, BandExpectation::from_target(targets.gamma)),
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
            if diff < 0.05 {
                MetricStatus::Pass
            } else if diff < 0.15 {
                MetricStatus::Warn
            } else {
                MetricStatus::Fail
            }
        } else {
            MetricStatus::Pass // Don't care
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

        // Firing rate score: 1.0 inside target range, decays outside
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

        // ISI regularity score
        if let Some(target_cv) = targets.target_isi_cv {
            let cv_diff = (fhn.isi_cv - target_cv).abs();
            let cv_score = (-5.0 * cv_diff).exp();
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
    High,
    Low,
    Neutral,
    Flat(f64), // target value for isolation
}

impl BandExpectation {
    fn from_target(target: f64) -> Self {
        if target > 0.2 {
            BandExpectation::High
        } else if target < -0.2 {
            BandExpectation::Low
        } else {
            BandExpectation::Neutral
        }
    }
}

impl fmt::Display for BandExpectation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BandExpectation::High => write!(f, "HIGH"),
            BandExpectation::Low => write!(f, "LOW"),
            BandExpectation::Neutral => write!(f, "---"),
            BandExpectation::Flat(_) => write!(f, "FLAT"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct BandDiagnosis {
    pub name: &'static str,
    pub target_weight: f64,
    pub actual: f64,
    pub expectation: BandExpectation,
    pub status: MetricStatus,
}

impl BandDiagnosis {
    fn new(name: &'static str, target_weight: f64, actual: f64, expectation: BandExpectation) -> Self {
        let status = match expectation {
            BandExpectation::High => {
                if actual >= 0.25 { MetricStatus::Pass }
                else if actual >= 0.15 { MetricStatus::Warn }
                else { MetricStatus::Fail }
            }
            BandExpectation::Low => {
                if actual <= 0.15 { MetricStatus::Pass }
                else if actual <= 0.25 { MetricStatus::Warn }
                else { MetricStatus::Fail }
            }
            BandExpectation::Neutral => MetricStatus::Pass,
            BandExpectation::Flat(target) => {
                let diff = (actual - target).abs();
                if diff < 0.05 { MetricStatus::Pass }
                else if diff < 0.10 { MetricStatus::Warn }
                else { MetricStatus::Fail }
            }
        };
        BandDiagnosis { name, target_weight, actual, expectation, status }
    }
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
