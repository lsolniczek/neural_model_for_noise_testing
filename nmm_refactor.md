# NMM Refactor Plan

## Goal

Refactor the current neural mass model into a versioned, scientifically clearer system that can eventually assess noise presets against human-relevant outcomes without destroying reproducibility of the existing simulator.

The plan deliberately avoids a direct rewrite. The current model should remain available as `legacy_v1`; the scientifically corrected architecture should be built beside it as `candidate_v2`, measured against the same presets, then promoted only after validation.

## Why a staged refactor is required

The current NMM already contains useful work:

- a cochlear-style front end,
- a real neural-dynamics core,
- explicit evaluation tooling,
- extensive regression tests,
- acoustic comfort metrics,
- and a practical preset workflow.

Its main problem is not low engineering quality. Its main problem is that several **scientific dimensions are collapsed together**:

1. **Carrier frequency** and **temporal modulation rate** are mixed in the cortical path.
2. **Relative band power** and **true oscillatory structure** are treated as the same thing.
3. **Product goals** and **validated neural states** are scored with the same certainty.
4. **Preset heuristics** and **physiological mechanisms** are not always separated in the API.

The refactor therefore has two requirements:

1. Keep all current behavior reproducible.
2. Create a cleaner second architecture that can be validated against data instead of against the old simulator.

## Scientific basis for the refactor

### 1. Carrier frequency and modulation frequency must be separate model dimensions

Auditory cortex is tonotopic for acoustic carrier frequency, but it also encodes temporal modulation properties. Multiple experimental papers show that modulation frequency is a distinct response dimension, including:

- Lu et al. 2011, *J Neurophysiol*, "Coding of amplitude modulation in primary auditory cortex" (PMID: 21148093).
- Barton et al. 2012, *J Neurosci*, "The topography of frequency and time representation in primate auditory cortices."
- Langner-related periodicity-map work showing periodicity represented orthogonally to frequency maps in auditory cortex.

**Implication for the NMM**

- Tonotopic energy should remain useful for acoustics, masking, and perhaps calibrated arousal priors.
- It should **not** by itself determine whether the modeled cortical response becomes theta, alpha, beta, or gamma.
- Temporal modulation features should be the primary driver of ASSR / envelope-tracking / rhythm-specific response.

### 2. Periodic and aperiodic EEG structure must be separated

Modern spectral analysis shows that canonical band measures can be confounded by the aperiodic 1/f background:

- Donoghue et al. 2020, *Nature Neuroscience*, "Parameterizing neural power spectra into periodic and aperiodic components."
- Donoghue, Dominguez, Voytek 2020, *eNeuro*, "Electrophysiological Frequency Band Ratio Measures Conflate Periodic and Aperiodic Neural Activity."

**Implication for the NMM**

- Current band fractions should remain available for compatibility.
- New diagnostics must expose:
  - aperiodic exponent,
  - aperiodic offset,
  - oscillatory peak frequency,
  - oscillatory peak power above the aperiodic fit.
- Scoring should not be redesigned until those quantities can be observed.

### 3. ADHD scoring must not assume stochastic resonance is the only mechanism

Earlier work supported noise-related cognitive benefit in ADHD:

- Söderlund et al. 2007, *J Child Psychol Psychiatry*, "Listen to the noise: noise is beneficial for cognitive performance in ADHD."

More recent EEG work challenges the stronger claim that random noise and stochastic resonance are required:

- Rijmen et al. 2026, *Journal of Attention Disorders*, "Pink Noise and a Pure Tone Both Reduce 1/f Neural Noise in Adults With Elevated ADHD Traits."

**Implication for the NMM**

- The ADHD profile may keep stochastic-resonance-like behavior as one hypothesis.
- It must also gain aperiodic diagnostics before ADHD-specific claims are made.
- Future ADHD scores should be calibrated empirically, not defended only by one mechanistic story.

