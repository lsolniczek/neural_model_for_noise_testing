//! P-10 duration and search-panel reliability benchmark.
//!
//! The benchmark evaluates six deliberately different preset structures for
//! all nine goals using a common 64-seed panel. It compares candidate audio
//! durations with 60 seconds and N={1,4,8,16,32} search-panel prefixes with the
//! full panel. Raw observations, summary metrics, a manifest, and SHA-256
//! checksums are written to the requested output directory.

use clap::Parser;
use neural_preset_optimizer::brain_type::BrainType;
use neural_preset_optimizer::model_signature::EnvironmentRenderRevision;
use neural_preset_optimizer::movement::MovementConfig;
use neural_preset_optimizer::pipeline::SimulationConfig;
use neural_preset_optimizer::preset::{BinauralBeatPresetConfig, ModConfig, Preset};
use neural_preset_optimizer::replicated::evaluate_preset_replicated;
use neural_preset_optimizer::reproducibility::SeedPanel;
use neural_preset_optimizer::scoring::{Goal, GoalKind};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const P95_LIMIT: f64 = 0.01;
const SPEARMAN_LIMIT: f64 = 0.90;
const TOP1_LIMIT: f64 = 0.95;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value = "benchmarks/p10/reliability")]
    output_dir: PathBuf,
    #[arg(long, default_value_t = 64)]
    replicates: usize,
    #[arg(long, default_value_t = 0x5031_305f_7632)]
    seed: u64,
    #[arg(long, value_delimiter = ',', default_value = "3,6,10,12,20,30,60")]
    durations: Vec<f32>,
}

#[derive(Clone)]
struct Fixture {
    name: &'static str,
    preset: Preset,
}

#[derive(Debug, Clone, Serialize)]
struct ReliabilityMetric {
    candidate: String,
    p95_absolute_score_error: f64,
    spearman: f64,
    top1_agreement: f64,
    passes: bool,
}

#[derive(Serialize)]
struct Manifest {
    schema: &'static str,
    generated_at: String,
    run_seed: u64,
    seed_panel: &'static str,
    replicates: usize,
    durations_secs: Vec<f32>,
    fixtures: Vec<&'static str>,
    goals: Vec<String>,
    goal_brain_mapping: BTreeMap<String, String>,
    environment_render_revision: &'static str,
    thresholds: BTreeMap<&'static str, f64>,
    duration_metrics: Vec<ReliabilityMetric>,
    search_panel_metrics: Vec<ReliabilityMetric>,
    selected_duration_secs: Option<f32>,
    selected_search_replicates: Option<usize>,
    files_sha256: BTreeMap<String, String>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.replicates < 2 {
        return Err("--replicates must be at least 2".into());
    }
    if !args.durations.iter().any(|d| (*d - 60.0).abs() < 1e-6) {
        return Err("--durations must include the 60 second reference".into());
    }
    if args.durations.iter().any(|d| !d.is_finite() || *d <= 2.0) {
        return Err("all durations must be finite and greater than the 2 second warm-up".into());
    }
    fs::create_dir_all(&args.output_dir)?;
    let fixtures = fixtures();
    let goals = goals();
    let raw_path = args.output_dir.join("raw_scores.csv");
    let mut raw = String::from("duration_secs,fixture,goal,brain_type,replicate,score\n");
    let mut scores: BTreeMap<String, Vec<f64>> = BTreeMap::new();

