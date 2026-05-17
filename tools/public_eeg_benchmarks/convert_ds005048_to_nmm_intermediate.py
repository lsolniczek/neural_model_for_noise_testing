#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
from datetime import datetime, timezone
from pathlib import Path


def main() -> int:
    ap = argparse.ArgumentParser(description="Create provenance-preserving intermediate manifest for ds005048 conversion.")
    ap.add_argument("--source-root", type=Path, required=True)
    ap.add_argument("--out-root", type=Path, required=True)
    ap.add_argument("--source-version", required=True)
    args = ap.parse_args()

    args.out_root.mkdir(parents=True, exist_ok=True)
    # Conversion of raw BIDS files to *_timeseries.csv is intentionally not implemented here.
    # This scaffold emits declared metadata only; it is not evidence-usable provenance.
    manifest = {
        "dataset_id": "ds005048",
        "source_dataset_id": "ds005048",
        "source_dataset_version": args.source_version,
        "source_paths": [],
        "source_file_hashes": {},
        "intermediate_paths": [],
        "intermediate_file_hashes": {},
        "conversion_tool_version": "convert_ds005048_to_nmm_intermediate.py@v1",
        "conversion_timestamp": datetime.now(timezone.utc).isoformat(),
        "subjects": [],
        "provenance_status_hint": "declared_only",
    }
    (args.out_root / "nmm_benchmark_manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote provenance manifest: {args.out_root / 'nmm_benchmark_manifest.json'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
