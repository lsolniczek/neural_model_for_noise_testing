#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import json
import random
from collections import defaultdict
from pathlib import Path
from typing import Dict, List, Set


def _load_trials(path: Path) -> List[Dict[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as f:
        return list(csv.DictReader(f))


def _participant_index(trials: List[Dict[str, str]]) -> Dict[str, str]:
    idx: Dict[str, str] = {}
    for row in trials:
        pid = row["participant_id"]
        cohort = row.get("cohort", "unknown")
        if pid not in idx:
            idx[pid] = cohort
    return idx


def build_grouped_splits(
    trials: List[Dict[str, str]],
    k_folds: int,
    holdout_frac: float,
    seed: int,
) -> Dict:
    if k_folds < 2:
        raise ValueError("k_folds must be >= 2")
    rng = random.Random(seed)
    by_cohort: Dict[str, List[str]] = defaultdict(list)
    participants = _participant_index(trials)
    for pid, cohort in participants.items():
        by_cohort[cohort].append(pid)
    for plist in by_cohort.values():
        rng.shuffle(plist)

    holdout: Set[str] = set()
    if holdout_frac > 0:
        for plist in by_cohort.values():
            n = max(1, int(round(len(plist) * holdout_frac))) if plist else 0
            holdout.update(plist[:n])

    dev_participants = sorted(set(participants.keys()) - holdout)
    if len(dev_participants) < 2:
        raise ValueError("Not enough development participants after holdout extraction.")
    effective_k = min(k_folds, len(dev_participants))
    warnings: List[str] = []
    if effective_k != k_folds:
        warnings.append(
            f"Reduced k_folds from {k_folds} to {effective_k} because only {len(dev_participants)} development participants are available."
        )

    dev_by_cohort: Dict[str, List[str]] = defaultdict(list)
    for pid in dev_participants:
        dev_by_cohort[participants[pid]].append(pid)

    fold_buckets: List[Set[str]] = [set() for _ in range(effective_k)]
    for cohort in sorted(dev_by_cohort.keys()):
        plist = sorted(dev_by_cohort[cohort])
        for i, pid in enumerate(plist):
            fold_buckets[i % effective_k].add(pid)

    non_empty_buckets = [b for b in fold_buckets if b]
    if not non_empty_buckets:
        raise ValueError("No non-empty folds generated; adjust k_folds/holdout_frac for available participants.")
    if len(non_empty_buckets) < effective_k:
        warnings.append(
            f"Reduced effective fold count from {effective_k} to {len(non_empty_buckets)} to avoid empty test folds."
        )
        effective_k = len(non_empty_buckets)

    folds: List[Dict] = []
    for fold_idx, test in enumerate(non_empty_buckets):
        train = sorted(set(dev_participants) - test)
        folds.append(
            {
                "fold_id": f"fold_{fold_idx}",
                "train_participants": train,
                "test_participants": sorted(test),
            }
        )

    return {
        "split_schema_version": "human_validation_split_v1",
        "seed": seed,
        "k_folds_requested": k_folds,
        "k_folds": effective_k,
        "holdout_fraction": holdout_frac,
        "holdout_participants": sorted(holdout),
        "folds": folds,
        "warnings": warnings,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description="Build participant-grouped calibration splits.")
    parser.add_argument("--trials", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--k-folds", type=int, default=3)
    parser.add_argument("--holdout-frac", type=float, default=0.2)
    parser.add_argument("--seed", type=int, default=1234)
    args = parser.parse_args()

    trials = _load_trials(args.trials)
    manifest = build_grouped_splits(trials, args.k_folds, args.holdout_frac, args.seed)
    args.out.parent.mkdir(parents=True, exist_ok=True)
    args.out.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"Wrote split manifest: {args.out}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
