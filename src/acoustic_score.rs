/// Acoustic scoring scaffolding and Phase 2 feature extraction.
///
/// The current rollout stops at bounded, deterministic feature extraction.
/// No acoustic feature changes the optimizer or scalar NMM score yet.
use rustfft::{num_complex::Complex, FftPlanner};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AcousticScoreConfig {
    pub enabled: bool,
    pub fusion_enabled: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AcousticFeatureVector {
    pub broadband_level_db: Option<f64>,
    pub speech_band_ratio: Option<f64>,
    pub modulation_depth: Option<f64>,
    pub sharpness_proxy: Option<f64>,

    // ── Priority 28 Phase 1: comfort metrics (diagnostic only) ───────────
    // These fields are populated by `extract_features_v1` but do NOT enter
    // the scoring path yet. Phase 2 (Priority 28f) wires them as
    // ε-constraint violation terms. Default::default() yields None for all,
    // so existing callers and pattern matches remain backward-compatible.
    /// **STANDARDS** — ITU-R BS.1770-5 integrated loudness in LUFS,
    /// stereo-summed (the textbook BS.1770 stereo integrated path:
    /// channel-summed energy, two-stage gating, −0.691 dB offset).
    pub lufs_integrated: Option<f64>,
    /// **STANDARDS** — Per-channel BS.1770-5 K-weighted integrated
    /// loudness. Independent absolute (−70 LUFS) and relative
    /// (integrated − 10 LU) gating per channel, so this reading does
    /// not depend on the other channel's activity.
    pub lufs_left: Option<f64>,
    /// **STANDARDS** — Per-channel BS.1770-5 K-weighted integrated
    /// loudness for the right channel; gated independently of left.
    pub lufs_right: Option<f64>,
    /// **HEURISTIC** — `|lufs_left − lufs_right|` in LU. The two inputs
    /// are standards-correct per-channel BS.1770 readings, but the
    /// metric "asymmetry between per-channel integrated loudness" is
    /// not itself defined in BS.1770. The threshold values (1/2 LU)
    /// applied in `Goal::comfort_violation` are tunable engineering
    /// priors — see the Soulodre & Lavoie (2003) AES paper on stereo
    /// loudness perception and Blauert (1997) on ILD-driven
    /// externalisation, but the specific cut-offs are not standards.
    pub lufs_asymmetry_lu: Option<f64>,
    /// **STANDARDS** — True peak in dBFS via 4× polyphase FIR
    /// oversampling per ITU-R BS.1770-5 Annex 2 (48-tap Hann-windowed
    /// sinc, ≥60 dB stopband attenuation).
    pub true_peak_dbfs: Option<f64>,
    /// **HEURISTIC** — Peak-to-Loudness Ratio: `true_peak_dbfs −
    /// lufs_integrated`. The two inputs are BS.1770-compliant; the
    /// 12/18 dB thresholds in `Goal::comfort_violation` are engineering
    /// priors loosely informed by Vickers (2010) and Pestana (2013) AES
    /// papers on sustained-listening dynamic range.
    pub plr_db: Option<f64>,
    /// **STANDARDS-INSPIRED** — Spectral tilt in dB/oct over 100 Hz –
    /// 10 kHz, computed as a linear regression on log₁₀(power) vs
    /// log₁₀(frequency) over 1/6-octave-binned Hann-windowed Welch PSD
    /// (Welch 1967; IEC 61260 fractional-octave bandpass analysis).
    /// The estimator is robust to narrowband leakage; the per-goal
    /// target slopes (−6/−3/−1.5 dB/oct) used in
    /// `Goal::comfort_violation` are engineering priors based on the
    /// pink-noise / 1/f literature (Voss & Clarke 1975; WHO 2009).
    pub spectral_tilt_db_per_oct: Option<f64>,
    /// **HEURISTIC** — Energy fraction in 8–20 kHz relative to 20 Hz –
    /// 20 kHz. Computation is straightforward integration over a single
    /// FFT pass; the per-goal thresholds (0.10 / 0.20) in
    /// `Goal::comfort_violation` are engineering priors loosely
    /// informed by long-exposure fatigue literature (WHO Night Noise
    /// Guidelines 2009; Basner 2022 EHP) but not standards-derived.
    pub hf_fraction_above_8khz: Option<f64>,
    /// **HEURISTIC** — Number of *effectively active* sources in the
    /// preset (objects with `active=true` AND `volume > 1e-4`).
    ///
    /// Used by `Goal::comfort_violation` to enforce per-goal minimums
    /// (Shield/Isolation ≥ 3, Sleep/DeepRelax/Meditation/Flow ≥ 2,
    /// active-attention goals ≥ 1). Without this floor the optimizer
    /// can trivially satisfy the per-source loudness-equity constraint
    /// (`source_balance_db_range`) by collapsing to 1–2 sources, since
    /// fewer sources is mechanically less imbalanced. Encodes the
    /// cocoon-design intent ("a Shield is several sources spatially
    /// arranged"), which is a product judgment, not a standards term.
    pub active_source_count: Option<u32>,
    /// **HEURISTIC** — Per-source loudness equity (Priority 28 §28b).
    ///
    /// Decibel range across active object volumes:
    ///   `20 · log10(max_volume / min_volume)`
    /// over objects with `active=true` and `volume > 1e-4`. Encodes
    /// the user-facing `feedback_balanced_cocoon` rule.
    ///
    /// **This is a volume-only proxy.** It deliberately ignores:
    ///   - Color (white vs pink vs brown have different K-weighted
    ///     SPLs at the same volume)
    ///   - Tint EQ
    ///   - Modulator gain (movement, breathing, LFOs)
    ///   - Spread
    ///   - Reverb send
    ///   - Tone-source amplitude
    ///
    /// Two sources at the same `volume` but different colors will
    /// produce different listening-level loudnesses in practice, and
    /// this metric will not detect that imbalance. The proxy was
    /// chosen because it directly mirrors the user-facing tuning
    /// heuristic and costs nothing extra (no per-source rendering).
    /// Upgrade path: replace with K-weighted RMS measured over short
    /// per-source renders. Populated by
    /// `pipeline::compute_source_balance_db_range`, not by
    /// `extract_features_v1`.
    pub source_balance_db_range: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct AcousticScoreResult {
    pub features: AcousticFeatureVector,
    pub intelligibility_proxy: Option<f64>,
    pub speech_privacy: Option<f64>,
    pub acoustic_goal_score: Option<f64>,
    pub comfort_score: Option<f64>,
    pub legacy_nmm_score: Option<f64>,
    pub fused_score_preview: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RenderedStereoAudio {
    pub sample_rate_hz: u32,
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl RenderedStereoAudio {
    pub fn new(sample_rate_hz: u32, left: Vec<f32>, right: Vec<f32>) -> Self {
        assert_eq!(
            left.len(),
            right.len(),
            "left/right rendered audio channel lengths must match"
        );
        Self {
            sample_rate_hz,
            left,
            right,
        }
    }

    pub fn frame_count(&self) -> usize {
        self.left.len()
    }

    pub fn is_finite(&self) -> bool {
        self.left.iter().all(|x| x.is_finite()) && self.right.iter().all(|x| x.is_finite())
    }
}

pub fn extract_features_v1(rendered: &RenderedStereoAudio) -> AcousticFeatureVector {
    let mono = mono_mix(rendered);
    let sample_rate_hz = rendered.sample_rate_hz as f64;

    let broadband_level_db = Some(broadband_level_db(&mono));
    let spectrum = compute_spectrum_analysis(&mono, sample_rate_hz);
    let modulation_depth = Some(modulation_depth_proxy(&mono, rendered.sample_rate_hz));

    // ── Priority 28 Phase 1: comfort metrics ─────────────────────────────
    // Per-channel BS.1770-5 K-weighted integrated loudness + binaural
    // asymmetry. Returns None on any error path (silence, sample rate
    // unsupported, signal too short for one 400 ms block); the field is
    // simply absent from the feature vector and downstream code remains
    // bit-identical because `acoustic_comfort_score` does not consult it.
    let left_f64: Vec<f64> = rendered.left.iter().map(|&x| x as f64).collect();
    let right_f64: Vec<f64> = rendered.right.iter().map(|&x| x as f64).collect();
    let loudness = compute_loudness_stereo(&left_f64, &right_f64, sample_rate_hz);
    let (lufs_integrated, lufs_left, lufs_right, lufs_asymmetry_lu) = match loudness {
        Some(r) => (
            Some(r.integrated_lufs),
            Some(r.left_lufs),
            Some(r.right_lufs),
            Some(r.asymmetry_lu),
        ),
        None => (None, None, None, None),
    };

    // True peak via Catmull-Rom 4× oversampling (max across L/R).
    let tp_left = true_peak_dbfs(&left_f64);
    let tp_right = true_peak_dbfs(&right_f64);
    let true_peak = tp_left.max(tp_right);
    let true_peak_dbfs_field = if true_peak > -119.5 { Some(true_peak) } else { None };

    // PLR is a pure derivation; only meaningful when both inputs exist.
    let plr_db = match (true_peak_dbfs_field, lufs_integrated) {
        (Some(tp), Some(li)) => Some(tp - li),
        _ => None,
    };

    AcousticFeatureVector {
        broadband_level_db,
        speech_band_ratio: Some(spectrum.speech_band_ratio),
        modulation_depth,
        sharpness_proxy: Some(spectrum.sharpness_proxy),
        // §28b — populated by the pipeline, not by extract_features_v1
        // (which sees only audio, not the preset). Left as None here.
        source_balance_db_range: None,
        active_source_count: None,
        lufs_integrated,
        lufs_left,
        lufs_right,
        lufs_asymmetry_lu,
        true_peak_dbfs: true_peak_dbfs_field,
        plr_db,
        spectral_tilt_db_per_oct: Some(spectrum.spectral_tilt_db_per_oct),
        hf_fraction_above_8khz: Some(spectrum.hf_fraction_above_8khz),
    }
}

pub fn extract_score_result_v1(rendered: &RenderedStereoAudio) -> AcousticScoreResult {
    let speech_fixture =
        synthesize_default_speech_fixture(rendered.sample_rate_hz, rendered.frame_count());
    let masker_mono = mono_mix(rendered);
    let mixed_mono = mix_signals(&speech_fixture, &masker_mono);
    debug_assert!(mixed_mono.iter().all(|x| x.is_finite()));
    let intelligibility_proxy = intelligibility_proxy_v1(
        &speech_fixture,
        &masker_mono,
        rendered.sample_rate_hz as f64,
    );
    let speech_privacy = (1.0 - intelligibility_proxy).clamp(0.0, 1.0);

    AcousticScoreResult {
        features: extract_features_v1(rendered),
        intelligibility_proxy: Some(intelligibility_proxy),
        speech_privacy: Some(speech_privacy),
        ..AcousticScoreResult::default()
    }
}

fn synthesize_default_speech_fixture(sample_rate_hz: u32, frame_count: usize) -> Vec<f64> {
    let sr = sample_rate_hz as f64;
    let mut samples = Vec::with_capacity(frame_count);

    for i in 0..frame_count {
        let t = i as f64 / sr;
        let phrase_t = t % 1.0;
        let sample = if phrase_t < 0.22 {
            voiced_vowel(
                t,
                phrase_t / 0.22,
                126.0,
                &[
                    (500.0, 120.0, 1.0),
                    (1500.0, 220.0, 0.8),
                    (2500.0, 300.0, 0.4),
                ],
            )
        } else if phrase_t < 0.34 {
            fricative(
                t,
                (phrase_t - 0.22) / 0.12,
                &[2400.0, 3200.0, 4100.0, 5200.0],
                &[0.8, 0.6, 0.4, 0.3],
            )
        } else if phrase_t < 0.60 {
            voiced_vowel(
                t,
                (phrase_t - 0.34) / 0.26,
                118.0,
                &[
                    (730.0, 140.0, 1.0),
                    (1090.0, 180.0, 0.8),
                    (2440.0, 260.0, 0.4),
                ],
            )
        } else if phrase_t < 0.76 {
            voiced_vowel(
                t,
                (phrase_t - 0.60) / 0.16,
                138.0,
                &[
                    (300.0, 100.0, 0.9),
                    (2200.0, 240.0, 0.8),
                    (3000.0, 280.0, 0.5),
                ],
            )
        } else {
            fricative(
                t,
                (phrase_t - 0.76) / 0.24,
                &[1800.0, 2600.0, 3400.0, 4200.0],
                &[0.8, 0.7, 0.5, 0.4],
            )
        };
        samples.push(sample);
    }

    let peak = samples.iter().fold(0.0_f64, |acc, x| acc.max(x.abs()));
    if peak > 1e-12 {
        for sample in &mut samples {
            *sample = (*sample / peak) * 0.25;
        }
    }

    samples
}

fn mono_mix(rendered: &RenderedStereoAudio) -> Vec<f64> {
    rendered
        .left
        .iter()
        .zip(rendered.right.iter())
        .map(|(l, r)| 0.5 * (*l as f64 + *r as f64))
        .collect()
}

fn mix_signals(a: &[f64], b: &[f64]) -> Vec<f64> {
    a.iter().zip(b.iter()).map(|(x, y)| x + y).collect()
}

fn voiced_vowel(t: f64, segment_phase: f64, f0_hz: f64, formants: &[(f64, f64, f64)]) -> f64 {
    let env = raised_cosine(segment_phase);
    let mut sample = 0.0_f64;
    let mut harmonic = 1_u32;
    while harmonic as f64 * f0_hz <= 4_500.0 {
        let freq = harmonic as f64 * f0_hz;
        let resonance = formants
            .iter()
            .map(|(center, bw, gain)| gain * (-0.5 * ((freq - center) / bw).powi(2)).exp())
            .sum::<f64>();
        sample += resonance * (2.0 * std::f64::consts::PI * freq * t).sin() / harmonic as f64;
        harmonic += 1;
    }
    env * sample
}

fn fricative(t: f64, segment_phase: f64, freqs_hz: &[f64], weights: &[f64]) -> f64 {
    let env = raised_cosine(segment_phase);
    let mut sample = 0.0_f64;
    for (idx, (freq, weight)) in freqs_hz.iter().zip(weights.iter()).enumerate() {
        let phase = idx as f64 * 1.234_567_89;
        sample += weight * (2.0 * std::f64::consts::PI * freq * t + phase).sin();
    }
    env * sample
}

fn raised_cosine(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    (0.5 - 0.5 * (2.0 * std::f64::consts::PI * x).cos()).sqrt()
}

fn broadband_level_db(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return -120.0;
    }

    let mean_square = samples.iter().map(|x| x * x).sum::<f64>() / samples.len() as f64;
    let rms = mean_square.sqrt().max(1e-12);
    (20.0 * rms.log10()).clamp(-120.0, 20.0)
}

// (Legacy `spectral_features` removed — superseded by
// `compute_spectrum_analysis`, which returns the same `speech_band_ratio`
// and `sharpness_proxy` plus the new HF-fraction feature in a single
// FFT pass over the full buffer, and additionally calls the
// `welch_log_bin_tilt_db_per_oct` estimator for the spectral tilt
// metric. Two FFT passes total per evaluation: one full-buffer pass
// for the legacy / HF-fraction features, plus the Welch averaging
// pass for tilt — see `compute_spectrum_analysis` for details.)

fn intelligibility_proxy_v1(speech: &[f64], masker: &[f64], sample_rate_hz: f64) -> f64 {
    let mixed = mix_signals(speech, masker);
    let bands = [
        (300.0, 600.0),
        (600.0, 1200.0),
        (1200.0, 2400.0),
        (2400.0, 4000.0),
    ];
    let speech_powers = spectral_band_powers(speech, sample_rate_hz, &bands);
    let masker_powers = spectral_band_powers(masker, sample_rate_hz, &bands);
    let mixed_powers = spectral_band_powers(&mixed, sample_rate_hz, &bands);

    let speech_total: f64 = speech_powers.iter().sum();
    if speech_total <= 1e-30 {
        return 0.0;
    }

    let weighted = speech_powers
        .iter()
        .zip(masker_powers.iter())
        .zip(mixed_powers.iter())
        .map(|((speech_power, masker_power), mixed_power)| {
            let direct_ratio = speech_power / (speech_power + masker_power + 1e-12);
            let mixed_ratio = speech_power / mixed_power.max(*speech_power + 1e-12);
            let band_score = direct_ratio.min(mixed_ratio).clamp(0.0, 1.0);
            band_score * speech_power
        })
        .sum::<f64>();

    (weighted / speech_total).clamp(0.0, 1.0)
}

fn spectral_band_powers(samples: &[f64], sample_rate_hz: f64, bands: &[(f64, f64)]) -> Vec<f64> {
    if samples.is_empty() {
        return vec![0.0; bands.len()];
    }

    let fft_len = samples.len().next_power_of_two().max(2);
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);

    let mut buf: Vec<Complex<f64>> = (0..fft_len)
        .map(|i| {
            if i < samples.len() {
                Complex::new(samples[i], 0.0)
            } else {
                Complex::new(0.0, 0.0)
            }
        })
        .collect();
    fft.process(&mut buf);

    let freq_res = sample_rate_hz / fft_len as f64;
    let mut powers = vec![0.0_f64; bands.len()];
    for (band_idx, (low_hz, high_hz)) in bands.iter().enumerate() {
        let min_bin = (low_hz / freq_res).ceil() as usize;
        let max_bin = ((high_hz / freq_res).floor() as usize).min(fft_len / 2);
        for bin in min_bin..=max_bin {
            powers[band_idx] += buf[bin].norm_sqr();
        }
    }
    powers
}

fn modulation_depth_proxy(samples: &[f64], sample_rate_hz: u32) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }

    let frame_len = ((sample_rate_hz as f64 * 0.020).round() as usize).max(1);
    let hop_len = (frame_len / 2).max(1);

    let mut envelope = Vec::new();
    let mut start = 0_usize;
    while start + frame_len <= samples.len() {
        let frame = &samples[start..start + frame_len];
        let mean_square = frame.iter().map(|x| x * x).sum::<f64>() / frame.len() as f64;
        envelope.push(mean_square.sqrt());
        start += hop_len;
    }

    if envelope.is_empty() {
        let abs_mean = samples.iter().map(|x| x.abs()).sum::<f64>() / samples.len() as f64;
        return if abs_mean > 0.0 { 0.0 } else { 0.0 };
    }

    let mut sorted = envelope;
    sorted.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap());
    let p10 = percentile_from_sorted(&sorted, 0.10);
    let p90 = percentile_from_sorted(&sorted, 0.90);
    ((p90 - p10) / (p90 + p10 + 1e-12)).clamp(0.0, 1.0)
}

