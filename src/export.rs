use crate::model_signature::ModelSignature;
/// JSON export of optimised presets.
///
/// Outputs a JSON file that maps directly to the NoiseEngine API,
/// making it trivial to load in iOS/WASM apps.
use crate::pipeline::{evaluate_preset, SignatureReplayError, SimulationConfig, SimulationResult};
use crate::preset::Preset;
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
    };

    let json = serde_json::to_string_pretty(&export)?;
    std::fs::write(output_path, json)?;
    Ok(())
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

    if !differences.is_empty() {
        return Err(ReplayExportError::NumericalMismatch(differences));
    }
    Ok(ReplayReport {
        goal: goal_kind,
        score: result.score,
        checked_numeric_fields: 10,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain_type::BrainType;
    use crate::model_signature::{RendererRevision, MODEL_SIGNATURE_SCHEMA_VERSION};

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
        let signature = SimulationConfig::default().model_signature();
        assert_eq!(signature.schema_version, MODEL_SIGNATURE_SCHEMA_VERSION);
        assert_eq!(signature.renderer_revision, RendererRevision::DspBrownHfV2);
        assert_eq!(
            signature.renderer_revision.as_str(),
            noise_generator_core::RENDERER_REVISION
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
