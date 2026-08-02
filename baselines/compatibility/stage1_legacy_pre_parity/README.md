# Stage 1 legacy baseline

This self-contained snapshot records pre-renderer-parity NMM behavior. It is a regression reference, not a production-equivalent renderer baseline and not evidence of human efficacy. Replay always rebuilds current NMM/DSP code and evaluates the frozen preset inputs under `inputs/presets`.

Verify offline: `python3 tools/compatibility/stage1_baseline.py verify --baseline baselines/compatibility/stage1_legacy_pre_parity`

Replay current code: `python3 tools/compatibility/stage1_baseline.py replay --baseline baselines/compatibility/stage1_legacy_pre_parity --nmm-repo . --dsp-repo ../noise_generator_dsp --ios-repo ../noise_generator_ios_app`