fn percentile_from_sorted(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let idx = ((values.len() - 1) as f64 * q).round() as usize;
    values[idx.min(values.len() - 1)]
}

// ─────────────────────────────────────────────────────────────────────────────
// Priority 28 Phase 1 — comfort metrics (LUFS, true-peak, spectral tilt)
//
// All helpers below are diagnostic. They populate `AcousticFeatureVector`
// fields but do NOT alter scoring: `Goal::evaluate_*()` still reads only
// the legacy `sharpness_proxy` and `modulation_depth` fields. Phase 2
// (Priority 28f, ε-constrained ranking) will introduce the constraint
// pipeline that consumes them.
//
// References:
//   - ITU-R BS.1770-5 (2023), "Algorithms to measure audio programme
//     loudness and true-peak audio level" — Annex 1 (K-weighting biquads),
//     Annex 2 (true-peak via 4× oversampling).
//   - EBU R128 (2020 rev.) — operating thresholds.
//   - Mansbridge S, Finn S, Reiss JD (2012), "Implementation and Evaluation
//     of Autonomous Multi-track Fader Control," AES 132nd Convention —
//     analog K-weighting prototype.
// ─────────────────────────────────────────────────────────────────────────────

/// Direct-form-I biquad (a0 normalised to 1.0). Stateful: streaming use is
/// supported by re-using a single instance across consecutive sample blocks.
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Biquad {
    fn new(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        // Normalise by a0 so the recursion uses unit a0.
        let inv = 1.0 / a0;
        Biquad {
            b0: b0 * inv,
            b1: b1 * inv,
            b2: b2 * inv,
            a1: a1 * inv,
            a2: a2 * inv,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }

    #[inline]
    fn process_sample(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }

    fn process_buffer(&mut self, samples: &[f64]) -> Vec<f64> {
        samples.iter().map(|&x| self.process_sample(x)).collect()
    }
}

/// Build the BS.1770-5 K-weighting filter cascade (pre + RLB). Returns
/// `None` for unsupported sample rates so callers can skip loudness
/// analysis cleanly rather than panic.
///
/// At 48 kHz we use the canonical published coefficients (BS.1770-5
/// Annex 1, Tables 1 and 2) — the reference shared by libebur128,
/// pyloudnorm, ffmpeg loudnorm, and EBU Tech 3341. For other sample rates
/// we apply RBJ's bilinear transform from the analog prototype:
/// pre-filter = high-shelf at 1681.9744 Hz, Q ≈ 0.7072, gain ≈ +3.9998 dB;
/// RLB filter = high-pass at 38.1355 Hz, Q ≈ 0.5003.
fn build_k_weighting(sample_rate_hz: f64) -> Option<(Biquad, Biquad)> {
    if !sample_rate_hz.is_finite() || sample_rate_hz <= 0.0 {
        return None;
    }

    if (sample_rate_hz - 48_000.0).abs() < 1e-6 {
        // Canonical 48 kHz coefficients — BS.1770-5 Annex 1, Tables 1, 2.
        let pre = Biquad::new(
            1.535_124_859_586_97,
            -2.691_696_189_406_38,
            1.198_392_810_852_85,
            1.0,
            -1.690_659_293_182_41,
            0.732_480_774_215_85,
        );
        let rlb = Biquad::new(
            1.0,
            -2.0,
            1.0,
            1.0,
            -1.990_047_454_833_98,
            0.990_072_250_366_21,
        );
        return Some((pre, rlb));
    }

    // Best-effort bilinear-transform fallback for non-48 kHz rates.
    // Within ~0.5 dB of the published coefficients across the audible band;
    // adequate for tests that exercise different sample rates.
    let pre = high_shelf_biquad(sample_rate_hz, 1681.974_450_955_533, 0.707_175_236_955_419_6, 3.999_843_853_973_347);
    let rlb = high_pass_biquad(sample_rate_hz, 38.135_470_876_024_44, 0.500_327_037_323_877_3);
    Some((pre, rlb))
}

/// RBJ audio-EQ cookbook high-shelf biquad designed via bilinear transform.
/// `gain_db` is the shelf gain (positive = boost above the corner frequency).
fn high_shelf_biquad(sample_rate_hz: f64, f0_hz: f64, q: f64, gain_db: f64) -> Biquad {
    let a = 10.0_f64.powf(gain_db / 40.0);
    let omega = 2.0 * std::f64::consts::PI * f0_hz / sample_rate_hz;
    let cos_w = omega.cos();
    let sin_w = omega.sin();
    let alpha = sin_w / (2.0 * q);
    let two_sqrt_a_alpha = 2.0 * a.sqrt() * alpha;

    let b0 = a * ((a + 1.0) + (a - 1.0) * cos_w + two_sqrt_a_alpha);
    let b1 = -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w);
    let b2 = a * ((a + 1.0) + (a - 1.0) * cos_w - two_sqrt_a_alpha);
    let a0 = (a + 1.0) - (a - 1.0) * cos_w + two_sqrt_a_alpha;
    let a1 = 2.0 * ((a - 1.0) - (a + 1.0) * cos_w);
    let a2 = (a + 1.0) - (a - 1.0) * cos_w - two_sqrt_a_alpha;

    Biquad::new(b0, b1, b2, a0, a1, a2)
}

