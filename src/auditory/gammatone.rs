/// Gammatone filterbank — cochlear model.
///
/// Simulates the basilar membrane's frequency decomposition using a bank of
/// 4th-order gammatone filters with center frequencies spaced on the ERB
/// (Equivalent Rectangular Bandwidth) scale.
///
/// After filtering, half-wave rectification + low-pass smoothing models
/// inner hair cell transduction, producing a firing-rate envelope per channel.

use rustfft::{num_complex::Complex, FftPlanner};
use std::f64::consts::PI;

const DEFAULT_NUM_CHANNELS: usize = 32;
const DEFAULT_LOW_FREQ: f64 = 50.0;
const DEFAULT_HIGH_FREQ: f64 = 8000.0;
const FILTER_ORDER: usize = 4;

/// Envelope smoothing LPF cutoff (Hz) — models hair cell membrane time constant.
const ENVELOPE_CUTOFF: f64 = 50.0;

/// Equivalent Rectangular Bandwidth at frequency `f` (Hz).
fn erb(f: f64) -> f64 {
    24.7 * (4.37 * f / 1000.0 + 1.0)
}

/// Convert frequency to ERB-rate scale position.
fn freq_to_erb_rate(f: f64) -> f64 {
    21.4 * (4.37e-3 * f + 1.0).ln() / (10.0_f64.ln())
}

/// Convert ERB-rate scale position back to frequency.
fn erb_rate_to_freq(e: f64) -> f64 {
    (10.0_f64.powf(e / 21.4) - 1.0) / 4.37e-3
}

/// State for one 1st-order complex gammatone stage.
#[derive(Clone)]
struct GammatoneStage {
    re: f64,
    im: f64,
}

/// A single gammatone channel (4th-order = 4 cascaded complex 1st-order stages).
#[derive(Clone)]
struct GammatoneChannel {
    cf: f64,
    stages: [GammatoneStage; FILTER_ORDER],
    // Precomputed coefficients
    cos_cf: f64,
    sin_cf: f64,
    decay: f64,
    // Envelope smoother (1-pole LPF)
    env_coeff: f64,
    env_state: f64,
}

impl GammatoneChannel {
    fn new(center_freq: f64, sample_rate: f64) -> Self {
        let bw = 1.019 * 2.0 * PI * erb(center_freq);
        let dt = 1.0 / sample_rate;
        let decay = (-bw * dt).exp();
        let cf_rad = 2.0 * PI * center_freq * dt;

        // Envelope smoother coefficient
        let env_rc = 1.0 / (2.0 * PI * ENVELOPE_CUTOFF);
        let env_coeff = dt / (env_rc + dt);

        GammatoneChannel {
            cf: center_freq,
            stages: [
                GammatoneStage { re: 0.0, im: 0.0 },
                GammatoneStage { re: 0.0, im: 0.0 },
                GammatoneStage { re: 0.0, im: 0.0 },
                GammatoneStage { re: 0.0, im: 0.0 },
            ],
            cos_cf: cf_rad.cos(),
            sin_cf: cf_rad.sin(),
            decay,
            env_coeff,
            env_state: 0.0,
        }
    }

    /// Process one sample, return the envelope (firing rate) value.
    fn process(&mut self, input: f64) -> f64 {
        // Feed input into first stage
        let mut re = input;
        let mut im = 0.0;

        for stage in &mut self.stages {
            // Complex multiply by exp(j*2π*cf*dt) then decay
            let new_re = self.decay * (stage.re * self.cos_cf - stage.im * self.sin_cf) + re;
            let new_im = self.decay * (stage.re * self.sin_cf + stage.im * self.cos_cf) + im;
            stage.re = new_re;
            stage.im = new_im;
            re = new_re;
            im = new_im;
        }

        // Take magnitude (half-wave rectification is implicit — magnitude is always ≥ 0)
        let magnitude = (re * re + im * im).sqrt();

        // Smooth envelope (1-pole LPF)
        self.env_state += self.env_coeff * (magnitude - self.env_state);
        self.env_state
    }

    fn reset(&mut self) {
        for stage in &mut self.stages {
            stage.re = 0.0;
            stage.im = 0.0;
        }
        self.env_state = 0.0;
    }
}

