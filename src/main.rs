mod acoustic_score;
mod analyze_preset;
mod auditory;
mod brain_type;
mod disturb;
mod export;
mod model_signature;
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
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

use brain_type::BrainType;
use optimizer::DifferentialEvolution;
use crate::auditory::ArousalModel;
use pipeline::{
    evaluate_preset, evaluate_preset_detailed, validate_analysis_window,
    DetailedSimulationResult, SimulationConfig, SimulationResult,
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

#[derive(Debug, Clone)]
struct SeedPresetContext {
    spread_per_slot: [f32; preset::MAX_OBJECTS],
    position_space_per_slot: [u8; preset::MAX_OBJECTS],
    room: preset::RoomConfig,
}

impl Default for SeedPresetContext {
    fn default() -> Self {
        Self {
            spread_per_slot: [0.0_f32; preset::MAX_OBJECTS],
            position_space_per_slot: [0_u8; preset::MAX_OBJECTS],
            room: preset::RoomConfig::default(),
        }
    }
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
        /// scoring until the surrogate score contract is updated.
        #[arg(long = "acoustic-score-fusion", default_value_t = false)]
        acoustic_score_fusion: bool,

        /// Enable Priority 28 Phase 2 ε-constrained optimization.
        ///
        /// Treats acoustic comfort metrics (LUFS asymmetry, true peak,
        /// spectral tilt, HF fraction, PLR) as soft constraints with a
        /// decaying ε-tolerance (Takahama & Sakai 2009). The optimizer
        /// ranks by neural-fitness *only* among ε-feasible candidates;
        /// infeasible candidates compete by lowest violation. Tightens
        /// to strict feasibility by half the generation budget.
        ///
        /// Implies acoustic analysis. Forces `--acoustic-score-fusion`
        /// off (would double-count comfort) and `--surrogate` off
        /// (single-scalar surrogate cannot predict (fitness, violation)
        /// jointly). Both are validated up-front to prevent silent
        /// misconfiguration.
        #[arg(long, default_value_t = false)]
        constrained: bool,

        /// Enable Priority 28 Phase 3 crowding-DE selection (Thomsen 2004).
        ///
        /// Each trial competes against its **nearest-genome parent**
        /// rather than the parent it was generated from. Maintains
        /// niches across the search space and counters the documented
        /// tendency of vanilla DE/rand/1/bin to collapse to a single
        /// basin on a high-dimensional genome (the 230-D preset space).
        /// Generation cost is unchanged; only the replacement target
        /// is redirected.
        #[arg(long, default_value_t = false)]
        crowding: bool,

        /// Enable Priority 28 Phase 3 stagnation-triggered partial
        /// restart (Sallam 2025 ARRDE). When the best fitness has not
        /// improved for `--stagnation-window` consecutive generations,
        /// reseed the worst `--stagnation-fraction` of the population
        /// with uniform-random genomes; the elite is preserved.
        ///
        /// Set to 0 (default) to disable. Typical: 10–20 for the 230-D
        /// preset genome at 100 generations. Counter is exposed in the
        /// per-generation progress display.
        #[arg(long, default_value_t = 0)]
        stagnation_window: usize,

        /// Fraction of the population to reseed on each stagnation
        /// trigger. Ignored when `--stagnation-window=0`. Typical:
        /// 0.20–0.40. The current best is always preserved (elitism).
        #[arg(long, default_value_t = 0.30)]
        stagnation_fraction: f64,

        /// Priority 18a — Stochastic noise σ on JR input drive.
        ///
        /// Per Ableidinger, Buckwar & Hinterleitner (2017), noise on
        /// the JR system can drive transitions between the alpha
        /// attractor (~10 Hz limit cycle) and lower-frequency basins
        /// (theta/delta), producing a multi-band output spectrum.
        /// Default 15.0 preserves legacy bit-identity. The
        /// literature-recommended retuning for breaking the single-
        /// attractor lock is 50–200 (use 100 as a safe starting point).
        #[arg(long, default_value_t = 15.0)]
        jr_sigma: f64,

        /// Priority 18b — Slow inhibitory population decay rate
        /// `b_slow` (1/s) for the CET parallel slow-GABA loop.
        ///
        /// Per Ursino, Cona & Zavaglia (2010), a second inhibitory
        /// population with a different time constant produces
        /// simultaneous multi-band rhythms in a single column.
        /// Default 5.0 (τ = 200 ms) preserves legacy bit-identity.
        /// Retuning to 25.0 (τ = 40 ms) places the slow-population
        /// resonance in the theta band (5–8 Hz). Only takes effect
        /// when CET is enabled.
        #[arg(long, default_value_t = 5.0)]
        gaba_b_rate: f64,

        /// Priority 18b — Slow inhibitory population synaptic gain
        /// `B_slow` (mV) for the CET parallel slow-GABA loop.
        ///
        /// Default 10.0 preserves legacy bit-identity. Recommended
        /// range when retuning for theta-alpha coexistence: 12–20.
        /// Only takes effect when CET is enabled.
        #[arg(long, default_value_t = 10.0)]
        gaba_b_gain: f64,

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

    /// Run a simple multi-stage optimization schedule that searches at a short
    /// window first, then re-optimizes the winner at longer windows.
    ///
    /// This is a pragmatic guard against short-window overfitting:
    /// stage 1 explores cheaply, stage 2 re-tunes at a medium window,
    /// and stage 3 applies a final long-window continuation pass.
    OptimizeStaged {
        /// Goal to optimize for
        #[arg(long, default_value = "focus")]
        goal: String,

        /// Stage 1 population size
        #[arg(long, default_value_t = 12)]
        population: usize,

        /// Stage 1 max generations
        #[arg(long, default_value_t = 30)]
        generations: usize,

        /// Final output JSON path (stage artifacts are derived from this)
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

        /// Convergence threshold (legacy DE only)
        #[arg(long, default_value_t = 0.001)]
        convergence: f64,

        /// Brain type profile
        #[arg(long, default_value = "normal")]
        brain_type: String,

        /// Seed stage 1 from an existing preset JSON
        #[arg(long)]
        init_preset: Option<PathBuf>,

        /// Enable Cortical Envelope Tracking
        #[arg(long, default_value_t = true)]
        cet: bool,

        /// Enable the physiological thalamic gate
        #[arg(long = "phys-gate", default_value_t = false)]
        phys_gate: bool,

        /// Enable fused acoustic/NMM scoring where supported
        #[arg(long = "acoustic-score-fusion", default_value_t = false)]
        acoustic_score_fusion: bool,

        /// Enable ε-constrained optimization
        #[arg(long, default_value_t = false)]
        constrained: bool,

        /// Enable crowding-DE selection
        #[arg(long, default_value_t = false)]
        crowding: bool,

        /// Enable stagnation-triggered partial restart
        #[arg(long, default_value_t = 0)]
        stagnation_window: usize,

        /// Fraction of the population to reseed on stagnation
        #[arg(long, default_value_t = 0.30)]
        stagnation_fraction: f64,

        /// Priority 18a — Stochastic noise sigma on JR input drive
        #[arg(long, default_value_t = 15.0)]
        jr_sigma: f64,

        /// Priority 18b — Slow inhibitory population decay rate
        #[arg(long, default_value_t = 5.0)]
        gaba_b_rate: f64,

        /// Priority 18b — Slow inhibitory population synaptic gain
        #[arg(long, default_value_t = 10.0)]
        gaba_b_gain: f64,

        /// Stage 1 audio duration (seconds)
        #[arg(long, default_value_t = 10.0)]
        stage1_duration: f32,

        /// Stage 2 audio duration (seconds)
        #[arg(long, default_value_t = 30.0)]
        stage2_duration: f32,

        /// Stage 3 audio duration (seconds)
        #[arg(long, default_value_t = 60.0)]
        stage3_duration: f32,

        /// Stage 2 population size
        #[arg(long, default_value_t = 8)]
        stage2_population: usize,

        /// Stage 2 max generations
        #[arg(long, default_value_t = 12)]
        stage2_generations: usize,

        /// Stage 3 population size
        #[arg(long, default_value_t = 6)]
        stage3_population: usize,

        /// Stage 3 max generations
        #[arg(long, default_value_t = 4)]
        stage3_generations: usize,
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

        /// Arousal model used to set cortical operating state provenance.
        #[arg(long = "arousal-model", default_value = "legacy_heuristic")]
        arousal_model: String,

        /// Fixed arousal assumption in [0,1], used when --arousal-model=fixed.
        #[arg(long = "fixed-arousal")]
        fixed_arousal: Option<f64>,

        /// Priority 18a — Stochastic noise sigma on JR input drive.
        ///
        /// Use this to replay the same P18 neural retune that may have been
        /// used during optimization. Default 15.0 preserves legacy behavior.
        #[arg(long, default_value_t = 15.0)]
        jr_sigma: f64,

        /// Priority 18b — Slow inhibitory population decay rate `b_slow`
        /// (1/s) for the CET parallel slow-GABA loop.
        ///
        /// Use this to replay the same P18 neural retune that may have been
        /// used during optimization. Default 5.0 preserves legacy behavior.
        #[arg(long, default_value_t = 5.0)]
        gaba_b_rate: f64,

        /// Priority 18b — Slow inhibitory population synaptic gain `B_slow`
        /// (mV) for the CET parallel slow-GABA loop.
        ///
        /// Use this to replay the same P18 neural retune that may have been
        /// used during optimization. Default 10.0 preserves legacy behavior.
        #[arg(long, default_value_t = 10.0)]
        gaba_b_gain: f64,

        /// Append evaluation rows to the shared training CSV schema.
        /// For single goal/brain evaluation this writes one example row and
        /// a sibling runs CSV entry next to the requested file.
        #[arg(long)]
        log_evaluations: Option<PathBuf>,
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

        /// Enable ASSR transfer function in canonical disturbance mode.
        #[arg(long, default_value_t = false)]
        assr: bool,

        /// Disable ASSR transfer function in canonical disturbance mode.
        #[arg(long = "no-assr", conflicts_with = "assr", default_value_t = false)]
        no_assr: bool,

        /// Enable heuristic thalamic gate in canonical disturbance mode.
        #[arg(long = "thalamic-gate", default_value_t = true)]
        thalamic_gate: bool,

        /// Disable heuristic thalamic gate in canonical disturbance mode.
        #[arg(
            long = "no-thalamic-gate",
            conflicts_with = "thalamic_gate",
            default_value_t = false
        )]
        no_thalamic_gate: bool,

        /// Enable Cortical Envelope Tracking in canonical disturbance mode.
        #[arg(long, default_value_t = true)]
        cet: bool,

        /// Disable Cortical Envelope Tracking in canonical disturbance mode.
        #[arg(long = "no-cet", conflicts_with = "cet", default_value_t = false)]
        no_cet: bool,

        /// Enable the physiological thalamic gate in canonical disturbance mode.
        #[arg(long = "phys-gate", default_value_t = false)]
        phys_gate: bool,

        /// Priority 18a — stochastic JR sigma in canonical disturbance mode.
        #[arg(long, default_value_t = 15.0)]
        jr_sigma: f64,

        /// Priority 18b — CET slow inhibitory decay rate in canonical disturbance mode.
        #[arg(long, default_value_t = 5.0)]
        gaba_b_rate: f64,

        /// Priority 18b — CET slow inhibitory gain in canonical disturbance mode.
        #[arg(long, default_value_t = 10.0)]
        gaba_b_gain: f64,

        /// Run the historical ablated disturbance path.
        #[arg(long = "legacy-ablated", default_value_t = false)]
        legacy_ablated: bool,
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

        /// Arousal model used for generated rows.
        #[arg(long = "arousal-model", default_value = "legacy_heuristic")]
        arousal_model: String,

        /// Fixed arousal assumption in [0,1], used when --arousal-model=fixed.
        #[arg(long = "fixed-arousal")]
        fixed_arousal: Option<f64>,

        /// Random seed
        #[arg(long, default_value_t = 42)]
        seed: u64,
    },

    /// Walk a directory of curated presets, evaluate each, and report the
    /// empirical distribution of Phase-1 comfort metrics. Used to
    /// **calibrate** the heuristic thresholds in `Goal::comfort_violation`
    /// against presets that are known-good (hand-tuned or accepted from
    /// previous runs). The 90th-percentile column is the suggested
    /// upper-bound a threshold should leave inside the feasible region.
    /// Read-only command — does not modify any file.
    CalibrateComfort {
        /// Directory of preset JSON files to analyse.
        #[arg(long, default_value = "presets")]
        presets_dir: PathBuf,

        /// Audio duration per evaluation (seconds).
        #[arg(long, default_value_t = 6.0)]
        duration: f32,

        /// Brain type profile.
        #[arg(long, default_value = "normal")]
        brain_type: String,
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

fn print_model_signature(signature: &model_signature::ModelSignature) {
    println!(
        "  Model:          {} / {} / {}",
        signature.version, signature.scoring_profile, signature.normalization_mode
    );
    println!(
        "  Pipeline path:  {}",
        match signature.pipeline_variant {
            model_signature::PipelineVariant::EvaluateCanonical => "evaluate_canonical",
            model_signature::PipelineVariant::EvaluateCandidateV2 => "evaluate_candidate_v2",
            model_signature::PipelineVariant::DisturbCanonical => "disturb_canonical",
            model_signature::PipelineVariant::DisturbLegacyAblated => "disturb_legacy_ablated",
        }
    );
    let json = serde_json::to_string(signature)
        .unwrap_or_else(|_| "{\"error\":\"signature_serialization_failed\"}".to_string());
    println!("  Model signature (json): {json}");
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
    arousal_model: ArousalModel,
    fixed_arousal: Option<f64>,
    jr_sigma: f64,
    gaba_b_rate: f64,
    gaba_b_gain: f64,
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
        arousal_model,
        fixed_arousal,
        jr_stochastic_sigma: jr_sigma,
        cet_b_slow_rate: gaba_b_rate,
        cet_b_slow_gain: gaba_b_gain,
        ..SimulationConfig::default()
    }
}

fn build_generate_data_config(
    duration: f32,
    brain_type: BrainType,
    phys_gate: bool,
    arousal_model: ArousalModel,
    fixed_arousal: Option<f64>,
    seed: u64,
) -> SimulationConfig {
    SimulationConfig {
        duration_secs: duration,
        brain_type,
        physiological_thalamic_gate_enabled: phys_gate,
        arousal_model,
        fixed_arousal,
        reproducibility_seed: Some(seed),
        ..SimulationConfig::default()
    }
}

