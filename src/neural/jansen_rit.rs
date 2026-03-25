/// Jansen-Rit Neural Mass Model (1995).
///
/// Models three interacting neural populations:
///   - Pyramidal cells (main output)
///   - Excitatory interneurons
///   - Inhibitory interneurons
///
/// Produces EEG-like output with recognisable brain rhythms (delta, theta,
/// alpha, beta, gamma) depending on the input drive.
///
/// State variables (6):
///   y0, y3: excitatory interneuron PSP and its derivative
///   y1, y4: pyramidal cell PSP and its derivative
///   y2, y5: inhibitory interneuron PSP and its derivative
///
/// EEG output: y1 - y2 (net pyramidal PSP)

use rustfft::{num_complex::Complex, FftPlanner};
use std::f64::consts::PI;

// ── Standard Jansen-Rit parameters ──────────────────────────────────────────

const A: f64 = 3.25;     // Excitatory synaptic gain (mV)
const B: f64 = 22.0;     // Inhibitory synaptic gain (mV)
const A_RATE: f64 = 100.0; // Excitatory time constant (1/s)
const B_RATE: f64 = 50.0;  // Inhibitory time constant (1/s)

// Sigmoid parameters
const V_MAX: f64 = 5.0;  // Max firing rate (1/s)
const V0: f64 = 6.0;     // Firing threshold (mV)
const R: f64 = 0.56;     // Sigmoid steepness (1/mV)

// Connectivity constants (from Jansen & Rit 1995)
const C: f64 = 135.0;
const C1: f64 = C;
const C2: f64 = 0.8 * C;
const C3: f64 = 0.25 * C;
const C4: f64 = 0.25 * C;

/// Frequency bands for EEG analysis (Hz).
pub struct BandPowers {
    pub delta: f64, // 0.5–4 Hz — deep sleep
    pub theta: f64, // 4–8 Hz — relaxation, meditation
    pub alpha: f64, // 8–13 Hz — calm alertness
    pub beta: f64,  // 13–30 Hz — active focus
    pub gamma: f64, // 30–50 Hz — high-level processing
}

impl BandPowers {
    /// Total power across all bands.
    pub fn total(&self) -> f64 {
        self.delta + self.theta + self.alpha + self.beta + self.gamma
    }

    /// Normalised power (each band as fraction of total).
    pub fn normalized(&self) -> BandPowers {
        let t = self.total();
        if t < 1e-30 {
            return BandPowers {
                delta: 0.2,
                theta: 0.2,
                alpha: 0.2,
                beta: 0.2,
                gamma: 0.2,
            };
        }
        BandPowers {
            delta: self.delta / t,
            theta: self.theta / t,
            alpha: self.alpha / t,
            beta: self.beta / t,
            gamma: self.gamma / t,
        }
    }
}

pub struct JansenRitResult {
    /// EEG-like output signal (y1 - y2).
    pub eeg: Vec<f64>,
    /// Spectral band powers.
    pub band_powers: BandPowers,
    /// Dominant frequency (Hz).
    pub dominant_freq: f64,
}

pub struct JansenRitModel {
    dt: f64,
    sample_rate: f64,
    /// Input scaling — maps auditory nerve signal to pulse density (p).
    /// Typical resting input: ~120–320 pulses/s.
    pub input_offset: f64,
    pub input_scale: f64,
    // Configurable model parameters (default to module constants)
    a_gain: f64,
    b_gain: f64,
    a_rate: f64,
    b_rate: f64,
    c1: f64,
    c2: f64,
    c3: f64,
    c4: f64,
}

impl JansenRitModel {
    pub fn new(sample_rate: f64) -> Self {
        JansenRitModel {
            dt: 1.0 / sample_rate,
            sample_rate,
            input_offset: 220.0,
            input_scale: 100.0,
            a_gain: A,
            b_gain: B,
            a_rate: A_RATE,
            b_rate: B_RATE,
            c1: C1,
            c2: C2,
            c3: C3,
            c4: C4,
        }
    }

    /// Create with custom parameters (for brain type profiles).
    pub fn with_params(
        sample_rate: f64,
        a_gain: f64,
        b_gain: f64,
        a_rate: f64,
        b_rate: f64,
        c: f64,
        input_offset: f64,
        input_scale: f64,
    ) -> Self {
        JansenRitModel {
            dt: 1.0 / sample_rate,
            sample_rate,
            input_offset,
            input_scale,
            a_gain,
            b_gain,
            a_rate,
            b_rate,
            c1: c,
            c2: 0.8 * c,
            c3: 0.25 * c,
            c4: 0.25 * c,
        }
    }

    /// Sigmoid transfer function: converts PSP to firing rate.
    #[inline]
    fn sigmoid(v: f64) -> f64 {
        V_MAX / (1.0 + (-R * (v - V0)).exp())
    }

