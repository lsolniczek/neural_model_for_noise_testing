import contextlib
import copy
import importlib.util
import io
import shutil
import tempfile
import unittest
from pathlib import Path
from unittest import mock


MODULE_PATH = Path(__file__).parents[2] / "tools" / "compatibility" / "stage1_baseline.py"
REPO_ROOT = MODULE_PATH.parents[2]
HISTORICAL_BASELINE = REPO_ROOT / "baselines" / "compatibility" / "stage1_legacy_pre_parity"
SPEC = importlib.util.spec_from_file_location("stage1_baseline", MODULE_PATH)
stage1 = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
SPEC.loader.exec_module(stage1)


def write(path: Path, text: str = "") -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def valid_report(profile: dict) -> dict:
    return {
        "preset_path": "presets/showcase_white.json", "goal": profile["goal"],
        "brain_type": "Normal", "score": 0.5, "practical_status": "usable",
        "goal_semantics": {}, "practical_report": {},
        "band_powers": {"delta": 0.1, "theta": 0.2, "alpha": 0.3, "beta": 0.3, "gamma": 0.1},
        "dominant_frequency_hz": 10.0, "fhn_firing_rate": 12.0, "fhn_isi_cv": None,
        "acoustic_summary": {}, "limitations": ["proxy"],
        "model_signature": {
            "version": "legacy_v1", "pipeline_variant": "evaluate_canonical",
            "duration_secs": profile["duration_secs"], "warmup_discard_secs": profile["warmup_discard_secs"],
            "auditory_flags": {
                "assr_enabled": profile["assr"], "thalamic_gate_enabled": profile["thalamic_gate"],
                "physiological_thalamic_gate_enabled": profile["physiological_thalamic_gate"],
                "cet_enabled": profile["cet"], "acoustic_scoring_enabled": profile["acoustic_scoring"],
                "acoustic_score_fusion_enabled": profile["acoustic_score_fusion"], "arousal_model": profile["arousal_model"],
            },
        },
    }


def valid_v3_manifest() -> dict:
    green = {"summary": {"passed": 1, "failed": 0, "ignored": 0}, "failures": [], "exit_code": 0}
    digest = "sha256:" + "a" * 64
    return {
        "schema_version": stage1.SCHEMA_VERSION,
        "baseline_id": "stage1_green_example",
        "captured_at_utc": "now",
        "warning": "regression reference",
        "repositories": {name: {"head": "a" * 40} for name in ("nmm", "dsp", "ios")},
        "source_fingerprints": {},
        "toolchain": {},
        "capture_tool_sha256": digest,
        "binary_sha256": digest,
        "renderer_observation": stage1.RENDERER_OBSERVATION,
        "evaluation_profiles": stage1.PROFILES,
        "preset_corpus": {"root": "inputs/presets", "count": 60},
        "tests": {"nmm": copy.deepcopy(green), "dsp": copy.deepcopy(green)},
        "evaluation_result": {"attempted": 120, "succeeded": 120, "report_root": "evaluations"},
        "capture_state": {"pre_sha256": digest, "post_sha256": digest, "capture_state_stable": True},
        "artifact_hashes": {},
    }


