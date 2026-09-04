#!/usr/bin/env python3
"""Capture, verify, and replay the Stage 1 pre-renderer-parity baseline."""

from __future__ import annotations

import argparse
import copy
import hashlib
import json
import math
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable


SCHEMA_VERSION = 3
SUPPORTED_SCHEMA_VERSIONS = (2, SCHEMA_VERSION)
BASELINE_ID_RE = re.compile(r"^[a-z0-9][a-z0-9_-]{2,63}$")
PROFILE_IDS = ("compat_smoke_v1", "compat_regression_v1")
PROFILES = {
    "compat_smoke_v1": {
        "id": "compat_smoke_v1", "goal": "focus", "brain_type": "normal",
        "duration_secs": 2.1, "warmup_discard_secs": 2.0, "analysis_duration_secs": 0.1,
        "assr": False, "thalamic_gate": False, "cet": False,
        "physiological_thalamic_gate": False, "arousal_model": "legacy_heuristic",
        "acoustic_scoring": False, "acoustic_score_fusion": False,
    },
    "compat_regression_v1": {
        "id": "compat_regression_v1", "goal": "focus", "brain_type": "normal",
        "duration_secs": 12.0, "warmup_discard_secs": 2.0, "analysis_duration_secs": 10.0,
        "assr": False, "thalamic_gate": True, "cet": True,
        "physiological_thalamic_gate": False, "arousal_model": "legacy_heuristic",
        "acoustic_scoring": False, "acoustic_score_fusion": False,
    },
}
RENDERER_OBSERVATION = {
    "nmm_legacy_path": {
        "sample_rate_hz": 48000, "engine_constructor_master_gain": 0.8,
        "dsp_default_room_mode": "legacy", "dsp_default_reverb_mode": "fdn",
        "dsp_default_crossfeed_enabled": False,
        "preset_application_order": ["acoustic_environment", "room_mode", "room_geometry", "objects"],
        "dsp_render_warmup_secs": 1.0, "model_warmup_discard_secs": 2.0,
        "post_dsp_rir": "enabled for non-anechoic presets except image-source mode",
    },
    "shipping_app_reference": {
        "room_mode": "outdoor", "reverb_mode": "sparse_multiband_velvet",
        "crossfeed_enabled": True, "crossfeed_strength": 0.4,
    },
    "status": "observed_pre_parity_configuration_not_an_approved_contract",
}
SUMMARY_RE = re.compile(r"test result: (?:ok|FAILED)\. (?P<passed>\d+) passed; (?P<failed>\d+) failed; (?P<ignored>\d+) ignored;")
FAIL_HEADER_RE = re.compile(r"^---- (?P<name>[\w:]+) stdout ----$", re.MULTILINE)
SHA256_RE = re.compile(r"^sha256:[0-9a-f]{64}$")
HEAD_RE = re.compile(r"^[0-9a-f]{40}$")
EXCLUDED_PARTS = {".git", "target", "__pycache__"}
EXCLUDED_NAMES = {".DS_Store"}


class BaselineError(RuntimeError):
    pass


def run(command: list[str], cwd: Path, check: bool = False) -> subprocess.CompletedProcess[str]:
    completed = subprocess.run(command, cwd=cwd, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=False)
    if check and completed.returncode != 0:
        raise BaselineError(f"command failed ({completed.returncode}): {' '.join(command)}\n{completed.stdout}")
    return completed


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(block)
    return f"sha256:{digest.hexdigest()}"


def write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True, allow_nan=False) + "\n", encoding="utf-8")


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
    return {"path": str(repo.resolve()), "head": git_text(repo, "rev-parse", "HEAD"), "dirty_paths": [line for line in status.splitlines() if line]}


