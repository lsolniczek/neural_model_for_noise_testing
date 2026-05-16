use crate::acoustic_score::{extract_score_result_v1, AcousticScoreResult, RenderedStereoAudio};
/// Simulation pipeline: Engine → Auditory → Neural → Score.
///
/// Wires together the noise engine, cochlear filterbank, and neural models
/// into a single evaluation function that the optimizer calls.
use crate::auditory::{
    apply_rir, generate_rir, AssrTransfer, ButterworthCrossover, EnvironmentParams,
    GammatoneFilterbank, PhysiologicalThalamicGate, ThalamicGate,
};
use crate::brain_type::BrainType;
use crate::model_signature::{
    AuditoryFeatureFlags, ModelSignature, ModelVersion, NeuralFeatureFlags, NormalizationMode,
    NumericParamsSnapshot, PipelineVariant, ReproducibilitySeeds, ScoringProfile,
};
use crate::movement::MovementController;
use crate::neural::{
    simulate_bilateral, BilateralResult, FastInhibParams, FhnModel, FhnResult, PerformanceVector,
};
use crate::preset::Preset;
use crate::scoring::Goal;
use noise_generator_core::NoiseEngine;

use rustfft::{num_complex::Complex, FftPlanner};

pub(crate) const SAMPLE_RATE: u32 = 48_000;
/// Decimation factor: 48 kHz → 1 kHz for neural models.
pub(crate) const DECIMATION_FACTOR: usize = 48;
/// Neural model sample rate after decimation.
pub(crate) const NEURAL_SR: f64 = SAMPLE_RATE as f64 / DECIMATION_FACTOR as f64;
/// Default neural-analysis warm-up discard window (seconds).
pub(crate) const DEFAULT_WARMUP_DISCARD_SECS: f32 = 2.0;

/// Validate that the rendered duration leaves a non-empty analysis window
/// after the neural warm-up discard.
pub(crate) fn validate_analysis_window(
    duration_secs: f32,
    warmup_discard_secs: f32,
) -> Result<f32, String> {
    if !duration_secs.is_finite() || duration_secs <= 0.0 {
        return Err(format!(
            "duration must be a positive finite value; got {duration_secs:.3}s"
        ));
    }
    if !warmup_discard_secs.is_finite() || warmup_discard_secs < 0.0 {
        return Err(format!(
            "warm-up discard must be a non-negative finite value; got {warmup_discard_secs:.3}s"
        ));
    }

    let analysis_secs = duration_secs - warmup_discard_secs;
    if analysis_secs <= 0.0 {
        return Err(format!(
            "duration {duration_secs:.3}s must exceed warm-up discard {warmup_discard_secs:.3}s"
        ));
    }

    Ok(analysis_secs)
}

/// Decimate a signal by averaging blocks of `factor` samples (boxcar anti-alias + downsample).
///
/// Note: The boxcar filter has -13 dB sidelobes (Oppenheim & Schafer 2009),
/// which is insufficient for sharp anti-aliasing. However, the gammatone
/// filterbank's 80 Hz envelope lowpass (in gammatone.rs) already removes
/// content above ~80 Hz before this stage, so the boxcar only needs to
/// handle residual carrier leakage — which is adequately suppressed.
/// A Hann window over 48 samples (1ms) is too short for better cutoff;
/// proper improvement would require multi-stage decimation or a long FIR
/// (Crochiere & Rabiner 1983).
pub(crate) fn decimate(signal: &[f64], factor: usize) -> Vec<f64> {
    let out_len = signal.len() / factor;
    let inv = 1.0 / factor as f64;
    (0..out_len)
        .map(|i| {
            let start = i * factor;
            signal[start..start + factor].iter().sum::<f64>() * inv
        })
        .collect()
}

/// Deinterleave stereo buffer into separate L/R channels.
pub(crate) fn deinterleave(interleaved: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let num_frames = interleaved.len() / 2;
    let mut left = Vec::with_capacity(num_frames);
    let mut right = Vec::with_capacity(num_frames);
    for i in 0..num_frames {
        left.push(interleaved[i * 2]);
        right.push(interleaved[i * 2 + 1]);
    }
    (left, right)
}

pub struct SimulationConfig {
    /// Duration of audio to render per evaluation (seconds).
    pub duration_secs: f32,
    /// Initial seconds of neural output to discard before analysis.
    /// Allows differential-equation models to settle past startup transients.
    pub warmup_discard_secs: f32,
    /// Brain type profile for neural models.
    pub brain_type: BrainType,
    /// Enable ASSR transfer function between cochlea and cortex.
    pub assr_enabled: bool,
    /// Enable thalamic gate (arousal-dependent filtering).
    pub thalamic_gate_enabled: bool,
    /// Enable neural habituation (synaptic depression over time).
    /// Per Moran et al. (2011): sustained activity depresses connectivity.
    pub habituation_enabled: bool,
    /// Enable stochastic JR (noise breaks alpha attractor).
    /// Per Ableidinger et al. (2017): enables theta/delta production.
    pub stochastic_jr_enabled: bool,
    /// Enable Cortical Envelope Tracking (Priority 13).
    ///
    /// When true: (1) splits each band into a slow ≤10 Hz path and a fast
    /// >10 Hz path via the complementary crossover; (2) bypasses ASSR on
    /// the slow path so 1–8 Hz envelope modulations reach JR undamped;
    /// (3) enables the slow GABA_B inhibitory population in JR so the
    /// circuit can phase-lock to envelope rhythms; (4) computes envelope-
    /// phase PLV against the slow drive in addition to carrier PLV.
    /// Default false → bitwise regression-safe with all existing presets.
    pub cet_enabled: bool,
    /// Enable the physiological thalamic gate (Priority 9).
    ///
    /// When true, replaces the linear arousal → band_offset heuristic
    /// (`thalamic_gate_enabled`) with a single-compartment Hodgkin-Huxley
    /// TC cell whose K⁺ leak conductance (g_KL) is the master arousal knob.
    /// Burst↔tonic mode switching is driven by ion-channel dynamics
    /// (T-type Ca²⁺, Bazhenov 2002 / Paul 2016 / Destexhe 1996), producing
    /// a sigmoidal shift-vs-arousal shape rather than a linear ramp.
    ///
    /// Takes precedence over `thalamic_gate_enabled` when both are set:
    /// only one gate is applied per evaluation. Default false → bitwise
    /// regression-safe; the heuristic gate path is unchanged when this
    /// flag is off.
    pub physiological_thalamic_gate_enabled: bool,
    /// Enable Phase 0 acoustic-scoring scaffolding.
    ///
    /// This flag is intentionally inert in Phase 0 so the evaluation path
    /// stays bit-identical while later phases wire in acoustic analysis.
    pub acoustic_scoring_enabled: bool,
    /// Enable Phase 5 acoustic/NMM score fusion.
    ///
    /// Requires `acoustic_scoring_enabled=true`. In the first rollout pass
    /// this only affects Shield and Isolation scoring. All other goals keep
    /// the exact legacy NMM score path even when this flag is set.
    pub acoustic_score_fusion_enabled: bool,
    /// Enable Priority 28 Phase 2 ε-constrained optimization.
    ///
    /// This is a **scoring-path semantics flag**, not a pipeline-side
    /// computation flag. The pipeline still produces the standard
    /// `SimulationResult` and the legacy `score` field; the optimizer's
    /// constrained comparator is the consumer.
    ///
    /// Invariants enforced by `evaluate_preset_detailed`:
    ///   1. `acoustic_constraints_enabled = true` requires
    ///      `acoustic_scoring_enabled = true` (the comfort-violation
    ///      function in `Goal::comfort_violation` reads
    ///      `AcousticFeatureVector` fields, which are populated only when
    ///      acoustic scoring is enabled).
    ///   2. `acoustic_constraints_enabled = true` is incompatible with
    ///      `acoustic_score_fusion_enabled = true` because the latter
    ///      adds comfort terms to the score, while the former treats
    ///      comfort as a separate constraint dimension. Combining the
    ///      two double-counts comfort.
    ///
    /// Default `false` → legacy weighted-sum behaviour is preserved.
    pub acoustic_constraints_enabled: bool,
    /// Explicit model version for reproducible scientific baselines.
    pub model_version: ModelVersion,
    /// Optional run seed when an outer command has an explicit reproducibility seed.
    /// Does not affect simulation behavior by itself; metadata only.
    pub reproducibility_seed: Option<u64>,

