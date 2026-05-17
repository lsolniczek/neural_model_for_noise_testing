# Stage 8d: Source-Verified ASSR Validation Path

## Objective

Turn the Stage 8c ASSR scaffold into one real external-validation path for `ds005048`.

Stage 8d is complete only when the repo can:

1. ingest `ds005048` from a source-backed path,
2. produce a `source_verified` ASSR benchmark result,
3. compute NMM prediction-vs-observation ASSR metrics,
4. and report those results without changing runtime scoring defaults.

This is intentionally narrower than "finish all public EEG work." It validates one component path end to end before expanding to more datasets or benchmark families.

## Why this stage comes next

Stage 8c now has:

- fixture plumbing,
- an intermediate ASSR adapter,
- observation-side metrics,
- evidence-promotion gating,
- and honest provenance states.

What it still lacks is the first benchmark result that can legitimately move beyond scaffold status:

- no source-verified lineage,
- no raw/public-source ingestion path,
- no NMM-vs-observed comparison.

Without those three pieces, the system can inspect EEG observations but cannot yet say whether the NMM predicts the ASSR behavior it claims to model.

## Scientific boundary

Stage 8d may support only this claim:

> The NMM's candidate ASSR-related outputs can be compared against observed ASSR metrics from a source-verified public EEG dataset.

Stage 8d must not claim:

- final preset efficacy,
- ADHD treatment benefit,
- shielding/masking efficacy,
- individualized benefit,
- sleep benefit,
- or global validation of all `candidate_v2` behavior.

Those require later calibration data and Stage 9 promotion policy.

## Exact scope

### In scope

1. `ds005048` only.
2. ASSR benchmark only.
3. One trusted public-data path:
   - either direct raw/public-source ingestion,
   - or a source-verified conversion path.
4. A deterministic offline bridge from benchmark rows to NMM prediction outputs needed for ASSR comparison.
5. New prediction-vs-observation metrics and reports.
6. Evidence-map integration so a real source-verified run may become `partially_supported`.

### Out of scope

1. Adding new datasets.
2. Implementing aperiodic or auditory-attention real-data paths.
3. Re-tuning model priors.
4. Changing runtime preset scoring.
5. Promoting `candidate_v2` to default.
6. Building Stage 9.
7. Claiming external validation if the data path remains fixture-only or intermediate-only.

## Design principle

Do not mix these three concerns:

1. **Observation extraction**
   - what was measured in EEG
2. **Model prediction extraction**
   - what the NMM predicts for the same stimulus condition
3. **Evidence promotion**
   - whether the run is trustworthy enough to count as public-data evidence

Each needs its own explicit artifact and tests.

## Work package A: lock the data path

### Goal

Create exactly one source-backed `ds005048` ingestion path that can truthfully emit:

```text
input_kind = raw_public_source
provenance_status = source_verified
```

or, if conversion is chosen:

```text
input_kind = preprocessed_intermediate
provenance_status = source_verified
conversion_status = implemented
```

### Required decisions

Pick one path and document it before implementation:

#### Option 1: direct raw adapter

- Read the public/raw dataset structure directly.
- Record consumed raw paths.
- Hash-check consumed raw files.
- Emit normalized ASSR benchmark rows.
- Registry path uses:

```text
raw_adapter_status = implemented
```

#### Option 2: source-verified converter

- Read raw public files.
- Convert to the current normalized intermediate layout.
- Record:
  - source paths and hashes,
  - intermediate paths and hashes,
  - converter version,
  - conversion timestamp.
- Recompute source and intermediate hashes during validation.
- Registry path uses:

```text
conversion_status = implemented
```

### Recommendation

Prefer **Option 2** unless direct raw parsing is clearly simpler. The repo already has an intermediate adapter and provenance model, so a real converter is likely the shortest path to a source-verified result without duplicating extraction logic.

### Required output artifacts

- a real converter or raw adapter implementation,
- an updated registry entry for `ds005048`,
- one raw-fixture test subset representing the chosen source path,
- docs showing the exact source-to-benchmark flow.

## Work package B: source verification contract

### Goal

Make `source_verified` mean something stronger than `intermediate_verified`.

### Minimum contract

For any result marked `source_verified`, the system must have checked:

1. expected dataset identity,
2. source dataset version / release identifier,
3. every raw/public-source file consumed by the pipeline,
4. valid SHA-256 hashes for all consumed raw/public-source files,
5. recomputed raw/public-source file hashes match the manifest,
6. if conversion is used, every emitted intermediate file is listed and hash-checked too.

### Non-negotiable rule

`source_verified` must never be emitted from:

- fixture rows,
- declared metadata only,
- intermediate files without verified raw/public-source lineage.

## Work package C: define the NMM prediction bridge

### Goal

Create one deterministic offline prediction path for ASSR comparison.

### Required input

Prediction code should consume normalized benchmark condition fields, not raw EEG:

- `modulation_rate_hz`
- condition/task labels if needed
- any explicitly required acoustic descriptors available from the benchmark contract

### Required predicted outputs

Stage 8d should compare only quantities the current model can defensibly expose.

Recommended prediction outputs:

```text
predicted_dominant_modulation_hz
predicted_gamma_assr_response_strength
predicted_target_condition_rank_or_delta
```

Do not invent a predicted PLV unless the model really computes PLV.

### Important modeling rule

If the NMM cannot yet predict a given observed quantity on the same scale, do not force a fake direct-error metric. Use:

- rank agreement,
- sign agreement,
- condition contrast agreement,
- or clearly named surrogate metrics.

The report must distinguish:

