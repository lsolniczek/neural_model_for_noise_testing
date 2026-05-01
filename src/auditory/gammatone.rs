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

/// Envelope smoothing LPF cutoff (Hz) — models auditory cortex temporal resolution.
/// Raised from 50→80 Hz to allow gamma-band (30-80 Hz) envelope modulations
/// to reach the Wendling 2002 fast-inhibitory population.
const ENVELOPE_CUTOFF: f64 = 80.0;

/// Inner band boundaries (Hz) shared between channel assignment and FFT energy
/// fractions.  The 4 tonotopic bands are:
///   Band 0 — Low:      [LOW_FREQ,  200)
///   Band 1 — Low-mid:  [200,       800)
///   Band 2 — Mid-high: [800,      3000)
///   Band 3 — High:     [3000, HIGH_FREQ]
const BAND_BOUNDARIES: [f64; 3] = [200.0, 800.0, 3000.0];

/// Return the tonotopic band index (0–3) for a given frequency.
/// Frequencies below the first boundary → band 0; above the last → band 3.
fn band_for_freq(freq: f64) -> usize {
    for (b, &edge) in BAND_BOUNDARIES.iter().enumerate() {
        if freq < edge {
            return b;
        }
    }
    3
}

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
        Self::with_params(
            sample_rate,
            DEFAULT_NUM_CHANNELS,
            DEFAULT_LOW_FREQ,
            DEFAULT_HIGH_FREQ,
        )
    }

    pub fn with_params(
        sample_rate: f64,
        num_channels: usize,
        low_freq: f64,
        high_freq: f64,
    ) -> Self {
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
        let num_samples = audio.len();

        // Assign each channel to a band based on center frequency
        let band_membership: Vec<usize> = self
            .channels
            .iter()
            .map(|ch| band_for_freq(ch.cf))
            .collect();

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
    /// Uses the same `BAND_BOUNDARIES` as channel assignment.  Only energy in
    /// the filterbank range [`DEFAULT_LOW_FREQ`, `DEFAULT_HIGH_FREQ`] is
    /// counted; out-of-range bins are ignored since no gammatone channel covers
    /// them.  A Hann window is applied before the FFT to reduce spectral
    /// leakage at band boundaries.
    fn fft_band_energy_fractions(audio: &[f32], sample_rate: f64) -> [f64; 4] {
        let n = audio.len();
        let fft_len = n.next_power_of_two();
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(fft_len);

        // Hann window: w(i) = 0.5 * (1 - cos(2π·i / (N-1)))
        let hann_denom = if n > 1 { (n - 1) as f64 } else { 1.0 };
        let mut buf: Vec<Complex<f64>> = (0..fft_len)
            .map(|i| {
                if i < n {
                    let w = 0.5 * (1.0 - (2.0 * PI * i as f64 / hann_denom).cos());
                    Complex::new(audio[i] as f64 * w, 0.0)
                } else {
                    Complex::new(0.0, 0.0)
                }
            })
            .collect();
        fft.process(&mut buf);

        let freq_res = sample_rate / fft_len as f64;
        let mut band_powers = [0.0_f64; 4];

        // Only accumulate bins in [DEFAULT_LOW_FREQ, DEFAULT_HIGH_FREQ]
        let min_bin = (DEFAULT_LOW_FREQ / freq_res).ceil() as usize;
        let max_bin = ((DEFAULT_HIGH_FREQ / freq_res).floor() as usize).min(fft_len / 2);

        for bin in min_bin..max_bin {
            let freq = bin as f64 * freq_res;
            let power = buf[bin].norm_sqr();
            band_powers[band_for_freq(freq)] += power;
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
pub const BAND_LABELS: [&str; 4] = [
    "Low (50-200)",
    "Low-mid (200-800)",
    "Mid-high (800-3k)",
    "High (3k-8k)",
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48_000.0;

    /// Generate a pure sine tone at the given frequency and duration.
    fn sine_tone(freq: f64, sample_rate: f64, duration_secs: f64) -> Vec<f32> {
        let n = (sample_rate * duration_secs) as usize;
        (0..n)
            .map(|i| (2.0 * PI * freq * i as f64 / sample_rate).sin() as f32)
            .collect()
    }

    /// Generate silence.
    fn silence(sample_rate: f64, duration_secs: f64) -> Vec<f32> {
        vec![0.0_f32; (sample_rate * duration_secs) as usize]
    }

    // ---------------------------------------------------------------
    // ERB formula
    // ---------------------------------------------------------------

    #[test]
    fn erb_known_values() {
        // ERB(1000) = 24.7 * (4.37 + 1) = 24.7 * 5.37 = 132.639
        let e = erb(1000.0);
        assert!((e - 132.639).abs() < 0.01, "ERB(1000) = {e}");

        // ERB(0) = 24.7 (only the constant term)
        assert!((erb(0.0) - 24.7).abs() < 1e-10);

        // ERB(50) ≈ 30.1 (lowest filterbank freq)
        assert!((erb(50.0) - 30.097).abs() < 0.1, "ERB(50) = {}", erb(50.0));
    }

    // ---------------------------------------------------------------
    // ERB-rate round-trip
    // ---------------------------------------------------------------

    #[test]
    fn erb_rate_round_trip() {
        for &f in &[50.0, 200.0, 500.0, 1000.0, 4000.0, 8000.0] {
            let e = freq_to_erb_rate(f);
            let f2 = erb_rate_to_freq(e);
            assert!(
                (f - f2).abs() < 1e-6,
                "Round-trip failed for {f} Hz: got {f2}"
            );
        }
    }

    // ---------------------------------------------------------------
    // band_for_freq helper
    // ---------------------------------------------------------------

    #[test]
    fn band_for_freq_boundaries() {
        // Band 0: [0, 200)
        assert_eq!(band_for_freq(50.0), 0);
        assert_eq!(band_for_freq(199.9), 0);
        // Band 1: [200, 800)
        assert_eq!(band_for_freq(200.0), 1);
        assert_eq!(band_for_freq(500.0), 1);
        assert_eq!(band_for_freq(799.9), 1);
        // Band 2: [800, 3000)
        assert_eq!(band_for_freq(800.0), 2);
        assert_eq!(band_for_freq(2000.0), 2);
        assert_eq!(band_for_freq(2999.9), 2);
        // Band 3: [3000, ∞)
        assert_eq!(band_for_freq(3000.0), 3);
        assert_eq!(band_for_freq(8000.0), 3);
    }

    // ---------------------------------------------------------------
    // Filterbank construction
    // ---------------------------------------------------------------

    #[test]
    fn filterbank_channel_count() {
        let fb = GammatoneFilterbank::new(SR);
        assert_eq!(fb.num_channels(), 32);
    }

    #[test]
    fn filterbank_center_freq_range_and_ordering() {
        let fb = GammatoneFilterbank::new(SR);
        let cfs = fb.center_frequencies();

        // First and last match the configured range
        assert!(
            (cfs[0] - DEFAULT_LOW_FREQ).abs() < 0.1,
            "First cf = {}",
            cfs[0]
        );
        assert!(
            (cfs[31] - DEFAULT_HIGH_FREQ).abs() < 0.5,
            "Last cf = {}",
            cfs[31]
        );

        // Monotonically increasing
        for w in cfs.windows(2) {
            assert!(w[1] > w[0], "CFs not monotonic: {} >= {}", w[0], w[1]);
        }
    }

    #[test]
    fn channel_weights_sum_to_one() {
        let fb = GammatoneFilterbank::new(SR);
        let sum: f64 = fb.channel_weights.iter().sum();
        assert!((sum - 1.0).abs() < 1e-12, "Weights sum = {sum}");
    }

    #[test]
    fn channel_weights_decrease_with_frequency() {
        // Higher ERB at high frequencies → lower weight
        let fb = GammatoneFilterbank::new(SR);
        assert!(
            fb.channel_weights[0] > fb.channel_weights[31],
            "Low-freq weight {} should exceed high-freq weight {}",
            fb.channel_weights[0],
            fb.channel_weights[31]
        );
    }

    // ---------------------------------------------------------------
    // Silent input
    // ---------------------------------------------------------------

    #[test]
    fn silence_produces_zero_envelope() {
        let mut fb = GammatoneFilterbank::new(SR);
        let audio = silence(SR, 0.1);
        let envelopes = fb.process(&audio);

        for (c, ch_env) in envelopes.iter().enumerate() {
            let max = ch_env.iter().cloned().fold(0.0_f64, f64::max);
            assert!(
                max < 1e-15,
                "Channel {c} envelope should be zero for silence, got {max}"
            );
        }
    }

    #[test]
    fn silence_neural_input_is_zero() {
        let mut fb = GammatoneFilterbank::new(SR);
        let audio = silence(SR, 0.1);
        let ni = fb.process_to_neural_input(&audio);
        let max = ni.iter().cloned().fold(0.0_f64, f64::max);
        assert!(
            max < 1e-15,
            "Neural input should be zero for silence, got {max}"
        );
    }

    #[test]
    fn silence_band_groups_zero_signal_uniform_fractions() {
        let mut fb = GammatoneFilterbank::new(SR);
        let audio = silence(SR, 0.1);
        let bg = fb.process_to_band_groups(&audio);

        for b in 0..4 {
            let max = bg.signals[b].iter().cloned().fold(0.0_f64, f64::max);
            assert!(max < 1e-15, "Band {b} signal should be zero for silence");
        }
        // Energy fractions should be uniform [0.25; 4] for silence
        for b in 0..4 {
            assert!(
                (bg.energy_fractions[b] - 0.25).abs() < 1e-10,
                "Band {b} energy fraction should be 0.25 for silence, got {}",
                bg.energy_fractions[b]
            );
        }
    }

    // ---------------------------------------------------------------
    // Pure tone — envelope concentrated in correct channel
    // ---------------------------------------------------------------

    #[test]
    fn pure_tone_1khz_peak_channel() {
        let mut fb = GammatoneFilterbank::new(SR);
        let audio = sine_tone(1000.0, SR, 0.5);
        let envelopes = fb.process(&audio);

        // Find channel with max energy (take the last 25% to avoid transient)
        let start = audio.len() * 3 / 4;
        let mut best_ch = 0;
        let mut best_energy = 0.0_f64;
        for (c, ch_env) in envelopes.iter().enumerate() {
            let energy: f64 = ch_env[start..].iter().map(|x| x * x).sum();
            if energy > best_energy {
                best_energy = energy;
                best_ch = c;
            }
        }

        // The peak channel's center freq should be close to 1000 Hz
        let peak_cf = fb.center_frequencies()[best_ch];
        assert!(
            (peak_cf - 1000.0).abs() < 200.0,
            "1 kHz tone peaked at channel {best_ch} with cf={peak_cf} Hz"
        );
    }

    // ---------------------------------------------------------------
    // Pure tone — FFT energy fractions in correct band
    // ---------------------------------------------------------------

    #[test]
    fn pure_tone_100hz_energy_in_band0() {
        let mut fb = GammatoneFilterbank::new(SR);
        let audio = sine_tone(100.0, SR, 1.0);
        let bg = fb.process_to_band_groups(&audio);
        assert!(
            bg.energy_fractions[0] > 0.95,
            "100 Hz tone: band 0 fraction = {} (expected > 0.95)",
            bg.energy_fractions[0]
        );
    }

    #[test]
    fn pure_tone_500hz_energy_in_band1() {
        let mut fb = GammatoneFilterbank::new(SR);
        let audio = sine_tone(500.0, SR, 1.0);
        let bg = fb.process_to_band_groups(&audio);
        assert!(
            bg.energy_fractions[1] > 0.95,
            "500 Hz tone: band 1 fraction = {} (expected > 0.95)",
            bg.energy_fractions[1]
        );
    }

    #[test]
    fn pure_tone_1500hz_energy_in_band2() {
        let mut fb = GammatoneFilterbank::new(SR);
        let audio = sine_tone(1500.0, SR, 1.0);
        let bg = fb.process_to_band_groups(&audio);
        assert!(
            bg.energy_fractions[2] > 0.95,
            "1500 Hz tone: band 2 fraction = {} (expected > 0.95)",
            bg.energy_fractions[2]
        );
    }

    #[test]
    fn pure_tone_5000hz_energy_in_band3() {
        let mut fb = GammatoneFilterbank::new(SR);
        let audio = sine_tone(5000.0, SR, 1.0);
        let bg = fb.process_to_band_groups(&audio);
        assert!(
            bg.energy_fractions[3] > 0.95,
            "5000 Hz tone: band 3 fraction = {} (expected > 0.95)",
            bg.energy_fractions[3]
        );
    }

    // ---------------------------------------------------------------
    // Energy fractions always sum to 1.0
    // ---------------------------------------------------------------

    #[test]
    fn energy_fractions_sum_to_one() {
        let mut fb = GammatoneFilterbank::new(SR);
        // Use a signal with energy across bands (mixed tones)
        let n = (SR * 1.0) as usize;
        let audio: Vec<f32> = (0..n)
            .map(|i| {
                let t = i as f64 / SR;
                ((2.0 * PI * 150.0 * t).sin()
                    + (2.0 * PI * 600.0 * t).sin()
                    + (2.0 * PI * 2000.0 * t).sin()
                    + (2.0 * PI * 5000.0 * t).sin()) as f32
                    * 0.25
            })
            .collect();

        let bg = fb.process_to_band_groups(&audio);
        let sum: f64 = bg.energy_fractions.iter().sum();
        assert!(
            (sum - 1.0).abs() < 1e-6,
            "Energy fractions sum = {sum} (expected 1.0)"
        );
    }

    // ---------------------------------------------------------------
    // Hann window reduces spectral leakage
    // ---------------------------------------------------------------

    #[test]
    fn hann_window_reduces_leakage() {
        // A 199 Hz tone sits just below the band 0/1 boundary (200 Hz).
        // Without windowing, leakage would bleed significant energy into
        // band 1.  With windowing, band 0 should capture almost all energy.
        let mut fb = GammatoneFilterbank::new(SR);
        let audio = sine_tone(199.0, SR, 1.0);
        let bg = fb.process_to_band_groups(&audio);
        assert!(
            bg.energy_fractions[0] > 0.90,
            "199 Hz tone near boundary: band 0 fraction = {} (expected > 0.90 with Hann window)",
            bg.energy_fractions[0]
        );
    }

    // ---------------------------------------------------------------
    // Band group channel membership
    // ---------------------------------------------------------------

    #[test]
    fn all_channels_assigned_to_band() {
        let fb = GammatoneFilterbank::new(SR);
        let mut band_counts = [0_usize; 4];
        for ch in &fb.channels {
            let b = band_for_freq(ch.cf);
            assert!(b < 4, "Channel cf={} assigned to invalid band {b}", ch.cf);
            band_counts[b] += 1;
        }
        // Every band should have at least one channel
        for (b, &count) in band_counts.iter().enumerate() {
            assert!(count > 0, "Band {b} has no channels");
        }
        assert_eq!(
            band_counts.iter().sum::<usize>(),
            32,
            "Total channels across bands should be 32"
        );
    }

    // ---------------------------------------------------------------
    // Band signal: non-zero output for tone in that band
    // ---------------------------------------------------------------

    #[test]
    fn band_signal_nonzero_for_matching_tone() {
        let mut fb = GammatoneFilterbank::new(SR);
        let audio = sine_tone(500.0, SR, 0.5);
        let bg = fb.process_to_band_groups(&audio);

        // Band 1 (200–800 Hz) should have substantial signal
        let band1_energy: f64 = bg.signals[1].iter().map(|x| x * x).sum();
        assert!(
            band1_energy > 1e-6,
            "Band 1 should have non-zero signal for 500 Hz tone, energy = {band1_energy}"
        );
    }

    // ---------------------------------------------------------------
    // Reset clears state
    // ---------------------------------------------------------------

    #[test]
    fn reset_clears_filter_state() {
        let mut fb = GammatoneFilterbank::new(SR);

        // Process some audio to build up state
        let audio = sine_tone(1000.0, SR, 0.1);
        let _ = fb.process(&audio);

        // Reset
        fb.reset();

        // Process silence — should be zero (no ringing from prior state)
        let audio2 = silence(SR, 0.05);
        let envelopes = fb.process(&audio2);
        for (c, ch_env) in envelopes.iter().enumerate() {
            let max = ch_env.iter().cloned().fold(0.0_f64, f64::max);
            assert!(
                max < 1e-15,
                "After reset, channel {c} should produce zero for silence, got {max}"
            );
        }
    }

    // ---------------------------------------------------------------
    // process_to_neural_input produces non-negative output
    // ---------------------------------------------------------------

    #[test]
    fn neural_input_non_negative() {
        let mut fb = GammatoneFilterbank::new(SR);
        let audio = sine_tone(440.0, SR, 0.5);
        let ni = fb.process_to_neural_input(&audio);
        for (i, &v) in ni.iter().enumerate() {
            assert!(v >= 0.0, "Neural input sample {i} is negative: {v}");
        }
    }

    // ---------------------------------------------------------------
    // Output length matches input length
    // ---------------------------------------------------------------

    #[test]
    fn output_length_matches_input() {
        let mut fb = GammatoneFilterbank::new(SR);
        let audio = sine_tone(1000.0, SR, 0.1);
        let n = audio.len();

        // process
        fb.reset();
        let env = fb.process(&audio);
        assert_eq!(env.len(), 32);
        for ch in &env {
            assert_eq!(ch.len(), n);
        }

        // process_to_neural_input
        fb.reset();
        let ni = fb.process_to_neural_input(&audio);
        assert_eq!(ni.len(), n);

        // process_to_band_groups
        fb.reset();
        let bg = fb.process_to_band_groups(&audio);
        for b in 0..4 {
            assert_eq!(bg.signals[b].len(), n);
        }
    }
}
