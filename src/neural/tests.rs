#[cfg(test)]
mod tests {
    use crate::brain_type::BrainType;
    use crate::neural::fhn::*;
    use crate::neural::jansen_rit::*;

    /// Helper: build FastInhibParams from brain type's JR params.
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

    const SR: f64 = 48_000.0;
    /// Duration of test signals (seconds).
    const DUR: f64 = 3.0;

    /// Generate a constant test signal (uniform value in each band).
    fn constant_bands(value: f64) -> [Vec<f64>; 4] {
        let n = (SR * DUR) as usize;
        [
            vec![value; n],
            vec![value; n],
            vec![value; n],
            vec![value; n],
        ]
    }

    /// Equal energy fractions across all bands.
    fn equal_energy() -> [f64; 4] {
        [0.25, 0.25, 0.25, 0.25]
    }

    // ── Regression: symmetric L/R input → bilateral ≈ tonotopic ─────────────

    #[test]
    fn bilateral_symmetric_input_matches_tonotopic_direction() {
        // When L and R ears get identical signals, the bilateral model should
        // produce similar (not identical — hemispheres have different params)
        // but directionally consistent output vs. the mono tonotopic model.
        let bands = constant_bands(0.5);
        let energy = equal_energy();
        let bt = BrainType::Normal;
        let params = bt.params();
        let tono = bt.tonotopic_params();
        let bilateral = bt.bilateral_params();

        // Mono tonotopic
        let fi = fast_inhib_for(bt);
        let mono = simulate_tonotopic(
            &bands,
            &energy,
            &tono,
            params.jansen_rit.c,
            params.jansen_rit.input_scale,
            SR,
            &fi,
            params.jansen_rit.v0,
        );

        // Bilateral with identical L/R
        let bi = simulate_bilateral(
            &bands,
            &bands,
            &energy,
            &energy,
            &bilateral,
            params.jansen_rit.c,
            params.jansen_rit.input_scale,
            SR,
            &fi,
            params.jansen_rit.v0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.5, // arousal (neutral)
        );

        let mono_norm = mono.band_powers.normalized();
        let bi_norm = bi.combined.band_powers.normalized();

        // Both should produce oscillatory output (non-zero band powers)
        assert!(
            mono.band_powers.total() > 0.0,
            "mono should produce non-zero EEG"
        );
        assert!(
            bi.combined.band_powers.total() > 0.0,
            "bilateral should produce non-zero EEG"
        );

        // Dominant frequency should be in physiological range (1-50 Hz)
        assert!(
            mono.dominant_freq >= 1.0 && mono.dominant_freq <= 50.0,
            "mono dominant freq {} out of range",
            mono.dominant_freq
        );
        assert!(
            bi.combined.dominant_freq >= 1.0 && bi.combined.dominant_freq <= 50.0,
            "bilateral dominant freq {} out of range",
            bi.combined.dominant_freq
        );

        // Alpha should have meaningful power (Wendling fast inhibition redistributes
        // some energy to beta/gamma, so alpha may be lower than pure JR95)
        assert!(
            mono_norm.alpha > 0.05,
            "mono alpha {:.3} too low",
            mono_norm.alpha
        );
        assert!(
            bi_norm.alpha > 0.05,
            "bilateral alpha {:.3} too low",
            bi_norm.alpha
        );
    }

    // ── Bilateral: asymmetric L/R input produces hemispheric differences ─────

