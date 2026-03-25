/// FitzHugh-Nagumo (FHN) single-neuron model.
///
/// A 2-variable simplification of Hodgkin-Huxley that captures the essential
/// dynamics of neuronal excitability: resting state, threshold, spiking,
/// and recovery.
///
/// Equations:
///   dv/dt = v - v³/3 - w + I(t)
///   dw/dt = ε(v + a - b·w)
///
/// Where:
///   v = membrane potential (fast variable)
///   w = recovery variable (slow variable)
///   I(t) = external input current (from auditory model)
///   ε = time-scale separation (small → slow recovery)
///   a, b = shape parameters

/// Standard FHN parameters.
const DEFAULT_A: f64 = 0.7;
const DEFAULT_B: f64 = 0.8;
const DEFAULT_EPSILON: f64 = 0.08;

/// Spike detection threshold (v crosses this upward → spike).
const SPIKE_THRESHOLD: f64 = 1.0;

pub struct FhnModel {
    pub a: f64,
    pub b: f64,
    pub epsilon: f64,
    /// Internal time step for RK4 integration (seconds).
    dt: f64,
    /// Simulation sample rate (how many input samples per second).
    sample_rate: f64,
    /// Time-scale factor: maps real time to model time.
    /// Standard FHN dynamics are slow (period ~12s at ε=0.08).
    /// A time_scale of ~100 maps 1 model unit to ~10ms (membrane τ),
    /// making the FHN respond to EEG-rate (~5-30 Hz) input.
    time_scale: f64,
}

/// Result of simulating the FHN model on an input signal.
pub struct FhnResult {
    /// Membrane potential trace.
    pub voltage: Vec<f64>,
    /// Recovery variable trace.
    pub recovery: Vec<f64>,
    /// Times of detected spikes (in sample indices).
    pub spike_times: Vec<usize>,
    /// Mean firing rate (spikes/second).
    pub firing_rate: f64,
    /// Coefficient of variation of inter-spike intervals.
    /// Low CV → regular firing. High CV → irregular/chaotic.
    pub isi_cv: f64,
    /// Mean membrane potential.
    pub mean_voltage: f64,
    /// Variance of membrane potential.
    pub voltage_variance: f64,
}

impl FhnModel {
    pub fn new(sample_rate: f64) -> Self {
        FhnModel {
            a: DEFAULT_A,
            b: DEFAULT_B,
            epsilon: DEFAULT_EPSILON,
            dt: 1.0 / sample_rate,
            sample_rate,
            time_scale: 300.0,
        }
    }

    /// Create with custom parameters (for brain type profiles).
    pub fn with_params(sample_rate: f64, a: f64, b: f64, epsilon: f64) -> Self {
        FhnModel {
            a,
            b,
            epsilon,
            dt: 1.0 / sample_rate,
            sample_rate,
            time_scale: 300.0,
        }
    }

    /// Simulate the FHN model driven by an input current signal.
    ///
    /// `input` is the external current I(t) at each time step.
    /// `input_scale` scales the auditory signal into a suitable current range
    /// (typically 0.3–1.5 for interesting dynamics).
    pub fn simulate(&self, input: &[f64], input_scale: f64) -> FhnResult {
        let n = input.len();
        let mut voltage = vec![0.0_f64; n];
        let mut recovery = vec![0.0_f64; n];
        let mut spike_times = Vec::new();

        // Initial conditions: resting state
        let mut v: f64 = -1.2;
        let mut w: f64 = -0.6;
        let mut prev_v: f64 = v;

        // Number of RK4 sub-steps per input sample for stability.
        // h is in model time: real_dt × time_scale, so model dynamics
        // run time_scale× faster than real time.
        let sub_steps = 4_usize;
        let h = self.dt * self.time_scale / sub_steps as f64;

        for i in 0..n {
            let current = input[i] * input_scale;

            // RK4 integration with sub-stepping
            for _ in 0..sub_steps {
                let (k1v, k1w) = self.derivatives(v, w, current);
                let (k2v, k2w) = self.derivatives(v + 0.5 * h * k1v, w + 0.5 * h * k1w, current);
                let (k3v, k3w) = self.derivatives(v + 0.5 * h * k2v, w + 0.5 * h * k2w, current);
                let (k4v, k4w) = self.derivatives(v + h * k3v, w + h * k3w, current);

                v += h / 6.0 * (k1v + 2.0 * k2v + 2.0 * k3v + k4v);
                w += h / 6.0 * (k1w + 2.0 * k2w + 2.0 * k3w + k4w);
            }

            voltage[i] = v;
            recovery[i] = w;

            // Spike detection: upward threshold crossing
            if prev_v < SPIKE_THRESHOLD && v >= SPIKE_THRESHOLD {
                spike_times.push(i);
            }
            prev_v = v;
        }

        // Compute statistics
        let duration_s = n as f64 / self.sample_rate;
        let firing_rate = spike_times.len() as f64 / duration_s;

        let isi_cv = Self::compute_isi_cv(&spike_times, self.sample_rate);

        let mean_voltage = voltage.iter().sum::<f64>() / n as f64;
        let voltage_variance = voltage.iter().map(|v| (v - mean_voltage).powi(2)).sum::<f64>() / n as f64;

        FhnResult {
            voltage,
            recovery,
            spike_times,
            firing_rate,
            isi_cv,
            mean_voltage,
            voltage_variance,
        }
    }

    #[inline]
    fn derivatives(&self, v: f64, w: f64, i_ext: f64) -> (f64, f64) {
        let dv = v - v.powi(3) / 3.0 - w + i_ext;
        let dw = self.epsilon * (v + self.a - self.b * w);
        (dv, dw)
    }

    /// Coefficient of variation of inter-spike intervals.
    fn compute_isi_cv(spike_times: &[usize], sample_rate: f64) -> f64 {
        if spike_times.len() < 3 {
            return 0.0;
        }

        let isis: Vec<f64> = spike_times
            .windows(2)
            .map(|w| (w[1] - w[0]) as f64 / sample_rate)
            .collect();

        let mean_isi = isis.iter().sum::<f64>() / isis.len() as f64;
        if mean_isi < 1e-12 {
            return 0.0;
        }

        let var_isi = isis.iter().map(|x| (x - mean_isi).powi(2)).sum::<f64>() / isis.len() as f64;
        var_isi.sqrt() / mean_isi
    }
}
