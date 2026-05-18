from __future__ import annotations

import csv
import hashlib
import json
import math
from dataclasses import dataclass
from pathlib import Path
from typing import Dict, Iterable, List


class DatasetLayoutError(RuntimeError):
    pass


MIN_EPOCH_DURATION_S = 1.0
TARGET_RECOVERY_TOLERANCE_HZ = 2.0


@dataclass
class Ds005048Trial:
    dataset_id: str
    subject_id: str
    trial_id: str
    condition_id: str
    stimulus_type: str
    modulation_rate_hz: float
    carrier_or_task_label: str
    eeg_signal_ref: str
    behavior_available: bool
    expected_effect_label: str
    condition_label: str
    observed_dominant_modulation_hz: float
    observed_target_band_power: float
    observed_assr_strength: float
    observed_target_frequency_amplitude_ratio: float
    epoch_duration_s: float
    frequency_resolution_hz: float
    resolution_sufficient_for_target_metric: bool

    def as_dict(self) -> Dict[str, str]:
        return {
            "dataset_id": self.dataset_id,
            "subject_id": self.subject_id,
            "trial_id": self.trial_id,
            "condition_id": self.condition_id,
            "stimulus_type": self.stimulus_type,
            "modulation_rate_hz": f"{self.modulation_rate_hz:.6f}",
            "carrier_or_task_label": self.carrier_or_task_label,
            "eeg_signal_ref": self.eeg_signal_ref,
            "behavior_available": "true" if self.behavior_available else "false",
            "expected_effect_label": self.expected_effect_label,
            "condition_label": self.condition_label,
            "observed_dominant_modulation_hz": f"{self.observed_dominant_modulation_hz:.6f}",
            "observed_target_band_power": f"{self.observed_target_band_power:.6f}",
            "observed_assr_strength": f"{self.observed_assr_strength:.6f}",
            "observed_target_frequency_amplitude_ratio": f"{self.observed_target_frequency_amplitude_ratio:.6f}",
            "epoch_duration_s": f"{self.epoch_duration_s:.6f}",
            "frequency_resolution_hz": f"{self.frequency_resolution_hz:.6f}",
            "resolution_sufficient_for_target_metric": "true" if self.resolution_sufficient_for_target_metric else "false",
        }


