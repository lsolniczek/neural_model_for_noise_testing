use crate::auditory::{LatentStateEstimate, TemporalModulationFeatures};
use crate::brain_type::BrainType;
use serde::{Deserialize, Serialize};

const EPS: f64 = 1e-12;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateRhythmModule {
    SlowDeltaTheta,
    Alpha,
    Beta,
    GammaAssr,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateCorticalDrive {
    /// Absolute candidate modulation-band power sum from Stage 3 features.
    pub total_modulation_power: f64,
    /// Monotonic engineering compression of `total_modulation_power`.
    /// This is a provisional candidate descriptor, not a physiological transfer function.
    pub modulation_strength: f64,
    pub slow_drive: f64,
    pub alpha_drive: f64,
    pub beta_drive: f64,
    pub gamma_drive: f64,
    pub dominant_modulation_hz: Option<f64>,
    pub estimated_arousal: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateRhythmModuleResponse {
    pub module: CandidateRhythmModule,
    pub drive: f64,
    pub gain: f64,
    pub response_strength: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateCorticalResponse {
    pub drive: CandidateCorticalDrive,
    pub slow: CandidateRhythmModuleResponse,
    pub alpha: CandidateRhythmModuleResponse,
    pub beta: CandidateRhythmModuleResponse,
    pub gamma: CandidateRhythmModuleResponse,
    pub dominant_module: Option<CandidateRhythmModule>,
    pub modulation_responsiveness_index: f64,
}

/// Stage 4 candidate priors are explicit engineering coefficients.
/// They are intentionally provisional and not validated physiology.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CandidateCorticalPrior {
    pub module_gain_slow: f64,
    pub module_gain_alpha: f64,
    pub module_gain_beta: f64,
    pub module_gain_gamma: f64,
    pub assr_center_hz: f64,
    pub assr_sigma_hz: f64,
    pub assr_gamma_boost: f64,
}

impl CandidateCorticalPrior {
    fn neutral() -> Self {
        Self {
            module_gain_slow: 1.0,
            module_gain_alpha: 1.0,
            module_gain_beta: 1.0,
            module_gain_gamma: 1.0,
            assr_center_hz: 40.0,
            assr_sigma_hz: 6.0,
            assr_gamma_boost: 0.60,
        }
    }
}

fn gamma_assr_boost(dominant_modulation_hz: Option<f64>, prior: &CandidateCorticalPrior) -> f64 {
    let Some(f) = dominant_modulation_hz else {
        return 1.0;
    };
    if !f.is_finite() {
        return 1.0;
    }
    // Provisional ASSR-like candidate prior around 40 Hz.
    let z = (f - prior.assr_center_hz) / prior.assr_sigma_hz.max(1e-9);
    1.0 + prior.assr_gamma_boost.max(0.0) * (-0.5 * z * z).exp()
}

fn module_response(
    module: CandidateRhythmModule,
    drive: f64,
    modulation_strength: f64,
    gain: f64,
) -> CandidateRhythmModuleResponse {
    CandidateRhythmModuleResponse {
        module,
        drive,
        gain,
        response_strength: (modulation_strength.max(0.0) * drive.max(0.0) * gain.max(0.0)).max(0.0),
    }
}

fn modulation_strength_from_total_power(total_modulation_power: f64) -> f64 {
    // Monotonic dynamic-range compression (engineering choice): keep magnitude sensitivity
    // while avoiding domination by outlier amplitudes.
    total_modulation_power.max(0.0).sqrt()
}

fn inactive_candidate_response(
    modulation: &TemporalModulationFeatures,
    state: &LatentStateEstimate,
) -> CandidateCorticalResponse {
    let drive = CandidateCorticalDrive {
        total_modulation_power: modulation.total_modulation_power.max(0.0),
        modulation_strength: 0.0,
        slow_drive: 0.0,
        alpha_drive: 0.0,
        beta_drive: 0.0,
        gamma_drive: 0.0,
        dominant_modulation_hz: None,
        estimated_arousal: state.estimated_arousal.clamp(0.0, 1.0),
    };
    let zero = |module: CandidateRhythmModule| CandidateRhythmModuleResponse {
        module,
        drive: 0.0,
        gain: 1.0,
        response_strength: 0.0,
    };
    CandidateCorticalResponse {
        drive,
        slow: zero(CandidateRhythmModule::SlowDeltaTheta),
        alpha: zero(CandidateRhythmModule::Alpha),
        beta: zero(CandidateRhythmModule::Beta),
        gamma: zero(CandidateRhythmModule::GammaAssr),
        dominant_module: None,
        modulation_responsiveness_index: 0.0,
    }
}

pub fn simulate_candidate_v2(
    modulation: &TemporalModulationFeatures,
    state: &LatentStateEstimate,
    brain: &BrainType,
) -> CandidateCorticalResponse {
    // Stage 7: candidate brain profiles are exported/inspectable metadata.
    // They intentionally do not retune candidate dynamics yet.
    let _profile = brain.candidate_profile_v2();

    let band = modulation.band_power_by_mod_rate;
    let slow_raw = band.slow_0p5_4_hz + band.theta_4_8_hz;
    let alpha_raw = band.alpha_8_13_hz;
    let beta_raw = band.beta_13_30_hz;
    let gamma_raw = band.gamma_30_50_hz;
    let sum_raw = (slow_raw + alpha_raw + beta_raw + gamma_raw).max(0.0);
    if modulation.dominant_modulation_hz.is_none() || sum_raw <= EPS {
        return inactive_candidate_response(modulation, state);
    }

    let prior = CandidateCorticalPrior::neutral();
    let modulation_strength = modulation_strength_from_total_power(sum_raw);
    let denom = sum_raw.max(EPS);

    let drive = CandidateCorticalDrive {
        total_modulation_power: sum_raw,
        modulation_strength,
        slow_drive: (slow_raw / denom).max(0.0),
        alpha_drive: (alpha_raw / denom).max(0.0),
        beta_drive: (beta_raw / denom).max(0.0),
        gamma_drive: (gamma_raw / denom).max(0.0),
        dominant_modulation_hz: modulation.dominant_modulation_hz,
        estimated_arousal: state.estimated_arousal.clamp(0.0, 1.0),
    };

    let slow = module_response(
        CandidateRhythmModule::SlowDeltaTheta,
        drive.slow_drive,
        drive.modulation_strength,
        prior.module_gain_slow,
    );
    let alpha = module_response(
        CandidateRhythmModule::Alpha,
        drive.alpha_drive,
        drive.modulation_strength,
        prior.module_gain_alpha,
    );
    let beta = module_response(
        CandidateRhythmModule::Beta,
        drive.beta_drive,
        drive.modulation_strength,
        prior.module_gain_beta,
    );
    let gamma = module_response(
        CandidateRhythmModule::GammaAssr,
        drive.gamma_drive,
        drive.modulation_strength,
        prior.module_gain_gamma * gamma_assr_boost(drive.dominant_modulation_hz, &prior),
    );

    let mut module_strengths = [
        (
            CandidateRhythmModule::SlowDeltaTheta,
            slow.response_strength,
        ),
        (CandidateRhythmModule::Alpha, alpha.response_strength),
        (CandidateRhythmModule::Beta, beta.response_strength),
        (CandidateRhythmModule::GammaAssr, gamma.response_strength),
    ];
    module_strengths.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                let key = |m: CandidateRhythmModule| match m {
                    CandidateRhythmModule::SlowDeltaTheta => 0,
                    CandidateRhythmModule::Alpha => 1,
                    CandidateRhythmModule::Beta => 2,
                    CandidateRhythmModule::GammaAssr => 3,
                };
                key(a.0).cmp(&key(b.0))
            })
    });
    let dominant_module = Some(module_strengths[0].0);
    let modulation_responsiveness_index = (slow.response_strength
        + alpha.response_strength
        + beta.response_strength
        + gamma.response_strength)
        .max(0.0);

    CandidateCorticalResponse {
        drive,
        slow,
        alpha,
        beta,
        gamma,
        dominant_module,
        modulation_responsiveness_index,
    }
}