def state_digest(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode("utf-8")
    return f"sha256:{hashlib.sha256(encoded).hexdigest()}"


def relative_files(root: Path) -> list[Path]:
    return sorted(
        (path.relative_to(root) for path in root.rglob("*") if path.is_file() and not (set(path.relative_to(root).parts) & EXCLUDED_PARTS) and path.name not in EXCLUDED_NAMES),
        key=lambda path: path.as_posix(),
    )


def file_hash_inventory(root: Path, paths: Iterable[Path]) -> dict[str, str]:
    result = {}
    for relative in sorted(set(paths), key=lambda path: path.as_posix()):
        path = root / relative
        if not path.is_file():
            raise BaselineError(f"fingerprint input missing: {path}")
        result[relative.as_posix()] = sha256_file(path)
    return result


def fingerprint(files: dict[str, str]) -> dict[str, Any]:
    return {"sha256": state_digest(files), "file_count": len(files)}


def dependency_paths_from_metadata(nmm: Path, dsp: Path) -> set[Path] | None:
    command = ["cargo", "metadata", "--format-version", "1", "--locked", "--offline", "--manifest-path", str(nmm / "Cargo.toml")]
    completed = run(command, nmm)
    if completed.returncode != 0:
        return None
    data = load_json_text(completed.stdout, "cargo metadata")
    packages = data.get("packages", [])
    roots = set()
    for package in packages:
        manifest = Path(package.get("manifest_path", "")).resolve()
        try:
            manifest.relative_to(dsp.resolve())
        except ValueError:
            continue
        roots.add(manifest.parent.relative_to(dsp.resolve()))
    return roots or None


def load_json_text(text: str, description: str) -> Any:
    try:
        return json.loads(text, parse_constant=reject_json_constant)
    except (json.JSONDecodeError, ValueError) as exc:
        raise BaselineError(f"invalid {description}: {exc}") from exc


def toml_local_paths(manifest: Path) -> list[Path]:
    text = manifest.read_text(encoding="utf-8")
    return [Path(value) for value in re.findall(r"\bpath\s*=\s*\"([^\"]+)\"", text)]


def dependency_paths_from_manifests(dsp: Path) -> set[Path]:
    root = dsp.resolve()
    pending = [root / "crates/core"]
    seen: set[Path] = set()
    while pending:
        package = pending.pop().resolve()
        try:
            relative = package.relative_to(root)
        except ValueError as exc:
            raise BaselineError(f"DSP local dependency outside repository: {package}") from exc
        if relative in seen:
            continue
        manifest = package / "Cargo.toml"
        if not manifest.is_file():
            raise BaselineError(f"DSP package manifest missing: {manifest}")
        seen.add(relative)
        for dependency in toml_local_paths(manifest):
            pending.append((package / dependency).resolve())
    return seen


def dsp_package_roots(nmm: Path, dsp: Path) -> list[Path]:
    metadata_roots = dependency_paths_from_metadata(nmm, dsp)
    roots = metadata_roots if metadata_roots is not None else dependency_paths_from_manifests(dsp)
    required = {Path("crates/core"), Path("crates/engine_shared"), Path("crates/signal_core"), Path("crates/spatial_core")}
    if not required.issubset(roots):
        raise BaselineError(f"DSP dependency closure incomplete: required={sorted(path.as_posix() for path in required)}, found={sorted(path.as_posix() for path in roots)}")
    return sorted(roots, key=lambda path: path.as_posix())


def ios_evidence_paths(ios: Path) -> list[Path]:
    base = Path("noise_generator_app")
    provider_root = ios / base / "model" / "PresetProviders"
    providers = [path.relative_to(ios) for path in provider_root.rglob("*.swift") if path.is_file()]
    required = [
        base / "AudioManager.swift", base / "AudioManagerMac.swift", base / "AudioManagerProtocol.swift",
        base / "model" / "ActivePreset.swift", base / "noise_generator_appApp.swift",
        base / "Debug" / "JsonPresetModel.swift", base / "Debug" / "JsonPresetProvider.swift",
    ]
    return sorted(set(required + providers), key=lambda path: path.as_posix())


def collect_runtime_paths(nmm: Path, dsp: Path, ios: Path) -> dict[str, list[Path]]:
    nmm_paths = [Path("Cargo.toml"), Path("Cargo.lock"), Path("surrogate_weights_small.bin")]
    nmm_paths.extend(path.relative_to(nmm) for path in (nmm / "src").rglob("*.rs"))
    dsp_paths = [Path("Cargo.toml"), Path("Cargo.lock")]
    for package in dsp_package_roots(nmm, dsp):
        dsp_paths.extend((package / relative) for relative in relative_files(dsp / package))
    return {"nmm_runtime": sorted(set(nmm_paths)), "dsp_runtime": sorted(set(dsp_paths)), "ios_integration": ios_evidence_paths(ios)}


def source_inventory(nmm: Path, dsp: Path, ios: Path) -> dict[str, Any]:
    paths = collect_runtime_paths(nmm, dsp, ios)
    roots = {"nmm_runtime": nmm, "dsp_runtime": dsp, "ios_integration": ios}
    return {name: {"files": file_hash_inventory(roots[name], paths[name])} for name in sorted(paths)}


def source_fingerprints(inventory: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {name: fingerprint(value["files"]) for name, value in sorted(inventory.items())}


def validate_repo_paths(nmm: Path, dsp: Path) -> None:
    dependency_path = (nmm / ".." / "noise_generator_dsp").resolve()
    if dsp.resolve() != dependency_path:
        raise BaselineError(f"DSP path {dsp.resolve()} does not match the NMM Cargo path dependency {dependency_path}")


def toolchain_metadata() -> dict[str, str]:
    return {
        "rustc_verbose": run(["rustc", "--version", "--verbose"], Path.cwd(), check=True).stdout.strip(),
        "cargo": run(["cargo", "--version"], Path.cwd(), check=True).stdout.strip(),
        "python": platform.python_version(), "os": platform.platform(), "architecture": platform.machine(),
    }


def discover_presets(preset_dir: Path) -> list[Path]:
    presets = sorted(path for path in preset_dir.rglob("*.json") if path.is_file())
    if len(presets) != 60:
        raise BaselineError(f"expected exactly 60 preset JSON files, found {len(presets)}")
    return presets


def copy_file(source: Path, destination: Path) -> None:
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)


def normalized_report_path(profile_id: str, relative_preset: Path) -> Path:
    return Path("evaluations") / profile_id / relative_preset


def snapshot_preset_path(relative_preset: Path) -> Path:
    return Path("inputs") / "presets" / relative_preset


def safe_relative(path: str, label: str) -> Path:
    candidate = Path(path)
    if candidate.is_absolute() or ".." in candidate.parts or str(candidate) in {"", "."}:
        raise BaselineError(f"unsafe {label}: {path!r}")
    return candidate


def path_under(root: Path, value: str, label: str) -> Path:
    relative = safe_relative(value, label)
    path = root / relative
    try:
        path.resolve().relative_to(root.resolve())
    except ValueError as exc:
        raise BaselineError(f"{label} escapes baseline: {value!r}") from exc
    return path


def parse_summary(output: str) -> dict[str, int]:
    matches = [{key: int(value) for key, value in match.groupdict().items()} for match in SUMMARY_RE.finditer(output)]
    if not matches:
        raise BaselineError(f"could not find a Rust test summary; output tail={output[-1000:]!r}")
    return max(matches, key=lambda item: item["passed"] + item["failed"] + item["ignored"])


def parse_failure_names(output: str) -> set[str]:
    return set(FAIL_HEADER_RE.findall(output))


def assert_green_test_run(output: str, returncode: int, label: str) -> dict[str, int]:
    summary, failures = parse_summary(output), parse_failure_names(output)
    if returncode != 0 or summary["failed"] != 0 or failures:
        raise BaselineError(
            f"{label} tests are not green: summary={summary}, "
            f"failures={sorted(failures)}, exit={returncode}"
        )
    return summary


def validate_baseline_id(value: Any) -> str:
    if not isinstance(value, str) or not BASELINE_ID_RE.fullmatch(value):
        raise BaselineError(
            "baseline id must contain 3-64 lowercase letters, digits, underscores, or hyphens"
        )
    return value


def command_for_evaluation(binary: Path, relative_preset: Path, report_path: Path, profile: dict[str, Any]) -> list[str]:
    command = [str(binary), "evaluate", str(Path("presets") / relative_preset), "--goal", profile["goal"], "--brain-type", profile["brain_type"], "--duration", str(profile["duration_secs"])]
    if not profile["assr"]:
        command.append("--no-assr")
    if not profile["thalamic_gate"]:
        command.append("--no-thalamic-gate")
    if not profile["cet"]:
        command.append("--no-cet")
    if profile["physiological_thalamic_gate"]:
        command.append("--phys-gate")
    command.extend(["--arousal-model", profile["arousal_model"], "--json-report", str(report_path)])
    return command


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


def validate_profile(profile: dict[str, Any]) -> None:
    required = {"id", "goal", "brain_type", "duration_secs", "warmup_discard_secs", "analysis_duration_secs", "assr", "thalamic_gate", "cet", "physiological_thalamic_gate", "arousal_model", "acoustic_scoring", "acoustic_score_fusion"}
    if not isinstance(profile, dict) or set(profile) != required or profile["id"] not in PROFILE_IDS:
        raise BaselineError("evaluation profile is invalid")
    if not math.isclose(profile["duration_secs"] - profile["warmup_discard_secs"], profile["analysis_duration_secs"], rel_tol=0.0, abs_tol=1e-9):
        raise BaselineError(f"analysis duration is inconsistent for {profile['id']}")
    if profile["id"] == "compat_regression_v1" and profile["analysis_duration_secs"] < 10.0:
        raise BaselineError("regression profile analysis window is too short")


def validate_report(report: dict[str, Any], relative_preset: Path, profile: dict[str, Any]) -> None:
    required = {"preset_path", "goal", "brain_type", "score", "practical_status", "goal_semantics", "practical_report", "band_powers", "dominant_frequency_hz", "fhn_firing_rate", "fhn_isi_cv", "acoustic_summary", "model_signature", "limitations"}
    if not isinstance(report, dict) or not required.issubset(report):
        raise BaselineError(f"report fields missing for {relative_preset}")
    assert_all_numbers_finite(report)
    if report["preset_path"] != str(Path("presets") / relative_preset) or report["goal"] != profile["goal"] or str(report["brain_type"]).lower() != profile["brain_type"]:
        raise BaselineError(f"report profile mismatch for {relative_preset}/{profile['id']}")
    if not finite_number(report["score"]):
        raise BaselineError(f"report score is not finite for {relative_preset}")
    powers = report["band_powers"]
    if not isinstance(powers, dict) or any(not finite_number(powers.get(name)) for name in ("delta", "theta", "alpha", "beta", "gamma")):
        raise BaselineError(f"report band powers invalid for {relative_preset}")
    if any(not finite_number(report[name]) for name in ("dominant_frequency_hz", "fhn_firing_rate")) or (report["fhn_isi_cv"] is not None and not finite_number(report["fhn_isi_cv"])):
        raise BaselineError(f"report metrics invalid for {relative_preset}")
    signature = report["model_signature"]
    if not isinstance(signature, dict) or signature.get("version") != "legacy_v1" or signature.get("pipeline_variant") != "evaluate_canonical" or signature.get("duration_secs") != profile["duration_secs"] or signature.get("warmup_discard_secs") != profile["warmup_discard_secs"]:
        raise BaselineError(f"report signature invalid for {relative_preset}/{profile['id']}")
    flags = signature.get("auditory_flags")
    expected = {"assr_enabled": profile["assr"], "thalamic_gate_enabled": profile["thalamic_gate"], "physiological_thalamic_gate_enabled": profile["physiological_thalamic_gate"], "cet_enabled": profile["cet"], "acoustic_scoring_enabled": profile["acoustic_scoring"], "acoustic_score_fusion_enabled": profile["acoustic_score_fusion"], "arousal_model": profile["arousal_model"]}
    if not isinstance(flags, dict) or any(flags.get(key) != value for key, value in expected.items()):
        raise BaselineError(f"report feature flags mismatch for {relative_preset}/{profile['id']}")


def candidate_specs() -> dict[str, tuple[list[str], str]]:
    result = {
        "NormalFlow1PresetProvider": (["the_flow_v4.json"], "Flow v4 is likely, but carriers, levels, and positions differ."),
        "NormalIgnition1PresetProvider": (["normal_set_ignition_v3.json"], "Both identify as Ignition v3; the JSON has seven objects."),
        "NormalRelax1PresetProvider": (["unwind/relax_v1.json", "normal_set_deep_relax.json"], "Relaxation intent only; no canonical identity is asserted."),
        "NormalReset1PresetProvider": (["normal_set_reset.json", "unwind/reset_v1.json"], "Reset intent only; no canonical identity is asserted."),
        "NormalShield1PresetProvider": (["normal_set_shield_v6.json"], "Known room-coordinate and renderer mismatch."),
    }
    for colour in ("Black", "Blue", "Brown", "Green", "Grey", "Pink", "SSN", "White"):
        result[f"{colour}PresetProvider"] = ([f"showcase_{colour.lower()}.json"], "Colour/name counterpart only; renderer settings differ.")
    return result


def swift_registry(ios: Path, staging: Path) -> dict[str, Any]:
    active_path = ios / "noise_generator_app/model/ActivePreset.swift"
    text = active_path.read_text(encoding="utf-8")
    enum_start = text.find("enum ActivePreset")
    enum_end = text.find("extension ActivePreset", enum_start)
    cases = re.findall(r"^\s*case\s+([A-Za-z][A-Za-z0-9_]*)\s*$", text[enum_start:enum_end], re.MULTILINE)
    active_block = text[text.find("var isActive", enum_end):text.find("var compositionType", enum_end)]
    statuses: dict[str, str] = {}
    for match in re.finditer(r"case\s+([^:]+):\s*return\s+(true|false)", active_block):
        for name in re.findall(r"\.([A-Za-z][A-Za-z0-9_]*)", match.group(1)):
            statuses[name] = "active" if match.group(2) == "true" else "inactive"
    provider_block = text[text.find("var presetProvider"):]
    provider_map = {case: provider for case, provider in re.findall(r"case\s+\.([A-Za-z][A-Za-z0-9_]*):\s*([A-Za-z][A-Za-z0-9_]*)\(\)", provider_block)}
    provider_root = ios / "noise_generator_app/model/PresetProviders"
    provider_files = {}
    for path in provider_root.rglob("*.swift"):
        found = re.search(r"struct\s+([A-Za-z][A-Za-z0-9_]*)\s*:\s*NoisePresetProvider", path.read_text(encoding="utf-8"))
        if found:
            provider_files[found.group(1)] = path.relative_to(ios).as_posix()
    if set(cases) != set(statuses) or set(cases) != set(provider_map):
        raise BaselineError("ActivePreset registry does not classify every case")
    specs = candidate_specs()
    entries = []
    mapped_providers = set(provider_map.values())
    for provider, path in sorted(provider_files.items()):
        case = next((name for name, value in provider_map.items() if value == provider), None)
        state = statuses[case] if case else "provider_only"
        candidates, note = specs.get(provider, ([], "No JSON candidate was reviewed."))
        candidate_entries = [{"preset_path": str(Path("presets") / candidate), "snapshot_path": str(snapshot_preset_path(Path(candidate))), "sha256": sha256_file(staging / snapshot_preset_path(Path(candidate)))} for candidate in candidates]
        entries.append({"provider": provider, "provider_path": path, "provider_snapshot_path": str(Path("inputs/source-evidence/ios") / path), "provider_sha256": sha256_file(staging / "inputs/source-evidence/ios" / path), "active_preset_case": case, "shipping_status": state, "candidate_presets": candidate_entries, "relationship_status": "candidate_only", "known_mismatches": note})
    if mapped_providers - set(provider_files):
        raise BaselineError("ActivePreset references a missing provider")
    return {"active_preset_cases": sorted(cases), "providers": entries}


def artifact_hashes(root: Path) -> dict[str, str]:
    return {path.relative_to(root).as_posix(): sha256_file(path) for path in root.rglob("*") if path.is_file() and path.name != "manifest.json"}


def capture_state(nmm: Path, dsp: Path, ios: Path, presets: list[Path]) -> dict[str, Any]:
    inventory = source_inventory(nmm, dsp, ios)
    return {"repositories": {"nmm": current_repo_metadata(nmm), "dsp": current_repo_metadata(dsp), "ios": current_repo_metadata(ios)}, "source_inventory": inventory, "source_fingerprints": source_fingerprints(inventory), "preset_hashes": {str(path.relative_to(nmm)): sha256_file(path) for path in presets}, "ios_evidence_hashes": file_hash_inventory(ios, ios_evidence_paths(ios))}


def capture_state_differences(before: dict[str, Any], after: dict[str, Any]) -> list[str]:
    differences = []
    for name in ("nmm", "dsp", "ios"):
        if before["repositories"][name] != after["repositories"][name]:
            differences.append(f"{name} repository metadata")
    for group in ("source_inventory", "preset_hashes", "ios_evidence_hashes"):
        old, new = before[group], after[group]
        if old == new:
            continue
        if group == "source_inventory":
            for runtime in sorted(set(old) | set(new)):
                old_files = old.get(runtime, {}).get("files", {})
                new_files = new.get(runtime, {}).get("files", {})
                for path in sorted(set(old_files) | set(new_files)):
                    if old_files.get(path) != new_files.get(path):
                        differences.append(f"source_inventory:{runtime}:{path}")
            continue
        if isinstance(old, dict) and isinstance(new, dict):
            keys = set(old) | set(new)
            changed = sorted(key for key in keys if old.get(key) != new.get(key))
            differences.extend(f"{group}:{key}" for key in changed[:12])
            if len(changed) > 12:
                differences.append(f"{group}:and {len(changed) - 12} more")
        else:
            differences.append(group)
    return differences


def write_readme(root: Path, baseline_id: str) -> None:
    (root / "README.md").write_text(
        f"# Stage 1 baseline: {baseline_id}\n\n"
        "This schema-v3 snapshot is a regression reference, not evidence of human efficacy. "
        "It may be captured only when the current NMM and DSP test suites are green.\n\n"
        "Verify offline: `python3 tools/compatibility/stage1_baseline.py verify "
        "--baseline <baseline-directory>`\n\n"
        "Replay current code: `python3 tools/compatibility/stage1_baseline.py replay "
        "--baseline <baseline-directory> --nmm-repo . --dsp-repo ../noise_generator_dsp "
        "--ios-repo ../noise_generator_ios_app --with-tests`\n",
        encoding="utf-8",
    )


def capture(args: argparse.Namespace) -> None:
    nmm, dsp, ios, output = args.nmm_repo.resolve(), args.dsp_repo.resolve(), args.ios_repo.resolve(), args.output.resolve()
    baseline_id = validate_baseline_id(args.baseline_id)
    if output.exists():
        raise BaselineError(f"refusing to overwrite existing baseline: {output}")
    validate_repo_paths(nmm, dsp)
    presets = discover_presets(nmm / "presets")
    staging = Path(tempfile.mkdtemp(prefix="nmm-stage1-capture-"))
    try:
        pre = capture_state(nmm, dsp, ios, presets)
        for preset in presets:
            copy_file(preset, staging / snapshot_preset_path(preset.relative_to(nmm / "presets")))
        for relative in ios_evidence_paths(ios):
            copy_file(ios / relative, staging / "inputs/source-evidence/ios" / relative)
        build = run(["cargo", "build", "--locked", "--bin", "neural_preset_optimizer"], nmm)
        (staging / "test-results").mkdir(parents=True)
        (staging / "test-results/build.log").write_text(build.stdout, encoding="utf-8")
        if build.returncode != 0:
            raise BaselineError(f"NMM build failed; output tail={build.stdout[-1000:]!r}")
        binary = nmm / "target/debug/neural_preset_optimizer"
        nmm_tests = run(["cargo", "test", "--locked", "--all-targets"], nmm)
        (staging / "test-results/nmm.log").write_text(nmm_tests.stdout, encoding="utf-8")
        nmm_summary = assert_green_test_run(nmm_tests.stdout, nmm_tests.returncode, "NMM")
        dsp_tests = run(["cargo", "test", "--locked", "-p", "noise_generator_core", "--no-default-features"], dsp)
        (staging / "test-results/dsp.log").write_text(dsp_tests.stdout, encoding="utf-8")
        dsp_summary = assert_green_test_run(dsp_tests.stdout, dsp_tests.returncode, "DSP")
        entries = []
        for preset in presets:
            relative = preset.relative_to(nmm / "presets")
            reports = {}
            for profile_id in PROFILE_IDS:
                profile = PROFILES[profile_id]
                report_rel = normalized_report_path(profile_id, relative)
                report_path = staging / report_rel
                report_path.parent.mkdir(parents=True, exist_ok=True)
                result = run(command_for_evaluation(binary, relative, report_path, profile), staging / "inputs")
                if result.returncode != 0:
                    raise BaselineError(f"evaluation failed for {relative}/{profile_id}:\n{result.stdout}")
                validate_report(load_json(report_path), relative, profile)
                reports[profile_id] = str(report_rel)
            snapshot = staging / snapshot_preset_path(relative)
            preset_json = load_json(snapshot)
            entries.append({"preset_path": str(Path("presets") / relative), "snapshot_path": str(snapshot_preset_path(relative)), "sha256": sha256_file(snapshot), "capture_git_state": tracked_state(nmm, Path("presets") / relative), "object_count": len(preset_json.get("objects", [])), "source_count": preset_json.get("source_count"), "reports": reports})
        post = capture_state(nmm, dsp, ios, presets)
        if pre != post:
            changed = ", ".join(capture_state_differences(pre, post))
            raise BaselineError(f"capture source state changed during build/test/evaluation: {changed}")
        write_json(staging / "preset_inventory.json", {"count": len(entries), "profiles": list(PROFILE_IDS), "presets": entries})
        write_json(staging / "source_inventory.json", pre["source_inventory"])
        write_json(staging / "shipping_preset_registry.json", swift_registry(ios, staging))
        write_readme(staging, baseline_id)
        manifest = {"schema_version": SCHEMA_VERSION, "baseline_id": baseline_id, "captured_at_utc": datetime.now(timezone.utc).isoformat(), "warning": "Regression reference; not evidence of human efficacy.", "repositories": pre["repositories"], "source_fingerprints": pre["source_fingerprints"], "toolchain": toolchain_metadata(), "capture_tool_sha256": sha256_file(Path(__file__).resolve()), "binary_sha256": sha256_file(binary), "renderer_observation": RENDERER_OBSERVATION, "evaluation_profiles": PROFILES, "preset_corpus": {"root": "inputs/presets", "count": 60, "inventory": "preset_inventory.json"}, "tests": {"nmm": {"command": ["cargo", "test", "--locked", "--all-targets"], "exit_code": nmm_tests.returncode, "summary": nmm_summary, "failures": []}, "dsp": {"command": ["cargo", "test", "--locked", "-p", "noise_generator_core", "--no-default-features"], "exit_code": dsp_tests.returncode, "summary": dsp_summary, "failures": []}}, "evaluation_result": {"attempted": 120, "succeeded": 120, "report_root": "evaluations"}, "capture_state": {"pre_sha256": state_digest(pre), "post_sha256": state_digest(post), "capture_state_stable": True}, "artifact_hashes": artifact_hashes(staging)}
        write_json(staging / "manifest.json", manifest)
        verify_baseline(staging)
        output.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(staging), str(output))
        staging = None
    finally:
        if staging is not None:
            shutil.rmtree(staging, ignore_errors=True)


