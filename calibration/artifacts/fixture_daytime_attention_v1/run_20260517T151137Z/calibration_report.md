# Calibration Report

Run directory: `calibration/artifacts/fixture_daytime_attention_v1/run_20260517T151137Z`

## Development (CV) Metrics

| Outcome | Model | MAE | RMSE | R2 | MAE 95% CI |
|---|---|---:|---:|---:|---|
| aperiodic_exponent | acoustic_only | 1.2688 | 1.2747 | -1.9289 | [0.0825, 2.4408] |
| aperiodic_exponent | modulation_only | 0.0580 | 0.0639 | -1.9289 | [0.0193, 0.1080] |
| aperiodic_exponent | legacy_v1 | 0.0527 | 0.0586 | -1.9289 | [0.0088, 0.1080] |
| aperiodic_offset | acoustic_only | 0.2538 | 0.2562 | -1.2308 | [0.0225, 0.4808] |
| aperiodic_offset | modulation_only | 0.0209 | 0.0234 | -1.2308 | [0.0117, 0.0310] |
| aperiodic_offset | legacy_v1 | 0.0171 | 0.0195 | -1.2308 | [0.0074, 0.0300] |
| envelope_plv | acoustic_only | 0.5075 | 0.5148 | -1.3629 | [0.0750, 0.9283] |
| envelope_plv | modulation_only | 0.0652 | 0.0725 | -1.3629 | [0.0412, 0.0967] |
| envelope_plv | legacy_v1 | 0.0504 | 0.0577 | -1.3629 | [0.0141, 0.0960] |
| assr_plv | acoustic_only | 0.2538 | 0.2638 | -0.6786 | [0.0525, 0.4442] |
| assr_plv | modulation_only | 0.0676 | 0.0776 | -0.6786 | [0.0373, 0.0824] |
| assr_plv | legacy_v1 | 0.0496 | 0.0597 | -0.6786 | [0.0253, 0.0752] |
| alpha_peak_frequency_hz | acoustic_only | 7.6792 | 7.6948 | -0.0833 | [0.2250, 15.1250] |
| alpha_peak_frequency_hz | modulation_only | 0.3447 | 0.3603 | -0.0833 | [0.2250, 0.4560] |
| alpha_peak_frequency_hz | legacy_v1 | 0.3342 | 0.3498 | -0.0833 | [0.2250, 0.4350] |
| alpha_asymmetry | acoustic_only | 0.5208 | 0.5221 | -0.1875 | [0.0100, 1.0250] |
| alpha_asymmetry | modulation_only | 0.0319 | 0.0331 | -0.1875 | [0.0100, 0.0471] |
| alpha_asymmetry | legacy_v1 | 0.0312 | 0.0324 | -0.1875 | [0.0100, 0.0457] |
| vigilance_accuracy | acoustic_only | 0.7613 | 0.7634 | -2.5789 | [0.0400, 1.4758] |
| vigilance_accuracy | modulation_only | 0.0247 | 0.0269 | -2.5789 | [0.0027, 0.0520] |
| vigilance_accuracy | legacy_v1 | 0.0240 | 0.0261 | -2.5789 | [0.0013, 0.0520] |
| reaction_time_ms | acoustic_only | 431.3750 | 432.8740 | -1.8560 | [19.7500, 839.0833] |
| reaction_time_ms | modulation_only | 13.4214 | 14.9204 | -1.8560 | [3.1762, 26.8000] |
| reaction_time_ms | legacy_v1 | 13.5319 | 15.0308 | -1.8560 | [3.3971, 26.8000] |
| reaction_time_variability_ms | acoustic_only | 203.0000 | 203.5545 | -2.2857 | [9.0000, 395.3333] |
| reaction_time_variability_ms | modulation_only | 5.5855 | 6.1400 | -2.2857 | [0.5043, 12.0000] |
| reaction_time_variability_ms | legacy_v1 | 5.5066 | 6.0611 | -2.2857 | [0.3466, 12.0000] |
| comfort_rating | acoustic_only | 18.5292 | 18.5551 | -3.5743 | [0.6000, 36.2917] |
| comfort_rating | modulation_only | 0.7740 | 0.7999 | -3.5743 | [0.6000, 0.9937] |
| comfort_rating | legacy_v1 | 0.9434 | 0.9694 | -3.5743 | [0.6000, 1.1202] |
| irritation_rating | acoustic_only | 18.1958 | 18.2390 | -1.1419 | [0.2500, 35.9583] |
| irritation_rating | modulation_only | 0.4989 | 0.5421 | -1.1419 | [0.2500, 0.7215] |
| irritation_rating | legacy_v1 | 0.6508 | 0.6939 | -1.1419 | [0.2500, 0.8682] |
| masking_effectiveness_rating | acoustic_only | 7.6125 | 7.6308 | -5.3333 | [0.7250, 14.4250] |
| masking_effectiveness_rating | modulation_only | 0.5220 | 0.5403 | -5.3333 | [0.2440, 0.8600] |
| masking_effectiveness_rating | legacy_v1 | 0.5325 | 0.5508 | -5.3333 | [0.2650, 0.8600] |

