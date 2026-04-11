/// Performance Vector — diagnostic metrics for preset evaluation.
///
/// Instead of collapsing a simulation into a single score, these three
/// metrics expose *how* the noise is affecting the neural model:
///
/// 1. **Entrainment Ratio**: Does the EEG lock onto the preset's LFO?
/// 2. **E/I Stability Index**: Is the fast inhibitory loop stable?
/// 3. **Spectral Centroid**: Where is the EEG's "centre of gravity"?

use rustfft::{num_complex::Complex, FftPlanner};
use std::f64::consts::PI;

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

    /// Phase-Locking Value (PLV) to the target modulation frequency.
    /// Per Lachaux et al. (1999): PLV = |1/N × Σ exp(i·φ(t))|
    /// where φ(t) is the instantaneous phase difference between the
    /// neural EEG and a reference sinusoid at the target frequency.
    /// Range: [0, 1]. 1.0 = perfect phase-locking (true entrainment).
    /// 0.0 = no phase relationship (coincidental power, not entrained).
    /// None if no target frequency.
    pub plv: Option<f64>,

    /// Envelope-phase PLV (CET 13c). Phase-locking between the cortical EEG
    /// (bandpassed in the CET-relevant 2–9 Hz band) and the *instantaneous
    /// phase of the slow envelope* extracted from the auditory drive.
    ///
    /// This is the cortical envelope tracking metric proper: rather than
    /// asking "does the EEG carry power at the LFO frequency?" (carrier PLV),
    /// it asks "does the EEG follow the slow envelope of the stimulus
    /// time-locked, phase by phase?" — exactly what Ding & Simon (2014)
    /// and Luo & Poeppel (2007) measure as CET.
    ///
    /// Computed via Hilbert transform on both signals (Marple 1999) →
    /// instantaneous phase difference → averaged unit phasors. Reuses 80%
    /// of the carrier PLV machinery (`compute_plv`).
    ///
    /// Range [0, 1]. None if no envelope reference signal provided
    /// (CET disabled or pre-CET legacy call site).
    pub envelope_plv: Option<f64>,
}

impl PerformanceVector {
    /// Compute the performance vector from simulation outputs.
    ///
    /// - `eeg`: Combined bilateral EEG signal (detrended preferred).
    /// - `fast_inhib_trace`: y[3] trace from the Wendling model (empty if G=0).
    /// - `sample_rate`: Sample rate of the EEG signal (Hz).
    /// - `target_freq`: NeuralLFO frequency from the preset (None if no LFO).
    ///
    /// Backward-compatible legacy entry point — does not compute envelope PLV.
    /// New CET call sites should use `compute_with_envelope` to also produce
    /// the envelope-phase PLV metric.
    pub fn compute(
        eeg: &[f64],
        fast_inhib_trace: &[f64],
        sample_rate: f64,
        target_freq: Option<f64>,
    ) -> Self {
        Self::compute_with_envelope(eeg, fast_inhib_trace, sample_rate, target_freq, None)
    }

    /// CET 13c: same as `compute` but also computes envelope-phase PLV when
    /// an envelope reference signal is supplied. The envelope is the slow
    /// (≤10 Hz) component extracted from the auditory drive — typically the
    /// slow path of the CET crossover. When `envelope` is `None`,
    /// `envelope_plv` is `None` and the result is identical to `compute()`.
    pub fn compute_with_envelope(
        eeg: &[f64],
        fast_inhib_trace: &[f64],
        sample_rate: f64,
        target_freq: Option<f64>,
        envelope: Option<&[f64]>,
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

        let plv = target_freq.map(|freq| {
            compute_plv(eeg, sample_rate, freq)
        });

        let envelope_plv = envelope.map(|env| compute_envelope_plv(eeg, env, sample_rate));

        PerformanceVector {
            entrainment_ratio,
            ei_stability,
            spectral_centroid,
            plv,
            envelope_plv,
        }
    }
}

