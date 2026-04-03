# Neural Preset Optimizer — Project Review Map

This document divides the project into logical sections for a systematic review covering:
- Correctness of mathematics and science
- Code quality and potential bugs
- Test coverage and regression safety

---

## Table of Contents

1. [Auditory Model — Gammatone Filterbank](#1-auditory-model--gammatone-filterbank)
2. [Neural Models — FitzHugh-Nagumo](#2-neural-models--fitzhugh-nagumo)
3. [Neural Models — Jansen-Rit (Wendling 2002)](#3-neural-models--jansen-rit-wendling-2002)
4. [Neural Models — Wilson-Cowan](#4-neural-models--wilson-cowan)
5. [Neural Diagnostics — Performance Vector](#5-neural-diagnostics--performance-vector)
6. [Brain Type Profiles](#6-brain-type-profiles)
7. [Scoring System](#7-scoring-system)
8. [Preset Parameter Space](#8-preset-parameter-space)
9. [Spatial Movement Patterns](#9-spatial-movement-patterns)
10. [Simulation Pipeline](#10-simulation-pipeline)
11. [Differential Evolution Optimizer](#11-differential-evolution-optimizer)
12. [Validation & Disturbance Tests](#12-validation--disturbance-tests)
13. [Unit Test Coverage Gaps](#13-unit-test-coverage-gaps)
14. [CLI and Export](#14-cli-and-export)

---

## 1. Auditory Model — Gammatone Filterbank

**File:** `src/auditory/gammatone.rs`

### What it does
Simulates cochlear (basilar membrane) frequency decomposition. Audio at 48 kHz is decomposed
into 32 frequency channels using Gammatone filters, producing per-channel envelope signals that
serve as inputs to the neural models.

### Scientific basis
- Patterson et al. (1992) — Gammatone filters approximate auditory nerve tuning curves
- ERB (Equivalent Rectangular Bandwidth) scale maps to human perceptual frequency resolution
- Half-wave rectification + low-pass envelope smoothing models inner hair cell transduction
- 80 Hz envelope LP cutoff matches the modulation bandwidth of auditory nerve fibers

### Key math
```
ERB(f) = 24.7 · (4.37·f/1000 + 1)
center_freqs: ERB-spaced from 50 Hz to 8000 Hz (32 channels)
Per-channel filter: 4th-order cascaded Gammatone (complex-valued, 4 × 1st-order stages)
Envelope: |z| (magnitude of complex output)
Smoothing: α = exp(-2π·80/48000), y[n] = α·y[n-1] + (1-α)·|z[n]|
Per-channel weight: 1 / ERB(fc) (compensates low-freq gain boost)
```

### Key functions
| Function | Purpose |
|----------|---------|
| `GammatoneFilterbank::new(sample_rate, n_channels, f_min, f_max)` | Build filterbank |
| `process(signal) -> Vec<Vec<f32>>` | Full per-channel envelope output |
| `process_to_neural_input(signal) -> Vec<f32>` | Weighted sum → single signal |
| `process_to_band_groups(signal) -> [Vec<f32>; 4]` | 4 tonotopic bands |

### Tonotopic band split
| Band | Frequency Range | Cortical Target |
|------|----------------|-----------------|
| Low | 50–300 Hz | Delta/Theta drive |
| Low-Mid | 300–900 Hz | Theta/Alpha drive |
| Mid-High | 900–3000 Hz | Alpha/Beta drive |
| High | 3000–8000 Hz | Beta/Gamma drive |

### Review checklist
- [ ] Verify ERB formula constants (24.7 and 4.37 — these come from Glasberg & Moore 1990)
- [ ] Verify gammatone filter coefficient derivation matches literature (Patterson 1992)
- [ ] Confirm 4th-order cascade is correctly implemented (4 sequential complex 1st-order stages)
- [ ] Verify envelope smoothing LP cutoff (80 Hz) is appropriate for neural model input rate
- [ ] Check per-channel normalization: does inverse-ERB weighting produce a flat spectrum?
- [ ] Verify band boundary indices are computed correctly for the 32-channel layout
- [ ] Test: flat spectrum input → equal energy across bands?
- [ ] Test: pure tone → energy concentrated in single channel?
- [ ] Test: band group energies sum to total energy (conservation)?

---

## 2. Neural Models — FitzHugh-Nagumo

**File:** `src/neural/fhn.rs`

### What it does
Simulates a single spiking neuron using the simplified 2-variable FitzHugh-Nagumo (FHN) model.
Driven by the bilateral Jansen-Rit EEG output, it produces firing rate and ISI CV statistics
that contribute to scoring.

### Scientific basis
- FitzHugh (1961), Nagumo et al. (1962) — reduced Hodgkin-Huxley model
- v = fast membrane voltage variable; w = slow recovery variable
- Captures excitability, oscillation, and refractoriness without ion channel detail
- ISI CV (coefficient of variation of inter-spike intervals) measures spike train regularity

### Key math
```
dv/dt = v - v³/3 - w + I(t)
dw/dt = ε(v + a - b·w)

Parameters (default):
  a = 0.7, b = 0.8, ε = 0.08
  time_scale = 300 ms (maps model time → real time)
  input_scale = 1.0 (scales neural input I(t))

Integration: RK4 with 4 sub-steps per sample
Spike detection: upward crossing of v = 1.0
Firing rate: n_spikes / duration_sec
ISI CV: std(ISIs) / mean(ISIs) — 0=regular, ~1=Poisson, >1=bursting
```

### Key functions
| Function | Purpose |
|----------|---------|
| `FhnModel::new(params)` | Create model with brain-type params |
| `simulate(input_signal, dt) -> FhnResult` | Run simulation, return trace + metrics |
| `detect_spikes(trace, threshold)` | Extract spike times |
| `compute_isi_cv(spike_times)` | Compute ISI regularity metric |

### Review checklist
- [ ] Verify RK4 implementation is correct (k1–k4 steps, weighted average)
- [ ] Verify sub-stepping logic: are 4 sub-steps computed correctly per sample?
- [ ] Check spike detection: upward crossing only (v[n-1] < threshold AND v[n] >= threshold)?
- [ ] Check ISI CV edge case: fewer than 2 spikes → CV undefined; what is returned?
- [ ] Check ISI CV edge case: all ISIs equal → CV = 0 (valid); verify no division by zero
- [ ] Verify `time_scale` is applied consistently (both to dt and to I(t) scaling?)
- [ ] Verify `input_scale` is applied before or after the time-scale adjustment?
- [ ] Verify FHN parameters per brain type match literature / make biological sense
- [ ] Test: zero input → no spikes (subthreshold)?
- [ ] Test: constant suprathreshold input → regular firing (low CV)?
- [ ] Test: noisy input → irregular firing (higher CV)?
- [ ] Test: firing rate increases monotonically with input amplitude?

---

## 3. Neural Models — Jansen-Rit (Wendling 2002)

**File:** `src/neural/jansen_rit.rs`

### What it does
The core neural mass model. Simulates EEG-like dynamics from 4 interacting neural populations
in a cortical column (pyramidal cells + 3 interneuron populations). Drives the FHN single-neuron
model and produces band power spectra for scoring.

### Scientific basis
- Jansen & Rit (1995) — original 3-population neural mass model
- Wendling et al. (2002) — extension adding fast GABA-A inhibition (4th population)
- Models realistic EEG: delta (0.5–4 Hz), theta (4–8 Hz), alpha (8–13 Hz), beta (13–30 Hz), gamma (30–50 Hz)
- Sigmoid function: sigmoidal firing rate from PSP (population transfer function)

### Key math (8 ODEs)
```
Neural populations:
  y0, y4: Excitatory interneuron (Glutamate) PSP + d/dt
  y1, y5: Pyramidal cell excitatory PSP + d/dt
  y2, y6: Slow inhibitory (GABA-B) PSP + d/dt
  y3, y7: Fast inhibitory (GABA-A) PSP + d/dt  ← Wendling 2002

Sigmoid: S(v) = 2·Vmax / (1 + exp(r·(V0 - v)))
  Vmax = 5.0 spikes/s, V0 = 6.0 mV, r = 0.56–0.62 1/mV (brain-type dependent)

EEG output: eeg = y1 - y2 - y3

Connectivity (Jansen-Rit 1995):
  C1 = C (Excitatory interneuron ← Pyramidal)
  C2 = 0.8·C (Slow inhibitory ← Pyramidal)
  C3 = 0.25·C (Pyramidal ← Excitatory interneuron, feedback)
  C4 = 0.25·C (Pyramidal ← Slow inhibitory)

Connectivity (Wendling 2002 additions):
  C5, C6, C7: Fast inhibitory couplings (brain-type dependent)

Time constants:
  Excitatory: a_gain·A / a_rate (where A=3.25 mV, a=100/s typically)
  Slow inhibitory: b_gain·B / b_rate (where B=22 mV, b=50/s typically)
  Fast inhibitory: g_fast_gain / g_fast_rate

Integration: Euler (step = 1/NEURAL_SR = 1 ms)
```

### Bilateral simulation
- Two independent JR model instances (Left/Right hemispheres)
- Different band_offset parameters per hemisphere and per tonotopic band
- Left hemisphere alpha asymmetry: left_α - right_α
- Combined output: weighted average of L/R EEG for FHN drive

### Key functions
| Function | Purpose |
|----------|---------|
| `JansenRitModel::new()` | Classic JR 1995 defaults |
| `with_params(params)` | Custom JR 1995 parameters |
| `with_fast_inhib(params)` | Enable Wendling 2002 fast GABA-A |
| `simulate(input, dt) -> (eeg, BandPowers)` | Single hemisphere simulation |
| `simulate_bilateral(l_input, r_input, params_l, params_r)` | Bilateral cortical model |
| `simulate_tonotopic(band_signals, band_params)` | 4-band tonotopic model |
| `extract_band_powers(eeg, sample_rate)` | FFT → delta/theta/alpha/beta/gamma |

### Review checklist
- [ ] Verify all 8 ODE right-hand sides match Wendling 2002 equations exactly
- [ ] Verify C1–C4 connectivity constants (JR 1995: C=135, C1=C, C2=0.8C, C3=C4=0.25C)
- [ ] Verify sigmoid parameters: V0=6.0 mV, Vmax=5.0, r=0.56 1/mV (JR 1995 uses these)
- [ ] Verify Euler integration stability at dt=1ms (is this step size small enough?)
- [ ] Check EEG output sign: y1 - y2 - y3 (should be pyramidal excitation minus inhibitions)
- [ ] Verify band power extraction: FFT bins correctly mapped to Hz bands?
- [ ] Check Nyquist: with NEURAL_SR=1000 Hz, max detectable freq = 500 Hz (sufficient for gamma at 50 Hz)
- [ ] Verify bilateral hemisphere band_offsets are physiologically motivated
- [ ] Verify alpha asymmetry formula: (L - R) / (L + R) range [-1, +1] correct
- [ ] Check warmup discard: are initial transients properly excluded from FFT?
- [ ] Verify fast inhibition equations (Wendling 2002) — C5/C6/C7 coupling points
- [ ] Check: does model degenerate correctly to JR 1995 when g_fast_gain = 0?
- [ ] Test: alpha peak emerges with default parameters (8–13 Hz dominant)?
- [ ] Test: increasing input → transition from alpha to beta/gamma (bifurcation)?
- [ ] Test: bilateral symmetry when L=R parameters and equal inputs?
- [ ] Test: band powers sum to approximately total EEG variance?

---

## 4. Neural Models — Wilson-Cowan

**File:** `src/neural/wilson_cowan.rs`

### What it does
Models fast cortical oscillations (beta/gamma range, ~20–80 Hz) via excitatory-inhibitory
population dynamics. Used as an alternative to JR for high-frequency tonotopic bands.

### Scientific basis
- Wilson & Cowan (1972) — foundational E-I population model
- E population = excitatory (Glutamate); I population = inhibitory (GABA-A)
- Mutual feedback generates gamma oscillations (~40 Hz in cortex)
- Frequency tunable via τ_e, τ_i time constants and coupling weights

### Key math
```
τ_e · dE/dt = -E + S(w_ee·E - w_ei·I + P + h_e)
τ_i · dI/dt = -I + S(w_ie·E - w_ii·I + h_i)

Sigmoid: S(x) = 1 / (1 + exp(-a·(x - θ)))

Frequency approximation:
  f_target ≈ 1 / (2.45·(τ_e + τ_i))  → tune τ values to hit target Hz

Default parameters (for 40 Hz):
  τ_e = 0.004 s, τ_i = 0.006 s
  w_ee = 10, w_ei = 12, w_ie = 10, w_ii = 3
  h_e = 2.0, h_i = 3.0
```

### Key functions
| Function | Purpose |
|----------|---------|
| `WilsonCowanModel::new(params)` | Create with custom params |
| `for_frequency(hz)` | Auto-tune τ values to oscillate at target Hz |
| `simulate(input, dt) -> (e_trace, i_trace)` | Run simulation |

### Review checklist
- [ ] Verify WC ODE right-hand sides match Wilson & Cowan (1972)
- [ ] Verify the frequency-tuning formula: f = 1/(2.45·(τ_e+τ_i)) — is constant 2.45 correct?
- [ ] Check sigmoid parameters: are `a` and `θ` consistent with the cited range (a=1.3, θ=4.0)?
- [ ] Verify integration method (Euler or RK4?) and step size stability
- [ ] Check: does `for_frequency()` actually produce the target oscillation frequency?
- [ ] Test: pure E-I feedback without external input produces sustained oscillation?
- [ ] Test: frequency output matches `for_frequency()` target (within 10%)?
- [ ] Test: increasing P (external drive) shifts frequency upward?

---

## 5. Neural Diagnostics — Performance Vector

**File:** `src/neural/performance.rs`

### What it does
Computes 3 diagnostic metrics from the neural simulation output. These are not used in the
primary scoring function, but are reported alongside the score for insight.

### Metrics
| Metric | Formula | Interpretation |
|--------|---------|----------------|
| Entrainment Ratio | power_at_LFO_freq / total_power | How well neural output locks to LFO input |
| E/I Stability Index | CV of y[3] (fast inhibitory PSP) | GABA-A balance stability (lower = more stable) |
| Spectral Centroid | Σ(f·P(f)) / Σ(P(f)) | "Centre of mass" of EEG spectrum [1–50 Hz] |

### Review checklist
- [ ] Verify entrainment ratio: is the LFO frequency extracted correctly from preset?
- [ ] Verify the FFT bin selection for LFO frequency is accurate (no off-by-one in bin index)
- [ ] Verify E/I stability: y[3] is the fast inhibitory PSP (check index into state vector)
- [ ] Verify spectral centroid range: is [1–50 Hz] window correctly applied?
- [ ] Check: spectral centroid with white noise input → ~25 Hz (midpoint of 1–50 Hz range)?
- [ ] Check: entrainment ratio is in [0, 1]?

---

## 6. Brain Type Profiles

**File:** `src/brain_type.rs`

### What it does
Defines 5 brain type profiles that parameterize the neural models (JR and FHN) to represent
individual neurological variation. Each profile has different neural dynamics reflecting
the clinical/physiological literature.

### Brain Types
| Type | Description | Key Differences |
|------|-------------|-----------------|
| `Normal` | Healthy adult baseline | Default JR parameters |
| `HighAlpha` | Meditation practitioner | Stronger alpha (lower a_gain, adjusted C) |
| `Adhd` | Hypoaroused cortex | Weaker inhibition, faster dynamics, lower input_offset |
| `Aging` | Older adult | Slower time constants (lower a_rate, b_rate) |
| `Anxious` | Hyperactive cortex | Elevated beta, stronger excitation (higher a_gain) |

### Parameter sensitivity
```
JansenRitParams:
  a_gain, b_gain     — scale PSP amplitude (affects oscillation magnitude)
  a_rate, b_rate     — scale time constants (affects oscillation frequency)
  c                  — global connectivity scaling (C1=c, C2=0.8c, C3=C4=0.25c)
  input_offset       — DC offset on external input (tonic arousal)
  input_scale        — scales stochastic input amplitude
  g_fast_gain        — Wendling fast GABA-A amplitude (0 = classic JR)
  g_fast_rate        — fast GABA-A time constant
  c5, c6, c7         — fast inhibitory connectivity (Wendling 2002)
  slow_inhib_ratio   — scales C4 connectivity (slow GABA-B feedback)
  v0                 — sigmoid inflection point

FhnParams:
  a, b               — shape parameters
  epsilon            — time-scale separation
  input_scale        — scales JR output to FHN input
  time_scale         — maps FHN time to real time (ms)
```

### Bilateral parameters
Each brain type also defines per-hemisphere band_offset arrays (4 values each for L/R),
controlling the DC input bias to each tonotopic JR model.

### Review checklist
- [ ] Normal: verify defaults match Jansen & Rit (1995) reference values
- [ ] HighAlpha: does reduced a_gain actually shift peak toward alpha? (verify with simulation)
- [ ] ADHD: verify band_offsets are in range 50–110 (per SCIENTIFIC_AUDIT_TASKS.md fix)
- [ ] ADHD: verify input_offset is 95 (not the old 75 — per audit fix)
- [ ] Aging: verify a_rate and b_rate reduction produces correct low-frequency shift
- [ ] Aging: callosal coupling — is 0.07 used or the old 0.05? (medium-priority TODO)
- [ ] Anxious: verify elevated beta without gamma spill-over
- [ ] All types: verify FHN input_scale is compatible with JR output amplitude range
- [ ] All types: verify FHN time_scale = 300 is appropriate (low-priority TODO: make brain-type-dependent)
- [ ] Test: each brain type produces its expected dominant frequency band under neutral input?

---

## 7. Scoring System

**File:** `src/scoring.rs`

### What it does
Evaluates a neural simulation result against a specific goal. Returns a scalar fitness score
in [0, 1] representing how well the simulated brain state matches the target.

### Goals
| Goal | Target Brain State | Key Band Targets |
|------|--------------------|-----------------|
| `DeepRelaxation` | Pre-sleep, body-scan | High theta + alpha, low beta/gamma |
| `Focus` | Studying, problem-solving | Elevated beta, frontal theta |
| `Sleep` | NREM stage 1–2 | Theta dominant, delta emerging |
| `Isolation` | Noise masking | Spectrally flat (no dominant band) |
| `Meditation` | Deep alpha state | Strong alpha, minimal delta/gamma |
| `DeepWork` | Flow state | Strong beta, suppressed alpha |

### Scoring formula
```
For each band b:
  σ_b = (max_b - min_b) / 2.448      ← constant chosen so score = 0.05 at edges
  score_b = exp(-0.5 · (power_b - ideal_b)² / σ_b²)

Band score = weighted sum over δ, θ, α, β, γ
FHN score  = Gaussian score on firing_rate and ISI CV
Total score = band_weight · band_score + fhn_weight · fhn_score
```

### Review checklist
- [ ] Verify sigma formula: 2.448 ≈ sqrt(2·ln(20)) which gives ~5% score at band edges — confirm this is intended
- [ ] Verify each goal's band targets (min, ideal, max) against neuroscience literature:
  - [ ] DeepRelaxation: theta 4–8 Hz, alpha 8–13 Hz targets
  - [ ] Focus: beta 13–30 Hz, frontal theta targets
  - [ ] Sleep: theta dominant, delta emerging
  - [ ] Isolation: flat spectrum (all bands equal?)
  - [ ] Meditation: alpha peak, minimal flanking bands
  - [ ] DeepWork: beta dominant, alpha suppressed
- [ ] Verify band_weight + fhn_weight = 1.0 for all goals
- [ ] Verify FHN firing rate targets are physiologically reasonable per goal
- [ ] Verify FHN ISI CV targets: relaxation = lower CV (more regular), focus = slightly higher CV?
- [ ] Check `evaluate_with_brightness()`: how does audio brightness affect the score?
- [ ] Test: perfect band powers → score approaches 1.0?
- [ ] Test: worst-case band powers → score approaches 0.0?
- [ ] Test: score is monotonically decreasing with distance from ideal for each band?

---

## 8. Preset Parameter Space

**File:** `src/preset.rs`

### What it does
Defines the 190-dimensional parameter space (genome) for the Differential Evolution optimizer.
Handles encoding/decoding between structured preset and flat float vector. Also manages
applying presets to the NoiseEngine.

### Genome structure
```
GENOME_LEN = 190
Global (6 genes):
  [0] master_gain       [0.0, 1.0]
  [1] spatial_mode      {0=Stereo, 1=Immersive}
  [2] source_count      [2.0, 8.0]  ← decoded as integer
  [3] anchor_color      {0..6}      ← noise color enum
  [4] anchor_volume     [0.0, 1.0]
  [5] environment       {0..4}      ← acoustic environment enum

Per object (8 objects × 23 genes = 184 genes):
  active, color, x, y, z, volume, reverb_send
  bass_mod: kind, param_a, param_b, param_c
  satellite_mod: kind, param_a, param_b, param_c
  movement: kind, radius, speed, phase, depth_min, depth_max, reverb_min, reverb_max
```

### Modulator types
| Index | Type | param_a | param_b | param_c |
|-------|------|---------|---------|---------|
| 0 | Flat | — | — | — |
| 1 | SineLfo | frequency (Hz) | depth | phase |
| 2 | Breathing | inhale_time (s) | exhale_time (s) | depth |
| 3 | Stochastic | update_rate (Hz) | depth | smoothing |
| 4 | NeuralLfo | freq (Hz) | depth | sync_mode |

### Review checklist
- [ ] Verify GENOME_LEN = 6 + 8×23 = 190 (count is correct)
- [ ] Verify all parameter bounds are physiologically/technically sensible
- [ ] Verify `from_genome()` and `to_genome()` are exact inverses (round-trip test)
- [ ] Verify `clamp()` covers all 190 parameters with correct min/max
- [ ] Verify `discrete_gene_indices()` lists all integer/enum genes
- [ ] Check: are spatial_mode, anchor_color, environment properly rounded during DE?
- [ ] Check: is source_count correctly decoded as int (floor or round)?
- [ ] Verify `apply_to_engine()` maps all genome genes to correct engine parameters
- [ ] Test: genome encode → decode → encode produces identical floats?
- [ ] Test: clamp on out-of-bounds genome → all parameters in valid ranges?
- [ ] Test: apply_to_engine does not panic on boundary values (0 active sources, max sources)?

---

## 9. Spatial Movement Patterns

**File:** `src/movement.rs`

### What it does
Controls 3D spatial trajectories of audio sources over time. Each source has an independent
movement controller that updates position and reverb send every audio chunk.

### Movement patterns
| Pattern | Description | Key parameters |
|---------|-------------|---------------|
| `Static` | No movement | — |
| `Orbit` | Circular orbit in XZ plane | radius, speed, phase |
| `FigureEight` | Lemniscate path (∞ shape) | radius, speed, phase |
| `RandomWalk` | Brownian walk (bounded) | radius (max displacement), speed |
| `DepthBreathing` | Sinusoidal Z + reverb modulation | depth_min/max, reverb_min/max, speed |
| `Pendulum` | Arc pendulum swing | radius, speed, phase |

### Review checklist
- [ ] Verify Orbit: x = r·cos(θ), z = r·sin(θ) — correct XZ plane orbit?
- [ ] Verify FigureEight: lemniscate parametric equations (x = r·sin(t), z = r·sin(t)·cos(t)?
- [ ] Verify RandomWalk: is the walk bounded within radius correctly?
- [ ] Verify DepthBreathing: reverb modulation is in phase with depth oscillation?
- [ ] Verify Pendulum: is the arc correctly implemented (different from Orbit)?
- [ ] Check: all position updates are bounded to valid engine ranges (x/y/z in [-1, 1]?)
- [ ] Check: reverb_send is clamped to [0, 1] at all movement extremes
- [ ] Check: time parameter (phase accumulation) does not overflow for long simulations
- [ ] Test: static movement → position unchanged over time?
- [ ] Test: orbit → position traces a perfect circle (x² + z² = r²)?
- [ ] Test: depth_breathing → reverb oscillates between reverb_min and reverb_max?

---

## 10. Simulation Pipeline

**File:** `src/pipeline.rs`

### What it does
Orchestrates the full evaluation chain: preset → audio → cochlear → neural → score.
This is the central function called by both the optimizer (for fitness) and the CLI.

### Signal flow
```
1. Preset → NoiseEngine (configure audio sources)
2. Render audio: chunks of 50 ms at 48 kHz (stereo, interleaved)
3. Deinterleave → Left channel (L only used for neural model)
4. Gammatone filterbank → 4 tonotopic band envelopes
5. Decimate 48 kHz → 1 kHz (boxcar anti-alias, factor=48)
6. JR bilateral simulation (L/R hemispheres with band inputs)
7. Combine bilateral EEG → FHN drive signal
8. FHN simulation → firing_rate, ISI_CV
9. Compute band powers from bilateral JR output
10. Spectral brightness from raw audio FFT
11. Score against goal → fitness scalar
12. Compute performance vector (entrainment, E/I, spectral centroid)
```

### Constants
```
SAMPLE_RATE = 48_000 Hz
DECIMATION_FACTOR = 48   →  NEURAL_SR = 1_000 Hz
CHUNK_SIZE = 2_400 samples (50 ms at 48 kHz)
```

### Review checklist
- [ ] Verify decimation factor: 48_000 / 48 = 1_000 Hz neural sample rate
- [ ] Verify boxcar anti-aliasing before downsampling (is LP filter applied before decimation?)
- [ ] Check: only left channel used — is this correct? Should stereo be averaged?
- [ ] Verify warmup_discard_secs is applied before extracting metrics (not before rendering)
- [ ] Check chunk rendering loop: no off-by-one in sample count
- [ ] Verify movement updates are called once per chunk (not per sample, not per second)
- [ ] Verify band energy normalization: are band envelopes normalized per-chunk or over full signal?
- [ ] Check `spectral_brightness()`: FFT on full audio → centroid in [0, 1] mapping correct?
- [ ] Verify SimulationResult contains all expected fields
- [ ] Test: silent input → all band powers near zero?
- [ ] Test: pink noise input → roughly equal band powers (1/f spectrum)?
- [ ] Test: longer duration → more stable band power estimates (lower variance)?
- [ ] Test: warmup discard actually removes initial transient (score with/without warmup differs)?

---

## 11. Differential Evolution Optimizer

**File:** `src/optimizer/differential_evolution.rs`

### What it does
Gradient-free global optimizer that searches the 190-dimensional preset space to maximize
the neural simulation score. Uses the DE/rand/1/bin strategy.

### Algorithm
```
Strategy: DE/rand/1/bin

Initialization:
  - Population of N individuals uniformly sampled in [lower, upper] bounds
  - Optional: seed one individual from existing preset genome

Each generation:
  For each individual x_i:
    1. Select 3 random individuals a, b, c (all ≠ i)
    2. Mutation: v = a + F·(b - c)    [F = mutation factor, default 0.8]
    3. Crossover (binomial): for each gene j:
         trial_j = v_j if rand() < CR else x_i_j
         (at least 1 gene always comes from v, via random index)
    4. Clamp trial to bounds
    5. Round discrete genes (color, environment, etc.)
    6. Evaluate fitness of trial
    7. Greedy selection: if f(trial) >= f(x_i), replace x_i

Termination:
  - After max_generations
  - Or if fitness std < convergence_threshold (default 0.001)
```

### Review checklist
- [ ] Verify DE/rand/1/bin mutation: v = x_a + F·(x_b - x_c) with a≠b≠c≠i
- [ ] Verify binomial crossover: random index always takes trial gene (jrand guarantee)
- [ ] Verify greedy selection: >= (not just >) to allow neutral drift
- [ ] Verify bounds clamping is applied BEFORE discrete rounding
- [ ] Check: F and CR are within valid ranges (F in [0,2], CR in [0,1])?
- [ ] Check: population size ≥ 4 (required for DE/rand/1: need 3 distinct individuals ≠ i)?
- [ ] Verify `generate_trials()` produces exactly pop_size trial vectors
- [ ] Verify `report_fitness()` correctly updates the population (replaces when trial is better)
- [ ] Check: `best()` returns the individual with maximum fitness (not minimum)
- [ ] Check: `mean_fitness()` and `fitness_std()` use correct statistical formulas
- [ ] Check: convergence criterion uses fitness std (not parameter std)
- [ ] Test: known 1D function (e.g., -x²) → converges to correct optimum?
- [ ] Test: all-discrete genome → optimizer handles rounding without infinite loops?
- [ ] Test: single-individual population → graceful error or assertion?

---

## 12. Validation & Disturbance Tests

**Files:** `src/validate.rs`, `src/disturb.rs`, `src/neural/tests.rs`

### Validation tests (`validate.rs`, `neural/tests.rs`)
| Test | What it checks |
|------|---------------|
| Pure tone frequency tracking (10/20/40 Hz) | JR output dominant frequency matches input |
| Bifurcation threshold | Model transitions from non-oscillatory to oscillatory |
| Impulse response | Broadband spike produces measurable EEG transient |
| Stochastic resonance | Adding noise to weak signal improves detection |
| Spectral discrimination | Two closely-spaced tones produce distinct band power profiles |

### Disturbance resilience (`disturb.rs`)
```
1. Render baseline (pre-disturbance) → measure entrainment, freq, centroid
2. Inject acoustic spike at t_spike
3. Render post-spike window → measure nadir (worst drop)
4. Render recovery window → measure recovery metrics
5. Return DisturbResult: baseline / nadir / recovery triplet
```

### Review checklist
- [ ] Verify pure tone test: is input a clean sinusoid (no aliasing)?
- [ ] Verify frequency tracking tolerance: ±1 Hz or ±0.5 Hz?
- [ ] Check bifurcation test: is the transition input level physiologically realistic?
- [ ] Verify impulse response test: spike amplitude and duration are well-defined
- [ ] Verify stochastic resonance: SNR improvement criterion is well-defined (not just "noise helps")
- [ ] Check spectral discrimination: are the two test frequencies far enough apart to be reliably distinguished?
- [ ] Verify disturbance spike: what is the amplitude and duration? Is it documented?
- [ ] Check nadir measurement: is the window after the spike correct length?
- [ ] Check recovery measurement: is convergence threshold well-defined?
- [ ] Test: disturbance with no preset → graceful handling?

---

## 13. Unit Test Coverage Gaps

Current tests (as identified in the codebase):

| Area | Has Tests | Missing Tests |
|------|-----------|--------------|
| Gammatone filterbank | None | Pure tone response, ERB spacing, band group energy |
| FHN model | None | Spike detection, ISI CV, RK4 correctness |
| Jansen-Rit model | Partial (validate.rs) | Exact ODE values, sigmoid correctness |
| Wilson-Cowan model | None | Oscillation frequency, E-I stability |
| Brain type profiles | None | Per-type dominant frequency, parameter validity |
| Scoring system | Partial (regression_tests.rs) | Per-goal band target coverage, edge cases |
| Preset encode/decode | Partial (regression_tests.rs) | Full round-trip, boundary clamping |
| Movement patterns | None | Position traces, boundary enforcement |
| Pipeline integration | None | End-to-end with known preset |
| DE optimizer | None | Convergence on toy function, bounds handling |
| Performance vector | None | Entrainment ratio, E/I stability, centroid |

### Priority additions
1. **FHN spike detection** — critical for scoring correctness
2. **Gammatone frequency response** — validates auditory→neural signal path
3. **JR sigmoid** — foundational for all neural dynamics
4. **Scoring round-trip** — each goal with handcrafted band powers
5. **Genome round-trip** — from_genome(to_genome(x)) == x
6. **DE convergence** — simple 1D test function

---

## 14. CLI and Export

**Files:** `src/main.rs`, `src/export.rs`

### CLI subcommands
| Command | Purpose | Key Args |
|---------|---------|---------|
| `optimize` | Run DE optimizer | goal, generations, population, duration, seed, F, CR |
| `evaluate` | Score existing preset | preset_file, goal, brain_type |
| `disturb` | Test neural resilience | preset_file, spike_time |
| `validate` | Run neural model tests | (no args) |

### Export format (JSON)
```json
{
  "goal": "Focus",
  "score": 0.847,
  "timestamp": "2024-01-15T10:23:45Z",
  "analysis": {
    "band_powers": { "delta": 0.1, "theta": 0.2, ... },
    "fhn_firing_rate": 12.5,
    "fhn_isi_cv": 0.34,
    "brightness": 0.42
  },
  "preset": { ... }
}
```

### Review checklist
- [ ] Verify `optimize` default parameters are sensible (generations, pop size, duration)
- [ ] Verify `evaluate` handles missing preset file gracefully (error message, not panic)
- [ ] Verify `disturb` spike time defaults and documentation
- [ ] Verify `validate` runs all tests and reports pass/fail clearly
- [ ] Check JSON export: timestamp format is ISO 8601?
- [ ] Check JSON export: all score metadata fields are present
- [ ] Check: exported preset can be re-imported and scored identically (round-trip)?
- [ ] Test: `evaluate` on an exported preset → score matches original optimization score?

---

## Review Session Log

| Section | Reviewed | Math OK | Science OK | Tests OK | Issues Found |
|---------|----------|---------|-----------|---------|--------------|
| 1. Gammatone | | | | | |
| 2. FHN | | | | | |
| 3. Jansen-Rit | | | | | |
| 4. Wilson-Cowan | | | | | |
| 5. Performance Vector | | | | | |
| 6. Brain Types | | | | | |
| 7. Scoring | | | | | |
| 8. Preset Space | | | | | |
| 9. Movement | | | | | |
| 10. Pipeline | | | | | |
| 11. DE Optimizer | | | | | |
| 12. Validation Tests | | | | | |
| 13. Test Coverage | | | | | |
| 14. CLI & Export | | | | | |
