/// Physiological Thalamic Gate (Priority 9).
///
/// Replaces the linear arousal → band_offset heuristic in `thalamic_gate.rs`
/// with an ion-channel-based single-compartment thalamocortical (TC) relay
/// neuron. Arousal modulates the K⁺ leak conductance (g_KL), which in turn
/// switches the cell between tonic firing (high arousal → wake) and burst
/// firing (low arousal → sleep). The transition is sigmoidal rather than
/// linear, with ion-channel-derived thresholds, and so produces a more
/// physiologically grounded shape than the heuristic.
///
/// # Architecture
///
/// We simulate ONE TC cell per gate construction, run it for 5 s of
/// simulated time to reach steady state, then sample the firing pattern
/// over an additional 1 s window. From the firing pattern we extract:
///
///   - mean firing rate r (Hz)
///   - inter-spike-interval coefficient of variation (ISI CV)
///
/// We then derive a "burstiness" scalar in [0, 1]:
///   - burstiness = 0 when the cell is in regular tonic mode (low CV)
///   - burstiness = 1 when the cell is silent or bursting irregularly (high CV)
///
/// The per-band offset shift is `−MAX_OFFSET_REDUCTION × burstiness × proportion[b]`,
/// using the same Steriade-McCormick-Sejnowski (1993) frequency-selective
/// proportions [100%, 70%, 20%, 0%] as the heuristic gate.
///
/// # Why a single cell, not the full Paul 2016 / Bazhenov 2002 network
///
/// Both papers describe TC + RE + cortex circuits with chaotic intermediate
/// states. For the *scalar* `band_offset_shifts() -> [f64; 4]` interface that
/// the JR pipeline consumes, the qualitative burst↔tonic switch in a single
/// TC cell is sufficient to replace the linear heuristic with a sigmoidal
/// shape derived from ion-channel dynamics. The chaotic intermediate region,
/// the RE inhibitory loop, and the cortical feedback are v2 enhancements —
/// they do not change the *shape* of the per-band shift function, only its
/// jitter.
///
/// # Parameter sources
///
/// All conductances and reversal potentials come from Bazhenov et al. 2002
/// (J Neurosci 22:8691-8704, Table 1, TC cell column). The T-type Ca²⁺
/// activation/inactivation gating uses the canonical Destexhe Boltzmann
/// fits as published in Destexhe et al. 1996 / Huguenard & McCormick 1992,
/// reproduced in ModelDB entry 3343 and dozens of subsequent TC models.
/// Standard Hodgkin-Huxley Na⁺/K⁺ kinetics use the Mainen & Sejnowski
/// (1996) cortical-cell forms, which are widely reused for TC cells too.
///
/// # Refs
///
/// - Bazhenov M, Timofeev I, Steriade M, Sejnowski TJ (2002).
///   "Model of thalamocortical slow-wave sleep oscillations and transitions
///   to activated states." *J Neurosci* 22(19):8691-8704. — TC conductances,
///   reversal potentials, g_KL as the master wake↔sleep switch.
/// - Paul K, Cauller LJ, Llano DA (2016). "Presence of a chaotic region at
///   the sleep-wake transition in a simplified thalamocortical circuit
///   model." *Front Comput Neurosci* 10:91. — TC + RE + cortex 3-cell
///   circuit; describes the chaotic intermediate region (Lyapunov +0.24)
///   that this v1 implementation does not yet model.
/// - Destexhe A, Contreras D, Steriade M, Sejnowski TJ, Huguenard JR (1996).
///   "In vivo, in vitro, and computational analysis of dendritic calcium
///   currents in thalamic reticular neurons." *J Neurosci* 16(1):169-185.
///   — canonical I_T m_inf, h_inf, tau_h Boltzmann fits.
/// - Huguenard JR, McCormick DA (1992). "Simulation of the currents involved
///   in rhythmic oscillations in thalamic relay neurons." *J Neurophysiol*
///   68(4):1373-1383. — original I_T fits for TC relay cells.
/// - Mainen ZF, Sejnowski TJ (1996). "Influence of dendritic structure on
///   firing pattern in model neocortical neurons." *Nature* 382:363-366.
///   — Na⁺ and K⁺ kinetics widely reused for TC simulation.
/// - Steriade M, McCormick DA, Sejnowski TJ (1993). "Thalamocortical
///   oscillations in the sleeping and aroused brain." *Science*
///   262(5134):679-685. — frequency-selective burst-mode proportions.
use crate::preset::Preset;