### 4. ASSR should model both amplitude and temporal consistency

The current ASSR implementation approximates response magnitude as a scalar gain curve. Recent work shows the 40 Hz ASSR peak is closely tied to network resonance and enhanced temporal consistency:

- Johnson et al. 2024, *Scientific Reports*, "Network resonance and the auditory steady state response."

**Implication for the NMM**

- Keep amplitude transmission as one component.
- Add a separate phase-consistency / latency-jitter component.
- Do not let "high 40 Hz amplitude" stand in for "high-quality entrainment."

### 5. Sleep enhancement claims require closed-loop timing

The strongest causal human evidence for boosting slow oscillations and memory uses phase-locked stimulation:

- Ngo et al. 2013, *Neuron*, "Auditory closed-loop stimulation of the sleep slow oscillation enhances memory."

**Implication for the NMM**

- Open-loop preset assessment can score:
  - comfort,
  - masking,
  - sleep-onset friendliness.
- It must not claim to validate:
  - slow-wave enhancement,
  - spindle coupling,
  - memory improvement.
- Those should be separate future closed-loop goals.

### 6. Callosal coupling should be documented as a net-effect approximation

Callosal projections themselves are excitatory, but they can recruit strong local inhibition:

- Slater and Isaacson 2020, *eNeuro*, "Interhemispheric Callosal Projections Sharpen Frequency Tuning and Enforce Response Fidelity in Primary Auditory Cortex."

**Implication for the NMM**

- A delayed net inhibitory coupling term can remain as a reduced model.
- Documentation must stop saying the corpus callosum is "primarily inhibitory."

## Refactor constraints

These constraints are mandatory for every stage:

1. `legacy_v1` behavior must remain reproducible.
2. New logic must be feature-flagged or model-versioned.
3. New diagnostics are added before they are allowed to change scores.
4. Every stage must add regression coverage before changing defaults.
5. `evaluate`, matrix mode, `optimize`, `generate-data`, and `disturb` must emit enough metadata to reconstruct the exact model path used.
6. No new scalar score may silently replace the old score.

## Target architecture

### Legacy path

```text
audio
  -> gammatone tonotopic bands
  -> legacy band normalization
  -> legacy CET / ASSR / gate logic
  -> legacy tonotopic JR/WC coupling
  -> legacy scalar scoring
```

### Candidate path

```text
audio
  -> cochlear spectral features
  -> temporal modulation features
  -> optional calibrated latent state features
  -> rhythm-response model driven by modulation/state, not carrier band
  -> richer diagnostics
  -> candidate scoring namespace
```

The architectural separation should eventually be visible in types, not only comments:

```text
CochlearFeatures
TemporalModulationFeatures
LatentStateEstimate
CorticalResponse
LegacyScore
CandidateScore
```

## Stage 0 - Freeze and version the current system

### Objective

Make the current simulator reproducible forever before changing its science.

### Scientific rationale

Any model refactor without a stable baseline destroys the ability to tell whether later score changes come from:

- fixed bugs,
- changed assumptions,
- or accidental regressions.

This stage is engineering work, but it is required for scientific interpretability.

### Required implementation work

1. Add explicit model versions:
   - `legacy_v1`
   - reserve `candidate_v2`
2. Export a full model signature with every result:
   - model version,
   - CLI feature flags,
   - all neural parameters,
   - normalization mode,
   - scoring profile,
   - random seeds where applicable.
3. Generate golden fixtures for representative presets:
   - at least one dark, mid, bright, modulated, unmodulated, lateralized, and symmetric preset,
   - all brain types,
   - all goals,
   - short and long durations,
   - canonical and ablation runs.
4. Add a compact serialized config object to exports and CSV logging.

### Suggested files

- `src/pipeline.rs`
- `src/main.rs`
- `src/export.rs`
- `src/surrogate.rs`
- `src/regression_tests.rs`

### Pseudocode

