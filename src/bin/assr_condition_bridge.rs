use neural_preset_optimizer::auditory::{
    ArousalSource, AssrTransfer, LatentStateEstimate, ModulationBandPowers, TemporalModulationFeatures,
};
use neural_preset_optimizer::brain_type::BrainType;
use neural_preset_optimizer::neural::simulate_candidate_v2;
use serde::Serialize;

#[derive(Debug, Serialize)]
struct BridgeOutput {
    bridge_version: &'static str,
    model_version: &'static str,
    prediction_level: &'static str,
    prediction_status: &'static str,
    strength_scale: &'static str,
    predicted_dominant_modulation_hz: Option<f64>,
    dominant_rate_status: &'static str,
    predicted_gamma_assr_response_strength: f64,
}

fn band_powers_for_rate(rate_hz: f64, total_power: f64) -> ModulationBandPowers {
    let mut p = ModulationBandPowers {
        slow_0p5_4_hz: 0.0,
        theta_4_8_hz: 0.0,
        alpha_8_13_hz: 0.0,
        beta_13_30_hz: 0.0,
        gamma_30_50_hz: 0.0,
    };
    if (0.5..4.0).contains(&rate_hz) {
        p.slow_0p5_4_hz = total_power;
    } else if (4.0..8.0).contains(&rate_hz) {
        p.theta_4_8_hz = total_power;
    } else if (8.0..13.0).contains(&rate_hz) {
        p.alpha_8_13_hz = total_power;
    } else if (13.0..30.0).contains(&rate_hz) {
        p.beta_13_30_hz = total_power;
    } else {
        p.gamma_30_50_hz = total_power;
    }
    p
}

fn main() {
    let mut args = std::env::args().skip(1);
    let mut modulation_rate_hz = 40.0_f64;
    while let Some(arg) = args.next() {
        if arg == "--modulation-rate-hz" {
            if let Some(v) = args.next() {
                if let Ok(parsed) = v.parse::<f64>() {
                    modulation_rate_hz = parsed;
                }
            }
        }
    }

    let assr = AssrTransfer::new();
    let total_power = assr.gain(modulation_rate_hz).max(0.0);
    let temporal = TemporalModulationFeatures {
        modulation_psd: Vec::new(),
        dominant_modulation_hz: Some(modulation_rate_hz),
        band_power_by_mod_rate: band_powers_for_rate(modulation_rate_hz, total_power),
        total_modulation_power: total_power,
    };
    let latent = LatentStateEstimate {
        estimated_arousal: 0.5,
        arousal_source: ArousalSource::NeutralDefault,
    };
    let response = simulate_candidate_v2(&temporal, &latent, &BrainType::Normal);
    let out = BridgeOutput {
        bridge_version: "stage8d_b_assr_condition_bridge_v1",
        model_version: "candidate_v2",
        prediction_level: "condition_level",
        prediction_status: "model_derived_condition_level",
        strength_scale: "surrogate_not_same_scale_eeg_power",
        predicted_dominant_modulation_hz: None,
        dominant_rate_status: "unavailable_no_independent_model_rate_estimator_stage8d_b",
        predicted_gamma_assr_response_strength: response.gamma.response_strength,
    };
    println!("{}", serde_json::to_string(&out).expect("bridge JSON"));
}
