from __future__ import annotations

import csv
import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from tools.public_eeg_benchmarks.common import FixtureAdapter, load_registry
from tools.public_eeg_benchmarks.adapters.ds005048 import DatasetLayoutError, Ds005048PreprocessedAdapter
from tools.public_eeg_benchmarks.validate_registry import validate_registry


ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "benchmarks" / "public_eeg" / "datasets_v1.json"
REGISTRY_SCHEMA = ROOT / "benchmarks" / "public_eeg" / "schema" / "public_eeg_registry_v1.schema.json"
DS005048_FIXTURE = ROOT / "tests" / "public_eeg_benchmarks" / "fixtures" / "ds005048_mock"


class Stage8cPublicEegTests(unittest.TestCase):
    def test_registry_has_required_component_support_and_limitations(self) -> None:
        reg = load_registry()
        self.assertEqual(reg["registry_version"], "public_eeg_datasets_v1")
        for d in reg["datasets"]:
            self.assertTrue(d["nmm_components_supported"])
            self.assertTrue(d["limitations"])
            self.assertIn("dataset_status", d)
            self.assertIn("evidence_usable", d)

    def test_registry_validator_accepts_current_registry(self) -> None:
        report = validate_registry(REGISTRY, REGISTRY_SCHEMA)
        self.assertTrue(report["ok"], report["errors"])

    def test_registry_validator_rejects_invalid_enum_missing_and_extra(self) -> None:
        reg = json.loads(REGISTRY.read_text(encoding="utf-8"))
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            bad_path = td_path / "bad_registry.json"

            # invalid enum
            bad = json.loads(json.dumps(reg))
            bad["datasets"][0]["download_status"] = "bad_status"
            bad_path.write_text(json.dumps(bad), encoding="utf-8")
            report = validate_registry(bad_path, REGISTRY_SCHEMA)
            self.assertFalse(report["ok"])
            self.assertTrue(any("invalid enum value" in e for e in report["errors"]))

            # missing required dataset property
            bad = json.loads(json.dumps(reg))
            del bad["datasets"][0]["name"]
            bad_path.write_text(json.dumps(bad), encoding="utf-8")
            report = validate_registry(bad_path, REGISTRY_SCHEMA)
            self.assertFalse(report["ok"])
            self.assertTrue(any("missing 'name'" in e for e in report["errors"]))

            # unexpected extra property
            bad = json.loads(json.dumps(reg))
            bad["datasets"][0]["unexpected_prop"] = "x"
            bad_path.write_text(json.dumps(bad), encoding="utf-8")
            report = validate_registry(bad_path, REGISTRY_SCHEMA)
            self.assertFalse(report["ok"])
            self.assertTrue(any("unexpected key 'unexpected_prop'" in e for e in report["errors"]))

            # wrong type
            bad = json.loads(json.dumps(reg))
            bad["datasets"][0]["participants"] = "not_int"
            bad_path.write_text(json.dumps(bad), encoding="utf-8")
            report = validate_registry(bad_path, REGISTRY_SCHEMA)
            self.assertFalse(report["ok"])
            self.assertTrue(any("type mismatch" in e for e in report["errors"]))

    def test_fixture_adapter_normalizes_common_row_contract(self) -> None:
        rows = FixtureAdapter("assr").export_benchmark_rows()
        self.assertTrue(rows)
        required = {
            "dataset_id",
            "subject_id",
            "trial_id",
            "condition_id",
            "stimulus_type",
            "modulation_rate_hz",
            "carrier_or_task_label",
            "eeg_signal_ref",
            "behavior_available",
            "expected_effect_label",
        }
        self.assertSetEqual(set(rows[0].keys()), required)

    def test_assr_runner_fails_cleanly_when_dataset_not_downloaded(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            result = subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/run_assr_benchmark.py",
                    "--dataset",
                    "ds005048",
                    "--output-dir",
                    td,
                ],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 2)
            self.assertIn("dataset_not_downloaded", result.stdout)

    def test_ds005048_adapter_discovers_mock_layout(self) -> None:
        adapter = Ds005048PreprocessedAdapter(DS005048_FIXTURE)
        rows = adapter.export_benchmark_rows()
        self.assertTrue(rows)
        self.assertIn("observed_assr_strength", rows[0])
        self.assertIn("observed_dominant_modulation_hz", rows[0])
        self.assertNotIn("observed_assr_plv", rows[0])

    def test_ds005048_adapter_rejects_malformed_layout(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            (td_path / "nmm_benchmark_manifest.json").write_text(
                json.dumps({"dataset_id": "ds005048", "subjects": ["sub-01"]}), encoding="utf-8"
            )
            (td_path / "sub-01").mkdir()
            with self.assertRaises(DatasetLayoutError):
                Ds005048PreprocessedAdapter(td_path).export_benchmark_rows()

    def test_ds005048_adapter_rejects_weak_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            (td_path / "nmm_benchmark_manifest.json").write_text(
                json.dumps(
                    {
                        "dataset_id": "ds005048",
                        "subjects": ["sub-01"],
                        "source_dataset_id": "wrong",
                        "source_dataset_version": "",
                        "source_paths": [],
                        "source_file_hashes": {},
                        "conversion_tool_version": "",
                        "conversion_timestamp": "",
                    }
                ),
                encoding="utf-8",
            )
            eeg = td_path / "sub-01" / "eeg"
            eeg.mkdir(parents=True)
            (eeg / "x_events.tsv").write_text("onset\tduration\tmodulation_rate_hz\tcondition_label\n0\t1\t40\tt\n", encoding="utf-8")
            (eeg / "x_timeseries.csv").write_text("time,sample_rate_hz,eeg\n0,200,0.0\n", encoding="utf-8")
            with self.assertRaises(DatasetLayoutError):
                Ds005048PreprocessedAdapter(td_path).export_benchmark_rows()

    def test_ds005048_adapter_rejects_intermediate_hash_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            eeg = td_path / "sub-01" / "eeg"
            eeg.mkdir(parents=True)
            ev = eeg / "sub-01_task-assr_events.tsv"
            ts = eeg / "sub-01_task-assr_timeseries.csv"
            ev.write_text("onset\tduration\tmodulation_rate_hz\tcondition_label\n0\t1\t40\tt\n", encoding="utf-8")
            ts.write_text("time,sample_rate_hz,eeg\n0.0,200,0.0\n0.005,200,0.1\n", encoding="utf-8")
            (td_path / "nmm_benchmark_manifest.json").write_text(
                json.dumps(
                    {
                        "dataset_id": "ds005048",
                        "subjects": ["sub-01"],
                        "source_dataset_id": "ds005048",
                        "source_dataset_version": "x",
                        "source_paths": [str(ev.relative_to(td_path))],
                        "source_file_hashes": {str(ev.relative_to(td_path)): "sha256:" + "a" * 64},
                        "intermediate_paths": [str(ev.relative_to(td_path)), str(ts.relative_to(td_path))],
                        "intermediate_file_hashes": {
                            str(ev.relative_to(td_path)): "sha256:" + "b" * 64,
                            str(ts.relative_to(td_path)): "sha256:" + "c" * 64,
                        },
                        "conversion_tool_version": "x",
                        "conversion_timestamp": "2026-01-01T00:00:00Z",
                    }
                ),
                encoding="utf-8",
            )
            with self.assertRaises(DatasetLayoutError):
                Ds005048PreprocessedAdapter(td_path).export_benchmark_rows()

    def test_fixture_benchmark_reports_and_evidence_map(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            out = Path(td)
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/run_assr_benchmark.py",
                    "--dataset",
                    "fixture_public_eeg",
                    "--output-dir",
                    str(out),
                    "--use-fixture",
                ],
                check=True,
            )
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/run_aperiodic_benchmark.py",
                    "--dataset",
                    "fixture_public_eeg",
                    "--output-dir",
                    str(out),
                    "--use-fixture",
                ],
                check=True,
            )
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/run_auditory_attention_benchmark.py",
                    "--dataset",
                    "fixture_public_eeg",
                    "--output-dir",
                    str(out),
                    "--use-fixture",
                ],
                check=True,
            )
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/build_evidence_map.py",
                    "--input-dir",
                    str(out),
                    "--output",
                    str(out / "public_eeg_evidence_map.md"),
                ],
                check=True,
            )
            evidence = (out / "public_eeg_evidence_map.md").read_text(encoding="utf-8")
            self.assertIn("validated_by_public_data", evidence)
            self.assertIn("None yet. No real-public-data validated entries were found.", evidence)
            self.assertIn("not_yet_evidence_usable", evidence)
            self.assertIn("fixture_smoke_tests", evidence)
            self.assertIn("unsupported_requires_own_study", evidence)
            self.assertTrue((out / "assr_benchmark_report.md").exists())
            self.assertTrue((out / "assr_benchmark_result.json").exists())
            self.assertTrue((out / "assr_metrics.csv").exists())
            self.assertTrue((out / "assr_failure_cases.csv").exists())
            self.assertTrue((out / "aperiodic_benchmark_report.md").exists())
            self.assertTrue((out / "aperiodic_benchmark_result.json").exists())
            self.assertTrue((out / "auditory_attention_benchmark_report.md").exists())
            self.assertTrue((out / "auditory_attention_benchmark_result.json").exists())

            assr_meta = json.loads((out / "assr_benchmark_result.json").read_text(encoding="utf-8"))
            self.assertEqual(assr_meta["run_mode"], "fixture_smoke_test")
            self.assertEqual(assr_meta["evidence_category"], "plumbing_verified")

    def test_assr_metric_engine_computes_expected_fixture_outputs(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            out = Path(td)
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/run_assr_benchmark.py",
                    "--dataset",
                    "fixture_public_eeg",
                    "--output-dir",
                    str(out),
                    "--use-fixture",
                ],
                check=True,
            )
            with (out / "assr_metrics.csv").open("r", encoding="utf-8", newline="") as f:
                rows = {r["metric"]: float(r["value"]) for r in csv.DictReader(f)}
            self.assertIn("observed_target_rate_recovery_accuracy", rows)
            self.assertIn("observed_target_vs_control_strength_delta", rows)
            self.assertIn("observed_dominant_modulation_hz_error", rows)
            self.assertGreaterEqual(rows["observed_target_rate_recovery_accuracy"], 0.0)
            self.assertGreaterEqual(rows["observed_dominant_modulation_hz_error"], 0.0)

    def test_real_assr_mode_requires_dataset_root(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            result = subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/run_assr_benchmark.py",
                    "--dataset",
                    "ds005048",
                    "--output-dir",
                    td,
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)

    def test_real_assr_run_uses_observed_fields_from_adapter(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            out = Path(td)
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/run_assr_benchmark.py",
                    "--dataset",
                    "ds005048",
                    "--dataset-root",
                    str(DS005048_FIXTURE),
                    "--output-dir",
                    str(out),
                ],
                check=True,
            )
            result = json.loads((out / "assr_benchmark_result.json").read_text(encoding="utf-8"))
            self.assertEqual(result["run_mode"], "real_public_data")
            self.assertIn("observed_target_rate_recovery_accuracy", result["metrics_computed"])
            self.assertTrue(result["resolution_sufficient_for_target_metric"])
            self.assertEqual(result["provenance_status"], "intermediate_verified")
            self.assertFalse(result["provenance_verified"])
            self.assertIn("min_epoch_duration_s", result)
            self.assertIn("max_epoch_duration_s", result)
            self.assertIn("max_frequency_resolution_hz", result)
            self.assertTrue(result["all_rows_resolution_sufficient"])
            self.assertTrue((out / "assr_observed_metrics.csv").exists())
            self.assertTrue((out / "assr_prediction_metrics.csv").exists())
            self.assertTrue((out / "assr_comparison_metrics.csv").exists())
            self.assertTrue((out / "assr_prediction_rows.csv").exists())

    def test_short_epoch_is_rejected_for_resolution(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            eeg = td_path / "sub-01" / "eeg"
            eeg.mkdir(parents=True)
            events_rel = "sub-01/eeg/sub-01_task-assr_events.tsv"
            ts_rel = "sub-01/eeg/sub-01_task-assr_timeseries.csv"
            events_path = eeg / "sub-01_task-assr_events.tsv"
            ts_path = eeg / "sub-01_task-assr_timeseries.csv"
            events_path.write_text(
                "onset\tduration\tmodulation_rate_hz\tcondition_label\n0.0\t0.1\t40.0\ttarget\n", encoding="utf-8"
            )
            ts_path.write_text(
                "time,sample_rate_hz,eeg\n0.0,200,0.0\n0.005,200,0.1\n", encoding="utf-8"
            )
            import hashlib
            ev_hash = "sha256:" + hashlib.sha256(events_path.read_bytes()).hexdigest()
            ts_hash = "sha256:" + hashlib.sha256(ts_path.read_bytes()).hexdigest()
            (td_path / "nmm_benchmark_manifest.json").write_text(
                json.dumps(
                    {
                        "dataset_id": "ds005048",
                        "subjects": ["sub-01"],
                        "source_dataset_id": "ds005048",
                        "source_dataset_version": "x",
                        "source_paths": [events_rel, ts_rel],
                        "source_file_hashes": {
                            events_rel: ev_hash,
                            ts_rel: ts_hash,
                        },
                        "intermediate_paths": [
                            events_rel,
                            ts_rel,
                        ],
                        "intermediate_file_hashes": {
                            events_rel: ev_hash,
                            ts_rel: ts_hash,
                        },
                        "conversion_tool_version": "x",
                        "conversion_timestamp": "2026-01-01T00:00:00Z",
                    }
                ),
                encoding="utf-8",
            )
            result = subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/run_assr_benchmark.py",
                    "--dataset",
                    "ds005048",
                    "--dataset-root",
                    str(td_path),
                    "--output-dir",
                    str(td_path / "out"),
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("epoch too short for ASSR benchmark", result.stdout)

    def test_evidence_map_uses_real_result_metadata_for_promotion(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            out = Path(td)
            (out / "assr_benchmark_result.json").write_text(
                json.dumps(
                    {
                        "dataset_id": "ds005048",
                        "benchmark_family": "assr",
                        "run_mode": "real_public_data",
                        "data_status": "downloaded",
                        "metrics_computed": ["observed_target_rate_recovery"],
                        "evidence_category": "partially_supported",
                    }
                ),
                encoding="utf-8",
            )
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/build_evidence_map.py",
                    "--input-dir",
                    str(out),
                    "--output",
                    str(out / "public_eeg_evidence_map.md"),
                ],
                check=True,
            )
            evidence = (out / "public_eeg_evidence_map.md").read_text(encoding="utf-8")
            self.assertIn("not_yet_evidence_usable", evidence)
            self.assertIn("assr via `ds005048`", evidence)

    def test_evidence_map_promotes_only_when_registry_ready(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            out = Path(td)
            (out / "assr_benchmark_result.json").write_text(
                json.dumps(
                    {
                        "dataset_id": "ds005048",
                        "benchmark_family": "assr",
                        "run_mode": "real_public_data",
                        "data_status": "downloaded",
                        "metrics_computed": ["observed_target_rate_recovery_accuracy"],
                        "evidence_category": "partially_supported",
                        "input_kind": "preprocessed_intermediate",
                        "provenance_status": "intermediate_verified",
                    }
                ),
                encoding="utf-8",
            )
            reg = load_registry()
            for d in reg["datasets"]:
                if d["dataset_id"] == "ds005048":
                    d["benchmark_ready"] = True
                    d["conversion_status"] = "implemented"
            reg_path = out / "reg.json"
            reg_path.write_text(json.dumps(reg), encoding="utf-8")
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/build_evidence_map.py",
                    "--input-dir",
                    str(out),
                    "--output",
                    str(out / "public_eeg_evidence_map.md"),
                    "--registry-path",
                    str(reg_path),
                ],
                check=True,
            )
            evidence = (out / "public_eeg_evidence_map.md").read_text(encoding="utf-8")
            self.assertIn("## not_yet_evidence_usable", evidence)
            self.assertIn("input_kind=preprocessed_intermediate", evidence)
            self.assertIn("provenance_status=intermediate_verified", evidence)

            # only source_verified can promote through conversion path
            (out / "assr_benchmark_result.json").write_text(
                json.dumps(
                    {
                        "dataset_id": "ds005048",
                        "benchmark_family": "assr",
                        "run_mode": "real_public_data",
                        "data_status": "downloaded",
                        "metrics_computed": ["observed_target_rate_recovery_accuracy"],
                        "evidence_category": "partially_supported",
                        "input_kind": "preprocessed_intermediate",
                        "provenance_status": "source_verified",
                    }
                ),
                encoding="utf-8",
            )
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/build_evidence_map.py",
                    "--input-dir",
                    str(out),
                    "--output",
                    str(out / "public_eeg_evidence_map.md"),
                    "--registry-path",
                    str(reg_path),
                ],
                check=True,
            )
            evidence2 = (out / "public_eeg_evidence_map.md").read_text(encoding="utf-8")
            self.assertIn("## partially_supported", evidence2)
            self.assertIn("assr via `ds005048`", evidence2)

    def test_intermediate_result_does_not_promote_via_raw_adapter_branch(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            out = Path(td)
            (out / "assr_benchmark_result.json").write_text(
                json.dumps(
                    {
                        "dataset_id": "ds005048",
                        "benchmark_family": "assr",
                        "run_mode": "real_public_data",
                        "data_status": "downloaded",
                        "metrics_computed": ["observed_target_rate_recovery_accuracy"],
                        "evidence_category": "partially_supported",
                        "input_kind": "preprocessed_intermediate",
                        "provenance_status": "intermediate_verified",
                    }
                ),
                encoding="utf-8",
            )
            reg = load_registry()
            for d in reg["datasets"]:
                if d["dataset_id"] == "ds005048":
                    d["benchmark_ready"] = True
                    d["raw_adapter_status"] = "implemented"
                    d["conversion_status"] = "not_started"
            reg_path = out / "reg.json"
            reg_path.write_text(json.dumps(reg), encoding="utf-8")
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/build_evidence_map.py",
                    "--input-dir",
                    str(out),
                    "--output",
                    str(out / "public_eeg_evidence_map.md"),
                    "--registry-path",
                    str(reg_path),
                ],
                check=True,
            )
            evidence = (out / "public_eeg_evidence_map.md").read_text(encoding="utf-8")
            self.assertIn("## not_yet_evidence_usable", evidence)
            self.assertIn("input_kind=preprocessed_intermediate", evidence)
            self.assertIn("provenance_status=intermediate_verified", evidence)

    def test_intermediate_declared_only_result_does_not_promote(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            out = Path(td)
            (out / "assr_benchmark_result.json").write_text(
                json.dumps(
                    {
                        "dataset_id": "ds005048",
                        "benchmark_family": "assr",
                        "run_mode": "real_public_data",
                        "data_status": "downloaded",
                        "metrics_computed": ["observed_target_rate_recovery_accuracy"],
                        "evidence_category": "partially_supported",
                        "input_kind": "preprocessed_intermediate",
                        "provenance_status": "declared_only",
                    }
                ),
                encoding="utf-8",
            )
            reg = load_registry()
            for d in reg["datasets"]:
                if d["dataset_id"] == "ds005048":
                    d["benchmark_ready"] = True
                    d["conversion_status"] = "implemented"
            reg_path = out / "reg.json"
            reg_path.write_text(json.dumps(reg), encoding="utf-8")
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/build_evidence_map.py",
                    "--input-dir",
                    str(out),
                    "--output",
                    str(out / "public_eeg_evidence_map.md"),
                    "--registry-path",
                    str(reg_path),
                ],
                check=True,
            )
            evidence = (out / "public_eeg_evidence_map.md").read_text(encoding="utf-8")
            self.assertIn("## not_yet_evidence_usable", evidence)
            self.assertIn("provenance_status=declared_only", evidence)

    def test_intermediate_consumed_file_coverage_is_required(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            eeg = td_path / "sub-01" / "eeg"
            eeg.mkdir(parents=True)
            ev = eeg / "sub-01_task-assr_events.tsv"
            ts = eeg / "sub-01_task-assr_timeseries.csv"
            ev.write_text("onset\tduration\tmodulation_rate_hz\tcondition_label\n0\t1\t40\tt\n", encoding="utf-8")
            ts_rows = ["time,sample_rate_hz,eeg"] + [f"{i/200:.3f},200,0.0" for i in range(0, 240)]
            ts.write_text("\n".join(ts_rows) + "\n", encoding="utf-8")
            import hashlib
            ev_rel = str(ev.relative_to(td_path))
            ts_rel = str(ts.relative_to(td_path))
            ev_hash = "sha256:" + hashlib.sha256(ev.read_bytes()).hexdigest()
            ts_hash = "sha256:" + hashlib.sha256(ts.read_bytes()).hexdigest()
            (td_path / "nmm_benchmark_manifest.json").write_text(
                json.dumps(
                    {
                        "dataset_id": "ds005048",
                        "subjects": ["sub-01"],
                        "source_dataset_id": "ds005048",
                        "source_dataset_version": "x",
                        "source_paths": [ev_rel],
                        "source_file_hashes": {ev_rel: ev_hash},
                        "intermediate_paths": [ev_rel],  # missing consumed timeseries file
                        "intermediate_file_hashes": {ev_rel: ev_hash, ts_rel: ts_hash},
                        "conversion_tool_version": "x",
                        "conversion_timestamp": "2026-01-01T00:00:00Z",
                    }
                ),
                encoding="utf-8",
            )
            adapter = Ds005048PreprocessedAdapter(td_path)
            adapter.export_benchmark_rows()
            self.assertEqual(adapter.last_provenance_status(), "declared_only")

    def test_placeholder_datasets_are_not_evidence_usable(self) -> None:
        reg = load_registry()
        placeholders = [d for d in reg["datasets"] if d["dataset_status"] == "placeholder"]
        self.assertTrue(placeholders)
        for d in placeholders:
            self.assertFalse(d["evidence_usable"])

    def test_stage8d_converter_remains_non_evidence_scaffold(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            out = td_path / "converted"
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/convert_ds005048_to_nmm_intermediate.py",
                    "--source-root",
                    str(DS005048_FIXTURE),
                    "--out-root",
                    str(out),
                    "--source-version",
                    "fixture_v1",
                ],
                check=True,
            )
            manifest = json.loads((out / "nmm_benchmark_manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest["source_dataset_id"], "ds005048")
            self.assertTrue(manifest["source_paths"])
            self.assertTrue(manifest["intermediate_paths"])
            self.assertEqual(manifest["provenance_status_hint"], "intermediate_verified")
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/run_assr_benchmark.py",
                    "--dataset",
                    "ds005048",
                    "--dataset-root",
                    str(out),
                    "--output-dir",
                    str(out / "bench"),
                ],
                check=True,
            )
            result = json.loads((out / "bench" / "assr_benchmark_result.json").read_text(encoding="utf-8"))
            self.assertEqual(result["provenance_status"], "intermediate_verified")
            self.assertIn("comparison_unavailable_noncommensurate_output", result["metrics_computed"])
            self.assertEqual(result["evidence_category"], "not_yet_evidence_usable")

    def test_non_fixture_assr_result_lists_both_downgrade_limitations(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            out = Path(td)
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/run_assr_benchmark.py",
                    "--dataset",
                    "ds005048",
                    "--dataset-root",
                    str(DS005048_FIXTURE),
                    "--output-dir",
                    str(out),
                ],
                check=True,
            )
            result = json.loads((out / "assr_benchmark_result.json").read_text(encoding="utf-8"))
            self.assertIn(
                "Source lineage is not fully verified; this run is not yet evidence-usable.",
                result["limitations"],
            )
            self.assertIn(
                "NMM prediction bridge is not implemented; prediction/comparison metrics are unavailable.",
                result["limitations"],
            )


if __name__ == "__main__":
    unittest.main()
