/// Simulation pipeline: Engine → Auditory → Neural → Score.
///
/// Wires together the noise engine, cochlear filterbank, and neural models
/// into a single evaluation function that the optimizer calls.

use crate::auditory::GammatoneFilterbank;
use crate::brain_type::BrainType;
use crate::neural::{FhnModel, simulate_tonotopic};
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

    // 2. Render audio — skip first 1.0s to let engine filters and
    //    gammatone/JR models settle into their steady-state regime.
    let warmup_frames = (SAMPLE_RATE as f32 * 1.0) as u32;
    let _ = engine.render_audio(warmup_frames);
    let audio = engine.render_audio(num_frames);

    // 3. Deinterleave to L/R
    let (left, right) = deinterleave(&audio);

    // 4. Cochlear model: tonotopic band-grouped processing
    //    Groups 32 gammatone channels into 4 frequency bands, preserving
    //    spectral energy distribution for the cortical model.
    let mut filterbank_l = GammatoneFilterbank::new(sr);
    let mut filterbank_r = GammatoneFilterbank::new(sr);

    let bands_l = filterbank_l.process_to_band_groups(&left);
    let bands_r = filterbank_r.process_to_band_groups(&right);

    // 5. Average L/R band signals and normalise each band to [0, 1]
    let mut band_signals: [Vec<f64>; 4] = [
        vec![0.0; bands_l.signals[0].len()],
        vec![0.0; bands_l.signals[1].len()],
        vec![0.0; bands_l.signals[2].len()],
        vec![0.0; bands_l.signals[3].len()],
    ];
    let mut energy_fractions = [0.0_f64; 4];

    for b in 0..4 {
        // Average L/R
        let raw: Vec<f64> = bands_l.signals[b]
            .iter()
            .zip(bands_r.signals[b].iter())
            .map(|(l, r)| (l + r) * 0.5)
            .collect();

        // Normalise to [0, 1]
        let max_val = raw.iter().cloned().fold(0.0_f64, f64::max);
        let norm = if max_val > 1e-10 { 1.0 / max_val } else { 1.0 };
        band_signals[b] = raw.iter().map(|x| x * norm).collect();

        // Average energy fractions from L/R
        energy_fractions[b] = (bands_l.energy_fractions[b] + bands_r.energy_fractions[b]) * 0.5;
    }

    // Re-normalise energy fractions after averaging
    let ef_sum: f64 = energy_fractions.iter().sum();
    if ef_sum > 1e-30 {
        for ef in &mut energy_fractions {
            *ef /= ef_sum;
        }
    }

    // 5b. Spectral brightness from audio FFT (psychoacoustic complement)
    let brightness = spectral_brightness(&left, sr);

    // 6. Tonotopic Jansen-Rit: 4 parallel cortical models with different
    //    time constants per frequency band, weighted by energy fractions.
    let neural_params = config.brain_type.params();
    let tono_params = config.brain_type.tonotopic_params();

    let jr_result = simulate_tonotopic(
        &band_signals,
        &energy_fractions,
        &tono_params.band_rates,
        &tono_params.band_gains,
        &tono_params.band_offsets,
        neural_params.jansen_rit.c,
        neural_params.jansen_rit.input_scale,
        sr,
    );

    // 7. FHN: single-neuron driven by combined EEG oscillations.
    //    The tonotopic EEG has real oscillatory content (unlike the smooth
    //    nerve aggregate), so the FHN can actually fire spikes.
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
    let score = goal.evaluate_with_brightness(&fhn_result, &jr_result, brightness);
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
    }
}