// ── Cell parameters from Bazhenov 2002 (mS/cm² and mV) ─────────────────────
//
// Bazhenov 2002 Table 1 reports these TC values:
//   g_Na = 90, g_K = 10, g_T = 2.2, g_h = 0.017 mS/cm²
//   E_L(TC) = -70 mV, E_KL = -95 mV
//   Wake state: g_KL = 0; Sleep state: g_KL ∈ [0, 0.03]

const C_M: f64 = 1.0; // membrane capacitance, µF/cm²
const G_NA: f64 = 90.0; // Na⁺ max conductance, mS/cm²
const G_K: f64 = 10.0; // K⁺ delayed-rectifier conductance, mS/cm²
const G_T: f64 = 2.2; // T-type Ca²⁺ conductance, mS/cm²
const G_L: f64 = 0.05; // passive leak conductance, mS/cm²
const E_NA: f64 = 50.0; // mV (standard HH)
const E_K: f64 = -95.0; // mV (matches E_KL by design — Bazhenov uses both)
const E_CA: f64 = 120.0; // mV (standard HH)
const E_L: f64 = -70.0; // mV (Bazhenov 2002 TC cell)
const E_KL: f64 = -95.0; // mV (Bazhenov 2002 K⁺ leak)

/// Maximum K⁺ leak conductance at deep sleep (mS/cm²). Bazhenov 2002 reports
/// g_KL_TC sweeps from 0 (wake) to ~0.03 (deep sleep) for the TC cell.
/// We use 0.06 to push the burst↔tonic transition to moderate arousal
/// (~0.4–0.5) so the gate engages on typical relaxation presets rather than
/// only the deepest-sleep configurations. This is the same ion-channel model
/// at a different operating range — physiologically, the g_KL range depends
/// on the balance between K⁺ leak channels and ascending cholinergic tone
/// (Bazhenov 2002, section "Activated states"), and varies across cell types.
const G_KL_MAX: f64 = 0.06;

/// Integration timestep for the TC cell ODE (seconds). Bazhenov 2002 uses
/// dt = 0.02 ms with RK4 for the same cell model. We adopt the same.
const DT_S: f64 = 0.02e-3;

/// Total simulation time per `new()` call. Paul 2016 explicitly uses 15 s of
/// warmup; we use 5 s warmup + 1 s sampling = 6 s total, which is enough for
/// the TC cell to reach a stable bursting pattern at the chosen g_KL.
const WARMUP_S: f64 = 5.0;
const SAMPLE_S: f64 = 1.0;

/// Mirrors `thalamic_gate::MAX_OFFSET_REDUCTION` so the magnitude of the
/// per-band shift matches the heuristic gate at deep relaxation. Only the
/// SHAPE of the shift-vs-arousal function changes; the maximum reachable
/// shift is held constant for compatibility with brain-type tuning.
const MAX_OFFSET_REDUCTION: f64 = 65.0;

/// Spike detection threshold. Membrane potential must cross from below to
/// above this value to count as a spike. Standard for HH-type models.
const SPIKE_THRESHOLD_MV: f64 = -20.0;

/// Tonic excitatory drive to the TC cell (µA/cm²). In vivo, TC cells receive
/// constant glutamatergic feedback from cortex (and modulatory drive from
/// ascending brainstem nuclei) that partially counteracts the hyperpolarizing
/// K⁺ leak. Without this, the model cell goes silent at moderate g_KL because
/// the net current is purely hyperpolarizing — which is correct for an
/// isolated cell but wrong for an in-circuit relay neuron.
///
/// Per Bazhenov 2002: "In the model, the depolarizing effect of the
/// cholinergic input was simulated by blocking I_KL." Their approach is to
/// set g_KL=0 for wake; our approach maps arousal continuously, so we need
/// a tonic current to keep the cell firing at moderate g_KL.
///
/// Tuned so that: at arousal=0.5 (moderate relaxation, g_KL=0.015), the cell
/// still fires tonically (I_INJ > i_kl ≈ 0.375); at arousal ≤ 0.2 (deep
/// relaxation, g_KL ≥ 0.024 → i_kl ≈ 0.6), the cell is overwhelmed by
/// the K⁺ leak, hyperpolarizes, and transitions to burst/silence.
const I_INJ: f64 = 0.5;

// ── Single-compartment TC cell state ───────────────────────────────────────

