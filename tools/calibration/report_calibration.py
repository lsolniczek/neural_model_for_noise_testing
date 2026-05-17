#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
from pathlib import Path
from typing import Dict, List


def _load_csv(path: Path) -> List[Dict[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as f:
        return list(csv.DictReader(f))


def main() -> int:
    parser = argparse.ArgumentParser(description="Render calibration markdown report from artifact CSVs.")
    parser.add_argument("--run-dir", type=Path, required=True)
    args = parser.parse_args()

    metrics_cv = _load_csv(args.run_dir / "metrics_cv.csv")
    metrics_holdout = _load_csv(args.run_dir / "metrics_holdout.csv")
    metrics_cv_common = _load_csv(args.run_dir / "metrics_cv_common_support.csv")
    metrics_holdout_common = _load_csv(args.run_dir / "metrics_holdout_common_support.csv")
    failures_cv = _load_csv(args.run_dir / "failure_cases_cv.csv")
    failures_holdout = _load_csv(args.run_dir / "failure_cases_holdout.csv")
    missingness_cv = _load_csv(args.run_dir / "missingness_cv.csv")
    missingness_holdout = _load_csv(args.run_dir / "missingness_holdout.csv")
    run_manifest = {}
    manifest_path = args.run_dir / "calibration_run_manifest.json"
    if manifest_path.exists():
        import json
        run_manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    out_path = args.run_dir / "calibration_report.md"

    lines = []
    lines.append("# Calibration Report")
    lines.append("")
    lines.append(f"Run directory: `{args.run_dir}`")
    lines.append("")
    lines.append("## Development (CV) Metrics on Common Support")
    lines.append("")
    lines.append("| Outcome | Model | MAE | RMSE | R2 | MAE 95% CI |")
    lines.append("|---|---|---:|---:|---:|---|")
    for row in metrics_cv_common:
        if "mae_ci95_low" in row and "mae_ci95_high" in row and row["mae_ci95_low"] != "" and row["mae_ci95_high"] != "":
            ci = f"[{float(row['mae_ci95_low']):.4f}, {float(row['mae_ci95_high']):.4f}]"
        else:
            ci = "n/a"
        lines.append(
            f"| {row['outcome']} | {row['model_family']} | {float(row['mae']):.4f} | {float(row['rmse']):.4f} | {float(row['r2']):.4f} | {ci} |"
        )
    lines.append("")
    lines.append("## Locked Holdout Metrics on Common Support")
    lines.append("")
    if metrics_holdout_common:
        lines.append("| Outcome | Model | MAE | RMSE | R2 | MAE 95% CI |")
        lines.append("|---|---|---:|---:|---:|---|")
        for row in metrics_holdout_common:
            if "mae_ci95_low" in row and "mae_ci95_high" in row and row["mae_ci95_low"] != "" and row["mae_ci95_high"] != "":
                ci = f"[{float(row['mae_ci95_low']):.4f}, {float(row['mae_ci95_high']):.4f}]"
            else:
                ci = "n/a"
            lines.append(
                f"| {row['outcome']} | {row['model_family']} | {float(row['mae']):.4f} | {float(row['rmse']):.4f} | {float(row['r2']):.4f} | {ci} |"
            )
    else:
        lines.append("Not available (no locked holdout participants in split manifest).")
    lines.append("")
    lines.append("## Missingness Exclusions")
    lines.append("")
    lines.append(f"CV rows: {len(missingness_cv)}")
    lines.append(f"Holdout rows: {len(missingness_holdout)}")
    lines.append("")
    lines.append("## Family-specific Coverage Metrics")
    lines.append("")
    lines.append(f"CV family-specific rows: {len(metrics_cv)}")
    lines.append(f"Holdout family-specific rows: {len(metrics_holdout)}")
    lines.append("Use common-support holdout metrics for model-family promotion decisions.")
    lines.append("")
    lines.append("## Failure Cases")
    lines.append("")
    lines.append(f"CV failure rows: {len(failures_cv)}")
    lines.append(f"Holdout failure rows: {len(failures_holdout)}")
    if run_manifest.get("failure_thresholds"):
        lines.append("Failure rule: outcome-specific absolute-error thresholds from `calibration_run_manifest.json`.")
    lines.append("")
    lines.append("This report is offline calibration evidence only; it does not promote runtime defaults.")
    out_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(out_path)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