/// Gammatone filterbank with N channels.
pub struct GammatoneFilterbank {
    channels: Vec<GammatoneChannel>,
    sample_rate: f64,
    /// Per-channel normalization weights (inverse ERB bandwidth).
    /// Compensates for the gammatone filter's higher gain at low frequencies.
    channel_weights: Vec<f64>,
}

impl GammatoneFilterbank {
    pub fn new(sample_rate: f64) -> Self {
        Self::with_params(sample_rate, DEFAULT_NUM_CHANNELS, DEFAULT_LOW_FREQ, DEFAULT_HIGH_FREQ)
    }

    pub fn with_params(sample_rate: f64, num_channels: usize, low_freq: f64, high_freq: f64) -> Self {
        let low_erb = freq_to_erb_rate(low_freq);
        let high_erb = freq_to_erb_rate(high_freq);

        let channels: Vec<GammatoneChannel> = (0..num_channels)
            .map(|i| {
                let erb_pos = low_erb + (high_erb - low_erb) * i as f64 / (num_channels - 1) as f64;
                let cf = erb_rate_to_freq(erb_pos);
                GammatoneChannel::new(cf, sample_rate)
            })
            .collect();

        // Compute per-channel weights: inverse of ERB bandwidth normalised
        // so that a flat-spectrum (white noise) input produces equal energy
        // contribution from each channel.
        let raw_weights: Vec<f64> = channels.iter().map(|ch| 1.0 / erb(ch.cf)).collect();
        let weight_sum: f64 = raw_weights.iter().sum();
        let channel_weights: Vec<f64> = raw_weights.iter().map(|w| w / weight_sum).collect();

        GammatoneFilterbank {
            channels,
            sample_rate,
            channel_weights,
        }
    }

    pub fn num_channels(&self) -> usize {
        self.channels.len()
    }

    pub fn center_frequencies(&self) -> Vec<f64> {
        self.channels.iter().map(|c| c.cf).collect()
    }

    /// Process a mono audio signal, returning the envelope for each channel.
    ///
    /// Returns a Vec of length `num_channels`, each containing a Vec<f64> of
    /// envelope values (one per input sample).
    pub fn process(&mut self, audio: &[f32]) -> Vec<Vec<f64>> {
        let num_channels = self.channels.len();
        let num_samples = audio.len();
        let mut output = vec![vec![0.0_f64; num_samples]; num_channels];

        for (s, &sample) in audio.iter().enumerate() {
            let x = sample as f64;
            for (c, channel) in self.channels.iter_mut().enumerate() {
                output[c][s] = channel.process(x);
            }
        }

        output
    }

    /// Process and return a single aggregated neural input signal.
    ///
    /// Produces a weighted sum of envelopes across all channels at each time
    /// step, using ERB-normalised weights so that each frequency region
    /// contributes proportionally. Without weighting, low-frequency channels
    /// dominate due to higher gammatone filter gain at narrow ERB bandwidths.
    pub fn process_to_neural_input(&mut self, audio: &[f32]) -> Vec<f64> {
        let num_samples = audio.len();
        let mut output = vec![0.0_f64; num_samples];

        for (s, &sample) in audio.iter().enumerate() {
            let x = sample as f64;
            let mut sum = 0.0;
            for (c, channel) in self.channels.iter_mut().enumerate() {
                sum += channel.process(x) * self.channel_weights[c];
            }
            output[s] = sum;
        }

        output
    }

