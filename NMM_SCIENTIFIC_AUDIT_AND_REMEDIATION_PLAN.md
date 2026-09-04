# NMM Scientific Audit and Remediation Plan

> **Status dokumentu:** uzasadnienie naukowe i historia planu. Aktualny status zadań znajduje się w [NMM — kanoniczny rejestr rozwoju](NMM_DEVELOPMENT_REGISTER.md).

## Purpose

This document separates three different questions that are currently mixed together in the NMM:

1. Is the software internally correct and reproducible?
2. Is the mathematical model self-consistent?
3. Is the model scientifically valid for assessing noise presets in humans?

The current answer is:

- Software quality: relatively strong.
- Mathematical consistency: mixed.
- Scientific validity for preset assessment: not yet sufficient for strong claims.

The NMM is best understood today as a **hybrid research simulator**:

- physiologically inspired auditory preprocessing,
- a neural-mass dynamical core,
- several hand-authored physiological heuristics,
- and a product-oriented scoring layer.

That can be useful for iterative design, but it is not yet a validated surrogate for human EEG or human behavioral benefit.

## Highest-Priority Findings

### F1. The model conflates acoustic carrier frequency with cortical EEG rhythm

**Status:** scientifically incorrect architecture.

The current tonotopic design maps low acoustic bands to slow neural rhythms and high acoustic bands to beta/gamma-capable models. In code, low carrier-frequency groups are assigned JR parameters tuned toward theta/alpha while upper carrier-frequency groups are assigned Wilson-Cowan oscillators tuned to faster rhythms.

**Why this is a problem**

- In auditory neuroscience, **carrier frequency** and **amplitude-modulation frequency** are separate stimulus dimensions.
- Tonotopy represents acoustic frequency content.
- ASSR / envelope tracking represent temporal modulation rate.
- A brown carrier can be modulated at 40 Hz; a bright carrier can be unmodulated. The carrier color does not by itself imply theta versus beta cortical rhythms.

**Current code surfaces**

- `src/brain_type.rs`: `tonotopic_params()`
- `src/neural/jansen_rit.rs`: `simulate_tonotopic()`

**Consequence**

The model can manufacture a strong causal story that is not justified:

- dark / low-frequency noise -> slow EEG rhythms,
- bright / high-frequency noise -> fast EEG rhythms.

That may be a useful engineering prior for product tuning, but it is not a validated neural mechanism.

**Fix direction**

Decouple:

- spectral carrier features,
- temporal envelope features,
- and latent state / arousal variables.

Carrier color should affect:

- cochlear excitation pattern,
- loudness / comfort / masking,
- possibly arousal priors after calibration.

Temporal modulation should drive:

- ASSR,
- envelope PLV,
- rhythm-specific cortical entrainment.

Do not let carrier band alone choose the cortical EEG rhythm family.

---

### F2. The scoring layer uses hand-authored normalized band fractions as if they were validated brain-state targets

**Status:** mathematically convenient, scientifically under-validated.

The goals in `src/scoring.rs` define exact ideal normalized fractions for delta/theta/alpha/beta/gamma plus FHN targets. Those target vectors are useful optimizer objectives, but they are not direct literature-derived human biomarkers.

**Why this is a problem**

- Relative band power can change because of aperiodic 1/f slope changes, not because the oscillation of interest changed.
- Canonical fixed frequency bands ignore individual alpha peak frequency and age-related shifts.
- Many goal labels are cognitive constructs, not one-to-one EEG signatures.
- Some goals are product constructs (`shield`, `flow`, `ignition`) without direct ground-truth EEG definitions.

**Current code surfaces**

- `src/scoring.rs`

**Consequence**

The optimizer is rewarded for matching the simulator's preferred spectral composition, not a verified human state.

**Fix direction**

- Add periodic / aperiodic spectral decomposition.
- Distinguish oscillatory peaks from aperiodic background.
- Replace universal hand-authored targets with calibrated empirical targets once human EEG exists.
- Keep current target vectors as `legacy_v1` heuristics until calibration data supports replacement.

---

### F3. The disturbance test is not the same model as standard evaluation