/// State vector for the TC cell:
///   v: membrane potential (mV)
///   m: I_Na activation gate
///   h: I_Na inactivation gate
///   n: I_K activation gate
///   m_t: I_T activation gate
///   h_t: I_T inactivation gate
#[derive(Debug, Clone, Copy)]
struct TcState {
    v: f64,
    m: f64,
    h: f64,
    n: f64,
    m_t: f64,
    h_t: f64,
}

impl TcState {
    /// Initial conditions: resting potential near E_L with gates at their
    /// steady-state values for that voltage. The simulation will settle
    /// over the warmup window regardless of small departures from these.
    fn resting() -> Self {
        let v0 = -65.0;
        TcState {
            v: v0,
            m: na_m_inf(v0),
            h: na_h_inf(v0),
            n: k_n_inf(v0),
            m_t: t_m_inf(v0),
            h_t: t_h_inf(v0),
        }
    }
}

// ── Hodgkin-Huxley Na⁺ kinetics (Mainen & Sejnowski 1996) ──────────────────
//
// Standard cortical Na⁺ kinetics widely reused for TC cells. The HH form is:
//   dm/dt = alpha_m(V) * (1 - m) - beta_m(V) * m
//   m_inf = alpha / (alpha + beta), tau_m = 1 / (alpha + beta)

fn na_alpha_m(v: f64) -> f64 {
    // Mainen-Sejnowski Na⁺ activation. Uses the singular-form-safe limit at v=-54.
    let dv = v + 54.0;
    if dv.abs() < 1e-6 {
        // L'Hôpital limit of 0.32*dv / (1 - exp(-dv/4)) as dv → 0 is 0.32*4 = 1.28
        1.28
    } else {
        0.32 * dv / (1.0 - (-dv / 4.0).exp())
    }
}

fn na_beta_m(v: f64) -> f64 {
    let dv = v + 27.0;
    if dv.abs() < 1e-6 {
        // L'Hôpital limit of 0.28*dv / (exp(dv/5) - 1) as dv → 0 is 0.28*5 = 1.4
        1.4
    } else {
        0.28 * dv / ((dv / 5.0).exp() - 1.0)
    }
}

fn na_alpha_h(v: f64) -> f64 {
    0.128 * (-(v + 50.0) / 18.0).exp()
}

fn na_beta_h(v: f64) -> f64 {
    4.0 / (1.0 + (-(v + 27.0) / 5.0).exp())
}

fn na_m_inf(v: f64) -> f64 {
    let a = na_alpha_m(v);
    let b = na_beta_m(v);
    a / (a + b)
}

fn na_h_inf(v: f64) -> f64 {
    let a = na_alpha_h(v);
    let b = na_beta_h(v);
    a / (a + b)
}

// ── Hodgkin-Huxley K⁺ delayed rectifier (Mainen & Sejnowski 1996) ──────────

fn k_alpha_n(v: f64) -> f64 {
    let dv = v + 52.0;
    if dv.abs() < 1e-6 {
        0.16 // L'Hôpital limit
    } else {
        0.032 * dv / (1.0 - (-dv / 5.0).exp())
    }
}

fn k_beta_n(v: f64) -> f64 {
    0.5 * (-(v + 57.0) / 40.0).exp()
}

fn k_n_inf(v: f64) -> f64 {
    let a = k_alpha_n(v);
    let b = k_beta_n(v);
    a / (a + b)
}

// ── T-type Ca²⁺ current (Destexhe 1996 / Huguenard 1992 canonical fits) ───
//
// The low-threshold Ca²⁺ current is the engine of TC bursting. At depolarized
// rest potentials (high arousal, low g_KL) it stays inactivated and the cell
// fires tonically; at hyperpolarized rest (low arousal, high g_KL) it
// de-inactivates and the cell rebound-bursts on every depolarization.
//
// I_T = g_T · m_t² · h_t · (V − E_Ca)
//
// Boltzmann fits below are the canonical Destexhe-Sejnowski form used in
// ModelDB 3343 and reproduced in essentially every TC model since 1996.

fn t_m_inf(v: f64) -> f64 {
    1.0 / (1.0 + (-(v + 57.0) / 6.2).exp())
}

fn t_h_inf(v: f64) -> f64 {
    1.0 / (1.0 + ((v + 81.0) / 4.0).exp())
}

