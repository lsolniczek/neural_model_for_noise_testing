/// Slow / fast envelope crossover for Cortical Envelope Tracking (Priority 13a).
///
/// Splits a band envelope signal into a SLOW path (≤ ~10 Hz, the cortical
/// envelope tracking band per Doelling et al. 2014) and a FAST path
/// (> ~10 Hz, the carrier modulation band that ASSR processes today).
///
/// ## Architecture: 1st-order leaky integrator + complementary HP
///
/// The crossover uses a 1st-order Butterworth lowpass (one-pole leaky
/// integrator) and derives the highpass by subtraction: `hp[n] = x[n] −
/// lp[n]`. This is the standard "complementary filter" pair (Smith 2007,
/// *Introduction to Digital Filters*, ch. 9):
///
///   y[n] = α·x[n] + (1−α)·y[n−1]      // 1st-order LP
///   α    = 1 − exp(−2π·f_c / sr)
///   slow = y[n]
///   fast = x[n] − y[n]
///
/// Why 1st-order rather than 2nd-order Butterworth? At the crossover
/// frequency a 2nd-order Butterworth LP has phase −π/2, which makes the
/// complementary HP magnitude at f_c equal to |1 − 0.707·e^(−jπ/2)| = √1.5
/// ≈ +1.76 dB instead of −3 dB. The slow/fast paths are then asymmetric
/// around the crossover. A 1st-order LP has phase −π/4 at f_c, so the
/// complementary HP at f_c is |1 − 0.707·e^(−jπ/4)| = 0.707, giving the
/// textbook symmetric −3 dB on both paths.
///
/// The 6 dB/oct slope is gentle but adequate for the CET use case: at 5 Hz
/// the LP retains ≥85% of input RMS, and at 40 Hz it suppresses to ≤25%.
/// Steeper slopes would buy nothing here because the gammatone front-end
/// already runs an 80 Hz envelope LPF (gammatone.rs:19).
///
/// Reconstruction: lp[n]+hp[n] equals x[n] up to one floating-point ULP per
/// sample (the algebraic identity x = y + (x−y) is preserved by IEEE 754
/// addition when y is computed by a single-multiply-add and the magnitudes
/// are comparable, which they are here). The pipeline mix-down therefore
/// reintroduces no detectable phase artefact.
///
/// ## Refs
/// - Smith JO (2007). *Introduction to Digital Filters with Audio
///   Applications.* W3K Publishing, ch. 9. — 1st-order LP and complementary
///   highpass derivation.
/// - Doelling KB et al. (2014). "Acoustic landmarks drive delta-theta
///   oscillations to enable speech comprehension by facilitating perceptual
///   parsing." *NeuroImage* 85:761-768. — 10 Hz envelope LPF rationale and
///   the 2-9 Hz CET-relevant band.
/// - Ghitza O (2011). "Linking speech perception and neurophysiology: speech
///   decoding guided by cascaded oscillators locked to the input rhythm."
///   *Front Psychol* 2:130. — cascaded slow/fast envelope architecture.
use std::f64::consts::PI;

/// Default crossover cutoff in Hz. Per Doelling et al. (2014), envelope
/// fluctuations above 10 Hz are not part of the cortical envelope tracking
/// pathway; below 10 Hz (and especially in the 2-9 Hz band) they drive A1.
pub const DEFAULT_CET_CUTOFF_HZ: f64 = 10.0;

/// Maximum permitted reconstruction error (`|lp + hp − x|`) per sample at
/// unit input scale. With 1st-order leaky-integrator math and inputs in
/// [-1, 1], the round-off is well under ε.
pub const RECONSTRUCTION_TOL: f64 = 1e-12;

/// 1st-order leaky integrator LP with complementary HP (= input − LP).
pub struct ButterworthCrossover {
    /// Smoothing coefficient α = 1 − exp(−2π·f_c / sr).
    alpha: f64,
    /// LP state: y[n−1].
    y_prev: f64,
}

impl ButterworthCrossover {
    /// Build a 1st-order LP at `cutoff_hz` for a signal sampled at
    /// `sample_rate_hz`. Smith (2007) ch. 9 derivation: the impulse-invariant
    /// 1st-order Butterworth LP has α = 1 − exp(−ω_c·T) where ω_c = 2π·f_c
    /// and T = 1/sr.
    pub fn new(cutoff_hz: f64, sample_rate_hz: f64) -> Self {
        assert!(cutoff_hz > 0.0, "cutoff must be positive");
        assert!(sample_rate_hz > 0.0, "sample rate must be positive");
        let alpha = 1.0 - (-2.0 * PI * cutoff_hz / sample_rate_hz).exp();
        ButterworthCrossover { alpha, y_prev: 0.0 }
    }

    /// Build with the default 10 Hz CET cutoff for `sample_rate_hz`.
    pub fn cet_default(sample_rate_hz: f64) -> Self {
        Self::new(DEFAULT_CET_CUTOFF_HZ, sample_rate_hz)
    }

    /// Reset filter state. Call between independent signals so previous
    /// state doesn't leak.
    pub fn reset(&mut self) {
        self.y_prev = 0.0;
    }

