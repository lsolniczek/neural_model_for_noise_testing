# Neural Preset Optimizer

A scientifically-grounded system that optimizes audio noise generator presets to produce specific psychoacoustic effects on the human brain. It combines real-time DSP audio synthesis, cochlear auditory models, neural mass models from computational neuroscience, and a gradient-free optimizer to answer a single question:

**Which noise preset (color, spatial layout, movement, modulators) best induces a target brain state in a given individual?**

Brain states are quantified by EEG band powers (delta, theta, alpha, beta, gamma) and single-neuron firing statistics, validated against peer-reviewed neuroscience literature.

---

## Table of Contents

1. [How It Works](#how-it-works)
2. [Auditory Model: Gammatone Filterbank](#1-auditory-model-gammatone-filterbank)
3. [Neural Model: FitzHugh-Nagumo](#2-neural-model-fitzhugh-nagumo)
4. [Neural Model: Jansen-Rit / Wendling 2002](#3-neural-model-jansen-rit--wendling-2002)
5. [Neural Model: Wilson-Cowan](#4-neural-model-wilson-cowan)
6. [Neural Diagnostics: Performance Vector](#5-neural-diagnostics-performance-vector)
7. [Brain Type Profiles](#6-brain-type-profiles)
8. [Scoring System](#7-scoring-system)
9. [Preset Parameter Space](#8-preset-parameter-space)
10. [Spatial Movement Patterns](#9-spatial-movement-patterns)
11. [Simulation Pipeline](#10-simulation-pipeline)
12. [Differential Evolution Optimizer](#11-differential-evolution-optimizer)
13. [CLI Usage](#12-cli-usage)
14. [References](#references)

---

## How It Works

```
Audio Engine (48 kHz)
  7 noise colors x 8 spatial objects x 5 modulator types x 6 movement patterns
        |
        v
Cochlear Model (Gammatone Filterbank)
  32 channels, ERB-spaced, 50-8000 Hz
  --> 4 tonotopic frequency bands
        |
        v
Decimate 48 kHz --> 1 kHz
  Boxcar anti-aliasing filter (factor 48)
        |
        v
Bilateral Cortical Model
  Left hemisphere (fast, alpha/beta)  <-- 65% right ear + 35% left ear
  Right hemisphere (slow, delta/theta) <-- 65% left ear + 35% right ear
  Each: 4 tonotopic bands --> Jansen-Rit or Wilson-Cowan
  Coupled via corpus callosum (10 ms delay, 10-15% strength)
        |
        v
Single Neuron Model (FitzHugh-Nagumo)
  Driven by combined bilateral EEG
  --> Firing rate, ISI regularity
        |
        v
Scoring
  Band powers vs goal targets (Gaussian)
  FHN firing characteristics
  Audio spectral brightness
  --> Single scalar fitness [0, 1]
        |
        v
Differential Evolution Optimizer (DE/rand/1/bin)
  190-dimensional search space
  30+ population, 100+ generations
  --> Best preset for the target brain state
```

---

## 1. Auditory Model: Gammatone Filterbank

**File:** `src/auditory/gammatone.rs`

Simulates the basilar membrane's frequency decomposition using a bank of 4th-order gammatone filters. This is the standard cochlear model in computational auditory neuroscience, following Patterson et al. (1992).

### Design

| Parameter | Value | Reason |
|-----------|-------|--------|
| Channels | 32 | Sufficient resolution for 4-band tonotopic grouping |
| Frequency range | 50 - 8000 Hz | Covers speech and music fundamentals; above 8 kHz contributes little to cortical entrainment |
| Filter order | 4 | Matches auditory nerve tuning curve shape (Patterson 1992) |
| Frequency spacing | ERB scale | Perceptually uniform spacing matching human frequency resolution |
| Envelope LP cutoff | 80 Hz | Passes gamma-band (30-80 Hz) envelope modulations to the Wendling fast-inhibitory population; raised from the standard 50 Hz |
| Bandwidth factor | 1.019 | Adjusts 4th-order cascade -3dB bandwidth to match ERB (Holdsworth et al. 1988) |

### ERB Formula (Glasberg & Moore 1990)

```
ERB(f) = 24.7 * (4.37 * f/1000 + 1)
```

- ERB(50 Hz) = 30.1 Hz (narrowest channel)
- ERB(1000 Hz) = 132.6 Hz
- ERB(8000 Hz) = 888.2 Hz (widest channel)

### Filter Implementation

Each channel is a cascade of 4 complex 1st-order IIR stages:

```
y[n] = decay * exp(j*omega) * y[n-1] + x[n]
decay = exp(-1.019 * 2*pi * ERB(fc) / fs)
omega = 2*pi * fc / fs
```

The magnitude of the final complex output serves as half-wave rectification (always >= 0), followed by a 1-pole low-pass envelope smoother at 80 Hz to model inner hair cell transduction.

### Channel Weights

Each channel is weighted by `1/ERB(fc)` (normalized to sum to 1.0). This compensates for the gammatone filter's higher peak gain at low frequencies where ERB bandwidth is narrower. The weight ratio between the lowest (50 Hz) and highest (8000 Hz) channel is approximately 30x.

### Tonotopic Band Grouping

Channels are grouped into 4 frequency bands for cortical modeling:

| Band | Frequency Range | Cortical Target | Typical Channels |
|------|----------------|-----------------|:----------------:|
| 0 - Low | 50 - 200 Hz | Delta/Theta drive | ~4 |
| 1 - Low-mid | 200 - 800 Hz | Theta/Alpha drive | ~8 |
| 2 - Mid-high | 800 - 3000 Hz | Alpha/Beta drive | ~11 |
| 3 - High | 3000 - 8000 Hz | Beta/Gamma drive | ~9 |

Band boundaries at 200, 800, and 3000 Hz are shared between channel assignment and FFT energy fraction computation.

---

## 2. Neural Model: FitzHugh-Nagumo

**File:** `src/neural/fhn.rs`

A 2-variable simplification of the Hodgkin-Huxley model that captures the essential dynamics of neuronal excitability: resting state, threshold, spiking, and recovery. Used as a single-neuron probe driven by the bilateral cortical EEG.

### Equations (FitzHugh 1961, Nagumo et al. 1962)

```
dv/dt = v - v^3/3 - w + I(t)
dw/dt = epsilon * (v + a - b*w)
```

| Variable | Meaning |
|----------|---------|
| v | Membrane potential (fast variable) |
| w | Recovery variable (slow variable) |
| I(t) | External input current (from JR bilateral EEG) |
| epsilon | Time-scale separation (small = slow recovery) |
| a, b | Shape parameters controlling nullcline geometry |

### Parameters

| Parameter | Default | Range across brain types | Reason |
|-----------|---------|:------------------------:|--------|
| a | 0.7 | 0.7 - 0.75 | Controls resting equilibrium position; 0.75 for Aging raises excitation threshold |
| b | 0.8 | 0.8 (all types) | Restitution strength |
| epsilon | 0.08 | 0.06 - 0.10 | 0.06 for HighAlpha/Aging (slower recovery, more regular firing); 0.10 for ADHD/Anxious (faster recovery, more responsive) |
| input_scale | 1.5 | 1.2 - 1.8 | Maps normalized EEG [-1,1] to effective drive current |
| time_scale | 300 | 250 - 350 | Maps FHN model time to real time; 250 for Aging (slower), 350 for ADHD (faster dopamine-driven processing) |

### Integration

4th-order Runge-Kutta (RK4) with 4 sub-steps per input sample. At the 1 kHz neural sample rate with time_scale=300:

```
h = (1/1000) * 300 / 4 = 0.075 model time units per sub-step
```

The FHN natural period is ~12 model time units, giving ~160 steps per cycle --- well within RK4 accuracy requirements.

### Outputs

- **Spike detection**: Upward threshold crossing at v = 1.0 (the cubic nullcline peak).
- **Firing rate**: `n_spikes / duration_seconds`.
- **ISI CV** (coefficient of variation of inter-spike intervals): `std(ISIs) / mean(ISIs)`. Returns NaN for fewer than 3 spikes (insufficient data). CV = 0 means perfectly regular; CV ~ 0.3-0.4 is typical for cortical neurons.

### Bifurcation Structure

The FHN model has two Hopf bifurcations:
- **Lower** (~I = 0.34): transition from resting to oscillation
- **Upper** (~I = 1.4): transition from oscillation back to a stable high-v fixed point

The pipeline drives the FHN with oscillatory EEG that periodically crosses the lower bifurcation, triggering spikes on each EEG oscillation peak.

---

## 3. Neural Model: Jansen-Rit / Wendling 2002

**File:** `src/neural/jansen_rit.rs`

The core neural mass model. Simulates EEG-like dynamics from 4 interacting neural populations in a cortical column. This is the Wendling et al. (2002) extension of the classic Jansen & Rit (1995) model, adding a fast GABA-A inhibitory population to the original 3-population system.

### Populations

| Population | Neurotransmitter | Role |
|-----------|-----------------|------|
| Pyramidal cells | Glutamate | Main output (EEG) |
| Excitatory interneurons | Glutamate | Recurrent excitation |
| Slow inhibitory interneurons | GABA-B | Slow inhibition (~50 ms time constant) |
| Fast inhibitory interneurons | GABA-A | Fast inhibition (~2 ms time constant) |

### 8-State ODE System

```
State variables:
  y0, y4: Excitatory interneuron PSP and derivative
  y1, y5: Pyramidal cell excitatory PSP and derivative
  y2, y6: Slow inhibitory (GABA-B) PSP and derivative
  y3, y7: Fast inhibitory (GABA-A) PSP and derivative

EEG output: y1 - y2 - y3 (net pyramidal membrane voltage)
```

The derivative equations:

```
dy0/dt = y4
dy1/dt = y5
dy2/dt = y6
dy3/dt = y7

dy4/dt = A*a*S(y1-y2-y3)     - 2a*y4 - a^2*y0
dy5/dt = A*a*(p + C2*S(C1*y0)) - 2a*y5 - a^2*y1
dy6/dt = B*b*C4*S(C3*y0)     - 2b*y6 - b^2*y2
dy7/dt = G*g*(C5*S(vpyr) - C6*S(C3*y0)) - 2g*y7 - g^2*y3
```

When G = 0 (g_fast_gain = 0), y3 and y7 stay at zero, reducing to the classic JR 1995 model with EEG = y1 - y2.

### Sigmoid Transfer Function

```
S(v) = V_MAX / (1 + exp(-r * (v - V0)))
```

| Parameter | Value | Source |
|-----------|-------|--------|
| V_MAX | 5.0 spikes/s | 2 * e0, where e0 = 2.5 (Jansen & Rit 1995) |
| V0 | 6.0 mV | Firing threshold (5.5 for ADHD = increased excitability) |
| r | 0.62 mV^-1 | Sigmoid steepness; tuned from literature 0.56 for this application |

S(V0) = V_MAX/2 = 2.5 (half-maximum firing rate at threshold).

### Connectivity Constants

| Constant | Formula | Value | Pathway |
|----------|---------|-------|---------|
| C1 | C | 135 | Pyramidal <-> Excitatory interneuron |
| C2 | 0.8 * C | 108 | Pyramidal -> Slow inhibitory |
| C3 | 0.20 * C | 27 | Slow inhibitory -> Pyramidal (tuned from 0.25 for beta access) |
| C4 | 0.20 * C | 27 | Excitatory interneuron -> Pyramidal feedback |
| C5 | 0.3 * C | 40.5 | Pyramidal -> Fast inhibitory |
| C6 | 0.1 * C | 13.5 | Slow inhibitory -> Fast inhibitory (disinhibition) |
| C7 | ~115 | per brain type | Fast inhibitory -> Pyramidal |

C3 and C4 are reduced from the literature value of 0.25*C to 0.20*C. This loosens the GABA-B theta anchor, allowing the model to access beta-range oscillations when driven by higher-frequency input.

### Second-Order Linear Operators

Each population's PSP dynamics are modeled as a second-order linear filter:

```
Excitatory: H_e(s) = A*a / (s + a)^2    (gain A=3.25 mV, rate a=100/s, tau=10 ms)
Slow inhibitory: H_i(s) = B*b / (s + b)^2  (gain B=22 mV, rate b=50/s, tau=20 ms)
Fast inhibitory: H_f(s) = G*g / (s + g)^2  (gain G=10 mV, rate g=500/s, tau=2 ms)
```

The fast inhibitory population has a much shorter time constant (2 ms vs 20 ms), enabling it to track rapid oscillations up to ~17 Hz.

### Integration

RK4 with adaptive sub-stepping:
- g_fast_rate > 200: 3 sub-steps (h = 0.333 ms at 1 kHz)
- g_fast_rate <= 200: 2 sub-steps (h = 0.5 ms)

The fastest time constant is 1/g_fast_rate = 2 ms. At h = 0.333 ms: h/tau = 0.167 --- within RK4 stability limits.

### Band Power Extraction

EEG output is detrended (DC removed) and analyzed via Hann-windowed FFT:

| Band | Frequency Range | EEG Correlate |
|------|----------------|---------------|
| Delta | 0.5 - 4 Hz | Deep sleep |
| Theta | 4 - 8 Hz | Relaxation, meditation |
| Alpha | 8 - 13 Hz | Calm alertness |
| Beta | 13 - 30 Hz | Active focus |
| Gamma | 30 - 50 Hz | High-level processing |

### Bilateral Simulation

Two independent sets of 4 tonotopic JR/WC models (one per hemisphere), implementing the Asymmetric Sampling in Time (AST) hypothesis (Poeppel 2003):

- **Left hemisphere**: Faster time constants (alpha/beta preference). Receives 65% right ear + 35% left ear (contralateral routing).
- **Right hemisphere**: Slower time constants (delta/theta preference). Receives 65% left ear + 35% right ear.
- **Callosal coupling**: Linear additive coupling with ~10 ms delay and 11-15% strength (brain-type dependent). Modeled as a perturbative effect since callosal input is ~10% of convergent intracortical input.
- **Alpha asymmetry index**: `(left_alpha - right_alpha) / (left_alpha + right_alpha)`, range [-1, +1].

### Tonotopic RMS Normalization

Each band's EEG output is sqrt-compressed (`norm = 1/sqrt(rms)`) before mixing by energy fraction. This reduces the dynamic range between slow (high-amplitude) and fast (low-amplitude) bands while preserving relative amplitude differences. Full unit-RMS normalization (1/rms) would erase inter-band dynamics entirely.

---

## 4. Neural Model: Wilson-Cowan

**File:** `src/neural/wilson_cowan.rs`

Models fast cortical oscillations (beta/gamma, 13-100 Hz) via excitatory-inhibitory population dynamics. Used as a complement to Jansen-Rit for tonotopic bands 2-3, where JR cannot produce oscillations above ~17 Hz.

### Equations (Wilson & Cowan 1972)

```
tau_e * dE/dt = -E + S(w_ee*E - w_ei*I + P + h_e)
tau_i * dI/dt = -I + S(w_ie*E - w_ii*I + h_i)
```

| Variable | Meaning |
|----------|---------|
| E | Excitatory population firing rate [0, 1] |
| I | Inhibitory population firing rate [0, 1] |
| P | External drive (from auditory model) |
| S(x) | Sigmoid: `1 / (1 + exp(-a*(x - theta)))` |

### Parameters

| Parameter | Value | Reason |
|-----------|-------|--------|
| w_ee | 16.0 | Recurrent excitation; strong enough for sustained oscillation |
| w_ei | 15.0 | Inhibition -> excitation; main oscillation driver |
| w_ie | 15.0 | Excitation -> inhibition; closes the E-I loop |
| w_ii | 3.0 | Recurrent inhibition; stabilizer |
| sigmoid_a | 1.3 | Steep enough for robust oscillation |
| sigmoid_theta | 4.0 | Firing threshold |
| h_e | 1.5 | External bias pushing E into oscillatory regime |
| h_i | 0.0 | No external bias on inhibition |

### Frequency Tuning

The oscillation frequency is controlled by `tau_e` and `tau_i`:

```
f = 1 / (2.45 * (tau_e + tau_i))    (empirically calibrated)
tau_e = 0.45 * tau_sum               (excitation slightly faster)
tau_i = 0.55 * tau_sum               (inhibition slightly slower)
```

For a 25 Hz target: `tau_sum = 1/(2.45*25) = 16.3 ms`, so `tau_e = 7.3 ms`, `tau_i = 9.0 ms`.

### EEG Output

`EEG = E(t) - I(t)` (net excitatory-inhibitory balance). Integration uses RK2 (midpoint method) with 2 sub-steps.

### Usage in Tonotopic Model

| Brain Type | Band 0-1 | Band 2 | Band 3 |
|-----------|----------|--------|--------|
| Normal | JR | WC @ 14 Hz | WC @ 25 Hz |
| ADHD | JR | WC @ 14 Hz | WC @ 25 Hz |
| HighAlpha | JR | JR | JR |
| Aging | JR | JR | JR |
| Anxious | JR | JR | JR |

HighAlpha, Aging, and Anxious use all-JR because their characteristic brain states don't require high-frequency WC oscillators.

---

## 5. Neural Diagnostics: Performance Vector

**File:** `src/neural/performance.rs`

Three diagnostic metrics that expose *how* the noise affects the neural model, reported alongside the scalar score but not used in optimization.

### Entrainment Ratio

```
ratio = power_in([target_freq +/- 1 Hz]) / total_power_in([0.5, 80 Hz])
```

Measures how well the neural output locks onto the preset's NeuralLFO frequency. Range [0, 1]. Higher = stronger entrainment. Only computed when a NeuralLFO modulator is active (kind=4 in the preset).

### E/I Stability Index

```
CV = std_dev(y[3]) / |mean(y[3])|
```

Coefficient of variation of the fast inhibitory PSP trace. Low CV = stable GABA-A loop; high CV = chaotic dynamics. Falls back to raw std_dev when mean is near zero (fast inhibition barely active). Only computed when Wendling fast inhibition is enabled.

### Spectral Centroid

```
centroid = sum(freq * power) / sum(power)    over [1, 50 Hz]
```

The "centre of mass" of the EEG power spectrum. A shift from the ~10 Hz alpha baseline indicates the noise is pulling the brain toward a different regime. Default 10.0 Hz for silent/zero-power signals.

---

## 6. Brain Type Profiles

**File:** `src/brain_type.rs`

Five neurological profiles that parameterize both the FHN and Jansen-Rit models to simulate individual variation. Each profile adjusts excitability, inhibition, time constants, connectivity, and hemispheric coupling based on the clinical/physiological literature.

### Normal (healthy adult baseline)

The reference configuration. Standard JR parameters (Jansen & Rit 1995) with Wendling 2002 fast inhibition.

- **FHN**: a=0.7, b=0.8, epsilon=0.08, input_scale=1.5, time_scale=300
- **JR**: A=3.25, B=22, a=100, b=50, C=135, input_offset=175, g_fast_gain=10, g_fast_rate=500, C7=115
- **Bilateral**: callosal_coupling=0.15, delay=10 ms

### HighAlpha (meditation practitioner)

Stronger alpha oscillation, higher inhibition, less responsive to external input. Models experienced meditators who show robust bilateral alpha synchrony (Lomas et al. 2015).

- **FHN**: epsilon=0.06 (slower recovery = more regular firing), input_scale=1.3 (less responsive)
- **JR**: B=25 (stronger GABA-B, Streeter et al. 2010), input_offset=195 (deeper in oscillatory regime), input_scale=40 (less influenced by external drive), g_fast_gain=8 (reduced fast inhibition preserves alpha dominance)
- **Bilateral**: callosal_coupling=0.15 (strong bilateral synchrony from training)

### ADHD (hypoaroused cortex)

Weaker inhibition, theta-dominant at rest, beta deficit without external drive. Near the lower Hopf bifurcation boundary so external noise can push higher bands across threshold via stochastic resonance (Barry et al. 2003).

- **FHN**: epsilon=0.10 (faster recovery = more responsive), input_scale=1.8 (more sensitive to input), time_scale=350 (faster dopamine-driven temporal processing)
- **JR**: a_gain=3.5 (increased excitation), B=18 (weaker GABA-B, Edden et al. 2012), input_offset=135 (near bifurcation = hypoaroused), input_scale=80 (more sensitive to external drive), g_fast_rate=450 (slower GABA-A kinetics), slow_inhib_ratio=0.15 (weaker GABA-B), V0=5.5 (lower threshold = increased excitability)
- **Bilateral**: callosal_coupling=0.12 (weaker interhemispheric transfer)

### Aging (older adult)

Slower time constants, reduced connectivity, weaker callosal transfer. Models age-related cortical slowing and white matter degradation (Babiloni et al. 2006, Voytek et al. 2015).

- **FHN**: a=0.75 (higher excitation threshold), epsilon=0.06 (slower recovery), input_scale=1.2 (less responsive), time_scale=250 (slower dynamics / reduced neural sustain)
- **JR**: a_rate=80 (slower excitatory dynamics), b_rate=40 (slower inhibitory dynamics), C=120 (reduced connectivity, Salat et al. 2005), g_fast_rate=350 (slowed GABA-A kinetics)
- **Bilateral**: callosal_coupling=0.11 (~25% reduction from Normal, Sullivan & Pfefferbaum 2006), delay=12 ms (slower transfer)

### Anxious (hyperactive cortex)

Overdriven cortex, elevated beta, always oscillating. Hyperexcitability driven by increased excitatory gain and connectivity (Mathersul et al. 2008).

- **FHN**: epsilon=0.10 (faster recovery), input_scale=1.8 (more sensitive)
- **JR**: a_gain=3.5 (increased excitation), B=20 (slightly more GABA-B tone), C=145 (stronger connectivity), input_offset=220 (deep in oscillatory regime = always active), g_fast_gain=15 (hyperactive fast inhibition = excessive beta/gamma)
- **Bilateral**: callosal_coupling=0.14 (hyperconnected)

### Cross-Type Invariants

These orderings are enforced by unit tests:

| Property | Ordering | Interpretation |
|----------|----------|---------------|
| GABA-B strength (b_gain) | ADHD(18) < Anxious(20) < Normal(22) < HighAlpha(25) | ADHD deficit, meditation GABA increase |
| Arousal (input_offset) | ADHD(135) < Aging(165) < Normal(175) < HighAlpha(195) < Anxious(220) | Hypoaroused to hyperaroused |
| Callosal coupling | Aging(0.11) < ADHD(0.12) < Anxious(0.14) < Normal(0.15) | White matter integrity |
| FHN epsilon | HighAlpha(0.06) < Normal(0.08) < ADHD(0.10) | Stability to responsiveness |
| FHN time_scale | Aging(250) < Normal(300) < ADHD(350) | Slow to fast temporal processing |

---

## 7. Scoring System

**File:** `src/scoring.rs`

Evaluates neural simulation results against evidence-based neuroscience targets. Returns a scalar fitness in [0, 1] for the optimizer.

### Formula

```
total = 0.9 * neural_score + 0.1 * brightness_modifier

neural_score = band_weight * band_score + fhn_weight * fhn_score
```

Neural models do 90% of the work; spectral brightness is a 10% psychoacoustic complement.

### Gaussian Band Scoring

Each EEG band is scored against a target (min, ideal, max):

```
sigma = (max - min) / (2 * 2.448)
score = exp(-0.5 * ((power - ideal) / sigma)^2)
```

The constant 2.448 = sqrt(-2 * ln(0.05)) is chosen so the score equals approximately 5% at the min/max boundaries. This provides smooth gradients (good for optimization) with continuous non-zero values (no hard cutoffs).

### Goals

#### Deep Relaxation
Theta + alpha dominant. Eyes-closed relaxation, body scan, pre-sleep unwinding.
Ref: Klimesch 1999 (alpha in relaxation), Niedermeyer 2005.

| Band | Min | Ideal | Max |
|------|-----|-------|-----|
| Delta | 0.05 | 0.22 | 0.40 |
| Theta | 0.18 | 0.35 | 0.52 |
| Alpha | 0.20 | 0.36 | 0.52 |
| Beta | 0.00 | 0.03 | 0.14 |
| Gamma | 0.00 | 0.01 | 0.06 |

FHN: rate 1-6 Hz, ISI CV 0.38, weight 0.30. Band weight 0.70.

#### Focus
Beta prominent, frontal theta present. Active task engagement --- studying, monitoring, problem-solving.
Ref: Engel & Fries 2010 (beta maintenance), Cavanagh & Frank 2014 (frontal theta).

| Band | Min | Ideal | Max |
|------|-----|-------|-----|
| Delta | 0.00 | 0.01 | 0.08 |
| Theta | 0.08 | 0.18 | 0.32 |
| Alpha | 0.18 | 0.33 | 0.50 |
| Beta | 0.25 | 0.42 | 0.60 |
| Gamma | 0.02 | 0.06 | 0.15 |

FHN: rate 8-20 Hz, ISI CV 0.30, weight 0.30. Band weight 0.70.
Note: Gamma power is generated by WilsonCowan oscillators in bands 2-3, not by the JR model (which maxes out at ~17 Hz).

#### Sleep
NREM stage 1-2: theta dominant, delta emerging, alpha fading. Models sleep onset.
Ref: Ogilvie 2001 (sleep onset EEG), Carskadon & Dement 2011.

| Band | Min | Ideal | Max |
|------|-----|-------|-----|
| Delta | 0.08 | 0.30 | 0.50 |
| Theta | 0.28 | 0.48 | 0.68 |
| Alpha | 0.00 | 0.12 | 0.25 |
| Beta | 0.00 | 0.02 | 0.08 |
| Gamma | 0.00 | 0.02 | 0.06 |

FHN: rate 0.5-4 Hz, ISI CV 0.42 (bursting pattern), weight 0.35. Band weight 0.65.

#### Isolation
Flat spectral distribution --- neutral cortical state for noise masking.

All bands: min=0.10, ideal=0.20, max=0.30 (uniform).
Uses flatness scoring: `1.0 - sum(|band - 0.2|) / 2.0` instead of Gaussian.
FHN: rate 2-10 Hz, no ISI CV target, weight 0.20. Band weight 0.80.

#### Meditation
Theta + alpha co-dominant. Focused-attention meditation (samatha, zazen, TM).
Ref: Lomas et al. 2015 meta-analysis.

| Band | Min | Ideal | Max |
|------|-----|-------|-----|
| Delta | 0.02 | 0.08 | 0.22 |
| Theta | 0.25 | 0.40 | 0.56 |
| Alpha | 0.25 | 0.40 | 0.56 |
| Beta | 0.00 | 0.03 | 0.12 |
| Gamma | 0.00 | 0.02 | 0.08 |

FHN: rate 1-6 Hz, ISI CV 0.28 (rhythmic), weight 0.35. Band weight 0.65.

#### Deep Work
Alpha dominant with theta support. Flow state for cognitively demanding tasks.
Ref: Katahira et al. 2018 (alpha in flow), Ulrich et al. 2016.

| Band | Min | Ideal | Max |
|------|-----|-------|-----|
| Delta | 0.00 | 0.01 | 0.06 |
| Theta | 0.15 | 0.30 | 0.46 |
| Alpha | 0.35 | 0.52 | 0.70 |
| Beta | 0.02 | 0.10 | 0.24 |
| Gamma | 0.00 | 0.02 | 0.08 |

FHN: rate 4-12 Hz, ISI CV 0.30, weight 0.25. Band weight 0.75.

### FHN Scoring

- **Firing rate**: 1.0 inside target range; exponential decay outside: `exp(-2 * |deviation| / boundary)`
- **ISI CV**: Gaussian: `exp(-4 * |cv - target_cv|)`. NaN CV (< 3 spikes) receives zero credit.
- Total: weighted average of rate score and CV score.

### Brightness Modifier

Maps the audio spectral centroid (dark/brown to bright/white) to a [0, 1] preference per goal:

| Goal | Preference | Peak |
|------|-----------|------|
| Sleep | Dark sounds (inverse with brightness) | brightness = 0.0 |
| Deep Relaxation | Lower brightness (natural 1/f spectra) | brightness ~ 0.15 |
| Meditation | Low-to-moderate | brightness ~ 0.35 |
| Deep Work | Moderate dark (brown/pink) | brightness = 0.35 |
| Focus | Moderate-to-bright (inverted U) | brightness = 0.55 |
| Isolation | Bright (wider masking bandwidth) | brightness = 1.0 |

---

## 8. Preset Parameter Space

**File:** `src/preset.rs`

### Genome Structure

The full noise engine configuration is encoded as a flat 190-dimensional float vector for the optimizer:

```
GENOME_LEN = 6 (global) + 8 * 23 (per-object) = 190
```

**Global parameters (6):**

| Index | Parameter | Range | Type |
|:-----:|-----------|-------|------|
| 0 | master_gain | [0.1, 1.0] | continuous |
| 1 | spatial_mode | {0=Stereo, 1=Immersive} | discrete |
| 2 | source_count | [2, 8] | discrete |
| 3 | anchor_color | {0..6} | discrete |
| 4 | anchor_volume | [0.0, 1.0] | continuous |
| 5 | environment | {0..4} | discrete |

**Per-object parameters (23 x 8 objects = 184):**

| Offset | Parameter | Range | Type |
|:------:|-----------|-------|------|
| 0 | active | {0, 1} | discrete |
| 1 | color | {0..6} | discrete |
| 2-4 | x, y, z | [-5,5], [-3,3], [-5,5] | continuous |
| 5 | volume | [0, 1] | continuous |
| 6 | reverb_send | [0, 1] | continuous |
| 7 | bass_mod.kind | {0..4} | discrete |
| 8-10 | bass_mod params | varies by kind | continuous |
| 11 | sat_mod.kind | {0..4} | discrete |
| 12-14 | sat_mod params | varies by kind | continuous |
| 15 | movement.kind | {0..5} | discrete |
| 16-22 | movement params | varies by kind | continuous |

**Noise colors (7):** White, Pink, Brown, Blue, Violet, Grey, SSN (Speech-Shaped Noise).

**Modulator types (5):**

| Kind | Name | param_a | param_b | param_c |
|:----:|------|---------|---------|---------|
| 0 | Flat | --- | --- | --- |
| 1 | SineLfo | freq [0.01, 2] Hz | depth [0, 1] | --- |
| 2 | Breathing | pattern [0, 3] | min_gain [0, 1] | --- |
| 3 | Stochastic | lambda [0.1, 10] | decay_ms [10, 500] | min_gain [0.05, 0.5] |
| 4 | NeuralLfo | freq [1, 40] Hz | depth [0, 1] | --- |

The Stochastic decay_ms parameter is remapped to [0, 1] in the genome space via `(decay_ms - 10) / 490` so the optimizer can explore its full range.

**Acoustic environments (5):** AnechoicChamber, FocusRoom, OpenLounge, VastSpace, DeepSanctuary.

**44 discrete genes** (out of 190 total) are rounded to integers after DE mutation to prevent wasting optimizer budget on continuous values that map to the same discrete setting.

---

## 9. Spatial Movement Patterns

**File:** `src/movement.rs`

Six spatial trajectory patterns for audio sources, updated at 50 ms intervals during audio rendering.

| Pattern | Equations | Key Parameters |
|---------|-----------|---------------|
| **Static** | No movement | --- |
| **Orbit** | x = r*cos(theta), z = r*sin(theta) | radius, speed (rad/s), phase |
| **FigureEight** | x = r*cos(theta), z = r*sin(theta)*cos(theta) | radius, speed, phase |
| **RandomWalk** | Brownian with damping (0.98/tick), velocity clamp (0.5), boundary reflection | radius (constraint), speed |
| **DepthBreathing** | z = min_z + (max_z-min_z) * (sin(theta)+1)/2, reverb follows depth | depth_min/max, reverb_min/max, speed |
| **Pendulum** | angle = 0.8*sin(theta), x = r*sin(angle), z = r*cos(angle) | radius, speed, phase |

- **Orbit** traces a perfect circle in the XZ plane (horizontal around the listener): x^2 + z^2 = r^2.
- **FigureEight** is a Lissajous curve (1:2 frequency ratio) that crosses the origin twice per cycle.
- **DepthBreathing** modulates reverb send in phase with Z depth --- farther objects get more reverb (physically correct).
- **Pendulum** swings +/-0.8 radians (+/-45.8 deg) on a circular arc of radius r.
- **RandomWalk** uses a seeded xorshift64 PRNG for determinism across evaluations.

---

## 10. Simulation Pipeline

**File:** `src/pipeline.rs`

The core evaluation chain called by the optimizer for each candidate preset.

### Constants

| Parameter | Value | Reason |
|-----------|-------|--------|
| SAMPLE_RATE | 48,000 Hz | Standard audio rate; matches the NoiseEngine |
| DECIMATION_FACTOR | 48 | 48,000 / 48 = 1,000 Hz neural sample rate |
| NEURAL_SR | 1,000 Hz | Well above the 50 Hz gamma ceiling (Nyquist = 500 Hz) |

### Signal Flow

1. **Engine warmup**: 1 second of audio rendered and discarded (engine + HRTF settle).
2. **Audio rendering**: `duration_secs` (default 12 s) at 48 kHz stereo, in 50 ms chunks with movement updates.
3. **Deinterleave**: Stereo to separate L/R channels.
4. **Gammatone filterbank**: Separate filterbank per ear. 32 channels grouped into 4 tonotopic bands.
5. **Band normalization**: Per-band max-normalization to [0, 1] for each ear independently.
6. **Spectral brightness**: FFT-based spectral centroid of left channel, mapped to [0, 1] via log scale (100 Hz = 0, 10 kHz = 1).
7. **Decimation**: 48x boxcar averaging (48 kHz to 1 kHz). Then discard first 2 seconds (neural warmup).
8. **Bilateral JR/WC model**: 2 x 4 parallel models at NEURAL_SR, with callosal coupling.
9. **FHN neuron**: Driven by normalized bilateral EEG [-1, 1] at NEURAL_SR.
10. **Performance vector**: Entrainment ratio, E/I stability, spectral centroid (diagnostic).
11. **Score**: `0.9 * (band_weight * band_score + fhn_weight * fhn_score) + 0.1 * brightness_modifier`.

### Warmup Strategy

| Phase | Duration | Purpose |
|-------|----------|---------|
| Audio warmup | 1 second | Engine + HRTF pipeline settle |
| Neural warmup discard | 2 seconds | Gammatone filterbank ring-up + JR/WC ODE transient |
| **Total before analysis** | **3 seconds** | Leaves 10 seconds of clean data from 12-second render |

---

## 11. Differential Evolution Optimizer

**File:** `src/optimizer/differential_evolution.rs`

Gradient-free global optimizer that searches the 190-dimensional preset space. Uses the DE/rand/1/bin strategy (Storn & Price 1997).

### Algorithm

```
For each target vector x_i in population:
  1. Pick 3 distinct random individuals a, b, c (all != i)
  2. Mutation: donor = x_a + F * (x_b - x_c)
  3. Binomial crossover: for each gene j:
       trial[j] = donor[j]  if rand() < CR or j == j_rand
                  x_i[j]    otherwise
  4. Clamp trial to bounds
  5. Round discrete genes
  6. Evaluate fitness
  7. If f(trial) >= f(x_i): replace x_i with trial
```

### Parameters

| Parameter | Typical Value | Meaning |
|-----------|:-------------:|---------|
| F | 0.8 | Mutation scale factor; controls exploration radius |
| CR | 0.9 | Crossover probability; high = more genes from donor |
| Population size | 30 | For 190 dimensions; 5-10x dim for full coverage would be 950-1900, but 30 is practical for expensive evaluations |

### Features

- **j_rand guarantee**: At least 1 gene always comes from the donor vector (prevents identity trials).
- **Greedy selection with neutral drift**: Uses >= (not >), allowing ties to replace parents. This helps escape fitness plateaus.
- **Discrete gene handling**: 44 genes (colors, movement kinds, modulator kinds, etc.) are rounded to integers after mutation.
- **Seed from genome**: Existing preset can be used to initialize the population with perturbations, warm-starting the search.
- **Convergence detection**: Via `fitness_std()` --- if population fitness standard deviation < threshold, the search has converged.

---

## 12. CLI Usage

### Commands

```bash
# Optimize a preset for a goal
cargo run -- optimize --goal focus --generations 100 --pop-size 30 --duration 12

# Evaluate an existing preset
cargo run -- evaluate --preset presets/deep_work_v6.json --goal deep_work --brain-type normal

# Test neural resilience to acoustic spike
cargo run -- disturb --preset presets/balanced_theta_smr.json --spike-time 4.0

# Run neural model validation tests
cargo run -- validate
```

### Goals

`deep_relaxation`, `focus`, `sleep`, `isolation`, `meditation`, `deep_work`

### Brain Types

`normal`, `high_alpha`, `adhd`, `aging`, `anxious`

---

## References

- **Jansen & Rit 1995**: Electroencephalogram and visual evoked potential generation in a mathematical model of coupled cortical columns. *Biological Cybernetics*, 73(4), 357-366.
- **Wendling et al. 2002**: Epileptic fast activity can be explained by a model of impaired GABAergic dendritic inhibition. *European Journal of Neuroscience*, 15(9), 1499-1508.
- **Wilson & Cowan 1972**: Excitatory and inhibitory interactions in localized populations of model neurons. *Biophysical Journal*, 12(1), 1-24.
- **FitzHugh 1961**: Impulses and physiological states in theoretical models of nerve membrane. *Biophysical Journal*, 1(6), 445-466.
- **Patterson et al. 1992**: Complex sounds and auditory images. In *Auditory Physiology and Perception* (pp. 429-446).
- **Glasberg & Moore 1990**: Derivation of auditory filter shapes from notched-noise data. *Hearing Research*, 47(1-2), 103-138.
- **Holdsworth et al. 1988**: Implementing a gammatone filter bank. *SVOS Final Report*, Part A.
- **Storn & Price 1997**: Differential evolution --- a simple and efficient heuristic for global optimization. *Journal of Global Optimization*, 11(4), 341-359.
- **Poeppel 2003**: The analysis of speech in different temporal integration windows. *Speech Communication*, 41(1), 245-255.
- **Klimesch 1999**: EEG alpha and theta oscillations reflect cognitive and memory performance. *Brain Research Reviews*, 29(2-3), 169-195.
- **Engel & Fries 2010**: Beta-band oscillations --- signalling the status quo? *Current Opinion in Neurobiology*, 20(2), 156-165.
- **Cavanagh & Frank 2014**: Frontal theta as a mechanism for cognitive control. *Trends in Cognitive Sciences*, 18(8), 414-421.
- **Ogilvie 2001**: The process of falling asleep. *Sleep Medicine Reviews*, 5(3), 247-270.
- **Lomas et al. 2015**: A systematic review of neurophysiological evidence for mindfulness meditation. *Neuroscience & Biobehavioral Reviews*, 57, 401-410.
- **Katahira et al. 2018**: EEG correlates of the flow state. *Frontiers in Psychology*, 9, 300.
- **Barry et al. 2003**: A review of electrophysiology in ADHD. *Clinical Neurophysiology*, 114(2), 171-183.
- **Edden et al. 2012**: Reduced GABA concentration in ADHD. *Archives of General Psychiatry*, 69(7), 750-753.
- **Streeter et al. 2010**: Effects of yoga versus walking on mood, anxiety, and brain GABA levels. *Journal of Alternative and Complementary Medicine*, 16(11), 1145-1152.
- **Sullivan & Pfefferbaum 2006**: Diffusion tensor imaging and aging. *Neuroscience & Biobehavioral Reviews*, 30(6), 749-761.
- **Babiloni et al. 2006**: Sources of cortical rhythms in adults during physiological aging. *Neurobiology of Aging*, 27(12), 1733-1746.
- **Voytek et al. 2015**: Age-related changes in 1/f neural electrophysiological noise. *Journal of Neuroscience*, 35(38), 13257-13265.
- **Mathersul et al. 2008**: Anxious on the inside: Electrophysiological markers of anxiety. *International Journal of Psychophysiology*, 68(3), 223-231.
