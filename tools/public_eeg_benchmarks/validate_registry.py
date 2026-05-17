#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any, Dict, List


def _validate_schema(node: Any, schema: Dict[str, Any], path: str, errors: List[str]) -> None:
    expected_type = schema.get("type")
    if expected_type == "object":
        if not isinstance(node, dict):
            errors.append(f"{path}: expected object")
            return
        props = schema.get("properties", {})
        required = schema.get("required", [])
        for key in required:
            if key not in node:
                errors.append(f"{path}: missing '{key}'")
        if schema.get("additionalProperties") is False:
            unknown = sorted(set(node.keys()) - set(props.keys()))
            for k in unknown:
                errors.append(f"{path}: unexpected key '{k}'")
        for key, val in node.items():
            if key in props:
                _validate_schema(val, props[key], f"{path}.{key}", errors)
        return

    if expected_type == "array":
        if not isinstance(node, list):
            errors.append(f"{path}: expected array")
            return
        min_items = schema.get("minItems")
        if min_items is not None and len(node) < min_items:
            errors.append(f"{path}: requires at least {min_items} items")
        item_schema = schema.get("items")
        if item_schema:
            for i, item in enumerate(node):
                _validate_schema(item, item_schema, f"{path}[{i}]", errors)
        return

    # scalar / union checks
    types = expected_type if isinstance(expected_type, list) else [expected_type]
    valid_type = False
    for t in types:
        if t == "string" and isinstance(node, str):
            valid_type = True
            min_len = schema.get("minLength")
            if min_len is not None and len(node) < min_len:
                errors.append(f"{path}: string length < {min_len}")
        elif t == "integer" and isinstance(node, int):
            valid_type = True
            if "minimum" in schema and node < schema["minimum"]:
                errors.append(f"{path}: value < minimum {schema['minimum']}")
        elif t == "boolean" and isinstance(node, bool):
            valid_type = True
        elif t == "null" and node is None:
            valid_type = True
    if not valid_type:
        errors.append(f"{path}: type mismatch, expected {types}")
        return

    if "enum" in schema and node not in schema["enum"]:
        errors.append(f"{path}: invalid enum value '{node}'")
    if "const" in schema and node != schema["const"]:
        errors.append(f"{path}: expected const '{schema['const']}', got '{node}'")


def validate_registry(registry_path: Path, schema_path: Path) -> Dict:
    errors: List[str] = []
    reg = json.loads(registry_path.read_text(encoding="utf-8"))
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    _validate_schema(reg, schema, "registry", errors)

    datasets = reg.get("datasets", []) if isinstance(reg, dict) else []
    seen = set()
    for i, d in enumerate(datasets):
        label = f"datasets[{i}]"
        did = d.get("dataset_id") if isinstance(d, dict) else None
        if not did:
            errors.append(f"{label}: missing dataset_id")
        elif did in seen:
            errors.append(f"{label}: duplicate dataset_id '{did}'")
        else:
            seen.add(did)
    return {"ok": not errors, "errors": errors}


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--registry", type=Path, required=True)
    ap.add_argument("--schema", type=Path, required=True)
    args = ap.parse_args()
    report = validate_registry(args.registry, args.schema)
    print(json.dumps(report, indent=2))
    return 0 if report["ok"] else 2


if __name__ == "__main__":
    raise SystemExit(main())
