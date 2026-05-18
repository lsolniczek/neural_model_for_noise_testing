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
    def _build_real_layout_source_fixture(self, root: Path, sample_rate: float = 250.0) -> Path:
        import struct
        src = root / "ds005048_source_real_layout"
        eeg_dir = src / "sub-01" / "eeg"
        eeg_dir.mkdir(parents=True)
        (src / "dataset_description.json").write_text(json.dumps({"Name": "40Hz Auditory Entrainment", "DatasetVersion": "v1.0.1"}), encoding="utf-8")
        (src / "task-40HzAuditoryEntrainment_events.json").write_text(
            json.dumps({"value": {"Levels": {"1": "Rest", "2": "Stimulus"}}}), encoding="utf-8"
        )
        stem = "sub-01_task-40HzAuditoryEntrainment"
        (eeg_dir / f"{stem}_eeg.set").write_bytes(b"EEGLAB_PLACEHOLDER")
        # 19 channels, 500 samples => 2.0 seconds at 250 Hz
        n_channels = 19
        n_samples = 500
        data = []
        for t in range(n_samples):
            # first channel carries 40 Hz sinusoid; other channels zeros
            ch0 = 0.5 if (t % 6) < 3 else -0.5
            data.append(ch0)
            data.extend([0.0] * (n_channels - 1))
        (eeg_dir / f"{stem}_eeg.fdt").write_bytes(struct.pack("<" + ("f" * len(data)), *data))
        (eeg_dir / f"{stem}_eeg.json").write_text(
            json.dumps({"TaskName": "40HzAuditoryEntrainment", "SamplingFrequency": sample_rate}), encoding="utf-8"
        )
        (eeg_dir / f"{stem}_channels.tsv").write_text(
            "name\ttype\tunits\nFp1\tn/a\tn/a\nFp2\tn/a\tn/a\nF7\tn/a\tn/a\nF3\tn/a\tn/a\nFz\tn/a\tn/a\nF4\tn/a\tn/a\nF8\tn/a\tn/a\nT7\tn/a\tn/a\nC3\tn/a\tn/a\nCz\tn/a\tn/a\nC4\tn/a\tn/a\nT8\tn/a\tn/a\nP7\tn/a\tn/a\nP3\tn/a\tn/a\nPz\tn/a\tn/a\nP4\tn/a\tn/a\nP8\tn/a\tn/a\nO1\tn/a\tn/a\nO2\tn/a\tn/a\n",
            encoding="utf-8",
        )
        (eeg_dir / f"{stem}_events.tsv").write_text(
            "onset\tduration\tsample\tvalue\ttrial_type\n0\t1.2\t1\t2\tStimulus\n1.2\t0.8\t301\t1\tRest\n",
            encoding="utf-8",
        )
        return src

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
                parsed = {r["metric"]: r["value"] for r in csv.DictReader(f)}
            rows = {k: float(v) for k, v in parsed.items() if k != "observed_target_vs_control_strength_delta_status"}
            self.assertIn("observed_target_rate_recovery_accuracy", rows)
            self.assertIn("observed_target_vs_control_strength_delta", rows)
            self.assertIn("observed_dominant_modulation_hz_error", rows)
            self.assertEqual(parsed["observed_target_vs_control_strength_delta_status"], "ok")
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
            real_src = self._build_real_layout_source_fixture(td_path)
            out = td_path / "converted"
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/convert_ds005048_to_nmm_intermediate.py",
                    "--source-root",
                    str(real_src),
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
            self.assertNotEqual(set(manifest["source_paths"]), set(manifest["intermediate_paths"]))
            self.assertEqual(manifest["provenance_status_hint"], "source_verified")
            self.assertIn("source_root_ref", manifest)
            self.assertIn("conversion_inputs_by_intermediate", manifest)
            self.assertIn("source_contract", manifest)
            self.assertIn("semantic_source_inputs", manifest["source_contract"])
            self.assertIn("lineage_only_inputs", manifest["source_contract"])
            self.assertIn("*_eeg.set", manifest["source_contract"]["lineage_only_inputs"])
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
            self.assertEqual(result["provenance_status"], "source_verified")
            self.assertTrue(result["provenance_verified"])
            self.assertNotIn(
                "Source lineage is not fully verified; this run is not yet evidence-usable.",
                result["limitations"],
            )
            self.assertIn("predicted_gamma_assr_response_strength_surrogate", result["metrics_computed"])
            self.assertIn("dominant_rate_comparison_unavailable", result["metrics_computed"])
            self.assertIn("prediction_bridge", result)
            self.assertEqual(result["prediction_bridge"]["prediction_level"], "condition_level")
            self.assertEqual(
                result["prediction_bridge"]["predicted_dominant_modulation_hz_status"],
                "unavailable_no_independent_model_rate_estimator_stage8d_b",
            )
            self.assertEqual(result["evidence_category"], "not_yet_evidence_usable")
            with (out / "bench" / "assr_observed_metrics.csv").open("r", encoding="utf-8", newline="") as f:
                observed_metrics = list(csv.DictReader(f))
            observed_by_name = {r["metric"]: r["value"] for r in observed_metrics}
            self.assertEqual(observed_by_name["observed_target_vs_control_strength_delta_status"], "unavailable_no_control_rows")
            self.assertEqual(observed_by_name["observed_target_vs_control_strength_delta"], "")

    def test_source_verified_downgrades_when_source_root_missing(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            real_src = self._build_real_layout_source_fixture(td_path)
            out = td_path / "converted"
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/convert_ds005048_to_nmm_intermediate.py",
                    "--source-root",
                    str(real_src),
                    "--out-root",
                    str(out),
                    "--source-version",
                    "fixture_v1",
                ],
                check=True,
            )
            manifest_path = out / "nmm_benchmark_manifest.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["source_root_ref"] = str(td_path / "missing_source_root")
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
            adapter = Ds005048PreprocessedAdapter(out)
            adapter.export_benchmark_rows()
            self.assertEqual(adapter.last_provenance_status(), "intermediate_verified")

    def test_source_verified_rejected_on_source_hash_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            src_copy = self._build_real_layout_source_fixture(td_path)
            out = td_path / "converted"
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/convert_ds005048_to_nmm_intermediate.py",
                    "--source-root",
                    str(src_copy),
                    "--out-root",
                    str(out),
                    "--source-version",
                    "fixture_v1",
                ],
                check=True,
            )
            manifest_path = out / "nmm_benchmark_manifest.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            src_root = Path(manifest["source_root_ref"])
            first_src = manifest["source_paths"][0]
            original = src_root / first_src
            original.write_bytes(original.read_bytes() + b"\n# tamper\n")
            adapter = Ds005048PreprocessedAdapter(out)
            with self.assertRaises(DatasetLayoutError):
                adapter.export_benchmark_rows()

    def test_source_coverage_gap_prevents_source_verified(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            src_copy = self._build_real_layout_source_fixture(td_path)
            out = td_path / "converted"
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/convert_ds005048_to_nmm_intermediate.py",
                    "--source-root",
                    str(src_copy),
                    "--out-root",
                    str(out),
                    "--source-version",
                    "fixture_v1",
                ],
                check=True,
            )
            manifest_path = out / "nmm_benchmark_manifest.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            missing_src = Path(manifest["source_root_ref"]) / manifest["source_paths"][-1]
            missing_src.unlink()
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
            adapter = Ds005048PreprocessedAdapter(out)
            adapter.export_benchmark_rows()
            self.assertEqual(adapter.last_provenance_status(), "intermediate_verified")

    def test_manifest_underreporting_conversion_inputs_prevents_source_verified(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            src_copy = self._build_real_layout_source_fixture(td_path)
            out = td_path / "converted"
            subprocess.run(
                [
                    "python3",
                    "tools/public_eeg_benchmarks/convert_ds005048_to_nmm_intermediate.py",
                    "--source-root",
                    str(src_copy),
                    "--out-root",
                    str(out),
                    "--source-version",
                    "fixture_v1",
                ],
                check=True,
            )
            manifest_path = out / "nmm_benchmark_manifest.json"
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            any_intermediate = next(iter(manifest["conversion_inputs_by_intermediate"]))
            manifest["conversion_inputs_by_intermediate"][any_intermediate] = []
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")
            adapter = Ds005048PreprocessedAdapter(out)
            adapter.export_benchmark_rows()
            self.assertEqual(adapter.last_provenance_status(), "intermediate_verified")

    def test_low_sample_rate_rejected_for_40hz_target(self) -> None:
        with tempfile.TemporaryDirectory() as td:
            td_path = Path(td)
            eeg = td_path / "sub-01" / "eeg"
            eeg.mkdir(parents=True)
            ev_rel = "sub-01/eeg/sub-01_task-assr_events.tsv"
            ts_rel = "sub-01/eeg/sub-01_task-assr_timeseries.csv"
            events_path = td_path / ev_rel
            ts_path = td_path / ts_rel
            events_path.write_text("onset\tduration\tmodulation_rate_hz\tcondition_label\n0.0\t1.2\t40.0\ttarget\n", encoding="utf-8")
            ts_rows = ["time,sample_rate_hz,eeg"] + [f"{i/10:.1f},10,0.0" for i in range(20)]
            ts_path.write_text("\n".join(ts_rows) + "\n", encoding="utf-8")
            import hashlib
            ev_hash = "sha256:" + hashlib.sha256(events_path.read_bytes()).hexdigest()
            ts_hash = "sha256:" + hashlib.sha256(ts_path.read_bytes()).hexdigest()
            manifest = {
                "dataset_id": "ds005048",
                "subjects": ["sub-01"],
                "source_dataset_id": "ds005048",
                "source_dataset_version": "x",
                "source_root_ref": str(td_path),
                "source_paths": [ev_rel, ts_rel],
                "source_file_hashes": {ev_rel: ev_hash, ts_rel: ts_hash},
                "intermediate_paths": [ev_rel, ts_rel],
                "intermediate_file_hashes": {ev_rel: ev_hash, ts_rel: ts_hash},
                "conversion_inputs_by_intermediate": {ev_rel: [ev_rel], ts_rel: [ts_rel]},
                "conversion_tool_version": "x",
                "conversion_timestamp": "2026-01-01T00:00:00Z",
            }
            (td_path / "nmm_benchmark_manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
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
            self.assertIn("sample rate too low for target modulation frequency", result.stdout)

    def test_non_fixture_assr_result_lists_dominant_rate_unavailable_limitations(self) -> None:
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
                "Dominant-rate prediction/comparison is unavailable: no independent model rate estimator is exposed in Stage 8d-B.",
                result["limitations"],
            )
            self.assertIn(
                "Strength outputs are surrogate-only and not same-scale EEG power; control/rank/sign comparisons remain unavailable.",
                result["limitations"],
            )

    def test_stage8d_b_readme_no_longer_claims_predictions_unavailable(self) -> None:
        readme = (ROOT / "benchmarks" / "public_eeg" / "README.md").read_text(encoding="utf-8")
        self.assertNotIn(
            "prediction/comparison outputs are explicitly unavailable pending model bridge implementation.",
            readme,
        )
        self.assertIn("predicted_gamma_assr_response_strength", readme)

    def test_registry_ds005048_conversion_status_truthfulness(self) -> None:
        reg = load_registry()
        ds = next(d for d in reg["datasets"] if d["dataset_id"] == "ds005048")
        self.assertEqual(ds["conversion_status"], "implemented")
        self.assertFalse(ds["benchmark_ready"])


if __name__ == "__main__":
    unittest.main()