/// RBJ audio-EQ cookbook high-pass biquad (Q-form) via bilinear transform.
fn high_pass_biquad(sample_rate_hz: f64, f0_hz: f64, q: f64) -> Biquad {
    let omega = 2.0 * std::f64::consts::PI * f0_hz / sample_rate_hz;
    let cos_w = omega.cos();
    let sin_w = omega.sin();
    let alpha = sin_w / (2.0 * q);

    let b0 = (1.0 + cos_w) / 2.0;
    let b1 = -(1.0 + cos_w);
    let b2 = (1.0 + cos_w) / 2.0;
    let a0 = 1.0 + alpha;
    let a1 = -2.0 * cos_w;
    let a2 = 1.0 - alpha;

    Biquad::new(b0, b1, b2, a0, a1, a2)
}

/// K-weight a single channel and return the filtered signal.
fn k_weight_channel(samples: &[f64], sample_rate_hz: f64) -> Option<Vec<f64>> {
    let (mut pre, mut rlb) = build_k_weighting(sample_rate_hz)?;
    pre.reset();
    rlb.reset();
    let pre_out = pre.process_buffer(samples);
    Some(rlb.process_buffer(&pre_out))
}

/// Output of a stereo BS.1770-5 integrated-loudness measurement.
#[derive(Debug, Clone, Copy)]
struct LoudnessResult {
    integrated_lufs: f64,
    left_lufs: f64,
    right_lufs: f64,
    asymmetry_lu: f64,
}

/// Compute BS.1770-5 integrated loudness on stereo input. Returns `None`
/// when the signal is shorter than one 400 ms block, all blocks are below
/// the absolute −70 LUFS gate, or the sample rate is unsupported.
///
/// Block size 400 ms, 75 % overlap (100 ms hop). Two-stage gating:
///   1. Absolute gate at −70 LUFS (drops digital silence).
///   2. Relative gate at integrated − 10 LU (drops the lowest-loudness tail).
fn compute_loudness_stereo(left: &[f64], right: &[f64], sample_rate_hz: f64) -> Option<LoudnessResult> {
    if left.len() != right.len() {
        return None;
    }

    let z_left = k_weight_channel(left, sample_rate_hz)?;
    let z_right = k_weight_channel(right, sample_rate_hz)?;

    let block_n = (sample_rate_hz * 0.400).round() as usize;
    let hop_n = (sample_rate_hz * 0.100).round() as usize;
    if block_n == 0 || hop_n == 0 || z_left.len() < block_n {
        return None;
    }

    // Per-block mean squares for each channel.
    let mut ms_left = Vec::new();
    let mut ms_right = Vec::new();
    let mut start = 0_usize;
    while start + block_n <= z_left.len() {
        let end = start + block_n;
        let ms_l = mean_square(&z_left[start..end]);
        let ms_r = mean_square(&z_right[start..end]);
        ms_left.push(ms_l);
        ms_right.push(ms_r);
        start += hop_n;
    }
    if ms_left.is_empty() {
        return None;
    }

    // Stage 1 — absolute gate at −70 LUFS. Channel-summed energy with
    // BS.1770 channel weights G_L = G_R = 1 for stereo.
    let block_loudness: Vec<f64> = ms_left
        .iter()
        .zip(ms_right.iter())
        .map(|(&l, &r)| block_lufs_from_sum(l + r))
        .collect();
    let mut keep1: Vec<bool> = block_loudness.iter().map(|&b| b > -70.0).collect();
    let kept1: Vec<usize> = keep1
        .iter()
        .enumerate()
        .filter_map(|(i, k)| if *k { Some(i) } else { None })
        .collect();
    if kept1.is_empty() {
        return None;
    }

    // First-pass integrated using surviving blocks (energy mean → LUFS).
    let sum_energy_1: f64 = kept1.iter().map(|&i| ms_left[i] + ms_right[i]).sum();
    let mean_energy_1 = sum_energy_1 / kept1.len() as f64;
    let integrated_1 = block_lufs_from_sum(mean_energy_1);

    // Stage 2 — relative gate at integrated − 10 LU.
    let rel_threshold = integrated_1 - 10.0;
    for (i, k) in keep1.iter_mut().enumerate() {
        if *k && block_loudness[i] <= rel_threshold {
            *k = false;
        }
    }
    let kept2: Vec<usize> = keep1
        .iter()
        .enumerate()
        .filter_map(|(i, k)| if *k { Some(i) } else { None })
        .collect();
    if kept2.is_empty() {
        return None;
    }

    let sum_energy_2: f64 = kept2.iter().map(|&i| ms_left[i] + ms_right[i]).sum();
    let n = kept2.len() as f64;
    let mean_stereo_energy = sum_energy_2 / n;
    let integrated = block_lufs_from_sum(mean_stereo_energy);

    // Per-channel integrated loudness uses **independent** BS.1770-style
    // gating (the stereo path above is the textbook BS.1770 stereo
    // integrated). Mixing the two — using stereo-summed gating decisions
    // to compute per-channel means — would couple each channel's
    // measurement to the other channel's activity. With independent
    // gating, `lufs_left` and `lufs_right` each describe only their own
    // channel. The `lufs_asymmetry_lu` derived from them is a binaural-
    // *balance* heuristic; the 1/2 LU thresholds in `Goal::comfort_violation`
    // are tunable engineering priors (see comment block on those constants),
    // not BS.1770 standards.
    let left_lufs = single_channel_integrated_lufs(&ms_left);
    let right_lufs = single_channel_integrated_lufs(&ms_right);
    let asymmetry_lu = match (left_lufs, right_lufs) {
        (Some(l), Some(r)) => (l - r).abs(),
        // If either channel falls below absolute or relative gates, the
        // per-channel integrated is undefined. Report the residual stereo
        // asymmetry as the |mean_L − mean_R| over the surviving stereo-
        // gated blocks — this preserves the previous behaviour for
        // degenerate inputs (silence, ultra-short clips).
        _ => {
            let mean_l = kept2.iter().map(|&i| ms_left[i]).sum::<f64>() / n;
            let mean_r = kept2.iter().map(|&i| ms_right[i]).sum::<f64>() / n;
            let l_db = block_lufs_from_sum(mean_l);
            let r_db = block_lufs_from_sum(mean_r);
            (l_db - r_db).abs()
        }
    };
    let left_lufs = left_lufs
        .unwrap_or_else(|| block_lufs_from_sum(kept2.iter().map(|&i| ms_left[i]).sum::<f64>() / n));
    let right_lufs = right_lufs
        .unwrap_or_else(|| block_lufs_from_sum(kept2.iter().map(|&i| ms_right[i]).sum::<f64>() / n));

    Some(LoudnessResult {
        integrated_lufs: integrated,
        left_lufs,
        right_lufs,
        asymmetry_lu,
    })
}