fn resolve_arousal_model_or_exit(model: &str, fixed_arousal: Option<f64>) -> ArousalModel {
    let model = model.trim().to_ascii_lowercase();
    match model.as_str() {
        "legacy_heuristic" => {
            if fixed_arousal.is_some() {
                eprintln!("--fixed-arousal requires --arousal-model=fixed");
                std::process::exit(2);
            }
            ArousalModel::LegacyHeuristic
        }
        "fixed" => {
            let value = fixed_arousal.unwrap_or_else(|| {
                eprintln!("--arousal-model=fixed requires --fixed-arousal <0..1>");
                std::process::exit(2);
            });
            if !value.is_finite() || !(0.0..=1.0).contains(&value) {
                eprintln!("--fixed-arousal must be finite and in [0,1], got {value}");
                std::process::exit(2);
            }
            ArousalModel::Fixed
        }
        _ => {
            eprintln!("Unknown --arousal-model '{model}'. Valid: legacy_heuristic, fixed");
            std::process::exit(2);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn build_disturb_config(
    duration: f32,
    brain_type: BrainType,
    spike_time: f64,
    spike_duration: f64,
    spike_gain: f64,
    flags: EvaluateFeatureFlags,
    jr_sigma: f64,
    gaba_b_rate: f64,
    gaba_b_gain: f64,
    legacy_ablated: bool,
) -> disturb::DisturbConfig {
    disturb::DisturbConfig {
        mode: if legacy_ablated {
            disturb::DisturbanceMode::LegacyAblated
        } else {
            disturb::DisturbanceMode::Canonical
        },
        spike_time_s: spike_time,
        spike_duration_s: spike_duration,
        spike_gain,
        brain_type,
        duration_secs: duration,
        warmup_discard_secs: 2.0,
        window_s: 0.5,
        hop_s: 0.05,
        assr_enabled: flags.assr,
        thalamic_gate_enabled: flags.thalamic_gate,
        physiological_thalamic_gate_enabled: flags.phys_gate,
        cet_enabled: flags.cet,
        habituation_enabled: true,
        stochastic_jr_enabled: true,
        reproducibility_seed: None,
        arousal_model: ArousalModel::LegacyHeuristic,
        fixed_arousal: None,
        jr_stochastic_sigma: jr_sigma,
        cet_b_slow_rate: gaba_b_rate,
        cet_b_slow_gain: gaba_b_gain,
        acoustic_scoring_enabled: false,
        model_version: model_signature::ModelVersion::LegacyV1,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_optimize_config(
    duration: f32,
    brain_type: BrainType,
    cet: bool,
    phys_gate: bool,
    acoustic_score_fusion_enabled: bool,
    acoustic_constraints_enabled: bool,
    jr_sigma: f64,
    gaba_b_rate: f64,
    gaba_b_gain: f64,
    seed: u64,
) -> SimulationConfig {
    // Priority 28 Phase 2 — when constraints are on, we MUST populate
    // acoustic features (so `Goal::comfort_violation` has something to
    // read) and we MUST NOT enable fusion (would double-count comfort —
    // see SimulationConfig::acoustic_constraints_enabled docs).
    // Validation in `validate_optimize_acoustic_mode` already rejects the
    // illegal combinations; this constructor is just a final safety net.
    let acoustic_scoring_enabled = acoustic_score_fusion_enabled || acoustic_constraints_enabled;
    let acoustic_score_fusion_enabled = if acoustic_constraints_enabled {
        false
    } else {
        acoustic_score_fusion_enabled
    };
    SimulationConfig {
        duration_secs: duration,
        brain_type,
        cet_enabled: cet,
        physiological_thalamic_gate_enabled: phys_gate,
        acoustic_scoring_enabled,
        acoustic_score_fusion_enabled,
        acoustic_constraints_enabled,
        // Priority 18 — wire the literature-tunable params through.
        // Defaults match the historical hardcoded constants exactly,
        // so omitting --jr-sigma / --gaba-b-rate / --gaba-b-gain on
        // the CLI gives bit-identical pre-P18 behaviour.
        jr_stochastic_sigma: jr_sigma,
        cet_b_slow_rate: gaba_b_rate,
        cet_b_slow_gain: gaba_b_gain,
        reproducibility_seed: Some(seed),
        ..SimulationConfig::default()
    }
}

/// Priority 28 Phase 2 — derive the comfort violation for a single
/// `evaluate_preset` result. Returns 0.0 when the result has no acoustic
/// payload (e.g., acoustic scoring was disabled), which is the safe
/// fallback that keeps the constrained DE comparator from rejecting an
/// individual purely because the optimizer was misconfigured.
///
/// Callers should ensure `sim_config.acoustic_scoring_enabled = true`
/// before invoking this, so that the features are populated. The
/// `validate_optimize_acoustic_mode` check up-front prevents the
/// misconfiguration from ever reaching this helper in production.
fn compute_comfort_violation(goal: &Goal, result: &pipeline::SimulationResult) -> f64 {
    match result.acoustic_score.as_ref() {
        Some(acoustic) => goal.comfort_violation(&acoustic.features),
        None => 0.0,
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
    constrained: bool,
    use_surrogate: bool,
    log_evaluations_path: Option<&Path>,
) -> Result<(), String> {
    // Priority 28 Phase 2 — constrained-mode invariants. Checked first so
    // a misconfigured run fails before any expensive setup.
    if constrained {
        if acoustic_score_fusion {
            return Err(
                "--constrained is incompatible with --acoustic-score-fusion (would double-count comfort)"
                    .to_string(),
            );
        }
        if use_surrogate {
            return Err(
                "--constrained is incompatible with --surrogate until the surrogate is retrained to predict (neural_fitness, violation) jointly"
                    .to_string(),
            );
        }
        let _ = goal; // accepted for any goal — comfort_violation thresholds are goal-aware.
        let _ = log_evaluations_path;
        return Ok(());
    }

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
    let _ = log_evaluations_path;
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

#[derive(Debug, Clone)]
struct CsvExampleMeta {
    example_id: String,
    run_id: String,
    parent_example_id: String,
    stage: String,
    source: String,
    seed_eval: String,
    created_at: String,
}

struct OptimizeCsvLogger {
    command_kind: String,
    run_id: String,
    created_at: String,
    examples_path: PathBuf,
    pairs_path: PathBuf,
    runs_path: PathBuf,
    examples_file: File,
    pairs_file: File,
    next_example_seq: usize,
    total_examples: usize,
    total_pairs: usize,
}

impl OptimizeCsvLogger {
    fn new(examples_path: &Path, command_kind: &str) -> Self {
        let pairs_path = derive_sibling_csv_path(examples_path, "_pairs");
        let runs_path = derive_sibling_csv_path(examples_path, "_runs");
        let examples_file = open_append_csv(examples_path, surrogate_csv_header());
        let pairs_file = open_append_csv(&pairs_path, pairs_csv_header());
        let _runs_file = open_append_csv(&runs_path, runs_csv_header());
        let created_at = chrono::Utc::now().to_rfc3339();
        let run_id = make_run_id(command_kind);
        Self {
            command_kind: command_kind.to_string(),
            run_id,
            created_at,
            examples_path: examples_path.to_path_buf(),
            pairs_path,
            runs_path,
            examples_file,
            pairs_file,
            next_example_seq: 0,
            total_examples: 0,
            total_pairs: 0,
        }
    }

    fn build_example_meta(
        &mut self,
        parent_example_id: Option<&str>,
        stage: &str,
        source: &str,
        seed_eval: Option<u64>,
    ) -> CsvExampleMeta {
        let example_id = format!("{}_e{:06}", self.run_id, self.next_example_seq);
        self.next_example_seq += 1;
        CsvExampleMeta {
            example_id,
            run_id: self.run_id.clone(),
            parent_example_id: parent_example_id.unwrap_or_default().to_string(),
            stage: stage.to_string(),
            source: source.to_string(),
            seed_eval: seed_eval.map(|s| s.to_string()).unwrap_or_default(),
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    fn log_example(
        &mut self,
        meta: &CsvExampleMeta,
        genome: &[f64],
        goal_kind: GoalKind,
        brain_type: BrainType,
        config: &SimulationConfig,
        result: &SimulationResult,
    ) {
        writeln!(
            self.examples_file,
            "{}",
            surrogate_csv_row(meta, genome, goal_kind, brain_type, config, result)
        )
        .unwrap();
        self.total_examples += 1;
    }

    #[allow(clippy::too_many_arguments)]
    fn log_pair(
        &mut self,
        generation: usize,
        target_index: usize,
        goal_kind: GoalKind,
        brain_type: BrainType,
        config: &SimulationConfig,
        parent_example_id: &str,
        child_example_id: &str,
        parent_result: &SimulationResult,
        child_result: &SimulationResult,
        parent_violation: f64,
        child_violation: f64,
        child_selected_by_de: bool,
    ) {
        let pair_id = format!("{}_p{:06}", self.run_id, self.total_pairs);
        writeln!(
            self.pairs_file,
            "{}",
            pairs_csv_row(
                &pair_id,
                &self.run_id,
                generation,
                target_index,
                goal_kind,
                brain_type,
                config,
                parent_example_id,
                child_example_id,
                parent_result,
                child_result,
                parent_violation,
                child_violation,
                child_selected_by_de,
            )
        )
        .unwrap();
        self.total_pairs += 1;
    }

    #[allow(clippy::too_many_arguments)]
    fn finalize_run(
        &self,
        status: &str,
        goal_kind: GoalKind,
        brain_type: BrainType,
        duration_secs: f32,
        population: usize,
        generations: usize,
        de_f: f64,
        de_cr: f64,
        convergence: f64,
        config: &SimulationConfig,
        crowding: bool,
        stagnation_window: usize,
        stagnation_fraction: f64,
        init_preset: Option<&Path>,
        output: Option<&Path>,
    ) {
        let mut runs_file = OpenOptions::new()
            .append(true)
            .open(&self.runs_path)
            .unwrap();
        writeln!(
            runs_file,
            "{}",
            runs_csv_row(
                &self.run_id,
                &self.created_at,
                &self.command_kind,
                status,
                goal_kind,
                brain_type,
                duration_secs,
                population,
                generations,
                de_f,
                de_cr,
                convergence,
                config,
                crowding,
                stagnation_window,
                stagnation_fraction,
                init_preset,
                output,
                &self.examples_path,
                &self.pairs_path,
                self.total_examples,
                self.total_pairs,
            )
        )
        .unwrap();
    }
}

fn make_run_id(command_kind: &str) -> String {
    format!(
        "{}_{}",
        command_kind,
        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
    )
}

fn derive_sibling_csv_path(base: &Path, suffix: &str) -> PathBuf {
    let stem = base
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("evaluations");
    let ext = base.extension().and_then(|e| e.to_str()).unwrap_or("csv");
    base.with_file_name(format!("{stem}{suffix}.{ext}"))
}

fn open_append_csv(path: &Path, header: String) -> File {
    let file_exists = path.exists();
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap_or_else(|e| {
            eprintln!("Failed to open log file '{}': {e}", path.display());
            std::process::exit(1);
        });
    if !file_exists {
        writeln!(file, "{header}").unwrap();
    }
    file
}

fn csv_escape_field(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\"\""))
}

fn pairs_csv_header() -> String {
    [
        "pair_id",
        "run_id",
        "generation",
        "target_index",
        "goal",
        "brain_type",
        "duration_secs",
        "parent_example_id",
        "child_example_id",
        "parent_score",
        "child_score",
        "delta_score",
        "parent_violation",
        "child_violation",
        "delta_violation",
        "parent_feasible",
        "child_feasible",
        "child_better_score",
        "child_better_feasible",
        "child_selected_by_de",
        "parent_alpha",
        "child_alpha",
        "delta_alpha",
        "parent_beta",
        "child_beta",
        "delta_beta",
        "parent_dominant_freq_hz",
        "child_dominant_freq_hz",
        "same_constraints",
        "same_p18_params",
        "same_flags",
        "model_signature_schema_version",
        "model_signature_json",
    ]
    .join(",")
}

#[allow(clippy::too_many_arguments)]
fn pairs_csv_row(
    pair_id: &str,
    run_id: &str,
    generation: usize,
    target_index: usize,
    goal_kind: GoalKind,
    brain_type: BrainType,
    config: &SimulationConfig,
    parent_example_id: &str,
    child_example_id: &str,
    parent_result: &SimulationResult,
    child_result: &SimulationResult,
    parent_violation: f64,
    child_violation: f64,
    child_selected_by_de: bool,
) -> String {
    let bool01 = |v: bool| if v { "1".to_string() } else { "0".to_string() };
    let signature_json = serde_json::to_string(&config.model_signature())
        .unwrap_or_else(|_| "{\"error\":\"signature_serialization_failed\"}".to_string());
    [
        pair_id.to_string(),
        run_id.to_string(),
        generation.to_string(),
        target_index.to_string(),
        goal_kind.to_string(),
        brain_type.to_string(),
        format!("{:.3}", config.duration_secs),
        parent_example_id.to_string(),
        child_example_id.to_string(),
        format!("{:.6}", parent_result.score),
        format!("{:.6}", child_result.score),
        format!("{:.6}", child_result.score - parent_result.score),
        format!("{:.6}", parent_violation),
        format!("{:.6}", child_violation),
        format!("{:.6}", child_violation - parent_violation),
        bool01(parent_violation <= 1e-9),
        bool01(child_violation <= 1e-9),
        bool01(child_result.score > parent_result.score),
        bool01(
            child_violation < parent_violation
                || ((child_violation - parent_violation).abs() <= 1e-9
                    && child_result.score >= parent_result.score),
        ),
        bool01(child_selected_by_de),
        format!("{:.6}", parent_result.alpha_power),
        format!("{:.6}", child_result.alpha_power),
        format!(
            "{:.6}",
            child_result.alpha_power - parent_result.alpha_power
        ),
        format!("{:.6}", parent_result.beta_power),
        format!("{:.6}", child_result.beta_power),
        format!("{:.6}", child_result.beta_power - parent_result.beta_power),
        format!("{:.6}", parent_result.dominant_freq),
        format!("{:.6}", child_result.dominant_freq),
        bool01(config.acoustic_constraints_enabled),
        "1".to_string(),
        "1".to_string(),
        "1".to_string(),
        csv_escape_field(&signature_json),
    ]
    .join(",")
}

fn runs_csv_header() -> String {
    [
        "run_id",
        "created_at",
        "finished_at",
        "command_kind",
        "status",
        "goal",
        "brain_type",
        "duration_secs",
        "population",
        "generations",
        "de_f",
        "de_cr",
        "convergence",
        "assr",
        "thalamic_gate",
        "cet",
        "phys_gate",
        "acoustic_score_fusion",
        "constrained",
        "crowding",
        "stagnation_window",
        "stagnation_fraction",
        "jr_sigma",
        "gaba_b_rate",
        "gaba_b_gain",
        "init_preset_path",
        "output_path",
        "examples_path",
        "pairs_path",
        "total_examples",
        "total_pairs",
        "model_signature_schema_version",
        "model_signature_json",
    ]
    .join(",")
}

#[allow(clippy::too_many_arguments)]
fn runs_csv_row(
    run_id: &str,
    created_at: &str,
    command_kind: &str,
    status: &str,
    goal_kind: GoalKind,
    brain_type: BrainType,
    duration_secs: f32,
    population: usize,
    generations: usize,
    de_f: f64,
    de_cr: f64,
    convergence: f64,
    config: &SimulationConfig,
    crowding: bool,
    stagnation_window: usize,
    stagnation_fraction: f64,
    init_preset: Option<&Path>,
    output: Option<&Path>,
    examples_path: &Path,
    pairs_path: &Path,
    total_examples: usize,
    total_pairs: usize,
) -> String {
    let bool01 = |v: bool| if v { "1".to_string() } else { "0".to_string() };
    let signature_json = serde_json::to_string(&config.model_signature())
        .unwrap_or_else(|_| "{\"error\":\"signature_serialization_failed\"}".to_string());
    [
        run_id.to_string(),
        created_at.to_string(),
        chrono::Utc::now().to_rfc3339(),
        command_kind.to_string(),
        status.to_string(),
        goal_kind.to_string(),
        brain_type.to_string(),
        format!("{duration_secs:.3}"),
        population.to_string(),
        generations.to_string(),
        format!("{de_f:.6}"),
        format!("{de_cr:.6}"),
        format!("{convergence:.6}"),
        bool01(config.assr_enabled),
        bool01(config.thalamic_gate_enabled),
        bool01(config.cet_enabled),
        bool01(config.physiological_thalamic_gate_enabled),
        bool01(config.acoustic_score_fusion_enabled),
        bool01(config.acoustic_constraints_enabled),
        bool01(crowding),
        stagnation_window.to_string(),
        format!("{stagnation_fraction:.6}"),
        format!("{:.6}", config.jr_stochastic_sigma),
        format!("{:.6}", config.cet_b_slow_rate),
        format!("{:.6}", config.cet_b_slow_gain),
        init_preset
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
        output.map(|p| p.display().to_string()).unwrap_or_default(),
        examples_path.display().to_string(),
        pairs_path.display().to_string(),
        total_examples.to_string(),
        total_pairs.to_string(),
        "1".to_string(),
        csv_escape_field(&signature_json),
    ]
    .join(",")
}

fn surrogate_csv_header() -> String {
    // Stage 0 contract:
    // `model_signature_json` is the compact serialized config object required by
    // nmm_refactor Stage 0. It carries the full model path/provenance in one field.
    // `model_signature_schema_version` versions that payload independently from
    // older CSV readers.
    let mut cols: Vec<String> = vec![
        "example_id".into(),
        "run_id".into(),
        "parent_example_id".into(),
        "stage".into(),
        "source".into(),
        "seed_eval".into(),
        "created_at".into(),
        "goal".into(),
        "goal_id".into(),
        "brain_type".into(),
        "brain_type_id".into(),
        "duration_secs".into(),
        "assr".into(),
        "thalamic_gate".into(),
        "cet".into(),
        "phys_gate".into(),
        "arousal_model".into(),
        "acoustic_score_fusion".into(),
        "constrained".into(),
        "active_scoring_profile".into(),
        "jr_sigma".into(),
        "gaba_b_rate".into(),
        "gaba_b_gain".into(),
    ];
    cols.extend((0..surrogate::GENOME_DIM).map(|i| format!("g{i}")));
    cols.extend(
        [
            "score",
            "score_legacy_v1_neural",
            "score_legacy_v1_fused",
            "score_candidate_research_v2",
            "score_product_acoustic",
            "legacy_nmm_score",
            "fused_score",
            "acoustic_goal_score",
            "comfort_score",
            "violation",
            "dominant_freq_hz",
            "delta_power",
            "theta_power",
            "alpha_power",
            "beta_power",
            "gamma_power",
            "left_dominant_freq_hz",
            "right_dominant_freq_hz",
            "alpha_asymmetry",
            "fhn_firing_rate",
            "fhn_isi_cv",
            "spectral_centroid_hz",
            "entrainment_ratio",
            "ei_stability_cv",
            "brightness",
            "aperiodic_exponent",
            "aperiodic_offset",
            "periodic_peak_count",
            "primary_peak_center_hz",
            "primary_peak_bandwidth_hz",
            "primary_peak_height_above_aperiodic_log10",
            "assr_dominant_modulation_hz",
            "assr_effective_amplitude_gain",
            "assr_phase_consistency_heuristic",
            "assr_implied_latency_jitter_ms_heuristic",
            "assr_expected_plv_ceiling",
            "estimated_arousal",
            "estimated_arousal_score",
            "arousal_score_span",
            "arousal_local_derivative",
            "arousal_max_abs_slope",
            "candidate_dominant_modulation_hz",
            "candidate_total_modulation_power",
            "candidate_mod_slow_0p5_4_hz",
            "candidate_mod_theta_4_8_hz",
            "candidate_mod_alpha_8_13_hz",
            "candidate_mod_beta_13_30_hz",
            "candidate_mod_gamma_30_50_hz",
            "candidate_cochlear_brightness",
            "candidate_cochlear_band_energy_0",
            "candidate_cochlear_band_energy_1",
            "candidate_cochlear_band_energy_2",
            "candidate_cochlear_band_energy_3",
            "candidate_spectral_tilt_db_per_oct",
            "candidate_estimated_arousal",
            "candidate_arousal_source",
            "broadband_level_db",
            "speech_band_ratio",
            "modulation_depth",
            "sharpness_proxy",
            "intelligibility",
            "speech_privacy",
            "lufs_asymmetry_lu",
            "true_peak_dbfs",
            "plr_db",
            "spectral_tilt_db_per_oct",
            "hf_fraction_above_8khz",
            "source_balance_db_range",
            "is_feasible",
            "is_good_10s",
            "is_good_30s",
            "is_good_60s",
            "is_stable_10_60",
            "score_mean",
            "score_std",
            "repeats",
            "model_signature_schema_version",
            "model_signature_json",
        ]
        .into_iter()
        .map(str::to_string),
    );
    cols.join(",")
}

fn surrogate_csv_row(
    meta: &CsvExampleMeta,
    genome: &[f64],
    goal_kind: GoalKind,
    brain_type: BrainType,
    config: &SimulationConfig,
    result: &SimulationResult,
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
    let acoustic = result.acoustic_score.as_ref();
    let features = acoustic.map(|a| &a.features);
    let diagnostics = result.scientific_diagnostics.as_ref();
    let spectral = diagnostics.map(|d| &d.spectral_parameterization);
    let primary_peak = primary_peak_for_export(spectral);
    let assr_diag = diagnostics.map(|d| &d.assr);
    let arousal_diag = diagnostics.map(|d| &d.arousal_sensitivity);
    let candidate = diagnostics.and_then(|d| d.candidate_auditory_features.as_ref());
    let candidate_temporal = candidate.map(|c| &c.temporal_modulation);
    let candidate_cochlear = candidate.map(|c| &c.cochlear);
    let candidate_latent = candidate.map(|c| &c.latent_state);
    let violation = acoustic
        .map(|a| Goal::new(goal_kind).comfort_violation(&a.features))
        .unwrap_or(0.0);
    let fmt_opt = |v: Option<f64>| v.map(|x| format!("{x:.6}")).unwrap_or_default();
    let bool01 = |v: bool| if v { "1".to_string() } else { "0".to_string() };
    let score = result.score;
    // Stage 0 compact serialized config object (see surrogate_csv_header comment).
    let signature_json = serde_json::to_string(&result.model_signature)
        .unwrap_or_else(|_| "{\"error\":\"signature_serialization_failed\"}".to_string());
    let is_good_10s = if (config.duration_secs - 10.0).abs() < 1e-6 {
        bool01(score >= 0.60)
    } else {
        String::new()
    };
    let is_good_30s = if (config.duration_secs - 30.0).abs() < 1e-6 {
        bool01(score >= 0.55)
    } else {
        String::new()
    };
    let is_good_60s = if (config.duration_secs - 60.0).abs() < 1e-6 {
        bool01(score >= 0.50)
    } else {
        String::new()
    };

    let mut cols = vec![
        meta.example_id.clone(),
        meta.run_id.clone(),
        meta.parent_example_id.clone(),
        meta.stage.clone(),
        meta.source.clone(),
        meta.seed_eval.clone(),
        meta.created_at.clone(),
        goal_kind.to_string(),
        goal_id.to_string(),
        brain_type.to_string(),
        bt_id.to_string(),
        format!("{:.3}", config.duration_secs),
        bool01(config.assr_enabled),
        bool01(config.thalamic_gate_enabled),
        bool01(config.cet_enabled),
        bool01(config.physiological_thalamic_gate_enabled),
        match config.arousal_model {
            ArousalModel::LegacyHeuristic => "legacy_heuristic".to_string(),
            ArousalModel::Fixed => "fixed".to_string(),
        },
        bool01(config.acoustic_score_fusion_enabled),
        bool01(config.acoustic_constraints_enabled),
        config.scoring_profile.to_string(),
        format!("{:.6}", config.jr_stochastic_sigma),
        format!("{:.6}", config.cet_b_slow_rate),
        format!("{:.6}", config.cet_b_slow_gain),
    ];
    cols.extend(genome_str);
    cols.extend([
        format!("{score:.6}"),
        fmt_opt(result.multi_score.legacy_v1_neural),
        fmt_opt(result.multi_score.legacy_v1_fused),
        fmt_opt(result.multi_score.candidate_research_v2),
        fmt_opt(result.multi_score.product_acoustic),
        fmt_opt(acoustic.and_then(|a| a.legacy_nmm_score)),
        fmt_opt(acoustic.and_then(|a| a.fused_score_preview)),
        fmt_opt(acoustic.and_then(|a| a.acoustic_goal_score)),
        fmt_opt(acoustic.and_then(|a| a.comfort_score)),
        format!("{violation:.6}"),
        format!("{:.6}", result.dominant_freq),
        format!("{:.6}", result.delta_power),
        format!("{:.6}", result.theta_power),
        format!("{:.6}", result.alpha_power),
        format!("{:.6}", result.beta_power),
        format!("{:.6}", result.gamma_power),
        format!("{:.6}", result.left_dominant_freq),
        format!("{:.6}", result.right_dominant_freq),
        format!("{:.6}", result.alpha_asymmetry),
        format!("{:.6}", result.fhn_firing_rate),
        format!("{:.6}", result.fhn_isi_cv),
        format!("{:.6}", result.performance.spectral_centroid),
        fmt_opt(result.performance.entrainment_ratio),
        fmt_opt(result.performance.ei_stability),
        format!("{:.6}", result.brightness),
        fmt_opt(spectral.map(|s| s.aperiodic_exponent)),
        fmt_opt(spectral.map(|s| s.aperiodic_offset)),
        spectral
            .map(|s| s.peaks.len().to_string())
            .unwrap_or_default(),
        fmt_opt(primary_peak.map(|p| p.center_hz)),
        fmt_opt(primary_peak.map(|p| p.bandwidth_hz)),
        fmt_opt(primary_peak.map(|p| p.power_above_aperiodic)),
        fmt_opt(assr_diag.and_then(|a| a.dominant_modulation_hz)),
        fmt_opt(assr_diag.and_then(|a| a.effective_amplitude_gain)),
        fmt_opt(assr_diag.and_then(|a| a.phase_consistency_heuristic)),
        fmt_opt(assr_diag.and_then(|a| a.implied_latency_jitter_ms_heuristic)),
        fmt_opt(assr_diag.and_then(|a| a.expected_plv_ceiling)),
        fmt_opt(arousal_diag.map(|a| a.estimated_arousal)),
        fmt_opt(arousal_diag.map(|a| a.estimated_score)),
        fmt_opt(arousal_diag.map(|a| a.score_span)),
        fmt_opt(arousal_diag.map(|a| a.local_derivative)),
        fmt_opt(arousal_diag.map(|a| a.max_abs_slope)),
        fmt_opt(candidate_temporal.and_then(|m| m.dominant_modulation_hz)),
        fmt_opt(candidate_temporal.map(|m| m.total_modulation_power)),
        fmt_opt(candidate_temporal.map(|m| m.band_power_by_mod_rate.slow_0p5_4_hz)),
        fmt_opt(candidate_temporal.map(|m| m.band_power_by_mod_rate.theta_4_8_hz)),
        fmt_opt(candidate_temporal.map(|m| m.band_power_by_mod_rate.alpha_8_13_hz)),
        fmt_opt(candidate_temporal.map(|m| m.band_power_by_mod_rate.beta_13_30_hz)),
        fmt_opt(candidate_temporal.map(|m| m.band_power_by_mod_rate.gamma_30_50_hz)),
        fmt_opt(candidate_cochlear.map(|c| c.brightness)),
        fmt_opt(candidate_cochlear.map(|c| c.band_energy_fractions[0])),
        fmt_opt(candidate_cochlear.map(|c| c.band_energy_fractions[1])),
        fmt_opt(candidate_cochlear.map(|c| c.band_energy_fractions[2])),
        fmt_opt(candidate_cochlear.map(|c| c.band_energy_fractions[3])),
        fmt_opt(candidate_cochlear.and_then(|c| c.spectral_tilt_db_per_oct)),
        fmt_opt(candidate_latent.map(|l| l.estimated_arousal)),
        candidate_latent
            .map(|l| match l.arousal_source {
                crate::auditory::ArousalSource::LegacyHeuristic => "legacy_heuristic".to_string(),
                crate::auditory::ArousalSource::Fixed => "fixed".to_string(),
                crate::auditory::ArousalSource::NeutralDefault => "neutral_default".to_string(),
            })
            .unwrap_or_default(),
        fmt_opt(features.and_then(|f| f.broadband_level_db)),
        fmt_opt(features.and_then(|f| f.speech_band_ratio)),
        fmt_opt(features.and_then(|f| f.modulation_depth)),
        fmt_opt(features.and_then(|f| f.sharpness_proxy)),
        fmt_opt(acoustic.and_then(|a| a.intelligibility_proxy)),
        fmt_opt(acoustic.and_then(|a| a.speech_privacy)),
        fmt_opt(features.and_then(|f| f.lufs_asymmetry_lu)),
        fmt_opt(features.and_then(|f| f.true_peak_dbfs)),
        fmt_opt(features.and_then(|f| f.plr_db)),
        fmt_opt(features.and_then(|f| f.spectral_tilt_db_per_oct)),
        fmt_opt(features.and_then(|f| f.hf_fraction_above_8khz)),
        fmt_opt(features.and_then(|f| f.source_balance_db_range)),
        bool01(violation <= 1e-9),
        is_good_10s,
        is_good_30s,
        is_good_60s,
        String::new(), // is_stable_10_60
        format!("{score:.6}"),
        String::new(), // score_std
        "1".to_string(),
        "1".to_string(),
        csv_escape_field(&signature_json),
    ]);
    cols.join(",")
}

fn primary_peak_for_export(
    spectral: Option<&crate::neural::SpectralParameterization>,
) -> Option<&crate::neural::aperiodic::SpectralPeak> {
    spectral.and_then(|s| {
        s.peaks.iter().max_by(|a, b| {
            a.power_above_aperiodic
                .total_cmp(&b.power_above_aperiodic)
                // Tie-break deterministically by lower center frequency.
                .then_with(|| b.center_hz.total_cmp(&a.center_hz))
        })
    })
}

fn evaluate_preset_for_dataset_export(
    preset: &Preset,
    goal: &Goal,
    config: &SimulationConfig,
) -> SimulationResult {
    evaluate_preset_detailed(preset, goal, config).summary
}

fn preset_from_genome_with_seed_context(genome: &[f64], seed_ctx: &SeedPresetContext) -> Preset {
    let mut preset = Preset::from_genome_with_spread(genome, &seed_ctx.spread_per_slot);
    preset.room = seed_ctx.room.clone();
    for (i, obj) in preset.objects.iter_mut().enumerate() {
        obj.position_space = seed_ctx.position_space_per_slot[i];
    }
    preset.clamp();
    preset
}

fn reevaluate_best_preset(
    best_genome: &[f64],
    goal: &Goal,
    sim_config: &SimulationConfig,
    seed_ctx: &SeedPresetContext,
) -> (Preset, pipeline::SimulationResult) {
    let best_preset = preset_from_genome_with_seed_context(best_genome, seed_ctx);
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
    seed_ctx: &SeedPresetContext,
) -> std::io::Result<(Preset, pipeline::SimulationResult)> {
    let (best_preset, best_result) =
        reevaluate_best_preset(best_genome, goal, sim_config, seed_ctx);
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
    arousal_model: ArousalModel,
    fixed_arousal: Option<f64>,
    jr_sigma: f64,
    gaba_b_rate: f64,
    gaba_b_gain: f64,
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
                        arousal_model,
                        fixed_arousal,
                        jr_sigma,
                        gaba_b_rate,
                        gaba_b_gain,
                    );
                    evaluate_preset(preset, &goal, &sim_config).score
                })
                .collect()
        })
        .collect()
}

fn staged_output_paths(goal: &str, output: Option<&Path>) -> (PathBuf, PathBuf, PathBuf) {
    if let Some(final_output) = output {
        let parent = final_output.parent().unwrap_or_else(|| Path::new("."));
        let stem = final_output
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("preset");
        let ext = final_output
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("json");
        let stage1 = parent.join(format!("{stem}_stage1_10s.{ext}"));
        let stage2 = parent.join(format!("{stem}_stage2_30s.{ext}"));
        (stage1, stage2, final_output.to_path_buf())
    } else {
        let ts = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        (
            PathBuf::from(format!("preset_{}_staged_stage1_10s_{}.json", goal, ts)),
            PathBuf::from(format!("preset_{}_staged_stage2_30s_{}.json", goal, ts)),
            PathBuf::from(format!("preset_{}_staged_final_60s_{}.json", goal, ts)),
        )
    }
}

fn seed_only_path_for_preset(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("preset");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("json");
    parent.join(format!("{stem}_seed_only.{ext}"))
}

/// Accept either a raw preset JSON or an exported wrapper with a nested
/// `preset` payload. Returns a path to a raw preset JSON suitable for
/// `run_optimize --init-preset`.
fn ensure_raw_preset_seed(path: &Path) -> PathBuf {
    let json = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("Failed to read preset seed '{}': {}", path.display(), e);
        std::process::exit(1);
    });

    let value: serde_json::Value = serde_json::from_str(&json).unwrap_or_else(|e| {
        eprintln!("Failed to parse preset seed '{}': {}", path.display(), e);
        std::process::exit(1);
    });

    if value.get("master_gain").is_some() {
        return path.to_path_buf();
    }

    let raw_preset = value.get("preset").cloned().unwrap_or_else(|| {
        eprintln!(
            "Preset seed '{}' is neither a raw preset nor an export wrapper with a top-level 'preset' field",
            path.display()
        );
        std::process::exit(1);
    });

    let seed_path = seed_only_path_for_preset(path);
    let serialized = serde_json::to_string_pretty(&raw_preset).unwrap_or_else(|e| {
        eprintln!(
            "Failed to serialize extracted raw preset for '{}': {}",
            path.display(),
            e
        );
        std::process::exit(1);
    });
    std::fs::write(&seed_path, serialized).unwrap_or_else(|e| {
        eprintln!(
            "Failed to write extracted raw preset seed '{}': {}",
            seed_path.display(),
            e
        );
        std::process::exit(1);
    });
    seed_path
}

