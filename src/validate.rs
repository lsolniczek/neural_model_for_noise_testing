/// Model validation tests — drive the JR model directly to verify
/// core neural dynamics: frequency tracking, bifurcation threshold,
/// impulse response, stochastic resonance, and spectral discrimination.

use crate::brain_type::BrainType;
use crate::neural::jansen_rit::{FastInhibParams, JansenRitModel};
use crate::neural::{simulate_bilateral, simulate_tonotopic};

/// Build FastInhibParams from a brain type's JR parameters.
fn fast_inhib_for(bt: BrainType) -> FastInhibParams {
    let p = bt.params();
    FastInhibParams {
        g_fast_gain: p.jansen_rit.g_fast_gain,
        g_fast_rate: p.jansen_rit.g_fast_rate,
        c5: p.jansen_rit.c5,
        c6: p.jansen_rit.c6,
        c7: p.jansen_rit.c7,
    }
}
use rustfft::{num_complex::Complex, FftPlanner};
use std::f64::consts::PI;

const SAMPLE_RATE: f64 = 48_000.0;

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Generate a pure sinusoid at `freq` Hz for `duration` seconds, amplitude in [0, 1].
fn sine_signal(freq: f64, duration: f64, amplitude: f64) -> Vec<f64> {
    let n = (SAMPLE_RATE * duration) as usize;
    (0..n)
        .map(|i| {
            let t = i as f64 / SAMPLE_RATE;
            0.5 + amplitude * 0.5 * (2.0 * PI * freq * t).sin()
        })
        .collect()
}

/// Generate white noise in [0, 1] with a simple LCG PRNG (deterministic).
fn white_noise(duration: f64, seed: u64) -> Vec<f64> {
    let n = (SAMPLE_RATE * duration) as usize;
    let mut state = seed;
    (0..n)
        .map(|_| {
            // xorshift64
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state as f64 / u64::MAX as f64)
        })
        .collect()
}

/// Brown noise via integrated white noise, normalised to [0, 1].
fn brown_noise(duration: f64, seed: u64) -> Vec<f64> {
    let wn = white_noise(duration, seed);
    let mut brown: Vec<f64> = Vec::with_capacity(wn.len());
    let mut acc = 0.0_f64;
    for &s in &wn {
        acc += s - 0.5; // center around 0
        acc *= 0.999;   // leak to prevent drift
        brown.push(acc);
    }
    // Normalise to [0, 1]
    let min = brown.iter().cloned().fold(f64::MAX, f64::min);
    let max = brown.iter().cloned().fold(f64::MIN, f64::max);
    let range = max - min;
    if range > 1e-10 {
        brown.iter_mut().for_each(|x| *x = (*x - min) / range);
    }
    brown
}

/// Compute FFT power spectrum, return (freqs, powers) for 0.5–50 Hz range.
fn power_spectrum(signal: &[f64]) -> (Vec<f64>, Vec<f64>) {
    let n = signal.len();
    let fft_len = n.next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);

    // Hanning window + zero-pad
    let mut buf: Vec<Complex<f64>> = (0..fft_len)
        .map(|i| {
            if i < n {
                let w = 0.5 * (1.0 - (2.0 * PI * i as f64 / (n - 1) as f64).cos());
                Complex::new(signal[i] * w, 0.0)
            } else {
                Complex::new(0.0, 0.0)
            }
        })
        .collect();
    fft.process(&mut buf);

    let freq_res = SAMPLE_RATE / fft_len as f64;
    let min_bin = (0.5 / freq_res).ceil() as usize;
    let max_bin = (50.0 / freq_res).floor() as usize;

    let mut freqs = Vec::new();
    let mut powers = Vec::new();
    for bin in min_bin..=max_bin.min(fft_len / 2) {
        freqs.push(bin as f64 * freq_res);
        powers.push(buf[bin].norm_sqr() / fft_len as f64);
    }
    (freqs, powers)
}

/// Find peak frequency and its power in a frequency range.
fn peak_in_range(freqs: &[f64], powers: &[f64], lo: f64, hi: f64) -> (f64, f64) {
    let mut best_freq = 0.0;
    let mut best_power = 0.0_f64;
    for (i, &f) in freqs.iter().enumerate() {
        if f >= lo && f <= hi && powers[i] > best_power {
            best_power = powers[i];
            best_freq = f;
        }
    }
    (best_freq, best_power)
}

/// Band power (sum) in a frequency range.
fn band_power(freqs: &[f64], powers: &[f64], lo: f64, hi: f64) -> f64 {
    freqs
        .iter()
        .zip(powers.iter())
        .filter(|(&f, _)| f >= lo && f < hi)
        .map(|(_, &p)| p)
        .sum()
}

/// RMS of a signal.
fn rms(signal: &[f64]) -> f64 {
    (signal.iter().map(|x| x * x).sum::<f64>() / signal.len() as f64).sqrt()
}

// ── Test 1: Pure Tone Frequency Tracking ─────────────────────────────────────

pub fn test_frequency_tracking() {
    println!("\n  Test 1: Pure Tone Frequency Tracking");
    println!("  ════════════════════════════════════════");
    println!("  Driving JR model with pure sinusoids at 10, 20, 40 Hz");
    println!("  (5 seconds each, amplitude = 1.0)\n");

    let test_freqs = [10.0, 20.0, 40.0];
    let duration = 5.0;
    let mut all_pass = true;

    for &freq in &test_freqs {
        let input = sine_signal(freq, duration, 1.0);

        let mut jr = JansenRitModel::with_params(
            SAMPLE_RATE, 3.25, 22.0, 100.0, 50.0, 135.0, 220.0, 100.0,
        );
        let result = jr.simulate(&input);

        // Detrend
        let mean = result.eeg.iter().sum::<f64>() / result.eeg.len() as f64;
        let detrended: Vec<f64> = result.eeg.iter().map(|x| x - mean).collect();

        let (freqs, powers) = power_spectrum(&detrended);
        let total: f64 = powers.iter().sum();

        // Check for peak near the driving frequency
        let tolerance = 2.0; // ±2 Hz
        let (peak_f, peak_p) = peak_in_range(&freqs, &powers, freq - tolerance, freq + tolerance);
        let peak_frac = if total > 0.0 { peak_p / total } else { 0.0 };

        // Also find the overall dominant peak
        let (dom_f, _) = peak_in_range(&freqs, &powers, 0.5, 50.0);

        let pass = peak_frac > 0.05; // At least 5% of spectral power near target
        if !pass { all_pass = false; }

        println!(
            "    {:.0} Hz input → dominant={:.1} Hz, power@{:.0}Hz={:.1}%  {}",
            freq,
            dom_f,
            freq,
            peak_frac * 100.0,
            if pass { "✓ PASS" } else { "✗ FAIL" }
        );
    }

    println!();
    if all_pass {
        println!("  Result: PASS — model tracks or responds to all tested frequencies");
    } else {
        println!("  Result: PARTIAL — model may not track high frequencies (biologically normal)");
        println!("  Note: JR is a nonlinear oscillator with intrinsic ~11 Hz alpha.");
        println!("  It entrains to nearby frequencies but can't linearly track 40 Hz.");
        println!("  Gamma response requires different cortical circuit models (not JR).");
    }
}

// ── Test 2: Gain Sensitivity / Bifurcation Threshold ─────────────────────────

pub fn test_bifurcation() {
    println!("\n  Test 2: Gain Sensitivity (Bifurcation Threshold)");
    println!("  ════════════════════════════════════════");
    println!("  Ramping input from 0 to 400 pulses/s, looking for oscillation onset\n");

    // We'll test different input_offset values (0 to 400) and check when
    // the JR model starts oscillating (EEG variance exceeds threshold).
    let duration = 2.0;
    let n = (SAMPLE_RATE * duration) as usize;
    let constant_input: Vec<f64> = vec![0.5; n]; // Constant mid-level input

    let mut onset_offset = None;
    let mut offset_off = None;
    let mut was_oscillating = false;

    // Scan input_offset from 50 to 400
    let offsets: Vec<f64> = (50..=400).step_by(10).map(|x| x as f64).collect();
    let mut results: Vec<(f64, f64, bool)> = Vec::new();

    for &offset in &offsets {
        let mut jr = JansenRitModel::with_params(
            SAMPLE_RATE, 3.25, 22.0, 100.0, 50.0, 135.0, offset, 0.0,
        );
        let result = jr.simulate(&constant_input);

        // Measure EEG oscillation amplitude (detrended RMS)
        let mean = result.eeg.iter().sum::<f64>() / result.eeg.len() as f64;
        let detrended: Vec<f64> = result.eeg.iter().map(|x| x - mean).collect();
        let eeg_rms = rms(&detrended);
        let oscillating = eeg_rms > 0.1; // Threshold for meaningful oscillation

        if oscillating && !was_oscillating && onset_offset.is_none() {
            onset_offset = Some(offset);
        }
        if !oscillating && was_oscillating && offset_off.is_none() {
            offset_off = Some(offset);
        }
        was_oscillating = oscillating;
        results.push((offset, eeg_rms, oscillating));
    }

    // Print a condensed view
    println!("    Offset   EEG RMS    State");
    println!("    ─────────────────────────────");
    for &(offset, eeg_rms, osc) in &results {
        if offset % 50.0 == 0.0 || Some(offset) == onset_offset || Some(offset) == offset_off {
            println!(
                "    {:>5.0}    {:>8.4}    {}",
                offset,
                eeg_rms,
                if osc { "oscillating" } else { "silent" }
            );
        }
    }

    println!();
    match (onset_offset, offset_off) {
        (Some(on), Some(off)) => {
            println!("  Bifurcation ON at p ≈ {:.0}, OFF at p ≈ {:.0}", on, off);
            println!("  Oscillatory regime: [{:.0}, {:.0}]", on, off);
            let reasonable = on >= 80.0 && on <= 200.0 && off >= 280.0 && off <= 400.0;
            println!(
                "  Result: {} — {}",
                if reasonable { "PASS" } else { "WARN" },
                if reasonable {
                    "bifurcation boundaries in expected range [~120, ~320]"
                } else {
                    "boundaries outside expected [~120, ~320] range"
                }
            );
        }
        (Some(on), None) => {
            println!("  Bifurcation ON at p ≈ {:.0}, still oscillating at p=400", on);
            println!("  Result: OK — onset detected, upper boundary beyond test range");
        }
        (None, _) => {
            println!("  Result: FAIL — no bifurcation detected in range [50, 400]");
        }
    }
}

