#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import sys
from pathlib import Path
from statistics import mean

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))

from tools.public_eeg_benchmarks.adapters.ds005048 import (
    DatasetLayoutError,
    Ds005048PreprocessedAdapter,
)
from tools.public_eeg_benchmarks.common import (
    DatasetNotDownloadedError,
    FixtureAdapter,
    ensure_downloaded,
    write_markdown_report,
    write_result_json,
)


def compute_assr_metrics(rows: list[dict[str, str]]) -> dict:
    def _f(v: str | None, default: float) -> float:
        if v in (None, ""):
            return default
        x = float(v)
        return default if x != x else x

    parsed = []
    for r in rows:
        nominal = _f(r.get("modulation_rate_hz"), 0.0)
        parsed.append(
            {
                "trial_id": r["trial_id"],
                "expected_effect_label": r["expected_effect_label"],
                "observed_dominant_modulation_hz": _f(r.get("observed_dominant_modulation_hz"), nominal),
                "observed_assr_strength": _f(r.get("observed_assr_strength"), 0.0),
            }
        )
    target = [r for r in parsed if r["expected_effect_label"] == "target_entrainment"]
    control = [r for r in parsed if r["expected_effect_label"] != "target_entrainment"]
    if not parsed:
        return {
            "observed_target_rate_recovery_accuracy": 0.0,
            "observed_target_band_strength": 0.0,
            "observed_target_vs_control_strength_delta": 0.0,
            "observed_dominant_modulation_hz_error": 0.0,
            "observed_assr_strength_summary": 0.0,
            "failure_cases": [],
        }
    target_correct = sum(1 for r in target if abs(r["observed_dominant_modulation_hz"] - 40.0) <= 2.0)
    target_recovery = target_correct / len(target) if target else 0.0
    target_strength = mean([r["observed_assr_strength"] for r in target]) if target else 0.0
    control_strength = mean([r["observed_assr_strength"] for r in control]) if control else None
    delta = (target_strength - control_strength) if control_strength is not None else None
    hz_err = mean([abs(r["observed_dominant_modulation_hz"] - 40.0) for r in target]) if target else 0.0
    strength_summary = mean([r["observed_assr_strength"] for r in parsed])
    failures = [
        {
            "trial_id": r["trial_id"],
            "reason": "observed_target_not_near_40hz",
            "observed_modulation_rate_hz": r["observed_dominant_modulation_hz"],
        }
        for r in target
        if abs(r["observed_dominant_modulation_hz"] - 40.0) > 2.0
    ]
    return {
        "observed_target_rate_recovery_accuracy": target_recovery,
        "observed_target_band_strength": target_strength,
        "observed_target_vs_control_strength_delta": delta,
        "observed_target_vs_control_strength_delta_status": "ok" if delta is not None else "unavailable_no_control_rows",
        "observed_dominant_modulation_hz_error": hz_err,
        "observed_assr_strength_summary": strength_summary,
        "failure_cases": failures,
    }


def compute_prediction_rows(rows: list[dict[str, str]]) -> list[dict[str, str]]:
    # Stage 8d closure guardrail:
    # prediction rows are intentionally unavailable until a real model-derived bridge
    # is implemented. Do not derive "predictions" from benchmark labels.
    out: list[dict[str, str]] = []
    for r in rows:
        out.append(
            {
                "trial_id": r["trial_id"],
                "condition_id": r["condition_id"],
                "predicted_dominant_modulation_hz": "",
                "predicted_gamma_assr_response_strength": "",
                "prediction_status": "unavailable_model_bridge_not_implemented",
            }
        )
    return out


def compute_prediction_metrics(rows: list[dict[str, str]], pred_rows: list[dict[str, str]]) -> dict:
    joined = []
    return {
        "predicted_target_rate_recovery_accuracy": None,
        "predicted_target_vs_control_strength_delta": None,
        "prediction_observation_target_rate_agreement": None,
        "prediction_observation_condition_rank_agreement": None,
        "prediction_observation_strength_delta_sign_agreement": None,
        "comparison_status": "unavailable_noncommensurate_output",
        "joined_rows": joined,
    }


