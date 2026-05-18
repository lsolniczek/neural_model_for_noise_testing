#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import hashlib
import json
import struct
from datetime import datetime, timezone
from pathlib import Path


TASK_STEM = "task-40HzAuditoryEntrainment"


def _sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return f"sha256:{h.hexdigest()}"


def _read_rows(path: Path, delimiter: str = ",") -> list[dict[str, str]]:
    with path.open("r", encoding="utf-8", newline="") as f:
        return list(csv.DictReader(f, delimiter=delimiter))


def _subject_files(source_root: Path, subject: str) -> dict[str, Path]:
    eeg_dir = source_root / subject / "eeg"
    stem = f"{subject}_{TASK_STEM}"
    files = {
        "set": eeg_dir / f"{stem}_eeg.set",
        "fdt": eeg_dir / f"{stem}_eeg.fdt",
        "json": eeg_dir / f"{stem}_eeg.json",
        "events": eeg_dir / f"{stem}_events.tsv",
        "channels": eeg_dir / f"{stem}_channels.tsv",
    }
    missing = [str(p) for p in files.values() if not p.exists()]
    if missing:
        raise SystemExit(f"missing required ds005048 source files for {subject}: {missing}")
    return files


def _load_sampling_rate(eeg_json_path: Path) -> float:
    meta = json.loads(eeg_json_path.read_text(encoding="utf-8"))
    sr = float(meta.get("SamplingFrequency", 0.0))
    if sr <= 0:
        raise SystemExit(f"invalid SamplingFrequency in {eeg_json_path}")
    return sr


def _load_channel_names(channels_path: Path) -> list[str]:
    rows = _read_rows(channels_path, delimiter="\t")
    names = [r.get("name", "").strip() for r in rows if r.get("name", "").strip()]
    if not names:
        raise SystemExit(f"no channel names found in {channels_path}")
    return names