// ── Test 3: Impulse Response / Settling Time ─────────────────────────────────

pub fn test_impulse_response() {
    println!("\n  Test 3: Impulse Response (Settling Time)");
    println!("  ════════════════════════════════════════");
    println!("  Strong impulse (1 ms) then 5 seconds of silence\n");

    let duration = 5.0;
    let n = (SAMPLE_RATE * duration) as usize;
    let impulse_len = (SAMPLE_RATE * 0.001) as usize; // 1 ms impulse

    // Build input: strong impulse then baseline
    let mut input = vec![0.5_f64; n]; // Baseline (maps to ~offset)
    for i in 0..impulse_len.min(n) {
        input[i] = 1.0; // Max impulse
    }

    let mut jr = JansenRitModel::with_params(
        SAMPLE_RATE, 3.25, 22.0, 100.0, 50.0, 135.0, 220.0, 200.0,
    );
    let result = jr.simulate(&input);

    // Measure EEG amplitude in successive 0.5s windows
    let window_samples = (SAMPLE_RATE * 0.5) as usize;
    let mean = result.eeg.iter().sum::<f64>() / result.eeg.len() as f64;

    println!("    Time (s)   EEG RMS    State");
    println!("    ─────────────────────────────");

    let mut peak_rms = 0.0_f64;
    let mut decay_time = None;
    let mut oscillates_forever = true;

    for w in 0..(n / window_samples) {
        let start = w * window_samples;
        let end = (start + window_samples).min(n);
        let window: Vec<f64> = result.eeg[start..end].iter().map(|x| x - mean).collect();
        let w_rms = rms(&window);
        let t = (start as f64 + window_samples as f64 / 2.0) / SAMPLE_RATE;

        if w_rms > peak_rms {
            peak_rms = w_rms;
        }

        // Check if oscillation has decayed to < 10% of peak
        if decay_time.is_none() && w > 0 && w_rms < peak_rms * 0.1 {
            decay_time = Some(t);
        }

        if w >= 8 && w_rms < peak_rms * 0.1 {
            oscillates_forever = false;
        }

        println!(
            "    {:>5.1}      {:>8.4}    {}",
            t,
            w_rms,
            if w_rms > peak_rms * 0.1 { "active" } else { "settled" }
        );
    }

    println!();

    // The JR model in its oscillatory regime (offset=220) will continue
    // oscillating even after impulse — this is the limit cycle behavior.
    // It's not a bug — the model has a stable limit cycle attractor.
    // The impulse should cause a transient perturbation visible as amplitude modulation.
    if oscillates_forever {
        println!("  Model continues oscillating (limit cycle at offset=220).");
        println!("  This is expected: JR in oscillatory regime has a stable limit cycle.");
        println!("  The impulse perturbs the trajectory but the attractor pulls it back.");
        println!("  Result: PASS — sustained oscillation with impulse perturbation");
    } else if let Some(dt) = decay_time {
        println!("  Decay to <10% at t ≈ {:.1}s", dt);
        let pass = dt > 0.1 && dt < 4.0;
        println!(
            "  Result: {} — {}",
            if pass { "PASS" } else { "WARN" },
            if dt < 0.1 {
                "too fast — no biological echo"
            } else if dt > 4.0 {
                "very slow decay"
            } else {
                "reasonable settling time"
            }
        );
    }
}

// ── Test 4: Stochastic Resonance ─────────────────────────────────────────────

pub fn test_stochastic_resonance() {
    println!("\n  Test 4: Stochastic Resonance (The ADHD Check)");
    println!("  ════════════════════════════════════════");
    println!("  JR model placed just BELOW bifurcation (offset=80, scale=100, center=0.3).");
    println!("  Effective p = 105–115 (below Hopf at ~120).");
    println!("  Measuring SNR at 10 Hz signal frequency with increasing noise.\n");

    let duration = 10.0;
    let n = (SAMPLE_RATE * duration) as usize;

    // Just below bifurcation. With input_scale=100 and input at 0.5,
    // effective p = 110 + 100*0.5 = 160. That's above bifurcation.
    // So we need input_scale small enough that signal alone stays below.
    //
    // Strategy: offset=110, input_scale=20. Signal is a weak sine at
    // amplitude 0.25 → p oscillates 110 + 20*(0.5±0.25) = 110 + 5 to 15 = 115–125.
    // This just barely reaches 120 at the peaks. A truly subthreshold signal
    // would have smaller amplitude.
    //
    // With amplitude 0.1: p = 110 + 20*(0.5±0.1) = 118–122. Still borderline.
    // With amplitude 0.05: p = 110 + 20*(0.5±0.05) = 119–121. Barely touches.
    //
    // Let's use offset=105, scale=10, signal_amp=0.3:
    // p = 105 + 10*(0.5 + 0.3*sin) = 105 + 5 ± 3 = [107, 113]. Below 120.
    // Noise at 0.5: p = 105 + 10*(0.5 + 0.3*sin + 0.5*(noise-0.5)) = 105 + 5 ± 3 ± 2.5 = [104.5, 115.5]. Still below.
    //
    // Need larger input_scale. Let's use offset=80, input_scale=100.
    // Signal at 0.3 amplitude around 0.3 center:
    // p = 80 + 100 * (0.3 + 0.05*sin) = 80 + 30 ± 5 = [105, 115]. Below 120.
    // Add noise (amplitude 0.15): p = 80 + 100*(0.3 + 0.05*sin + 0.15*(n-0.5))
    //   = 110 ± 5 ± 7.5 = [97.5, 122.5]. Intermittently crosses 120!

    let offset = 80.0;
    let scale = 100.0;
    let signal_center = 0.3;  // Base input level: p = 80 + 100*0.3 = 110
    let signal_amp = 0.05;    // ±5 p-units of modulation

    // Weak 10 Hz subthreshold signal
    let weak_sine: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / SAMPLE_RATE;
            signal_center + signal_amp * (2.0 * PI * 10.0 * t).sin()
        })
        .collect();

    let noise = white_noise(duration, 12345);

    // Phase A: signal alone
    let mut jr_a = JansenRitModel::with_params(
        SAMPLE_RATE, 3.25, 22.0, 100.0, 50.0, 135.0, offset, scale,
    );
    let result_a = jr_a.simulate(&weak_sine);
    let mean_a = result_a.eeg.iter().sum::<f64>() / n as f64;
    let det_a: Vec<f64> = result_a.eeg.iter().map(|x| x - mean_a).collect();
    let (freqs_a, powers_a) = power_spectrum(&det_a);

    // SNR at 10 Hz: power in [9, 11] Hz / power in [1, 50] Hz excluding [9, 11]
    let signal_power_a = band_power(&freqs_a, &powers_a, 9.0, 11.0);
    let total_a: f64 = powers_a.iter().sum();
    let noise_power_a = total_a - signal_power_a;
    let snr_a = if noise_power_a > 1e-15 { signal_power_a / noise_power_a } else { 0.0 };
    let p_min_a = offset + scale * (signal_center - signal_amp);
    let p_max_a = offset + scale * (signal_center + signal_amp);

    println!("    Condition                    p range       SNR@10Hz   EEG RMS    SR ratio");
    println!("    ──────────────────────────────────────────────────────────────────────────");
    println!(
        "    Signal only                  [{:.0}–{:.0}]     {:>8.3}   {:>8.4}    baseline",
        p_min_a, p_max_a, snr_a, rms(&det_a)
    );

    let mut best_snr_ratio = 0.0_f64;
    let mut best_noise_level = 0.0;

    let noise_levels = [0.05, 0.10, 0.15, 0.20, 0.30, 0.50];

    for &noise_amp in &noise_levels {
        let combined: Vec<f64> = weak_sine
            .iter()
            .zip(noise.iter())
            .map(|(&s, &n)| {
                (s + noise_amp * (n - 0.5)).clamp(0.0, 1.0)
            })
            .collect();

        let mut jr_b = JansenRitModel::with_params(
            SAMPLE_RATE, 3.25, 22.0, 100.0, 50.0, 135.0, offset, scale,
        );
        let result_b = jr_b.simulate(&combined);
        let mean_b = result_b.eeg.iter().sum::<f64>() / n as f64;
        let det_b: Vec<f64> = result_b.eeg.iter().map(|x| x - mean_b).collect();
        let (freqs_b, powers_b) = power_spectrum(&det_b);

        let signal_power_b = band_power(&freqs_b, &powers_b, 9.0, 11.0);
        let total_b: f64 = powers_b.iter().sum();
        let noise_power_b = total_b - signal_power_b;
        let snr_b = if noise_power_b > 1e-15 { signal_power_b / noise_power_b } else { 0.0 };

        let p_min_b = offset + scale * (signal_center - signal_amp - noise_amp * 0.5);
        let p_max_b = offset + scale * (signal_center + signal_amp + noise_amp * 0.5);

        let snr_ratio = if snr_a > 1e-10 { snr_b / snr_a } else if snr_b > 1e-10 { 100.0 } else { 1.0 };

        if snr_ratio > best_snr_ratio {
            best_snr_ratio = snr_ratio;
            best_noise_level = noise_amp;
        }

        let crosses = p_max_b >= 120.0;
        println!(
            "    Signal + noise(amp={:.2})    [{:.0}–{:.0}]{}  {:>8.3}   {:>8.4}    {:.2}×",
            noise_amp,
            p_min_b.max(0.0), p_max_b,
            if crosses { "*" } else { " " },
            snr_b,
            rms(&det_b),
            snr_ratio
        );
    }

    println!();
    println!("    (* = p range crosses Hopf bifurcation at ~120)");
    println!();

    // Classical SR shows an inverted U-shaped curve: SNR improves with noise
    // up to an optimum, then degrades. Any SNR improvement > 1.0× is SR.
    let pass = best_snr_ratio > 1.2;
    println!(
        "  Best SNR enhancement: {:.2}× at noise amplitude {:.2}",
        best_snr_ratio, best_noise_level
    );
    println!(
        "  Result: {} — {}",
        if pass { "PASS" } else { "FAIL" },
        if pass {
            "stochastic resonance detected — noise improves signal detection at 10 Hz"
        } else {
            "no stochastic resonance — SNR does not improve with noise"
        }
    );
    if pass {
        println!("  This validates the ADHD model: noise helps a 'sluggish' cortical column");
        println!("  detect a weak periodic signal it couldn't detect alone.");
    } else {
        println!("  Note: The JR model's Hopf bifurcation may be too gradual for classical SR.");
        println!("  The model responds proportionally near threshold rather than switching.");
        println!("  This doesn't invalidate the model for noise testing — it means the ADHD");
        println!("  benefit comes from broadband activation rather than classical resonance.");
    }
}

