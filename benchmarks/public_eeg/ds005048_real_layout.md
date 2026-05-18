# ds005048 Real Layout Inspection (Stage 8d-A-Real)

Status: inspected from local real dataset at `ds005048-download/`

Date: 2026-05-18

## Dataset Version Inspected

- Dataset id: `ds005048`
- Dataset title: `40Hz Auditory Entrainment`
- Source tree inspected: `ds005048-download/`
- Release metadata source: `ds005048-download/dataset_description.json`
- Release/version recorded in dataset metadata: `v1.0.1`

## Subject/Layout Pattern

- Subject folders: `sub-01` through `sub-35`
- Per subject EEG folder: `sub-XX/eeg/`
- Per subject task stem:
  - `sub-XX_task-40HzAuditoryEntrainment`

## Raw Source Files Present Per Subject

1. `sub-XX/eeg/sub-XX_task-40HzAuditoryEntrainment_eeg.set`
2. `sub-XX/eeg/sub-XX_task-40HzAuditoryEntrainment_eeg.fdt`
3. `sub-XX/eeg/sub-XX_task-40HzAuditoryEntrainment_eeg.json`
4. `sub-XX/eeg/sub-XX_task-40HzAuditoryEntrainment_events.tsv`
5. `sub-XX/eeg/sub-XX_task-40HzAuditoryEntrainment_channels.tsv`

Root-level task metadata:

- `task-40HzAuditoryEntrainment_events.json`

## ASSR Converter Inputs (Stage 8d-A-Real)

The real converter consumes these source files for each subject:

Semantic inputs read by converter:

1. `*_eeg.fdt` for continuous EEG numeric payload
2. `*_eeg.json` for sampling frequency (`SamplingFrequency`)
3. `*_events.tsv` for event intervals
4. `*_channels.tsv` for deterministic channel naming/order
5. `task-40HzAuditoryEntrainment_events.json` for task-level event coding audit trail

Lineage-only input (provenance-required, not semantically parsed in Stage 8d-A-Real):

1. `*_eeg.set`

## Source of Truth

Sampling rate source of truth:

- `sub-XX/eeg/sub-XX_task-40HzAuditoryEntrainment_eeg.json`
- field: `SamplingFrequency`
- observed value in inspected dataset: `250`

Channel metadata source of truth:

- `sub-XX/eeg/sub-XX_task-40HzAuditoryEntrainment_channels.tsv`
- field: `name`

Event semantics source of truth:

- per-subject `*_events.tsv`
- fields include: `onset`, `duration`, `sample`, `value`, `trial_type`
- dataset-level `task-40HzAuditoryEntrainment_events.json` indicates:
  - `value=1` -> Rest
  - `value=2` -> Stimulus

## Event Mapping Rule Used

- Benchmark rows are emitted only for rows where `trial_type == Stimulus`.
- `modulation_rate_hz` is set to `40.0` for emitted rows because the dataset/task itself is explicitly a 40 Hz auditory entrainment protocol (`TaskName=40HzAuditoryEntrainment`, documented event coding), not as a generic ASSR assumption.

## Control Policy Decision (A2)

- Chosen policy: **A2**
- `ds005048` Stage 8d-A-Real runs do not emit an explicit control group for the ASSR benchmark rows.
- Therefore `observed_target_vs_control_strength_delta` is reported as unavailable/not-applicable for real converted runs where control rows are absent.
- Silent fallback to `0.0` control strength is disallowed.

## Channel Policy Chosen (Stage 8d-A-Real)

- Deterministic single-channel extraction from the first channel listed in `*_channels.tsv`.
- This keeps conversion deterministic and auditable for this stage.
- Full multi-channel policy/optimization is deferred.

## Chosen Implementation Path

- Chosen path: **real converter** to the existing normalized intermediate format.
- Why:
  1. Existing intermediate adapter/provenance flow is already in place.
  2. Real dataset payload is EEGLAB (`.set` + `.fdt`), not CSV.
  3. Reusing intermediate benchmark shape minimizes risk and avoids parallel adapter logic.

## Remaining Limitation (Intentional Stage Boundary)

- Stage 8d-B prediction/comparison bridge remains unavailable by design.
- This stage only establishes truthful real-source ingestion and provenance.
- `.set` is currently lineage-only (B2), not semantically parsed for header/dimension validation in this stage.
