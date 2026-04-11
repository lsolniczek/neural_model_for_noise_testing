# API Documentation

Command-line interface, testing, and evaluation guide for the Neural Preset Optimizer.

---

## Table of Contents

- [Building](#building)
- [CLI Commands](#cli-commands)
  - [optimize](#optimize)
  - [evaluate](#evaluate)
  - [disturb](#disturb)
  - [validate](#validate)
- [Running Tests](#running-tests)
  - [Full Test Suite](#full-test-suite)
  - [Module-Specific Tests](#module-specific-tests)
  - [Test Categories](#test-categories)
- [Goals Reference](#goals-reference)
- [Brain Types Reference](#brain-types-reference)
- [Preset JSON Format](#preset-json-format)
- [Output Formats](#output-formats)

---

## Building

```bash
# Debug build
cargo build

# Release build (recommended for optimization runs — ~10x faster)
cargo build --release

# Run directly
cargo run -- <command> [options]
cargo run --release -- <command> [options]
```

---

## CLI Commands

### optimize

Run evolutionary optimization to find the best noise preset for a target brain state.

```bash
cargo run --release -- optimize [OPTIONS]
```

#### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `--goal` | string | `focus` | Target brain state to optimize for |
| `--generations` | int | `100` | Maximum DE generations |
| `--population` | int | `30` | DE population size |
| `--duration` | float | `3.0` | Audio duration per evaluation (seconds) |
| `--output` | path | auto-generated | Output JSON file path |
| `--seed` | int | `42` | RNG seed for reproducibility |
| `--de-f` | float | `0.7` | DE mutation scale factor (0.5-0.9 typical) |
| `--de-cr` | float | `0.8` | DE crossover rate (0.7-0.9 typical) |
| `--convergence` | float | `0.001` | Stop if fitness std drops below this |
| `--brain-type` | string | `normal` | Neurological profile for the simulation |
| `--init-preset` | path | none | Seed population from an existing preset JSON |

#### Examples

```bash
# Basic optimization for focus
cargo run --release -- optimize --goal focus

# High-quality run with more generations and longer evaluation
cargo run --release -- optimize --goal deep_work --generations 200 --population 50 --duration 12

# Optimize for ADHD brain type
cargo run --release -- optimize --goal focus --brain-type adhd

# Warm-start from an existing preset
cargo run --release -- optimize --goal meditation --init-preset presets/theta_gamma_refined.json

# Reproducible run with custom DE parameters
cargo run --release -- optimize --goal sleep --seed 123 --de-f 0.8 --de-cr 0.9

# Save output to specific path
cargo run --release -- optimize --goal isolation --output presets/my_isolation_v1.json
```

#### Output

Prints per-generation progress with best score, then a summary:

```
Generation  12 | Best: 0.743 | Mean: 0.612 | Std: 0.089
...
=== Optimization Complete ===
  Goal:             focus
  Brain type:       Normal
  Best score:       0.847
  Generations:      87 (converged)

  Band Powers (normalized):
    Delta:  0.012  Theta:  0.189  Alpha:  0.331  Beta:  0.421  Gamma:  0.047
  FHN firing rate:  14.2 spikes/s
  FHN ISI CV:       0.312

  Performance Vector:
    Spectral centroid:  12.4 Hz
    Entrainment ratio:  0.73 (at 10.0 Hz LFO)
    E/I stability:      0.28

  Saved to: presets/optimized_focus_20260404_143022.json
```

---

### evaluate

Score an existing preset against one or more goals and brain types. Produces a detailed diagnostic breakdown.

```bash
cargo run --release -- evaluate <PRESET_PATH> [OPTIONS]
```

#### Arguments

| Argument | Required | Description |
|----------|:--------:|-------------|
| `preset` | YES | Path to a preset JSON file |

#### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `--goal` | string | `all` | Goal to evaluate against. Use `all` to test all 9 goals |
| `--brain-type` | string | `normal` | Brain type profile. Use `all` to test all 5 types |
| `--duration` | float | `10.0` | Audio duration per evaluation (seconds) |
| `--assr` | flag | `false` | Enable ASSR transfer function (auditory pathway filtering). Off by default for `evaluate`; always on in the `optimize` pipeline |
| `--thalamic-gate` | flag | `false` | Enable arousal-dependent thalamic gating. Off by default for `evaluate`; always on in the `optimize` pipeline |

#### Examples

```bash
# Evaluate preset against all goals with Normal brain
cargo run --release -- evaluate presets/deep_work_optimized_v6.json

# Evaluate against a specific goal
cargo run --release -- evaluate presets/my_preset.json --goal sleep

# Evaluate across all brain types for a single goal
cargo run --release -- evaluate presets/my_preset.json --goal focus --brain-type all

# Full matrix: all goals x all brain types
cargo run --release -- evaluate presets/my_preset.json --goal all --brain-type all

# Match the optimize pipeline (ASSR + thalamic gate enabled)
cargo run --release -- evaluate presets/my_preset.json --goal focus --assr --thalamic-gate

# Longer evaluation for more stable results
cargo run --release -- evaluate presets/my_preset.json --goal meditation --duration 20
```

#### Output

Per-goal diagnostic table:

```
=== Focus (Normal) ===
  Score: 0.743  Verdict: OK
    (base 0.721 + PLV bonus 0.022, asymmetry penalty 0.000)

    Band              Target           Actual     Status
    Delta             0.00-0.08        0.012      PASS
    Theta             0.08-0.32        0.189      PASS
    Alpha             0.18-0.50        0.331      PASS
    Beta              0.25-0.60        0.421      PASS
    Gamma             0.02-0.15        0.047      PASS

    Firing rate       8.0-20.0 Hz      14.2       PASS
    ISI regularity    CV ~ 0.30        0.312      PASS  (irregular)

    Dominant freq:    12.4 Hz (Beta)
    Spectral centroid: 12.4 Hz
    Alpha asymmetry:   0.12 (L-R / L+R)
    PLV (beta):        0.22
```

The score is the result of `Goal::evaluate_full()` (`src/scoring.rs`):
`final = clamp(band_weight·band_score + fhn_weight·fhn_score − asymmetry_penalty + plv_weight·PLV·0.10, 0, 1)`.
See the Goals Reference below for per-goal weights.

---

### disturb

Inject an acoustic spike into the neural simulation and measure recovery dynamics. Tests how resilient a preset's neural entrainment is to sudden disruptions.

```bash
cargo run --release -- disturb <PRESET_PATH> [OPTIONS]
```

#### Arguments

| Argument | Required | Description |
|----------|:--------:|-------------|
| `preset` | YES | Path to a preset JSON file |

#### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `--brain-type` | string | `normal` | Brain type profile |
| `--spike-time` | float | `4.0` | Time of spike injection (seconds into analysis) |
| `--spike-duration` | float | `0.05` | Duration of the spike (seconds) |
| `--spike-gain` | float | `0.8` | Spike amplitude (0.0-1.0) |
| `--duration` | float | `15.0` | Total simulation duration (seconds) |

#### Examples

```bash
# Basic disturbance test
cargo run --release -- disturb presets/balanced_theta_smr.json

# Strong spike, early injection
cargo run --release -- disturb presets/my_preset.json --spike-gain 1.0 --spike-time 3.0

# Test with ADHD brain type
cargo run --release -- disturb presets/adhd_dopaminergic_v8.json --brain-type adhd

# Longer observation window
cargo run --release -- disturb presets/my_preset.json --duration 20 --spike-time 5.0
```

#### Output

Sliding-window timeline showing entrainment, dominant frequency, and spectral centroid before and after the spike, plus recovery metrics:

```
=== Disturbance Resilience Test ===
  Preset: balanced_theta_smr.json
  Spike: 0.05s at 4.0s, gain=0.80

  Baseline (pre-spike):
    Entrainment:     0.72
    Dominant freq:   10.2 Hz
    Spectral centroid: 11.4 Hz

  Nadir (post-spike):
    Entrainment:     0.31 at 4.2s
    Frequency dev:   3.8 Hz

  Recovery:
    50% recovery:    180 ms
    90% recovery:    520 ms

  Timeline:
    Time   Entrain   DomFreq   Centroid
    1.0s   0.71      10.2 Hz   11.3 Hz
    2.0s   0.73      10.1 Hz   11.5 Hz
    3.0s   0.72      10.2 Hz   11.4 Hz
    4.0s   0.31      13.8 Hz   14.2 Hz   <-- SPIKE
    4.5s   0.58      11.1 Hz   12.0 Hz
    5.0s   0.69      10.4 Hz   11.6 Hz
    ...
```

---

### validate

Run the neural model validation test suite. These tests drive the Jansen-Rit model directly with controlled inputs to verify core neuroscience dynamics.

```bash
cargo run --release -- validate
```

No options. Runs 5 tests:

| Test | What it verifies |
|------|-----------------|
| **Frequency tracking** | Drive JR with 10, 20, 40 Hz pure tones; verify output locks to input frequency |
| **Bifurcation threshold** | Sweep input to confirm oscillatory/non-oscillatory transition |
| **Impulse response** | Apply broadband spike; measure EEG response characteristics |
| **Stochastic resonance** | Add noise to weak signal; verify signal-to-noise ratio improves (ADHD model) |
| **Spectral discrimination** | Two closely-spaced tones produce distinct band power profiles |

---

## Running Tests

### Full Test Suite

```bash
# Run the full suite (~300+ tests)
cargo test

# Run in release mode (faster for pipeline tests)
cargo test --release

# Run with output from println! statements
cargo test -- --nocapture

# Run a specific test by name
cargo test sigmoid_at_v0_equals_half_max
```

### Module-Specific Tests

Run tests for a single module using the module path filter:

```bash
# Auditory gammatone filterbank — ERB formula, channel spacing, band grouping, FFT energy
cargo test auditory::gammatone::tests

# ASSR transfer function — DC/AC separation, frequency-dependent attenuation
cargo test auditory::assr::tests

# Thalamic gate — arousal mapping, band-dependent offset shifts
cargo test auditory::thalamic_gate::tests

# FitzHugh-Nagumo model — ODE derivatives, spike detection, ISI CV, bifurcation
cargo test neural::fhn::tests

# Jansen-Rit model — sigmoid, ODE structure, band powers, Wendling extension,
# inhibitory callosal coupling, stochastic drive, habituation
cargo test neural::jansen_rit::tests

# Wilson-Cowan model — adaptive frequency tracking (±5 Hz Arnold tongue), E-I oscillation
cargo test neural::wilson_cowan::tests

# Performance vector — entrainment ratio, E/I stability, spectral centroid, PLV
cargo test neural::performance::tests

# Neural integration tests — bilateral model, hemispheric asymmetry, callosal coupling
cargo test neural::tests::tests

# Brain type profiles — parameter validity, cross-type invariants
cargo test brain_type::tests

# Scoring system — Gaussian formula, goal targets, asymmetry penalty, PLV bonus, FHN scoring
cargo test scoring::tests

# Preset parameter space — genome encoding, bounds, clamping, stochastic remapping
cargo test preset::tests

# Spatial movement patterns — orbit, pendulum, figure-eight, random walk, boundary enforcement
cargo test movement::tests

# Simulation pipeline — global normalization, 95th percentile FHN scaling, deinterleave, decimation
cargo test pipeline::tests

# Differential evolution optimizer — DE/rand/1/bin, convergence, discrete handling
cargo test optimizer::differential_evolution::tests

# Regression tests — scoring snapshots, genome round-trip, pipeline integration
cargo test regression_tests::tests

# Preset analysis tests — parameter sweep sensitivity
cargo test analyze_preset::tests
```

### Test Categories

#### Fast Tests (~0.1 seconds)

Unit tests that verify math, formulas, and data structures without running the full pipeline:

```bash
# All unit tests across all modules (excludes long-running pipeline tests)
cargo test auditory::gammatone::tests neural::fhn::tests neural::jansen_rit::tests \
  neural::wilson_cowan::tests neural::performance::tests brain_type::tests \
  scoring::tests preset::tests movement::tests pipeline::tests \
  optimizer::differential_evolution::tests
```

#### Slow Tests (~1-5 minutes each)

Full pipeline tests that render audio and run bilateral neural models:

```bash
# Pipeline integration (renders 12s audio at 48 kHz per test)
cargo test regression_tests::tests::pipeline

# Preset analysis sweeps (multiple evaluations)
cargo test analyze_preset::tests

# Neural integration tests (bilateral simulation at 48 kHz)
cargo test neural::tests::tests
```

#### Running with Timing

```bash
# Show time per test
cargo test -- -Z unstable-options --report-time

# Run tests in single thread (useful for debugging)
cargo test -- --test-threads=1
```

---

## Goals Reference

Available values for the `--goal` option. All 9 goals are iterated when `--goal all` is used.

| Goal | Aliases | Target Brain State | Band/FHN Weights | PLV Bonus | Asymmetry Penalty |
|------|---------|-------------------|:-:|:-:|:-:|
| `focus` | `concentration` | Beta dominant, frontal theta. Active task engagement | 0.70 / 0.30 | 100% | 5% (thr 0.5) |
| `deep_work` | `deepwork` | Alpha dominant with theta support. Flow for cognitively demanding work | 0.75 / 0.25 | 60% | 5% (thr 0.5) |
| `sleep` | — | Theta dominant, delta emerging. NREM stage 1-2 | 0.65 / 0.35 | 0% | none |
| `deep_relaxation` | `relaxation`, `relax` | Theta + alpha dominant. Pre-sleep unwinding | 0.70 / 0.30 | 0% | 12% (thr 0.3) |
| `meditation` | `meditate` | Theta + alpha co-dominant. Focused-attention meditation (samatha) | 0.65 / 0.35 | 30% | 15% (thr 0.2) |
| `isolation` | `masking` | Flat spectrum. Noise masking, neutral cortical state | 0.80 / 0.20 | 80% | 8% (thr 0.4) |
| `shield` | — | Beta-dominant focused masking, minimal theta, stable FHN | 0.70 / 0.30 | 70% | 8% (thr 0.4) |
| `flow` | — | Alpha-dominant rhythmic synchronization, moderate beta | 0.70 / 0.30 | 30% | 12% (thr 0.3) |
| `ignition` | — | Gamma-driven activation, high FHN for ADHD cognitive binding | 0.70 / 0.30 | 100% | 3% (thr 0.6) |

**PLV bonus**: up to `10% × weight × PLV` added to the score. Goals that want genuine phase-locked entrainment (focus, ignition, isolation, shield) are rewarded when the cortical response locks onto the driving modulation frequency.

**Asymmetry penalty**: linear ramp from 0 at the threshold to the listed max at |asymmetry|=1. Penalizes excessive L/R alpha lateralization for balance-wanting goals; sleep ignores it entirely.

See `src/scoring.rs` for the exact band targets and the `Goal::evaluate_full()` formula.

---

## Brain Types Reference

Available values for the `--brain-type` option:

| Brain Type | Aliases | Profile |
|-----------|---------|---------|
| `normal` | `default`, `healthy` | Healthy adult baseline |
| `high_alpha` | `highalpha`, `alpha`, `meditation` | Meditation practitioner, strong alpha |
| `adhd` | — | Hypoaroused cortex, weaker inhibition |
| `aging` | `aged`, `elderly` | Slower dynamics, reduced connectivity |
| `anxious` | `anxiety` | Heightened excitability, elevated beta |

---

## Preset JSON Format

### Input Format

Presets are JSON files with this structure:

```json
{
  "master_gain": 0.8,
  "spatial_mode": 1,
  "source_count": 4,
  "anchor_color": 2,
  "anchor_volume": 0.0,
  "environment": 0,
  "objects": [
    {
      "active": true,
      "color": 3,
      "x": 2.0,
      "y": 0.0,
      "z": 1.5,
      "volume": 0.75,
      "reverb_send": 0.2,
      "bass_mod": {
        "kind": 4,
        "param_a": 10.0,
        "param_b": 0.6,
        "param_c": 0.0
      },
      "satellite_mod": {
        "kind": 1,
        "param_a": 0.5,
        "param_b": 0.8,
        "param_c": 0.0
      },
      "movement": {
        "kind": 1,
        "radius": 2.5,
        "speed": 0.8,
        "phase": 0.0,
        "depth_min": 1.0,
        "depth_max": 4.0,
        "reverb_min": 0.05,
        "reverb_max": 0.5
      }
    }
  ]
}
```

#### Field Reference

**Noise colors** (`anchor_color`, `color`): 0=White, 1=Pink, 2=Brown, 3=Green, 4=Grey, 5=Black, 6=SSN, 7=Blue

(Canonical source: `NoiseColor::from_u8` in `noise_generator_dsp/crates/core/src/lib.rs:176–188`, labels from `src/main.rs:1048`.)

**Spatial modes** (`spatial_mode`): 0=Stereo, 1=Immersive

**Environments** (`environment`): 0=AnechoicChamber, 1=FocusRoom, 2=OpenLounge, 3=VastSpace, 4=DeepSanctuary

**Modulator kinds** (`bass_mod.kind`, `satellite_mod.kind`): 0=Flat, 1=SineLfo, 2=Breathing, 3=Stochastic, 4=NeuralLfo

**Movement kinds** (`movement.kind`): 0=Static, 1=Orbit, 2=FigureEight, 3=RandomWalk, 4=DepthBreathing, 5=Pendulum

### Export Format

Optimized presets are exported with metadata:

```json
{
  "meta": {
    "goal": "focus",
    "score": 0.847,
    "generated_at": "2026-04-04T14:30:22+00:00",
    "optimizer_generations": 87,
    "audio_duration_secs": 12.0
  },
  "preset": { ... },
  "analysis": {
    "fhn_firing_rate": 14.2,
    "fhn_isi_cv": 0.312,
    "dominant_freq_hz": 12.4,
    "band_powers": {
      "delta": 0.012,
      "theta": 0.189,
      "alpha": 0.331,
      "beta": 0.421,
      "gamma": 0.047
    }
  }
}
```

---

## Output Formats

### Score Range

All scores are in **[0.0, 1.0]**:

| Range | Verdict | Meaning |
|-------|---------|---------|
| 0.75 - 1.00 | GOOD | Strong match to target brain state |
| 0.50 - 0.74 | OK | Partial match, room for improvement |
| 0.00 - 0.49 | POOR | Weak match |

### Band Powers

Reported as **normalized fractions** summing to 1.0:

| Band | Frequency | Typical Range |
|------|-----------|:-------------:|
| Delta | 0.5 - 4 Hz | 0.01 - 0.40 |
| Theta | 4 - 8 Hz | 0.05 - 0.50 |
| Alpha | 8 - 13 Hz | 0.10 - 0.70 |
| Beta | 13 - 30 Hz | 0.01 - 0.60 |
| Gamma | 30 - 50 Hz | 0.00 - 0.15 |

### FHN Metrics

| Metric | Range | Interpretation |
|--------|-------|---------------|
| Firing rate | 0 - 25 spikes/s | Higher = more aroused |
| ISI CV | 0.0 - 1.0+ | 0 = perfectly regular, 0.3 = typical cortical, >0.5 = bursting |
| ISI CV = NaN | — | Fewer than 3 spikes detected (insufficient data) |
| ISI CV = -1.0 (JSON) | — | NaN sentinel in exported JSON |

### Spectral Brightness

Brightness is computed and reported for diagnostic purposes, but **no longer contributes to the score**. After the shift to global band normalization, the neural model captures spectral differences directly, so a separate brightness term would double-count the same information (`src/scoring.rs:402–407`). The `brightness` parameter on `evaluate_with_brightness()` is kept for API compatibility only.

| Value | Character | Noise Example |
|-------|-----------|---------------|
| 0.0 | Very dark | Brown noise (1/f^2) |
| 0.35 | Mid-dark | Pink noise (1/f) |
| 0.5 | Neutral | — |
| 0.7 | Bright | White noise (flat) |
| 1.0 | Very bright | Blue/violet noise |

### Alpha Asymmetry

Reported as `(L − R) / (L + R)` on alpha-band power across hemispheres. Range [-1, 1]: 0 = balanced, ±1 = fully lateralized. Used in scoring via `Goal::asymmetry_penalty()`.

### Phase-Locking Value (PLV)

PLV ∈ [0, 1] measures phase coherence between the driving modulation frequency and the cortical response (Lachaux et al. 1999). Computed in `src/neural/performance.rs` via bandpass filter (±3 Hz) → Hilbert analytic signal → averaged unit phasors: `PLV = |1/N × Σ exp(i·Δφ)|`. Values > 0.6 indicate strong entrainment; < 0.3 is essentially noise.