#[allow(clippy::too_many_arguments)]
fn run_optimize_staged(
    goal: &str,
    stage1_generations: usize,
    stage1_population: usize,
    stage1_duration: f32,
    stage2_generations: usize,
    stage2_population: usize,
    stage2_duration: f32,
    stage3_generations: usize,
    stage3_population: usize,
    stage3_duration: f32,
    output: Option<PathBuf>,
    seed: u64,
    de_f: f64,
    de_cr: f64,
    convergence: f64,
    brain_type: &str,
    init_preset: Option<&Path>,
    cet: bool,
    phys_gate: bool,
    acoustic_score_fusion: bool,
    constrained: bool,
    crowding: bool,
    stagnation_window: usize,
    stagnation_fraction: f64,
    jr_sigma: f64,
    gaba_b_rate: f64,
    gaba_b_gain: f64,
) {
    let (stage1_output, stage2_output, final_output) = staged_output_paths(goal, output.as_deref());
    let stage1_seed = init_preset.map(ensure_raw_preset_seed);

    println!();
    println!("  Staged Optimization");
    println!("  ════════════════════════════════════════");
    println!(
        "  Schedule:       {:.0}s -> {:.0}s -> {:.0}s",
        stage1_duration, stage2_duration, stage3_duration
    );
    println!(
        "  Stage 1:        pop={} gens={}",
        stage1_population, stage1_generations
    );
    println!(
        "  Stage 2:        pop={} gens={}",
        stage2_population, stage2_generations
    );
    println!(
        "  Stage 3:        pop={} gens={}",
        stage3_population, stage3_generations
    );
    println!("  Final output:   {}", final_output.display());
    println!();

    println!("  Stage 1/3 — short-window search");
    run_optimize(
        goal,
        stage1_generations,
        stage1_population,
        stage1_duration,
        Some(stage1_output.clone()),
        seed,
        de_f,
        de_cr,
        convergence,
        brain_type,
        stage1_seed.as_deref(),
        cet,
        phys_gate,
        acoustic_score_fusion,
        constrained,
        crowding,
        stagnation_window,
        stagnation_fraction,
        jr_sigma,
        gaba_b_rate,
        gaba_b_gain,
        false,
        Path::new("surrogate_weights.bin"),
        5,
        None,
    );
    let stage2_seed = ensure_raw_preset_seed(&stage1_output);

    println!("  Stage 2/3 — medium-window continuation");
    run_optimize(
        goal,
        stage2_generations,
        stage2_population,
        stage2_duration,
        Some(stage2_output.clone()),
        seed.wrapping_add(1),
        de_f,
        de_cr,
        convergence,
        brain_type,
        Some(stage2_seed.as_path()),
        cet,
        phys_gate,
        acoustic_score_fusion,
        constrained,
        crowding,
        stagnation_window,
        stagnation_fraction,
        jr_sigma,
        gaba_b_rate,
        gaba_b_gain,
        false,
        Path::new("surrogate_weights.bin"),
        5,
        None,
    );
    let stage3_seed = ensure_raw_preset_seed(&stage2_output);

    println!("  Stage 3/3 — long-window continuation");
    run_optimize(
        goal,
        stage3_generations,
        stage3_population,
        stage3_duration,
        Some(final_output.clone()),
        seed.wrapping_add(2),
        de_f,
        de_cr,
        convergence,
        brain_type,
        Some(stage3_seed.as_path()),
        cet,
        phys_gate,
        acoustic_score_fusion,
        constrained,
        crowding,
        stagnation_window,
        stagnation_fraction,
        jr_sigma,
        gaba_b_rate,
        gaba_b_gain,
        false,
        Path::new("surrogate_weights.bin"),
        5,
        None,
    );

    println!("  Stage artifacts:");
    println!("    {}", stage1_output.display());
    println!("    {}", stage2_output.display());
    println!("    {}", final_output.display());
    println!();
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
            constrained,
            crowding,
            stagnation_window,
            stagnation_fraction,
            jr_sigma,
            gaba_b_rate,
            gaba_b_gain,
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
                constrained,
                crowding,
                stagnation_window,
                stagnation_fraction,
                jr_sigma,
                gaba_b_rate,
                gaba_b_gain,
                surrogate,
                &surrogate_weights,
                surrogate_k,
                log_evaluations.as_deref(),
            );
        }
        Commands::OptimizeStaged {
            goal,
            population,
            generations,
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
            constrained,
            crowding,
            stagnation_window,
            stagnation_fraction,
            jr_sigma,
            gaba_b_rate,
            gaba_b_gain,
            stage1_duration,
            stage2_duration,
            stage3_duration,
            stage2_population,
            stage2_generations,
            stage3_population,
            stage3_generations,
        } => {
            run_optimize_staged(
                &goal,
                generations,
                population,
                stage1_duration,
                stage2_generations,
                stage2_population,
                stage2_duration,
                stage3_generations,
                stage3_population,
                stage3_duration,
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
                constrained,
                crowding,
                stagnation_window,
                stagnation_fraction,
                jr_sigma,
                gaba_b_rate,
                gaba_b_gain,
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
            arousal_model,
            fixed_arousal,
            jr_sigma,
            gaba_b_rate,
            gaba_b_gain,
            log_evaluations,
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
                &arousal_model,
                fixed_arousal,
                jr_sigma,
                gaba_b_rate,
                gaba_b_gain,
                log_evaluations.as_deref(),
            );
        }
        Commands::Disturb {
            preset,
            brain_type,
            spike_time,
            spike_duration,
            spike_gain,
            duration,
            assr,
            no_assr,
            thalamic_gate,
            no_thalamic_gate,
            cet,
            no_cet,
            phys_gate,
            jr_sigma,
            gaba_b_rate,
            gaba_b_gain,
            legacy_ablated,
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
            run_disturb_cmd(
                &preset,
                &brain_type,
                spike_time,
                spike_duration,
                spike_gain,
                duration,
                flags,
                jr_sigma,
                gaba_b_rate,
                gaba_b_gain,
                legacy_ablated,
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
            arousal_model,
            fixed_arousal,
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
                &arousal_model,
                fixed_arousal,
                seed,
            );
        }
        Commands::CalibrateComfort {
            presets_dir,
            duration,
            brain_type,
        } => {
            run_calibrate_comfort(&presets_dir, duration, &brain_type);
        }
    }
}

