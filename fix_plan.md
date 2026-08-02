# NMM–DSP Compatibility Fix Plan

## Goal

Make NMM evaluation reproduce the same audio path used by the shipping iOS presets, then regenerate model baselines only after renderer parity is proven.

The order is intentional: engine configuration and preset identity must be corrected before model scores or golden snapshots are recalibrated. Otherwise, new baselines would legitimize audio produced by the wrong renderer.

## Stage 1 — Freeze and document the current baseline

**Purpose:** Preserve enough information to compare the current behavior with the corrected implementation.

**Work:**

- Record the NMM, DSP, and iOS repository commit hashes used for the review.
- Save the current NMM test result: 511 passed, 2 failed, 5 ignored.
- Save the current DSP result: 667 passed, 0 failed, 2 ignored.
- Preserve evaluation summaries for all 60 preset JSON files.
- Identify which JSON and Swift presets are intended to represent the same shipping preset.
- Do not modify or delete existing user reports or experimental preset files.

**Completion criteria:** A baseline note or machine-readable manifest identifies all three revisions, renderer settings, preset inputs, test commands, and results.

**Completed and review-repaired (2026-08-01):** [`baselines/compatibility/stage1_legacy_pre_parity/manifest.json`](baselines/compatibility/stage1_legacy_pre_parity/manifest.json) is now a self-contained schema-v2 baseline. It freezes all 60 preset inputs, relevant iOS provider/integration evidence, source fingerprints, renderer observations, toolchain provenance, 60 structured 2.1-second `focus`/`normal` reports, candidate-only Swift↔JSON mappings, and full test logs. Offline verification passes; replay always rebuilds current NMM/DSP code and reproduced 60/60 reports byte-for-byte. The recorded test baseline remains NMM 511 passed / 2 known golden failures / 5 ignored and DSP 667 passed / 0 failed / 2 ignored.

## Stage 2 — Define and version the production renderer contract

**Purpose:** Give NMM one explicit description of the engine it is supposed to reproduce.

**Work:**

- Define the production defaults currently configured by the iOS app:
  - 48 kHz sample rate.
  - Crossfeed enabled with strength 0.4.
  - Sparse multiband velvet reverb.
  - Outdoor room mode.
  - Explicit OpenField or Forest environment selected by each preset.
- Decide which settings are global app settings and which belong in individual presets.
- Add a renderer-contract version so future DSP changes cannot silently alter the meaning of old reports.
- Pin the DSP dependency to an identifiable revision or expose a DSP build/version identifier to NMM.
- Extend `ModelSignature` with at least:
  - DSP revision/version.
  - Renderer-contract version.
  - Room mode and acoustic environment.
  - Reverb mode.
  - Crossfeed enabled/strength.
  - Sample rate and relevant warm-up behavior.

**Completion criteria:** Two reports created with different DSP revisions or renderer settings have different signatures, while identical configurations produce identical signatures.

## Stage 3 — Bring the NMM preset schema up to date

**Purpose:** Allow NMM to represent every parameter used by current production presets without silent fallback.

**Work:**

- Add `RoomMode::Outdoor` to the NMM room configuration.
- Add `OpenField` and `Forest` to the NMM environment mapping.
- Expand anchor color bounds to include Blue.
- Confirm that all DSP colors, source kinds, modulators, movement types, position spaces, spread, tint, tone, and room fields have lossless JSON mappings.
- Remove `spatial_mode` if it no longer has an engine meaning, or implement and test its intended behavior.
- Replace catch-all enum fallbacks with validation errors for unsupported values.
- Add a schema version and a documented migration path for existing preset JSON files.

**Completion criteria:** Every shipping preset can be serialized to NMM JSON and decoded again without losing or changing any engine parameter.

## Stage 4 — Make NMM configure the DSP exactly like production

**Purpose:** Ensure the samples analyzed by NMM are produced by the same DSP path as the app.

**Work:**

- Create one NMM engine-construction/configuration function based on the renderer contract from Stage 2.
- Enable crossfeed and set its production strength.
- Select sparse multiband velvet reverb.
- Set room mode **before** applying the acoustic environment, because the DSP interprets environments according to the active room mode.
- Apply room geometry only for modes that use it.
- Apply preset fields in a deterministic order and document that order.
- Add focused tests that read back configuration where possible and compare rendered samples where it is not.

**Completion criteria:** Given the same seed and preset, NMM and a production-configured DSP harness generate matching initial frames and matching audio hashes within the chosen deterministic tolerance.

## Stage 5 — Remove unintended double room processing

**Purpose:** Prevent NMM from analyzing audio with more reverberation than the production app.

**Work:**

- Stop applying the additional NMM-generated RIR after the DSP has already rendered its room/environment path.
- If the custom RIR remains scientifically useful, expose it as a separate, explicitly named analysis variant rather than the production-equivalent default.
- Include that analysis variant in `ModelSignature`.
- Rename `render_preset_stereo_dry` if it continues to contain DSP room effects; the function name must describe the actual signal.

**Completion criteria:** The production-equivalent path contains exactly one room/environment implementation. Tests demonstrate that enabling an optional analysis RIR changes both the signature and the output.

## Stage 6 — Establish one canonical source for production presets

**Purpose:** Eliminate drift between NMM JSON files and handwritten Swift preset providers.

**Work:**

