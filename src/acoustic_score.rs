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
    let (speech_band_ratio, sharpness_proxy) = spectral_features(&mono, sample_rate_hz);
    let modulation_depth = Some(modulation_depth_proxy(&mono, rendered.sample_rate_hz));

    AcousticFeatureVector {
        broadband_level_db,
        speech_band_ratio: Some(speech_band_ratio),
        modulation_depth,
        sharpness_proxy: Some(sharpness_proxy),
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

fn spectral_features(samples: &[f64], sample_rate_hz: f64) -> (f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0);
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
    let min_bin = (20.0 / freq_res).ceil() as usize;
    let max_bin = ((10_000.0 / freq_res).floor() as usize).min(fft_len / 2);

    let mut total_power = 0.0_f64;
    let mut speech_power = 0.0_f64;
    let mut weighted_sum = 0.0_f64;

    for bin in min_bin..max_bin {
        let freq = bin as f64 * freq_res;
        let power = buf[bin].norm_sqr();
        total_power += power;
        weighted_sum += freq * power;
        if (300.0..=4_000.0).contains(&freq) {
            speech_power += power;
        }
    }

    if total_power <= 1e-30 {
        return (0.0, 0.0);
    }

    let centroid = weighted_sum / total_power;
    let log_low = 100.0_f64.ln();
    let log_high = 10_000.0_f64.ln();
    let sharpness_proxy =
        ((centroid.max(100.0).ln() - log_low) / (log_high - log_low)).clamp(0.0, 1.0);
    let speech_band_ratio = (speech_power / total_power).clamp(0.0, 1.0);

    (speech_band_ratio, sharpness_proxy)
}

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
}
