# Stage 8d-A-Real: Inspect and Ingest Real `ds005048`

## Objective

Replace the current synthetic source-contract scaffold with one truthful ingestion path for the **actual public `ds005048` dataset**.

Stage 8d-A-Real is complete only when the repo can answer, from real dataset files rather than assumptions:

1. what the real `ds005048` source layout is,
2. which real source files are consumed,
3. how those files are transformed into normalized ASSR benchmark rows,
4. and whether that real path is ready to replace the current synthetic scaffold.

## Why this is the next stage

The current repo state is intentionally conservative:

- provenance machinery works on a synthetic source-contract scaffold,
- prediction/comparison outputs remain unavailable,
- real public-source ingestion is still pending,
- registry readiness remains disabled.

The next useful step is therefore **not** prediction work and **not** more synthetic fixtures. It is to inspect and support the real public dataset structure.

## Public dataset facts known before implementation

Current public metadata indicates that:

- `ds005048` is the OpenNeuro dataset **“40Hz Auditory Entrainment”**,
- OpenNeuro lists version `1.0.1`,
- the EEGDash index lists 35 subjects and 250 Hz recordings.

These facts are enough to justify inspection of the dataset, but **not enough** to design the converter from memory. The on-disk source contract must be derived from the real files.

## Stage boundary

### In scope

1. Inspect the actual public dataset layout.
2. Record the real source files and metadata required for ASSR extraction.
3. Decide whether the correct implementation is:
   - a direct raw adapter, or
   - a real converter into the existing normalized intermediate format.
4. Implement that one real path only after inspection.
5. Add a small real-layout fixture derived from the inspected structure.
6. Update provenance so `source_verified` is earned from real source files.
7. Keep prediction/comparison outputs unavailable until Stage 8d-B.

### Out of scope

1. No NMM prediction bridge.
2. No aperiodic benchmark.
3. No auditory-attention benchmark.
4. No runtime scoring changes.
5. No promotion policy changes.
6. No claims of preset efficacy.

## Required phase order

### Phase 1: inspect before coding

Before changing converter logic, document:

1. dataset version inspected,
2. subject directory pattern,
3. raw EEG file type(s),
4. event/stimulus metadata file type(s),
5. sampling-rate source of truth,
6. channel source of truth,
7. exact files required for the ASSR benchmark,
8. any ambiguity or blocker.

Write this into a repo document before implementation, for example:

```text
benchmarks/public_eeg/ds005048_real_layout.md
```

### Phase 2: choose one real path

After inspection, choose exactly one:

#### Option A: direct raw adapter

Use this if the real source files are straightforward to parse directly.

#### Option B: real converter

Use this if preserving the current normalized benchmark row flow is simpler.

Recommendation:

- prefer the path that minimizes duplicate signal-processing logic,
- but do not create a converter until the real source file contract is written down.

### Phase 3: implement real provenance

For any real path, `source_verified` must require:

1. actual real source files from the inspected dataset layout,
2. complete source-input coverage for each emitted intermediate if conversion is used,
3. valid source hashes,
4. source-root/version metadata,
5. valid ASSR sampling constraints.

Synthetic scaffold files must never be confused with real public-source files.

## Required deliverables

### Documentation

1. `benchmarks/public_eeg/ds005048_real_layout.md`
   - exact real source layout
   - inspected version
   - files used
   - implementation decision
   - blockers if any

2. README update
   - distinguish:
     - synthetic scaffold
     - real source path
   - keep claims narrow

### Code

Depending on inspection outcome:

- either a direct real adapter,
- or a real converter plus source-aware verifier updates.

### Fixtures

Add a minimal fixture that mirrors the **real** inspected file layout.

Do not use invented names such as `*_source_events.csv` unless those names actually exist in the public dataset.

### Tests

Add tests for:

1. inspected real-layout fixture parses successfully,
2. required real source files are discovered,
3. missing required real source files fail clearly,
4. source hash mismatch fails,
5. sample-rate metadata is taken from the correct real source file,
6. sampling validity for 40 Hz is enforced,
7. synthetic scaffold still remains non-evidence-promotable unless intentionally retired,
8. prediction/comparison outputs remain unavailable.

## Registry policy

Keep:

```text
conversion_status = not_started
benchmark_ready = false
```

until the real public-source path is actually implemented and tested.

If inspection reveals that `ds005048` is not practical for the intended benchmark, do not force the dataset. Document the blocker and recommend a replacement dataset instead.

## Anti-loop rules

1. Do not write converter code before the real layout document exists.
2. Do not infer file names from BIDS conventions alone; inspect the actual dataset.
3. Do not call synthetic fixture support “real public ingestion.”
4. Do not touch prediction/comparison code except to preserve the current unavailable state.
5. Do not enable readiness flags based only on a mock fixture.
6. If inspection disproves an assumption, update the plan instead of coding around the assumption.

## Completion criteria

Stage 8d-A-Real is complete only when the final review can answer:

1. Which exact real `ds005048` files are consumed?
2. Where does the sampling rate come from?
3. How are events/conditions mapped into benchmark rows?
4. Which path was chosen: direct raw adapter or converter?
5. Why is the path truthful for the real dataset rather than only for a mock?
6. Are readiness flags still truthful?

If any answer depends on assumption rather than inspected files, the stage is not complete.

## Recommended implementation order

1. Inspect dataset files.
2. Write the real-layout document.
3. Choose direct adapter vs converter.
4. Add a real-layout fixture.
5. Implement parser/converter.
6. Add provenance and sampling tests.
7. Update docs.
8. Update registry flags last, only if real ingestion is complete.