class Ds005048PreprocessedAdapter:
    dataset_id = "ds005048"

    def __init__(self, dataset_root: Path):
        self.dataset_root = dataset_root
        self._manifest = self._load_manifest()

    @staticmethod
    def _is_sha256(value: str) -> bool:
        if not isinstance(value, str) or not value.startswith("sha256:"):
            return False
        h = value.split(":", 1)[1]
        return len(h) == 64 and all(c in "0123456789abcdefABCDEF" for c in h)

    def _hash_file(self, path: Path) -> str:
        h = hashlib.sha256()
        with path.open("rb") as f:
            for chunk in iter(lambda: f.read(1024 * 1024), b""):
                h.update(chunk)
        return f"sha256:{h.hexdigest()}"

    def _load_manifest(self) -> Dict:
        path = self.dataset_root / "nmm_benchmark_manifest.json"
        if not path.exists():
            raise DatasetLayoutError(
                f"missing manifest: {path}. Expected nmm_benchmark_manifest.json in dataset root."
            )
        return json.loads(path.read_text(encoding="utf-8"))

    def verify_layout(self) -> None:
        if self._manifest.get("dataset_id") != "ds005048":
            raise DatasetLayoutError("manifest dataset_id must be ds005048")
        if self._manifest.get("source_dataset_id") != "ds005048":
            raise DatasetLayoutError("source_dataset_id must be ds005048")
        if not str(self._manifest.get("source_dataset_version", "")).strip():
            raise DatasetLayoutError("source_dataset_version must be non-empty")
        source_paths = self._manifest.get("source_paths")
        if not isinstance(source_paths, list) or not source_paths:
            raise DatasetLayoutError("source_paths must be a non-empty list")
        if any((not isinstance(p, str)) or (not p.strip()) for p in source_paths):
            raise DatasetLayoutError("all source_paths entries must be non-empty strings")
        source_hashes = self._manifest.get("source_file_hashes")
        if not isinstance(source_hashes, dict) or not source_hashes:
            raise DatasetLayoutError("source_file_hashes must be a non-empty object")
        for p in source_paths:
            if p not in source_hashes or not str(source_hashes[p]).strip():
                raise DatasetLayoutError("source_file_hashes must cover all source_paths with non-empty values")
        if not str(self._manifest.get("conversion_tool_version", "")).strip():
            raise DatasetLayoutError("conversion_tool_version must be non-empty")
        if not str(self._manifest.get("conversion_timestamp", "")).strip():
            raise DatasetLayoutError("conversion_timestamp must be non-empty")
        if "subjects" not in self._manifest or not self._manifest["subjects"]:
            raise DatasetLayoutError("manifest subjects list is missing/empty")
        for s in self._manifest["subjects"]:
            sub = self.dataset_root / s
            eeg = sub / "eeg"
            if not eeg.exists():
                raise DatasetLayoutError(f"missing eeg folder: {eeg}")
            if not list(eeg.glob("*_events.tsv")):
                raise DatasetLayoutError(f"missing events.tsv in: {eeg}")
            if not list(eeg.glob("*_timeseries.csv")):
                raise DatasetLayoutError(f"missing timeseries csv in: {eeg}")

    def compute_provenance_status(self, consumed_relative_paths: List[str]) -> str:
        # provenance_status contract: fixture | declared_only | intermediate_verified | source_verified
        required = [
            "source_dataset_id",
            "source_dataset_version",
            "source_paths",
            "source_file_hashes",
            "intermediate_paths",
            "intermediate_file_hashes",
            "conversion_tool_version",
            "conversion_timestamp",
        ]
        for key in required:
            if key not in self._manifest:
                return "declared_only"
        intermediate_paths = self._manifest.get("intermediate_paths")
        intermediate_hashes = self._manifest.get("intermediate_file_hashes")
        conversion_inputs = self._manifest.get("conversion_inputs_by_intermediate")
        if not isinstance(intermediate_paths, list) or not intermediate_paths:
            return "declared_only"
        if not isinstance(intermediate_hashes, dict) or not intermediate_hashes:
            return "declared_only"
        if conversion_inputs is not None and (not isinstance(conversion_inputs, dict) or not conversion_inputs):
            return "declared_only"
        listed = {str(p) for p in intermediate_paths if isinstance(p, str) and p.strip()}
        if not all(p in listed for p in consumed_relative_paths):
            return "declared_only"
        for p in consumed_relative_paths:
            expected = intermediate_hashes.get(p, "")
            if not self._is_sha256(expected):
                return "declared_only"
            actual_path = self.dataset_root / p
            if not actual_path.exists():
                return "declared_only"
            if self._hash_file(actual_path) != expected:
                raise DatasetLayoutError(f"intermediate hash mismatch for consumed file '{p}'")
        source_root_ref = str(self._manifest.get("source_root_ref", "")).strip()
        if not source_root_ref:
            return "intermediate_verified"
        source_root = Path(source_root_ref)
        if not source_root.exists():
            return "intermediate_verified"
        source_paths = self._manifest.get("source_paths")
        source_hashes = self._manifest.get("source_file_hashes")
        if not isinstance(source_paths, list) or not source_paths:
            return "intermediate_verified"
        if not isinstance(source_hashes, dict) or not source_hashes:
            return "intermediate_verified"
        # Source coverage rule: every consumed intermediate must declare every source input used to generate it.
        # Missing conversion-input links prevent source_verified even when hashes exist.
        if isinstance(conversion_inputs, dict):
            for p in consumed_relative_paths:
                mapped_sources = conversion_inputs.get(p)
                if not isinstance(mapped_sources, list) or not mapped_sources:
                    return "intermediate_verified"
                for src_p in mapped_sources:
                    if not isinstance(src_p, str) or not src_p.strip():
                        return "intermediate_verified"
                    if src_p not in source_paths:
                        return "intermediate_verified"
                    expected_src_hash = source_hashes.get(src_p, "")
                    if not self._is_sha256(expected_src_hash):
                        return "intermediate_verified"
        else:
            return "intermediate_verified"
        for p in source_paths:
            if not isinstance(p, str) or not p.strip():
                return "intermediate_verified"
            expected = source_hashes.get(p, "")
            if not self._is_sha256(expected):
                return "intermediate_verified"
            actual_path = source_root / p
            if not actual_path.exists():
                return "intermediate_verified"
            if self._hash_file(actual_path) != expected:
                raise DatasetLayoutError(f"source hash mismatch for source file '{p}'")
        return "source_verified"

    def load_subjects(self) -> List[str]:
        self.verify_layout()
        return list(self._manifest["subjects"])

    def iter_trials(self) -> Iterable[Dict[str, str]]:
        yield from self.export_benchmark_rows()

    def export_benchmark_rows(self) -> List[Dict[str, str]]:
        self.verify_layout()
        rows: List[Dict[str, str]] = []
        consumed_rel_paths: List[str] = []
        for subject in self._manifest["subjects"]:
            eeg_dir = self.dataset_root / subject / "eeg"
            events_path = sorted(eeg_dir.glob("*_events.tsv"))[0]
            timeseries_path = sorted(eeg_dir.glob("*_timeseries.csv"))[0]
            consumed_rel_paths.extend(
                [
                    str(events_path.relative_to(self.dataset_root)),
                    str(timeseries_path.relative_to(self.dataset_root)),
                ]
            )
            for trial in self._extract_trials_from_pair(subject, timeseries_path, events_path):
                rows.append(trial.as_dict())
        self._last_provenance_status = self.compute_provenance_status(consumed_rel_paths)
        return rows

    def last_provenance_status(self) -> str:
        return getattr(self, "_last_provenance_status", "declared_only")

    def _extract_trials_from_pair(self, subject: str, timeseries_path: Path, events_path: Path) -> List[Ds005048Trial]:
        with timeseries_path.open("r", encoding="utf-8", newline="") as f:
            sig_rows = list(csv.DictReader(f))
        if not sig_rows:
            raise DatasetLayoutError(f"empty timeseries file: {timeseries_path}")

        sample_rate_hz = float(sig_rows[0].get("sample_rate_hz", "0") or "0")
        if sample_rate_hz <= 0:
            raise DatasetLayoutError(f"invalid sample_rate_hz in {timeseries_path}")
        signal = [float(r["eeg"]) for r in sig_rows]

        with events_path.open("r", encoding="utf-8", newline="") as f:
            ev_rows = list(csv.DictReader(f, delimiter="\t"))
        if not ev_rows:
            raise DatasetLayoutError(f"empty events file: {events_path}")

        trials: List[Ds005048Trial] = []
        for i, ev in enumerate(ev_rows):
            onset_s = float(ev["onset"])
            duration_s = float(ev["duration"])
            target_hz = float(ev["modulation_rate_hz"])
            nyquist_hz = sample_rate_hz / 2.0
            if nyquist_hz < (target_hz + TARGET_RECOVERY_TOLERANCE_HZ):
                raise DatasetLayoutError(
                    f"sample rate too low for target modulation frequency: sr={sample_rate_hz:.3f}Hz nyquist={nyquist_hz:.3f}Hz target={target_hz:.3f}Hz"
                )
            condition_label = ev.get("condition_label", "")
            start = int(onset_s * sample_rate_hz)
            end = int((onset_s + duration_s) * sample_rate_hz)
            seg = signal[start:end]
            epoch_duration_s = len(seg) / sample_rate_hz
            if epoch_duration_s < MIN_EPOCH_DURATION_S:
                raise DatasetLayoutError(
                    f"epoch too short for ASSR benchmark: {epoch_duration_s:.3f}s < {MIN_EPOCH_DURATION_S:.3f}s"
                )
            observed_hz, band_power, strength, amp_ratio, freq_res, res_ok = _compute_assr_observations(
                seg, sample_rate_hz, target_hz
            )
            trials.append(
                Ds005048Trial(
                    dataset_id="ds005048",
                    subject_id=subject,
                    trial_id=f"{subject}_{i:04d}",
                    condition_id=f"{condition_label}_{target_hz:.1f}",
                    stimulus_type="auditory_entrainment",
                    modulation_rate_hz=target_hz,
                    carrier_or_task_label=condition_label or "unknown",
                    eeg_signal_ref=f"{timeseries_path.name}:{start}-{end}",
                    behavior_available=False,
                    expected_effect_label="target_entrainment" if abs(target_hz - 40.0) <= 2.0 else "non_target",
                    condition_label=condition_label or "unknown",
                    observed_dominant_modulation_hz=observed_hz,
                    observed_target_band_power=band_power,
                    observed_assr_strength=strength,
                    observed_target_frequency_amplitude_ratio=amp_ratio,
                    epoch_duration_s=epoch_duration_s,
                    frequency_resolution_hz=freq_res,
                    resolution_sufficient_for_target_metric=res_ok,
                )
            )
        return trials


