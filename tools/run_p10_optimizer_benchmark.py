#!/usr/bin/env python3
"""Run the frozen P-10 optimizer development/confirmation protocol.

The script deliberately keeps execution separate from analysis. Every run is
an ordinary CLI invocation with a complete exported preset, independent
64-replicate held-out evaluation, raw CSV row, and captured stdout/stderr.
Re-running with --resume skips complete rows. The default binary must be built
with `cargo build --release --bin neural_preset_optimizer` first.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import random
import statistics
import subprocess
from dataclasses import dataclass
from pathlib import Path


GOALS = {
    "focus": "adhd",
    "shield": "adhd",
    "ignition": "adhd",
    "sleep": "aging",
    "deep_relaxation": "anxious",
    "meditation": "high_alpha",
    "flow": "high_alpha",
    "deep_work": "normal",
    "isolation": "normal",
}

VARIANTS = {
    "legacy_clamp": ["--optimizer-schema", "legacy-v1", "--boundary-policy", "clamp"],
    "mixed_clamp": ["--optimizer-schema", "mixed-v2", "--boundary-policy", "clamp"],
    "mixed_reflect": ["--optimizer-schema", "mixed-v2", "--boundary-policy", "reflect"],
    "mixed_resample": ["--optimizer-schema", "mixed-v2", "--boundary-policy", "resample"],
    "mixed_reflect_crowding": [
        "--optimizer-schema", "mixed-v2", "--boundary-policy", "reflect", "--crowding",
    ],
    "mixed_reflect_restart": [
        "--optimizer-schema", "mixed-v2", "--boundary-policy", "reflect",
        "--stagnation-window", "10", "--stagnation-fraction", "0.30",
    ],
    "mixed_reflect_both": [
        "--optimizer-schema", "mixed-v2", "--boundary-policy", "reflect", "--crowding",
        "--stagnation-window", "10", "--stagnation-fraction", "0.30",
    ],
    "mixed_reflect_shade_h6": [
        "--optimizer-schema", "mixed-v2", "--boundary-policy", "reflect",
        "--shade-memory", "6",
    ],
    "mixed_reflect_shade_h6_lpsr": [
        "--optimizer-schema", "mixed-v2", "--boundary-policy", "reflect",
        "--shade-memory", "6", "--population-min", "4",
    ],
}

FIELDS = [
    "phase", "variant", "goal", "brain_type", "start", "optimizer_seed",
    "heldout_seed", "heldout_score", "strict_feasible", "generated_trials",
    "duplicate_trials", "dead_only_trials", "export_path", "heldout_path",
]


@dataclass(frozen=True)
class Protocol:
    duration: float
    search_replicates: int
    population: int = 32
    generations: int = 31
    heldout_replicates: int = 64


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", type=Path, default=Path("target/release/neural_preset_optimizer"))
    parser.add_argument("--output-dir", type=Path, default=Path("benchmarks/p10/optimizer"))
    parser.add_argument("--reliability-manifest", type=Path,
                        default=Path("benchmarks/p10/reliability/manifest.json"))
    parser.add_argument("--seed-preset", type=Path,
                        default=Path("presets/the_flow_handcrafted_v1.json"))
    parser.add_argument("--dev-seeds", type=int, default=10)
    parser.add_argument("--confirm-seeds", type=int, default=20)
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--analyze-only", action="store_true")
    return parser.parse_args()


def load_protocol(path: Path) -> Protocol:
    data = json.loads(path.read_text())
    duration = data.get("selected_duration_secs")
    search = data.get("selected_search_replicates")
    if duration is None or search is None:
        raise SystemExit("reliability manifest has no passing selected duration/search panel")
    return Protocol(float(duration), int(search))


def run_checked(command: list[str], log_path: Path) -> None:
    result = subprocess.run(command, text=True, capture_output=True)
    log_path.write_text(
        "$ " + " ".join(command) + "\n\nSTDOUT\n" + result.stdout
        + "\nSTDERR\n" + result.stderr
    )
    if result.returncode:
        raise RuntimeError(f"command failed ({result.returncode}); see {log_path}")


def existing_keys(csv_path: Path) -> set[tuple[str, ...]]:
    if not csv_path.exists():
        return set()
    with csv_path.open(newline="") as handle:
        return {
            (row["phase"], row["variant"], row["goal"], row["start"], row["optimizer_seed"])
            for row in csv.DictReader(handle)
        }


def append_row(csv_path: Path, row: dict[str, object]) -> None:
    write_header = not csv_path.exists()
    with csv_path.open("a", newline="") as handle:
        writer = csv.DictWriter(handle, FIELDS)
        if write_header:
            writer.writeheader()
        writer.writerow(row)


def evaluate_run(
    args: argparse.Namespace,
    protocol: Protocol,
    phase: str,
    variant: str,
    goal: str,
    brain: str,
    start: str,
    optimizer_seed: int,
    heldout_seed: int,
) -> dict[str, object]:
    stem = f"{phase}_{variant}_{goal}_{start}_{optimizer_seed}"
    run_dir = args.output_dir / "runs" / stem
    run_dir.mkdir(parents=True, exist_ok=True)
    export_path = run_dir / "preset.json"
    heldout_path = run_dir / "heldout.json"
    command = [
        str(args.binary), "optimize", "--goal", goal, "--brain-type", brain,
        "--population", str(protocol.population), "--generations", str(protocol.generations),
        "--duration", str(protocol.duration), "--search-replicates", str(protocol.search_replicates),
        "--finalist-count", "5", "--finalist-replicates", "64",
        "--seed", str(optimizer_seed), "--output", str(export_path), "--constrained",
        *VARIANTS[variant],
    ]
    if start == "seeded":
        command += ["--init-preset", str(args.seed_preset)]
    run_checked(command, run_dir / "optimize.log")
    run_checked(
        [
            str(args.binary), "evaluate", str(export_path), "--goal", goal,
            "--brain-type", brain, "--duration", str(protocol.duration),
            "--seed", str(heldout_seed), "--replicates", str(protocol.heldout_replicates),
            "--json-report", str(heldout_path),
        ],
        run_dir / "heldout.log",
    )
    export = json.loads(export_path.read_text())
    heldout = json.loads(heldout_path.read_text())
    provenance = export.get("optimizer_provenance") or {}
    return {
        "phase": phase,
        "variant": variant,
        "goal": goal,
        "brain_type": brain,
        "start": start,
        "optimizer_seed": optimizer_seed,
        "heldout_seed": heldout_seed,
        "heldout_score": heldout["score"]["mean"],
        "strict_feasible": provenance.get("final_strict_feasible", False),
        "generated_trials": provenance.get("generated_trials", 0),
        "duplicate_trials": provenance.get("duplicate_trials", 0),
        "dead_only_trials": provenance.get("dead_only_trials", 0),
        "export_path": export_path,
        "heldout_path": heldout_path,
    }


def bootstrap_lower(differences: list[float], seed: int = 0x503130) -> float:
    rng = random.Random(seed)
    means = []
    for _ in range(20_000):
        means.append(statistics.fmean(rng.choice(differences) for _ in differences))
    means.sort()
    return means[int(0.05 * len(means))]


def choose_development_winner(rows: list[dict[str, str]], expected_seeds: int) -> str:
    expected_per_variant = len(GOALS) * 2 * expected_seeds
    incomplete = {
        name: len([row for row in rows if row["variant"] == name])
        for name in VARIANTS
        if len([row for row in rows if row["variant"] == name]) != expected_per_variant
    }
    if incomplete:
        raise SystemExit(
            f"development rows are incomplete; expected {expected_per_variant} per variant, got {incomplete}"
        )
    candidates = [name for name in VARIANTS if name != "legacy_clamp"]
    keyed = {
        (row["variant"], row["goal"], row["start"], row["optimizer_seed"]): row
        for row in rows
    }
    medians = {}
    duplicate_rates = {}
    for name in candidates:
        candidate_rows = [row for row in rows if row["variant"] == name]
        differences = []
        for row in candidate_rows:
            legacy = keyed.get((
                "legacy_clamp", row["goal"], row["start"], row["optimizer_seed"]
            ))
            if legacy is None:
                raise SystemExit(f"development row has no paired legacy baseline: {row}")
            differences.append(float(row["heldout_score"]) - float(legacy["heldout_score"]))
        medians[name] = statistics.median(differences)
        duplicate_rates[name] = sum(float(row["duplicate_trials"]) for row in candidate_rows) / max(
            1.0, sum(float(row["generated_trials"]) for row in candidate_rows)
        )
    best_score = max(medians.values())
    tied = [name for name, score in medians.items() if best_score - score <= 0.005]
    return min(tied, key=lambda name: duplicate_rates[name])


def promotion_summary(
    rows: list[dict[str, str]], winner: str, expected_seeds: int
) -> dict[str, object]:
    confirm = [row for row in rows if row["phase"] == "confirmation"]
    keyed = {(r["goal"], r["start"], r["optimizer_seed"], r["variant"]): r for r in confirm}
    differences = []
    by_goal: dict[str, list[float]] = {goal: [] for goal in GOALS}
    for goal in GOALS:
        for start in ("random", "seeded"):
            for seed_text in sorted({r["optimizer_seed"] for r in confirm}):
                mixed = keyed.get((goal, start, seed_text, winner))
                legacy = keyed.get((goal, start, seed_text, "legacy_clamp"))
                if mixed and legacy:
                    difference = float(mixed["heldout_score"]) - float(legacy["heldout_score"])
                    differences.append(difference)
                    by_goal[goal].append(difference)
    expected_pairs = len(GOALS) * 2 * expected_seeds
    if len(differences) != expected_pairs:
        return {
            "passes": False,
            "reason": "confirmation rows are incomplete",
            "paired_n": len(differences),
            "expected_paired_n": expected_pairs,
        }
    lower = bootstrap_lower(differences)
    goal_means = {goal: statistics.fmean(values) for goal, values in by_goal.items() if values}
    winner_rows = [r for r in confirm if r["variant"] == winner]
    legacy_rows = [r for r in confirm if r["variant"] == "legacy_clamp"]
    winner_duplicate_rate = sum(float(r["duplicate_trials"]) for r in winner_rows) / max(
        1.0, sum(float(r["generated_trials"]) for r in winner_rows)
    )
    legacy_duplicate_rate = sum(float(r["duplicate_trials"]) for r in legacy_rows) / max(
        1.0, sum(float(r["generated_trials"]) for r in legacy_rows)
    )
    duplicate_reduction = (
        1.0 - winner_duplicate_rate / legacy_duplicate_rate
        if legacy_duplicate_rate > 0.0
        else (0.0 if winner_duplicate_rate == 0.0 else -winner_duplicate_rate)
    )
    dead_zero = all(int(r["dead_only_trials"]) == 0 for r in winner_rows)
    winner_feasible_rate = statistics.fmean(
        str(r["strict_feasible"]).lower() in ("true", "1") for r in winner_rows
    )
    legacy_feasible_rate = statistics.fmean(
        str(r["strict_feasible"]).lower() in ("true", "1") for r in legacy_rows
    )
    feasible_rate_difference = winner_feasible_rate - legacy_feasible_rate
    passes = (
        lower > -0.01
        and all(value >= -0.01 for value in goal_means.values())
        and feasible_rate_difference >= -0.05
        and duplicate_reduction >= 0.50
        and dead_zero
    )
    return {
        "passes": passes,
        "winner": winner,
        "paired_n": len(differences),
        "mean_difference": statistics.fmean(differences),
        "one_sided_bootstrap_95_lower": lower,
        "goal_mean_differences": goal_means,
        "duplicate_reduction": duplicate_reduction,
        "winner_duplicate_rate": winner_duplicate_rate,
        "legacy_duplicate_rate": legacy_duplicate_rate,
        "dead_only_trials_zero": dead_zero,
        "winner_strict_feasible_rate": winner_feasible_rate,
        "legacy_strict_feasible_rate": legacy_feasible_rate,
        "strict_feasible_rate_difference": feasible_rate_difference,
        "thresholds": {
            "bootstrap_lower_gt": -0.01,
            "each_goal_mean_gte": -0.01,
            "strict_feasible_rate_difference_gte": -0.05,
            "duplicate_reduction_gte": 0.50,
        },
    }


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main() -> None:
    args = parse_args()
    if args.dev_seeds < 1 or args.confirm_seeds < 1:
        raise SystemExit("--dev-seeds and --confirm-seeds must be at least 1")
    args.output_dir.mkdir(parents=True, exist_ok=True)
    csv_path = args.output_dir / "raw_runs.csv"
    if csv_path.exists() and not args.resume and not args.analyze_only:
        raise SystemExit(
            f"{csv_path} already exists; use --resume or choose a new --output-dir"
        )
    protocol = load_protocol(args.reliability_manifest)
    completed = existing_keys(csv_path) if args.resume else set()

    if not args.analyze_only:
        phases = [
            ("development", list(VARIANTS), args.dev_seeds, 0xD310_0000, 0xD310_8000),
        ]
        for phase, variants, count, optimizer_base, heldout_base in phases:
            for variant in variants:
                for goal, brain in GOALS.items():
                    for start in ("random", "seeded"):
                        for offset in range(count):
                            optimizer_seed = optimizer_base + offset
                            key = (phase, variant, goal, start, str(optimizer_seed))
                            if key in completed:
                                continue
                            row = evaluate_run(
                                args, protocol, phase, variant, goal, brain, start,
                                optimizer_seed, heldout_base + offset,
                            )
                            append_row(csv_path, row)

        with csv_path.open(newline="") as handle:
            dev_rows = [r for r in csv.DictReader(handle) if r["phase"] == "development"]
        winner = choose_development_winner(dev_rows, args.dev_seeds)
        for variant in ("legacy_clamp", winner):
            for goal, brain in GOALS.items():
                for start in ("random", "seeded"):
                    for offset in range(args.confirm_seeds):
                        optimizer_seed = 0xC010_0000 + offset
                        key = ("confirmation", variant, goal, start, str(optimizer_seed))
                        if key in completed:
                            continue
                        row = evaluate_run(
                            args, protocol, "confirmation", variant, goal, brain, start,
                            optimizer_seed, 0xC010_8000 + offset,
                        )
                        append_row(csv_path, row)

    if not csv_path.exists():
        raise SystemExit(f"no benchmark rows found at {csv_path}")
    with csv_path.open(newline="") as handle:
        rows = list(csv.DictReader(handle))
    dev_rows = [r for r in rows if r["phase"] == "development"]
    winner = choose_development_winner(dev_rows, args.dev_seeds)
    promotion = promotion_summary(rows, winner, args.confirm_seeds)
    manifest = {
        "schema": "nmm_p10_optimizer_benchmark_v1",
        "protocol": protocol.__dict__,
        "development_seed_domain": "0xD3100000",
        "confirmation_seed_domain": "0xC0100000",
        "heldout_panels": "independent direct-panel 64-replicate evaluations",
        "variants": VARIANTS,
        "development_winner": winner,
        "promotion": promotion,
        "raw_runs_sha256": sha256(csv_path),
    }
    (args.output_dir / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True))
    print(json.dumps(promotion, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
