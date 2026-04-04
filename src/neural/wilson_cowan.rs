/// Wilson-Cowan Neural Mass Model for fast cortical rhythms (Beta/Gamma).
///
/// Models two interacting neural populations:
///   - E: Excitatory (Glutamate)
///   - I: Inhibitory (GABA-A)
///
/// Unlike Jansen-Rit (which encodes membrane potentials), WC tracks
/// population firing rates. This makes frequency directly tunable
/// through coupling weights and time constants.
///
/// Equations:
///   τ_e · dE/dt = -E + S(w_ee·E - w_ei·I + P + h_e)
///   τ_i · dI/dt = -I + S(w_ie·E - w_ii·I + h_i)
///
/// EEG output: E(t) - I(t) (net excitatory-inhibitory balance)
///
/// Target frequency is approximately:
///   f ≈ (1/2π) · √(w_ei·w_ie·S'_e·S'_i / (τ_e·τ_i))

/// Wilson-Cowan model result (compatible with JR pipeline).
pub struct WilsonCowanResult {
    /// EEG-like output: E(t) - I(t).
    pub eeg: Vec<f64>,
    /// Inhibitory trace (analogous to JR's y[3] for E/I balance).
    pub inhib_trace: Vec<f64>,
}

/// Wilson-Cowan model parameters.
pub struct WilsonCowanParams {
    /// Excitatory time constant (seconds). Smaller = faster E dynamics.
    pub tau_e: f64,
    /// Inhibitory time constant (seconds). Smaller = faster I dynamics.
    pub tau_i: f64,
    /// E→E coupling (recurrent excitation).
    pub w_ee: f64,
    /// E→I coupling (excitation drives inhibition).
    pub w_ie: f64,
    /// I→E coupling (inhibition suppresses excitation).
    pub w_ei: f64,
    /// I→I coupling (recurrent inhibition).
    pub w_ii: f64,
    /// External bias to E population.
    pub h_e: f64,
    /// External bias to I population.
    pub h_i: f64,
    /// Sigmoid steepness.
    pub sigmoid_a: f64,
    /// Sigmoid threshold.
    pub sigmoid_theta: f64,
    /// Input scaling: maps audio [0,1] to drive current.
    pub input_scale: f64,
    /// Input offset: baseline drive when audio = 0.
    pub input_offset: f64,
}

pub struct WilsonCowanModel {
    dt: f64,
    sample_rate: f64,
    params: WilsonCowanParams,
}

impl WilsonCowanModel {
    pub fn new(sample_rate: f64, params: WilsonCowanParams) -> Self {
        WilsonCowanModel {
            dt: 1.0 / sample_rate,
            sample_rate,
            params,
        }
    }

    /// Create a WC model tuned to oscillate at the given target frequency.
    ///
    /// The frequency is controlled primarily by τ_e, τ_i and the coupling
    /// weights. This constructor sets physiologically reasonable defaults
    /// and tunes τ values to hit the target.
    pub fn for_frequency(sample_rate: f64, target_hz: f64, input_scale: f64) -> Self {
        // Wilson-Cowan oscillation frequency is primarily set by τ_e and τ_i.
        // For a standard E-I circuit with the coupling weights below,
        // the oscillation frequency ≈ 1 / (2.45 * (τ_e + τ_i)) (empirically calibrated).
        //
        // Coupling weights — strong enough for sustained oscillation,
        // balanced for clean sinusoidal output:
        let w_ee = 16.0;   // Recurrent excitation
        let w_ei = 15.0;   // Inhibition → excitation (main oscillation driver)
        let w_ie = 15.0;   // Excitation → inhibition
        let w_ii = 3.0;    // Recurrent inhibition (stabiliser)

        // Sigmoid — steep enough for robust oscillation
        let sigmoid_a = 1.3;
        let sigmoid_theta = 4.0;

        // Target period → τ values
        // Empirically calibrated: f ≈ 1 / (2.45 * (τ_e + τ_i)) for these coupling weights
        let tau_sum = 1.0 / (2.45 * target_hz);
        // E is slightly faster than I (excitation leads, inhibition follows)
        let tau_e = tau_sum * 0.45;
        let tau_i = tau_sum * 0.55;

        WilsonCowanModel::new(sample_rate, WilsonCowanParams {
            tau_e,
            tau_i,
            w_ee,
            w_ie,
            w_ei,
            w_ii,
            h_e: 1.5,    // Drive into oscillatory regime
            h_i: 0.0,
            sigmoid_a,
            sigmoid_theta,
            input_scale,
            input_offset: 1.0,
        })
    }

