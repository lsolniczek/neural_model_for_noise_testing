/// Simulation pipeline: Engine → Auditory → Neural → Score.
///
/// Wires together the noise engine, cochlear filterbank, and neural models
/// into a single evaluation function that the optimizer calls.

use crate::auditory::{GammatoneFilterbank, AssrTransfer, ThalamicGate, PhysiologicalThalamicGate, ButterworthCrossover};
use crate::brain_type::BrainType;
use crate::movement::MovementController;
use crate::neural::{FhnModel, FastInhibParams, PerformanceVector, simulate_bilateral};
use crate::preset::Preset;
use crate::scoring::Goal;
use noise_generator_core::NoiseEngine;

use rustfft::{num_complex::Complex, FftPlanner};

pub(crate) const SAMPLE_RATE: u32 = 48_000;
/// Decimation factor: 48 kHz → 1 kHz for neural models.
pub(crate) const DECIMATION_FACTOR: usize = 48;
/// Neural model sample rate after decimation.
pub(crate) const NEURAL_SR: f64 = SAMPLE_RATE as f64 / DECIMATION_FACTOR as f64;

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
}

impl Default for SimulationConfig {
    fn default() -> Self {
        SimulationConfig {
            duration_secs: 12.0,
            warmup_discard_secs: 2.0,
            brain_type: BrainType::Normal,
            assr_enabled: true,
            thalamic_gate_enabled: true,
            habituation_enabled: true,
            stochastic_jr_enabled: true,
            cet_enabled: true,
            physiological_thalamic_gate_enabled: false,
        }
    }
}

