mod auditory;
mod brain_type;
mod export;
mod neural;
mod optimizer;
mod pipeline;
mod preset;
mod scoring;
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
            );
        }
        Commands::Evaluate {
            preset,
            goal,
            brain_type,
            duration,
        } => {
            run_evaluate(&preset, &goal, &brain_type, duration);
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
) {
    let goal_kind = GoalKind::from_str(goal_str).unwrap_or_else(|| {
        eprintln!(
            "Unknown goal: '{}'. Valid: deep_relaxation, focus, sleep, isolation, meditation",
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
    let mut de = DifferentialEvolution::new(bounds, population, de_f, de_cr, seed);

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
    println!("    FHN ISI CV:       {:.3}", best_result.fhn_isi_cv);
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

fn run_evaluate(preset_path: &PathBuf, goal_str: &str, brain_type_str: &str, duration: f32) {
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
        print_comparison_matrix(&preset, &goals, &brain_types, duration);
    } else {
        // ── Single evaluation with full diagnosis ───────────────────────────
        let bt = brain_types[0];
        let goal_kind = goals[0];
        let goal = Goal::new(goal_kind);

        let sim_config = SimulationConfig {
            duration_secs: duration,
            brain_type: bt,
        };
        let result = evaluate_preset(&preset, &goal, &sim_config);

        // Re-run pipeline for detailed diagnosis (need FHN/JR results)
        let (fhn_result, jr_result, brightness, energy_fractions) =
            run_detailed_pipeline(&preset, bt, duration);

        let diagnosis = goal.diagnose(&fhn_result, &jr_result, brightness);

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
                scoring::BandExpectation::High => {
                    if band.actual >= 0.25 { "want high, got high" }
                    else if band.actual >= 0.15 { "want high, moderate" }
                    else { "want high, got low" }
                }
                scoring::BandExpectation::Low => {
                    if band.actual <= 0.15 { "want low, got low" }
                    else if band.actual <= 0.25 { "want low, moderate" }
                    else { "want low, got high" }
                }
                scoring::BandExpectation::Neutral => "neutral",
                scoring::BandExpectation::Flat(_) => {
                    if (band.actual - 0.2).abs() < 0.05 { "near uniform" }
                    else { "deviates from uniform" }
                }
            };
            println!(
                "    {:<8} {:<8} {:<8.3} {} {}  ({})",
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
        println!();

        println!(
            "  Dominant frequency: {:.1} Hz ({} range)",
            diagnosis.dominant_freq,
            diagnosis.dominant_band_name()
        );

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
fn run_detailed_pipeline(
    preset: &Preset,
    brain_type: BrainType,
    duration: f32,
) -> (neural::FhnResult, neural::JansenRitResult, f64, [f64; 4]) {
    use crate::auditory::GammatoneFilterbank;
    use noice_generator_core::NoiseEngine;
    use rustfft::{num_complex::Complex, FftPlanner};

    let sample_rate = 48_000_u32;
    let sr = sample_rate as f64;
    let num_frames = (sample_rate as f32 * duration) as u32;

    let engine = NoiseEngine::new(sample_rate, 0.8);
    preset.apply_to_engine(&engine);

    // Warmup — let engine filters settle
    let _ = engine.render_audio((sample_rate as f32 * 1.0) as u32);
    let audio = engine.render_audio(num_frames);

    // Deinterleave to L/R
    let num_samples = audio.len() / 2;
    let mut left = Vec::with_capacity(num_samples);
    let mut right = Vec::with_capacity(num_samples);
    for i in 0..num_samples {
        left.push(audio[i * 2]);
        right.push(audio[i * 2 + 1]);
    }

    // Tonotopic band-grouped cochlear processing
    let mut fb_l = GammatoneFilterbank::new(sr);
    let mut fb_r = GammatoneFilterbank::new(sr);
    let bands_l = fb_l.process_to_band_groups(&left);
    let bands_r = fb_r.process_to_band_groups(&right);

    // Average L/R and normalise each band
    let mut band_signals: [Vec<f64>; 4] = [
        vec![0.0; bands_l.signals[0].len()],
        vec![0.0; bands_l.signals[1].len()],
        vec![0.0; bands_l.signals[2].len()],
        vec![0.0; bands_l.signals[3].len()],
    ];
    let mut energy_fractions = [0.0_f64; 4];

    for b in 0..4 {
        let raw: Vec<f64> = bands_l.signals[b].iter()
            .zip(bands_r.signals[b].iter())
            .map(|(l, r)| (l + r) * 0.5)
            .collect();
        let max_val = raw.iter().cloned().fold(0.0_f64, f64::max);
        let norm = if max_val > 1e-10 { 1.0 / max_val } else { 1.0 };
        band_signals[b] = raw.iter().map(|x| x * norm).collect();
        energy_fractions[b] = (bands_l.energy_fractions[b] + bands_r.energy_fractions[b]) * 0.5;
    }

    let ef_sum: f64 = energy_fractions.iter().sum();
    if ef_sum > 1e-30 {
        for ef in &mut energy_fractions {
            *ef /= ef_sum;
        }
    }

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

    // Tonotopic JR
    let neural_params = brain_type.params();
    let tono_params = brain_type.tonotopic_params();

    let jr_result = neural::simulate_tonotopic(
        &band_signals,
        &energy_fractions,
        &tono_params.band_rates,
        &tono_params.band_gains,
        &tono_params.band_offsets,
        neural_params.jansen_rit.c,
        neural_params.jansen_rit.input_scale,
        sr,
    );

    // FHN driven by combined EEG
    let fhn = neural::FhnModel::with_params(
        sr,
        neural_params.fhn.a,
        neural_params.fhn.b,
        neural_params.fhn.epsilon,
    );
    let eeg_max = jr_result.eeg.iter().map(|x| x.abs()).fold(0.0_f64, f64::max);
    let eeg_norm = if eeg_max > 1e-10 { 1.0 / eeg_max } else { 1.0 };
    let fhn_input: Vec<f64> = jr_result.eeg.iter().map(|x| x * eeg_norm).collect();
    let fhn_result = fhn.simulate(&fhn_input, neural_params.fhn.input_scale);

    (fhn_result, jr_result, brightness, energy_fractions)
}

fn print_comparison_matrix(
    preset: &Preset,
    goals: &[GoalKind],
    brain_types: &[BrainType],
    duration: f32,
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
    let mod_kind_names = ["Flat", "SineLfo", "Breathing", "Stochastic"];

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
    }
}