def tracked_state(repo: Path, relative_path: Path) -> str:
    status = run(["git", "status", "--porcelain=v1", "--untracked-files=all", "--", str(relative_path)], repo, check=True).stdout.strip()
    return "untracked" if status.startswith("??") else "modified" if status else "tracked_clean"


def normalize_manifest(manifest: dict[str, Any]) -> dict[str, Any]:
    """Adapt supported manifests to the schema-v3 in-memory shape."""
    if not isinstance(manifest, dict) or manifest.get("schema_version") not in SUPPORTED_SCHEMA_VERSIONS:
        raise BaselineError("unsupported baseline manifest")
    normalized = copy.deepcopy(manifest)
    source_schema_version = normalized["schema_version"]
    if source_schema_version == 2:
        profile = normalized.pop("evaluation_profile", None)
        if not isinstance(profile, dict):
            raise BaselineError("schema-v2 manifest is missing evaluation_profile")
        profile["warmup_discard_secs"] = 2.0
        duration = profile.get("duration_secs")
        if not finite_number(duration):
            raise BaselineError("schema-v2 evaluation duration is invalid")
        profile["analysis_duration_secs"] = round(float(duration) - profile["warmup_discard_secs"], 12)
        normalized["evaluation_profiles"] = {profile.get("id"): profile}
        normalized.setdefault("binary_sha256", None)
        normalized.setdefault("capture_state", None)
        normalized["schema_version"] = SCHEMA_VERSION
    normalized["_source_schema_version"] = source_schema_version
    return normalized