    /// Process audio into 4 tonotopic frequency bands for cortical modelling.
    ///
    /// Groups channels by center frequency into:
    ///   Band 0 — Low:      50–200 Hz
    ///   Band 1 — Low-mid:  200–800 Hz
    ///   Band 2 — Mid-high: 800–3000 Hz
    ///   Band 3 — High:     3000–8000 Hz
    ///
    /// Returns per-band aggregated signals and energy fractions.
    pub fn process_to_band_groups(&mut self, audio: &[f32]) -> BandGroupOutput {
        const BAND_EDGES: [f64; 5] = [0.0, 200.0, 800.0, 3000.0, 20000.0];

        let num_samples = audio.len();
        let num_channels = self.channels.len();

        // Assign each channel to a band based on center frequency
        let mut band_membership: Vec<usize> = vec![0; num_channels];
        for (c, ch) in self.channels.iter().enumerate() {
            for b in 0..4 {
                if ch.cf >= BAND_EDGES[b] && ch.cf < BAND_EDGES[b + 1] {
                    band_membership[c] = b;
                    break;
                }
            }
        }

        // Compute per-band weight sums for normalisation within each band
        let mut band_weight_sums = [0.0_f64; 4];
        for (c, &b) in band_membership.iter().enumerate() {
            band_weight_sums[b] += self.channel_weights[c];
        }

        // Count channels per band for averaging
        let mut band_channel_counts = [0_usize; 4];
        for &b in &band_membership {
            band_channel_counts[b] += 1;
        }

        let mut band_signals = [
            vec![0.0_f64; num_samples],
            vec![0.0_f64; num_samples],
            vec![0.0_f64; num_samples],
            vec![0.0_f64; num_samples],
        ];

        for (s, &sample) in audio.iter().enumerate() {
            let x = sample as f64;
            for (c, channel) in self.channels.iter_mut().enumerate() {
                let env = channel.process(x);
                let b = band_membership[c];
                // Use raw envelopes (no channel_weights) — the 1/ERB weighting
                // attenuates high-frequency channels by ~30x, making those band
                // signals near-zero and preventing the JR model from oscillating.
                // Per-band normalization to [0,1] in pipeline.rs handles scaling.
                band_signals[b][s] += env;
            }
        }

        // Average by channel count so bands with more channels don't dominate
        for b in 0..4 {
            if band_channel_counts[b] > 0 {
                let scale = 1.0 / band_channel_counts[b] as f64;
                for s in 0..num_samples {
                    band_signals[b][s] *= scale;
                }
            }
        }

        // Compute energy fractions directly from FFT of the audio signal.
        // The gammatone filter's 4th-order gain creates ~10^11x low-freq bias
        // in envelope energy, making compensation impractical. FFT gives
        // ground-truth spectral energy distribution.
        let energy_fractions = Self::fft_band_energy_fractions(audio, self.sample_rate);

        BandGroupOutput {
            signals: band_signals,
            energy_fractions,
        }
    }

    /// Compute energy fractions in 4 frequency bands directly from FFT.
    ///
    /// Band edges: 50-200, 200-800, 800-3000, 3000-8000 Hz.
    fn fft_band_energy_fractions(audio: &[f32], sample_rate: f64) -> [f64; 4] {
        const EDGES: [f64; 5] = [50.0, 200.0, 800.0, 3000.0, 8000.0];

        let n = audio.len();
        let fft_len = n.next_power_of_two();
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(fft_len);

        let mut buf: Vec<Complex<f64>> = (0..fft_len)
            .map(|i| {
                if i < n {
                    Complex::new(audio[i] as f64, 0.0)
                } else {
                    Complex::new(0.0, 0.0)
                }
            })
            .collect();
        fft.process(&mut buf);

        let freq_res = sample_rate / fft_len as f64;
        let mut band_powers = [0.0_f64; 4];

        for bin in 1..(fft_len / 2) {
            let freq = bin as f64 * freq_res;
            let power = buf[bin].norm_sqr();

            for b in 0..4 {
                if freq >= EDGES[b] && freq < EDGES[b + 1] {
                    band_powers[b] += power;
                    break;
                }
            }
        }

        let total: f64 = band_powers.iter().sum();
        if total > 1e-30 {
            [
                band_powers[0] / total,
                band_powers[1] / total,
                band_powers[2] / total,
                band_powers[3] / total,
            ]
        } else {
            [0.25; 4]
        }
    }

    pub fn reset(&mut self) {
        for channel in &mut self.channels {
            channel.reset();
        }
    }
}

/// Output of tonotopic band-grouped processing.
pub struct BandGroupOutput {
    /// Per-band aggregated neural input signals (4 bands).
    pub signals: [Vec<f64>; 4],
    /// Fraction of total energy in each band (sums to 1.0).
    pub energy_fractions: [f64; 4],
}

/// Band labels for display.
pub const BAND_LABELS: [&str; 4] = ["Low (50-200)", "Low-mid (200-800)", "Mid-high (800-3k)", "High (3k-8k)"];
