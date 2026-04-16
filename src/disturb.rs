/// Disturbance Test — measures neural resilience after an acoustic spike.
///
/// Runs the standard auditory→neural pipeline, injects a broadband spike
/// into the decimated tonotopic signals at a specified time, then tracks
/// entrainment ratio and dominant frequency through sliding-window analysis
/// to measure perturbation impact and recovery dynamics.

use crate::auditory::GammatoneFilterbank;
use crate::brain_type::BrainType;
use crate::movement::MovementController;
use crate::neural::{FastInhibParams, simulate_bilateral, BilateralResult};
use crate::pipeline::{
    SAMPLE_RATE, DECIMATION_FACTOR, NEURAL_SR,
    decimate, deinterleave, spectral_brightness, SimulationConfig,
};
use crate::preset::Preset;
use noise_generator_core::NoiseEngine;
use rustfft::{num_complex::Complex, FftPlanner};

// ── Sliding-window metrics ──────────────────────────────────────────────────

/// Metrics computed at each sliding window position.
#[derive(Debug, Clone)]
pub struct WindowMetrics {
    /// Centre time of the window (seconds from analysis start).
    pub time_s: f64,
    /// Entrainment ratio at target frequency (None if no target).
    pub entrainment_ratio: Option<f64>,
    /// Dominant frequency in this window (Hz).
    pub dominant_freq: f64,
    /// Spectral centroid in this window (Hz).
    pub spectral_centroid: f64,
    /// Normalized EEG band powers [delta, theta, alpha, beta, gamma].
    /// Per Pfurtscheller & Lopes da Silva (1999): band power fractions
    /// are the foundation for ERD/ERS-based resilience metrics.
    pub band_powers: [f64; 5],
}

/// Summary of the disturbance test.
pub struct DisturbResult {
    /// Per-window metrics across the entire simulation.
    pub windows: Vec<WindowMetrics>,
    /// Baseline mean entrainment ratio (pre-spike).
    pub baseline_entrainment: Option<f64>,
    /// Baseline mean dominant frequency (pre-spike).
    pub baseline_dominant_freq: f64,
    /// Baseline mean spectral centroid (pre-spike).
    pub baseline_centroid: f64,
    /// Baseline mean band powers [delta, theta, alpha, beta, gamma].
    pub baseline_band_powers: [f64; 5],
    /// Minimum entrainment ratio during/after spike.
    pub nadir_entrainment: Option<f64>,
    /// Time of nadir (seconds).
    pub nadir_time: f64,
    /// Peak frequency deviation from baseline (Hz).
    pub peak_freq_deviation: f64,
    /// Time to 50% entrainment recovery (seconds after spike, None if not recovered).
    pub recovery_50_ms: Option<f64>,
    /// Time to 90% entrainment recovery (seconds after spike, None if not recovered).
    pub recovery_90_ms: Option<f64>,

    // ── Spectral resilience metrics (Priority 15) ──────────────────
    //
    // These work for ALL preset types including binaural beats and
    // static noise, unlike the entrainment-based metrics above.

    /// Band Power Preservation Ratio (BPPR) ∈ [0, 1].
    /// Per Pfurtscheller & Lopes da Silva (1999): worst-case fractional
    /// preservation of the dominant band power after the spike.
    /// 1.0 = perfect preservation, 0.0 = complete desynchronization.
    pub bppr: f64,
    /// Spectral Recovery Time at 50% (ms after spike).
    /// How fast band power deviation drops to 50% of its nadir value.
    pub spectral_recovery_50_ms: Option<f64>,
    /// Spectral Recovery Time at 90% (ms after spike).
    pub spectral_recovery_90_ms: Option<f64>,
    /// Spectral Centroid Deviation Integral (Hz).
    /// Mean absolute centroid deviation from baseline across post-spike windows.
    /// Lower is better. 0.0 = no spectral displacement.
    pub scdi_hz: f64,
    /// Composite spectral resilience score ∈ [0, 1].
    /// 0.40×BPPR + 0.30×(1-norm_SRT) + 0.30×(1-norm_SCDI).
    /// Works for all preset types including binaural beats.
    pub spectral_resilience: f64,

    /// Combined bilateral result for full-trace diagnostics.
    pub bilateral: BilateralResult,
    /// Brightness from audio.
    pub brightness: f64,
    /// Target LFO frequency (if any).
    pub target_freq: Option<f64>,
}