    /// Compute derivatives for the 6-state system.
    #[inline]
    fn derivatives(&self, y: &[f64; 6], p: f64) -> [f64; 6] {
        let sig_y1_y2 = Self::sigmoid(y[1] - y[2]);
        let sig_c1_y0 = Self::sigmoid(self.c1 * y[0]);
        let sig_c3_y0 = Self::sigmoid(self.c3 * y[0]);

        let a = self.a_gain;
        let b = self.b_gain;
        let ar = self.a_rate;
        let br = self.b_rate;

        [
            // dy0/dt = y3
            y[3],
            // dy1/dt = y4
            y[4],
            // dy2/dt = y5
            y[5],
            // dy3/dt = A·a·S(y1-y2) - 2a·y3 - a²·y0
            a * ar * sig_y1_y2 - 2.0 * ar * y[3] - ar * ar * y[0],
            // dy4/dt = A·a·(p + C2·S(C1·y0)) - 2a·y4 - a²·y1
            a * ar * (p + self.c2 * sig_c1_y0) - 2.0 * ar * y[4] - ar * ar * y[1],
            // dy5/dt = B·b·C4·S(C3·y0) - 2b·y5 - b²·y2
            b * br * self.c4 * sig_c3_y0 - 2.0 * br * y[5] - br * br * y[2],
        ]
    }

    /// Simulate the Jansen-Rit model with external input.
    ///
    /// `input` is the aggregated auditory nerve signal at each time step.
    pub fn simulate(&self, input: &[f64]) -> JansenRitResult {
        let n = input.len();
        let mut eeg = vec![0.0_f64; n];

        // State: [y0, y1, y2, y3, y4, y5]
        let mut y = [0.001_f64, 0.0, 0.0, 0.0, 0.0, 0.0];

        let sub_steps = 2_usize;
        let h = self.dt / sub_steps as f64;

        // JR warmup: run model for 1 second on the first input sample
        // to let it reach the limit cycle before recording EEG output.
        let warmup_steps = (self.sample_rate * 1.0) as usize;
        let warmup_p = self.input_offset + input[0] * self.input_scale;
        for _ in 0..warmup_steps {
            for _ in 0..sub_steps {
                let k1 = self.derivatives(&y, warmup_p);
                let mut y_tmp = [0.0; 6];
                for j in 0..6 { y_tmp[j] = y[j] + 0.5 * h * k1[j]; }
                let k2 = self.derivatives(&y_tmp, warmup_p);
                for j in 0..6 { y_tmp[j] = y[j] + 0.5 * h * k2[j]; }
                let k3 = self.derivatives(&y_tmp, warmup_p);
                for j in 0..6 { y_tmp[j] = y[j] + h * k3[j]; }
                let k4 = self.derivatives(&y_tmp, warmup_p);
                for j in 0..6 {
                    y[j] += h / 6.0 * (k1[j] + 2.0 * k2[j] + 2.0 * k3[j] + k4[j]);
                }
            }
        }

        for i in 0..n {
            let p = self.input_offset + input[i] * self.input_scale;

            for _ in 0..sub_steps {
                let k1 = self.derivatives(&y, p);
                let mut y_tmp = [0.0; 6];
                for j in 0..6 { y_tmp[j] = y[j] + 0.5 * h * k1[j]; }
                let k2 = self.derivatives(&y_tmp, p);
                for j in 0..6 { y_tmp[j] = y[j] + 0.5 * h * k2[j]; }
                let k3 = self.derivatives(&y_tmp, p);
                for j in 0..6 { y_tmp[j] = y[j] + h * k3[j]; }
                let k4 = self.derivatives(&y_tmp, p);
                for j in 0..6 {
                    y[j] += h / 6.0 * (k1[j] + 2.0 * k2[j] + 2.0 * k3[j] + k4[j]);
                }
            }

            eeg[i] = y[1] - y[2];
        }

        // Remove DC offset before spectral analysis
        let eeg_mean = eeg.iter().sum::<f64>() / n as f64;
        let eeg_detrended: Vec<f64> = eeg.iter().map(|x| x - eeg_mean).collect();

        let band_powers = self.compute_band_powers(&eeg_detrended);
        let dominant_freq = self.find_dominant_frequency(&eeg_detrended);

        JansenRitResult {
            eeg,
            band_powers,
            dominant_freq,
        }
    }

    /// Compute power in each EEG frequency band using FFT.
    fn compute_band_powers(&self, signal: &[f64]) -> BandPowers {
        let n = signal.len();
        if n < 2 {
            return BandPowers {
                delta: 0.0,
                theta: 0.0,
                alpha: 0.0,
                beta: 0.0,
                gamma: 0.0,
            };
        }

        // Zero-pad to next power of 2 for FFT efficiency
        let fft_len = n.next_power_of_two();
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(fft_len);

        // Apply Hanning window
        let mut buffer: Vec<Complex<f64>> = (0..fft_len)
            .map(|i| {
                if i < n {
                    let window = 0.5 * (1.0 - (2.0 * PI * i as f64 / (n - 1) as f64).cos());
                    Complex::new(signal[i] * window, 0.0)
                } else {
                    Complex::new(0.0, 0.0)
                }
            })
            .collect();

        fft.process(&mut buffer);

        // Compute power spectral density (one-sided)
        let freq_resolution = self.sample_rate / fft_len as f64;
        let nyquist_bin = fft_len / 2;

        let mut delta = 0.0_f64;
        let mut theta = 0.0_f64;
        let mut alpha = 0.0_f64;
        let mut beta = 0.0_f64;
        let mut gamma = 0.0_f64;

        for bin in 1..nyquist_bin {
            let freq = bin as f64 * freq_resolution;
            let power = buffer[bin].norm_sqr() / fft_len as f64;

            if freq >= 0.5 && freq < 4.0 {
                delta += power;
            } else if freq >= 4.0 && freq < 8.0 {
                theta += power;
            } else if freq >= 8.0 && freq < 13.0 {
                alpha += power;
            } else if freq >= 13.0 && freq < 30.0 {
                beta += power;
            } else if freq >= 30.0 && freq < 50.0 {
                gamma += power;
            }
        }

        BandPowers {
            delta,
            theta,
            alpha,
            beta,
            gamma,
        }
    }

