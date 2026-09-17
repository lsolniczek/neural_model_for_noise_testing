use neural_preset_optimizer::brain_type::BrainType;
use neural_preset_optimizer::pipeline::{evaluate_preset_goal_scores, SimulationConfig};
use neural_preset_optimizer::preset::Preset;
use neural_preset_optimizer::replicated::{
    compare_finalist_scores, FinalistScores, FinalistStatus,
};
use neural_preset_optimizer::reproducibility::{SeedPanel, SeedPolicy};
use neural_preset_optimizer::scoring::GoalKind;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetSpec {
    id: String,
    path: PathBuf,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Thresholds {
    absolute_mean_error_p95_max: f64,
    median_spearman_min: f64,
    top1_or_reference_inconclusive_rate_min: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    schema: String,
    model_signature_schema_version: u32,
    seed_tree_revision: String,
    dsp_source_revision: String,
    run_seed: u64,
    panel: String,
    duration_secs: f32,
    reference_replicates: usize,
    candidate_panel_sizes: Vec<usize>,
    subpanels_per_size: usize,
    thresholds: Thresholds,
    profiles: Vec<String>,
    goals: Vec<String>,
    presets: Vec<PresetSpec>,
    subpanels: BTreeMap<String, Vec<Vec<usize>>>,
    selected_default_finalist_replicates: Option<usize>,
    result_summary_path: PathBuf,
    raw_results_path: PathBuf,
}

#[derive(Debug, Clone)]
struct Job {
    preset_index: usize,
    profile_index: usize,
    replicate_index: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawScore {
    preset_id: String,
    profile: String,
    goal: String,
    replicate_index: usize,
    score: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PanelSummary {
    replicates: usize,
    absolute_mean_error_p95: f64,
    median_spearman: f64,
    top1_or_reference_inconclusive_rate: f64,
    passes_absolute_mean_error: bool,
    passes_spearman: bool,
    passes_top1: bool,
    passes_all: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PairedDifferenceVarianceSummary {
    pairs: usize,
    sample_variance_median: f64,
    sample_variance_p95: f64,
    sample_variance_max: f64,
}

#[derive(Debug, Serialize)]
struct BenchmarkSummary {
    schema: &'static str,
    manifest_path: String,
    elapsed_seconds: f64,
    raw_rows: usize,
    reference_replicates: usize,
    paired_difference_variance: PairedDifferenceVarianceSummary,
    panel_summaries: Vec<PanelSummary>,
    selected_default_finalist_replicates: usize,
}

fn parse_args() -> (PathBuf, usize) {
    let mut manifest = PathBuf::from("benchmarks/p06/manifest.json");
    let mut threads = 1_usize;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--manifest" => {
                manifest = PathBuf::from(args.next().expect("--manifest requires a path"));
            }
            "--threads" => {
                threads = args
                    .next()
                    .expect("--threads requires a value")
                    .parse()
                    .expect("--threads must be an integer");
                assert!(threads > 0, "--threads must be at least 1");
            }
            "--help" | "-h" => {
                println!(
                    "Usage: p06_replication_benchmark [--manifest PATH] [--threads N]\n\
                     Runs the frozen 64-seed P-06 precision benchmark and updates its manifest."
                );
                std::process::exit(0);
            }
            unknown => panic!("unknown argument {unknown}"),
        }
    }
    (manifest, threads)
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn parse_profile(value: &str) -> BrainType {
    BrainType::from_str(value).unwrap_or_else(|| panic!("unknown profile {value}"))
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let values = values.collect::<Vec<_>>();
    values.iter().sum::<f64>() / values.len() as f64
}

fn percentile_nearest_rank(values: &mut [f64], probability: f64) -> f64 {
    values.sort_by(f64::total_cmp);
    let rank = (probability * values.len() as f64).ceil() as usize;
    values[rank.saturating_sub(1).min(values.len() - 1)]
}

fn sample_variance(values: &[f64]) -> f64 {
    assert!(values.len() >= 2);
    let average = values.iter().sum::<f64>() / values.len() as f64;
    values
        .iter()
        .map(|value| (value - average).powi(2))
        .sum::<f64>()
        / (values.len() - 1) as f64
}

fn ranks(values: &[f64]) -> Vec<f64> {
    let mut indexed = values.iter().copied().enumerate().collect::<Vec<_>>();
    indexed.sort_by(|first, second| first.1.total_cmp(&second.1));
    let mut result = vec![0.0; values.len()];
    let mut start = 0;
    while start < indexed.len() {
        let mut end = start + 1;
        while end < indexed.len() && indexed[end].1.to_bits() == indexed[start].1.to_bits() {
            end += 1;
        }
        let average_rank = (start + 1 + end) as f64 / 2.0;
        for &(original_index, _) in &indexed[start..end] {
            result[original_index] = average_rank;
        }
        start = end;
    }
    result
}

fn spearman(first: &[f64], second: &[f64]) -> f64 {
    let first = ranks(first);
    let second = ranks(second);
    let first_mean = first.iter().sum::<f64>() / first.len() as f64;
    let second_mean = second.iter().sum::<f64>() / second.len() as f64;
    let covariance = first
        .iter()
        .zip(second.iter())
        .map(|(a, b)| (a - first_mean) * (b - second_mean))
        .sum::<f64>();
    let first_ss = first
        .iter()
        .map(|value| (value - first_mean).powi(2))
        .sum::<f64>();
    let second_ss = second
        .iter()
        .map(|value| (value - second_mean).powi(2))
        .sum::<f64>();
    if first_ss <= 1e-15 || second_ss <= 1e-15 {
        if first == second {
            1.0
        } else {
            0.0
        }
    } else {
        covariance / (first_ss * second_ss).sqrt()
    }
}

fn write_raw_csv(path: &Path, rows: &[RawScore]) -> std::io::Result<()> {
    let mut output = String::from("preset_id,profile,goal,replicate_index,score\n");
    for row in rows {
        output.push_str(&format!(
            "{},{},{},{},{:.15}\n",
            row.preset_id, row.profile, row.goal, row.replicate_index, row.score
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, output)
}

fn main() {
    let (manifest_path, threads) = parse_args();
    let manifest_bytes = std::fs::read(&manifest_path).expect("cannot read benchmark manifest");
    let mut manifest: Manifest =
        serde_json::from_slice(&manifest_bytes).expect("invalid benchmark manifest");
    assert_eq!(manifest.schema, "nmm_p06_replication_benchmark_v1");
    assert_eq!(manifest.panel, "finalist");
    assert_eq!(manifest.reference_replicates, 64);
    assert_eq!(
        manifest.seed_tree_revision,
        neural_preset_optimizer::reproducibility::SEED_DERIVATION_REVISION
    );
    assert_eq!(
        manifest.dsp_source_revision,
        neural_preset_optimizer::model_signature::DSP_SOURCE_REVISION
    );

    let presets = manifest
        .presets
        .iter()
        .map(|spec| {
            let bytes = std::fs::read(&spec.path)
                .unwrap_or_else(|error| panic!("cannot read {}: {error}", spec.path.display()));
            assert_eq!(
                sha256(&bytes),
                spec.sha256,
                "preset hash mismatch for {}",
                spec.id
            );
            serde_json::from_slice::<Preset>(&bytes)
                .unwrap_or_else(|error| panic!("invalid preset {}: {error}", spec.path.display()))
        })
        .collect::<Vec<_>>();
    let profiles = manifest
        .profiles
        .iter()
        .map(|profile| parse_profile(profile))
        .collect::<Vec<_>>();
    let goals = manifest
        .goals
        .iter()
        .map(|goal| GoalKind::from_str(goal).unwrap_or_else(|| panic!("unknown goal {goal}")))
        .collect::<Vec<_>>();
    assert_eq!(goals, GoalKind::all());

    let jobs = (0..presets.len())
        .flat_map(|preset_index| {
            (0..profiles.len()).flat_map(move |profile_index| {
                (0..manifest.reference_replicates).map(move |replicate_index| Job {
                    preset_index,
                    profile_index,
                    replicate_index,
                })
            })
        })
        .collect::<Vec<_>>();
    let jobs = Arc::new(jobs);
    let presets = Arc::new(presets);
    let profiles = Arc::new(profiles);
    let next = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let results = Arc::new(Mutex::new(Vec::<RawScore>::with_capacity(
        jobs.len() * goals.len(),
    )));
    let start = Instant::now();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            let jobs = Arc::clone(&jobs);
            let presets = Arc::clone(&presets);
            let profiles = Arc::clone(&profiles);
            let next = Arc::clone(&next);
            let results = Arc::clone(&results);
            let manifest_ref = &manifest;
            let goals_ref = &goals;
            scope.spawn(move || loop {
                let index = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let Some(job) = jobs.get(index) else { break };
                let config = SimulationConfig {
                    duration_secs: manifest_ref.duration_secs,
                    brain_type: profiles[job.profile_index],
                    seed_policy: SeedPolicy::domain_separated(
                        manifest_ref.run_seed,
                        SeedPanel::Finalist,
                        job.replicate_index as u64,
                    ),
                    ..SimulationConfig::default()
                };
                let scores = evaluate_preset_goal_scores(&presets[job.preset_index], &config);
                let mut output = results.lock().expect("benchmark result lock poisoned");
                for &goal in goals_ref {
                    let score = scores
                        .iter()
                        .find(|(kind, _)| *kind == goal)
                        .map(|(_, score)| *score)
                        .expect("all goals were scored");
                    output.push(RawScore {
                        preset_id: manifest_ref.presets[job.preset_index].id.clone(),
                        profile: manifest_ref.profiles[job.profile_index].clone(),
                        goal: goal.to_string(),
                        replicate_index: job.replicate_index,
                        score,
                    });
                }
                if index % 25 == 0 {
                    eprintln!(
                        "completed {}/{} stochastic realizations",
                        index + 1,
                        jobs.len()
                    );
                }
            });
        }
    });
    let mut rows = Arc::try_unwrap(results)
        .expect("benchmark results still shared")
        .into_inner()
        .expect("benchmark result lock poisoned");
    rows.sort_by(|first, second| {
        first
            .profile
            .cmp(&second.profile)
            .then_with(|| first.goal.cmp(&second.goal))
            .then_with(|| first.preset_id.cmp(&second.preset_id))
            .then_with(|| first.replicate_index.cmp(&second.replicate_index))
    });
    write_raw_csv(&manifest.raw_results_path, &rows).expect("cannot write raw benchmark CSV");

    let lookup = |preset: &str, profile: &str, goal: &str| {
        let mut values = rows
            .iter()
            .filter(|row| row.preset_id == preset && row.profile == profile && row.goal == goal)
            .map(|row| (row.replicate_index, row.score))
            .collect::<Vec<_>>();
        values.sort_by_key(|(replicate, _)| *replicate);
        assert_eq!(values.len(), manifest.reference_replicates);
        values
            .into_iter()
            .map(|(_, score)| score)
            .collect::<Vec<_>>()
    };

    let mut paired_variances = Vec::new();
    for profile in &manifest.profiles {
        for goal in &manifest.goals {
            let scores = manifest
                .presets
                .iter()
                .map(|preset| lookup(&preset.id, profile, goal))
                .collect::<Vec<_>>();
            for first in 0..scores.len() {
                for second in (first + 1)..scores.len() {
                    let differences = scores[first]
                        .iter()
                        .zip(scores[second].iter())
                        .map(|(a, b)| a - b)
                        .collect::<Vec<_>>();
                    paired_variances.push(sample_variance(&differences));
                }
            }
        }
    }
    paired_variances.sort_by(f64::total_cmp);
    let paired_variance_count = paired_variances.len();
    let paired_variance_median = if paired_variance_count % 2 == 0 {
        let upper = paired_variance_count / 2;
        (paired_variances[upper - 1] + paired_variances[upper]) * 0.5
    } else {
        paired_variances[paired_variance_count / 2]
    };
    let paired_variance_max = *paired_variances.last().expect("at least one preset pair");
    let paired_variance_p95 = percentile_nearest_rank(&mut paired_variances, 0.95);
    let paired_difference_variance = PairedDifferenceVarianceSummary {
        pairs: paired_variance_count,
        sample_variance_median: paired_variance_median,
        sample_variance_p95: paired_variance_p95,
        sample_variance_max: paired_variance_max,
    };

    let mut panel_summaries = Vec::new();
    for &panel_size in &manifest.candidate_panel_sizes {
        let subpanels = manifest
            .subpanels
            .get(&panel_size.to_string())
            .expect("manifest is missing subpanels");
        assert_eq!(subpanels.len(), manifest.subpanels_per_size);
        let mut absolute_errors = Vec::new();
        let mut correlations = Vec::new();
        let mut top_matches = 0_usize;
        let mut top_total = 0_usize;
        for profile in &manifest.profiles {
            for goal in &manifest.goals {
                let full = manifest
                    .presets
                    .iter()
                    .map(|preset| lookup(&preset.id, profile, goal))
                    .collect::<Vec<_>>();
                let reference_means = full
                    .iter()
                    .map(|scores| mean(scores.iter().copied()))
                    .collect::<Vec<_>>();
                let reference = compare_finalist_scores(
                    manifest
                        .presets
                        .iter()
                        .zip(full.iter())
                        .map(|(preset, scores)| FinalistScores {
                            canonical_hash: preset.id.clone(),
                            scores: scores.clone(),
                        })
                        .collect(),
                    0.01,
                )
                .expect("reference finalist comparison failed");
                for subpanel in subpanels {
                    assert_eq!(subpanel.len(), panel_size);
                    let sub_means = full
                        .iter()
                        .map(|scores| mean(subpanel.iter().map(|index| scores[*index])))
                        .collect::<Vec<_>>();
                    absolute_errors.extend(
                        sub_means
                            .iter()
                            .zip(reference_means.iter())
                            .map(|(sample, reference)| (sample - reference).abs()),
                    );
                    correlations.push(spearman(&sub_means, &reference_means));
                    let mut leader = 0;
                    for index in 1..sub_means.len() {
                        if sub_means[index] > sub_means[leader]
                            || (sub_means[index].to_bits() == sub_means[leader].to_bits()
                                && manifest.presets[index].id < manifest.presets[leader].id)
                        {
                            leader = index;
                        }
                    }
                    let leader_id = &manifest.presets[leader].id;
                    let matches = match reference.status {
                        FinalistStatus::Unique => {
                            reference.observed_leader.as_deref() == Some(leader_id.as_str())
                        }
                        FinalistStatus::Inconclusive
                        | FinalistStatus::InsufficientDistinctCandidates => {
                            reference.inconclusive_set.contains(leader_id)
                        }
                    };
                    top_matches += usize::from(matches);
                    top_total += 1;
                }
            }
        }
        let error_p95 = percentile_nearest_rank(&mut absolute_errors, 0.95);
        correlations.sort_by(f64::total_cmp);
        let median_spearman = if correlations.len() % 2 == 0 {
            let upper = correlations.len() / 2;
            (correlations[upper - 1] + correlations[upper]) * 0.5
        } else {
            correlations[correlations.len() / 2]
        };
        let top_rate = top_matches as f64 / top_total as f64;
        let passes_absolute_mean_error =
            error_p95 <= manifest.thresholds.absolute_mean_error_p95_max;
        let passes_spearman = median_spearman >= manifest.thresholds.median_spearman_min;
        let passes_top1 = top_rate >= manifest.thresholds.top1_or_reference_inconclusive_rate_min;
        panel_summaries.push(PanelSummary {
            replicates: panel_size,
            absolute_mean_error_p95: error_p95,
            median_spearman,
            top1_or_reference_inconclusive_rate: top_rate,
            passes_absolute_mean_error,
            passes_spearman,
            passes_top1,
            passes_all: passes_absolute_mean_error && passes_spearman && passes_top1,
        });
    }
    let selected = panel_summaries
        .iter()
        .find(|summary| summary.passes_all)
        .map(|summary| summary.replicates)
        .unwrap_or(manifest.reference_replicates);
    assert_eq!(
        selected,
        neural_preset_optimizer::replicated::DEFAULT_REPLICATES,
        "update replicated::DEFAULT_REPLICATES to the calibrated benchmark result"
    );
    let summary = BenchmarkSummary {
        schema: "nmm_p06_replication_benchmark_result_v1",
        manifest_path: manifest_path.display().to_string(),
        elapsed_seconds: start.elapsed().as_secs_f64(),
        raw_rows: rows.len(),
        reference_replicates: manifest.reference_replicates,
        paired_difference_variance,
        panel_summaries,
        selected_default_finalist_replicates: selected,
    };
    if let Some(parent) = manifest.result_summary_path.parent() {
        std::fs::create_dir_all(parent).expect("cannot create result directory");
    }
    std::fs::write(
        &manifest.result_summary_path,
        serde_json::to_string_pretty(&summary).expect("summary serializes") + "\n",
    )
    .expect("cannot write benchmark summary");
    manifest.selected_default_finalist_replicates = Some(selected);
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("manifest serializes") + "\n",
    )
    .expect("cannot update benchmark manifest");
    println!(
        "selected default finalist_replicates={selected}; summary={}",
        manifest.result_summary_path.display()
    );
}