/// Configuration for the disturbance test.
pub struct DisturbConfig {
    pub spike_time_s: f64,
    pub spike_duration_s: f64,
    pub spike_gain: f64,
    pub brain_type: BrainType,
    pub duration_secs: f32,
    pub warmup_discard_secs: f32,
    pub window_s: f64,
    pub hop_s: f64,
}

impl Default for DisturbConfig {
    fn default() -> Self {
        DisturbConfig {
            spike_time_s: 4.0,
            spike_duration_s: 0.05,
            spike_gain: 0.5,
            brain_type: BrainType::Normal,
            duration_secs: 15.0,
            warmup_discard_secs: 2.0,
            window_s: 0.5,
            hop_s: 0.05,
        }
    }
}

// ── Auditory pipeline (shared with evaluate) ────────────────────────────────

struct AuditoryOutput {
    left_bands_dec: [Vec<f64>; 4],
    right_bands_dec: [Vec<f64>; 4],
    left_energy: [f64; 4],
    right_energy: [f64; 4],
    brightness: f64,
    target_lfo_freq: Option<f64>,
}

/// Run steps 1-5 of the pipeline: Engine → Audio → Gammatone → Normalize → Decimate.
fn run_auditory_pipeline(preset: &Preset, config: &DisturbConfig) -> AuditoryOutput {
    let num_frames = (SAMPLE_RATE as f32 * config.duration_secs) as u32;
    let sr = SAMPLE_RATE as f64;

    // 1. Create engine and apply preset
    let engine = NoiseEngine::new(SAMPLE_RATE, 0.8);
    preset.apply_to_engine(&engine);

    // 2. Movement
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

    // 3. Deinterleave
    let (left, right) = deinterleave(&audio);

    // 4. Gammatone filterbank
    let mut filterbank_l = GammatoneFilterbank::new(sr);
    let mut filterbank_r = GammatoneFilterbank::new(sr);
    let bands_l = filterbank_l.process_to_band_groups(&left);
    let bands_r = filterbank_r.process_to_band_groups(&right);

    // 5. Normalise bands
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

    for b in 0..4 {
        let max_l = bands_l.signals[b].iter().cloned().fold(0.0_f64, f64::max);
        let norm_l = if max_l > 1e-10 { 1.0 / max_l } else { 1.0 };
        left_bands[b] = bands_l.signals[b].iter().map(|x| x * norm_l).collect();

        let max_r = bands_r.signals[b].iter().cloned().fold(0.0_f64, f64::max);
        let norm_r = if max_r > 1e-10 { 1.0 / max_r } else { 1.0 };
        right_bands[b] = bands_r.signals[b].iter().map(|x| x * norm_r).collect();
    }

    let brightness = spectral_brightness(&left, sr);

    // Decimate
    let discard_samples = (config.warmup_discard_secs as f64 * NEURAL_SR) as usize;
    let trim = |signal: &[f64]| -> Vec<f64> {
        let dec = decimate(signal, DECIMATION_FACTOR);
        let skip = discard_samples.min(dec.len());
        dec[skip..].to_vec()
    };

    let left_bands_dec = [
        trim(&left_bands[0]),
        trim(&left_bands[1]),
        trim(&left_bands[2]),
        trim(&left_bands[3]),
    ];
    let right_bands_dec = [
        trim(&right_bands[0]),
        trim(&right_bands[1]),
        trim(&right_bands[2]),
        trim(&right_bands[3]),
    ];

    // Extract target LFO frequency from preset.
    // Scans for NeuralLfo (kind=4), Isochronic (kind=5) — both drive entrainment.
    // Bug fix (Priority 15): previously only detected kind==4, missing isochronic.
    let target_lfo_freq = preset.objects.iter()
        .filter(|obj| obj.active)
        .flat_map(|obj| {
            let vol = obj.volume as f64;
            let mut lfos = Vec::new();
            for modcfg in [&obj.bass_mod, &obj.satellite_mod] {
                if (modcfg.kind == 4 || modcfg.kind == 5) && modcfg.param_a > 0.5 {
                    lfos.push((modcfg.param_a as f64, modcfg.param_b as f64 * vol));
                }
            }
            lfos
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(freq, _)| freq);

    AuditoryOutput {
        left_bands_dec,
        right_bands_dec,
        left_energy: bands_l.energy_fractions,
        right_energy: bands_r.energy_fractions,
        brightness,
        target_lfo_freq,
    }
}

// ── Spike injection ─────────────────────────────────────────────────────────

/// Inject a broadband white noise spike into all 4 tonotopic bands.
///
/// Uses a cosine onset/offset ramp (5ms) to avoid discontinuity artifacts.
fn inject_spike(
    bands: &mut [Vec<f64>; 4],
    spike_time_s: f64,
    spike_duration_s: f64,
    spike_gain: f64,
    sample_rate: f64,
    seed: u64,
) {
    let start = (spike_time_s * sample_rate) as usize;
    let duration = (spike_duration_s * sample_rate) as usize;
    let ramp_samples = (0.005 * sample_rate) as usize; // 5ms cosine ramp

    if bands[0].is_empty() || start + duration > bands[0].len() {
        return;
    }

    // Simple deterministic PRNG (xorshift64) for reproducibility
    let mut state = seed;
    let mut next_noise = || -> f64 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        // Map to [-1, 1]
        (state as f64 / u64::MAX as f64) * 2.0 - 1.0
    };

    for b in 0..4 {
        for i in 0..duration {
            let sample_idx = start + i;
            // Cosine ramp envelope
            let env = if i < ramp_samples {
                0.5 * (1.0 - (std::f64::consts::PI * i as f64 / ramp_samples as f64).cos())
            } else if i >= duration - ramp_samples {
                let tail = duration - 1 - i;
                0.5 * (1.0 - (std::f64::consts::PI * tail as f64 / ramp_samples as f64).cos())
            } else {
                1.0
            };

            let noise = next_noise();
            bands[b][sample_idx] += spike_gain * env * noise;
        }
    }
}

