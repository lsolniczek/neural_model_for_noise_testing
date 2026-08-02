#!/usr/bin/env python3
"""Capture, verify, and replay the legacy pre-parity NMM baseline."""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import platform
import re
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable


SCHEMA_VERSION = 2
BASELINE_ID = "stage1_legacy_pre_parity"
EXPECTED_HEADS = {
    "nmm": "eb46171725c706ef2c989956c390c336b2aeebb3",
    "dsp": "40b01c88da91d5985466d1e7e926c89dccc884b2",
    "ios": "89f70e23f2e3398e5d6363e67caddb5f0f87990b",
}
EXPECTED_NMM_FAILURES = {
    "disturb::tests::disturb_canonical_golden_snapshot_reference_case",
    "disturb::tests::disturb_legacy_ablated_golden_snapshot_reference_case",
}
EXPECTED_NMM_SUMMARY = {"passed": 511, "failed": 2, "ignored": 5}
EXPECTED_DSP_SUMMARY = {"passed": 667, "failed": 0, "ignored": 2}
PROFILE = {
    "id": "compat_smoke_v1",
    "goal": "focus",
    "brain_type": "normal",
    "duration_secs": 2.1,
    "assr": False,
    "thalamic_gate": False,
    "cet": False,
    "physiological_thalamic_gate": False,
    "arousal_model": "legacy_heuristic",
    "acoustic_scoring": False,
    "acoustic_score_fusion": False,
}
RENDERER_OBSERVATION = {
    "nmm_legacy_path": {
        "sample_rate_hz": 48000,
        "engine_constructor_master_gain": 0.8,
        "dsp_default_room_mode": "legacy",
        "dsp_default_reverb_mode": "fdn",
        "dsp_default_crossfeed_enabled": False,
        "preset_application_order": ["acoustic_environment", "room_mode", "room_geometry", "objects"],
        "dsp_render_warmup_secs": 1.0,
        "model_warmup_discard_secs": 2.0,
        "post_dsp_rir": "enabled for non-anechoic presets except image-source mode",
    },
    "shipping_app_reference": {
        "room_mode": "outdoor",
        "reverb_mode": "sparse_multiband_velvet",
        "crossfeed_enabled": True,
        "crossfeed_strength": 0.4,
    },
    "status": "observed_pre_parity_configuration_not_an_approved_contract",
}
SUMMARY_RE = re.compile(
    r"test result: (?:ok|FAILED)\. (?P<passed>\d+) passed; (?P<failed>\d+) failed; "
    r"(?P<ignored>\d+) ignored;"
)
FAIL_HEADER_RE = re.compile(r"^---- (?P<name>[\w:]+) stdout ----$", re.MULTILINE)


class BaselineError(RuntimeError):
    pass


def run(command: list[str], cwd: Path, check: bool = False) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(
        command,
        cwd=cwd,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        check=False,
    )
    if check and completed.returncode != 0:
        raise BaselineError(
            f"command failed ({completed.returncode}): {' '.join(command)}\n{completed.stdout}"
        )
    return completed


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return f"sha256:{digest.hexdigest()}"


def tree_fingerprint(root: Path, relative_paths: Iterable[Path]) -> dict[str, Any]:
    paths = sorted(set(relative_paths), key=lambda value: value.as_posix())
    digest = hashlib.sha256()
    for relative in paths:
        path = root / relative
        if not path.is_file():
            raise BaselineError(f"fingerprint input missing: {path}")
        digest.update(relative.as_posix().encode("utf-8"))
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    return {"sha256": f"sha256:{digest.hexdigest()}", "file_count": len(paths)}


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n",
        encoding="utf-8",
    )


def reject_json_constant(value: str) -> None:
    raise ValueError(f"invalid JSON numeric constant: {value}")


def load_json(path: Path) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"), parse_constant=reject_json_constant)
    except (OSError, json.JSONDecodeError, ValueError) as exc:
        raise BaselineError(f"invalid JSON file {path}: {exc}") from exc