```rust
enum ModelVersion {
    LegacyV1,
    CandidateV2,
}

struct ModelSignature {
    version: ModelVersion,
    scoring_profile: ScoringProfile,
    normalization_mode: NormalizationMode,
    auditory_flags: AuditoryFlags,
    neural_flags: NeuralFlags,
    numeric_params: NumericParamsSnapshot,
}

struct SimulationResult {
    model_signature: ModelSignature,
    legacy_score: f64,
    candidate_score: Option<f64>,
    ...
}
```

### Regression requirements

- Current default behavior under `legacy_v1` must stay bit-identical.
- Existing surrogate contracts remain unchanged until explicitly versioned later.
- Golden fixtures must fail loudly if old behavior changes.

### Acceptance criteria

- Any exported score can be reproduced from its metadata alone.
- A user can compare two runs and see exactly which model path differed.

## Stage 1 - Canonicalize the pipeline and remove documentation drift

### Objective

Ensure all commands mean what they claim and share one canonical implementation path unless explicitly labeled otherwise.

### Scientific rationale

Before discussing validity, every measured quantity must at least come from the same mathematical system. Today `disturb` does not.

### Required implementation work

1. Refactor the canonical pipeline into reusable sub-stages:
   - render,
   - cochlear extraction,
   - normalization,
   - CET / ASSR processing,
   - thalamic gate,
   - cortical simulation,
   - scoring.
2. Insert a named spike-injection hook between auditory preprocessing and cortical simulation.
3. Make canonical disturbance reuse the exact same stages as `evaluate`.
4. Preserve the old disturbance behavior behind an explicit legacy mode if needed.
5. Correct stale documentation:
   - GABA_B is gain modulation, not EEG subtraction,
   - callosal coupling is a net inhibitory approximation, not literal inhibitory axons,
   - clarify CLI defaults versus `SimulationConfig::default()`.

### Suggested files

- `src/pipeline.rs`
- `src/disturb.rs`
- `src/main.rs`
- `src/neural/jansen_rit.rs`
- `BRAIN_MODEL_GUIDE.md`
- `API_DOCUMENTATION.md`

### Pseudocode

```rust
fn preprocess_auditory(
    preset: &Preset,
    config: &SimulationConfig,
) -> AuditoryPreparedState;

fn run_cortical_stage(
    auditory: &AuditoryPreparedState,
    config: &SimulationConfig,
) -> CorticalStageOutput;

fn evaluate_preset(...) -> SimulationResult {
    let auditory = preprocess_auditory(...);
    let cortical = run_cortical_stage(&auditory, ...);
    score(cortical, ...);
}

fn run_disturb(...) -> DisturbResult {
    let mut auditory = preprocess_auditory(...);
    inject_spike(&mut auditory, ...);
    let cortical = run_cortical_stage(&auditory, ...);
    analyze_recovery(cortical, ...);
}
```

### Regression requirements

- `legacy_v1 evaluate` scores unchanged.
- `disturb --legacy-ablated` reproduces old disturbance outputs if retained.
- New canonical disturbance gets independent golden fixtures.

### Acceptance criteria

- The documentation statement "disturb uses the canonical path" becomes true.
- The emitted metadata shows whether a disturbance result was canonical or legacy-ablated.

## Stage 2 - Add scientifically necessary diagnostics without changing scores

### Objective

Expose the measurements the current model is missing before changing any behavior.

### Scientific rationale

Current EEG metrics use normalized band powers only. Donoghue et al. showed that canonical bands and ratios can conflate periodic and aperiodic components. The next scientifically sound step is **measurement expansion**, not score replacement.

### Required implementation work

1. Add periodic / aperiodic spectral decomposition:
   - exponent,
   - offset,
   - peak center frequency,
   - peak bandwidth,
   - peak power above aperiodic fit.
2. Add ASSR temporal-consistency diagnostics:
   - modulation-rate gain,
   - estimated latency-jitter score,
   - expected PLV ceiling / consistency modifier.