// ── Sliding-window spectral analysis ────────────────────────────────────────

/// Compute entrainment ratio, dominant freq, and centroid in a short window.
fn analyze_window(
    eeg_window: &[f64],
    sample_rate: f64,
    target_freq: Option<f64>,
    fft: &std::sync::Arc<dyn rustfft::Fft<f64>>,
    fft_len: usize,
) -> WindowMetrics {
    // Apply Hann window
    let n = eeg_window.len();
    let mut buffer: Vec<Complex<f64>> = (0..fft_len)
        .map(|i| {
            if i < n {
                let hann = 0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos());
                Complex::new(eeg_window[i] * hann, 0.0)
            } else {
                Complex::new(0.0, 0.0)
            }
        })
        .collect();

    fft.process(&mut buffer);

    let freq_res = sample_rate / fft_len as f64;
    let nyquist_bin = fft_len / 2;

    // Physiological range: 1–50 Hz
    let min_bin = (1.0 / freq_res).ceil() as usize;
    let max_bin = (50.0 / freq_res).ceil().min(nyquist_bin as f64) as usize;

    let mut total_power = 0.0_f64;
    let mut weighted_sum = 0.0_f64;
    let mut peak_power = 0.0_f64;
    let mut peak_freq = 0.0_f64;
    let mut target_power = 0.0_f64;

    // Per-band power accumulators (Priority 15a)
    // Frequency ranges match JansenRitModel::compute_band_powers()
    let mut band_power = [0.0_f64; 5]; // delta, theta, alpha, beta, gamma

    let (target_lo, target_hi) = if let Some(tf) = target_freq {
        let lo = ((tf - 1.0).max(0.5) / freq_res).floor() as usize;
        let hi = ((tf + 1.0) / freq_res).ceil().min(nyquist_bin as f64) as usize;
        (lo, hi)
    } else {
        (0, 0)
    };

    for bin in min_bin..max_bin {
        let freq = bin as f64 * freq_res;
        let power = buffer[bin].norm_sqr();
        total_power += power;
        weighted_sum += freq * power;

        if power > peak_power {
            peak_power = power;
            peak_freq = freq;
        }

        if target_freq.is_some() && bin >= target_lo && bin <= target_hi {
            target_power += power;
        }

        // Accumulate per-band power
        if freq < 4.0 { band_power[0] += power; }       // delta 0.5-4
        else if freq < 8.0 { band_power[1] += power; }   // theta 4-8
        else if freq < 13.0 { band_power[2] += power; }  // alpha 8-13
        else if freq < 30.0 { band_power[3] += power; }  // beta 13-30
        else { band_power[4] += power; }                  // gamma 30-50
    }

    let spectral_centroid = if total_power > 1e-30 {
        weighted_sum / total_power
    } else {
        10.0
    };

    let entrainment_ratio = if target_freq.is_some() && total_power > 1e-30 {
        Some(target_power / total_power)
    } else {
        None
    };

    // Normalize band powers to fractions summing to 1.0
    let bp_sum: f64 = band_power.iter().sum();
    if bp_sum > 1e-30 {
        for p in &mut band_power { *p /= bp_sum; }
    } else {
        band_power = [0.2; 5]; // uniform fallback
    }

    WindowMetrics {
        time_s: 0.0, // filled by caller
        entrainment_ratio,
        dominant_freq: peak_freq,
        spectral_centroid,
        band_powers: band_power,
    }
}

