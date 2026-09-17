//! Replicated evaluation and common-random-number finalist comparison.

use crate::pipeline::{evaluate_preset, SimulationConfig, SimulationResult};
use crate::preset::Preset;
use crate::reproducibility::{ReplicateIdentity, SeedPanel, SeedPolicy};
use crate::scoring::Goal;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::BTreeMap;

pub const CONFIDENCE_LEVEL: f64 = 0.95;
pub const FAMILY_ALPHA: f64 = 0.05;
pub const DEFAULT_INDIFFERENCE_DELTA: f64 = 0.01;
/// Calibrated by `benchmarks/p06/manifest.json` against the frozen 64-replicate panel.
pub const DEFAULT_REPLICATES: usize = 64;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfidenceInterval {
    pub lower: f64,
    pub upper: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScalarSummary {
    pub n: usize,
    pub mean: f64,
    pub sample_std: Option<f64>,
    pub standard_error: Option<f64>,
    pub confidence_interval_95: Option<ConfidenceInterval>,
    pub min: f64,
    pub max: f64,
}

impl ScalarSummary {
    pub fn from_values(values: &[f64]) -> Result<Self, ReplicationError> {
        if values.is_empty() {
            return Err(ReplicationError::new("at least one replicate is required"));
        }
        if values.iter().any(|value| !value.is_finite()) {
            return Err(ReplicationError::new(
                "replicated evaluation produced a non-finite value",
            ));
        }
        let n = values.len();
        let mean = values.iter().sum::<f64>() / n as f64;
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let (sample_std, standard_error, confidence_interval_95) = if n == 1 {
            (None, None, None)
        } else {
            let sum_squares = values
                .iter()
                .map(|value| {
                    let centered = value - mean;
                    centered * centered
                })
                .sum::<f64>();
            let sd = (sum_squares / (n - 1) as f64).sqrt();
            let se = sd / (n as f64).sqrt();
            let critical = student_t_quantile(0.975, n - 1)?;
            (
                Some(sd),
                Some(se),
                Some(ConfidenceInterval {
                    lower: mean - critical * se,
                    upper: mean + critical * se,
                }),
            )
        };
        Ok(Self {
            n,
            mean,
            sample_std,
            standard_error,
            confidence_interval_95,
            min,
            max,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicateObservation {
    pub identity: ReplicateIdentity,
    pub score: f64,
    pub fhn_firing_rate: f64,
    pub dominant_freq: f64,
    pub delta_power: f64,
    pub theta_power: f64,
    pub alpha_power: f64,
    pub beta_power: f64,
    pub gamma_power: f64,
    pub brightness: f64,
}

impl ReplicateObservation {
    fn from_result(identity: ReplicateIdentity, result: &SimulationResult) -> Self {
        Self {
            identity,
            score: result.score,
            fhn_firing_rate: result.fhn_firing_rate,
            dominant_freq: result.dominant_freq,
            delta_power: result.delta_power,
            theta_power: result.theta_power,
            alpha_power: result.alpha_power,
            beta_power: result.beta_power,
            gamma_power: result.gamma_power,
            brightness: result.brightness,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicatedEvaluation {
    pub replicates: Vec<ReplicateObservation>,
    pub score: ScalarSummary,
    pub fhn_firing_rate: ScalarSummary,
    pub dominant_freq: ScalarSummary,
    pub delta_power: ScalarSummary,
    pub theta_power: ScalarSummary,
    pub alpha_power: ScalarSummary,
    pub beta_power: ScalarSummary,
    pub gamma_power: ScalarSummary,
    pub brightness: ScalarSummary,
}

pub fn evaluate_preset_replicated(
    preset: &Preset,
    goal: &Goal,
    base_config: &SimulationConfig,
    run_seed: u64,
    panel: SeedPanel,
    replicates: usize,
) -> Result<ReplicatedEvaluation, ReplicationError> {
    if replicates == 0 {
        return Err(ReplicationError::new("replicates must be at least 1"));
    }
    let mut observations = Vec::with_capacity(replicates);
    for replicate_index in 0..replicates {
        let mut config = base_config.clone();
        config.seed_policy = SeedPolicy::domain_separated(run_seed, panel, replicate_index as u64);
        let identity = config
            .seed_policy
            .evaluation_plan()
            .expect("domain-separated policy has an evaluation plan")
            .identity();
        let result = evaluate_preset(preset, goal, &config);
        observations.push(ReplicateObservation::from_result(identity, &result));
    }
    ReplicatedEvaluation::from_observations(observations)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisturbanceObservation {
    pub identity: ReplicateIdentity,
    pub bppr: f64,
    pub spectral_resilience: f64,
    pub scdi_hz: f64,
    pub peak_freq_deviation: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReplicatedDisturbance {
    pub replicates: Vec<DisturbanceObservation>,
    pub bppr: ScalarSummary,
    pub spectral_resilience: ScalarSummary,
    pub scdi_hz: ScalarSummary,
    pub peak_freq_deviation: ScalarSummary,
}

pub fn evaluate_disturbance_replicated(
    preset: &Preset,
    base_config: &crate::disturb::DisturbConfig,
    run_seed: u64,
    replicates: usize,
) -> Result<ReplicatedDisturbance, ReplicationError> {
    if replicates == 0 {
        return Err(ReplicationError::new("replicates must be at least 1"));
    }
    if base_config.mode == crate::disturb::DisturbanceMode::LegacyAblated {
        return Err(ReplicationError::new(
            "replicated disturbance requires canonical mode",
        ));
    }
    let mut observations = Vec::with_capacity(replicates);
    for replicate_index in 0..replicates {
        let mut config = base_config.clone();
        config.seed_policy =
            SeedPolicy::domain_separated(run_seed, SeedPanel::Disturb, replicate_index as u64);
        let identity = config
            .seed_policy
            .evaluation_plan()
            .expect("domain-separated policy has an evaluation plan")
            .identity();
        let result = crate::disturb::run_disturb(preset, &config);
        observations.push(DisturbanceObservation {
            identity,
            bppr: result.bppr,
            spectral_resilience: result.spectral_resilience,
            scdi_hz: result.scdi_hz,
            peak_freq_deviation: result.peak_freq_deviation,
        });
    }
    let values = |field: fn(&DisturbanceObservation) -> f64| {
        observations.iter().map(field).collect::<Vec<_>>()
    };
    Ok(ReplicatedDisturbance {
        bppr: ScalarSummary::from_values(&values(|item| item.bppr))?,
        spectral_resilience: ScalarSummary::from_values(&values(|item| item.spectral_resilience))?,
        scdi_hz: ScalarSummary::from_values(&values(|item| item.scdi_hz))?,
        peak_freq_deviation: ScalarSummary::from_values(&values(|item| item.peak_freq_deviation))?,
        replicates: observations,
    })
}

impl ReplicatedEvaluation {
    pub fn from_observations(
        replicates: Vec<ReplicateObservation>,
    ) -> Result<Self, ReplicationError> {
        macro_rules! summarize {
            ($field:ident) => {{
                let values = replicates
                    .iter()
                    .map(|replicate| replicate.$field)
                    .collect::<Vec<_>>();
                ScalarSummary::from_values(&values)?
            }};
        }
        Ok(Self {
            score: summarize!(score),
            fhn_firing_rate: summarize!(fhn_firing_rate),
            dominant_freq: summarize!(dominant_freq),
            delta_power: summarize!(delta_power),
            theta_power: summarize!(theta_power),
            alpha_power: summarize!(alpha_power),
            beta_power: summarize!(beta_power),
            gamma_power: summarize!(gamma_power),
            brightness: summarize!(brightness),
            replicates,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinalistStatus {
    Unique,
    Inconclusive,
    InsufficientDistinctCandidates,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalistScores {
    pub canonical_hash: String,
    pub scores: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PairwiseComparison {
    pub first_hash: String,
    pub second_hash: String,
    pub mean_difference_first_minus_second: f64,
    pub sample_std_difference: Option<f64>,
    pub standard_error_difference: Option<f64>,
    pub bonferroni_confidence_interval: Option<ConfidenceInterval>,
    pub first_is_practically_better: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalistComparison {
    pub status: FinalistStatus,
    pub observed_leader: Option<String>,
    pub inconclusive_set: Vec<String>,
    pub delta: f64,
    pub confidence_level: f64,
    pub correction: String,
    pub comparisons: usize,
    pub n: usize,
    pub finalists: Vec<FinalistScores>,
    pub pairwise: Vec<PairwiseComparison>,
    pub interval_assumption: String,
}

pub fn compare_finalist_scores(
    finalists: Vec<FinalistScores>,
    delta: f64,
) -> Result<FinalistComparison, ReplicationError> {
    if !delta.is_finite() || delta < 0.0 {
        return Err(ReplicationError::new(
            "indifference delta must be finite and non-negative",
        ));
    }
    let mut distinct = BTreeMap::<String, Vec<f64>>::new();
    for finalist in finalists {
        if finalist.scores.is_empty() {
            return Err(ReplicationError::new(
                "every finalist must contain at least one score",
            ));
        }
        if finalist.scores.iter().any(|score| !score.is_finite()) {
            return Err(ReplicationError::new("finalist score must be finite"));
        }
        match distinct.get(&finalist.canonical_hash) {
            Some(existing) if existing != &finalist.scores => {
                return Err(ReplicationError::new(
                    "duplicate canonical finalist hash has different scores",
                ));
            }
            Some(_) => {}
            None => {
                distinct.insert(finalist.canonical_hash, finalist.scores);
            }
        }
    }
    let finalists = distinct
        .into_iter()
        .map(|(canonical_hash, scores)| FinalistScores {
            canonical_hash,
            scores,
        })
        .collect::<Vec<_>>();
    if finalists.len() < 2 {
        return Ok(FinalistComparison {
            status: FinalistStatus::InsufficientDistinctCandidates,
            observed_leader: finalists.first().map(|f| f.canonical_hash.clone()),
            inconclusive_set: finalists.iter().map(|f| f.canonical_hash.clone()).collect(),
            delta,
            confidence_level: CONFIDENCE_LEVEL,
            correction: "bonferroni_all_pairs".to_string(),
            comparisons: 0,
            n: finalists.first().map_or(0, |f| f.scores.len()),
            finalists,
            pairwise: Vec::new(),
            interval_assumption: interval_assumption(),
        });
    }
    let n = finalists[0].scores.len();
    if n < 2 {
        return Err(ReplicationError::new(
            "finalist comparison requires at least 2 paired replicates",
        ));
    }
    if finalists.iter().any(|finalist| finalist.scores.len() != n) {
        return Err(ReplicationError::new(
            "all finalists must use the same common-random-number panel",
        ));
    }
    let means = finalists
        .iter()
        .map(|finalist| finalist.scores.iter().sum::<f64>() / n as f64)
        .collect::<Vec<_>>();
    let mut leader_index = 0;
    for index in 1..finalists.len() {
        let order = means[index]
            .partial_cmp(&means[leader_index])
            .unwrap_or(Ordering::Equal);
        if order == Ordering::Greater
            || (order == Ordering::Equal
                && finalists[index].canonical_hash < finalists[leader_index].canonical_hash)
        {
            leader_index = index;
        }
    }

    let comparison_count = finalists.len() * (finalists.len() - 1) / 2;
    let probability = 1.0 - FAMILY_ALPHA / (2.0 * comparison_count as f64);
    let critical = student_t_quantile(probability, n - 1)?;
    let mut pairwise = Vec::with_capacity(comparison_count);
    let mut leader_lower_bounds = BTreeMap::<usize, f64>::new();
    for first in 0..finalists.len() {
        for second in (first + 1)..finalists.len() {
            let differences = finalists[first]
                .scores
                .iter()
                .zip(finalists[second].scores.iter())
                .map(|(a, b)| a - b)
                .collect::<Vec<_>>();
            let summary = ScalarSummary::from_values(&differences)?;
            let se = summary
                .standard_error
                .expect("n >= 2 provides standard error");
            let interval = ConfidenceInterval {
                lower: summary.mean - critical * se,
                upper: summary.mean + critical * se,
            };
            let first_is_practically_better = interval.lower > delta;
            if first == leader_index {
                leader_lower_bounds.insert(second, interval.lower);
            } else if second == leader_index {
                leader_lower_bounds.insert(first, -interval.upper);
            }
            pairwise.push(PairwiseComparison {
                first_hash: finalists[first].canonical_hash.clone(),
                second_hash: finalists[second].canonical_hash.clone(),
                mean_difference_first_minus_second: summary.mean,
                sample_std_difference: summary.sample_std,
                standard_error_difference: summary.standard_error,
                bonferroni_confidence_interval: Some(interval),
                first_is_practically_better,
            });
        }
    }
    let unique = (0..finalists.len())
        .filter(|index| *index != leader_index)
        .all(|index| {
            leader_lower_bounds
                .get(&index)
                .copied()
                .unwrap_or(f64::NEG_INFINITY)
                > delta
        });
    let mut inconclusive_set = vec![finalists[leader_index].canonical_hash.clone()];
    if !unique {
        inconclusive_set.extend(
            (0..finalists.len())
                .filter(|index| *index != leader_index)
                .filter(|index| {
                    leader_lower_bounds
                        .get(index)
                        .copied()
                        .unwrap_or(f64::NEG_INFINITY)
                        <= delta
                })
                .map(|index| finalists[index].canonical_hash.clone()),
        );
        inconclusive_set.sort();
    }
    Ok(FinalistComparison {
        status: if unique {
            FinalistStatus::Unique
        } else {
            FinalistStatus::Inconclusive
        },
        observed_leader: Some(finalists[leader_index].canonical_hash.clone()),
        inconclusive_set,
        delta,
        confidence_level: CONFIDENCE_LEVEL,
        correction: "bonferroni_all_pairs".to_string(),
        comparisons: comparison_count,
        n,
        finalists,
        pairwise,
        interval_assumption: interval_assumption(),
    })
}

fn interval_assumption() -> String {
    "Approximate paired Student-t intervals assume sufficiently regular mean differences; they are an engineering uncertainty measure, not a biological or clinical guarantee."
        .to_string()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicationError {
    message: String,
}

impl ReplicationError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for ReplicationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ReplicationError {}

/// Inverse CDF of Student's t distribution, computed from the regularized
/// incomplete beta function and bisection. The implementation is local so the
/// exact interval contract does not depend on an unpinned statistics crate.
pub fn student_t_quantile(
    probability: f64,
    degrees_of_freedom: usize,
) -> Result<f64, ReplicationError> {
    if !(0.0..1.0).contains(&probability) || degrees_of_freedom == 0 {
        return Err(ReplicationError::new(
            "Student-t probability must be in (0,1) and degrees of freedom must be positive",
        ));
    }
    if probability == 0.5 {
        return Ok(0.0);
    }
    if probability < 0.5 {
        return Ok(-student_t_quantile(1.0 - probability, degrees_of_freedom)?);
    }
    let mut low = 0.0;
    let mut high = 1.0;
    while student_t_cdf(high, degrees_of_freedom) < probability {
        high *= 2.0;
        if high > 1.0e12 {
            return Err(ReplicationError::new("Student-t quantile did not converge"));
        }
    }
    for _ in 0..120 {
        let middle = 0.5 * (low + high);
        if student_t_cdf(middle, degrees_of_freedom) < probability {
            low = middle;
        } else {
            high = middle;
        }
    }
    Ok(0.5 * (low + high))
}

fn student_t_cdf(value: f64, degrees_of_freedom: usize) -> f64 {
    if value == 0.0 {
        return 0.5;
    }
    let degrees = degrees_of_freedom as f64;
    let x = degrees / (degrees + value * value);
    let tail = 0.5 * regularized_incomplete_beta(x, degrees * 0.5, 0.5);
    if value > 0.0 {
        1.0 - tail
    } else {
        tail
    }
}

fn regularized_incomplete_beta(x: f64, a: f64, b: f64) -> f64 {
    if x <= 0.0 {
        return 0.0;
    }
    if x >= 1.0 {
        return 1.0;
    }
    let front = (ln_gamma(a + b) - ln_gamma(a) - ln_gamma(b) + a * x.ln() + b * (-x).ln_1p()).exp();
    if x < (a + 1.0) / (a + b + 2.0) {
        front * beta_continued_fraction(x, a, b) / a
    } else {
        1.0 - front * beta_continued_fraction(1.0 - x, b, a) / b
    }
}

fn beta_continued_fraction(x: f64, a: f64, b: f64) -> f64 {
    const EPSILON: f64 = 3.0e-14;
    const FLOOR: f64 = 1.0e-300;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < FLOOR {
        d = FLOOR;
    }
    d = 1.0 / d;
    let mut result = d;
    for iteration in 1..=200 {
        let m = iteration as f64;
        let m2 = 2.0 * m;
        let mut coefficient = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + coefficient * d;
        if d.abs() < FLOOR {
            d = FLOOR;
        }
        c = 1.0 + coefficient / c;
        if c.abs() < FLOOR {
            c = FLOOR;
        }
        d = 1.0 / d;
        result *= d * c;
        coefficient = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + coefficient * d;
        if d.abs() < FLOOR {
            d = FLOOR;
        }
        c = 1.0 + coefficient / c;
        if c.abs() < FLOOR {
            c = FLOOR;
        }
        d = 1.0 / d;
        let delta = d * c;
        result *= delta;
        if (delta - 1.0).abs() < EPSILON {
            break;
        }
    }
    result
}

fn ln_gamma(value: f64) -> f64 {
    const COEFFICIENTS: [f64; 9] = [
        0.999_999_999_999_809_9,
        676.520_368_121_885_1,
        -1_259.139_216_722_402_8,
        771.323_428_777_653_1,
        -176.615_029_162_140_6,
        12.507_343_278_686_905,
        -0.138_571_095_265_720_12,
        9.984_369_578_019_572e-6,
        1.505_632_735_149_311_6e-7,
    ];
    if value < 0.5 {
        return std::f64::consts::PI.ln()
            - (std::f64::consts::PI * value).sin().ln()
            - ln_gamma(1.0 - value);
    }
    let z = value - 1.0;
    let mut x = COEFFICIENTS[0];
    for (index, coefficient) in COEFFICIENTS.iter().enumerate().skip(1) {
        x += coefficient / (z + index as f64);
    }
    let t = z + 7.5;
    0.5 * (2.0 * std::f64::consts::PI).ln() + (z + 0.5) * t.ln() - t + x.ln()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finalist(hash: &str, scores: &[f64]) -> FinalistScores {
        FinalistScores {
            canonical_hash: hash.to_string(),
            scores: scores.to_vec(),
        }
    }

    #[test]
    fn scalar_summary_matches_hand_calculated_sample_statistics() {
        let summary = ScalarSummary::from_values(&[1.0, 2.0, 3.0, 4.0]).unwrap();
        assert_eq!(summary.n, 4);
        assert_eq!(summary.mean, 2.5);
        assert!((summary.sample_std.unwrap() - 1.290_994_448_735_805_6).abs() < 1e-12);
        assert!((summary.standard_error.unwrap() - 0.645_497_224_367_902_8).abs() < 1e-12);
        let interval = summary.confidence_interval_95.unwrap();
        assert!((interval.lower - 0.445_739_743_239_121).abs() < 1e-10);
        assert!((interval.upper - 4.554_260_256_760_878).abs() < 1e-10);
        assert_eq!(summary.min, 1.0);
        assert_eq!(summary.max, 4.0);
    }

    #[test]
    fn one_replicate_has_null_uncertainty() {
        let summary = ScalarSummary::from_values(&[0.25]).unwrap();
        assert_eq!(summary.sample_std, None);
        assert_eq!(summary.standard_error, None);
        assert_eq!(summary.confidence_interval_95, None);
    }

    #[test]
    fn compiled_default_matches_frozen_benchmark_manifest() {
        let manifest: serde_json::Value =
            serde_json::from_str(include_str!("../benchmarks/p06/manifest.json")).unwrap();
        assert_eq!(
            manifest["selected_default_finalist_replicates"].as_u64(),
            Some(DEFAULT_REPLICATES as u64)
        );
    }

    #[test]
    fn same_seed_replays_the_complete_aggregate_exactly() {
        let preset = Preset::default();
        let goal = Goal::new(crate::scoring::GoalKind::Focus);
        let config = SimulationConfig {
            duration_secs: 2.1,
            ..SimulationConfig::default()
        };
        let run = || {
            evaluate_preset_replicated(&preset, &goal, &config, 42, SeedPanel::Direct, 2).unwrap()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn student_t_known_quantiles_are_stable() {
        assert!((student_t_quantile(0.975, 4).unwrap() - 2.776_445_105_197_798_7).abs() < 1e-10);
        assert!((student_t_quantile(0.975, 30).unwrap() - 2.042_272_456_301_237_3).abs() < 1e-10);
    }

    #[test]
    fn bonferroni_comparison_count_and_quantile_are_correct() {
        let report = compare_finalist_scores(
            vec![
                finalist("a", &[1.0, 1.1, 0.9, 1.05]),
                finalist("b", &[0.5, 0.6, 0.4, 0.55]),
                finalist("c", &[0.0, 0.1, -0.1, 0.05]),
            ],
            0.01,
        )
        .unwrap();
        assert_eq!(report.comparisons, 3);
        assert_eq!(report.n, 4);
        assert_eq!(report.pairwise.len(), 3);
    }

    #[test]
    fn clear_winner_is_unique() {
        let report = compare_finalist_scores(
            vec![
                finalist("winner", &[1.00, 1.02, 0.98, 1.01, 0.99]),
                finalist("runner", &[0.70, 0.72, 0.68, 0.71, 0.69]),
            ],
            0.01,
        )
        .unwrap();
        assert_eq!(report.status, FinalistStatus::Unique);
        assert_eq!(report.observed_leader.as_deref(), Some("winner"));
    }

    #[test]
    fn small_paired_difference_is_inconclusive() {
        let report = compare_finalist_scores(
            vec![
                finalist("a", &[0.50, 0.52, 0.48, 0.51, 0.49]),
                finalist("b", &[0.495, 0.515, 0.475, 0.505, 0.485]),
            ],
            0.01,
        )
        .unwrap();
        assert_eq!(report.status, FinalistStatus::Inconclusive);
        assert_eq!(report.inconclusive_set.len(), 2);
    }

    #[test]
    fn duplicate_candidate_cannot_be_called_unique() {
        let report = compare_finalist_scores(
            vec![finalist("same", &[0.5, 0.6]), finalist("same", &[0.5, 0.6])],
            0.01,
        )
        .unwrap();
        assert_eq!(
            report.status,
            FinalistStatus::InsufficientDistinctCandidates
        );
    }
}