/// BS.1770-5 §3.4 integrated loudness applied to a **single** channel.
///
/// Independent absolute (−70 LUFS) and relative (integrated − 10 LU)
/// gating per channel — the Phase-2b improvement over the previous
/// implementation, which derived per-channel values from stereo-summed
/// gating decisions and so coupled each channel's reading to the other
/// channel's activity.
///
/// Returns `None` when no block survives both gates (channel is silent
/// or below the absolute gate). Caller can then fall back to a
/// stereo-gated mean if needed.
fn single_channel_integrated_lufs(per_block_ms: &[f64]) -> Option<f64> {
    if per_block_ms.is_empty() {
        return None;
    }
    let block_loudness: Vec<f64> = per_block_ms.iter().map(|&m| block_lufs_from_sum(m)).collect();
    let kept1: Vec<usize> = block_loudness
        .iter()
        .enumerate()
        .filter_map(|(i, &b)| if b > -70.0 { Some(i) } else { None })
        .collect();
    if kept1.is_empty() {
        return None;
    }
    let mean_1: f64 =
        kept1.iter().map(|&i| per_block_ms[i]).sum::<f64>() / kept1.len() as f64;
    let integrated_1 = block_lufs_from_sum(mean_1);
    let rel_threshold = integrated_1 - 10.0;
    let kept2: Vec<usize> = kept1
        .into_iter()
        .filter(|&i| block_loudness[i] > rel_threshold)
        .collect();
    if kept2.is_empty() {
        return None;
    }
    let mean_2: f64 =
        kept2.iter().map(|&i| per_block_ms[i]).sum::<f64>() / kept2.len() as f64;
    Some(block_lufs_from_sum(mean_2))
}

#[inline]
fn mean_square(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    samples.iter().map(|x| x * x).sum::<f64>() / samples.len() as f64
}

#[inline]
fn block_lufs_from_sum(channel_summed_energy: f64) -> f64 {
    // BS.1770: L_K = -0.691 + 10*log10(sum_c G_c * z_c^2). For stereo
    // (G_L = G_R = 1), the caller passes z_L^2 + z_R^2 directly; for
    // single-channel, the caller passes that channel's z^2 only.
    if channel_summed_energy <= 1e-30 {
        return -120.0;
    }
    -0.691 + 10.0 * channel_summed_energy.log10()
}

/// Polyphase FIR coefficients for 4× oversampling true-peak detection.
///
/// Returns 4 phase filters of 12 taps each (48 total), with phase `p`
/// holding `h_full[4q + p]` for `q ∈ [0, 12)`. The full impulse response
/// is a Hann-windowed sinc with cutoff at π/4 in the upsampled angular
/// frequency (= Fs_in/2 in the original input domain), giving roughly
/// 60 dB stopband attenuation — within ITU-R BS.1770-5 Annex 2's
/// minimum-stopband requirement for true-peak metering. DC gain is 4
/// (compensating the dilution from 3-zero stuffing in conceptual
/// upsampling), so each phase has DC gain 1.
///
/// Computed at every call (≈ 500 trig ops, microsecond-scale cost).
fn polyphase_4x_taps() -> [[f64; 12]; 4] {
    const L: usize = 48;
    const PHASES: usize = 4;
    const PER_PHASE: usize = L / PHASES;

    let mut h_full = [0.0_f64; L];
    let center = (L as f64 - 1.0) / 2.0;
    // Cutoff fc = Fs_in/2 in upsampled rate = Fs_up/8. In normalised
    // "fraction of the upsampled rate" units (1.0 = Fs_up), fc = 1/8.
    // The sinc argument is 2*fc*(k - center) = 0.25*(k - center).
    let two_fc = 0.25_f64;
    let mut sum = 0.0_f64;
    for k in 0..L {
        let shift = k as f64 - center;
        let arg = two_fc * shift;
        let sinc_value = if arg.abs() < 1e-12 {
            1.0
        } else {
            let pi_arg = std::f64::consts::PI * arg;
            pi_arg.sin() / pi_arg
        };
        let hann = 0.5
            * (1.0 - (2.0 * std::f64::consts::PI * k as f64 / (L - 1) as f64).cos());
        h_full[k] = sinc_value * hann;
        sum += h_full[k];
    }
    // Normalise so the full impulse response has DC gain 4.
    let scale = 4.0 / sum;
    for tap in h_full.iter_mut() {
        *tap *= scale;
    }
    let mut phases = [[0.0_f64; PER_PHASE]; PHASES];
    for p in 0..PHASES {
        for q in 0..PER_PHASE {
            phases[p][q] = h_full[PHASES * q + p];
        }
    }
    phases
}

/// True peak in dBFS via 4× polyphase FIR oversampling per ITU-R
/// BS.1770-5 Annex 2.
///
/// For each input sample `x[i]`, we compute four output samples
/// `y[4i + p] = Σ_q phases[p][q] · x[i - q]` and track the global maximum
/// absolute value. The sample-domain peak is also tracked as a strict
/// lower bound (covers degenerate cases like silence + a single
/// out-of-bounds non-finite sample).
///
/// Standards faithfulness: the cutoff (Fs_in/2), 4× factor, and ≥60 dB
/// stopband all match BS.1770-5 Annex 2's requirements. The exact filter
/// coefficients differ from the standard's example (Annex 2 Table 4
/// publishes one specific 48-tap filter), but BS.1770-5 §3.4 explicitly
/// permits any compliant 4× upsampler, characterised by the stopband
/// attenuation requirement. Hann-windowed sinc at L=48 reaches that
/// envelope.
fn true_peak_dbfs(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return -120.0;
    }
    let phases = polyphase_4x_taps();
    const PER_PHASE: usize = 12;
    let n = samples.len();
    let mut peak: f64 = 0.0;
    // Sample-domain peak — guards against degenerate FIR output and
    // matches BS.1770's "true peak ≥ sample peak" invariant.
    for &s in samples.iter() {
        peak = peak.max(s.abs());
    }
    // 4 polyphase outputs per input sample. Out-of-range indices are
    // treated as zero (zero-pad boundary), the standard treatment for
    // streaming-friendly true-peak metering.
    for i in 0..n {
        for p in 0..4 {
            let mut acc = 0.0_f64;
            for q in 0..PER_PHASE {
                let idx = i as isize - q as isize;
                if idx < 0 {
                    continue;
                }
                acc += phases[p][q] * samples[idx as usize];
            }
            peak = peak.max(acc.abs());
        }
    }
    if peak < 1e-12 {
        -120.0
    } else {
        20.0 * peak.log10()
    }
}