3. Add arousal sensitivity diagnostics:
   - score at estimated arousal,
   - score under a bounded arousal sweep,
   - sensitivity / derivative summary.
4. Export diagnostics only; do not feed them into legacy scoring yet.

### Suggested files

- `src/neural/performance.rs`
- new `src/neural/aperiodic.rs`
- `src/auditory/assr.rs`
- `src/pipeline.rs`
- `src/scoring.rs` diagnostics structs only

### Pseudocode: periodic / aperiodic decomposition

```rust
fn parameterize_psd(psd: &Psd) -> SpectralParameterization {
    let fit_band = psd.in_range(2.0, 40.0);
    let log_freq = log10(fit_band.freqs);
    let log_power = log10(fit_band.power);

    let initial_aperiodic_fit = robust_linear_fit(log_freq, log_power);
    let residual = log_power - initial_aperiodic_fit;
    let peaks = detect_gaussian_like_peaks(residual);
    let refit_without_peaks = robust_linear_fit(exclude_peak_bins(log_freq, peaks), ...);

    SpectralParameterization {
        aperiodic_exponent: -slope(refit_without_peaks),
        aperiodic_offset: intercept(refit_without_peaks),
        peaks,
    }
}
```

### Pseudocode: ASSR consistency diagnostics

```rust
fn assr_response(freq_hz: f64) -> AssrResponse {
    AssrResponse {
        amplitude_gain: gain_curve(freq_hz),
        phase_consistency: consistency_curve(freq_hz),
        implied_latency_jitter_ms: jitter_curve(freq_hz),
    }
}
```

### Regression requirements

- Legacy scores unchanged.
- New metrics deterministic and finite on current fixtures.
- New metrics covered for silence, broadband noise, narrowband oscillation, and mixed spectra.

### Acceptance criteria

- We can inspect whether a score change came from oscillatory power or aperiodic background.
- We can report ASSR amplitude and ASSR phase-quality separately.

## Stage 3 - Decouple sensory features in `candidate_v2`

### Objective

Stop using carrier color as the direct selector of modeled EEG rhythm family.

### Scientific rationale

Carrier frequency and modulation rate are distinct experimental dimensions. The NMM should reflect that separation in both math and data structures.

### Required implementation work

1. Introduce separate feature structures:
   - `CochlearFeatures`
   - `TemporalModulationFeatures`
   - `LatentStateEstimate`
2. Keep cochlear band energies for:
   - masking,
   - comfort,
   - loudness,
   - spectral tilt,
   - optional calibrated arousal priors.
3. Extract candidate modulation features from cochlear envelopes independently of carrier-color weighting:
   - build a bilateral per-band envelope signal,
   - standardize each band's envelope to zero mean / unit variance,
   - combine bands with equal weights,
   - run Welch PSD on the combined signal,
   - summarize modulation spectrum features from that PSD.
   - optionally keep per-ear or per-band spectra as diagnostics only.
4. Remove the assumption:
   - low carrier band -> theta,
   - high carrier band -> beta/gamma.
5. Keep the old assumption only inside `legacy_v1`.

### Suggested files

- `src/auditory/gammatone.rs`
- new `src/auditory/features.rs`
- `src/pipeline.rs`
- `src/brain_type.rs`
- `src/neural/jansen_rit.rs`

### Pseudocode

```rust
struct CochlearFeatures {
    band_energy_fractions: [f64; 4],
    brightness: f64,
    spectral_tilt_db_per_oct: f64,
    ...
}

struct TemporalModulationFeatures {
    modulation_psd: Vec<(f64, f64)>,   // e.g. 0.5-80 Hz
    dominant_modulation_hz: Option<f64>,
    band_power_by_mod_rate: ModulationBands,
}

fn extract_temporal_modulation(envelopes_l: &[Vec<f64>; 4], envelopes_r: &[Vec<f64>; 4]) -> TemporalModulationFeatures {
    let mut combined = vec![0.0; n];
    for b in 0..4 {
        let bilateral = 0.5 * (envelopes_l[b] + envelopes_r[b]);
        let z = zscore(bilateral);           // zero-mean, unit-variance per band
        combined += 0.25 * z;               // equal band weighting
    }
    let ac = remove_mean(combined);
    let psd = welch_psd(ac);
    summarize_modulation_spectrum(psd)
}
```

