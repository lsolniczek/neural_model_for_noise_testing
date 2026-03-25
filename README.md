# Noice Generator

A real-time DSP audio engine written in Rust that generates **intelligent noise** optimised for deep work and focus. Exposed to iOS via [Mozilla UniFFI](https://mozilla.github.io/uniffi-rs/).

Unlike static noise, the engine continuously modulates the spectral content, stereo image, and amplitude using slow LFOs — reducing listener fatigue while preserving the masking properties of broadband noise.

**Seven noise colors** are available, selectable at runtime with a smooth equal-power crossfade:

| Color | Spectrum / character | Algorithm |
|-------|---------------------|-----------|
| `White` | Flat — 0 dB/octave | Raw Wyrand PRNG (default) |
| `Pink` | −3 dB/octave (1/f) | Paul Kellet 7-stage IIR cascade |
| `Brown` | −6 dB/octave (1/f²) | Leaky integrator + DC blocker |
| `Green` | Band-pass ~500 Hz | 2nd-order BPF, Q = 0.5 |
| `Grey` | Inverted equal-loudness (ISO 226) | Low-shelf + high-shelf boost |
| `Black` | −12 dB/octave | Two cascaded 2nd-order Butterworth LPFs |
| `Ssn` | Speech-shaped noise (LTASS) | HPF@125 + Peak@400 + LPF@1k + LPF@3.5k |

Two spatial modes are available:

| Mode | Level | Output | Description |
|------|-------|--------|-------------|
| `Stereo` | 0 | Interleaved stereo | Two decorrelated sources with micro-panning LFO (default) |
| `Immersive` | 2 | Interleaved stereo | N configurable virtual sources around the listener — enveloping noise field |

An optional **Early Reflections** module simulates first-order room reflections using a 6-tap tapped delay line, providing psychoacoustic cues to externalise the sound. An optional **Late Reverb** module provides a reverberant tail. Both can be configured independently or applied as a matched pair via **Acoustic Environment** presets.

---

## Signal Chains

The **color filter** stage appears immediately after every PRNG and before all notch filters and spatial processing. Each independent noise source has its own dedicated `ColorFilterState` holding **two complete filter banks** — one fed from the dense PRNG and one unused (reserved for future Velvet support). Both banks are ticked on every sample, keeping all filter states warm so that crossfades between any pair of colors are transient-free.

### Level 0 — Stereo (default)

```
PRNG-L ──► ColorFilter-L ──► Notch-L-1 ──► Notch-L-2 ──┐
                                                          ├──► Micro-Pan ──► Crossover ──► Bass Modulator
PRNG-R ──► ColorFilter-R ──► Notch-R-1 ──► Notch-R-2 ──┘              └──► Satellite Modulator
                                                                                │
                                                                    ┌──────────────────────────┐
                                                                    │  + Anchor L/R (× 0.707)  │
                                                                    │  + Early Reflections      │
                                                                    │  + Late Reverb            │
                                                                    │  + Master Gain ──► tanh   │
                                                                    └──────────────────────────┘
                                                                                │
                                                                              L / R

LFO-notch-1  0.013 Hz  ──►  notch centre 200 – 6 000 Hz
LFO-notch-2  0.023 Hz  ──►  notch centre 500 – 7 500 Hz
LFO-pan      0.070 Hz  ──►  stereo image ±3 %

Anchor (always running):
  PRNG-L/R ──► ColorFilter ──► ITD/ILD ──► Pinna ──► Torso ──► volume_linear
```

### Level 2 — Immersive

```
For each source i in [0, N):
  PRNG-i ──► ColorFilter-i ──► Notch-i-1 ──► Notch-i-2 ──►
                                                     │
                                                     ▼
                                         ITD Delay L/R ──► HeadShadow L/R ──► Pinna L/R ──► Torso L/R
                                                                                                 │
                                                     ┌──────────────────────────────────────────┘
                                                     ▼
                                               accumulate L/R
                                          + Crossover → Bass Modulator-i / Satellite Modulator-i

sum-L × (0.95/√N) ──► + Anchor (× 0.707) ──► [Early Reflections] ──► [Late Reverb] ──► Master Gain ──► tanh ──► L
sum-R × (0.95/√N) ──► + Anchor (× 0.707) ──► [Early Reflections] ──► [Late Reverb] ──► Master Gain ──► tanh ──► R

Each source has:
  - Independent PRNG and ColorFilter state
  - Per-source ITD/ILD/Pinna/Torso processing (full binaural cues)
  - Per-source Bass and Satellite band modulators

Sources start evenly distributed at azimuth = 2π·i/N
```

### Early Reflections — 6-Tap Tapped Delay Line

```
dry L/R ──► circular delay buffer ──┬──► Tap 0 (11 ms, near left wall)  ──► LPF 12 kHz ──► gain 0.50 ──► pan 0.80/0.20 ──┐
                                    ├──► Tap 1 (17 ms, near right wall) ──► LPF 10 kHz ──► gain 0.45 ──► pan 0.20/0.80 ──┤
                                    ├──► Tap 2 (29 ms, floor)           ──► LPF  7 kHz ──► gain 0.35 ──► pan 0.50/0.50 ──┤ wet L/R
                                    ├──► Tap 3 (37 ms, ceiling)         ──► LPF  6 kHz ──► gain 0.30 ──► pan 0.60/0.40 ──┤
                                    ├──► Tap 4 (53 ms, far wall left)   ──► LPF  4 kHz ──► gain 0.20 ──► pan 0.70/0.30 ──┤
                                    └──► Tap 5 (71 ms, far wall right)  ──► LPF  3 kHz ──► gain 0.15 ──► pan 0.30/0.70 ──┘

out = dry × (1 − mix) + wet × mix

room_size [0.5, 2.0] scales all tap delays at render time — no reallocation.
Buffers pre-allocated for room_size 2.0 × 71 ms = 142 ms → 8 192 samples at 48 kHz.
Tap delays are prime numbers to prevent comb-filter ringing.
```

### Spatial Anchor — Dual-Band Spatial Masking Layer

A stereo foundation layer using two fixed virtual speakers at ±90° that prevents "in-the-head" localisation. The anchor runs independently of the main satellite noise, with its own PRNGs, color filter states, and frozen HRTF processing.

| Property | Value | Purpose |
|----------|-------|---------|
| Left speaker azimuth | −π/2 (full left) | Locked position — no LFO movement |
| Right speaker azimuth | +π/2 (full right) | Locked position — no LFO movement |
| HRTF updates | None | All ITD/ILD/pinna parameters pre-computed at construction |
| Mix coefficient | 0.707 (−3 dB) | Prevents intermodulation before tanh limiter |

**Signal path (per speaker):**

```
PRNG-L/R ──► ColorFilter ──► Notch-1 ──► Notch-2 ──► ITD Delay ──► HeadShadow ──► Pinna ──► TorsoBounce
                                                                                                     │
                                                                              L/R speaker → (L_out, R_out)

Final mix: (satellite_L + anchor_L) × 0.707 → [ER] → [LR] → tanh → output_L
           (satellite_R + anchor_R) × 0.707 → [ER] → [LR] → tanh → output_R
```

---

## Project Structure

This is a Cargo workspace. The DSP engine lives in `core` and is shared by the iOS and WASM crates.

```
noice_generator/
├── Cargo.toml                        — workspace manifest
├── build_xcframework.sh              — builds NoiceGenerator.xcframework for iOS
└── crates/
    ├── core/                         — pure DSP engine + all tests
    │   ├── Cargo.toml                — name: noice_generator_core
    │   └── src/lib.rs                — all DSP logic, UniFFI annotations (optional feature)
    ├── ios/                          — iOS bindings (UniFFI + C-FFI)
    │   ├── Cargo.toml                — name: noice_generator (staticlib + cdylib)
    │   └── src/
    │       ├── lib.rs                — re-exports core, adds #[no_mangle] C-FFI entry points
    │       └── bin/uniffi-bindgen.rs — Swift binding generator entry point
    └── wasm/                         — WebAssembly bindings (wasm-bindgen)
        ├── Cargo.toml                — name: noice_generator_wasm (cdylib)
        └── src/lib.rs                — re-exports core, wasm-bindgen exports go here
```

### Crate responsibilities

| Crate | Purpose | UniFFI | C-FFI | WASM |
|-------|---------|--------|-------|------|
| `noice_generator_core` | All DSP logic, all tests | optional feature | — | — |
| `noice_generator` (ios) | iOS XCFramework target | ✓ enabled | ✓ | — |
| `noice_generator_wasm` | WebAssembly target | — | — | ✓ |

---

## Building

### Prerequisites

| Tool | Purpose |
|------|---------|
| Rust (stable, ≥ 1.75) | `rustup.rs` |
| Xcode (≥ 15) | iOS cross-compilation + XCFramework packaging |
| iOS Rust targets | `rustup target add aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios` |
| WASM target (optional) | `rustup target add wasm32-unknown-unknown` |
| wasm-pack (optional) | `cargo install wasm-pack` — for JS glue generation |

### Development build & tests

```bash
# Build + test the DSP core (no FFI overhead)
cargo build -p noice_generator_core
cargo test  -p noice_generator_core

# Build + test the iOS bindings
cargo build -p noice_generator
cargo test  -p noice_generator

# Run all tests across every crate
cargo test --workspace
```

The test suite covers:

**Core correctness**

| Test | What it checks |
|------|----------------|
| `output_length_is_correct` | `render_audio(512)` returns exactly 1 024 elements |
| `all_samples_within_unit_range` | Every sample is finite and strictly in `(-1.0, 1.0)` |
| `left_and_right_channels_are_decorrelated` | Pearson correlation between L and R channels < 0.15 |
| `set_master_gain_clamps_correctly` | Values outside `[0, 1]` are clamped silently |
| `silence_at_zero_gain` | `master_gain = 0` produces all-zero output |
| `successive_render_calls_are_stateful` | PRNG + LFO state advances between calls |
| `long_run_no_clipping` | 60 s of audio — no sample exceeds `±1.0` |

**Spatial modes**

| Test | What it checks |
|------|----------------|
| `default_mode_is_stereo` | `NoiseEngine::new` defaults to `SpatialMode::Stereo` |
| `new_with_spatial_stores_mode` | `new_with_spatial` stores the requested mode |
| `source_count_clamping` | Values outside `[2, 8]` are clamped silently for Immersive |
| `immersive_output_length` | Immersive (2/4/6/8 sources): correct output length |
| `immersive_samples_in_range` | Immersive: no clipping with summed sources |
| `immersive_decorrelation` | Immersive: L/R channels remain decorrelated |
| `immersive_zero_gain_is_silence` | Immersive: zero gain → all zeros |
| `mode_switch_no_crash_or_nan` | Switching modes mid-stream: no panic/NaN |
| `immersive_long_run_no_clipping` | 60 s of Immersive audio — no clipping |

**Early Reflections**

| Test | What it checks |
|------|----------------|
| `early_reflections_disabled_by_default` | ER is off after construction |
| `early_reflections_parameter_defaults` | `room_size = 1.0`, `mix = 0.3` |
| `early_reflections_room_size_clamping` | Values outside `[0.5, 2.0]` are clamped silently |
| `early_reflections_mix_clamping` | Values outside `[0.0, 1.0]` are clamped silently |
| `early_reflections_enabled_samples_in_range` | Both spatial modes with ER on: no clipping, no NaN |
| `early_reflections_zero_mix_equals_disabled` | `mix = 0.0` produces bit-identical output to ER disabled |
| `early_reflections_long_run_no_clipping` | 60 s of Stereo + ER at `mix = 0.5` — no clipping |
| `early_reflections_room_size_affects_output` | `room_size` min vs max produces different output |
| `early_reflections_pan_distribution_correct` | Each tap pans correctly to its expected stereo position |
| `early_reflections_pan_pairs_sum_to_one` | L + R pan coefficients always sum to 1.0 |
| `early_reflections_total_energy_preserved` | Total energy after ER is within ±3 dB of dry signal |

**Noise colors**

| Test | What it checks |
|------|----------------|
| `default_noise_color_is_white` | Engine starts in White mode |
| `white_noise_unchanged_by_color_filter` | Explicit `.white` produces bit-identical output to default |
| `pink_noise_output_in_range` | Pink output (after crossfade settles) stays inside `(-1, 1)` |
| `brown_noise_output_in_range` | Brown output (after crossfade settles) stays inside `(-1, 1)` |
| `new_color_output_in_range` | Green, Grey, Black, SSN output after crossfade stays in `(-1, 1)` |
| `pink_noise_long_run_no_clipping` | 60 s of Pink audio — no clipping |
| `brown_noise_long_run_no_clipping` | 60 s of Brown audio — no clipping |
| `green_noise_long_run_no_clipping` | 60 s of Green audio — no clipping |
| `grey_noise_long_run_no_clipping` | 60 s of Grey audio — no clipping |
| `black_noise_long_run_no_clipping` | 60 s of Black audio — no clipping |
| `ssn_noise_long_run_no_clipping` | 60 s of SSN audio — no clipping |
| `all_modes_all_colors_no_nan_or_clip` | All spatial modes × all 7 colors: finite, in `(-1, 1)` |
| `crossfade_transition_completes` | After fade duration elapses, output is stable and finite |
| `crossfade_no_amplitude_dip` | Mid-fade RMS stays within ±6 dB of settled endpoints |
| `fade_duration_setter_clamping` | Values outside `[0.1, 5.0]` are clamped silently |
| `no_color_is_nan_or_infinite_immediately` | Every color produces valid audio on the first render call |
| `color_enum_round_trip` | Every `NoiseColor` variant survives a `to_u8 → from_u8` round-trip |
| `immersive_per_source_color_independence` | Immersive L/R decorrelation preserved per-source |

**Modulators**

| Test | What it checks |
|------|----------------|
| `test_modulator_lfo_bounds` | SineLfo output is always in `[0, 1]` |
| `test_modulator_breathing_recursive` | Breathing envelope stays in `[min_gain, 1.0]` |
| `test_modulator_stochastic_floor` | Stochastic output never drops below min_gain |
| `test_modulator_stochastic_spike_and_decay` | Spike + decay shape is correct |
| `test_stereo_crossover_integration` | Crossover splits correctly below/above 250 Hz |
| `test_stereo_crossover_modulation_affects_output` | Bass modulator changes output energy |
| `test_stereo_crossover_bass_is_mono` | Bass band is mono-summed before modulation |
| `test_immersive_modulator_integration` | Per-object modulators work in Immersive mode |
| `test_immersive_modulator_long_run_no_clipping` | 60 s Immersive + modulators — no clipping |
| `test_modulator_kind_round_trip` | `ModulatorKind` survives `to_u8 → from_u8` round-trip |
| `test_modulator_cross_kind_no_click` | Switching modulator kind mid-stream: no NaN |

### iOS XCFramework

```bash
chmod +x build_xcframework.sh
./build_xcframework.sh
```

The script compiles three targets (`aarch64-apple-ios`, `aarch64-apple-ios-sim`, `x86_64-apple-ios`), merges the simulator slices with `lipo`, generates Swift bindings, and packages everything:

Or build a single target manually:

```bash
# Physical device
cargo build --release -p noice_generator --target aarch64-apple-ios

# Generate Swift bindings from the built library
cargo run --release -p noice_generator --bin uniffi-bindgen generate \
    --library target/aarch64-apple-ios/release/libnoice_generator.a \
    --language swift \
    --out-dir ./bindings/swift
```

```
build/
├── NoiceGenerator.xcframework/     — drag into Xcode project
│   ├── ios-arm64/
│   │   ├── libnoice_generator.a
│   │   └── Headers/
│   │       ├── module.modulemap
│   │       └── noice_generatorFFI.h
│   └── ios-arm64_x86_64-simulator/
│       ├── libnoice_generator.a
│       └── Headers/
│           ├── module.modulemap
│           └── noice_generatorFFI.h
└── swift/
    └── noice_generator.swift       — add to app target sources
```

---

## Xcode Integration

**Step 1 — Link the XCFramework**

Drag `build/NoiceGenerator.xcframework` into the Xcode Project Navigator, then:

- Target → General → **Frameworks, Libraries, and Embedded Content**
- Set the embed option to **Do Not Embed** (it is a static library)

**Step 2 — Add the Swift wrapper**

Drag `build/swift/noice_generator.swift` into the Project Navigator and add it to your **app target**.

No bridging header is needed.

---

## WebAssembly

Build the WASM module:

```bash
# Bare .wasm file (no JS glue)
cargo build --release -p noice_generator_wasm --target wasm32-unknown-unknown
# Output: target/wasm32-unknown-unknown/release/noice_generator_wasm.wasm
```

With JS glue via [wasm-pack](https://rustwasm.github.io/wasm-pack/):

```bash
wasm-pack build crates/wasm --target web      # ES module for browsers
wasm-pack build crates/wasm --target nodejs   # CommonJS for Node.js
wasm-pack build crates/wasm --target bundler  # webpack / rollup
# Output: crates/wasm/pkg/
```

> `#[wasm_bindgen]` exports are added in `crates/wasm/src/lib.rs`. The core DSP compiles to WASM unchanged — UniFFI is not enabled for this target.

---

## Using as a dependency

### Local path

```toml
[dependencies]
noice_generator_core = { path = "../noice_generator/crates/core" }
```

### Git

```toml
[dependencies]
noice_generator_core = {
    git     = "https://github.com/you/noice_generator",
    package = "noice_generator_core",
    tag     = "v0.1.0"   # or: branch = "main", rev = "abc1234"
}
```

The `package` key is required because the crate name (`noice_generator_core`) differs from the repository name.

### Workspace sibling

Add the new crate to the workspace root `Cargo.toml`:

```toml
[workspace]
members = ["crates/core", "crates/ios", "crates/wasm", "crates/my_project"]
```

Then in `crates/my_project/Cargo.toml`:

```toml
[dependencies]
noice_generator_core = { path = "../core" }
```

### Feature flags

By default `noice_generator_core` compiles with UniFFI annotations enabled. Disable them (e.g. for WASM or plain Rust consumers) with:

```toml
noice_generator_core = { path = "../core", default-features = false }
```

---

## API Reference

The library exposes one class (`NoiseEngine`) and five enums (`SpatialMode`, `NoiseColor`, `ModulatorKind`, `RoomPreset`, `AcousticEnvironment`).

### `SpatialMode` enum

```swift
enum SpatialMode {
    case stereo      // Level 0: two decorrelated sources with micro-panning (default)
    case immersive   // Level 2: N virtual sources around the listener [2, 8]
}
```

---

### `NoiseColor` enum

```swift
enum NoiseColor {
    case white  // Flat spectrum — 0 dB/octave. Raw PRNG. Default.
    case pink   // −3 dB/octave (1/f). Paul Kellet 7-stage IIR cascade.
    case brown  // −6 dB/octave (1/f²). Leaky integrator + DC blocker.
    case green  // Band-pass ~500 Hz, Q = 0.5. Environmental hiss character.
    case grey   // Inverted equal-loudness (ISO 226). Low+high shelf boost.
    case black  // −12 dB/octave. Two cascaded 2nd-order Butterworth LPFs.
    case ssn    // Speech-shaped noise (LTASS). Multi-stage EQ filter.
}
```

Color transitions use an **equal-power crossfade** — no clicks or amplitude dips when switching. The crossfade duration is configurable (default 1.5 s, range `[0.1, 5.0]` s).

---

### `ModulatorKind` enum

Selects the amplitude modulation algorithm applied to the bass and satellite frequency bands.

```swift
enum ModulatorKind {
    case flat        // Constant gain of 1.0 — no modulation (default)
    case sineLfo     // Ultra-slow sine LFO for vestibular entrainment (0.01–2.0 Hz)
    case breathing   // Breathing envelope state machine (4-7-8, Box, Coherence, WimHof)
    case stochastic  // Poisson spike train with exponential decay
}
```

**Parameter meanings by `kind`:**

| `kind` | `param_a` | `param_b` | `param_c` |
|--------|-----------|-----------|-----------|
| `flat` | — | — | — |
| `sineLfo` | `freq_hz` (0.01–2.0) | `depth` (0.0–1.0) | — |
| `breathing` | `pattern_id` (0=4-7-8, 1=Box, 2=Coherence, 3=WimHof) | `min_gain` | — |
| `stochastic` | `lambda` (spikes/s) | `decay_ms` | `min_gain` (0.05–0.5) |

---

### `RoomPreset` enum

Preset room types for late reverb. Each preset defines RT60 decay time, damping frequency, and wet mix.

```swift
enum RoomPreset {
    case intimate        // Small room: RT60 = 0.3 s, damping = 3500 Hz, mix = 0.08
    case studio          // Treated studio: RT60 = 0.5 s, damping = 6000 Hz, mix = 0.15
    case livingRoom      // Domestic living room: RT60 = 0.8 s, damping = 5000 Hz, mix = 0.25
    case hall            // Concert hall: RT60 = 1.8 s, damping = 4000 Hz, mix = 0.45
    case cathedral       // Large cathedral: RT60 = 3.5 s, damping = 2500 Hz, mix = 0.65
    case isolationBooth  // Extreme focus: RT60 = 0.1 s, damping = 8000 Hz, mix = 0.03
    case sanctuary       // Deep relaxation: RT60 = 2.5 s, damping = 1500 Hz, mix = 0.55
}
```

Use via `setLateReverbPreset(preset:)` to apply all three parameters (decay, damping, mix) in one call.

---

### `AcousticEnvironment` enum

High-level acoustic environment presets that bundle Early Reflections and Late Reverb into physically consistent configurations. Eliminates "acoustic paradoxes" (e.g., tiny room size with cathedral decay).

```swift
enum AcousticEnvironment {
    case anechoicChamber  // Near-anechoic space. Minimal reflections, no reverb tail.
    case focusRoom        // Tight, heavily damped booth. Barely perceptible room cues.
    case openLounge       // Medium domestic room with balanced early reflections.
    case vastSpace        // Large open hall with prominent spatial depth.
    case deepSanctuary    // Dark, enveloping reverberant space with suppressed highs.
}
```

| Environment | ER size | ER mix | LR preset | LR mix |
|-------------|---------|--------|-----------|--------|
| `anechoicChamber` | 0.50 | 0.02 | IsolationBooth | 0.01 |
| `focusRoom` | 0.72 | 0.15 | Intimate | 0.06 |
| `openLounge` | 1.00 | 0.30 | LivingRoom | 0.25 |
| `vastSpace` | 1.31 | 0.40 | Hall | 0.45 |
| `deepSanctuary` | 1.46 | 0.35 | Sanctuary | 0.55 |

Use via `setAcousticEnvironment(environment:)` to apply all six parameters in one call.

---

### Constructors

#### `NoiseEngine(sampleRate:masterGain:)`

```swift
NoiseEngine(sampleRate: UInt32, masterGain: Float)
```

Creates a `NoiseEngine` in **Stereo** mode.

| Parameter | Type | Description |
|-----------|------|-------------|
| `sampleRate` | `UInt32` | Hardware sample rate in Hz. Typically `48_000`. |
| `masterGain` | `Float` | Initial output gain. Clamped to `[0.0, 1.0]`. |

---

#### `NoiseEngine(sampleRate:masterGain:spatialMode:sourceCount:)`

```swift
NoiseEngine(sampleRate: UInt32, masterGain: Float, spatialMode: SpatialMode, sourceCount: UInt32)
```

Creates a `NoiseEngine` with an explicit spatial mode.

| Parameter | Type | Description |
|-----------|------|-------------|
| `sampleRate` | `UInt32` | Hardware sample rate in Hz. Typically `48_000`. |
| `masterGain` | `Float` | Initial output gain. Clamped to `[0.0, 1.0]`. |
| `spatialMode` | `SpatialMode` | The spatial rendering mode. |
| `sourceCount` | `UInt32` | Number of virtual sources. For `.immersive`: clamped to `[2, 8]`. Ignored for `.stereo`. |

**Examples:**

```swift
// Level 0 — classic stereo (default)
let engine = NoiseEngine(sampleRate: 48_000, masterGain: 0.8)

// Level 2 — 6 virtual sources around the listener
let engine = NoiseEngine(sampleRate: 48_000, masterGain: 0.8,
                         spatialMode: .immersive, sourceCount: 6)
```

---

### Master Gain

#### `setMasterGain(gain:)`

```swift
func setMasterGain(gain: Float)
```

Updates the master output gain. Silently clamped to `[0.0, 1.0]`.

**Lock-free:** stores the new value in an `AtomicU32`. The audio thread picks it up on the next render call with no blocking.

---

#### `masterGain() -> Float`

```swift
func masterGain() -> Float
```

Returns the current master gain. Lock-free.

---

### Spatial Mode

#### `setSpatialMode(mode:sourceCount:)`

```swift
func setSpatialMode(mode: SpatialMode, sourceCount: UInt32)
```

Switches the spatial mode at runtime.

**Lock-free:** stores the new mode and source count in atomics. The audio thread applies the change at the start of the next render call.

| Parameter | Type | Description |
|-----------|------|-------------|
| `mode` | `SpatialMode` | The new spatial mode. |
| `sourceCount` | `UInt32` | For `.immersive`: clamped to `[2, 8]`. Ignored for `.stereo`. |

---

#### `spatialMode() -> SpatialMode`

```swift
func spatialMode() -> SpatialMode
```

Returns the current spatial mode. Lock-free.

---

#### `sourceCount() -> UInt32`

```swift
func sourceCount() -> UInt32
```

Returns the effective source count: the configured value for `.immersive`; `1` for `.stereo`. Lock-free.

---

### Early Reflections API

#### `setEarlyReflectionsEnabled(enabled:)`

```swift
func setEarlyReflectionsEnabled(enabled: Bool)
```

Enables or disables the early-reflections room simulator. Disabled by default.

**Lock-free.** When disabled the processing is entirely bypassed (a single atomic load per render call). The delay buffers are not cleared, so re-enabling produces a seamless transition.

---

#### `earlyReflectionsEnabled() -> Bool`

```swift
func earlyReflectionsEnabled() -> Bool
```

Returns the current enabled state. Lock-free.

---

#### `setRoomSize(size:)`

```swift
func setRoomSize(size: Float)
```

Sets the room-size multiplier for early reflections, clamped to `[0.5, 2.0]`. Default `1.0`.

**Lock-free.** Changing `room_size` never allocates — all buffers are pre-sized at construction.

| Value | Effect |
|-------|--------|
| `0.5` | Small room — tight reflections, intimate sound |
| `1.0` | Reference room (default) |
| `2.0` | Large room — wide delay times, spacious sound |

---

#### `roomSize() -> Float`

```swift
func roomSize() -> Float
```

Returns the current room-size multiplier. Lock-free.

---

#### `setReflectionsMix(mix:)`

```swift
func setReflectionsMix(mix: Float)
```

Sets the dry/wet blend for the early-reflections module, clamped to `[0.0, 1.0]`. Default `0.3`.

**Lock-free.**

| Value | Effect |
|-------|--------|
| `0.0` | Fully dry — use `setEarlyReflectionsEnabled(false)` to skip calculation entirely |
| `0.3` | Default — subtle blend adding room depth |
| `1.0` | Fully wet — only the reflected signal is heard |

---

#### `reflectionsMix() -> Float`

```swift
func reflectionsMix() -> Float
```

Returns the current dry/wet mix. Lock-free.

---

### Late Reverb API

A reverberant tail simulating the accumulated late reflections in a space.

#### `setLateReverbEnabled(enabled:)`

```swift
func setLateReverbEnabled(enabled: Bool)
```

Enables or disables the late reverb. Disabled by default. **Lock-free.**

---

#### `lateReverbEnabled() -> Bool`

```swift
func lateReverbEnabled() -> Bool
```

Returns whether late reverb is enabled. Lock-free.

---

#### `setLateReverbDecay(decayS:)`

```swift
func setLateReverbDecay(decayS: Float)
```

Sets the RT60 decay time in seconds. Clamped to `[0.2, 4.0]`. **Lock-free.**

---

#### `lateReverbDecay() -> Float`

```swift
func lateReverbDecay() -> Float
```

Returns the current decay time in seconds. Lock-free.

---

#### `setLateReverbDamping(dampingHz:)`

```swift
func setLateReverbDamping(dampingHz: Float)
```

Sets the damping LPF cutoff in Hz. Clamped to `[1000, 16000]`. Lower values produce a darker, more absorbed reverb tail. **Lock-free.**

---

#### `lateReverbDamping() -> Float`

```swift
func lateReverbDamping() -> Float
```

Returns the current damping frequency. Lock-free.

---

#### `setLateReverbMix(mix:)`

```swift
func setLateReverbMix(mix: Float)
```

Sets the wet/dry mix for late reverb. Clamped to `[0.0, 1.0]`. **Lock-free.**

---

#### `lateReverbMix() -> Float`

```swift
func lateReverbMix() -> Float
```

Returns the current late reverb mix. Lock-free.

---

#### `setLateReverbPreset(preset:)`

```swift
func setLateReverbPreset(preset: RoomPreset)
```

Applies a `RoomPreset` — sets decay, damping, and mix in one call. **Lock-free.**

```swift
engine.setLateReverbPreset(preset: .hall)       // RT60 = 1.8 s, damping = 4000 Hz, mix = 0.45
engine.setLateReverbPreset(preset: .cathedral)  // RT60 = 3.5 s, damping = 2500 Hz, mix = 0.65
engine.setLateReverbPreset(preset: .intimate)   // RT60 = 0.3 s, damping = 3500 Hz, mix = 0.08
```

---

### Acoustic Environment API

#### `setAcousticEnvironment(environment:)`

```swift
func setAcousticEnvironment(environment: AcousticEnvironment)
```

Applies a complete acoustic environment — sets Early Reflections (enabled, room size, mix) and Late Reverb (enabled, preset, mix) in a single call. All parameters are atomic and safe to call at any time.

```swift
// Focus session: tight, controlled acoustic
engine.setAcousticEnvironment(environment: .focusRoom)

// Spacious ambiance: large hall feel
engine.setAcousticEnvironment(environment: .vastSpace)

// Deep, enveloping reverb for relaxation
engine.setAcousticEnvironment(environment: .deepSanctuary)

// Near-silence, no room coloration
engine.setAcousticEnvironment(environment: .anechoicChamber)
```

---

### Noise Color API

#### `setTargetNoiseColor(color:)` / `setNoiseColor(color:)`

```swift
func setTargetNoiseColor(color: NoiseColor)
func setNoiseColor(color: NoiseColor)  // alias
```

Sets the target noise color. The audio thread starts an **equal-power crossfade** from the currently active color over the configured fade duration.

**Lock-free:** stores the new value in an `AtomicU8`. Safe to call from any thread at any time.

| Color | Spectrum / character | Typical use |
|-------|---------------------|-------------|
| `.white` | Flat, 0 dB/oct | Masking, focus, general use |
| `.pink` | −3 dB/oct (1/f) | Warmer, gentler high-frequency content |
| `.brown` | −6 dB/oct (1/f²) | Deep, low-dominant rumble; sleep and relaxation |
| `.green` | Band-pass ~500 Hz | Natural environmental ambiance, mid-range presence |
| `.grey` | Inverted equal-loudness | Perceptually flat; audiophile masking |
| `.black` | −12 dB/oct | Extremely deep bass-dominant; immersive sub-bass texture |
| `.ssn` | Speech-shaped (LTASS) | Cocktail-party masking, speech privacy, audiology testing |

---

#### `targetNoiseColor() -> NoiseColor`

```swift
func targetNoiseColor() -> NoiseColor
```

Returns the target noise color as last set. The audio thread may still be mid-crossfade. Lock-free.

---

#### `setNoiseColorFadeDuration(seconds:)`

```swift
func setNoiseColorFadeDuration(seconds: Float)
```

Sets the crossfade duration for noise color transitions, in seconds. Clamped to `[0.1, 5.0]`. Default `1.5`.

**Lock-free.** Changes take effect at the start of the **next** transition.

| Value | Effect |
|-------|--------|
| `0.1` | Near-instant switch |
| `1.5` | Default — smooth, imperceptible transition |
| `5.0` | Slow cinematic blend |

---

#### `noiseColorFadeDuration() -> Float`

```swift
func noiseColorFadeDuration() -> Float
```

Returns the configured crossfade duration in seconds. Lock-free.

---

### Anchor API

#### `setAnchorColor(color:)`

```swift
func setAnchorColor(color: NoiseColor)
```

Sets the noise color for the Spatial Anchor layer. Independent of the satellite — any combination of colors is valid.

**Lock-free:** stores the new value in an `AtomicU8`. The audio thread starts an equal-power crossfade from the anchor's current color.

---

#### `anchorColor() -> NoiseColor`

```swift
func anchorColor() -> NoiseColor
```

Returns the anchor's current target color. Lock-free.

---

#### `setAnchorVolume(volume:)`

```swift
func setAnchorVolume(volume: Float)
```

Sets the anchor output volume, clamped to `[0.0, 1.0]`. Default `0.0` (silent).

**Lock-free.** Filter states continue running even at volume 0, so fade-in is instant and transient-free.

| Value | Effect |
|-------|--------|
| `0.0` | Anchor silent (default) |
| `1.0` | Anchor at full level — combined with satellite at −3 dB |

---

#### `anchorVolume() -> Float`

```swift
func anchorVolume() -> Float
```

Returns the anchor volume. Lock-free.

---

### Object-Based Audio API

Configure up to **8 spatial objects** (`MAX_SOURCES = 8`). All writes go through a **lock-free atomic bridge** — the audio thread reads from it once per render block with no mutex and no priority inversion.

Available in both `Stereo` and `Immersive` modes. In `Stereo` mode, object 0 drives the global crossover modulation.

#### `setObject(index:active:color:x:y:z:volume:reverbSend:)`

```swift
func setObject(
    index: UInt32,
    active: Bool,
    color: NoiseColor,
    x: Float,
    y: Float,
    z: Float,
    volume: Float,
    reverbSend: Float
)
```

Sets all parameters for a single spatial object in one call. **Lock-free.**

| Parameter | Type | Description |
|-----------|------|-------------|
| `index` | `UInt32` | Object slot (0–7). Out-of-range is silently ignored. |
| `active` | `Bool` | Whether this object renders audio. |
| `color` | `NoiseColor` | Noise color for this object's independent generator. |
| `x` | `Float` | Horizontal position in metres (negative = left). |
| `y` | `Float` | Vertical position in metres. |
| `z` | `Float` | Depth position in metres (negative = front). |
| `volume` | `Float` | Per-object gain, clamped to `[0.0, 1.0]`. Default `1.0`. |
| `reverbSend` | `Float` | Amount sent to room bus, clamped to `[0.0, 1.0]`. Default `0.1`. |

---

#### `setObjectPosition(index:x:y:z:)`

```swift
func setObjectPosition(index: UInt32, x: Float, y: Float, z: Float)
```

Updates only the 3D position of an existing spatial object. Distance is derived as `sqrt(x²+y²+z²)`, clamped to a minimum of 0.2 m. **Lock-free.**

---

#### `setObjectActive(index:active:)`

```swift
func setObjectActive(index: UInt32, active: Bool)
```

Activates or deactivates an object. **Deactivation resets all parameters to defaults** (White color, position (0,0,1), volume 1.0, reverb_send 0.1, Flat modulators). **Lock-free.**

---

#### `setObjectColor(index:color:)`

```swift
func setObjectColor(index: UInt32, color: NoiseColor)
```

Changes the noise color of a single spatial object. Each object has its own independent `ColorFilterState`. **Lock-free.**

---

#### `setObjectVolume(index:volume:)`

```swift
func setObjectVolume(index: UInt32, volume: Float)
```

Sets the per-object volume, clamped to `[0.0, 1.0]`. **Lock-free.**

---

#### `setObjectReverbSend(index:send:)`

```swift
func setObjectReverbSend(index: UInt32, send: Float)
```

Sets the per-object reverb send amount, clamped to `[0.0, 1.0]`. **Lock-free.**

---

#### Example — object-based scene

```swift
let engine = NoiseEngine(sampleRate: 48_000, masterGain: 0.8,
                         spatialMode: .immersive, sourceCount: 4)

// Object 0: overhead ambient — brown noise, no reverb
engine.setObject(index: 0, active: true, color: .brown,
                 x: 0.0, y: 1.5, z: 0.0, volume: 0.8, reverbSend: 0.0)

// Object 1: front-left source — pink noise, moderate reverb
engine.setObject(index: 1, active: true, color: .pink,
                 x: -1.2, y: 0.0, z: -2.0, volume: 1.0, reverbSend: 0.15)

// Object 2: front-right source — white noise, moderate reverb
engine.setObject(index: 2, active: true, color: .white,
                 x: 1.2, y: 0.0, z: -2.0, volume: 1.0, reverbSend: 0.15)

// Object 3: rear ambient — green noise, high reverb
engine.setObject(index: 3, active: true, color: .green,
                 x: 0.0, y: 0.0, z: 2.5, volume: 0.7, reverbSend: 0.4)

// Animate object 1's position at runtime (lock-free from UI thread):
engine.setObjectPosition(index: 1, x: -0.8, y: 0.2, z: -1.8)
```

---

### Modulator API

Per-object amplitude modulation on bass (< 250 Hz) and satellite (≥ 250 Hz) frequency bands.

#### `setStereoBassMod(index:kind:paramA:paramB:paramC:)`

```swift
func setStereoBassMod(index: UInt32, kind: ModulatorKind,
                      paramA: Float, paramB: Float, paramC: Float)
```

Sets the bass-band modulator for a noise object. Works in both `Stereo` and `Immersive` modes. In `Stereo`, object 0 drives the global bass modulation. **Lock-free.**

---

#### `setStereoBassModulator(index:kind:paramA:paramB:paramC:)`

> **Note:** In Swift/UniFFI binding the method name maps from the Rust function `set_stereo_bass_modulator`.

---

#### `setStereoSatelliteModulator(index:kind:paramA:paramB:paramC:)`

```swift
func setStereoSatelliteModulator(index: UInt32, kind: ModulatorKind,
                                 paramA: Float, paramB: Float, paramC: Float)
```

Sets the satellite-band modulator for a noise object. **Lock-free.**

---

#### Modulator examples

```swift
// Sine LFO on bass — slow vestibular entrainment at 0.05 Hz, full depth
engine.setStereoBassModulator(index: 0, kind: .sineLfo,
                              paramA: 0.05, paramB: 1.0, paramC: 0.0)

// Coherence breathing on satellite — synchronized breathing guide
engine.setStereoSatelliteModulator(index: 0, kind: .breathing,
                                   paramA: 2.0,  // pattern_id 2 = Coherence (6 bpm)
                                   paramB: 0.2,  // min_gain
                                   paramC: 0.0)

// Stochastic spikes on bass — 2 spikes/s, 150 ms decay, floor 0.1
engine.setStereoBassModulator(index: 1, kind: .stochastic,
                              paramA: 2.0, paramB: 150.0, paramC: 0.1)

// Disable modulation on all bands for object 0
engine.setStereoBassModulator(index: 0, kind: .flat, paramA: 0, paramB: 0, paramC: 0)
engine.setStereoSatelliteModulator(index: 0, kind: .flat, paramA: 0, paramB: 0, paramC: 0)
```

---

### `renderAudio(numFrames:) -> [Float]`

```swift
func renderAudio(numFrames: UInt32) -> [Float]
```

> **Not suitable for real-time audio.** This method allocates a `[Float]` on every call. Use it for unit tests, offline rendering, or prototyping **only**.
>
> For `AVAudioSourceNode` or any real-time audio callback use `noice_generator_render_into` instead — see [Real-Time Render API](#real-time-render-api) below.

Generates `numFrames` frames of noise and returns them as an **interleaved** `[Float]` of size `numFrames × 2`.

---

## Real-Time Render API

These C functions are exported by the static library alongside the UniFFI symbols. They are declared in `noice_generatorFFI.h` and are directly callable from Swift with no bridging header.

### `noice_generator_engine_ptr`

```swift
func noice_generator_engine_ptr(_ enginePtr: UnsafeRawPointer?) -> UnsafeRawPointer?
```

Returns the raw opaque pointer to the `NoiseEngine` for use with `noice_generator_render_into`. Call **once** during setup on the main/UI thread. Do not free the returned pointer — its lifetime is managed by the `NoiseEngine` object.

### `noice_generator_render_into`

```swift
func noice_generator_render_into(
    _ enginePtr: UnsafeRawPointer?,
    _ bufferPtr: UnsafeMutablePointer<Float>?,
    _ numFrames: UInt32
) -> UInt32
```

**Lock-free. Allocation-free. Call from the `AVAudioSourceNode` render block.**

Writes `numFrames` of noise directly into `bufferPtr`. No heap allocation, no mutex, no copy.

| Parameter | Description |
|-----------|-------------|
| `enginePtr` | Opaque pointer from `noice_generator_engine_ptr`. |
| `bufferPtr` | Pre-allocated interleaved `Float` buffer. Must hold at least `numFrames × 2` elements. |
| `numFrames` | Number of audio frames to render. |

**Returns** `2` (the channel count written). Returns `0` on null/invalid arguments.

**Must be called from a single thread** (the audio render thread). `AVAudioSourceNode` serialises its render block, so this constraint is automatically satisfied in normal usage.

---

### `noice_generator_set_target_noise_color`

```swift
func noice_generator_set_target_noise_color(
    _ enginePtr: UnsafeRawPointer?,
    _ color: UInt8
)
```

**Lock-free. Call from any thread.** A thin C-FFI wrapper around `setTargetNoiseColor`.

| `color` value | Noise color |
|:---:|---|
| `0` | White (flat spectrum) |
| `1` | Pink (−3 dB/oct) |
| `2` | Brown (−6 dB/oct) |
| `3` | Green (band-pass ~500 Hz) |
| `4` | Grey (inverted equal-loudness) |
| `5` | Black (−12 dB/oct) |
| `6` | SSN (speech-shaped noise) |

Unknown values are treated as White. Null `enginePtr` is silently ignored.

---

### Complete `AVAudioSourceNode` example

```swift
import AVFAudio

// ── Setup (main thread, once) ────────────────────────────────────────────────
let engine = NoiseEngine(sampleRate: 48_000, masterGain: 0.8)

// Grab the raw pointer once. Its lifetime is tied to `engine`.
let enginePtr = noice_generator_engine_ptr(engine.pointer)

let stereoFormat = AVAudioFormat(standardFormatWithSampleRate: 48_000, channels: 2)!

let sourceNode = AVAudioSourceNode(format: stereoFormat) { [enginePtr]
    _, _, frameCount, audioBufferList -> OSStatus in

    let ablPtr = UnsafeMutableAudioBufferListPointer(audioBufferList)
    guard let buf = ablPtr[0].mData else { return kAudioUnitErr_InvalidParameter }

    // Zero allocations. Zero copies. No locks.
    noice_generator_render_into(enginePtr, buf.assumingMemoryBound(to: Float.self), frameCount)
    return noErr
}

// ── Parameter updates (UI thread — atomic stores, no blocking) ───────────────
engine.setMasterGain(gain: 0.6)
engine.setSpatialMode(mode: .immersive, sourceCount: 6)

// Switch noise color — smooth equal-power crossfade over 1.5 s (default).
engine.setTargetNoiseColor(color: .pink)

// Apply acoustic environment preset.
engine.setAcousticEnvironment(environment: .openLounge)

// Or configure room manually.
engine.setEarlyReflectionsEnabled(enabled: true)
engine.setRoomSize(size: 1.2)
engine.setReflectionsMix(mix: 0.3)
engine.setLateReverbPreset(preset: .hall)

// Enable anchor for a more stable spatial image.
engine.setAnchorColor(color: .brown)
engine.setAnchorVolume(volume: 0.25)

// Or via the raw C-FFI pointer (0–6, see color value table):
noice_generator_set_target_noise_color(enginePtr, 2)   // 2 = Brown
noice_generator_set_target_noise_color(enginePtr, 1)   // 1 = Pink
```

---

## Output Buffer Format

### Length

| Mode | Length |
|------|--------|
| `Stereo` / `Immersive` | `numFrames × 2` |

A call with `numFrames = 512` writes **1 024 `Float` values**.

### Layout — Interleaved Stereo

```
Index:  0    1    2    3    4    5    6    7   ...  2n-2  2n-1
        L₀   R₀   L₁   R₁   L₂   R₂   L₃   R₃  ...  Lₙ₋₁  Rₙ₋₁
```

This layout matches the interleaved format expected by `AVAudioSourceNode`.

### Value Range

Every sample is **strictly within `(-1.0, 1.0)`** — guaranteed by the `tanh` soft clipper.

---

## DSP Signal Chain Details

### Shared Stages (All Modes)

| Stage | Algorithm | Parameters | Purpose |
|-------|-----------|------------|---------|
| **White Noise** | Wyrand PRNG (`fastrand`) | Uniform `[-1, 1]` | Dense broadband base generator |
| **Color Filter** | Dual filter bank (Standard + reserved) | 7 colors; equal-power crossfade 0.1–5 s | Spectral color shaping |
| **Notch Filter 1** | Biquad band-reject, Q = 5 | Centre: 200 – 6 000 Hz, LFO: 0.013 Hz | Slow spectral sweep |
| **Notch Filter 2** | Biquad band-reject, Q = 5 | Centre: 500 – 7 500 Hz, LFO: 0.023 Hz | Asynchronous second sweep |
| **Crossover** | 1st-order LPF + HPF at 250 Hz | Fixed crossover frequency | Splits bass / satellite bands |
| **Bass Modulator** | Per-object (Flat / SineLfo / Breathing / Stochastic) | Configurable | Bass band amplitude shaping |
| **Satellite Modulator** | Per-object (Flat / SineLfo / Breathing / Stochastic) | Configurable | Satellite band amplitude shaping |
| **Master Gain** | Scalar multiply | `[0.0, 1.0]`, user-controlled | Output volume |
| **Soft Clipping** | `tanh(x)` | Output strictly `(-1, 1)` | Safety net |

### Level 0 — Stereo Only

| Stage | Algorithm | Parameters | Purpose |
|-------|-----------|------------|---------|
| **Micro-Panning** | Linear gain law | ±3 %, LFO: 0.07 Hz | Subtle horizontal movement |

### Anchor (All Stereo Modes)

| Stage | Algorithm | Parameters | Purpose |
|-------|-----------|------------|---------|
| **Dual PRNGs** | Wyrand (left + right speaker) | Independent seeds | Two fixed virtual speakers |
| **Color Filter** | Dual bank (independent color control) | Same as satellite | Anchor can differ from satellite |
| **ITD** | Pre-computed at ±90° | Frozen — no updates | Maximum lateral delay |
| **Head Shadow** | Pre-computed at ±90° | Frozen — no updates | Maximum contralateral attenuation |
| **Pinna** | Pre-computed at ±90° | Frozen — no updates | Consistent side-localisation notch |
| **Torso** | Pre-computed at ±90° | Frozen — no updates | Shoulder bounce reflection |
| **Mix** | Linear add with 0.707 scale | −3 dB headroom | Prevent intermodulation before tanh |

### Level 2 — Immersive Only

| Stage | Algorithm | Parameters | Purpose |
|-------|-----------|------------|---------|
| **ITD Delay** | Fractional delay line | Woodworth model, max ~31 samples at 48 kHz | Inter-aural time difference |
| **Head Shadow** | 1-pole IIR LPF | Cutoff: 20 kHz (front) → 8 kHz (side) | Inter-aural level difference |
| **Pinna Notch** | Dynamic biquad | 6–10 kHz, Q = 6 | Front/back resolution (Blauert bands) |
| **Torso Reflection** | Delay + LPF | 1.2–2.5 ms, LPF @ 3.5 kHz | Shoulder bounce |
| **Source Normalisation** | `0.95 / √N` scale | N = active source count | Prevent summing louder than stereo |

### Binaural cues (Immersive)

```
ITD:   Woodworth model, max ≈ 31 samples at 48 kHz (head radius 8.75 cm)
ILD:   1-pole LPF, cutoff 20 kHz (front) → 8 kHz (side)
Pinna: Dynamic notch 6–10 kHz (front/back resolution)
Torso: 1.2–2.5 ms delay, LPF @ 3.5 kHz, −10 dB attenuation
```

### Floating-Point Safety

All IIR filter state variables are sanitized after each sample to prevent NaN/Inf/subnormal propagation. On ARM64, flush-to-zero (FPCR.FZ) is enabled per-render to prevent the ~100× slowdown of denormal processing.
