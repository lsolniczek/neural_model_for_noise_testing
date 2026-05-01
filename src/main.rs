mod acoustic_score;
mod analyze_preset;
mod auditory;
mod brain_type;
mod disturb;
mod export;
mod movement;
mod neural;
mod optimizer;
mod pipeline;
mod preset;
mod regression_tests;
mod scoring;
mod surrogate;
mod validate;

use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};
use std::time::Instant;

use brain_type::BrainType;
use optimizer::DifferentialEvolution;
use pipeline::{
    evaluate_preset, evaluate_preset_detailed, validate_analysis_window, DetailedSimulationResult,
    SimulationConfig,
};
use preset::Preset;
use scoring::{Goal, GoalKind, MetricStatus};

#[derive(Parser)]
#[command(name = "neural-preset-optimizer")]
#[command(about = "Neural model-based optimizer and evaluator for noise generator presets")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EvaluateFeatureFlags {
    assr: bool,
    thalamic_gate: bool,
    cet: bool,
    phys_gate: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run evolutionary optimization to find the best preset for a goal.
    Optimize {
        /// Optimization goal
        #[arg(long, default_value = "focus")]
        goal: String,

        /// Maximum generations
        #[arg(long, default_value_t = 100)]
        generations: usize,

        /// Population size
        #[arg(long, default_value_t = 30)]
        population: usize,

        /// Audio duration per evaluation (seconds)
        #[arg(long, default_value_t = 3.0)]
        duration: f32,

        /// Output JSON path (auto-generated if omitted)
        #[arg(long)]
        output: Option<PathBuf>,

        /// Random seed for reproducibility
        #[arg(long, default_value_t = 42)]
        seed: u64,

        /// DE mutation scale factor
        #[arg(long, default_value_t = 0.7)]
        de_f: f64,

        /// DE crossover rate
        #[arg(long, default_value_t = 0.8)]
        de_cr: f64,

        /// Convergence threshold (stop if fitness std < this)
        #[arg(long, default_value_t = 0.001)]
        convergence: f64,

        /// Brain type profile
        #[arg(long, default_value = "normal")]
        brain_type: String,

        /// Seed population from an existing preset JSON (explores around it)
        #[arg(long)]
        init_preset: Option<PathBuf>,

        /// Enable Cortical Envelope Tracking (Priority 13).
        /// ON by default for physiologically correct GABA_B gain modulation.
        #[arg(long, default_value_t = true)]
        cet: bool,

        /// Enable the physiological thalamic gate (Priority 9)
        #[arg(long = "phys-gate", default_value_t = false)]
        phys_gate: bool,

        /// Enable the Phase 5 fused scalar score during optimization.
        /// Currently supported only for `shield` and `isolation`.
        /// Implies acoustic analysis and is incompatible with surrogate
        /// scoring and CSV logging until those score contracts are updated.
        #[arg(long = "acoustic-score-fusion", default_value_t = false)]
        acoustic_score_fusion: bool,

        /// Enable surrogate-assisted pre-screening (Priority 14).
        /// Uses a trained MLP to rank candidates before selective real evaluation.
        /// Only validated real-pipeline scores are allowed to replace DE parents.
        #[arg(long, default_value_t = false)]
        surrogate: bool,

        /// Path to surrogate weights file
        #[arg(long, default_value = "surrogate_weights.bin")]
        surrogate_weights: PathBuf,

        /// Log every real-pipeline evaluation to a CSV file for surrogate training.
        /// Appends to the file (doesn't overwrite), so multiple runs accumulate.
        /// Format matches generate-data output — can be concatenated directly.
        #[arg(long)]
        log_evaluations: Option<PathBuf>,

        /// Number of top surrogate candidates to validate with the real pipeline
        #[arg(long, default_value_t = 5)]
        surrogate_k: usize,
    },

    /// Evaluate an existing preset against goal(s) and brain type(s).
    Evaluate {
        /// Path to preset JSON file
        preset: PathBuf,

        /// Goal to evaluate against (or "all")
        #[arg(long, default_value = "all")]
        goal: String,

        /// Brain type profile (or "all")
        #[arg(long, default_value = "normal")]
        brain_type: String,

        /// Audio duration per evaluation (seconds)
        #[arg(long, default_value_t = 10.0)]
        duration: f32,

        /// Enable ASSR transfer function (auditory pathway filtering)
        #[arg(long, default_value_t = false)]
        assr: bool,

        /// Disable ASSR transfer function (auditory pathway filtering)
        #[arg(long = "no-assr", conflicts_with = "assr", default_value_t = false)]
        no_assr: bool,

        /// Enable thalamic gate (arousal-dependent filtering).
        /// ON by default — required for physiologically correct arousal sensitivity.
        /// Use --no-thalamic-gate to disable.
        #[arg(long, default_value_t = true)]
        thalamic_gate: bool,

        /// Disable thalamic gate (arousal-dependent filtering)
        #[arg(
            long = "no-thalamic-gate",
            conflicts_with = "thalamic_gate",
            default_value_t = false
        )]
        no_thalamic_gate: bool,

        /// Enable Cortical Envelope Tracking (Priority 13).
        /// Splits each band into slow (≤10 Hz) and fast (>10 Hz) paths,
        /// bypasses ASSR on the slow path, and engages the slow GABA_B
        /// gain modulation in JR. ON by default — required for theta-alpha
        /// coexistence. Use --no-cet to disable.
        #[arg(long, default_value_t = true)]
        cet: bool,

        /// Disable Cortical Envelope Tracking
        #[arg(long = "no-cet", conflicts_with = "cet", default_value_t = false)]
        no_cet: bool,

        /// Enable the physiological thalamic gate (Priority 9). Replaces
        /// the linear heuristic gate with an ion-channel-based TC cell
        /// (Bazhenov 2002 / Paul 2016) where K+ leak conductance is the
        /// arousal knob. Sigmoidal shape derived from real ion-channel
        /// dynamics. Takes precedence over --thalamic-gate when both set.
        #[arg(long = "phys-gate", default_value_t = false)]
        phys_gate: bool,

        /// Print acoustic subscore metrics (Phase 4). Leaves the scalar NMM
        /// score unchanged and is only shown for single goal/brain evaluation.
        #[arg(long = "acoustic-score", default_value_t = false)]
        acoustic_score: bool,

        /// Enable Phase 5 acoustic/NMM score fusion for supported goals
        /// (`shield`, `isolation`) during evaluation only. Implies acoustic
        /// analysis and leaves optimize/surrogate behavior unchanged.
        #[arg(long = "acoustic-score-fusion", default_value_t = false)]
        acoustic_score_fusion: bool,
    },

    /// Run disturbance resilience test — inject acoustic spike and measure recovery.
    Disturb {
        /// Path to preset JSON file
        preset: PathBuf,

        /// Brain type profile
        #[arg(long, default_value = "normal")]
        brain_type: String,

        /// Time of spike injection (seconds into analysis window)
        #[arg(long, default_value_t = 4.0)]
        spike_time: f64,

        /// Duration of spike (seconds)
        #[arg(long, default_value_t = 0.05)]
        spike_duration: f64,

        /// Spike amplitude gain (0.0–1.0)
        #[arg(long, default_value_t = 0.8)]
        spike_gain: f64,

        /// Total simulation duration (seconds)
        #[arg(long, default_value_t = 15.0)]
        duration: f32,
    },

    /// Run neural model validation tests (frequency tracking, bifurcation, etc.)
    Validate,

    /// Generate training data for the surrogate model (Priority 14a).
    /// Samples random presets, evaluates with the real pipeline, writes CSV.
    GenerateData {
        /// Output CSV path
        #[arg(long, default_value = "training_data.csv")]
        output: PathBuf,

        /// Number of random presets to sample
        #[arg(long, default_value_t = 1000)]
        count: usize,

        /// Goals to evaluate (comma-separated, or "all")
        #[arg(long, default_value = "all")]
        goals: String,

        /// Brain type (or "all")
        #[arg(long, default_value = "normal")]
        brain_type: String,

        /// Audio duration per evaluation (seconds)
        #[arg(long, default_value_t = 3.0)]
        duration: f32,

        /// Number of parallel threads
        #[arg(long, default_value_t = 4)]
        threads: usize,

        /// Enable the physiological thalamic gate for generated rows.
        /// This lets surrogate datasets cover the `optimize --phys-gate` mode.
        #[arg(long = "phys-gate", default_value_t = false)]
        phys_gate: bool,

        /// Random seed
        #[arg(long, default_value_t = 42)]
        seed: u64,
    },
}

fn bar(value: f64, width: usize) -> String {
    let filled = (value * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("{}{}", "\u{2588}".repeat(filled), "\u{2591}".repeat(empty))
}

fn status_icon(status: &MetricStatus) -> &'static str {
    match status {
        MetricStatus::Pass => "\u{2713}",
        MetricStatus::Warn => "~",
        MetricStatus::Fail => "\u{2717}",
    }
}

fn resolve_evaluate_feature_flags(
    assr: bool,
    no_assr: bool,
    thalamic_gate: bool,
    no_thalamic_gate: bool,
    cet: bool,
    no_cet: bool,
    phys_gate: bool,
) -> EvaluateFeatureFlags {
    EvaluateFeatureFlags {
        assr: assr && !no_assr,
        thalamic_gate: thalamic_gate && !no_thalamic_gate,
        cet: cet && !no_cet,
        phys_gate,
    }
}

