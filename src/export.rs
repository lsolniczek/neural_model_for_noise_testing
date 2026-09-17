use crate::model_signature::ModelSignature;
/// JSON export of optimised presets.
///
/// Outputs a JSON file that maps directly to the NoiseEngine API,
/// making it trivial to load in iOS/WASM apps.
use crate::pipeline::{evaluate_preset, SignatureReplayError, SimulationConfig, SimulationResult};
use crate::preset::Preset;
use crate::replicated::{evaluate_preset_replicated, ReplicatedEvaluation};
use crate::scoring::{Goal, GoalKind, GoalSemantics};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::Path;

const REPLAY_TOLERANCE: f64 = 1e-12;

#[derive(Debug, Serialize, Deserialize)]
pub struct PresetExport {
    pub meta: ExportMeta,
    pub preset: Preset,
    pub analysis: ExportAnalysis,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replicated_analysis: Option<ReplicatedEvaluation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optimizer_provenance: Option<OptimizerProvenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizerProvenance {
    pub schema: String,
    pub genome_schema: String,
    pub environment_render_revision: String,
    pub seed: u64,
    pub population: usize,
    pub generations: usize,
    pub de_f: f64,
    pub de_cr: f64,
    pub boundary_policy: String,
    pub search_replicates: usize,
    pub finalist_count: usize,
    pub finalist_replicates: usize,
    pub duration_secs: f32,
    pub constrained: bool,
    pub crowding: bool,
    pub stagnation_window: usize,
    pub stagnation_fraction: f64,
    pub frozen_context_hash: String,
    pub generated_trials: usize,
    pub duplicate_trials: usize,
    pub dead_only_trials: usize,
    pub shade_memory: usize,
    pub population_min: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_population_had_strict_feasible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_comfort_violation: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub final_strict_feasible: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportMeta {
    pub goal: String,
    pub goal_semantics: GoalSemantics,
    pub score: f64,
    /// Stage 0 compact serialized config object for exact model provenance.
    pub model_signature: ModelSignature,
    pub generated_at: String,
    pub optimizer_generations: usize,
    pub audio_duration_secs: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportAnalysis {
    pub fhn_firing_rate: f64,
    pub fhn_isi_cv: f64,
    pub dominant_freq_hz: f64,
    pub band_powers: ExportBandPowers,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ExportBandPowers {
    pub delta: f64,
    pub theta: f64,
    pub alpha: f64,
    pub beta: f64,
    pub gamma: f64,
}

pub fn export_preset(
    preset: &Preset,
    result: &SimulationResult,
    goal: GoalKind,
    generations: usize,
    duration_secs: f32,
    output_path: &Path,
) -> std::io::Result<()> {
    let export = PresetExport {
        meta: ExportMeta {
            goal: goal.to_string(),
            goal_semantics: goal.semantics(),
            score: result.score,
            model_signature: result.model_signature.clone(),
            generated_at: Utc::now().to_rfc3339(),
            optimizer_generations: generations,
            audio_duration_secs: duration_secs,
        },
        preset: preset.clone(),
        analysis: ExportAnalysis {
            fhn_firing_rate: result.fhn_firing_rate,
            fhn_isi_cv: if result.fhn_isi_cv.is_nan() {
                -1.0
            } else {
                result.fhn_isi_cv
            },
            dominant_freq_hz: result.dominant_freq,
            band_powers: ExportBandPowers {
                delta: result.delta_power,
                theta: result.theta_power,
                alpha: result.alpha_power,
                beta: result.beta_power,
                gamma: result.gamma_power,
            },
        },
        replicated_analysis: None,
        optimizer_provenance: None,
    };

    let json = serde_json::to_string_pretty(&export)?;
    std::fs::write(output_path, json)?;
    Ok(())
}

pub fn attach_optimizer_provenance(
    output_path: &Path,
    provenance: &OptimizerProvenance,
) -> std::io::Result<()> {
    let json = std::fs::read_to_string(output_path)?;
    let mut export: PresetExport = serde_json::from_str(&json)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;
    export.optimizer_provenance = Some(provenance.clone());
    std::fs::write(
        output_path,
        serde_json::to_string_pretty(&export)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?,
    )
}

pub fn attach_replicated_analysis(
    output_path: &Path,
    replicated: &ReplicatedEvaluation,
) -> std::io::Result<()> {
    let bytes = std::fs::read(output_path)?;
    let mut export: PresetExport = serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
    export.replicated_analysis = Some(replicated.clone());
    let json = serde_json::to_string_pretty(&export).map_err(std::io::Error::other)?;
    std::fs::write(output_path, json)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplayReport {
    pub goal: GoalKind,
    pub score: f64,
    pub checked_numeric_fields: usize,
}

#[derive(Debug)]
pub enum ReplayExportError {
    Input(String),
    UnsupportedSignature(SignatureReplayError),
    NumericalMismatch(Vec<String>),
}

impl ReplayExportError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Input(_) => 1,
            Self::UnsupportedSignature(_) | Self::NumericalMismatch(_) => 2,
        }
    }
}

impl fmt::Display for ReplayExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(message) => f.write_str(message),
            Self::UnsupportedSignature(error) => write!(f, "unsupported signature: {error}"),
            Self::NumericalMismatch(differences) => {
                writeln!(f, "replay differs from the exported result:")?;
                for difference in differences {
                    writeln!(f, "  - {difference}")?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for ReplayExportError {}

fn compare_replay_field(differences: &mut Vec<String>, field: &str, expected: f64, actual: f64) {
    if !expected.is_finite() || !actual.is_finite() || (expected - actual).abs() > REPLAY_TOLERANCE
    {
        differences.push(format!(
            "{field}: expected {expected:.15}, got {actual:.15}"
        ));
    }
}

fn compare_optional_replay_field(
    differences: &mut Vec<String>,
    field: &str,
    expected: Option<f64>,
    actual: Option<f64>,
) {
    match (expected, actual) {
        (Some(expected), Some(actual)) => {
            compare_replay_field(differences, field, expected, actual)
        }
        (None, None) => {}
        _ => differences.push(format!("{field}: optional value presence differs")),
    }
}

fn compare_summary(
    differences: &mut Vec<String>,
    field: &str,
    expected: &crate::replicated::ScalarSummary,
    actual: &crate::replicated::ScalarSummary,
) {
    if expected.n != actual.n {
        differences.push(format!(
            "{field}.n: expected {}, got {}",
            expected.n, actual.n
        ));
    }
    compare_replay_field(
        differences,
        &format!("{field}.mean"),
        expected.mean,
        actual.mean,
    );
    compare_optional_replay_field(
        differences,
        &format!("{field}.sample_std"),
        expected.sample_std,
        actual.sample_std,
    );
    compare_optional_replay_field(
        differences,
        &format!("{field}.standard_error"),
        expected.standard_error,
        actual.standard_error,
    );
    match (
        expected.confidence_interval_95.as_ref(),
        actual.confidence_interval_95.as_ref(),
    ) {
        (Some(expected), Some(actual)) => {
            compare_replay_field(
                differences,
                &format!("{field}.confidence_interval_95.lower"),
                expected.lower,
                actual.lower,
            );
            compare_replay_field(
                differences,
                &format!("{field}.confidence_interval_95.upper"),
                expected.upper,
                actual.upper,
            );
        }
        (None, None) => {}
        _ => differences.push(format!(
            "{field}.confidence_interval_95: optional value presence differs"
        )),
    }
    compare_replay_field(
        differences,
        &format!("{field}.min"),
        expected.min,
        actual.min,
    );
    compare_replay_field(
        differences,
        &format!("{field}.max"),
        expected.max,
        actual.max,
    );
}

/// Re-evaluate an exported preset with the exact configuration recorded in its
/// model signature and compare every exported numerical result.
pub fn replay_export(path: &Path) -> Result<ReplayReport, ReplayExportError> {
    let bytes = std::fs::read(path).map_err(|error| {
        ReplayExportError::Input(format!("cannot read '{}': {error}", path.display()))
    })?;
    let export: PresetExport = serde_json::from_slice(&bytes).map_err(|error| {
        ReplayExportError::Input(format!("invalid export JSON '{}': {error}", path.display()))
    })?;
    let goal_kind = GoalKind::from_str(&export.meta.goal).ok_or_else(|| {
        ReplayExportError::Input(format!("unknown goal '{}' in export", export.meta.goal))
    })?;
    if export.meta.goal_semantics.goal != goal_kind {
        return Err(ReplayExportError::Input(format!(
            "goal '{}' disagrees with goal_semantics '{}'; export is inconsistent",
            export.meta.goal, export.meta.goal_semantics.goal
        )));
    }

    let config = SimulationConfig::try_from(&export.meta.model_signature)
        .map_err(ReplayExportError::UnsupportedSignature)?;
    let goal = Goal::new(goal_kind);
    let result = evaluate_preset(&export.preset, &goal, &config);
    let replay_isi_cv = if result.fhn_isi_cv.is_nan() {
        -1.0
    } else {
        result.fhn_isi_cv
    };

    let mut differences = Vec::new();
    compare_replay_field(
        &mut differences,
        "meta.audio_duration_secs",
        f64::from(export.meta.audio_duration_secs),
        f64::from(config.duration_secs),
    );
    compare_replay_field(
        &mut differences,
        "meta.score",
        export.meta.score,
        result.score,
    );
    compare_replay_field(
        &mut differences,
        "analysis.fhn_firing_rate",
        export.analysis.fhn_firing_rate,
        result.fhn_firing_rate,
    );
    compare_replay_field(
        &mut differences,
        "analysis.fhn_isi_cv",
        export.analysis.fhn_isi_cv,
        replay_isi_cv,
    );
    compare_replay_field(
        &mut differences,
        "analysis.dominant_freq_hz",
        export.analysis.dominant_freq_hz,
        result.dominant_freq,
    );
    compare_replay_field(
        &mut differences,
        "analysis.band_powers.delta",
        export.analysis.band_powers.delta,
        result.delta_power,
    );
    compare_replay_field(
        &mut differences,
        "analysis.band_powers.theta",
        export.analysis.band_powers.theta,
        result.theta_power,
    );
    compare_replay_field(
        &mut differences,
        "analysis.band_powers.alpha",
        export.analysis.band_powers.alpha,
        result.alpha_power,
    );
    compare_replay_field(
        &mut differences,
        "analysis.band_powers.beta",
        export.analysis.band_powers.beta,
        result.beta_power,
    );
    compare_replay_field(
        &mut differences,
        "analysis.band_powers.gamma",
        export.analysis.band_powers.gamma,
        result.gamma_power,
    );

    let mut checked_numeric_fields = 10;
    if let Some(expected_aggregate) = export.replicated_analysis.as_ref() {
        let first = expected_aggregate.replicates.first().ok_or_else(|| {
            ReplayExportError::Input("replicated_analysis has no realizations".to_string())
        })?;
        let replay_aggregate = evaluate_preset_replicated(
            &export.preset,
            &goal,
            &config,
            first.identity.run_seed,
            first.identity.panel,
            expected_aggregate.replicates.len(),
        )
        .map_err(|error| ReplayExportError::Input(error.to_string()))?;
        if expected_aggregate.replicates.len() != replay_aggregate.replicates.len() {
            differences.push(format!(
                "replicated_analysis.replicates: expected {}, got {}",
                expected_aggregate.replicates.len(),
                replay_aggregate.replicates.len()
            ));
        }
        for (index, (expected, actual)) in expected_aggregate
            .replicates
            .iter()
            .zip(replay_aggregate.replicates.iter())
            .enumerate()
        {
            if expected.identity != actual.identity {
                differences.push(format!(
                    "replicated_analysis.replicates[{index}].identity differs"
                ));
            }
            macro_rules! compare_observation {
                ($field:ident) => {
                    compare_replay_field(
                        &mut differences,
                        &format!(
                            "replicated_analysis.replicates[{index}].{}",
                            stringify!($field)
                        ),
                        expected.$field,
                        actual.$field,
                    )
                };
            }
            compare_observation!(score);
            compare_observation!(fhn_firing_rate);
            compare_observation!(dominant_freq);
            compare_observation!(delta_power);
            compare_observation!(theta_power);
            compare_observation!(alpha_power);
            compare_observation!(beta_power);
            compare_observation!(gamma_power);
            compare_observation!(brightness);
        }
        compare_summary(
            &mut differences,
            "replicated_analysis.score",
            &expected_aggregate.score,
            &replay_aggregate.score,
        );
        compare_summary(
            &mut differences,
            "replicated_analysis.fhn_firing_rate",
            &expected_aggregate.fhn_firing_rate,
            &replay_aggregate.fhn_firing_rate,
        );
        compare_summary(
            &mut differences,
            "replicated_analysis.dominant_freq",
            &expected_aggregate.dominant_freq,
            &replay_aggregate.dominant_freq,
        );
        compare_summary(
            &mut differences,
            "replicated_analysis.delta_power",
            &expected_aggregate.delta_power,
            &replay_aggregate.delta_power,
        );
        compare_summary(
            &mut differences,
            "replicated_analysis.theta_power",
            &expected_aggregate.theta_power,
            &replay_aggregate.theta_power,
        );
        compare_summary(
            &mut differences,
            "replicated_analysis.alpha_power",
            &expected_aggregate.alpha_power,
            &replay_aggregate.alpha_power,
        );
        compare_summary(
            &mut differences,
            "replicated_analysis.beta_power",
            &expected_aggregate.beta_power,
            &replay_aggregate.beta_power,
        );
        compare_summary(
            &mut differences,
            "replicated_analysis.gamma_power",
            &expected_aggregate.gamma_power,
            &replay_aggregate.gamma_power,
        );
        compare_summary(
            &mut differences,
            "replicated_analysis.brightness",
            &expected_aggregate.brightness,
            &replay_aggregate.brightness,
        );
        checked_numeric_fields += expected_aggregate.replicates.len() * 10 + 9 * 7;
    }

    if !differences.is_empty() {
        return Err(ReplayExportError::NumericalMismatch(differences));
    }
    Ok(ReplayReport {
        goal: goal_kind,
        score: result.score,
        checked_numeric_fields,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain_type::BrainType;
    use crate::model_signature::{
        RendererRevision, LEGACY_MODEL_SIGNATURE_SCHEMA_VERSION, MODEL_SIGNATURE_SCHEMA_VERSION,
    };
    use crate::reproducibility::{SeedPanel, SeedPolicy};

    fn temp_export_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "nmm_{name}_{}_{}.json",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ))
    }

    #[test]
    fn replay_export_round_trip_matches_current_renderer() {
        let path = temp_export_path("replay_round_trip");
        let preset = Preset::default();
        let goal_kind = GoalKind::Focus;
        let goal = Goal::new(goal_kind);
        let config = SimulationConfig {
            duration_secs: 3.0,
            brain_type: BrainType::Normal,
            ..SimulationConfig::default()
        };
        let result = evaluate_preset(&preset, &goal, &config);
        export_preset(&preset, &result, goal_kind, 3, config.duration_secs, &path).unwrap();

        let replay = replay_export(&path).expect("current export should replay exactly");
        assert_eq!(replay.goal, goal_kind);
        assert_eq!(replay.score.to_bits(), result.score.to_bits());
        assert_eq!(replay.checked_numeric_fields, 10);

        let mut json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        json["meta"]["score"] = serde_json::json!(result.score + 0.01);
        std::fs::write(&path, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
        let error = replay_export(&path).unwrap_err();
        assert_eq!(error.exit_code(), 2);
        assert!(error.to_string().contains("meta.score"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn replay_export_checks_every_seeded_replicate_and_aggregate() {
        let path = temp_export_path("replicated_replay_round_trip");
        let preset = Preset::default();
        let goal_kind = GoalKind::Focus;
        let goal = Goal::new(goal_kind);
        let config = SimulationConfig {
            duration_secs: 2.1,
            seed_policy: SeedPolicy::domain_separated(42, SeedPanel::Finalist, 0),
            ..SimulationConfig::default()
        };
        let result = evaluate_preset(&preset, &goal, &config);
        let aggregate =
            evaluate_preset_replicated(&preset, &goal, &config, 42, SeedPanel::Finalist, 2)
                .unwrap();
        export_preset(&preset, &result, goal_kind, 3, config.duration_secs, &path).unwrap();
        attach_replicated_analysis(&path, &aggregate).unwrap();

        let decoded: PresetExport = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            serde_json::to_value(&decoded.preset).unwrap(),
            serde_json::to_value(&preset).unwrap(),
            "exported preset must round-trip exactly"
        );

        let replay = replay_export(&path).expect("replicated export should replay exactly");
        assert!(replay.checked_numeric_fields > 10);

        let mut json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let score = json["replicated_analysis"]["replicates"][1]["score"]
            .as_f64()
            .unwrap();
        json["replicated_analysis"]["replicates"][1]["score"] = serde_json::json!(score + 0.01);
        std::fs::write(&path, serde_json::to_vec_pretty(&json).unwrap()).unwrap();
        let error = replay_export(&path).unwrap_err();
        assert!(error.to_string().contains("replicated_analysis"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn old_unversioned_signature_is_readable_but_not_replayable() {
        let mut signature = SimulationConfig::default().model_signature();
        signature.schema_version = 1;
        signature.renderer_revision = RendererRevision::LegacyUnversioned;
        signature.renderer_source_revision = None;
        let json = serde_json::to_value(&signature).unwrap();
        let mut object = json.as_object().unwrap().clone();
        object.remove("schema_version");
        object.remove("renderer_revision");
        object.remove("renderer_source_revision");
        let decoded: crate::model_signature::ModelSignature =
            serde_json::from_value(serde_json::Value::Object(object)).unwrap();
        assert_eq!(decoded.schema_version, 1);
        assert_eq!(
            decoded.renderer_revision,
            RendererRevision::LegacyUnversioned
        );
        assert!(SimulationConfig::try_from(&decoded).is_err());
    }

    #[test]
    fn malformed_export_is_an_input_error() {
        let path = temp_export_path("malformed");
        std::fs::write(&path, b"not-json").unwrap();
        let error = replay_export(&path).unwrap_err();
        assert_eq!(error.exit_code(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn signature_schema_and_renderer_match_the_linked_dsp() {
        let legacy = SimulationConfig::default().model_signature();
        assert_eq!(legacy.schema_version, LEGACY_MODEL_SIGNATURE_SCHEMA_VERSION);
        assert_eq!(
            legacy.renderer_revision,
            RendererRevision::DspBrownHfV2BinauralBeatV1
        );
        assert_eq!(
            legacy.renderer_revision.as_str(),
            noise_generator_core::RENDERER_REVISION
        );
        let seeded = SimulationConfig {
            seed_policy: SeedPolicy::domain_separated(42, SeedPanel::Direct, 0),
            ..SimulationConfig::default()
        }
        .model_signature();
        assert_eq!(seeded.schema_version, MODEL_SIGNATURE_SCHEMA_VERSION);
        assert_eq!(
            seeded.renderer_revision,
            RendererRevision::DspBrownHfV2BinauralBeatV1SeededV1
        );
        assert_eq!(
            seeded.renderer_revision.as_str(),
            noise_generator_core::SEEDED_RENDERER_REVISION
        );
        let lockfile = include_str!("../Cargo.lock");
        assert!(
            lockfile.contains(crate::model_signature::DSP_SOURCE_REVISION),
            "Cargo.lock must pin the DSP revision exported in ModelSignature"
        );
        let manifest = include_str!("../Cargo.toml");
        assert!(
            manifest.contains(crate::model_signature::DSP_SOURCE_REVISION),
            "Cargo.toml must pin the DSP revision exported in ModelSignature"
        );
    }
}