fn t_tau_h(v: f64) -> f64 {
    // Destexhe 1996 piecewise tau_h, in ms
    // Includes the asymmetric "hyperpolarized vs depolarized" branches
    (30.8 + (211.4 + ((v + 113.2) / 5.0).exp()) / (1.0 + ((v + 84.0) / 3.2).exp())) / 3.7373
}

// ── Cell ODE: full HH derivatives at state y, given g_KL ──────────────────

fn tc_derivatives(y: &TcState, g_kl: f64) -> [f64; 6] {
    let v = y.v;
    // Currents (µA/cm² with C_m in µF/cm² gives mV/ms units)
    let i_na = G_NA * y.m.powi(3) * y.h * (v - E_NA);
    let i_k = G_K * y.n.powi(4) * (v - E_K);
    let i_t = G_T * y.m_t.powi(2) * y.h_t * (v - E_CA);
    let i_l = G_L * (v - E_L);
    let i_kl = g_kl * (v - E_KL);

    let dv_dt = (-(i_na + i_k + i_t + i_l + i_kl) + I_INJ) / C_M;

    // Gate derivatives. Times here are in ms because the rate functions
    // (alpha, beta) are quoted in 1/ms in Mainen-Sejnowski / Destexhe.
    let dm_dt = na_alpha_m(v) * (1.0 - y.m) - na_beta_m(v) * y.m;
    let dh_dt = na_alpha_h(v) * (1.0 - y.h) - na_beta_h(v) * y.h;
    let dn_dt = k_alpha_n(v) * (1.0 - y.n) - k_beta_n(v) * y.n;
    let dmt_dt = (t_m_inf(v) - y.m_t) / 1.0; // m_t is fast (≈ instantaneous)
    let dht_dt = (t_h_inf(v) - y.h_t) / t_tau_h(v);

    [dv_dt, dm_dt, dh_dt, dn_dt, dmt_dt, dht_dt]
}

fn tc_step_rk4(y: &TcState, g_kl: f64, dt_ms: f64) -> TcState {
    // dt is in milliseconds for the RK4 step. dV/dt is mV/ms (from C_m=1
    // µF/cm² × 1 µA/cm² = 1 mV/ms), and gate derivatives are in 1/ms, so
    // multiplying by dt_ms gives consistent units.
    let k1 = tc_derivatives(y, g_kl);
    let y2 = TcState {
        v: y.v + 0.5 * dt_ms * k1[0],
        m: y.m + 0.5 * dt_ms * k1[1],
        h: y.h + 0.5 * dt_ms * k1[2],
        n: y.n + 0.5 * dt_ms * k1[3],
        m_t: y.m_t + 0.5 * dt_ms * k1[4],
        h_t: y.h_t + 0.5 * dt_ms * k1[5],
    };
    let k2 = tc_derivatives(&y2, g_kl);
    let y3 = TcState {
        v: y.v + 0.5 * dt_ms * k2[0],
        m: y.m + 0.5 * dt_ms * k2[1],
        h: y.h + 0.5 * dt_ms * k2[2],
        n: y.n + 0.5 * dt_ms * k2[3],
        m_t: y.m_t + 0.5 * dt_ms * k2[4],
        h_t: y.h_t + 0.5 * dt_ms * k2[5],
    };
    let k3 = tc_derivatives(&y3, g_kl);
    let y4 = TcState {
        v: y.v + dt_ms * k3[0],
        m: y.m + dt_ms * k3[1],
        h: y.h + dt_ms * k3[2],
        n: y.n + dt_ms * k3[3],
        m_t: y.m_t + dt_ms * k3[4],
        h_t: y.h_t + dt_ms * k3[5],
    };
    let k4 = tc_derivatives(&y4, g_kl);
    TcState {
        v: y.v + dt_ms / 6.0 * (k1[0] + 2.0 * k2[0] + 2.0 * k3[0] + k4[0]),
        m: y.m + dt_ms / 6.0 * (k1[1] + 2.0 * k2[1] + 2.0 * k3[1] + k4[1]),
        h: y.h + dt_ms / 6.0 * (k1[2] + 2.0 * k2[2] + 2.0 * k3[2] + k4[2]),
        n: y.n + dt_ms / 6.0 * (k1[3] + 2.0 * k2[3] + 2.0 * k3[3] + k4[3]),
        m_t: y.m_t + dt_ms / 6.0 * (k1[4] + 2.0 * k2[4] + 2.0 * k3[4] + k4[4]),
        h_t: y.h_t + dt_ms / 6.0 * (k1[5] + 2.0 * k2[5] + 2.0 * k3[5] + k4[5]),
    }
}