def normalize_inventory(inventory: dict[str, Any], profiles: dict[str, Any], source_schema_version: int) -> dict[str, Any]:
    if not isinstance(inventory, dict) or not isinstance(inventory.get("presets"), list):
        raise BaselineError("baseline inventory is invalid")
    normalized = copy.deepcopy(inventory)
    profile_ids = list(profiles)
    if source_schema_version == 2:
        if len(profile_ids) != 1:
            raise BaselineError("schema-v2 inventory requires exactly one evaluation profile")
        profile_id = profile_ids[0]
        for item in normalized["presets"]:
            if not isinstance(item, dict) or not isinstance(item.get("report_path"), str):
                raise BaselineError("schema-v2 inventory entry is missing report_path")
            item["reports"] = {profile_id: item.pop("report_path")}
        normalized["profiles"] = profile_ids
    return normalized


def validate_recorded_test(name: str, record: Any, require_green: bool) -> None:
    if not isinstance(record, dict) or not isinstance(record.get("exit_code"), int):
        raise BaselineError(f"recorded {name} test result is invalid")
    summary = record.get("summary")
    if not isinstance(summary, dict) or set(summary) != {"passed", "failed", "ignored"}:
        raise BaselineError(f"recorded {name} test summary is invalid")
    if any(not isinstance(summary[key], int) or summary[key] < 0 for key in summary):
        raise BaselineError(f"recorded {name} test counts are invalid")
    failures = record.get("failures", [])
    if not isinstance(failures, list) or any(not isinstance(value, str) for value in failures):
        raise BaselineError(f"recorded {name} failures are invalid")
    if len(failures) != summary["failed"]:
        raise BaselineError(f"recorded {name} failure count is inconsistent")
    is_green = record["exit_code"] == 0 and summary["failed"] == 0 and not failures
    if (summary["failed"] == 0) != (record["exit_code"] == 0):
        raise BaselineError(f"recorded {name} exit code is inconsistent")
    if require_green and not is_green:
        raise BaselineError(f"schema-v3 baseline records non-green {name} tests")


