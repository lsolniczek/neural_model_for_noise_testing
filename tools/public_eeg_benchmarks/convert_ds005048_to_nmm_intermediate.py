#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import hashlib
import json
from datetime import datetime, timezone
from pathlib import Path


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return f"sha256:{h.hexdigest()}"


def main() -> int:
    ap = argparse.ArgumentParser(description="Convert ds005048 source files into NMM intermediate ASSR benchmark layout.")
    ap.add_argument("--source-root", type=Path, required=True)
    ap.add_argument("--out-root", type=Path, required=True)
    ap.add_argument("--source-version", required=True)
    ap.add_argument("--conversion-tool-version", default="convert_ds005048_to_nmm_intermediate.py@v2")
    args = ap.parse_args()

    args.out_root.mkdir(parents=True, exist_ok=True)
    source_paths: list[str] = []
    source_hashes: dict[str, str] = {}
    intermediate_paths: list[str] = []
    intermediate_hashes: dict[str, str] = {}
    conversion_inputs_by_intermediate: dict[str, list[str]] = {}
    subjects: list[str] = []

    # Stage 8d-A source contract:
    # Source files are distinct from normalized intermediate files:
    # - source events:   sub-*/eeg/*_source_events.csv
    # - source signal:   sub-*/eeg/*_source_signal.csv
    # Converted outputs:
    # - intermediate events:     *_events.tsv
    # - intermediate timeseries: *_timeseries.csv
    for ev_src in sorted(args.source_root.glob("sub-*/eeg/*_source_events.csv")):
        sub = ev_src.parents[1].name
        sig_src = ev_src.parent / ev_src.name.replace("_source_events.csv", "_source_signal.csv")
        if not sig_src.exists():
            continue
        if sub not in subjects:
            subjects.append(sub)

        ev_rel_source = str(ev_src.relative_to(args.source_root))
        sig_rel_source = str(sig_src.relative_to(args.source_root))
        source_paths.extend([ev_rel_source, sig_rel_source])
        source_hashes[ev_rel_source] = _sha256(ev_src)
        source_hashes[sig_rel_source] = _sha256(sig_src)

        out_ev = args.out_root / ev_rel_source.replace("_source_events.csv", "_events.tsv")
        out_ts = args.out_root / sig_rel_source.replace("_source_signal.csv", "_timeseries.csv")
        out_ev.parent.mkdir(parents=True, exist_ok=True)

        # Convert source events CSV into normalized events TSV.
        with ev_src.open("r", encoding="utf-8", newline="") as f_in:
            src_ev_rows = list(csv.DictReader(f_in))
        with out_ev.open("w", encoding="utf-8", newline="") as f_out:
            w = csv.DictWriter(f_out, fieldnames=["onset", "duration", "modulation_rate_hz", "condition_label"], delimiter="\t")
            w.writeheader()
            for r in src_ev_rows:
                w.writerow(
                    {
                        "onset": r["onset_sec"],
                        "duration": r["duration_sec"],
                        "modulation_rate_hz": r["mod_rate_hz"],
                        "condition_label": r.get("condition_label", ""),
                    }
                )

        # Convert source signal CSV into normalized timeseries CSV.
        with sig_src.open("r", encoding="utf-8", newline="") as f_in:
            src_sig_rows = list(csv.DictReader(f_in))
        with out_ts.open("w", encoding="utf-8", newline="") as f_out:
            w = csv.DictWriter(f_out, fieldnames=["time", "sample_rate_hz", "eeg"])
            w.writeheader()
            for r in src_sig_rows:
                w.writerow(
                    {
                        "time": r["t_sec"],
                        "sample_rate_hz": r["sr_hz"],
                        "eeg": r["signal_uv"],
                    }
                )

        ev_rel_out = str(out_ev.relative_to(args.out_root))
        ts_rel_out = str(out_ts.relative_to(args.out_root))
        intermediate_paths.extend([ev_rel_out, ts_rel_out])
        intermediate_hashes[ev_rel_out] = _sha256(out_ev)
        intermediate_hashes[ts_rel_out] = _sha256(out_ts)
        conversion_inputs_by_intermediate[ev_rel_out] = [ev_rel_source]
        conversion_inputs_by_intermediate[ts_rel_out] = [sig_rel_source]

    if not subjects:
        raise SystemExit("no source sub-*/eeg/*_source_events.csv and *_source_signal.csv files found")

    manifest = {
        "dataset_id": "ds005048",
        "source_dataset_id": "ds005048",
        "source_dataset_version": args.source_version,
        "source_root_ref": str(args.source_root.resolve()),
        "source_paths": source_paths,
        "source_file_hashes": source_hashes,
        "intermediate_paths": intermediate_paths,
        "intermediate_file_hashes": intermediate_hashes,
        "conversion_tool_version": args.conversion_tool_version,
        "conversion_timestamp": datetime.now(timezone.utc).isoformat(),
        "subjects": sorted(subjects),
        "conversion_inputs_by_intermediate": conversion_inputs_by_intermediate,
        "provenance_status_hint": "intermediate_verified",
    }
    (args.out_root / "nmm_benchmark_manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote provenance manifest: {args.out_root / 'nmm_benchmark_manifest.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
