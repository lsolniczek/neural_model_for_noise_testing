# Stage 8 Calibration Workflow

This folder defines the offline human-validation and calibration workflow for NMM Stage 8.

It is intentionally separate from runtime scoring. Nothing here promotes `candidate_v2` or changes production defaults.

## Dataset layout

Required inputs:

1. `manifest.json` (schema versioned metadata)
2. `trials.csv` (one row per participant × session × condition)
3. `peaks.csv` (long-format zero-or-more periodic peaks per trial)

Schema files:

- `schema/human_validation_manifest_v1.schema.json`
- `schema/human_validation_trials_v1.schema.json`
- `schema/human_validation_peaks_v1.schema.json`

Fixture dataset:

- `fixtures/human_validation_manifest_v1.json`
- `fixtures/human_validation_trials_v1.csv`
- `fixtures/human_validation_peaks_v1.csv`

## Why separate tables

Periodic peaks are variable length. They are stored in `peaks.csv` instead of flattened trial columns.

## Validation

```bash
python tools/calibration/validate_human_dataset.py \
  --manifest calibration/fixtures/human_validation_manifest_v1.json \
  --trials calibration/fixtures/human_validation_trials_v1.csv \
  --peaks calibration/fixtures/human_validation_peaks_v1.csv \
  --out /tmp/nmm_validation_report.json
```

The validator checks:

- schema version and required identifiers
- duplicate `trial_id`
- value ranges (`modulation_depth`, `spl_db_a`)
- peaks table referential integrity (`trial_id` exists in trials)

## Split generation

```bash
python tools/calibration/build_splits.py \
  --trials calibration/fixtures/human_validation_trials_v1.csv \
  --out /tmp/nmm_splits.json \
  --k-folds 3 \
  --holdout-frac 0.2 \
  --seed 1234
```

Splits are participant-grouped to prevent participant leakage across train/test.

## Calibration run

```bash
python tools/calibration/run_calibration.py \
  --manifest calibration/fixtures/human_validation_manifest_v1.json \
  --trials calibration/fixtures/human_validation_trials_v1.csv \
  --peaks calibration/fixtures/human_validation_peaks_v1.csv \
  --splits /tmp/nmm_splits.json \
  --artifacts-root calibration/artifacts \
  --seed 1234
```

Compared model families:

- `acoustic_only`
- `modulation_only`
- `legacy_v1`
- `candidate_v2`

Missing-data policy:

- rows with missing outcome for a given endpoint are excluded for that endpoint,
- rows with missing features for a given model family are excluded for that family,
- missing values are never silently converted to zero.

Outputs are written to:

`calibration/artifacts/<dataset_id>/<run_id>/`

including:

- `calibration_run_manifest.json`
- `split_manifest.json`
- `metrics_by_model.csv`
- `metrics_by_outcome.csv`
- `metrics_cv.csv`
- `metrics_holdout.csv`
- `metrics_cv_common_support.csv`
- `metrics_holdout_common_support.csv`
- `predictions_cv.csv`
- `predictions_holdout.csv`
- `predictions_cv_common_support.csv`
- `predictions_holdout_common_support.csv`
- `failure_cases_cv.csv`
- `failure_cases_holdout.csv`
- `missingness_cv.csv`
- `missingness_holdout.csv`

`predictions_holdout.csv` is reserved for the locked participant holdout only (never CV rows).

Model-family comparison rule:

- Use only `*_common_support.csv` outputs for cross-family comparison and promotion decisions.
- Family-specific files (`metrics_cv.csv`, `metrics_holdout.csv`) are coverage diagnostics and can use different row subsets by family.

## Report generation

```bash
python tools/calibration/report_calibration.py \
  --run-dir calibration/artifacts/<dataset_id>/<run_id>
```

This writes `calibration_report.md` with held-out metrics and uncertainty intervals.

## Scientific scope

- Periodic and aperiodic endpoints stay separate.
- Daytime attention datasets do not validate sleep claims.
- Outputs are evidence artifacts for later promotion decisions, not promotion itself.