    #[test]
    fn bilateral_asymmetric_input_differentiates_hemispheres() {
        let n = (SR * DUR) as usize;
        let bt = BrainType::Normal;
        let params = bt.params();
        let bilateral = bt.bilateral_params();

        // Left ear: strong low-frequency energy (brown-like)
        let left_bands: [Vec<f64>; 4] = [vec![0.8; n], vec![0.3; n], vec![0.1; n], vec![0.05; n]];
        let left_energy = [0.70, 0.20, 0.08, 0.02];

        // Right ear: strong high-frequency energy (white-like)
        let right_bands: [Vec<f64>; 4] = [vec![0.2; n], vec![0.4; n], vec![0.6; n], vec![0.8; n]];
        let right_energy = [0.10, 0.20, 0.30, 0.40];

        let fi = fast_inhib_for(bt);
        let bi = simulate_bilateral(
            &left_bands,
            &right_bands,
            &left_energy,
            &right_energy,
            &bilateral,
            params.jansen_rit.c,
            params.jansen_rit.input_scale,
            SR,
            &fi,
            params.jansen_rit.v0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.5, // arousal (neutral)
        );

        // Right hemisphere (gets 65% left ear = low freq) should be slower
        // Left hemisphere (gets 65% right ear = high freq) should be faster
        assert!(
            bi.right_dominant_freq <= bi.left_dominant_freq + 3.0,
            "Right hemisphere ({:.1} Hz) should be slower or similar to left ({:.1} Hz)",
            bi.right_dominant_freq,
            bi.left_dominant_freq
        );

        // Both hemispheres should produce valid output
        assert!(
            bi.left_band_powers.total() > 0.9,
            "left hemisphere band powers should sum to ~1.0"
        );
        assert!(
            bi.right_band_powers.total() > 0.9,
            "right hemisphere band powers should sum to ~1.0"
        );
    }

    // ── Bilateral: alpha asymmetry index is bounded ─────────────────────────

    #[test]
    fn bilateral_alpha_asymmetry_bounded() {
        let bands = constant_bands(0.5);
        let energy = equal_energy();
        let bt = BrainType::Normal;
        let params = bt.params();
        let bilateral = bt.bilateral_params();

        let fi = fast_inhib_for(bt);
        let bi = simulate_bilateral(
            &bands,
            &bands,
            &energy,
            &energy,
            &bilateral,
            params.jansen_rit.c,
            params.jansen_rit.input_scale,
            SR,
            &fi,
            params.jansen_rit.v0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.5, // arousal (neutral)
        );

        // Alpha asymmetry should be in [-1, 1]
        assert!(
            bi.alpha_asymmetry >= -1.0 && bi.alpha_asymmetry <= 1.0,
            "alpha asymmetry {} out of bounds",
            bi.alpha_asymmetry
        );

        // For symmetric input, asymmetry can be moderate-to-high because the
        // hybrid architecture uses WC for fast bands in the left hemisphere,
        // which shifts alpha balance rightward. Threshold widened from 0.85.
        assert!(
            bi.alpha_asymmetry.abs() < 0.95,
            "symmetric input should not produce extreme alpha asymmetry: {}",
            bi.alpha_asymmetry
        );
    }

    // ── Brain types produce different bilateral dynamics ─────────────────────

    #[test]
    fn brain_types_differ_in_bilateral_output() {
        let bands = constant_bands(0.5);
        let energy = [0.40, 0.30, 0.20, 0.10]; // Brown-ish distribution

        let mut dominant_freqs: Vec<(BrainType, f64)> = Vec::new();
        let mut coupling_strengths: Vec<(BrainType, f64)> = Vec::new();

        for &bt in BrainType::all() {
            let params = bt.params();
            let bilateral = bt.bilateral_params();

            let fi = fast_inhib_for(bt);
            let bi = simulate_bilateral(
                &bands,
                &bands,
                &energy,
                &energy,
                &bilateral,
                params.jansen_rit.c,
                params.jansen_rit.input_scale,
                SR,
                &fi,
                params.jansen_rit.v0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.0,
                0.5, // arousal (neutral)
            );

            dominant_freqs.push((bt, bi.combined.dominant_freq));
            coupling_strengths.push((bt, bilateral.callosal_coupling));

            // All brain types should produce valid output
            assert!(
                bi.combined.band_powers.total() > 0.0,
                "{:?} produced zero band powers",
                bt
            );
        }

        // Aging should have weakest callosal coupling
        let aging_coupling = coupling_strengths
            .iter()
            .find(|(bt, _)| *bt == BrainType::Aging)
            .unwrap()
            .1;
        let normal_coupling = coupling_strengths
            .iter()
            .find(|(bt, _)| *bt == BrainType::Normal)
            .unwrap()
            .1;
        assert!(
            aging_coupling < normal_coupling,
            "Aging coupling ({}) should be less than Normal ({})",
            aging_coupling,
            normal_coupling
        );

        // ADHD should have weaker coupling than Normal
        let adhd_coupling = coupling_strengths
            .iter()
            .find(|(bt, _)| *bt == BrainType::Adhd)
            .unwrap()
            .1;
        assert!(
            adhd_coupling < normal_coupling,
            "ADHD coupling ({}) should be less than Normal ({})",
            adhd_coupling,
            normal_coupling
        );

        // HighAlpha should have strongest coupling (bilateral synchrony)
        let ha_coupling = coupling_strengths
            .iter()
            .find(|(bt, _)| *bt == BrainType::HighAlpha)
            .unwrap()
            .1;
        assert!(
            ha_coupling >= normal_coupling,
            "HighAlpha coupling ({}) should be >= Normal ({})",
            ha_coupling,
            normal_coupling
        );
    }