    fn find_dominant_frequency(&self, signal: &[f64]) -> f64 {
        let n = signal.len();
        if n < 2 {
            return 0.0;
        }

        let fft_len = n.next_power_of_two();
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(fft_len);

        let mut buffer: Vec<Complex<f64>> = (0..fft_len)
            .map(|i| {
                if i < n {
                    Complex::new(signal[i], 0.0)
                } else {
                    Complex::new(0.0, 0.0)
                }
            })
            .collect();

        fft.process(&mut buffer);

        let freq_resolution = self.sample_rate / fft_len as f64;
        let nyquist_bin = fft_len / 2;

        let mut max_power = 0.0_f64;
        let mut max_bin = 1_usize;

        // Only look at physiologically relevant range (0.5–50 Hz)
        let min_bin = (0.5 / freq_resolution).ceil() as usize;
        let max_freq_bin = (50.0 / freq_resolution).ceil() as usize;

        for bin in min_bin..max_freq_bin.min(nyquist_bin) {
            let power = buffer[bin].norm_sqr();
            if power > max_power {
                max_power = power;
                max_bin = bin;
            }
        }

        max_bin as f64 * freq_resolution
    }
}

/// Run 4 parallel Jansen-Rit models (one per tonotopic band) and combine
/// their EEG outputs weighted by each band's energy fraction.
///
/// This produces spectrally-sensitive neural responses: low bands (slow JR)
/// generate delta/theta, high bands (fast JR) generate beta/gamma. The
/// noise colour determines which bands dominate the combined EEG.
pub fn simulate_tonotopic(
    band_signals: &[Vec<f64>; 4],
    energy_fractions: &[f64; 4],
    band_rates: &[(f64, f64); 4],
    band_gains: &[(f64, f64); 4],
    band_offsets: &[f64; 4],
    c: f64,
    input_scale: f64,
    sample_rate: f64,
) -> JansenRitResult {
    let n = band_signals[0].len();
    let mut combined_eeg = vec![0.0_f64; n];

    // The JR model needs input variability to break out of the unstable fixed
    // point and enter the limit cycle. High-frequency gammatone bands produce
    // very smooth envelopes (all fast fluctuations removed by the 50 Hz smoother),
    // which keeps the model near its fixed point. A small noise perturbation
    // (physiological noise) ensures the model oscillates in the correct regime.

    // Run 4 independent JR models, normalise each to unit RMS, then
    // mix by energy fraction. Normalisation is critical because slower
    // JR models produce higher-amplitude oscillations (amplitude ∝ 1/a_rate),
    // which would otherwise drown out the faster bands' contribution.
    for b in 0..4 {
        let (a_rate, b_rate) = band_rates[b];
        let (a_gain, b_gain) = band_gains[b];

        let jr = JansenRitModel::with_params(
            sample_rate,
            a_gain,
            b_gain,
            a_rate,
            b_rate,
            c,
            band_offsets[b],
            input_scale,
        );

        let band_result = jr.simulate(&band_signals[b]);

        // Detrend (remove DC) before RMS normalisation — the JR model
        // oscillates around a non-zero equilibrium, so raw RMS is dominated
        // by the DC offset, making all bands appear identical after normalisation.
        let band_mean = band_result.eeg.iter().sum::<f64>() / n as f64;
        let detrended: Vec<f64> = band_result.eeg.iter().map(|x| x - band_mean).collect();

        let rms = (detrended.iter().map(|x| x * x).sum::<f64>() / n as f64).sqrt();
        let norm = if rms > 1e-10 { 1.0 / rms } else { 1.0 };

        let weight = energy_fractions[b];
        for i in 0..n {
            combined_eeg[i] += weight * detrended[i] * norm;
        }
    }

    // Detrend and analyse the combined signal
    let combined_mean = combined_eeg.iter().sum::<f64>() / n as f64;
    let combined_detrended: Vec<f64> = combined_eeg.iter().map(|x| x - combined_mean).collect();
    let analysis_jr = JansenRitModel::new(sample_rate);
    let band_powers = analysis_jr.compute_band_powers(&combined_detrended);
    let dominant_freq = analysis_jr.find_dominant_frequency(&combined_detrended);

    JansenRitResult {
        eeg: combined_eeg,
        band_powers,
        dominant_freq,
    }
}