def _compute_assr_observations(signal: List[float], sample_rate_hz: float, target_hz: float) -> tuple[float, float, float, float, float, bool]:
    n = len(signal)
    mean_sig = sum(signal) / n
    centered = [x - mean_sig for x in signal]
    # simple DFT over 5..min(60, nyquist) Hz for deterministic offline baseline
    nyquist_hz = sample_rate_hz / 2.0
    max_analyzed_hz = min(60.0, nyquist_hz)
    upper = int(math.floor(max_analyzed_hz))
    if upper < 5:
        raise DatasetLayoutError("sample rate too low for ASSR analysis band")
    freqs = [float(f) for f in range(5, upper + 1)]
    power = []
    for f in freqs:
        re = 0.0
        im = 0.0
        for t, x in enumerate(centered):
            a = 2.0 * math.pi * f * (t / sample_rate_hz)
            re += x * math.cos(a)
            im -= x * math.sin(a)
        p = (re * re + im * im) / max(n, 1)
        power.append(p)
    idx = max(range(len(power)), key=lambda i: power[i])
    dominant = freqs[idx]

    target_bins = [power[i] for i, f in enumerate(freqs) if abs(f - target_hz) <= 1.0]
    base_bins = [power[i] for i, f in enumerate(freqs) if f < 20.0 or f > 50.0]
    target_band_power = sum(target_bins) / max(1, len(target_bins))
    baseline_power = sum(base_bins) / max(1, len(base_bins))
    strength = target_band_power / max(1e-12, baseline_power)

    # target-frequency amplitude ratio (not PLV; single-epoch amplitude surrogate)
    f = target_hz
    re = 0.0
    im = 0.0
    for t, x in enumerate(centered):
        a = 2.0 * math.pi * f * (t / sample_rate_hz)
        re += x * math.cos(a)
        im += x * math.sin(a)
    amp = math.sqrt(re * re + im * im)
    energy = math.sqrt(sum(x * x for x in centered))
    amp_ratio = amp / max(1e-12, energy)
    amp_ratio = max(0.0, min(1.0, amp_ratio))
    freq_res = sample_rate_hz / n
    resolution_ok = freq_res <= TARGET_RECOVERY_TOLERANCE_HZ / 2.0
    return dominant, target_band_power, strength, amp_ratio, freq_res, resolution_ok