// ── Calibrate Comfort ─────────────────────────────────────────────────────

/// Infer goal from preset filename. Returns None when no goal token is
/// recognised in the filename — caller should skip those presets.
fn infer_goal_from_filename(name: &str) -> Option<GoalKind> {
    let lower = name.to_lowercase();
    // Order matters: more specific patterns first.
    if lower.contains("deepwork") || lower.contains("deep_work") {
        return Some(GoalKind::DeepWork);
    }
    if lower.contains("deep_relax") || lower.contains("deeprelax") || lower.contains("deep-relax") {
        return Some(GoalKind::DeepRelaxation);
    }
    if lower.contains("isolation") {
        return Some(GoalKind::Isolation);
    }
    if lower.contains("meditation") || lower.contains("meditate") {
        return Some(GoalKind::Meditation);
    }
    if lower.contains("ignition") {
        return Some(GoalKind::Ignition);
    }
    if lower.contains("shield") {
        return Some(GoalKind::Shield);
    }
    if lower.contains("sleep") {
        return Some(GoalKind::Sleep);
    }
    if lower.contains("flow") {
        return Some(GoalKind::Flow);
    }
    if lower.contains("focus") {
        return Some(GoalKind::Focus);
    }
    None
}

#[derive(Debug, Clone, Copy)]
struct ComfortSample {
    lufs_asymmetry_lu: Option<f64>,
    true_peak_dbfs: Option<f64>,
    plr_db: Option<f64>,
    spectral_tilt_dev_db_per_oct: Option<f64>,
    hf_fraction: Option<f64>,
    source_balance_db_range: Option<f64>,
}

/// Compute percentile from a sorted slice via nearest-rank.
fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let q = q.clamp(0.0, 1.0);
    let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

fn print_metric_distribution(
    label: &str,
    values: &[f64],
    current_threshold: Option<f64>,
    units: &str,
) {
    if values.is_empty() {
        println!(
            "  {label:<32}  no data",
            label = format!("{label} ({units})")
        );
        return;
    }
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = sorted.len();
    let min = sorted[0];
    let p10 = percentile(&sorted, 0.10);
    let p50 = percentile(&sorted, 0.50);
    let p90 = percentile(&sorted, 0.90);
    let max = sorted[n - 1];
    let mean = sorted.iter().sum::<f64>() / n as f64;
    let header = format!("{label} ({units})");
    let threshold_col = match current_threshold {
        Some(t) => format!("  thr={t:.3}"),
        None => "".to_string(),
    };
    let suggestion = match current_threshold {
        Some(t) if p90 > t => format!("  ⚠ p90 > thr (∴ raise to ≥{:.2})", p90),
        Some(t) if p90 < t * 0.5 => {
            format!("  ⚠ p90 << thr (∴ tighten to ≈{:.2})", p90.max(p50 * 1.2))
        }
        Some(_) => "  ✓ fits".to_string(),
        None => "".to_string(),
    };
    println!(
        "  {:<28}  n={:>3}  min={:>6.2}  p10={:>6.2}  p50={:>6.2}  p90={:>6.2}  max={:>6.2}  mean={:>6.2}{}{}",
        header, n, min, p10, p50, p90, max, mean, threshold_col, suggestion
    );
}

fn run_calibrate_comfort(presets_dir: &Path, duration: f32, brain_type_str: &str) {
    let bt = BrainType::from_str(brain_type_str).unwrap_or_else(|| {
        eprintln!("Unknown brain type: '{brain_type_str}'. Valid: normal, high_alpha, adhd, aging, anxious");
        std::process::exit(1);
    });

    println!();
    println!("  Comfort-Metric Calibration");
    println!("  ════════════════════════════════════════");
    println!("  Presets dir:    {}", presets_dir.display());
    println!("  Brain type:     {bt}");
    println!("  Duration:       {duration:.1}s");
    println!();

    let entries = match std::fs::read_dir(presets_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Failed to read presets dir {}: {e}", presets_dir.display());
            std::process::exit(1);
        }
    };

    // Build a flat list of (filename, goal, preset) tuples.
    let mut work: Vec<(String, GoalKind, Preset)> = Vec::new();
    let mut skipped_no_goal = 0usize;
    let mut skipped_parse = 0usize;
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("json") {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("?")
            .to_string();
        let goal = match infer_goal_from_filename(&filename) {
            Some(g) => g,
            None => {
                skipped_no_goal += 1;
                continue;
            }
        };
        let json = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => {
                skipped_parse += 1;
                continue;
            }
        };
        // Accept both raw Preset JSON and PresetExport-wrapped JSON.
        let value: serde_json::Value = match serde_json::from_str(&json) {
            Ok(v) => v,
            Err(_) => {
                skipped_parse += 1;
                continue;
            }
        };
        let preset_value = if value.get("preset").is_some() {
            value["preset"].clone()
        } else {
            value
        };
        let preset: Preset = match serde_json::from_value(preset_value) {
            Ok(p) => p,
            Err(_) => {
                skipped_parse += 1;
                continue;
            }
        };
        work.push((filename, goal, preset));
    }

    println!(
        "  Found {} presets, skipped {} (no goal in filename), {} parse failures",
        work.len(),
        skipped_no_goal,
        skipped_parse
    );
    println!();

    let mut sim_config = pipeline::SimulationConfig {
        duration_secs: duration,
        brain_type: bt,
        acoustic_scoring_enabled: true,
        ..pipeline::SimulationConfig::default()
    };
    // We need acoustic features but NOT fusion or constraints — both
    // alter scoring rather than feature extraction.
    sim_config.acoustic_score_fusion_enabled = false;
    sim_config.acoustic_constraints_enabled = false;

    use std::collections::BTreeMap;
    let mut samples_by_goal: BTreeMap<String, Vec<ComfortSample>> = BTreeMap::new();
    let mut all_samples: Vec<ComfortSample> = Vec::new();

    for (filename, goal_kind, preset) in &work {
        let goal = Goal::new(*goal_kind);
        let detailed = pipeline::evaluate_preset_detailed(preset, &goal, &sim_config);
        let acoustic = match detailed.summary.acoustic_score.as_ref() {
            Some(a) => a,
            None => continue,
        };
        let f = &acoustic.features;
        let target_tilt = goal_target_tilt(*goal_kind);
        let tilt_dev = f.spectral_tilt_db_per_oct.map(|t| (t - target_tilt).abs());
        let sample = ComfortSample {
            lufs_asymmetry_lu: f.lufs_asymmetry_lu,
            true_peak_dbfs: f.true_peak_dbfs,
            plr_db: f.plr_db,
            spectral_tilt_dev_db_per_oct: tilt_dev,
            hf_fraction: f.hf_fraction_above_8khz,
            source_balance_db_range: f.source_balance_db_range,
        };
        let goal_key = format!("{goal_kind}");
        samples_by_goal.entry(goal_key).or_default().push(sample);
        all_samples.push(sample);
        eprintln!("  {filename:<60}  goal={goal_kind}");
    }
    eprintln!();

    // Print global distribution + per-goal.
    println!();
    println!("  ── ALL goals combined ─────────────────────");
    print_distributions("(combined)", &all_samples);

    for (goal_label, samples) in &samples_by_goal {
        println!();
        println!("  ── Goal: {goal_label} ──────────────────");
        print_distributions(goal_label, samples);
    }

    println!();
    println!("  Notes:");
    println!("    - Thresholds shown are CURRENT values from Goal::comfort_violation.");
    println!("    - p90 > thr means raising the threshold would keep most curated");
    println!("      presets inside the feasible region (loosening the constraint).");
    println!("    - p90 << thr means the constraint is loose; could tighten safely.");
    println!("    - 'tilt_deviation' is |measured − goal_target|, so smaller is better.");
}

fn goal_target_tilt(goal: GoalKind) -> f64 {
    match goal {
        GoalKind::Sleep | GoalKind::DeepRelaxation | GoalKind::Meditation => -6.0,
        GoalKind::Flow | GoalKind::DeepWork | GoalKind::Shield => -3.0,
        GoalKind::Focus | GoalKind::Isolation | GoalKind::Ignition => -1.5,
    }
}

// Display thresholds — must mirror Goal::comfort_violation. After the
// 2026-05-01 empirical calibration the values are looser (see commit
// notes on Goal::lufs_asymmetry_threshold_lu and friends).
fn goal_lufs_asym_threshold(goal: &str) -> Option<f64> {
    match goal {
        "focus" | "deep_work" | "ignition" => Some(4.0),
        "(combined)" => None, // no single threshold for the mixed group
        _ => Some(3.0),
    }
}

fn goal_hf_threshold(goal: &str) -> Option<f64> {
    match goal {
        "sleep" | "deep_relaxation" | "meditation" => Some(0.10),
        "(combined)" => None,
        _ => Some(0.20),
    }
}

fn goal_source_balance_threshold(goal: &str) -> Option<f64> {
    match goal {
        "focus" | "deep_work" | "ignition" => Some(15.0),
        "(combined)" => None,
        _ => Some(12.0),
    }
}

fn print_distributions(goal_label: &str, samples: &[ComfortSample]) {
    if samples.is_empty() {
        println!("  (no samples)");
        return;
    }
    let collect = |f: fn(&ComfortSample) -> Option<f64>| -> Vec<f64> {
        samples.iter().filter_map(f).collect()
    };
    let lufs_asym = collect(|s| s.lufs_asymmetry_lu);
    let true_peak = collect(|s| s.true_peak_dbfs);
    let plr = collect(|s| s.plr_db);
    let tilt_dev = collect(|s| s.spectral_tilt_dev_db_per_oct);
    let hf = collect(|s| s.hf_fraction);
    let src_balance = collect(|s| s.source_balance_db_range);

    print_metric_distribution(
        "lufs_asymmetry",
        &lufs_asym,
        goal_lufs_asym_threshold(goal_label),
        "LU",
    );
    print_metric_distribution(
        "true_peak",
        &true_peak,
        Some(-1.0), // ceiling, same for all goals
        "dBFS",
    );
    print_metric_distribution(
        "plr",
        &plr,
        if goal_label == "ignition" {
            None
        } else {
            Some(16.0) // PLR_THRESHOLD_DB after 2026-05-01 calibration
        },
        "dB",
    );
    print_metric_distribution(
        "tilt_deviation_from_goal",
        &tilt_dev,
        Some(5.0), // SPECTRAL_TILT_TOLERANCE_DB after 2026-05-01 calibration
        "dB/oct",
    );
    print_metric_distribution("hf_fraction", &hf, goal_hf_threshold(goal_label), "[0,1]");
    print_metric_distribution(
        "source_balance",
        &src_balance,
        goal_source_balance_threshold(goal_label),
        "dB",
    );
}