### Candidate-feature semantics note

For candidate Stage 3, this formulation is intentional:

- `TemporalModulationFeatures` is a candidate engineering descriptor for modulation structure that reduces direct carrier-energy dominance.
- `CochlearFeatures` remains the carrier descriptor (brightness, band-energy profile, tilt).
- `total_modulation_power` in this candidate path is therefore a normalized modulation-spectrum magnitude (unitless), not an absolute acoustic-energy proxy.
- This keeps carrier and modulation dimensions explicitly separable before Stage 4 cortical routing.

This is an architecture choice for refactor safety, not a claim that biology literally computes equal-weight z-scored cross-band envelopes.

### Important scientific guardrail

The candidate model may still allow carrier-dependent **gain weighting** if human calibration later supports it. What it must not do by construction is assign the neural rhythm class from carrier band alone.

### Scientific scope of this Stage 3 extractor

The literature supports separation between spectral/carrier and temporal-envelope dimensions, but does not support a strong claim of full carrier invariance (Baumann et al., 2015; Malone et al., 2013). Temporal-envelope coding can still depend on spectral context and carrier type. Stage 3 therefore treats this extractor as a provisional candidate feature for deconfounding the legacy carrier-color -> rhythm mapping, not as a finalized physiological model.

### Regression requirements

- `legacy_v1` remains unchanged.
- Candidate feature extraction must reproduce expected modulation frequencies for known synthetic stimuli.
- Brown and white carriers with the same modulation rate should share the same dominant modulation feature, while retaining different cochlear spectral features.

### Acceptance criteria

- The model can represent:
  - brown noise with 40 Hz modulation,
  - white noise with no modulation,
  - pink noise with 5 Hz modulation,
  without forcing carrier color to define the EEG band outcome.

## Stage 4 - Build the `candidate_v2` cortical response path

### Objective

Make cortical rhythm response depend on temporal modulation and latent state rather than tonotopic carrier assignment.

### Scientific rationale

The current architecture uses JR for low bands and WC for high bands because JR alone cannot generate the desired fast activity. That is an engineering patch, not a physiologically clean statement. The candidate architecture should instead expose its rhythm modules honestly.

### Recommended design

Use explicit response modules:

- slow envelope / delta-theta response,
- alpha resonance response,
- beta response,
- gamma / ASSR response.

These modules may still use JR, Wilson-Cowan, or another oscillator family internally, but their inputs should be:

- temporal modulation features,
- latent arousal/state,
- optional calibrated carrier-dependent gain,
not raw acoustic band index.

### Suggested files

- new `src/neural/candidate_v2.rs`
- `src/neural/mod.rs`
- `src/pipeline.rs`
- `src/brain_type.rs`

### Pseudocode

```rust
fn simulate_candidate_v2(
    modulation: &TemporalModulationFeatures,
    state: &LatentStateEstimate,
    brain: &BrainProfile,
) -> CandidateCorticalResponse {
    let slow_drive = modulation.band(0.5, 9.0);
    let alpha_drive = modulation.band(8.0, 13.0);
    let beta_drive = modulation.band(13.0, 30.0);
    let gamma_drive = modulation.band(30.0, 50.0);

    let slow = slow_module.simulate(slow_drive, state, brain);
    let alpha = alpha_module.simulate(alpha_drive, state, brain);
    let beta = beta_module.simulate(beta_drive, state, brain);
    let gamma = gamma_module.simulate(gamma_drive, state, brain);

    combine_responses(slow, alpha, beta, gamma, brain)
}
```