def _read_fdt_channel_signal(fdt_path: Path, n_channels: int, channel_index: int) -> list[float]:
    raw = fdt_path.read_bytes()
    if len(raw) % 4 != 0:
        raise SystemExit(f"invalid fdt byte length (not multiple of 4): {fdt_path}")
    n_float = len(raw) // 4
    if n_float % n_channels != 0:
        raise SystemExit(
            f"fdt float count {n_float} not divisible by channel count {n_channels}: {fdt_path}"
        )
    floats = struct.unpack("<" + ("f" * n_float), raw)
    # EEGLAB .fdt data matrix is typically [nbchan x pnts] stored column-major.
    # This extracts one deterministic channel across all samples.
    signal = [floats[i] for i in range(channel_index, n_float, n_channels)]
    return signal


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Convert real ds005048 BIDS/EEGLAB files into NMM intermediate ASSR benchmark layout."
    )
    ap.add_argument("--source-root", type=Path, required=True)
    ap.add_argument("--out-root", type=Path, required=True)
    ap.add_argument("--source-version", required=True)
    ap.add_argument(
        "--conversion-tool-version",
        default="convert_ds005048_to_nmm_intermediate.py@v3_real_bids_eeglab",
    )
    args = ap.parse_args()

    source_root = args.source_root
    out_root = args.out_root
    out_root.mkdir(parents=True, exist_ok=True)

    task_event_meta = source_root / f"{TASK_STEM}_events.json"
    if not task_event_meta.exists():
        raise SystemExit(f"missing dataset task metadata: {task_event_meta}")
    task_event_meta_doc = json.loads(task_event_meta.read_text(encoding="utf-8"))

    subjects = sorted(
        p.name for p in source_root.glob("sub-*") if p.is_dir() and p.name.startswith("sub-")
    )
    if not subjects:
        raise SystemExit("no sub-* directories found in source root")

    source_paths: list[str] = []
    source_hashes: dict[str, str] = {}
    intermediate_paths: list[str] = []
    intermediate_hashes: dict[str, str] = {}
    conversion_inputs_by_intermediate: dict[str, list[str]] = {}
    converted_subjects: list[str] = []

    for subject in subjects:
        files = _subject_files(source_root, subject)
        sr_hz = _load_sampling_rate(files["json"])
        channel_names = _load_channel_names(files["channels"])
        chosen_channel = channel_names[0]
        signal = _read_fdt_channel_signal(files["fdt"], n_channels=len(channel_names), channel_index=0)

        # Record source lineage for all required subject inputs.
        source_rel = {k: str(v.relative_to(source_root)) for k, v in files.items()}
        for rel in source_rel.values():
            if rel not in source_hashes:
                source_paths.append(rel)
                source_hashes[rel] = _sha256(source_root / rel)

        # Convert events: keep only Stimulus rows, set 40 Hz by dataset-task contract.
        events_rows = _read_rows(files["events"], delimiter="\t")
        out_eeg_dir = out_root / subject / "eeg"
        out_eeg_dir.mkdir(parents=True, exist_ok=True)
        out_ev = out_eeg_dir / f"{subject}_{TASK_STEM}_events.tsv"
        out_ts = out_eeg_dir / f"{subject}_{TASK_STEM}_timeseries.csv"

        with out_ev.open("w", encoding="utf-8", newline="") as f_out:
            writer = csv.DictWriter(
                f_out,
                fieldnames=["onset", "duration", "modulation_rate_hz", "condition_label"],
                delimiter="\t",
            )
            writer.writeheader()
            for row in events_rows:
                if (row.get("trial_type", "") or "").strip() != "Stimulus":
                    continue
                writer.writerow(
                    {
                        "onset": row.get("onset", ""),
                        "duration": row.get("duration", ""),
                        "modulation_rate_hz": "40.0",
                        "condition_label": "Stimulus",
                    }
                )

        with out_ts.open("w", encoding="utf-8", newline="") as f_out:
            writer = csv.DictWriter(f_out, fieldnames=["time", "sample_rate_hz", "eeg"])
            writer.writeheader()
            for idx, value in enumerate(signal):
                writer.writerow(
                    {
                        "time": f"{idx / sr_hz:.9f}",
                        "sample_rate_hz": f"{sr_hz:.9f}",
                        "eeg": f"{value:.9f}",
                    }
                )

        ev_rel_out = str(out_ev.relative_to(out_root))
        ts_rel_out = str(out_ts.relative_to(out_root))
        intermediate_paths.extend([ev_rel_out, ts_rel_out])
        intermediate_hashes[ev_rel_out] = _sha256(out_ev)
        intermediate_hashes[ts_rel_out] = _sha256(out_ts)
        conversion_inputs_by_intermediate[ev_rel_out] = [source_rel["events"], str(task_event_meta.relative_to(source_root))]
        conversion_inputs_by_intermediate[ts_rel_out] = [source_rel["fdt"], source_rel["set"], source_rel["json"], source_rel["channels"]]
        converted_subjects.append(subject)

    # Track root-level metadata used in conversion.
    task_meta_rel = str(task_event_meta.relative_to(source_root))
    if task_meta_rel not in source_hashes:
        source_paths.append(task_meta_rel)
        source_hashes[task_meta_rel] = _sha256(task_event_meta)

    manifest = {
        "dataset_id": "ds005048",
        "source_dataset_id": "ds005048",
        "source_dataset_version": args.source_version,
        "source_root_ref": str(source_root.resolve()),
        "source_paths": source_paths,
        "source_file_hashes": source_hashes,
        "intermediate_paths": intermediate_paths,
        "intermediate_file_hashes": intermediate_hashes,
        "conversion_tool_version": args.conversion_tool_version,
        "conversion_timestamp": datetime.now(timezone.utc).isoformat(),
        "subjects": converted_subjects,
        "conversion_inputs_by_intermediate": conversion_inputs_by_intermediate,
        "provenance_status_hint": "source_verified",
        "source_contract": {
            "task_stem": TASK_STEM,
            "sampling_rate_source": "*_eeg.json:SamplingFrequency",
            "channel_policy": "first channel listed in *_channels.tsv",
            "event_mapping": "trial_type == Stimulus -> benchmark event row with modulation_rate_hz=40.0",
            "semantic_source_inputs": ["*_eeg.fdt", "*_eeg.json", "*_events.tsv", "*_channels.tsv", "task-40HzAuditoryEntrainment_events.json"],
            "lineage_only_inputs": ["*_eeg.set"],
            "task_events_metadata_file": task_meta_rel,
            "task_events_metadata_snapshot": task_event_meta_doc,
        },
    }
    manifest_path = out_root / "nmm_benchmark_manifest.json"
    manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    print(f"Wrote provenance manifest: {manifest_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