def validate_manifest(manifest: dict[str, Any]) -> dict[str, Any]:
    normalized = normalize_manifest(manifest)
    source_schema_version = normalized["_source_schema_version"]
    required = {"schema_version", "baseline_id", "captured_at_utc", "warning", "repositories", "source_fingerprints", "toolchain", "capture_tool_sha256", "binary_sha256", "renderer_observation", "evaluation_profiles", "preset_corpus", "tests", "evaluation_result", "capture_state", "artifact_hashes"}
    if not required.issubset(normalized):
        raise BaselineError("baseline manifest fields are incomplete")
    validate_baseline_id(normalized["baseline_id"])
    for name in ("nmm", "dsp", "ios"):
        repo = normalized["repositories"].get(name)
        if not isinstance(repo, dict) or not HEAD_RE.fullmatch(str(repo.get("head", ""))):
            raise BaselineError(f"recorded {name} revision is invalid")
    expected_profiles = {"compat_smoke_v1": PROFILES["compat_smoke_v1"]} if source_schema_version == 2 else PROFILES
    if normalized["renderer_observation"] != RENDERER_OBSERVATION or normalized["evaluation_profiles"] != expected_profiles:
        raise BaselineError("baseline contract is missing or changed")
    for profile in normalized["evaluation_profiles"].values():
        validate_profile(profile)
    if not SHA256_RE.fullmatch(str(normalized["capture_tool_sha256"])):
        raise BaselineError("capture hash is invalid")
    if source_schema_version == SCHEMA_VERSION and not SHA256_RE.fullmatch(str(normalized["binary_sha256"])):
        raise BaselineError("binary hash is invalid")
    expected_evaluations = 60 * len(normalized["evaluation_profiles"])
    if normalized["preset_corpus"].get("root") != "inputs/presets" or normalized["preset_corpus"].get("count") != 60 or normalized["evaluation_result"] != {"attempted": expected_evaluations, "succeeded": expected_evaluations, "report_root": "evaluations"}:
        raise BaselineError("baseline corpus metadata is invalid")
    tests = normalized["tests"]
    if not isinstance(tests, dict):
        raise BaselineError("recorded tests are invalid")
    validate_recorded_test("NMM", tests.get("nmm"), require_green=source_schema_version == SCHEMA_VERSION)
    validate_recorded_test("DSP", tests.get("dsp"), require_green=source_schema_version == SCHEMA_VERSION)
    state = normalized["capture_state"]
    if source_schema_version == SCHEMA_VERSION:
        if not isinstance(state, dict) or state.get("capture_state_stable") is not True or not SHA256_RE.fullmatch(str(state.get("pre_sha256", ""))) or state.get("pre_sha256") != state.get("post_sha256"):
            raise BaselineError("capture state is incomplete or unstable")
    elif state is not None:
        raise BaselineError("schema-v2 capture state must be absent")
    return normalized