**Status:** implementation / mathematical correctness defect.

`disturb` claims to run the standard auditory-neural pipeline, but it currently:

- renders dry audio rather than canonical ear signals,
- normalizes each tonotopic band independently instead of globally,
- bypasses CET,
- bypasses ASSR,
- bypasses thalamic gating,
- bypasses habituation,
- bypasses stochastic JR.

**Current code surface**

- `src/disturb.rs`

**Consequence**

Disturbance resilience is measured on a different model than the one used for preset scoring. It cannot be interpreted as resilience of the canonical NMM.

**Fix direction**

Refactor disturbance analysis to reuse the canonical pipeline up to an explicit spike-injection hook. If an ablated disturbance mode is still useful, name it separately and make the feature flags explicit in output metadata.

---

### F4. The thalamic gate implements a useful heuristic, not a validated acoustic-to-arousal mechanism

**Status:** plausible engineering heuristic, not established science.

The current gate converts brightness, reverb, modulation speed, and movement into a scalar arousal value, then maps that to JR offset shifts.

**Why this is a problem**

- Thalamocortical literature supports state-dependent neuromodulation and burst/tonic mode changes.
- It does **not** establish the specific linear mapping:
  - brightness -> arousal,
  - reverb -> arousal,
  - movement -> arousal,
  - then arousal -> fixed `[100%, 70%, 20%, 0%]` band-offset shifts.
- The physiological gate changes the shape of the response, but it still inherits the same hand-authored acoustic arousal estimate.

**Current code surfaces**

- `src/auditory/thalamic_gate.rs`
- `src/auditory/physiological_thalamic_gate.rs`

**Consequence**

Large score differences between presets may partly reflect the model's priors rather than evidence that those acoustic properties actually induce the claimed brain state.

**Fix direction**

- Treat arousal as a latent variable, not a hard-coded truth.
- Keep the current heuristic behind `legacy_v1`.
- Calibrate any future acoustic-to-arousal mapping against psychophysiology and EEG data.
- Report sensitivity to arousal assumptions, not only a single scalar score.

---

### F5. The ADHD mechanism is overcommitted to stochastic resonance

**Status:** incomplete and now scientifically questionable as a primary explanation.

The current ADHD profile is built around hypoarousal, weaker inhibition, and stochastic resonance-like responsiveness to noise.

**Why this is a problem**

- A recent primary EEG study found that both pink noise and a pure tone changed aperiodic slope in adults with elevated ADHD traits, challenging the claim that random noise and stochastic resonance are required.
- The current model has no explicit aperiodic EEG component, so it cannot represent that mechanism even diagnostically.

**Current code surfaces**

- `src/brain_type.rs`
- `src/pipeline.rs`
- `src/scoring.rs`

**Consequence**

The model may reward presets for the wrong ADHD mechanism.

**Fix direction**

- Add aperiodic exponent / offset metrics.
- Treat stochastic resonance as one hypothesis, not the only explanatory mechanism.
- Calibrate ADHD-specific scoring only after EEG and behavioral validation.

---

### F6. The ASSR model captures amplitude gain but misses the best current explanation for the 40 Hz peak

**Status:** incomplete mechanism.

The ASSR path is currently a scalar frequency-response curve applied to modulation amplitude. Recent experimental work indicates the 40 Hz peak is driven strongly by reduced latency variability / greater temporal consistency, not simply larger response amplitude.

**Current code surfaces**

- `src/auditory/assr.rs`
- `src/neural/performance.rs`

**Consequence**

The model can mis-estimate PLV and phase-locking quality, especially for `ignition`-style goals.

**Fix direction**

- Keep amplitude transfer,
- add a phase-jitter / latency-consistency transfer,
- expose both amplitude and phase metrics in diagnostics,
- do not let a high-amplitude 40 Hz response stand in for true entrainment.

---

### F7. The callosal coupling mechanism is defensible as a net-effect approximation, but the wording is scientifically overstated

**Status:** implementation can remain, documentation should be corrected.