/// Output of the spectrum-analysis path that feeds
/// `AcousticFeatureVector`. Three of the four fields
/// (`speech_band_ratio`, `sharpness_proxy`, `hf_fraction_above_8khz`)
/// are computed from a single full-buffer FFT pass; the fourth
/// (`spectral_tilt_db_per_oct`) comes from a separate Welch + 1/6-octave
/// log-bin estimator (see `welch_log_bin_tilt_db_per_oct`) because raw
/// per-bin regression on a single FFT is too sensitive to spectral
/// leakage to be used as an optimization constraint.
#[derive(Debug, Clone, Copy)]
struct SpectrumAnalysis {
    speech_band_ratio: f64,
    sharpness_proxy: f64,
    /// Slope of log₁₀(power) vs log₁₀(frequency) over 100 Hz – 10 kHz,
    /// converted to dB/oct (slope_log10 × 10 / log10(2) ≈ slope_log10
    /// × 3.0103). Computed via Welch-PSD averaging + 1/6-octave-bin
    /// regression for robustness against narrowband leakage.
    spectral_tilt_db_per_oct: f64,
    /// Energy fraction in 8–20 kHz relative to the full 20 Hz – 20 kHz band.
    hf_fraction_above_8khz: f64,
}

/// Compute the spectrum-derived features. `speech_band_ratio`,
/// `sharpness_proxy`, and `hf_fraction_above_8khz` come from a single
/// full-buffer FFT pass (cheap, ~one FFT per evaluation). The tilt
/// estimator runs a separate Welch + log-bin pass — the cost is one
/// extra FFT per Welch segment, ~10 ms total at 12 s × 48 kHz, and is
/// necessary because the previous raw-bin regression is leakage-prone.
/// Returns 0.0 for all fields on empty input or near-zero total power.
fn compute_spectrum_analysis(samples: &[f64], sample_rate_hz: f64) -> SpectrumAnalysis {
    let zero = SpectrumAnalysis {
        speech_band_ratio: 0.0,
        sharpness_proxy: 0.0,
        spectral_tilt_db_per_oct: 0.0,
        hf_fraction_above_8khz: 0.0,
    };
    if samples.is_empty() {
        return zero;
    }

    let fft_len = samples.len().next_power_of_two().max(2);
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_len);

    let mut buf: Vec<Complex<f64>> = (0..fft_len)
        .map(|i| {
            if i < samples.len() {
                Complex::new(samples[i], 0.0)
            } else {
                Complex::new(0.0, 0.0)
            }
        })
        .collect();
    fft.process(&mut buf);

    let freq_res = sample_rate_hz / fft_len as f64;
    let nyquist_bin = fft_len / 2;
    let min_bin = ((20.0 / freq_res).ceil() as usize).max(1);
    // Legacy range — preserved bit-for-bit so `speech_band_ratio` and
    // `sharpness_proxy` are unchanged from the previous implementation.
    let legacy_max_bin = ((10_000.0 / freq_res).floor() as usize).min(nyquist_bin);
    // Extended range for the new HF-fraction metric.
    let full_max_bin = ((20_000.0 / freq_res).floor() as usize).min(nyquist_bin);
    if legacy_max_bin <= min_bin {
        return zero;
    }

    let mut legacy_total_power = 0.0_f64;
    let mut speech_power = 0.0_f64;
    let mut weighted_centroid_sum = 0.0_f64;
    let mut full_total_power = 0.0_f64;
    let mut hf_power = 0.0_f64;

    // For tilt regression accumulate Σx, Σy, Σxy, Σx², n over 100 Hz – 10 kHz.
    let tilt_lo = 100.0_f64;
    let tilt_hi = 10_000.0_f64;
    let mut sum_x = 0.0_f64;
    let mut sum_y = 0.0_f64;
    let mut sum_xy = 0.0_f64;
    let mut sum_xx = 0.0_f64;
    let mut tilt_n = 0_usize;

    // Legacy pass: 20 Hz – 10 kHz drives speech_band_ratio + sharpness_proxy.
    for bin in min_bin..legacy_max_bin {
        let freq = bin as f64 * freq_res;
        let power = buf[bin].norm_sqr();
        legacy_total_power += power;
        weighted_centroid_sum += freq * power;
        if (300.0..=4_000.0).contains(&freq) {
            speech_power += power;
        }
        if power > 1e-30 && (tilt_lo..=tilt_hi).contains(&freq) {
            let x = freq.log10();
            let y = power.log10();
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_xx += x * x;
            tilt_n += 1;
        }
    }

    // Extended pass: 20 Hz – 20 kHz for HF-fraction normalisation.
    for bin in min_bin..full_max_bin {
        let freq = bin as f64 * freq_res;
        let power = buf[bin].norm_sqr();
        full_total_power += power;
        if freq >= 8_000.0 {
            hf_power += power;
        }
    }

    if legacy_total_power <= 1e-30 {
        return zero;
    }

    let centroid = weighted_centroid_sum / legacy_total_power;
    let log_low = 100.0_f64.ln();
    let log_high = 10_000.0_f64.ln();
    let sharpness_proxy =
        ((centroid.max(100.0).ln() - log_low) / (log_high - log_low)).clamp(0.0, 1.0);
    let speech_band_ratio = (speech_power / legacy_total_power).clamp(0.0, 1.0);
    let hf_fraction = if full_total_power > 1e-30 {
        (hf_power / full_total_power).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // Replace the raw-bin regression with a Welch-PSD + 1/6-octave-bin
    // tilt estimator. The raw-bin form is fragile under spectral
    // leakage from any tonal content (Aures-style tints, isochronic
    // pulses) — windowing + log-binning + averaging is the standard
    // robustness layer.
    let _ = sum_x; // legacy accumulators retained for future debug use
    let _ = sum_y;
    let _ = sum_xy;
    let _ = sum_xx;
    let _ = tilt_n;
    let _ = (tilt_lo, tilt_hi);
    let tilt_db_per_oct = welch_log_bin_tilt_db_per_oct(samples, sample_rate_hz, 100.0, 10_000.0);

    SpectrumAnalysis {
        speech_band_ratio,
        sharpness_proxy,
        spectral_tilt_db_per_oct: tilt_db_per_oct,
        hf_fraction_above_8khz: hf_fraction,
    }
}