    // ── Priority 18 — Theta-Alpha Coexistence parameters ───────────────
    //
    // Three parameters previously hardcoded inside `evaluate_preset_detailed`.
    // Promoted to `SimulationConfig` so they can be tuned per-run via CLI
    // without recompilation. Defaults match the historical hardcoded values
    // exactly, so legacy callers (those that build `SimulationConfig` via
    // `..Default::default()`) get bit-identical behaviour.
    /// Priority 18a — Stochastic noise σ on JR input drive `p`.
    ///
    /// Per Ableidinger, Buckwar & Hinterleitner (2017), noise on the
    /// JR system can drive transitions between the alpha attractor
    /// (~10 Hz limit cycle) and lower-frequency basins (theta/delta).
    /// Their preferred placement is on the velocity state variables
    /// (their σ₃, σ₄, σ₅), but our existing implementation places noise
    /// on the input drive `p` — a different stochastic model that still
    /// modulates basin escape, just less directly.
    ///
    /// Default 15.0 mirrors the pre-Priority-18 hardcoded constant.
    /// Useful range: 50–300 for breaking single-attractor lock on
    /// relaxation-family goals (Priority 18a Phase 1 sweep). Only takes
    /// effect when `stochastic_jr_enabled = true`; otherwise σ is forced
    /// to 0 so the model is fully deterministic.
    ///
    /// **Numerical stability note** (Ableidinger 2017 Fig. 9):
    /// Euler-Maruyama produces spurious bifurcations at σ ≈ 1000 with
    /// dt ∈ {1e-3, 2e-3, 5e-3}. Our RK4 integrator at dt = 1 ms is
    /// adequate for σ ≤ 200; values much larger should be validated
    /// case-by-case before production use.
    pub jr_stochastic_sigma: f64,

    /// Priority 18b — Slow inhibitory population decay rate `b_slow` (1/s)
    /// for the CET parallel slow-GABA loop in JR.
    ///
    /// Per Ursino, Cona & Zavaglia (2010): a second inhibitory population
    /// with a different time constant produces simultaneous multi-band
    /// rhythms in a single column. At the pre-Priority-18 default of
    /// 5.0 (τ = 200 ms), the slow population resonates near 0.8 Hz —
    /// effectively a DC offset, not a theta oscillator. Increasing to
    /// 25.0 (τ = 40 ms) places the resonance in the theta band (5–8 Hz)
    /// and produces a more balanced theta/alpha ratio.
    ///
    /// Default 5.0 preserves legacy behaviour. Only takes effect when
    /// `cet_enabled = true`; otherwise forced to 0.0.
    pub cet_b_slow_rate: f64,

    /// Priority 18b — Slow inhibitory population synaptic gain `B_slow` (mV)
    /// for the CET parallel slow-GABA loop in JR.
    ///
    /// Per Wendling et al. (2002): the gain on the slow inhibitory
    /// synapse controls whether the column produces alpha (B ≈ 45 in
    /// their paper's scale), sporadic spikes (B ≈ 38), or gamma (B ≈ 8).
    /// Our parameterisation uses a smaller absolute gain because the
    /// slow population is additive on top of the standard Wendling-JR
    /// fast inhibition; recommended range when retuning for theta is
    /// 12–20 mV.
    ///
    /// Default 10.0 preserves legacy behaviour. Only takes effect when
    /// `cet_enabled = true`; otherwise forced to 0.0.
    pub cet_b_slow_gain: f64,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        SimulationConfig {
            duration_secs: 12.0,
            warmup_discard_secs: DEFAULT_WARMUP_DISCARD_SECS,
            brain_type: BrainType::Normal,
            assr_enabled: true,
            thalamic_gate_enabled: true,
            habituation_enabled: true,
            stochastic_jr_enabled: true,
            cet_enabled: true,
            physiological_thalamic_gate_enabled: false,
            acoustic_scoring_enabled: false,
            acoustic_score_fusion_enabled: false,
            acoustic_constraints_enabled: false,
            model_version: ModelVersion::LegacyV1,
            reproducibility_seed: None,
            jr_stochastic_sigma: 15.0,
            cet_b_slow_rate: 5.0,
            cet_b_slow_gain: 10.0,
        }
    }
}

impl SimulationConfig {
    pub fn model_signature(&self) -> ModelSignature {
        ModelSignature {
            version: self.model_version,
            pipeline_variant: PipelineVariant::EvaluateCanonical,
            scoring_profile: ScoringProfile::LegacyV1,
            normalization_mode: NormalizationMode::GlobalPerEar,
            brain_type: self.brain_type,
            audio_sample_rate_hz: SAMPLE_RATE,
            neural_decimation_factor: DECIMATION_FACTOR,
            neural_sample_rate_hz: NEURAL_SR,
            auditory_flags: AuditoryFeatureFlags {
                assr_enabled: self.assr_enabled,
                thalamic_gate_enabled: self.thalamic_gate_enabled,
                physiological_thalamic_gate_enabled: self.physiological_thalamic_gate_enabled,
                cet_enabled: self.cet_enabled,
                habituation_enabled: self.habituation_enabled,
                acoustic_scoring_enabled: self.acoustic_scoring_enabled,
                acoustic_score_fusion_enabled: self.acoustic_score_fusion_enabled,
                acoustic_constraints_enabled: self.acoustic_constraints_enabled,
            },
            neural_flags: NeuralFeatureFlags {
                stochastic_jr_enabled: self.stochastic_jr_enabled,
            },
            numeric_params: NumericParamsSnapshot::from_runtime(
                self.brain_type,
                self.jr_stochastic_sigma,
                self.cet_b_slow_rate,
                self.cet_b_slow_gain,
                self.habituation_enabled,
                self.cet_enabled,
            ),
            warmup_discard_secs: self.warmup_discard_secs,
            duration_secs: self.duration_secs,
            seeds: ReproducibilitySeeds {
                primary_seed: self.reproducibility_seed,
                disturbance_left_spike_seed: None,
                disturbance_right_spike_seed: None,
            },
        }
    }
}

pub struct SimulationResult {
    pub model_signature: ModelSignature,
    pub score: f64,
    pub fhn_firing_rate: f64,
    pub fhn_isi_cv: f64,
    pub dominant_freq: f64,
    pub delta_power: f64,
    pub theta_power: f64,
    pub alpha_power: f64,
    pub beta_power: f64,
    pub gamma_power: f64,
    /// Spectral brightness [0, 1] — dark (brown) to bright (white).
    pub brightness: f64,
    /// Energy fraction per tonotopic band [Low, Low-mid, Mid-high, High].
    pub band_energy_fractions: [f64; 4],
    /// Left hemisphere dominant frequency (Hz).
    pub left_dominant_freq: f64,
    /// Right hemisphere dominant frequency (Hz).
    pub right_dominant_freq: f64,
    /// Alpha asymmetry index: (left_alpha - right_alpha) / (left_alpha + right_alpha).
    pub alpha_asymmetry: f64,
    /// Performance vector: entrainment, E/I stability, spectral centroid.
    pub performance: PerformanceVector,
    /// Optional acoustic-scoring payload reserved for later rollout phases.
    pub acoustic_score: Option<AcousticScoreResult>,
}

pub struct DetailedSimulationResult {
    /// Canonical scalar summary used by optimizer, matrix mode, export, etc.
    pub summary: SimulationResult,
    /// Full FHN output used by single-preset diagnostics.
    pub fhn: FhnResult,
    /// Full bilateral cortical result used by single-preset diagnostics.
    pub bilateral: BilateralResult,
    /// Optional rendered stereo ear signal reserved for acoustic analysis.
    pub acoustic_render: Option<RenderedStereoAudio>,
}

#[derive(Clone)]
pub(crate) struct AuditoryPreparedState {
    pub rendered_audio: RenderedStereoAudio,
    pub left_bands_dec: [Vec<f64>; 4],
    pub right_bands_dec: [Vec<f64>; 4],
    pub left_energy: [f64; 4],
    pub right_energy: [f64; 4],
    pub band_energy_fractions: [f64; 4],
    pub cet_envelope_ref: Option<Vec<f64>>,
    pub brightness: f64,
    pub arousal: f64,
    pub thalamic_band_shifts: [f64; 4],
    pub target_lfo_freq: Option<f64>,
}

pub(crate) struct CanonicalCorticalStageOutput {
    pub bilateral: BilateralResult,
    pub fhn: FhnResult,
    pub performance: PerformanceVector,
}

