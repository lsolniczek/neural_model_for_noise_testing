#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from tools.public_eeg_benchmarks.common import load_registry, write_markdown_report


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--input-dir", type=Path, required=True)
    ap.add_argument("--output", type=Path, required=True)
    ap.add_argument("--registry-path", type=Path, required=False)
    args = ap.parse_args()

    input_dir = args.input_dir
    result_files = [
        input_dir / "assr_benchmark_result.json",
        input_dir / "aperiodic_benchmark_result.json",
        input_dir / "auditory_attention_benchmark_result.json",
    ]
    results = []
    for p in result_files:
        if p.exists():
            results.append(json.loads(p.read_text(encoding="utf-8")))

    registry = load_registry(args.registry_path) if args.registry_path else load_registry()
    by_id = {d["dataset_id"]: d for d in registry["datasets"]}
    real_results = []
    not_yet_usable = []
    for r in results:
        if r.get("run_mode") != "real_public_data" or r.get("data_status") != "downloaded" or not r.get("metrics_computed"):
            continue
        ds = by_id.get(r.get("dataset_id"))
        if not ds:
            not_yet_usable.append((r, "dataset missing from registry"))
            continue
        base_ok = (
            ds.get("dataset_status") == "selected"
            and ds.get("evidence_usable") is True
            and ds.get("benchmark_ready") is True
        )
        input_kind = r.get("input_kind")
        if input_kind == "raw_public_source":
            path_ok = ds.get("raw_adapter_status") == "implemented"
        elif input_kind == "preprocessed_intermediate":
            path_ok = (
                ds.get("conversion_status") == "implemented"
                and r.get("provenance_status") == "source_verified"
            )
        else:
            path_ok = False
        if base_ok and path_ok:
            real_results.append(r)
        else:
            reason = []
            if not base_ok:
                reason.append("dataset selected/evidence_usable/benchmark_ready gate not satisfied")
            if not path_ok:
                ik = r.get("input_kind", "unknown")
                ps = r.get("provenance_status", "unknown")
                reason.append(
                    f"trusted-path gate not satisfied (input_kind={ik}, provenance_status={ps})"
                )
            not_yet_usable.append((r, "; ".join(reason)))
    fixture_results = [r for r in results if r.get("run_mode") == "fixture_smoke_test"]

    validated = [
        r for r in real_results if r.get("evidence_category") == "validated_by_public_data"
    ]
    partial = [
        r for r in real_results if r.get("evidence_category") == "partially_supported"
    ]

    lines = [
        "## validated_by_public_data",
    ]
    if validated:
        for r in validated:
            lines.append(f"- {r['benchmark_family']} via `{r['dataset_id']}`")
    else:
        lines.append("- None yet. No real-public-data validated entries were found.")

    lines += [
        "",
        "## partially_supported",
    ]
    if partial:
        for r in partial:
            lines.append(f"- {r['benchmark_family']} via `{r['dataset_id']}`")
    else:
        lines.append("- None yet from real public data.")

    lines += [
        "",
        "## not_yet_evidence_usable",
    ]
    if not_yet_usable:
        for r, reason in not_yet_usable:
            lines.append(f"- {r['benchmark_family']} via `{r['dataset_id']}`: {reason}")
    else:
        lines.append("- None.")

    lines += [
        "",
        "## fixture_smoke_tests",
    ]
    if fixture_results:
        for r in fixture_results:
            lines.append(f"- {r['benchmark_family']} fixture run (`{r['dataset_id']}`): {r.get('evidence_category')}")
    else:
        lines.append("- No fixture smoke-test results found.")

    lines += [
        "",
        "## unsupported_requires_own_study",
        "- Fixture-only results are engineering checks and do not count as public-data evidence.",
        "- Placeholder datasets are excluded from evidence promotion decisions.",
        "- ADHD-like preset effectiveness and individualized benefit claims remain unsupported by public data alone",
        "- Shield/product efficacy beyond acoustic utility remains unsupported without owned user studies",
    ]
    write_markdown_report(args.output, "Public EEG Evidence Map", lines)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