## Locked Holdout Metrics

| Outcome | Model | MAE | RMSE | R2 | MAE 95% CI |
|---|---|---:|---:|---:|---|
| aperiodic_exponent | acoustic_only | 0.0418 | 0.0438 | 0.8416 | [0.0289, 0.0548] |
| aperiodic_exponent | modulation_only | 0.0205 | 0.0221 | 0.9596 | [0.0122, 0.0288] |
| aperiodic_exponent | legacy_v1 | 0.0175 | 0.0208 | 0.9641 | [0.0062, 0.0288] |
| aperiodic_exponent | candidate_v2 | 0.0400 | 0.0400 | 0.0000 | [0.0400, 0.0400] |
| aperiodic_offset | acoustic_only | 0.0067 | 0.0095 | 0.7748 | [0.0000, 0.0134] |
| aperiodic_offset | modulation_only | 0.0080 | 0.0085 | 0.8185 | [0.0051, 0.0109] |
| aperiodic_offset | legacy_v1 | 0.0094 | 0.0094 | 0.7773 | [0.0094, 0.0094] |
| aperiodic_offset | candidate_v2 | 0.0100 | 0.0100 | 0.0000 | [0.0100, 0.0100] |
| envelope_plv | acoustic_only | 0.0417 | 0.0456 | 0.8117 | [0.0234, 0.0600] |
| envelope_plv | modulation_only | 0.0159 | 0.0192 | 0.9666 | [0.0050, 0.0267] |
| envelope_plv | legacy_v1 | 0.0200 | 0.0243 | 0.9465 | [0.0062, 0.0338] |
| envelope_plv | candidate_v2 | 0.0300 | 0.0300 | 0.0000 | [0.0300, 0.0300] |
| assr_plv | acoustic_only | 0.0484 | 0.0552 | 0.6232 | [0.0217, 0.0751] |
| assr_plv | modulation_only | 0.0244 | 0.0269 | 0.9104 | [0.0130, 0.0358] |
| assr_plv | legacy_v1 | 0.0325 | 0.0347 | 0.8517 | [0.0205, 0.0445] |
| assr_plv | candidate_v2 | 0.0300 | 0.0300 | 0.0000 | [0.0300, 0.0300] |
| alpha_peak_frequency_hz | acoustic_only | 0.3463 | 0.3608 | 0.3573 | [0.2454, 0.4473] |
| alpha_peak_frequency_hz | modulation_only | 0.2104 | 0.2163 | 0.7689 | [0.1600, 0.2607] |
| alpha_peak_frequency_hz | legacy_v1 | 0.1598 | 0.1765 | 0.8462 | [0.0848, 0.2348] |
| alpha_peak_frequency_hz | candidate_v2 | 0.2000 | 0.2000 | 0.0000 | [0.2000, 0.2000] |
| alpha_asymmetry | acoustic_only | 0.0247 | 0.0318 | -3.5035 | [0.0046, 0.0448] |
| alpha_asymmetry | modulation_only | 0.0218 | 0.0231 | -1.3670 | [0.0140, 0.0295] |
| alpha_asymmetry | legacy_v1 | 0.0225 | 0.0230 | -1.3454 | [0.0179, 0.0271] |
| alpha_asymmetry | candidate_v2 | 0.0200 | 0.0200 | 0.0000 | [0.0200, 0.0200] |
| vigilance_accuracy | acoustic_only | 0.0068 | 0.0074 | 0.9779 | [0.0037, 0.0098] |
| vigilance_accuracy | modulation_only | 0.0109 | 0.0141 | 0.9203 | [0.0018, 0.0199] |
| vigilance_accuracy | legacy_v1 | 0.0050 | 0.0069 | 0.9808 | [0.0002, 0.0098] |
| vigilance_accuracy | candidate_v2 | 0.0100 | 0.0100 | 0.0000 | [0.0100, 0.0100] |
| reaction_time_ms | acoustic_only | 7.2210 | 7.3159 | 0.9144 | [6.0459, 8.3960] |
| reaction_time_ms | modulation_only | 3.6091 | 3.7326 | 0.9777 | [2.6571, 4.5611] |
| reaction_time_ms | legacy_v1 | 1.2500 | 1.2827 | 0.9974 | [0.9622, 1.5378] |
| reaction_time_ms | candidate_v2 | 6.0000 | 6.0000 | 0.0000 | [6.0000, 6.0000] |
| reaction_time_variability_ms | acoustic_only | 2.1922 | 2.2056 | 0.9559 | [1.9498, 2.4347] |
| reaction_time_variability_ms | modulation_only | 1.5118 | 2.0777 | 0.9608 | [0.0865, 2.9370] |
| reaction_time_variability_ms | legacy_v1 | 0.5000 | 0.5130 | 0.9976 | [0.3854, 0.6146] |
| reaction_time_variability_ms | candidate_v2 | 2.0000 | 2.0000 | 0.0000 | [2.0000, 2.0000] |
| comfort_rating | acoustic_only | 0.1554 | 0.1775 | 0.9125 | [0.0697, 0.2411] |
| comfort_rating | modulation_only | 0.3156 | 0.3765 | 0.6063 | [0.1105, 0.5208] |
| comfort_rating | legacy_v1 | 0.3429 | 0.4396 | 0.4633 | [0.0679, 0.6179] |
| comfort_rating | candidate_v2 | 0.1000 | 0.1000 | 0.0000 | [0.1000, 0.1000] |
| irritation_rating | acoustic_only | 0.0730 | 0.0793 | 0.9487 | [0.0420, 0.1040] |
| irritation_rating | modulation_only | 0.1683 | 0.1926 | 0.6971 | [0.0746, 0.2620] |
| irritation_rating | legacy_v1 | 0.3085 | 0.3175 | 0.1770 | [0.2335, 0.3835] |
| irritation_rating | candidate_v2 | 0.2000 | 0.2000 | 0.0000 | [0.2000, 0.2000] |
| masking_effectiveness_rating | acoustic_only | 0.1509 | 0.1596 | 0.9547 | [0.0992, 0.2027] |
| masking_effectiveness_rating | modulation_only | 0.1454 | 0.1837 | 0.9400 | [0.0332, 0.2576] |
| masking_effectiveness_rating | legacy_v1 | 0.0824 | 0.0964 | 0.9835 | [0.0324, 0.1324] |
| masking_effectiveness_rating | candidate_v2 | 0.2000 | 0.2000 | 0.0000 | [0.2000, 0.2000] |

## Missingness Exclusions

CV rows: 96
Holdout rows: 48

## Failure Cases

CV failure rows: 25
Holdout failure rows: 0
Failure rule: outcome-specific absolute-error thresholds from `calibration_run_manifest.json`.

This report is offline calibration evidence only; it does not promote runtime defaults.
