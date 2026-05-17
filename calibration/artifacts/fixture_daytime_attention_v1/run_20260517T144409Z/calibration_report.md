# Calibration Report

Run directory: `calibration/artifacts/fixture_daytime_attention_v1/run_20260517T144409Z`

## Held-out Metrics

| Outcome | Model | MAE | RMSE | R2 | MAE 95% CI |
|---|---|---:|---:|---:|---|
| aperiodic_exponent | acoustic_only | 0.7026 | 0.7085 | -1.9289 | [0.0825, 1.3086] |
| aperiodic_exponent | modulation_only | 0.0580 | 0.0639 | -1.9289 | [0.0193, 0.1080] |
| aperiodic_exponent | legacy_v1 | 0.0527 | 0.0586 | -1.9289 | [0.0088, 0.1080] |
| aperiodic_exponent | candidate_v2 | 0.0691 | 0.0750 | -1.9289 | [0.0405, 0.1083] |
| envelope_plv | acoustic_only | 0.8744 | 0.8817 | -1.3629 | [0.0750, 1.6621] |
| envelope_plv | modulation_only | 0.0652 | 0.0725 | -1.3629 | [0.0412, 0.0967] |
| envelope_plv | legacy_v1 | 0.0504 | 0.0577 | -1.3629 | [0.0141, 0.0960] |
| envelope_plv | candidate_v2 | 0.0549 | 0.0623 | -1.3629 | [0.0232, 0.0960] |
| assr_plv | acoustic_only | 0.9117 | 0.9217 | -0.6786 | [0.0525, 1.7600] |
| assr_plv | modulation_only | 0.0676 | 0.0776 | -0.6786 | [0.0373, 0.0824] |
| assr_plv | legacy_v1 | 0.0496 | 0.0597 | -0.6786 | [0.0253, 0.0752] |
| assr_plv | candidate_v2 | 0.0331 | 0.0432 | -0.6786 | [0.0029, 0.0720] |
| vigilance_accuracy | acoustic_only | 0.2436 | 0.2458 | -2.5789 | [0.0400, 0.4405] |
| vigilance_accuracy | modulation_only | 0.0247 | 0.0269 | -2.5789 | [0.0027, 0.0520] |
| vigilance_accuracy | legacy_v1 | 0.0240 | 0.0261 | -2.5789 | [0.0013, 0.0520] |
| vigilance_accuracy | candidate_v2 | 0.0365 | 0.0386 | -2.5789 | [0.0221, 0.0532] |
| reaction_time_ms | acoustic_only | 147.8869 | 149.3859 | -1.8560 | [19.7500, 272.1071] |
| reaction_time_ms | modulation_only | 13.4214 | 14.9204 | -1.8560 | [3.1762, 26.8000] |
| reaction_time_ms | legacy_v1 | 13.5319 | 15.0308 | -1.8560 | [3.3971, 26.8000] |
| reaction_time_ms | candidate_v2 | 17.3993 | 18.8982 | -1.8560 | [9.0440, 27.4264] |
| comfort_rating | acoustic_only | 2.7360 | 2.7619 | -3.5743 | [0.6000, 4.7054] |
| comfort_rating | modulation_only | 0.7740 | 0.7999 | -3.5743 | [0.6000, 0.9937] |
| comfort_rating | legacy_v1 | 0.9434 | 0.9694 | -3.5743 | [0.6000, 1.1202] |
| comfort_rating | candidate_v2 | 0.7558 | 0.7817 | -3.5743 | [0.6000, 0.9816] |
| masking_effectiveness_rating | acoustic_only | 3.5268 | 3.5451 | -5.3333 | [0.7250, 6.2536] |
| masking_effectiveness_rating | modulation_only | 0.5220 | 0.5403 | -5.3333 | [0.2440, 0.8600] |
| masking_effectiveness_rating | legacy_v1 | 0.5325 | 0.5508 | -5.3333 | [0.2650, 0.8600] |
| masking_effectiveness_rating | candidate_v2 | 0.6691 | 0.6874 | -5.3333 | [0.5127, 0.8676] |

## Failure Cases

Rows with `abs_error > 0.2`: 52

This report is offline calibration evidence only; it does not promote runtime defaults.