### Open design decision for the coder model

Do **not** assume the current JR/WC split is automatically the final implementation. The refactor should first create the new interface boundary. The first `candidate_v2` may reuse JR/WC internally for low implementation risk, but the carrier-index dependency must be gone from the API.

### Regression requirements

- Candidate outputs namespace separately from legacy outputs.
- Legacy optimizer and surrogate remain on `legacy_v1` until retrained.
- Add synthetic tests:
  - identical modulation, different carrier colors,
  - same carrier, different modulation rates,
  - no modulation baseline,
  - ASSR-frequency sweep.

### Acceptance criteria

- Candidate model response changes strongly with modulation rate.
- Candidate model response changes modestly with carrier color unless calibrated data supports more.

## Stage 5 - Convert arousal from hard-coded truth to explicit latent estimate

### Objective

Expose acoustic-to-arousal assumptions instead of embedding them as unquestioned physiology.

### Scientific rationale

Thalamocortical literature supports neuromodulatory state changes. It does not directly justify the exact current hand-authored formula:

```text
0.30 * brightness + 0.25 * reverb + 0.25 * modulation_speed + 0.20 * movement
```

That formula may be useful, but it is a prior, not established biology.

### Required implementation work

1. Introduce:
   - `ArousalModel::LegacyHeuristic`
   - `ArousalModel::Fixed`
   - later `ArousalModel::Calibrated`
2. Emit the estimated arousal and model source in metadata.
3. Add a sweep mode:
   - evaluate a preset over a configured arousal range,
   - return score sensitivity.
4. Keep physiological thalamic gate separate from acoustic arousal estimation.

### Suggested files

- `src/auditory/thalamic_gate.rs`
- `src/auditory/physiological_thalamic_gate.rs`
- `src/pipeline.rs`
- `src/main.rs`

### Pseudocode

```rust
enum ArousalModel {
    LegacyHeuristic,
    Fixed(f64),
    Calibrated(CalibrationId),
}

fn estimate_arousal(features: &AcousticFeatures, model: ArousalModel) -> ArousalEstimate;

fn sensitivity_sweep(
    preset: &Preset,
    arousal_values: &[f64],
) -> Vec<ArousalSweepPoint>;
```

### Regression requirements

- Legacy heuristic unchanged in `legacy_v1`.
- Candidate path can run with fixed arousal for controlled experiments.

### Acceptance criteria

- A future researcher can distinguish:
  - "the cortical model predicts X under arousal 0.4"
  - from "the preset is known to induce arousal 0.4."

## Stage 6 - Rebuild scoring as multiple profiles, not one universal truth

### Objective

Separate legacy heuristic scoring, candidate research scoring, and product scoring.

### Scientific rationale

Band-power patterns are not one-to-one definitions of high-level constructs such as focus, flow, or meditation. The current scalar goals are useful optimizer handles, but they are not validated human endpoints.

### Required implementation work

1. Introduce explicit scoring profiles:
   - `LegacyV1`
   - `CandidateResearchV2`
   - `ProductAcoustic`
2. Keep current goals unchanged under `LegacyV1`.
3. Redefine research scores in terms of:
   - periodic peaks,
   - aperiodic slope,
   - PLV,
   - envelope PLV,
   - instability / resilience,
   - calibrated brain-type parameters when available.
4. Split semantically overloaded goals:
   - `sleep_onset`
   - `slow_wave_enhancement_future_closed_loop`
   - `masking`
   - `focused_attention`
   - `40hz_response`
5. Keep product-level acoustic goals separate:
   - privacy,
   - comfort,
   - loudness safety,
   - masking.

### Suggested files

- `src/scoring.rs`
- `src/acoustic_score.rs`
- `src/main.rs`
- `src/export.rs`

### Pseudocode