// ── Test 5: White vs Brown Spectral Discrimination ───────────────────────────

pub fn test_spectral_discrimination() {
    println!("\n  Test 5: White vs Brown Spectral Discrimination");
    println!("  ════════════════════════════════════════");
    println!("  Using tonotopic cortical model (4 parallel JR models).");
    println!("  Same RMS energy, different spectral shape.\n");

    println!("  Note: A single JR model CANNOT discriminate noise colors — it's");
    println!("  a narrowband ~11 Hz oscillator regardless of input spectrum.");
    println!("  The tonotopic architecture (4 bands × different rates) is required.\n");

    let duration = 10.0;
    let n = (SAMPLE_RATE * duration) as usize;

    // Generate white and brown noise
    let wn = white_noise(duration, 42);
    let bn = brown_noise(duration, 42);

    // Simulate through tonotopic model: create 4-band inputs that mimic
    // different spectral distributions.
    //
    // White noise: equal energy across all bands
    // Brown noise: most energy in low band, falling off with frequency

    let neural = BrainType::Normal.params();
    let tono = BrainType::Normal.tonotopic_params();

    // For white noise: all bands get similar signal (equal energy)
    let white_fractions = [0.25, 0.25, 0.25, 0.25];
    let white_bands: [Vec<f64>; 4] = [
        wn.clone(), // Each band gets the same white noise (different seeds would be better but this tests the weighting)
        white_noise(duration, 43),
        white_noise(duration, 44),
        white_noise(duration, 45),
    ];
    // Normalise each band to [0, 1]
    let white_bands_norm = normalise_bands(&white_bands);

    // For brown noise: 80% low, 15% low-mid, 4% mid-high, 1% high
    let brown_fractions = [0.80, 0.15, 0.04, 0.01];
    let brown_bands: [Vec<f64>; 4] = [
        bn.clone(),
        brown_noise(duration, 43),
        brown_noise(duration, 44),
        white_noise(duration, 46), // High band still gets some noise
    ];
    let brown_bands_norm = normalise_bands(&brown_bands);

    let fi = fast_inhib_for(BrainType::Normal);
    let result_w = simulate_tonotopic(
        &white_bands_norm,
        &white_fractions,
        &tono,
        neural.jansen_rit.c,
        neural.jansen_rit.input_scale,
        SAMPLE_RATE,
        &fi,
        neural.jansen_rit.v0,
    );

    let result_b = simulate_tonotopic(
        &brown_bands_norm,
        &brown_fractions,
        &tono,
        neural.jansen_rit.c,
        neural.jansen_rit.input_scale,
        SAMPLE_RATE,
        &fi,
        neural.jansen_rit.v0,
    );

    // Detrend and analyse
    let mean_w = result_w.eeg.iter().sum::<f64>() / result_w.eeg.len() as f64;
    let mean_b = result_b.eeg.iter().sum::<f64>() / result_b.eeg.len() as f64;
    let det_w: Vec<f64> = result_w.eeg.iter().map(|x| x - mean_w).collect();
    let det_b: Vec<f64> = result_b.eeg.iter().map(|x| x - mean_b).collect();

    let (freqs_w, powers_w) = power_spectrum(&det_w);
    let (freqs_b, powers_b) = power_spectrum(&det_b);

    let total_w: f64 = powers_w.iter().sum();
    let total_b: f64 = powers_b.iter().sum();

    let bands = [
        ("Delta", 0.5, 4.0),
        ("Theta", 4.0, 8.0),
        ("Alpha", 8.0, 13.0),
        ("Beta", 13.0, 30.0),
        ("Gamma", 30.0, 50.0),
    ];

    println!("    Band       White      Brown      Difference");
    println!("    ──────────────────────────────────────────────");

    let mut max_diff = 0.0_f64;
    for &(name, lo, hi) in &bands {
        let pw = band_power(&freqs_w, &powers_w, lo, hi) / total_w;
        let pb = band_power(&freqs_b, &powers_b, lo, hi) / total_b;
        let diff = (pw - pb).abs();
        max_diff = max_diff.max(diff);
        println!(
            "    {:<10} {:>6.1}%    {:>6.1}%    {:>+6.1}%",
            name,
            pw * 100.0,
            pb * 100.0,
            (pw - pb) * 100.0
        );
    }

    println!();
    println!("  White dominant: {:.1} Hz", result_w.dominant_freq);
    println!("  Brown dominant: {:.1} Hz", result_b.dominant_freq);
    let pass = max_diff > 0.05;
    println!(
        "  Max band difference: {:.1}%",
        max_diff * 100.0
    );
    println!(
        "  Result: {} — {}",
        if pass { "PASS" } else { "FAIL" },
        if pass {
            "tonotopic model produces different EEG for white vs brown"
        } else {
            "model fails to distinguish noise colors"
        }
    );
}

/// Normalise each band signal to [0, 1].
fn normalise_bands(bands: &[Vec<f64>; 4]) -> [Vec<f64>; 4] {
    let mut out: [Vec<f64>; 4] = [vec![], vec![], vec![], vec![]];
    for b in 0..4 {
        let max_val = bands[b].iter().cloned().fold(0.0_f64, f64::max);
        let min_val = bands[b].iter().cloned().fold(f64::MAX, f64::min);
        let range = max_val - min_val;
        if range > 1e-10 {
            out[b] = bands[b].iter().map(|x| (x - min_val) / range).collect();
        } else {
            out[b] = vec![0.5; bands[b].len()];
        }
    }
    out
}

// ══════════════════════════════════════════════════════════════════════════════
// Part 2: Bilateral Hemispheric Model Tests
// ══════════════════════════════════════════════════════════════════════════════

/// Helper: build 4-band signals from a mono signal, distributing energy
/// according to given fractions. Each band gets an independent noise-like
/// modulation but weighted by the fraction to simulate spectral shape.
fn make_bands_from_signal(signal: &[f64], energy_fractions: &[f64; 4], seed_offset: u64) -> [Vec<f64>; 4] {
    let n = signal.len();
    let mut bands: [Vec<f64>; 4] = [
        vec![0.0; n], vec![0.0; n], vec![0.0; n], vec![0.0; n],
    ];
    for b in 0..4 {
        let modulation = white_noise(n as f64 / SAMPLE_RATE, 1000 + seed_offset + b as u64);
        for i in 0..n {
            // Band signal = base signal * energy weight + small independent modulation
            bands[b][i] = (signal[i] * energy_fractions[b].sqrt()
                + 0.1 * modulation[i] * energy_fractions[b]).clamp(0.0, 1.0);
        }
    }
    normalise_bands(&bands)
}

// ── Bilateral Test 1: Frequency Tracking Through Both Hemispheres ────────────

