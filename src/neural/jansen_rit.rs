/// Wendling (2002) Neural Mass Model — extended Jansen-Rit.
///
/// Models four interacting neural populations:
///   - Pyramidal cells (main output)
///   - Excitatory interneurons (Glutamate)
///   - Slow inhibitory interneurons (GABA-B)
///   - Fast inhibitory interneurons (GABA-A) ← Wendling extension
///
/// Produces EEG-like output with recognisable brain rhythms (delta, theta,
/// alpha, beta, gamma) depending on the input drive.
///
/// State variables (8):
///   y0, y4: excitatory interneuron PSP and its derivative
///   y1, y5: pyramidal cell excitatory PSP and its derivative
///   y2, y6: slow inhibitory interneuron PSP and its derivative (GABA-B)
///   y3, y7: fast inhibitory interneuron PSP and its derivative (GABA-A)
///
/// EEG output: y1 - y2 - y3 (net pyramidal membrane voltage)
///
/// When g_fast_gain = 0, the model degenerates to the classic JR 1995
/// 6-state system (y3 stays at zero, EEG = y1 - y2).

use crate::brain_type::{BandModelType, BilateralParams, TonotopicParams};
use crate::neural::wilson_cowan::WilsonCowanModel;
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
const R: f64 = 0.62;     // Sigmoid steepness (1/mV) — Universal sweet spot (0.56→0.65→0.62)

// Connectivity constants (from Jansen & Rit 1995)
const C: f64 = 135.0;
const C1: f64 = C;
const C2: f64 = 0.8 * C;
const C3: f64 = 0.20 * C;    // Universal: loosen GABA-B for beta access (0.25→0.225→0.20)
const C4: f64 = 0.20 * C;    // Universal: allow fast loop to drive SMR/beta

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
    /// Fast inhibitory PSP trace (y[3]) — activity of GABA-A interneurons.
    /// Empty when fast inhibition is disabled (G=0 / JR95 mode).
    pub fast_inhib_trace: Vec<f64>,
}

/// Fast inhibitory (GABA-A) parameters for the Wendling 2002 extension.
/// When `g_fast_gain` is 0.0, the model behaves as classic JR 1995.
#[derive(Debug, Clone)]
pub struct FastInhibParams {
    pub g_fast_gain: f64,
    pub g_fast_rate: f64,
    pub c5: f64,
    pub c6: f64,
    pub c7: f64,
}

impl Default for FastInhibParams {
    fn default() -> Self {
        FastInhibParams {
            g_fast_gain: 0.0,
            g_fast_rate: 0.0,
            c5: 0.0,
            c6: 0.0,
            c7: 0.0,
        }
    }
}

pub struct JansenRitModel {
    dt: f64,
    sample_rate: f64,
    /// Input scaling — maps auditory nerve signal to pulse density (p).
    /// Typical resting input: ~120–320 pulses/s.
    pub input_offset: f64,
    pub input_scale: f64,
    // JR95 parameters
    a_gain: f64,
    b_gain: f64,
    a_rate: f64,
    b_rate: f64,
    c1: f64,
    c2: f64,
    c3: f64,
    c4: f64,
    // Wendling 2002 fast inhibitory (GABA-A) parameters
    g_fast_gain: f64,
    g_fast_rate: f64,
    c5: f64,
    c6: f64,
    c7: f64,
    // Per-model sigmoid threshold (allows per-brain-type tuning)
    v0: f64,
    // Per-model sigmoid steepness (allows per-band frequency tuning)
    sigmoid_r: f64,
}

