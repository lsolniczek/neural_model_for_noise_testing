#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import json
from pathlib import Path
from typing import Dict, List

SCHEMA_VERSION = "human_validation_v1"

REQUIRED_TRIAL_COLUMNS = [
    "dataset_id",
    "experiment_id",
    "participant_id",
    "session_id",
    "trial_id",
    "condition_id",
    "stimulus_id",
    "preset_id",
    "cohort",
    "repeat_index",
    "carrier_color",
    "modulation_rate_hz",
    "modulation_depth",
    "reverb_level",
    "movement_level",
    "spl_db_a",
    "model_signature_schema_version",
    "model_signature_json",
    "legacy_v1_neural_score",
    "legacy_v1_fused_score",
    "candidate_research_v2_score",
    "product_acoustic_score",
]

REQUIRED_PEAK_COLUMNS = [
    "trial_id",
    "peak_rank",
    "center_hz",
    "bandwidth_hz",
    "power_above_aperiodic",
]

SCHEMA_DIR = Path(__file__).resolve().parents[2] / "calibration" / "schema"


def _load_csv(path: Path) -> List[Dict[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as f:
        return list(csv.DictReader(f))


def _require_columns(rows: List[Dict[str, str]], required: List[str], name: str) -> List[str]:
    if not rows:
        return [f"{name}: file has no rows"]
    cols = set(rows[0].keys())
    missing = [c for c in required if c not in cols]
    return [f"{name}: missing required column '{m}'" for m in missing]


def _parse_float(value: str, label: str, errors: List[str]) -> float | None:
    if value == "":
        return None
    try:
        return float(value)
    except ValueError:
        errors.append(f"{label}: invalid float '{value}'")
        return None


def _load_json_schema(path: Path) -> Dict:
    return json.loads(path.read_text(encoding="utf-8"))


def _validate_object_against_schema(name: str, obj: Dict, schema: Dict, errors: List[str]) -> None:
    props = schema.get("properties", {})
    required = schema.get("required", [])
    if schema.get("additionalProperties") is False:
        unknown = sorted(set(obj.keys()) - set(props.keys()))
        for key in unknown:
            errors.append(f"{name}: unexpected property '{key}'")
    for key in required:
        if key not in obj:
            errors.append(f"{name}: missing required key '{key}'")


def _coerce_row_for_schema(
    row: Dict[str, str], schema: Dict, errors: List[str], name: str, row_index: int
) -> Dict:
    out: Dict = {}
    props = schema.get("properties", {})
    if schema.get("additionalProperties") is False:
        unknown = sorted(set(row.keys()) - set(props.keys()))
        for key in unknown:
            errors.append(f"{name}:{row_index}: unexpected property '{key}'")
    for key, value in row.items():
        if key not in props:
            continue
        prop = props[key]
        t = prop.get("type")
        types = t if isinstance(t, list) else [t]
        if value == "":
            out[key] = None if "null" in types else value
        elif "integer" in types:
            try:
                out[key] = int(value)
            except ValueError:
                errors.append(f"{name}:{row_index}: '{key}' invalid numeric value '{value}'")
                continue
        elif "number" in types:
            try:
                out[key] = float(value)
            except ValueError:
                errors.append(f"{name}:{row_index}: '{key}' invalid numeric value '{value}'")
                continue
        else:
            out[key] = value
    return out


def _validate_row_against_schema(name: str, row_index: int, row: Dict[str, str], schema: Dict, errors: List[str]) -> None:
    _validate_object_against_schema(f"{name}:{row_index}", row, schema, errors)
    props = schema.get("properties", {})
    normalized = _coerce_row_for_schema(row, schema, errors, name, row_index)
    for key, rule in props.items():
        if key not in normalized:
            continue
        value = normalized[key]
        t = rule.get("type")
        types = t if isinstance(t, list) else [t]
        if value is None:
            if "null" not in types:
                errors.append(f"{name}:{row_index}: '{key}' cannot be null")
            continue
        if "string" in types and isinstance(value, str):
            min_len = rule.get("minLength")
            if min_len is not None and len(value) < min_len:
                errors.append(f"{name}:{row_index}: '{key}' length < {min_len}")
        if "number" in types and isinstance(value, float):
            if "minimum" in rule and value < rule["minimum"]:
                errors.append(f"{name}:{row_index}: '{key}' < minimum {rule['minimum']}")
            if "maximum" in rule and value > rule["maximum"]:
                errors.append(f"{name}:{row_index}: '{key}' > maximum {rule['maximum']}")
            if "exclusiveMinimum" in rule and value <= rule["exclusiveMinimum"]:
                errors.append(f"{name}:{row_index}: '{key}' <= exclusiveMinimum {rule['exclusiveMinimum']}")
        if "integer" in types and isinstance(value, int):
            if "minimum" in rule and value < rule["minimum"]:
                errors.append(f"{name}:{row_index}: '{key}' < minimum {rule['minimum']}")


def validate_dataset(manifest_path: Path, trials_path: Path, peaks_path: Path) -> Dict:
    errors: List[str] = []
    warnings: List[str] = []

    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    manifest_schema = _load_json_schema(SCHEMA_DIR / "human_validation_manifest_v1.schema.json")
    _validate_object_against_schema("manifest", manifest, manifest_schema, errors)
    if manifest.get("schema_version") != manifest_schema.get("properties", {}).get("schema_version", {}).get("const"):
        errors.append(
            f"manifest: expected schema_version='{SCHEMA_VERSION}', got '{manifest.get('schema_version')}'"
        )
    if manifest.get("created_at", "") == "":
        errors.append("manifest: 'created_at' must be non-empty")

    trials = _load_csv(trials_path)
    peaks = _load_csv(peaks_path)
    trial_schema = _load_json_schema(SCHEMA_DIR / "human_validation_trials_v1.schema.json")
    peak_schema = _load_json_schema(SCHEMA_DIR / "human_validation_peaks_v1.schema.json")
    errors.extend(_require_columns(trials, REQUIRED_TRIAL_COLUMNS, "trials"))
    errors.extend(_require_columns(peaks, REQUIRED_PEAK_COLUMNS, "peaks"))

    trial_ids = set()
    for i, row in enumerate(trials, start=2):
        _validate_row_against_schema("trials", i, row, trial_schema, errors)
        trial_id = row.get("trial_id", "")
        if not trial_id:
            errors.append(f"trials:{i}: missing trial_id")
        elif trial_id in trial_ids:
            errors.append(f"trials:{i}: duplicate trial_id '{trial_id}'")
        else:
            trial_ids.add(trial_id)

        for id_col in ("participant_id", "session_id", "dataset_id", "experiment_id"):
            if row.get(id_col, "") == "":
                errors.append(f"trials:{i}: missing {id_col}")

        spl = _parse_float(row.get("spl_db_a", ""), f"trials:{i}:spl_db_a", errors)
        if spl is not None and spl < 0.0:
            errors.append(f"trials:{i}: spl_db_a must be >= 0.0")

        depth = _parse_float(row.get("modulation_depth", ""), f"trials:{i}:modulation_depth", errors)
        if depth is not None and not (0.0 <= depth <= 1.0):
            errors.append(f"trials:{i}: modulation_depth must be in [0, 1]")

        _parse_float(row.get("modulation_rate_hz", ""), f"trials:{i}:modulation_rate_hz", errors)

    for i, row in enumerate(peaks, start=2):
        _validate_row_against_schema("peaks", i, row, peak_schema, errors)
        trial_id = row.get("trial_id", "")
        if trial_id not in trial_ids:
            errors.append(f"peaks:{i}: unknown trial_id '{trial_id}'")
        _parse_float(row.get("center_hz", ""), f"peaks:{i}:center_hz", errors)
        _parse_float(row.get("bandwidth_hz", ""), f"peaks:{i}:bandwidth_hz", errors)
        _parse_float(row.get("power_above_aperiodic", ""), f"peaks:{i}:power_above_aperiodic", errors)

    report = {
        "ok": len(errors) == 0,
        "schema_version": SCHEMA_VERSION,
        "dataset_id": manifest.get("dataset_id"),
        "trials_count": len(trials),
        "peaks_count": len(peaks),
        "errors": errors,
        "warnings": warnings,
    }
    return report


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate Stage 8 human-validation dataset inputs.")
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--trials", type=Path, required=True)
    parser.add_argument("--peaks", type=Path, required=True)
    parser.add_argument("--out", type=Path, help="Optional JSON report path")
    args = parser.parse_args()

    report = validate_dataset(args.manifest, args.trials, args.peaks)
    text = json.dumps(report, indent=2, sort_keys=True)
    print(text)
    if args.out:
        args.out.write_text(text + "\n", encoding="utf-8")
    return 0 if report["ok"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