def verify_baseline(root: Path) -> dict[str, Any]:
    manifest = validate_manifest(load_json(root / "manifest.json"))
    source_schema_version = manifest["_source_schema_version"]
    if source_schema_version == SCHEMA_VERSION:
        source = load_json(root / "source_inventory.json")
        if source_fingerprints(source) != manifest["source_fingerprints"]:
            raise BaselineError("source inventory does not match manifest fingerprints")
    inventory_data = normalize_inventory(
        load_json(root / "preset_inventory.json"),
        manifest["evaluation_profiles"],
        source_schema_version,
    )
    entries = inventory_data.get("presets")
    profile_ids = list(manifest["evaluation_profiles"])
    if inventory_data.get("count") != 60 or inventory_data.get("profiles") != profile_ids or not isinstance(entries, list) or len(entries) != 60:
        raise BaselineError("baseline inventory does not match its evaluation profiles")
    seen_presets, seen_reports = set(), set()
    for item in entries:
        preset_path = item.get("preset_path")
        snapshot_path = item.get("snapshot_path")
        reports = item.get("reports")
        if not isinstance(preset_path, str) or not isinstance(snapshot_path, str) or not isinstance(reports, dict) or set(reports) != set(profile_ids):
            raise BaselineError("invalid inventory entry")
        if preset_path in seen_presets:
            raise BaselineError("duplicate preset inventory entry")
        seen_presets.add(preset_path)
        relative = safe_relative(preset_path, "preset path").relative_to("presets")
        if Path(snapshot_path) != snapshot_preset_path(relative):
            raise BaselineError(f"frozen preset path mismatch: {preset_path}")
        snapshot = path_under(root, snapshot_path, "snapshot path")
        if not snapshot.is_file() or sha256_file(snapshot) != item.get("sha256"):
            raise BaselineError(f"frozen preset hash mismatch: {preset_path}")
        for profile_id, report_path in reports.items():
            expected_report = Path("evaluations") / relative if source_schema_version == 2 else normalized_report_path(profile_id, relative)
            if not isinstance(report_path, str) or Path(report_path) != expected_report or report_path in seen_reports:
                raise BaselineError(f"invalid report path for {preset_path}/{profile_id}")
            seen_reports.add(report_path)
            report = path_under(root, report_path, "report path")
            if not report.is_file():
                raise BaselineError(f"missing report: {report_path}")
            validate_report(load_json(report), relative, manifest["evaluation_profiles"][profile_id])
    if len(seen_reports) != 60 * len(profile_ids):
        raise BaselineError("baseline report count is inconsistent")
    shipping_file = "shipping_preset_candidates.json" if source_schema_version == 2 else "shipping_preset_registry.json"
    registry = load_json(root / shipping_file)
    providers = registry.get("providers") if isinstance(registry, dict) else None
    if not isinstance(providers, list) or not providers:
        raise BaselineError("shipping preset registry is missing")
    if source_schema_version == SCHEMA_VERSION:
        statuses = {entry.get("provider"): entry.get("shipping_status") for entry in providers}
        if statuses.get("BluePresetProvider") != "provider_only" or statuses.get("SSNPresetProvider") != "inactive" or any(status not in {"active", "inactive", "provider_only"} for status in statuses.values()):
            raise BaselineError("shipping preset registry status is invalid")
    for entry in providers:
        provider = path_under(root, entry.get("provider_snapshot_path", ""), "provider snapshot path")
        if not provider.is_file() or sha256_file(provider) != entry.get("provider_sha256"):
            raise BaselineError(f"provider snapshot mismatch: {entry.get('provider')}")
        for candidate in entry.get("candidate_presets", []):
            candidate_path = path_under(root, candidate.get("snapshot_path", ""), "candidate snapshot path")
            if not candidate_path.is_file() or sha256_file(candidate_path) != candidate.get("sha256"):
                raise BaselineError("candidate preset snapshot mismatch")
    for name in ("test-results/build.log", "test-results/nmm.log", "test-results/dsp.log", "README.md"):
        if not path_under(root, name, "required artifact").is_file():
            raise BaselineError(f"missing required artifact: {name}")
    if manifest["artifact_hashes"] != artifact_hashes(root):
        raise BaselineError("artifact hashes do not match manifest")
    return manifest


