# Stage 8d-B: Deterministic ASSR Prediction Bridge (A2 Accepted)

## Objective

Document and preserve the accepted Stage 8d-B A2 implementation:

- keep the source-verified `ds005048` observation path from Stage 8d-A unchanged,
- expose deterministic **condition-level** model output for ASSR surrogate strength,
- keep dominant-rate and downstream comparison metrics explicitly unavailable.

No runtime preset scoring changes.

## Accepted Stage 8d-B decision

Stage 8d-B uses **A2**:

- no independent model dominant-rate estimator is exposed in this stage,
- direct echo of stimulus label is not accepted as prediction,
- therefore dominant-rate prediction and dominant-rate comparison remain unavailable,
- bridge output is limited to condition-level surrogate strength.

## Scientific boundary

Stage 8d-B may claim only:

> deterministic condition-level model surrogate strength can be exported alongside source-verified ASSR observations.

Stage 8d-B must not claim:

- subject-specific EEG prediction,
- dominant-rate prediction validity,
- target-rate recovery agreement validity,
- control/rank/sign strength agreement validity,
- clinical or preset efficacy validation.

## Current output contract (accepted)

### Available now

1. `predicted_gamma_assr_response_strength`
   - deterministic
   - condition-level
   - surrogate, not same-scale EEG power

2. Bridge audit metadata
   - `prediction_level`
   - `bridge_version`
   - `model_version`
   - `strength_scale`

### Explicitly unavailable now

1. `predicted_dominant_modulation_hz`
2. `predicted_target_rate_recovery_accuracy`
3. `prediction_observation_target_rate_agreement`
4. `predicted_target_vs_control_strength_delta`
5. `prediction_observation_condition_rank_agreement`
6. `prediction_observation_strength_delta_sign_agreement`

Reason codes must remain explicit (for example:
`unavailable_no_independent_model_rate_estimator_stage8d_b`,
`unavailable_no_control_rows`,
`unavailable_surrogate_strength`).

## Why dominant-rate comparison is unavailable

- The current Stage 8d-B bridge does not expose an independent dominant-response frequency estimator from model dynamics.
- Any direct reuse/echo of benchmark condition labels is tautological and not acceptable as model prediction.
- Therefore dominant-rate recovery and row-level target-rate agreement are not valid Stage 8d-B metrics.

## Required artifacts (current behavior)

Keep:

- `assr_prediction_rows.csv`
- `assr_prediction_metrics.csv`
- `assr_comparison_metrics.csv`
- `assr_failure_cases.csv`
- `assr_benchmark_result.json`
- `assr_benchmark_report.md`

And ensure they encode unavailable dominant-rate/comparison fields honestly.

## Pseudocode (A2-consistent)

```python
def predict_condition(row):
    model_out = run_candidate_assr_bridge(row["modulation_rate_hz"])
    return {
        "trial_id": row["trial_id"],
        "condition_id": row["condition_id"],
        "predicted_dominant_modulation_hz": unavailable(
            "no_independent_model_rate_estimator_stage8d_b"
        ),
        "predicted_gamma_assr_response_strength": model_out.gamma_response_strength,
        "prediction_level": "condition_level",
        "prediction_status": "model_derived_condition_level",
        "strength_scale": "surrogate_not_same_scale_eeg_power",
        "bridge_version": model_out.bridge_version,
        "model_version": model_out.model_version,
    }

def comparison_metrics(observed_rows, predicted_rows):
    return {
        "predicted_target_rate_recovery_accuracy": unavailable(
            "no_independent_model_rate_estimator_stage8d_b"
        ),
        "prediction_observation_target_rate_agreement": unavailable(
            "no_independent_model_rate_estimator_stage8d_b"
        ),
        "predicted_target_vs_control_strength_delta": unavailable("no_control_rows"),
        "prediction_observation_condition_rank_agreement": unavailable("no_control_rows"),
        "prediction_observation_strength_delta_sign_agreement": unavailable(
            "surrogate_not_same_scale_eeg_power"
        ),
    }
```

## Tests required (A2-consistent)

1. Deterministic repeated surrogate predictions for identical condition inputs.
2. No fake subject-specific variation.
3. Dominant-rate prediction fields are unavailable with explicit status.
4. Dominant-rate comparison metrics are unavailable with explicit status.
5. Control/rank/sign comparisons remain unavailable for `ds005048`.
6. Bridge metadata appears in `assr_benchmark_result.json`.
7. Fixture runs remain engineering-only.
8. Real runs preserve Stage 8d-A provenance semantics.
9. Runtime preset scoring remains unchanged.

## Acceptance criteria (A2)

Stage 8d-B is complete when:

1. Placeholder prediction rows are replaced by model-derived **surrogate strength** rows.
2. Prediction level is explicitly `condition_level`.
3. Dominant-rate prediction/comparison are explicitly unavailable (no tautological metric).
4. Unsupported comparisons remain unavailable with reason codes.
5. Bridge audit metadata is present in summary JSON and row outputs.
6. No subject-specific claim is introduced.
7. Stage 8d-A provenance behavior remains unchanged.
8. Tests pass.

## Out of scope

1. Independent dominant-rate model estimator.
2. Subject-level calibration.
3. New datasets or benchmark families.
4. Preset scoring redesign or promotion logic.
5. Stage 9 work.
