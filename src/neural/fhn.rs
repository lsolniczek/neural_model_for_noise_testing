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
    pub fn with_params(sample_rate: f64, a: f64, b: f64, epsilon: f64, time_scale: f64) -> Self {
        FhnModel {
            a,
            b,
            epsilon,
            dt: 1.0 / sample_rate,
            sample_rate,
            time_scale,
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
        let voltage_variance = voltage
            .iter()
            .map(|v| (v - mean_voltage).powi(2))
            .sum::<f64>()
            / n as f64;

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
    ///
    /// Returns `NaN` when fewer than 3 spikes are detected (fewer than 2 ISIs),
    /// because a meaningful variance estimate requires at least 2 data points.
    /// Callers must check `is_nan()` before using the result in arithmetic.
    pub(crate) fn compute_isi_cv(spike_times: &[usize], sample_rate: f64) -> f64 {
        if spike_times.len() < 3 {
            return f64::NAN;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 1000.0; // 1 kHz (neural model sample rate)

    // ---------------------------------------------------------------
    // ODE equations: verify derivatives at known points
    // ---------------------------------------------------------------

    #[test]
    fn derivatives_at_resting_state() {
        let fhn = FhnModel::new(SR);
        // Near resting equilibrium: v ≈ -1.2, w ≈ -0.625
        let (dv, dw) = fhn.derivatives(-1.2, -0.625, 0.0);
        // dv = -1.2 - (-1.2)^3/3 - (-0.625) = -1.2 + 0.576 + 0.625 = 0.001
        assert!(dv.abs() < 0.01, "dv at rest should be near 0, got {dv}");
        // dw = 0.08*(-1.2 + 0.7 - 0.8*(-0.625)) = 0.08*(-1.2 + 0.7 + 0.5) = 0.08*0.0 = 0
        assert!(dw.abs() < 0.01, "dw at rest should be near 0, got {dw}");
    }

    #[test]
    fn derivatives_cubic_term() {
        let fhn = FhnModel::new(SR);
        // At v=0, w=0, I=0: dv = 0 - 0 - 0 + 0 = 0
        let (dv, _) = fhn.derivatives(0.0, 0.0, 0.0);
        assert!((dv - 0.0).abs() < 1e-12);

        // At v=1, w=0, I=0: dv = 1 - 1/3 - 0 = 2/3
        let (dv, _) = fhn.derivatives(1.0, 0.0, 0.0);
        assert!((dv - 2.0 / 3.0).abs() < 1e-12);
    }

    #[test]
    fn derivatives_external_current_adds_linearly() {
        let fhn = FhnModel::new(SR);
        let (dv0, dw0) = fhn.derivatives(0.5, 0.3, 0.0);
        let (dv1, dw1) = fhn.derivatives(0.5, 0.3, 1.0);
        // dv should increase by exactly 1.0 (I enters linearly)
        assert!((dv1 - dv0 - 1.0).abs() < 1e-12);
        // dw should be unaffected by I
        assert!((dw1 - dw0).abs() < 1e-12);
    }

    // ---------------------------------------------------------------
    // Zero input → subthreshold (no spikes)
    // ---------------------------------------------------------------

    #[test]
    fn zero_input_no_spikes() {
        let fhn = FhnModel::new(SR);
        let input = vec![0.0; 3000]; // 3 seconds of silence
        let result = fhn.simulate(&input, 1.0);

        assert_eq!(result.spike_times.len(), 0, "No spikes for zero input");
        assert_eq!(result.firing_rate, 0.0);
    }

    // ---------------------------------------------------------------
    // ISI CV: NaN for insufficient spikes
    // ---------------------------------------------------------------

    #[test]
    fn isi_cv_nan_for_zero_spikes() {
        let cv = FhnModel::compute_isi_cv(&[], SR);
        assert!(cv.is_nan(), "CV should be NaN for 0 spikes, got {cv}");
    }

    #[test]
    fn isi_cv_nan_for_one_spike() {
        let cv = FhnModel::compute_isi_cv(&[100], SR);
        assert!(cv.is_nan(), "CV should be NaN for 1 spike, got {cv}");
    }

    #[test]
    fn isi_cv_nan_for_two_spikes() {
        let cv = FhnModel::compute_isi_cv(&[100, 200], SR);
        assert!(cv.is_nan(), "CV should be NaN for 2 spikes, got {cv}");
    }

    #[test]
    fn isi_cv_zero_for_regular_spikes() {
        // Perfectly regular: ISIs all equal → CV = 0
        let spikes: Vec<usize> = (0..10).map(|i| i * 100).collect();
        let cv = FhnModel::compute_isi_cv(&spikes, SR);
        assert!(!cv.is_nan());
        assert!(
            cv.abs() < 1e-10,
            "CV should be 0 for regular spikes, got {cv}"
        );
    }

    #[test]
    fn isi_cv_positive_for_irregular_spikes() {
        // Irregular: alternating short/long ISIs
        let spikes = vec![0, 50, 200, 250, 400, 450, 600];
        let cv = FhnModel::compute_isi_cv(&spikes, SR);
        assert!(!cv.is_nan());
        assert!(
            cv > 0.1,
            "CV should be positive for irregular spikes, got {cv}"
        );
    }

    // ---------------------------------------------------------------
    // Constant input in oscillatory regime → regular firing
    // ---------------------------------------------------------------

    #[test]
    fn constant_oscillatory_input_fires_regularly() {
        let fhn = FhnModel::new(SR);
        // I=0.5 is in the oscillatory regime (between lower and upper Hopf
        // bifurcations for standard FHN: a=0.7, b=0.8).
        // I > ~1.4 pushes past the upper bifurcation into a stable fixed point.
        let input = vec![0.5; 5000]; // 5 seconds
        let result = fhn.simulate(&input, 1.0);

        assert!(
            result.spike_times.len() >= 3,
            "Should fire multiple spikes in oscillatory regime, got {} spikes",
            result.spike_times.len()
        );
        assert!(result.firing_rate > 0.0, "Firing rate should be positive");
        // Constant input → regular firing → low CV
        assert!(
            !result.isi_cv.is_nan(),
            "Should have enough spikes for ISI CV"
        );
        assert!(
            result.isi_cv < 0.15,
            "Constant input should produce regular firing (low CV), got {:.3}",
            result.isi_cv
        );
    }

    // ---------------------------------------------------------------
    // Firing rate increases with input amplitude (within oscillatory regime)
    // ---------------------------------------------------------------

    #[test]
    fn firing_rate_increases_with_input() {
        let fhn = FhnModel::new(SR);
        let n = 5000;

        // Both values must be in the oscillatory regime (roughly I ∈ [0.34, 1.3]
        // for standard FHN params). Higher I → faster oscillation.
        let input_low = vec![0.4; n];
        let input_high = vec![0.9; n];

        let result_low = fhn.simulate(&input_low, 1.0);
        let result_high = fhn.simulate(&input_high, 1.0);

        assert!(
            result_high.firing_rate > result_low.firing_rate,
            "Higher input (in oscillatory regime) should produce higher firing rate: {:.1} vs {:.1}",
            result_high.firing_rate,
            result_low.firing_rate
        );
    }

    // ---------------------------------------------------------------
    // Spike detection: upward crossing only
    // ---------------------------------------------------------------

    #[test]
    fn spike_detection_counts_upward_crossings_only() {
        let fhn = FhnModel::new(SR);
        // Oscillatory input to produce spikes
        let n = 5000;
        let input: Vec<f64> = (0..n)
            .map(|i| 1.2 * (2.0 * PI * 10.0 * i as f64 / SR).sin())
            .collect();
        let result = fhn.simulate(&input, 1.0);

        // Each spike should correspond to a v crossing SPIKE_THRESHOLD upward
        for &t in &result.spike_times {
            assert!(
                result.voltage[t] >= SPIKE_THRESHOLD,
                "Spike at t={t}: v={:.3} should be >= {SPIKE_THRESHOLD}",
                result.voltage[t]
            );
            if t > 0 {
                // Previous sample should be below threshold (not asserted for t=0
                // because prev_v is initialized to -1.2 which is below threshold)
            }
        }
    }

    // ---------------------------------------------------------------
    // Output lengths match input
    // ---------------------------------------------------------------

    #[test]
    fn output_length_matches_input() {
        let fhn = FhnModel::new(SR);
        let input = vec![0.5; 1000];
        let result = fhn.simulate(&input, 1.0);

        assert_eq!(result.voltage.len(), 1000);
        assert_eq!(result.recovery.len(), 1000);
    }

    // ---------------------------------------------------------------
    // Determinism: same input → same output
    // ---------------------------------------------------------------

    #[test]
    fn deterministic_output() {
        let fhn = FhnModel::new(SR);
        let n = 2000;
        let input: Vec<f64> = (0..n)
            .map(|i| 0.8 * (2.0 * PI * 5.0 * i as f64 / SR).sin())
            .collect();

        let r1 = fhn.simulate(&input, 1.5);
        let r2 = fhn.simulate(&input, 1.5);

        assert_eq!(r1.firing_rate, r2.firing_rate);
        assert_eq!(r1.spike_times, r2.spike_times);
        assert!(r1.isi_cv.total_cmp(&r2.isi_cv).is_eq());
        for i in 0..n {
            assert_eq!(r1.voltage[i], r2.voltage[i]);
        }
    }

    // ---------------------------------------------------------------
    // input_scale modulates the drive
    // ---------------------------------------------------------------

    #[test]
    fn input_scale_modulates_drive() {
        let fhn = FhnModel::new(SR);
        // Use weak oscillatory input. Low input_scale stays subthreshold
        // (no spikes); high input_scale crosses threshold (spikes).
        let n = 5000;
        let input: Vec<f64> = (0..n)
            .map(|i| 0.3 * (2.0 * PI * 8.0 * i as f64 / SR).sin())
            .collect();

        let result_low = fhn.simulate(&input, 0.5); // 0.3×0.5 = 0.15 peak — subthreshold
        let result_high = fhn.simulate(&input, 5.0); // 0.3×5.0 = 1.5 peak — suprathreshold

        assert!(
            result_high.firing_rate > result_low.firing_rate,
            "Higher input_scale should produce higher firing rate: {:.1} vs {:.1}",
            result_high.firing_rate,
            result_low.firing_rate
        );
    }

    // ---------------------------------------------------------------
    // time_scale affects dynamics speed
    // ---------------------------------------------------------------

    #[test]
    fn time_scale_affects_firing_rate() {
        // Use constant drive in the oscillatory regime so we measure
        // the intrinsic oscillation frequency, which scales with time_scale.
        let fhn_slow = FhnModel::with_params(SR, 0.7, 0.8, 0.08, 150.0);
        let fhn_fast = FhnModel::with_params(SR, 0.7, 0.8, 0.08, 600.0);

        let input = vec![0.5; 5000];
        let result_slow = fhn_slow.simulate(&input, 1.0);
        let result_fast = fhn_fast.simulate(&input, 1.0);

        // Faster time_scale → faster intrinsic dynamics → higher firing rate
        assert!(
            result_fast.firing_rate > result_slow.firing_rate,
            "Faster time_scale should produce higher firing rate: {:.1} vs {:.1}",
            result_fast.firing_rate,
            result_slow.firing_rate
        );
    }

    // ---------------------------------------------------------------
    // Voltage stays bounded (global stability of FHN)
    // ---------------------------------------------------------------

    #[test]
    fn voltage_stays_bounded() {
        let fhn = FhnModel::new(SR);
        // Large input — test that the cubic saturation prevents divergence
        let input = vec![5.0; 5000];
        let result = fhn.simulate(&input, 1.0);

        for (i, &v) in result.voltage.iter().enumerate() {
            assert!(
                v.is_finite() && v.abs() < 10.0,
                "Voltage should be bounded, got v={v} at sample {i}"
            );
        }
    }

    // ---------------------------------------------------------------
    // Voltage statistics are correct
    // ---------------------------------------------------------------

    #[test]
    fn voltage_statistics_correct() {
        let fhn = FhnModel::new(SR);
        let input = vec![0.0; 2000];
        let result = fhn.simulate(&input, 1.0);

        // Manual computation of mean
        let expected_mean = result.voltage.iter().sum::<f64>() / result.voltage.len() as f64;
        assert!(
            (result.mean_voltage - expected_mean).abs() < 1e-12,
            "Mean voltage mismatch"
        );

        // Manual computation of variance
        let expected_var = result
            .voltage
            .iter()
            .map(|v| (v - expected_mean).powi(2))
            .sum::<f64>()
            / result.voltage.len() as f64;
        assert!(
            (result.voltage_variance - expected_var).abs() < 1e-12,
            "Voltage variance mismatch"
        );
    }

    // ---------------------------------------------------------------
    // Brain type parameters produce expected behavior
    // ---------------------------------------------------------------

    #[test]
    fn brain_types_produce_different_dynamics() {
        use crate::brain_type::BrainType;

        // Use constant drive in the oscillatory regime so that differences
        // in ε, a, and input_scale produce measurably different intrinsic
        // firing patterns.  An oscillatory drive at a single frequency
        // causes all types to lock 1:1 and mask the parameter differences.
        let input = vec![0.5; 5000];

        let mut voltages = Vec::new();
        for bt in &[
            BrainType::Normal,
            BrainType::HighAlpha,
            BrainType::Adhd,
            BrainType::Aging,
            BrainType::Anxious,
        ] {
            let params = bt.params();
            let fhn = FhnModel::with_params(
                SR,
                params.fhn.a,
                params.fhn.b,
                params.fhn.epsilon,
                params.fhn.time_scale,
            );
            let result = fhn.simulate(&input, params.fhn.input_scale);
            // Use mean voltage as a fingerprint of the dynamics
            voltages.push((format!("{:?}", bt), result.mean_voltage));
        }

        // Not all brain types should produce identical mean voltage
        let unique: std::collections::HashSet<u64> =
            voltages.iter().map(|(_, v)| v.to_bits()).collect();
        assert!(
            unique.len() > 1,
            "Brain types should differ in dynamics: {:?}",
            voltages
        );
    }

    // ---------------------------------------------------------------
    // Firing rate calculation is correct
    // ---------------------------------------------------------------

    #[test]
    fn firing_rate_formula_correct() {
        let fhn = FhnModel::new(SR);
        let input = vec![1.5; 3000]; // 3 seconds
        let result = fhn.simulate(&input, 1.0);

        let expected_rate = result.spike_times.len() as f64 / 3.0;
        assert!(
            (result.firing_rate - expected_rate).abs() < 1e-10,
            "Firing rate should be n_spikes / duration: {} vs {}",
            result.firing_rate,
            expected_rate
        );
    }
}