def run_assr(dataset_id: str, output_dir: Path, use_fixture: bool, dataset_root: Path | None) -> int:
    run_mode = "fixture_smoke_test" if use_fixture else "real_public_data"
    data_status = "fixture" if use_fixture else "downloaded"
    if use_fixture:
        rows = FixtureAdapter("assr").export_benchmark_rows()
    else:
        if dataset_root is None:
            ensure_downloaded(dataset_id)
            raise DatasetLayoutError("--dataset-root is required when not using --use-fixture")
        if dataset_id != "ds005048":
            raise DatasetLayoutError(f"real adapter path currently supports only ds005048, got {dataset_id}")
        adapter = Ds005048PreprocessedAdapter(dataset_root)
        rows = adapter.export_benchmark_rows()
        provenance_status = adapter.last_provenance_status()
        if not all(r.get("resolution_sufficient_for_target_metric", "false").lower() == "true" for r in rows):
            raise DatasetLayoutError("insufficient frequency resolution for ±2Hz target recovery metric")
    if use_fixture:
        provenance_status = "fixture"

    output_dir.mkdir(parents=True, exist_ok=True)
    with (output_dir / "assr_rows.csv").open("w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=list(rows[0].keys()))
        w.writeheader()
        w.writerows(rows)

    metrics = compute_assr_metrics(rows)
    metric_rows = [
        {"metric": "observed_target_rate_recovery_accuracy", "value": metrics["observed_target_rate_recovery_accuracy"]},
        {"metric": "observed_target_band_strength", "value": metrics["observed_target_band_strength"]},
        {"metric": "observed_target_vs_control_strength_delta", "value": metrics["observed_target_vs_control_strength_delta"]},
        {"metric": "observed_target_vs_control_strength_delta_status", "value": metrics["observed_target_vs_control_strength_delta_status"]},
        {"metric": "observed_dominant_modulation_hz_error", "value": metrics["observed_dominant_modulation_hz_error"]},
        {"metric": "observed_assr_strength_summary", "value": metrics["observed_assr_strength_summary"]},
    ]
    for p in ("assr_metrics.csv", "assr_observed_metrics.csv"):
        with (output_dir / p).open("w", encoding="utf-8", newline="") as f:
            w = csv.DictWriter(f, fieldnames=["metric", "value"])
            w.writeheader()
            w.writerows(metric_rows)
    pred_rows = compute_prediction_rows(rows)
    pred_metrics = compute_prediction_metrics(rows, pred_rows)
    with (output_dir / "assr_prediction_metrics.csv").open("w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["metric", "value"])
        w.writeheader()
        w.writerow({"metric": "predicted_target_rate_recovery_accuracy", "value": ""})
        w.writerow({"metric": "predicted_target_vs_control_strength_delta", "value": ""})
        w.writerow({"metric": "status", "value": "unavailable_model_bridge_not_implemented"})
    with (output_dir / "assr_comparison_metrics.csv").open("w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["metric", "value"])
        w.writeheader()
        w.writerow({"metric": "prediction_observation_target_rate_agreement", "value": ""})
        w.writerow({"metric": "prediction_observation_condition_rank_agreement", "value": ""})
        w.writerow({"metric": "prediction_observation_strength_delta_sign_agreement", "value": ""})
        w.writerow({"metric": "status", "value": "comparison_unavailable_noncommensurate_output"})
    with (output_dir / "assr_prediction_rows.csv").open("w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["trial_id", "condition_id", "predicted_dominant_modulation_hz", "predicted_gamma_assr_response_strength", "prediction_status"])
        w.writeheader()
        w.writerows(pred_rows)
    with (output_dir / "assr_failure_cases.csv").open("w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=["trial_id", "reason", "observed_modulation_rate_hz"])
        w.writeheader()
        w.writerows(metrics["failure_cases"])
        w.writerow(
            {
                "trial_id": "",
                "reason": "comparison_unavailable_noncommensurate_output",
                "observed_modulation_rate_hz": "",
            }
        )

    evidence_category = "plumbing_verified" if use_fixture else "not_yet_evidence_usable"
    res_ok = None
    min_epoch_duration_s = None
    max_epoch_duration_s = None
    max_frequency_resolution_hz = None
    if not use_fixture:
        epoch_durations = [float(r["epoch_duration_s"]) for r in rows]
        freq_res = [float(r["frequency_resolution_hz"]) for r in rows]
        res_ok = all(r.get("resolution_sufficient_for_target_metric", "false").lower() == "true" for r in rows)
        min_epoch_duration_s = min(epoch_durations)
        max_epoch_duration_s = max(epoch_durations)
        max_frequency_resolution_hz = max(freq_res)
    provenance_verified = provenance_status == "source_verified" if not use_fixture else False
    limitations = []
    if not use_fixture and provenance_status != "source_verified":
        limitations.append("Source lineage is not fully verified; this run is not yet evidence-usable.")
    if not use_fixture:
        limitations.append("NMM prediction bridge is not implemented; prediction/comparison metrics are unavailable.")

    result = {
        "dataset_id": dataset_id,
        "benchmark_family": "assr",
        "run_mode": run_mode,
        "data_status": data_status,
        "adapter_status": "intermediate_adapter_implemented" if not use_fixture else "fixture_only",
        "metrics_computed": ["plumbing_smoke_test"] if use_fixture else [
            "observed_target_rate_recovery_accuracy",
            "observed_target_band_strength",
            "observed_target_vs_control_strength_delta",
            "observed_dominant_modulation_hz_error",
            "comparison_unavailable_noncommensurate_output",
        ],
        "evidence_category": evidence_category,
        "limitations": limitations,
        "input_kind": "fixture" if use_fixture else "preprocessed_intermediate",
        "provenance_status": provenance_status,
        "provenance_verified": provenance_verified,
        "min_epoch_duration_s": min_epoch_duration_s,
        "max_epoch_duration_s": max_epoch_duration_s,
        "max_frequency_resolution_hz": max_frequency_resolution_hz,
        "all_rows_resolution_sufficient": res_ok,
        "resolution_sufficient_for_target_metric": res_ok,
    }
    write_result_json(output_dir / "assr_benchmark_result.json", result)

    lines = [
        f"Dataset: `{dataset_id}`",
        f"Rows: `{len(rows)}`",
        f"Run mode: `{run_mode}`",
        f"Observed target rate recovery accuracy: `{metrics['observed_target_rate_recovery_accuracy']:.3f}`",
        "Observed target-vs-control strength delta: "
        + (
            f"`{metrics['observed_target_vs_control_strength_delta']:.3f}`"
            if metrics["observed_target_vs_control_strength_delta"] is not None
            else "`unavailable_no_control_rows`"
        ),
        f"Observed dominant modulation 40 Hz error: `{metrics['observed_dominant_modulation_hz_error']:.3f}`",
        "Prediction rows: `unavailable_model_bridge_not_implemented`",
        "Comparison metrics: `unavailable_noncommensurate_output`",
        f"Evidence category: `{evidence_category}`",
    ]
    write_markdown_report(output_dir / "assr_benchmark_report.md", "ASSR Benchmark Report", lines)
    return 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--dataset", required=True)
    ap.add_argument("--output-dir", type=Path, required=True)
    ap.add_argument("--use-fixture", action="store_true")
    ap.add_argument("--dataset-root", type=Path)
    args = ap.parse_args()
    try:
        return run_assr(args.dataset, args.output_dir, args.use_fixture, args.dataset_root)
    except (DatasetNotDownloadedError, DatasetLayoutError) as e:
        print(str(e))
        return 2


if __name__ == "__main__":
    raise SystemExit(main())
