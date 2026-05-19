# Practical Evaluation Contract (Stage 8 Practical-E)

## Purpose

The `evaluate` workflow checks how well a preset aligns with NMM model proxies for a selected goal and brain type.
It is intended to:

- compare presets under the same model settings,
- surface obvious proxy mismatches quickly,
- provide machine-readable output for API/UI/automation.

It does **not** prove:

- clinical benefit,
- ADHD treatment effect,
- cognitive enhancement,
- improved sleep outcomes,
- guaranteed user experience.

This is a model-proxy evaluation contract, not human efficacy validation.

## CLI Usage

Create the output directory first:

```bash
mkdir -p reports
```

Then run one of:

```bash
cargo run -- evaluate presets/isolation_normal_clean.json --goal isolation --brain-type normal --duration 10 --json-report reports/isolation_normal.json
cargo run -- evaluate presets/the_shield_v5.json --goal shield --brain-type normal --duration 10 --json-report reports/shield_normal.json
cargo run -- evaluate presets/deepwork_adhd.json --goal deepwork --brain-type adhd --duration 10 --json-report reports/deepwork_adhd.json
```

Notes:

- `--json-report` is supported only for **single goal + single brain type** evaluation.
- `--goal all` with `--json-report` is rejected.
- `--brain-type all` with `--json-report` is rejected.
- The parent directory for `--json-report` must already exist.
- Duration must exceed warm-up discard so the run is valid.

## Stable JSON Fields

The current single-evaluation consumer contract includes:

- `preset_path`: evaluated preset file path.
- `goal`: stable goal id (snake_case, e.g. `shield`, `isolation`).
- `brain_type`: evaluated brain profile id/name used by the run.
- `score`: scalar proxy-alignment score for this goal/brain run.
- `practical_status`: user-facing status derived from score bands (e.g. strong/usable/weak/poor).
- `goal_semantics`: typed meaning/disclaimer block for the goal.
- `practical_report`: structured practical interpretation (status, intended use, reasons, interpretation, limitation).
- `band_powers`: modeled delta/theta/alpha/beta/gamma proportions.
- `dominant_frequency_hz`: modeled dominant frequency diagnostic.
- `fhn_firing_rate`: model diagnostic from the FHN layer.
- `fhn_isi_cv`: model diagnostic for spike interval variability.
- `acoustic_summary`: whether acoustic masking/comfort scoring was run and, if run, summary values.
- `model_signature`: model/version/config signature for comparability.
- `limitations`: explicit limitations/disclaimer list.

## Interpretation Rules

- Higher `score` means better alignment with the model proxy for the chosen goal.
- `practical_status` is the primary high-level summary for user-facing views.
- `band_powers` are **modeled** EEG-band proxies, not measured human EEG.
- `dominant_frequency_hz`, `fhn_firing_rate`, and `fhn_isi_cv` are model diagnostics.
- `acoustic_summary.status = "not_scored"` means acoustic masking/comfort was not evaluated in that run.
- `acoustic_summary.status = "scored"` means acoustic submetrics are present, but still model/product proxies.

## Goal Semantics

`goal_semantics` is part of the required contract so API/UI consumers can present:

- what the goal is for,
- what proxies are being optimized,
- what the output does **not** prove.

Consumer surfaces should display unsupported claims/disclaimers next to user-facing results.

## Example JSON Excerpt

```json
{
  "preset_path": "presets/isolation_normal_clean.json",
  "goal": "isolation",
  "brain_type": "Normal",
  "score": 0.4153,
  "practical_status": "weak",
  "goal_semantics": {
    "goal": "isolation",
    "evidence_level": "practical_model_heuristic"
  },
  "acoustic_summary": {
    "status": "not_scored",
    "note": "Acoustic masking/comfort: not scored in this run."
  }
}
```

## Consumer Guidance (API/UI/Automation)

- Prefer `practical_status` and `practical_report` for user-facing summaries.
- Use raw diagnostics (`band_powers`, FHN metrics, dominant frequency) in advanced/debug views.
- Always show `limitations` in user-visible contexts.
- Do not transform this output into medical or clinical claims.
- Do not compare reports across different `model_signature.version` values (or materially different model flags) without warning.

## Non-Goals of This Stage

This stage does **not** provide:

- human validation,
- clinical diagnosis or treatment claims,
- real EEG prediction,
- multi-goal/multi-brain JSON matrix output,
- batch JSON export.

## Next: Stage 8 Practical-F

Next practical step is batch/matrix structured JSON output, built on this single-report contract.

Requirements for Stage 8 Practical-F:

- preserve compatibility with this document’s field contract where feasible,
- keep single-report semantics/limitations intact,
- extend format for multi-row workflows without weakening disclaimers.