/// Run sliding-window analysis over the EEG signal.
fn sliding_window_analysis(
    eeg: &[f64],
    sample_rate: f64,
    window_s: f64,
    hop_s: f64,
    target_freq: Option<f64>,
) -> Vec<WindowMetrics> {
    let window_samples = (window_s * sample_rate) as usize;
    let hop_samples = (hop_s * sample_rate) as usize;
    let fft_len = window_samples.next_power_of_two();

    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);

    let mut results = Vec::new();
    let mut pos = 0_usize;

    while pos + window_samples <= eeg.len() {
        let window = &eeg[pos..pos + window_samples];
        // Detrend window
        let mean = window.iter().sum::<f64>() / window_samples as f64;
        let detrended: Vec<f64> = window.iter().map(|x| x - mean).collect();

        let mut metrics = analyze_window(&detrended, sample_rate, target_freq, &fft, fft_len);
        metrics.time_s = (pos as f64 + window_samples as f64 * 0.5) / sample_rate;
        results.push(metrics);
        pos += hop_samples;
    }

    results
}

// ── Main disturbance test runner ────────────────────────────────────────────

/// Run the disturbance test and return full results.
pub fn run_disturb(preset: &Preset, config: &DisturbConfig) -> DisturbResult {
    // Phase 1: Auditory pipeline
    let audio = run_auditory_pipeline(preset, config);

    // Phase 2: Clone and inject spike
    let mut left_spiked = audio.left_bands_dec.clone();
    let mut right_spiked = audio.right_bands_dec.clone();

    inject_spike(
        &mut left_spiked,
        config.spike_time_s,
        config.spike_duration_s,
        config.spike_gain,
        NEURAL_SR,
        0xDEAD_BEEF_CAFE_1234,
    );
    inject_spike(
        &mut right_spiked,
        config.spike_time_s,
        config.spike_duration_s,
        config.spike_gain,
        NEURAL_SR,
        0xCAFE_BABE_DEAD_5678, // different seed for independence
    );

    // Phase 3: Run bilateral JR model on disturbed signals
    let neural_params = config.brain_type.params();
    let bilateral_params = config.brain_type.bilateral_params();
    let fast_inhib = FastInhibParams {
        g_fast_gain: neural_params.jansen_rit.g_fast_gain,
        g_fast_rate: neural_params.jansen_rit.g_fast_rate,
        c5: neural_params.jansen_rit.c5,
        c6: neural_params.jansen_rit.c6,
        c7: neural_params.jansen_rit.c7,
    };

    let bi_result = simulate_bilateral(
        &left_spiked,
        &right_spiked,
        &audio.left_energy,
        &audio.right_energy,
        &bilateral_params,
        neural_params.jansen_rit.c,
        neural_params.jansen_rit.input_scale,
        NEURAL_SR,
        &fast_inhib,
        neural_params.jansen_rit.v0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.5,
    );

    // Phase 4: Sliding-window analysis
    let windows = sliding_window_analysis(
        &bi_result.combined.eeg,
        NEURAL_SR,
        config.window_s,
        config.hop_s,
        audio.target_lfo_freq,
    );

    // Phase 5: Compute summary metrics
    let spike_window_time = config.spike_time_s;

    // Baseline: windows before spike
    let baseline_windows: Vec<&WindowMetrics> = windows.iter()
        .filter(|w| w.time_s + config.window_s * 0.5 < spike_window_time)
        .collect();

    let baseline_entrainment = if !baseline_windows.is_empty() {
        let vals: Vec<f64> = baseline_windows.iter()
            .filter_map(|w| w.entrainment_ratio)
            .collect();
        if vals.is_empty() { None } else { Some(vals.iter().sum::<f64>() / vals.len() as f64) }
    } else {
        None
    };

    let baseline_dominant_freq = if !baseline_windows.is_empty() {
        baseline_windows.iter().map(|w| w.dominant_freq).sum::<f64>() / baseline_windows.len() as f64
    } else {
        0.0
    };

    let baseline_centroid = if !baseline_windows.is_empty() {
        baseline_windows.iter().map(|w| w.spectral_centroid).sum::<f64>() / baseline_windows.len() as f64
    } else {
        10.0
    };

    // Baseline band powers (Priority 15)
    let baseline_band_powers = mean_band_powers(&baseline_windows);

    // Post-spike windows
    let post_spike_windows: Vec<&WindowMetrics> = windows.iter()
        .filter(|w| w.time_s > spike_window_time)
        .collect();

    // Nadir: minimum entrainment after spike
    let (nadir_entrainment, nadir_time) = if let Some(base_ent) = baseline_entrainment {
        let mut min_ent = base_ent;
        let mut min_time = spike_window_time;
        for w in &post_spike_windows {
            if let Some(er) = w.entrainment_ratio {
                if er < min_ent {
                    min_ent = er;
                    min_time = w.time_s;
                }
            }
        }
        (Some(min_ent), min_time)
    } else {
        (None, spike_window_time)
    };

    // Peak frequency deviation
    let peak_freq_deviation = post_spike_windows.iter()
        .map(|w| (w.dominant_freq - baseline_dominant_freq).abs())
        .fold(0.0_f64, f64::max);

    // Entrainment recovery times (original metrics — kept for backward compat)
    let recovery_50_ms = compute_recovery_time(
        &post_spike_windows, baseline_entrainment, 0.50, spike_window_time,
    );
    let recovery_90_ms = compute_recovery_time(
        &post_spike_windows, baseline_entrainment, 0.90, spike_window_time,
    );

    // ── Spectral resilience metrics (Priority 15) ──────────────────
    let bppr = compute_bppr(&baseline_band_powers, &post_spike_windows);
    let spectral_recovery_50_ms = compute_spectral_recovery(
        &baseline_band_powers, &baseline_windows, &post_spike_windows, 0.50, spike_window_time,
    );
    let spectral_recovery_90_ms = compute_spectral_recovery(
        &baseline_band_powers, &baseline_windows, &post_spike_windows, 0.90, spike_window_time,
    );
    let scdi_hz = compute_scdi(baseline_centroid, &post_spike_windows);
    let spectral_resilience = compute_spectral_resilience(bppr, spectral_recovery_50_ms, spectral_recovery_90_ms, scdi_hz);

    DisturbResult {
        windows,
        baseline_entrainment,
        baseline_dominant_freq,
        baseline_centroid,
        baseline_band_powers,
        nadir_entrainment,
        nadir_time,
        peak_freq_deviation,
        recovery_50_ms,
        recovery_90_ms,
        bppr,
        spectral_recovery_50_ms,
        spectral_recovery_90_ms,
        scdi_hz,
        spectral_resilience,
        bilateral: bi_result,
        brightness: audio.brightness,
        target_freq: audio.target_lfo_freq,
    }
}

