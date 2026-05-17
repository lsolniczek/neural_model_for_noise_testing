from __future__ import annotations

import csv
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from tools.calibration.build_splits import build_grouped_splits
from tools.calibration.run_calibration import (
    FAILURE_THRESHOLDS,
    FEATURE_FAMILIES,
    OUTCOMES,
    _evaluate_partition,
)
from tools.calibration.validate_human_dataset import validate_dataset


ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "calibration" / "fixtures"


class Stage8CalibrationTests(unittest.TestCase):
    def test_schema_accepts_valid_fixture(self) -> None:
        report = validate_dataset(
            FIXTURES / "human_validation_manifest_v1.json",
            FIXTURES / "human_validation_trials_v1.csv",
            FIXTURES / "human_validation_peaks_v1.csv",
        )
        self.assertTrue(report["ok"], report["errors"])

    def test_schema_rejects_duplicate_trial_id(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            manifest = FIXTURES / "human_validation_manifest_v1.json"
            trials_src = FIXTURES / "human_validation_trials_v1.csv"
            peaks = FIXTURES / "human_validation_peaks_v1.csv"
            trials_bad = td_path / "trials_bad.csv"
            with trials_src.open("r", encoding="utf-8", newline="") as f:
                rows = list(csv.DictReader(f))
            rows[1]["trial_id"] = rows[0]["trial_id"]
            with trials_bad.open("w", encoding="utf-8", newline="") as f:
                w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
                w.writeheader()
                w.writerows(rows)
            report = validate_dataset(manifest, trials_bad, peaks)
            self.assertFalse(report["ok"])
            self.assertTrue(any("duplicate trial_id" in e for e in report["errors"]))

    def test_split_generation_keeps_participants_disjoint(self) -> None:
        with (FIXTURES / "human_validation_trials_v1.csv").open("r", encoding="utf-8", newline="") as f:
            trials = list(csv.DictReader(f))
        splits = build_grouped_splits(trials, k_folds=3, holdout_frac=0.2, seed=7)
        for fold in splits["folds"]:
            self.assertTrue(set(fold["train_participants"]).isdisjoint(set(fold["test_participants"])))

    def test_calibration_runner_emits_required_families_and_failures(self) -> None:
        with (FIXTURES / "human_validation_trials_v1.csv").open("r", encoding="utf-8", newline="") as f:
            trials = list(csv.DictReader(f))
        splits = build_grouped_splits(trials, k_folds=3, holdout_frac=0.0, seed=11)
        out = _evaluate_partition(trials, splits, seed=11, mode="cv")
        families = {r["model_family"] for r in out["metrics_rows"]}
        self.assertSetEqual(families, {"acoustic_only", "modulation_only", "legacy_v1", "candidate_v2"})
        self.assertIn("failure_rows", out)

    def test_acoustic_only_has_no_subjective_targets_as_predictors(self) -> None:
        self.assertEqual(FEATURE_FAMILIES["acoustic_only"], ["product_acoustic_score", "spl_db_a"])

    def test_outcomes_cover_stage8_required_list(self) -> None:
        required = {
            "aperiodic_exponent",
            "aperiodic_offset",
            "envelope_plv",
            "assr_plv",
            "alpha_peak_frequency_hz",
            "alpha_asymmetry",
            "vigilance_accuracy",
            "reaction_time_ms",
            "reaction_time_variability_ms",
            "comfort_rating",
            "irritation_rating",
            "masking_effectiveness_rating",
        }
        self.assertSetEqual(set(OUTCOMES), required)

    def test_missing_candidate_score_is_not_silently_zero_imputed(self) -> None:
        with (FIXTURES / "human_validation_trials_v1.csv").open("r", encoding="utf-8", newline="") as f:
            trials = list(csv.DictReader(f))
        splits = build_grouped_splits(trials, k_folds=2, holdout_frac=0.0, seed=19)
        out = _evaluate_partition(trials, splits, seed=19, mode="cv")
        dropped = [
            r for r in out["missingness_rows"]
            if r["model_family"] == "candidate_v2" and r["test_dropped_missing_feature"] > 0
        ]
        self.assertTrue(dropped, "Expected missing candidate_v2 feature rows to be dropped, not imputed to zero.")

    def test_split_generation_reduces_or_rejects_empty_fold_case(self) -> None:
        rows = [
            {"participant_id": "p1", "cohort": "c1"},
            {"participant_id": "p2", "cohort": "c1"},
        ]
        splits = build_grouped_splits(rows, k_folds=5, holdout_frac=0.0, seed=1)
        self.assertEqual(splits["k_folds"], 2)
        for fold in splits["folds"]:
            self.assertTrue(fold["test_participants"])

    def test_holdout_outputs_are_separate_from_cv(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            subprocess.run(
                [
                    "python3",
                    "tools/calibration/build_splits.py",
                    "--trials",
                    str(FIXTURES / "human_validation_trials_v1.csv"),
                    "--out",
                    str(td_path / "splits.json"),
                    "--k-folds",
                    "3",
                    "--holdout-frac",
                    "0.2",
                    "--seed",
                    "1234",
                ],
                check=True,
            )
            subprocess.run(
                [
                    "python3",
                    "tools/calibration/run_calibration.py",
                    "--manifest",
                    str(FIXTURES / "human_validation_manifest_v1.json"),
                    "--trials",
                    str(FIXTURES / "human_validation_trials_v1.csv"),
                    "--peaks",
                    str(FIXTURES / "human_validation_peaks_v1.csv"),
                    "--splits",
                    str(td_path / "splits.json"),
                    "--artifacts-root",
                    str(td_path / "artifacts"),
                    "--seed",
                    "1234",
                ],
                check=True,
            )
            run_dirs = sorted((td_path / "artifacts" / "fixture_daytime_attention_v1").glob("run_*"))
            self.assertTrue(run_dirs)
            run_dir = run_dirs[-1]
            self.assertTrue((run_dir / "predictions_cv.csv").exists())
            self.assertTrue((run_dir / "predictions_holdout.csv").exists())
            self.assertTrue((run_dir / "metrics_cv.csv").exists())
            self.assertTrue((run_dir / "metrics_holdout.csv").exists())
            self.assertTrue((run_dir / "metrics_cv_common_support.csv").exists())
            self.assertTrue((run_dir / "metrics_holdout_common_support.csv").exists())

    def test_common_support_outputs_have_shared_trial_ids_across_families(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            subprocess.run(
                [
                    "python3",
                    "tools/calibration/build_splits.py",
                    "--trials",
                    str(FIXTURES / "human_validation_trials_v1.csv"),
                    "--out",
                    str(td_path / "splits.json"),
                    "--k-folds",
                    "2",
                    "--holdout-frac",
                    "0.2",
                    "--seed",
                    "1234",
                ],
                check=True,
            )
            subprocess.run(
                [
                    "python3",
                    "tools/calibration/run_calibration.py",
                    "--manifest",
                    str(FIXTURES / "human_validation_manifest_v1.json"),
                    "--trials",
                    str(FIXTURES / "human_validation_trials_v1.csv"),
                    "--peaks",
                    str(FIXTURES / "human_validation_peaks_v1.csv"),
                    "--splits",
                    str(td_path / "splits.json"),
                    "--artifacts-root",
                    str(td_path / "artifacts"),
                    "--seed",
                    "1234",
                ],
                check=True,
            )
            run_dirs = sorted((td_path / "artifacts" / "fixture_daytime_attention_v1").glob("run_*"))
            run_dir = run_dirs[-1]
            with (run_dir / "predictions_cv_common_support.csv").open("r", encoding="utf-8", newline="") as f:
                rows = list(csv.DictReader(f))
            self.assertTrue(rows)
            by_outcome_fold_family = {}
            for r in rows:
                k = (r["outcome"], r["fold_id"], r["model_family"])
                by_outcome_fold_family.setdefault(k, set()).add(r["trial_id"])
            key_groups = {}
            for (outcome, fold_id, family), ids in by_outcome_fold_family.items():
                key_groups.setdefault((outcome, fold_id), []).append(ids)
            for groups in key_groups.values():
                base = groups[0]
                for g in groups[1:]:
                    self.assertSetEqual(base, g)

    def test_failure_thresholds_cover_all_outcomes(self) -> None:
        self.assertSetEqual(set(FAILURE_THRESHOLDS.keys()), set(OUTCOMES))

    def test_report_includes_family_specific_coverage_section_and_promotion_note(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            subprocess.run(
                [
                    "python3",
                    "tools/calibration/build_splits.py",
                    "--trials",
                    str(FIXTURES / "human_validation_trials_v1.csv"),
                    "--out",
                    str(td_path / "splits.json"),
                    "--k-folds",
                    "2",
                    "--holdout-frac",
                    "0.2",
                    "--seed",
                    "1234",
                ],
                check=True,
            )
            subprocess.run(
                [
                    "python3",
                    "tools/calibration/run_calibration.py",
                    "--manifest",
                    str(FIXTURES / "human_validation_manifest_v1.json"),
                    "--trials",
                    str(FIXTURES / "human_validation_trials_v1.csv"),
                    "--peaks",
                    str(FIXTURES / "human_validation_peaks_v1.csv"),
                    "--splits",
                    str(td_path / "splits.json"),
                    "--artifacts-root",
                    str(td_path / "artifacts"),
                    "--seed",
                    "1234",
                ],
                check=True,
            )
            run_dirs = sorted((td_path / "artifacts" / "fixture_daytime_attention_v1").glob("run_*"))
            run_dir = run_dirs[-1]
            subprocess.run(
                ["python3", "tools/calibration/report_calibration.py", "--run-dir", str(run_dir)],
                check=True,
            )
            report_text = (run_dir / "calibration_report.md").read_text(encoding="utf-8")
            self.assertIn("## Family-specific Coverage Metrics", report_text)
            self.assertIn(
                "Use common-support holdout metrics for model-family promotion decisions.",
                report_text,
            )

    def test_validation_rejects_manifest_missing_created_at_and_extra_field(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            manifest_src = json.loads((FIXTURES / "human_validation_manifest_v1.json").read_text(encoding="utf-8"))
            manifest_src.pop("created_at", None)
            manifest_src["unexpected"] = "x"
            bad_manifest = td_path / "manifest_bad.json"
            bad_manifest.write_text(json.dumps(manifest_src), encoding="utf-8")
            report = validate_dataset(
                bad_manifest,
                FIXTURES / "human_validation_trials_v1.csv",
                FIXTURES / "human_validation_peaks_v1.csv",
            )
            self.assertFalse(report["ok"])
            self.assertTrue(any("created_at" in e for e in report["errors"]))
            self.assertTrue(any("unexpected property 'unexpected'" in e for e in report["errors"]))

    def test_validation_rejects_invalid_numeric_without_crash(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            trials_src = FIXTURES / "human_validation_trials_v1.csv"
            peaks_src = FIXTURES / "human_validation_peaks_v1.csv"
            with trials_src.open("r", encoding="utf-8", newline="") as f:
                rows = list(csv.DictReader(f))
            rows[0]["spl_db_a"] = "abc"
            rows[0]["repeat_index"] = "oops"
            bad_trials = td_path / "bad_trials.csv"
            with bad_trials.open("w", encoding="utf-8", newline="") as f:
                w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
                w.writeheader()
                w.writerows(rows)
            with peaks_src.open("r", encoding="utf-8", newline="") as f:
                prows = list(csv.DictReader(f))
            prows[0]["center_hz"] = "nope"
            bad_peaks = td_path / "bad_peaks.csv"
            with bad_peaks.open("w", encoding="utf-8", newline="") as f:
                w = csv.DictWriter(f, fieldnames=list(prows[0].keys()))
                w.writeheader()
                w.writerows(prows)
            report = validate_dataset(
                FIXTURES / "human_validation_manifest_v1.json",
                bad_trials,
                bad_peaks,
            )
            self.assertFalse(report["ok"])
            self.assertTrue(any("invalid numeric value" in e for e in report["errors"]))

    def test_validation_rejects_unexpected_trial_and_peak_columns(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            with (FIXTURES / "human_validation_trials_v1.csv").open("r", encoding="utf-8", newline="") as f:
                trials = list(csv.DictReader(f))
            fieldnames = list(trials[0].keys()) + ["extra_trial_col"]
            for r in trials:
                r["extra_trial_col"] = "x"
            bad_trials = td_path / "bad_trials_extra.csv"
            with bad_trials.open("w", encoding="utf-8", newline="") as f:
                w = csv.DictWriter(f, fieldnames=fieldnames)
                w.writeheader()
                w.writerows(trials)
            with (FIXTURES / "human_validation_peaks_v1.csv").open("r", encoding="utf-8", newline="") as f:
                peaks = list(csv.DictReader(f))
            p_fields = list(peaks[0].keys()) + ["extra_peak_col"]
            for r in peaks:
                r["extra_peak_col"] = "x"
            bad_peaks = td_path / "bad_peaks_extra.csv"
            with bad_peaks.open("w", encoding="utf-8", newline="") as f:
                w = csv.DictWriter(f, fieldnames=p_fields)
                w.writeheader()
                w.writerows(peaks)
            report = validate_dataset(
                FIXTURES / "human_validation_manifest_v1.json",
                bad_trials,
                bad_peaks,
            )
            self.assertFalse(report["ok"])
            self.assertTrue(any("extra_trial_col" in e for e in report["errors"]))
            self.assertTrue(any("extra_peak_col" in e for e in report["errors"]))


if __name__ == "__main__":
    unittest.main()
