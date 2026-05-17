#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tools.public_eeg_benchmarks.common import (
    DatasetNotDownloadedError,
    FixtureAdapter,
    ensure_downloaded,
    write_markdown_report,
    write_result_json,
)


def run_attention(dataset_id: str, output_dir: Path, use_fixture: bool) -> int:
    run_mode = "fixture_smoke_test" if use_fixture else "real_public_data"
    data_status = "fixture" if use_fixture else "downloaded"
    if use_fixture:
        rows = FixtureAdapter("auditory_attention").export_benchmark_rows()
    else:
        ensure_downloaded(dataset_id)
        raise NotImplementedError("Real dataset adapter parsing is not implemented in Stage 8c scaffold.")

    output_dir.mkdir(parents=True, exist_ok=True)
    csv_path = output_dir / "auditory_attention_rows.csv"
    with csv_path.open("w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader()
        w.writerows(rows)

    evidence_category = "fixture_smoke_test_only" if use_fixture else "unsupported_requires_own_study"
    write_result_json(
        output_dir / "auditory_attention_benchmark_result.json",
        {
            "dataset_id": dataset_id,
            "benchmark_family": "auditory_attention",
            "run_mode": run_mode,
            "data_status": data_status,
            "metrics_computed": ["plumbing_smoke_test"] if use_fixture else ["heldout_behavior_linkage_metrics"],
            "evidence_category": evidence_category,
        },
    )

    lines = [
        f"Dataset: `{dataset_id}`",
        f"Rows: `{len(rows)}`",
        f"Run mode: `{run_mode}`",
        f"Data status: `{data_status}`",
        "",
        f"Evidence category: `{evidence_category}`",
        "Public auditory-attention datasets can benchmark linkage trends, but cannot validate final preset efficacy claims.",
    ]
    write_markdown_report(
        output_dir / "auditory_attention_benchmark_report.md",
        "Auditory Attention Benchmark Report",
        lines,
    )
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dataset", required=True)
    ap.add_argument("--output-dir", type=Path, required=True)
    ap.add_argument("--use-fixture", action="store_true")
    args = ap.parse_args()
    try:
        return run_attention(args.dataset, args.output_dir, args.use_fixture)
    except DatasetNotDownloadedError as e:
        print(str(e))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