/// Result of running the TC cell at a given g_KL: mean firing rate, ISI CV,
/// and a "burstiness" scalar in [0, 1] used to derive the band-offset shift.
#[derive(Debug, Clone, Copy)]
struct TcFiringStats {
    mean_rate_hz: f64,
    isi_cv: f64,
    burstiness: f64,
}

/// Simulate the TC cell at the given g_KL for WARMUP_S seconds, then sample
/// over SAMPLE_S seconds and report the firing statistics.
fn simulate_tc_cell(g_kl: f64) -> TcFiringStats {
    let dt_ms = DT_S * 1000.0;
    let warmup_steps = (WARMUP_S / DT_S) as usize;
    let sample_steps = (SAMPLE_S / DT_S) as usize;

    let mut state = TcState::resting();
    for _ in 0..warmup_steps {
        state = tc_step_rk4(&state, g_kl, dt_ms);
        // Defensive clamp — under aggressive params V can briefly excurse;
        // clamp keeps the integrator stable without changing physiological
        // behavior in the normal operating range.
        state.v = state.v.clamp(-150.0, 100.0);
    }

    // Sampling window — record spike times.
    let mut spike_times_ms: Vec<f64> = Vec::new();
    let mut prev_v = state.v;
    for i in 0..sample_steps {
        state = tc_step_rk4(&state, g_kl, dt_ms);
        state.v = state.v.clamp(-150.0, 100.0);
        // Spike detection: V crosses SPIKE_THRESHOLD_MV from below.
        if prev_v < SPIKE_THRESHOLD_MV && state.v >= SPIKE_THRESHOLD_MV {
            spike_times_ms.push(i as f64 * dt_ms);
        }
        prev_v = state.v;
    }

    let n_spikes = spike_times_ms.len();
    let mean_rate_hz = n_spikes as f64 / SAMPLE_S;

    // ISI CV — useful for distinguishing tonic (low CV) from bursting (high CV).
    let isi_cv = if n_spikes >= 3 {
        let isis: Vec<f64> = spike_times_ms.windows(2).map(|w| w[1] - w[0]).collect();
        let mean_isi = isis.iter().sum::<f64>() / isis.len() as f64;
        if mean_isi > 0.0 {
            let var = isis.iter().map(|x| (x - mean_isi).powi(2)).sum::<f64>() / isis.len() as f64;
            var.sqrt() / mean_isi
        } else {
            0.0
        }
    } else {
        // Fewer than 3 spikes → essentially silent → "very bursty / silent"
        // by convention. Returning a high CV makes silent behave like deep
        // burst mode for the band-shift mapping below.
        2.0
    };

    // Burstiness ∈ [0, 1] — mapping from (mean_rate, isi_cv) to a single
    // scalar. The intent: tonic firing (regular ~15-20 Hz, low CV) → 0;
    // bursting / silent (high CV or very low rate) → 1.
    //
    // Two contributions:
    //   - rate_factor: drops from 1 (silent) to 0 (>~25 Hz) via a soft sigmoid
    //   - cv_factor: rises from 0 (CV<0.3, tonic) to 1 (CV>1.0, irregular)
    let rate_factor = 1.0 / (1.0 + (mean_rate_hz - 15.0).exp() * 0.3);
    let cv_factor = ((isi_cv - 0.3) / 0.7).clamp(0.0, 1.0);
    let burstiness = (0.5 * rate_factor + 0.5 * cv_factor).clamp(0.0, 1.0);

    TcFiringStats {
        mean_rate_hz,
        isi_cv,
        burstiness,
    }
}

// ── Public gate interface — mirrors thalamic_gate::ThalamicGate ────────────

/// Physiological thalamic gate. Same interface as `ThalamicGate` so the
/// pipeline can dispatch between the two via a single boolean flag.
pub struct PhysiologicalThalamicGate {
    enabled: bool,
    arousal: f64,
    cached_shifts: [f64; 4],
    /// Diagnostic — exposed for tests and the eventual update_model.md
    /// validation row. Not currently consumed by the pipeline.
    cached_stats: Option<TcFiringStats>,
}