    for &duration in &args.durations {
        for fixture in &fixtures {
            for &(goal_kind, brain_type) in &goals {
                let mut config = SimulationConfig {
                    duration_secs: duration,
                    warmup_discard_secs: 2.0,
                    brain_type,
                    environment_render_revision: EnvironmentRenderRevision::DspOnlyV2,
                    ..SimulationConfig::default()
                };
                config.seed_policy =
                    neural_preset_optimizer::reproducibility::SeedPolicy::domain_separated(
                        args.seed,
                        SeedPanel::BenchmarkDevelopment,
                        0,
                    );
                let aggregate = evaluate_preset_replicated(
                    &fixture.preset,
                    &Goal::new(goal_kind),
                    &config,
                    args.seed,
                    SeedPanel::BenchmarkDevelopment,
                    args.replicates,
                )?;
                let key = key(duration, fixture.name, goal_kind);
                let values = aggregate
                    .replicates
                    .iter()
                    .enumerate()
                    .map(|(replicate, observation)| {
                        raw.push_str(&format!(
                            "{duration:.1},{},{},{},{replicate},{:.17}\n",
                            fixture.name, goal_kind, brain_type, observation.score
                        ));
                        observation.score
                    })
                    .collect();
                scores.insert(key, values);
            }
        }
    }
    fs::write(&raw_path, raw)?;

    let duration_metrics = args
        .durations
        .iter()
        .copied()
        .filter(|duration| (*duration - 60.0).abs() > 1e-6)
        .map(|duration| duration_metric(duration, &fixtures, &goals, &scores))
        .collect::<Vec<_>>();
    let selected_duration_secs = duration_metrics
        .iter()
        .filter(|metric| metric.passes)
        .filter_map(|metric| metric.candidate.strip_suffix('s')?.parse::<f32>().ok())
        .min_by(f32::total_cmp)
        .or(Some(60.0));

    let reference_duration = selected_duration_secs.unwrap_or(60.0);
    let search_panel_metrics = [1usize, 4, 8, 16, 32]
        .into_iter()
        .filter(|n| *n < args.replicates)
        .map(|n| search_panel_metric(n, reference_duration, &fixtures, &goals, &scores))
        .collect::<Vec<_>>();
    let selected_search_replicates = search_panel_metrics
        .iter()
        .filter(|metric| metric.passes)
        .filter_map(|metric| metric.candidate.strip_prefix("N=")?.parse::<usize>().ok())
        .min()
        .or(Some(args.replicates));

    let summary_path = args.output_dir.join("summary.csv");
    let mut summary =
        String::from("kind,candidate,p95_absolute_score_error,spearman,top1_agreement,passes\n");
    for (kind, metrics) in [
        ("duration", &duration_metrics),
        ("search_panel", &search_panel_metrics),
    ] {
        for metric in metrics {
            summary.push_str(&format!(
                "{kind},{},{:.17},{:.17},{:.17},{}\n",
                metric.candidate,
                metric.p95_absolute_score_error,
                metric.spearman,
                metric.top1_agreement,
                metric.passes
            ));
        }
    }
    fs::write(&summary_path, summary)?;