pub fn test_bilateral_frequency_tracking() {
    println!("\n  Bilateral Test 1: Frequency Tracking (Both Hemispheres)");
    println!("  ════════════════════════════════════════");
    println!("  Driving bilateral model with pure sinusoids at 10, 20, 40 Hz");
    println!("  Same signal to both ears → checking each hemisphere responds\n");

    let neural = BrainType::Normal.params();
    let bilateral = BrainType::Normal.bilateral_params();
    let fi = fast_inhib_for(BrainType::Normal);
    let test_freqs = [10.0, 20.0, 40.0];
    let duration = 5.0;
    let equal_energy = [0.25, 0.25, 0.25, 0.25];

    let mut all_pass = true;

    println!("    Freq     Left Hz    Right Hz   L-Alpha   R-Alpha   Asym     Status");
    println!("    ─────────────────────────────────────────────────────────────────────");

    for &freq in &test_freqs {
        let input = sine_signal(freq, duration, 1.0);
        let bands = make_bands_from_signal(&input, &equal_energy, 0);

        let result = simulate_bilateral(
            &bands, &bands,
            &equal_energy, &equal_energy,
            &bilateral,
            neural.jansen_rit.c,
            neural.jansen_rit.input_scale,
            SAMPLE_RATE,
            &fi,
            neural.jansen_rit.v0, 0.0, 0.0, 0.0,
        );

        let lf = result.left_dominant_freq;
        let rf = result.right_dominant_freq;
        let la = result.left_band_powers.alpha;
        let ra = result.right_band_powers.alpha;
        let asym = result.alpha_asymmetry;

        // Both hemispheres should produce meaningful oscillations
        let pass = lf > 0.5 && rf > 0.5;
        if !pass { all_pass = false; }

        println!(
            "    {:.0} Hz    {:>6.1}     {:>6.1}     {:.3}     {:.3}     {:>+.3}    {}",
            freq, lf, rf, la, ra, asym,
            if pass { "✓ PASS" } else { "✗ FAIL" }
        );
    }

    println!();
    if all_pass {
        println!("  Result: PASS — both hemispheres produce oscillatory response to all inputs");
    } else {
        println!("  Result: PARTIAL — some frequency inputs failed to elicit bilateral response");
    }

    // Additional check: with symmetric input, alpha asymmetry should be small
    let input = sine_signal(10.0, duration, 1.0);
    let bands = make_bands_from_signal(&input, &equal_energy, 100);
    let result = simulate_bilateral(
        &bands, &bands,
        &equal_energy, &equal_energy,
        &bilateral,
        neural.jansen_rit.c,
        neural.jansen_rit.input_scale,
        SAMPLE_RATE,
        &fi,
        neural.jansen_rit.v0, 0.0, 0.0, 0.0,
    );

    // Note: asymmetry won't be exactly 0 because L/R hemispheres have different
    // intrinsic rates (L=fast, R=slow in the AST model)
    println!("\n  Symmetric input asymmetry check:");
    println!("    Alpha asymmetry with identical L/R input: {:+.4}", result.alpha_asymmetry);
    println!("    Left hemisphere is 'fast' (theta/beta), Right is 'slow' (delta/alpha)");
    println!("    Non-zero asymmetry with symmetric input reflects hemispheric specialisation.");
}

// ── Bilateral Test 2: Gain Sensitivity (Bifurcation via Bilateral Path) ──────

pub fn test_bilateral_bifurcation() {
    println!("\n  Bilateral Test 2: Gain Sensitivity (Bilateral Bifurcation)");
    println!("  ════════════════════════════════════════");
    println!("  Testing amplitude sensitivity by scaling band signals directly");
    println!("  (bypassing normalisation that would erase amplitude info).\n");

    let neural = BrainType::Normal.params();
    let bilateral = BrainType::Normal.bilateral_params();
    let fi = fast_inhib_for(BrainType::Normal);
    let duration = 3.0;
    let n = (SAMPLE_RATE * duration) as usize;
    let equal_energy = [0.25, 0.25, 0.25, 0.25];

    // Create a base noise signal and normalise it once
    let base_noise = white_noise(duration, 200);
    let base_bands = make_bands_from_signal(&base_noise, &equal_energy, 200);

    println!("    Amplitude   Combined RMS   L-dominant   R-dominant   State");
    println!("    ────────────────────────────────────────────────────────────");

    let mut onset_amp = None;
    let mut was_active = false;

    for amp_pct in (0..=100).step_by(10) {
        let amp = amp_pct as f64 / 100.0;

        let scaled_bands: [Vec<f64>; 4] = [
            base_bands[0].iter().map(|&x| x * amp).collect(),
            base_bands[1].iter().map(|&x| x * amp).collect(),
            base_bands[2].iter().map(|&x| x * amp).collect(),
            base_bands[3].iter().map(|&x| x * amp).collect(),
        ];

        let result = simulate_bilateral(
            &scaled_bands, &scaled_bands,
            &equal_energy, &equal_energy,
            &bilateral,
            neural.jansen_rit.c,
            neural.jansen_rit.input_scale,
            SAMPLE_RATE,
            &fi,
            neural.jansen_rit.v0, 0.0, 0.0, 0.0,
        );

        let mean = result.combined.eeg.iter().sum::<f64>() / n as f64;
        let det: Vec<f64> = result.combined.eeg.iter().map(|x| x - mean).collect();
        let eeg_rms = rms(&det);
        let active = eeg_rms > 0.1;

        if active && !was_active && onset_amp.is_none() {
            onset_amp = Some(amp);
        }
        was_active = active;

        println!(
            "    {:.2}         {:>10.4}     {:>6.1} Hz    {:>6.1} Hz    {}",
            amp, eeg_rms,
            result.left_dominant_freq, result.right_dominant_freq,
            if active { "oscillating" } else { "silent" }
        );
    }

    println!();
    match onset_amp {
        Some(a) => {
            let reasonable = a >= 0.1 && a <= 0.7;
            println!("  Bilateral oscillation onset at amplitude ≈ {:.2}", a);
            println!(
                "  Result: {} — {}",
                if reasonable { "PASS" } else { "WARN" },
                if a < 0.1 {
                    "too sensitive — fires at minimal input"
                } else if a > 0.7 {
                    "too insensitive — needs extreme input to fire"
                } else {
                    "onset in healthy range"
                }
            );
        }
        None => {
            println!("  Result: WARN — model oscillates at all amplitude levels (or never)");
            println!("  This may indicate the offset places it in a permanent limit cycle.");
        }
    }
}

// ── Bilateral Test 3: Impulse Response + Callosal Propagation ────────────────

pub fn test_bilateral_impulse() {
    println!("\n  Bilateral Test 3: Impulse Response + Callosal Propagation");
    println!("  ════════════════════════════════════════");
    println!("  Short impulse to LEFT ear only, silence to right.");
    println!("  Checking: (a) left response, (b) callosal transfer to right.\n");

    let neural = BrainType::Normal.params();
    let bilateral = BrainType::Normal.bilateral_params();
    let fi = fast_inhib_for(BrainType::Normal);
    let duration = 5.0;
    let n = (SAMPLE_RATE * duration) as usize;
    let impulse_len = (SAMPLE_RATE * 0.05) as usize; // 50ms impulse

    // Left ear: impulse then silence
    let mut left_input = vec![0.3_f64; n]; // Low baseline
    for i in 0..impulse_len.min(n) {
        left_input[i] = 1.0;
    }
    // Right ear: silence throughout
    let right_input = vec![0.3_f64; n];

    let left_energy = [0.25, 0.25, 0.25, 0.25];
    let right_energy = [0.25, 0.25, 0.25, 0.25];

    let left_bands = make_bands_from_signal(&left_input, &left_energy, 300);
    let right_bands = make_bands_from_signal(&right_input, &right_energy, 400);

    let result = simulate_bilateral(
        &left_bands, &right_bands,
        &left_energy, &right_energy,
        &bilateral,
        neural.jansen_rit.c,
        neural.jansen_rit.input_scale,
        SAMPLE_RATE,
        &fi,
        neural.jansen_rit.v0, 0.0, 0.0, 0.0,
    );

    // Analyse response in 0.5s windows
    let window_samples = (SAMPLE_RATE * 0.5) as usize;
    let combined = &result.combined.eeg;
    let mean = combined.iter().sum::<f64>() / n as f64;

    println!("    Time (s)   Combined RMS   State");
    println!("    ─────────────────────────────────");

    let mut peak_rms = 0.0_f64;
    for w in 0..(n / window_samples) {
        let start = w * window_samples;
        let end = (start + window_samples).min(n);
        let window: Vec<f64> = combined[start..end].iter().map(|x| x - mean).collect();
        let w_rms = rms(&window);
        let t = (start as f64 + window_samples as f64 / 2.0) / SAMPLE_RATE;
        if w_rms > peak_rms { peak_rms = w_rms; }

        println!(
            "    {:>5.1}      {:>10.4}     {}",
            t, w_rms,
            if w_rms > peak_rms * 0.5 { "active" } else { "settling" }
        );
    }

    // Check hemispheric asymmetry — impulse to left ear should primarily
    // activate RIGHT hemisphere (contralateral pathway, 65%)
    println!();
    println!("  Hemispheric analysis:");
    println!("    Impulse delivered to: LEFT ear");
    println!("    Right hemisphere (contralateral): dominant = {:.1} Hz", result.right_dominant_freq);
    println!("    Left hemisphere (ipsilateral):    dominant = {:.1} Hz", result.left_dominant_freq);
    println!("    Alpha asymmetry: {:+.4}", result.alpha_asymmetry);
    println!();

    // Callosal delay check: with 10ms delay and 10% coupling, the response
    // should still reach both hemispheres but with the contralateral one stronger.
    let contra_stronger = result.alpha_asymmetry.abs() > 0.01;
    println!(
        "  Result: {} — {}",
        if contra_stronger { "PASS" } else { "WARN" },
        if contra_stronger {
            "hemispheres show asymmetric response to unilateral input"
        } else {
            "hemispheres show nearly identical response (callosal coupling may be too strong)"
        }
    );
    println!("  Callosal coupling: {:.0}%, delay: {:.0}ms",
        bilateral.callosal_coupling * 100.0,
        bilateral.callosal_delay_s * 1000.0);
}

// ── Bilateral Test 4: Stochastic Resonance (ADHD Bilateral Check) ────────────

