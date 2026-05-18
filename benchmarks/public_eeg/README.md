# Stage 8c: Public EEG Benchmark Layer

This benchmark layer is offline-only and does not change runtime NMM scoring behavior.

It exists to test NMM subcomponents against public EEG datasets before any future promotion decision.

## Evidence categories

- `validated_by_public_data`: a benchmarked component has direct support in at least one public dataset benchmark report.
- `partially_supported`: only part of the component behavior is benchmarked, or support is indirect.
- `unsupported_requires_own_study`: no appropriate public benchmark exists for the claim; requires owned study protocol.
- `fixture_smoke_test_only` / `plumbing_verified`: engineering-only fixture execution, not scientific external evidence.

## Dataset registry

- Registry file: `benchmarks/public_eeg/datasets_v1.json`
- Registry schema: `benchmarks/public_eeg/schema/public_eeg_registry_v1.schema.json`

The registry tracks coverage and status without forcing downloads in code.

Registry fields `dataset_status` and `evidence_usable` explicitly prevent placeholder entries from being used as external evidence sources.
Evidence promotion also requires:
- `benchmark_ready == true`
- and trusted path gating:
  - `raw_adapter_status == implemented`, or
  - `conversion_status == implemented` plus benchmark-result `provenance_status == source_verified`.

## Benchmark families

1. ASSR benchmark
   - target: auditory entrainment diagnostics (observed dominant-rate recovery and target-band strength)
   - selected first real dataset: `ds005048` (OpenNeuro 40 Hz auditory entrainment)

2. Aperiodic benchmark
   - target: periodic-vs-aperiodic extraction stability and plausibility
   - example dataset slot: `resting_aperiodic_placeholder`

3. Auditory-attention benchmark
   - target: relationship between auditory/modulation descriptors and attention outcomes under distractors
   - example dataset slot: `auditory_attention_placeholder`

## Scope boundaries

Public benchmarks can strengthen component-level validity (entrainment, modulation features, aperiodic extraction).
They do **not** by themselves prove final preset efficacy or clinical claims.

Sleep claims require separate protocol evidence and are not validated by daytime attention public datasets.

## Commands

```bash
python3 tools/public_eeg_benchmarks/list_datasets.py
python3 tools/public_eeg_benchmarks/run_assr_benchmark.py --dataset ds005048 --output-dir /tmp/nmm_public
python3 tools/public_eeg_benchmarks/run_aperiodic_benchmark.py --dataset resting_aperiodic_placeholder --output-dir /tmp/nmm_public
python3 tools/public_eeg_benchmarks/run_auditory_attention_benchmark.py --dataset auditory_attention_placeholder --output-dir /tmp/nmm_public
python3 tools/public_eeg_benchmarks/build_evidence_map.py --input-dir /tmp/nmm_public --output /tmp/nmm_public/public_eeg_evidence_map.md
```

If a dataset is not downloaded, runners fail clearly with status `dataset_not_downloaded`.

Each runner emits machine-readable benchmark result metadata JSON (`*_benchmark_result.json`) with:
- `run_mode` (`fixture_smoke_test` or `real_public_data`)
- `data_status`
- `metrics_computed`
- `evidence_category`
- `provenance_status` (`fixture`, `declared_only`, `intermediate_verified`, `source_verified`)

Only `run_mode=real_public_data` with usable selected datasets and a trusted provenance path may contribute to public-data evidence in the evidence map.

## What still needs real data

- Actual OpenNeuro/local dataset downloads.
- Adapter-specific parsing from raw files to normalized benchmark rows.
- Cohort and protocol decisions for final acceptance thresholds.

Fixture smoke tests only verify tooling/plumbing and should never be interpreted as validation evidence.
Real-data ASSR run (current path uses converted intermediate, not direct raw-BIDS ingestion):

```bash
python3 tools/public_eeg_benchmarks/run_assr_benchmark.py \
  --dataset ds005048 \
  --dataset-root /path/to/local/ds005048 \
  --output-dir /tmp/nmm_public_assr
```

Expected local dataset shape for the current **intermediate adapter**:
- `/path/to/local/ds005048/nmm_benchmark_manifest.json`
- `sub-*/eeg/*_events.tsv`
- `sub-*/eeg/*_timeseries.csv`

The current adapter is `Ds005048PreprocessedAdapter` and requires `nmm_benchmark_manifest.json` fields that separately cover source lineage and consumed intermediate integrity:
- `source_dataset_id`
- `source_dataset_version`
- `source_paths`
- `source_file_hashes` (non-empty and covering all `source_paths`)
- `intermediate_paths`
- `intermediate_file_hashes` (non-empty and covering every consumed intermediate file)
- `conversion_tool_version`
- `conversion_timestamp`

Current Stage 8d status:
- observation-side benchmarking is implemented;
- fixture runs are plumbing only;
- Stage 8d-A currently provides a **synthetic source-contract scaffold** via `tools/public_eeg_benchmarks/convert_ds005048_to_nmm_intermediate.py`:
  - source files (`*_source_events.csv`, `*_source_signal.csv`) are structurally distinct from emitted intermediate files (`*_events.tsv`, `*_timeseries.csv`);
  - manifest stores `source_root_ref`, `source_paths/source_file_hashes`, `intermediate_paths/intermediate_file_hashes`, and `conversion_inputs_by_intermediate`;
  - adapter verifies intermediate hashes and source hashes, and enforces source-input coverage per consumed intermediate file.
  - this is still **not** demonstrated ingestion of actual public ds005048 raw/BIDS files, so ds005048 remains not evidence-promotable (`conversion_status=not_started`, `benchmark_ready=false`).
- ASSR benchmark emits observation-side metrics and explicit unavailable status for prediction/comparison metrics until a real NMM prediction bridge is implemented (Stage 8d-B).

Current scientifically valid ASSR outputs:
- observed target-rate recovery from `observed_dominant_modulation_hz`
- observed target-band strength
- observed target-vs-control strength delta
- observed dominant modulation error
- prediction/comparison outputs are explicitly unavailable pending model bridge implementation.

`observed_assr_plv` is intentionally not used/named because true cross-trial PLV is not implemented yet in this path.