def git_text(repo: Path, *args: str) -> str:
    return run(["git", *args], repo, check=True).stdout.strip()


def current_repo_metadata(repo: Path) -> dict[str, Any]:
    status = git_text(repo, "status", "--porcelain=v1", "--untracked-files=normal")
    return {
        "path": str(repo.resolve()),
        "head": git_text(repo, "rev-parse", "HEAD"),
        "dirty_paths": [line for line in status.splitlines() if line],
    }


def baseline_repo_metadata(name: str, repo: Path) -> dict[str, Any]:
    metadata = current_repo_metadata(repo)
    if metadata["head"] != EXPECTED_HEADS[name]:
        raise BaselineError(
            f"{name} HEAD changed: expected {EXPECTED_HEADS[name]}, found {metadata['head']}. "
            "Re-review compatibility before recapturing Stage 1."
        )
    return metadata


def tracked_state(repo: Path, relative_path: Path) -> str:
    status = run(
        ["git", "status", "--porcelain=v1", "--untracked-files=all", "--", str(relative_path)],
        repo,
        check=True,
    ).stdout.strip()
    if status.startswith("??"):
        return "untracked"
    if status:
        return "modified"
    return "tracked_clean"


def collect_runtime_paths(nmm: Path, dsp: Path, ios: Path) -> dict[str, list[Path]]:
    nmm_paths = [Path("Cargo.toml"), Path("Cargo.lock"), Path("surrogate_weights_small.bin")]
    nmm_paths.extend(path.relative_to(nmm) for path in (nmm / "src").rglob("*.rs"))
    dsp_paths = [Path("Cargo.toml"), Path("Cargo.lock")]
    for directory in (dsp / "crates" / "core", dsp / "crates" / "signal_core"):
        dsp_paths.extend(
            path.relative_to(dsp)
            for path in directory.rglob("*")
            if path.is_file() and "target" not in path.parts
        )
    ios_paths = ios_evidence_paths()
    return {"nmm": nmm_paths, "dsp": dsp_paths, "ios": ios_paths}


def source_fingerprints(nmm: Path, dsp: Path, ios: Path) -> dict[str, dict[str, Any]]:
    paths = collect_runtime_paths(nmm, dsp, ios)
    return {
        "nmm_runtime": tree_fingerprint(nmm, paths["nmm"]),
        "dsp_runtime": tree_fingerprint(dsp, paths["dsp"]),
        "ios_integration": tree_fingerprint(ios, paths["ios"]),
    }


def validate_repo_paths(nmm: Path, dsp: Path) -> None:
    dependency_path = (nmm / ".." / "noise_generator_dsp").resolve()
    if dsp.resolve() != dependency_path:
        raise BaselineError(
            f"DSP path {dsp.resolve()} does not match the NMM Cargo path dependency {dependency_path}"
        )


def toolchain_metadata() -> dict[str, str]:
    rustc = run(["rustc", "--version", "--verbose"], Path.cwd(), check=True).stdout.strip()
    cargo = run(["cargo", "--version"], Path.cwd(), check=True).stdout.strip()
    return {
        "rustc_verbose": rustc,
        "cargo": cargo,
        "python": platform.python_version(),
        "os": platform.platform(),
        "architecture": platform.machine(),
    }


def discover_presets(preset_dir: Path) -> list[Path]:
    presets = sorted(path for path in preset_dir.rglob("*.json") if path.is_file())
    if len(presets) != 60:
        raise BaselineError(f"expected exactly 60 preset JSON files, found {len(presets)}")
    return presets