    let mut hashes = BTreeMap::new();
    hashes.insert("raw_scores.csv".to_string(), sha256_file(&raw_path)?);
    hashes.insert("summary.csv".to_string(), sha256_file(&summary_path)?);
    let mut thresholds = BTreeMap::new();
    thresholds.insert("p95_absolute_score_error_max", P95_LIMIT);
    thresholds.insert("spearman_min", SPEARMAN_LIMIT);
    thresholds.insert("top1_agreement_min", TOP1_LIMIT);
    let mapping = goals
        .iter()
        .map(|(goal, brain)| (goal.to_string(), brain.to_string()))
        .collect();
    let manifest = Manifest {
        schema: "nmm_p10_reliability_benchmark_v1",
        generated_at: chrono::Utc::now().to_rfc3339(),
        run_seed: args.seed,
        seed_panel: "benchmark_development",
        replicates: args.replicates,
        durations_secs: args.durations,
        fixtures: fixtures.iter().map(|f| f.name).collect(),
        goals: goals.iter().map(|(g, _)| g.to_string()).collect(),
        goal_brain_mapping: mapping,
        environment_render_revision: "dsp_only_v2",
        thresholds,
        duration_metrics,
        search_panel_metrics,
        selected_duration_secs,
        selected_search_replicates,
        files_sha256: hashes,
    };
    fs::write(
        args.output_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    println!("selected duration: {:?}s", manifest.selected_duration_secs);
    println!(
        "selected search panel: N={:?}",
        manifest.selected_search_replicates
    );
    Ok(())
}

fn duration_metric(
    duration: f32,
    fixtures: &[Fixture],
    goals: &[(GoalKind, BrainType)],
    scores: &BTreeMap<String, Vec<f64>>,
) -> ReliabilityMetric {
    let candidate = aggregate_vector(duration, fixtures, goals, scores, usize::MAX);
    let reference = aggregate_vector(60.0, fixtures, goals, scores, usize::MAX);
    metric(
        format!("{duration}s"),
        &candidate,
        &reference,
        fixtures.len(),
    )
}

fn search_panel_metric(
    n: usize,
    duration: f32,
    fixtures: &[Fixture],
    goals: &[(GoalKind, BrainType)],
    scores: &BTreeMap<String, Vec<f64>>,
) -> ReliabilityMetric {
    let candidate = aggregate_vector(duration, fixtures, goals, scores, n);
    let reference = aggregate_vector(duration, fixtures, goals, scores, usize::MAX);
    metric(format!("N={n}"), &candidate, &reference, fixtures.len())
}

fn aggregate_vector(
    duration: f32,
    fixtures: &[Fixture],
    goals: &[(GoalKind, BrainType)],
    scores: &BTreeMap<String, Vec<f64>>,
    take: usize,
) -> Vec<f64> {
    goals
        .iter()
        .flat_map(|(goal, _)| {
            fixtures.iter().map(move |fixture| {
                let values = &scores[&key(duration, fixture.name, *goal)];
                let n = take.min(values.len());
                values[..n].iter().sum::<f64>() / n as f64
            })
        })
        .collect()
}

fn metric(
    label: String,
    candidate: &[f64],
    reference: &[f64],
    group_size: usize,
) -> ReliabilityMetric {
    let mut errors = candidate
        .iter()
        .zip(reference)
        .map(|(a, b)| (a - b).abs())
        .collect::<Vec<_>>();
    errors.sort_by(f64::total_cmp);
    let p95_index = ((errors.len() - 1) as f64 * 0.95).ceil() as usize;
    let p95 = errors[p95_index.min(errors.len() - 1)];
    let spearman = spearman(candidate, reference);
    let groups = candidate.len() / group_size;
    let top1_matches = (0..groups)
        .filter(|group| {
            let start = group * group_size;
            argmax(&candidate[start..start + group_size])
                == argmax(&reference[start..start + group_size])
        })
        .count();
    let top1 = top1_matches as f64 / groups as f64;
    ReliabilityMetric {
        candidate: label,
        p95_absolute_score_error: p95,
        spearman,
        top1_agreement: top1,
        passes: p95 <= P95_LIMIT && spearman >= SPEARMAN_LIMIT && top1 >= TOP1_LIMIT,
    }
}

fn spearman(a: &[f64], b: &[f64]) -> f64 {
    let ra = ranks(a);
    let rb = ranks(b);
    let mean_a = ra.iter().sum::<f64>() / ra.len() as f64;
    let mean_b = rb.iter().sum::<f64>() / rb.len() as f64;
    let covariance = ra
        .iter()
        .zip(&rb)
        .map(|(x, y)| (x - mean_a) * (y - mean_b))
        .sum::<f64>();
    let va = ra.iter().map(|x| (x - mean_a).powi(2)).sum::<f64>();
    let vb = rb.iter().map(|x| (x - mean_b).powi(2)).sum::<f64>();
    if va == 0.0 || vb == 0.0 {
        0.0
    } else {
        covariance / (va * vb).sqrt()
    }
}

fn ranks(values: &[f64]) -> Vec<f64> {
    let mut order = values.iter().copied().enumerate().collect::<Vec<_>>();
    order.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut ranks = vec![0.0; values.len()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start + 1;
        while end < order.len() && order[end].1.to_bits() == order[start].1.to_bits() {
            end += 1;
        }
        let rank = (start + end - 1) as f64 / 2.0;
        for &(index, _) in &order[start..end] {
            ranks[index] = rank;
        }
        start = end;
    }
    ranks
}

fn argmax(values: &[f64]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(index, _)| index)
        .unwrap_or(0)
}

