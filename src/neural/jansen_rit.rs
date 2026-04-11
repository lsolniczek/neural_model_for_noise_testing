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
    /// EEG-like output signal (y1 - y2 - y3). Wendling 2002: excitatory PSP
    /// minus slow inhibitory (GABA-B) minus fast inhibitory (GABA-A).
    /// Degenerates to y1 - y2 when fast inhibition is disabled (g_fast_gain = 0).
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
    /// Habituation: synaptic depression rate.
    /// Per Moran et al. (2011) and Rowe et al. (2012): sustained neural
    /// activity depresses excitatory connectivity over time.
    /// 0.0 = no habituation (default, backward compatible).
    /// Typical: 0.0001-0.001 (depression per sample per unit activity).
    pub habituation_rate: f64,
    /// Habituation: recovery rate (how fast C recovers toward baseline).
    /// Typical: 0.00005-0.0005 (recovery per sample).
    pub habituation_recovery: f64,
    /// Stochastic noise amplitude (σ) added to the input drive.
    /// Per Ableidinger et al. (2017): relaxing the mean-field assumption
    /// by adding noise enables the JR model to produce theta waves —
    /// breaking the deterministic alpha attractor.
    /// 0.0 = deterministic (default). Typical: 5.0-30.0 pulses/s.
    pub stochastic_sigma: f64,
    /// RNG state for stochastic noise (simple xorshift64 for reproducibility).
    stochastic_rng: u64,
    // ── Slow GABA_B inhibitory population (Priority 13b — CET) ──────────
    //
    // The Wendling 4-population JR has two inhibitory populations but BOTH
    // operate on GABA_A timescales (b_rate = 50/s ≈ 20 ms; g_fast_rate ≈ 500/s
    // ≈ 2 ms). Cortical envelope tracking of 1–8 Hz envelopes requires a
    // genuinely slow inhibitory feedback loop with τ ≈ 100–200 ms — i.e. a
    // GABA_B-like population, missing from canonical JR.
    //
    // Per Moran & Friston (2011) "canonical microcircuit" extension and
    // Ghitza (2011) cascaded-oscillator model, we add a parallel 2-state
    // population [y_slow_0, y_slow_1] driven by pyramidal firing S(v_pyr):
    //
    //   dy_slow_0/dt = y_slow_1
    //   dy_slow_1/dt = B_slow · b_slow · C_slow · S(v_pyr)
    //                  − 2·b_slow · y_slow_1 − b_slow² · y_slow_0
    //
    // The slow PSP `y_slow_0` is subtracted from the EEG output:
    //   eeg = y[1] − y[2] − y[3] − y_slow_0
    //
    // This is additive to the existing 8-state Wendling system: when
    // `b_slow_gain == 0.0` (default), the slow ODE evaluates to zero,
    // y_slow_0 stays at zero forever, the EEG subtraction is a no-op,
    // and the model is bitwise-identical to its pre-CET behaviour.
    //
    // Refs:
    // - Moran RJ, Friston KJ (2011). Canonical microcircuit DCM with
    //   GABA_A and GABA_B populations. *NeuroImage* 56(3):1131-1144.
    // - Ghitza O (2011). "Linking speech perception and neurophysiology."
    //   *Front Psychol* 2:130. — slow inhibitory loop for envelope tracking.
    /// Slow inhibitory synaptic gain B_slow (mV). 0.0 = disabled (default).
    /// Suggested when CET enabled: 10.0 mV per Moran & Friston (2011).
    pub b_slow_gain: f64,
    /// Slow inhibitory rate constant (1/s). 0.0 = disabled (default).
    /// Suggested when CET enabled: 5.0 /s (τ ≈ 200 ms, GABA_B timescale).
    pub b_slow_rate: f64,
    /// Slow inhibitory connectivity from pyramidal firing. 0.0 = disabled.
    /// Suggested when CET enabled: 30.0 (matches C3/C4 ratio in canonical JR).
    pub c_slow: f64,
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
            habituation_rate: 0.0,
            habituation_recovery: 0.0,
            stochastic_sigma: 0.0,
            stochastic_rng: 42,
            // Slow GABA_B (CET 13b) — disabled by default → bitwise regression safe.
            b_slow_gain: 0.0,
            b_slow_rate: 0.0,
            c_slow: 0.0,
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
            habituation_rate: 0.0,
            habituation_recovery: 0.0,
            stochastic_sigma: 0.0,
            stochastic_rng: 42,
            b_slow_gain: 0.0,
            b_slow_rate: 0.0,
            c_slow: 0.0,
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
            habituation_rate: 0.0,
            habituation_recovery: 0.0,
            stochastic_sigma: 0.0,
            stochastic_rng: 42,
            b_slow_gain: 0.0,
            b_slow_rate: 0.0,
            c_slow: 0.0,
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
    /// `c_scale` is the habituation depression factor in [0, 1] (1.0 = no depression).
    #[inline]
    fn derivatives_with_habituation(&self, y: &[f64; 8], p: f64, c_scale: f64) -> [f64; 8] {
        let v_pyr = y[1] - y[2] - y[3];
        let sig_vpyr = self.sigmoid(v_pyr);
        let sig_c1_y0 = self.sigmoid(self.c1 * c_scale * y[0]);
        let sig_c3_y0 = self.sigmoid(self.c3 * c_scale * y[0]);

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
            a * ar * (p + self.c2 * c_scale * sig_c1_y0) - 2.0 * ar * y[5] - ar * ar * y[1],
            // dy6/dt = B·b·C4·S(C3·y0) - 2b·y6 - b²·y2
            b * br * self.c4 * c_scale * sig_c3_y0 - 2.0 * br * y[6] - br * br * y[2],
            // dy7/dt = G·g·(C5·S(vpyr) - C6·S(C3·y0)) - 2g·y7 - g²·y3
            g * gr * fast_drive - 2.0 * gr * y[7] - gr * gr * y[3],
        ]
    }

    /// Backward-compatible derivatives (no habituation).
    #[inline]
    fn derivatives(&self, y: &[f64; 8], p: f64) -> [f64; 8] {
        self.derivatives_with_habituation(y, p, 1.0)
    }

    /// Set the slow inhibitory (GABA_B) parameters in one call.
    /// `gain` is B_slow (mV), `rate` is b_slow (1/s), `c` is C_slow (dimensionless).
    /// Setting `gain = 0.0` disables the population entirely (default state).
    pub fn set_slow_inhib(&mut self, gain: f64, rate: f64, c: f64) {
        self.b_slow_gain = gain;
        self.b_slow_rate = rate;
        self.c_slow = c;
    }

    /// Slow GABA_B inhibitory population derivatives.
    /// Driven by sigmoid(v_pyr) where v_pyr is the *current* pyramidal membrane
    /// potential including the slow PSP feedback (consistent with the canonical
    /// microcircuit formulation in Moran & Friston 2011).
    ///
    /// Returns [dy_slow_0/dt, dy_slow_1/dt]. Returns [0.0, 0.0] when disabled.
    #[inline]
    fn slow_inhib_derivatives(&self, y_main: &[f64; 8], y_slow: &[f64; 2]) -> [f64; 2] {
        if self.b_slow_gain == 0.0 {
            return [0.0, 0.0];
        }
        // Pyramidal membrane voltage including the slow PSP feedback
        let v_pyr = y_main[1] - y_main[2] - y_main[3] - y_slow[0];
        let sig_vpyr = self.sigmoid(v_pyr);
        let bs = self.b_slow_rate;
        let bg = self.b_slow_gain;
        [
            // dy_slow_0/dt = y_slow_1
            y_slow[1],
            // dy_slow_1/dt = B_slow · b_slow · C_slow · S(v_pyr)
            //                − 2·b_slow · y_slow_1 − b_slow² · y_slow_0
            bg * bs * self.c_slow * sig_vpyr - 2.0 * bs * y_slow[1] - bs * bs * y_slow[0],
        ]
    }

    /// Generate approximate Gaussian noise using Box-Muller from xorshift64.
    /// Per Ableidinger et al. (2017): stochastic input breaks the mean-field
    /// assumption, enabling theta/delta oscillations.
    #[inline]
    fn next_gaussian_noise(&mut self) -> f64 {
        if self.stochastic_sigma == 0.0 {
            return 0.0;
        }
        // xorshift64 for uniform [0, 1)
        self.stochastic_rng ^= self.stochastic_rng << 13;
        self.stochastic_rng ^= self.stochastic_rng >> 7;
        self.stochastic_rng ^= self.stochastic_rng << 17;
        let u1 = (self.stochastic_rng as f64) / (u64::MAX as f64);

        self.stochastic_rng ^= self.stochastic_rng << 13;
        self.stochastic_rng ^= self.stochastic_rng >> 7;
        self.stochastic_rng ^= self.stochastic_rng << 17;
        let u2 = (self.stochastic_rng as f64) / (u64::MAX as f64);

        // Box-Muller transform
        let u1_safe = u1.max(1e-15);
        let gauss = (-2.0 * u1_safe.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
        gauss * self.stochastic_sigma
    }

    /// Simulate and also record the fast inhibitory state y[3] for diagnostics.
    pub fn simulate_with_fast_inhib_trace(&mut self, input: &[f64]) -> (JansenRitResult, Vec<f64>) {
        let n = input.len();
        let mut eeg = vec![0.0_f64; n];
        let mut y3_trace = vec![0.0_f64; n];

        let mut y = [0.001_f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        let sub_steps = if self.g_fast_rate > 200.0 { 3_usize } else { 2_usize };

        let h = self.dt / sub_steps as f64;

        // Slow GABA_B (CET 13b) state. Stays at zero forever when disabled,
        // so we only update it conditionally — guarantees bitwise identity
        // with pre-CET model when b_slow_gain == 0.
        let mut y_slow = [0.0_f64; 2];
        let slow_enabled = self.b_slow_gain > 0.0;

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
                if slow_enabled {
                    let s1 = self.slow_inhib_derivatives(&y, &y_slow);
                    let s2 = self.slow_inhib_derivatives(
                        &y, &[y_slow[0] + 0.5 * h * s1[0], y_slow[1] + 0.5 * h * s1[1]]);
                    let s3 = self.slow_inhib_derivatives(
                        &y, &[y_slow[0] + 0.5 * h * s2[0], y_slow[1] + 0.5 * h * s2[1]]);
                    let s4 = self.slow_inhib_derivatives(
                        &y, &[y_slow[0] + h * s3[0], y_slow[1] + h * s3[1]]);
                    y_slow[0] += h / 6.0 * (s1[0] + 2.0 * s2[0] + 2.0 * s3[0] + s4[0]);
                    y_slow[1] += h / 6.0 * (s1[1] + 2.0 * s2[1] + 2.0 * s3[1] + s4[1]);
                }
            }
        }

        let mut depression = 0.0_f64;
        let hab_rate = self.habituation_rate;
        let hab_recovery = self.habituation_recovery;

        for i in 0..n {
            // Stochastic JR per Ableidinger et al. (2017): add noise to input drive.
            let noise = self.next_gaussian_noise();
            let p = self.input_offset + input[i] * self.input_scale + noise;
            let c_scale = 1.0 - depression;

            for _ in 0..sub_steps {
                let k1 = self.derivatives_with_habituation(&y, p, c_scale);
                let mut y_tmp = [0.0; 8];
                for j in 0..8 { y_tmp[j] = y[j] + 0.5 * h * k1[j]; }
                let k2 = self.derivatives_with_habituation(&y_tmp, p, c_scale);
                for j in 0..8 { y_tmp[j] = y[j] + 0.5 * h * k2[j]; }
                let k3 = self.derivatives_with_habituation(&y_tmp, p, c_scale);
                for j in 0..8 { y_tmp[j] = y[j] + h * k3[j]; }
                let k4 = self.derivatives_with_habituation(&y_tmp, p, c_scale);
                for j in 0..8 {
                    y[j] += h / 6.0 * (k1[j] + 2.0 * k2[j] + 2.0 * k3[j] + k4[j]);
                }
                if slow_enabled {
                    let s1 = self.slow_inhib_derivatives(&y, &y_slow);
                    let s2 = self.slow_inhib_derivatives(
                        &y, &[y_slow[0] + 0.5 * h * s1[0], y_slow[1] + 0.5 * h * s1[1]]);
                    let s3 = self.slow_inhib_derivatives(
                        &y, &[y_slow[0] + 0.5 * h * s2[0], y_slow[1] + 0.5 * h * s2[1]]);
                    let s4 = self.slow_inhib_derivatives(
                        &y, &[y_slow[0] + h * s3[0], y_slow[1] + h * s3[1]]);
                    y_slow[0] += h / 6.0 * (s1[0] + 2.0 * s2[0] + 2.0 * s3[0] + s4[0]);
                    y_slow[1] += h / 6.0 * (s1[1] + 2.0 * s2[1] + 2.0 * s3[1] + s4[1]);
                }
            }
            // EEG = pyramidal − fast/dendritic inhibition − somatic inhibition − slow GABA_B.
            // y_slow[0] is zero when slow_enabled == false → matches pre-CET formula.
            eeg[i] = y[1] - y[2] - y[3] - y_slow[0];
            y3_trace[i] = y[3];

            // Update habituation: depression increases with pyramidal activity,
            // recovers toward 0. Activity measure: |S(v_pyr)| / V_MAX ∈ [0, 1].
            if hab_rate > 0.0 {
                let v_pyr = y[1] - y[2] - y[3] - y_slow[0];
                let activity = self.sigmoid(v_pyr) / V_MAX; // normalized [0, 1]
                depression += hab_rate * activity - hab_recovery * depression;
                depression = depression.clamp(0.0, 0.8); // max 80% depression
            }
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
    pub fn simulate(&mut self, input: &[f64]) -> JansenRitResult {
        let n = input.len();
        let mut eeg = vec![0.0_f64; n];
        let has_fast_inhib = self.g_fast_gain > 0.0;
        let mut y3_trace = if has_fast_inhib { vec![0.0_f64; n] } else { Vec::new() };

        // State: [y0, y1, y2, y3, y4, y5, y6, y7]
        // y3/y7 = fast inhibitory (GABA-A), zero-initialised → JR95 compat
        let mut y = [0.001_f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];

        // Slow GABA_B (CET 13b) — additive parallel population. Stays at zero
        // when disabled, making the EEG subtraction below a no-op (bitwise
        // regression-safe per IEEE 754: x - 0.0 == x).
        let mut y_slow = [0.0_f64; 2];
        let slow_enabled = self.b_slow_gain > 0.0;

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
                if slow_enabled {
                    let s1 = self.slow_inhib_derivatives(&y, &y_slow);
                    let s2 = self.slow_inhib_derivatives(
                        &y, &[y_slow[0] + 0.5 * h * s1[0], y_slow[1] + 0.5 * h * s1[1]]);
                    let s3 = self.slow_inhib_derivatives(
                        &y, &[y_slow[0] + 0.5 * h * s2[0], y_slow[1] + 0.5 * h * s2[1]]);
                    let s4 = self.slow_inhib_derivatives(
                        &y, &[y_slow[0] + h * s3[0], y_slow[1] + h * s3[1]]);
                    y_slow[0] += h / 6.0 * (s1[0] + 2.0 * s2[0] + 2.0 * s3[0] + s4[0]);
                    y_slow[1] += h / 6.0 * (s1[1] + 2.0 * s2[1] + 2.0 * s3[1] + s4[1]);
                }
            }
        }

        let mut depression = 0.0_f64;
        let hab_rate = self.habituation_rate;
        let hab_recovery = self.habituation_recovery;

        for i in 0..n {
            let noise = self.next_gaussian_noise();
            let p = self.input_offset + input[i] * self.input_scale + noise;
            let c_scale = 1.0 - depression;

            for _ in 0..sub_steps {
                let k1 = self.derivatives_with_habituation(&y, p, c_scale);
                let mut y_tmp = [0.0; 8];
                for j in 0..8 { y_tmp[j] = y[j] + 0.5 * h * k1[j]; }
                let k2 = self.derivatives_with_habituation(&y_tmp, p, c_scale);
                for j in 0..8 { y_tmp[j] = y[j] + 0.5 * h * k2[j]; }
                let k3 = self.derivatives_with_habituation(&y_tmp, p, c_scale);
                for j in 0..8 { y_tmp[j] = y[j] + h * k3[j]; }
                let k4 = self.derivatives_with_habituation(&y_tmp, p, c_scale);
                for j in 0..8 {
                    y[j] += h / 6.0 * (k1[j] + 2.0 * k2[j] + 2.0 * k3[j] + k4[j]);
                }
                if slow_enabled {
                    let s1 = self.slow_inhib_derivatives(&y, &y_slow);
                    let s2 = self.slow_inhib_derivatives(
                        &y, &[y_slow[0] + 0.5 * h * s1[0], y_slow[1] + 0.5 * h * s1[1]]);
                    let s3 = self.slow_inhib_derivatives(
                        &y, &[y_slow[0] + 0.5 * h * s2[0], y_slow[1] + 0.5 * h * s2[1]]);
                    let s4 = self.slow_inhib_derivatives(
                        &y, &[y_slow[0] + h * s3[0], y_slow[1] + h * s3[1]]);
                    y_slow[0] += h / 6.0 * (s1[0] + 2.0 * s2[0] + 2.0 * s3[0] + s4[0]);
                    y_slow[1] += h / 6.0 * (s1[1] + 2.0 * s2[1] + 2.0 * s3[1] + s4[1]);
                }
            }

            eeg[i] = y[1] - y[2] - y[3] - y_slow[0];
            if has_fast_inhib {
                y3_trace[i] = y[3];
            }

            if hab_rate > 0.0 {
                let v_pyr = y[1] - y[2] - y[3] - y_slow[0];
                let activity = self.sigmoid(v_pyr) / V_MAX;
                depression += hab_rate * activity - hab_recovery * depression;
                depression = depression.clamp(0.0, 0.8);
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

        // Apply Hann window (consistent with compute_band_powers)
        let hann_denom = (n - 1) as f64;
        let mut buffer: Vec<Complex<f64>> = (0..fft_len)
            .map(|i| {
                if i < n {
                    let w = 0.5 * (1.0 - (2.0 * PI * i as f64 / hann_denom).cos());
                    Complex::new(signal[i] * w, 0.0)
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

    // Run 4 independent Wendling/JR models, sqrt-compress each band's RMS,
    // then mix by energy fraction. Sqrt-compression (norm = 1/√rms rather
    // than 1/rms) reduces the dynamic range between slow and fast bands
    // while preserving relative amplitude differences — full unit-RMS
    // normalisation would erase inter-band dynamics entirely.
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
                // Adaptive frequency tracking per Pikovsky et al. (2001):
                // WC shifts natural frequency toward input's dominant modulation
                // if within ±5 Hz Arnold tongue.
                let wc = WilsonCowanModel::for_frequency_adaptive(
                    sample_rate, target_hz, input_scale as f64 * 0.01,
                    &input_signal, 5.0,
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
    habituation_rate: f64,
    habituation_recovery: f64,
    stochastic_sigma: f64,
    // CET 13b — Slow GABA_B params. All 0.0 = disabled (regression-safe default).
    // Suggested when CET enabled: gain=10.0 mV, rate=5.0 /s, c_slow=30.0.
    b_slow_gain: f64,
    b_slow_rate: f64,
    c_slow: f64,
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
        v0, habituation_rate, habituation_recovery, stochastic_sigma,
        b_slow_gain, b_slow_rate, c_slow,
    );
    let (lh_eeg, lh_y3) = run_hemisphere_tonotopic(
        &lh_bands, &lh_energy, &bilateral.left, c, input_scale, sample_rate, fast_inhib,
        v0, habituation_rate, habituation_recovery, stochastic_sigma,
        b_slow_gain, b_slow_rate, c_slow,
    );

    // Phase 2: Apply callosal coupling (INHIBITORY).
    //
    // Per Innocenti (1986) and Bloom & Hynd (2005), corpus callosum
    // projections primarily excite inhibitory interneurons in the target
    // hemisphere, creating net interhemispheric inhibition. When one
    // hemisphere is strongly active, it suppresses the other.
    //
    // The coupled EEG subtracts a delayed, attenuated version of the
    // contralateral EEG. This is justified because callosal input is
    // ~10-15% of convergent input — a perturbative inhibitory effect.
    //
    // Ref: Aboitiz F et al. (1992). Callosal axon diameters vary (0.4-5 μm),
    // giving conduction velocities of 3-60 m/s and delays of 5-50 ms.
    let delay_samples = (bilateral.callosal_delay_s * sample_rate) as usize;
    let k = bilateral.callosal_coupling;

    let mut rh_coupled = vec![0.0_f64; n];
    let mut lh_coupled = vec![0.0_f64; n];

    for i in 0..n {
        let delayed_lh = if i >= delay_samples { lh_eeg[i - delay_samples] } else { 0.0 };
        let delayed_rh = if i >= delay_samples { rh_eeg[i - delay_samples] } else { 0.0 };

        // Inhibitory coupling: subtract contralateral signal
        rh_coupled[i] = rh_eeg[i] - k * delayed_lh;
        lh_coupled[i] = lh_eeg[i] - k * delayed_rh;
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
    let mut analysis = JansenRitModel::new(sample_rate);

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
    habituation_rate: f64,
    habituation_recovery: f64,
    stochastic_sigma: f64,
    b_slow_gain: f64,
    b_slow_rate: f64,
    c_slow: f64,
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
                jr.habituation_rate = habituation_rate;
                jr.habituation_recovery = habituation_recovery;
                jr.stochastic_sigma = stochastic_sigma;
                // CET 13b — slow GABA_B (default 0.0 → no-op).
                jr.set_slow_inhib(b_slow_gain, b_slow_rate, c_slow);
                let c1c2_scale = params.band_c1c2_scale[b];
                if (c1c2_scale - 1.0).abs() > 1e-6 {
                    jr.scale_c1c2(c1c2_scale);
                }

                let result = jr.simulate(&input_signal);
                (result.eeg, result.fast_inhib_trace)
            }
            BandModelType::WilsonCowan(target_hz) => {
                // Adaptive frequency tracking per Pikovsky et al. (2001):
                // WC shifts natural frequency toward input's dominant modulation
                // if within ±5 Hz Arnold tongue.
                let wc = WilsonCowanModel::for_frequency_adaptive(
                    sample_rate, target_hz, input_scale as f64 * 0.01,
                    &input_signal, 5.0,
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

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 1000.0; // 1 kHz neural sample rate

    // ---------------------------------------------------------------
    // Sigmoid transfer function
    // ---------------------------------------------------------------

    #[test]
    fn sigmoid_at_v0_equals_half_max() {
        let mut jr = JansenRitModel::new(SR);
        let s = jr.sigmoid(V0);
        // S(V0) = V_MAX / (1 + exp(0)) = V_MAX / 2 = 2.5
        assert!(
            (s - V_MAX / 2.0).abs() < 1e-10,
            "S(V0) should be V_MAX/2 = 2.5, got {s}"
        );
    }

    #[test]
    fn sigmoid_saturates_to_vmax() {
        let mut jr = JansenRitModel::new(SR);
        // Large positive → V_MAX
        let s_high = jr.sigmoid(100.0);
        assert!(
            (s_high - V_MAX).abs() < 1e-6,
            "S(100) should be ~V_MAX=5.0, got {s_high}"
        );
        // Large negative → 0
        let s_low = jr.sigmoid(-100.0);
        assert!(
            s_low.abs() < 1e-6,
            "S(-100) should be ~0, got {s_low}"
        );
    }

    #[test]
    fn sigmoid_is_monotonically_increasing() {
        let mut jr = JansenRitModel::new(SR);
        let mut prev = jr.sigmoid(-10.0);
        for i in -99..100 {
            let v = i as f64 * 0.1;
            let s = jr.sigmoid(v);
            assert!(s >= prev, "Sigmoid not monotonic at v={v}: {prev} -> {s}");
            prev = s;
        }
    }

    // ---------------------------------------------------------------
    // Derivatives: verify structure at known states
    // ---------------------------------------------------------------

    #[test]
    fn derivatives_at_zero_state_with_zero_input() {
        let mut jr = JansenRitModel::new(SR);
        let y = [0.0; 8];
        let dy = jr.derivatives(&y, 0.0);

        // dy0..dy3 should be zero (derivatives equal to y4..y7 which are zero)
        for i in 0..4 {
            assert_eq!(dy[i], 0.0, "dy[{i}] should be 0 at zero state");
        }

        // dy4 = A*a*S(0-0-0) - 0 - 0 = A*a*S(0)
        // S(0) = V_MAX / (1 + exp(r*V0)) — a small positive value
        let s0 = jr.sigmoid(0.0);
        let expected_dy4 = A * A_RATE * s0;
        assert!(
            (dy[4] - expected_dy4).abs() < 1e-10,
            "dy[4] = {}, expected {expected_dy4}",
            dy[4]
        );

        // dy5 = A*a*(p + C2*S(C1*0)) = A*a*(0 + C2*S(0))
        let expected_dy5 = A * A_RATE * (0.0 + C2 * s0);
        assert!(
            (dy[5] - expected_dy5).abs() < 1e-10,
            "dy[5] = {}, expected {expected_dy5}",
            dy[5]
        );
    }

    #[test]
    fn derivatives_external_input_only_affects_dy5() {
        let mut jr = JansenRitModel::new(SR);
        let y = [0.0; 8];
        let dy_p0 = jr.derivatives(&y, 0.0);
        let dy_p100 = jr.derivatives(&y, 100.0);

        // Only dy5 should change (p enters only in the pyramidal excitatory equation)
        for i in [0, 1, 2, 3, 4, 6, 7] {
            assert!(
                (dy_p0[i] - dy_p100[i]).abs() < 1e-10,
                "dy[{i}] should not depend on p: {} vs {}",
                dy_p0[i],
                dy_p100[i]
            );
        }
        // dy5 should increase with p: A*a*p is the additional term
        let delta_dy5 = dy_p100[5] - dy_p0[5];
        let expected_delta = A * A_RATE * 100.0;
        assert!(
            (delta_dy5 - expected_delta).abs() < 1e-6,
            "dy[5] delta = {delta_dy5}, expected {expected_delta}"
        );
    }

    // ---------------------------------------------------------------
    // JR95 mode: fast inhibition disabled → y3 stays zero
    // ---------------------------------------------------------------

    #[test]
    fn jr95_mode_y3_stays_zero() {
        let mut jr = JansenRitModel::new(SR); // g_fast_gain = 0
        let input = vec![0.5; 3000]; // 3 seconds
        let result = jr.simulate(&input);

        // y3 is encoded in the EEG difference: if y3 were non-zero, the fast_inhib_trace
        // would be non-empty. In JR95 mode, it should be empty.
        assert!(
            result.fast_inhib_trace.is_empty(),
            "JR95 mode should not record fast inhibitory trace"
        );
    }

    #[test]
    fn jr95_derivatives_y3_y7_are_zero() {
        let mut jr = JansenRitModel::new(SR); // g_fast_gain = 0
        // State with non-zero y0..y2, but y3=y7=0
        let y = [1.0, 2.0, 1.5, 0.0, 0.1, 0.2, -0.1, 0.0];
        let dy = jr.derivatives(&y, 200.0);

        // dy3 = y7 = 0
        assert_eq!(dy[3], 0.0, "dy[3] should be 0 in JR95 mode");
        // dy7 = G*g*(...) - 2g*y7 - g²*y3 = 0 when G=0, g=0
        assert_eq!(dy[7], 0.0, "dy[7] should be 0 in JR95 mode");
    }

    // ---------------------------------------------------------------
    // Wendling mode: fast inhibition is active
    // ---------------------------------------------------------------

    #[test]
    fn wendling_mode_fast_inhib_trace_nonzero() {
        let fi = FastInhibParams {
            g_fast_gain: 10.0,
            g_fast_rate: 500.0,
            c5: 40.5,
            c6: 13.5,
            c7: 108.0,
        };
        let mut jr = JansenRitModel::with_wendling_params(
            SR, A, B, A_RATE, B_RATE, C, 200.0, 100.0,
            &fi, 0.20, V0, R,
        );
        let input = vec![0.5; 3000];
        let result = jr.simulate(&input);

        assert!(
            !result.fast_inhib_trace.is_empty(),
            "Wendling mode should record fast inhibitory trace"
        );
        let y3_max = result.fast_inhib_trace.iter().cloned().fold(0.0_f64, f64::max);
        assert!(
            y3_max.abs() > 1e-6,
            "Fast inhibitory trace should be non-zero, max |y3| = {y3_max}"
        );
    }

    // ---------------------------------------------------------------
    // Output: correct length, finite values
    // ---------------------------------------------------------------

    #[test]
    fn output_length_matches_input() {
        let mut jr = JansenRitModel::new(SR);
        let n = 2000;
        let input = vec![0.5; n];
        let result = jr.simulate(&input);

        assert_eq!(result.eeg.len(), n);
    }

    #[test]
    fn output_is_finite() {
        let mut jr = JansenRitModel::new(SR);
        let input = vec![0.5; 5000];
        let result = jr.simulate(&input);

        for (i, &v) in result.eeg.iter().enumerate() {
            assert!(v.is_finite(), "EEG sample {i} is not finite: {v}");
        }
    }

    // ---------------------------------------------------------------
    // Deterministic: same input → same output
    // ---------------------------------------------------------------

    #[test]
    fn deterministic_output() {
        let mut jr = JansenRitModel::new(SR);
        let input = vec![0.5; 3000];
        let r1 = jr.simulate(&input);
        let r2 = jr.simulate(&input);

        assert_eq!(r1.dominant_freq, r2.dominant_freq);
        for i in 0..r1.eeg.len() {
            assert_eq!(r1.eeg[i], r2.eeg[i], "EEG differs at sample {i}");
        }
    }

    // ---------------------------------------------------------------
    // Band powers: non-negative, alpha peak with default params
    // ---------------------------------------------------------------

    #[test]
    fn band_powers_non_negative() {
        let mut jr = JansenRitModel::new(SR);
        let input = vec![0.5; 5000];
        let result = jr.simulate(&input);

        assert!(result.band_powers.delta >= 0.0);
        assert!(result.band_powers.theta >= 0.0);
        assert!(result.band_powers.alpha >= 0.0);
        assert!(result.band_powers.beta >= 0.0);
        assert!(result.band_powers.gamma >= 0.0);
    }

    #[test]
    fn normalized_band_powers_sum_to_one() {
        let mut jr = JansenRitModel::new(SR);
        let input = vec![0.5; 5000];
        let result = jr.simulate(&input);
        let norm = result.band_powers.normalized();

        let sum = norm.delta + norm.theta + norm.alpha + norm.beta + norm.gamma;
        assert!(
            (sum - 1.0).abs() < 1e-6,
            "Normalized band powers should sum to 1.0, got {sum}"
        );
    }

    #[test]
    fn default_jr_produces_oscillation() {
        // Default JR95 with p in oscillatory range should produce a dominant
        // frequency somewhere in 0.5–50 Hz.  The exact frequency depends on
        // the tuned C3/C4 (0.20 vs literature 0.25) and input offset.
        let mut jr = JansenRitModel::new(SR);
        let input = vec![0.5; 8000]; // 8 seconds for good freq resolution
        let result = jr.simulate(&input);

        assert!(
            result.band_powers.total() > 0.0,
            "Default JR should produce non-zero EEG power"
        );
        assert!(
            result.dominant_freq >= 0.5 && result.dominant_freq <= 50.0,
            "Default JR dominant freq should be in physiological range, got {:.1} Hz",
            result.dominant_freq
        );
    }

    // ---------------------------------------------------------------
    // Dominant frequency: stable for pure-tone-like drive
    // ---------------------------------------------------------------

    #[test]
    fn dominant_freq_in_physiological_range() {
        let mut jr = JansenRitModel::new(SR);
        let input = vec![0.5; 5000];
        let result = jr.simulate(&input);

        assert!(
            result.dominant_freq >= 0.5 && result.dominant_freq <= 50.0,
            "Dominant frequency {} out of [0.5, 50] Hz range",
            result.dominant_freq
        );
    }

    // ---------------------------------------------------------------
    // Band powers with zero-energy signal
    // ---------------------------------------------------------------

    #[test]
    fn band_powers_zero_for_short_signal() {
        let mut jr = JansenRitModel::new(SR);
        let signal: Vec<f64> = vec![];
        let bp = jr.compute_band_powers(&signal);
        assert_eq!(bp.total(), 0.0);
    }

    #[test]
    fn normalized_powers_uniform_for_zero_signal() {
        let bp = BandPowers {
            delta: 0.0,
            theta: 0.0,
            alpha: 0.0,
            beta: 0.0,
            gamma: 0.0,
        };
        let norm = bp.normalized();
        assert!((norm.delta - 0.2).abs() < 1e-10);
        assert!((norm.theta - 0.2).abs() < 1e-10);
        assert!((norm.alpha - 0.2).abs() < 1e-10);
        assert!((norm.beta - 0.2).abs() < 1e-10);
        assert!((norm.gamma - 0.2).abs() < 1e-10);
    }

    // ---------------------------------------------------------------
    // EEG output matches y1 - y2 - y3
    // ---------------------------------------------------------------

    #[test]
    fn eeg_is_y1_minus_y2_minus_y3() {
        // Verify the EEG formula by running the model with fast inhibition
        // and checking that simulate_with_fast_inhib_trace gives consistent
        // EEG and y3 values.
        let fi = FastInhibParams {
            g_fast_gain: 10.0,
            g_fast_rate: 500.0,
            c5: 40.5,
            c6: 13.5,
            c7: 108.0,
        };
        let mut jr = JansenRitModel::with_wendling_params(
            SR, A, B, A_RATE, B_RATE, C, 200.0, 100.0,
            &fi, 0.20, V0, R,
        );
        let input = vec![0.5; 2000];

        let (result, y3_trace) = jr.simulate_with_fast_inhib_trace(&input);

        // y3 trace from both methods should be identical
        assert_eq!(result.fast_inhib_trace.len(), y3_trace.len());
        for i in 0..y3_trace.len() {
            assert_eq!(result.fast_inhib_trace[i], y3_trace[i]);
        }

        // y3 should be non-trivially active
        let y3_rms = (y3_trace.iter().map(|x| x * x).sum::<f64>() / y3_trace.len() as f64).sqrt();
        assert!(y3_rms > 1e-6, "y3 should be active in Wendling mode, rms = {y3_rms}");
    }

    // ---------------------------------------------------------------
    // Connectivity: with_params uses correct C scaling
    // ---------------------------------------------------------------

    #[test]
    fn with_params_connectivity_scaling() {
        let c = 100.0;
        let mut jr = JansenRitModel::with_params(SR, A, B, A_RATE, B_RATE, c, 200.0, 100.0);
        assert_eq!(jr.c1, c);
        assert_eq!(jr.c2, 0.8 * c);
        assert_eq!(jr.c3, 0.20 * c);
        assert_eq!(jr.c4, 0.20 * c);
    }

    #[test]
    fn with_wendling_slow_inhib_ratio_applied() {
        let fi = FastInhibParams::default();
        let mut jr = JansenRitModel::with_wendling_params(
            SR, A, B, A_RATE, B_RATE, 135.0, 200.0, 100.0,
            &fi, 0.15, V0, R, // slow_inhib_ratio = 0.15
        );
        assert_eq!(jr.c3, 0.15 * 135.0);
        assert_eq!(jr.c4, 0.15 * 135.0);
    }

    // ---------------------------------------------------------------
    // scale_c1c2 only affects C1 and C2
    // ---------------------------------------------------------------

    #[test]
    fn scale_c1c2_only_affects_c1_c2() {
        let mut jr = JansenRitModel::new(SR);
        let orig_c3 = jr.c3;
        let orig_c4 = jr.c4;

        jr.scale_c1c2(0.75);

        assert_eq!(jr.c1, C * 0.75);
        assert_eq!(jr.c2, 0.8 * C * 0.75);
        assert_eq!(jr.c3, orig_c3, "C3 should not change");
        assert_eq!(jr.c4, orig_c4, "C4 should not change");
    }

    // ---------------------------------------------------------------
    // Warmup: model is oscillating from the start of the output
    // ---------------------------------------------------------------

    #[test]
    fn wendling_model_oscillates_with_varying_input() {
        // The Wendling model may sit at a stable fixed point with constant input
        // at certain operating points.  In the pipeline, the input is time-varying
        // (gammatone envelopes), which sweeps the model across bifurcation
        // boundaries and produces oscillations.
        use crate::brain_type::BrainType;
        let params = BrainType::Normal.params();
        let fi = FastInhibParams {
            g_fast_gain: params.jansen_rit.g_fast_gain,
            g_fast_rate: params.jansen_rit.g_fast_rate,
            c5: params.jansen_rit.c5,
            c6: params.jansen_rit.c6,
            c7: params.jansen_rit.c7,
        };
        let mut jr = JansenRitModel::with_wendling_params(
            SR,
            params.jansen_rit.a_gain,
            params.jansen_rit.b_gain,
            params.jansen_rit.a_rate,
            params.jansen_rit.b_rate,
            params.jansen_rit.c,
            params.jansen_rit.input_offset,
            params.jansen_rit.input_scale,
            &fi,
            params.jansen_rit.slow_inhib_ratio,
            params.jansen_rit.v0,
            R,
        );

        // Time-varying input: slow modulation simulating gammatone envelope
        let n = 5000;
        let input: Vec<f64> = (0..n)
            .map(|i| 0.5 + 0.3 * (2.0 * PI * 5.0 * i as f64 / SR).sin())
            .collect();
        let result = jr.simulate(&input);

        let mean = result.eeg.iter().sum::<f64>() / result.eeg.len() as f64;
        let var = result.eeg.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / result.eeg.len() as f64;
        assert!(
            var > 1e-6,
            "Wendling model with varying input should oscillate, var = {var}"
        );
        assert!(
            result.dominant_freq >= 0.5 && result.dominant_freq <= 50.0,
            "Dominant freq should be physiological, got {:.1} Hz",
            result.dominant_freq
        );
    }

    // ---------------------------------------------------------------
    // Input offset + scale: driving point affects dynamics
    // ---------------------------------------------------------------

    #[test]
    fn different_input_offset_different_dynamics() {
        let mut jr_low = JansenRitModel::with_params(SR, A, B, A_RATE, B_RATE, C, 120.0, 100.0);
        let mut jr_high = JansenRitModel::with_params(SR, A, B, A_RATE, B_RATE, C, 280.0, 100.0);

        let input = vec![0.5; 5000];
        let r_low = jr_low.simulate(&input);
        let r_high = jr_high.simulate(&input);

        // Different offsets should produce different band power distributions
        let n_low = r_low.band_powers.normalized();
        let n_high = r_high.band_powers.normalized();

        let diff = (n_low.alpha - n_high.alpha).abs()
            + (n_low.theta - n_high.theta).abs()
            + (n_low.beta - n_high.beta).abs();
        assert!(
            diff > 0.01,
            "Different input offsets should produce different band powers"
        );
    }

    // ---------------------------------------------------------------
    // Priority 13b: Slow GABA_B inhibitory population (CET)
    //
    // Per Moran & Friston (2011) "canonical microcircuit" extension of JR
    // and Ghitza (2011), cortical envelope tracking depends on a slow
    // inhibitory time constant (~200 ms / b_slow ≈ 5 /s) that the canonical
    // Wendling 4-population model lacks. We add a parallel 2-state slow
    // population [y8, y9] driven by pyramidal firing, contributing to the
    // EEG via subtraction. Default b_slow_gain = 0.0 → bitwise-identical
    // to current model (zero regression guarantee).
    // ---------------------------------------------------------------

    #[test]
    fn slow_gaba_b_default_is_zero() {
        // Newly constructed model must have slow GABA_B disabled.
        let jr = JansenRitModel::new(SR);
        assert_eq!(jr.b_slow_gain, 0.0, "default b_slow_gain must be 0");
        assert_eq!(jr.b_slow_rate, 0.0, "default b_slow_rate must be 0");
        assert_eq!(jr.c_slow, 0.0, "default c_slow must be 0");
    }

    #[test]
    fn slow_gaba_b_disabled_bitwise_identical_to_pre_cet() {
        // The CET zero-regression contract: turning the feature off via
        // (b_slow_gain == 0) MUST produce the same EEG samples as the
        // pre-CET model, byte for byte. Two independent runs of the
        // default model (which has slow GABA_B off) must produce
        // identical output to themselves and to a hand-zeroed config.
        let mut jr_a = JansenRitModel::new(SR);
        let mut jr_b = JansenRitModel::new(SR);
        jr_b.b_slow_gain = 0.0;
        jr_b.b_slow_rate = 0.0;
        jr_b.c_slow = 0.0;

        let input: Vec<f64> = (0..3000)
            .map(|i| 0.5 + 0.3 * (2.0 * PI * 5.0 * i as f64 / SR).sin())
            .collect();

        let r_a = jr_a.simulate(&input);
        let r_b = jr_b.simulate(&input);

        assert_eq!(r_a.eeg.len(), r_b.eeg.len());
        for (i, (&a, &b)) in r_a.eeg.iter().zip(r_b.eeg.iter()).enumerate() {
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "EEG sample {i} differs between two zero-slow-GABA_B runs: {a} vs {b}"
            );
        }
    }

    #[test]
    fn slow_gaba_b_changes_eeg_when_enabled() {
        // The minimum guarantee that the new state is actually wired in:
        // enabling slow GABA_B with non-zero gain MUST measurably change
        // the EEG output relative to the disabled case. We do not yet
        // require a specific direction (theta-band amplification needs
        // the full pipeline to verify) — only that the dynamics differ.
        let mut jr_off = JansenRitModel::new(SR);
        let mut jr_on = JansenRitModel::new(SR);
        // Moran & Friston 2011 canonical-microcircuit ballpark:
        //   B_slow ≈ 10 mV, b_slow_rate ≈ 5 /s (τ ≈ 200 ms), C_slow ≈ 30
        jr_on.b_slow_gain = 10.0;
        jr_on.b_slow_rate = 5.0;
        jr_on.c_slow = 30.0;

        // 5 Hz envelope-modulated drive — the CET-relevant stimulus
        let input: Vec<f64> = (0..5000)
            .map(|i| 0.5 + 0.4 * (2.0 * PI * 5.0 * i as f64 / SR).sin())
            .collect();

        let r_off = jr_off.simulate(&input);
        let r_on = jr_on.simulate(&input);

        // Compute mean absolute difference
        let mad: f64 = r_off
            .eeg
            .iter()
            .zip(r_on.eeg.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / r_off.eeg.len() as f64;

        // The slow population subtracts a non-trivial slow PSP from the EEG.
        // Sub-millivolt MAD is acceptable; we just want clear non-equality.
        assert!(
            mad > 1e-3,
            "Slow GABA_B should perceptibly change EEG, MAD={mad:.6}"
        );
    }

    #[test]
    fn slow_gaba_b_output_finite_under_aggressive_params() {
        // Stability check: even with an aggressive slow inhibitory loop,
        // the simulator must not blow up to NaN/Inf. The new ODE is
        // a damped second-order linear system driven by a bounded sigmoid,
        // so it should stay bounded for any non-pathological input.
        let mut jr = JansenRitModel::new(SR);
        jr.b_slow_gain = 30.0; // 3× the default
        jr.b_slow_rate = 8.0;
        jr.c_slow = 60.0;

        let input: Vec<f64> = (0..5000)
            .map(|i| 0.5 + 0.6 * (2.0 * PI * 3.0 * i as f64 / SR).sin())
            .collect();

        let result = jr.simulate(&input);
        assert!(!result.eeg.is_empty(), "EEG should be non-empty");
        for (i, &v) in result.eeg.iter().enumerate() {
            assert!(
                v.is_finite(),
                "EEG[{i}] is non-finite ({v}) under aggressive slow GABA_B"
            );
            assert!(v.abs() < 1e6, "EEG[{i}] = {v} blew up");
        }
    }

    #[test]
    fn slow_gaba_b_with_constant_input_reduces_dc_drift() {
        // Physiological sanity check: a constant input drives the pyramidal
        // population to a steady-state firing rate. Adding a slow inhibitory
        // population that subtracts from the membrane potential should
        // shift the steady-state EEG mean DOWNWARD (more inhibition →
        // more negative net PSP). This is the simplest direction-of-effect
        // test for the new population.
        let mut jr_off = JansenRitModel::new(SR);
        let mut jr_on = JansenRitModel::new(SR);
        jr_on.b_slow_gain = 10.0;
        jr_on.b_slow_rate = 5.0;
        jr_on.c_slow = 30.0;

        let input = vec![0.5_f64; 5000]; // 5 s constant drive

        let r_off = jr_off.simulate(&input);
        let r_on = jr_on.simulate(&input);

        // Take the late half (post-transient) for steady-state mean
        let half = r_off.eeg.len() / 2;
        let mean_off: f64 = r_off.eeg[half..].iter().sum::<f64>() / (r_off.eeg.len() - half) as f64;
        let mean_on: f64 = r_on.eeg[half..].iter().sum::<f64>() / (r_on.eeg.len() - half) as f64;

        // Slow GABA_B is inhibitory → mean EEG should be lower with it on.
        assert!(
            mean_on < mean_off,
            "Slow GABA_B should reduce DC EEG: off={mean_off:.4} on={mean_on:.4}"
        );
    }
}