- same-scale metrics,
- surrogate agreement metrics,
- not-yet-comparable outputs.

## Work package D: ASSR comparison metrics

### Observation-side metrics already present

- `observed_target_rate_recovery_accuracy`
- `observed_target_band_strength`
- `observed_target_vs_control_strength_delta`
- `observed_dominant_modulation_hz_error`

### Add prediction-side and comparison metrics

Minimum useful set:

1. `predicted_target_rate_recovery_accuracy`
2. `predicted_target_vs_control_strength_delta`
3. `prediction_observation_target_rate_agreement`
4. `prediction_observation_condition_rank_agreement`
5. `prediction_observation_strength_delta_sign_agreement`

If a same-scale error is defensible for a field, add it. If not, label it as unavailable rather than fabricating precision.

### Failure cases

Emit explicit failure-case rows for at least:

- observed target not near 40 Hz,
- predicted target not near 40 Hz,
- observed and predicted condition contrast disagree,
- comparison metric unavailable because model output is not commensurate.

## Work package E: reports and promotion

### Required files

Update the ASSR runner to emit distinct artifacts:

- `assr_observed_metrics.csv`
- `assr_prediction_metrics.csv`
- `assr_comparison_metrics.csv`
- `assr_failure_cases.csv`
- `assr_benchmark_result.json`
- `assr_benchmark_report.md`

### Evidence map rule

Only a result that satisfies the existing trusted-path rules may become `partially_supported`.

Stage 8d should still not produce `validated_by_public_data` unless the repo explicitly defines a stronger acceptance threshold later.

### Report language

The report must say exactly what is supported:

- observation extraction succeeded,
- source lineage was verified,
- the NMM did or did not agree with observed ASSR diagnostics,
- final preset efficacy remains unsupported.

## Work package F: tests

### Required test categories

#### Data path

1. source fixture is accepted by the chosen raw/conversion path,
2. source hash mismatch is rejected,
3. missing consumed raw file is rejected,
4. source-verified path is impossible from fixture-only data.

#### Prediction bridge

5. deterministic model inputs produce deterministic prediction rows,
6. predicted columns are present and numerically valid,
7. no PLV-like field is emitted unless actually implemented.

#### Comparison metrics

8. correct synthetic agreement case passes,
9. contrast disagreement case is detected,
10. unavailable comparison metrics are labeled unavailable rather than filled with zeros.

#### Evidence promotion

11. source-verified real-public-data result may become `partially_supported`,
12. intermediate-verified result still does not promote,
13. fixture result still does not promote.

#### Regression

14. existing Stage 8c tests still pass,
15. runtime scoring outputs remain unchanged.

## Suggested pseudocode

### Source-verified converter

```python
def convert_ds005048_source_to_intermediate(source_root, out_root):
    source_files = discover_required_source_files(source_root)
    source_hashes = hash_files(source_files)

    normalized_files = convert_to_normalized_assr_layout(source_files, out_root)
    intermediate_hashes = hash_files(normalized_files)

    manifest = {
        "dataset_id": "ds005048",
        "source_dataset_id": "ds005048",
        "source_dataset_version": read_source_version(source_root),
        "source_paths": relpaths(source_files),
        "source_file_hashes": source_hashes,
        "intermediate_paths": relpaths(normalized_files),
        "intermediate_file_hashes": intermediate_hashes,
        "conversion_tool_version": TOOL_VERSION,
        "conversion_timestamp": now_utc(),
        "subjects": discovered_subjects,
    }
    write_manifest(out_root, manifest)
```

### Provenance

```python
def compute_provenance_status(manifest, consumed_source_files, consumed_intermediate_files):
    if not verified_intermediate_files(manifest, consumed_intermediate_files):
        return "declared_only"

    if verified_source_files(manifest, consumed_source_files):
        return "source_verified"

    return "intermediate_verified"
```

### Comparison

```python
def compare_assr(observed_rows, predicted_rows):
    joined = join_on_trial_or_condition(observed_rows, predicted_rows)

    return {
        "predicted_target_rate_recovery_accuracy": ...,
        "predicted_target_vs_control_strength_delta": ...,
        "prediction_observation_target_rate_agreement": ...,
        "prediction_observation_condition_rank_agreement": ...,
        "prediction_observation_strength_delta_sign_agreement": ...,
    }
```

## Completion criteria

Stage 8d is done only when all are true:

1. `ds005048` has one implemented source-backed path.
2. A trusted run can emit `source_verified`.
3. The ASSR benchmark emits observed, predicted, and comparison artifacts separately.
4. The evidence map can promote only the source-verified path to `partially_supported`.
5. Reports make no claim stronger than the metrics support.
6. Existing Stage 8c behavior remains intact.
7. New Stage 8d tests pass.
8. The final review can answer these three questions unambiguously:
   - What exact source path was verified?
   - What exact ASSR behavior did the NMM predict?
   - Where did prediction agree or disagree with observed EEG?

## Recommended implementation order

1. Freeze the chosen source path and write its fixture.
2. Implement source verification.
3. Implement raw ingestion or converter.
4. Expose deterministic NMM ASSR prediction rows.
5. Add comparison metrics.
6. Update reports and evidence map behavior.
7. Add the complete test matrix.
8. Only then update docs and registry readiness flags.

## What not to do early

Do not:

1. add more datasets before one source-verified path works,
2. add aperiodic or attention benchmarks before ASSR comparison is end to end,
3. tune model parameters to fit the first dataset,
4. rename surrogate agreement metrics as direct physiological errors,
5. set `validated_by_public_data` merely because the pipeline runs,
6. weaken provenance rules to get promotion sooner.