/// Find first post-spike window where entrainment recovers to `fraction` of baseline.
fn compute_recovery_time(
    post_spike: &[&WindowMetrics],
    baseline_ent: Option<f64>,
    fraction: f64,
    spike_time: f64,
) -> Option<f64> {
    let baseline = baseline_ent?;
    let threshold = baseline * fraction;

    for w in post_spike {
        if let Some(er) = w.entrainment_ratio {
            if er >= threshold {
                return Some((w.time_s - spike_time) * 1000.0);
            }
        }
    }
    None
}

// ── Spectral resilience metrics (Priority 15) ──────────────────────────────
//
// Per Pfurtscheller & Lopes da Silva (1999): ERD% = (P(t) - R) / R × 100
// We adapt this to compute Band Power Preservation Ratio (BPPR),
// Spectral Recovery Time (SRT), and Spectral Centroid Deviation Integral
// (SCDI). All three work for any preset type including binaural beats.

/// Compute the mean band powers across a set of windows.
fn mean_band_powers(windows: &[&WindowMetrics]) -> [f64; 5] {
    if windows.is_empty() {
        return [0.2; 5];
    }
    let mut sum = [0.0_f64; 5];
    for w in windows {
        for b in 0..5 { sum[b] += w.band_powers[b]; }
    }
    let n = windows.len() as f64;
    for b in 0..5 { sum[b] /= n; }
    sum
}