def ios_evidence_paths() -> list[Path]:
    base = Path("noise_generator_app")
    providers = base / "model" / "PresetProviders"
    names = [
        "NormalFlow1PresetProvider.swift",
        "NormalIgnition1PresetProvider.swift",
        "NormalRelax1PresetProvider.swift",
        "NormalReset1PresetProvider.swift",
        "NormalShield1PresetProvider.swift",
        "PureColors/BlackPresetProvider.swift",
        "PureColors/BluePresetProvider.swift",
        "PureColors/BrownPresetProvider.swift",
        "PureColors/GreenPresetProvider.swift",
        "PureColors/GreyPresetProvider.swift",
        "PureColors/PinkPresetProvider.swift",
        "PureColors/SSNPresetProvider.swift",
        "PureColors/WhitePresetProvider.swift",
    ]
    return [
        base / "AudioManager.swift",
        base / "Debug" / "JsonPresetModel.swift",
        base / "Debug" / "JsonPresetProvider.swift",
        *[providers / name for name in names],
    ]


def copy_file(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)


def normalized_report_path(relative_preset: Path) -> Path:
    return Path("evaluations") / relative_preset


def snapshot_preset_path(relative_preset: Path) -> Path:
    return Path("inputs") / "presets" / relative_preset


def parse_summary(output: str) -> dict[str, int]:
    matches = [
        {key: int(value) for key, value in match.groupdict().items()}
        for match in SUMMARY_RE.finditer(output)
    ]
    if not matches:
        raise BaselineError(f"could not find a Rust test summary; output tail={output[-1000:]!r}")
    return max(matches, key=lambda item: item["passed"] + item["failed"] + item["ignored"])


def parse_failure_names(output: str) -> set[str]:
    return set(FAIL_HEADER_RE.findall(output))


def assert_nmm_test_baseline(output: str, returncode: int) -> dict[str, int]:
    summary = parse_summary(output)
    failures = parse_failure_names(output)
    if summary != EXPECTED_NMM_SUMMARY or failures != EXPECTED_NMM_FAILURES or returncode != 101:
        raise BaselineError(
            "NMM test baseline changed: "
            f"summary={summary}, failures={sorted(failures)}, exit={returncode}"
        )
    return summary


def assert_dsp_test_baseline(output: str, returncode: int) -> dict[str, int]:
    summary = parse_summary(output)
    if summary != EXPECTED_DSP_SUMMARY or returncode != 0:
        raise BaselineError(f"DSP test baseline changed: summary={summary}, exit={returncode}")
    return summary


def command_for_evaluation(binary: Path, relative_preset: Path, report_path: Path) -> list[str]:
    return [
        str(binary),
        "evaluate",
        str(Path("presets") / relative_preset),
        "--goal",
        PROFILE["goal"],
        "--brain-type",
        PROFILE["brain_type"],
        "--duration",
        str(PROFILE["duration_secs"]),
        "--no-assr",
        "--no-thalamic-gate",
        "--no-cet",
        "--arousal-model",
        PROFILE["arousal_model"],
        "--json-report",
        str(report_path),
    ]