fn build_eval_config(
    duration: f32,
    brain_type: BrainType,
    flags: EvaluateFeatureFlags,
    acoustic_scoring_enabled: bool,
    acoustic_score_fusion_enabled: bool,
) -> SimulationConfig {
    SimulationConfig {
        duration_secs: duration,
        brain_type,
        assr_enabled: flags.assr,
        thalamic_gate_enabled: flags.thalamic_gate,
        cet_enabled: flags.cet,
        physiological_thalamic_gate_enabled: flags.phys_gate,
        acoustic_scoring_enabled,
        acoustic_score_fusion_enabled,
        ..SimulationConfig::default()
    }
}

fn build_generate_data_config(
    duration: f32,
    brain_type: BrainType,
    phys_gate: bool,
) -> SimulationConfig {
    SimulationConfig {
        duration_secs: duration,
        brain_type,
        physiological_thalamic_gate_enabled: phys_gate,
        ..SimulationConfig::default()
    }
}

fn build_optimize_config(
    duration: f32,
    brain_type: BrainType,
    cet: bool,
    phys_gate: bool,
    acoustic_score_fusion_enabled: bool,
) -> SimulationConfig {
    SimulationConfig {
        duration_secs: duration,
        brain_type,
        cet_enabled: cet,
        physiological_thalamic_gate_enabled: phys_gate,
        acoustic_scoring_enabled: acoustic_score_fusion_enabled,
        acoustic_score_fusion_enabled,
        ..SimulationConfig::default()
    }
}

fn ensure_analysis_window_or_exit(command_name: &str, duration: f32, warmup_discard_secs: f32) {
    if let Err(message) = validate_analysis_window(duration, warmup_discard_secs) {
        eprintln!(
            "Invalid --duration for {command_name}: {message}. Increase --duration above {:.1}s.",
            warmup_discard_secs
        );
        std::process::exit(2);
    }
}

fn validate_optimize_acoustic_mode(
    goal: &Goal,
    acoustic_score_fusion: bool,
    use_surrogate: bool,
    log_evaluations_path: Option<&Path>,
) -> Result<(), String> {
    if !acoustic_score_fusion {
        return Ok(());
    }
    if !goal.supports_acoustic_fusion() {
        return Err(format!(
            "--acoustic-score-fusion is currently supported only for shield and isolation; got {}",
            goal.kind()
        ));
    }
    if use_surrogate {
        return Err(
            "--acoustic-score-fusion is incompatible with --surrogate until the surrogate score contract is updated"
                .to_string(),
        );
    }
    if log_evaluations_path.is_some() {
        return Err(
            "--acoustic-score-fusion is incompatible with --log-evaluations until the surrogate CSV contract records fused-score runs"
                .to_string(),
        );
    }
    Ok(())
}

fn surrogate_validation_mask(
    candidate_count: usize,
    surrogate_k: usize,
    generation: usize,
) -> Vec<bool> {
    let mut validate = vec![false; candidate_count];
    let k = surrogate_k.min(candidate_count);

    for flag in validate.iter_mut().take(k) {
        *flag = true;
    }

    if candidate_count > k {
        let exploration_rank = k + (generation * 7 + 13) % (candidate_count - k);
        validate[exploration_rank] = true;
    }

    validate
}

fn surrogate_csv_header() -> String {
    let genome_cols: Vec<String> = (0..surrogate::GENOME_DIM)
        .map(|i| format!("g{i}"))
        .collect();
    format!(
        "{},{}",
        genome_cols.join(","),
        surrogate::CSV_METADATA_COLUMNS.join(",")
    )
}

fn surrogate_csv_row(
    genome: &[f64],
    goal_kind: GoalKind,
    brain_type: BrainType,
    config: &SimulationConfig,
    score: f64,
) -> String {
    assert_eq!(
        genome.len(),
        surrogate::GENOME_DIM,
        "surrogate CSV genome length mismatch: got {}, expected {}",
        genome.len(),
        surrogate::GENOME_DIM
    );

    let genome_str: Vec<String> = genome.iter().map(|v| format!("{v:.6}")).collect();
    let goal_id = GoalKind::all()
        .iter()
        .position(|&g| g == goal_kind)
        .unwrap_or(0);
    let bt_id = BrainType::all()
        .iter()
        .position(|&b| b == brain_type)
        .unwrap_or(0);

    format!(
        "{},{goal_id},{bt_id},{},{},{},{},{score:.6}",
        genome_str.join(","),
        if config.assr_enabled { 1 } else { 0 },
        if config.thalamic_gate_enabled { 1 } else { 0 },
        if config.cet_enabled { 1 } else { 0 },
        if config.physiological_thalamic_gate_enabled {
            1
        } else {
            0
        },
    )
}

fn reevaluate_best_preset(
    best_genome: &[f64],
    goal: &Goal,
    sim_config: &SimulationConfig,
    spread_per_slot: &[f32; preset::MAX_OBJECTS],
) -> (Preset, pipeline::SimulationResult) {
    let best_preset = Preset::from_genome_with_spread(best_genome, spread_per_slot);
    let best_result = evaluate_preset(&best_preset, goal, sim_config);
    (best_preset, best_result)
}

fn export_best_genome(
    output_path: &Path,
    best_genome: &[f64],
    goal: &Goal,
    goal_kind: GoalKind,
    generations: usize,
    duration_secs: f32,
    sim_config: &SimulationConfig,
    spread_per_slot: &[f32; preset::MAX_OBJECTS],
) -> std::io::Result<(Preset, pipeline::SimulationResult)> {
    let (best_preset, best_result) =
        reevaluate_best_preset(best_genome, goal, sim_config, spread_per_slot);
    export::export_preset(
        &best_preset,
        &best_result,
        goal_kind,
        generations,
        duration_secs,
        output_path,
    )?;
    Ok((best_preset, best_result))
}

fn diagnose_detailed_result(goal: &Goal, result: &DetailedSimulationResult) -> scoring::Diagnosis {
    goal.diagnose(
        &result.fhn,
        &result.bilateral.combined,
        result.summary.brightness,
        result.summary.alpha_asymmetry,
        result.summary.performance.plv,
        result.summary.performance.envelope_plv,
        Some(result.summary.performance),
    )
}

fn evaluate_score_matrix(
    preset: &Preset,
    goals: &[GoalKind],
    brain_types: &[BrainType],
    duration: f32,
    flags: EvaluateFeatureFlags,
    acoustic_score_fusion: bool,
) -> Vec<Vec<f64>> {
    brain_types
        .iter()
        .map(|bt| {
            goals
                .iter()
                .map(|goal_kind| {
                    let goal = Goal::new(*goal_kind);
                    let sim_config = build_eval_config(
                        duration,
                        *bt,
                        flags,
                        acoustic_score_fusion,
                        acoustic_score_fusion,
                    );
                    evaluate_preset(preset, &goal, &sim_config).score
                })
                .collect()
        })
        .collect()
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Optimize {
            goal,
            generations,
            population,
            duration,
            output,
            seed,
            de_f,
            de_cr,
            convergence,
            brain_type,
            init_preset,
            cet,
            phys_gate,
            acoustic_score_fusion,
            log_evaluations,
            surrogate,
            surrogate_weights,
            surrogate_k,
        } => {
            run_optimize(
                &goal,
                generations,
                population,
                duration,
                output,
                seed,
                de_f,
                de_cr,
                convergence,
                &brain_type,
                init_preset.as_deref(),
                cet,
                phys_gate,
                acoustic_score_fusion,
                surrogate,
                &surrogate_weights,
                surrogate_k,
                log_evaluations.as_deref(),
            );
        }
        Commands::Evaluate {
            preset,
            goal,
            brain_type,
            duration,
            assr,
            no_assr,
            thalamic_gate,
            no_thalamic_gate,
            cet,
            no_cet,
            phys_gate,
            acoustic_score,
            acoustic_score_fusion,
        } => {
            let flags = resolve_evaluate_feature_flags(
                assr,
                no_assr,
                thalamic_gate,
                no_thalamic_gate,
                cet,
                no_cet,
                phys_gate,
            );
            run_evaluate(
                &preset,
                &goal,
                &brain_type,
                duration,
                flags,
                acoustic_score,
                acoustic_score_fusion,
            );
        }
        Commands::Disturb {
            preset,
            brain_type,
            spike_time,
            spike_duration,
            spike_gain,
            duration,
        } => {
            run_disturb_cmd(
                &preset,
                &brain_type,
                spike_time,
                spike_duration,
                spike_gain,
                duration,
            );
        }
        Commands::Validate => {
            validate::run_all();
        }
        Commands::GenerateData {
            output,
            count,
            goals,
            brain_type,
            duration,
            threads,
            phys_gate,
            seed,
        } => {
            run_generate_data(
                &output,
                count,
                &goals,
                &brain_type,
                duration,
                threads,
                phys_gate,
                seed,
            );
        }
    }
}

// ── Optimize ─────────────────────────────────────────────────────────────────

