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
        // the oscillation period ≈ 4 * (τ_e + τ_i) (empirically calibrated).
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
