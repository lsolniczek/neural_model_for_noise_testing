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
  - [generate-data](#generate-data)
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
| `--cet` | flag | `false` | Enable Cortical Envelope Tracking (Priority 13) |
| `--phys-gate` | flag | `false` | Enable physiological thalamic gate (Priority 9) |
| `--surrogate` | flag | `false` | Enable surrogate-assisted pre-screening (Priority 14). Uses a trained MLP to filter DE candidates before expensive real-pipeline evaluation. ~10x speedup |
| `--surrogate-weights` | path | `surrogate_weights.bin` | Path to the trained surrogate weights file |
| `--surrogate-k` | int | `5` | Number of top surrogate candidates to validate per generation with the real pipeline |

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

# Optimize sleep with physiological gate + CET (best for relaxation goals)
cargo run --release -- optimize --goal sleep --phys-gate --cet --generations 200 --population 50 --duration 12

# Same but with surrogate pre-screening (~10x faster, requires trained weights)
cargo run --release -- optimize --goal sleep --phys-gate --cet --surrogate \
  --surrogate-weights surrogate_weights.bin --surrogate-k 5 \
  --generations 200 --population 50 --duration 12
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
| `--cet` | flag | `false` | Enable Cortical Envelope Tracking (Priority 13). Splits each band into a slow (≤10 Hz) path that bypasses ASSR and a fast (>10 Hz) path that gets ASSR, engages the slow GABA_B inhibitory population in JR, and adds envelope-phase PLV to scoring. Off by default for `evaluate`; off in `optimize` until validated. Refs: Doelling et al. 2014, Ding & Simon 2014, Moran & Friston 2011 |
| `--phys-gate` | flag | `false` | Enable the physiological thalamic gate (Priority 9). Replaces the linear heuristic gate with a Hodgkin-Huxley TC cell where K⁺ leak conductance is the arousal knob. Ion-channel dynamics (T-type Ca²⁺, Bazhenov 2002 / Destexhe 1996) produce a sigmoidal burst↔tonic mode switch rather than a linear ramp. Dramatically improves sleep/relaxation scores (+0.12 to +0.28); takes precedence over `--thalamic-gate` when both set. Off by default for `evaluate` |

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

# Enable Cortical Envelope Tracking — relaxation/sleep/meditation goals
# get a new envelope-phase PLV reward channel; presets with slow NeuralLfo
# (1–8 Hz) on broadband noise gain ~3–5% on those goals.
cargo run --release -- evaluate presets/my_preset.json --goal sleep --cet

# Enable the physiological thalamic gate — ion-channel-derived sigmoid
# replaces the linear heuristic. Sleep/relaxation presets gain +0.12 to +0.28.
cargo run --release -- evaluate presets/my_preset.json --goal sleep --phys-gate

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
`final = clamp(band_weight·band_score + fhn_weight·fhn_score − asymmetry_penalty + plv_weight·PLV·0.10 + envelope_plv_weight·envelope_PLV·0.10, 0, 1)`.

The two PLV terms are additive on different perceptual axes: `plv_weight·PLV` rewards entrainment to the carrier modulation frequency (Lachaux et al. 1999), while `envelope_plv_weight·envelope_PLV` rewards cortical envelope tracking — phase-locking of the EEG to the slow (2–9 Hz) auditory envelope, the metric Ding & Simon (2014) and Luo & Poeppel (2007) use to quantify CET. The envelope PLV bonus is only computed when `--cet` is enabled. See the Goals Reference below for per-goal weights for both PLV terms.

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

Sliding-window timeline showing entrainment, dominant frequency, and spectral centroid before and after the spike, plus two resilience scores:

**Entrainment Resilience** (requires NeuralLfo or Isochronic modulator): measures how fast the EEG phase-locks back to the driving frequency. Based on PLV recovery.

**Spectral Resilience** (Priority 15 — works for ALL preset types including binaural beats and static noise): measures how fast the EEG band power distribution returns to its pre-spike baseline. Based on three metrics from the ERD/ERS literature (Pfurtscheller & Lopes da Silva 1999):

| Metric | What it measures | Range |
|---|---|---|
| **BPPR** (Band Power Preservation Ratio) | Worst-case fractional preservation of band powers during spike | 0-1 (1=perfect) |
| **SRT** (Spectral Recovery Time) | Milliseconds until band powers return to within 50%/90% of baseline | 0-∞ ms |
| **SCDI** (Spectral Centroid Deviation Integral) | Mean spectral centroid displacement from baseline post-spike | 0-∞ Hz (0=none) |
| **Composite score** | `0.40×BPPR + 0.30×(1-norm_SRT) + 0.30×(1-norm_SCDI)` | 0-1 (1=perfect) |

```
=== Disturbance Resilience Test ===
  Preset: normal_set_shield.json
  Spike: 0.05s at 5.0s, gain=0.80
  Target LFO: 12.0 Hz

  Baseline (pre-spike):
    Dominant freq:   9.48 Hz
    Spectral centroid: 11.80 Hz
    Entrainment ratio: 0.570

  Spike Impact:
    Entrainment nadir: 0.189 (67% drop)
    Peak freq deviation: ±6.14 Hz

  Recovery:
    50% recovery:    50 ms
    90% recovery:    50 ms

    Entrainment Resilience: 0.92
       (preservation=0.87, speed=0.99)

  Spectral Resilience (Priority 15):
    BPPR (band preservation):  0.501
    Spectral recovery 50%:     3650 ms
    Spectral recovery 90%:     3650 ms
    SCDI (centroid deviation):  1.14 Hz

    Spectral Resilience Score: 0.43
       (BPPR=0.50, SRT=3650ms, SCDI=1.14Hz)

  Timeline:
    Time   Entrain   DomFreq   Centroid
    0.2s   0.803      9.8 Hz   10.9 Hz
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

### generate-data

Generate training data for the surrogate model (Priority 14). Samples random presets, evaluates each against specified goals and brain types using the real simulation pipeline, and writes the results as CSV for use by `tools/train_surrogate.py`.

```bash
cargo run --release -- generate-data [OPTIONS]
```

#### Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `--output` | path | `training_data.csv` | Output CSV file path |
| `--count` | int | `1000` | Number of random presets to sample |
| `--goals` | string | `all` | Goals to evaluate (comma-separated, or "all" for all 9) |
| `--brain-type` | string | `normal` | Brain type (or "all" for all 5) |
| `--duration` | float | `3.0` | Audio duration per evaluation (seconds). Shorter = faster but noisier |
| `--threads` | int | `4` | Number of parallel evaluation threads |
| `--seed` | int | `42` | RNG seed for preset generation |

#### Examples

```bash
# Quick dataset (1000 presets × 4 goals × Normal, ~7 minutes with 8 threads)
cargo run --release -- generate-data --count 1000 --goals sleep,deep_relaxation,focus,deep_work --threads 8

# Full dataset for surrogate training (20000 presets × all goals × Normal, ~3.5 hours with 8 threads)
cargo run --release -- generate-data --count 20000 --goals all --duration 3 --threads 8

# All brain types (20000 × 9 goals × 5 brains = 900k evals — long run)
cargo run --release -- generate-data --count 20000 --goals all --brain-type all --threads 8
```

#### Output Format

CSV with columns: `g0, g1, ..., g189, goal_id, brain_type_id, assr, thalamic_gate, cet, phys_gate, score`

- `g0..g189`: genome values (raw, not normalized — the surrogate normalizes on input)
- `goal_id`: integer index into `GoalKind::all()` (0=Focus, 1=DeepWork, 2=Sleep, ...)
- `brain_type_id`: integer index into `BrainType::all()` (0=Normal, 1=HighAlpha, ...)
- `assr`, `thalamic_gate`, `cet`, `phys_gate`: config flags (0 or 1)
- `score`: real-pipeline score in [0, 1]

#### Surrogate Training Workflow

```bash
# 1. Generate training data
cargo run --release -- generate-data --count 20000 --goals all --threads 8

# 2. Train the surrogate MLP (requires Python + PyTorch)
python tools/train_surrogate.py training_data.csv surrogate_weights.bin

# 3. Use surrogate in the optimizer (~10x faster)
cargo run --release -- optimize --goal sleep --surrogate --surrogate-weights surrogate_weights.bin
```

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

# CET crossover (Priority 13a) — 1st-order LP + complementary HP, symmetric -3 dB
# at 10 Hz, 5 Hz envelope passes, 40 Hz carrier rejected, near-bitwise reconstruction
cargo test auditory::crossover::tests

# Physiological thalamic gate (Priority 9) — TC cell ODE, T-type Ca2+ gating curves,
# HH singularity handling, burst-vs-tonic mode switch, Steriade proportions, arousal delegation
cargo test auditory::physiological_thalamic_gate::tests

# Surrogate model (Priority 14c) — MLP weight loading, forward pass, input builder,
# round-trip serialization, output range [0,1], batch consistency
cargo test surrogate::tests

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

| Goal | Aliases | Target Brain State | Band/FHN Weights | Carrier PLV | Envelope PLV (CET) | Asymmetry Penalty |
|------|---------|-------------------|:-:|:-:|:-:|:-:|
| `focus` | `concentration` | Beta dominant, frontal theta. Active task engagement | 0.70 / 0.30 | 100% | 0% | 5% (thr 0.5) |
| `deep_work` | `deepwork` | Alpha dominant with theta support. Flow for cognitively demanding work | 0.75 / 0.25 | 60% | 20% | 5% (thr 0.5) |
| `sleep` | — | Theta dominant, delta emerging. NREM stage 1-2 | 0.65 / 0.35 | 0% | **80%** | none |
| `deep_relaxation` | `relaxation`, `relax` | Theta + alpha dominant. Pre-sleep unwinding | 0.70 / 0.30 | 0% | **70%** | 12% (thr 0.3) |
| `meditation` | `meditate` | Theta + alpha co-dominant. Focused-attention meditation (samatha) | 0.65 / 0.35 | 30% | **60%** | 15% (thr 0.2) |
| `isolation` | `masking` | Flat spectrum. Noise masking, neutral cortical state | 0.80 / 0.20 | 80% | 0% | 8% (thr 0.4) |
| `shield` | — | Beta-dominant focused masking, minimal theta, stable FHN | 0.70 / 0.30 | 70% | 0% | 8% (thr 0.4) |
| `flow` | — | Alpha-dominant rhythmic synchronization, moderate beta | 0.70 / 0.30 | 30% | 40% | 12% (thr 0.3) |
| `ignition` | — | Gamma-driven activation, high FHN for ADHD cognitive binding | 0.70 / 0.30 | 100% | 0% | 3% (thr 0.6) |

**Carrier PLV bonus**: up to `10% × weight × PLV` added to the score. Goals that want genuine phase-locked entrainment to the modulation carrier (focus, ignition, isolation, shield) are rewarded when the cortical response locks onto the driving modulation frequency. Per Lachaux et al. (1999).

**Envelope PLV bonus (CET, --cet only)**: up to `10% × weight × envelope_PLV` added on top. Goals that want natural slow-rhythm tracking (sleep, deep_relaxation, meditation) are rewarded when the cortex phase-locks to the 2–9 Hz envelope of the auditory drive — the cortical envelope tracking signal per Ding & Simon (2014) and Luo & Poeppel (2007). The two PLV terms are additive on different perceptual axes; sleep/relaxation goals get a CET reward channel they previously lacked entirely (carrier PLV was 0% for them).

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

**Source kind** (`source_kind`): 0=Noise (default, colored noise through filter chain), 1=Tone (pure sine oscillator, bypasses color filters). When `source_kind=1`, use `tone_freq` (20-8000 Hz) and `tone_amplitude` (0.0-1.0) to control the sine. Useful for binaural beats (two tone objects at L/R with a frequency difference = beat rate).

(Canonical source: `NoiseColor::from_u8` in `noise_generator_dsp/crates/core/src/lib.rs:176–188`, labels from `src/main.rs:1048`.)

**Spatial modes** (`spatial_mode`): 0=Stereo, 1=Immersive

**Environments** (`environment`): 0=AnechoicChamber, 1=FocusRoom, 2=OpenLounge, 3=VastSpace, 4=DeepSanctuary

**Modulator kinds** (`bass_mod.kind`, `satellite_mod.kind`): 0=Flat, 1=SineLfo, 2=Breathing, 3=Stochastic, 4=NeuralLfo, 5=Isochronic, 6=RandomPulse

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

**Carrier PLV** ∈ [0, 1] measures phase coherence between the driving modulation frequency and the cortical response (Lachaux et al. 1999). Computed in `src/neural/performance.rs::compute_plv()` via bandpass filter (±3 Hz around the carrier) → Hilbert analytic signal → averaged unit phasors: `PLV = |1/N × Σ exp(i·Δφ)|` where Δφ is the phase difference between the EEG and a synthetic reference sinusoid at the LFO frequency. Values > 0.6 indicate strong entrainment; < 0.3 is essentially noise. Always computed when a NeuralLfo modulator is present.

**Envelope-phase PLV (CET)** ∈ [0, 1] measures phase coherence between the EEG (bandpassed in the 2–9 Hz CET band per Doelling et al. 2014) and the *instantaneous phase of the slow auditory envelope* (Hilbert-extracted from the slow-path crossover output). This is the cortical envelope tracking metric proper — Ding & Simon (2014), Luo & Poeppel (2007), Lakatos et al. (2008). Computed in `compute_envelope_plv()`. Only available when `--cet` is enabled. Reported as `envelope_plv` on `PerformanceVector`.

**Why two PLV metrics?** Carrier PLV asks "does the EEG carry power locked to the LFO frequency?" — appropriate for goals that drive entrainment to a tone (focus, ignition, isolation). Envelope PLV asks "does the EEG follow the slow envelope of the stimulus, phase by phase?" — appropriate for goals that want natural slow-rhythm tracking (sleep, relaxation, meditation). The two are additive on different perceptual axes.