/// Envelope-phase PLV (CET 13c) per Ding & Simon (2014) and Luo & Poeppel (2007).
///
/// Measures phase coherence between the EEG (bandpassed in the CET-relevant
/// 2–9 Hz delta-theta band per Doelling et al. 2014) and the instantaneous
/// phase of the *envelope reference signal* — the slow auditory drive that
/// the cortex is trying to track.
///
/// Algorithm:
/// 1. Bandpass both signals to 2–9 Hz via FFT zero-out (matches `compute_plv`).
/// 2. Hilbert transform → analytic signal → instantaneous phase for each.
/// 3. Compute Δφ(t) = φ_eeg(t) − φ_envelope(t).
/// 4. Average the unit phasors: PLV = |1/N · Σ exp(i·Δφ)|.
///
/// Returns 0.0 when either signal is too short or has no power in the band.
/// Mostly mirrors `compute_plv` but the reference is the envelope's actual
/// phase (Hilbert-extracted) rather than a synthetic sinusoid.
pub fn compute_envelope_plv(eeg: &[f64], envelope: &[f64], sample_rate: f64) -> f64 {
    let n = eeg.len().min(envelope.len());
    if n < 64 {
        return 0.0;
    }
    let fft_len = n.next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let fft_fwd = planner.plan_fft_forward(fft_len);
    let fft_inv = planner.plan_fft_inverse(fft_len);
    let freq_res = sample_rate / fft_len as f64;

    // CET-relevant band: 2–9 Hz (Doelling et al. 2014). The lo bin is clamped
    // to ≥1 to keep DC out of the bandpass.
    let lo_bin = (2.0 / freq_res).floor().max(1.0) as usize;
    let hi_bin = (9.0 / freq_res).ceil().min((fft_len / 2) as f64) as usize;
    if hi_bin <= lo_bin {
        return 0.0;
    }

    // Helper: bandpass + Hilbert analytic signal in one FFT pass.
    // Mirrors the structure of `compute_plv` so the two share an algorithm.
    let analytic = |signal: &[f64]| -> Vec<Complex<f64>> {
        let hann_denom = if n > 1 { (n - 1) as f64 } else { 1.0 };
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
        fft_fwd.process(&mut buf);
        // Bandpass: zero everything outside [lo_bin, hi_bin]
        for i in 0..fft_len {
            let bin = if i <= fft_len / 2 { i } else { fft_len - i };
            if bin < lo_bin || bin > hi_bin {
                buf[i] = Complex::new(0.0, 0.0);
            }
        }
        // Hilbert: zero negative frequencies, double positive
        for i in 1..fft_len / 2 {
            buf[i] *= 2.0;
        }
        for i in (fft_len / 2 + 1)..fft_len {
            buf[i] = Complex::new(0.0, 0.0);
        }
        fft_inv.process(&mut buf);
        buf
    };

    let eeg_anal = analytic(eeg);
    let env_anal = analytic(envelope);

    let mut phasor_sum = Complex::new(0.0_f64, 0.0_f64);
    let valid = n;
    let inv_fft_len = 1.0 / fft_len as f64;
    for i in 0..valid {
        let z_eeg = eeg_anal[i] * inv_fft_len;
        let z_env = env_anal[i] * inv_fft_len;
        let phi_eeg = z_eeg.im.atan2(z_eeg.re);
        let phi_env = z_env.im.atan2(z_env.re);
        let dphi = phi_eeg - phi_env;
        phasor_sum += Complex::new(dphi.cos(), dphi.sin());
    }

    let plv = (phasor_sum / valid as f64).norm();
    plv.clamp(0.0, 1.0)
}