class Stage1BaselineTests(unittest.TestCase):
    def test_profiles_are_complete_and_regression_window_is_meaningful(self):
        self.assertEqual(tuple(stage1.PROFILES), stage1.PROFILE_IDS)
        for profile in stage1.PROFILES.values():
            stage1.validate_profile(profile)
        self.assertEqual(stage1.PROFILES["compat_regression_v1"]["analysis_duration_secs"], 10.0)

    def test_smoke_command_is_legacy_ablated_and_regression_preserves_standard_flags(self):
        binary, preset, report = Path("/binary"), Path("showcase_white.json"), Path("/report.json")
        smoke = stage1.command_for_evaluation(binary, preset, report, stage1.PROFILES["compat_smoke_v1"])
        regression = stage1.command_for_evaluation(binary, preset, report, stage1.PROFILES["compat_regression_v1"])
        self.assertIn("--no-assr", smoke)
        self.assertIn("--no-thalamic-gate", smoke)
        self.assertIn("--no-cet", smoke)
        self.assertIn("--no-assr", regression)
        self.assertNotIn("--no-thalamic-gate", regression)
        self.assertNotIn("--no-cet", regression)

    def test_snapshot_and_report_paths_preserve_nesting(self):
        relative = Path("unwind/calm_v1.json")
        self.assertEqual(stage1.snapshot_preset_path(relative), Path("inputs/presets/unwind/calm_v1.json"))
        self.assertEqual(stage1.normalized_report_path("compat_regression_v1", relative), Path("evaluations/compat_regression_v1/unwind/calm_v1.json"))

    def test_safe_relative_rejects_absolute_and_parent_paths(self):
        for path in ("../outside.json", "/tmp/outside.json", ""):
            with self.assertRaises(stage1.BaselineError):
                stage1.safe_relative(path, "test")
        self.assertEqual(stage1.safe_relative("evaluations/a.json", "test"), Path("evaluations/a.json"))

    def test_parse_summary_selects_largest_suite(self):
        output = "test result: ok. 0 passed; 0 failed; 0 ignored;\n" "test result: FAILED. 511 passed; 2 failed; 5 ignored;\n"
        self.assertEqual(stage1.parse_summary(output), {"passed": 511, "failed": 2, "ignored": 5})

    def test_current_test_contract_requires_green_without_fixed_pass_count(self):
        first = "test result: ok. 511 passed; 0 failed; 5 ignored;\n"
        second = "test result: ok. 900 passed; 0 failed; 6 ignored;\n"
        self.assertEqual(stage1.assert_green_test_run(first, 0, "NMM")["passed"], 511)
        self.assertEqual(stage1.assert_green_test_run(second, 0, "NMM")["passed"], 900)
        failed = "---- new::failure stdout ----\ntest result: FAILED. 511 passed; 1 failed; 5 ignored;\n"
        with self.assertRaises(stage1.BaselineError):
            stage1.assert_green_test_run(failed, 101, "NMM")

    def test_validate_report_rejects_profile_and_nonfinite_mismatch(self):
        profile = stage1.PROFILES["compat_regression_v1"]
        report = valid_report(profile)
        stage1.validate_report(report, Path("showcase_white.json"), profile)
        report["model_signature"]["duration_secs"] = 2.1
        with self.assertRaises(stage1.BaselineError):
            stage1.validate_report(report, Path("showcase_white.json"), profile)

    def test_write_json_rejects_nan(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ValueError):
                stage1.write_json(Path(directory) / "bad.json", {"value": float("nan")})

    def test_manifest_rejects_unstable_capture(self):
        manifest = valid_v3_manifest()
        manifest["capture_state"]["post_sha256"] = "sha256:" + "d" * 64
        manifest["capture_state"]["capture_state_stable"] = False
        with self.assertRaises(stage1.BaselineError):
            stage1.validate_manifest(manifest)

    def test_schema_v3_rejects_historical_red_tests(self):
        manifest = valid_v3_manifest()
        manifest["tests"]["nmm"] = {
            "summary": {"passed": 511, "failed": 2, "ignored": 5},
            "failures": ["old::failure_one", "old::failure_two"],
            "exit_code": 101,
        }
        with self.assertRaisesRegex(stage1.BaselineError, "non-green NMM"):
            stage1.validate_manifest(manifest)

    def test_schema_v2_manifest_and_inventory_are_adapted_in_memory(self):
        raw_manifest = stage1.load_json(HISTORICAL_BASELINE / "manifest.json")
        raw_inventory = stage1.load_json(HISTORICAL_BASELINE / "preset_inventory.json")
        manifest = stage1.validate_manifest(raw_manifest)
        inventory = stage1.normalize_inventory(
            raw_inventory, manifest["evaluation_profiles"], manifest["_source_schema_version"]
        )
        self.assertEqual(manifest["schema_version"], 3)
        self.assertEqual(manifest["_source_schema_version"], 2)
        self.assertEqual(list(manifest["evaluation_profiles"]), ["compat_smoke_v1"])
        self.assertEqual(inventory["profiles"], ["compat_smoke_v1"])
        self.assertEqual(
            inventory["presets"][0]["reports"],
            {"compat_smoke_v1": "evaluations/deep_relax_phys_cet_v1.json"},
        )

    def test_historical_schema_v2_baseline_integrity_is_valid(self):
        manifest = stage1.verify_baseline(HISTORICAL_BASELINE)
        self.assertEqual(manifest["_source_schema_version"], 2)
        self.assertEqual(manifest["tests"]["nmm"]["exit_code"], 101)

    def test_historical_baseline_tampering_is_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            copied = Path(directory) / "baseline"
            shutil.copytree(HISTORICAL_BASELINE, copied)
            path = copied / "inputs" / "presets" / "showcase_white.json"
            path.write_text(path.read_text(encoding="utf-8") + "\n", encoding="utf-8")
            with self.assertRaises(stage1.BaselineError):
                stage1.verify_baseline(copied)

    def test_capture_cli_requires_explicit_baseline_id(self):
        parser = stage1.build_parser()
        with contextlib.redirect_stderr(io.StringIO()), self.assertRaises(SystemExit):
            parser.parse_args(["capture", "--output", "/tmp/baseline"])
        args = parser.parse_args(
            ["capture", "--output", "/tmp/baseline", "--baseline-id", "p01_current_v1"]
        )
        self.assertEqual(args.baseline_id, "p01_current_v1")

    def test_capture_state_difference_names_the_changed_runtime_group(self):
        before = {"repositories": {name: {"head": "a"} for name in ("nmm", "dsp", "ios")}, "source_inventory": {"dsp_runtime": {"files": {"crates/core/src/lib.rs": "sha256:old"}}}, "preset_hashes": {}, "ios_evidence_hashes": {}}
        after = {"repositories": {name: {"head": "a"} for name in ("nmm", "dsp", "ios")}, "source_inventory": {"dsp_runtime": {"files": {"crates/core/src/lib.rs": "sha256:new"}}}, "preset_hashes": {}, "ios_evidence_hashes": {}}
        self.assertEqual(stage1.capture_state_differences(before, after), ["source_inventory:dsp_runtime:crates/core/src/lib.rs"])

    def test_manifest_dependency_walker_covers_all_dsp_path_crates(self):
        with tempfile.TemporaryDirectory() as directory:
            dsp = Path(directory)
            for package, dependencies in {
                "crates/core": ['engine = { path = "../engine_shared" }', 'signal = { path = "../signal_core" }', 'spatial = { path = "../spatial_core" }'],
                "crates/engine_shared": [], "crates/signal_core": [], "crates/spatial_core": [],
            }.items():
                write(dsp / package / "Cargo.toml", "\n".join(dependencies))
            roots = stage1.dependency_paths_from_manifests(dsp)
            self.assertEqual(roots, {Path("crates/core"), Path("crates/engine_shared"), Path("crates/signal_core"), Path("crates/spatial_core")})

    def test_file_inventory_changes_when_dependency_file_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            write(root / "crates/engine_shared/src/lib.rs", "first")
            before = stage1.fingerprint(stage1.file_hash_inventory(root, [Path("crates/engine_shared/src/lib.rs")]))
            write(root / "crates/engine_shared/src/lib.rs", "second")
            after = stage1.fingerprint(stage1.file_hash_inventory(root, [Path("crates/engine_shared/src/lib.rs")]))
            self.assertNotEqual(before, after)

    def test_swift_registry_classifies_active_inactive_and_provider_only(self):
        with tempfile.TemporaryDirectory() as directory:
            root, staging = Path(directory) / "ios", Path(directory) / "baseline"
            active = '''enum ActivePreset: String {\n case white\n case ssn\n}\nextension ActivePreset {\n var isActive: Bool { switch self { case .ssn: return false\n case .white: return true } }\n var compositionType: Int { 0 }\n var presetProvider: NoisePresetProvider { switch self { case .white: WhitePresetProvider()\n case .ssn: SSNPresetProvider() } }\n}'''
            write(root / "noise_generator_app/model/ActivePreset.swift", active)
            for provider in ("WhitePresetProvider", "SSNPresetProvider", "BluePresetProvider"):
                write(root / f"noise_generator_app/model/PresetProviders/{provider}.swift", f"struct {provider}: NoisePresetProvider {{}}")
            for preset in ("showcase_white.json", "showcase_ssn.json", "showcase_blue.json"):
                write(staging / "inputs/presets" / preset, "{}")
            for source in (root / "noise_generator_app/model/PresetProviders").rglob("*.swift"):
                target = staging / "inputs/source-evidence/ios" / source.relative_to(root)
                write(target, source.read_text(encoding="utf-8"))
            registry = stage1.swift_registry(root, staging)
            states = {entry["provider"]: entry["shipping_status"] for entry in registry["providers"]}
            self.assertEqual(states, {"BluePresetProvider": "provider_only", "SSNPresetProvider": "inactive", "WhitePresetProvider": "active"})

    def test_replay_uses_frozen_inputs_and_validates_all_profiles(self):
        args = mock.Mock(baseline=Path("/baseline"), nmm_repo=Path("/nmm"), dsp_repo=Path("/noise_generator_dsp"), ios_repo=Path("/ios"), profile="all", with_tests=False)
        inventory = {"profiles": list(stage1.PROFILE_IDS), "presets": [{"preset_path": "presets/showcase_white.json", "reports": {profile: f"evaluations/{profile}/showcase_white.json" for profile in stage1.PROFILE_IDS}}]}
        completed = mock.Mock(returncode=0, stdout="")
        reports = [valid_report(stage1.PROFILES[profile]) for profile in stage1.PROFILE_IDS]
        manifest = {"evaluation_profiles": stage1.PROFILES, "_source_schema_version": 3}
        with mock.patch.object(stage1, "verify_baseline", return_value=manifest), mock.patch.object(stage1, "drift_summary", return_value=[]), mock.patch.object(stage1, "load_json", side_effect=[inventory, *reports]), mock.patch.object(stage1, "run", return_value=completed) as run_mock, mock.patch.object(stage1, "validate_report"), mock.patch.object(stage1, "sha256_file", return_value="sha256:same"), mock.patch.object(stage1, "path_under", side_effect=lambda root, value, label: Path(root) / value):
            with contextlib.redirect_stdout(io.StringIO()):
                self.assertEqual(stage1.replay(args), 0)
        evaluation_calls = run_mock.call_args_list[1:]
        self.assertEqual(len(evaluation_calls), 2)
        self.assertTrue(all(call.args[1] == Path("/baseline") / "inputs" for call in evaluation_calls))

    def test_replay_with_tests_uses_green_current_contract(self):
        green = mock.Mock(returncode=0, stdout="test result: ok. 12 passed; 0 failed; 1 ignored;\n")
        failed = mock.Mock(returncode=101, stdout="test result: FAILED. 11 passed; 1 failed; 1 ignored;\n")
        with mock.patch.object(stage1, "run", side_effect=[green, green]):
            stage1.replay_tests(Path("/nmm"), Path("/dsp"))
        with mock.patch.object(stage1, "run", return_value=failed), self.assertRaises(stage1.BaselineError):
            stage1.replay_tests(Path("/nmm"), Path("/dsp"))


if __name__ == "__main__":
    unittest.main()
