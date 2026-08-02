import importlib.util
import contextlib
import io
import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock


MODULE_PATH = Path(__file__).parents[2] / "tools" / "compatibility" / "stage1_baseline.py"
SPEC = importlib.util.spec_from_file_location("stage1_baseline", MODULE_PATH)
stage1 = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(stage1)


def valid_report():
    return {
        "preset_path": "presets/showcase_white.json",
        "goal": "focus",
        "brain_type": "Normal",
        "score": 0.5,
        "practical_status": "usable",
        "goal_semantics": {},
        "practical_report": {},
        "band_powers": {"delta": 0.1, "theta": 0.2, "alpha": 0.3, "beta": 0.3, "gamma": 0.1},
        "dominant_frequency_hz": 10.0,
        "fhn_firing_rate": 12.0,
        "fhn_isi_cv": None,
        "acoustic_summary": {},
        "limitations": ["proxy"],
        "model_signature": {
            "duration_secs": 2.1,
            "auditory_flags": {
                "assr_enabled": False,
                "thalamic_gate_enabled": False,
                "physiological_thalamic_gate_enabled": False,
                "cet_enabled": False,
                "acoustic_scoring_enabled": False,
                "acoustic_score_fusion_enabled": False,
            },
        },
    }


def valid_manifest():
    return {
        "schema_version": 2,
        "baseline_id": stage1.BASELINE_ID,
        "captured_at_utc": "2026-08-01T00:00:00+00:00",
        "warning": "legacy",
        "repositories": {name: {"head": head} for name, head in stage1.EXPECTED_HEADS.items()},
        "source_fingerprints": {
            "nmm_runtime": {"sha256": "sha256:a", "file_count": 1},
            "dsp_runtime": {"sha256": "sha256:b", "file_count": 1},
            "ios_integration": {"sha256": "sha256:c", "file_count": 1},
        },
        "toolchain": {
            "rustc_verbose": "rustc",
            "cargo": "cargo",
            "python": "3.9",
            "os": "macOS",
            "architecture": "arm64",
        },
        "capture_tool_sha256": "sha256:d",
        "renderer_observation": stage1.RENDERER_OBSERVATION,
        "evaluation_profile": stage1.PROFILE,
        "preset_corpus": {"root": "inputs/presets", "count": 60},
        "tests": {
            "nmm": {
                "summary": stage1.EXPECTED_NMM_SUMMARY,
                "failures": sorted(stage1.EXPECTED_NMM_FAILURES),
                "exit_code": 101,
            },
            "dsp": {"summary": stage1.EXPECTED_DSP_SUMMARY, "exit_code": 0},
        },
        "evaluation_result": {"attempted": 60, "succeeded": 60},
        "artifact_hashes": {},
    }