/// Band Power Preservation Ratio (BPPR) — per Pfurtscheller 1999.
///
/// Returns the worst-case fractional preservation of band power across
/// all 5 bands during/after the spike. Weighted by a simple "all bands
/// matter equally" approach (goal-specific weighting can be added later).
///
/// BPPR = min over post-spike windows of (Σ_b min(P_b(t)/P_b_baseline, 1.0)) / 5
fn compute_bppr(
    baseline_bp: &[f64; 5],
    post_spike: &[&WindowMetrics],
) -> f64 {
    if post_spike.is_empty() {
        return 1.0;
    }

    let mut worst_preservation = 1.0_f64;

    for w in post_spike {
        let mut window_preservation = 0.0_f64;
        let mut count = 0;
        for b in 0..5 {
            if baseline_bp[b] > 1e-6 {
                let ratio = (w.band_powers[b] / baseline_bp[b]).min(2.0); // cap at 2x to avoid overshoot weighting
                // ERD: ratio < 1.0 means desynchronization
                // ERS: ratio > 1.0 means rebound — treat as preserved
                window_preservation += ratio.min(1.0);
                count += 1;
            }
        }
        if count > 0 {
            let p = window_preservation / count as f64;
            if p < worst_preservation {
                worst_preservation = p;
            }
        }
    }

    worst_preservation.clamp(0.0, 1.0)
}

/// Spectral Recovery Time — how fast band power deviation returns to baseline.
///
/// Uses a baseline-variance-aware threshold: recovery means the deviation
/// drops below `baseline_std + (1-fraction) × (nadir - baseline_std)`.
/// This correctly handles presets with ongoing spectral variation (movement,
/// asymmetric modulation) where the deviation never reaches zero because
/// the baseline itself fluctuates.
///
/// `pre_spike_windows` is used to compute the baseline variance.
fn compute_spectral_recovery(
    baseline_bp: &[f64; 5],
    pre_spike_windows: &[&WindowMetrics],
    post_spike: &[&WindowMetrics],
    fraction: f64,
    spike_time: f64,
) -> Option<f64> {
    if post_spike.is_empty() {
        return Some(0.0);
    }

    // Compute deviation at each window
    let deviation_of = |w: &WindowMetrics| -> f64 {
        let mut dev = 0.0_f64;
        for b in 0..5 {
            if baseline_bp[b] > 1e-6 {
                dev += ((w.band_powers[b] - baseline_bp[b]) / baseline_bp[b]).abs();
            }
        }
        dev
    };

    // Baseline deviation variance: how much the bands naturally fluctuate
    let baseline_std = if pre_spike_windows.len() > 2 {
        let baseline_devs: Vec<f64> = pre_spike_windows.iter().map(|w| deviation_of(w)).collect();
        let mean_dev = baseline_devs.iter().sum::<f64>() / baseline_devs.len() as f64;
        let variance = baseline_devs.iter()
            .map(|d| (d - mean_dev).powi(2))
            .sum::<f64>() / baseline_devs.len() as f64;
        variance.sqrt()
    } else {
        0.0
    };

    // Post-spike deviations
    let deviations: Vec<(f64, f64)> = post_spike.iter()
        .map(|w| (w.time_s, deviation_of(w)))
        .collect();

    // Find the nadir (maximum deviation after spike)
    let max_dev = deviations.iter().map(|(_, d)| *d).fold(0.0_f64, f64::max);
    if max_dev < 1e-6 {
        return Some(0.0);
    }

    // Threshold: baseline_std + (1-fraction) × excess above baseline_std
    // For 90% recovery: threshold = baseline_std + 10% of (nadir - baseline_std)
    // This means "recovered" = deviation is within the normal baseline fluctuation range
    let excess = (max_dev - baseline_std).max(0.0);
    let threshold = baseline_std + excess * (1.0 - fraction);

    let nadir_time = deviations.iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        .map(|(t, _)| *t)
        .unwrap_or(spike_time);

    for &(time, dev) in &deviations {
        if time > nadir_time && dev <= threshold {
            return Some((time - spike_time) * 1000.0);
        }
    }
    None
}

/// Spectral Centroid Deviation Integral (SCDI).
///
/// Mean absolute centroid deviation from baseline across post-spike windows.
/// Lower is better. 0.0 = no spectral displacement.
fn compute_scdi(
    baseline_centroid: f64,
    post_spike: &[&WindowMetrics],
) -> f64 {
    if post_spike.is_empty() {
        return 0.0;
    }
    let sum: f64 = post_spike.iter()
        .map(|w| (w.spectral_centroid - baseline_centroid).abs())
        .sum();
    sum / post_spike.len() as f64
}