/// Compute spectral brightness from audio via FFT.
///
/// Returns a value in [0, 1] where 0 = very dark (all energy < 200 Hz)
/// and 1 = very bright (all energy > 4 kHz). Based on the spectral centroid
/// mapped through the audible range on a log scale.
pub(crate) fn spectral_brightness(audio: &[f32], sample_rate: f64) -> f64 {
    let n = audio.len();
    let fft_len = n.next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);

    let mut buf: Vec<Complex<f64>> = (0..fft_len)
        .map(|i| {
            if i < n {
                Complex::new(audio[i] as f64, 0.0)
            } else {
                Complex::new(0.0, 0.0)
            }
        })
        .collect();
    fft.process(&mut buf);

    let freq_res = sample_rate / fft_len as f64;
    let mut weighted_sum = 0.0_f64;
    let mut total_power = 0.0_f64;

    // Only consider 20 Hz – 20 kHz (audible range)
    let min_bin = (20.0 / freq_res).ceil() as usize;
    let max_bin = ((20000.0 / freq_res).floor() as usize).min(fft_len / 2);

    for bin in min_bin..max_bin {
        let freq = bin as f64 * freq_res;
        let power = buf[bin].norm_sqr();
        weighted_sum += freq * power;
        total_power += power;
    }

    let centroid = if total_power > 0.0 {
        weighted_sum / total_power
    } else {
        500.0
    };

    // Map centroid to [0, 1] on a log scale over 100 Hz – 10 kHz
    let log_low = 100.0_f64.ln();
    let log_high = 10000.0_f64.ln();
    let brightness = ((centroid.max(100.0).ln() - log_low) / (log_high - log_low)).clamp(0.0, 1.0);

    brightness
}

/// **HEURISTIC** — per-source volume range, in dB.
///
/// Returns `20·log10(max_volume / min_volume)` over the preset's
/// active objects (`active=true` and `volume > 1e-4`). Encodes the
/// user-facing `feedback_balanced_cocoon` rule: "active sources within
/// ~6 dB of each other; no single source should dominate".
///
/// Returns `0.0` when zero or one source is effectively active (no
/// pairwise comparison to make).
///
/// **What this is NOT.** It is not a K-weighted measurement of each
/// source's perceptual SPL. It deliberately ignores color (white,
/// pink, brown all have different K-weighted SPLs at the same volume
/// parameter), tint EQ, modulator gain, spread, reverb send, and
/// tone-source amplitude. Two sources at the same `volume` but
/// different colors will sound different in practice and this proxy
/// will not detect that imbalance.
///
/// **Why volume-only.**
///   1. No extra audio rendering required (free at runtime).
///   2. Directly mirrors the user-facing tuning rule.
///   3. For the typical Shield / Sleep / Flow color mix (pink, brown,
///      grey at similar RMS), volume ratios correlate reasonably with
///      K-weighted SPL ratios.
///
/// **Upgrade path.** Render each active source in isolation for ~0.5 s,
/// apply BS.1770-5 K-weighting, compute mean square per source,
/// convert to dB and return max−min. This costs ~0.5 s per active
/// source per evaluation. Worth doing if empirical tuning shows the
/// volume-only proxy missing audible imbalances.
/// **HEURISTIC** — Count of *effectively active* sources in the preset.
///
/// An object counts as effectively active when both `active=true` and
/// `volume > 1e-4`. The volume floor matches the
/// `compute_source_balance_db_range` helper so the two metrics agree on
/// "what is active". Used by `Goal::comfort_violation` to penalise
/// cocoon goals that collapse to 1–2 sources (which trivially satisfy
/// the per-source loudness-equity constraint but defeat the design
/// intent of a multi-source spatial cocoon).
pub fn compute_active_source_count(preset: &Preset) -> u32 {
    const VOLUME_FLOOR: f32 = 1e-4;
    preset
        .objects
        .iter()
        .filter(|obj| obj.active && obj.volume > VOLUME_FLOOR)
        .count() as u32
}

pub fn compute_source_balance_db_range(preset: &Preset) -> f64 {
    const VOLUME_FLOOR: f32 = 1e-4;
    let active_volumes: Vec<f64> = preset
        .objects
        .iter()
        .filter(|obj| obj.active && obj.volume > VOLUME_FLOOR)
        .map(|obj| obj.volume as f64)
        .collect();
    if active_volumes.len() <= 1 {
        return 0.0;
    }
    let max = active_volumes
        .iter()
        .cloned()
        .fold(f64::NEG_INFINITY, f64::max);
    let min = active_volumes.iter().cloned().fold(f64::INFINITY, f64::min);
    if !(max.is_finite() && min.is_finite()) || min < VOLUME_FLOOR as f64 {
        return 0.0;
    }
    20.0 * (max / min).log10()
}

/// Evaluate a preset against a goal.
///
/// This is the core function the optimizer calls for each candidate.
pub fn evaluate_preset(
    preset: &Preset,
    goal: &Goal,
    config: &SimulationConfig,
) -> SimulationResult {
    evaluate_preset_detailed(preset, goal, config).summary
}