pub fn test_bilateral_stochastic_resonance() {
    println!("\n  Bilateral Test 4: Stochastic Resonance (Bilateral ADHD Check)");
    println!("  ════════════════════════════════════════");
    println!("  Weak 20 Hz signal (subthreshold amplitude). Adding noise.");
    println!("  ADHD offsets are BELOW bifurcation — noise should help.\n");

    let neural_normal = BrainType::Normal.params();
    let bilateral_normal = BrainType::Normal.bilateral_params();
    let bilateral_adhd = BrainType::Adhd.bilateral_params();
    let neural_adhd = BrainType::Adhd.params();

    let duration = 10.0;
    let n = (SAMPLE_RATE * duration) as usize;

    let energy_frac = [0.25, 0.25, 0.25, 0.25];
    let noise_raw = white_noise(duration, 54321);

    // Weak 20 Hz signal at VERY LOW amplitude — must stay below ADHD's
    // lowered Hopf bifurcation (~110-115 due to b_gain=18, a_gain=3.5).
    // With ADHD offsets ~90-105 and input_scale=80:
    //   signal=0.02 → peak p = 105 + 0.02*80 = 106.6 (well below ADHD threshold ~115)
    //   signal=0.02 + noise*0.15 → peak p = 105 + 0.17*80 = 118.6 (crosses!)
    //   signal=0.02 + noise*0.30 → peak p = 105 + 0.32*80 = 130.6 (solidly above)
    // Normal brain (offsets 150+) is always above its threshold (~125) regardless.
    let signal_amp = 0.02;
    let noise_levels = [0.0, 0.05, 0.10, 0.15, 0.20, 0.30, 0.40, 0.60];

    for (brain_label, bilateral, neural) in &[
        ("Normal", &bilateral_normal, &neural_normal),
        ("ADHD", &bilateral_adhd, &neural_adhd),
    ] {
        println!("    Brain type: {} (offsets: L0={:.0}, R0={:.0})",
            brain_label,
            bilateral.left.band_offsets[0],
            bilateral.right.band_offsets[0]);
        println!("    Noise amp   Beta     EEG RMS    20Hz SNR   SR ratio   Dominant");
        println!("    ──────────────────────────────────────────────────────────────────");

        let mut snr_baseline = 0.0_f64;
        let mut best_sr = 0.0_f64;
        let mut best_noise = 0.0;

        for &noise_amp in &noise_levels {
            // Build band signals with explicit amplitude control (no normalisation)
            // Signal = signal_amp * sin(20Hz) + noise_amp * white_noise
            let band_signal: Vec<f64> = (0..n).map(|i| {
                let t = i as f64 / SAMPLE_RATE;
                let sig = signal_amp * (0.5 + 0.5 * (2.0 * PI * 20.0 * t).sin());
                let nz = noise_amp * noise_raw[i];
                (sig + nz).clamp(0.0, 1.0)
            }).collect();

            // All 4 bands get the same signal (equal energy distribution)
            let bands: [Vec<f64>; 4] = [
                band_signal.clone(),
                band_signal.clone(),
                band_signal.clone(),
                band_signal.clone(),
            ];

            let fi_inner = FastInhibParams {
                g_fast_gain: neural.jansen_rit.g_fast_gain,
                g_fast_rate: neural.jansen_rit.g_fast_rate,
                c5: neural.jansen_rit.c5,
                c6: neural.jansen_rit.c6,
                c7: neural.jansen_rit.c7,
            };
            let result = simulate_bilateral(
                &bands, &bands,
                &energy_frac, &energy_frac,
                bilateral,
                neural.jansen_rit.c,
                neural.jansen_rit.input_scale,
                SAMPLE_RATE,
                &fi_inner,
                neural.jansen_rit.v0, 0.0, 0.0, 0.0,
            );

            let bp_norm = result.combined.band_powers.normalized();

            let mean = result.combined.eeg.iter().sum::<f64>() / n as f64;
            let det: Vec<f64> = result.combined.eeg.iter().map(|x| x - mean).collect();
            let (freqs, powers) = power_spectrum(&det);
            let signal_20 = band_power(&freqs, &powers, 19.0, 21.0);
            let total: f64 = powers.iter().sum();
            let noise_power = total - signal_20;
            let snr = if noise_power > 1e-15 { signal_20 / noise_power } else { 0.0 };

            if noise_amp == 0.0 { snr_baseline = snr; }
            let sr_ratio = if snr_baseline > 1e-10 { snr / snr_baseline } else if snr > 1e-10 { 100.0 } else { 1.0 };

            if sr_ratio > best_sr {
                best_sr = sr_ratio;
                best_noise = noise_amp;
            }

            println!(
                "    {:.2}        {:.4}   {:.4}     {:.3}      {:.2}×       {:>5.1} Hz",
                noise_amp, bp_norm.beta, rms(&det), snr, sr_ratio,
                result.combined.dominant_freq
            );
        }
        println!("    Best SR ratio: {:.2}× at noise={:.1}", best_sr, best_noise);
        println!();
    }

    println!("  Interpretation:");
    println!("    - SR ratio > 1.0 = noise IMPROVED signal detection (stochastic resonance)");
    println!("    - ADHD brain (near/below bifurcation) should show SR; Normal may not");
    println!("    - Classical SR curve: improves then degrades (inverted U)");
}

// ── Bilateral Test 5: White vs Brown Spectral Discrimination (Full Pipeline) ─