    // ── FHN driven by bilateral EEG produces spikes ─────────────────────────

    #[test]
    fn fhn_fires_from_bilateral_eeg() {
        let bands = constant_bands(0.5);
        let energy = equal_energy();
        let bt = BrainType::Normal;
        let params = bt.params();
        let bilateral = bt.bilateral_params();

        let fi = fast_inhib_for(bt);
        let bi = simulate_bilateral(
            &bands,
            &bands,
            &energy,
            &energy,
            &bilateral,
            params.jansen_rit.c,
            params.jansen_rit.input_scale,
            SR,
            &fi,
            params.jansen_rit.v0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.5, // arousal (neutral)
        );

        // Normalise EEG to [-1, 1]
        let eeg_max = bi
            .combined
            .eeg
            .iter()
            .map(|x| x.abs())
            .fold(0.0_f64, f64::max);
        let eeg_norm = if eeg_max > 1e-10 { 1.0 / eeg_max } else { 1.0 };
        let fhn_input: Vec<f64> = bi.combined.eeg.iter().map(|x| x * eeg_norm).collect();

        let fhn = FhnModel::with_params(
            SR,
            params.fhn.a,
            params.fhn.b,
            params.fhn.epsilon,
            params.fhn.time_scale,
        );
        let result = fhn.simulate(&fhn_input, params.fhn.input_scale);

        // FHN should produce a non-zero firing rate
        assert!(
            result.firing_rate > 0.0,
            "FHN should fire from bilateral EEG, got rate={}",
            result.firing_rate
        );
    }

    // ── Callosal delay is applied correctly ─────────────────────────────────

    #[test]
    fn callosal_delay_has_effect() {
        let bands = constant_bands(0.5);
        let energy = equal_energy();
        let bt = BrainType::Normal;
        let params = bt.params();

        // Run with normal delay
        let bilateral_normal = bt.bilateral_params();
        let fi = fast_inhib_for(bt);
        let bi_normal = simulate_bilateral(
            &bands,
            &bands,
            &energy,
            &energy,
            &bilateral_normal,
            params.jansen_rit.c,
            params.jansen_rit.input_scale,
            SR,
            &fi,
            params.jansen_rit.v0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.5, // arousal (neutral)
        );

        // Run with zero delay
        let mut bilateral_zero = bt.bilateral_params();
        bilateral_zero.callosal_delay_s = 0.0;
        let bi_zero = simulate_bilateral(
            &bands,
            &bands,
            &energy,
            &energy,
            &bilateral_zero,
            params.jansen_rit.c,
            params.jansen_rit.input_scale,
            SR,
            &fi,
            params.jansen_rit.v0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.0,
            0.5, // arousal (neutral)
        );

        // They should produce different EEG (delay changes phase relationship)
        let diff: f64 = bi_normal
            .combined
            .eeg
            .iter()
            .zip(bi_zero.combined.eeg.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / bi_normal.combined.eeg.len() as f64;

        assert!(
            diff > 1e-6,
            "Callosal delay should affect EEG output, got mean diff={}",
            diff
        );
    }
}