pub struct SimulationResult {
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

/// Evaluate a preset against a goal.
///
/// This is the core function the optimizer calls for each candidate.
pub fn evaluate_preset(preset: &Preset, goal: &Goal, config: &SimulationConfig) -> SimulationResult {
    let num_frames = (SAMPLE_RATE as f32 * config.duration_secs) as u32;
    let sr = SAMPLE_RATE as f64;

    // 1. Create engine and apply preset
    let engine = NoiseEngine::new(SAMPLE_RATE, 0.8);
    preset.apply_to_engine(&engine);

    // 2. Set up movement controller
    let mut movement = MovementController::from_preset(preset);

    // 3. Render audio with movement updates.
    //    When objects move, we render in small chunks (~50ms) and update
    //    positions between chunks so the HRTF pipeline reflects motion.
    //    For static presets we render in one shot for efficiency.
    let warmup_frames = (SAMPLE_RATE as f32 * 1.0) as u32;
    let chunk_frames = (SAMPLE_RATE as f32 * 0.05) as u32; // 50ms chunks

    if movement.has_movement() {
        // Warmup with movement ticking
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

    // 3. Deinterleave to L/R
    let (left, right) = deinterleave(&audio);

    // 4. Cochlear model: tonotopic band-grouped processing
    //    Groups 32 gammatone channels into 4 frequency bands, preserving
    //    spectral energy distribution for the cortical model.
    let mut filterbank_l = GammatoneFilterbank::new(sr);
    let mut filterbank_r = GammatoneFilterbank::new(sr);

    let bands_l = filterbank_l.process_to_band_groups(&left);
    let bands_r = filterbank_r.process_to_band_groups(&right);

    // 5. Normalise each ear's band signals to [0, 1] using GLOBAL max.
    //    Per Patterson et al. (1992) and Glasberg & Moore (2002), inter-band
    //    energy ratios carry critical spectral information. Global normalization
    //    preserves these ratios: Brown noise keeps dominant low-band energy,
    //    White noise keeps flat distribution across bands.
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

    // Find global max across ALL bands for each ear
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
    }

    // Average energy fractions for display (the bilateral model uses per-ear fractions)
    let mut energy_fractions = [0.0_f64; 4];
    for b in 0..4 {
        energy_fractions[b] = (bands_l.energy_fractions[b] + bands_r.energy_fractions[b]) * 0.5;
    }
    let ef_sum: f64 = energy_fractions.iter().sum();
    if ef_sum > 1e-30 {
        for ef in &mut energy_fractions {
            *ef /= ef_sum;
        }
    }

    // 5b. Spectral brightness from audio FFT (psychoacoustic complement)
    let brightness = spectral_brightness(&left, sr);

    // 5c. Decimate band signals from 48 kHz → 1 kHz for neural models.
    //     Neural models operate at 0.5–50 Hz; feeding 48 kHz wastes ~98% of
    //     compute on samples that carry no additional information.
    // Discard initial warm-up samples from decimated signals so the neural
    // models only analyse the settled portion of the auditory response.
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
    //     When CET is enabled, split each band into a SLOW (≤10 Hz) path and
    //     a FAST (>10 Hz) path before ASSR. The slow path bypasses the ASSR
    //     attenuation in the next step so 1–8 Hz envelope modulations reach
    //     JR undamped. Both paths recombine into the band signal that drives
    //     JR. Refs: Doelling et al. (2014), Ghitza (2011), Ding & Simon (2014).
    //     Stored separately so 5e (ASSR) can scale only the fast path.
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
            // Replace the band with the FAST path so the ASSR block below
            // attenuates only the carrier modulation. Stash the slow path.
            left_bands_dec[b] = fl;
            right_bands_dec[b] = fr;
            slow_l[b] = sl;
            slow_r[b] = sr_l;
        }
        cet_slow_left = Some(slow_l);
        cet_slow_right = Some(slow_r);
    }

    // 5e. (Optional) ASSR: attenuate modulation (AC) in band signals.
    //     Per Picton et al. (2003), ASSR models frequency-dependent transmission
    //     of amplitude modulation through the auditory pathway.
    //     IMPORTANT: Only scale the AC component, not the DC mean.
    //     DC (mean drive level) is the thalamic gate's domain — ASSR should not
    //     shift the cortical operating point, only reduce modulation strength.
    //     When CET is enabled, this only acts on the FAST path (slow path was
    //     extracted in 5d above).
    if config.assr_enabled {
        let assr = AssrTransfer::new();
        let assr_mod = assr.compute_input_scale_modifier(preset);
        if assr_mod < 1.0 - 1e-10 {
            for bands in [&mut left_bands_dec, &mut right_bands_dec] {
                for band in bands.iter_mut() {
                    let n = band.len();
                    if n == 0 { continue; }
                    let mean = band.iter().sum::<f64>() / n as f64;
                    for sample in band.iter_mut() {
                        let ac = *sample - mean;
                        *sample = mean + ac * assr_mod;
                    }
                }
            }
        }
    }

    // 5f. (CET only) Recombine slow path with the (now ASSR-attenuated) fast
    //     path to produce the final band signal that drives JR. The slow
    //     envelope reaches JR with full amplitude — exactly the architectural
    //     fix the precheck identified.
    //
    // Also build the *envelope reference* used by 13c envelope-phase PLV:
    // an energy-weighted average of the slow paths across all 4 bands and
    // both ears. This is the cortex's slow drive — what JR is supposed to
    // track. We'll bandpass it to 2-9 Hz inside compute_envelope_plv.
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
                if w < 1e-10 { continue; }
                for i in 0..n_env {
                    env[i] += w * 0.5 * (slow_l[b][i] + slow_r[b][i]);
                }
            }
            Some(env)
        } else {
            None
        };

    // 5e. (Optional) Thalamic gate — modulates cortical operating point.
    //     Dark, reverberant, gentle presets → low arousal → lower input_offset
    //     → JR model shifts toward bifurcation → theta/delta possible.
    //     Per Steriade et al. (1993): thalamic burst mode is frequency-selective.
    //     Low bands (delta/theta) shift fully toward bifurcation.
    //     High bands (beta/gamma) stay in tonic mode for fast rhythms.
    //
    // Two implementations available:
    //   - heuristic ThalamicGate (linear arousal → shift, default)
    //   - PhysiologicalThalamicGate (Priority 9: HH TC cell with T-current
    //     and K⁺ leak as the wake↔sleep knob — Bazhenov 2002 / Paul 2016 /
    //     Destexhe 1996). Sigmoidal shape derived from ion-channel dynamics.
    //
    // The physiological gate takes precedence when both flags are set.
    let thalamic_band_shifts = if config.physiological_thalamic_gate_enabled {
        let arousal = PhysiologicalThalamicGate::compute_arousal(preset, brightness);
        let gate = PhysiologicalThalamicGate::new(arousal);
        gate.band_offset_shifts()
    } else if config.thalamic_gate_enabled {
        let arousal = ThalamicGate::compute_arousal(preset, brightness);
        let gate = ThalamicGate::new(arousal);
        gate.band_offset_shifts()
    } else {
        [0.0; 4]
    };

    // 6. Bilateral cortical model: 2×4 parallel Jansen-Rit models
    //    Left hemisphere (fast, α/β) ← mainly right ear (contralateral)
    //    Right hemisphere (slow, δ/θ) ← mainly left ear (contralateral)
    //    Coupled through corpus callosum with ~10ms delay, ~10% strength.
    let neural_params = config.brain_type.params();
    let mut bilateral = config.brain_type.bilateral_params();
    // Apply thalamic gate: per-band offset shifts toward bifurcation at low arousal.
    for b in 0..4 {
        if thalamic_band_shifts[b].abs() > 1e-10 {
            bilateral.left.band_offsets[b] += thalamic_band_shifts[b];
            bilateral.right.band_offsets[b] += thalamic_band_shifts[b];
        }
    }

    let fast_inhib = FastInhibParams {
        g_fast_gain: neural_params.jansen_rit.g_fast_gain,
        g_fast_rate: neural_params.jansen_rit.g_fast_rate,
        c5: neural_params.jansen_rit.c5,
        c6: neural_params.jansen_rit.c6,
        c7: neural_params.jansen_rit.c7,
    };

    // input_scale is no longer modified by ASSR — ASSR operates on signal AC only.
    //
    // CET 13b — Slow GABA_B (CET-relevant slow inhibitory loop, τ ≈ 200 ms).
    // Per Moran & Friston (2011) canonical microcircuit and Ghitza (2011)
    // cascaded oscillator: CET requires a slow inhibitory feedback that
    // canonical Wendling-JR lacks. When `cet_enabled = true`, we enable the
    // additive parallel slow population in JR with these parameters.
    // Default 0.0 → bitwise-identical to pre-CET model.
    let (b_slow_gain, b_slow_rate, c_slow) = if config.cet_enabled {
        (10.0, 5.0, 30.0)
    } else {
        (0.0, 0.0, 0.0)
    };

    let bi_result = simulate_bilateral(
        &left_bands_dec,
        &right_bands_dec,
        &bands_l.energy_fractions,
        &bands_r.energy_fractions,
        &bilateral,
        neural_params.jansen_rit.c,
        neural_params.jansen_rit.input_scale,
        NEURAL_SR,
        &fast_inhib,
        neural_params.jansen_rit.v0,
        if config.habituation_enabled { 0.0003 } else { 0.0 },
        if config.habituation_enabled { 0.0001 } else { 0.0 },
        if config.stochastic_jr_enabled { 15.0 } else { 0.0 },
        b_slow_gain,
        b_slow_rate,
        c_slow,
    );

    let jr_result = &bi_result.combined;

    // 7. FHN: single-neuron driven by combined bilateral EEG oscillations.
    //    Uses the same decimated sample rate as the JR output.
    let fhn = FhnModel::with_params(
        NEURAL_SR,
        neural_params.fhn.a,
        neural_params.fhn.b,
        neural_params.fhn.epsilon,
        neural_params.fhn.time_scale,
    );

    // Scale EEG for FHN input using percentile-based normalization.
    // Per FitzHugh (1961) and Izhikevich (2003), neuron firing rate depends
    // monotonically on input current amplitude. Max-normalization destroys
    // this by collapsing all amplitudes to [-1,1]. Percentile scaling
    // preserves relative amplitude: strong EEG → higher current → more spikes.
    let fhn_input: Vec<f64> = {
        let mut abs_values: Vec<f64> = jr_result.eeg.iter().map(|x| x.abs()).collect();
        abs_values.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
        let p95_idx = (abs_values.len() as f64 * 0.95) as usize;
        let p95 = abs_values[p95_idx.min(abs_values.len() - 1)];
        let scale = if p95 > 1e-10 { 1.0 / p95 } else { 1.0 };
        jr_result.eeg.iter().map(|x| (x * scale).clamp(-3.0, 3.0)).collect()
    };
    let fhn_result = fhn.simulate(&fhn_input, neural_params.fhn.input_scale);

    // 8. Performance Vector — diagnostic metrics for preset evaluation.
    //    Extract the STRONGEST NeuralLFO frequency from the preset.
    //    "Strongest" = highest (depth × volume) product, which is the actual
    //    entrainment driver — not just the first one found.
    //    Recognizes both NeuralLfo (kind=4) and Isochronic (kind=5) as
    //    entrainment modulators. Isochronic tones produce stronger cortical
    //    FFR due to sharp transients (Chaieb et al. 2015).
    let target_lfo_freq = preset.objects.iter()
        .filter(|obj| obj.active)
        .flat_map(|obj| {
            let vol = obj.volume as f64;
            let mut lfos = Vec::new();
            // NeuralLfo (kind=4) and Isochronic (kind=5) both drive entrainment
            for modcfg in [&obj.bass_mod, &obj.satellite_mod] {
                if (modcfg.kind == 4 || modcfg.kind == 5) && modcfg.param_a > 0.5 {
                    lfos.push((modcfg.param_a as f64, modcfg.param_b as f64 * vol));
                }
            }
            lfos
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()) // strongest by depth*volume
        .map(|(freq, _strength)| freq);

    // Detrend EEG for spectral analysis
    let eeg_mean = jr_result.eeg.iter().sum::<f64>() / jr_result.eeg.len() as f64;
    let eeg_detrended: Vec<f64> = jr_result.eeg.iter().map(|x| x - eeg_mean).collect();

    let performance = PerformanceVector::compute_with_envelope(
        &eeg_detrended,
        &jr_result.fast_inhib_trace,
        NEURAL_SR,
        target_lfo_freq,
        cet_envelope_ref.as_deref(),
    );

    // 9. Score: neural model + asymmetry penalty + carrier PLV bonus + envelope PLV bonus.
    let score = goal.evaluate_full(
        &fhn_result,
        jr_result,
        bi_result.alpha_asymmetry,
        performance.plv,
        performance.envelope_plv,
    );
    let norm_bands = jr_result.band_powers.normalized();

    SimulationResult {
        score,
        fhn_firing_rate: fhn_result.firing_rate,
        fhn_isi_cv: fhn_result.isi_cv,
        dominant_freq: jr_result.dominant_freq,
        delta_power: norm_bands.delta,
        theta_power: norm_bands.theta,
        alpha_power: norm_bands.alpha,
        beta_power: norm_bands.beta,
        gamma_power: norm_bands.gamma,
        brightness,
        band_energy_fractions: energy_fractions,
        left_dominant_freq: bi_result.left_dominant_freq,
        right_dominant_freq: bi_result.right_dominant_freq,
        alpha_asymmetry: bi_result.alpha_asymmetry,
        performance,
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

    // ---------------------------------------------------------------
    // decimate
    // ---------------------------------------------------------------

    #[test]
    fn decimate_constant_signal_unchanged() {
        let signal = vec![3.0; 480]; // 480 samples / 48 = 10 output
        let dec = decimate(&signal, 48);
        assert_eq!(dec.len(), 10);
        for &v in &dec {
            assert!((v - 3.0).abs() < 1e-12, "Constant signal should decimate to same value");
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
        assert!(b < 0.15, "100 Hz sine should be dark, got brightness={b:.3}");
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
        assert!(b > 0.80, "8 kHz sine should be bright, got brightness={b:.3}");
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
        assert_eq!(config.warmup_discard_secs, 2.0);
        assert_eq!(config.brain_type, BrainType::Normal);
    }

    #[test]
    fn warmup_discard_samples_count() {
        let config = SimulationConfig::default();
        let discard = (config.warmup_discard_secs as f64 * NEURAL_SR) as usize;
        assert_eq!(discard, 2000); // 2s × 1000 Hz
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
        let norm_l = if global_max_l > 1e-10 { 1.0 / global_max_l } else { 1.0 };
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
            x: 0.0,
            y: 0.0,
            z: 1.5,
            volume: 0.9,
            reverb_send: 0.05,
            bass_mod: ModConfig { kind: 4, param_a: 5.0, param_b: 0.9, param_c: 0.0 },
            satellite_mod: ModConfig { kind: 4, param_a: 5.0, param_b: 0.9, param_c: 0.0 },
            movement: Default::default(),
            tint_freq: 0.0, tint_db: 0.0, source_kind: 0, tone_freq: 200.0, tone_amplitude: 0.0 };
        p
    }

    fn ac_dc_stats(signal: &[f64]) -> (f64, f64, f64, f64) {
        let n = signal.len() as f64;
        let mean = signal.iter().sum::<f64>() / n;
        let total_power = signal.iter().map(|x| x * x).sum::<f64>() / n;
        let ac_power = signal.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
        let ac_fraction = if total_power > 1e-30 { ac_power / total_power } else { 0.0 };
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
        assert!(total_power.is_finite() && total_power >= 0.0, "total power finite & nonneg");
    }
}
