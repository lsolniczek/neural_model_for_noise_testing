/// JSON export of optimised presets.
///
/// Outputs a JSON file that maps directly to the NoiseEngine API,
/// making it trivial to load in iOS/WASM apps.
use crate::pipeline::SimulationResult;
use crate::preset::Preset;
use crate::scoring::GoalKind;
use chrono::Utc;
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
pub struct PresetExport {
    pub meta: ExportMeta,
    pub preset: Preset,
    pub analysis: ExportAnalysis,
}

#[derive(Serialize)]
pub struct ExportMeta {
    pub goal: String,
    pub score: f64,
    pub generated_at: String,
    pub optimizer_generations: usize,
    pub audio_duration_secs: f32,
}

#[derive(Serialize)]
pub struct ExportAnalysis {
    pub fhn_firing_rate: f64,
    pub fhn_isi_cv: f64,
    pub dominant_freq_hz: f64,
    pub band_powers: ExportBandPowers,
}

#[derive(Serialize)]
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
            score: result.score,
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
