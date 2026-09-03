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

**Current status (reviewed 2026-08-07; repair required):** [`baselines/compatibility/stage1_legacy_pre_parity/manifest.json`](baselines/compatibility/stage1_legacy_pre_parity/manifest.json) remains the schema-v2 reference and its frozen 60-preset corpus is byte-identical to the current live corpus. A schema-v3 implementation exists in the working tree and its 14 focused unit tests pass, but no schema-v3 candidate has been captured or promoted. Review found five remaining gate defects: smoke compatibility is not enforced, Cargo metadata discovery is not rooted at `noise_generator_core`, offline verification accepts incomplete source inventories, registry verification does not prove complete one-to-one coverage, and the recorded binary hash cannot be checked because the binary is not retained.

### Stage 1 repair plan — schema v3

**Objective:** Finish schema v3 without changing runtime behavior: make its provenance independently verifiable, enforce 60/60 schema-v2 smoke compatibility inside the capture workflow, then capture and promote one stable 120-report baseline.

**Scope guardrails:**

- Modify only `tools/compatibility/stage1_baseline.py`, `tests/compatibility/test_stage1_baseline.py`, `fix_plan.md`, and the tracked Stage 1 baseline directory.
- Treat the current `presets/**` and `reports/**` working-tree changes as user-owned inputs. Read them for preflight and capture, but do not edit, delete, stage, or normalize them.
- Preserve the schema-v2 baseline in Git history. Build schema v3 in a temporary directory outside all three repositories, materialize it as a new candidate sibling only after the capture-stability check passes, verify it completely, and replace the canonical directory only after every acceptance test passes.
- Do not change NMM model, DSP runtime, iOS application source, or the 60-preset corpus during this repair. Preflight must require the live preset path set and hashes to equal schema v2 before any long evaluation begins.
- Do not commit automatically. If a commit is authorized, stage only the four Stage 1 path groups listed above and audit the staged path list before committing.

### Required implementation order

Each step begins by adding the listed failing tests. Do not start the next step until those tests and all earlier Stage 1 tests pass.

#### Step 1 — Make the schema-v2 reference an enforced input

1. Add required `capture --smoke-reference <schema-v2-baseline>` input. Reject a reference whose manifest is not schema 2, whose `baseline_id` differs, or whose artifact hashes, inventory, or 60 smoke reports fail strict validation.
2. Compare the live preset corpus with the reference before building: require exactly the same 60 relative paths and SHA-256 values. Abort with all differing paths if anything drifted.
3. Match reports by `preset_path`, never by enumeration order or basename. Preserve nested paths.
4. After generating schema-v3 smoke reports, compare their raw bytes with the corresponding schema-v2 reports. A single mismatch must abort capture before candidate materialization.
5. Write `compatibility/schema_v2_smoke_comparison.json` containing the reference manifest hash, all 60 preset paths, old/new report hashes, and `matched: 60`. Include it in artifact hashes.
6. Add a `compare-smoke --reference <v2> --candidate <v3>` command that repeats the same validation independently and returns nonzero on corpus or report drift.

**Tests:** reject a corrupt v2 artifact, missing/duplicate preset, preset hash drift, report-path escape, missing report, and one-byte report mismatch; accept 60 reports with nested paths and prove matching is order-independent.

**Success criterion:** capture cannot create a candidate unless the input corpus matches schema v2 60/60 and every `compat_smoke_v1` report is byte-identical 60/60.

#### Step 2 — Resolve the actual DSP dependency graph

1. Change `dependency_paths_from_metadata()` to use `resolve.nodes`, starting at the NMM package node and following its resolved `noise_generator_core` edge to `noise_generator_dsp/crates/core`.
2. Traverse dependency package IDs from that node. Include only path packages whose manifests are under the DSP repository; allow registry/git dependencies without fingerprinting them.
3. Reject any traversed path package whose `source` is null and whose manifest lies outside the declared NMM and DSP roots. Do not include unrelated DSP workspace members merely because they appear in `packages`.
4. Keep the manifest walker only as a documented fallback when offline Cargo metadata cannot resolve because a registry package is unavailable. Anchor it at `crates/core`, parse all applicable dependency tables, reject paths outside the DSP root, and require its result to contain `core`, `engine_shared`, `signal_core`, and `spatial_core`.
5. Store the resolved DSP package roots in `source_inventory.json` and require the same sorted roots during drift calculation.

**Tests:** use synthetic metadata containing the four required crates, one unrelated DSP workspace crate, one registry crate, and one external path crate. Assert that the unrelated and registry crates are excluded, the external path crate fails, shuffled metadata gives the same result, and fallback discovery gives the same four-crate closure.

