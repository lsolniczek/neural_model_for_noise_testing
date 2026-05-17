#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import math
import random
from datetime import datetime, timezone
from pathlib import Path
from statistics import mean
from typing import Dict, Iterable, List, Tuple


OUTCOMES = [
    "aperiodic_exponent",
    "aperiodic_offset",
    "envelope_plv",
    "assr_plv",
    "alpha_peak_frequency_hz",
    "alpha_asymmetry",
    "vigilance_accuracy",
    "reaction_time_ms",
    "reaction_time_variability_ms",
    "comfort_rating",
    "irritation_rating",
    "masking_effectiveness_rating",
]

FEATURE_FAMILIES = {
    "acoustic_only": ["product_acoustic_score", "spl_db_a"],
    "modulation_only": [
        "candidate_total_modulation_power",
        "candidate_mod_slow_0p5_4_hz",
        "candidate_mod_theta_4_8_hz",
        "candidate_mod_alpha_8_13_hz",
        "candidate_mod_beta_13_30_hz",
        "candidate_mod_gamma_30_50_hz",
    ],
    "legacy_v1": ["legacy_v1_neural_score", "legacy_v1_fused_score"],
    "candidate_v2": [
        "candidate_research_v2_score",
        "candidate_gamma_response_strength",
        "candidate_modulation_responsiveness_index",
        "candidate_total_modulation_power",
    ],
}

FAILURE_THRESHOLDS = {
    "aperiodic_exponent": 0.25,
    "aperiodic_offset": 0.20,
    "envelope_plv": 0.12,
    "assr_plv": 0.12,
    "alpha_peak_frequency_hz": 1.0,
    "alpha_asymmetry": 0.08,
    "vigilance_accuracy": 0.10,
    "reaction_time_ms": 50.0,
    "reaction_time_variability_ms": 25.0,
    "comfort_rating": 1.0,
    "irritation_rating": 1.0,
    "masking_effectiveness_rating": 1.0,
}


def _maybe_float(row: Dict[str, str], key: str) -> float | None:
    v = row.get(key, "")
    return float(v) if v not in ("", None) else None