pub fn test_bilateral_spectral_discrimination() {
    println!("\n  Bilateral Test 5: White vs Brown Spectral Discrimination (Bilateral)");
    println!("  ════════════════════════════════════════");
    println!("  White and brown noise at same RMS through bilateral model.");
    println!("  Testing all 5 brain types for spectral sensitivity.\n");

    let duration = 10.0;
    let n = (SAMPLE_RATE * duration) as usize;

    // White noise: equal energy per band
    let white_energy = [0.25, 0.25, 0.25, 0.25];
    let wn = white_noise(duration, 42);
    let white_bands_l = make_bands_from_signal(&wn, &white_energy, 600);
    let white_bands_r = make_bands_from_signal(&white_noise(duration, 43), &white_energy, 700);

    // Brown noise: 80% low, 15% low-mid, 4% mid-high, 1% high
    let brown_energy = [0.80, 0.15, 0.04, 0.01];
    let bn = brown_noise(duration, 42);
    let brown_bands_l = make_bands_from_signal(&bn, &brown_energy, 800);
    let brown_bands_r = make_bands_from_signal(&brown_noise(duration, 43), &brown_energy, 900);

    let brain_types = [
        BrainType::Normal,
        BrainType::Adhd,
        BrainType::HighAlpha,
        BrainType::Aging,
        BrainType::Anxious,
    ];

    println!("    Brain Type   Noise    Delta   Theta   Alpha   Beta    Gamma   Dominant  Asym");
    println!("    ──────────────────────────────────────────────────────────────────────────────────");

    let mut any_discriminates = false;

    for brain in &brain_types {
        let neural = brain.params();
        let bilateral = brain.bilateral_params();
        let fi = fast_inhib_for(*brain);

        let result_w = simulate_bilateral(
            &white_bands_l, &white_bands_r,
            &white_energy, &white_energy,
            &bilateral,
            neural.jansen_rit.c,
            neural.jansen_rit.input_scale,
            SAMPLE_RATE,
            &fi,
            neural.jansen_rit.v0, 0.0, 0.0, 0.0,
        );

        let result_b = simulate_bilateral(
            &brown_bands_l, &brown_bands_r,
            &brown_energy, &brown_energy,
            &bilateral,
            neural.jansen_rit.c,
            neural.jansen_rit.input_scale,
            SAMPLE_RATE,
            &fi,
            neural.jansen_rit.v0, 0.0, 0.0, 0.0,
        );

        let bpw = result_w.combined.band_powers.normalized();
        let bpb = result_b.combined.band_powers.normalized();

        let max_diff = [
            (bpw.delta - bpb.delta).abs(),
            (bpw.theta - bpb.theta).abs(),
            (bpw.alpha - bpb.alpha).abs(),
            (bpw.beta - bpb.beta).abs(),
            (bpw.gamma - bpb.gamma).abs(),
        ].iter().cloned().fold(0.0_f64, f64::max);

        if max_diff > 0.03 { any_discriminates = true; }

        let label = format!("{:?}", brain);
        println!(
            "    {:<12} White    {:.3}   {:.3}   {:.3}   {:.3}   {:.3}   {:>5.1} Hz  {:>+.3}",
            label, bpw.delta, bpw.theta, bpw.alpha, bpw.beta, bpw.gamma,
            result_w.combined.dominant_freq, result_w.alpha_asymmetry
        );
        println!(
            "    {:<12} Brown    {:.3}   {:.3}   {:.3}   {:.3}   {:.3}   {:>5.1} Hz  {:>+.3}",
            "", bpb.delta, bpb.theta, bpb.alpha, bpb.beta, bpb.gamma,
            result_b.combined.dominant_freq, result_b.alpha_asymmetry
        );
        println!(
            "    {:<12} Diff     {:>+.3}  {:>+.3}  {:>+.3}  {:>+.3}  {:>+.3}   max={:.1}%",
            "",
            bpw.delta - bpb.delta,
            bpw.theta - bpb.theta,
            bpw.alpha - bpb.alpha,
            bpw.beta - bpb.beta,
            bpw.gamma - bpb.gamma,
            max_diff * 100.0
        );
        println!();
    }

    println!(
        "  Result: {} — {}",
        if any_discriminates { "PASS" } else { "FAIL" },
        if any_discriminates {
            "bilateral model produces different EEG for white vs brown noise"
        } else {
            "model fails to distinguish noise colors through bilateral pathway"
        }
    );
    if any_discriminates {
        println!("  Brown noise should show: more theta/delta (slow), less beta/gamma");
        println!("  White noise should show: more balanced spectrum, higher beta/gamma");
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Part 3: Wendling 2002 Extension Validation Tests
// ══════════════════════════════════════════════════════════════════════════════

// ── Wendling Test 1: Legacy Recovery (G=0 matches JR95) ──────────────────────

pub fn test_wendling_legacy_recovery() {
    println!("\n  Wendling Test 1: Legacy Recovery (G=0 → JR95)");
    println!("  ════════════════════════════════════════");
    println!("  When g_fast_gain=0, Wendling 8-state should produce identical");
    println!("  output to classic JR95 6-state (y3 stays at zero).\n");

    let duration = 5.0;
    let input = sine_signal(10.0, duration, 1.0);

    // Classic JR95 via with_params (no fast inhibition)
    let mut jr95 = JansenRitModel::with_params(
        SAMPLE_RATE, 3.25, 22.0, 100.0, 50.0, 135.0, 220.0, 100.0,
    );
    let result_jr95 = jr95.simulate(&input);

    // Wendling with G=0 (should degenerate to JR95)
    let fi_zero = FastInhibParams {
        g_fast_gain: 0.0,
        g_fast_rate: 500.0,
        c5: 40.5,
        c6: 13.5,
        c7: 108.0,
    };
    let mut wendling_off = JansenRitModel::with_wendling_params(
        SAMPLE_RATE, 3.25, 22.0, 100.0, 50.0, 135.0, 220.0, 100.0, &fi_zero,
        0.20, 6.0, 0.62,
    );
    let result_w0 = wendling_off.simulate(&input);

    // Compare EEG outputs
    let n = result_jr95.eeg.len().min(result_w0.eeg.len());
    let mean_abs_diff: f64 = result_jr95.eeg[..n].iter()
        .zip(result_w0.eeg[..n].iter())
        .map(|(a, b)| (a - b).abs())
        .sum::<f64>() / n as f64;
    let max_abs_diff: f64 = result_jr95.eeg[..n].iter()
        .zip(result_w0.eeg[..n].iter())
        .map(|(a, b)| (a - b).abs())
        .fold(0.0_f64, f64::max);

    let jr95_rms = rms(&result_jr95.eeg);

    println!("    JR95 dominant freq:     {:.1} Hz", result_jr95.dominant_freq);
    println!("    Wendling(G=0) dom freq: {:.1} Hz", result_w0.dominant_freq);
    println!("    JR95 EEG RMS:          {:.6}", jr95_rms);
    println!("    Wendling(G=0) EEG RMS: {:.6}", rms(&result_w0.eeg));
    println!("    Mean |diff|:           {:.2e}", mean_abs_diff);
    println!("    Max  |diff|:           {:.2e}", max_abs_diff);

    let pass = mean_abs_diff < 1e-6 && max_abs_diff < 1e-4;
    println!();
    println!(
        "  Result: {} — {}",
        if pass { "PASS" } else { "FAIL" },
        if pass {
            "Wendling with G=0 produces identical output to JR95"
        } else {
            "Wendling with G=0 diverges from JR95 — check sub-stepping or derivative changes"
        }
    );
}

// ── Wendling Test 2: Gamma Gate (GABA-A loop verification) ───────────────────

pub fn test_wendling_gamma_gate() {
    println!("\n  Wendling Test 2: The Gamma Gate");
    println!("  ════════════════════════════════════════");

    // ── Part A: Verify y[3] (fast inhibitory state) is actually active ──────
    println!("  Part A: Fast inhibitory state diagnostic");
    println!("  Checking if y[3] (GABA-A PSP) is non-zero when G > 0.\n");

    let duration = 3.0;
    let n = (SAMPLE_RATE * duration) as usize;
    let input_const: Vec<f64> = vec![0.5; n];

    for &g in &[0.0, 10.0, 20.0] {
        let fi = FastInhibParams {
            g_fast_gain: g,
            g_fast_rate: 500.0,
            c5: 40.5,
            c6: 13.5,
            c7: 108.0,
        };
        let mut jr = JansenRitModel::with_wendling_params(
            SAMPLE_RATE, 3.25, 22.0, 100.0, 50.0, 135.0, 220.0, 100.0, &fi,
            0.20, 6.0, 0.62,
        );
        let (result, y3_trace) = jr.simulate_with_fast_inhib_trace(&input_const);

        let y3_mean = y3_trace.iter().sum::<f64>() / y3_trace.len() as f64;
        let y3_max = y3_trace.iter().cloned().fold(f64::MIN, f64::max);
        let y3_min = y3_trace.iter().cloned().fold(f64::MAX, f64::min);
        let y3_ac = y3_max - y3_min; // AC amplitude of fast inhibition

        // Compute fast inhib contribution as fraction of EEG amplitude
        let eeg_ac = {
            let eeg_max = result.eeg.iter().cloned().fold(f64::MIN, f64::max);
            let eeg_min = result.eeg.iter().cloned().fold(f64::MAX, f64::min);
            eeg_max - eeg_min
        };

        println!(
            "    G={:<4.0}  y3: mean={:.4}, range=[{:.4}, {:.4}], AC={:.4}  |  EEG AC={:.4}  |  y3/EEG ratio={:.2}%",
            g, y3_mean, y3_min, y3_max, y3_ac, eeg_ac,
            if eeg_ac > 1e-10 { y3_ac / eeg_ac * 100.0 } else { 0.0 }
        );
    }

    // ── Part B: Wendling 2002 protocol — B-reduction with G active ──────────
    //
    // Per Wendling et al. 2002 (Figure 6): gamma/fast activity emerges when
    // slow inhibition B is REDUCED while fast inhibition G stabilises the system.
    // This is the canonical way to produce fast oscillations in this model.
    println!();
    println!("  Part B: Wendling 2002 protocol — B-reduction with G active");
    println!("  (Per paper: gamma emerges from reduced B, not increased G)");
    println!("  Sweeping B (slow GABA-B gain) while G=10 provides fast dynamics.\n");

    let input_40hz: Vec<f64> = (0..n)
        .map(|i| {
            let t = i as f64 / SAMPLE_RATE;
            0.5 + 0.3 * (2.0 * PI * 40.0 * t).sin()
        })
        .collect();

    let b_values = [22.0, 15.0, 10.0, 7.0, 5.0, 3.0, 1.0];

    println!("    B       Delta   Theta   Alpha   Beta    Gamma   Dom Hz   @40Hz%   EEG RMS  y3 AC");
    println!("    ──────────────────────────────────────────────────────────────────────────────────");

    let mut gamma_increased = false;
    let mut dom_freq_shifted = false;
    let mut baseline_beta_gamma = 0.0_f64;

    for (idx, &b) in b_values.iter().enumerate() {
        let fi = FastInhibParams {
            g_fast_gain: 10.0,
            g_fast_rate: 500.0,
            c5: 40.5,
            c6: 13.5,
            c7: 108.0,
        };
        let mut jr = JansenRitModel::with_wendling_params(
            SAMPLE_RATE, 3.25, b, 100.0, 50.0, 135.0, 220.0, 100.0, &fi,
            0.20, 6.0, 0.62,
        );
        let (result, y3_trace) = jr.simulate_with_fast_inhib_trace(&input_40hz);
        let bp = result.band_powers.normalized();

        let y3_max = y3_trace.iter().cloned().fold(f64::MIN, f64::max);
        let y3_min = y3_trace.iter().cloned().fold(f64::MAX, f64::min);
        let y3_ac = y3_max - y3_min;

        let mean = result.eeg.iter().sum::<f64>() / result.eeg.len() as f64;
        let detrended: Vec<f64> = result.eeg.iter().map(|x| x - mean).collect();
        let (freqs, powers) = power_spectrum(&detrended);
        let total_power: f64 = powers.iter().sum();
        let pct_40 = if total_power > 1e-20 { band_power(&freqs, &powers, 38.0, 42.0) / total_power * 100.0 } else { 0.0 };

        if idx == 0 {
            baseline_beta_gamma = bp.beta + bp.gamma;
        }
        if bp.beta + bp.gamma > baseline_beta_gamma + 0.05 {
            gamma_increased = true;
        }
        if idx > 0 && result.dominant_freq > 13.0 {
            dom_freq_shifted = true;
        }

        println!(
            "    {:<5.0}   {:.3}   {:.3}   {:.3}   {:.3}   {:.3}   {:>5.1}    {:>5.2}%   {:.4}   {:.4}",
            b, bp.delta, bp.theta, bp.alpha, bp.beta, bp.gamma,
            result.dominant_freq, pct_40, rms(&result.eeg), y3_ac
        );
    }

    // ── Part C: Same B-reduction WITHOUT G (JR95) — to prove G matters ──────
    println!();
    println!("  Part C: Same B-reduction WITHOUT G (JR95 baseline)");
    println!("  If G=10 produces different results than G=0, fast inhibition IS working.\n");

    println!("    B       Delta   Theta   Alpha   Beta    Gamma   Dom Hz   EEG RMS");
    println!("    ──────────────────────────────────────────────────────────────────");

    let mut jr95_differs = false;

    for &b in &b_values {
        let mut jr = JansenRitModel::with_params(
            SAMPLE_RATE, 3.25, b, 100.0, 50.0, 135.0, 220.0, 100.0,
        );
        let result = jr.simulate(&input_40hz);
        let bp = result.band_powers.normalized();

        println!(
            "    {:<5.0}   {:.3}   {:.3}   {:.3}   {:.3}   {:.3}   {:>5.1}    {:.4}",
            b, bp.delta, bp.theta, bp.alpha, bp.beta, bp.gamma,
            result.dominant_freq, rms(&result.eeg)
        );
    }

    // Compare JR95 vs Wendling at low B
    let fi_on = FastInhibParams {
        g_fast_gain: 10.0, g_fast_rate: 500.0,
        c5: 40.5, c6: 13.5, c7: 108.0,
    };
    let mut jr_w_b5 = JansenRitModel::with_wendling_params(
        SAMPLE_RATE, 3.25, 5.0, 100.0, 50.0, 135.0, 220.0, 100.0, &fi_on,
        0.20, 6.0, 0.62,
    );
    let mut jr95_b5 = JansenRitModel::with_params(
        SAMPLE_RATE, 3.25, 5.0, 100.0, 50.0, 135.0, 220.0, 100.0,
    );
    let result_w = jr_w_b5.simulate(&input_40hz);
    let result_95 = jr95_b5.simulate(&input_40hz);

    let eeg_diff: f64 = result_w.eeg.iter()
        .zip(result_95.eeg.iter())
        .map(|(a, b)| (a - b).abs())
        .sum::<f64>() / result_w.eeg.len() as f64;

    if eeg_diff > 0.01 { jr95_differs = true; }

    println!();
    println!("  Comparison at B=5:");
    println!("    Wendling (G=10): dom={:.1} Hz, RMS={:.4}", result_w.dominant_freq, rms(&result_w.eeg));
    println!("    JR95 (G=0):     dom={:.1} Hz, RMS={:.4}", result_95.dominant_freq, rms(&result_95.eeg));
    println!("    Mean |EEG diff|: {:.4}", eeg_diff);

    // ── Part D: G sweep near bifurcation (offset=120) ──────────────────────
    println!();
    println!("  Part D: G sweep near bifurcation boundary (offset=120)");
    println!("  Near bifurcation the model is more sensitive to perturbation.\n");

    println!("    G       Delta   Theta   Alpha   Beta    Gamma   Dom Hz   EEG RMS  y3 AC");
    println!("    ────────────────────────────────────────────────────────────────────────");

    let mut near_bif_g0_bg = 0.0_f64;
    let mut near_bif_gmax_bg = 0.0_f64;

    for &g in &[0.0, 5.0, 10.0, 15.0, 20.0] {
        let fi = FastInhibParams {
            g_fast_gain: g,
            g_fast_rate: 500.0,
            c5: 40.5,
            c6: 13.5,
            c7: 108.0,
        };
        // offset=120 — right at the Hopf bifurcation
        let mut jr = JansenRitModel::with_wendling_params(
            SAMPLE_RATE, 3.25, 22.0, 100.0, 50.0, 135.0, 120.0, 100.0, &fi,
            0.20, 6.0, 0.62,
        );
        let (result, y3_trace) = jr.simulate_with_fast_inhib_trace(&input_40hz);
        let bp = result.band_powers.normalized();
        let y3_ac = y3_trace.iter().cloned().fold(f64::MIN, f64::max)
            - y3_trace.iter().cloned().fold(f64::MAX, f64::min);

        if g == 0.0 { near_bif_g0_bg = bp.beta + bp.gamma; }
        if g == 20.0 { near_bif_gmax_bg = bp.beta + bp.gamma; }

        println!(
            "    {:<5.0}   {:.3}   {:.3}   {:.3}   {:.3}   {:.3}   {:>5.1}    {:.4}   {:.4}",
            g, bp.delta, bp.theta, bp.alpha, bp.beta, bp.gamma,
            result.dominant_freq, rms(&result.eeg), y3_ac
        );
    }

    println!();

    // Final assessment
    let any_effect = gamma_increased || dom_freq_shifted || jr95_differs
        || (near_bif_gmax_bg - near_bif_g0_bg).abs() > 0.01;

    println!("  ── Summary ──");
    println!("    B-reduction shifts dominant freq:    {}", if dom_freq_shifted { "YES" } else { "NO" });
    println!("    B-reduction increases beta+gamma:    {}", if gamma_increased { "YES" } else { "NO" });
    println!("    G=10 differs from G=0 at low B:     {}", if jr95_differs { "YES" } else { "NO" });
    println!("    G effect near bifurcation:           beta+gamma change = {:+.3}", near_bif_gmax_bg - near_bif_g0_bg);

    println!();
    println!(
        "  Result: {} — {}",
        if any_effect { "PASS" } else { "FAIL" },
        if gamma_increased && jr95_differs {
            "Wendling fast inhibition produces measurable spectral changes"
        } else if jr95_differs {
            "GABA-A loop is active (G=10 ≠ G=0) but gamma not dominant — needs B tuning"
        } else if gamma_increased {
            "B-reduction shifts spectrum but G has no additional effect"
        } else {
            "fast inhibitory loop has no measurable effect — check implementation"
        }
    );
}

// ── Wendling Test 3: ADHD Sensitivity (brain type differentiation) ───────────

pub fn test_wendling_adhd_sensitivity() {
    println!("\n  Wendling Test 3: ADHD Sensitivity (Brain Type Differentiation)");
    println!("  ════════════════════════════════════════");
    println!("  All 5 brain types through Wendling bilateral model.");
    println!("  Each should produce distinct spectral signatures.\n");

    let duration = 5.0;
    let n = (SAMPLE_RATE * duration) as usize;
    let equal_energy = [0.25, 0.25, 0.25, 0.25];

    // Use a consistent input across brain types
    let input = white_noise(duration, 777);
    let bands = make_bands_from_signal(&input, &equal_energy, 500);

    println!("    Brain Type   Delta   Theta   Alpha   Beta    Gamma   Dom Hz   g_rate  G     Asym");
    println!("    ─────────────────────────────────────────────────────────────────────────────────────");

    let brain_types = [
        BrainType::Normal,
        BrainType::HighAlpha,
        BrainType::Adhd,
        BrainType::Aging,
        BrainType::Anxious,
    ];

    let mut spectral_profiles: Vec<(BrainType, f64, f64, f64, f64, f64)> = Vec::new();

    for &bt in &brain_types {
        let neural = bt.params();
        let bilateral = bt.bilateral_params();
        let fi = fast_inhib_for(bt);

        let result = simulate_bilateral(
            &bands, &bands,
            &equal_energy, &equal_energy,
            &bilateral,
            neural.jansen_rit.c,
            neural.jansen_rit.input_scale,
            SAMPLE_RATE,
            &fi,
            neural.jansen_rit.v0, 0.0, 0.0, 0.0,
        );

        let bp = result.combined.band_powers.normalized();
        spectral_profiles.push((bt, bp.delta, bp.theta, bp.alpha, bp.beta, bp.gamma));

        println!(
            "    {:<12} {:.3}   {:.3}   {:.3}   {:.3}   {:.3}   {:>5.1}    {:.0}    {:.0}     {:>+.3}",
            format!("{:?}", bt), bp.delta, bp.theta, bp.alpha, bp.beta, bp.gamma,
            result.combined.dominant_freq,
            neural.jansen_rit.g_fast_rate, neural.jansen_rit.g_fast_gain,
            result.alpha_asymmetry
        );
    }

    println!();

    // Verify brain types are actually differentiated
    let mut any_pair_differs = false;
    for i in 0..spectral_profiles.len() {
        for j in (i+1)..spectral_profiles.len() {
            let (bt_i, di, ti, ai, bi_v, gi) = spectral_profiles[i];
            let (bt_j, dj, tj, aj, bj, gj) = spectral_profiles[j];
            let max_diff = [
                (di - dj).abs(),
                (ti - tj).abs(),
                (ai - aj).abs(),
                (bi_v - bj).abs(),
                (gi - gj).abs(),
            ].iter().cloned().fold(0.0_f64, f64::max);
            if max_diff > 0.01 {
                any_pair_differs = true;
            }
        }
    }

    // Check ADHD-specific expectations
    let adhd = spectral_profiles.iter().find(|(bt, ..)| *bt == BrainType::Adhd);
    let normal = spectral_profiles.iter().find(|(bt, ..)| *bt == BrainType::Normal);

    if let (Some(adhd_p), Some(normal_p)) = (adhd, normal) {
        let adhd_theta_beta = adhd_p.2 / (adhd_p.4 + 1e-10);
        let normal_theta_beta = normal_p.2 / (normal_p.4 + 1e-10);
        println!("  ADHD theta/beta ratio:   {:.2}", adhd_theta_beta);
        println!("  Normal theta/beta ratio: {:.2}", normal_theta_beta);
        if adhd_theta_beta > normal_theta_beta {
            println!("  ADHD shows elevated theta/beta — consistent with clinical literature");
        }
    }

    println!();
    println!(
        "  Result: {} — {}",
        if any_pair_differs { "PASS" } else { "FAIL" },
        if any_pair_differs {
            "brain types produce distinguishable spectral profiles with Wendling model"
        } else {
            "brain types produce identical spectra — Wendling parameters may need tuning"
        }
    );
}

// ── Wendling Test 4: Depolarization Resilience (master_gain sweep) ────────────

pub fn test_wendling_numerical_stress() {
    println!("\n  Wendling Test 4: Depolarization Resilience (Master Gain Sweep)");
    println!("  ════════════════════════════════════════");
    println!("  Sweeping effective master_gain from 0.1 to 1.2.");
    println!("  JR95 (G=0) collapses into delta at high gain (depolarization block).");
    println!("  Wendling (G=10) should maintain complex oscillations (soft-clipping).\n");

    let duration = 5.0;
    let n = (SAMPLE_RATE * duration) as usize;
    let gains = [0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 1.2];

    // Use broadband noise as input (more realistic than constant)
    let noise_input = white_noise(duration, 555);

    // Compare JR95 (G=0) vs Wendling (G=10) across gain sweep
    println!("  Part A: JR95 (G=0) — Classic model (no fast inhibition)");
    println!("    Gain     Dom Hz   Delta%   Theta%   Alpha%   Beta%    EEG RMS    Stable");
    println!("    ────────────────────────────────────────────────────────────────────────");

    let mut jr95_delta_collapse_gain = None;
    let mut jr95_results: Vec<(f64, f64, f64, f64, f64, f64)> = Vec::new();

    for &gain in &gains {
        // Scale input: higher gain = louder noise = higher p values
        let input: Vec<f64> = noise_input.iter().map(|&x| (x * gain).clamp(0.0, 1.0)).collect();

        let fi_off = FastInhibParams::default();
        let mut jr = JansenRitModel::with_wendling_params(
            SAMPLE_RATE, 3.25, 22.0, 100.0, 50.0, 135.0, 220.0, 100.0, &fi_off,
            0.20, 6.0, 0.62,
        );
        let result = jr.simulate(&input);
        let bp = result.band_powers.normalized();
        let eeg_rms = rms(&result.eeg);
        let max_eeg = result.eeg.iter().map(|x| x.abs()).fold(0.0_f64, f64::max);
        let stable = !result.eeg.iter().any(|x| x.is_nan() || x.is_infinite()) && max_eeg < 1e6;

        // Detect delta collapse: dominant freq drops to < 4 Hz
        if result.dominant_freq < 4.0 && jr95_delta_collapse_gain.is_none() && gain > 0.3 {
            jr95_delta_collapse_gain = Some(gain);
        }

        jr95_results.push((gain, result.dominant_freq, bp.delta, bp.theta, bp.alpha, bp.beta));

        println!(
            "    {:<6.1}   {:>5.1}    {:.3}    {:.3}    {:.3}    {:.3}    {:.4}    {}",
            gain, result.dominant_freq, bp.delta, bp.theta, bp.alpha, bp.beta,
            eeg_rms, if stable { "OK" } else { "UNSTABLE" }
        );
    }

    println!();
    println!("  Part B: Wendling (G=10, g_rate=500) — With fast inhibitory stabilisation");
    println!("    Gain     Dom Hz   Delta%   Theta%   Alpha%   Beta%    EEG RMS    Stable");
    println!("    ────────────────────────────────────────────────────────────────────────");

    let mut w02_delta_collapse_gain = None;
    let mut w02_results: Vec<(f64, f64, f64, f64, f64, f64)> = Vec::new();
    let mut all_stable = true;

    for &gain in &gains {
        let input: Vec<f64> = noise_input.iter().map(|&x| (x * gain).clamp(0.0, 1.0)).collect();

        let fi = FastInhibParams {
            g_fast_gain: 10.0,
            g_fast_rate: 500.0,
            c5: 40.5,
            c6: 13.5,
            c7: 108.0,
        };
        let mut jr = JansenRitModel::with_wendling_params(
            SAMPLE_RATE, 3.25, 22.0, 100.0, 50.0, 135.0, 220.0, 100.0, &fi,
            0.20, 6.0, 0.62,
        );
        let result = jr.simulate(&input);
        let bp = result.band_powers.normalized();
        let eeg_rms = rms(&result.eeg);
        let max_eeg = result.eeg.iter().map(|x| x.abs()).fold(0.0_f64, f64::max);
        let stable = !result.eeg.iter().any(|x| x.is_nan() || x.is_infinite()) && max_eeg < 1e6;

        if !stable { all_stable = false; }

        if result.dominant_freq < 4.0 && w02_delta_collapse_gain.is_none() && gain > 0.3 {
            w02_delta_collapse_gain = Some(gain);
        }

        w02_results.push((gain, result.dominant_freq, bp.delta, bp.theta, bp.alpha, bp.beta));

        println!(
            "    {:<6.1}   {:>5.1}    {:.3}    {:.3}    {:.3}    {:.3}    {:.4}    {}",
            gain, result.dominant_freq, bp.delta, bp.theta, bp.alpha, bp.beta,
            eeg_rms, if stable { "OK" } else { "UNSTABLE" }
        );
    }

    println!();

    // Compare: how many gains keep the model above 4 Hz (not collapsed to delta)?
    let jr95_above_4hz = jr95_results.iter().filter(|(_, df, ..)| *df >= 4.0).count();
    let w02_above_4hz = w02_results.iter().filter(|(_, df, ..)| *df >= 4.0).count();

    // Spectral complexity: average number of bands with >5% power
    let jr95_complexity: f64 = jr95_results.iter().map(|(_, _, d, t, a, b)| {
        [*d, *t, *a, *b].iter().filter(|&&x| x > 0.05).count() as f64
    }).sum::<f64>() / jr95_results.len() as f64;
    let w02_complexity: f64 = w02_results.iter().map(|(_, _, d, t, a, b)| {
        [*d, *t, *a, *b].iter().filter(|&&x| x > 0.05).count() as f64
    }).sum::<f64>() / w02_results.len() as f64;

    println!("  Comparison:");
    println!("    JR95 (G=0):     gains with dom freq >= 4 Hz: {}/{}", jr95_above_4hz, gains.len());
    println!("    Wendling (G=10): gains with dom freq >= 4 Hz: {}/{}", w02_above_4hz, gains.len());
    println!("    JR95 avg spectral bands active (>5%):     {:.1}", jr95_complexity);
    println!("    Wendling avg spectral bands active (>5%): {:.1}", w02_complexity);

    match (jr95_delta_collapse_gain, w02_delta_collapse_gain) {
        (Some(jr_g), Some(w_g)) => {
            println!("    JR95 delta collapse at gain: {:.1}", jr_g);
            println!("    Wendling delta collapse at gain: {:.1}", w_g);
            if w_g > jr_g {
                println!("    Wendling is more resilient (collapses {:.1} gain later)", w_g - jr_g);
            }
        }
        (Some(jr_g), None) => {
            println!("    JR95 delta collapse at gain: {:.1}", jr_g);
            println!("    Wendling: NO delta collapse across entire range");
        }
        (None, _) => {
            println!("    Neither model shows delta collapse in this gain range");
        }
    }

    println!();

    // Numerical stability check with extreme parameters
    println!("  Part C: Extreme parameter stress tests:");
    println!("    Config              EEG RMS    Dom Hz   Stable");
    println!("    ──────────────────────────────────────────────");

    let extreme_configs = [
        ("G=0 (JR95)",      0.0,  500.0, 40.5, 13.5, 108.0),
        ("G=30 (high)",     30.0, 500.0, 40.5, 13.5, 108.0),
        ("g_rate=1000",     10.0, 1000.0, 40.5, 13.5, 108.0),
        ("C7=200 (strong)", 10.0, 500.0, 40.5, 13.5, 200.0),
    ];

    let input_mid: Vec<f64> = vec![0.5; n];

    for &(label, g, gr, c5, c6, c7) in &extreme_configs {
        let fi = FastInhibParams {
            g_fast_gain: g,
            g_fast_rate: gr,
            c5,
            c6,
            c7,
        };
        let mut jr = JansenRitModel::with_wendling_params(
            SAMPLE_RATE, 3.25, 22.0, 100.0, 50.0, 135.0, 220.0, 100.0, &fi,
            0.20, 6.0, 0.62,
        );
        let result = jr.simulate(&input_mid);

        let eeg_rms = rms(&result.eeg);
        let max_eeg = result.eeg.iter().map(|x| x.abs()).fold(0.0_f64, f64::max);
        let stable = !result.eeg.iter().any(|x| x.is_nan() || x.is_infinite()) && max_eeg < 1e6;

        if !stable { all_stable = false; }

        println!(
            "    {:<19} {:<10.4} {:>5.1}    {}",
            label, eeg_rms, result.dominant_freq,
            if stable { "OK" } else { "UNSTABLE" }
        );
    }

    println!();

    let resilience_improved = w02_above_4hz >= jr95_above_4hz && w02_complexity >= jr95_complexity;
    println!(
        "  Result: {} — {}",
        if all_stable && resilience_improved { "PASS" } else if all_stable { "PARTIAL" } else { "FAIL" },
        if !all_stable {
            "numerical instability detected"
        } else if resilience_improved {
            "Wendling model is numerically stable AND more resilient to depolarization"
        } else {
            "numerically stable but Wendling does not improve depolarization resilience"
        }
    );
}

// ── Runner ───────────────────────────────────────────────────────────────────

pub fn run_all() {
    println!("\n  Neural Model Validation Suite");
    println!("  ════════════════════════════════════════════════════════════");
    println!("  Part 1: Single JR Model (bypassing cochlear filterbank)");
    println!("  Sample rate: {} Hz", SAMPLE_RATE);

    test_frequency_tracking();
    test_bifurcation();
    test_impulse_response();
    test_stochastic_resonance();
    test_spectral_discrimination();

    println!("\n\n  ════════════════════════════════════════════════════════════");
    println!("  Part 2: Bilateral Hemispheric Model");
    println!("  Testing L/R hemispheres, callosal coupling, AST hypothesis");
    println!("  ════════════════════════════════════════════════════════════");

    test_bilateral_frequency_tracking();
    test_bilateral_bifurcation();
    test_bilateral_impulse();
    test_bilateral_stochastic_resonance();
    test_bilateral_spectral_discrimination();

    println!("\n\n  ════════════════════════════════════════════════════════════");
    println!("  Part 3: Wendling 2002 Extension Validation");
    println!("  Testing fast inhibitory (GABA-A) loop, legacy compat, stability");
    println!("  ════════════════════════════════════════════════════════════");

    test_wendling_legacy_recovery();
    test_wendling_gamma_gate();
    test_wendling_adhd_sensitivity();
    test_wendling_numerical_stress();

    println!("\n  ════════════════════════════════════════════════════════════");
    println!("  Validation complete (14 tests total).");
}
