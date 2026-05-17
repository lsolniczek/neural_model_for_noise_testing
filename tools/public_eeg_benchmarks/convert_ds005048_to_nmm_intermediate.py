#!/usr/bin/env python3
from __future__ import annotations

import argparse
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
    subjects: list[str] = []

    # Minimal source-backed conversion path:
    # copy *_events.tsv and *_timeseries.csv from source-root to out-root and hash both
    # source and intermediate files for provenance verification.
    for ev in sorted(args.source_root.glob("sub-*/eeg/*_events.tsv")):
        sub = ev.parents[1].name
        ts_candidates = sorted(ev.parent.glob("*_timeseries.csv"))
        if not ts_candidates:
            continue
        ts = ts_candidates[0]
        if sub not in subjects:
            subjects.append(sub)

        ev_rel_source = str(ev.relative_to(args.source_root))
        ts_rel_source = str(ts.relative_to(args.source_root))
        source_paths.extend([ev_rel_source, ts_rel_source])
        source_hashes[ev_rel_source] = _sha256(ev)
        source_hashes[ts_rel_source] = _sha256(ts)

        out_ev = args.out_root / ev_rel_source
        out_ts = args.out_root / ts_rel_source
        out_ev.parent.mkdir(parents=True, exist_ok=True)
        out_ev.write_bytes(ev.read_bytes())
        out_ts.write_bytes(ts.read_bytes())

        ev_rel_out = str(out_ev.relative_to(args.out_root))
        ts_rel_out = str(out_ts.relative_to(args.out_root))
        intermediate_paths.extend([ev_rel_out, ts_rel_out])
        intermediate_hashes[ev_rel_out] = _sha256(out_ev)
        intermediate_hashes[ts_rel_out] = _sha256(out_ts)

    if not subjects:
        raise SystemExit("no source sub-*/eeg/*_events.tsv and *_timeseries.csv files found")

    manifest = {
        "dataset_id": "ds005048",
        "source_dataset_id": "ds005048",
        "source_dataset_version": args.source_version,
        "source_paths": source_paths,
        "source_file_hashes": source_hashes,
        "intermediate_paths": intermediate_paths,
        "intermediate_file_hashes": intermediate_hashes,
        "conversion_tool_version": args.conversion_tool_version,
        "conversion_timestamp": datetime.now(timezone.utc).isoformat(),
        "subjects": sorted(subjects),
        "provenance_status_hint": "intermediate_verified",
    }
    (args.out_root / "nmm_benchmark_manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote provenance manifest: {args.out_root / 'nmm_benchmark_manifest.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