**Success criterion:** the recorded closure is exactly the transitive local runtime closure rooted at `noise_generator_core`, and mutations in any of its four required crates change `dsp_runtime.sha256`.

#### Step 3 — Make source provenance verifiable offline

1. Give `source_inventory.json` an explicit contract with exactly `nmm_runtime`, `dsp_runtime`, and `ios_integration`. Each group must contain a sorted `files` map; DSP must also contain its sorted `package_roots`.
2. Copy every inventoried file into `inputs/source-evidence/<group>/<original-relative-path>` during capture, including NMM and DSP evidence rather than only iOS evidence.
3. Add `validate_source_inventory()`. Require exact group names, nonempty maps, safe normalized relative paths, valid SHA-256 values, no excluded/generated path components, and no duplicate/case-colliding paths.
4. Require NMM `Cargo.toml`, `Cargo.lock`, all expected NMM runtime inputs, DSP root `Cargo.toml`/`Cargo.lock`, and a `Cargo.toml` for every recorded DSP package root. Require the four minimum DSP roots.
5. Enumerate each frozen evidence directory and require its path set to equal the inventory exactly; then hash every file and verify group `file_count` and tree hash against the manifest. Extra, missing, or altered evidence must fail.
6. Reuse this validator from capture, offline verify, and replay drift reporting so the three paths cannot interpret provenance differently.

**Tests:** independently remove a group, required crate root, manifest, inventory entry, or evidence file; add an unreferenced evidence file; mutate a hash; use `..`, absolute, excluded, duplicate, and case-colliding paths. Every case must raise `BaselineError`, not a raw `KeyError` or `TypeError`.

**Success criterion:** verification can prove the complete recorded source snapshot from the baseline directory alone and rejects incomplete or structurally malformed provenance.

#### Step 4 — Prove complete Swift registry coverage

1. Keep `swift_registry()` strict during capture: every `ActivePreset` case must appear once in `isActive` and once in `presetProvider`, every referenced provider type must have one provider file, and every provider file must produce one registry entry.
2. Strengthen offline registry verification to require unique provider names, provider paths, and registered case names. The set of non-null `active_preset_case` values must equal `active_preset_cases` exactly.
3. Registered entries may only be `active` or `inactive`; `provider_only` entries must have a null case. Require Blue to be provider-only and SSN to be registered/inactive.
4. Require `relationship_status == "candidate_only"` for every entry. Candidate paths must refer to the frozen 60-preset inventory, be unique per provider, and have valid hashes.
5. Derive provider Swift paths from the frozen iOS source inventory and require exact equality with registry provider paths. This detects an omitted or fabricated provider entry without access to the sibling iOS repository.

**Tests:** reject duplicate provider/case/path values, missing or extra cases, omitted provider files, invalid status/case combinations, a registered Blue, active SSN, non-candidate relationships, and candidates outside the frozen corpus.

**Success criterion:** the frozen registry forms a complete one-to-one accounting of all captured `ActivePreset` cases and Swift provider files.

#### Step 5 — Retain and verify the evaluated binary

1. After the successful build, copy the exact executable used for all evaluations to `artifacts/neural_preset_optimizer` before starting evaluations. Execute the retained copy, not the mutable `target/debug` path.
2. Replace the free-standing `binary_sha256` with a binary artifact object containing safe relative path, SHA-256, byte size, target triple, and executable filename. Keep toolchain and OS metadata to make its platform scope explicit.
3. Offline verification must require the artifact, verify its size and SHA-256, and require the same path/hash in the global artifact inventory. It must not execute the retained binary.
4. Replay rebuilds current code and reports whether its binary hash matches the captured artifact; report drift is diagnostic, while report byte mismatches remain fatal.

**Tests:** reject a missing, truncated, altered, escaped, or wrongly sized binary and a manifest/global-artifact hash disagreement. Test that evaluations invoke the staged binary path.

**Success criterion:** the manifest hash is tied to the exact retained executable that generated all 120 reports and is checkable without rebuilding or network access.

#### Step 6 — Preserve atomic capture without unrelated-worktree false failures

1. Keep staging under the system temporary directory and refuse an existing output path.
2. Record each repository HEAD plus status scoped to its inventoried runtime/evidence paths; record the 60 live preset paths separately. Do not let `.DS_Store`, Xcode user-interface state, `target`, or unrelated planning documents invalidate a long capture.
3. Recompute HEAD, scoped status, source inventories, preset hashes, evidence hashes, capture-tool hash, and retained-binary hash after the final evaluation.
4. Abort and remove staging on any relevant pre/post difference. Diagnostics must name every changed repository/path, capped only after a documented limit.
5. Write the comparison proof and manifest only after stability succeeds; run offline verification while still in temporary staging; materialize the candidate with one rename.