Callosal projections are glutamatergic / excitatory, but can recruit local inhibitory interneurons and yield net suppression in target principal cells. Modeling a delayed inhibitory **net effect** is reasonable as a low-order approximation. Saying the corpus callosum is simply or primarily inhibitory is inaccurate.

**Current code surface**

- `src/neural/jansen_rit.rs`

**Fix direction**

- Keep net inhibitory coupling if it fits calibration.
- Rewrite the claim as:
  - excitatory callosal projections can preferentially recruit inhibitory interneurons,
  - therefore the current term is a reduced net-effect approximation.

---

### F8. Some sleep claims are stronger than the open-loop model can support

**Status:** overclaim risk.

The strongest causal sleep evidence for enhancing slow oscillations and memory comes from **closed-loop phase-locked** auditory stimulation during sleep. A preset-level open-loop model can assess:

- masking,
- comfort,
- perhaps sleep-onset friendliness,

but it cannot by itself validate:

- slow-wave enhancement,
- spindle coupling,
- memory consolidation benefit.

**Fix direction**

- Split "sleep onset" from "slow-wave enhancement".
- Keep open-loop preset assessment for sleep onset / comfort.
- Treat slow-wave enhancement as a future closed-loop system goal, not a preset score.

---

### F9. Documentation is not yet internally consistent

**Status:** low-effort correctness issue.

Examples:

- `src/neural/jansen_rit.rs` still documents slow GABA_B subtraction from EEG, while the implementation now uses gain modulation.
- `BRAIN_MODEL_GUIDE.md` describes `disturb` as part of the same canonical stack even though it is not.
- CLI `evaluate` defaults and `SimulationConfig::default()` differ in ASSR behavior, which is intentional but easy to misread.

**Fix direction**

Documentation cleanup should happen before deeper validation work so the team stops reasoning from stale mechanisms.

## What Is Currently Defensible

These parts are reasonable to keep:

- Global rather than per-band normalization in the canonical evaluation pipeline.
- Gammatone-style auditory front end as a practical cochlear approximation.
- Separate treatment of slow envelope tracking and faster modulation pathways.
- Explicit distinction between carrier PLV and envelope PLV.
- Use of a bilateral model and alpha-asymmetry diagnostics, provided claims stay modest.
- FHN as a secondary probe of excitability, provided it is treated as a model readout rather than a direct biomarker.
- Acoustic comfort / masking metrics as a separate branch from neural scoring.

## Remediation Strategy

### Stage 0 - Freeze and version the current model

**Goal:** preserve reproducibility before changing science.

Actions:

1. Introduce explicit `model_version = legacy_v1`.
2. Export every feature flag and every model parameter with each evaluation result.
3. Create golden outputs for representative presets across:
   - all goals,
   - all brain types,
   - short and long durations,
   - canonical and ablation configurations.
4. Keep current scores reproducible forever under `legacy_v1`.

Regression guard:

- All current scores remain bit-identical for `legacy_v1`.

### Stage 1 - Fix implementation parity and documentation

**Goal:** make every command mean what it says.

Actions:

1. Refactor `disturb` to share the canonical auditory and neural path.
2. If legacy disturbance behavior is still useful, expose it as `disturb --legacy-ablated`.
3. Correct GABA_B documentation to gain modulation.
4. Correct callosal wording to net-effect inhibition.
5. Make CLI/config default differences explicit in emitted metadata and docs.

Regression guard:

- `legacy_v1 evaluate` unchanged.
- New `disturb canonical` gets its own golden snapshots.

### Stage 2 - Add missing measurements before changing the model

**Goal:** measure more honestly before scoring differently.

Actions:

1. Add periodic / aperiodic decomposition to diagnostics.
2. Export:
   - aperiodic exponent,
   - aperiodic offset,
   - detected oscillatory peaks,
   - individual alpha peak proxy where applicable.
3. Add ASSR phase-consistency diagnostics separate from amplitude.
4. Add sensitivity reports for arousal assumptions.

Regression guard:

- Diagnostics only.
- No score changes yet.

### Stage 3 - Build `candidate_v2` with decoupled auditory dimensions

**Goal:** remove the carrier-color -> EEG-rhythm conflation.

Actions:

1. Preserve cochlear tonotopy for spectral analysis.
2. Route modulation-frequency information separately into cortical entrainment modules.
3. Make carrier color affect:
   - cochlear energy,
   - comfort,
   - masking,
   - calibrated arousal priors,
   not the oscillator family by construction.
4. Replace hardwired high-band Wilson-Cowan rhythm assignment with an explicit modulation-response path.

Regression guard:

- `legacy_v1` remains untouched.
- `candidate_v2` runs beside it and is compared on the same fixtures.

### Stage 4 - Rebuild the scoring layer from measurable quantities

**Goal:** stop treating hand-authored spectral fractions as ground truth.

Actions:

1. Keep legacy goals unchanged for backward compatibility.
2. Define candidate metrics using:
   - periodic peaks,
   - aperiodic slope,
   - PLV / envelope PLV,
   - comfort,
   - masking,
   - instability / recovery.
3. Split product goals from biological claims:
   - `sleep_onset` versus `slow_wave_enhancement`,
   - `masking` versus `focus`,
   - `40hz_response` versus `cognitive_activation`.
4. Refit goal weights only after empirical data exists.

Regression guard:

- New scores are namespaced, e.g. `legacy_score`, `candidate_score`.
- No silent replacement.

### Stage 5 - Calibrate with human data

**Goal:** turn the simulator from plausible to validated.

Minimum dataset:

- balanced set of presets spanning color, modulation, movement, reverb, and SPL,
- repeated sessions,
- EEG plus behavioral and subjective outcomes,
- enough participants to estimate between-subject variance,
- ADHD and non-ADHD cohorts if ADHD claims remain in scope.

Measurements:

- periodic and aperiodic EEG parameters,
- entrainment PLV,
- subjective comfort / irritation,
- masking efficacy,
- task performance where relevant,
- sleep outcomes only with a protocol appropriate to sleep claims.

Deliverables:

- calibration curves,
- uncertainty intervals,
- model selection results,
- explicit failure cases.

Regression guard:

- keep held-out validation sets,
- require prospective validation before promoting `candidate_v2`.

### Stage 6 - Promote only validated components

**Goal:** make the production model scientifically honest.

Promotion rule:

- A component becomes default only if it improves held-out prediction of human data without degrading acoustic-safety constraints.

Possible outcomes:

- Some legacy heuristics survive because they work empirically.
- Some are removed because they only helped the simulator imitate itself.
- Some goals remain product scores rather than neuroscience scores, and should be labeled that way.

## Practical Fix Order

If engineering time is limited, do the work in this exact order:

1. Canonicalize `disturb`.
2. Fix documentation drift.
3. Add model-versioned outputs and golden fixtures.
4. Add aperiodic metrics.
5. Add ASSR phase-consistency metrics.
6. Build `candidate_v2` with decoupled carrier and modulation pathways.
7. Split unsupported goal semantics.
8. Collect human validation data.
9. Refit scoring.
10. Promote only validated pieces.

## Acceptance Criteria for "Works for Assessing Noise Presets"

The NMM should not be considered scientifically ready until it can do all of the following:

1. Reproduce itself exactly under frozen versioned configs.
2. Use one canonical pipeline across evaluation modes.
3. Separate carrier color, temporal modulation, and latent arousal instead of conflating them.
4. Report periodic and aperiodic EEG structure separately.
5. Distinguish product goals from biological claims.
6. Predict held-out human EEG / behavioral data better than simpler baselines.
7. Quantify uncertainty rather than outputting a single authoritative score.

## Bottom Line

The current NMM is already useful for:

- structured preset exploration,
- ablation studies,
- hypothesis generation,
- product-facing sound design constraints.

It is **not yet scientifically strong enough** to claim that a high preset score means the preset will induce the intended human brain state. The highest-value repairs are:

1. remove the carrier-frequency -> EEG-rhythm conflation,
2. stop treating hand-authored normalized band fractions as validated biomarkers,
3. make disturbance testing use the same model as scoring,
4. add missing aperiodic and phase-consistency measurements,
5. then calibrate against real human data before promoting a new default model.
