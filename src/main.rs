mod auditory;
mod brain_type;
mod disturb;
mod export;
mod movement;
mod neural;
mod optimizer;
mod pipeline;
mod preset;
mod scoring;
mod analyze_preset;
mod regression_tests;
mod validate;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Instant;

use brain_type::BrainType;
use optimizer::DifferentialEvolution;
use pipeline::{evaluate_preset, SimulationConfig};
use preset::Preset;
use scoring::{Goal, GoalKind, MetricStatus};

#[derive(Parser)]
#[command(name = "neural-preset-optimizer")]
#[command(about = "Neural model-based optimizer and evaluator for noise generator presets")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
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

        /// Enable thalamic gate (arousal-dependent filtering)
        #[arg(long, default_value_t = false)]
        thalamic_gate: bool,
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
            );
        }
        Commands::Evaluate {
            preset,
            goal,
            brain_type,
            duration,
            assr,
            thalamic_gate,
        } => {
            run_evaluate(&preset, &goal, &brain_type, duration, assr, thalamic_gate);
        }
        Commands::Disturb {
            preset,
            brain_type,
            spike_time,
            spike_duration,
            spike_gain,
            duration,
        } => {
            run_disturb_cmd(&preset, &brain_type, spike_time, spike_duration, spike_gain, duration);
        }
        Commands::Validate => {
            validate::run_all();
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
) {
    let goal_kind = GoalKind::from_str(goal_str).unwrap_or_else(|| {
        eprintln!(
            "Unknown goal: '{}'. Valid: deep_relaxation, focus, sleep, isolation, meditation, deep_work",
            goal_str
        );
        std::process::exit(1);
    });
    let goal = Goal::new(goal_kind);

    let bt = BrainType::from_str(brain_type_str).unwrap_or_else(|| {
        eprintln!(
            "Unknown brain type: '{}'. Valid: normal, high_alpha, adhd, aging, anxious",
            brain_type_str
        );
        std::process::exit(1);
    });

    let sim_config = SimulationConfig {
        duration_secs: duration,
        brain_type: bt,
        ..SimulationConfig::default()
    };

    println!();
    println!("  Neural Preset Optimizer");
    println!("  \u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
    println!("  Goal:           {}", goal_kind);
    println!("  Brain type:     {} ({})", bt, bt.description());
    println!("  Population:     {}", population);
    println!("  Max generations:{}", generations);
    println!("  Audio duration: {:.1}s per evaluation", duration);
    println!("  Seed:           {}", seed);
    println!();

    let bounds = Preset::bounds();
    let discrete_dims = Preset::discrete_gene_indices();
    let mut de = DifferentialEvolution::with_discrete(bounds, population, de_f, de_cr, seed, discrete_dims);

    // Seed population from an existing preset if provided
    if let Some(path) = init_preset {
        let json = std::fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("Failed to read init preset: {}", e);
            std::process::exit(1);
        });
        let preset: Preset = serde_json::from_str(&json).unwrap_or_else(|e| {
            eprintln!("Failed to parse init preset: {}", e);
            std::process::exit(1);
        });
        let genome = preset.to_genome();
        de.seed_from_genome(&genome, 0.15);
        println!("  Init preset:    {}", path.display());
    }

    let start = Instant::now();

    // ── Initial population evaluation ───────────────────────────────────────
    println!("  Evaluating initial population...");
    let pending = de.pending_evaluations();
    for (idx, genome) in &pending {
        let preset = Preset::from_genome(genome);
        let result = evaluate_preset(&preset, &goal, &sim_config);
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

        for (target_idx, trial_genome) in trials {
            let preset = Preset::from_genome(&trial_genome);
            let result = evaluate_preset(&preset, &goal, &sim_config);
            de.report_trial_result(target_idx, trial_genome, result.score);
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
    let best_preset = Preset::from_genome(&best_genome);
    let best_result = evaluate_preset(&best_preset, &goal, &sim_config);

    println!();
    println!("  \u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
    println!("  Result");
    println!("  \u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
    println!("  Goal:            {}", goal_kind);
    println!("  Brain type:      {}", bt);
    println!("  Score:           {:.4}", best_result.score);
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
    println!("    Spectral centroid:  {:.1} Hz", best_result.performance.spectral_centroid);
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

    // Preset summary
    print_preset_summary(&best_preset);

    // ── Export ───────────────────────────────────────────────────────────────
    let output_path = output.unwrap_or_else(|| {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        PathBuf::from(format!("preset_{}_{}.json", goal_kind, ts))
    });

    match export::export_preset(
        &best_preset,
        &best_result,
        goal_kind,
        de.generation(),
        duration,
        &output_path,
    ) {
        Ok(()) => {
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

fn run_evaluate(preset_path: &PathBuf, goal_str: &str, brain_type_str: &str, duration: f32, assr: bool, thalamic_gate: bool) {
    // Load preset from JSON
    let json = std::fs::read_to_string(preset_path).unwrap_or_else(|e| {
        eprintln!("Failed to read preset file '{}': {}", preset_path.display(), e);
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
    println!();

    if is_matrix {
        // ── Matrix mode ─────────────────────────────────────────────────────
        print_comparison_matrix(&preset, &goals, &brain_types, duration, assr, thalamic_gate);
    } else {
        // ── Single evaluation with full diagnosis ───────────────────────────
        let bt = brain_types[0];
        let goal_kind = goals[0];
        let goal = Goal::new(goal_kind);

        let sim_config = SimulationConfig {
            duration_secs: duration,
            brain_type: bt,
            assr_enabled: assr,
            thalamic_gate_enabled: thalamic_gate,
            ..SimulationConfig::default()
        };
        let result = evaluate_preset(&preset, &goal, &sim_config);

        // Re-run pipeline for detailed diagnosis (need FHN/JR results)
        let detailed = run_detailed_pipeline(&preset, bt, duration, assr, thalamic_gate);
        let brightness = detailed.brightness;
        let energy_fractions = detailed.energy_fractions;

        let diagnosis = goal.diagnose(&detailed.fhn, &detailed.bilateral.combined, brightness, Some(detailed.performance));

        println!("  Brain type: {} ({})", bt, bt.description());
        println!("  Goal:       {}", goal_kind);
        println!("  Score:      {:.4}", diagnosis.score);
        println!();

        // Tonotopic Band Energies
        println!("  Tonotopic Input:");
        println!("    {:<22} {:<10} {}", "Band", "Energy", "");
        println!("    {}", "\u{2500}".repeat(40));
        for (b, label) in auditory::BAND_LABELS.iter().enumerate() {
            let pct = energy_fractions[b] * 100.0;
            let bar_str = bar(energy_fractions[b], 15);
            println!("    {:<22} {} {:.1}%", label, bar_str, pct);
        }
        println!();

        // EEG Band Powers
        println!("  EEG Band Powers:");
        println!("    {:<8} {:<8} {:<8} {:<6} {}", "Band", "Target", "Actual", "Status", "");
        println!("    {}", "\u{2500}".repeat(50));
        for band in &diagnosis.bands {
            let detail = match band.expectation {
                scoring::BandExpectation::Range(min, ideal, max) => {
                    if band.actual < min { "below range" }
                    else if band.actual > max { "above range" }
                    else if (band.actual - ideal).abs() <= (max - min) * 0.25 { "in range" }
                    else { "within range" }
                }
                scoring::BandExpectation::Flat(_) => {
                    if (band.actual - 0.2).abs() < 0.05 { "near uniform" }
                    else { "deviates from uniform" }
                }
                scoring::BandExpectation::High => {
                    if band.actual >= 0.25 { "in range" } else { "below range" }
                }
                scoring::BandExpectation::Low => {
                    if band.actual <= 0.15 { "in range" } else { "above range" }
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
        println!("    {:<18} {:<16} {:<10} {}", "Metric", "Target", "Actual", "Status");
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
        println!("    {:<8} {:<20} {:<20} {:<20}",
            "Band", "Left (fast α/β)", "Right (slow δ/θ)", "Combined");
        println!("    {}", "\u{2500}".repeat(72));
        let cb = &bi.combined.band_powers.normalized();
        let bands = [
            ("Delta", lh_bp.delta, rh_bp.delta, cb.delta),
            ("Theta", lh_bp.theta, rh_bp.theta, cb.theta),
            ("Alpha", lh_bp.alpha, rh_bp.alpha, cb.alpha),
            ("Beta",  lh_bp.beta,  rh_bp.beta,  cb.beta),
            ("Gamma", lh_bp.gamma, rh_bp.gamma, cb.gamma),
        ];
        for (name, lv, rv, cv) in &bands {
            println!("    {:<8} {:>5.1}%  {}   {:>5.1}%  {}   {:>5.1}%  {}",
                name,
                lv * 100.0, bar(*lv, 10),
                rv * 100.0, bar(*rv, 10),
                cv * 100.0, bar(*cv, 10),
            );
        }
        println!();
        println!("    Dominant freq:  Left {:.2} Hz   Right {:.2} Hz   Combined {:.2} Hz",
            bi.left_dominant_freq, bi.right_dominant_freq, bi.combined.dominant_freq);
        let asym_label = if bi.alpha_asymmetry.abs() < 0.05 {
            "balanced"
        } else if bi.alpha_asymmetry > 0.0 {
            "left-dominant"
        } else {
            "right-dominant"
        };
        println!("    Alpha asymmetry: {:+.3} ({})", bi.alpha_asymmetry, asym_label);
        println!();

        println!("  Performance Vector:");
        println!("    Spectral centroid:  {:.1} Hz", detailed.performance.spectral_centroid);
        if let Some(er) = detailed.performance.entrainment_ratio {
            println!("    Entrainment ratio:  {:.3}", er);
        } else {
            println!("    Entrainment ratio:  N/A (no NeuralLFO)");
        }
        if let Some(ei) = detailed.performance.ei_stability {
            println!("    E/I stability (CV): {:.3}", ei);
        } else {
            println!("    E/I stability:      N/A (G=0)");
        }
        println!();

        let brightness_label = if brightness > 0.7 {
            "bright (white-like)"
        } else if brightness > 0.4 {
            "moderate (pink-like)"
        } else if brightness > 0.15 {
            "dark (brown-like)"
        } else {
            "very dark"
        };
        println!(
            "  Spectral brightness: {:.2} ({})",
            brightness, brightness_label
        );
        println!();

        // Verdict
        let verdict_detail = match diagnosis.verdict {
            scoring::Verdict::Good => "neural rhythms align well with goal",
            scoring::Verdict::Ok => "partial alignment, some metrics off-target",
            scoring::Verdict::Poor => "poor alignment, most metrics off-target",
        };
        println!("  Verdict: {} \u{2014} {}", diagnosis.verdict, verdict_detail);
        println!();

        // Preset summary
        print_preset_summary(&preset);
    }

    println!();
}

/// Run the full tonotopic pipeline for detailed diagnosis.
///
/// Returns (FhnResult, JansenRitResult, brightness, energy_fractions).
struct DetailedResult {
    fhn: neural::FhnResult,
    bilateral: neural::BilateralResult,
    brightness: f64,
    energy_fractions: [f64; 4],
    performance: neural::PerformanceVector,
}

fn run_detailed_pipeline(
    preset: &Preset,
    brain_type: BrainType,
    duration: f32,
    assr_enabled: bool,
    thalamic_gate_enabled: bool,
) -> DetailedResult {
    use crate::auditory::GammatoneFilterbank;
    use noise_generator_core::NoiseEngine;
    use rustfft::{num_complex::Complex, FftPlanner};

    let sample_rate = 48_000_u32;
    let sr = sample_rate as f64;
    let num_frames = (sample_rate as f32 * duration) as u32;

    let engine = NoiseEngine::new(sample_rate, 0.8);
    preset.apply_to_engine(&engine);

    // Movement controller
    let mut movement = movement::MovementController::from_preset(preset);
    let warmup_frames = (sample_rate as f32 * 1.0) as u32;
    let chunk_frames = (sample_rate as f32 * 0.05) as u32;

    if movement.has_movement() {
        let warmup_chunks = warmup_frames / chunk_frames;
        let dt = chunk_frames as f64 / sample_rate as f64;
        for _ in 0..warmup_chunks {
            movement.tick(dt, &engine);
            let _ = engine.render_audio(chunk_frames);
        }
    } else {
        let _ = engine.render_audio(warmup_frames);
    }

    let audio = if movement.has_movement() {
        let dt = chunk_frames as f64 / sample_rate as f64;
        let mut all_audio = Vec::with_capacity((num_frames * 2) as usize);
        let mut rendered = 0_u32;
        while rendered < num_frames {
            let this_chunk = chunk_frames.min(num_frames - rendered);
            movement.tick(dt, &engine);
            all_audio.extend_from_slice(&engine.render_audio(this_chunk));
            rendered += this_chunk;
        }
        all_audio
    } else {
        engine.render_audio(num_frames)
    };

    // Deinterleave to L/R
    let num_samples = audio.len() / 2;
    let mut left = Vec::with_capacity(num_samples);
    let mut right = Vec::with_capacity(num_samples);
    for i in 0..num_samples {
        left.push(audio[i * 2]);
        right.push(audio[i * 2 + 1]);
    }

    // Cochlear processing — separate L/R
    let mut fb_l = GammatoneFilterbank::new(sr);
    let mut fb_r = GammatoneFilterbank::new(sr);
    let bands_l = fb_l.process_to_band_groups(&left);
    let bands_r = fb_r.process_to_band_groups(&right);

    // Normalise each ear using GLOBAL max (preserves inter-band energy ratios)
    let mut left_bands: [Vec<f64>; 4] = [
        vec![0.0; bands_l.signals[0].len()],
        vec![0.0; bands_l.signals[1].len()],
        vec![0.0; bands_l.signals[2].len()],
        vec![0.0; bands_l.signals[3].len()],
    ];
    let mut right_bands: [Vec<f64>; 4] = [
        vec![0.0; bands_r.signals[0].len()],
        vec![0.0; bands_r.signals[1].len()],
        vec![0.0; bands_r.signals[2].len()],
        vec![0.0; bands_r.signals[3].len()],
    ];
    let mut energy_fractions = [0.0_f64; 4];

    let global_max_l = (0..4)
        .map(|b| bands_l.signals[b].iter().cloned().fold(0.0_f64, f64::max))
        .fold(0.0_f64, f64::max);
    let global_max_r = (0..4)
        .map(|b| bands_r.signals[b].iter().cloned().fold(0.0_f64, f64::max))
        .fold(0.0_f64, f64::max);

    let norm_l = if global_max_l > 1e-10 { 1.0 / global_max_l } else { 1.0 };
    let norm_r = if global_max_r > 1e-10 { 1.0 / global_max_r } else { 1.0 };

    for b in 0..4 {
        left_bands[b] = bands_l.signals[b].iter().map(|x| x * norm_l).collect();
        right_bands[b] = bands_r.signals[b].iter().map(|x| x * norm_r).collect();
        energy_fractions[b] = (bands_l.energy_fractions[b] + bands_r.energy_fractions[b]) * 0.5;
    }
    let ef_sum: f64 = energy_fractions.iter().sum();
    if ef_sum > 1e-30 { for ef in &mut energy_fractions { *ef /= ef_sum; } }

    // Brightness from FFT
    let n = left.len();
    let fft_len = n.next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);
    let mut buf: Vec<Complex<f64>> = (0..fft_len)
        .map(|i| if i < n { Complex::new(left[i] as f64, 0.0) } else { Complex::new(0.0, 0.0) })
        .collect();
    fft.process(&mut buf);

    let freq_res = sr / fft_len as f64;
    let min_bin = (20.0 / freq_res).ceil() as usize;
    let max_bin = ((20000.0 / freq_res).floor() as usize).min(fft_len / 2);
    let mut weighted_sum = 0.0_f64;
    let mut total_power = 0.0_f64;
    for bin in min_bin..max_bin {
        let freq = bin as f64 * freq_res;
        let power = buf[bin].norm_sqr();
        weighted_sum += freq * power;
        total_power += power;
    }
    let centroid = if total_power > 0.0 { weighted_sum / total_power } else { 500.0 };
    let log_low = 100.0_f64.ln();
    let log_high = 10000.0_f64.ln();
    let brightness = ((centroid.max(100.0).ln() - log_low) / (log_high - log_low)).clamp(0.0, 1.0);

    // (Optional) ASSR: compute input_scale modifier from preset's modulation frequencies
    let assr_scale_modifier = if assr_enabled {
        let assr = crate::auditory::AssrTransfer::new();
        assr.compute_input_scale_modifier(preset)
    } else {
        1.0
    };

    // Bilateral cortical model
    let neural_params = brain_type.params();
    let mut bilateral = brain_type.bilateral_params();

    // (Optional) Thalamic gate: shift JR operating point based on arousal
    if thalamic_gate_enabled {
        let arousal = crate::auditory::ThalamicGate::compute_arousal(preset, brightness);
        let gate = crate::auditory::ThalamicGate::new(arousal);
        let shift = gate.offset_shift();
        if shift.abs() > 1e-10 {
            for offset in bilateral.left.band_offsets.iter_mut() {
                *offset += shift;
            }
            for offset in bilateral.right.band_offsets.iter_mut() {
                *offset += shift;
            }
        }
    }

    let fast_inhib = neural::FastInhibParams {
        g_fast_gain: neural_params.jansen_rit.g_fast_gain,
        g_fast_rate: neural_params.jansen_rit.g_fast_rate,
        c5: neural_params.jansen_rit.c5,
        c6: neural_params.jansen_rit.c6,
        c7: neural_params.jansen_rit.c7,
    };

    let effective_input_scale = neural_params.jansen_rit.input_scale * assr_scale_modifier;

    let bi_result = neural::simulate_bilateral(
        &left_bands,
        &right_bands,
        &bands_l.energy_fractions,
        &bands_r.energy_fractions,
        &bilateral,
        neural_params.jansen_rit.c,
        effective_input_scale,
        sr,
        &fast_inhib,
        neural_params.jansen_rit.v0,
    );

    // FHN driven by combined bilateral EEG
    let fhn = neural::FhnModel::with_params(
        sr,
        neural_params.fhn.a,
        neural_params.fhn.b,
        neural_params.fhn.epsilon,
        neural_params.fhn.time_scale,
    );
    let eeg_max = bi_result.combined.eeg.iter().map(|x| x.abs()).fold(0.0_f64, f64::max);
    let eeg_norm = if eeg_max > 1e-10 { 1.0 / eeg_max } else { 1.0 };
    let fhn_input: Vec<f64> = bi_result.combined.eeg.iter().map(|x| x * eeg_norm).collect();
    let fhn_result = fhn.simulate(&fhn_input, neural_params.fhn.input_scale);

    // Performance Vector
    let target_lfo_freq = preset.objects.iter()
        .flat_map(|obj| [&obj.bass_mod, &obj.satellite_mod])
        .filter(|m| m.kind == 4 && m.param_a > 0.5)
        .map(|m| m.param_a as f64)
        .next();

    let jr = &bi_result.combined;
    let eeg_mean = jr.eeg.iter().sum::<f64>() / jr.eeg.len() as f64;
    let eeg_detrended: Vec<f64> = jr.eeg.iter().map(|x| x - eeg_mean).collect();
    let performance = neural::PerformanceVector::compute(
        &eeg_detrended, &jr.fast_inhib_trace, sr, target_lfo_freq,
    );

    DetailedResult {
        fhn: fhn_result,
        bilateral: bi_result,
        brightness,
        energy_fractions,
        performance,
    }
}

fn print_comparison_matrix(
    preset: &Preset,
    goals: &[GoalKind],
    brain_types: &[BrainType],
    duration: f32,
    assr: bool,
    thalamic_gate: bool,
) {
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
    for bt in brain_types {
        print!("  {:<12}", format!("{}", bt));

        for goal_kind in goals {
            let goal = Goal::new(*goal_kind);
            let sim_config = SimulationConfig {
                duration_secs: duration,
                brain_type: *bt,
                assr_enabled: assr,
                thalamic_gate_enabled: thalamic_gate,
                ..SimulationConfig::default()
            };

            let result = evaluate_preset(preset, &goal, &sim_config);

            let icon = if result.score >= 0.75 {
                "\u{2713}"
            } else if result.score >= 0.50 {
                "~"
            } else {
                "\u{2717}"
            };
            print!("  {} {:<10.4}", icon, result.score);
        }
        println!();
    }

    println!();

    // Legend
    println!("  \u{2713} >= 0.75 (good)   ~ >= 0.50 (ok)   \u{2717} < 0.50 (poor)");
    println!();
}

fn print_preset_summary(preset: &Preset) {
    let color_names = ["White", "Pink", "Brown", "Green", "Grey", "Black", "SSN"];
    let env_names = [
        "AnechoicChamber",
        "FocusRoom",
        "OpenLounge",
        "VastSpace",
        "DeepSanctuary",
    ];
    let mod_kind_names = ["Flat", "SineLfo", "Breathing", "Stochastic", "NeuralLfo"];

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
        color_names[preset.anchor_color as usize],
        preset.anchor_volume
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
            i,
            color_names[obj.color as usize],
            obj.x,
            obj.y,
            obj.z,
            obj.volume,
            obj.reverb_send,
        );
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
                        pattern.label(), mv.speed, mv.depth_min, mv.depth_max,
                        mv.reverb_min, mv.reverb_max,
                    );
                }
                _ => {
                    println!(
                        "      Movement:  {} (radius={:.2}, speed={:.2}, phase={:.2})",
                        pattern.label(), mv.radius, mv.speed, mv.phase,
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
    // Load preset
    let json = std::fs::read_to_string(preset_path).unwrap_or_else(|e| {
        eprintln!("Failed to read preset file '{}': {}", preset_path.display(), e);
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
    };

    let start = Instant::now();
    let result = disturb::run_disturb(&preset, &config);
    let elapsed = start.elapsed();

    // ── Display ─────────────────────────────────────────────────────────
    println!();
    println!("  \u{2550}\u{2550}\u{2550} Disturbance Resilience Test \u{2550}\u{2550}\u{2550}");
    println!();
    println!("  Brain type:      {}", bt);
    println!("  Spike:           {:.0}ms white noise burst at t={:.1}s, gain={:.2}",
        spike_duration * 1000.0, spike_time, spike_gain);
    if let Some(tf) = result.target_freq {
        println!("  Target LFO:      {:.1} Hz", tf);
    }
    println!("  Brightness:      {:.2}", result.brightness);
    println!("  Duration:        {:.1}s ({:.2}s elapsed)", duration, elapsed.as_secs_f64());
    println!();

    // Baseline
    println!("  \u{2500}\u{2500} Baseline (0 \u{2013} {:.1}s) \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}", spike_time);
    println!("    Dominant freq:       {:.2} Hz", result.baseline_dominant_freq);
    println!("    Spectral centroid:   {:.2} Hz", result.baseline_centroid);
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
        println!("    Entrainment nadir:   {:.3} ({:.0}% drop at t={:.2}s)", nadir, drop_pct, result.nadir_time);
    }
    println!("    Peak freq deviation: \u{00b1}{:.2} Hz", result.peak_freq_deviation);
    println!();

    // Recovery
    println!("  \u{2500}\u{2500} Recovery \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}");
    match result.recovery_50_ms {
        Some(ms) => println!("    50% recovery:        {:.0} ms", ms),
        None     => println!("    50% recovery:        NOT RECOVERED"),
    }
    match result.recovery_90_ms {
        Some(ms) => println!("    90% recovery:        {:.0} ms", ms),
        None     => println!("    90% recovery:        NOT RECOVERED"),
    }

    // Final state (last 2s)
    let final_windows: Vec<&disturb::WindowMetrics> = result.windows.iter()
        .filter(|w| w.time_s > (duration as f64 - 2.0 - 2.0)) // last 2s of analysis
        .collect();
    if !final_windows.is_empty() {
        let final_ent: Option<f64> = {
            let vals: Vec<f64> = final_windows.iter()
                .filter_map(|w| w.entrainment_ratio)
                .collect();
            if vals.is_empty() { None } else { Some(vals.iter().sum::<f64>() / vals.len() as f64) }
        };
        let final_freq = final_windows.iter().map(|w| w.dominant_freq).sum::<f64>() / final_windows.len() as f64;

        println!();
        println!("    Final entrainment:   {}", match final_ent {
            Some(e) => format!("{:.3}", e),
            None    => "N/A".to_string(),
        });
        println!("    Final dominant freq: {:.2} Hz", final_freq);

        // Resilience score: weighted combination of recovery speed and entrainment preservation
        if let (Some(base), Some(fin)) = (result.baseline_entrainment, final_ent) {
            let preservation = if base > 1e-10 { (fin / base).min(1.0) } else { 0.0 };
            let speed_score = match result.recovery_90_ms {
                Some(ms) if ms < 5000.0 => 1.0 - (ms / 5000.0),
                Some(_) => 0.0,
                None => 0.0,
            };
            let resilience = 0.6 * preservation + 0.4 * speed_score;
            println!();
            println!("    \u{2550}\u{2550} Resilience Score: {:.2} \u{2550}\u{2550}", resilience);
            println!("       (preservation={:.2}, speed={:.2})", preservation, speed_score);
        }
    }

    // Timeline (sampled every ~0.5s for compact output)
    println!();
    println!("  \u{2500}\u{2500} Timeline \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}");
    println!("    {:>6}  {:>8}  {:>8}  {:>8}  {}", "Time", "Entrain", "DomFreq", "Centroid", "");

    let step = (0.5 / config.hop_s) as usize; // print every ~0.5s
    let step = step.max(1);
    for (i, w) in result.windows.iter().enumerate() {
        if i % step != 0 { continue; }
        let marker = if (w.time_s - spike_time).abs() < config.hop_s * 2.0 {
            " \u{25c0} SPIKE"
        } else if (w.time_s - result.nadir_time).abs() < config.hop_s * 2.0 {
            " \u{25c0} NADIR"
        } else {
            ""
        };
        let ent_str = match w.entrainment_ratio {
            Some(e) => format!("{:.3}", e),
            None    => "  N/A".to_string(),
        };
        println!("    {:>5.1}s  {:>8}  {:>7.1} Hz  {:>6.1} Hz{}", w.time_s, ent_str, w.dominant_freq, w.spectral_centroid, marker);
    }

    println!();
}