def drift_summary(manifest: dict[str, Any], nmm: Path, dsp: Path, ios: Path) -> list[str]:
    current = capture_state(nmm, dsp, ios, discover_presets(nmm / "presets"))
    lines = []
    for name in ("nmm", "dsp", "ios"):
        old, new = manifest["repositories"][name]["head"], current["repositories"][name]["head"]
        lines.append(f"{name} revision: {'same' if old == new else f'drift {old[:12]} -> {new[:12]}'}")
    for name in ("nmm_runtime", "dsp_runtime", "ios_integration"):
        old, new = manifest["source_fingerprints"][name]["sha256"], current["source_fingerprints"][name]["sha256"]
        lines.append(f"{name} fingerprint: {'same' if old == new else 'drift'}")
    lines.append(f"Rust toolchain: {'same' if manifest['toolchain']['rustc_verbose'] == toolchain_metadata()['rustc_verbose'] else 'drift'}")
    return lines


def replay_tests(nmm: Path, dsp: Path) -> None:
    nmm_tests = run(["cargo", "test", "--locked", "--all-targets"], nmm)
    assert_green_test_run(nmm_tests.stdout, nmm_tests.returncode, "NMM")
    dsp_tests = run(["cargo", "test", "--locked", "-p", "noise_generator_core", "--no-default-features"], dsp)
    assert_green_test_run(dsp_tests.stdout, dsp_tests.returncode, "DSP")


