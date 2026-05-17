from __future__ import annotations

import csv
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, List, Protocol


REPO_ROOT = Path(__file__).resolve().parents[2]
REGISTRY_PATH = REPO_ROOT / "benchmarks" / "public_eeg" / "datasets_v1.json"
FIXTURE_ROWS_PATH = REPO_ROOT / "benchmarks" / "public_eeg" / "fixtures" / "benchmark_rows_fixture.csv"


COMMON_ROW_FIELDS = [
    "dataset_id",
    "subject_id",
    "trial_id",
    "condition_id",
    "stimulus_type",
    "modulation_rate_hz",
    "carrier_or_task_label",
    "eeg_signal_ref",
    "behavior_available",
    "expected_effect_label",
]


class DatasetNotDownloadedError(RuntimeError):
    pass


class PublicEegAdapter(Protocol):
    dataset_id: str

    def load_subjects(self) -> List[str]:
        ...

    def iter_trials(self) -> Iterable[Dict[str, str]]:
        ...

    def export_benchmark_rows(self) -> List[Dict[str, str]]:
        ...


@dataclass
class RegistryDataset:
    dataset_id: str
    name: str
    source_url: str
    task: str
    nmm_components_supported: List[str]
    limitations: List[str]
    download_status: str


def load_registry(path: Path = REGISTRY_PATH) -> Dict:
    return json.loads(path.read_text(encoding="utf-8"))


def get_dataset_entry(dataset_id: str) -> Dict:
    reg = load_registry()
    for d in reg["datasets"]:
        if d["dataset_id"] == dataset_id:
            return d
    raise KeyError(f"Unknown dataset_id '{dataset_id}'")


def ensure_downloaded(dataset_id: str) -> Dict:
    entry = get_dataset_entry(dataset_id)
    if entry.get("download_status") != "downloaded":
        raise DatasetNotDownloadedError(
            f"dataset_not_downloaded: dataset_id={dataset_id}, status={entry.get('download_status')}"
        )
    return entry


class FixtureAdapter:
    dataset_id = "fixture_public_eeg"

    def __init__(self, benchmark_family: str) -> None:
        self.benchmark_family = benchmark_family

    def load_subjects(self) -> List[str]:
        rows = self.export_benchmark_rows()
        return sorted({r["subject_id"] for r in rows})

    def iter_trials(self) -> Iterable[Dict[str, str]]:
        return iter(self.export_benchmark_rows())

    def export_benchmark_rows(self) -> List[Dict[str, str]]:
        with FIXTURE_ROWS_PATH.open("r", encoding="utf-8", newline="") as f:
            rows = [r for r in csv.DictReader(f) if r["benchmark_family"] == self.benchmark_family]
        out: List[Dict[str, str]] = []
        for r in rows:
            out.append({k: r[k] for k in COMMON_ROW_FIELDS})
        return out


def write_markdown_report(output: Path, title: str, lines: List[str]) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text("# " + title + "\n\n" + "\n".join(lines) + "\n", encoding="utf-8")


def write_result_json(output: Path, payload: Dict) -> None:
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")