```rust
enum ScoringProfile {
    LegacyV1,
    CandidateResearchV2,
    ProductAcoustic,
}

struct MultiScoreResult {
    legacy_v1: Option<f64>,
    candidate_research_v2: Option<f64>,
    product_acoustic: Option<f64>,
}
```

### Regression requirements

- Existing optimizer still uses `LegacyV1` unless explicitly changed.
- New profile names must appear in exports and CSV.

### Acceptance criteria

- No single score silently pretends to mean:
  - neural entrainment,
  - listener comfort,
  - and behavioral benefit
  all at once.

## Stage 7 - Revisit brain-type models after diagnostics exist

### Objective

Avoid re-tuning brain types on the wrong measurement basis.

### Scientific rationale

The current ADHD, anxious, aging, and high-alpha profiles are plausible simulation profiles, but they are partly hand-tuned. Refitting them before periodic/aperiodic diagnostics exist risks tuning to artifacts.

### Required implementation work

1. Keep current brain types under `legacy_v1`.
2. For `candidate_v2`, expose brain-profile parameters in structured form:
   - inhibitory kinetics,
   - baseline excitation,
   - response gain,
   - arousal sensitivity,
   - aperiodic priors if supported by data.
3. Only retune after calibration data exists.
4. For ADHD specifically:
   - model stochastic resonance as optional,
   - add aperiodic metrics,
   - do not assume white/pink/brown hierarchy without data.

### Suggested files

- `src/brain_type.rs`
- `src/neural/candidate_v2.rs`
- future calibration config files

### Regression requirements

- `legacy_v1` brain profiles unchanged.
- Candidate profiles must be serialized and exportable.

### Acceptance criteria

- Brain-type differences are inspectable, testable, and later fit to data rather than hidden in comments.

## Stage 8 - Validation dataset and calibration workflow

### Objective

Make future model promotion depend on human evidence.

### Scientific rationale

No neural simulator can validate preset efficacy solely by being internally plausible. The model must predict held-out EEG and behavioral outcomes better than simpler baselines.

### Minimum study design

Participants:

- neurotypical adults,
- elevated-ADHD-trait or clinically diagnosed ADHD cohort if ADHD claims remain in scope.

Stimuli:

- controlled factorial design over:
  - carrier color,
  - modulation rate,
  - modulation depth,
  - reverb,
  - movement,
  - SPL.

Measures:

- EEG:
  - periodic peaks,
  - aperiodic exponent and offset,
  - PLV / envelope PLV,
  - alpha peak frequency,
  - asymmetry where relevant.
- Behavioral:
  - vigilance / CPT-style outcomes for attention claims,
  - task performance if claiming focus benefit.
- Acoustic / subjective:
  - comfort,
  - irritation,
  - masking effectiveness.
- Sleep:
  - separate protocol if sleep claims remain in scope,
  - closed-loop design required for slow-wave enhancement claims.

### Required implementation work

1. Add dataset schema and experiment identifiers.
2. Add calibration scripts outside the runtime path.
3. Fit:
   - arousal model,
   - brain-profile parameters,
   - scoring weights,
   - uncertainty estimates.
4. Compare candidate model against simple baselines:
   - acoustic-only baseline,
   - modulation-only baseline,
   - legacy-v1 score baseline.

### Acceptance criteria

- Candidate model improves held-out prediction over simpler baselines.
- Failure cases are documented.
- Calibration artifacts are versioned.

## Stage 9 - Promotion policy

### Objective

Prevent plausible but unvalidated changes from becoming defaults.

### Promotion rule

No `candidate_v2` component becomes default unless:

1. it improves held-out prediction of human data,
2. it does not worsen safety / acoustic constraints,
3. it survives prospective validation,
4. and it remains interpretable enough to diagnose failure.

### Required implementation work

1. Keep side-by-side reporting:
   - `legacy_v1`
   - `candidate_v2`
