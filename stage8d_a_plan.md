# Stage 8d-A: Real Source Ingestion and Provenance

## Objective

Implement one real source-verification path for `ds005048` without touching the unfinished NMM prediction bridge.

Stage 8d-A is complete only when the repo can truthfully produce:

```text
dataset_id = ds005048
input_kind = preprocessed_intermediate
provenance_status = source_verified
```

from a conversion flow that verifies a **distinct source tree** and a **distinct emitted intermediate tree**.

## Why this stage is split out

The previous Stage 8d attempt mixed two independent problems:

1. source provenance,
2. model prediction.

That encouraged false completion. Stage 8d-A handles only provenance.

Stage 8d-B will later handle:

- real NMM-derived prediction rows,
- comparison metrics,
- disagreement cases.

Until Stage 8d-B is implemented, ASSR prediction/comparison outputs must remain explicitly unavailable.

## Current starting point

The repo is currently honest but incomplete:

- `Ds005048PreprocessedAdapter` verifies intermediate files only.
- `convert_ds005048_to_nmm_intermediate.py` is a scaffold that copies normalized files.
- runtime emits `intermediate_verified`.
- registry keeps:
  - `conversion_status = not_started`
  - `benchmark_ready = false`
- ASSR prediction/comparison outputs are intentionally unavailable.

## Scope

### In scope

1. Define the real ds005048 source contract.
2. Add a source fixture that is structurally distinct from the normalized intermediate fixture.
3. Implement one conversion path from source fixture layout to normalized intermediate layout.
4. Persist enough source-root identity in the manifest to re-verify source files after conversion.
5. Make `source_verified` depend on recomputing hashes against the real source tree, not the converted output tree.
6. Add tests that prove copied intermediates alone cannot earn `source_verified`.

### Out of scope

1. Implementing NMM predictions.
2. Changing prediction/comparison unavailable status.
3. Adding datasets beyond `ds005048`.
4. Changing runtime preset scoring.
5. Claiming final validation or final preset efficacy.
6. Adding aperiodic or attention benchmarks.

## Required design decisions

### 1. Source and intermediate must be distinguishable

The source fixture must not be the same shape as the current normalized benchmark fixture.

Examples of acceptable distinctions:

- source files live under a dedicated source/raw layout,
- source file names/extensions differ from normalized `*_events.tsv` and `*_timeseries.csv`,
- conversion performs a documented transformation into the normalized intermediate layout.

The exact source layout should mirror the real dataset as closely as practical for the chosen public-data path.

### 2. The manifest must preserve source-root identity

The converted dataset root alone is not enough to verify source lineage.

The manifest needs an explicit way to locate or identify the source snapshot used for conversion, for example:

```json
{
  "source_root_ref": "/absolute/or/declared/source/path",
  "source_snapshot_id": "...",
  "source_paths": [...],
  "source_file_hashes": {...}
}
```

Use the design that is most robust for the repo, but it must allow the verifier to hash the actual source files again later.

### 3. `source_verified` requires both checks

`source_verified` may be returned only if:

1. all consumed intermediate files are covered and hash-verified, and
2. all consumed source files are covered and hash-verified against the real source tree.

If the source tree is absent, moved, or hash-mismatched, status must not be `source_verified`.

### 4. Registry changes happen last

Do not set:

```text
conversion_status = implemented
benchmark_ready = true
```

until:

- source verification works,
- tests prove copied intermediates alone do not pass,
- docs describe the exact source contract.

## Required implementation behavior

### Converter

The converter must:

1. discover the chosen source-format files,
2. hash those actual source files,
3. transform them into normalized intermediate files,
4. hash the normalized intermediate files,
5. write a manifest that clearly separates:
   - source tree
   - intermediate tree
6. set:

```text
provenance_status_hint = source_verified
```

only if the converter really created a manifest that can later be re-verified against the source tree.

### Adapter / verifier

The verifier must:

1. verify normalized intermediate files as it already does,
2. resolve the source tree using manifest metadata,
3. verify all listed source files against source hashes,
4. return:
   - `source_verified` only when both checks pass,
   - `intermediate_verified` when only intermediate verification passes,
   - `declared_only` when coverage/hash metadata is insufficient.

## Required tests

### Positive path

1. valid source fixture converts successfully
2. converted manifest has distinct source and intermediate path sets
3. valid converted dataset run returns `source_verified`

### Negative provenance path

4. missing source tree prevents `source_verified`
5. source hash mismatch prevents `source_verified`
6. source path coverage gap prevents `source_verified`
7. copied-intermediate-only manifest remains `intermediate_verified`
8. fixture-only run cannot become `source_verified`

### Regression

9. existing Stage 8c tests still pass
10. ASSR prediction/comparison outputs remain unavailable
11. runtime scoring behavior remains unchanged

## Pseudocode

### Manifest shape

```python
manifest = {
    "dataset_id": "ds005048",
    "source_dataset_id": "ds005048",
    "source_dataset_version": source_version,
    "source_root_ref": str(source_root.resolve()),
    "source_snapshot_id": compute_snapshot_id(source_files),
    "source_paths": relpaths_from_source_root(source_files),
    "source_file_hashes": hash_files(source_files),
    "intermediate_paths": relpaths_from_out_root(intermediate_files),
    "intermediate_file_hashes": hash_files(intermediate_files),
    "conversion_tool_version": TOOL_VERSION,
    "conversion_timestamp": now_utc(),
    "subjects": subjects,
    "provenance_status_hint": "source_verified",
}
```

### Verification

```python
def compute_provenance_status(consumed_intermediate_paths):
    if not verify_intermediate_files(consumed_intermediate_paths):
        return "declared_only"

    if not manifest_has_source_root_ref():
        return "intermediate_verified"

    if not verify_source_files_against_source_root():
        return "intermediate_verified"

    return "source_verified"
```

## Completion criteria

Stage 8d-A is complete only when all are true:

1. source and intermediate layouts are genuinely distinct,
2. `source_verified` requires re-checking the actual source tree,
3. copied intermediates alone cannot earn `source_verified`,
4. `ds005048` registry readiness is enabled only after the source contract is real,
5. docs describe the exact verified source path,
6. prediction/comparison outputs remain explicitly unavailable,
7. the final review can answer:
   - which source files were verified,
   - where the verifier found them,
   - how they differ from the intermediate files.

## Recommended implementation order

1. Define the real source fixture layout.
2. Extend the manifest schema/contract for source-root identity.
3. Implement source-aware conversion.
4. Implement source-aware verification.
5. Add the negative provenance tests.
6. Update docs.
7. Update registry flags last.

## Anti-loop rules

1. Do not touch prediction/comparison code except to preserve the existing unavailable status.
2. Do not call copied normalized files "raw source."
3. Do not return `source_verified` from data located only under the converted dataset root.
4. Do not set readiness flags before negative provenance tests pass.
5. If the real source contract cannot be implemented, keep the current honest `intermediate_verified` state and report the blocker instead of weakening semantics.