**Tests:** simulate changes to HEAD, relevant dirty status, new/deleted source files, presets, iOS evidence, capture tool, and binary; each must fail and leave no candidate. Simulate `.DS_Store` and Xcode UI-state churn and confirm capture state remains stable.

**Success criterion:** relevant source drift always aborts atomically, while unrelated repository metadata cannot waste or invalidate the capture.

#### Step 7 — Close verifier and replay gaps

1. Use a strict JSON loader that rejects duplicate object keys, then validate manifest and JSON node types before calling `.get()` or indexing. Unknown or missing keys that affect the contract must fail with `BaselineError`.
2. Require exactly 60 unique presets, exactly both profile IDs per preset, exactly 120 unique referenced reports, and no unreferenced files below `evaluations/`.
3. Require all test logs, source evidence, comparison proof, registry, retained binary, README, preset snapshots, and reports in the global artifact map. Reject any unreferenced file outside an explicitly documented allowlist.
4. Keep replay defaulted to both profiles. Validate each fresh report before hashing and make `--with-tests` enforce the stored NMM/DSP result contracts.
5. Add integration-style temporary fixtures covering a valid minimal schema-v3 tree and one mutation for every required artifact class.

**Success criterion:** offline verification fails cleanly for corruption, omission, path escape, duplicate identity, incomplete provenance, or an extra report; full replay succeeds only on 120/120 valid byte matches and the stored test contracts.

### Capture and promotion runbook

Run these commands in order from the NMM repository root. Stop at the first nonzero exit; do not promote a partially accepted candidate.

1. `PYTHONPYCACHEPREFIX=/tmp/nmm_stage1_pycache python3 -m unittest tests.compatibility.test_stage1_baseline`
2. `PYTHONPYCACHEPREFIX=/tmp/nmm_stage1_pycache python3 -m py_compile tools/compatibility/stage1_baseline.py tests/compatibility/test_stage1_baseline.py`
3. `python3 tools/compatibility/stage1_baseline.py capture --nmm-repo . --dsp-repo ../noise_generator_dsp --ios-repo ../noise_generator_ios_app --smoke-reference baselines/compatibility/stage1_legacy_pre_parity --output baselines/compatibility/stage1_legacy_pre_parity_schema_v3_candidate`
4. `python3 tools/compatibility/stage1_baseline.py verify --baseline baselines/compatibility/stage1_legacy_pre_parity_schema_v3_candidate`
5. `python3 tools/compatibility/stage1_baseline.py compare-smoke --reference baselines/compatibility/stage1_legacy_pre_parity --candidate baselines/compatibility/stage1_legacy_pre_parity_schema_v3_candidate`
6. `python3 tools/compatibility/stage1_baseline.py replay --baseline baselines/compatibility/stage1_legacy_pre_parity_schema_v3_candidate --nmm-repo . --dsp-repo ../noise_generator_dsp --ios-repo ../noise_generator_ios_app --profile all --with-tests`
7. Repeat offline verification and compare candidate artifact hashes before/after replay; they must be unchanged.
8. Audit `git status --short`, hashes of live `presets/**` and `reports/**`, and the candidate inventory. Only then replace the canonical baseline using a recoverable schema-v2 backup. Verify the canonical path once more before removing any temporary backup.

### Stage 1 schema-v3 acceptance criteria

Stage 1 is repaired only when all of these statements are true:

- Exactly 60 frozen inputs match schema v2, 60 smoke reports match schema v2 byte-for-byte, and 60 long regression reports validate.
- The long profile analyzes at least 10 seconds after warm-up and locks all intended flags in the command and report signature.
- DSP provenance is the rooted local dependency closure and includes `core`, `engine_shared`, `signal_core`, and `spatial_core`, with no unrelated workspace crate.
- Frozen NMM, DSP, and iOS evidence exactly matches the validated source inventory and combined manifest fingerprints.
- The Swift registry accounts for every active, inactive, and provider-only entry exactly once; Blue is provider-only, SSN is inactive, and every JSON relationship remains `candidate_only`.
- The exact executable used for evaluation is retained and its hash and size verify offline.
- Stable capture, strict verification, independent smoke comparison, and full rebuilt replay all pass; replay matches 120/120 and enforces the documented NMM/DSP test contracts.
- Negative tests cover every reviewed failure mode and produce controlled `BaselineError` failures.
- No NMM/DSP/iOS runtime source, live preset, user report, or unrelated dirty file is modified, staged, or deleted.

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