def _load_csv(path: Path) -> List[Dict[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as f:
        return list(csv.DictReader(f))


def _load_split(path: Path) -> Dict:
    return json.loads(path.read_text(encoding="utf-8"))


def _rows_by_participants(rows: List[Dict[str, str]], participants: Iterable[str]) -> List[Dict[str, str]]:
    ids = set(participants)
    return [r for r in rows if r["participant_id"] in ids]


def _feature_scalar(row: Dict[str, str], feature_names: List[str]) -> float | None:
    vals = [_maybe_float(row, f) for f in feature_names]
    if any(v is None for v in vals):
        return None
    cast = [v for v in vals if v is not None]
    if not cast:
        return None
    # Low-risk interpretable baseline: mean of available pre-outcome predictors.
    vals = cast
    return mean(vals) if vals else 0.0


def _eligible_rows(rows: List[Dict[str, str]], features: List[str], outcome: str) -> Tuple[List[Tuple[Dict[str, str], float, float]], int, int]:
    keep: List[Tuple[Dict[str, str], float, float]] = []
    dropped_missing_feature = 0
    dropped_missing_outcome = 0
    for row in rows:
        y = _maybe_float(row, outcome)
        if y is None:
            dropped_missing_outcome += 1
            continue
        x = _feature_scalar(row, features)
        if x is None:
            dropped_missing_feature += 1
            continue
        keep.append((row, x, y))
    return keep, dropped_missing_feature, dropped_missing_outcome


def _fit_linear(train: List[Tuple[float, float]]) -> Tuple[float, float]:
    # y = a + b x
    xs = [x for x, _ in train]
    ys = [y for _, y in train]
    mx = mean(xs)
    my = mean(ys)
    denom = sum((x - mx) ** 2 for x in xs)
    if denom <= 1e-12:
        return my, 0.0
    b = sum((x - mx) * (y - my) for x, y in train) / denom
    a = my - b * mx
    return a, b


def _metrics(y_true: List[float], y_pred: List[float]) -> Dict[str, float]:
    n = len(y_true)
    err = [p - t for p, t in zip(y_pred, y_true)]
    mae = sum(abs(e) for e in err) / n
    rmse = math.sqrt(sum(e * e for e in err) / n)
    mu = sum(y_true) / n
    ss_tot = sum((y - mu) ** 2 for y in y_true)
    ss_res = sum((t - p) ** 2 for t, p in zip(y_true, y_pred))
    r2 = 1.0 - ss_res / ss_tot if ss_tot > 1e-12 else 0.0
    return {"mae": mae, "rmse": rmse, "r2": r2}


def _participant_bootstrap_ci(rows: List[Dict], value_key: str, seed: int, n_boot: int = 200) -> Tuple[float, float]:
    by_pid: Dict[str, List[float]] = {}
    for r in rows:
        by_pid.setdefault(r["participant_id"], []).append(float(r[value_key]))
    pids = sorted(by_pid.keys())
    rng = random.Random(seed)
    samples: List[float] = []
    for _ in range(n_boot):
        chosen = [rng.choice(pids) for _ in pids]
        vals: List[float] = []
        for pid in chosen:
            vals.extend(by_pid[pid])
        samples.append(sum(vals) / max(1, len(vals)))
    samples.sort()
    lo = samples[int(0.025 * (len(samples) - 1))]
    hi = samples[int(0.975 * (len(samples) - 1))]
    return lo, hi


def _evaluate_partition(
    trials: List[Dict[str, str]],
    split_manifest: Dict,
    seed: int,
    mode: str,
) -> Dict[str, List[Dict]]:
    metrics_rows: List[Dict] = []
    prediction_rows: List[Dict] = []
    missingness_rows: List[Dict] = []

    common_metrics_rows: List[Dict] = []
    common_prediction_rows: List[Dict] = []
    common_failure_rows: List[Dict] = []

    for outcome in OUTCOMES:
        fold_defs = split_manifest["folds"] if mode == "cv" else [{
                "fold_id": "locked_holdout",
                "train_participants": sorted(
                    set(p["participant_id"] for p in trials) - set(split_manifest.get("holdout_participants", []))
                ),
                "test_participants": split_manifest.get("holdout_participants", []),
            }]
        common_fold_metrics_by_family: Dict[str, List[Dict[str, float]]] = {k: [] for k in FEATURE_FAMILIES}
        for fold in fold_defs:
                if mode == "holdout" and not fold["test_participants"]:
                    continue
                train_rows = _rows_by_participants(trials, fold["train_participants"])
                test_rows = _rows_by_participants(trials, fold["test_participants"])
                eligible_train_by_family = {}
                eligible_test_by_family = {}
                for family_name, family_features in FEATURE_FAMILIES.items():
                    tr_e, tr_df, tr_do = _eligible_rows(train_rows, family_features, outcome)
                    te_e, te_df, te_do = _eligible_rows(test_rows, family_features, outcome)
                    eligible_train_by_family[family_name] = tr_e
                    eligible_test_by_family[family_name] = te_e
                    missingness_rows.append(
                        {
                            "evaluation_set": mode,
                            "fold_id": fold["fold_id"],
                            "model_family": family_name,
                            "outcome": outcome,
                            "train_rows_total": len(train_rows),
                            "train_rows_kept": len(tr_e),
                            "train_dropped_missing_feature": tr_df,
                            "train_dropped_missing_outcome": tr_do,
                            "test_rows_total": len(test_rows),
                            "test_rows_kept": len(te_e),
                            "test_dropped_missing_feature": te_df,
                            "test_dropped_missing_outcome": te_do,
                        }
                    )

                common_test_ids = None
                for fam in FEATURE_FAMILIES:
                    ids = {r["trial_id"] for r, _, _ in eligible_test_by_family[fam]}
                    common_test_ids = ids if common_test_ids is None else (common_test_ids & ids)
                common_test_ids = common_test_ids or set()

                for model_name, features in FEATURE_FAMILIES.items():
                    fold_metrics = []
                    train_eligible = eligible_train_by_family[model_name]
                    test_eligible = eligible_test_by_family[model_name]
                    train_pairs = [(x, y) for _, x, y in train_eligible]
                    if train_pairs and test_eligible:
                        a, b = _fit_linear(train_pairs)
                        y_true: List[float] = []
                        y_pred: List[float] = []
                        for r, x, tgt in test_eligible:
                            pred = a + b * x
                            y_true.append(tgt)
                            y_pred.append(pred)
                            prediction_rows.append(
                                {
                                    "evaluation_set": mode,
                                    "model_family": model_name,
                                    "outcome": outcome,
                                    "fold_id": fold["fold_id"],
                                    "participant_id": r["participant_id"],
                                    "trial_id": r["trial_id"],
                                    "y_true": tgt,
                                    "y_pred": pred,
                                    "abs_error": abs(pred - tgt),
                                }
                            )
                        fold_metrics.append(_metrics(y_true, y_pred))

                    if train_pairs and common_test_ids:
                        a, b = _fit_linear(train_pairs)
                        test_common = [(r, x, y) for (r, x, y) in test_eligible if r["trial_id"] in common_test_ids]
                        if test_common:
                            y_true_c: List[float] = []
                            y_pred_c: List[float] = []
                            for r, x, tgt in test_common:
                                pred = a + b * x
                                y_true_c.append(tgt)
                                y_pred_c.append(pred)
                                common_prediction_rows.append(
                                    {
                                        "evaluation_set": mode,
                                        "model_family": model_name,
                                        "outcome": outcome,
                                        "fold_id": fold["fold_id"],
                                        "participant_id": r["participant_id"],
                                        "trial_id": r["trial_id"],
                                        "y_true": tgt,
                                        "y_pred": pred,
                                        "abs_error": abs(pred - tgt),
                                    }
                                )
                            common_fold_metrics_by_family[model_name].append(_metrics(y_true_c, y_pred_c))

                    if fold_metrics:
                        agg = {
                            "evaluation_set": mode,
                            "model_family": model_name,
                            "outcome": outcome,
                            "mae": mean(m["mae"] for m in fold_metrics),
                            "rmse": mean(m["rmse"] for m in fold_metrics),
                            "r2": mean(m["r2"] for m in fold_metrics),
                        }
                        metrics_rows.append(agg)
        for model_name in FEATURE_FAMILIES:
            fam = common_fold_metrics_by_family[model_name]
            if not fam:
                continue
            common_metrics_rows.append(
                {
                    "evaluation_set": mode,
                    "model_family": model_name,
                    "outcome": outcome,
                    "mae": mean(m["mae"] for m in fam),
                    "rmse": mean(m["rmse"] for m in fam),
                    "r2": mean(m["r2"] for m in fam),
                }
            )

    failure_rows: List[Dict] = []
    for p in prediction_rows:
        threshold = FAILURE_THRESHOLDS[p["outcome"]]
        if p["abs_error"] > threshold:
            fp = dict(p)
            fp["failure_threshold"] = threshold
            failure_rows.append(fp)
    for p in common_prediction_rows:
        threshold = FAILURE_THRESHOLDS[p["outcome"]]
        if p["abs_error"] > threshold:
            fp = dict(p)
            fp["failure_threshold"] = threshold
            common_failure_rows.append(fp)

    for row in metrics_rows:
        cohort_slice = [p for p in prediction_rows if p["model_family"] == row["model_family"] and p["outcome"] == row["outcome"]]
        if cohort_slice:
            lo, hi = _participant_bootstrap_ci(cohort_slice, "abs_error", seed + 17)
            row["mae_ci95_low"] = lo
            row["mae_ci95_high"] = hi
        else:
            row["mae_ci95_low"] = 0.0
            row["mae_ci95_high"] = 0.0

    return {
        "metrics_rows": metrics_rows,
        "metrics_common_support_rows": common_metrics_rows,
        "prediction_rows": prediction_rows,
        "prediction_common_support_rows": common_prediction_rows,
        "failure_rows": failure_rows,
        "failure_common_support_rows": common_failure_rows,
        "missingness_rows": missingness_rows,
    }


def _write_csv(path: Path, rows: List[Dict]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if not rows:
        path.write_text("", encoding="utf-8")
        return
    fieldnames = list(rows[0].keys())
    with path.open("w", encoding="utf-8", newline="") as f:
        w = csv.DictWriter(f, fieldnames=fieldnames)
        w.writeheader()
        w.writerows(rows)


def main() -> int:
    parser = argparse.ArgumentParser(description="Run offline Stage 8 calibration against grouped splits.")
    parser.add_argument("--manifest", type=Path, required=True)
    parser.add_argument("--trials", type=Path, required=True)
    parser.add_argument("--peaks", type=Path, required=True)
    parser.add_argument("--splits", type=Path, required=True)
    parser.add_argument("--artifacts-root", type=Path, default=Path("calibration/artifacts"))
    parser.add_argument("--seed", type=int, default=1234)
    args = parser.parse_args()

    manifest = json.loads(args.manifest.read_text(encoding="utf-8"))
    trials = _load_csv(args.trials)
    _ = _load_csv(args.peaks)  # kept for contract symmetry; peaks are available for later models.
    splits = _load_split(args.splits)

    dataset_id = manifest["dataset_id"]
    run_id = datetime.now(timezone.utc).strftime("run_%Y%m%dT%H%M%SZ")
    run_dir = args.artifacts_root / dataset_id / run_id
    run_dir.mkdir(parents=True, exist_ok=True)

    cv_out = _evaluate_partition(trials, splits, args.seed, mode="cv")
    holdout_out = _evaluate_partition(trials, splits, args.seed, mode="holdout")
    _write_csv(run_dir / "metrics_cv.csv", cv_out["metrics_rows"])
    _write_csv(run_dir / "metrics_holdout.csv", holdout_out["metrics_rows"])
    _write_csv(run_dir / "metrics_cv_common_support.csv", cv_out["metrics_common_support_rows"])
    _write_csv(run_dir / "metrics_holdout_common_support.csv", holdout_out["metrics_common_support_rows"])
    _write_csv(run_dir / "metrics_by_model.csv", cv_out["metrics_rows"])
    _write_csv(run_dir / "metrics_by_outcome.csv", sorted(cv_out["metrics_rows"], key=lambda r: (r["outcome"], r["model_family"])))
    _write_csv(run_dir / "predictions_cv.csv", cv_out["prediction_rows"])
    _write_csv(run_dir / "predictions_holdout.csv", holdout_out["prediction_rows"])
    _write_csv(run_dir / "predictions_cv_common_support.csv", cv_out["prediction_common_support_rows"])
    _write_csv(run_dir / "predictions_holdout_common_support.csv", holdout_out["prediction_common_support_rows"])
    _write_csv(run_dir / "failure_cases_cv.csv", cv_out["failure_rows"])
    _write_csv(run_dir / "failure_cases_holdout.csv", holdout_out["failure_rows"])
    _write_csv(run_dir / "failure_cases_cv_common_support.csv", cv_out["failure_common_support_rows"])
    _write_csv(run_dir / "failure_cases_holdout_common_support.csv", holdout_out["failure_common_support_rows"])
    _write_csv(run_dir / "missingness_cv.csv", cv_out["missingness_rows"])
    _write_csv(run_dir / "missingness_holdout.csv", holdout_out["missingness_rows"])

    digest = hashlib.sha256(args.trials.read_bytes()).hexdigest()
    run_manifest = {
        "dataset_id": dataset_id,
        "schema_version": manifest.get("schema_version"),
        "run_id": run_id,
        "timestamp_utc": datetime.now(timezone.utc).isoformat(),
        "split_manifest": str(args.splits),
        "feature_families": FEATURE_FAMILIES,
        "outcomes": OUTCOMES,
        "failure_thresholds": FAILURE_THRESHOLDS,
        "holdout_available": bool(splits.get("holdout_participants")),
        "trials_sha256": digest,
    }
    (run_dir / "split_manifest.json").write_text(json.dumps(splits, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    (run_dir / "calibration_run_manifest.json").write_text(json.dumps(run_manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(run_dir)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