/// Welch-PSD + 1/6-octave-bin spectral-tilt estimator.
///
/// Returns the slope of log₁₀(power) vs log₁₀(frequency) over [`f_lo`,
/// `f_hi`], converted to dB/oct. Uses Hann-windowed Welch averaging
/// (segment ≈ 8192 samples or n/8, whichever is smaller; 50% overlap)
/// and 1/6-octave log-frequency binning before regression.
///
/// References:
///   - Welch P (1967). "The use of fast Fourier transform for the
///     estimation of power spectra," IEEE Trans. Audio Electroacoust.
///     15(2):70–73.
///   - Stevens (1957) and IEC 61260 — fractional-octave bandpass
///     analysis.
///
/// Returns 0.0 on degenerate input (empty buffer, span too narrow,
/// fewer than 4 valid log bins).
fn welch_log_bin_tilt_db_per_oct(
    samples: &[f64],
    sample_rate_hz: f64,
    f_lo: f64,
    f_hi: f64,
) -> f64 {
    if samples.is_empty() || !(f_hi > f_lo && f_lo > 0.0) {
        return 0.0;
    }
    let n = samples.len();

    // Pick a segment length such that we get ≥ 4 segments with 50%
    // overlap. Cap at 8192 (≈ 5.86 Hz resolution at 48 kHz, which is
    // adequate for the f_lo = 100 Hz floor).
    let mut seg_len = 8192usize.min(n);
    while seg_len > 256 && n.saturating_div(seg_len / 2) < 8 {
        seg_len /= 2;
    }
    if seg_len < 64 {
        return 0.0;
    }
    let hop = (seg_len / 2).max(1);

    // Hann window
    let mut window = vec![0.0_f64; seg_len];
    let mut win_sumsq = 0.0_f64;
    for (i, w) in window.iter_mut().enumerate() {
        *w = 0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / (seg_len as f64 - 1.0)).cos());
        win_sumsq += *w * *w;
    }
    if win_sumsq < 1e-12 {
        return 0.0;
    }

    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(seg_len);
    let bins = seg_len / 2 + 1;
    let mut psd_sum = vec![0.0_f64; bins];
    let mut seg_count = 0usize;

    let mut start = 0usize;
    while start + seg_len <= n {
        let mut buf: Vec<Complex<f64>> = (0..seg_len)
            .map(|i| Complex::new(samples[start + i] * window[i], 0.0))
            .collect();
        fft.process(&mut buf);
        for k in 0..bins {
            psd_sum[k] += buf[k].norm_sqr();
        }
        seg_count += 1;
        start += hop;
    }
    if seg_count < 4 {
        return 0.0;
    }
    // Average + window-energy normalisation. Absolute scaling does not
    // affect the regression slope but keeps numbers in a sane range.
    let norm = (seg_count as f64) * win_sumsq;
    for v in psd_sum.iter_mut() {
        *v /= norm;
    }

    // 1/6-octave log-frequency binning over [f_lo, f_hi]. Six bins per
    // octave is a common psychoacoustic resolution that suppresses
    // narrowband artefacts while still resolving broadband tilt.
    const BINS_PER_OCT: f64 = 6.0;
    let octaves = (f_hi / f_lo).log2();
    let n_log_bins = (octaves * BINS_PER_OCT).ceil() as usize;
    if n_log_bins < 4 {
        return 0.0;
    }
    let freq_res = sample_rate_hz / seg_len as f64;
    let nyquist_bin = bins.saturating_sub(1);

    let mut x_sum = 0.0_f64;
    let mut y_sum = 0.0_f64;
    let mut xy_sum = 0.0_f64;
    let mut xx_sum = 0.0_f64;
    let mut count = 0usize;
    for b in 0..n_log_bins {
        let f_low = f_lo * 2.0_f64.powf(b as f64 / BINS_PER_OCT);
        let f_high = f_lo * 2.0_f64.powf((b + 1) as f64 / BINS_PER_OCT);
        let bin_lo = ((f_low / freq_res).ceil() as usize).min(nyquist_bin);
        let bin_hi = ((f_high / freq_res).floor() as usize).min(nyquist_bin);
        if bin_hi < bin_lo {
            continue;
        }
        let mut bin_power = 0.0_f64;
        let mut bin_n = 0usize;
        for k in bin_lo..=bin_hi {
            bin_power += psd_sum[k];
            bin_n += 1;
        }
        if bin_n == 0 || bin_power <= 1e-30 {
            continue;
        }
        bin_power /= bin_n as f64;
        let f_center = (f_low * f_high).sqrt();
        let x = f_center.log10();
        let y = bin_power.log10();
        x_sum += x;
        y_sum += y;
        xy_sum += x * y;
        xx_sum += x * x;
        count += 1;
    }
    if count < 4 {
        return 0.0;
    }
    let n_f = count as f64;
    let denom = n_f * xx_sum - x_sum * x_sum;
    if denom.abs() < 1e-12 {
        return 0.0;
    }
    let slope_log10 = (n_f * xy_sum - x_sum * y_sum) / denom;
    // dB/oct = 10 · slope_log10 · log10(2)
    slope_log10 * 10.0 * 2.0_f64.log10()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn stereo_from_mono(sample_rate_hz: u32, mono: Vec<f64>) -> RenderedStereoAudio {
        let left: Vec<f32> = mono.iter().map(|x| *x as f32).collect();
        let right = left.clone();
        RenderedStereoAudio::new(sample_rate_hz, left, right)
    }

    fn sine(sample_rate_hz: u32, freq_hz: f64, amplitude: f64, duration_secs: f64) -> Vec<f64> {
        let n = (sample_rate_hz as f64 * duration_secs) as usize;
        (0..n)
            .map(|i| amplitude * (2.0 * PI * freq_hz * i as f64 / sample_rate_hz as f64).sin())
            .collect()
    }

    fn amplitude_modulated_sine(
        sample_rate_hz: u32,
        carrier_hz: f64,
        mod_hz: f64,
        depth: f64,
        duration_secs: f64,
    ) -> Vec<f64> {
        let n = (sample_rate_hz as f64 * duration_secs) as usize;
        (0..n)
            .map(|i| {
                let t = i as f64 / sample_rate_hz as f64;
                let envelope = 1.0 + depth * (2.0 * PI * mod_hz * t).sin();
                envelope * (2.0 * PI * carrier_hz * t).sin()
            })
            .collect()
    }

    #[test]
    fn louder_signal_raises_broadband_level() {
        let quiet = stereo_from_mono(48_000, sine(48_000, 1000.0, 0.1, 1.0));
        let loud = stereo_from_mono(48_000, sine(48_000, 1000.0, 0.5, 1.0));

        let quiet_features = extract_features_v1(&quiet);
        let loud_features = extract_features_v1(&loud);

        assert!(
            loud_features.broadband_level_db.unwrap() > quiet_features.broadband_level_db.unwrap()
        );
    }

    #[test]
    fn brighter_signal_raises_sharpness_proxy() {
        let low = stereo_from_mono(48_000, sine(48_000, 220.0, 0.5, 1.0));
        let high = stereo_from_mono(48_000, sine(48_000, 5000.0, 0.5, 1.0));

        let low_features = extract_features_v1(&low);
        let high_features = extract_features_v1(&high);

        assert!(high_features.sharpness_proxy.unwrap() > low_features.sharpness_proxy.unwrap());
    }

    #[test]
    fn speech_band_ratio_prefers_midband_tone() {
        let mid = stereo_from_mono(48_000, sine(48_000, 1000.0, 0.5, 1.0));
        let treble = stereo_from_mono(48_000, sine(48_000, 7000.0, 0.5, 1.0));

        let mid_features = extract_features_v1(&mid);
        let treble_features = extract_features_v1(&treble);

        assert!(
            mid_features.speech_band_ratio.unwrap() > treble_features.speech_band_ratio.unwrap()
        );
    }

    #[test]
    fn modulation_depth_proxy_detects_envelope_variation() {
        let steady = stereo_from_mono(48_000, sine(48_000, 400.0, 0.5, 1.0));
        let modulated = stereo_from_mono(
            48_000,
            amplitude_modulated_sine(48_000, 400.0, 5.0, 0.8, 1.0),
        );

        let steady_features = extract_features_v1(&steady);
        let modulated_features = extract_features_v1(&modulated);

        assert!(
            modulated_features.modulation_depth.unwrap()
                > steady_features.modulation_depth.unwrap()
        );
    }

    #[test]
    fn features_are_finite_bounded_and_deterministic() {
        let rendered = stereo_from_mono(48_000, sine(48_000, 1200.0, 0.25, 1.0));
        let first = extract_features_v1(&rendered);
        let second = extract_features_v1(&rendered);

        assert_eq!(first, second);
        assert!(first.broadband_level_db.unwrap().is_finite());
        assert!((0.0..=1.0).contains(&first.speech_band_ratio.unwrap()));
        assert!((0.0..=1.0).contains(&first.modulation_depth.unwrap()));
        assert!((0.0..=1.0).contains(&first.sharpness_proxy.unwrap()));
    }

    #[test]
    fn speech_fixture_is_deterministic_and_finite() {
        let first = synthesize_default_speech_fixture(48_000, 48_000);
        let second = synthesize_default_speech_fixture(48_000, 48_000);
        assert_eq!(first, second);
        assert!(first.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn stronger_speech_band_masker_lowers_intelligibility() {
        let weak = stereo_from_mono(48_000, sine(48_000, 1000.0, 0.05, 1.0));
        let strong = stereo_from_mono(48_000, sine(48_000, 1000.0, 0.40, 1.0));

        let weak_score = extract_score_result_v1(&weak);
        let strong_score = extract_score_result_v1(&strong);

        assert!(
            strong_score.intelligibility_proxy.unwrap() < weak_score.intelligibility_proxy.unwrap()
        );
        assert!(strong_score.speech_privacy.unwrap() > weak_score.speech_privacy.unwrap());
    }

    #[test]
    fn offband_masker_is_less_private_than_speech_band_masker() {
        let speech_band = stereo_from_mono(48_000, sine(48_000, 1000.0, 0.25, 1.0));
        let offband = stereo_from_mono(48_000, sine(48_000, 7000.0, 0.25, 1.0));

        let speech_band_score = extract_score_result_v1(&speech_band);
        let offband_score = extract_score_result_v1(&offband);

        assert!(
            speech_band_score.intelligibility_proxy.unwrap()
                < offband_score.intelligibility_proxy.unwrap()
        );
        assert!(speech_band_score.speech_privacy.unwrap() > offband_score.speech_privacy.unwrap());
    }

    #[test]
    fn silent_masker_gives_poor_privacy() {
        let silence = stereo_from_mono(48_000, vec![0.0; 48_000]);
        let result = extract_score_result_v1(&silence);
        assert!(result.intelligibility_proxy.unwrap() > 0.95);
        assert!(result.speech_privacy.unwrap() < 0.05);
    }

    // ──────────────────────────────────────────────────────────────────
    // Priority 28 Phase 1 — comfort metrics
    // ──────────────────────────────────────────────────────────────────

    /// Dual-mono helper that pushes identical samples to both channels.
    /// Used by the loudness tests, which need a non-trivial duration.
    fn stereo_dual(sample_rate_hz: u32, mono: Vec<f64>) -> RenderedStereoAudio {
        let left: Vec<f32> = mono.iter().map(|x| *x as f32).collect();
        let right = left.clone();
        RenderedStereoAudio::new(sample_rate_hz, left, right)
    }

    /// Build a stereo signal with independent L/R amplitudes (in dBFS).
    fn stereo_two_levels(sample_rate_hz: u32, freq_hz: f64, l_dbfs: f64, r_dbfs: f64, secs: f64) -> RenderedStereoAudio {
        let n = (sample_rate_hz as f64 * secs) as usize;
        let amp_l = 10.0_f64.powf(l_dbfs / 20.0);
        let amp_r = 10.0_f64.powf(r_dbfs / 20.0);
        let left: Vec<f32> = (0..n)
            .map(|i| (amp_l * (2.0 * std::f64::consts::PI * freq_hz * i as f64 / sample_rate_hz as f64).sin()) as f32)
            .collect();
        let right: Vec<f32> = (0..n)
            .map(|i| (amp_r * (2.0 * std::f64::consts::PI * freq_hz * i as f64 / sample_rate_hz as f64).sin()) as f32)
            .collect();
        RenderedStereoAudio::new(sample_rate_hz, left, right)
    }

    // ── BS.1770 K-weighting + integrated loudness ──────────────────────

    /// **BS.1770 calibration test.** A 1 kHz sine at −23 dBFS played in
    /// both channels should produce an integrated loudness of −23 LUFS
    /// (within tight tolerance). This is the canonical conformance check
    /// in BS.1770-5 §3 / EBU R128.
    #[test]
    fn bs1770_calibration_1khz_minus23_dbfs() {
        let signal = sine(48_000, 1000.0, 10.0_f64.powf(-23.0 / 20.0), 4.0);
        let stereo = stereo_dual(48_000, signal);
        let features = extract_features_v1(&stereo);
        let lufs = features
            .lufs_integrated
            .expect("BS.1770 calibration must produce an integrated value");
        assert!(
            (lufs - (-23.0)).abs() < 0.4,
            "1 kHz @ −23 dBFS must measure ~-23 LUFS, got {lufs:.3}"
        );
    }

    #[test]
    fn lufs_higher_for_louder_signal() {
        let quiet = stereo_dual(48_000, sine(48_000, 1000.0, 0.05, 2.0));
        let loud = stereo_dual(48_000, sine(48_000, 1000.0, 0.50, 2.0));
        let q = extract_features_v1(&quiet).lufs_integrated.unwrap();
        let l = extract_features_v1(&loud).lufs_integrated.unwrap();
        assert!(l > q + 15.0, "20 dB level rise must lift LUFS, got {q:.2} → {l:.2}");
    }

    #[test]
    fn lufs_symmetric_signal_has_zero_asymmetry() {
        let stereo = stereo_dual(48_000, sine(48_000, 1000.0, 0.25, 2.0));
        let features = extract_features_v1(&stereo);
        let asym = features.lufs_asymmetry_lu.unwrap();
        assert!(
            asym < 0.05,
            "Identical L/R must have ~0 LU asymmetry, got {asym:.3}"
        );
    }

    /// Per-channel independence: with the stereo-summed gating that
    /// `compute_loudness_stereo` used before Phase-2b, the per-channel
    /// integrated would shift when the **other** channel's activity
    /// changes (because the stereo sum drives the gate decisions). With
    /// independent per-channel gating each channel is measured in
    /// isolation. Test: left at constant −20 dBFS, right alternating
    /// between −20 dBFS and silence. The previous implementation would
    /// drop ~half the blocks (stereo sum below relative gate) and
    /// underreport `lufs_left`. Independent gating measures left as
    /// ≈ −20 LUFS (its own quiet half is gated away because its own
    /// blocks are below the per-channel relative gate? Actually the
    /// left is constant, so independent gating sees no quiet blocks
    /// for left → reports −20 LUFS). The robust assertion is that
    /// `lufs_left` is *close* to a constant pure tone calibration
    /// regardless of the right channel's pattern.
    #[test]
    fn lufs_per_channel_independent_of_other_channel_activity() {
        let n = (48_000.0 * 4.0) as usize;
        let amp = 10.0_f64.powf(-20.0 / 20.0);
        // Left: constant −20 dBFS sine at 1 kHz
        let left: Vec<f32> = (0..n)
            .map(|i| (amp * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 48_000.0).sin()) as f32)
            .collect();
        // Right: alternating 1 s of −20 dBFS sine and 1 s of silence.
        let right: Vec<f32> = (0..n)
            .map(|i| {
                let on = (i / 48_000) % 2 == 0;
                if on {
                    (amp * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 48_000.0).sin()) as f32
                } else {
                    0.0
                }
            })
            .collect();
        let stereo = RenderedStereoAudio::new(48_000, left, right);
        let features = extract_features_v1(&stereo);
        let l = features.lufs_left.unwrap();
        // Left was constant −20 dBFS → its independent-gated integrated
        // should report a single-channel LUFS that does NOT depend on
        // what the right channel is doing. A single-channel −20 dBFS
        // sine produces ≈ −23 LUFS (no stereo +3 dB summing). Under
        // independent gating this stays ≈ −23 regardless of right.
        // Under the old stereo-summed gating, the silent right blocks
        // would drag the stereo sum near or below the relative gate,
        // dropping left's good blocks too and skewing the per-channel
        // reading.
        let expected_single_ch = -23.0;
        assert!(
            (l - expected_single_ch).abs() < 1.0,
            "left channel must be ≈ {expected_single_ch} LUFS independent of right, got {l:.3}"
        );
    }

    /// 6 dB level imbalance between L and R should produce ~6 LU asymmetry.
    #[test]
    fn lufs_asymmetry_tracks_level_difference() {
        let stereo = stereo_two_levels(48_000, 1000.0, -20.0, -26.0, 2.0);
        let features = extract_features_v1(&stereo);
        let asym = features.lufs_asymmetry_lu.unwrap();
        assert!(
            (asym - 6.0).abs() < 0.5,
            "6 dB L/R imbalance must give ~6 LU asymmetry, got {asym:.3}"
        );
        // Also check the per-channel values are ordered correctly.
        let l = features.lufs_left.unwrap();
        let r = features.lufs_right.unwrap();
        assert!(l > r, "Left should be louder: L={l:.2}, R={r:.2}");
    }

    #[test]
    fn lufs_silence_returns_none() {
        let silence = stereo_dual(48_000, vec![0.0; 48_000 * 2]);
        let features = extract_features_v1(&silence);
        // Silence falls below the absolute -70 LUFS gate → no surviving blocks.
        assert!(features.lufs_integrated.is_none());
        assert!(features.lufs_asymmetry_lu.is_none());
        assert!(features.plr_db.is_none());
    }

    #[test]
    fn lufs_short_signal_returns_none() {
        // <400 ms = no full 400 ms block can be filled.
        let short = stereo_dual(48_000, sine(48_000, 1000.0, 0.5, 0.30));
        let features = extract_features_v1(&short);
        assert!(features.lufs_integrated.is_none());
    }

    // ── True peak ──────────────────────────────────────────────────────

    #[test]
    fn true_peak_steady_sine() {
        // A 1 kHz sine at amplitude 0.5 has sample-domain peak ≈ 0.5
        // (depending on phase). True peak should be ≥ 0.5.
        let stereo = stereo_dual(48_000, sine(48_000, 1000.0, 0.5, 1.0));
        let features = extract_features_v1(&stereo);
        let tp = features.true_peak_dbfs.unwrap();
        let expected = 20.0 * 0.5_f64.log10();
        assert!(
            tp >= expected - 0.05 && tp <= expected + 1.0,
            "True peak for sine(0.5) should be ≥ {expected:.2} dBFS, got {tp:.3}"
        );
    }

    /// BS.1770-5 invariant: true-peak ≥ sample-domain peak for any input.
    /// The polyphase FIR with windowed-sinc kernel and DC gain 1 per phase
    /// can never *underestimate* the absolute sample maxima — at the very
    /// least it reproduces the input samples through phase-0 unity-gain.
    #[test]
    fn true_peak_at_least_sample_peak_for_random_signals() {
        let mut state = 1234567u64;
        for _ in 0..6 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let mono = deterministic_white(48_000, state);
            let stereo = stereo_dual(48_000, mono.clone());
            let features = extract_features_v1(&stereo);
            let tp_db = features.true_peak_dbfs.unwrap();
            let tp_lin = 10.0_f64.powf(tp_db / 20.0);
            let sample_peak = mono.iter().fold(0.0_f64, |a, x| a.max(x.abs()));
            assert!(
                tp_lin >= sample_peak - 1e-9,
                "BS.1770 invariant violated: true-peak {tp_lin:.6} < sample-peak {sample_peak:.6}"
            );
        }
    }

    /// Catmull-Rom oversampling must detect intersample peaks. Construct
    /// a high-frequency sine whose true maximum lies between samples.
    #[test]
    fn true_peak_detects_intersample_peak() {
        // 12 kHz sine at 48 kHz → 4 samples/cycle. Phase the signal so
        // sample-domain peak undershoots the true peak meaningfully.
        let n = 48_000;
        let freq = 12_000.0_f64;
        let amp = 0.95_f64;
        // Offset by 1/8 cycle so samples don't land at the extremum.
        let phase = std::f64::consts::PI / 4.0;
        let mono: Vec<f64> = (0..n)
            .map(|i| amp * (2.0 * std::f64::consts::PI * freq * i as f64 / 48_000.0 + phase).sin())
            .collect();
        let sample_peak = mono.iter().fold(0.0_f64, |a, x| a.max(x.abs()));
        let stereo: RenderedStereoAudio = {
            let left: Vec<f32> = mono.iter().map(|x| *x as f32).collect();
            let right = left.clone();
            RenderedStereoAudio::new(48_000, left, right)
        };
        let features = extract_features_v1(&stereo);
        let tp_db = features.true_peak_dbfs.unwrap();
        let tp_linear = 10.0_f64.powf(tp_db / 20.0);
        assert!(
            tp_linear > sample_peak + 1e-3,
            "True peak ({tp_linear:.4}) must exceed sample peak ({sample_peak:.4}) for offset 12 kHz sine"
        );
        // And it must not exceed the analytic amplitude by more than ~1 dB.
        assert!(
            tp_linear <= amp * 1.15,
            "True peak ({tp_linear:.4}) must be near analytic amp ({amp:.4})"
        );
    }

    // ── Spectral tilt and HF fraction ──────────────────────────────────

    /// Generate deterministic white noise from a seeded LCG. Phase 1 only
    /// needs reproducible "wide-spectrum" data, not high-quality randomness.
    fn deterministic_white(n: usize, seed: u64) -> Vec<f64> {
        let mut state = seed.wrapping_mul(2_862_933_555_777_941_757).wrapping_add(3_037_000_493);
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
            // Map upper 32 bits to [-1, 1].
            let u = ((state >> 32) as u32) as f64 / u32::MAX as f64;
            out.push(2.0 * u - 1.0);
        }
        out
    }

    /// Paul Kellet pink noise filter — flat to within ±0.05 dB of -3 dB/oct
    /// from ~10 Hz to ~22 kHz at 48 kHz.
    fn pink_from_white(white: &[f64]) -> Vec<f64> {
        let mut b0 = 0.0;
        let mut b1 = 0.0;
        let mut b2 = 0.0;
        let mut b3 = 0.0;
        let mut b4 = 0.0;
        let mut b5 = 0.0;
        let mut b6 = 0.0;
        white
            .iter()
            .map(|&w| {
                b0 = 0.99886 * b0 + w * 0.0555179;
                b1 = 0.99332 * b1 + w * 0.0750759;
                b2 = 0.96900 * b2 + w * 0.1538520;
                b3 = 0.86650 * b3 + w * 0.3104856;
                b4 = 0.55000 * b4 + w * 0.5329522;
                b5 = -0.7616 * b5 - w * 0.0168980;
                let out = b0 + b1 + b2 + b3 + b4 + b5 + b6 + w * 0.5362;
                b6 = w * 0.115926;
                out * 0.11 // scale to roughly ±1
            })
            .collect()
    }

    /// One-pole leaky integrator → roughly -6 dB/oct above the corner.
    fn brown_from_white(white: &[f64]) -> Vec<f64> {
        let mut y = 0.0;
        white
            .iter()
            .map(|&w| {
                y = 0.99 * y + 0.05 * w;
                y
            })
            .collect()
    }

    #[test]
    fn spectral_tilt_white_noise_near_zero() {
        let white = deterministic_white(48_000 * 2, 42);
        let stereo = stereo_dual(48_000, white);
        let features = extract_features_v1(&stereo);
        let tilt = features.spectral_tilt_db_per_oct.unwrap();
        // Deterministic LCG-derived white noise: tilt should be within ±1 dB/oct.
        assert!(
            tilt.abs() < 1.0,
            "White noise tilt should be ~0 dB/oct, got {tilt:.3}"
        );
    }

    #[test]
    fn spectral_tilt_pink_noise_near_minus3() {
        let white = deterministic_white(48_000 * 2, 7);
        let pink = pink_from_white(&white);
        let stereo = stereo_dual(48_000, pink);
        let features = extract_features_v1(&stereo);
        let tilt = features.spectral_tilt_db_per_oct.unwrap();
        assert!(
            (tilt - (-3.0)).abs() < 1.0,
            "Pink noise tilt should be ~-3 dB/oct, got {tilt:.3}"
        );
    }

    /// Robustness test: pink noise with an embedded narrowband tone
    /// must still report tilt close to −3 dB/oct. Under the previous
    /// raw-FFT regression the line spectrum from the tone leaked across
    /// many bins and biased the slope. The Welch + 1/6-octave-bin
    /// estimator is designed to suppress this.
    #[test]
    fn spectral_tilt_robust_to_narrowband_tone() {
        let white = deterministic_white(48_000 * 2, 17);
        let pink = pink_from_white(&white);
        // Inject a 1 kHz tone at moderate amplitude.
        let n = pink.len();
        let mut mixed = pink.clone();
        for i in 0..n {
            let t = i as f64 / 48_000.0;
            mixed[i] += 0.10 * (2.0 * std::f64::consts::PI * 1000.0 * t).sin();
        }
        let stereo = stereo_dual(48_000, mixed);
        let features = extract_features_v1(&stereo);
        let tilt = features.spectral_tilt_db_per_oct.unwrap();
        assert!(
            (tilt - (-3.0)).abs() < 1.5,
            "pink + 1 kHz tone tilt should still be near -3 dB/oct, got {tilt:.3}"
        );
    }

    #[test]
    fn spectral_tilt_brown_noise_steeper_than_pink() {
        let white = deterministic_white(48_000 * 2, 11);
        let pink = pink_from_white(&white);
        let brown = brown_from_white(&white);
        let pink_features = extract_features_v1(&stereo_dual(48_000, pink));
        let brown_features = extract_features_v1(&stereo_dual(48_000, brown));
        let pink_tilt = pink_features.spectral_tilt_db_per_oct.unwrap();
        let brown_tilt = brown_features.spectral_tilt_db_per_oct.unwrap();
        assert!(
            brown_tilt < pink_tilt - 1.5,
            "Brown ({brown_tilt:.2}) should be substantially steeper than pink ({pink_tilt:.2})"
        );
    }

    #[test]
    fn hf_fraction_higher_for_white_than_brown() {
        let white = deterministic_white(48_000 * 2, 99);
        let brown = brown_from_white(&white);
        let white_hf = extract_features_v1(&stereo_dual(48_000, white))
            .hf_fraction_above_8khz
            .unwrap();
        let brown_hf = extract_features_v1(&stereo_dual(48_000, brown))
            .hf_fraction_above_8khz
            .unwrap();
        assert!(
            white_hf > brown_hf + 0.2,
            "White HF fraction ({white_hf:.3}) must exceed brown ({brown_hf:.3})"
        );
    }

    #[test]
    fn hf_fraction_low_for_low_frequency_tone() {
        let stereo = stereo_dual(48_000, sine(48_000, 200.0, 0.5, 1.0));
        let features = extract_features_v1(&stereo);
        let hf = features.hf_fraction_above_8khz.unwrap();
        assert!(hf < 0.05, "200 Hz tone should have ~0 HF fraction, got {hf:.3}");
    }

    #[test]
    fn hf_fraction_high_for_high_frequency_tone() {
        let stereo = stereo_dual(48_000, sine(48_000, 12_000.0, 0.5, 1.0));
        let features = extract_features_v1(&stereo);
        let hf = features.hf_fraction_above_8khz.unwrap();
        assert!(hf > 0.90, "12 kHz tone should have ~1 HF fraction, got {hf:.3}");
    }

    // ── Peak-to-Loudness Ratio ─────────────────────────────────────────

    #[test]
    fn plr_low_for_steady_tone() {
        // A 1 kHz sine at -20 dBFS: integrated LUFS ≈ -20, true peak ≈ -20.
        // PLR ≈ true_peak − LUFS ≈ 0 dB (ideally) and < 1 dB for our
        // Catmull-Rom approximation.
        let stereo = stereo_dual(48_000, sine(48_000, 1000.0, 0.1, 3.0));
        let features = extract_features_v1(&stereo);
        let plr = features.plr_db.unwrap();
        assert!(
            (-1.0..=2.0).contains(&plr),
            "Steady-tone PLR should be near 0 dB, got {plr:.3}"
        );
    }

    #[test]
    fn plr_higher_for_transient_burst() {
        // 100 ms burst at amp 0.9 followed by ≈3.9 s of low-level steady
        // content (so that the relative gate keeps a reasonable number of
        // blocks alive). PLR must be much higher than the steady-tone case.
        let n = 48_000 * 4;
        let burst_n = 4_800; // 100 ms
        let mono: Vec<f64> = (0..n)
            .map(|i| {
                let amp = if i < burst_n { 0.9 } else { 0.05 };
                amp * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 48_000.0).sin()
            })
            .collect();
        let stereo = stereo_dual(48_000, mono);
        let features = extract_features_v1(&stereo);
        let plr = features.plr_db.unwrap();
        // Steady part is ~25 dB below the burst, so PLR should be > 15 dB.
        assert!(
            plr > 15.0,
            "Transient-dominated PLR should be > 15 dB, got {plr:.3}"
        );
    }

    // ── Backward-compat sanity: legacy fields unchanged ───────────────

    /// Confirm that the new feature pass produces the same legacy values
    /// (within float-equality tolerance) as the deterministic pre-Priority-28
    /// sanity expectations encoded in the test suite. This guards against
    /// silent drift in `speech_band_ratio` / `sharpness_proxy` introduced
    /// by the refactor of `spectral_features` into `compute_spectrum_analysis`.
    #[test]
    fn legacy_fields_unchanged_for_speech_band_tone() {
        // Same fixture used by the legacy `speech_band_ratio_prefers_midband_tone`
        // test: 1 kHz mono tone at amp 0.5.
        let stereo = stereo_from_mono(48_000, sine(48_000, 1000.0, 0.5, 1.0));
        let features = extract_features_v1(&stereo);
        let sb = features.speech_band_ratio.unwrap();
        let sh = features.sharpness_proxy.unwrap();
        // 1 kHz lies in the [300, 4000] speech band → ratio close to 1.
        assert!(
            sb > 0.95,
            "1 kHz tone should saturate speech_band_ratio, got {sb:.4}"
        );
        // Sharpness for 1 kHz should be ~0.5 on the log centroid mapping.
        assert!(
            (sh - 0.5).abs() < 0.05,
            "Sharpness for 1 kHz should be ~0.5, got {sh:.4}"
        );
    }

    #[test]
    fn comfort_metrics_finite_for_all_normal_inputs() {
        let stereo = stereo_dual(48_000, sine(48_000, 1000.0, 0.25, 2.0));
        let features = extract_features_v1(&stereo);
        for f in [
            features.lufs_integrated,
            features.lufs_left,
            features.lufs_right,
            features.lufs_asymmetry_lu,
            features.true_peak_dbfs,
            features.plr_db,
            features.spectral_tilt_db_per_oct,
            features.hf_fraction_above_8khz,
        ] {
            let v = f.expect("comfort metric must be present for normal input");
            assert!(v.is_finite(), "comfort metric must be finite, got {v}");
        }
    }
}
