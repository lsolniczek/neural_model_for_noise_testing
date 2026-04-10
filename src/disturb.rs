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
    /// Minimum entrainment ratio during/after spike.
    pub nadir_entrainment: Option<f64>,
    /// Time of nadir (seconds).
    pub nadir_time: f64,
    /// Peak frequency deviation from baseline (Hz).
    pub peak_freq_deviation: f64,
    /// Time to 50% recovery (seconds after spike, None if not recovered).
    pub recovery_50_ms: Option<f64>,
    /// Time to 90% recovery (seconds after spike, None if not recovered).
    pub recovery_90_ms: Option<f64>,
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

    // Extract target LFO frequency from preset
    let target_lfo_freq = preset.objects.iter()
        .flat_map(|obj| [&obj.bass_mod, &obj.satellite_mod])
        .filter(|m| m.kind == 4 && m.param_a > 0.5)
        .map(|m| m.param_a as f64)
        .next();

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

    WindowMetrics {
        time_s: 0.0, // filled by caller
        entrainment_ratio,
        dominant_freq: peak_freq,
        spectral_centroid,
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
        neural_params.jansen_rit.v0, 0.0, 0.0, 0.0,
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

    // Recovery times
    let recovery_50_ms = compute_recovery_time(
        &post_spike_windows, baseline_entrainment, 0.50, spike_window_time,
    );
    let recovery_90_ms = compute_recovery_time(
        &post_spike_windows, baseline_entrainment, 0.90, spike_window_time,
    );

    DisturbResult {
        windows,
        baseline_entrainment,
        baseline_dominant_freq,
        baseline_centroid,
        nadir_entrainment,
        nadir_time,
        peak_freq_deviation,
        recovery_50_ms,
        recovery_90_ms,
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
                // Convert to ms after spike
                return Some((w.time_s - spike_time) * 1000.0);
            }
        }
    }
    None
}