impl JansenRitModel {
    /// Create with default JR95 parameters (no fast inhibition).
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
            // JR95 mode: fast inhibition disabled
            g_fast_gain: 0.0,
            g_fast_rate: 0.0,
            c5: 0.0,
            c6: 0.0,
            c7: 0.0,
            v0: V0,
            sigmoid_r: R,
        }
    }

    /// Create with custom JR95 parameters (no fast inhibition).
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
            c3: 0.20 * c,
            c4: 0.20 * c,
            g_fast_gain: 0.0,
            g_fast_rate: 0.0,
            c5: 0.0,
            c6: 0.0,
            c7: 0.0,
            v0: V0,
            sigmoid_r: R,
        }
    }

    /// Create with full Wendling 2002 parameters (fast inhibition enabled).
    pub fn with_wendling_params(
        sample_rate: f64,
        a_gain: f64,
        b_gain: f64,
        a_rate: f64,
        b_rate: f64,
        c: f64,
        input_offset: f64,
        input_scale: f64,
        fast_inhib: &FastInhibParams,
        slow_inhib_ratio: f64,
        v0: f64,
        sigmoid_r: f64,
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
            c3: slow_inhib_ratio * c,
            c4: slow_inhib_ratio * c,
            g_fast_gain: fast_inhib.g_fast_gain,
            g_fast_rate: fast_inhib.g_fast_rate,
            c5: fast_inhib.c5,
            c6: fast_inhib.c6,
            c7: fast_inhib.c7,
            v0,
            sigmoid_r,
        }
    }

    /// Scale the primary excitatory loop coupling (C1, C2).
    /// A lighter loop can cycle faster — "Lean Loop" strategy for beta access.
    /// scale = 1.0 is standard, 0.75 = 25% reduction.
    pub fn scale_c1c2(&mut self, scale: f64) {
        self.c1 *= scale;
        self.c2 *= scale;
    }

    /// Sigmoid transfer function: converts PSP to firing rate.
    /// Uses per-model v0 threshold for brain-type-specific tuning.
    #[inline]
    fn sigmoid(&self, v: f64) -> f64 {
        V_MAX / (1.0 + (-self.sigmoid_r * (v - self.v0)).exp())
    }

    /// Compute derivatives for the 8-state Wendling system.
    ///
    /// When g_fast_gain = 0, derivatives[3] and [7] are zero, reducing to JR95.
    #[inline]
    fn derivatives(&self, y: &[f64; 8], p: f64) -> [f64; 8] {
        // v_pyr = y1 - y2 - y3 (Wendling EEG: excitatory - slow_inhib - fast_inhib)
        let v_pyr = y[1] - y[2] - y[3];
        let sig_vpyr = self.sigmoid(v_pyr);
        let sig_c1_y0 = self.sigmoid(self.c1 * y[0]);
        let sig_c3_y0 = self.sigmoid(self.c3 * y[0]);

        let a = self.a_gain;
        let b = self.b_gain;
        let ar = self.a_rate;
        let br = self.b_rate;
        let g = self.g_fast_gain;
        let gr = self.g_fast_rate;

        // Fast inhibitory afferent input (Wendling 2002):
        //   Excitatory drive: C5 * S(v_pyr) — fast inhib fires when pyramidal cells fire
        //   Inhibitory drive: C6 * S(C3 * y0) — slow inhib suppresses fast inhib
        //
        // NOTE: The sigmoid must be applied to FIRING RATES (bounded [0, V_MAX]),
        // not to raw PSP state variables. PSPs have different scales due to
        // different gains (A=3.25 vs B=22), making S(C5*y0 - C6*y2) ≈ 0 always.
        // The correct formulation uses sigmoided population outputs:
        //   C5*S(vpyr) produces values in [0, C5*V_MAX] = [0, 202.5]
        //   C6*S(C3*y0) produces values in [0, C6*V_MAX] = [0, 67.5]
        let fast_drive = if g > 0.0 {
            self.c5 * sig_vpyr - self.c6 * sig_c3_y0
        } else {
            0.0
        };

        [
            // dy0/dt = y4  (excitatory interneuron)
            y[4],
            // dy1/dt = y5  (pyramidal excitatory)
            y[5],
            // dy2/dt = y6  (slow inhibitory GABA-B)
            y[6],
            // dy3/dt = y7  (fast inhibitory GABA-A)
            y[7],
            // dy4/dt = A·a·S(v_pyr) - 2a·y4 - a²·y0
            a * ar * sig_vpyr - 2.0 * ar * y[4] - ar * ar * y[0],
            // dy5/dt = A·a·(p + C2·S(C1·y0)) - 2a·y5 - a²·y1
            a * ar * (p + self.c2 * sig_c1_y0) - 2.0 * ar * y[5] - ar * ar * y[1],
            // dy6/dt = B·b·C4·S(C3·y0) - 2b·y6 - b²·y2
            b * br * self.c4 * sig_c3_y0 - 2.0 * br * y[6] - br * br * y[2],
            // dy7/dt = G·g·(C5·S(vpyr) - C6·S(C3·y0)) - 2g·y7 - g²·y3
            g * gr * fast_drive - 2.0 * gr * y[7] - gr * gr * y[3],
        ]
    }

    /// Simulate and also record the fast inhibitory state y[3] for diagnostics.
    pub fn simulate_with_fast_inhib_trace(&self, input: &[f64]) -> (JansenRitResult, Vec<f64>) {
        let n = input.len();
        let mut eeg = vec![0.0_f64; n];
        let mut y3_trace = vec![0.0_f64; n];

        let mut y = [0.001_f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let sub_steps = if self.g_fast_rate > 200.0 { 3_usize } else { 2_usize };

        let h = self.dt / sub_steps as f64;

        let warmup_steps = (self.sample_rate * 1.0) as usize;
        let warmup_p = self.input_offset + input[0] * self.input_scale;
        for _ in 0..warmup_steps {
            for _ in 0..sub_steps {
                let k1 = self.derivatives(&y, warmup_p);
                let mut y_tmp = [0.0; 8];
                for j in 0..8 { y_tmp[j] = y[j] + 0.5 * h * k1[j]; }
                let k2 = self.derivatives(&y_tmp, warmup_p);
                for j in 0..8 { y_tmp[j] = y[j] + 0.5 * h * k2[j]; }
                let k3 = self.derivatives(&y_tmp, warmup_p);
                for j in 0..8 { y_tmp[j] = y[j] + h * k3[j]; }
                let k4 = self.derivatives(&y_tmp, warmup_p);
                for j in 0..8 {
                    y[j] += h / 6.0 * (k1[j] + 2.0 * k2[j] + 2.0 * k3[j] + k4[j]);
                }
            }
        }

        for i in 0..n {
            let p = self.input_offset + input[i] * self.input_scale;
            for _ in 0..sub_steps {
                let k1 = self.derivatives(&y, p);
                let mut y_tmp = [0.0; 8];
                for j in 0..8 { y_tmp[j] = y[j] + 0.5 * h * k1[j]; }
                let k2 = self.derivatives(&y_tmp, p);
                for j in 0..8 { y_tmp[j] = y[j] + 0.5 * h * k2[j]; }
                let k3 = self.derivatives(&y_tmp, p);
                for j in 0..8 { y_tmp[j] = y[j] + h * k3[j]; }
                let k4 = self.derivatives(&y_tmp, p);
                for j in 0..8 {
                    y[j] += h / 6.0 * (k1[j] + 2.0 * k2[j] + 2.0 * k3[j] + k4[j]);
                }
            }
            eeg[i] = y[1] - y[2] - y[3];
            y3_trace[i] = y[3];
        }

        let eeg_mean = eeg.iter().sum::<f64>() / n as f64;
        let eeg_detrended: Vec<f64> = eeg.iter().map(|x| x - eeg_mean).collect();
        let band_powers = self.compute_band_powers(&eeg_detrended);
        let dominant_freq = self.find_dominant_frequency(&eeg_detrended);

        (JansenRitResult { eeg, band_powers, dominant_freq, fast_inhib_trace: y3_trace.clone() }, y3_trace)
    }

    /// Simulate the Wendling/JR model with external input.
    ///
    /// `input` is the aggregated auditory nerve signal at each time step.
    pub fn simulate(&self, input: &[f64]) -> JansenRitResult {
        let n = input.len();
        let mut eeg = vec![0.0_f64; n];
        let has_fast_inhib = self.g_fast_gain > 0.0;
        let mut y3_trace = if has_fast_inhib { vec![0.0_f64; n] } else { Vec::new() };

        // State: [y0, y1, y2, y3, y4, y5, y6, y7]
        // y3/y7 = fast inhibitory (GABA-A), zero-initialised → JR95 compat
        let mut y = [0.001_f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];

        // Use more sub-steps when fast inhibitory is active (g_rate up to 500)
        let sub_steps = if self.g_fast_rate > 200.0 { 3_usize } else { 2_usize };
        let h = self.dt / sub_steps as f64;

        // Warmup: run model for 1 second on the first input sample
        // to let it reach the limit cycle before recording EEG output.
        let warmup_steps = (self.sample_rate * 1.0) as usize;
        let warmup_p = self.input_offset + input[0] * self.input_scale;
        for _ in 0..warmup_steps {
            for _ in 0..sub_steps {
                let k1 = self.derivatives(&y, warmup_p);
                let mut y_tmp = [0.0; 8];
                for j in 0..8 { y_tmp[j] = y[j] + 0.5 * h * k1[j]; }
                let k2 = self.derivatives(&y_tmp, warmup_p);
                for j in 0..8 { y_tmp[j] = y[j] + 0.5 * h * k2[j]; }
                let k3 = self.derivatives(&y_tmp, warmup_p);
                for j in 0..8 { y_tmp[j] = y[j] + h * k3[j]; }
                let k4 = self.derivatives(&y_tmp, warmup_p);
                for j in 0..8 {
                    y[j] += h / 6.0 * (k1[j] + 2.0 * k2[j] + 2.0 * k3[j] + k4[j]);
                }
            }
        }

        for i in 0..n {
            let p = self.input_offset + input[i] * self.input_scale;

            for _ in 0..sub_steps {
                let k1 = self.derivatives(&y, p);
                let mut y_tmp = [0.0; 8];
                for j in 0..8 { y_tmp[j] = y[j] + 0.5 * h * k1[j]; }
                let k2 = self.derivatives(&y_tmp, p);
                for j in 0..8 { y_tmp[j] = y[j] + 0.5 * h * k2[j]; }
                let k3 = self.derivatives(&y_tmp, p);
                for j in 0..8 { y_tmp[j] = y[j] + h * k3[j]; }
                let k4 = self.derivatives(&y_tmp, p);
                for j in 0..8 {
                    y[j] += h / 6.0 * (k1[j] + 2.0 * k2[j] + 2.0 * k3[j] + k4[j]);
                }
            }

            // EEG = excitatory - slow_inhib - fast_inhib (Wendling 2002)
            eeg[i] = y[1] - y[2] - y[3];
            if has_fast_inhib {
                y3_trace[i] = y[3];
            }
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
            fast_inhib_trace: y3_trace,
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
    tono: &TonotopicParams,
    c: f64,
    input_scale: f64,
    sample_rate: f64,
    fast_inhib: &FastInhibParams,
    v0: f64,
) -> JansenRitResult {
    let n = band_signals[0].len();
    let mut combined_eeg = vec![0.0_f64; n];
    let has_fast_inhib = fast_inhib.g_fast_gain > 0.0;
    let mut combined_y3 = if has_fast_inhib { vec![0.0_f64; n] } else { Vec::new() };

    // Run 4 independent Wendling/JR models, normalise each to unit RMS, then
    // mix by energy fraction. Normalisation is critical because slower
    // models produce higher-amplitude oscillations (amplitude ∝ 1/a_rate),
    // which would otherwise drown out the faster bands' contribution.
    for b in 0..4 {
        // Apply per-band input gain scaling
        let gain = tono.band_input_gains[b];
        let input_signal = if (gain - 1.0).abs() < 1e-6 {
            band_signals[b].clone()
        } else {
            band_signals[b].iter().map(|&x| (x * gain).min(1.0)).collect()
        };

        // Dispatch: JansenRit or WilsonCowan per band
        let (band_eeg, band_inhib) = match tono.band_model_types[b] {
            BandModelType::JansenRit => {
                let (a_rate, b_rate) = tono.band_rates[b];
                let (a_gain, b_gain) = tono.band_gains[b];
                let band_slow_inhib = tono.band_slow_inhib_ratios[b];
                let band_r = tono.band_sigmoid_r[b];
                let band_v0 = tono.band_v0[b];
                let band_fi = FastInhibParams {
                    g_fast_gain: fast_inhib.g_fast_gain,
                    g_fast_rate: tono.band_g_fast_rate[b],
                    c5: fast_inhib.c5,
                    c6: fast_inhib.c6,
                    c7: tono.band_c7[b],
                };
                let mut jr = JansenRitModel::with_wendling_params(
                    sample_rate, a_gain, b_gain, a_rate, b_rate,
                    c, tono.band_offsets[b], input_scale,
                    &band_fi, band_slow_inhib, band_v0, band_r,
                );
                let c1c2_scale = tono.band_c1c2_scale[b];
                if (c1c2_scale - 1.0).abs() > 1e-6 {
                    jr.scale_c1c2(c1c2_scale);
                }
                let result = jr.simulate(&input_signal);
                (result.eeg, result.fast_inhib_trace)
            }
            BandModelType::WilsonCowan(target_hz) => {
                let wc = WilsonCowanModel::for_frequency(
                    sample_rate, target_hz, input_scale as f64 * 0.01,
                );
                let result = wc.simulate(&input_signal);
                (result.eeg, result.inhib_trace)
            }
        };

        // Detrend (remove DC) before RMS normalisation
        let band_mean = band_eeg.iter().sum::<f64>() / n as f64;
        let detrended: Vec<f64> = band_eeg.iter().map(|x| x - band_mean).collect();

        let rms = (detrended.iter().map(|x| x * x).sum::<f64>() / n as f64).sqrt();
        let norm = if rms > 1e-10 { 1.0 / rms.sqrt() } else { 0.0 };

        let weight = energy_fractions[b] * tono.band_output_weights[b];
        for i in 0..n {
            combined_eeg[i] += weight * detrended[i] * norm;
        }

        if has_fast_inhib && !band_inhib.is_empty() {
            for i in 0..n {
                combined_y3[i] += weight * band_inhib[i];
            }
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
        fast_inhib_trace: combined_y3,
    }
}

/// Result of bilateral cortical simulation.
pub struct BilateralResult {
    /// Combined bilateral EEG (average of both hemispheres).
    pub combined: JansenRitResult,
    /// Left hemisphere dominant frequency (Hz).
    pub left_dominant_freq: f64,
    /// Right hemisphere dominant frequency (Hz).
    pub right_dominant_freq: f64,
    /// Left hemisphere band powers (normalised).
    pub left_band_powers: BandPowers,
    /// Right hemisphere band powers (normalised).
    pub right_band_powers: BandPowers,
    /// Hemispheric asymmetry index: (left_alpha - right_alpha) / (left_alpha + right_alpha).
    /// Positive = left alpha dominance, negative = right alpha dominance.
    pub alpha_asymmetry: f64,
}

/// Run bilateral cortical model: 2×4 parallel Jansen-Rit models (one set per
/// hemisphere) with callosal coupling.
///
/// Each hemisphere receives:
///   - contralateral ear signal × contralateral_ratio (65%)
///   - ipsilateral ear signal × (1 - contralateral_ratio) (35%)
///
/// Callosal coupling: each hemisphere receives a delayed, attenuated copy of
/// the other hemisphere's combined EEG as additional excitatory input.
///
/// `left_bands` / `right_bands`: tonotopic band signals from left/right ear.
/// `left_energy` / `right_energy`: energy fractions from left/right ear.
pub fn simulate_bilateral(
    left_bands: &[Vec<f64>; 4],
    right_bands: &[Vec<f64>; 4],
    left_energy: &[f64; 4],
    right_energy: &[f64; 4],
    bilateral: &BilateralParams,
    c: f64,
    input_scale: f64,
    sample_rate: f64,
    fast_inhib: &FastInhibParams,
    v0: f64,
) -> BilateralResult {
    let n = left_bands[0].len();
    let contra = bilateral.contralateral_ratio;
    let ipsi = 1.0 - contra;

    // Mix input for each hemisphere:
    // Right hemisphere ← 65% left ear (contra) + 35% right ear (ipsi)
    // Left hemisphere  ← 65% right ear (contra) + 35% left ear (ipsi)
    let mut rh_bands: [Vec<f64>; 4] = [
        vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n],
    ];
    let mut lh_bands: [Vec<f64>; 4] = [
        vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n],
    ];
    let mut rh_energy = [0.0_f64; 4];
    let mut lh_energy = [0.0_f64; 4];

    for b in 0..4 {
        for i in 0..n {
            rh_bands[b][i] = contra * left_bands[b][i] + ipsi * right_bands[b][i];
            lh_bands[b][i] = contra * right_bands[b][i] + ipsi * left_bands[b][i];
        }
        rh_energy[b] = contra * left_energy[b] + ipsi * right_energy[b];
        lh_energy[b] = contra * right_energy[b] + ipsi * left_energy[b];
    }

    // Normalise energy fractions
    let rh_sum: f64 = rh_energy.iter().sum();
    let lh_sum: f64 = lh_energy.iter().sum();
    if rh_sum > 1e-30 { for e in &mut rh_energy { *e /= rh_sum; } }
    if lh_sum > 1e-30 { for e in &mut lh_energy { *e /= lh_sum; } }

    // Phase 1: Run each hemisphere's tonotopic model independently.
    // This gives us the base per-hemisphere EEG and y[3] traces.
    let (rh_eeg, rh_y3) = run_hemisphere_tonotopic(
        &rh_bands, &rh_energy, &bilateral.right, c, input_scale, sample_rate, fast_inhib,
        v0,
    );
    let (lh_eeg, lh_y3) = run_hemisphere_tonotopic(
        &lh_bands, &lh_energy, &bilateral.left, c, input_scale, sample_rate, fast_inhib,
        v0,
    );

    // Phase 2: Apply callosal coupling.
    // Each hemisphere receives a delayed, attenuated version of the other's EEG
    // as additive excitatory input, then we re-run a lightweight combination.
    //
    // Rather than re-running full JR (expensive), we model the callosal effect
    // as a linear mix: the coupled EEG is the independent EEG plus a small
    // fraction of the delayed contralateral EEG. This is justified because
    // callosal input is ~10% of convergent input — a perturbative effect.
    let delay_samples = (bilateral.callosal_delay_s * sample_rate) as usize;
    let k = bilateral.callosal_coupling;

    let mut rh_coupled = vec![0.0_f64; n];
    let mut lh_coupled = vec![0.0_f64; n];

    for i in 0..n {
        let delayed_lh = if i >= delay_samples { lh_eeg[i - delay_samples] } else { 0.0 };
        let delayed_rh = if i >= delay_samples { rh_eeg[i - delay_samples] } else { 0.0 };

        rh_coupled[i] = rh_eeg[i] + k * delayed_lh;
        lh_coupled[i] = lh_eeg[i] + k * delayed_rh;
    }

    // Combined bilateral EEG: weighted by hemispheric priority
    let lw = bilateral.left_weight;
    let rw = 1.0 - lw;
    let mut combined_eeg = vec![0.0_f64; n];
    for i in 0..n {
        combined_eeg[i] = lw * lh_coupled[i] + rw * rh_coupled[i];
    }

    // Combined y[3] trace: same weighting
    let combined_y3 = if !rh_y3.is_empty() {
        (0..n).map(|i| lw * lh_y3[i] + rw * rh_y3[i]).collect()
    } else {
        Vec::new()
    };

    // Analyse each hemisphere and the combined signal
    let analysis = JansenRitModel::new(sample_rate);

    let combined_mean = combined_eeg.iter().sum::<f64>() / n as f64;
    let combined_det: Vec<f64> = combined_eeg.iter().map(|x| x - combined_mean).collect();
    let combined_bp = analysis.compute_band_powers(&combined_det);
    let combined_df = analysis.find_dominant_frequency(&combined_det);

    let lh_mean = lh_coupled.iter().sum::<f64>() / n as f64;
    let lh_det: Vec<f64> = lh_coupled.iter().map(|x| x - lh_mean).collect();
    let lh_bp = analysis.compute_band_powers(&lh_det);
    let lh_df = analysis.find_dominant_frequency(&lh_det);

    let rh_mean = rh_coupled.iter().sum::<f64>() / n as f64;
    let rh_det: Vec<f64> = rh_coupled.iter().map(|x| x - rh_mean).collect();
    let rh_bp = analysis.compute_band_powers(&rh_det);
    let rh_df = analysis.find_dominant_frequency(&rh_det);

    // Compute alpha asymmetry index
    let lh_alpha_norm = lh_bp.normalized().alpha;
    let rh_alpha_norm = rh_bp.normalized().alpha;
    let alpha_sum = lh_alpha_norm + rh_alpha_norm;
    let alpha_asymmetry = if alpha_sum > 1e-30 {
        (lh_alpha_norm - rh_alpha_norm) / alpha_sum
    } else {
        0.0
    };

    BilateralResult {
        combined: JansenRitResult {
            eeg: combined_eeg,
            band_powers: combined_bp,
            dominant_freq: combined_df,
            fast_inhib_trace: combined_y3,
        },
        left_dominant_freq: lh_df,
        right_dominant_freq: rh_df,
        left_band_powers: lh_bp.normalized(),
        right_band_powers: rh_bp.normalized(),
        alpha_asymmetry,
    }
}

/// Run one hemisphere's tonotopic JR model (4 bands) and return raw EEG + y[3] trace.
fn run_hemisphere_tonotopic(
    bands: &[Vec<f64>; 4],
    energy: &[f64; 4],
    params: &TonotopicParams,
    c: f64,
    input_scale: f64,
    sample_rate: f64,
    fast_inhib: &FastInhibParams,
    v0: f64,
) -> (Vec<f64>, Vec<f64>) {
    let n = bands[0].len();
    let mut eeg = vec![0.0_f64; n];
    let has_fast_inhib = fast_inhib.g_fast_gain > 0.0;
    let mut y3 = if has_fast_inhib { vec![0.0_f64; n] } else { Vec::new() };

    for b in 0..4 {
        // Apply per-band input gain scaling
        let gain = params.band_input_gains[b];
        let input_signal = if (gain - 1.0).abs() < 1e-6 {
            bands[b].clone()
        } else {
            bands[b].iter().map(|&x| (x * gain).min(1.0)).collect()
        };

        // Dispatch: JansenRit or WilsonCowan per band
        let (band_eeg, band_inhib) = match params.band_model_types[b] {
            BandModelType::JansenRit => {
                let (a_rate, b_rate) = params.band_rates[b];
                let (a_gain, b_gain) = params.band_gains[b];
                let band_slow_inhib = params.band_slow_inhib_ratios[b];
                let band_r = params.band_sigmoid_r[b];
                let band_v0 = params.band_v0[b];
                let band_fi = FastInhibParams {
                    g_fast_gain: fast_inhib.g_fast_gain,
                    g_fast_rate: params.band_g_fast_rate[b],
                    c5: fast_inhib.c5,
                    c6: fast_inhib.c6,
                    c7: params.band_c7[b],
                };

                let mut jr = JansenRitModel::with_wendling_params(
                    sample_rate, a_gain, b_gain, a_rate, b_rate,
                    c, params.band_offsets[b], input_scale, &band_fi,
                    band_slow_inhib, band_v0, band_r,
                );
                let c1c2_scale = params.band_c1c2_scale[b];
                if (c1c2_scale - 1.0).abs() > 1e-6 {
                    jr.scale_c1c2(c1c2_scale);
                }

                let result = jr.simulate(&input_signal);
                (result.eeg, result.fast_inhib_trace)
            }
            BandModelType::WilsonCowan(target_hz) => {
                let wc = WilsonCowanModel::for_frequency(
                    sample_rate, target_hz, input_scale as f64 * 0.01,
                );
                let result = wc.simulate(&input_signal);
                (result.eeg, result.inhib_trace)
            }
        };

        // Detrend + sqrt-compress RMS (preserves amplitude dynamics for SR)
        let band_mean = band_eeg.iter().sum::<f64>() / n as f64;
        let detrended: Vec<f64> = band_eeg.iter().map(|x| x - band_mean).collect();
        let rms = (detrended.iter().map(|x| x * x).sum::<f64>() / n as f64).sqrt();
        let norm = if rms > 1e-10 { 1.0 / rms.sqrt() } else { 0.0 };

        let weight = energy[b] * params.band_output_weights[b];
        for i in 0..n {
            eeg[i] += weight * detrended[i] * norm;
        }

        if has_fast_inhib && !band_inhib.is_empty() {
            for i in 0..n {
                y3[i] += weight * band_inhib[i];
            }
        }
    }

    (eeg, y3)
}