def replay(args: argparse.Namespace) -> int:
    baseline, nmm, dsp, ios = args.baseline.resolve(), args.nmm_repo.resolve(), args.dsp_repo.resolve(), args.ios_repo.resolve()
    validate_repo_paths(nmm, dsp)
    manifest = verify_baseline(baseline)
    for line in drift_summary(manifest, nmm, dsp, ios):
        print(line)
    build = run(["cargo", "build", "--locked", "--bin", "neural_preset_optimizer"], nmm)
    if build.returncode != 0:
        raise BaselineError(f"could not rebuild NMM binary; output tail={build.stdout[-1000:]!r}")
    if args.with_tests:
        replay_tests(nmm, dsp)
    profiles = manifest["evaluation_profiles"]
    selected = tuple(profiles) if args.profile == "all" else (args.profile,)
    unavailable = set(selected) - set(profiles)
    if unavailable:
        raise BaselineError(f"profiles not present in baseline: {sorted(unavailable)}")
    inventory = normalize_inventory(
        load_json(baseline / "preset_inventory.json"),
        profiles,
        manifest["_source_schema_version"],
    )["presets"]
    mismatches = []
    with tempfile.TemporaryDirectory(prefix="nmm-stage1-replay-") as directory:
        temp, binary = Path(directory), nmm / "target/debug/neural_preset_optimizer"
        for item in inventory:
            relative = safe_relative(item["preset_path"], "preset path").relative_to("presets")
            for profile_id in selected:
                fresh = temp / normalized_report_path(profile_id, relative)
                fresh.parent.mkdir(parents=True, exist_ok=True)
                result = run(command_for_evaluation(binary, relative, fresh, profiles[profile_id]), baseline / "inputs")
                if result.returncode != 0:
                    raise BaselineError(f"replay evaluation failed for {relative}/{profile_id}:\n{result.stdout}")
                validate_report(load_json(fresh), relative, profiles[profile_id])
                if sha256_file(fresh) != sha256_file(path_under(baseline, item["reports"][profile_id], "report path")):
                    mismatches.append(f"{profile_id}/{relative}")
    if mismatches:
        print(f"Replay mismatches ({len(mismatches)}/{len(inventory) * len(selected)}):", file=sys.stderr)
        for mismatch in mismatches:
            print(f"  {mismatch}", file=sys.stderr)
        return 2
    total = len(inventory) * len(selected)
    print(f"Replay matched {total}/{total} reports byte-for-byte.")
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
    capture_parser.add_argument("--baseline-id", required=True)
    verify_parser = subparsers.add_parser("verify")
    verify_parser.add_argument("--baseline", type=Path, required=True)
    replay_parser = subparsers.add_parser("replay", parents=[repos])
    replay_parser.add_argument("--baseline", type=Path, required=True)
    replay_parser.add_argument("--profile", choices=("all", *PROFILE_IDS), default="all")
    replay_parser.add_argument("--with-tests", action="store_true")
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