fn key(duration: f32, fixture: &str, goal: GoalKind) -> String {
    format!("{duration:.3}|{fixture}|{goal}")
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let bytes = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn active_base() -> Preset {
    let mut preset = Preset::default();
    preset.objects[0].active = true;
    preset.objects[0].color = 2;
    preset.objects[0].volume = 0.55;
    preset.objects[0].x = -1.2;
    preset.objects[0].z = 1.8;
    preset.source_count = 1;
    preset
}

fn fixtures() -> Vec<Fixture> {
    let static_noise = active_base();

    let mut binaural = active_base();
    binaural.binaural_beat = BinauralBeatPresetConfig {
        enabled: true,
        center_frequency_hz: 320.0,
        beat_frequency_hz: 8.0,
        gain_db: -38.0,
        lower_frequency_ear: 0,
    };

    let mut periodic = active_base();
    periodic.objects[0].bass_mod = ModConfig {
        kind: 1,
        param_a: 0.5,
        param_b: 0.7,
        param_c: 0.0,
    };
    periodic.objects[0].satellite_mod = ModConfig {
        kind: 5,
        param_a: 10.0,
        param_b: 0.35,
        param_c: 0.45,
    };

    let mut stochastic = active_base();
    stochastic.objects[0].bass_mod = ModConfig {
        kind: 3,
        param_a: 2.0,
        param_b: 240.0,
        param_c: 0.22,
    };
    stochastic.objects[0].satellite_mod = ModConfig {
        kind: 6,
        param_a: 4.0,
        param_b: 0.5,
        param_c: 180.0,
    };

    let mut moving = active_base();
    moving.objects[0].spread = 0.72;
    moving.objects[0].movement = MovementConfig {
        kind: 1,
        radius: 1.8,
        speed: 0.7,
        phase: 0.3,
        ..MovementConfig::default()
    };

    let mut room_mix = active_base();
    room_mix.environment = 3;
    room_mix.objects[0].reverb_send = 0.75;
    room_mix.objects[1].active = true;
    room_mix.objects[1].color = 7;
    room_mix.objects[1].volume = 0.35;
    room_mix.objects[1].x = 1.4;
    room_mix.objects[1].z = 2.1;
    room_mix.source_count = 2;

    vec![
        Fixture {
            name: "static_noise",
            preset: static_noise,
        },
        Fixture {
            name: "binaural_only",
            preset: binaural,
        },
        Fixture {
            name: "periodic_modulation",
            preset: periodic,
        },
        Fixture {
            name: "stochastic_random_pulse",
            preset: stochastic,
        },
        Fixture {
            name: "moving_spread",
            preset: moving,
        },
        Fixture {
            name: "room_mixed",
            preset: room_mix,
        },
    ]
}

fn goals() -> Vec<(GoalKind, BrainType)> {
    vec![
        (GoalKind::Focus, BrainType::Adhd),
        (GoalKind::Shield, BrainType::Adhd),
        (GoalKind::Ignition, BrainType::Adhd),
        (GoalKind::Sleep, BrainType::Aging),
        (GoalKind::DeepRelaxation, BrainType::Anxious),
        (GoalKind::Meditation, BrainType::HighAlpha),
        (GoalKind::Flow, BrainType::HighAlpha),
        (GoalKind::DeepWork, BrainType::Normal),
        (GoalKind::Isolation, BrainType::Normal),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rank_metrics_accept_identical_vectors() {
        let values = [0.1, 0.4, 0.2, 0.3];
        let result = metric("same".to_string(), &values, &values, 2);
        assert_eq!(result.p95_absolute_score_error, 0.0);
        assert!((result.spearman - 1.0).abs() < 1e-12);
        assert_eq!(result.top1_agreement, 1.0);
        assert!(result.passes);
    }
}