fn run_optimize(
    goal_str: &str,
    generations: usize,
    population: usize,
    duration: f32,
    output: Option<PathBuf>,
    seed: u64,
    de_f: f64,
    de_cr: f64,
    convergence: f64,
    brain_type_str: &str,
    init_preset: Option<&std::path::Path>,
    cet: bool,
    phys_gate: bool,
    acoustic_score_fusion: bool,
    use_surrogate: bool,
    surrogate_weights_path: &Path,
    surrogate_k: usize,
    log_evaluations_path: Option<&Path>,
) {
    ensure_analysis_window_or_exit(
        "optimize",
        duration,
        SimulationConfig::default().warmup_discard_secs,
    );

    let goal_kind = GoalKind::from_str(goal_str).unwrap_or_else(|| {
        eprintln!(
            "Unknown goal: '{}'. Valid: deep_relaxation, focus, sleep, isolation, meditation, deep_work",
            goal_str
        );
        std::process::exit(1);
    });
    let goal = Goal::new(goal_kind);
    if let Err(message) = validate_optimize_acoustic_mode(
        &goal,
        acoustic_score_fusion,
        use_surrogate,
        log_evaluations_path,
    ) {
        eprintln!("Invalid optimize configuration: {message}");
        std::process::exit(2);
    }

    let bt = BrainType::from_str(brain_type_str).unwrap_or_else(|| {
        eprintln!(
            "Unknown brain type: '{}'. Valid: normal, high_alpha, adhd, aging, anxious",
            brain_type_str
        );
        std::process::exit(1);
    });

    let sim_config =
        build_optimize_config(duration, bt, cet, phys_gate, acoustic_score_fusion);

    println!();
    println!("  Neural Preset Optimizer");
    println!("  \u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
    println!("  Goal:           {}", goal_kind);
    println!("  Brain type:     {} ({})", bt, bt.description());
    println!("  Population:     {}", population);
    println!("  Max generations:{}", generations);
    println!("  Audio duration: {:.1}s per evaluation", duration);
    println!("  Seed:           {}", seed);
    if cet {
        println!("  CET:            enabled");
    }
    if phys_gate {
        println!("  Phys gate:      enabled");
    }
    if acoustic_score_fusion {
        println!("  Acoustic fusion: enabled ({goal_kind} objective)");
    }

    // Set up evaluation logger for surrogate training data collection.
    // Appends every real-pipeline evaluation to a CSV file. The file is
    // created with a header if it doesn't exist, or appended to if it does.
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    let eval_logger: Option<Arc<Mutex<std::fs::File>>> = log_evaluations_path.map(|path| {
        let file_exists = path.exists();
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap_or_else(|e| {
                eprintln!("Failed to open log file: {e}");
                std::process::exit(1);
            });
        if !file_exists {
            // Write CSV header
            let mut f = file;
            writeln!(f, "{}", surrogate_csv_header()).unwrap();
            println!("  Log evals:      {} (new file)", path.display());
            Arc::new(Mutex::new(f))
        } else {
            println!("  Log evals:      {} (appending)", path.display());
            Arc::new(Mutex::new(file))
        }
    });

    let log_eval = |genome: &[f64], score: f64| {
        if let Some(ref logger) = eval_logger {
            let mut f = logger.lock().unwrap();
            let _ = writeln!(
                f,
                "{}",
                surrogate_csv_row(genome, goal_kind, bt, &sim_config, score)
            );
        }
    };

    // Load surrogate model if requested (Priority 14).
    let surrogate_model = if use_surrogate {
        match surrogate::SurrogateModel::load(surrogate_weights_path) {
            Ok(model) => {
                println!("  Surrogate:      enabled (top-{surrogate_k} pre-screening)");
                println!("  Weights:        {}", surrogate_weights_path.display());
                Some(model)
            }
            Err(e) => {
                eprintln!("  WARNING: Failed to load surrogate weights: {e}");
                eprintln!("           Falling back to full pipeline evaluation.");
                None
            }
        }
    } else {
        None
    };
    println!();

    let bounds = Preset::bounds();
    let discrete_dims = Preset::discrete_gene_indices();
    let mut de =
        DifferentialEvolution::with_discrete(bounds, population, de_f, de_cr, seed, discrete_dims);

    // Seed population from an existing preset if provided. Spread is not part
    // of the genome (the surrogate contract requires a stable 230-dim input),
    // so we capture it as a per-slot side-channel here and re-apply it on every
    // `from_genome` call below. Without this, seed presets that use spread
    // would silently lose those values on the first round-trip and the
    // optimizer would "improve" a structurally different preset.
    let mut spread_per_slot = [0.0_f32; preset::MAX_OBJECTS];
    if let Some(path) = init_preset {
        let json = std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("Failed to read init preset: {}", e);
            std::process::exit(1);
        });
        let preset: Preset = serde_json::from_str(&json).unwrap_or_else(|e| {
            eprintln!("Failed to parse init preset: {}", e);
            std::process::exit(1);
        });
        for (i, obj) in preset.objects.iter().take(preset::MAX_OBJECTS).enumerate() {
            spread_per_slot[i] = obj.spread.clamp(0.0, 1.0);
        }
        let genome = preset.to_genome();
        de.seed_from_genome(&genome, 0.15);
        println!("  Init preset:    {}", path.display());
        let nonzero_spread: Vec<String> = spread_per_slot
            .iter()
            .enumerate()
            .filter(|(_, &s)| s > 0.0)
            .map(|(i, &s)| format!("obj{i}={s:.2}"))
            .collect();
        if !nonzero_spread.is_empty() {
            println!(
                "  Spread (preserved from seed, not searched by DE): {}",
                nonzero_spread.join(", ")
            );
        }
    }

    let start = Instant::now();

    // ── Initial population evaluation ───────────────────────────────────────
    println!("  Evaluating initial population...");
    let pending = de.pending_evaluations();
    for (idx, genome) in &pending {
        let preset = Preset::from_genome_with_spread(genome, &spread_per_slot);
        let result = evaluate_preset(&preset, &goal, &sim_config);
        log_eval(genome, result.score);
        de.report_fitness(*idx, result.score);
    }

    println!(
        "  Initial best: {:.4}  mean: {:.4}",
        de.best().fitness,
        de.mean_fitness()
    );
    println!();

    // ── Evolution loop ──────────────────────────────────────────────────────
    let mut stale_count = 0;
    let mut prev_best = de.best().fitness;

    for gen in 0..generations {
        let trials = de.generate_trials();

        if let Some(ref surr) = surrogate_model {
            // Surrogate-assisted mode (Priority 14d):
            // 1. Score ALL candidates with the surrogate (~µs each)
            // 2. Rank by surrogate score, take top-K
            // 3. Also include 1 random candidate for exploration
            // 4. Validate only those K+1 with the real pipeline
            // 5. Only validated real scores are allowed to replace DE parents
            let mut scored: Vec<(usize, Vec<f64>, f32)> = trials
                .iter()
                .map(|(idx, genome)| {
                    let input = surrogate::SurrogateModel::build_input(
                        genome,
                        goal_kind,
                        bt,
                        sim_config.assr_enabled,
                        sim_config.thalamic_gate_enabled,
                        sim_config.cet_enabled,
                        sim_config.physiological_thalamic_gate_enabled,
                    );
                    let surr_score = surr.predict(&input);
                    (*idx, genome.clone(), surr_score)
                })
                .collect();

            // Sort descending by surrogate score
            scored.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

            // Top-K + 1 random exploration candidate
            let validate_mask = surrogate_validation_mask(scored.len(), surrogate_k, gen);

            for (rank, &(target_idx, ref trial_genome, surr_score)) in scored.iter().enumerate() {
                if validate_mask[rank] {
                    // Validate with real pipeline
                    let preset = Preset::from_genome_with_spread(trial_genome, &spread_per_slot);
                    let result = evaluate_preset(&preset, &goal, &sim_config);
                    log_eval(trial_genome, result.score);
                    de.report_trial_result(target_idx, trial_genome.clone(), result.score);
                } else {
                    let _ = surr_score;
                }
            }
        } else {
            // Standard mode: evaluate ALL trials with real pipeline
            for (target_idx, trial_genome) in trials {
                let preset = Preset::from_genome_with_spread(&trial_genome, &spread_per_slot);
                let result = evaluate_preset(&preset, &goal, &sim_config);
                log_eval(&trial_genome, result.score);
                de.report_trial_result(target_idx, trial_genome, result.score);
            }
        }

        let best_fitness = de.best().fitness;
        let mean_fitness = de.mean_fitness();
        let fitness_std = de.fitness_std();

        // Progress display
        if gen % 5 == 0 || gen == generations - 1 {
            let elapsed = start.elapsed().as_secs_f64();
            println!(
                "  Gen {:>4}  best: {:.4}  mean: {:.4}  std: {:.4}  [{:.0}s]",
                gen + 1,
                best_fitness,
                mean_fitness,
                fitness_std,
                elapsed,
            );
        }

        // Convergence check
        if (best_fitness - prev_best).abs() < 1e-6 {
            stale_count += 1;
        } else {
            stale_count = 0;
        }
        prev_best = best_fitness;

        if fitness_std < convergence && gen > 10 {
            println!();
            println!(
                "  Converged at generation {} (fitness std: {:.6})",
                gen + 1,
                fitness_std
            );
            break;
        }
        if stale_count > 20 {
            println!();
            println!(
                "  Stagnated at generation {} (no improvement for 20 generations)",
                gen + 1
            );
            break;
        }
    }

    let elapsed = start.elapsed();

    // ── Final evaluation of best preset ─────────────────────────────────────
    let best_genome = de.best().genome.clone();
    let (best_preset, best_result) =
        reevaluate_best_preset(&best_genome, &goal, &sim_config, &spread_per_slot);

    println!();
    println!("  \u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
    println!("  Result");
    println!("  \u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
    println!("  Goal:            {}", goal_kind);
    println!("  Brain type:      {}", bt);
    if acoustic_score_fusion {
        println!("  Score:           {:.4} (fused)", best_result.score);
    } else {
        println!("  Score:           {:.4}", best_result.score);
    }
    println!("  Generations:     {}", de.generation());
    println!("  Time:            {:.1}s", elapsed.as_secs_f64());
    println!();
    println!("  Neural Response:");
    println!(
        "    Delta  (0.5-4 Hz):  {} {:.3}",
        bar(best_result.delta_power, 20),
        best_result.delta_power
    );
    println!(
        "    Theta  (4-8 Hz):    {} {:.3}",
        bar(best_result.theta_power, 20),
        best_result.theta_power
    );
    println!(
        "    Alpha  (8-13 Hz):   {} {:.3}",
        bar(best_result.alpha_power, 20),
        best_result.alpha_power
    );
    println!(
        "    Beta   (13-30 Hz):  {} {:.3}",
        bar(best_result.beta_power, 20),
        best_result.beta_power
    );
    println!(
        "    Gamma  (30-50 Hz):  {} {:.3}",
        bar(best_result.gamma_power, 20),
        best_result.gamma_power
    );
    println!();
    println!("    Dominant freq:    {:.1} Hz", best_result.dominant_freq);
    println!(
        "    FHN firing rate:  {:.1} spikes/s",
        best_result.fhn_firing_rate
    );
    if best_result.fhn_isi_cv.is_nan() {
        println!("    FHN ISI CV:       N/A (< 3 spikes)");
    } else {
        println!("    FHN ISI CV:       {:.3}", best_result.fhn_isi_cv);
    }
    println!();
    println!("  Performance Vector:");
    println!(
        "    Spectral centroid:  {:.1} Hz",
        best_result.performance.spectral_centroid
    );
    if let Some(er) = best_result.performance.entrainment_ratio {
        println!("    Entrainment ratio:  {:.3}", er);
    } else {
        println!("    Entrainment ratio:  N/A (no NeuralLFO)");
    }
    if let Some(ei) = best_result.performance.ei_stability {
        println!("    E/I stability (CV): {:.3}", ei);
    } else {
        println!("    E/I stability:      N/A (G=0)");
    }
    println!();

    if acoustic_score_fusion {
        if let Some(acoustic) = &best_result.acoustic_score {
            print_acoustic_score_summary(acoustic);
        }
    }

    // Preset summary
    print_preset_summary(&best_preset);

    // ── Export ───────────────────────────────────────────────────────────────
    let output_path = output.unwrap_or_else(|| {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        PathBuf::from(format!("preset_{}_{}.json", goal_kind, ts))
    });

    match export_best_genome(
        &output_path,
        &best_genome,
        &goal,
        goal_kind,
        de.generation(),
        duration,
        &sim_config,
        &spread_per_slot,
    ) {
        Ok(_) => {
            println!();
            println!("  Exported: {}", output_path.display());
        }
        Err(e) => {
            eprintln!("  Export failed: {}", e);
        }
    }

    println!();
}

// ── Evaluate ─────────────────────────────────────────────────────────────────

fn run_evaluate(
    preset_path: &PathBuf,
    goal_str: &str,
    brain_type_str: &str,
    duration: f32,
    flags: EvaluateFeatureFlags,
    acoustic_score: bool,
    acoustic_score_fusion: bool,
) {
    ensure_analysis_window_or_exit(
        "evaluate",
        duration,
        SimulationConfig::default().warmup_discard_secs,
    );

    // Load preset from JSON
    let json = std::fs::read_to_string(preset_path).unwrap_or_else(|e| {
        eprintln!(
            "Failed to read preset file '{}': {}",
            preset_path.display(),
            e
        );
        std::process::exit(1);
    });

    let exported: serde_json::Value = serde_json::from_str(&json).unwrap_or_else(|e| {
        eprintln!("Failed to parse preset JSON: {}", e);
        std::process::exit(1);
    });

    // Support both raw Preset and exported PresetExport format
    let preset: Preset = if exported.get("preset").is_some() {
        serde_json::from_value(exported["preset"].clone()).unwrap_or_else(|e| {
            eprintln!("Failed to parse preset from export format: {}", e);
            std::process::exit(1);
        })
    } else {
        serde_json::from_value(exported).unwrap_or_else(|e| {
            eprintln!("Failed to parse preset: {}", e);
            std::process::exit(1);
        })
    };

    // Parse goals
    let goals: Vec<GoalKind> = if goal_str.to_lowercase() == "all" {
        GoalKind::all().to_vec()
    } else {
        vec![GoalKind::from_str(goal_str).unwrap_or_else(|| {
            eprintln!(
                "Unknown goal: '{}'. Valid: deep_relaxation, focus, sleep, isolation, meditation, all",
                goal_str
            );
            std::process::exit(1);
        })]
    };

    // Parse brain types
    let brain_types: Vec<BrainType> = if brain_type_str.to_lowercase() == "all" {
        BrainType::all().to_vec()
    } else {
        vec![BrainType::from_str(brain_type_str).unwrap_or_else(|| {
            eprintln!(
                "Unknown brain type: '{}'. Valid: normal, high_alpha, adhd, aging, anxious, all",
                brain_type_str
            );
            std::process::exit(1);
        })]
    };

    let is_matrix = goals.len() > 1 || brain_types.len() > 1;

    println!();
    println!("  Preset Evaluation");
    println!("  \u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
    println!("  Preset: {}", preset_path.display());
    println!("  Audio:  {:.1}s per evaluation", duration);
    println!(
        "  Features: assr={}  thalamic_gate={}  cet={}  phys_gate={}",
        flags.assr, flags.thalamic_gate, flags.cet, flags.phys_gate
    );
    if acoustic_score {
        println!("  Acoustic score: enabled (evaluate-only)");
    }
    if acoustic_score_fusion {
        println!("  Acoustic fusion: enabled for shield/isolation only");
    }
    println!();

    if is_matrix {
        // ── Matrix mode ─────────────────────────────────────────────────────
        print_comparison_matrix(
            &preset,
            &goals,
            &brain_types,
            duration,
            flags,
            acoustic_score_fusion,
        );
        if acoustic_score {
            println!("  Note: acoustic metrics are shown only for single goal/brain evaluate.");
            println!();
        }
        if acoustic_score_fusion {
            println!("  Note: acoustic fusion affects only shield/isolation cells in the matrix.");
            println!();
        }
    } else {
        // ── Single evaluation with full diagnosis ───────────────────────────
        let bt = brain_types[0];
        let goal_kind = goals[0];
        let goal = Goal::new(goal_kind);
        let show_acoustic = acoustic_score || acoustic_score_fusion;
        let sim_config = build_eval_config(
            duration,
            bt,
            flags,
            show_acoustic,
            acoustic_score_fusion,
        );
        let detailed = evaluate_preset_detailed(&preset, &goal, &sim_config);
        let result = &detailed.summary;
        let diagnosis = diagnose_detailed_result(&goal, &detailed);
        let fusion_applied = result
            .acoustic_score
            .as_ref()
            .and_then(|acoustic| acoustic.fused_score_preview)
            .is_some();

        println!("  Brain type: {} ({})", bt, bt.description());
        println!("  Goal:       {}", goal_kind);
        if fusion_applied {
            println!("  Score:      {:.4} (fused)", result.score);
        } else {
            println!("  Score:      {:.4}", result.score);
        }
        if acoustic_score_fusion && !goal.supports_acoustic_fusion() {
            println!("  Acoustic fusion: requested, but this goal still uses legacy NMM scoring.");
        }
        println!();

        // Tonotopic Band Energies
        println!("  Tonotopic Input:");
        println!("    {:<22} {:<10} {}", "Band", "Energy", "");
        println!("    {}", "\u{2500}".repeat(40));
        for (b, label) in auditory::BAND_LABELS.iter().enumerate() {
            let pct = result.band_energy_fractions[b] * 100.0;
            let bar_str = bar(result.band_energy_fractions[b], 15);
            println!("    {:<22} {} {:.1}%", label, bar_str, pct);
        }
        println!();

        // EEG Band Powers
        println!("  EEG Band Powers:");
        println!(
            "    {:<8} {:<8} {:<8} {:<6} {}",
            "Band", "Target", "Actual", "Status", ""
        );
        println!("    {}", "\u{2500}".repeat(50));
        for band in &diagnosis.bands {
            let detail = match band.expectation {
                scoring::BandExpectation::Range(min, ideal, max) => {
                    if band.actual < min {
                        "below range"
                    } else if band.actual > max {
                        "above range"
                    } else if (band.actual - ideal).abs() <= (max - min) * 0.25 {
                        "in range"
                    } else {
                        "within range"
                    }
                }
                scoring::BandExpectation::Flat(_) => {
                    if (band.actual - 0.2).abs() < 0.05 {
                        "near uniform"
                    } else {
                        "deviates from uniform"
                    }
                }
                scoring::BandExpectation::High => {
                    if band.actual >= 0.25 {
                        "in range"
                    } else {
                        "below range"
                    }
                }
                scoring::BandExpectation::Low => {
                    if band.actual <= 0.15 {
                        "in range"
                    } else {
                        "above range"
                    }
                }
                scoring::BandExpectation::Neutral => "neutral",
            };
            println!(
                "    {:<8} {:<10} {:<8.3} {} {}  ({})",
                band.name,
                band.expectation,
                band.actual,
                status_icon(&band.status),
                band.status,
                detail,
            );
        }
        println!();

        // FHN Neuron Response
        println!("  FHN Neuron Response:");
        println!(
            "    {:<18} {:<16} {:<10} {}",
            "Metric", "Target", "Actual", "Status"
        );
        println!("    {}", "\u{2500}".repeat(55));

        let rate_range = format!(
            "{:.1}-{:.1} sp/s",
            diagnosis.firing_rate_range.0, diagnosis.firing_rate_range.1
        );
        let rate_detail = if matches!(diagnosis.firing_rate_status, MetricStatus::Pass) {
            "in range"
        } else if diagnosis.firing_rate < diagnosis.firing_rate_range.0 {
            "too slow"
        } else {
            "too fast"
        };
        println!(
            "    {:<18} {:<16} {:<10.1} {} {}  ({})",
            "Firing rate",
            rate_range,
            diagnosis.firing_rate,
            status_icon(&diagnosis.firing_rate_status),
            diagnosis.firing_rate_status,
            rate_detail,
        );

        if let Some(target_cv) = diagnosis.target_isi_cv {
            let cv_target = format!("CV ~ {:.2}", target_cv);
            if diagnosis.isi_cv.is_nan() {
                println!(
                    "    {:<18} {:<16} {:<10} {} {}  ({})",
                    "ISI regularity",
                    cv_target,
                    "N/A",
                    status_icon(&diagnosis.isi_status),
                    diagnosis.isi_status,
                    "< 3 spikes",
                );
            } else {
                let cv_detail = if diagnosis.isi_cv < 0.1 {
                    "very regular"
                } else if diagnosis.isi_cv < 0.2 {
                    "regular"
                } else {
                    "irregular"
                };
                println!(
                    "    {:<18} {:<16} {:<10.3} {} {}  ({})",
                    "ISI regularity",
                    cv_target,
                    diagnosis.isi_cv,
                    status_icon(&diagnosis.isi_status),
                    diagnosis.isi_status,
                    cv_detail,
                );
            }
        }
        println!();

        println!(
            "  Dominant frequency: {:.1} Hz ({} range)",
            diagnosis.dominant_freq,
            diagnosis.dominant_band_name()
        );

        // Bilateral hemispheric info
        let bi = &detailed.bilateral;
        println!();
        println!("  Bilateral Cortical Model:");
        let lh_bp = &bi.left_band_powers;
        let rh_bp = &bi.right_band_powers;
        let coupling = bt.bilateral_params();
        println!(
            "    Callosal: coupling={:.0}%, delay={:.0}ms, contra={:.0}%",
            coupling.callosal_coupling * 100.0,
            coupling.callosal_delay_s * 1000.0,
            coupling.contralateral_ratio * 100.0,
        );
        println!();
        println!(
            "    {:<8} {:<20} {:<20} {:<20}",
            "Band", "Left (fast α/β)", "Right (slow δ/θ)", "Combined"
        );
        println!("    {}", "\u{2500}".repeat(72));
        let cb = &bi.combined.band_powers.normalized();
        let bands = [
            ("Delta", lh_bp.delta, rh_bp.delta, cb.delta),
            ("Theta", lh_bp.theta, rh_bp.theta, cb.theta),
            ("Alpha", lh_bp.alpha, rh_bp.alpha, cb.alpha),
            ("Beta", lh_bp.beta, rh_bp.beta, cb.beta),
            ("Gamma", lh_bp.gamma, rh_bp.gamma, cb.gamma),
        ];
        for (name, lv, rv, cv) in &bands {
            println!(
                "    {:<8} {:>5.1}%  {}   {:>5.1}%  {}   {:>5.1}%  {}",
                name,
                lv * 100.0,
                bar(*lv, 10),
                rv * 100.0,
                bar(*rv, 10),
                cv * 100.0,
                bar(*cv, 10),
            );
        }
        println!();
        println!(
            "    Dominant freq:  Left {:.2} Hz   Right {:.2} Hz   Combined {:.2} Hz",
            bi.left_dominant_freq, bi.right_dominant_freq, bi.combined.dominant_freq
        );
        let asym_label = if bi.alpha_asymmetry.abs() < 0.05 {
            "balanced"
        } else if bi.alpha_asymmetry > 0.0 {
            "left-dominant"
        } else {
            "right-dominant"
        };
        println!(
            "    Alpha asymmetry: {:+.3} ({})",
            bi.alpha_asymmetry, asym_label
        );
        println!();

        println!("  Performance Vector:");
        println!(
            "    Spectral centroid:  {:.1} Hz",
            result.performance.spectral_centroid
        );
        if let Some(er) = result.performance.entrainment_ratio {
            println!("    Entrainment ratio:  {:.3}", er);
        } else {
            println!("    Entrainment ratio:  N/A (no NeuralLFO)");
        }
        if let Some(ei) = result.performance.ei_stability {
            println!("    E/I stability (CV): {:.3}", ei);
        } else {
            println!("    E/I stability:      N/A (G=0)");
        }
        println!();

        let brightness_label = if result.brightness > 0.7 {
            "bright (white-like)"
        } else if result.brightness > 0.4 {
            "moderate (pink-like)"
        } else if result.brightness > 0.15 {
            "dark (brown-like)"
        } else {
            "very dark"
        };
        println!(
            "  Spectral brightness: {:.2} ({})",
            result.brightness, brightness_label
        );
        println!();

        if show_acoustic {
            if let Some(acoustic) = &result.acoustic_score {
                print_acoustic_score_summary(acoustic);
            } else {
                println!("  Acoustic subscore: unavailable");
                println!();
            }
        }

        // Verdict
        let verdict_detail = match diagnosis.verdict {
            scoring::Verdict::Good => "neural rhythms align well with goal",
            scoring::Verdict::Ok => "partial alignment, some metrics off-target",
            scoring::Verdict::Poor => "poor alignment, most metrics off-target",
        };
        println!(
            "  Verdict: {} \u{2014} {}",
            diagnosis.verdict, verdict_detail
        );
        println!();

        // Preset summary
        print_preset_summary(&preset);
    }

    println!();
}

fn print_comparison_matrix(
    preset: &Preset,
    goals: &[GoalKind],
    brain_types: &[BrainType],
    duration: f32,
    flags: EvaluateFeatureFlags,
    acoustic_score_fusion: bool,
) {
    let scores = evaluate_score_matrix(
        preset,
        goals,
        brain_types,
        duration,
        flags,
        acoustic_score_fusion,
    );

    // Header
    print!("  {:<12}", "Brain Type");
    for g in goals {
        print!("  {:<12}", format!("{}", g));
    }
    println!();
    print!("  {}", "\u{2500}".repeat(12));
    for _ in goals {
        print!("\u{2500}\u{2500}{}", "\u{2500}".repeat(12));
    }
    println!();

    // Rows
    for (row_idx, bt) in brain_types.iter().enumerate() {
        print!("  {:<12}", format!("{}", bt));

        for score in &scores[row_idx] {
            let icon = if *score >= 0.75 {
                "\u{2713}"
            } else if *score >= 0.50 {
                "~"
            } else {
                "\u{2717}"
            };
            print!("  {} {:<10.4}", icon, score);
        }
        println!();
    }

    println!();

    // Legend
    println!("  \u{2713} >= 0.75 (good)   ~ >= 0.50 (ok)   \u{2717} < 0.50 (poor)");
    println!();
}

fn print_acoustic_score_summary(acoustic: &crate::acoustic_score::AcousticScoreResult) {
    let features = &acoustic.features;
    println!("  Acoustic Subscore:");
    println!("    {:<24} {:<10}", "Metric", "Value");
    println!("    {}", "\u{2500}".repeat(38));
    if let Some(level_db) = features.broadband_level_db {
        println!("    {:<24} {:>8.2} dB", "Broadband level", level_db);
    }
    if let Some(ratio) = features.speech_band_ratio {
        println!("    {:<24} {:>8.3}", "Speech-band ratio", ratio);
    }
    if let Some(depth) = features.modulation_depth {
        println!("    {:<24} {:>8.3}", "Modulation depth", depth);
    }
    if let Some(sharpness) = features.sharpness_proxy {
        println!("    {:<24} {:>8.3}", "Sharpness proxy", sharpness);
    }
    if let Some(intelligibility) = acoustic.intelligibility_proxy {
        println!("    {:<24} {:>8.3}", "Intelligibility", intelligibility);
    }
    if let Some(privacy) = acoustic.speech_privacy {
        println!("    {:<24} {:>8.3}", "Speech privacy", privacy);
    }
    if let Some(comfort) = acoustic.comfort_score {
        println!("    {:<24} {:>8.3}", "Comfort score", comfort);
    }
    if let Some(acoustic_goal_score) = acoustic.acoustic_goal_score {
        println!(
            "    {:<24} {:>8.3}",
            "Acoustic goal score",
            acoustic_goal_score
        );
    }
    if let Some(legacy_nmm_score) = acoustic.legacy_nmm_score {
        println!("    {:<24} {:>8.3}", "Legacy NMM score", legacy_nmm_score);
    }
    if let Some(fused_score) = acoustic.fused_score_preview {
        println!("    {:<24} {:>8.3}", "Fused score", fused_score);
    }
    println!();
}

fn print_preset_summary(preset: &Preset) {
    let color_names = [
        "White", "Pink", "Brown", "Green", "Grey", "Black", "SSN", "Blue",
    ];
    let env_names = [
        "AnechoicChamber",
        "FocusRoom",
        "OpenLounge",
        "VastSpace",
        "DeepSanctuary",
    ];
    let mod_kind_names = [
        "Flat",
        "SineLfo",
        "Breathing",
        "Stochastic",
        "NeuralLfo",
        "Isochronic",
        "RandomPulse",
    ];

    println!("  Preset Configuration:");
    println!("    Master gain:    {:.2}", preset.master_gain);
    let mode_str = if preset.spatial_mode == 0 {
        "Stereo"
    } else {
        "Immersive"
    };
    println!(
        "    Spatial mode:   {} (sources: {})",
        mode_str, preset.source_count
    );
    println!(
        "    Anchor:         {} @ {:.2}",
        color_names[preset.anchor_color as usize], preset.anchor_volume
    );
    println!(
        "    Environment:    {}",
        env_names[preset.environment as usize]
    );
    println!();

    for (i, obj) in preset.objects.iter().enumerate() {
        if !obj.active {
            continue;
        }
        println!(
            "    Object {}: {} @ ({:+.1}, {:+.1}, {:+.1})  vol={:.2}  reverb={:.2}",
            i, color_names[obj.color as usize], obj.x, obj.y, obj.z, obj.volume, obj.reverb_send,
        );
        if obj.spread > 0.01 {
            println!("      Spread:    {:.2}", obj.spread);
        }
        if obj.bass_mod.kind > 0 {
            println!(
                "      Bass:      {} (a={:.2}, b={:.2}, c={:.2})",
                mod_kind_names[obj.bass_mod.kind as usize],
                obj.bass_mod.param_a,
                obj.bass_mod.param_b,
                obj.bass_mod.param_c,
            );
        }
        if obj.satellite_mod.kind > 0 {
            println!(
                "      Satellite: {} (a={:.2}, b={:.2}, c={:.2})",
                mod_kind_names[obj.satellite_mod.kind as usize],
                obj.satellite_mod.param_a,
                obj.satellite_mod.param_b,
                obj.satellite_mod.param_c,
            );
        }
        let pattern = obj.movement.pattern();
        if pattern != movement::MovementPattern::Static {
            let mv = &obj.movement;
            match pattern {
                movement::MovementPattern::DepthBreathing => {
                    println!(
                        "      Movement:  {} (speed={:.2}, z={:.1}–{:.1}, reverb={:.2}–{:.2})",
                        pattern.label(),
                        mv.speed,
                        mv.depth_min,
                        mv.depth_max,
                        mv.reverb_min,
                        mv.reverb_max,
                    );
                }
                _ => {
                    println!(
                        "      Movement:  {} (radius={:.2}, speed={:.2}, phase={:.2})",
                        pattern.label(),
                        mv.radius,
                        mv.speed,
                        mv.phase,
                    );
                }
            }
        }
    }
}

// ── Disturb ─────────────────────────────────────────────────────────────────

fn run_disturb_cmd(
    preset_path: &PathBuf,
    brain_type_str: &str,
    spike_time: f64,
    spike_duration: f64,
    spike_gain: f64,
    duration: f32,
) {
    ensure_analysis_window_or_exit(
        "disturb",
        duration,
        disturb::DisturbConfig::default().warmup_discard_secs,
    );

    // Load preset
    let json = std::fs::read_to_string(preset_path).unwrap_or_else(|e| {
        eprintln!(
            "Failed to read preset file '{}': {}",
            preset_path.display(),
            e
        );
        std::process::exit(1);
    });
    let exported: serde_json::Value = serde_json::from_str(&json).unwrap_or_else(|e| {
        eprintln!("Failed to parse preset JSON: {}", e);
        std::process::exit(1);
    });
    let preset: Preset = if exported.get("preset").is_some() {
        serde_json::from_value(exported["preset"].clone()).unwrap_or_else(|e| {
            eprintln!("Failed to parse preset from export format: {}", e);
            std::process::exit(1);
        })
    } else {
        serde_json::from_value(exported).unwrap_or_else(|e| {
            eprintln!("Failed to parse preset: {}", e);
            std::process::exit(1);
        })
    };

    let bt = BrainType::from_str(brain_type_str).unwrap_or_else(|| {
        eprintln!(
            "Unknown brain type: '{}'. Valid: normal, high_alpha, adhd, aging, anxious",
            brain_type_str
        );
        std::process::exit(1);
    });

    // Validate spike timing
    let analysis_duration = duration as f64 - 2.0; // subtract warmup
    if spike_time + spike_duration > analysis_duration {
        eprintln!(
            "Spike at {:.2}s + {:.3}s exceeds analysis window ({:.1}s). Increase --duration.",
            spike_time, spike_duration, analysis_duration
        );
        std::process::exit(1);
    }

    let config = disturb::DisturbConfig {
        spike_time_s: spike_time,
        spike_duration_s: spike_duration,
        spike_gain,
        brain_type: bt,
        duration_secs: duration,
        warmup_discard_secs: 2.0,
        window_s: 0.5,
        hop_s: 0.05,
        acoustic_scoring_enabled: false,
    };

    let start = Instant::now();
    let result = disturb::run_disturb(&preset, &config);
    let elapsed = start.elapsed();

    // ── Display ─────────────────────────────────────────────────────────
    println!();
    println!("  \u{2550}\u{2550}\u{2550} Disturbance Resilience Test \u{2550}\u{2550}\u{2550}");
    println!();
    println!("  Brain type:      {}", bt);
    println!(
        "  Spike:           {:.0}ms white noise burst at t={:.1}s, gain={:.2}",
        spike_duration * 1000.0,
        spike_time,
        spike_gain
    );
    if let Some(tf) = result.target_freq {
        println!("  Target LFO:      {:.1} Hz", tf);
    }
    println!("  Brightness:      {:.2}", result.brightness);
    println!(
        "  Duration:        {:.1}s ({:.2}s elapsed)",
        duration,
        elapsed.as_secs_f64()
    );
    println!();

    // Baseline
    println!("  \u{2500}\u{2500} Baseline (0 \u{2013} {:.1}s) \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}", spike_time);
    println!(
        "    Dominant freq:       {:.2} Hz",
        result.baseline_dominant_freq
    );
    println!(
        "    Spectral centroid:   {:.2} Hz",
        result.baseline_centroid
    );
    if let Some(ent) = result.baseline_entrainment {
        println!("    Entrainment ratio:   {:.3}", ent);
    }
    println!();

    // Spike impact
    println!("  \u{2500}\u{2500} Spike Impact \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}");
    if let (Some(nadir), Some(baseline)) = (result.nadir_entrainment, result.baseline_entrainment) {
        let drop_pct = if baseline > 1e-10 {
            (1.0 - nadir / baseline) * 100.0
        } else {
            0.0
        };
        println!(
            "    Entrainment nadir:   {:.3} ({:.0}% drop at t={:.2}s)",
            nadir, drop_pct, result.nadir_time
        );
    }
    println!(
        "    Peak freq deviation: \u{00b1}{:.2} Hz",
        result.peak_freq_deviation
    );
    println!();

    // Recovery
    println!("  \u{2500}\u{2500} Recovery \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}");
    match result.recovery_50_ms {
        Some(ms) => println!("    50% recovery:        {:.0} ms", ms),
        None => println!("    50% recovery:        NOT RECOVERED"),
    }
    match result.recovery_90_ms {
        Some(ms) => println!("    90% recovery:        {:.0} ms", ms),
        None => println!("    90% recovery:        NOT RECOVERED"),
    }

    // Final state (last 2s)
    let final_windows: Vec<&disturb::WindowMetrics> = result
        .windows
        .iter()
        .filter(|w| w.time_s > (duration as f64 - 2.0 - 2.0)) // last 2s of analysis
        .collect();
    if !final_windows.is_empty() {
        let final_ent: Option<f64> = {
            let vals: Vec<f64> = final_windows
                .iter()
                .filter_map(|w| w.entrainment_ratio)
                .collect();
            if vals.is_empty() {
                None
            } else {
                Some(vals.iter().sum::<f64>() / vals.len() as f64)
            }
        };
        let final_freq =
            final_windows.iter().map(|w| w.dominant_freq).sum::<f64>() / final_windows.len() as f64;

        println!();
        println!(
            "    Final entrainment:   {}",
            match final_ent {
                Some(e) => format!("{:.3}", e),
                None => "N/A".to_string(),
            }
        );
        println!("    Final dominant freq: {:.2} Hz", final_freq);

        // Entrainment resilience (original, requires LFO target)
        if let (Some(base), Some(fin)) = (result.baseline_entrainment, final_ent) {
            let preservation = if base > 1e-10 {
                (fin / base).min(1.0)
            } else {
                0.0
            };
            let speed_score = match result.recovery_90_ms {
                Some(ms) if ms < 5000.0 => 1.0 - (ms / 5000.0),
                Some(_) => 0.0,
                None => 0.0,
            };
            let resilience = 0.6 * preservation + 0.4 * speed_score;
            println!();
            println!(
                "    \u{2550}\u{2550} Entrainment Resilience: {:.2} \u{2550}\u{2550}",
                resilience
            );
            println!(
                "       (preservation={:.2}, speed={:.2})",
                preservation, speed_score
            );
        }
    }

    // Spectral resilience (Priority 15 — works for ALL presets including binaural)
    println!();
    println!("  \u{2500}\u{2500} Spectral Resilience (Priority 15) \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}");
    println!("    BPPR (band preservation):  {:.3}", result.bppr);
    println!(
        "    Spectral recovery 50%:     {}",
        match result.spectral_recovery_50_ms {
            Some(ms) => format!("{:.0} ms", ms),
            None => "NOT RECOVERED".to_string(),
        }
    );
    println!(
        "    Spectral recovery 90%:     {}",
        match result.spectral_recovery_90_ms {
            Some(ms) => format!("{:.0} ms", ms),
            None => "NOT RECOVERED".to_string(),
        }
    );
    println!("    SCDI (centroid deviation):  {:.2} Hz", result.scdi_hz);
    println!();
    println!(
        "    \u{2550}\u{2550} Spectral Resilience Score: {:.2} \u{2550}\u{2550}",
        result.spectral_resilience
    );
    println!(
        "       (BPPR={:.2}, SRT={}, SCDI={:.2}Hz)",
        result.bppr,
        match result.spectral_recovery_90_ms {
            Some(ms) => format!("{:.0}ms", ms),
            None => "N/R".to_string(),
        },
        result.scdi_hz,
    );

    // Timeline (sampled every ~0.5s for compact output)
    println!();
    println!("  \u{2500}\u{2500} Timeline \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}");
    println!(
        "    {:>6}  {:>8}  {:>8}  {:>8}  {}",
        "Time", "Entrain", "DomFreq", "Centroid", ""
    );

    let step = (0.5 / config.hop_s) as usize; // print every ~0.5s
    let step = step.max(1);
    for (i, w) in result.windows.iter().enumerate() {
        if i % step != 0 {
            continue;
        }
        let marker = if (w.time_s - spike_time).abs() < config.hop_s * 2.0 {
            " \u{25c0} SPIKE"
        } else if (w.time_s - result.nadir_time).abs() < config.hop_s * 2.0 {
            " \u{25c0} NADIR"
        } else {
            ""
        };
        let ent_str = match w.entrainment_ratio {
            Some(e) => format!("{:.3}", e),
            None => "  N/A".to_string(),
        };
        println!(
            "    {:>5.1}s  {:>8}  {:>7.1} Hz  {:>6.1} Hz{}",
            w.time_s, ent_str, w.dominant_freq, w.spectral_centroid, marker
        );
    }

    println!();
}

// ── Generate Training Data (Priority 14a) ──────────────────────────────────

fn run_generate_data(
    output: &Path,
    count: usize,
    goals_str: &str,
    brain_type_str: &str,
    duration: f32,
    threads: usize,
    phys_gate: bool,
    seed: u64,
) {
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    ensure_analysis_window_or_exit(
        "generate-data",
        duration,
        SimulationConfig::default().warmup_discard_secs,
    );

    let goals: Vec<GoalKind> = if goals_str.to_lowercase() == "all" {
        GoalKind::all().to_vec()
    } else {
        goals_str
            .split(',')
            .filter_map(|s| GoalKind::from_str(s.trim()))
            .collect()
    };
    if goals.is_empty() {
        eprintln!("No valid goals specified");
        std::process::exit(1);
    }

    let brain_types: Vec<BrainType> = if brain_type_str.to_lowercase() == "all" {
        BrainType::all().to_vec()
    } else {
        brain_type_str
            .split(',')
            .filter_map(|s| BrainType::from_str(s.trim()))
            .collect()
    };
    if brain_types.is_empty() {
        eprintln!("No valid brain types specified");
        std::process::exit(1);
    }

    let total_evals = count * goals.len() * brain_types.len();
    println!();
    println!("  Surrogate Training Data Generator");
    println!("  \u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
    println!("  Presets:        {count}");
    println!(
        "  Goals:          {} ({} total)",
        goals.len(),
        goals
            .iter()
            .map(|g| format!("{g}"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!("  Brain types:    {}", brain_types.len());
    println!("  Total evals:    {total_evals}");
    println!("  Duration:       {duration:.1}s per eval");
    println!("  Threads:        {threads}");
    println!(
        "  Phys gate:      {}",
        if phys_gate { "enabled" } else { "disabled" }
    );
    println!("  Output:         {}", output.display());
    println!();

    // Generate random presets using the same genome bounds as DE.
    let bounds = Preset::bounds();
    let mut rng_state = seed;
    let mut next_u64 = || -> u64 {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        rng_state
    };

    let mut genomes: Vec<Vec<f64>> = Vec::with_capacity(count);
    for _ in 0..count {
        let genome: Vec<f64> = bounds
            .iter()
            .map(|(lo, hi)| {
                let u = next_u64() as f64 / u64::MAX as f64;
                lo + u * (hi - lo)
            })
            .collect();
        genomes.push(genome);
    }

    // Build work items: (preset_idx, genome, goal, brain_type)
    let mut work_items: Vec<(usize, Vec<f64>, GoalKind, BrainType)> =
        Vec::with_capacity(total_evals);
    for (idx, genome) in genomes.iter().enumerate() {
        for &goal in &goals {
            for &bt in &brain_types {
                work_items.push((idx, genome.clone(), goal, bt));
            }
        }
    }

    // Thread-safe results collector
    let results: Arc<Mutex<Vec<(usize, Vec<f64>, GoalKind, BrainType, SimulationConfig, f64)>>> =
        Arc::new(Mutex::new(Vec::with_capacity(total_evals)));
    let progress = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Parallel evaluation
    let chunk_size = (work_items.len() + threads - 1) / threads;
    let work_items = Arc::new(work_items);

    std::thread::scope(|s| {
        for t in 0..threads {
            let work = Arc::clone(&work_items);
            let results = Arc::clone(&results);
            let progress = Arc::clone(&progress);
            let start_idx = t * chunk_size;
            let end_idx = (start_idx + chunk_size).min(work.len());

            s.spawn(move || {
                for i in start_idx..end_idx {
                    let (preset_idx, ref genome, goal_kind, bt) = work[i];
                    let preset = Preset::from_genome(genome);
                    let goal = Goal::new(goal_kind);
                    let config = build_generate_data_config(duration, bt, phys_gate);
                    let result = evaluate_preset(&preset, &goal, &config);

                    results.lock().unwrap().push((
                        preset_idx,
                        genome.clone(),
                        goal_kind,
                        bt,
                        config,
                        result.score,
                    ));

                    let done = progress.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    if done % 100 == 0 || done == total_evals {
                        eprint!(
                            "\r  Progress: {done}/{total_evals} ({:.1}%)",
                            100.0 * done as f64 / total_evals as f64
                        );
                    }
                }
            });
        }
    });
    eprintln!();

    // Write CSV
    let results = match Arc::try_unwrap(results) {
        Ok(mutex) => match mutex.into_inner() {
            Ok(values) => values,
            Err(poisoned) => poisoned.into_inner(),
        },
        Err(_) => {
            eprintln!("Internal error: generate-data results still have outstanding references");
            std::process::exit(1);
        }
    };
    let mut file = std::fs::File::create(output).unwrap_or_else(|e| {
        eprintln!("Failed to create output file: {e}");
        std::process::exit(1);
    });

    // Header
    writeln!(file, "{}", surrogate_csv_header()).unwrap();

    // Data rows
    for (_, genome, goal_kind, bt, config, score) in &results {
        writeln!(
            file,
            "{}",
            surrogate_csv_row(genome, *goal_kind, *bt, config, *score)
        )
        .unwrap();
    }

    println!("  Wrote {} rows to {}", results.len(), output.display());
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluate_disable_flags_override_enabled_defaults() {
        let flags = resolve_evaluate_feature_flags(true, true, true, true, true, true, false);

        assert_eq!(
            flags,
            EvaluateFeatureFlags {
                assr: false,
                thalamic_gate: false,
                cet: false,
                phys_gate: false,
            }
        );
    }

    #[test]
    fn build_eval_config_carries_feature_flags() {
        let flags = EvaluateFeatureFlags {
            assr: false,
            thalamic_gate: true,
            cet: false,
            phys_gate: true,
        };

        let config = build_eval_config(7.5, BrainType::Aging, flags, true, true);
        assert!((config.duration_secs - 7.5).abs() < 1e-12);
        assert_eq!(config.brain_type, BrainType::Aging);
        assert!(!config.assr_enabled);
        assert!(config.thalamic_gate_enabled);
        assert!(!config.cet_enabled);
        assert!(config.physiological_thalamic_gate_enabled);
        assert!(config.acoustic_scoring_enabled);
        assert!(config.acoustic_score_fusion_enabled);
    }

    #[test]
    fn build_generate_data_config_carries_phys_gate() {
        let config = build_generate_data_config(3.5, BrainType::Adhd, true);
        assert!((config.duration_secs - 3.5).abs() < 1e-12);
        assert_eq!(config.brain_type, BrainType::Adhd);
        assert!(config.assr_enabled);
        assert!(config.thalamic_gate_enabled);
        assert!(config.cet_enabled);
        assert!(config.physiological_thalamic_gate_enabled);
        assert!(!config.acoustic_scoring_enabled);
    }

    #[test]
    fn build_optimize_config_enables_fusion_implies_acoustic_scoring() {
        let config = build_optimize_config(4.0, BrainType::Anxious, true, true, true);
        assert!((config.duration_secs - 4.0).abs() < 1e-12);
        assert_eq!(config.brain_type, BrainType::Anxious);
        assert!(config.cet_enabled);
        assert!(config.physiological_thalamic_gate_enabled);
        assert!(config.acoustic_scoring_enabled);
        assert!(config.acoustic_score_fusion_enabled);
    }

    #[test]
    fn acoustic_scaffolding_defaults_are_disabled() {
        assert!(!crate::acoustic_score::AcousticScoreConfig::default().enabled);
        assert!(!crate::acoustic_score::AcousticScoreConfig::default().fusion_enabled);
        assert!(!SimulationConfig::default().acoustic_scoring_enabled);
        assert!(!SimulationConfig::default().acoustic_score_fusion_enabled);
        assert!(!disturb::DisturbConfig::default().acoustic_scoring_enabled);
    }

    #[test]
    fn validate_optimize_acoustic_mode_accepts_supported_goal_without_sidecars() {
        let goal = Goal::new(GoalKind::Shield);
        assert!(validate_optimize_acoustic_mode(&goal, true, false, None).is_ok());
    }

    #[test]
    fn validate_optimize_acoustic_mode_rejects_unsupported_goal() {
        let goal = Goal::new(GoalKind::Focus);
        let err = validate_optimize_acoustic_mode(&goal, true, false, None).unwrap_err();
        assert!(err.contains("supported only for shield and isolation"));
    }

    #[test]
    fn validate_optimize_acoustic_mode_rejects_surrogate_mix() {
        let goal = Goal::new(GoalKind::Shield);
        let err = validate_optimize_acoustic_mode(&goal, true, true, None).unwrap_err();
        assert!(err.contains("--surrogate"));
    }

    #[test]
    fn validate_optimize_acoustic_mode_rejects_logging_mix() {
        let goal = Goal::new(GoalKind::Isolation);
        let err = validate_optimize_acoustic_mode(
            &goal,
            true,
            false,
            Some(Path::new("/tmp/fused_optimize.csv")),
        )
        .unwrap_err();
        assert!(err.contains("--log-evaluations"));
    }

    #[test]
    fn surrogate_validation_mask_marks_top_k_and_exploration() {
        let mask = surrogate_validation_mask(8, 3, 2);
        let selected: Vec<usize> = mask
            .iter()
            .enumerate()
            .filter_map(|(idx, keep)| keep.then_some(idx))
            .collect();

        assert_eq!(selected, vec![0, 1, 2, 5]);
    }

    #[test]
    fn surrogate_validation_mask_supports_zero_k() {
        let mask = surrogate_validation_mask(5, 0, 1);
        let selected: Vec<usize> = mask
            .iter()
            .enumerate()
            .filter_map(|(idx, keep)| keep.then_some(idx))
            .collect();

        assert_eq!(selected, vec![0]);
    }

    #[test]
    fn surrogate_validation_mask_validates_all_when_k_covers_population() {
        let mask = surrogate_validation_mask(4, 9, 7);
        assert!(mask.into_iter().all(|keep| keep));
    }

    #[test]
    fn surrogate_csv_header_matches_current_contract() {
        let header = surrogate_csv_header();
        let cols: Vec<&str> = header.split(',').collect();

        assert_eq!(
            cols.len(),
            surrogate::GENOME_DIM + surrogate::CSV_METADATA_COLUMNS.len()
        );
        assert_eq!(cols[0], "g0");
        assert_eq!(
            cols[surrogate::GENOME_DIM - 1],
            format!("g{}", surrogate::GENOME_DIM - 1)
        );
        assert_eq!(
            &cols[surrogate::GENOME_DIM..],
            &surrogate::CSV_METADATA_COLUMNS
        );
    }

    #[test]
    fn surrogate_csv_row_serializes_actual_config_flags() {
        let genome = vec![0.5_f64; surrogate::GENOME_DIM];
        let config = SimulationConfig {
            duration_secs: 3.0,
            brain_type: BrainType::Aging,
            assr_enabled: true,
            thalamic_gate_enabled: false,
            cet_enabled: true,
            physiological_thalamic_gate_enabled: true,
            ..SimulationConfig::default()
        };

        let row = surrogate_csv_row(&genome, GoalKind::Sleep, BrainType::Aging, &config, 0.375);
        let cols: Vec<&str> = row.split(',').collect();
        let meta_start = surrogate::GENOME_DIM;
        let expected_goal_id = GoalKind::all()
            .iter()
            .position(|&g| g == GoalKind::Sleep)
            .unwrap()
            .to_string();
        let expected_brain_type_id = BrainType::all()
            .iter()
            .position(|&b| b == BrainType::Aging)
            .unwrap()
            .to_string();

        assert_eq!(cols[meta_start], expected_goal_id);
        assert_eq!(cols[meta_start + 1], expected_brain_type_id);
        assert_eq!(cols[meta_start + 2], "1");
        assert_eq!(cols[meta_start + 3], "0");
        assert_eq!(cols[meta_start + 4], "1");
        assert_eq!(cols[meta_start + 5], "1");
        assert_eq!(cols[meta_start + 6], "0.375000");
    }

    #[test]
    fn export_best_genome_uses_re_evaluated_real_score() {
        let best_genome = Preset::default().to_genome();
        let goal_kind = GoalKind::Focus;
        let goal = Goal::new(goal_kind);
        let config = SimulationConfig {
            duration_secs: 3.0,
            brain_type: BrainType::Normal,
            ..SimulationConfig::default()
        };
        let direct = evaluate_preset(&Preset::from_genome(&best_genome), &goal, &config);
        let fake_cached_fitness = direct.score + 0.12345;
        let output_path = std::env::temp_dir().join("test_export_best_genome_uses_real_score.json");
        let _ = std::fs::remove_file(&output_path);

        let zero_spread = [0.0_f32; preset::MAX_OBJECTS];
        let (_preset, exported_result) = export_best_genome(
            &output_path,
            &best_genome,
            &goal,
            goal_kind,
            7,
            config.duration_secs,
            &config,
            &zero_spread,
        )
        .expect("best-genome export should succeed");

        let json = std::fs::read_to_string(&output_path).expect("exported JSON should exist");
        let exported: serde_json::Value =
            serde_json::from_str(&json).expect("exported JSON should parse");
        let exported_score = exported["meta"]["score"]
            .as_f64()
            .expect("meta.score should be f64");
        let exported_beta = exported["analysis"]["band_powers"]["beta"]
            .as_f64()
            .expect("analysis.band_powers.beta should be f64");

        assert!((exported_result.score - direct.score).abs() < 1e-12);
        assert!((exported_score - direct.score).abs() < 1e-12);
        assert!((exported_beta - direct.beta_power).abs() < 1e-12);
        assert!((exported_score - fake_cached_fitness).abs() > 1e-6);

        let _ = std::fs::remove_file(output_path);
    }

    #[test]
    fn export_best_genome_uses_re_evaluated_fused_score() {
        let best_genome = Preset::default().to_genome();
        let goal_kind = GoalKind::Shield;
        let goal = Goal::new(goal_kind);
        let config = SimulationConfig {
            duration_secs: 3.0,
            brain_type: BrainType::Normal,
            acoustic_scoring_enabled: true,
            acoustic_score_fusion_enabled: true,
            ..SimulationConfig::default()
        };
        let direct = evaluate_preset(&Preset::from_genome(&best_genome), &goal, &config);
        let output_path =
            std::env::temp_dir().join("test_export_best_genome_uses_re_evaluated_fused_score.json");
        let _ = std::fs::remove_file(&output_path);

        let zero_spread = [0.0_f32; preset::MAX_OBJECTS];
        let (_preset, exported_result) = export_best_genome(
            &output_path,
            &best_genome,
            &goal,
            goal_kind,
            3,
            config.duration_secs,
            &config,
            &zero_spread,
        )
        .expect("fused best-genome export should succeed");

        let json = std::fs::read_to_string(&output_path).expect("exported JSON should exist");
        let exported: serde_json::Value =
            serde_json::from_str(&json).expect("exported JSON should parse");
        let exported_score = exported["meta"]["score"]
            .as_f64()
            .expect("meta.score should be f64");

        assert!((exported_result.score - direct.score).abs() < 1e-12);
        assert!((exported_score - direct.score).abs() < 1e-12);
        assert!(
            exported_result
                .acoustic_score
                .as_ref()
                .and_then(|acoustic| acoustic.fused_score_preview)
                .is_some(),
            "fused optimize export should preserve the fused-score payload"
        );

        let _ = std::fs::remove_file(output_path);
    }

    #[test]
    fn matrix_single_cell_score_matches_scalar_evaluate() {
        let preset = Preset::default();
        let flags = EvaluateFeatureFlags {
            assr: true,
            thalamic_gate: true,
            cet: true,
            phys_gate: false,
        };
        let duration = 4.0;
        let goal_kind = GoalKind::Meditation;
        let brain_type = BrainType::Anxious;
        let goal = Goal::new(goal_kind);
        let config = build_eval_config(duration, brain_type, flags, false, false);

        let direct = evaluate_preset(&preset, &goal, &config);
        let matrix = evaluate_score_matrix(
            &preset,
            &[goal_kind],
            &[brain_type],
            duration,
            flags,
            false,
        );

        assert_eq!(matrix.len(), 1);
        assert_eq!(matrix[0].len(), 1);
        assert!(
            (matrix[0][0] - direct.score).abs() < 1e-12,
            "matrix 1x1 score {:.12} must match scalar evaluate {:.12}",
            matrix[0][0],
            direct.score
        );
    }
}
