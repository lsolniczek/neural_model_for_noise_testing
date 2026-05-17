use crate::brain_type::{BandModelType, BrainType, CandidateBrainProfile, TonotopicParams};
use crate::auditory::ArousalModel;
use crate::neural::fhn::legacy_constants_snapshot as fhn_legacy_constants_snapshot;
use crate::neural::jansen_rit::legacy_constants_snapshot;
use crate::neural::wilson_cowan::{
    WilsonCowanModel, WilsonCowanParams, DEFAULT_ADAPTIVE_ENTRAINMENT_RANGE_HZ,
};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelVersion {
    LegacyV1,
    CandidateV2,
}

impl fmt::Display for ModelVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ModelVersion::LegacyV1 => write!(f, "legacy_v1"),
            ModelVersion::CandidateV2 => write!(f, "candidate_v2"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineVariant {
    EvaluateCanonical,
    EvaluateCandidateV2,
    DisturbCanonical,
    DisturbLegacyAblated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoringProfile {
    LegacyV1,
    CandidateResearchV2,
    ProductAcoustic,
}

impl fmt::Display for ScoringProfile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ScoringProfile::LegacyV1 => write!(f, "legacy_v1"),
            ScoringProfile::CandidateResearchV2 => write!(f, "candidate_research_v2"),
            ScoringProfile::ProductAcoustic => write!(f, "product_acoustic"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationMode {
    GlobalPerEar,
    PerBandPerEar,
}

impl fmt::Display for NormalizationMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NormalizationMode::GlobalPerEar => write!(f, "global_per_ear"),
            NormalizationMode::PerBandPerEar => write!(f, "per_band_per_ear"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum BandModelTypeSnapshot {
    JansenRit,
    WilsonCowan {
        target_hz: f64,
        tau_e: f64,
        tau_i: f64,
        w_ee: f64,
        w_ie: f64,
        w_ei: f64,
        w_ii: f64,
        h_e: f64,
        h_i: f64,
        sigmoid_a: f64,
        sigmoid_theta: f64,
        input_scale: f64,
        input_offset: f64,
        adaptive_entrainment_range_hz: f64,
    },
}

fn wc_snapshot_params(target_hz: f64, params: WilsonCowanParams) -> BandModelTypeSnapshot {
    BandModelTypeSnapshot::WilsonCowan {
        target_hz,
        tau_e: params.tau_e,
        tau_i: params.tau_i,
        w_ee: params.w_ee,
        w_ie: params.w_ie,
        w_ei: params.w_ei,
        w_ii: params.w_ii,
        h_e: params.h_e,
        h_i: params.h_i,
        sigmoid_a: params.sigmoid_a,
        sigmoid_theta: params.sigmoid_theta,
        input_scale: params.input_scale,
        input_offset: params.input_offset,
        adaptive_entrainment_range_hz: DEFAULT_ADAPTIVE_ENTRAINMENT_RANGE_HZ,
    }
}

fn band_model_snapshot(model: BandModelType, input_scale: f64) -> BandModelTypeSnapshot {
    match model {
        BandModelType::JansenRit => BandModelTypeSnapshot::JansenRit,
        BandModelType::WilsonCowan(target_hz) => {
            let wc_input_scale = input_scale * 0.01;
            wc_snapshot_params(
                target_hz,
                WilsonCowanModel::effective_params_for_frequency(target_hz, wc_input_scale),
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TonotopicParamsSnapshot {
    pub band_rates: [(f64, f64); 4],
    pub band_gains: [(f64, f64); 4],
    pub band_offsets: [f64; 4],
    pub band_input_gains: [f64; 4],
    pub band_output_weights: [f64; 4],
    pub band_slow_inhib_ratios: [f64; 4],
    pub band_c7: [f64; 4],
    pub band_sigmoid_r: [f64; 4],
    pub band_c1c2_scale: [f64; 4],
    pub band_g_fast_rate: [f64; 4],
    pub band_v0: [f64; 4],
    pub band_model_types: [BandModelTypeSnapshot; 4],
}

impl TonotopicParamsSnapshot {
    fn from_params(params: &TonotopicParams, input_scale: f64) -> Self {
        Self {
            band_rates: params.band_rates,
            band_gains: params.band_gains,
            band_offsets: params.band_offsets,
            band_input_gains: params.band_input_gains,
            band_output_weights: params.band_output_weights,
            band_slow_inhib_ratios: params.band_slow_inhib_ratios,
            band_c7: params.band_c7,
            band_sigmoid_r: params.band_sigmoid_r,
            band_c1c2_scale: params.band_c1c2_scale,
            band_g_fast_rate: params.band_g_fast_rate,
            band_v0: params.band_v0,
            band_model_types: params
                .band_model_types
                .map(|model| band_model_snapshot(model, input_scale)),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BilateralParamsSnapshot {
    pub left: TonotopicParamsSnapshot,
    pub right: TonotopicParamsSnapshot,
    pub callosal_coupling: f64,
    pub callosal_delay_s: f64,
    pub contralateral_ratio: f64,
    pub left_weight: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuditoryFeatureFlags {
    pub assr_enabled: bool,
    pub thalamic_gate_enabled: bool,
    pub physiological_thalamic_gate_enabled: bool,
    pub cet_enabled: bool,
    pub habituation_enabled: bool,
    pub acoustic_scoring_enabled: bool,
    pub acoustic_score_fusion_enabled: bool,
    pub acoustic_constraints_enabled: bool,
    pub arousal_model: ArousalModel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NeuralFeatureFlags {
    pub stochastic_jr_enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NumericParamsSnapshot {
    pub jr_stochastic_sigma: f64,
    pub cet_b_slow_rate: f64,
    pub cet_b_slow_gain: f64,
    pub fixed_arousal: Option<f64>,

    pub fhn_a: f64,
    pub fhn_b: f64,
    pub fhn_epsilon: f64,
    pub fhn_input_scale: f64,
    pub fhn_time_scale: f64,
    pub fhn_spike_threshold: f64,
    pub fhn_initial_voltage: f64,
    pub fhn_initial_recovery: f64,
    pub fhn_rk4_sub_steps: usize,
    pub fhn_isi_cv_min_spikes: usize,
    pub fhn_isi_cv_min_mean_isi: f64,

    pub jr_a_gain: f64,
    pub jr_b_gain: f64,
    pub jr_a_rate: f64,
    pub jr_b_rate: f64,
    pub jr_c: f64,
    pub jr_input_offset: f64,
    pub jr_input_scale: f64,
    pub jr_g_fast_gain: f64,
    pub jr_g_fast_rate: f64,
    pub jr_c5: f64,
    pub jr_c6: f64,
    pub jr_c7: f64,
    pub jr_slow_inhib_ratio: f64,
    pub jr_v0: f64,

    pub callosal_coupling: f64,
    pub callosal_delay_s: f64,
    pub contralateral_ratio: f64,
    pub left_weight: f64,
    pub habituation_rate: f64,
    pub habituation_recovery: f64,
    pub cet_c_slow_connectivity: f64,
    pub jr_stochastic_rng_seed: u64,
    pub jr_v_max: f64,
    pub jr_default_c: f64,
    pub jr_default_c1: f64,
    pub jr_default_c2: f64,
    pub jr_default_c3: f64,
    pub jr_default_c4: f64,
    pub jr_default_v0: f64,
    pub jr_default_sigmoid_r: f64,
    pub jr_warmup_seconds: f64,
    pub jr_sub_steps_base: usize,
    pub jr_sub_steps_fast: usize,
    pub jr_sub_steps_fast_rate_threshold: f64,
    pub wc_adaptive_entrainment_range_hz: f64,
    pub tonotopic_params: TonotopicParamsSnapshot,
    pub bilateral_params: BilateralParamsSnapshot,
}

impl NumericParamsSnapshot {
    pub fn from_runtime(
        brain_type: BrainType,
        jr_stochastic_sigma: f64,
        cet_b_slow_rate: f64,
        cet_b_slow_gain: f64,
        fixed_arousal: Option<f64>,
        habituation_enabled: bool,
        cet_enabled: bool,
    ) -> Self {
        let params = brain_type.params();
        let tonotopic = brain_type.tonotopic_params();
        let bilateral = brain_type.bilateral_params();
        let fhn_constants = fhn_legacy_constants_snapshot();
        let jr_constants = legacy_constants_snapshot();
        Self {
            jr_stochastic_sigma,
            cet_b_slow_rate,
            cet_b_slow_gain,
            fixed_arousal,
            fhn_a: params.fhn.a,
            fhn_b: params.fhn.b,
            fhn_epsilon: params.fhn.epsilon,
            fhn_input_scale: params.fhn.input_scale,
            fhn_time_scale: params.fhn.time_scale,
            fhn_spike_threshold: fhn_constants.spike_threshold,
            fhn_initial_voltage: fhn_constants.initial_voltage,
            fhn_initial_recovery: fhn_constants.initial_recovery,
            fhn_rk4_sub_steps: fhn_constants.rk4_sub_steps,
            fhn_isi_cv_min_spikes: fhn_constants.isi_cv_min_spikes,
            fhn_isi_cv_min_mean_isi: fhn_constants.isi_cv_min_mean_isi,
            jr_a_gain: params.jansen_rit.a_gain,
            jr_b_gain: params.jansen_rit.b_gain,
            jr_a_rate: params.jansen_rit.a_rate,
            jr_b_rate: params.jansen_rit.b_rate,
            jr_c: params.jansen_rit.c,
            jr_input_offset: params.jansen_rit.input_offset,
            jr_input_scale: params.jansen_rit.input_scale,
            jr_g_fast_gain: params.jansen_rit.g_fast_gain,
            jr_g_fast_rate: params.jansen_rit.g_fast_rate,
            jr_c5: params.jansen_rit.c5,
            jr_c6: params.jansen_rit.c6,
            jr_c7: params.jansen_rit.c7,
            jr_slow_inhib_ratio: params.jansen_rit.slow_inhib_ratio,
            jr_v0: params.jansen_rit.v0,
            callosal_coupling: bilateral.callosal_coupling,
            callosal_delay_s: bilateral.callosal_delay_s,
            contralateral_ratio: bilateral.contralateral_ratio,
            left_weight: bilateral.left_weight,
            habituation_rate: if habituation_enabled { 0.0003 } else { 0.0 },
            habituation_recovery: if habituation_enabled { 0.0001 } else { 0.0 },
            cet_c_slow_connectivity: if cet_enabled { 30.0 } else { 0.0 },
            jr_stochastic_rng_seed: 42,
            jr_v_max: jr_constants.v_max,
            jr_default_c: jr_constants.default_c,
            jr_default_c1: jr_constants.default_c1,
            jr_default_c2: jr_constants.default_c2,
            jr_default_c3: jr_constants.default_c3,
            jr_default_c4: jr_constants.default_c4,
            jr_default_v0: jr_constants.default_v0,
            jr_default_sigmoid_r: jr_constants.default_sigmoid_r,
            jr_warmup_seconds: jr_constants.warmup_seconds,
            jr_sub_steps_base: jr_constants.sub_steps_base,
            jr_sub_steps_fast: jr_constants.sub_steps_fast,
            jr_sub_steps_fast_rate_threshold: jr_constants.sub_steps_fast_rate_threshold,
            wc_adaptive_entrainment_range_hz: DEFAULT_ADAPTIVE_ENTRAINMENT_RANGE_HZ,
            tonotopic_params: TonotopicParamsSnapshot::from_params(
                &tonotopic,
                params.jansen_rit.input_scale,
            ),
            bilateral_params: BilateralParamsSnapshot {
                left: TonotopicParamsSnapshot::from_params(
                    &bilateral.left,
                    params.jansen_rit.input_scale,
                ),
                right: TonotopicParamsSnapshot::from_params(
                    &bilateral.right,
                    params.jansen_rit.input_scale,
                ),
                callosal_coupling: bilateral.callosal_coupling,
                callosal_delay_s: bilateral.callosal_delay_s,
                contralateral_ratio: bilateral.contralateral_ratio,
                left_weight: bilateral.left_weight,
            },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ReproducibilitySeeds {
    pub primary_seed: Option<u64>,
    pub disturbance_left_spike_seed: Option<u64>,
    pub disturbance_right_spike_seed: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelSignature {
    pub version: ModelVersion,
    pub pipeline_variant: PipelineVariant,
    pub scoring_profile: ScoringProfile,
    pub normalization_mode: NormalizationMode,
    pub brain_type: BrainType,
    pub audio_sample_rate_hz: u32,
    pub neural_decimation_factor: usize,
    pub neural_sample_rate_hz: f64,
    pub auditory_flags: AuditoryFeatureFlags,
    pub neural_flags: NeuralFeatureFlags,
    pub numeric_params: NumericParamsSnapshot,
    pub warmup_discard_secs: f32,
    pub duration_secs: f32,
    pub seeds: ReproducibilitySeeds,
    pub candidate_brain_profile_v2: Option<CandidateBrainProfile>,
}