- Choose a canonical, versioned, machine-readable preset representation.
- Generate Swift providers from that representation, or make the app load the canonical data directly.
- Give each preset a stable ID and revision.
- Store the preset ID/revision in NMM evaluation reports.
- Build a parity check that compares every canonical field with the values applied by the shipping provider.
- Migrate current presets deliberately; do not assume similarly named JSON and Swift files are equivalent.

**Completion criteria:** A CI check fails whenever a shipping Swift preset and its canonical NMM representation differ in any audible parameter.

## Stage 7 — Repair the DEBUG JSON preset bridge

**Purpose:** Make in-app JSON testing faithful to NMM and production behavior.

**Work:**

- Decode and apply room mode, room geometry, position space, spread, source kind, tone frequency/amplitude, tint, and all other current schema fields.
- Map Isochronic and RandomPulse instead of silently converting them to Flat.
- Map Outdoor, OpenField, and Forest.
- Reject unknown enum values and malformed presets with clear diagnostics.
- Add decoding/application tests covering all enum variants and optional fields.

**Completion criteria:** Loading a canonical JSON preset through the DEBUG bridge produces the same engine configuration and deterministic audio as loading the corresponding production preset.

## Stage 8 — Correct known preset-level discrepancies

**Purpose:** Fix concrete cases where comments, model inputs, and rendered behavior disagree.

**Work:**

- Resolve the Flow carrier intent:
  - Use 400/410 Hz if the intended binaural difference is 10 Hz.
  - Or retain 400/415 Hz and describe it correctly as 15 Hz.
- Reconcile Flow tone volumes and positions between JSON and Swift rather than copying either version without review.
- Replace the 0.1 Hz `NeuralLfo` with `SineLfo` if a true 0.1 Hz modulation is intended, because the DSP clamps `NeuralLfo` to a minimum of 1 Hz.
- Audit the remaining production/JSON pairs for the same kinds of mismatch.
- Treat neuroscience/therapeutic comments as hypotheses unless they are separately validated; they must not substitute for checking the rendered signal.

**Completion criteria:** Preset comments, canonical data, applied DSP parameters, and measured modulation/carrier frequencies agree.

## Stage 9 — Enforce preset structural validity

**Purpose:** Convert late optimizer panics into early, actionable validation errors.

**Work:**

- Require exactly eight object slots when the fixed-length 230-value genome format is used.
- Validate object count, source count, active-object consistency, finite values, enum ranges, and required fields during deserialization or before evaluation/optimization.
- Return structured errors instead of panicking on a short genome.
- Decide whether inactive padding objects should be inserted during migration of older seven-object presets.
- Add regression coverage using `normal_set_ignition_v3.json`.

**Completion criteria:** Evaluation and optimizer warm-start either accept the same validated preset or return a clear validation error; neither path panics.

## Stage 10 — Add cross-repository audio-parity regression tests

**Purpose:** Detect future integration drift immediately.

**Work:**

- Build a small shared fixture set containing representative presets:
  - Indoor/legacy.
  - Outdoor OpenField.
  - Outdoor Forest.
  - Tone/binaural sources.
  - Isochronic and RandomPulse modulation.
  - Movement, spread, and room-normalized positions.
- Render each fixture through the NMM harness and the production DSP configuration with identical seeds.
- Compare configuration snapshots, early audio frames, RMS/spectral summaries, and full deterministic hashes where supported.
- Run these checks whenever the DSP, NMM schema, renderer contract, or production preset data changes.

**Completion criteria:** CI proves that NMM analyzes the same deterministic signal the shipping engine produces for every fixture.

## Stage 11 — Recalibrate model baselines and golden snapshots

**Purpose:** Update model expectations only after input audio is known to be correct.

**Work:**

- Re-run the full NMM suite against the pinned, production-equivalent renderer.
- Investigate the two current disturbance golden failures and regenerate them only if the new values are explained by intentional DSP changes.
- Re-evaluate all canonical production presets.
- Compare old and new scores, attributing changes to renderer parity, preset corrections, or model changes.
- Store new reports with complete model, DSP, renderer, and preset signatures.
- Review score thresholds rather than mechanically preserving rankings from the incompatible renderer.

**Completion criteria:** The NMM suite passes, every golden update has a documented reason, and all production evaluations can be reproduced from their recorded signatures.

## Stage 12 — Roll out with drift protection

**Purpose:** Keep the integration correct after the initial repair.

**Work:**

- Add CI gates for schema round-trips, preset parity, DSP audio parity, optimizer warm-start, and golden-model tests.
- Make renderer-contract or DSP-version changes explicit review events.
- Require a preset revision bump for any audible parameter change.
- Keep legacy reports readable but mark reports lacking a DSP/renderer signature as non-comparable.
- Document the exact command used to validate all shipping presets before a release.

**Completion criteria:** A DSP or preset change cannot silently reuse old NMM scores, and a release has one repeatable compatibility-validation command.

## Required final validation

Before considering the repair complete, verify all of the following:

- DSP test suite passes.
- NMM test suite passes without unexplained golden changes.
- Every canonical preset validates, evaluates, and optimizer-warm-starts without panic.
- NMM and production renderers pass the audio-parity fixtures.
- DEBUG JSON loading is configuration- and audio-equivalent.
- Reports contain DSP, renderer-contract, model, seed, and preset revisions.
- Flow and all other migrated presets have matching data, comments, and measured output.