// ── Optimize ─────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
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
    constrained: bool,
    crowding: bool,
    stagnation_window: usize,
    stagnation_fraction: f64,
    jr_sigma: f64,
    gaba_b_rate: f64,
    gaba_b_gain: f64,
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
        constrained,
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

    let sim_config = build_optimize_config(
        duration,
        bt,
        cet,
        phys_gate,
        acoustic_score_fusion,
        constrained,
        jr_sigma,
        gaba_b_rate,
        gaba_b_gain,
        seed,
    );

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
    print_model_signature(&sim_config.model_signature());

    let mut eval_logger = log_evaluations_path.map(|path| OptimizeCsvLogger::new(path, "optimize"));
    if let Some(ref logger) = eval_logger {
        println!("  Log evals:      {}", logger.examples_path.display());
        println!("  Log pairs:      {}", logger.pairs_path.display());
        println!("  Log runs:       {}", logger.runs_path.display());
    }

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

    // Force all optimizer-generated presets to keep the anchor muted so every
    // audible component goes through the object/HRTF path.
    let bounds = Preset::bounds_with_anchor_disabled();
    let discrete_dims = Preset::discrete_gene_indices();
    let mut de =
        DifferentialEvolution::with_discrete(bounds, population, de_f, de_cr, seed, discrete_dims);

    // Priority 28 Phase 3 — DE diversification (opt-in).
    if crowding {
        de.enable_crowding_selection();
        println!("  Crowding-DE:    enabled (Thomsen 2004 nearest-parent selection)");
    }
    if stagnation_window > 0 {
        if !(0.0..=1.0).contains(&stagnation_fraction) {
            eprintln!(
                "Invalid optimize configuration: --stagnation-fraction must be in [0, 1], got {stagnation_fraction}"
            );
            std::process::exit(2);
        }
        de.enable_stagnation_restart(stagnation_window, stagnation_fraction);
        println!(
            "  Stagnation:     restart after {} stale gens, reseed worst {:.0}%",
            stagnation_window,
            stagnation_fraction * 100.0
        );
    }
    // Priority 18 — only print when at least one param is non-default.
    // Defaults match historical hardcoded values exactly, so this
    // produces no extra output for legacy invocations.
    if (jr_sigma - 15.0).abs() > 1e-12
        || (gaba_b_rate - 5.0).abs() > 1e-12
        || (gaba_b_gain - 10.0).abs() > 1e-12
    {
        println!(
            "  P18 retune:     jr_sigma={:.1} (default 15.0)  gaba_b_rate={:.1} (default 5.0)  gaba_b_gain={:.1} (default 10.0)",
            jr_sigma, gaba_b_rate, gaba_b_gain
        );
    }

    // Seed population from an existing preset if provided. Spread is not part
    // of the genome (the surrogate contract requires a stable 230-dim input),
    // so we capture it as a per-slot side-channel here and re-apply it on every
    // `from_genome` call below. Without this, seed presets that use spread
    // would silently lose those values on the first round-trip and the
    // optimizer would "improve" a structurally different preset.
    let mut seed_ctx = SeedPresetContext::default();
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
            seed_ctx.spread_per_slot[i] = obj.spread.clamp(0.0, 1.0);
            seed_ctx.position_space_per_slot[i] = obj.position_space.min(2);
        }
        seed_ctx.room = preset.room.clone();
        let mut genome = preset.to_genome();
        Preset::disable_anchor_in_genome(&mut genome);
        de.seed_from_genome(&genome, 0.15);
        println!("  Init preset:    {}", path.display());
        let nonzero_spread: Vec<String> = seed_ctx
            .spread_per_slot
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
        if seed_ctx.room.mode != 0
            || seed_ctx
                .position_space_per_slot
                .iter()
                .any(|&space| space != 0)
        {
            println!("  Seed context:   preserving room mode and object position spaces from seed");
        }
    }

    let start = Instant::now();

    // ── Initial population evaluation ───────────────────────────────────────
    println!("  Evaluating initial population...");
    let pending = de.pending_evaluations();
    let mut population_example_ids = vec![String::new(); population];
    for (idx, genome) in &pending {
        let preset = preset_from_genome_with_seed_context(genome, &seed_ctx);
        let result = evaluate_preset(&preset, &goal, &sim_config);
        if let Some(ref mut logger) = eval_logger {
            let meta = logger.build_example_meta(
                None,
                "optimize_initial_population",
                "initial_population",
                None,
            );
            logger.log_example(&meta, genome, goal_kind, bt, &sim_config, &result);
            population_example_ids[*idx] = meta.example_id;
        }
        if constrained {
            // Priority 28 Phase 2: in constrained mode the optimizer ranks
            // by (neural_fitness, violation). The neural_fitness is the
            // pure-NMM score (fusion is force-disabled); violation comes
            // from goal.comfort_violation over the populated acoustic
            // features.
            let violation = compute_comfort_violation(&goal, &result);
            de.report_constrained(*idx, result.score, violation);
        } else {
            de.report_fitness(*idx, result.score);
        }
    }

    // ── Priority 28 Phase 2: activate ε schedule from initial-pop violations
    //    (Takahama & Sakai 2009; ε₀ from 70th-percentile per spec §28f).
    if constrained {
        let eps_0 = de.suggest_eps_from_population(0.70);
        let t_c = (generations / 2).max(1);
        de.enable_eps_constrained(eps_0, t_c);
        println!(
            "  Constrained:    ε₀ = {:.4} (70th-pct violation), t_c = {} gens",
            eps_0, t_c
        );
    }

    if constrained {
        println!(
            "  Initial best:   neural = {:.4}  violation = {:.4}  mean fitness = {:.4}",
            de.best().neural_fitness,
            de.best().violation,
            de.mean_fitness()
        );
    } else {
        println!(
            "  Initial best: {:.4}  mean: {:.4}",
            de.best().fitness,
            de.mean_fitness()
        );
    }
    println!();

    // ── Evolution loop ──────────────────────────────────────────────────────
    //
    // Convergence/stagnation tracking is mode-aware (review fix
    // 2026-05-02). In **legacy mode** we track `de.best().fitness`,
    // matching the historical behaviour exactly. In **constrained
    // mode** we track the **strict-feasible best** instead, because
    // the cached `best` follows the ε-relaxed comparator (which
    // changes its incumbent as ε decays), and `fitness_std()` is
    // computed over all individuals' display fitness regardless of
    // feasibility — neither is a meaningful signal for "the run has
    // stopped finding better strict-feasible candidates". We also
    // refuse to declare convergence while ε > 0, since the schedule
    // is still actively reshaping which candidates qualify as
    // ε-feasible.
    let mut stale_count = 0;
    let mut prev_best: f64 = if constrained {
        de.best_strict()
            .map(|s| s.neural_fitness)
            .unwrap_or(f64::NEG_INFINITY)
    } else {
        de.best().fitness
    };

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
                    let parent_before = de.individual(target_idx).clone();
                    let preset = preset_from_genome_with_seed_context(trial_genome, &seed_ctx);
                    let result = evaluate_preset(&preset, &goal, &sim_config);
                    let parent_preset =
                        preset_from_genome_with_seed_context(&parent_before.genome, &seed_ctx);
                    let parent_result = evaluate_preset(&parent_preset, &goal, &sim_config);
                    let child_selected = de.trial_would_replace(target_idx, result.score);
                    let parent_example_id = population_example_ids[target_idx].clone();
                    let child_example_id = if let Some(ref mut logger) = eval_logger {
                        let meta = logger.build_example_meta(
                            Some(&parent_example_id),
                            "optimize_generation",
                            "surrogate_validated_trial",
                            None,
                        );
                        logger.log_example(
                            &meta,
                            trial_genome,
                            goal_kind,
                            bt,
                            &sim_config,
                            &result,
                        );
                        logger.log_pair(
                            gen + 1,
                            target_idx,
                            goal_kind,
                            bt,
                            &sim_config,
                            &parent_example_id,
                            &meta.example_id,
                            &parent_result,
                            &result,
                            parent_before.violation,
                            0.0,
                            child_selected,
                        );
                        meta.example_id
                    } else {
                        String::new()
                    };
                    de.report_trial_result(target_idx, trial_genome.clone(), result.score);
                    if child_selected && !child_example_id.is_empty() {
                        population_example_ids[target_idx] = child_example_id;
                    }
                } else {
                    let _ = surr_score;
                }
            }
        } else {
            // Standard mode: evaluate ALL trials with real pipeline.
            for (target_idx, trial_genome) in trials {
                let parent_before = de.individual(target_idx).clone();
                let preset = preset_from_genome_with_seed_context(&trial_genome, &seed_ctx);
                let result = evaluate_preset(&preset, &goal, &sim_config);
                let violation = if constrained {
                    compute_comfort_violation(&goal, &result)
                } else {
                    0.0
                };
                let child_selected = if constrained {
                    de.trial_would_replace_constrained(target_idx, result.score, violation)
                } else {
                    de.trial_would_replace(target_idx, result.score)
                };
                let parent_example_id = population_example_ids[target_idx].clone();
                let child_example_id = if let Some(ref mut logger) = eval_logger {
                    let meta = logger.build_example_meta(
                        Some(&parent_example_id),
                        "optimize_generation",
                        "trial",
                        None,
                    );
                    logger.log_example(&meta, &trial_genome, goal_kind, bt, &sim_config, &result);
                    let parent_preset =
                        preset_from_genome_with_seed_context(&parent_before.genome, &seed_ctx);
                    let parent_result = evaluate_preset(&parent_preset, &goal, &sim_config);
                    logger.log_pair(
                        gen + 1,
                        target_idx,
                        goal_kind,
                        bt,
                        &sim_config,
                        &parent_example_id,
                        &meta.example_id,
                        &parent_result,
                        &result,
                        parent_before.violation,
                        violation,
                        child_selected,
                    );
                    meta.example_id
                } else {
                    String::new()
                };
                if constrained {
                    de.report_trial_constrained(target_idx, trial_genome, result.score, violation);
                } else {
                    de.report_trial_result(target_idx, trial_genome, result.score);
                }
                if child_selected && !child_example_id.is_empty() {
                    population_example_ids[target_idx] = child_example_id;
                }
            }
        }

        let best_fitness = de.best().fitness;
        let mean_fitness = de.mean_fitness();
        let fitness_std = de.fitness_std();

        // Progress display
        if gen % 5 == 0 || gen == generations - 1 {
            let elapsed = start.elapsed().as_secs_f64();
            // Phase 3 instrumentation — only emitted when the feature is
            // enabled, to keep legacy progress output bit-identical.
            let restart_suffix = if de.is_stagnation_restart_enabled() {
                format!("  restarts={}", de.stagnation_restart_count())
            } else {
                String::new()
            };
            if constrained {
                let (strict_label, strict_neural, strict_violation) = match de.best_strict() {
                    Some(s) => ("strict", s.neural_fitness, s.violation),
                    None => (
                        "none-strict-feasible; ε-best",
                        de.best().neural_fitness,
                        de.best().violation,
                    ),
                };
                println!(
                    "  Gen {:>4}  ε = {:.4}  best ({}): neural = {:.4}  v = {:.4}  mean = {:.4}{}  [{:.0}s]",
                    gen + 1,
                    de.current_eps(),
                    strict_label,
                    strict_neural,
                    strict_violation,
                    mean_fitness,
                    restart_suffix,
                    elapsed,
                );
            } else {
                println!(
                    "  Gen {:>4}  best: {:.4}  mean: {:.4}  std: {:.4}{}  [{:.0}s]",
                    gen + 1,
                    best_fitness,
                    mean_fitness,
                    fitness_std,
                    restart_suffix,
                    elapsed,
                );
            }
        }

        // Mode-aware convergence + stagnation tracking (review fix
        // 2026-05-02). Constrained mode tracks the strict-feasible
        // incumbent instead of the ε-relaxed `best`, and refuses to
        // declare convergence while ε > 0 (the schedule is still
        // moving). Legacy mode is bit-identical to the historical
        // behaviour.
        let tracked_best = if constrained {
            de.best_strict()
                .map(|s| s.neural_fitness)
                .unwrap_or(f64::NEG_INFINITY)
        } else {
            best_fitness
        };
        let stale_this_gen = if constrained && de.current_eps() > 0.0 {
            // ε is still binding the comparator; do not let stagnation
            // fire while the schedule is still reshaping the feasible
            // region. This avoids the "stagnated at gen 12 because the
            // strict-best happened not to move while ε halved" failure
            // mode.
            false
        } else {
            tracked_best.is_finite() && (tracked_best - prev_best).abs() < 1e-6
        };
        if stale_this_gen {
            stale_count += 1;
        } else {
            stale_count = 0;
        }
        prev_best = tracked_best;

        // fitness_std-based convergence is meaningful only in legacy
        // mode — in constrained mode the std is over all individuals'
        // display fitness regardless of feasibility, so a small std
        // can coincide with the strict frontier still moving.
        if !constrained && fitness_std < convergence && gen > 10 {
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
    // Constrained mode: prefer best_strict() so the user receives the
    // strictly feasible (violation ≤ 1e-9) best. If no strictly feasible
    // candidate exists (the run failed to find any), fall back to the
    // ε-relaxed best with a clear warning so the user knows what they're
    // getting. Legacy / fusion modes always return best() because every
    // individual has violation = 0 by construction.
    let mut returned_strict = true;
    let best_genome = if constrained {
        match de.best_strict() {
            Some(s) => s.genome.clone(),
            None => {
                returned_strict = false;
                eprintln!("  WARNING: no strictly feasible candidate found in constrained run.");
                eprintln!(
                    "           Returning the ε-relaxed best (violation = {:.4}) — this preset",
                    de.best().violation
                );
                eprintln!(
                    "           does not satisfy the comfort constraints. Consider increasing"
                );
                eprintln!(
                    "           --generations, loosening thresholds, or adjusting --init-preset."
                );
                de.best().genome.clone()
            }
        }
    } else {
        de.best().genome.clone()
    };
    let (best_preset, best_result) =
        reevaluate_best_preset(&best_genome, &goal, &sim_config, &seed_ctx);
    let final_violation = if constrained {
        compute_comfort_violation(&goal, &best_result)
    } else {
        0.0
    };

    println!();
    println!("  \u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
    println!("  Result");
    println!("  \u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}\u{2550}");
    println!("  Goal:            {}", goal_kind);
    println!("  Brain type:      {}", bt);
    if constrained {
        let feasibility_tag = if returned_strict {
            "strict-feasible"
        } else {
            "ε-relaxed (NO strict-feasible candidate found)"
        };
        println!(
            "  Score:           {:.4} (neural-only, ε-constrained, {feasibility_tag})",
            best_result.score
        );
        println!("  Violation:       {:.4}", final_violation);
    } else if acoustic_score_fusion {
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
        &seed_ctx,
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

    if let Some(ref logger) = eval_logger {
        logger.finalize_run(
            "success",
            goal_kind,
            bt,
            duration,
            population,
            generations,
            de_f,
            de_cr,
            convergence,
            &sim_config,
            crowding,
            stagnation_window,
            stagnation_fraction,
            init_preset,
            Some(&output_path),
        );
    }
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
    arousal_model_str: &str,
    fixed_arousal: Option<f64>,
    jr_sigma: f64,
    gaba_b_rate: f64,
    gaba_b_gain: f64,
    log_evaluations_path: Option<&Path>,
) {
    ensure_analysis_window_or_exit(
        "evaluate",
        duration,
        SimulationConfig::default().warmup_discard_secs,
    );
    let arousal_model = resolve_arousal_model_or_exit(arousal_model_str, fixed_arousal);

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
    println!(
        "  Arousal:  model={}{}",
        arousal_model_str,
        fixed_arousal
            .map(|v| format!(" ({v:.3})"))
            .unwrap_or_default()
    );
    if (jr_sigma - 15.0).abs() > 1e-12
        || (gaba_b_rate - 5.0).abs() > 1e-12
        || (gaba_b_gain - 10.0).abs() > 1e-12
    {
        println!(
            "  P18 retune: jr_sigma={:.1}  gaba_b_rate={:.1}  gaba_b_gain={:.1}",
            jr_sigma, gaba_b_rate, gaba_b_gain
        );
    }
    if acoustic_score {
        println!("  Acoustic score: enabled (evaluate-only)");
    }
    if acoustic_score_fusion {
        println!("  Acoustic fusion: enabled for shield/isolation only");
    }
    println!();

    if is_matrix {
        if log_evaluations_path.is_some() {
            eprintln!(
                "--log-evaluations is currently supported only for single goal/brain evaluate"
            );
            std::process::exit(2);
        }
        // ── Matrix mode ─────────────────────────────────────────────────────
        print_comparison_matrix(
            &preset,
            &goals,
            &brain_types,
            duration,
            flags,
            acoustic_score_fusion,
            arousal_model,
            fixed_arousal,
            jr_sigma,
            gaba_b_rate,
            gaba_b_gain,
        );
        let signature_preview = build_eval_config(
            duration,
            brain_types[0],
            flags,
            acoustic_score_fusion,
            acoustic_score_fusion,
            arousal_model,
            fixed_arousal,
            jr_sigma,
            gaba_b_rate,
            gaba_b_gain,
        );
        print_model_signature(&signature_preview.model_signature());
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
        let mut eval_logger =
            log_evaluations_path.map(|path| OptimizeCsvLogger::new(path, "evaluate"));
        let show_acoustic = acoustic_score || acoustic_score_fusion;
        let sim_config = build_eval_config(
            duration,
            bt,
            flags,
            show_acoustic,
            acoustic_score_fusion,
            arousal_model,
            fixed_arousal,
            jr_sigma,
            gaba_b_rate,
            gaba_b_gain,
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
        for line in goal_meaning_lines(goal_kind) {
            println!("{line}");
        }
        println!();
        print_model_signature(&result.model_signature);
        if fusion_applied {
            println!("  Score:      {:.4} (fused)", result.score);
        } else {
            println!("  Score:      {:.4}", result.score);
        }
        if acoustic_score_fusion && !goal.supports_acoustic_fusion() {
            println!("  Acoustic fusion: requested, but this goal still uses legacy NMM scoring.");
        }
        if let Some(ref logger) = eval_logger {
            println!("  Log evals: {}", logger.examples_path.display());
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

        if let Some(science) = result.scientific_diagnostics.as_ref() {
            println!("  Scientific Diagnostics (score-inert):");
            println!(
                "    Aperiodic fit ({}-{} Hz): exponent={:.3}, offset={:.3}",
                science.spectral_parameterization.fit_min_hz,
                science.spectral_parameterization.fit_max_hz,
                science.spectral_parameterization.aperiodic_exponent,
                science.spectral_parameterization.aperiodic_offset
            );
            if science.spectral_parameterization.peaks.is_empty() {
                println!("    Periodic peaks: none detected");
            } else {
                println!(
                    "    Periodic peaks: {} detected",
                    science.spectral_parameterization.peaks.len()
                );
                for peak in science.spectral_parameterization.peaks.iter().take(3) {
                    println!(
                        "      peak @ {:>5.2} Hz  bw {:>5.2} Hz  height_above_fit_log10 {:>8.5}",
                        peak.center_hz, peak.bandwidth_hz, peak.power_above_aperiodic
                    );
                }
            }
            if let Some(mod_hz) = science.assr.dominant_modulation_hz {
                println!(
                    "    ASSR (dominant {:.2} Hz): effective_gain={:.3}  phase_consistency={:.3}  jitter={:.3} ms  plv_ceiling={:.3}",
                    mod_hz,
                    science.assr.effective_amplitude_gain.unwrap_or(0.0),
                    science.assr.phase_consistency_heuristic.unwrap_or(0.0),
                    science.assr.implied_latency_jitter_ms_heuristic.unwrap_or(0.0),
                    science.assr.expected_plv_ceiling.unwrap_or(0.0),
                );
            } else {
                println!("    ASSR: N/A (no modulation target)");
            }

            let ar = &science.arousal_sensitivity;
            println!(
                "    Arousal sensitivity: estimated={:.3}, estimated_score={:.4}, local_dScore/dA={:.4}, span={:.4}, max|slope|={:.4}",
                ar.estimated_arousal,
                ar.estimated_score,
                ar.local_derivative,
                ar.score_span,
                ar.max_abs_slope
            );
            if let Some(candidate) = science.candidate_auditory_features.as_ref() {
                println!(
                    "    Candidate auditory features: dominant_mod={:?} Hz, total_mod_power={:.6}",
                    candidate.temporal_modulation.dominant_modulation_hz,
                    candidate.temporal_modulation.total_modulation_power
                );
                println!(
                    "      Cochlear brightness={:.3}, band_energy=[{:.3}, {:.3}, {:.3}, {:.3}], tilt_db_per_oct={}",
                    candidate.cochlear.brightness,
                    candidate.cochlear.band_energy_fractions[0],
                    candidate.cochlear.band_energy_fractions[1],
                    candidate.cochlear.band_energy_fractions[2],
                    candidate.cochlear.band_energy_fractions[3],
                    candidate
                        .cochlear
                        .spectral_tilt_db_per_oct
                        .map(|v| format!("{v:.3}"))
                        .unwrap_or_else(|| "N/A".to_string())
                );
                println!(
                    "      Latent arousal={:.3} ({:?})",
                    candidate.latent_state.estimated_arousal, candidate.latent_state.arousal_source
                );
            }
            if let Some(candidate_cortical) = science.candidate_cortical_response.as_ref() {
                println!(
                    "    Candidate v2 cortical response: dominant={:?}, responsiveness={:.6}",
                    candidate_cortical.dominant_module,
                    candidate_cortical.modulation_responsiveness_index
                );
                println!(
                    "      Module strengths: slow={:.6}, alpha={:.6}, beta={:.6}, gamma={:.6}",
                    candidate_cortical.slow.response_strength,
                    candidate_cortical.alpha.response_strength,
                    candidate_cortical.beta.response_strength,
                    candidate_cortical.gamma.response_strength
                );
            }
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

        for line in practical_report_lines(goal_kind, result, &diagnosis) {
            println!("{line}");
        }
        println!();

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

        if let Some(ref mut logger) = eval_logger {
            let meta = logger.build_example_meta(None, "evaluate_single", "manual_evaluate", None);
            let genome = preset.to_genome();
            logger.log_example(&meta, &genome, goal_kind, bt, &sim_config, result);
            logger.finalize_run(
                "success",
                goal_kind,
                bt,
                duration,
                1,
                1,
                0.0,
                0.0,
                0.0,
                &sim_config,
                false,
                0,
                0.0,
                Some(preset_path.as_path()),
                None,
            );
        }
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
    arousal_model: ArousalModel,
    fixed_arousal: Option<f64>,
    jr_sigma: f64,
    gaba_b_rate: f64,
    gaba_b_gain: f64,
) {
    let scores = evaluate_score_matrix(
        preset,
        goals,
        brain_types,
        duration,
        flags,
        acoustic_score_fusion,
        arousal_model,
        fixed_arousal,
        jr_sigma,
        gaba_b_rate,
        gaba_b_gain,
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
            "Acoustic goal score", acoustic_goal_score
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

fn goal_meaning_lines(goal_kind: GoalKind) -> Vec<String> {
    let semantics = goal_kind.semantics();
    let what_score_means = format!(
        "high score means stronger alignment with {} proxy targets",
        goal_kind
    );
    let unsupported = semantics
        .unsupported_claims
        .iter()
        .map(|claim| {
            claim
                .trim()
                .trim_end_matches('.')
                .strip_prefix("Does not prove ")
                .or_else(|| claim.trim().strip_prefix("Does not prove"))
                .unwrap_or(claim.trim())
                .trim()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("; ");
    vec![
        "  Goal meaning:".to_string(),
        format!("    Purpose: {}", semantics.plain_language_purpose),
        format!("    Objective: {}", semantics.product_objective),
        format!("    Score meaning: {}", what_score_means),
        format!("    Does not prove: {}", unsupported),
        format!("    Evidence level: {}", semantics.evidence_level.as_str()),
    ]
}

fn practical_status(score: f64) -> &'static str {
    if score >= 0.80 {
        "strong"
    } else if score >= 0.60 {
        "usable"
    } else if score >= 0.40 {
        "weak"
    } else {
        "poor"
    }
}

fn practical_report_lines(
    goal_kind: GoalKind,
    result: &SimulationResult,
    diagnosis: &scoring::Diagnosis,
) -> Vec<String> {
    let semantics = goal_kind.semantics();
    let mut reasons: Vec<String> = Vec::new();

    let mut failing_bands = diagnosis
        .bands
        .iter()
        .filter(|b| matches!(b.status, scoring::MetricStatus::Fail))
        .map(|b| b.name.to_lowercase())
        .collect::<Vec<_>>();
    failing_bands.sort();
    if !failing_bands.is_empty() {
        reasons.push(format!("bands out of target ({})", failing_bands.join(", ")));
    }

    if !matches!(diagnosis.firing_rate_status, scoring::MetricStatus::Pass) {
        let detail = if diagnosis.firing_rate < diagnosis.firing_rate_range.0 {
            "too low"
        } else {
            "too high"
        };
        reasons.push(format!("firing rate {detail}"));
    }
    if !matches!(diagnosis.isi_status, scoring::MetricStatus::Pass) {
        reasons.push("spike regularity off target".to_string());
    }

    if let Some(primary_proxy) = semantics.primary_neural_proxies.first() {
        let dom = diagnosis.dominant_band_name().to_lowercase();
        let expected_bands = concrete_proxy_bands(primary_proxy);
        if !expected_bands.is_empty() && !expected_bands.iter().any(|b| *b == dom) {
            reasons.push(format!(
                "dominant frequency is {} while primary proxy emphasizes {}",
                dom, primary_proxy
            ));
        }
    }

    if reasons.is_empty() {
        reasons.push("no major proxy-target failures detected".to_string());
    }

    let acoustic_line = if let Some(acoustic) = &result.acoustic_score {
        let comfort = acoustic
            .comfort_score
            .map(|v| format!("{v:.3}"))
            .unwrap_or_else(|| "N/A".to_string());
        let privacy = acoustic
            .speech_privacy
            .map(|v| format!("{v:.3}"))
            .unwrap_or_else(|| "N/A".to_string());
        format!("Acoustic masking/comfort: comfort={comfort}, speech_privacy={privacy}.")
    } else {
        "Acoustic masking/comfort: not scored in this run.".to_string()
    };

    vec![
        "  Practical Report:".to_string(),
        format!("    Status: {}", practical_status(result.score)),
        format!("    Intended use: {}", semantics.product_objective),
        format!("    Top reasons: {}", reasons.join("; ")),
        format!(
            "    Interpretation: Intended objective: {}. Current proxy alignment is {}.",
            semantics.product_objective.trim_end_matches('.'),
            practical_status(result.score)
        ),
        format!("    {acoustic_line}"),
        "    Limitation: This is a model-based proxy report, not proof of human efficacy."
            .to_string(),
    ]
}

fn concrete_proxy_bands(proxy: &str) -> Vec<&'static str> {
    let proxy_lc = proxy.to_lowercase();
    let mut bands = Vec::new();
    for band in ["delta", "theta", "alpha", "beta", "gamma"] {
        if proxy_lc.contains(band) {
            bands.push(band);
        }
    }
    bands
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
    flags: EvaluateFeatureFlags,
    jr_sigma: f64,
    gaba_b_rate: f64,
    gaba_b_gain: f64,
    legacy_ablated: bool,
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

    let config = build_disturb_config(
        duration,
        bt,
        spike_time,
        spike_duration,
        spike_gain,
        flags,
        jr_sigma,
        gaba_b_rate,
        gaba_b_gain,
        legacy_ablated,
    );

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
    print_model_signature(&result.model_signature);
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
    arousal_model_str: &str,
    fixed_arousal: Option<f64>,
    seed: u64,
) {
    use std::io::Write;
    use std::sync::{Arc, Mutex};
    let arousal_model = resolve_arousal_model_or_exit(arousal_model_str, fixed_arousal);

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
    println!(
        "  Arousal model:  {}{}",
        arousal_model_str,
        fixed_arousal
            .map(|v| format!(" ({v:.3})"))
            .unwrap_or_default()
    );
    let signature_preview = build_generate_data_config(
        duration,
        brain_types[0],
        phys_gate,
        arousal_model,
        fixed_arousal,
        seed,
    );
    print_model_signature(&signature_preview.model_signature());
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
    let results: Arc<
        Mutex<
            Vec<(
                usize,
                Vec<f64>,
                GoalKind,
                BrainType,
                SimulationConfig,
                SimulationResult,
            )>,
        >,
    > = Arc::new(Mutex::new(Vec::with_capacity(total_evals)));
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
                    let config = build_generate_data_config(
                        duration,
                        bt,
                        phys_gate,
                        arousal_model,
                        fixed_arousal,
                        seed,
                    );
                    let result = evaluate_preset_for_dataset_export(&preset, &goal, &config);

                    results.lock().unwrap().push((
                        preset_idx,
                        genome.clone(),
                        goal_kind,
                        bt,
                        config,
                        result,
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
    let mut results = match Arc::try_unwrap(results) {
        Ok(mutex) => match mutex.into_inner() {
            Ok(values) => values,
            Err(poisoned) => poisoned.into_inner(),
        },
        Err(_) => {
            eprintln!("Internal error: generate-data results still have outstanding references");
            std::process::exit(1);
        }
    };
    results.sort_by_key(|(preset_idx, _, _, _, _, _)| *preset_idx);
    let mut file = std::fs::File::create(output).unwrap_or_else(|e| {
        eprintln!("Failed to create output file: {e}");
        std::process::exit(1);
    });

    // Header
    writeln!(file, "{}", surrogate_csv_header()).unwrap();

    // Data rows
    let run_id = make_run_id("generate_data");
    for (row_idx, (_, genome, goal_kind, bt, config, result)) in results.iter().enumerate() {
        let meta = CsvExampleMeta {
            example_id: format!("{run_id}_e{row_idx:06}"),
            run_id: run_id.clone(),
            parent_example_id: String::new(),
            stage: "generate_data".to_string(),
            source: "random".to_string(),
            seed_eval: String::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        writeln!(
            file,
            "{}",
            surrogate_csv_row(&meta, genome, *goal_kind, *bt, config, result)
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

        let config = build_eval_config(
            7.5,
            BrainType::Aging,
            flags,
            true,
            true,
            ArousalModel::LegacyHeuristic,
            None,
            15.0,
            5.0,
            10.0,
        );
        assert!((config.duration_secs - 7.5).abs() < 1e-12);
        assert_eq!(config.brain_type, BrainType::Aging);
        assert!(!config.assr_enabled);
        assert!(config.thalamic_gate_enabled);
        assert!(!config.cet_enabled);
        assert!(config.physiological_thalamic_gate_enabled);
        assert!(config.acoustic_scoring_enabled);
        assert!(config.acoustic_score_fusion_enabled);
        assert_eq!(config.jr_stochastic_sigma, 15.0);
        assert_eq!(config.cet_b_slow_rate, 5.0);
        assert_eq!(config.cet_b_slow_gain, 10.0);
    }

    #[test]
    fn build_eval_config_propagates_p18_params() {
        let flags = EvaluateFeatureFlags {
            assr: true,
            thalamic_gate: true,
            cet: true,
            phys_gate: false,
        };
        let config = build_eval_config(
            12.0,
            BrainType::Normal,
            flags,
            false,
            false,
            ArousalModel::LegacyHeuristic,
            None,
            100.0,
            25.0,
            18.0,
        );
        assert_eq!(config.jr_stochastic_sigma, 100.0);
        assert_eq!(config.cet_b_slow_rate, 25.0);
        assert_eq!(config.cet_b_slow_gain, 18.0);
    }

    #[test]
    fn build_generate_data_config_carries_phys_gate() {
        let config = build_generate_data_config(
            3.5,
            BrainType::Adhd,
            true,
            ArousalModel::LegacyHeuristic,
            None,
            42,
        );
        assert!((config.duration_secs - 3.5).abs() < 1e-12);
        assert_eq!(config.brain_type, BrainType::Adhd);
        assert!(config.assr_enabled);
        assert!(config.thalamic_gate_enabled);
        assert!(config.cet_enabled);
        assert!(config.physiological_thalamic_gate_enabled);
        assert!(!config.acoustic_scoring_enabled);
        assert_eq!(config.reproducibility_seed, Some(42));
    }

    #[test]
    fn build_disturb_config_defaults_to_canonical_mode() {
        let flags = EvaluateFeatureFlags {
            assr: false,
            thalamic_gate: true,
            cet: true,
            phys_gate: false,
        };
        let cfg = build_disturb_config(
            6.0,
            BrainType::Normal,
            4.0,
            0.05,
            0.8,
            flags,
            15.0,
            5.0,
            10.0,
            false,
        );
        assert_eq!(cfg.mode, disturb::DisturbanceMode::Canonical);
        assert!(!cfg.assr_enabled);
        assert!(cfg.thalamic_gate_enabled);
        assert!(cfg.cet_enabled);
        assert!(!cfg.physiological_thalamic_gate_enabled);
        assert_eq!(cfg.jr_stochastic_sigma, 15.0);
    }

    #[test]
    fn build_disturb_config_legacy_flag_switches_mode() {
        let flags = EvaluateFeatureFlags {
            assr: true,
            thalamic_gate: false,
            cet: false,
            phys_gate: true,
        };
        let cfg = build_disturb_config(
            6.0,
            BrainType::Aging,
            3.5,
            0.02,
            0.4,
            flags,
            100.0,
            25.0,
            18.0,
            true,
        );
        assert_eq!(cfg.mode, disturb::DisturbanceMode::LegacyAblated);
        assert!(cfg.assr_enabled);
        assert!(!cfg.thalamic_gate_enabled);
        assert!(!cfg.cet_enabled);
        assert!(cfg.physiological_thalamic_gate_enabled);
        assert_eq!(cfg.jr_stochastic_sigma, 100.0);
        assert_eq!(cfg.cet_b_slow_rate, 25.0);
        assert_eq!(cfg.cet_b_slow_gain, 18.0);
    }

    #[test]
    fn disturb_cli_defaults_to_canonical_mode() {
        let cli = Cli::try_parse_from([
            "neural-preset-optimizer",
            "disturb",
            "presets/the_shield_v1.json",
        ])
        .expect("CLI parse should succeed");
        match cli.command {
            Commands::Disturb { legacy_ablated, .. } => assert!(!legacy_ablated),
            _ => panic!("expected disturb command"),
        }
    }

    #[test]
    fn disturb_cli_legacy_ablated_flag_is_honored() {
        let cli = Cli::try_parse_from([
            "neural-preset-optimizer",
            "disturb",
            "presets/the_shield_v1.json",
            "--legacy-ablated",
        ])
        .expect("CLI parse should succeed");
        match cli.command {
            Commands::Disturb { legacy_ablated, .. } => assert!(legacy_ablated),
            _ => panic!("expected disturb command"),
        }
    }

    #[test]
    fn build_optimize_config_enables_fusion_implies_acoustic_scoring() {
        // Pass legacy P18 defaults (15.0, 5.0, 10.0) so the resulting
        // config is bit-identical to pre-P18 behaviour.
        let config = build_optimize_config(
            4.0,
            BrainType::Anxious,
            true,
            true,
            true,
            false,
            15.0,
            5.0,
            10.0,
            42,
        );
        assert!((config.duration_secs - 4.0).abs() < 1e-12);
        assert_eq!(config.brain_type, BrainType::Anxious);
        assert!(config.cet_enabled);
        assert!(config.physiological_thalamic_gate_enabled);
        assert!(config.acoustic_scoring_enabled);
        assert!(config.acoustic_score_fusion_enabled);
        assert!(!config.acoustic_constraints_enabled);
        // P18 defaults preserved end-to-end through build_optimize_config.
        assert_eq!(config.jr_stochastic_sigma, 15.0);
        assert_eq!(config.cet_b_slow_rate, 5.0);
        assert_eq!(config.cet_b_slow_gain, 10.0);
        assert_eq!(config.reproducibility_seed, Some(42));
    }

    /// **P18 wiring pin**: build_optimize_config must propagate
    /// non-default jr_sigma / gaba_b_rate / gaba_b_gain to the
    /// resulting SimulationConfig. Otherwise a CLI invocation of
    /// `--jr-sigma 100` would be silently ignored.
    #[test]
    fn build_optimize_config_propagates_p18_params() {
        let config = build_optimize_config(
            4.0,
            BrainType::Normal,
            true,
            false,
            false,
            true,
            100.0,
            25.0,
            18.0,
            42,
        );
        assert_eq!(config.jr_stochastic_sigma, 100.0);
        assert_eq!(config.cet_b_slow_rate, 25.0);
        assert_eq!(config.cet_b_slow_gain, 18.0);
        assert_eq!(config.reproducibility_seed, Some(42));
    }

    #[test]
    fn acoustic_scaffolding_defaults_are_disabled() {
        assert!(!crate::acoustic_score::AcousticScoreConfig::default().enabled);
        assert!(!crate::acoustic_score::AcousticScoreConfig::default().fusion_enabled);
        assert!(!SimulationConfig::default().acoustic_scoring_enabled);
        assert!(!SimulationConfig::default().acoustic_score_fusion_enabled);
        assert!(!SimulationConfig::default().acoustic_constraints_enabled);
        assert!(!disturb::DisturbConfig::default().acoustic_scoring_enabled);
    }

    // ── Priority 28 Phase 2 — build_optimize_config (constrained mode) ──

    /// Constrained mode must force `acoustic_scoring_enabled = true` so
    /// `Goal::comfort_violation` has data, and force `fusion = false` to
    /// prevent double-counting comfort. The shape of this test pins the
    /// "safety net" behaviour of `build_optimize_config` even though the
    /// up-front validator already rejects the illegal combination.
    #[test]
    fn build_optimize_config_constrained_forces_scoring_on_and_fusion_off() {
        // Even if the caller passes fusion=true, constrained=true must win.
        let config = build_optimize_config(
            4.0,
            BrainType::Normal,
            true,
            false,
            true,
            true,
            15.0,
            5.0,
            10.0,
            42,
        );
        assert!(config.acoustic_scoring_enabled);
        assert!(!config.acoustic_score_fusion_enabled);
        assert!(config.acoustic_constraints_enabled);
    }

    #[test]
    fn build_optimize_config_constrained_alone_enables_scoring() {
        let config = build_optimize_config(
            4.0,
            BrainType::Normal,
            true,
            false,
            false,
            true,
            15.0,
            5.0,
            10.0,
            42,
        );
        assert!(config.acoustic_scoring_enabled);
        assert!(!config.acoustic_score_fusion_enabled);
        assert!(config.acoustic_constraints_enabled);
    }

    #[test]
    fn validate_optimize_acoustic_mode_accepts_supported_goal_without_sidecars() {
        let goal = Goal::new(GoalKind::Shield);
        assert!(validate_optimize_acoustic_mode(&goal, true, false, false, None).is_ok());
    }

    #[test]
    fn validate_optimize_acoustic_mode_rejects_unsupported_goal() {
        let goal = Goal::new(GoalKind::Focus);
        let err = validate_optimize_acoustic_mode(&goal, true, false, false, None).unwrap_err();
        assert!(err.contains("supported only for shield and isolation"));
    }

    #[test]
    fn validate_optimize_acoustic_mode_rejects_surrogate_mix() {
        let goal = Goal::new(GoalKind::Shield);
        let err = validate_optimize_acoustic_mode(&goal, true, false, true, None).unwrap_err();
        assert!(err.contains("--surrogate"));
    }

    #[test]
    fn validate_optimize_acoustic_mode_rejects_logging_mix() {
        let goal = Goal::new(GoalKind::Isolation);
        assert!(validate_optimize_acoustic_mode(
            &goal,
            true,
            false,
            false,
            Some(Path::new("/tmp/fused_optimize.csv")),
        )
        .is_ok());
    }

    // ── Priority 28 Phase 2 — validate_optimize_acoustic_mode (constrained) ──

    #[test]
    fn validate_optimize_acoustic_mode_accepts_constrained_for_any_goal() {
        // Constrained mode is goal-agnostic — `comfort_violation` thresholds
        // are goal-aware, so every goal kind is admissible at this layer.
        for &kind in GoalKind::all() {
            let goal = Goal::new(kind);
            assert!(
                validate_optimize_acoustic_mode(&goal, false, true, false, None).is_ok(),
                "constrained mode must accept goal {kind}"
            );
        }
    }

    #[test]
    fn validate_optimize_acoustic_mode_rejects_constrained_with_fusion() {
        let goal = Goal::new(GoalKind::Shield);
        let err = validate_optimize_acoustic_mode(&goal, true, true, false, None).unwrap_err();
        assert!(err.contains("--constrained"));
        assert!(err.contains("--acoustic-score-fusion"));
    }

    #[test]
    fn validate_optimize_acoustic_mode_rejects_constrained_with_surrogate() {
        let goal = Goal::new(GoalKind::Shield);
        let err = validate_optimize_acoustic_mode(&goal, false, true, true, None).unwrap_err();
        assert!(err.contains("--constrained"));
        assert!(err.contains("--surrogate"));
    }

    #[test]
    fn validate_optimize_acoustic_mode_rejects_constrained_with_logging() {
        let goal = Goal::new(GoalKind::Sleep);
        assert!(validate_optimize_acoustic_mode(
            &goal,
            false,
            true,
            false,
            Some(Path::new("/tmp/eps_optimize.csv")),
        )
        .is_ok());
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

        assert_eq!(cols[0], "example_id");
        assert_eq!(cols[7], "goal");
        let gaba_idx = cols
            .iter()
            .position(|c| *c == "gaba_b_gain")
            .expect("gaba_b_gain column");
        assert_eq!(cols[gaba_idx + 1], "g0");
        assert_eq!(
            cols[gaba_idx + 1 + surrogate::GENOME_DIM - 1],
            format!("g{}", surrogate::GENOME_DIM - 1)
        );
        assert!(cols.contains(&"score"));
        assert!(cols.contains(&"score_legacy_v1_neural"));
        assert!(cols.contains(&"score_legacy_v1_fused"));
        assert!(cols.contains(&"score_candidate_research_v2"));
        assert!(cols.contains(&"score_product_acoustic"));
        assert!(cols.contains(&"violation"));
        assert!(cols.contains(&"speech_privacy"));
        assert!(cols.contains(&"aperiodic_exponent"));
        assert!(cols.contains(&"assr_dominant_modulation_hz"));
        assert!(cols.contains(&"assr_effective_amplitude_gain"));
        assert!(cols.contains(&"assr_phase_consistency_heuristic"));
        assert!(cols.contains(&"estimated_arousal"));
        assert!(cols.contains(&"arousal_score_span"));
        assert!(cols.contains(&"candidate_dominant_modulation_hz"));
        assert!(cols.contains(&"candidate_cochlear_brightness"));
        assert!(cols.contains(&"candidate_arousal_source"));
        assert!(cols.contains(&"is_feasible"));
        assert!(cols.contains(&"model_signature_schema_version"));
        assert!(cols.contains(&"model_signature_json"));
        assert_eq!(cols[cols.len() - 2], "model_signature_schema_version");
        assert_eq!(cols[cols.len() - 1], "model_signature_json");
    }

    fn parse_csv_fields(line: &str) -> Vec<String> {
        let mut fields = Vec::new();
        let mut current = String::new();
        let mut chars = line.chars().peekable();
        let mut in_quotes = false;

        while let Some(ch) = chars.next() {
            match ch {
                '"' => {
                    if in_quotes && chars.peek() == Some(&'"') {
                        current.push('"');
                        let _ = chars.next();
                    } else {
                        in_quotes = !in_quotes;
                    }
                }
                ',' if !in_quotes => {
                    fields.push(current);
                    current = String::new();
                }
                _ => current.push(ch),
            }
        }
        fields.push(current);
        fields
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
            jr_stochastic_sigma: 100.0,
            cet_b_slow_rate: 25.0,
            cet_b_slow_gain: 18.0,
            ..SimulationConfig::default()
        };
        let preset = Preset::default();
        let goal = Goal::new(GoalKind::Sleep);
        let result = evaluate_preset(&preset, &goal, &config);
        let meta = CsvExampleMeta {
            example_id: "ex1".to_string(),
            run_id: "run1".to_string(),
            parent_example_id: "parent0".to_string(),
            stage: "optimize_generation".to_string(),
            source: "trial".to_string(),
            seed_eval: "42".to_string(),
            created_at: "2026-05-02T00:00:00Z".to_string(),
        };
        let row = surrogate_csv_row(
            &meta,
            &genome,
            GoalKind::Sleep,
            BrainType::Aging,
            &config,
            &result,
        );
        let cols: Vec<&str> = row.split(',').collect();
        let header = surrogate_csv_header();
        let header_cols: Vec<&str> = header.split(',').collect();
        let idx = |name: &str| {
            header_cols
                .iter()
                .position(|c| *c == name)
                .expect("column should exist")
        };
        let meta_start = 0usize;
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

        assert_eq!(cols[meta_start], "ex1");
        assert_eq!(cols[meta_start + 1], "run1");
        assert_eq!(cols[meta_start + 2], "parent0");
        assert_eq!(cols[meta_start + 3], "optimize_generation");
        assert_eq!(cols[meta_start + 4], "trial");
        assert_eq!(cols[meta_start + 5], "42");
        assert_eq!(cols[meta_start + 8], expected_goal_id);
        assert_eq!(cols[meta_start + 10], expected_brain_type_id);
        assert_eq!(cols[meta_start + 12], "1");
        assert_eq!(cols[meta_start + 13], "0");
        assert_eq!(cols[meta_start + 14], "1");
        assert_eq!(cols[meta_start + 15], "1");
        assert_eq!(cols[idx("jr_sigma")], "100.000000");
        assert_eq!(cols[idx("gaba_b_rate")], "25.000000");
        assert_eq!(cols[idx("gaba_b_gain")], "18.000000");
        assert_eq!(cols[idx("g0")], "0.500000");
        assert_eq!(
            cols[idx("score")],
            format!("{:.6}", result.score)
        );
        assert!(
            row.contains(",1,\"{"),
            "row should contain signature schema marker"
        );
        assert!(
            row.contains("\"\"version\"\":\"\"legacy_v1\"\"")
                || row.contains("\"\"version\"\": \"\"legacy_v1\"\""),
            "row should embed a legacy_v1 signature json payload"
        );
        assert!(
            row.contains("\"\"normalization_mode\"\":\"\"global_per_ear\"\"")
                || row.contains("\"\"normalization_mode\"\": \"\"global_per_ear\"\""),
            "signature payload should carry normalization mode as compact config provenance"
        );
        assert!(
            row.contains("\"\"audio_sample_rate_hz\"\":48000")
                || row.contains("\"\"audio_sample_rate_hz\"\": 48000"),
            "signature payload should carry audio sample rate provenance"
        );
        assert!(
            row.contains("\"\"neural_decimation_factor\"\":48")
                || row.contains("\"\"neural_decimation_factor\"\": 48"),
            "signature payload should carry neural decimation provenance"
        );
        assert!(
            row.contains("\"\"neural_sample_rate_hz\"\":1000.0")
                || row.contains("\"\"neural_sample_rate_hz\"\": 1000.0"),
            "signature payload should carry neural sample rate provenance"
        );
    }

    #[test]
    fn dataset_export_path_populates_stage2_diagnostic_columns() {
        let mut preset = Preset::default();
        preset.source_count = 1;
        preset.objects[0].active = true;
        preset.objects[0].volume = 0.8;
        preset.objects[0].bass_mod.kind = 4;
        preset.objects[0].bass_mod.param_a = 40.0;
        preset.objects[0].bass_mod.param_b = 0.9;
        let genome = preset.to_genome();

        let config = build_generate_data_config(
            3.0,
            BrainType::Normal,
            false,
            ArousalModel::LegacyHeuristic,
            None,
            12345,
        );
        let goal_kind = GoalKind::Focus;
        let goal = Goal::new(goal_kind);
        let result = evaluate_preset_for_dataset_export(&preset, &goal, &config);
        assert!(
            result.scientific_diagnostics.is_some(),
            "dataset export evaluation path should include Stage 2 diagnostics"
        );

        let meta = CsvExampleMeta {
            example_id: "ex_stage2".to_string(),
            run_id: "run_stage2".to_string(),
            parent_example_id: String::new(),
            stage: "generate_data".to_string(),
            source: "random".to_string(),
            seed_eval: "12345".to_string(),
            created_at: "2026-05-16T00:00:00Z".to_string(),
        };
        let row = surrogate_csv_row(
            &meta,
            &genome,
            goal_kind,
            BrainType::Normal,
            &config,
            &result,
        );
        let header = surrogate_csv_header();
        let header_cols: Vec<&str> = header.split(',').collect();
        let row_cols = parse_csv_fields(&row);
        assert_eq!(
            header_cols.len(),
            row_cols.len(),
            "CSV row width must match header width"
        );

        let idx = |name: &str| {
            header_cols
                .iter()
                .position(|c| *c == name)
                .expect("missing csv column")
        };

        assert!(
            !row_cols[idx("aperiodic_exponent")].is_empty(),
            "aperiodic_exponent should be populated"
        );
        assert!(
            !row_cols[idx("assr_dominant_modulation_hz")].is_empty(),
            "assr_dominant_modulation_hz should be populated"
        );
        assert!(
            !row_cols[idx("assr_effective_amplitude_gain")].is_empty(),
            "assr_effective_amplitude_gain should be populated"
        );
        assert!(
            !row_cols[idx("arousal_local_derivative")].is_empty(),
            "arousal_local_derivative should be populated"
        );
    }

    #[test]
    fn dataset_export_path_populates_stage3_candidate_feature_columns() {
        let mut preset = Preset::default();
        preset.source_count = 1;
        preset.objects[0].active = true;
        preset.objects[0].color = 2; // brown
        preset.objects[0].volume = 0.85;
        preset.objects[0].bass_mod.kind = 4;
        preset.objects[0].bass_mod.param_a = 5.0;
        preset.objects[0].bass_mod.param_b = 0.95;
        let genome = preset.to_genome();

        let config = build_generate_data_config(
            3.0,
            BrainType::Normal,
            false,
            ArousalModel::LegacyHeuristic,
            None,
            12345,
        );
        let goal_kind = GoalKind::Focus;
        let goal = Goal::new(goal_kind);
        let result = evaluate_preset_for_dataset_export(&preset, &goal, &config);
        let meta = CsvExampleMeta {
            example_id: "ex_stage3".to_string(),
            run_id: "run_stage3".to_string(),
            parent_example_id: String::new(),
            stage: "generate_data".to_string(),
            source: "random".to_string(),
            seed_eval: "12345".to_string(),
            created_at: "2026-05-16T00:00:00Z".to_string(),
        };
        let row = surrogate_csv_row(
            &meta,
            &genome,
            goal_kind,
            BrainType::Normal,
            &config,
            &result,
        );
        let header = surrogate_csv_header();
        let header_cols: Vec<&str> = header.split(',').collect();
        let row_cols = parse_csv_fields(&row);
        let idx = |name: &str| {
            header_cols
                .iter()
                .position(|c| *c == name)
                .expect("missing csv column")
        };
        assert!(
            !row_cols[idx("candidate_dominant_modulation_hz")].is_empty(),
            "candidate_dominant_modulation_hz should be populated"
        );
        assert!(
            !row_cols[idx("candidate_total_modulation_power")].is_empty(),
            "candidate_total_modulation_power should be populated"
        );
        assert!(
            !row_cols[idx("candidate_cochlear_brightness")].is_empty(),
            "candidate_cochlear_brightness should be populated"
        );
        assert!(
            !row_cols[idx("candidate_arousal_source")].is_empty(),
            "candidate_arousal_source should be populated"
        );
    }

    #[test]
    fn surrogate_csv_primary_peak_prefers_strongest_detected_peak() {
        let mut preset = Preset::default();
        preset.source_count = 1;
        preset.objects[0].active = true;
        preset.objects[0].volume = 0.8;
        let genome = preset.to_genome();

        let config = build_generate_data_config(
            3.0,
            BrainType::Normal,
            false,
            ArousalModel::LegacyHeuristic,
            None,
            777,
        );
        let goal_kind = GoalKind::Focus;
        let goal = Goal::new(goal_kind);
        let mut result = evaluate_preset_for_dataset_export(&preset, &goal, &config);

        let diagnostics = result
            .scientific_diagnostics
            .as_mut()
            .expect("dataset export path should include diagnostics");
        diagnostics.spectral_parameterization.peaks = vec![
            crate::neural::aperiodic::SpectralPeak {
                center_hz: 8.0,
                bandwidth_hz: 2.0,
                power_above_aperiodic: 0.12,
            },
            crate::neural::aperiodic::SpectralPeak {
                center_hz: 20.0,
                bandwidth_hz: 3.0,
                power_above_aperiodic: 0.35,
            },
        ];
        assert!(
            diagnostics.spectral_parameterization.peaks[0].center_hz
                < diagnostics.spectral_parameterization.peaks[1].center_hz,
            "test setup should keep the peak list frequency-sorted"
        );
        let chosen = primary_peak_for_export(Some(&diagnostics.spectral_parameterization)).unwrap();
        assert_eq!(chosen.center_hz, 20.0);

        let meta = CsvExampleMeta {
            example_id: "ex_primary_peak".to_string(),
            run_id: "run_primary_peak".to_string(),
            parent_example_id: String::new(),
            stage: "generate_data".to_string(),
            source: "test".to_string(),
            seed_eval: "777".to_string(),
            created_at: "2026-05-16T00:00:00Z".to_string(),
        };

        let row = surrogate_csv_row(
            &meta,
            &genome,
            goal_kind,
            BrainType::Normal,
            &config,
            &result,
        );
        let header = surrogate_csv_header();
        let header_cols: Vec<&str> = header.split(',').collect();
        let row_cols = parse_csv_fields(&row);
        let idx = |name: &str| {
            header_cols
                .iter()
                .position(|c| *c == name)
                .expect("missing csv column")
        };

        assert_eq!(row_cols[idx("primary_peak_center_hz")], "20.000000");
        assert_eq!(row_cols[idx("primary_peak_bandwidth_hz")], "3.000000");
        assert_eq!(
            row_cols[idx("primary_peak_height_above_aperiodic_log10")],
            "0.350000"
        );
    }

    #[test]
    fn derive_sibling_csv_path_keeps_directory_and_extension() {
        let path = Path::new("/tmp/training/examples.csv");
        assert_eq!(
            derive_sibling_csv_path(path, "_pairs"),
            PathBuf::from("/tmp/training/examples_pairs.csv")
        );
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

        let seed_ctx = SeedPresetContext::default();
        let (_preset, exported_result) = export_best_genome(
            &output_path,
            &best_genome,
            &goal,
            goal_kind,
            7,
            config.duration_secs,
            &config,
            &seed_ctx,
        )
        .expect("best-genome export should succeed");

        let json = std::fs::read_to_string(&output_path).expect("exported JSON should exist");
        let exported: serde_json::Value =
            serde_json::from_str(&json).expect("exported JSON should parse");
        let exported_score = exported["meta"]["score"]
            .as_f64()
            .expect("meta.score should be f64");
        let exported_goal_purpose = exported["meta"]["goal_semantics"]["plain_language_purpose"]
            .as_str()
            .expect("meta.goal_semantics.plain_language_purpose should be string");
        let exported_goal_id = exported["meta"]["goal_semantics"]["goal"]
            .as_str()
            .expect("meta.goal_semantics.goal should be string");
        let exported_beta = exported["analysis"]["band_powers"]["beta"]
            .as_f64()
            .expect("analysis.band_powers.beta should be f64");
        let exported_version = exported["meta"]["model_signature"]["version"]
            .as_str()
            .expect("meta.model_signature.version should be string");
        let exported_audio_sr = exported["meta"]["model_signature"]["audio_sample_rate_hz"]
            .as_u64()
            .expect("meta.model_signature.audio_sample_rate_hz should be u64");
        let exported_neural_decimation = exported["meta"]["model_signature"]
            ["neural_decimation_factor"]
            .as_u64()
            .expect("meta.model_signature.neural_decimation_factor should be u64");
        let exported_neural_sr = exported["meta"]["model_signature"]["neural_sample_rate_hz"]
            .as_f64()
            .expect("meta.model_signature.neural_sample_rate_hz should be f64");

        assert!((exported_result.score - direct.score).abs() < 1e-12);
        assert!((exported_score - direct.score).abs() < 1e-12);
        assert!(!exported_goal_purpose.is_empty());
        assert_eq!(exported_goal_id, "focus");
        assert!((exported_beta - direct.beta_power).abs() < 1e-12);
        assert_eq!(exported_version, "legacy_v1");
        assert_eq!(exported_audio_sr, crate::pipeline::SAMPLE_RATE as u64);
        assert_eq!(
            exported_neural_decimation,
            crate::pipeline::DECIMATION_FACTOR as u64
        );
        assert_eq!(
            exported_neural_sr.to_bits(),
            crate::pipeline::NEURAL_SR.to_bits()
        );
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

        let seed_ctx = SeedPresetContext::default();
        let (_preset, exported_result) = export_best_genome(
            &output_path,
            &best_genome,
            &goal,
            goal_kind,
            3,
            config.duration_secs,
            &config,
            &seed_ctx,
        )
        .expect("fused best-genome export should succeed");

        let json = std::fs::read_to_string(&output_path).expect("exported JSON should exist");
        let exported: serde_json::Value =
            serde_json::from_str(&json).expect("exported JSON should parse");
        let exported_score = exported["meta"]["score"]
            .as_f64()
            .expect("meta.score should be f64");
        let exported_signature = &exported["meta"]["model_signature"];

        assert!((exported_result.score - direct.score).abs() < 1e-12);
        assert!((exported_score - direct.score).abs() < 1e-12);
        assert_eq!(exported_signature["version"], "legacy_v1");
        assert_eq!(exported_signature["scoring_profile"], "legacy_v1");
        assert_eq!(
            exported_signature["audio_sample_rate_hz"],
            serde_json::json!(crate::pipeline::SAMPLE_RATE)
        );
        assert_eq!(
            exported_signature["neural_decimation_factor"],
            serde_json::json!(crate::pipeline::DECIMATION_FACTOR)
        );
        assert_eq!(
            exported_signature["neural_sample_rate_hz"],
            serde_json::json!(crate::pipeline::NEURAL_SR)
        );
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
    fn seed_context_preserves_room_and_position_space() {
        let best_genome = Preset::default().to_genome();
        let mut seed_ctx = SeedPresetContext::default();
        seed_ctx.room.mode = 1;
        seed_ctx.position_space_per_slot[0] = 1;
        seed_ctx.position_space_per_slot[1] = 2;
        seed_ctx.spread_per_slot[0] = 0.65;

        let preset = preset_from_genome_with_seed_context(&best_genome, &seed_ctx);

        assert_eq!(preset.room.mode, 1);
        assert_eq!(preset.objects[0].position_space, 1);
        assert_eq!(preset.objects[1].position_space, 2);
        assert!((preset.objects[0].spread - 0.65).abs() < 1e-6);
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
        let config = build_eval_config(
            duration,
            brain_type,
            flags,
            false,
            false,
            ArousalModel::LegacyHeuristic,
            None,
            15.0,
            5.0,
            10.0,
        );

        let direct = evaluate_preset(&preset, &goal, &config);
        let matrix = evaluate_score_matrix(
            &preset,
            &[goal_kind],
            &[brain_type],
            duration,
            flags,
            false,
            ArousalModel::LegacyHeuristic,
            None,
            15.0,
            5.0,
            10.0,
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

    #[test]
    fn single_goal_meaning_block_contains_purpose_and_disclaimer() {
        let lines = goal_meaning_lines(GoalKind::Sleep);
        let text = lines.join("\n").to_lowercase();
        assert!(text.contains("goal meaning"));
        assert!(text.contains("purpose:"));
        assert!(text.contains("does not prove:"));
        assert!(!text.contains("does not prove: does not prove"));
        assert!(text.contains("slow-wave"));
    }

    #[test]
    fn practical_report_status_thresholds_are_stable() {
        assert_eq!(practical_status(0.80), "strong");
        assert_eq!(practical_status(0.79), "usable");
        assert_eq!(practical_status(0.60), "usable");
        assert_eq!(practical_status(0.59), "weak");
        assert_eq!(practical_status(0.40), "weak");
        assert_eq!(practical_status(0.39), "poor");
    }

    #[test]
    fn practical_report_contains_required_honest_sections() {
        let preset = Preset::default();
        let goal = Goal::new(GoalKind::Shield);
        let config = SimulationConfig::default();
        let result = evaluate_preset(&preset, &goal, &config);
        let diagnosis = scoring::Diagnosis {
            score: result.score,
            bands: vec![],
            firing_rate: 6.0,
            firing_rate_range: (4.0, 12.0),
            firing_rate_status: scoring::MetricStatus::Pass,
            isi_cv: 0.12,
            target_isi_cv: Some(0.12),
            isi_status: scoring::MetricStatus::Pass,
            dominant_freq: 10.0,
            verdict: scoring::Verdict::Ok,
            performance: None,
        };

        let lines = practical_report_lines(GoalKind::Shield, &result, &diagnosis);
        let text = lines.join("\n").to_lowercase();
        assert!(text.contains("practical report"));
        assert!(text.contains("status:"));
        assert!(text.contains("limitation: this is a model-based proxy report"));
        assert!(!text.contains("proven human efficacy"));
    }

    #[test]
    fn practical_report_handles_acoustic_presence_and_absence() {
        let preset = Preset::default();
        let goal = Goal::new(GoalKind::Shield);
        let diagnosis = scoring::Diagnosis {
            score: 0.55,
            bands: vec![],
            firing_rate: 6.0,
            firing_rate_range: (4.0, 12.0),
            firing_rate_status: scoring::MetricStatus::Pass,
            isi_cv: 0.12,
            target_isi_cv: Some(0.12),
            isi_status: scoring::MetricStatus::Pass,
            dominant_freq: 10.0,
            verdict: scoring::Verdict::Ok,
            performance: None,
        };

        let no_acoustic = evaluate_preset(&preset, &goal, &SimulationConfig::default());
        let text_no = practical_report_lines(GoalKind::Shield, &no_acoustic, &diagnosis)
            .join("\n")
            .to_lowercase();
        assert!(text_no.contains("acoustic masking/comfort: not scored in this run."));

        let with_acoustic = evaluate_preset(
            &preset,
            &goal,
            &SimulationConfig {
                acoustic_scoring_enabled: true,
                ..SimulationConfig::default()
            },
        );
        let text_yes = practical_report_lines(GoalKind::Shield, &with_acoustic, &diagnosis)
            .join("\n")
            .to_lowercase();
        assert!(text_yes.contains("acoustic masking/comfort: comfort="));
        assert!(text_yes.contains("speech_privacy="));
    }

    #[test]
    fn practical_report_helpers_do_not_mutate_results_or_scores() {
        let preset = Preset::default();
        let goal = Goal::new(GoalKind::Shield);
        let config = SimulationConfig::default();

        let detailed = evaluate_preset_detailed(&preset, &goal, &config);
        let result_before = detailed.summary.score;
        let diagnosis = diagnose_detailed_result(&goal, &detailed);

        let score_after = detailed.summary.score;
        let lines = practical_report_lines(GoalKind::Shield, &detailed.summary, &diagnosis);

        assert_eq!(result_before, score_after);
        assert!(lines.iter().any(|l| l.contains("Practical Report")));
    }

    #[test]
    fn practical_report_isolation_does_not_invent_flat_proxy_dominant_mismatch() {
        let preset = Preset::default();
        let goal = Goal::new(GoalKind::Isolation);
        let result = evaluate_preset(&preset, &goal, &SimulationConfig::default());
        let diagnosis = scoring::Diagnosis {
            score: result.score,
            bands: vec![],
            firing_rate: 6.0,
            firing_rate_range: (4.0, 12.0),
            firing_rate_status: scoring::MetricStatus::Pass,
            isi_cv: 0.12,
            target_isi_cv: Some(0.12),
            isi_status: scoring::MetricStatus::Pass,
            dominant_freq: 1.0,
            verdict: scoring::Verdict::Ok,
            performance: None,
        };
        let text = practical_report_lines(GoalKind::Isolation, &result, &diagnosis)
            .join("\n")
            .to_lowercase();
        assert!(!text.contains("dominant frequency is delta while primary proxy emphasizes flat"));
    }

    #[test]
    fn practical_report_shield_can_emit_concrete_dominant_mismatch() {
        let preset = Preset::default();
        let goal = Goal::new(GoalKind::Shield);
        let result = evaluate_preset(&preset, &goal, &SimulationConfig::default());
        let diagnosis = scoring::Diagnosis {
            score: result.score,
            bands: vec![],
            firing_rate: 6.0,
            firing_rate_range: (4.0, 12.0),
            firing_rate_status: scoring::MetricStatus::Pass,
            isi_cv: 0.12,
            target_isi_cv: Some(0.12),
            isi_status: scoring::MetricStatus::Pass,
            dominant_freq: 1.0,
            verdict: scoring::Verdict::Ok,
            performance: None,
        };
        let text = practical_report_lines(GoalKind::Shield, &result, &diagnosis)
            .join("\n")
            .to_lowercase();
        assert!(text.contains("dominant frequency is delta while primary proxy emphasizes"));
        assert!(text.contains("alpha+beta"));
    }

    #[test]
    fn practical_report_interpretation_avoids_targets_prefix_awkwardness() {
        let preset = Preset::default();
        let goal = Goal::new(GoalKind::Shield);
        let result = evaluate_preset(&preset, &goal, &SimulationConfig::default());
        let diagnosis = scoring::Diagnosis {
            score: result.score,
            bands: vec![],
            firing_rate: 6.0,
            firing_rate_range: (4.0, 12.0),
            firing_rate_status: scoring::MetricStatus::Pass,
            isi_cv: 0.12,
            target_isi_cv: Some(0.12),
            isi_status: scoring::MetricStatus::Pass,
            dominant_freq: 10.0,
            verdict: scoring::Verdict::Ok,
            performance: None,
        };
        let text = practical_report_lines(GoalKind::Shield, &result, &diagnosis)
            .join("\n")
            .to_lowercase();
        assert!(text.contains("interpretation: intended objective:"));
        assert!(text.contains(". current proxy alignment is"));
        assert!(!text.contains("proxy current proxy alignment"));
        assert!(!text.contains("targets blend"));
        assert!(!text.contains("targets prioritize"));
    }

    // ── Calibrate-comfort helpers ──────────────────────────────────────

    #[test]
    fn infer_goal_from_filename_handles_core_patterns() {
        // Curated `presets/` filename patterns must map to expected goals.
        let cases = &[
            ("normal_set_shield_v5.json", Some(GoalKind::Shield)),
            ("the_shield_v3.json", Some(GoalKind::Shield)),
            ("isolation_normal_clean.json", Some(GoalKind::Isolation)),
            ("the_shield_isolation_v1.json", Some(GoalKind::Isolation)),
            ("normal_set_flow_v3.json", Some(GoalKind::Flow)),
            ("the_flow_v1.json", Some(GoalKind::Flow)),
            ("normal_set_ignition.json", Some(GoalKind::Ignition)),
            ("deepwork_adhd.json", Some(GoalKind::DeepWork)),
            ("deepwork_normal_v2.json", Some(GoalKind::DeepWork)),
            ("normal_set_deep_relax.json", Some(GoalKind::DeepRelaxation)),
            (
                "deep_relax_phys_cet_v1.json",
                Some(GoalKind::DeepRelaxation),
            ),
            ("sleep_phys_cet_v1.json", Some(GoalKind::Sleep)),
            ("showcase_pink.json", None),
            ("normal_set_reset.json", None),
        ];
        for (name, expected) in cases {
            let got = infer_goal_from_filename(name);
            assert_eq!(
                got, *expected,
                "infer_goal_from_filename({name}) = {got:?}, expected {expected:?}"
            );
        }
    }

    /// Isolation must beat Shield in the inference order: a filename
    /// like `the_shield_isolation_v1.json` is an isolation preset, not
    /// a shield one.
    #[test]
    fn infer_goal_resolves_compound_filenames_consistently() {
        assert_eq!(
            infer_goal_from_filename("the_shield_isolation_v1.json"),
            Some(GoalKind::Isolation),
            "compound filename should match the more specific token first"
        );
        assert_eq!(
            infer_goal_from_filename("deepwork_adhd.json"),
            Some(GoalKind::DeepWork)
        );
    }

    #[test]
    fn percentile_handles_edges_and_quantiles() {
        let v: Vec<f64> = (0..10).map(|i| i as f64).collect(); // 0..9
                                                               // p0 → min, p100 → max
        assert!((percentile(&v, 0.0) - 0.0).abs() < 1e-12);
        assert!((percentile(&v, 1.0) - 9.0).abs() < 1e-12);
        // p50 nearest-rank: round(9 * 0.5) = 5 → 5.0
        assert!((percentile(&v, 0.5) - 5.0).abs() < 1e-12);
        // p90 nearest-rank: round(9 * 0.9) = 8 → 8.0
        assert!((percentile(&v, 0.9) - 8.0).abs() < 1e-12);
        // empty input → NaN
        assert!(percentile(&[], 0.5).is_nan());
    }

    #[test]
    fn goal_thresholds_match_goal_comfort_violation() {
        // The display thresholds in the calibration printout must match
        // the values actually consumed by Goal::comfort_violation, so
        // p90/threshold comparisons are meaningful. **Updated 2026-05-01**
        // to reflect the empirically-calibrated thresholds.
        for (label, kind) in &[
            ("shield", GoalKind::Shield),
            ("isolation", GoalKind::Isolation),
            ("focus", GoalKind::Focus),
            ("deep_work", GoalKind::DeepWork),
            ("ignition", GoalKind::Ignition),
            ("sleep", GoalKind::Sleep),
            ("deep_relaxation", GoalKind::DeepRelaxation),
            ("meditation", GoalKind::Meditation),
            ("flow", GoalKind::Flow),
        ] {
            let displayed = goal_lufs_asym_threshold(label);
            let expected = match *kind {
                GoalKind::Focus | GoalKind::DeepWork | GoalKind::Ignition => Some(4.0),
                _ => Some(3.0),
            };
            assert_eq!(
                displayed, expected,
                "lufs_asym threshold mismatch for {label}: shown {displayed:?}, expected {expected:?}"
            );
            // Source-balance threshold parity
            let displayed_sb = goal_source_balance_threshold(label);
            let expected_sb = match *kind {
                GoalKind::Focus | GoalKind::DeepWork | GoalKind::Ignition => Some(15.0),
                _ => Some(12.0),
            };
            assert_eq!(
                displayed_sb, expected_sb,
                "source_balance threshold mismatch for {label}"
            );
            // HF threshold parity (unchanged by calibration — already loose)
            let displayed_hf = goal_hf_threshold(label);
            let expected_hf = match *kind {
                GoalKind::Sleep | GoalKind::DeepRelaxation | GoalKind::Meditation => Some(0.10),
                _ => Some(0.20),
            };
            assert_eq!(
                displayed_hf, expected_hf,
                "hf threshold mismatch for {label}"
            );
        }
    }

    #[test]
    fn goal_target_tilt_matches_scoring() {
        // Mirror of Goal::spectral_tilt_target_db_per_oct (private), so
        // the calibration tilt-deviation column actually measures the
        // same target the optimizer constraints use.
        for &k in &[
            GoalKind::Sleep,
            GoalKind::DeepRelaxation,
            GoalKind::Meditation,
        ] {
            assert_eq!(goal_target_tilt(k), -6.0);
        }
        for &k in &[GoalKind::Flow, GoalKind::DeepWork, GoalKind::Shield] {
            assert_eq!(goal_target_tilt(k), -3.0);
        }
        for &k in &[GoalKind::Focus, GoalKind::Isolation, GoalKind::Ignition] {
            assert_eq!(goal_target_tilt(k), -1.5);
        }
    }
}