    /// Process one sample. Returns `(slow_lp, fast_hp)`. By construction
    /// `slow_lp + fast_hp == x` up to one ULP — the HP is computed as
    /// `x − slow_lp` so the algebraic identity is preserved.
    #[inline]
    pub fn process(&mut self, x: f64) -> (f64, f64) {
        // 1st-order LP: y[n] = α·x[n] + (1−α)·y[n−1]
        let slow = self.alpha * x + (1.0 - self.alpha) * self.y_prev;
        self.y_prev = slow;
        let fast = x - slow;
        (slow, fast)
    }

    /// Process a whole signal in one call. Returns (slow, fast) vectors of
    /// the same length as `input`. Filter state is reset at the start.
    pub fn process_signal(&mut self, input: &[f64]) -> (Vec<f64>, Vec<f64>) {
        self.reset();
        let n = input.len();
        let mut slow = Vec::with_capacity(n);
        let mut fast = Vec::with_capacity(n);
        for &x in input {
            let (s, f) = self.process(x);
            slow.push(s);
            fast.push(f);
        }
        (slow, fast)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SR: f64 = 1000.0; // matches NEURAL_SR

    fn sine(freq_hz: f64, secs: f64, sr: f64) -> Vec<f64> {
        let n = (secs * sr) as usize;
        (0..n)
            .map(|i| (2.0 * PI * freq_hz * i as f64 / sr).sin())
            .collect()
    }

    fn rms(signal: &[f64]) -> f64 {
        if signal.is_empty() {
            return 0.0;
        }
        let m = signal.iter().map(|x| x * x).sum::<f64>() / signal.len() as f64;
        m.sqrt()
    }

    #[test]
    fn cet_cutoff_default_is_10hz() {
        // Per Doelling et al. (2014); locked-down so a future change is intentional.
        assert_eq!(DEFAULT_CET_CUTOFF_HZ, 10.0);
    }

    #[test]
    fn unity_sum_reconstruction_is_within_ulp() {
        // The complementary-filter property: lp+hp ≡ x algebraically. In IEEE
        // 754 the round-off error of (slow + (x - slow)) for our 1st-order
        // leaky integrator is bounded by a small multiple of ε, well under
        // RECONSTRUCTION_TOL. Test on white noise to exercise the full range.
        let mut xover = ButterworthCrossover::cet_default(SR);
        let mut rng_state: u64 = 0xC0FFEE;
        let input: Vec<f64> = (0..2000)
            .map(|_| {
                rng_state ^= rng_state << 13;
                rng_state ^= rng_state >> 7;
                rng_state ^= rng_state << 17;
                ((rng_state as f64) / (u64::MAX as f64)) * 2.0 - 1.0
            })
            .collect();

        let (slow, fast) = xover.process_signal(&input);
        let mut max_err = 0.0_f64;
        for (i, &x) in input.iter().enumerate() {
            let sum = slow[i] + fast[i];
            let err = (sum - x).abs();
            if err > max_err {
                max_err = err;
            }
            assert!(
                err <= RECONSTRUCTION_TOL,
                "lp+hp at i={i} reconstruction error {err} > {RECONSTRUCTION_TOL}"
            );
        }
        eprintln!("max reconstruction error: {max_err:e}");
    }

    #[test]
    fn slow_path_passes_5hz_envelope() {
        // 5 Hz is in the CET passband (Doelling 2014) — slow output should
        // retain most of the power. With a 1st-order LP at 10 Hz, gain at
        // 5 Hz is 1/sqrt(1 + 0.5²) ≈ 0.894 (-1 dB).
        let mut xover = ButterworthCrossover::cet_default(SR);
        let input = sine(5.0, 4.0, SR);
        // Drop the first 200 ms to avoid the filter's startup transient.
        let trim = (0.2 * SR) as usize;
        let (slow, fast) = xover.process_signal(&input);
        let in_rms = rms(&input[trim..]);
        let slow_rms = rms(&slow[trim..]);
        let fast_rms = rms(&fast[trim..]);
        let slow_ratio = slow_rms / in_rms;
        let fast_ratio = fast_rms / in_rms;
        eprintln!("5 Hz: slow={slow_ratio:.3} fast={fast_ratio:.3}");
        // 1st-order LP at f_c=10 gives |H(5)| ≈ 0.894. The HP magnitude at
        // f = f_c/2 is sqrt(1 - |H|²) = sqrt(1 - 0.8) ≈ 0.447 (the 6 dB/oct
        // slope is gentle on purpose — see module docstring).
        assert!(
            slow_ratio > 0.85,
            "5 Hz slow ratio={slow_ratio} (expected >0.85)"
        );
        assert!(
            fast_ratio < 0.50,
            "5 Hz fast ratio={fast_ratio} (expected <0.50)"
        );
        // The slow path must dominate at 5 Hz (key CET requirement).
        assert!(
            slow_ratio > fast_ratio,
            "slow ({slow_ratio}) must dominate fast ({fast_ratio}) at 5 Hz"
        );
    }

    #[test]
    fn fast_path_passes_40hz_carrier() {
        // 40 Hz is the canonical ASSR carrier — fast output should dominate.
        // 1st-order LP at 10 Hz: gain at 40 Hz = 1/sqrt(1 + 16) ≈ 0.243 (-12 dB).
        let mut xover = ButterworthCrossover::cet_default(SR);
        let input = sine(40.0, 4.0, SR);
        let trim = (0.2 * SR) as usize;
        let (slow, fast) = xover.process_signal(&input);
        let in_rms = rms(&input[trim..]);
        let slow_rms = rms(&slow[trim..]);
        let fast_rms = rms(&fast[trim..]);
        let slow_ratio = slow_rms / in_rms;
        let fast_ratio = fast_rms / in_rms;
        eprintln!("40 Hz: slow={slow_ratio:.3} fast={fast_ratio:.3}");
        assert!(
            fast_ratio > 0.90,
            "40 Hz fast ratio={fast_ratio} (expected >0.90)"
        );
        assert!(
            slow_ratio < 0.30,
            "40 Hz slow ratio={slow_ratio} (expected <0.30)"
        );
        assert!(
            fast_ratio > slow_ratio,
            "fast ({fast_ratio}) must dominate slow ({slow_ratio}) at 40 Hz"
        );
    }

    #[test]
    fn crossover_at_10hz_is_symmetric_minus_3db() {
        // For the 1st-order LP the LP magnitude at f_c is exactly 1/sqrt(2),
        // and the complementary HP magnitude at f_c is 1 - 0.707·e^(-jπ/4),
        // which evaluates to magnitude 0.707. So both paths carry equal power
        // at the crossover — the textbook symmetric -3 dB split.
        let mut xover = ButterworthCrossover::cet_default(SR);
        let input = sine(10.0, 4.0, SR);
        let trim = (0.2 * SR) as usize;
        let (slow, fast) = xover.process_signal(&input);
        let in_rms = rms(&input[trim..]);
        let slow_ratio = rms(&slow[trim..]) / in_rms;
        let fast_ratio = rms(&fast[trim..]) / in_rms;
        eprintln!("10 Hz: slow={slow_ratio:.3} fast={fast_ratio:.3}");
        // -3 dB ≈ 0.707 RMS ratio. Allow ±0.05 for finite-window RMS noise.
        assert!(
            (slow_ratio - 0.707).abs() < 0.05,
            "10 Hz slow ratio={slow_ratio} (expected ~0.707)"
        );
        assert!(
            (fast_ratio - 0.707).abs() < 0.05,
            "10 Hz fast ratio={fast_ratio} (expected ~0.707)"
        );
    }

    #[test]
    fn dc_goes_entirely_to_slow_path() {
        // DC must end up in the slow path; the fast (HP) path should converge
        // toward zero on a constant input.
        let mut xover = ButterworthCrossover::cet_default(SR);
        let input = vec![0.5_f64; 2000];
        let (slow, fast) = xover.process_signal(&input);
        // After ~200 ms transient the slow path tracks DC and fast path → 0.
        let tail_start = 1000;
        for i in tail_start..2000 {
            assert!(
                (slow[i] - 0.5).abs() < 1e-3,
                "slow[{i}]={} should be ~0.5 (DC tracked)",
                slow[i]
            );
            assert!(
                fast[i].abs() < 1e-3,
                "fast[{i}]={} should be ~0 (DC removed)",
                fast[i]
            );
        }
    }

    #[test]
    fn empty_signal_returns_empty() {
        let mut xover = ButterworthCrossover::cet_default(SR);
        let (slow, fast) = xover.process_signal(&[]);
        assert!(slow.is_empty());
        assert!(fast.is_empty());
    }

    #[test]
    fn output_finite_for_white_noise() {
        let mut xover = ButterworthCrossover::cet_default(SR);
        let mut rng_state: u64 = 0xDEADBEEF;
        let input: Vec<f64> = (0..5000)
            .map(|_| {
                rng_state ^= rng_state << 13;
                rng_state ^= rng_state >> 7;
                rng_state ^= rng_state << 17;
                ((rng_state as f64) / (u64::MAX as f64)) * 2.0 - 1.0
            })
            .collect();
        let (slow, fast) = xover.process_signal(&input);
        for &v in slow.iter().chain(fast.iter()) {
            assert!(v.is_finite(), "non-finite output");
        }
    }

    #[test]
    fn reset_clears_state() {
        let mut xover = ButterworthCrossover::cet_default(SR);
        let pulse: Vec<f64> = std::iter::once(1.0)
            .chain(std::iter::repeat(0.0).take(99))
            .collect();
        let (slow1, _) = xover.process_signal(&pulse);
        // After processing, internal state is non-zero unless we reset.
        // process_signal already resets at start, so two consecutive runs
        // on the same input must produce identical output.
        let (slow2, _) = xover.process_signal(&pulse);
        for (a, b) in slow1.iter().zip(slow2.iter()) {
            assert_eq!(a.to_bits(), b.to_bits(), "reset broken");
        }
    }
}
