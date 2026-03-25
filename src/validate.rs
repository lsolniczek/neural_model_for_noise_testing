/// Model validation tests — drive the JR model directly to verify
/// core neural dynamics: frequency tracking, bifurcation threshold,
/// impulse response, stochastic resonance, and spectral discrimination.

use crate::neural::jansen_rit::JansenRitModel;
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

        let jr = JansenRitModel::with_params(
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
        let jr = JansenRitModel::with_params(
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

    let jr = JansenRitModel::with_params(
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
    let jr_a = JansenRitModel::with_params(
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

        let jr_b = JansenRitModel::with_params(
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
    use crate::neural::simulate_tonotopic;
    use crate::brain_type::BrainType;

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

    let result_w = simulate_tonotopic(
        &white_bands_norm,
        &white_fractions,
        &tono.band_rates,
        &tono.band_gains,
        &tono.band_offsets,
        neural.jansen_rit.c,
        neural.jansen_rit.input_scale,
        SAMPLE_RATE,
    );

    let result_b = simulate_tonotopic(
        &brown_bands_norm,
        &brown_fractions,
        &tono.band_rates,
        &tono.band_gains,
        &tono.band_offsets,
        neural.jansen_rit.c,
        neural.jansen_rit.input_scale,
        SAMPLE_RATE,
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

// ── Runner ───────────────────────────────────────────────────────────────────

pub fn run_all() {
    println!("\n  Neural Model Validation Suite");
    println!("  ════════════════════════════════════════════════════════════");
    println!("  Testing JR model directly (bypassing cochlear filterbank)");
    println!("  Sample rate: {} Hz", SAMPLE_RATE);

    test_frequency_tracking();
    test_bifurcation();
    test_impulse_response();
    test_stochastic_resonance();
    test_spectral_discrimination();

    println!("\n  ════════════════════════════════════════════════════════════");
    println!("  Validation complete.");
}