def finite_number(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(float(value))


def assert_all_numbers_finite(value: Any, path: str = "report") -> None:
    if isinstance(value, float) and not math.isfinite(value):
        raise BaselineError(f"non-finite number at {path}")
    if isinstance(value, dict):
        for key, child in value.items():
            assert_all_numbers_finite(child, f"{path}.{key}")
    elif isinstance(value, list):
        for index, child in enumerate(value):
            assert_all_numbers_finite(child, f"{path}[{index}]")


def validate_report(report: dict[str, Any], relative_preset: Path) -> None:
    required = {
        "preset_path", "goal", "brain_type", "score", "practical_status",
        "goal_semantics", "practical_report", "band_powers", "dominant_frequency_hz",
        "fhn_firing_rate", "fhn_isi_cv", "acoustic_summary", "model_signature", "limitations",
    }
    if not isinstance(report, dict) or not required.issubset(report):
        raise BaselineError(f"report fields missing for {relative_preset}")
    assert_all_numbers_finite(report)
    if report["preset_path"] != str(Path("presets") / relative_preset):
        raise BaselineError(f"report preset_path mismatch for {relative_preset}")
    if report["goal"] != PROFILE["goal"] or str(report["brain_type"]).lower() != "normal":
        raise BaselineError(f"report profile mismatch for {relative_preset}")
    if not finite_number(report["score"]):
        raise BaselineError(f"report score is not finite for {relative_preset}")
    band_powers = report["band_powers"]
    if not isinstance(band_powers, dict) or any(
        not finite_number(band_powers.get(name))
        for name in ("delta", "theta", "alpha", "beta", "gamma")
    ):
        raise BaselineError(f"report band powers invalid for {relative_preset}")
    for name in ("dominant_frequency_hz", "fhn_firing_rate"):
        if not finite_number(report[name]):
            raise BaselineError(f"report {name} invalid for {relative_preset}")
    if report["fhn_isi_cv"] is not None and not finite_number(report["fhn_isi_cv"]):
        raise BaselineError(f"report fhn_isi_cv invalid for {relative_preset}")
    if not isinstance(report["limitations"], list) or not isinstance(report["practical_status"], str):
        raise BaselineError(f"report interpretation fields invalid for {relative_preset}")
    signature = report["model_signature"]
    if not isinstance(signature, dict) or signature.get("duration_secs") != PROFILE["duration_secs"]:
        raise BaselineError(f"report signature invalid for {relative_preset}")
    flags = signature.get("auditory_flags")
    expected_flags = {
        "assr_enabled": False,
        "thalamic_gate_enabled": False,
        "physiological_thalamic_gate_enabled": False,
        "cet_enabled": False,
        "acoustic_scoring_enabled": False,
        "acoustic_score_fusion_enabled": False,
    }
    if not isinstance(flags, dict) or any(flags.get(key) != expected for key, expected in expected_flags.items()):
        raise BaselineError(f"report feature flags mismatch for {relative_preset}")


def candidate_specs() -> list[tuple[str, list[str], str]]:
    return [
        ("NormalFlow1PresetProvider.swift", ["the_flow_v4.json"], "Flow v4 is likely, but carriers, levels, and positions differ."),
        ("NormalIgnition1PresetProvider.swift", ["normal_set_ignition_v3.json"], "Both identify as Ignition v3; the JSON has seven objects."),
        ("NormalRelax1PresetProvider.swift", ["unwind/relax_v1.json", "normal_set_deep_relax.json"], "Relaxation intent only; no canonical identity is asserted."),
        ("NormalReset1PresetProvider.swift", ["normal_set_reset.json", "unwind/reset_v1.json"], "Reset intent only; no canonical identity is asserted."),
        ("NormalShield1PresetProvider.swift", ["normal_set_shield_v6.json"], "Known room-coordinate and renderer mismatch."),
        *[
            (f"PureColors/{colour}PresetProvider.swift", [f"showcase_{colour.lower()}.json"], "Colour/name counterpart only; renderer settings differ.")
            for colour in ("Black", "Blue", "Brown", "Green", "Grey", "Pink", "SSN", "White")
        ],
    ]


def build_candidate_mapping(staging: Path) -> list[dict[str, Any]]:
    provider_root = Path("noise_generator_app/model/PresetProviders")
    result = []
    for provider, candidates, note in candidate_specs():
        original_provider = provider_root / provider
        provider_snapshot = Path("inputs/source-evidence/ios") / original_provider
        candidate_entries = []
        for candidate in candidates:
            snapshot = snapshot_preset_path(Path(candidate))
            candidate_entries.append(
                {
                    "preset_path": str(Path("presets") / candidate),
                    "snapshot_path": str(snapshot),
                    "sha256": sha256_file(staging / snapshot),
                }
            )
        result.append(
            {
                "provider_path": str(original_provider),
                "provider_snapshot_path": str(provider_snapshot),
                "provider_sha256": sha256_file(staging / provider_snapshot),
                "candidate_presets": candidate_entries,
                "relationship_status": "candidate_only",
                "evidence": "provider and preset naming/version review",
                "known_mismatches": note,
            }
        )
    return result


def artifact_hashes(root: Path) -> dict[str, str]:
    return {
        str(path.relative_to(root)): sha256_file(path)
        for path in sorted(root.rglob("*"))
        if path.is_file() and path.name != "manifest.json"
    }


def write_readme(root: Path) -> None:
    (root / "README.md").write_text(
        "# Stage 1 legacy baseline\n\n"
        "This self-contained snapshot records pre-renderer-parity NMM behavior. It is a "
        "regression reference, not a production-equivalent renderer baseline and not evidence "
        "of human efficacy. Replay always rebuilds current NMM/DSP code and evaluates the frozen "
        "preset inputs under `inputs/presets`.\n\n"
        "Verify offline: `python3 tools/compatibility/stage1_baseline.py verify --baseline "
        "baselines/compatibility/stage1_legacy_pre_parity`\n\n"
        "Replay current code: `python3 tools/compatibility/stage1_baseline.py replay --baseline "
        "baselines/compatibility/stage1_legacy_pre_parity --nmm-repo . "
        "--dsp-repo ../noise_generator_dsp --ios-repo ../noise_generator_ios_app`\n",
        encoding="utf-8",
    )


def capture(args: argparse.Namespace) -> None:
    nmm, dsp, ios = args.nmm_repo.resolve(), args.dsp_repo.resolve(), args.ios_repo.resolve()
    output = args.output.resolve()
    if output.exists():
        raise BaselineError(f"refusing to overwrite existing baseline: {output}")
    if not (nmm / "Cargo.toml").is_file() or not (dsp / "Cargo.toml").is_file() or not ios.is_dir():
        raise BaselineError("NMM, DSP, or iOS repository path is invalid")
    validate_repo_paths(nmm, dsp)
    repositories = {
        "nmm": baseline_repo_metadata("nmm", nmm),
        "dsp": baseline_repo_metadata("dsp", dsp),
        "ios": baseline_repo_metadata("ios", ios),
    }
    fingerprints = source_fingerprints(nmm, dsp, ios)
    preset_dir = nmm / "presets"
    presets = discover_presets(preset_dir)
    output.parent.mkdir(parents=True, exist_ok=True)
    staging = Path(tempfile.mkdtemp(prefix=f".{output.name}.", dir=output.parent))
    try:
        relative_presets = [path.relative_to(preset_dir) for path in presets]
        for preset, relative in zip(presets, relative_presets):
            copy_file(preset, staging / snapshot_preset_path(relative))
        for relative in ios_evidence_paths():
            copy_file(ios / relative, staging / "inputs" / "source-evidence" / "ios" / relative)

        build = run(["cargo", "build", "--locked", "--bin", "neural_preset_optimizer"], nmm)
        (staging / "test-results").mkdir(parents=True)
        (staging / "test-results" / "build.log").write_text(build.stdout, encoding="utf-8")
        if build.returncode != 0:
            raise BaselineError(f"NMM build failed; output tail={build.stdout[-1000:]!r}")
        nmm_tests = run(["cargo", "test", "--locked", "--all-targets"], nmm)
        (staging / "test-results" / "nmm.log").write_text(nmm_tests.stdout, encoding="utf-8")
        nmm_summary = assert_nmm_test_baseline(nmm_tests.stdout, nmm_tests.returncode)
        dsp_tests = run(["cargo", "test", "--locked", "-p", "noise_generator_core", "--no-default-features"], dsp)
        (staging / "test-results" / "dsp.log").write_text(dsp_tests.stdout, encoding="utf-8")
        dsp_summary = assert_dsp_test_baseline(dsp_tests.stdout, dsp_tests.returncode)

        binary = nmm / "target" / "debug" / "neural_preset_optimizer"
        inventory = []
        for live_preset, relative in zip(presets, relative_presets):
            report_rel = normalized_report_path(relative)
            report_path = staging / report_rel
            report_path.parent.mkdir(parents=True, exist_ok=True)
            evaluation = run(command_for_evaluation(binary, relative, report_path), staging / "inputs")
            if evaluation.returncode != 0:
                raise BaselineError(f"evaluation failed for {relative}:\n{evaluation.stdout}")
            report = load_json(report_path)
            validate_report(report, relative)
            snapshot = staging / snapshot_preset_path(relative)
            preset_json = load_json(snapshot)
            inventory.append(
                {
                    "preset_path": str(Path("presets") / relative),
                    "snapshot_path": str(snapshot_preset_path(relative)),
                    "sha256": sha256_file(snapshot),
                    "capture_git_state": tracked_state(nmm, Path("presets") / relative),
                    "object_count": len(preset_json.get("objects", [])),
                    "source_count": preset_json.get("source_count"),
                    "report_path": str(report_rel),
                }
            )
            if sha256_file(live_preset) != sha256_file(snapshot):
                raise BaselineError(f"preset changed during capture: {relative}")

        write_json(staging / "preset_inventory.json", {"count": len(inventory), "presets": inventory})
        write_json(staging / "shipping_preset_candidates.json", {"providers": build_candidate_mapping(staging)})
        write_readme(staging)
        manifest = {
            "schema_version": SCHEMA_VERSION,
            "baseline_id": BASELINE_ID,
            "captured_at_utc": datetime.now(timezone.utc).isoformat(),
            "warning": "Legacy pre-renderer-parity regression reference; not production-equivalent.",
            "repositories": repositories,
            "source_fingerprints": fingerprints,
            "toolchain": toolchain_metadata(),
            "capture_tool_sha256": sha256_file(Path(__file__).resolve()),
            "renderer_observation": RENDERER_OBSERVATION,
            "evaluation_profile": PROFILE,
            "preset_corpus": {
                "root": "inputs/presets",
                "count": len(inventory),
                "inventory": "preset_inventory.json",
            },
            "tests": {
                "nmm": {
                    "command": ["cargo", "test", "--locked", "--all-targets"],
                    "exit_code": nmm_tests.returncode,
                    "summary": nmm_summary,
                    "failures": sorted(EXPECTED_NMM_FAILURES),
                },
                "dsp": {
                    "command": ["cargo", "test", "--locked", "-p", "noise_generator_core", "--no-default-features"],
                    "exit_code": dsp_tests.returncode,
                    "summary": dsp_summary,
                },
            },
            "evaluation_result": {"attempted": 60, "succeeded": 60, "report_root": "evaluations"},
            "artifact_hashes": artifact_hashes(staging),
        }
        write_json(staging / "manifest.json", manifest)
        verify_baseline(staging)
        staging.rename(output)
    except Exception:
        shutil.rmtree(staging, ignore_errors=True)
        raise


def validate_manifest(manifest: dict[str, Any]) -> None:
    required = {
        "schema_version", "baseline_id", "captured_at_utc", "warning", "repositories",
        "source_fingerprints", "toolchain", "capture_tool_sha256", "renderer_observation",
        "evaluation_profile", "preset_corpus", "tests", "evaluation_result", "artifact_hashes",
    }
    if not isinstance(manifest, dict) or not required.issubset(manifest):
        raise BaselineError("baseline manifest fields missing")
    if manifest["schema_version"] != SCHEMA_VERSION or manifest["baseline_id"] != BASELINE_ID:
        raise BaselineError("unsupported baseline manifest")
    for name, expected_head in EXPECTED_HEADS.items():
        repo = manifest["repositories"].get(name)
        if not isinstance(repo, dict) or repo.get("head") != expected_head:
            raise BaselineError(f"recorded {name} revision is invalid")
    fingerprints = manifest["source_fingerprints"]
    for name in ("nmm_runtime", "dsp_runtime", "ios_integration"):
        fingerprint = fingerprints.get(name) if isinstance(fingerprints, dict) else None
        if (
            not isinstance(fingerprint, dict)
            or not str(fingerprint.get("sha256", "")).startswith("sha256:")
            or not isinstance(fingerprint.get("file_count"), int)
            or fingerprint["file_count"] <= 0
        ):
            raise BaselineError(f"source fingerprint is incomplete: {name}")
    toolchain = manifest["toolchain"]
    if not isinstance(toolchain, dict) or any(not toolchain.get(key) for key in ("rustc_verbose", "cargo", "python", "os", "architecture")):
        raise BaselineError("toolchain provenance is incomplete")
    if manifest["renderer_observation"] != RENDERER_OBSERVATION:
        raise BaselineError("renderer observation is missing or changed")
    if manifest["evaluation_profile"] != PROFILE:
        raise BaselineError("evaluation profile is missing or changed")
    corpus = manifest["preset_corpus"]
    if corpus.get("root") != "inputs/presets" or corpus.get("count") != 60:
        raise BaselineError("preset corpus metadata is invalid")
    result = manifest["evaluation_result"]
    if result.get("attempted") != 60 or result.get("succeeded") != 60:
        raise BaselineError("evaluation result metadata is invalid")
    if manifest["tests"].get("nmm", {}).get("summary") != EXPECTED_NMM_SUMMARY:
        raise BaselineError("recorded NMM test summary is invalid")
    if set(manifest["tests"].get("nmm", {}).get("failures", [])) != EXPECTED_NMM_FAILURES:
        raise BaselineError("recorded NMM failure set is invalid")
    if manifest["tests"].get("nmm", {}).get("exit_code") != 101:
        raise BaselineError("recorded NMM exit code is invalid")
    if manifest["tests"].get("dsp", {}).get("summary") != EXPECTED_DSP_SUMMARY or manifest["tests"].get("dsp", {}).get("exit_code") != 0:
        raise BaselineError("recorded DSP result is invalid")


def verify_baseline(root: Path) -> None:
    manifest = load_json(root / "manifest.json")
    validate_manifest(manifest)
    inventory_data = load_json(root / "preset_inventory.json")
    inventory = inventory_data.get("presets")
    if inventory_data.get("count") != 60 or not isinstance(inventory, list) or len(inventory) != 60:
        raise BaselineError("baseline inventory must contain exactly 60 presets")
    seen_presets, seen_reports, signatures = set(), set(), set()
    for item in inventory:
        preset_path, snapshot_path, report_path = item.get("preset_path"), item.get("snapshot_path"), item.get("report_path")
        if not all(isinstance(value, str) for value in (preset_path, snapshot_path, report_path)):
            raise BaselineError("invalid inventory entry")
        if preset_path in seen_presets or report_path in seen_reports:
            raise BaselineError("duplicate preset or report inventory entry")
        seen_presets.add(preset_path)
        seen_reports.add(report_path)
        snapshot = root / snapshot_path
        if not snapshot.is_file() or sha256_file(snapshot) != item.get("sha256"):
            raise BaselineError(f"frozen preset hash mismatch: {preset_path}")
        relative = Path(preset_path).relative_to("presets")
        if Path(snapshot_path) != snapshot_preset_path(relative):
            raise BaselineError(f"frozen preset path mismatch: {preset_path}")
        report = load_json(root / report_path)
        validate_report(report, relative)
        signatures.add(json.dumps(report["model_signature"], sort_keys=True, separators=(",", ":")))
    if len(signatures) != 1:
        raise BaselineError("evaluation reports do not share one model signature")

    mapping = load_json(root / "shipping_preset_candidates.json").get("providers")
    if not isinstance(mapping, list) or len(mapping) != 13:
        raise BaselineError("shipping mapping must contain exactly 13 providers")
    for entry in mapping:
        if entry.get("relationship_status") != "candidate_only":
            raise BaselineError("Stage 1 must not assert preset equivalence")
        provider = root / entry.get("provider_snapshot_path", "")
        if not provider.is_file() or sha256_file(provider) != entry.get("provider_sha256"):
            raise BaselineError(f"provider snapshot mismatch: {entry.get('provider_path')}")
        candidates = entry.get("candidate_presets")
        if not isinstance(candidates, list) or not candidates:
            raise BaselineError("candidate mapping is empty")
        for candidate in candidates:
            snapshot = root / candidate.get("snapshot_path", "")
            if not snapshot.is_file() or sha256_file(snapshot) != candidate.get("sha256"):
                raise BaselineError(f"candidate preset snapshot mismatch: {candidate.get('preset_path')}")
    if manifest["artifact_hashes"] != artifact_hashes(root):
        raise BaselineError("artifact hashes do not match manifest")


def drift_summary(manifest: dict[str, Any], nmm: Path, dsp: Path, ios: Path) -> list[str]:
    current_repos = {
        "nmm": current_repo_metadata(nmm),
        "dsp": current_repo_metadata(dsp),
        "ios": current_repo_metadata(ios),
    }
    current_fingerprints = source_fingerprints(nmm, dsp, ios)
    current_toolchain = toolchain_metadata()
    lines = []
    for name in ("nmm", "dsp", "ios"):
        old, new = manifest["repositories"][name]["head"], current_repos[name]["head"]
        lines.append(f"{name} revision: {'same' if old == new else f'drift {old[:12]} -> {new[:12]}'}")
    for name in ("nmm_runtime", "dsp_runtime", "ios_integration"):
        old, new = manifest["source_fingerprints"][name]["sha256"], current_fingerprints[name]["sha256"]
        lines.append(f"{name} fingerprint: {'same' if old == new else 'drift'}")
    lines.append(f"Rust toolchain: {'same' if manifest['toolchain']['rustc_verbose'] == current_toolchain['rustc_verbose'] else 'drift'}")
    return lines


def replay(args: argparse.Namespace) -> int:
    baseline = args.baseline.resolve()
    nmm, dsp, ios = args.nmm_repo.resolve(), args.dsp_repo.resolve(), args.ios_repo.resolve()
    validate_repo_paths(nmm, dsp)
    verify_baseline(baseline)
    manifest = load_json(baseline / "manifest.json")
    for line in drift_summary(manifest, nmm, dsp, ios):
        print(line)
    build = run(["cargo", "build", "--locked", "--bin", "neural_preset_optimizer"], nmm)
    if build.returncode != 0:
        raise BaselineError(f"could not rebuild NMM binary; output tail={build.stdout[-1000:]!r}")
    binary = nmm / "target" / "debug" / "neural_preset_optimizer"
    inventory = load_json(baseline / "preset_inventory.json")["presets"]
    mismatches = []
    with tempfile.TemporaryDirectory(prefix="nmm-stage1-replay-") as temp_dir:
        temp = Path(temp_dir)
        for item in inventory:
            relative = Path(item["preset_path"]).relative_to("presets")
            fresh = temp / item["report_path"]
            fresh.parent.mkdir(parents=True, exist_ok=True)
            result = run(command_for_evaluation(binary, relative, fresh), baseline / "inputs")
            if result.returncode != 0:
                raise BaselineError(f"replay evaluation failed for {relative}:\n{result.stdout}")
            if sha256_file(fresh) != sha256_file(baseline / item["report_path"]):
                mismatches.append(str(relative))
    if mismatches:
        print(f"Replay mismatches ({len(mismatches)}/60):", file=sys.stderr)
        for mismatch in mismatches:
            print(f"  {mismatch}", file=sys.stderr)
        return 2
    print("Replay matched 60/60 reports byte-for-byte.")
    return 0


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    repos = argparse.ArgumentParser(add_help=False)
    repos.add_argument("--nmm-repo", type=Path, default=Path("."))
    repos.add_argument("--dsp-repo", type=Path, default=Path("../noise_generator_dsp"))
    repos.add_argument("--ios-repo", type=Path, default=Path("../noise_generator_ios_app"))
    capture_parser = subparsers.add_parser("capture", parents=[repos])
    capture_parser.add_argument("--output", type=Path, required=True)
    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("--baseline", type=Path, required=True)
    replay_parser = subparsers.add_parser("replay", parents=[repos])
    replay_parser.add_argument("--baseline", type=Path, required=True)
    return parser


def main(argv: Iterable[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    try:
        if args.command == "capture":
            capture(args)
            return 0
        if args.command == "verify":
            verify_baseline(args.baseline.resolve())
            return 0
        return replay(args)
    except BaselineError as exc:
        print(f"stage1 baseline: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
