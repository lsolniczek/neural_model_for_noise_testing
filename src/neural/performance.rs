/// Performance Vector — diagnostic metrics for preset evaluation.
///
/// Instead of collapsing a simulation into a single score, these three
/// metrics expose *how* the noise is affecting the neural model:
///
/// 1. **Entrainment Ratio**: Does the EEG lock onto the preset's LFO?
/// 2. **E/I Stability Index**: Is the fast inhibitory loop stable?
/// 3. **Spectral Centroid**: Where is the EEG's "centre of gravity"?

use rustfft::{num_complex::Complex, FftPlanner};

/// Performance vector returned alongside the scalar score.
#[derive(Debug, Clone, Copy)]
pub struct PerformanceVector {
    /// Ratio of EEG power at the target LFO frequency (±1 Hz) to total power.
    /// Range: [0, 1]. Higher = stronger entrainment.
    /// NaN if no target frequency (e.g. no NeuralLFO modulator active).
    pub entrainment_ratio: Option<f64>,

    /// Standard deviation of the fast inhibitory PSP (y[3]) normalised by its mean.
    /// Coefficient of Variation: low = stable GABA-A loop, high = chaotic.
    /// None when fast inhibition is disabled (G=0).
    pub ei_stability: Option<f64>,

    /// Spectral centroid of the EEG signal (Hz).
    /// The "centre of mass" of the power spectrum in the 1–50 Hz range.
    /// Shift from the ~10 Hz alpha baseline indicates the noise is pulling
    /// the brain toward a different regime.
    pub spectral_centroid: f64,
}

impl PerformanceVector {
    /// Compute the performance vector from simulation outputs.
    ///
    /// - `eeg`: Combined bilateral EEG signal (detrended preferred).
    /// - `fast_inhib_trace`: y[3] trace from the Wendling model (empty if G=0).
    /// - `sample_rate`: Sample rate of the EEG signal (Hz).
    /// - `target_freq`: NeuralLFO frequency from the preset (None if no LFO).
    pub fn compute(
        eeg: &[f64],
        fast_inhib_trace: &[f64],
        sample_rate: f64,
        target_freq: Option<f64>,
    ) -> Self {
        let spectral_centroid = compute_spectral_centroid(eeg, sample_rate);
        let entrainment_ratio = target_freq.map(|freq| {
            compute_entrainment_ratio(eeg, sample_rate, freq)
        });
        let ei_stability = if fast_inhib_trace.is_empty() {
            None
        } else {
            Some(compute_ei_stability(fast_inhib_trace))
        };

        PerformanceVector {
            entrainment_ratio,
            ei_stability,
            spectral_centroid,
        }
    }
}

/// Entrainment Ratio: power in [target_freq - 1, target_freq + 1] Hz / total power.
fn compute_entrainment_ratio(eeg: &[f64], sample_rate: f64, target_freq: f64) -> f64 {
    let n = eeg.len();
    if n < 2 {
        return 0.0;
    }

    let fft_len = n.next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);

    let mut buffer: Vec<Complex<f64>> = (0..fft_len)
        .map(|i| {
            if i < n {
                Complex::new(eeg[i], 0.0)
            } else {
                Complex::new(0.0, 0.0)
            }
        })
        .collect();

    fft.process(&mut buffer);

    let freq_res = sample_rate / fft_len as f64;
    let nyquist_bin = fft_len / 2;

    // Physiological range: 0.5–80 Hz
    let min_bin = (0.5 / freq_res).ceil() as usize;
    let max_bin = (80.0 / freq_res).ceil().min(nyquist_bin as f64) as usize;

    // Target band: target_freq ± 1 Hz
    let target_lo = ((target_freq - 1.0).max(0.5) / freq_res).floor() as usize;
    let target_hi = ((target_freq + 1.0) / freq_res).ceil().min(nyquist_bin as f64) as usize;

    let mut target_power = 0.0_f64;
    let mut total_power = 0.0_f64;

    for bin in min_bin..max_bin {
        let power = buffer[bin].norm_sqr();
        total_power += power;
        if bin >= target_lo && bin <= target_hi {
            target_power += power;
        }
    }

    if total_power > 1e-30 {
        target_power / total_power
    } else {
        0.0
    }
}

/// E/I Stability Index: coefficient of variation of the y[3] trace.
/// CV = std_dev / |mean|. Lower = more stable GABA-A loop.
fn compute_ei_stability(y3: &[f64]) -> f64 {
    let n = y3.len() as f64;
    if n < 2.0 {
        return 0.0;
    }

    let mean = y3.iter().sum::<f64>() / n;
    let variance = y3.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let std_dev = variance.sqrt();

    if mean.abs() > 1e-10 {
        std_dev / mean.abs()
    } else {
        // If mean ≈ 0, return raw std_dev (fast inhib barely active)
        std_dev
    }
}

/// Spectral centroid of the EEG in the 1–50 Hz range.
fn compute_spectral_centroid(eeg: &[f64], sample_rate: f64) -> f64 {
    let n = eeg.len();
    if n < 2 {
        return 10.0; // default alpha
    }

    let fft_len = n.next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);

    let mut buffer: Vec<Complex<f64>> = (0..fft_len)
        .map(|i| {
            if i < n {
                Complex::new(eeg[i], 0.0)
            } else {
                Complex::new(0.0, 0.0)
            }
        })
        .collect();

    fft.process(&mut buffer);

    let freq_res = sample_rate / fft_len as f64;
    let nyquist_bin = fft_len / 2;

    let min_bin = (1.0 / freq_res).ceil() as usize;
    let max_bin = (50.0 / freq_res).ceil().min(nyquist_bin as f64) as usize;

    let mut weighted_sum = 0.0_f64;
    let mut total_power = 0.0_f64;

    for bin in min_bin..max_bin {
        let freq = bin as f64 * freq_res;
        let power = buffer[bin].norm_sqr();
        weighted_sum += freq * power;
        total_power += power;
    }

    if total_power > 1e-30 {
        weighted_sum / total_power
    } else {
        10.0 // default alpha baseline
    }
}