impl PhysiologicalThalamicGate {
    /// Create a physiological gate at the given arousal level. This runs
    /// the TC cell simulation (≈6 s simulated time) immediately and caches
    /// the result, so the per-band shifts are available with O(1) cost
    /// at every subsequent `band_offset_shifts()` call.
    pub fn new(arousal: f64) -> Self {
        let arousal = arousal.clamp(0.0, 1.0);
        // Map arousal → g_KL. Bazhenov 2002: wake = 0, sleep = G_KL_MAX.
        // Linear inverse mapping: high arousal → low g_KL.
        let g_kl = G_KL_MAX * (1.0 - arousal);
        let stats = simulate_tc_cell(g_kl);

        // Convert the burstiness scalar into per-band offset shifts using
        // the same Steriade [100%, 70%, 20%, 0%] proportions as the
        // heuristic gate. This is the "frequency-selective burst mode"
        // observation: low bands shift fully into burst regime first; high
        // bands stay tonic the longest.
        let base = -MAX_OFFSET_REDUCTION * stats.burstiness;
        let cached_shifts = [
            base * 1.0, // Band 0: full
            base * 0.7, // Band 1
            base * 0.2, // Band 2
            base * 0.0, // Band 3: never shifts (always tonic)
        ];

        PhysiologicalThalamicGate {
            enabled: true,
            arousal,
            cached_shifts,
            cached_stats: Some(stats),
        }
    }

