/// Simulation pipeline: Engine → Auditory → Neural → Score.
///
/// Wires together the noise engine, cochlear filterbank, and neural models
/// into a single evaluation function that the optimizer calls.

use crate::auditory::GammatoneFilterbank;
use crate::brain_type::BrainType;
use crate::movement::MovementController;
use crate::neural::{FhnModel, simulate_bilateral};
use crate::preset::Preset;
use crate::scoring::Goal;
use noice_generator_core::NoiseEngine;

use rustfft::{num_complex::Complex, FftPlanner};

const SAMPLE_RATE: u32 = 48_000;

/// Deinterleave stereo buffer into separate L/R channels.
fn deinterleave(interleaved: &[f32]) -> (Vec<f32>, Vec<f32>) {
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
    /// Brain type profile for neural models.
    pub brain_type: BrainType,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        SimulationConfig {
            duration_secs: 3.0,
            brain_type: BrainType::Normal,
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
}

/// Compute spectral brightness from audio via FFT.
///
/// Returns a value in [0, 1] where 0 = very dark (all energy < 200 Hz)
/// and 1 = very bright (all energy > 4 kHz). Based on the spectral centroid
/// mapped through the audible range on a log scale.
fn spectral_brightness(audio: &[f32], sample_rate: f64) -> f64 {
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

    // 5. Normalise each ear's band signals to [0, 1] independently
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

    // 6. Bilateral cortical model: 2×4 parallel Jansen-Rit models
    //    Left hemisphere (fast, θ/γ) ← mainly right ear (contralateral)
    //    Right hemisphere (slow, δ/β) ← mainly left ear (contralateral)
    //    Coupled through corpus callosum with ~10ms delay, ~10% strength.
    let neural_params = config.brain_type.params();
    let bilateral = config.brain_type.bilateral_params();

    let bi_result = simulate_bilateral(
        &left_bands,
        &right_bands,
        &bands_l.energy_fractions,
        &bands_r.energy_fractions,
        &bilateral,
        neural_params.jansen_rit.c,
        neural_params.jansen_rit.input_scale,
        sr,
    );

    let jr_result = &bi_result.combined;

    // 7. FHN: single-neuron driven by combined bilateral EEG oscillations.
    let fhn = FhnModel::with_params(
        sr,
        neural_params.fhn.a,
        neural_params.fhn.b,
        neural_params.fhn.epsilon,
    );

    // Normalise EEG to [-1, 1] range for FHN input
    let eeg_max = jr_result.eeg.iter().map(|x| x.abs()).fold(0.0_f64, f64::max);
    let eeg_norm = if eeg_max > 1e-10 { 1.0 / eeg_max } else { 1.0 };
    let fhn_input: Vec<f64> = jr_result.eeg.iter().map(|x| x * eeg_norm).collect();
    let fhn_result = fhn.simulate(&fhn_input, neural_params.fhn.input_scale);

    // 8. Score (with reduced brightness modifier — neural model now does most work)
    let score = goal.evaluate_with_brightness(&fhn_result, jr_result, brightness);
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
    }
}
