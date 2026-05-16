use rustfft::{num_complex::Complex, FftPlanner};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

const POWER_FLOOR: f64 = 1e-18;
const MIN_PEAK_HEIGHT_LOG10: f64 = 0.05;
const PEAK_WINDOW_HALF_WIDTH_HZ: f64 = 4.0;
const PEAK_EXCLUSION_SIGMA_MULTIPLIER: f64 = 1.5;
const MIN_PEAK_SEPARATION_HZ: f64 = 1.0;
const MIN_SIGMA_HZ: f64 = 0.15;
const MAX_SIGMA_HZ: f64 = 8.0;
const MAX_PEAKS: usize = 6;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpectralPeak {
    /// Gaussian center frequency (mu), in Hz, on the flattened spectrum.
    pub center_hz: f64,
    /// Gaussian bandwidth in Hz defined as `2 * sigma`.
    pub bandwidth_hz: f64,
    /// Gaussian peak height above the aperiodic fit in log10(power) units.
    pub power_above_aperiodic: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpectralParameterization {
    pub fit_min_hz: f64,
    pub fit_max_hz: f64,
    pub aperiodic_exponent: f64,
    pub aperiodic_offset: f64,
    pub peaks: Vec<SpectralPeak>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PowerSpectrum {
    pub freqs_hz: Vec<f64>,
    pub power: Vec<f64>,
}

#[derive(Debug, Clone, Copy)]
struct LinearFit {
    slope: f64,
    intercept: f64,
}

#[derive(Debug, Clone, Copy)]
struct GaussianPeakFit {
    center_hz: f64,
    sigma_hz: f64,
    height_log10: f64,
}

pub fn compute_one_sided_psd(signal: &[f64], sample_rate: f64) -> PowerSpectrum {
    let n = signal.len();
    if n < 4 || !sample_rate.is_finite() || sample_rate <= 0.0 {
        return PowerSpectrum {
            freqs_hz: Vec::new(),
            power: Vec::new(),
        };
    }

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
    let nyquist_bin = fft_len / 2;
    let mut freqs = Vec::with_capacity(nyquist_bin);
    let mut power = Vec::with_capacity(nyquist_bin);

    for bin in 1..nyquist_bin {
        let f = bin as f64 * freq_res;
        if !f.is_finite() || f <= 0.0 {
            continue;
        }
        let p = (buf[bin].norm_sqr() / fft_len as f64).max(0.0);
        freqs.push(f);
        power.push(if p.is_finite() { p } else { 0.0 });
    }

    PowerSpectrum {
        freqs_hz: freqs,
        power,
    }
}

pub fn parameterize_signal_psd(
    signal: &[f64],
    sample_rate: f64,
    fit_min_hz: f64,
    fit_max_hz: f64,
) -> SpectralParameterization {
    let mean = if signal.is_empty() {
        0.0
    } else {
        signal.iter().sum::<f64>() / signal.len() as f64
    };
    let detrended: Vec<f64> = signal.iter().map(|x| x - mean).collect();
    let psd = compute_one_sided_psd(&detrended, sample_rate);
    parameterize_psd(&psd, fit_min_hz, fit_max_hz)
}

pub fn parameterize_psd(
    psd: &PowerSpectrum,
    fit_min_hz: f64,
    fit_max_hz: f64,
) -> SpectralParameterization {
    let band_min = fit_min_hz.min(fit_max_hz).max(0.1);
    let band_max = fit_min_hz.max(fit_max_hz).max(band_min + 0.1);
    let mut freqs = Vec::new();
    let mut power = Vec::new();

    for (&f, &p) in psd.freqs_hz.iter().zip(psd.power.iter()) {
        if f >= band_min && f <= band_max && f.is_finite() && p.is_finite() {
            freqs.push(f);
            power.push(p.max(0.0));
        }
    }

    if freqs.len() < 3 {
        return SpectralParameterization {
            fit_min_hz: band_min,
            fit_max_hz: band_max,
            aperiodic_exponent: 0.0,
            aperiodic_offset: POWER_FLOOR.log10(),
            peaks: Vec::new(),
        };
    }

    let log_freq: Vec<f64> = freqs.iter().map(|f| f.log10()).collect();
    let log_power: Vec<f64> = power.iter().map(|p| p.max(POWER_FLOOR).log10()).collect();
    let first_fit = robust_linear_fit(&log_freq, &log_power);
    let first_residual: Vec<f64> = log_freq
        .iter()
        .zip(log_power.iter())
        .map(|(&x, &y)| y - (first_fit.slope * x + first_fit.intercept))
        .collect();

    let first_candidates = detect_peak_candidates(&freqs, &first_residual);
    let first_peak_fits = fit_candidates_as_gaussians(&freqs, &first_residual, &first_candidates);
    let final_fit = if let Some(masked) =
        masked_samples_without_peaks(&log_freq, &log_power, &freqs, &first_peak_fits)
    {
        robust_linear_fit(&masked.0, &masked.1)
    } else {
        first_fit
    };

    let final_residual: Vec<f64> = log_freq
        .iter()
        .zip(log_power.iter())
        .map(|(&x, &y)| y - (final_fit.slope * x + final_fit.intercept))
        .collect();
    let final_candidates = detect_peak_candidates(&freqs, &final_residual);
    let final_peak_fits = fit_candidates_as_gaussians(&freqs, &final_residual, &final_candidates);
    let peaks = gaussian_fits_to_export(&final_peak_fits);

    let exponent = (-final_fit.slope).clamp(-10.0, 10.0);
    let offset = if final_fit.intercept.is_finite() {
        final_fit.intercept
    } else {
        POWER_FLOOR.log10()
    };

    SpectralParameterization {
        fit_min_hz: band_min,
        fit_max_hz: band_max,
        aperiodic_exponent: exponent,
        aperiodic_offset: offset,
        peaks,
    }
}

fn detect_peak_candidates(freqs: &[f64], residual: &[f64]) -> Vec<usize> {
    if residual.len() < 5 || freqs.len() != residual.len() {
        return Vec::new();
    }

    let median_residual = median(residual);
    let abs_dev: Vec<f64> = residual
        .iter()
        .map(|r| (r - median_residual).abs())
        .collect();
    let mad = median(&abs_dev).max(1e-9);
    let threshold = (median_residual + 2.5 * 1.4826 * mad).max(MIN_PEAK_HEIGHT_LOG10);

    let mut local_maxima: Vec<(usize, f64)> = Vec::new();
    for i in 1..(residual.len() - 1) {
        let y = residual[i];
        if !y.is_finite() || y <= threshold {
            continue;
        }
        if y > residual[i - 1] && y >= residual[i + 1] {
            local_maxima.push((i, y));
        }
    }
    local_maxima.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut selected: Vec<usize> = Vec::new();
    for (idx, _) in local_maxima {
        if selected
            .iter()
            .any(|&j| (freqs[j] - freqs[idx]).abs() < MIN_PEAK_SEPARATION_HZ)
        {
            continue;
        }
        selected.push(idx);
        if selected.len() >= MAX_PEAKS {
            break;
        }
    }
    selected.sort_by(|a, b| {
        freqs[*a]
            .partial_cmp(&freqs[*b])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    selected
}

fn masked_samples_without_peaks(
    xs: &[f64],
    ys: &[f64],
    freqs: &[f64],
    peaks: &[GaussianPeakFit],
) -> Option<(Vec<f64>, Vec<f64>)> {
    if peaks.is_empty() {
        return Some((xs.to_vec(), ys.to_vec()));
    }
    if xs.len() != ys.len() || xs.len() != freqs.len() {
        return None;
    }
    let mut keep = vec![true; freqs.len()];
    for peak in peaks {
        let radius = (PEAK_EXCLUSION_SIGMA_MULTIPLIER * peak.sigma_hz).max(MIN_SIGMA_HZ);
        for (i, &f) in freqs.iter().enumerate() {
            if (f - peak.center_hz).abs() <= radius {
                keep[i] = false;
            }
        }
    }
    let mut masked_x = Vec::new();
    let mut masked_y = Vec::new();
    for i in 0..xs.len() {
        if keep[i] {
            masked_x.push(xs[i]);
            masked_y.push(ys[i]);
        }
    }
    if masked_x.len() >= 3 {
        Some((masked_x, masked_y))
    } else {
        None
    }
}

fn fit_candidates_as_gaussians(
    freqs: &[f64],
    residual: &[f64],
    candidates: &[usize],
) -> Vec<GaussianPeakFit> {
    candidates
        .iter()
        .filter_map(|&idx| fit_single_gaussian_peak(freqs, residual, idx))
        .filter(|peak| {
            peak.height_log10.is_finite()
                && peak.height_log10 >= MIN_PEAK_HEIGHT_LOG10
                && peak.center_hz.is_finite()
                && peak.sigma_hz.is_finite()
                && peak.sigma_hz >= MIN_SIGMA_HZ
        })
        .collect()
}

fn fit_single_gaussian_peak(
    freqs: &[f64],
    residual: &[f64],
    center_idx: usize,
) -> Option<GaussianPeakFit> {
    if freqs.len() != residual.len() || center_idx >= freqs.len() {
        return None;
    }
    let center_hz = freqs[center_idx];
    if !center_hz.is_finite() {
        return None;
    }

    let window_idx: Vec<usize> = freqs
        .iter()
        .enumerate()
        .filter_map(|(i, &f)| ((f - center_hz).abs() <= PEAK_WINDOW_HALF_WIDTH_HZ).then_some(i))
        .collect();
    if window_idx.len() < 5 {
        return None;
    }

    let window_freqs: Vec<f64> = window_idx.iter().map(|&i| freqs[i]).collect();
    let window_residual: Vec<f64> = window_idx.iter().map(|&i| residual[i].max(0.0)).collect();
    let freq_step = frequency_resolution(&window_freqs).max(0.05);

    let mut best_fit: Option<GaussianPeakFit> = None;
    let mut best_sse = f64::INFINITY;

    let mu_start = center_hz - 1.0;
    let mu_end = center_hz + 1.0;
    let mut mu = mu_start;
    while mu <= mu_end + 1e-9 {
        for sigma in [
            0.2_f64, 0.3_f64, 0.45_f64, 0.65_f64, 0.9_f64, 1.2_f64, 1.6_f64, 2.1_f64, 2.8_f64,
            3.8_f64, 5.0_f64, 6.5_f64,
        ] {
            let sigma = sigma.clamp(MIN_SIGMA_HZ, MAX_SIGMA_HZ);
            if let Some((height, sse)) =
                fit_gaussian_height_and_error(&window_freqs, &window_residual, mu, sigma)
            {
                if sse < best_sse {
                    best_sse = sse;
                    best_fit = Some(GaussianPeakFit {
                        center_hz: mu,
                        sigma_hz: sigma,
                        height_log10: height,
                    });
                }
            }
        }
        mu += freq_step;
    }

    let mut fit = best_fit?;
    for _ in 0..2 {
        let mu_step = (freq_step * 0.5).max(0.02);
        let sigma_step = (fit.sigma_hz * 0.25).max(0.05);
        let mut improved = false;
        for mu in [
            fit.center_hz - mu_step,
            fit.center_hz,
            fit.center_hz + mu_step,
        ] {
            for sigma in [
                fit.sigma_hz - sigma_step,
                fit.sigma_hz,
                fit.sigma_hz + sigma_step,
            ] {
                let sigma = sigma.clamp(MIN_SIGMA_HZ, MAX_SIGMA_HZ);
                if let Some((height, sse)) =
                    fit_gaussian_height_and_error(&window_freqs, &window_residual, mu, sigma)
                {
                    if sse + 1e-12 < best_sse {
                        best_sse = sse;
                        fit = GaussianPeakFit {
                            center_hz: mu,
                            sigma_hz: sigma,
                            height_log10: height,
                        };
                        improved = true;
                    }
                }
            }
        }
        if !improved {
            break;
        }
    }

    if fit.height_log10 < MIN_PEAK_HEIGHT_LOG10 {
        return None;
    }
    Some(fit)
}

fn fit_gaussian_height_and_error(
    freqs: &[f64],
    residual: &[f64],
    mu: f64,
    sigma_hz: f64,
) -> Option<(f64, f64)> {
    if freqs.len() != residual.len() || freqs.is_empty() || sigma_hz <= 0.0 || !mu.is_finite() {
        return None;
    }
    let sigma = sigma_hz.clamp(MIN_SIGMA_HZ, MAX_SIGMA_HZ);
    let mut gg = 0.0_f64;
    let mut rg = 0.0_f64;
    let mut g_vec = Vec::with_capacity(freqs.len());
    for (&f, &r) in freqs.iter().zip(residual.iter()) {
        let z = (f - mu) / sigma;
        let g = (-0.5 * z * z).exp();
        g_vec.push(g);
        gg += g * g;
        rg += r * g;
    }
    if gg <= 1e-12 || !gg.is_finite() || !rg.is_finite() {
        return None;
    }
    let height = (rg / gg).max(0.0);
    let mut sse = 0.0_f64;
    for (&r, &g) in residual.iter().zip(g_vec.iter()) {
        let e = r - height * g;
        sse += e * e;
    }
    if !height.is_finite() || !sse.is_finite() {
        return None;
    }
    Some((height, sse))
}

fn gaussian_fits_to_export(fits: &[GaussianPeakFit]) -> Vec<SpectralPeak> {
    let mut peaks: Vec<SpectralPeak> = fits
        .iter()
        .map(|fit| SpectralPeak {
            center_hz: fit.center_hz,
            bandwidth_hz: (2.0 * fit.sigma_hz).max(2.0 * MIN_SIGMA_HZ),
            power_above_aperiodic: fit.height_log10.max(0.0),
        })
        .collect();
    peaks.sort_by(|a, b| {
        a.center_hz
            .partial_cmp(&b.center_hz)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    peaks
}

fn frequency_resolution(freqs: &[f64]) -> f64 {
    if freqs.len() < 2 {
        return 0.1;
    }
    let mut diffs = Vec::with_capacity(freqs.len() - 1);
    for pair in freqs.windows(2) {
        let d = pair[1] - pair[0];
        if d.is_finite() && d > 0.0 {
            diffs.push(d);
        }
    }
    if diffs.is_empty() {
        0.1
    } else {
        median(&diffs).max(0.01)
    }
}

fn robust_linear_fit(xs: &[f64], ys: &[f64]) -> LinearFit {
    if xs.len() < 2 || ys.len() < 2 || xs.len() != ys.len() {
        return LinearFit {
            slope: 0.0,
            intercept: POWER_FLOOR.log10(),
        };
    }

    let mut fit = weighted_linear_fit(xs, ys, &vec![1.0; xs.len()]);
    for _ in 0..4 {
        let residuals: Vec<f64> = xs
            .iter()
            .zip(ys.iter())
            .map(|(&x, &y)| y - (fit.slope * x + fit.intercept))
            .collect();
        let med = median(&residuals);
        let abs_dev: Vec<f64> = residuals.iter().map(|r| (r - med).abs()).collect();
        let scale = (1.4826 * median(&abs_dev)).max(1e-9);
        let huber_k = 1.5 * scale;
        let weights: Vec<f64> = residuals
            .iter()
            .map(|r| {
                let ar = r.abs();
                if ar <= huber_k {
                    1.0
                } else {
                    (huber_k / ar).clamp(0.0, 1.0)
                }
            })
            .collect();
        fit = weighted_linear_fit(xs, ys, &weights);
    }
    fit
}

fn weighted_linear_fit(xs: &[f64], ys: &[f64], ws: &[f64]) -> LinearFit {
    if xs.len() != ys.len() || xs.len() != ws.len() || xs.is_empty() {
        return LinearFit {
            slope: 0.0,
            intercept: POWER_FLOOR.log10(),
        };
    }

    let sw: f64 = ws.iter().sum();
    if sw <= 1e-12 || !sw.is_finite() {
        return LinearFit {
            slope: 0.0,
            intercept: POWER_FLOOR.log10(),
        };
    }
    let xw: f64 = xs.iter().zip(ws.iter()).map(|(x, w)| x * w).sum::<f64>() / sw;
    let yw: f64 = ys.iter().zip(ws.iter()).map(|(y, w)| y * w).sum::<f64>() / sw;
    let cov: f64 = xs
        .iter()
        .zip(ys.iter())
        .zip(ws.iter())
        .map(|((&x, &y), &w)| w * (x - xw) * (y - yw))
        .sum();
    let var: f64 = xs
        .iter()
        .zip(ws.iter())
        .map(|(&x, &w)| w * (x - xw).powi(2))
        .sum();
    if var <= 1e-12 || !var.is_finite() || !cov.is_finite() {
        return LinearFit {
            slope: 0.0,
            intercept: yw,
        };
    }
    let slope = cov / var;
    let intercept = yw - slope * xw;
    if slope.is_finite() && intercept.is_finite() {
        LinearFit { slope, intercept }
    } else {
        LinearFit {
            slope: 0.0,
            intercept: POWER_FLOOR.log10(),
        }
    }
}

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut v: Vec<f64> = values.iter().cloned().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let m = v.len() / 2;
    if v.len() % 2 == 1 {
        v[m]
    } else {
        0.5 * (v[m - 1] + v[m])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_psd(
        min_hz: f64,
        max_hz: f64,
        step_hz: f64,
        exponent: f64,
        offset: f64,
        peaks: &[(f64, f64, f64)],
    ) -> PowerSpectrum {
        let mut freqs = Vec::new();
        let mut power = Vec::new();
        let mut f = min_hz;
        while f <= max_hz {
            let mut log_p = offset - exponent * f.max(0.1).log10();
            for &(center, sigma_hz, height_log10) in peaks {
                let z = (f - center) / sigma_hz.max(1e-6);
                log_p += height_log10 * (-0.5 * z * z).exp();
            }
            let p = 10f64.powf(log_p);
            freqs.push(f);
            power.push(p.max(0.0));
            f += step_hz;
        }
        PowerSpectrum {
            freqs_hz: freqs,
            power,
        }
    }

    #[test]
    fn parameterization_silence_has_finite_outputs_and_no_peaks() {
        let signal = vec![0.0_f64; 4096];
        let spec = parameterize_signal_psd(&signal, 1000.0, 2.0, 40.0);
        assert!(spec.aperiodic_exponent.is_finite());
        assert!(spec.aperiodic_offset.is_finite());
        assert!(spec.peaks.is_empty());
    }

    #[test]
    fn parameterization_recovers_aperiodic_exponent_without_peaks() {
        let psd = synthetic_psd(1.0, 60.0, 0.25, 1.6, -2.0, &[]);
        let spec = parameterize_psd(&psd, 2.0, 40.0);
        assert!(spec.peaks.is_empty());
        assert!(
            (spec.aperiodic_exponent - 1.6).abs() < 0.20,
            "expected exponent near 1.6, got {}",
            spec.aperiodic_exponent
        );
    }

    #[test]
    fn parameterization_fits_single_periodic_peak_gaussian_semantics() {
        let expected_center = 10.0;
        let expected_sigma = 0.6;
        let expected_height = 0.42;
        let psd = synthetic_psd(
            1.0,
            60.0,
            0.25,
            1.2,
            -1.8,
            &[(expected_center, expected_sigma, expected_height)],
        );
        let spec = parameterize_psd(&psd, 2.0, 40.0);
        assert!(
            !spec.peaks.is_empty(),
            "expected at least one detected periodic peak"
        );
        let peak = spec
            .peaks
            .iter()
            .min_by(|a, b| {
                (a.center_hz - expected_center)
                    .abs()
                    .partial_cmp(&(b.center_hz - expected_center).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .expect("missing nearest peak");
        assert!(
            (peak.center_hz - expected_center).abs() < 0.6,
            "expected peak near {expected_center} Hz, got {:.3} Hz",
            peak.center_hz
        );
        let expected_bandwidth = 2.0 * expected_sigma;
        assert!(
            (peak.bandwidth_hz - expected_bandwidth).abs() < 0.8,
            "expected bandwidth near {expected_bandwidth:.3} Hz, got {:.3} Hz",
            peak.bandwidth_hz
        );
        assert!(
            (peak.power_above_aperiodic - expected_height).abs() < 0.18,
            "expected height near {expected_height:.3} log10 units, got {:.3}",
            peak.power_above_aperiodic
        );
    }

    #[test]
    fn parameterization_detects_multiple_periodic_peaks() {
        let psd = synthetic_psd(
            1.0,
            60.0,
            0.25,
            1.0,
            -1.6,
            &[(6.0, 0.5, 0.35), (18.0, 0.8, 0.30)],
        );
        let spec = parameterize_psd(&psd, 2.0, 40.0);
        let near_6 = spec.peaks.iter().any(|p| (p.center_hz - 6.0).abs() < 0.8);
        let near_18 = spec.peaks.iter().any(|p| (p.center_hz - 18.0).abs() < 1.0);
        assert!(
            near_6 && near_18,
            "expected peaks near 6 Hz and 18 Hz, got {:?}",
            spec.peaks.iter().map(|p| p.center_hz).collect::<Vec<_>>()
        );
        for peak in &spec.peaks {
            assert!(peak.center_hz.is_finite());
            assert!(peak.bandwidth_hz.is_finite());
            assert!(peak.power_above_aperiodic.is_finite());
        }
    }
}