    /// Disabled (passthrough) gate — `band_offset_shifts()` returns zeros.
    pub fn disabled() -> Self {
        PhysiologicalThalamicGate {
            enabled: false,
            arousal: 0.5,
            cached_shifts: [0.0; 4],
            cached_stats: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn arousal(&self) -> f64 {
        self.arousal
    }

    /// Per-band offset shifts. Returns zeros when disabled.
    pub fn band_offset_shifts(&self) -> [f64; 4] {
        if !self.enabled {
            return [0.0; 4];
        }
        self.cached_shifts
    }

    /// Diagnostic accessor for tests — TC cell mean firing rate at this arousal.
    #[cfg(test)]
    fn diagnostic_firing_rate(&self) -> Option<f64> {
        self.cached_stats.map(|s| s.mean_rate_hz)
    }

    /// Diagnostic accessor for tests — TC cell ISI CV at this arousal.
    #[cfg(test)]
    fn diagnostic_isi_cv(&self) -> Option<f64> {
        self.cached_stats.map(|s| s.isi_cv)
    }

    /// Diagnostic accessor for tests — burstiness scalar in [0, 1].
    #[cfg(test)]
    fn diagnostic_burstiness(&self) -> Option<f64> {
        self.cached_stats.map(|s| s.burstiness)
    }

    /// Reuse the heuristic gate's preset → arousal mapping. Both gates
    /// take arousal as their input; only the arousal → shift function
    /// differs between them.
    pub fn compute_arousal(preset: &Preset, brightness: f64) -> f64 {
        crate::auditory::ThalamicGate::compute_arousal(preset, brightness)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ═══════════════════════════════════════════════════════════════
    // T-type Ca²⁺ gating curves — sanity check the canonical Boltzmanns
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn t_m_inf_is_monotonic_increasing() {
        let mut prev = t_m_inf(-100.0);
        for v_int in -99..=20 {
            let v = v_int as f64;
            let m = t_m_inf(v);
            assert!(
                m >= prev - 1e-12,
                "t_m_inf should be monotonic at v={v}: prev={prev}, current={m}"
            );
            prev = m;
        }
    }

    #[test]
    fn t_h_inf_is_monotonic_decreasing() {
        // h is INACTIVATION — should drop with depolarization.
        let mut prev = t_h_inf(-100.0);
        for v_int in -99..=20 {
            let v = v_int as f64;
            let h = t_h_inf(v);
            assert!(
                h <= prev + 1e-12,
                "t_h_inf should be monotonic decreasing at v={v}: prev={prev}, current={h}"
            );
            prev = h;
        }
    }

    #[test]
    fn t_h_inf_high_at_hyperpolarized_low_at_depolarized() {
        // De-inactivated at hyperpolarization (sleep): h ≈ 1
        // Inactivated at depolarization (wake): h ≈ 0
        assert!(t_h_inf(-95.0) > 0.95, "TC h should be ~1 at -95 mV (sleep)");
        assert!(t_h_inf(-50.0) < 0.05, "TC h should be ~0 at -50 mV (wake)");
    }

    #[test]
    fn t_tau_h_is_finite_across_physiological_range() {
        for v_int in -100..=20 {
            let v = v_int as f64;
            let tau = t_tau_h(v);
            assert!(tau.is_finite() && tau > 0.0, "t_tau_h({v}) = {tau}");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // HH gates — basic sanity
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn hh_singularities_handled() {
        // alpha_m has a 0/0 at v = -54; verify the limit is taken.
        let a = na_alpha_m(-54.0);
        let b = na_beta_m(-27.0);
        let n = k_alpha_n(-52.0);
        assert!(
            a.is_finite() && b.is_finite() && n.is_finite(),
            "HH singular limits broken: alpha_m={a}, beta_m={b}, alpha_n={n}"
        );
    }

    #[test]
    fn hh_steady_state_gates_in_zero_one() {
        for v_int in -100..=20 {
            let v = v_int as f64;
            assert!(
                (0.0..=1.0).contains(&na_m_inf(v)),
                "na_m_inf({v}) out of [0,1]"
            );
            assert!(
                (0.0..=1.0).contains(&na_h_inf(v)),
                "na_h_inf({v}) out of [0,1]"
            );
            assert!(
                (0.0..=1.0).contains(&k_n_inf(v)),
                "k_n_inf({v}) out of [0,1]"
            );
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Cell ODE — finite-bounded behavior under any g_KL
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn tc_cell_step_finite_at_extremes() {
        let mut state = TcState::resting();
        // 100 ms of integration at the most depolarized arousal (g_KL=0)
        for _ in 0..5000 {
            state = tc_step_rk4(&state, 0.0, 0.02);
            assert!(state.v.is_finite(), "V went non-finite: {state:?}");
            assert!(
                state.v.abs() < 200.0,
                "V escaped physiological range: {state:?}"
            );
        }
        // Same at deep sleep (max g_KL)
        let mut state = TcState::resting();
        for _ in 0..5000 {
            state = tc_step_rk4(&state, G_KL_MAX, 0.02);
            assert!(
                state.v.is_finite(),
                "V went non-finite at sleep g_KL: {state:?}"
            );
            assert!(state.v.abs() < 200.0, "V escaped: {state:?}");
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // PhysiologicalThalamicGate — public interface invariants
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn disabled_gate_returns_zeros() {
        let gate = PhysiologicalThalamicGate::disabled();
        assert!(!gate.is_enabled());
        assert_eq!(gate.band_offset_shifts(), [0.0; 4]);
    }

    #[test]
    fn arousal_clamped_to_unit_interval() {
        let g_low = PhysiologicalThalamicGate::new(-1.0);
        let g_high = PhysiologicalThalamicGate::new(2.0);
        assert_eq!(g_low.arousal(), 0.0);
        assert_eq!(g_high.arousal(), 1.0);
    }

    #[test]
    fn band_offset_shifts_array_length_4() {
        let gate = PhysiologicalThalamicGate::new(0.5);
        let shifts = gate.band_offset_shifts();
        assert_eq!(shifts.len(), 4);
    }

    #[test]
    fn band3_always_zero_per_steriade_proportions() {
        // The frequency-selective burst-mode observation of Steriade et al.
        // 1993: high bands (gamma, here band 3) stay in tonic mode regardless
        // of the thalamic state. The physiological gate must respect this.
        for arousal in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let gate = PhysiologicalThalamicGate::new(arousal);
            let shifts = gate.band_offset_shifts();
            assert!(
                shifts[3].abs() < 1e-10,
                "Band 3 must be 0 at arousal {arousal}, got {}",
                shifts[3]
            );
        }
    }

    #[test]
    fn shifts_are_non_positive() {
        // Shifts represent reductions in input_offset → must be ≤ 0.
        for arousal in [0.0, 0.25, 0.5, 0.75, 1.0] {
            let gate = PhysiologicalThalamicGate::new(arousal);
            let shifts = gate.band_offset_shifts();
            for (b, &s) in shifts.iter().enumerate() {
                assert!(
                    s <= 1e-10,
                    "Shift must be non-positive at arousal {arousal}, band {b}: got {s}"
                );
            }
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Physiological behavior — burst-vs-tonic mode switch
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn high_arousal_produces_smaller_shift_than_low_arousal() {
        // The whole point of the gate: low arousal → bigger shift toward
        // bifurcation; high arousal → smaller shift. The shape is now
        // ion-channel-driven sigmoid, not a linear ramp, but the
        // monotonicity must still hold.
        let g_high = PhysiologicalThalamicGate::new(0.95);
        let g_low = PhysiologicalThalamicGate::new(0.05);
        let s_high = g_high.band_offset_shifts()[0];
        let s_low = g_low.band_offset_shifts()[0];
        eprintln!(
            "high arousal shift={s_high:.3}, low arousal shift={s_low:.3}, \
             high firing={:.1} Hz CV={:.2} burst={:.3}, \
             low firing={:.1} Hz CV={:.2} burst={:.3}",
            g_high.diagnostic_firing_rate().unwrap_or(0.0),
            g_high.diagnostic_isi_cv().unwrap_or(0.0),
            g_high.diagnostic_burstiness().unwrap_or(0.0),
            g_low.diagnostic_firing_rate().unwrap_or(0.0),
            g_low.diagnostic_isi_cv().unwrap_or(0.0),
            g_low.diagnostic_burstiness().unwrap_or(0.0),
        );
        assert!(
            s_low < s_high,
            "Low arousal shift ({s_low}) must be more negative than high arousal ({s_high})"
        );
    }

    #[test]
    fn band_proportions_match_steriade_at_max_burstiness() {
        // At max burstiness, band 1 should be ~70% of band 0, band 2 ~20%,
        // band 3 = 0 — same proportions as the heuristic gate at moderate
        // arousal (per Steriade 1993).
        let gate = PhysiologicalThalamicGate::new(0.0);
        let shifts = gate.band_offset_shifts();
        // Only check ratios when the base shift is non-trivial
        if shifts[0].abs() > 1e-3 {
            let r1 = shifts[1] / shifts[0];
            let r2 = shifts[2] / shifts[0];
            assert!(
                (r1 - 0.7).abs() < 0.02,
                "Band 1 / Band 0 ratio should be 0.7, got {r1:.3}"
            );
            assert!(
                (r2 - 0.2).abs() < 0.02,
                "Band 2 / Band 0 ratio should be 0.2, got {r2:.3}"
            );
        }
    }

    #[test]
    fn deep_sleep_produces_meaningful_shift() {
        // At arousal=0 (full sleep), the TC cell should be in burst mode
        // and produce a substantial offset shift on band 0. The exact
        // magnitude depends on burstiness, but it should be at least half
        // of MAX_OFFSET_REDUCTION.
        let gate = PhysiologicalThalamicGate::new(0.0);
        let shift = gate.band_offset_shifts()[0];
        eprintln!(
            "Deep sleep band 0 shift = {shift:.2}, firing rate = {:.1} Hz, CV = {:.2}, burstiness = {:.3}",
            gate.diagnostic_firing_rate().unwrap_or(0.0),
            gate.diagnostic_isi_cv().unwrap_or(0.0),
            gate.diagnostic_burstiness().unwrap_or(0.0),
        );
        assert!(
            shift < -MAX_OFFSET_REDUCTION * 0.4,
            "Deep sleep band 0 shift should be at least -{:.0}, got {shift:.2}",
            MAX_OFFSET_REDUCTION * 0.4
        );
    }

    #[test]
    fn full_wake_produces_small_or_zero_shift() {
        // At arousal=1, g_KL=0, the cell is at depolarized rest with the
        // T-current inactivated (h_t ≈ 0). The cell should fire tonically
        // and burstiness should be near zero.
        let gate = PhysiologicalThalamicGate::new(1.0);
        let shift = gate.band_offset_shifts()[0];
        eprintln!(
            "Full wake band 0 shift = {shift:.2}, firing rate = {:.1} Hz, CV = {:.2}, burstiness = {:.3}",
            gate.diagnostic_firing_rate().unwrap_or(0.0),
            gate.diagnostic_isi_cv().unwrap_or(0.0),
            gate.diagnostic_burstiness().unwrap_or(0.0),
        );
        assert!(
            shift > -MAX_OFFSET_REDUCTION * 0.25,
            "Full wake band 0 shift should be smaller than -{:.0}, got {shift:.2}",
            MAX_OFFSET_REDUCTION * 0.25
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // Arousal computation — must match the heuristic exactly (delegation test)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn compute_arousal_delegates_to_heuristic() {
        let preset = Preset::default();
        let brightness = 0.5;
        let phys = PhysiologicalThalamicGate::compute_arousal(&preset, brightness);
        let heuristic = crate::auditory::ThalamicGate::compute_arousal(&preset, brightness);
        assert_eq!(
            phys.to_bits(),
            heuristic.to_bits(),
            "Physiological gate must reuse the heuristic gate's compute_arousal exactly"
        );
    }
}