/// Entrainment Ratio: power in [target_freq - 1, target_freq + 1] Hz / total power.
///
/// A Hann window is applied before the FFT to reduce spectral leakage,
/// consistent with all other FFT paths in the codebase.
fn compute_entrainment_ratio(eeg: &[f64], sample_rate: f64, target_freq: f64) -> f64 {
    let n = eeg.len();
    if n < 2 {
        return 0.0;
    }

    let fft_len = n.next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);

    let hann_denom = if n > 1 { (n - 1) as f64 } else { 1.0 };
    let mut buffer: Vec<Complex<f64>> = (0..fft_len)
        .map(|i| {
            if i < n {
                let w = 0.5 * (1.0 - (2.0 * PI * i as f64 / hann_denom).cos());
                Complex::new(eeg[i] * w, 0.0)
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

/// Phase-Locking Value (PLV) per Lachaux et al. (1999).
///
/// Measures the consistency of the phase relationship between the neural
/// EEG oscillation and a reference sinusoid at the target frequency.
/// Uses Hilbert transform (Marple 1999) for instantaneous phase extraction.
///
/// PLV = |1/N × Σ exp(i·(φ_eeg(t) - φ_ref(t)))|
///
/// Steps:
/// 1. Bandpass filter EEG around target frequency (±3 Hz)
/// 2. Extract instantaneous phase via Hilbert transform
/// 3. Generate reference phase: 2π × f × t
/// 4. Compute phase difference and average the unit phasors
fn compute_plv(eeg: &[f64], sample_rate: f64, target_freq: f64) -> f64 {
    let n = eeg.len();
    if n < 64 || target_freq <= 0.5 {
        return 0.0;
    }

    let fft_len = n.next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let fft_fwd = planner.plan_fft_forward(fft_len);
    let fft_inv = planner.plan_fft_inverse(fft_len);

    // Step 1: Bandpass filter around target ±3 Hz using FFT
    let hann_denom = if n > 1 { (n - 1) as f64 } else { 1.0 };
    let mut buf: Vec<Complex<f64>> = (0..fft_len)
        .map(|i| {
            if i < n {
                let w = 0.5 * (1.0 - (2.0 * PI * i as f64 / hann_denom).cos());
                Complex::new(eeg[i] * w, 0.0)
            } else {
                Complex::new(0.0, 0.0)
            }
        })
        .collect();

    fft_fwd.process(&mut buf);

    let freq_res = sample_rate / fft_len as f64;
    let lo_bin = ((target_freq - 3.0).max(0.5) / freq_res).floor() as usize;
    let hi_bin = ((target_freq + 3.0) / freq_res).ceil().min((fft_len / 2) as f64) as usize;

    // Zero out everything outside the bandpass
    for i in 0..fft_len {
        let bin = if i <= fft_len / 2 { i } else { fft_len - i };
        if bin < lo_bin || bin > hi_bin {
            buf[i] = Complex::new(0.0, 0.0);
        }
    }

    // Step 2: Hilbert transform for analytic signal (Marple 1999)
    // Zero negative frequencies, double positive frequencies
    for i in 1..fft_len / 2 {
        buf[i] *= 2.0;
    }
    for i in (fft_len / 2 + 1)..fft_len {
        buf[i] = Complex::new(0.0, 0.0);
    }
    // DC and Nyquist unchanged

    fft_inv.process(&mut buf);

    let inv_n = 1.0 / fft_len as f64;

    // Step 3 & 4: Compute PLV from phase difference
    let mut phasor_sum = Complex::new(0.0_f64, 0.0_f64);
    let valid_samples = n.min(fft_len);

    for i in 0..valid_samples {
        let analytic = buf[i] * inv_n;
        let eeg_phase = analytic.im.atan2(analytic.re);

        // Reference phase: pure sinusoid at target frequency
        let ref_phase = 2.0 * PI * target_freq * i as f64 / sample_rate;

        // Phase difference
        let phase_diff = eeg_phase - ref_phase;

        // Accumulate unit phasor
        phasor_sum += Complex::new(phase_diff.cos(), phase_diff.sin());
    }

    // PLV = magnitude of average phasor
    let plv = (phasor_sum / valid_samples as f64).norm();
    plv.clamp(0.0, 1.0)
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
///
/// A Hann window is applied before the FFT to reduce spectral leakage,
/// consistent with all other FFT paths in the codebase.
fn compute_spectral_centroid(eeg: &[f64], sample_rate: f64) -> f64 {
    let n = eeg.len();
    if n < 2 {
        return 10.0; // default alpha
    }

    let fft_len = n.next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);

    let hann_denom = if n > 1 { (n - 1) as f64 } else { 1.0 };
    let mut buffer: Vec<Complex<f64>> = (0..fft_len)
        .map(|i| {
            if i < n {
                let w = 0.5 * (1.0 - (2.0 * PI * i as f64 / hann_denom).cos());
                Complex::new(eeg[i] * w, 0.0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 1000.0; // 1 kHz neural sample rate

    /// Generate a pure sine at `freq` Hz for `duration_secs` at `sample_rate`.
    fn sine(freq: f64, sample_rate: f64, duration_secs: f64) -> Vec<f64> {
        let n = (sample_rate * duration_secs) as usize;
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f64 / sample_rate).sin())
            .collect()
    }

    // ---------------------------------------------------------------
    // PerformanceVector::compute — structural tests
    // ---------------------------------------------------------------

    #[test]
    fn compute_no_lfo_no_fast_inhib() {
        let eeg = sine(10.0, SR, 5.0);
        let pv = PerformanceVector::compute(&eeg, &[], SR, None);

        assert!(pv.entrainment_ratio.is_none(), "No LFO → entrainment should be None");
        assert!(pv.ei_stability.is_none(), "Empty fast_inhib → ei_stability should be None");
        assert!(pv.spectral_centroid > 0.0, "Centroid should be positive");
    }

    #[test]
    fn compute_with_lfo_and_fast_inhib() {
        let eeg = sine(10.0, SR, 5.0);
        let y3 = vec![1.0; 5000]; // constant y3
        let pv = PerformanceVector::compute(&eeg, &y3, SR, Some(10.0));

        assert!(pv.entrainment_ratio.is_some());
        assert!(pv.ei_stability.is_some());
    }

    // ---------------------------------------------------------------
    // Entrainment Ratio
    // ---------------------------------------------------------------

    #[test]
    fn entrainment_pure_sine_at_target() {
        // Pure 10 Hz sine → almost all power at 10 Hz → high entrainment ratio
        let eeg = sine(10.0, SR, 5.0);
        let ratio = compute_entrainment_ratio(&eeg, SR, 10.0);
        assert!(
            ratio > 0.8,
            "Pure 10 Hz sine should have high entrainment at 10 Hz, got {ratio:.3}"
        );
    }

    #[test]
    fn entrainment_off_target_is_low() {
        // Pure 10 Hz sine → low entrainment at 30 Hz (no power there)
        let eeg = sine(10.0, SR, 5.0);
        let ratio = compute_entrainment_ratio(&eeg, SR, 30.0);
        assert!(
            ratio < 0.1,
            "10 Hz sine should have low entrainment at 30 Hz, got {ratio:.3}"
        );
    }

    #[test]
    fn entrainment_in_zero_to_one() {
        let eeg = sine(10.0, SR, 5.0);
        let ratio = compute_entrainment_ratio(&eeg, SR, 10.0);
        assert!(ratio >= 0.0 && ratio <= 1.0, "Ratio out of [0,1]: {ratio}");
    }

    #[test]
    fn entrainment_zero_for_silence() {
        let eeg = vec![0.0; 5000];
        let ratio = compute_entrainment_ratio(&eeg, SR, 10.0);
        assert_eq!(ratio, 0.0, "Silence should give 0 entrainment");
    }

    #[test]
    fn entrainment_zero_for_short_signal() {
        let eeg = vec![1.0]; // only 1 sample
        let ratio = compute_entrainment_ratio(&eeg, SR, 10.0);
        assert_eq!(ratio, 0.0, "Signal < 2 samples should return 0");
    }

    // ---------------------------------------------------------------
    // E/I Stability Index
    // ---------------------------------------------------------------

    #[test]
    fn ei_stability_zero_for_constant_signal() {
        // Constant y3 → std_dev = 0 → CV = 0
        let y3 = vec![5.0; 1000];
        let cv = compute_ei_stability(&y3);
        assert!(cv.abs() < 1e-10, "Constant signal should give CV=0, got {cv}");
    }

    #[test]
    fn ei_stability_positive_for_varying_signal() {
        // Oscillating y3 → positive CV
        let y3: Vec<f64> = (0..1000)
            .map(|i| 5.0 + 2.0 * (2.0 * PI * 10.0 * i as f64 / SR).sin())
            .collect();
        let cv = compute_ei_stability(&y3);
        assert!(cv > 0.0, "Oscillating signal should give positive CV, got {cv}");
    }

    #[test]
    fn ei_stability_known_value() {
        // Known: [1, 3] → mean=2, var=(1+1)/1=2, std=√2, CV=√2/2 ≈ 0.707
        let y3 = vec![1.0, 3.0];
        let cv = compute_ei_stability(&y3);
        let expected = (2.0_f64).sqrt() / 2.0;
        assert!(
            (cv - expected).abs() < 1e-10,
            "CV of [1,3] should be {expected:.4}, got {cv:.4}"
        );
    }

    #[test]
    fn ei_stability_near_zero_mean_returns_std() {
        // Mean ≈ 0, should return raw std_dev
        let y3: Vec<f64> = (0..1000)
            .map(|i| (2.0 * PI * 10.0 * i as f64 / SR).sin())
            .collect();
        let cv = compute_ei_stability(&y3);
        // For a sine wave with mean≈0, the result should be the std_dev
        // which is amp/√2 ≈ 1/√2 ≈ 0.707
        assert!(
            cv > 0.5 && cv < 0.9,
            "Near-zero mean sine: expected std≈0.707, got {cv:.3}"
        );
    }

    #[test]
    fn ei_stability_zero_for_single_sample() {
        let y3 = vec![42.0];
        let cv = compute_ei_stability(&y3);
        assert_eq!(cv, 0.0, "Single sample should return 0");
    }

    // ---------------------------------------------------------------
    // Spectral Centroid
    // ---------------------------------------------------------------

    #[test]
    fn centroid_pure_10hz_near_10() {
        let eeg = sine(10.0, SR, 5.0);
        let c = compute_spectral_centroid(&eeg, SR);
        assert!(
            (c - 10.0).abs() < 2.0,
            "Pure 10 Hz sine centroid should be near 10 Hz, got {c:.1}"
        );
    }

    #[test]
    fn centroid_pure_40hz_near_40() {
        let eeg = sine(40.0, SR, 5.0);
        let c = compute_spectral_centroid(&eeg, SR);
        assert!(
            (c - 40.0).abs() < 3.0,
            "Pure 40 Hz sine centroid should be near 40 Hz, got {c:.1}"
        );
    }

    #[test]
    fn centroid_higher_freq_higher_centroid() {
        let eeg_low = sine(5.0, SR, 5.0);
        let eeg_high = sine(30.0, SR, 5.0);
        let c_low = compute_spectral_centroid(&eeg_low, SR);
        let c_high = compute_spectral_centroid(&eeg_high, SR);
        assert!(
            c_high > c_low,
            "Higher frequency should give higher centroid: {c_low:.1} vs {c_high:.1}"
        );
    }

    #[test]
    fn centroid_default_for_silence() {
        let eeg = vec![0.0; 5000];
        let c = compute_spectral_centroid(&eeg, SR);
        assert_eq!(c, 10.0, "Silence should return default 10 Hz");
    }

    #[test]
    fn centroid_default_for_short_signal() {
        let eeg = vec![1.0]; // only 1 sample
        let c = compute_spectral_centroid(&eeg, SR);
        assert_eq!(c, 10.0, "Signal < 2 samples should return default 10 Hz");
    }

    #[test]
    fn centroid_in_physiological_range() {
        // Mixed signal with components in 5, 15, 35 Hz
        let n = 5000;
        let eeg: Vec<f64> = (0..n)
            .map(|i| {
                let t = i as f64 / SR;
                (2.0 * PI * 5.0 * t).sin()
                    + (2.0 * PI * 15.0 * t).sin()
                    + (2.0 * PI * 35.0 * t).sin()
            })
            .collect();
        let c = compute_spectral_centroid(&eeg, SR);
        assert!(
            c >= 1.0 && c <= 50.0,
            "Centroid should be in [1, 50] Hz, got {c:.1}"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // PLV tests — per Lachaux et al. (1999)
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn plv_perfect_sine_at_target() {
        // Pure 10 Hz sine → perfect phase-locking to 10 Hz reference
        let n = (SR * 5.0) as usize;
        let eeg: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 10.0 * i as f64 / SR).sin())
            .collect();
        let plv = compute_plv(&eeg, SR, 10.0);
        assert!(
            plv > 0.8,
            "Pure sine at target should have high PLV, got {plv:.3}"
        );
    }

    #[test]
    fn plv_off_target_is_low() {
        // Pure 10 Hz sine → low PLV at 25 Hz (no phase relationship)
        let n = (SR * 5.0) as usize;
        let eeg: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 10.0 * i as f64 / SR).sin())
            .collect();
        let plv = compute_plv(&eeg, SR, 25.0);
        assert!(
            plv < 0.3,
            "10 Hz sine should have low PLV at 25 Hz, got {plv:.3}"
        );
    }

    #[test]
    fn plv_noise_is_low() {
        // Random noise → no phase-locking
        let n = (SR * 5.0) as usize;
        let mut state = 12345u64;
        let eeg: Vec<f64> = (0..n)
            .map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                (state as f64 / u64::MAX as f64) * 2.0 - 1.0
            })
            .collect();
        let plv = compute_plv(&eeg, SR, 10.0);
        assert!(
            plv < 0.3,
            "Random noise should have low PLV, got {plv:.3}"
        );
    }

    #[test]
    fn plv_in_zero_to_one() {
        let n = (SR * 3.0) as usize;
        let eeg: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 14.0 * i as f64 / SR).sin())
            .collect();
        let plv = compute_plv(&eeg, SR, 14.0);
        assert!(plv >= 0.0 && plv <= 1.0, "PLV should be in [0,1], got {plv}");
    }

    #[test]
    fn plv_included_in_performance_vector() {
        let n = (SR * 3.0) as usize;
        let eeg: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 10.0 * i as f64 / SR).sin())
            .collect();
        let pv = PerformanceVector::compute(&eeg, &[], SR, Some(10.0));
        assert!(pv.plv.is_some(), "PLV should be present when target freq given");
        assert!(pv.plv.unwrap() > 0.5, "Pure sine PLV should be high");
    }

    #[test]
    fn plv_none_without_target() {
        let eeg = vec![0.5; 3000];
        let pv = PerformanceVector::compute(&eeg, &[], SR, None);
        assert!(pv.plv.is_none(), "PLV should be None without target freq");
    }

    // ---------------------------------------------------------------
    // CET 13c: Envelope-phase PLV
    //
    // Per Ding & Simon (2014), Luo & Poeppel (2007), Doelling et al. (2014):
    // CET is the phase-locking of cortical EEG (in the 2–9 Hz band) to the
    // *instantaneous phase of the slow auditory envelope*. Tests use synthetic
    // signals to verify the PLV detector responds correctly.
    // ---------------------------------------------------------------

    #[test]
    fn envelope_plv_high_for_eeg_locked_to_5hz_envelope() {
        // EEG and envelope are the same 5 Hz sine — perfect phase lock,
        // PLV should be very close to 1.0.
        let eeg = sine(5.0, SR, 6.0);
        let env = sine(5.0, SR, 6.0);
        let plv = compute_envelope_plv(&eeg, &env, SR);
        assert!(
            plv > 0.85,
            "Self-locked 5 Hz envelope PLV should be high, got {plv:.3}"
        );
    }

    #[test]
    fn envelope_plv_low_for_independent_signals() {
        // Two unrelated signals: EEG at 12 Hz (alpha-ish), envelope at 4 Hz.
        // After both are bandpass-filtered to 2–9 Hz, the EEG retains very
        // little signal in band, so the phase relationship is essentially
        // noise — PLV should be low.
        let eeg = sine(12.0, SR, 6.0); // out of CET band
        let env = sine(4.0, SR, 6.0);
        let plv = compute_envelope_plv(&eeg, &env, SR);
        assert!(
            plv < 0.5,
            "Independent (12 Hz vs 4 Hz) signals envelope PLV should be low, got {plv:.3}"
        );
    }

    #[test]
    fn envelope_plv_high_for_phase_shifted_lock() {
        // Constant phase offset must NOT reduce PLV — that's the whole point
        // of phase-locking-value: it measures CONSISTENCY of the offset, not
        // its size. EEG = sin(2π·5·t), envelope = cos(2π·5·t) (90° offset).
        let n = (SR * 6.0) as usize;
        let eeg: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 5.0 * i as f64 / SR).sin())
            .collect();
        let env: Vec<f64> = (0..n)
            .map(|i| (2.0 * PI * 5.0 * i as f64 / SR).cos())
            .collect();
        let plv = compute_envelope_plv(&eeg, &env, SR);
        assert!(
            plv > 0.85,
            "Phase-shifted but coherent signals should still PLV high, got {plv:.3}"
        );
    }

    #[test]
    fn envelope_plv_in_zero_to_one() {
        let eeg = sine(5.0, SR, 6.0);
        let env = sine(5.0, SR, 6.0);
        let plv = compute_envelope_plv(&eeg, &env, SR);
        assert!(plv >= 0.0 && plv <= 1.0, "envelope PLV out of [0,1]: {plv}");
    }

    #[test]
    fn envelope_plv_zero_for_short_signal() {
        let eeg = vec![0.0; 10];
        let env = vec![0.0; 10];
        let plv = compute_envelope_plv(&eeg, &env, SR);
        assert_eq!(plv, 0.0);
    }

    #[test]
    fn envelope_plv_field_present_when_envelope_supplied() {
        let eeg = sine(5.0, SR, 5.0);
        let env = sine(5.0, SR, 5.0);
        let pv = PerformanceVector::compute_with_envelope(&eeg, &[], SR, None, Some(&env));
        assert!(
            pv.envelope_plv.is_some(),
            "envelope_plv should be present when envelope reference is supplied"
        );
        assert!(
            pv.envelope_plv.unwrap() > 0.85,
            "self-locked envelope_plv should be high"
        );
    }

    #[test]
    fn envelope_plv_none_for_legacy_compute() {
        // Backward-compat: callers using the legacy `compute()` (no envelope)
        // must see envelope_plv = None.
        let eeg = sine(10.0, SR, 5.0);
        let pv = PerformanceVector::compute(&eeg, &[], SR, Some(10.0));
        assert!(
            pv.envelope_plv.is_none(),
            "envelope_plv must be None for legacy compute() with no envelope reference"
        );
    }

    #[test]
    fn envelope_plv_finite_under_real_jr_eeg_shapes() {
        // Stability check: realistic noisy EEG should not produce NaN/Inf.
        let mut rng_state: u64 = 0xBEEF;
        let n = (SR * 5.0) as usize;
        let eeg: Vec<f64> = (0..n)
            .map(|i| {
                rng_state ^= rng_state << 13;
                rng_state ^= rng_state >> 7;
                rng_state ^= rng_state << 17;
                let noise = ((rng_state as f64) / (u64::MAX as f64)) * 2.0 - 1.0;
                let alpha = (2.0 * PI * 10.0 * i as f64 / SR).sin();
                let theta = (2.0 * PI * 5.0 * i as f64 / SR).sin();
                0.6 * alpha + 0.3 * theta + 0.2 * noise
            })
            .collect();
        let env: Vec<f64> = (0..n)
            .map(|i| 0.5 + 0.4 * (2.0 * PI * 5.0 * i as f64 / SR).sin())
            .collect();
        let plv = compute_envelope_plv(&eeg, &env, SR);
        assert!(plv.is_finite(), "envelope PLV must be finite, got {plv}");
        assert!((0.0..=1.0).contains(&plv), "envelope PLV out of range: {plv}");
    }
}