class Stage1BaselineTests(unittest.TestCase):
    def test_snapshot_and_report_paths_preserve_nesting(self):
        relative = Path("unwind/calm_v1.json")
        self.assertEqual(stage1.snapshot_preset_path(relative), Path("inputs/presets/unwind/calm_v1.json"))
        self.assertEqual(stage1.normalized_report_path(relative), Path("evaluations/unwind/calm_v1.json"))

    def test_parse_summary_selects_largest_suite(self):
        output = "test result: ok. 0 passed; 0 failed; 0 ignored;\n" \
                 "test result: FAILED. 511 passed; 2 failed; 5 ignored;\n"
        self.assertEqual(stage1.parse_summary(output), stage1.EXPECTED_NMM_SUMMARY)

    def test_nmm_baseline_accepts_only_exact_failures_and_exit_101(self):
        output = "\n".join(
            f"---- {name} stdout ----" for name in sorted(stage1.EXPECTED_NMM_FAILURES)
        ) + "\ntest result: FAILED. 511 passed; 2 failed; 5 ignored;\n"
        self.assertEqual(stage1.assert_nmm_test_baseline(output, 101), stage1.EXPECTED_NMM_SUMMARY)
        with self.assertRaises(stage1.BaselineError):
            stage1.assert_nmm_test_baseline(output, 1)

    def test_nmm_baseline_rejects_new_failure(self):
        output = (
            "---- disturb::tests::disturb_canonical_golden_snapshot_reference_case stdout ----\n"
            "---- disturb::tests::disturb_legacy_ablated_golden_snapshot_reference_case stdout ----\n"
            "---- new::failure stdout ----\n"
            "test result: FAILED. 511 passed; 2 failed; 5 ignored;\n"
        )
        with self.assertRaises(stage1.BaselineError):
            stage1.assert_nmm_test_baseline(output, 101)

    def test_dsp_baseline_rejects_nonzero_exit(self):
        output = "test result: ok. 667 passed; 0 failed; 2 ignored;\n"
        with self.assertRaises(stage1.BaselineError):
            stage1.assert_dsp_test_baseline(output, 1)

    def test_evaluation_command_locks_profile(self):
        command = stage1.command_for_evaluation(Path("/binary"), Path("showcase_white.json"), Path("/report.json"))
        self.assertIn("--no-assr", command)
        self.assertIn("--no-thalamic-gate", command)
        self.assertIn("--no-cet", command)
        self.assertEqual(command[command.index("--goal") + 1], "focus")
        self.assertEqual(command[command.index("--duration") + 1], "2.1")

    def test_validate_report_accepts_contract(self):
        stage1.validate_report(valid_report(), Path("showcase_white.json"))

    def test_validate_report_rejects_missing_and_nonfinite_metrics(self):
        missing = valid_report()
        del missing["band_powers"]
        with self.assertRaises(stage1.BaselineError):
            stage1.validate_report(missing, Path("showcase_white.json"))
        nonfinite = valid_report()
        nonfinite["dominant_frequency_hz"] = float("inf")
        with self.assertRaises(stage1.BaselineError):
            stage1.validate_report(nonfinite, Path("showcase_white.json"))

    def test_load_json_rejects_nan(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            path = Path(temp_dir) / "bad.json"
            path.write_text('{"score": NaN}', encoding="utf-8")
            with self.assertRaises(stage1.BaselineError):
                stage1.load_json(path)

    def test_discover_presets_requires_exactly_sixty(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            directory = Path(temp_dir)
            (directory / "one.json").write_text("{}", encoding="utf-8")
            with self.assertRaises(stage1.BaselineError):
                stage1.discover_presets(directory)

    def test_manifest_requires_complete_provenance(self):
        stage1.validate_manifest(valid_manifest())
        manifest = valid_manifest()
        del manifest["toolchain"]
        with self.assertRaises(stage1.BaselineError):
            stage1.validate_manifest(manifest)

    def test_manifest_rejects_repository_revision_change(self):
        manifest = valid_manifest()
        manifest["repositories"]["dsp"]["head"] = "0" * 40
        with self.assertRaises(stage1.BaselineError):
            stage1.validate_manifest(manifest)

    def test_candidate_specs_cover_thirteen_non_equivalent_providers(self):
        specs = stage1.candidate_specs()
        self.assertEqual(len(specs), 13)
        self.assertEqual(len({provider for provider, _, _ in specs}), 13)

    def test_write_json_rejects_nan(self):
        with tempfile.TemporaryDirectory() as temp_dir:
            with self.assertRaises(ValueError):
                stage1.write_json(Path(temp_dir) / "bad.json", {"value": float("nan")})

    def test_replay_always_builds_and_returns_zero_for_match(self):
        args = mock.Mock(
            baseline=Path("/baseline"), nmm_repo=Path("/nmm"),
            dsp_repo=Path("/noise_generator_dsp"), ios_repo=Path("/ios")
        )
        inventory = {"presets": [{"preset_path": "presets/showcase_white.json", "report_path": "evaluations/showcase_white.json"}]}
        completed = mock.Mock(returncode=0, stdout="")
        with mock.patch.object(stage1, "verify_baseline"), \
             mock.patch.object(stage1, "load_json", side_effect=[valid_manifest(), inventory]), \
             mock.patch.object(stage1, "drift_summary", return_value=[]), \
             mock.patch.object(stage1, "run", return_value=completed) as run_mock, \
             mock.patch.object(stage1, "sha256_file", return_value="sha256:same"):
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(stage1.replay(args), 0)
        self.assertEqual(run_mock.call_args_list[0].args[0][:3], ["cargo", "build", "--locked"])

    def test_replay_returns_two_after_collecting_mismatch(self):
        args = mock.Mock(
            baseline=Path("/baseline"), nmm_repo=Path("/nmm"),
            dsp_repo=Path("/noise_generator_dsp"), ios_repo=Path("/ios")
        )
        inventory = {"presets": [{"preset_path": "presets/showcase_white.json", "report_path": "evaluations/showcase_white.json"}]}
        completed = mock.Mock(returncode=0, stdout="")
        with mock.patch.object(stage1, "verify_baseline"), \
             mock.patch.object(stage1, "load_json", side_effect=[valid_manifest(), inventory]), \
             mock.patch.object(stage1, "drift_summary", return_value=[]), \
             mock.patch.object(stage1, "run", return_value=completed), \
             mock.patch.object(stage1, "sha256_file", side_effect=["sha256:new", "sha256:old"]):
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(stage1.replay(args), 2)


if __name__ == "__main__":
    unittest.main()