2. Add comparison reports.
3. Add deprecation process only after evidence supports it.

## Recommended implementation order

1. Stage 0 - versioning and golden fixtures.
2. Stage 1 - canonical pipeline parity and documentation cleanup.
3. Stage 2 - diagnostics only.
4. Stage 3 - sensory feature separation.
5. Stage 4 - candidate cortical response path.
6. Stage 5 - explicit arousal model abstraction.
7. Stage 6 - multi-profile scoring.
8. Stage 7 - brain-type redesign.
9. Stage 8 - human validation / calibration.
10. Stage 9 - evidence-based promotion.

## What should not be done early

Do not do these before Stage 2 diagnostics exist:

1. Do not retune all goal weights.
2. Do not retune all brain types.
3. Do not remove `legacy_v1`.
4. Do not retrain surrogates on mixed, changing targets.
5. Do not add more optimizer sophistication to compensate for an invalid target.

## Suggested test matrix

### Legacy regression set

- all current presets,
- all goals,
- all brain types,
- canonical and ablation modes,
- short and long durations.

### Candidate synthetic set

1. Brown carrier, 5 Hz AM.
2. Brown carrier, 40 Hz AM.
3. White carrier, 5 Hz AM.
4. White carrier, 40 Hz AM.
5. Same carrier, modulation sweep 1-50 Hz.
6. Same modulation, color sweep.
7. Silence.
8. Static unmodulated noise.
9. Symmetric versus lateralized placement.
10. Arousal fixed low / mid / high.

### Required assertions

- Legacy fixtures remain exact.
- Candidate modulation features track modulation rate independently of carrier color.
- Candidate acoustic features track carrier color independently of modulation rate.
- Aperiodic metric extraction is stable on known synthetic spectra.
- ASSR amplitude and consistency are reported separately.
- Canonical disturbance uses same front-end state as canonical evaluation.

## Reference list

1. Lu T, Liang L, Wang X. Coding of amplitude modulation in primary auditory cortex. *J Neurophysiol*. 2011. PMID: 21148093.
2. Barton B, Venezia JH, Saberi K, Hickok G, Brewer AA. The topography of frequency and time representation in primate auditory cortices. *Cereb Cortex*. 2012.
3. Donoghue T et al. Parameterizing neural power spectra into periodic and aperiodic components. *Nat Neurosci*. 2020. DOI: 10.1038/s41593-020-00744-x.
4. Donoghue T, Dominguez J, Voytek B. Electrophysiological Frequency Band Ratio Measures Conflate Periodic and Aperiodic Neural Activity. *eNeuro*. 2020. DOI: 10.1523/ENEURO.0192-20.2020.
5. Söderlund G et al. Listen to the noise: noise is beneficial for cognitive performance in ADHD. *J Child Psychol Psychiatry*. 2007. PMID: 17683456.
6. Rijmen J, Senoussi M, Wiersema JR. Pink Noise and a Pure Tone Both Reduce 1/f Neural Noise in Adults With Elevated ADHD Traits. *J Atten Disord*. 2026. DOI: 10.1177/10870547251357074.
7. Johnson TD et al. Network resonance and the auditory steady state response. *Sci Rep*. 2024. DOI: 10.1038/s41598-024-66697-4.
8. Ngo HVV et al. Auditory closed-loop stimulation of the sleep slow oscillation enhances memory. *Neuron*. 2013. PMID: 23583623.
9. Slater BJ, Isaacson JS. Interhemispheric Callosal Projections Sharpen Frequency Tuning and Enforce Response Fidelity in Primary Auditory Cortex. *eNeuro*. 2020. PMID: 32769158.
10. Baumann S et al. The topography of frequency and time representation in primate auditory cortices. *eLife*. 2015.
11. Malone BJ, Scott BH, Semple MN. Spectral context affects temporal processing in awake auditory cortex. *J Neurosci*. 2013. PMID: 23658174.