/// Composite spectral resilience score ∈ [0, 1].
///
/// Combines BPPR (band preservation), SRT (recovery speed), and SCDI
/// (total displacement) into a single score. Works for all preset types.
///
/// SRT strategy: uses SRT 90% if available. If not (common for dynamic
/// presets with movement/modulation where the baseline itself fluctuates),
/// falls back to SRT 50% with a penalty factor. If neither is available,
/// SRT component scores 0.
fn compute_spectral_resilience(
    bppr: f64,
    srt_50_ms: Option<f64>,
    srt_90_ms: Option<f64>,
    scdi_hz: f64,
) -> f64 {
    let norm_srt = match srt_90_ms {
        Some(ms) => (ms / 2000.0).min(1.0),
        None => match srt_50_ms {
            // Fallback: SRT 50% available but 90% never reached.
            // The 50% recovery happened, so we give partial credit:
            // fast 50% recovery (< 1s) → norm ~0.5 (half credit)
            // slow 50% recovery (> 5s) → norm ~0.9 (nearly failed)
            Some(ms) => 0.5 + 0.5 * (ms / 10000.0).min(1.0),
            None => 1.0, // nothing recovered = worst case
        },
    };
    let norm_scdi = (scdi_hz / 5.0).min(1.0);

    (0.40 * bppr + 0.30 * (1.0 - norm_srt) + 0.30 * (1.0 - norm_scdi)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_window(time: f64, bp: [f64; 5], centroid: f64) -> WindowMetrics {
        WindowMetrics {
            time_s: time,
            entrainment_ratio: None,
            dominant_freq: 10.0,
            spectral_centroid: centroid,
            band_powers: bp,
        }
    }

    // ═══ BPPR tests ═══

    #[test]
    fn bppr_perfect_when_no_change() {
        let baseline = [0.05, 0.10, 0.50, 0.30, 0.05];
        let w = make_window(5.5, baseline, 10.0);
        let post = vec![&w];
        let bppr = compute_bppr(&baseline, &post);
        assert!((bppr - 1.0).abs() < 1e-6, "No change → BPPR=1.0, got {bppr}");
    }

    #[test]
    fn bppr_drops_when_bands_shift() {
        let baseline = [0.05, 0.10, 0.50, 0.30, 0.05];
        let spike_bp = [0.20, 0.20, 0.10, 0.40, 0.10]; // alpha collapsed, others shifted
        let w = make_window(5.5, spike_bp, 10.0);
        let post = vec![&w];
        let bppr = compute_bppr(&baseline, &post);
        // BPPR measures WORST-CASE preservation across all bands.
        // Alpha drops from 0.50 to 0.10 = 20% preserved.
        // But BPPR is averaged across bands, so it's ~0.84 not ~0.20.
        // The metric should be < 1.0 for any band shift.
        assert!(bppr < 1.0, "Band shift → BPPR < 1.0, got {bppr}");
        assert!(bppr > 0.0, "BPPR should be positive, got {bppr}");

        // Total collapse should produce very low BPPR
        let total_collapse = [0.01, 0.01, 0.01, 0.01, 0.96]; // all in gamma
        let w2 = make_window(5.5, total_collapse, 10.0);
        let bppr2 = compute_bppr(&baseline, &[&w2]);
        assert!(bppr2 < 0.3, "Total collapse → BPPR < 0.3, got {bppr2}");
    }

    #[test]
    fn bppr_in_unit_range() {
        let baseline = [0.05, 0.10, 0.50, 0.30, 0.05];
        for alpha in [0.0, 0.10, 0.30, 0.50, 0.80] {
            let bp = [0.10, 0.10, alpha, 0.80 - alpha, 0.0];
            let w = make_window(5.5, bp, 10.0);
            let bppr = compute_bppr(&baseline, &[&w]);
            assert!(bppr >= 0.0 && bppr <= 1.0, "BPPR {bppr} out of [0,1]");
        }
    }

    // ═══ SRT tests ═══

    #[test]
    fn spectral_recovery_zero_when_no_deviation() {
        let baseline = [0.05, 0.10, 0.50, 0.30, 0.05];
        let pre = make_window(3.0, baseline, 10.0);
        let w = make_window(5.5, baseline, 10.0);
        let srt = compute_spectral_recovery(&baseline, &[&pre], &[&w], 0.50, 5.0);
        assert_eq!(srt, Some(0.0), "No deviation → SRT=0, got {srt:?}");
    }

    #[test]
    fn spectral_recovery_positive_after_spike() {
        let baseline = [0.05, 0.10, 0.50, 0.30, 0.05];
        let pre = make_window(3.0, baseline, 10.0);
        let spike_bp = [0.20, 0.20, 0.10, 0.40, 0.10];
        let w1 = make_window(5.2, spike_bp, 15.0);
        let w2 = make_window(5.5, [0.10, 0.15, 0.30, 0.35, 0.10], 12.0);
        let w3 = make_window(5.8, baseline, 10.0);
        let srt = compute_spectral_recovery(&baseline, &[&pre], &[&w1, &w2, &w3], 0.90, 5.0);
        assert!(srt.is_some(), "Should recover");
        assert!(srt.unwrap() > 0.0, "Recovery time should be positive");
    }

    // ═══ SCDI tests ═══

    #[test]
    fn scdi_zero_when_no_deviation() {
        let w = make_window(5.5, [0.2; 5], 10.0);
        let scdi = compute_scdi(10.0, &[&w]);
        assert!((scdi - 0.0).abs() < 1e-6, "No deviation → SCDI=0, got {scdi}");
    }

    #[test]
    fn scdi_positive_when_centroid_shifts() {
        let w1 = make_window(5.5, [0.2; 5], 15.0); // centroid shifted +5
        let w2 = make_window(6.0, [0.2; 5], 12.0); // partially recovered
        let scdi = compute_scdi(10.0, &[&w1, &w2]);
        assert!(scdi > 0.0, "Centroid shift → SCDI > 0, got {scdi}");
        assert!((scdi - 3.5).abs() < 0.01, "SCDI should be (5+2)/2=3.5, got {scdi}");
    }

    // ═══ Composite resilience tests ═══

    #[test]
    fn resilience_perfect_when_no_disturbance() {
        let r = compute_spectral_resilience(1.0, Some(0.0), Some(0.0), 0.0);
        assert!((r - 1.0).abs() < 1e-6, "Perfect → 1.0, got {r}");
    }

    #[test]
    fn resilience_zero_when_worst_case() {
        let r = compute_spectral_resilience(0.0, None, None, 10.0);
        assert!((r - 0.0).abs() < 1e-6, "Worst case → 0.0, got {r}");
    }

    #[test]
    fn resilience_fallback_to_srt50_when_90_unavailable() {
        // SRT 90% not recovered but SRT 50% is — should get partial credit
        let r_with_90 = compute_spectral_resilience(0.5, Some(500.0), Some(500.0), 1.0);
        let r_only_50 = compute_spectral_resilience(0.5, Some(500.0), None, 1.0);
        assert!(r_only_50 > 0.0, "Should get partial credit with SRT 50% fallback");
        // Fallback gives norm_srt = 0.5 + 0.5*(500/10000) = 0.525
        // With 90%: norm_srt = 500/2000 = 0.25
        // So fallback scores lower (higher norm_srt → lower score)
        assert!(r_only_50 < r_with_90,
            "Fallback ({r_only_50:.3}) should score lower than full recovery ({r_with_90:.3})");
    }

    #[test]
    fn resilience_in_unit_range() {
        for bppr in [0.0, 0.3, 0.5, 0.8, 1.0] {
            for srt90 in [None, Some(0.0), Some(500.0), Some(2000.0)] {
                for srt50 in [None, Some(0.0), Some(1000.0), Some(5000.0)] {
                    for scdi in [0.0, 1.0, 3.0, 5.0, 10.0] {
                        let r = compute_spectral_resilience(bppr, srt50, srt90, scdi);
                        assert!(r >= 0.0 && r <= 1.0,
                            "resilience {r} out of [0,1] for bppr={bppr} srt50={srt50:?} srt90={srt90:?} scdi={scdi}");
                    }
                }
            }
        }
    }

    // ═══ Band power sum test ═══

    #[test]
    fn band_powers_sum_to_one() {
        // Create a simple sine signal and verify band powers sum to ~1.0
        let sr = 1000.0;
        let n: usize = 500;
        let signal: Vec<f64> = (0..n)
            .map(|i| (2.0 * std::f64::consts::PI * 10.0 * i as f64 / sr).sin())
            .collect();
        let fft_len = n.next_power_of_two();
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(fft_len);
        let w = analyze_window(&signal, sr, None, &fft, fft_len);
        let sum: f64 = w.band_powers.iter().sum();
        assert!(
            (sum - 1.0).abs() < 0.01,
            "Band powers should sum to ~1.0, got {sum}"
        );
    }
}