pub(crate) fn render_preset_stereo_dry(preset: &Preset, duration_secs: f32) -> RenderedStereoAudio {
    if !duration_secs.is_finite() || duration_secs <= 0.0 {
        panic!("invalid render duration: {duration_secs:.3}s");
    }

    let num_frames = (SAMPLE_RATE as f32 * duration_secs) as u32;
    let engine = NoiseEngine::new(SAMPLE_RATE, 0.8);
    preset.apply_to_engine(&engine);

    let mut movement = MovementController::from_preset(preset);
    let warmup_frames = (SAMPLE_RATE as f32 * 1.0) as u32;
    let chunk_frames = (SAMPLE_RATE as f32 * 0.05) as u32;

    if movement.has_movement() {
        let warmup_chunks = warmup_frames / chunk_frames;
        let dt = chunk_frames as f64 / SAMPLE_RATE as f64;
        for _ in 0..warmup_chunks {
            movement.tick(dt, &engine);
            let _ = engine.render_audio(chunk_frames);
        }
    } else {
        let _ = engine.render_audio(warmup_frames);
    }

    let audio = if movement.has_movement() {
        let dt = chunk_frames as f64 / SAMPLE_RATE as f64;
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

    let (left, right) = deinterleave(&audio);
    RenderedStereoAudio::new(SAMPLE_RATE, left, right)
}

pub(crate) fn render_preset_ear_signals(
    preset: &Preset,
    duration_secs: f32,
) -> RenderedStereoAudio {
    let rendered = render_preset_stereo_dry(preset, duration_secs);
    if preset.room.uses_image_source() {
        return rendered;
    }
    let env_params = EnvironmentParams::from_index(preset.environment);
    if env_params.is_anechoic() {
        rendered
    } else {
        let rir = generate_rir(&env_params, rendered.sample_rate_hz);
        let left = apply_rir(&rendered.left, &rir, env_params.wet_mix);
        let right = apply_rir(&rendered.right, &rir, env_params.wet_mix);
        RenderedStereoAudio::new(rendered.sample_rate_hz, left, right)
    }
}

fn extract_target_lfo_frequency_from_preset(preset: &Preset) -> Option<f64> {
    preset
        .objects
        .iter()
        .filter(|obj| obj.active)
        .flat_map(|obj| {
            let vol = obj.volume as f64;
            let mut lfos = Vec::new();
            // NeuralLfo (kind=4) and Isochronic (kind=5) both drive entrainment.
            for modcfg in [&obj.bass_mod, &obj.satellite_mod] {
                if (modcfg.kind == 4 || modcfg.kind == 5) && modcfg.param_a > 0.5 {
                    lfos.push((modcfg.param_a as f64, modcfg.param_b as f64 * vol));
                }
            }
            lfos
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(freq, _strength)| freq)
}

pub(crate) fn prepare_canonical_auditory_state(
    preset: &Preset,
    config: &SimulationConfig,
) -> AuditoryPreparedState {
    let rendered_audio = render_preset_ear_signals(preset, config.duration_secs);
    let sr = rendered_audio.sample_rate_hz as f64;
    let left = rendered_audio.left.clone();
    let right = rendered_audio.right.clone();

    // 4. Cochlear model: tonotopic band-grouped processing.
    let mut filterbank_l = GammatoneFilterbank::new(sr);
    let mut filterbank_r = GammatoneFilterbank::new(sr);
    let bands_l = filterbank_l.process_to_band_groups(&left);
    let bands_r = filterbank_r.process_to_band_groups(&right);

    // 5. Normalise each ear's band signals to [0, 1] using GLOBAL max.
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

    let global_max_l = (0..4)
        .map(|b| bands_l.signals[b].iter().cloned().fold(0.0_f64, f64::max))
        .fold(0.0_f64, f64::max);
    let global_max_r = (0..4)
        .map(|b| bands_r.signals[b].iter().cloned().fold(0.0_f64, f64::max))
        .fold(0.0_f64, f64::max);

    let norm_l = if global_max_l > 1e-10 {
        1.0 / global_max_l
    } else {
        1.0
    };
    let norm_r = if global_max_r > 1e-10 {
        1.0 / global_max_r
    } else {
        1.0
    };

    for b in 0..4 {
        left_bands[b] = bands_l.signals[b].iter().map(|x| x * norm_l).collect();
        right_bands[b] = bands_r.signals[b].iter().map(|x| x * norm_r).collect();
    }

    // Average energy fractions for display.
    let mut band_energy_fractions = [0.0_f64; 4];
    for b in 0..4 {
        band_energy_fractions[b] =
            (bands_l.energy_fractions[b] + bands_r.energy_fractions[b]) * 0.5;
    }
    let ef_sum: f64 = band_energy_fractions.iter().sum();
    if ef_sum > 1e-30 {
        for ef in &mut band_energy_fractions {
            *ef /= ef_sum;
        }
    }

    // 5b. Spectral brightness from audio FFT (psychoacoustic complement).
    let brightness = spectral_brightness(&left, sr);

    // 5c. Decimate band signals from 48 kHz → 1 kHz for neural models.
    let discard_samples = (config.warmup_discard_secs as f64 * NEURAL_SR) as usize;
    let trim = |signal: &[f64]| -> Vec<f64> {
        let dec = decimate(signal, DECIMATION_FACTOR);
        let skip = discard_samples.min(dec.len());
        dec[skip..].to_vec()
    };

    let mut left_bands_dec: [Vec<f64>; 4] = [
        trim(&left_bands[0]),
        trim(&left_bands[1]),
        trim(&left_bands[2]),
        trim(&left_bands[3]),
    ];
    let mut right_bands_dec: [Vec<f64>; 4] = [
        trim(&right_bands[0]),
        trim(&right_bands[1]),
        trim(&right_bands[2]),
        trim(&right_bands[3]),
    ];

    // 5d. (Optional) Cortical Envelope Tracking crossover (Priority 13a).
    let mut cet_slow_left: Option<[Vec<f64>; 4]> = None;
    let mut cet_slow_right: Option<[Vec<f64>; 4]> = None;
    if config.cet_enabled {
        let mut slow_l: [Vec<f64>; 4] = [vec![], vec![], vec![], vec![]];
        let mut slow_r: [Vec<f64>; 4] = [vec![], vec![], vec![], vec![]];
        for b in 0..4 {
            let mut xover_l = ButterworthCrossover::cet_default(NEURAL_SR);
            let mut xover_r = ButterworthCrossover::cet_default(NEURAL_SR);
            let (sl, fl) = xover_l.process_signal(&left_bands_dec[b]);
            let (sr_l, fr) = xover_r.process_signal(&right_bands_dec[b]);
            left_bands_dec[b] = fl;
            right_bands_dec[b] = fr;
            slow_l[b] = sl;
            slow_r[b] = sr_l;
        }
        cet_slow_left = Some(slow_l);
        cet_slow_right = Some(slow_r);
    }

    // 5e. (Optional) ASSR attenuation on AC component only.
    if config.assr_enabled {
        let assr = AssrTransfer::new();
        let assr_mod = assr.compute_input_scale_modifier(preset);
        if assr_mod < 1.0 - 1e-10 {
            for bands in [&mut left_bands_dec, &mut right_bands_dec] {
                for band in bands.iter_mut() {
                    let n = band.len();
                    if n == 0 {
                        continue;
                    }
                    let mean = band.iter().sum::<f64>() / n as f64;
                    for sample in band.iter_mut() {
                        let ac = *sample - mean;
                        *sample = mean + ac * assr_mod;
                    }
                }
            }
        }
    }

    // 5f. Recombine CET slow path and build envelope reference.
    let cet_envelope_ref: Option<Vec<f64>> =
        if let (Some(slow_l), Some(slow_r)) = (&cet_slow_left, &cet_slow_right) {
            for b in 0..4 {
                for i in 0..left_bands_dec[b].len() {
                    left_bands_dec[b][i] += slow_l[b][i];
                }
                for i in 0..right_bands_dec[b].len() {
                    right_bands_dec[b][i] += slow_r[b][i];
                }
            }
            let n_env = slow_l[0].len();
            let mut env = vec![0.0_f64; n_env];
            for b in 0..4 {
                let w = (bands_l.energy_fractions[b] + bands_r.energy_fractions[b]) * 0.5;
                if w < 1e-10 {
                    continue;
                }
                for i in 0..n_env {
                    env[i] += w * 0.5 * (slow_l[b][i] + slow_r[b][i]);
                }
            }
            Some(env)
        } else {
            None
        };

    // 5g. Thalamic gating control signals.
    let arousal = if config.physiological_thalamic_gate_enabled || config.thalamic_gate_enabled {
        if config.physiological_thalamic_gate_enabled {
            PhysiologicalThalamicGate::compute_arousal(preset, brightness)
        } else {
            ThalamicGate::compute_arousal(preset, brightness)
        }
    } else {
        0.5
    };

    let thalamic_band_shifts = if config.physiological_thalamic_gate_enabled {
        let gate = PhysiologicalThalamicGate::new(arousal);
        gate.band_offset_shifts()
    } else if config.thalamic_gate_enabled {
        let gate = ThalamicGate::new(arousal);
        gate.band_offset_shifts()
    } else {
        [0.0; 4]
    };

    AuditoryPreparedState {
        rendered_audio,
        left_bands_dec,
        right_bands_dec,
        left_energy: bands_l.energy_fractions,
        right_energy: bands_r.energy_fractions,
        band_energy_fractions,
        cet_envelope_ref,
        brightness,
        arousal,
        thalamic_band_shifts,
        target_lfo_freq: extract_target_lfo_frequency_from_preset(preset),
    }
}

pub(crate) fn run_canonical_cortical_stage(
    auditory: &AuditoryPreparedState,
    config: &SimulationConfig,
) -> CanonicalCorticalStageOutput {
    let neural_params = config.brain_type.params();
    let mut bilateral = config.brain_type.bilateral_params();
    for b in 0..4 {
        if auditory.thalamic_band_shifts[b].abs() > 1e-10 {
            bilateral.left.band_offsets[b] += auditory.thalamic_band_shifts[b];
            bilateral.right.band_offsets[b] += auditory.thalamic_band_shifts[b];
        }
    }

    let fast_inhib = FastInhibParams {
        g_fast_gain: neural_params.jansen_rit.g_fast_gain,
        g_fast_rate: neural_params.jansen_rit.g_fast_rate,
        c5: neural_params.jansen_rit.c5,
        c6: neural_params.jansen_rit.c6,
        c7: neural_params.jansen_rit.c7,
    };

    let (b_slow_gain, b_slow_rate, c_slow) = if config.cet_enabled {
        (config.cet_b_slow_gain, config.cet_b_slow_rate, 30.0)
    } else {
        (0.0, 0.0, 0.0)
    };

    let bilateral_result = simulate_bilateral(
        &auditory.left_bands_dec,
        &auditory.right_bands_dec,
        &auditory.left_energy,
        &auditory.right_energy,
        &bilateral,
        neural_params.jansen_rit.c,
        neural_params.jansen_rit.input_scale,
        NEURAL_SR,
        &fast_inhib,
        neural_params.jansen_rit.v0,
        if config.habituation_enabled {
            0.0003
        } else {
            0.0
        },
        if config.habituation_enabled {
            0.0001
        } else {
            0.0
        },
        if config.stochastic_jr_enabled {
            config.jr_stochastic_sigma
        } else {
            0.0
        },
        b_slow_gain,
        b_slow_rate,
        c_slow,
        auditory.arousal,
    );

    let jr_result = &bilateral_result.combined;
    let fhn = FhnModel::with_params(
        NEURAL_SR,
        neural_params.fhn.a,
        neural_params.fhn.b,
        neural_params.fhn.epsilon,
        neural_params.fhn.time_scale,
    );
    let fhn_input: Vec<f64> = {
        let mut abs_values: Vec<f64> = jr_result.eeg.iter().map(|x| x.abs()).collect();
        abs_values.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let p95_idx = (abs_values.len() as f64 * 0.95) as usize;
        let p95 = abs_values[p95_idx.min(abs_values.len() - 1)];
        let scale = if p95 > 1e-10 { 1.0 / p95 } else { 1.0 };
        jr_result
            .eeg
            .iter()
            .map(|x| (x * scale).clamp(-3.0, 3.0))
            .collect()
    };
    let fhn_result = fhn.simulate(&fhn_input, neural_params.fhn.input_scale);

    let eeg_mean = jr_result.eeg.iter().sum::<f64>() / jr_result.eeg.len() as f64;
    let eeg_detrended: Vec<f64> = jr_result.eeg.iter().map(|x| x - eeg_mean).collect();
    let performance = PerformanceVector::compute_with_envelope(
        &eeg_detrended,
        &jr_result.fast_inhib_trace,
        NEURAL_SR,
        auditory.target_lfo_freq,
        auditory.cet_envelope_ref.as_deref(),
    );

    CanonicalCorticalStageOutput {
        bilateral: bilateral_result,
        fhn: fhn_result,
        performance,
    }
}

/// Canonical detailed evaluation path used by the human-facing `evaluate`
/// command. Returns the same scalar summary as `evaluate_preset()` plus the
/// full neural results needed for diagnosis, without re-running a second
/// shadow pipeline from `main.rs`.
pub fn evaluate_preset_detailed(
    preset: &Preset,
    goal: &Goal,
    config: &SimulationConfig,
) -> DetailedSimulationResult {
    validate_analysis_window(config.duration_secs, config.warmup_discard_secs)
        .unwrap_or_else(|message| panic!("invalid SimulationConfig: {message}"));
    if config.acoustic_score_fusion_enabled && !config.acoustic_scoring_enabled {
        panic!("invalid SimulationConfig: acoustic score fusion requires acoustic scoring");
    }
    // Priority 28 Phase 2 invariants — see `acoustic_constraints_enabled`
    // doc comment. Constraints need the comfort metrics to compute the
    // violation, and they cannot coexist with fusion (double-count).
    if config.acoustic_constraints_enabled && !config.acoustic_scoring_enabled {
        panic!("invalid SimulationConfig: acoustic constraints require acoustic scoring");
    }
    if config.acoustic_constraints_enabled && config.acoustic_score_fusion_enabled {
        panic!(
            "invalid SimulationConfig: acoustic constraints are incompatible with acoustic score fusion (would double-count comfort)"
        );
    }
    let auditory = prepare_canonical_auditory_state(preset, config);
    let mut acoustic_score = config
        .acoustic_scoring_enabled
        .then(|| extract_score_result_v1(&auditory.rendered_audio));
    if let Some(payload) = acoustic_score.as_mut() {
        payload.features.source_balance_db_range = Some(compute_source_balance_db_range(preset));
        payload.features.active_source_count = Some(compute_active_source_count(preset));
    }
    let acoustic_render = config
        .acoustic_scoring_enabled
        .then(|| auditory.rendered_audio.clone());

    let cortical = run_canonical_cortical_stage(&auditory, config);
    let jr_result = &cortical.bilateral.combined;

    // 9. Score: neural model + asymmetry penalty + carrier PLV bonus + envelope PLV bonus.
    let neural_score = goal.evaluate_full(
        &cortical.fhn,
        jr_result,
        cortical.bilateral.alpha_asymmetry,
        cortical.performance.plv,
        cortical.performance.envelope_plv,
    );
    let score = if config.acoustic_score_fusion_enabled {
        if let Some(acoustic) = acoustic_score.as_mut() {
            if let Some(fused) = goal.evaluate_with_acoustic_fusion(neural_score, acoustic) {
                acoustic.acoustic_goal_score = Some(fused.acoustic_goal_score);
                acoustic.comfort_score = Some(fused.comfort_score);
                acoustic.legacy_nmm_score = Some(neural_score);
                acoustic.fused_score_preview = Some(fused.fused_score);
                fused.fused_score
            } else {
                neural_score
            }
        } else {
            neural_score
        }
    } else {
        neural_score
    };
    let norm_bands = jr_result.band_powers.normalized();

    let summary = SimulationResult {
        model_signature: config.model_signature(),
        score,
        fhn_firing_rate: cortical.fhn.firing_rate,
        fhn_isi_cv: cortical.fhn.isi_cv,
        dominant_freq: jr_result.dominant_freq,
        delta_power: norm_bands.delta,
        theta_power: norm_bands.theta,
        alpha_power: norm_bands.alpha,
        beta_power: norm_bands.beta,
        gamma_power: norm_bands.gamma,
        brightness: auditory.brightness,
        band_energy_fractions: auditory.band_energy_fractions,
        left_dominant_freq: cortical.bilateral.left_dominant_freq,
        right_dominant_freq: cortical.bilateral.right_dominant_freq,
        alpha_asymmetry: cortical.bilateral.alpha_asymmetry,
        performance: cortical.performance,
        acoustic_score,
    };

    DetailedSimulationResult {
        summary,
        fhn: cortical.fhn,
        bilateral: cortical.bilateral,
        acoustic_render,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    // ---------------------------------------------------------------
    // Constants
    // ---------------------------------------------------------------

    #[test]
    fn neural_sr_is_1000() {
        assert_eq!(NEURAL_SR, 1000.0);
        assert_eq!(SAMPLE_RATE as f64 / DECIMATION_FACTOR as f64, 1000.0);
    }

    #[test]
    fn default_config_uses_legacy_v1_model_version() {
        let cfg = SimulationConfig::default();
        assert_eq!(cfg.model_version, ModelVersion::LegacyV1);
    }

    #[test]
    fn model_version_supports_reserved_candidate_v2() {
        let json = serde_json::to_string(&ModelVersion::CandidateV2)
            .expect("candidate_v2 model version should serialize");
        assert_eq!(json, "\"candidate_v2\"");
    }

    // ---------------------------------------------------------------
    // decimate
    // ---------------------------------------------------------------

    #[test]
    fn decimate_constant_signal_unchanged() {
        let signal = vec![3.0; 480]; // 480 samples / 48 = 10 output
        let dec = decimate(&signal, 48);
        assert_eq!(dec.len(), 10);
        for &v in &dec {
            assert!(
                (v - 3.0).abs() < 1e-12,
                "Constant signal should decimate to same value"
            );
        }
    }

    #[test]
    fn decimate_averages_blocks() {
        // Block of [0, 1, 2, 3] averaged = 1.5
        let signal: Vec<f64> = (0..8).map(|i| i as f64).collect();
        let dec = decimate(&signal, 4);
        assert_eq!(dec.len(), 2);
        assert!((dec[0] - 1.5).abs() < 1e-12); // (0+1+2+3)/4
        assert!((dec[1] - 5.5).abs() < 1e-12); // (4+5+6+7)/4
    }

    #[test]
    fn decimate_output_length() {
        let signal = vec![0.0; 4800];
        let dec = decimate(&signal, 48);
        assert_eq!(dec.len(), 100); // 4800 / 48
    }

    #[test]
    fn decimate_discards_remainder() {
        // 100 samples / 48 = 2 full blocks (96 samples), 4 remainder discarded
        let signal = vec![1.0; 100];
        let dec = decimate(&signal, 48);
        assert_eq!(dec.len(), 2);
    }

    #[test]
    fn decimate_preserves_low_frequency() {
        // A 10 Hz sine at 48 kHz, decimated to 1 kHz, should still be ~10 Hz
        let n = 48_000; // 1 second
        let signal: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 10.0 * i as f64 / 48_000.0).sin())
            .collect();
        let dec = decimate(&signal, 48);
        assert_eq!(dec.len(), 1000);

        // The decimated signal should still oscillate at ~10 Hz
        // Check it crosses zero multiple times (10 Hz → ~20 crossings/sec)
        let mut crossings = 0;
        for w in dec.windows(2) {
            if w[0] * w[1] < 0.0 {
                crossings += 1;
            }
        }
        assert!(
            crossings >= 15 && crossings <= 25,
            "10 Hz sine should have ~20 zero crossings after decimation, got {crossings}"
        );
    }

    // ---------------------------------------------------------------
    // deinterleave
    // ---------------------------------------------------------------

    #[test]
    fn deinterleave_splits_correctly() {
        let interleaved = vec![1.0_f32, 2.0, 3.0, 4.0, 5.0, 6.0];
        let (left, right) = deinterleave(&interleaved);
        assert_eq!(left, vec![1.0, 3.0, 5.0]);
        assert_eq!(right, vec![2.0, 4.0, 6.0]);
    }

    #[test]
    fn deinterleave_empty() {
        let (left, right) = deinterleave(&[]);
        assert!(left.is_empty());
        assert!(right.is_empty());
    }

    #[test]
    fn deinterleave_output_length() {
        let interleaved = vec![0.0_f32; 200];
        let (left, right) = deinterleave(&interleaved);
        assert_eq!(left.len(), 100);
        assert_eq!(right.len(), 100);
    }

    fn simple_active_preset() -> crate::preset::Preset {
        let mut preset = crate::preset::Preset::default();
        preset.source_count = 1;
        preset.objects[0].active = true;
        preset.objects[0].color = 1;
        preset.objects[0].volume = 0.8;
        preset.objects[0].x = 1.5;
        preset.objects[0].z = 1.0;
        preset
    }

    /// Helper: build a preset with the given vector of (active, volume)
    /// pairs. Used by §28b source-balance tests.
    fn preset_with_volumes(volumes: &[(bool, f32)]) -> crate::preset::Preset {
        let mut preset = crate::preset::Preset::default();
        for (i, &(active, vol)) in volumes.iter().take(crate::preset::MAX_OBJECTS).enumerate() {
            preset.objects[i].active = active;
            preset.objects[i].color = 1;
            preset.objects[i].volume = vol;
        }
        preset.source_count = volumes.iter().filter(|(a, v)| *a && *v > 1e-4).count() as u32;
        preset
    }

    // ── Priority 28 §28b — compute_source_balance_db_range ────────────

    #[test]
    fn source_balance_zero_for_empty_preset() {
        let p = crate::preset::Preset::default();
        assert_eq!(compute_source_balance_db_range(&p), 0.0);
    }

    #[test]
    fn source_balance_zero_for_single_active_source() {
        let p = preset_with_volumes(&[(true, 0.5)]);
        assert_eq!(compute_source_balance_db_range(&p), 0.0);
    }

    #[test]
    fn source_balance_zero_for_equal_volumes() {
        let p = preset_with_volumes(&[(true, 0.3), (true, 0.3), (true, 0.3)]);
        assert!(compute_source_balance_db_range(&p).abs() < 1e-12);
    }

    #[test]
    fn source_balance_matches_log_ratio() {
        // 0.5 / 0.25 = 2× → 6.02 dB
        let p = preset_with_volumes(&[(true, 0.5), (true, 0.25)]);
        let r = compute_source_balance_db_range(&p);
        assert!(
            (r - 6.0).abs() < 0.05,
            "2× volume ratio should be ~6 dB, got {r:.4}"
        );
    }

    #[test]
    fn source_balance_ignores_inactive_sources() {
        // 0.5 active and 0.05 inactive → only 0.5 counts → 0 dB
        let p = preset_with_volumes(&[(true, 0.5), (false, 0.05)]);
        assert_eq!(compute_source_balance_db_range(&p), 0.0);
    }

    #[test]
    fn source_balance_ignores_below_volume_floor() {
        // active but vol < 1e-4 is treated as silent
        let p = preset_with_volumes(&[(true, 0.5), (true, 1e-6)]);
        assert_eq!(compute_source_balance_db_range(&p), 0.0);
    }

    #[test]
    fn source_balance_seed_shield_v5_within_six_db() {
        // Seed preset volumes from presets/normal_set_shield_v5.json:
        // 0.35, 0.35, 0.35, 0.20, 0.22 → 20·log10(0.35/0.20) ≈ 4.86 dB
        let p = preset_with_volumes(&[
            (true, 0.35),
            (true, 0.35),
            (true, 0.35),
            (true, 0.20),
            (true, 0.22),
        ]);
        let r = compute_source_balance_db_range(&p);
        assert!(
            (r - 4.86).abs() < 0.1,
            "shield_v5 source balance should be ~4.86 dB, got {r:.4}"
        );
        assert!(
            r < 6.0,
            "shield_v5 should be within Shield's 6 dB threshold"
        );
    }

    /// Pin the imbalance-detection contract using volumes from the
    /// optimized preset that motivated §28b: vols 0.28, 0.28, 0.11, 0.51
    /// → range 20·log10(0.51/0.11) ≈ 13.3 dB, well past any goal threshold.
    #[test]
    fn source_balance_dominant_source_violates_six_db() {
        let p = preset_with_volumes(&[
            (true, 0.28),
            (true, 0.28),
            (false, 0.0), // skipped (vol 0)
            (true, 0.11),
            (true, 0.51),
        ]);
        let r = compute_source_balance_db_range(&p);
        assert!(
            (r - 13.3).abs() < 0.1,
            "dominant-source preset should be ~13.3 dB, got {r:.4}"
        );
        assert!(r > 6.0, "must violate Shield's 6 dB threshold");
        assert!(r > 8.0, "must violate even the looser 8 dB threshold");
    }

    #[test]
    fn source_balance_finite_under_extreme_volume_ratio() {
        // Two sources with volumes 1.0 and 1e-3 → range ≈ 60 dB.
        let p = preset_with_volumes(&[(true, 1.0), (true, 1e-3)]);
        let r = compute_source_balance_db_range(&p);
        assert!(r.is_finite());
        assert!(
            (r - 60.0).abs() < 0.5,
            "1000× ratio should be ~60 dB, got {r:.3}"
        );
    }

    #[test]
    fn render_preset_ear_signals_short_silence_is_finite() {
        let rendered = render_preset_ear_signals(&crate::preset::Preset::default(), 0.25);
        assert_eq!(rendered.sample_rate_hz, SAMPLE_RATE);
        assert_eq!(rendered.frame_count(), (SAMPLE_RATE as f32 * 0.25) as usize);
        assert!(rendered.is_finite());
    }

    #[test]
    fn render_preset_ear_signals_normal_preset_is_finite() {
        let rendered = render_preset_ear_signals(&simple_active_preset(), 0.5);
        assert_eq!(rendered.sample_rate_hz, SAMPLE_RATE);
        assert_eq!(rendered.frame_count(), (SAMPLE_RATE as f32 * 0.5) as usize);
        assert!(rendered.is_finite());
    }

    #[test]
    #[should_panic(expected = "acoustic score fusion requires acoustic scoring")]
    fn acoustic_fusion_requires_acoustic_scoring() {
        let preset = simple_active_preset();
        let goal = crate::scoring::Goal::new(crate::scoring::GoalKind::Shield);
        let config = SimulationConfig {
            duration_secs: 3.0,
            acoustic_scoring_enabled: false,
            acoustic_score_fusion_enabled: true,
            ..SimulationConfig::default()
        };

        let _ = evaluate_preset_detailed(&preset, &goal, &config);
    }

    #[test]
    #[should_panic(expected = "acoustic constraints require acoustic scoring")]
    fn acoustic_constraints_require_acoustic_scoring() {
        let preset = simple_active_preset();
        let goal = crate::scoring::Goal::new(crate::scoring::GoalKind::Shield);
        let config = SimulationConfig {
            duration_secs: 3.0,
            acoustic_scoring_enabled: false,
            acoustic_constraints_enabled: true,
            ..SimulationConfig::default()
        };
        let _ = evaluate_preset_detailed(&preset, &goal, &config);
    }

    #[test]
    #[should_panic(expected = "acoustic constraints are incompatible with acoustic score fusion")]
    fn acoustic_constraints_incompatible_with_fusion() {
        let preset = simple_active_preset();
        let goal = crate::scoring::Goal::new(crate::scoring::GoalKind::Shield);
        let config = SimulationConfig {
            duration_secs: 3.0,
            acoustic_scoring_enabled: true,
            acoustic_score_fusion_enabled: true,
            acoustic_constraints_enabled: true,
            ..SimulationConfig::default()
        };
        let _ = evaluate_preset_detailed(&preset, &goal, &config);
    }

    #[test]
    fn acoustic_constraints_with_scoring_alone_is_ok() {
        let preset = simple_active_preset();
        let goal = crate::scoring::Goal::new(crate::scoring::GoalKind::Shield);
        let config = SimulationConfig {
            duration_secs: 3.0,
            acoustic_scoring_enabled: true,
            acoustic_score_fusion_enabled: false,
            acoustic_constraints_enabled: true,
            ..SimulationConfig::default()
        };
        // Must not panic — this is the production constrained-mode config.
        let result = evaluate_preset_detailed(&preset, &goal, &config);
        // Acoustic features must be populated so comfort_violation has data.
        let acoustic = result
            .summary
            .acoustic_score
            .as_ref()
            .expect("acoustic_score must be populated when scoring is enabled");
        assert!(acoustic.features.lufs_integrated.is_some());
        assert!(acoustic.features.spectral_tilt_db_per_oct.is_some());
    }

    // ---------------------------------------------------------------
    // spectral_brightness
    // ---------------------------------------------------------------

    #[test]
    fn brightness_dark_for_low_freq_sine() {
        // 100 Hz sine → centroid ≈ 100 Hz → brightness ≈ 0.0
        let n = 48_000;
        let sr = 48_000.0;
        let audio: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 100.0 * i as f64 / sr).sin() as f32)
            .collect();
        let b = spectral_brightness(&audio, sr);
        assert!(
            b < 0.15,
            "100 Hz sine should be dark, got brightness={b:.3}"
        );
    }

    #[test]
    fn brightness_bright_for_high_freq_sine() {
        // 8000 Hz sine → centroid ≈ 8000 Hz → brightness ≈ 0.95
        let n = 48_000;
        let sr = 48_000.0;
        let audio: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 8000.0 * i as f64 / sr).sin() as f32)
            .collect();
        let b = spectral_brightness(&audio, sr);
        assert!(
            b > 0.80,
            "8 kHz sine should be bright, got brightness={b:.3}"
        );
    }

    #[test]
    fn brightness_in_zero_to_one() {
        let sr = 48_000.0;
        // Test with various signals
        for &freq in &[50.0, 500.0, 5000.0, 15000.0] {
            let n = 48_000;
            let audio: Vec<f32> = (0..n)
                .map(|i| (2.0 * PI * freq * i as f64 / sr).sin() as f32)
                .collect();
            let b = spectral_brightness(&audio, sr);
            assert!(
                b >= 0.0 && b <= 1.0,
                "Brightness should be [0,1] for {freq} Hz, got {b:.3}"
            );
        }
    }

    #[test]
    fn brightness_higher_for_higher_freq() {
        let n = 48_000;
        let sr = 48_000.0;
        let low: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 200.0 * i as f64 / sr).sin() as f32)
            .collect();
        let high: Vec<f32> = (0..n)
            .map(|i| (2.0 * PI * 5000.0 * i as f64 / sr).sin() as f32)
            .collect();
        let b_low = spectral_brightness(&low, sr);
        let b_high = spectral_brightness(&high, sr);
        assert!(
            b_high > b_low,
            "Higher freq should be brighter: {b_low:.3} vs {b_high:.3}"
        );
    }

    #[test]
    fn brightness_silence_returns_mid_range() {
        let audio = vec![0.0_f32; 48_000];
        let b = spectral_brightness(&audio, 48_000.0);
        // Default centroid 500 Hz → brightness ≈ 0.35
        assert!(
            b >= 0.0 && b <= 1.0,
            "Silence brightness should be in [0,1], got {b:.3}"
        );
    }

    // ---------------------------------------------------------------
    // SimulationConfig defaults
    // ---------------------------------------------------------------

    #[test]
    fn default_config_values() {
        let config = SimulationConfig::default();
        assert_eq!(config.duration_secs, 12.0);
        assert_eq!(config.warmup_discard_secs, DEFAULT_WARMUP_DISCARD_SECS);
        assert_eq!(config.brain_type, BrainType::Normal);
    }

    /// **Priority 18 backward-compat pin.** The new SimulationConfig
    /// fields (`jr_stochastic_sigma`, `cet_b_slow_rate`, `cet_b_slow_gain`)
    /// must default to the historical hardcoded constants, otherwise
    /// every existing call site that builds `SimulationConfig` via
    /// `..Default::default()` would silently shift behaviour.
    #[test]
    fn priority18_default_params_match_pre_p18_constants() {
        let config = SimulationConfig::default();
        assert_eq!(
            config.jr_stochastic_sigma, 15.0,
            "default JR sigma must match pre-P18 hardcoded constant"
        );
        assert_eq!(
            config.cet_b_slow_rate, 5.0,
            "default CET b_slow rate must match pre-P18 hardcoded constant"
        );
        assert_eq!(
            config.cet_b_slow_gain, 10.0,
            "default CET B_slow gain must match pre-P18 hardcoded constant"
        );
    }

    /// Changing `jr_stochastic_sigma` must change the EEG output (sanity
    /// check that the new field is wired through, not silently ignored).
    #[test]
    fn priority18_jr_sigma_change_alters_output() {
        let preset = simple_active_preset();
        let goal = crate::scoring::Goal::new(crate::scoring::GoalKind::Shield);
        let config_legacy = SimulationConfig {
            duration_secs: 3.0,
            ..SimulationConfig::default()
        };
        let config_high = SimulationConfig {
            duration_secs: 3.0,
            jr_stochastic_sigma: 200.0,
            ..SimulationConfig::default()
        };

        let r_legacy = evaluate_preset(&preset, &goal, &config_legacy);
        let r_high = evaluate_preset(&preset, &goal, &config_high);

        // High sigma should noticeably alter band powers vs default sigma=15.
        // We don't pin direction (depends on attractor structure), only that
        // some metric moved by more than measurement noise.
        let any_band_changed = (r_legacy.delta_power - r_high.delta_power).abs() > 1e-6
            || (r_legacy.theta_power - r_high.theta_power).abs() > 1e-6
            || (r_legacy.alpha_power - r_high.alpha_power).abs() > 1e-6
            || (r_legacy.beta_power - r_high.beta_power).abs() > 1e-6;
        assert!(
            any_band_changed,
            "jr_stochastic_sigma must affect band powers; got identical bands at sigma=15 and sigma=200"
        );
    }

    /// Changing `cet_b_slow_rate` must change the EEG output (sanity
    /// check that the new field is wired through, only when CET is on).
    #[test]
    fn priority18_b_slow_rate_change_alters_output_with_cet() {
        let preset = simple_active_preset();
        let goal = crate::scoring::Goal::new(crate::scoring::GoalKind::Shield);
        let config_legacy = SimulationConfig {
            duration_secs: 3.0,
            cet_enabled: true,
            ..SimulationConfig::default()
        };
        let config_retuned = SimulationConfig {
            duration_secs: 3.0,
            cet_enabled: true,
            cet_b_slow_rate: 25.0,
            cet_b_slow_gain: 18.0,
            ..SimulationConfig::default()
        };

        let r_legacy = evaluate_preset(&preset, &goal, &config_legacy);
        let r_retuned = evaluate_preset(&preset, &goal, &config_retuned);

        let any_band_changed = (r_legacy.delta_power - r_retuned.delta_power).abs() > 1e-6
            || (r_legacy.theta_power - r_retuned.theta_power).abs() > 1e-6
            || (r_legacy.alpha_power - r_retuned.alpha_power).abs() > 1e-6
            || (r_legacy.beta_power - r_retuned.beta_power).abs() > 1e-6;
        assert!(
            any_band_changed,
            "cet_b_slow_rate / cet_b_slow_gain must affect band powers when CET is enabled"
        );
    }

    /// CET-off path must ignore the new `cet_b_slow_*` parameters
    /// entirely — they only take effect when `cet_enabled = true`.
    #[test]
    fn priority18_b_slow_params_ignored_when_cet_disabled() {
        let preset = simple_active_preset();
        let goal = crate::scoring::Goal::new(crate::scoring::GoalKind::Shield);
        let config_a = SimulationConfig {
            duration_secs: 3.0,
            cet_enabled: false,
            ..SimulationConfig::default()
        };
        let config_b = SimulationConfig {
            duration_secs: 3.0,
            cet_enabled: false,
            cet_b_slow_rate: 25.0,
            cet_b_slow_gain: 18.0,
            ..SimulationConfig::default()
        };

        let r_a = evaluate_preset(&preset, &goal, &config_a);
        let r_b = evaluate_preset(&preset, &goal, &config_b);

        // CET disabled → both runs should produce bit-identical results.
        assert_eq!(
            r_a.delta_power, r_b.delta_power,
            "CET-off must ignore cet_b_slow_rate"
        );
        assert_eq!(r_a.alpha_power, r_b.alpha_power);
        assert_eq!(r_a.score, r_b.score);
    }

    /// stochastic_jr_enabled = false must force σ to 0 regardless of
    /// `jr_stochastic_sigma` value, preserving the deterministic-mode
    /// invariant (same input → bit-identical output).
    #[test]
    fn priority18_stochastic_off_overrides_jr_sigma() {
        let preset = simple_active_preset();
        let goal = crate::scoring::Goal::new(crate::scoring::GoalKind::Shield);
        let config_a = SimulationConfig {
            duration_secs: 3.0,
            stochastic_jr_enabled: false,
            jr_stochastic_sigma: 15.0,
            ..SimulationConfig::default()
        };
        let config_b = SimulationConfig {
            duration_secs: 3.0,
            stochastic_jr_enabled: false,
            jr_stochastic_sigma: 200.0, // would matter if stoch were on
            ..SimulationConfig::default()
        };

        let r_a = evaluate_preset(&preset, &goal, &config_a);
        let r_b = evaluate_preset(&preset, &goal, &config_b);

        assert_eq!(
            r_a.delta_power, r_b.delta_power,
            "stochastic_jr_enabled=false must zero σ regardless of jr_stochastic_sigma"
        );
        assert_eq!(r_a.score, r_b.score);
    }

    #[test]
    fn warmup_discard_samples_count() {
        let config = SimulationConfig::default();
        let discard = (config.warmup_discard_secs as f64 * NEURAL_SR) as usize;
        assert_eq!(discard, 2000); // 2s × 1000 Hz
    }

    #[test]
    fn validate_analysis_window_returns_effective_duration() {
        let analysis = validate_analysis_window(3.0, DEFAULT_WARMUP_DISCARD_SECS).unwrap();
        assert!((analysis - 1.0).abs() < 1e-6);
    }

    #[test]
    fn validate_analysis_window_rejects_duration_not_exceeding_warmup() {
        let err = validate_analysis_window(2.0, DEFAULT_WARMUP_DISCARD_SECS).unwrap_err();
        assert!(err.contains("must exceed warm-up discard"));
    }

    // ---------------------------------------------------------------
    // Pipeline data flow: signal lengths
    // ---------------------------------------------------------------

    #[test]
    fn decimation_then_trim_length() {
        // 12 seconds at 48 kHz = 576000 samples
        // Decimated by 48 = 12000 samples at 1 kHz
        // Discard 2000 (2s warmup) = 10000 samples
        let n = 576_000;
        let signal = vec![0.5; n];
        let dec = decimate(&signal, DECIMATION_FACTOR);
        assert_eq!(dec.len(), 12_000);

        let discard = 2000_usize;
        let trimmed = &dec[discard..];
        assert_eq!(trimmed.len(), 10_000);
    }

    // ---------------------------------------------------------------
    // CET Priority 13a precheck — AC/DC ratio of band 0 with 5 Hz NeuralLfo.
    // Decision gate documented in update_model.md Priority 13a:
    //   AC fraction ≥ 0.30 → proceed with full CET plan (13a → 13b → 13c)
    //   AC fraction 0.15–0.30 → implement 13b (slow GABA_B) first, then retry
    //   AC fraction < 0.15 → JR-input-coupling is the bottleneck, abort 13a
    // ---------------------------------------------------------------

    /// Mirrors the front half of `evaluate_preset()` (engine → gammatone → global
    /// normalize → decimate → trim) and returns the decimated band 0 signal so a
    /// test can measure its AC/DC composition without instrumenting production
    /// code. Kept under cfg(test) to avoid leaking debug helpers into the binary.
    fn precheck_band0_signal(preset: &crate::preset::Preset, duration_secs: f32) -> Vec<f64> {
        use crate::auditory::GammatoneFilterbank;
        use noise_generator_core::NoiseEngine;
        use std::sync::Arc;

        let num_frames = (SAMPLE_RATE as f32 * duration_secs) as u32;
        let sr = SAMPLE_RATE as f64;

        let engine: Arc<NoiseEngine> = NoiseEngine::new(SAMPLE_RATE, 0.8);
        preset.apply_to_engine(&engine);

        // 1s engine warmup (matches evaluate_preset)
        let warmup_frames = (SAMPLE_RATE as f32 * 1.0) as u32;
        let _ = engine.render_audio(warmup_frames);

        let audio = engine.render_audio(num_frames);
        let (left, _right) = deinterleave(&audio);

        let mut filterbank_l = GammatoneFilterbank::new(sr);
        let bands_l = filterbank_l.process_to_band_groups(&left);

        // Global normalisation across all bands (matches evaluate_preset)
        let global_max_l = (0..4)
            .map(|b| bands_l.signals[b].iter().cloned().fold(0.0_f64, f64::max))
            .fold(0.0_f64, f64::max);
        let norm_l = if global_max_l > 1e-10 {
            1.0 / global_max_l
        } else {
            1.0
        };
        let band0_norm: Vec<f64> = bands_l.signals[0].iter().map(|x| x * norm_l).collect();

        // Decimate + 2s warmup discard (matches evaluate_preset's trim closure)
        let dec = decimate(&band0_norm, DECIMATION_FACTOR);
        let discard = (2.0_f64 * NEURAL_SR) as usize;
        let skip = discard.min(dec.len());
        dec[skip..].to_vec()
    }

    /// Build a synthetic preset: one active object emitting pink noise with a
    /// 5 Hz NeuralLfo at depth 0.9. This is the canonical "slow envelope on
    /// broadband noise" stimulus from the CET literature (Doelling 2014).
    fn synthetic_5hz_pink_preset() -> crate::preset::Preset {
        use crate::preset::{ModConfig, ObjectConfig, Preset};
        let mut p = Preset::default();
        p.master_gain = 0.8;
        p.spatial_mode = 1;
        p.source_count = 1;
        p.anchor_color = 1; // pink
        p.anchor_volume = 0.0;
        p.environment = 0;
        p.objects[0] = ObjectConfig {
            active: true,
            color: 1, // pink
            position_space: 0,
            x: 0.0,
            y: 0.0,
            z: 1.5,
            volume: 0.9,
            reverb_send: 0.05,
            spread: 0.0,
            bass_mod: ModConfig {
                kind: 4,
                param_a: 5.0,
                param_b: 0.9,
                param_c: 0.0,
            },
            satellite_mod: ModConfig {
                kind: 4,
                param_a: 5.0,
                param_b: 0.9,
                param_c: 0.0,
            },
            movement: Default::default(),
            tint_freq: 0.0,
            tint_db: 0.0,
            source_kind: 0,
            tone_freq: 200.0,
            tone_amplitude: 0.0,
        };
        p
    }

    fn ac_dc_stats(signal: &[f64]) -> (f64, f64, f64, f64) {
        let n = signal.len() as f64;
        let mean = signal.iter().sum::<f64>() / n;
        let total_power = signal.iter().map(|x| x * x).sum::<f64>() / n;
        let ac_power = signal.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
        let ac_fraction = if total_power > 1e-30 {
            ac_power / total_power
        } else {
            0.0
        };
        (mean, total_power, ac_power, ac_fraction)
    }

    #[test]
    fn cet_precheck_band0_ac_dc_5hz_neural_lfo() {
        let preset = synthetic_5hz_pink_preset();
        let band0 = precheck_band0_signal(&preset, 10.0);
        assert!(!band0.is_empty(), "decimated band 0 should be non-empty");
        let (mean, total_power, ac_power, ac_fraction) = ac_dc_stats(&band0);
        // Print so cargo test --nocapture surfaces the precheck verdict.
        // Decision gate per update_model.md Priority 13a.
        eprintln!("=== CET 13a precheck ===");
        eprintln!("  duration: 10s, preset: pink + 5 Hz NeuralLfo (depth 0.9)");
        eprintln!("  band 0 length:    {}", band0.len());
        eprintln!("  band 0 mean (DC): {mean:.6}");
        eprintln!("  total power:      {total_power:.6}");
        eprintln!("  AC power:         {ac_power:.6}");
        eprintln!("  AC fraction:      {ac_fraction:.4}");
        let verdict = if ac_fraction >= 0.30 {
            "GREEN — proceed with full CET plan"
        } else if ac_fraction >= 0.15 {
            "YELLOW — implement 13b (slow GABA_B) first, then retry 13a"
        } else {
            "RED — JR input coupling is the bottleneck, revisit Priority 1b finding"
        };
        eprintln!("  verdict:          {verdict}");
        // The test never fails on the verdict — its job is to MEASURE.
        // It only fails if the pipeline produces nonsense.
        assert!(ac_fraction.is_finite(), "AC fraction must be finite");
        assert!(
            total_power.is_finite() && total_power >= 0.0,
            "total power finite & nonneg"
        );
    }
}
