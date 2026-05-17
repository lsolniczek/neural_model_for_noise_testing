use crate::neural::aperiodic::{compute_one_sided_psd, PowerSpectrum};
use serde::{Deserialize, Serialize};

const MODULATION_MIN_HZ: f64 = 0.5;
const MODULATION_MAX_HZ: f64 = 80.0;
const DOMINANT_PEAK_MIN_SHARE: f64 = 0.03;
const POWER_FLOOR: f64 = 1e-18;
const WELCH_SEGMENT_SECONDS: f64 = 2.0;
const WELCH_OVERLAP_FRACTION: f64 = 0.5;
const WELCH_MIN_SEGMENT_SAMPLES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateArousalSource {
    LegacyHeuristicGate,
    PhysiologicalGateHeuristic,
    NeutralDefault,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CochlearFeatures {
    pub band_energy_fractions: [f64; 4],
    pub brightness: f64,
    pub spectral_tilt_db_per_oct: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ModulationBandPowers {
    pub slow_0p5_4_hz: f64,
    pub theta_4_8_hz: f64,
    pub alpha_8_13_hz: f64,
    pub beta_13_30_hz: f64,
    pub gamma_30_50_hz: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModulationPsdPoint {
    pub frequency_hz: f64,
    pub power: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TemporalModulationFeatures {
    pub modulation_psd: Vec<ModulationPsdPoint>,
    pub dominant_modulation_hz: Option<f64>,
    pub band_power_by_mod_rate: ModulationBandPowers,
    pub total_modulation_power: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LatentStateEstimate {
    pub estimated_arousal: f64,
    pub arousal_source: CandidateArousalSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CandidateAuditoryFeatures {
    pub cochlear: CochlearFeatures,
    pub temporal_modulation: TemporalModulationFeatures,
    pub latent_state: LatentStateEstimate,
}

pub fn extract_candidate_auditory_features(
    left_bands_dec: &[Vec<f64>; 4],
    right_bands_dec: &[Vec<f64>; 4],
    band_energy_fractions: [f64; 4],
    brightness: f64,
    spectral_tilt_db_per_oct: Option<f64>,
    estimated_arousal: f64,
    arousal_source: CandidateArousalSource,
    sample_rate_hz: f64,
) -> CandidateAuditoryFeatures {
    let cochlear = CochlearFeatures {
        band_energy_fractions: normalized_band_weights(band_energy_fractions),
        brightness: if brightness.is_finite() {
            brightness.clamp(0.0, 1.0)
        } else {
            0.5
        },
        spectral_tilt_db_per_oct: spectral_tilt_db_per_oct.filter(|v| v.is_finite()),
    };
    let temporal_modulation =
        extract_temporal_modulation_from_bands(left_bands_dec, right_bands_dec, sample_rate_hz);
    let latent_state = LatentStateEstimate {
        estimated_arousal: if estimated_arousal.is_finite() {
            estimated_arousal.clamp(0.0, 1.0)
        } else {
            0.5
        },
        arousal_source,
    };

    CandidateAuditoryFeatures {
        cochlear,
        temporal_modulation,
        latent_state,
    }
}

fn extract_temporal_modulation_from_bands(
    left: &[Vec<f64>; 4],
    right: &[Vec<f64>; 4],
    sample_rate_hz: f64,
) -> TemporalModulationFeatures {
    let len = min_common_len(left, right);
    if len < 8 || !sample_rate_hz.is_finite() || sample_rate_hz <= 0.0 {
        return empty_temporal_modulation_features();
    }

    // Candidate temporal modulation intentionally summarizes shared envelope
    // rhythm structure independently of carrier-color weighting.
    let weights = [0.25_f64; 4];
    let mut combined = vec![0.0_f64; len];
    let mut active_weight_sum = 0.0_f64;

    for b in 0..4 {
        let w = weights[b];
        if w <= 0.0 {
            continue;
        }

        let mut band = vec![0.0_f64; len];
        for i in 0..len {
            band[i] = 0.5 * (left[b][i] + right[b][i]);
        }
        let mean = band.iter().sum::<f64>() / len as f64;
        let var = band
            .iter()
            .map(|v| {
                let d = *v - mean;
                d * d
            })
            .sum::<f64>()
            / len as f64;
        let std = var.sqrt();
        if std <= 1e-12 || !std.is_finite() {
            continue;
        }

        active_weight_sum += w;
        for i in 0..len {
            combined[i] += w * ((band[i] - mean) / std);
        }
    }

    if active_weight_sum <= 1e-12 {
        return empty_temporal_modulation_features();
    }
    for v in &mut combined {
        *v /= active_weight_sum;
    }

    extract_temporal_modulation(&combined, sample_rate_hz)
}

pub fn extract_temporal_modulation(
    envelope: &[f64],
    sample_rate_hz: f64,
) -> TemporalModulationFeatures {
    if envelope.len() < 8 || !sample_rate_hz.is_finite() || sample_rate_hz <= 0.0 {
        return empty_temporal_modulation_features();
    }
    let mean = envelope.iter().sum::<f64>() / envelope.len() as f64;
    let ac: Vec<f64> = envelope.iter().map(|x| x - mean).collect();
    let psd = compute_welch_psd(&ac, sample_rate_hz);
    summarize_modulation_psd(&psd.freqs_hz, &psd.power)
}

fn compute_welch_psd(signal: &[f64], sample_rate_hz: f64) -> PowerSpectrum {
    let n = signal.len();
    if n < 8 || !sample_rate_hz.is_finite() || sample_rate_hz <= 0.0 {
        return PowerSpectrum {
            freqs_hz: Vec::new(),
            power: Vec::new(),
        };
    }

    let target_seg_len =
        ((sample_rate_hz * WELCH_SEGMENT_SECONDS).round() as usize).max(WELCH_MIN_SEGMENT_SAMPLES);
    let seg_len = target_seg_len.min(n);
    if seg_len < 8 {
        return PowerSpectrum {
            freqs_hz: Vec::new(),
            power: Vec::new(),
        };
    }

    let hop = ((seg_len as f64) * (1.0 - WELCH_OVERLAP_FRACTION))
        .round()
        .max(1.0) as usize;
    let mut starts = Vec::new();
    let mut start = 0usize;
    while start + seg_len <= n {
        starts.push(start);
        start = start.saturating_add(hop);
    }
    let tail_start = n.saturating_sub(seg_len);
    if starts.last().copied() != Some(tail_start) {
        starts.push(tail_start);
    }

    let mut accum_power: Vec<f64> = Vec::new();
    let mut freqs: Vec<f64> = Vec::new();
    let mut used_segments = 0usize;

    for seg_start in starts {
        let segment = &signal[seg_start..(seg_start + seg_len)];
        let seg_psd = compute_one_sided_psd(segment, sample_rate_hz);
        if seg_psd.power.is_empty() || seg_psd.freqs_hz.len() != seg_psd.power.len() {
            continue;
        }
        if accum_power.is_empty() {
            freqs = seg_psd.freqs_hz.clone();
            accum_power = vec![0.0; seg_psd.power.len()];
        }
        if seg_psd.power.len() != accum_power.len() {
            continue;
        }
        for (dst, src) in accum_power.iter_mut().zip(seg_psd.power.iter()) {
            *dst += *src;
        }
        used_segments += 1;
    }

    if used_segments == 0 {
        return compute_one_sided_psd(signal, sample_rate_hz);
    }

    let inv = 1.0 / used_segments as f64;
    for p in &mut accum_power {
        *p *= inv;
    }

    PowerSpectrum {
        freqs_hz: freqs,
        power: accum_power,
    }
}

fn summarize_modulation_psd(freqs_hz: &[f64], power: &[f64]) -> TemporalModulationFeatures {
    if freqs_hz.len() != power.len() || freqs_hz.is_empty() {
        return empty_temporal_modulation_features();
    }
    let mut modulation_psd = Vec::new();
    let mut total_power = 0.0_f64;
    let mut max_entry: Option<(f64, f64)> = None;
    let mut band = ModulationBandPowers {
        slow_0p5_4_hz: 0.0,
        theta_4_8_hz: 0.0,
        alpha_8_13_hz: 0.0,
        beta_13_30_hz: 0.0,
        gamma_30_50_hz: 0.0,
    };

    for (&f, &p_raw) in freqs_hz.iter().zip(power.iter()) {
        if !f.is_finite() || !p_raw.is_finite() || f < MODULATION_MIN_HZ || f > MODULATION_MAX_HZ {
            continue;
        }
        let p = p_raw.max(0.0);
        modulation_psd.push(ModulationPsdPoint {
            frequency_hz: f,
            power: p,
        });
        total_power += p;

        match max_entry {
            Some((_, best_p)) if p <= best_p => {}
            _ => max_entry = Some((f, p)),
        }

        if (0.5..4.0).contains(&f) {
            band.slow_0p5_4_hz += p;
        } else if (4.0..8.0).contains(&f) {
            band.theta_4_8_hz += p;
        } else if (8.0..13.0).contains(&f) {
            band.alpha_8_13_hz += p;
        } else if (13.0..30.0).contains(&f) {
            band.beta_13_30_hz += p;
        } else if (30.0..=50.0).contains(&f) {
            band.gamma_30_50_hz += p;
        }
    }

    let total_power = if total_power.is_finite() {
        total_power.max(0.0)
    } else {
        0.0
    };
    let dominant_modulation_hz = max_entry.and_then(|(freq, peak_power)| {
        if total_power <= POWER_FLOOR {
            return None;
        }
        let share = peak_power / total_power.max(POWER_FLOOR);
        if share >= DOMINANT_PEAK_MIN_SHARE {
            Some(freq)
        } else {
            None
        }
    });

    TemporalModulationFeatures {
        modulation_psd,
        dominant_modulation_hz,
        band_power_by_mod_rate: band,
        total_modulation_power: total_power,
    }
}

fn min_common_len(left: &[Vec<f64>; 4], right: &[Vec<f64>; 4]) -> usize {
    let left_min = left.iter().map(|v| v.len()).min().unwrap_or(0);
    let right_min = right.iter().map(|v| v.len()).min().unwrap_or(0);
    left_min.min(right_min)
}

fn normalized_band_weights(mut weights: [f64; 4]) -> [f64; 4] {
    for w in &mut weights {
        if !w.is_finite() || *w < 0.0 {
            *w = 0.0;
        }
    }
    let sum: f64 = weights.iter().sum();
    if sum > 1e-12 {
        for w in &mut weights {
            *w /= sum;
        }
    } else {
        weights = [0.25; 4];
    }
    weights
}

fn empty_temporal_modulation_features() -> TemporalModulationFeatures {
    TemporalModulationFeatures {
        modulation_psd: Vec::new(),
        dominant_modulation_hz: None,
        band_power_by_mod_rate: ModulationBandPowers {
            slow_0p5_4_hz: 0.0,
            theta_4_8_hz: 0.0,
            alpha_8_13_hz: 0.0,
            beta_13_30_hz: 0.0,
            gamma_30_50_hz: 0.0,
        },
        total_modulation_power: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn synthetic_bands_with_modulation(
        freq_hz: f64,
        sample_rate_hz: f64,
        seconds: f64,
    ) -> [Vec<f64>; 4] {
        let n = (sample_rate_hz * seconds) as usize;
        let mut bands = [
            vec![0.0_f64; n],
            vec![0.0_f64; n],
            vec![0.0_f64; n],
            vec![0.0_f64; n],
        ];
        for i in 0..n {
            let t = i as f64 / sample_rate_hz;
            let env = 1.0 + 0.6 * (2.0 * PI * freq_hz * t).sin();
            for (b, band) in bands.iter_mut().enumerate() {
                let carrier_like = (2.0 * PI * (50.0 * (b as f64 + 1.0)) * t).sin().abs();
                band[i] = env * (0.5 + 0.5 * carrier_like);
            }
        }
        bands
    }

    #[test]
    fn production_band_extractor_recovers_modulation_near_5hz() {
        let sr = 1000.0;
        let left = synthetic_bands_with_modulation(5.0, sr, 6.0);
        let right = left.clone();
        let modulation = extract_temporal_modulation_from_bands(&left, &right, sr);
        let dom = modulation
            .dominant_modulation_hz
            .expect("5 Hz modulation should produce dominant peak");
        assert!(
            (dom - 5.0).abs() < 1.0,
            "dominant modulation {dom:.3} should be near 5 Hz"
        );
    }

    #[test]
    fn production_band_extractor_recovers_modulation_near_40hz() {
        let sr = 1000.0;
        let left = synthetic_bands_with_modulation(40.0, sr, 6.0);
        let right = left.clone();
        let modulation = extract_temporal_modulation_from_bands(&left, &right, sr);
        let dom = modulation
            .dominant_modulation_hz
            .expect("40 Hz modulation should produce dominant peak");
        assert!(
            (dom - 40.0).abs() < 2.0,
            "dominant modulation {dom:.3} should be near 40 Hz"
        );
    }

    #[test]
    fn production_band_extractor_unmodulated_input_has_no_dominant_rate() {
        let sr = 1000.0;
        let n = (sr * 4.0) as usize;
        let left = [
            vec![0.8_f64; n],
            vec![0.8_f64; n],
            vec![0.8_f64; n],
            vec![0.8_f64; n],
        ];
        let right = left.clone();
        let modulation = extract_temporal_modulation_from_bands(&left, &right, sr);
        assert!(modulation.dominant_modulation_hz.is_none());
        assert!(modulation.total_modulation_power <= POWER_FLOOR);
    }
}