    /// Sigmoid activation function.
    #[inline]
    fn sigmoid(&self, x: f64) -> f64 {
        1.0 / (1.0 + (-self.params.sigmoid_a * (x - self.params.sigmoid_theta)).exp())
    }

    /// Simulate the WC model driven by an input signal.
    ///
    /// Input is expected in [0, 1] range (normalised audio band signal).
    /// Returns EEG-like output (E - I) and inhibitory trace.
    pub fn simulate(&self, input: &[f64]) -> WilsonCowanResult {
        let n = input.len();
        let mut eeg = vec![0.0_f64; n];
        let mut inhib_trace = vec![0.0_f64; n];

        // State variables
        let mut e = 0.1_f64;  // Excitatory firing rate
        let mut i = 0.05_f64; // Inhibitory firing rate

        let p = &self.params;

        // Sub-stepping for numerical stability at 48kHz
        let sub_steps = 2_usize;
        let h = self.dt / sub_steps as f64;

        // Warmup: 0.5 seconds to reach limit cycle
        let warmup_steps = (self.sample_rate * 0.5) as usize;
        let warmup_drive = p.input_offset + input[0] * p.input_scale;
        for _ in 0..warmup_steps {
            for _ in 0..sub_steps {
                let se = self.sigmoid(p.w_ee * e - p.w_ei * i + p.h_e + warmup_drive);
                let si = self.sigmoid(p.w_ie * e - p.w_ii * i + p.h_i);
                let de = (-e + se) / p.tau_e;
                let di = (-i + si) / p.tau_i;

                // RK2 (midpoint method)
                let e_mid = e + 0.5 * h * de;
                let i_mid = i + 0.5 * h * di;
                let se2 = self.sigmoid(p.w_ee * e_mid - p.w_ei * i_mid + p.h_e + warmup_drive);
                let si2 = self.sigmoid(p.w_ie * e_mid - p.w_ii * i_mid + p.h_i);
                let de2 = (-e_mid + se2) / p.tau_e;
                let di2 = (-i_mid + si2) / p.tau_i;

                e = (e + h * de2).clamp(0.0, 1.0);
                i = (i + h * di2).clamp(0.0, 1.0);
            }
        }

        // Main simulation
        for idx in 0..n {
            let drive = p.input_offset + input[idx] * p.input_scale;

            for _ in 0..sub_steps {
                let se = self.sigmoid(p.w_ee * e - p.w_ei * i + p.h_e + drive);
                let si = self.sigmoid(p.w_ie * e - p.w_ii * i + p.h_i);
                let de = (-e + se) / p.tau_e;
                let di = (-i + si) / p.tau_i;

                // RK2 (midpoint method)
                let e_mid = e + 0.5 * h * de;
                let i_mid = i + 0.5 * h * di;
                let se2 = self.sigmoid(p.w_ee * e_mid - p.w_ei * i_mid + p.h_e + drive);
                let si2 = self.sigmoid(p.w_ie * e_mid - p.w_ii * i_mid + p.h_i);
                let de2 = (-e_mid + se2) / p.tau_e;
                let di2 = (-i_mid + si2) / p.tau_i;

                e = (e + h * de2).clamp(0.0, 1.0);
                i = (i + h * di2).clamp(0.0, 1.0);
            }

            // EEG output: excitatory - inhibitory (net cortical activity)
            eeg[idx] = e - i;
            inhib_trace[idx] = i;
        }

        WilsonCowanResult { eeg, inhib_trace }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustfft::{num_complex::Complex, FftPlanner};
    use std::f64::consts::PI;

    const SR: f64 = 1000.0; // 1 kHz neural sample rate

    /// Find the dominant frequency of a signal via FFT (0.5–100 Hz range).
    fn dominant_frequency(signal: &[f64], sample_rate: f64) -> f64 {
        let n = signal.len();
        let fft_len = n.next_power_of_two();
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(fft_len);

        let hann_denom = (n - 1) as f64;
        let mut buf: Vec<Complex<f64>> = (0..fft_len)
            .map(|i| {
                if i < n {
                    let w = 0.5 * (1.0 - (2.0 * PI * i as f64 / hann_denom).cos());
                    Complex::new(signal[i] * w, 0.0)
                } else {
                    Complex::new(0.0, 0.0)
                }
            })
            .collect();
        fft.process(&mut buf);

        let freq_res = sample_rate / fft_len as f64;
        let min_bin = (0.5 / freq_res).ceil() as usize;
        let max_bin = ((100.0 / freq_res).floor() as usize).min(fft_len / 2);

        let mut best_power = 0.0_f64;
        let mut best_bin = min_bin;
        for bin in min_bin..max_bin {
            let power = buf[bin].norm_sqr();
            if power > best_power {
                best_power = power;
                best_bin = bin;
            }
        }
        best_bin as f64 * freq_res
    }

    // ---------------------------------------------------------------
    // Sigmoid
    // ---------------------------------------------------------------

    #[test]
    fn sigmoid_at_threshold_equals_half() {
        let wc = WilsonCowanModel::for_frequency(SR, 25.0, 0.5);
        let s = wc.sigmoid(wc.params.sigmoid_theta);
        assert!(
            (s - 0.5).abs() < 1e-10,
            "S(theta) should be 0.5, got {s}"
        );
    }

    #[test]
    fn sigmoid_range_zero_to_one() {
        let wc = WilsonCowanModel::for_frequency(SR, 25.0, 0.5);
        let s_low = wc.sigmoid(-100.0);
        let s_high = wc.sigmoid(100.0);
        assert!(s_low < 1e-6, "S(-100) should be ~0, got {s_low}");
        assert!((s_high - 1.0).abs() < 1e-6, "S(100) should be ~1, got {s_high}");
    }

    #[test]
    fn sigmoid_is_monotonically_increasing() {
        let wc = WilsonCowanModel::for_frequency(SR, 25.0, 0.5);
        let mut prev = wc.sigmoid(-20.0); // start below the loop range
        for k in -99..100 {
            let x = k as f64 * 0.2;
            let s = wc.sigmoid(x);
            assert!(s >= prev, "Sigmoid not monotonic at x={x}: {prev} > {s}");
            prev = s;
        }
    }

    // ---------------------------------------------------------------
    // Output shape and finiteness
    // ---------------------------------------------------------------

    #[test]
    fn output_length_matches_input() {
        let wc = WilsonCowanModel::for_frequency(SR, 20.0, 0.5);
        let input = vec![0.5; 2000];
        let result = wc.simulate(&input);
        assert_eq!(result.eeg.len(), 2000);
        assert_eq!(result.inhib_trace.len(), 2000);
    }

    #[test]
    fn output_is_finite() {
        let wc = WilsonCowanModel::for_frequency(SR, 20.0, 0.5);
        let input = vec![0.5; 3000];
        let result = wc.simulate(&input);
        for (idx, &v) in result.eeg.iter().enumerate() {
            assert!(v.is_finite(), "EEG sample {idx} is not finite: {v}");
        }
        for (idx, &v) in result.inhib_trace.iter().enumerate() {
            assert!(v.is_finite(), "Inhib sample {idx} is not finite: {v}");
        }
    }

    // ---------------------------------------------------------------
    // E and I stay in [0, 1] → EEG in [-1, 1]
    // ---------------------------------------------------------------

    #[test]
    fn eeg_bounded() {
        let wc = WilsonCowanModel::for_frequency(SR, 25.0, 0.5);
        let input = vec![0.5; 5000];
        let result = wc.simulate(&input);
        for (idx, &v) in result.eeg.iter().enumerate() {
            assert!(
                v >= -1.0 && v <= 1.0,
                "EEG sample {idx} = {v} out of [-1, 1]"
            );
        }
    }

    #[test]
    fn inhib_trace_bounded() {
        let wc = WilsonCowanModel::for_frequency(SR, 25.0, 0.5);
        let input = vec![0.5; 5000];
        let result = wc.simulate(&input);
        for (idx, &v) in result.inhib_trace.iter().enumerate() {
            assert!(
                v >= 0.0 && v <= 1.0,
                "Inhib sample {idx} = {v} out of [0, 1]"
            );
        }
    }

    // ---------------------------------------------------------------
    // Determinism
    // ---------------------------------------------------------------

    #[test]
    fn deterministic_output() {
        let wc = WilsonCowanModel::for_frequency(SR, 20.0, 0.5);
        let input = vec![0.5; 2000];
        let r1 = wc.simulate(&input);
        let r2 = wc.simulate(&input);
        for idx in 0..r1.eeg.len() {
            assert_eq!(r1.eeg[idx], r2.eeg[idx], "EEG differs at {idx}");
        }
    }

    // ---------------------------------------------------------------
    // Oscillation: model should oscillate with default for_frequency params
    // ---------------------------------------------------------------

    #[test]
    fn for_frequency_produces_oscillation() {
        let wc = WilsonCowanModel::for_frequency(SR, 20.0, 0.5);
        let input = vec![0.5; 5000]; // 5 seconds
        let result = wc.simulate(&input);

        let mean = result.eeg.iter().sum::<f64>() / result.eeg.len() as f64;
        let var = result.eeg.iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>() / result.eeg.len() as f64;
        assert!(
            var > 1e-6,
            "WC model should oscillate, EEG variance = {var}"
        );
    }

    // ---------------------------------------------------------------
    // for_frequency: dominant frequency is near target
    // ---------------------------------------------------------------

    #[test]
    fn for_frequency_14hz_within_tolerance() {
        let wc = WilsonCowanModel::for_frequency(SR, 14.0, 0.5);
        let input = vec![0.5; 8000]; // 8 seconds for good resolution
        let result = wc.simulate(&input);

        let dom_freq = dominant_frequency(&result.eeg, SR);
        assert!(
            (dom_freq - 14.0).abs() < 5.0,
            "Target 14 Hz, got dominant freq {dom_freq:.1} Hz"
        );
    }

    #[test]
    fn for_frequency_25hz_within_tolerance() {
        let wc = WilsonCowanModel::for_frequency(SR, 25.0, 0.5);
        let input = vec![0.5; 8000];
        let result = wc.simulate(&input);

        let dom_freq = dominant_frequency(&result.eeg, SR);
        assert!(
            (dom_freq - 25.0).abs() < 8.0,
            "Target 25 Hz, got dominant freq {dom_freq:.1} Hz"
        );
    }

    // ---------------------------------------------------------------
    // Higher target → higher oscillation frequency
    // ---------------------------------------------------------------

    #[test]
    fn higher_target_produces_higher_frequency() {
        let wc_low = WilsonCowanModel::for_frequency(SR, 10.0, 0.5);
        let wc_high = WilsonCowanModel::for_frequency(SR, 30.0, 0.5);

        let input = vec![0.5; 8000];
        let r_low = wc_low.simulate(&input);
        let r_high = wc_high.simulate(&input);

        let f_low = dominant_frequency(&r_low.eeg, SR);
        let f_high = dominant_frequency(&r_high.eeg, SR);

        assert!(
            f_high > f_low,
            "Higher target should produce higher freq: {f_low:.1} vs {f_high:.1}"
        );
    }

    // ---------------------------------------------------------------
    // for_frequency: tau_e < tau_i (excitation faster than inhibition)
    // ---------------------------------------------------------------

    #[test]
    fn tau_e_less_than_tau_i() {
        let wc = WilsonCowanModel::for_frequency(SR, 20.0, 0.5);
        assert!(
            wc.params.tau_e < wc.params.tau_i,
            "tau_e ({}) should be < tau_i ({})",
            wc.params.tau_e,
            wc.params.tau_i
        );
    }

    // ---------------------------------------------------------------
    // External drive only affects E population
    // ---------------------------------------------------------------

    #[test]
    fn drive_only_enters_e_equation() {
        // Verify structurally: with zero coupling (w_ie=0), changing the drive
        // should not affect I dynamics at all.
        let wc = WilsonCowanModel::new(SR, WilsonCowanParams {
            tau_e: 0.005,
            tau_i: 0.005,
            w_ee: 0.0,
            w_ei: 0.0,
            w_ie: 0.0, // E does not drive I
            w_ii: 0.0,
            h_e: 0.0,
            h_i: 0.0,
            sigmoid_a: 1.3,
            sigmoid_theta: 4.0,
            input_scale: 1.0,
            input_offset: 0.0,
        });

        let input_low = vec![0.0; 2000];
        let input_high = vec![1.0; 2000];
        let r_low = wc.simulate(&input_low);
        let r_high = wc.simulate(&input_high);

        // I trace should be identical (drive doesn't reach I when w_ie=0)
        for idx in 0..r_low.inhib_trace.len() {
            assert_eq!(
                r_low.inhib_trace[idx], r_high.inhib_trace[idx],
                "I should be unaffected by drive when w_ie=0, differs at {idx}"
            );
        }
    }

    // ---------------------------------------------------------------
    // for_frequency: tau values scale inversely with target Hz
    // ---------------------------------------------------------------

    #[test]
    fn tau_scales_inversely_with_frequency() {
        let wc_10 = WilsonCowanModel::for_frequency(SR, 10.0, 0.5);
        let wc_40 = WilsonCowanModel::for_frequency(SR, 40.0, 0.5);

        let tau_sum_10 = wc_10.params.tau_e + wc_10.params.tau_i;
        let tau_sum_40 = wc_40.params.tau_e + wc_40.params.tau_i;

        // 40 Hz target → tau_sum should be ~4x smaller than 10 Hz target
        let ratio = tau_sum_10 / tau_sum_40;
        assert!(
            (ratio - 4.0).abs() < 0.1,
            "tau_sum ratio should be ~4.0 for 4x frequency, got {ratio:.2}"
        );
    }
}
