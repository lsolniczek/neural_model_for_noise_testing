# Working with the Brain Model — Practical Guide

A field guide for designing presets against the bilateral Jansen-Rit + FHN neural simulator. This document captures what's load-bearing, what's noise, and where the model's preferences diverge from what real human listeners want. Updated after 12 major model improvements including global normalization, inhibitory bilateral coupling, thalamic gating, ASSR DC/AC separation, habituation, stochastic JR, and Cortical Envelope Tracking (CET — Priority 13: slow/fast crossover, slow GABA_B in JR, envelope-phase PLV).

---

## TL;DR for the impatient

1. **Trust the score directionally, not absolutely.** The gap between a 0.45 and a 0.75 is real, but exaggerated versus real EEG.
2. **Score below ~0.05 is noise.** Differences smaller than that are model jitter, not real improvements.
3. **Noise color is now CRITICAL.** Global max normalization (replacing per-band) means the model sees genuine spectral differences. SSN is the best *neural* carrier for dual-band goals (Flow, Shield, Focus) but carries a product-level cost — its speech-range energy engages bottom-up attention and causes cognitive fatigue over 5+ minute sessions. Use SSN where the neural spec requires it, use Pink/Brown where single-band targets allow cognitively restful alternatives.
4. **Reverb is the PRIMARY arousal lever.** The thalamic gate maps reverb (and brightness, modulation rate) to arousal, which determines whether the model can access theta/delta or stays in beta.
5. **Hemispheres differentiate, not synchronize.** Inhibitory corpus callosum coupling means asymmetric placement creates hemispheric contrast, not bilateral locking.
6. **For ADHD: multiple gentle drivers > one aggressive driver.** ADHD amplifies everything — moderate modulation (depth 0.30-0.40) from 4-5 sources beats max modulation from 2 sources.
7. **Design sound-first, measure second.** Build a preset from a perceptually-good base, then measure what neural state it induces. Do NOT start from a goal and let the optimizer search blank-slate. See the Preset Design Framework below — it's the single most important workflow recommendation in this guide.

---

## Preset Design Framework — Sound-First Layered Approach

The default temptation with this model is to pick a goal, run the DE optimizer, and ship the output. **Don't.** That workflow optimizes model scores, not products. This section describes the workflow that actually produces presets humans want to listen to.

### Why sound-first beats goal-first

1. **The fitness function is a shadow of the real target.** "Focus = beta dominant" is a convenient proxy, not a definition of focus. Real focus involves top-down attention, dopaminergic tone, and inhibitory control — none of which the model sees. Optimizing hard against the proxy gives you a preset that satisfies the proxy and nothing else.
2. **DE exploits model quirks.** Differential evolution is brutal. It will place ghost objects, pair modulators at weird frequencies, or lateralize extremely if that extracts 0.02 from the score. These presets routinely *sound* worse than a naive baseline even though they score 0.80 vs 0.55. This is the default failure mode, not an edge case.
3. **Goals over-constrain the design space.** A preset measuring 0.72 Focus / 0.65 Shield / 0.58 Deep Work can be a better product than three separate 0.80 optimizer presets, because it generalizes. Goal-first optimization never finds those.
4. **Sonic quality clusters on a small number of dimensions** — warmth, depth, breath, movement, air, grain. These are what ears resolve and what listener preferences form around. Band-power targets are an orthogonal measurement you apply *after* you have a texture you'd live with, not before.
5. **Brain-type pairing becomes combinatorial.** 9 goals × 5 brain types = 45 cells. That's a research grid, not a product SKU list. Most cells don't need bespoke presets — the underlying sonic texture is often the same with small gain/reverb tweaks per brain type.

### The six layers

Build each preset as a stack, layer by layer. **Measure after each layer with `evaluate --goal all`**; do not add the next layer until you've understood what the previous one did. Most of the skill is in stopping early, not in stacking more.

#### Layer 0 — Carrier stack (branch point, not a step)

**The most important discipline in the framework, and the easiest to get wrong.** Layer 0 makes structural commitments — carrier color, source count, symmetry, volume ratios — that later layers *cannot undo*. Reverb, drivers, movement, and resilience can shape the response; they cannot make Pink behave like SSN, and they cannot make symmetric passive input produce a unified bilateral state if stochastic JR + inhibitory callosal coupling splits it apart.

**Split Layer 0 into 0a and 0b. Test the anchor alone before any satellites.**

The anchor is structurally decoupled from sources in the engine: it has its own decorrelated stereo PRNG streams, its own color filter, its own HRTFs, and it is mixed into the output *additively* after the source bus (`noise_generator_dsp/crates/core/src/lib.rs:2691–2727`). Its expected band power is bilaterally balanced by construction (two independent PRNGs with identical statistics), which makes it the **only** input to this NMM that can drive both hemispheres symmetrically *without* introducing the per-ear HRTF asymmetries that directional sources inevitably produce. For goals that want bilaterally balanced drive — passive masking, habituation-first presets, anything where spatial lateralization would hurt — the anchor is not a background filler, it is the primary channel.

**Framework rule: run Layer 0a (anchor alone) before Layer 0b (adding satellites).**

- **Layer 0a — Anchor-only branch (diagnostic, not shippable).** Set `anchor_volume: 1.0`, `source_count: 1` with one inactive placeholder object (`active: false, volume: 0.0`), no modulation, no movement, neutral reverb. Branch on `anchor_color` across 3–4 structurally different colors (typically Pink / Grey / SSN / Blue — they produce dramatically different tonotopic distributions). Measure each at 20 s and 60 s on the target brain type. Read the per-hemisphere band breakdown. **The Layer 0a winner is the foundation, not the product.** Even if it matches every neural metric perfectly, a single-anchor preset is perceptually monolithic and does not ship.

- **Layer 0b — Satellites are required, not optional (product-level hard floor).** A shippable preset must meet these minimums regardless of what the neural metrics say:
  - **At least 3 active sources**, because fewer than 3 cannot create a room-like spatial field. (1 source = point cue, 2 sources = stereo axis, 3+ = an enveloping field.)
  - **At least 2 distinct colors** across `{anchor_color} ∪ {active source colors}`, because a single spectral character is perceptually flat no matter how well it scores. Multiple colors bring different timbral textures that listeners actually notice.
  - Each added satellite still has to earn its place by not *breaking* the Layer 0a neural metrics — the floor is additive, not a license to ignore spec. If 3 sources + 2 colors pushes your bands out of spec, you have a real conflict to resolve (change color choices, lower volumes, reconsider the anchor), not a reason to drop back to 1 source.

**Why the hard floor exists.** Not everything that matters to a product can be measured by the NMM. The neural metrics see band powers, FHN firing, PLV, asymmetry. They do not see perceptual depth, spectral richness, spatial envelopment, timbral variety, or any of the compositional qualities that make a preset feel like a *place* rather than a *sound*. A framework that optimizes only what it can measure will drift toward minimum-viable artifacts — the cheapest configuration that satisfies the score. The hard floor is how we encode product-level requirements that live outside the NMM's view.

**Anti-pattern caught by the hard floor:** a single anchor (or anchor + 1 satellite) delivers all five band targets perfectly, all habituation metrics perfectly, and is structurally the simplest thing the engine can produce. The framework's "stop when spec is met" rule would ship it. **But it is not a preset, it is a test tone.** A listener exposed to a pure diffuse anchor for 5 minutes reports "there is nothing happening, where is the preset"; the fix is not more measurement, it's compositional presence.

**How to honor the floor without breaking the neural spec.** Four proven tactics:
1. **Color variety at low volume.** Add 2–3 non-anchor-color sources at volumes 0.05–0.15 each. Because they're quiet, their tonotopic contribution is small, and because they're a different color than the anchor, they add spectral character that the anchor alone cannot provide.
2. **Prefer colors that complement the anchor's tonotopic profile.** If the anchor is SSN (Low-mid heavy), Pink sources add uniform distribution, Brown adds Low-band warmth, Blue adds High-band sparkle. Pick the complement that doesn't over-drive bands the anchor already feeds.
3. **Symmetric placement for neural neutrality, asymmetric positions for perceptual interest.** Source positions don't have to be symmetric across x=0 — staggered triangles, elevated corners, and front/rear/side mixes all produce neural-equivalent results (we verified this in testing) but wildly different perceptual spaces. Use the spatial freedom for character, the neural safety for the volume/color choices.
4. **`reverb_send` as a neural-isolation knob (the single most powerful tactic).** Setting `reverb_send` to 0.90–0.98 on a source routes almost all of its energy through the reverb bus, which is processed *after* the bilateral JR model reads the input. The source remains perceptually present (its reverb tail, HRTF-shaped wet signal, and spatial positioning all still contribute to how the room sounds to a listener) but its *direct* contribution to the cortical model collapses to near-zero. This is how you satisfy the 3-source hard floor without triggering the 2+ sources alpha saturation on Normal brain. Empirical result: three active sources with `reverb_send: 0.95–0.98` produce the same band powers as anchor-only, while listeners experience the full 3-source spatial field. **Without this technique, the hard floor and the α ≤ 0.60 spec conflict on Normal brain.** With it, they coexist cleanly.

**`reverb_send: 0.95+` is volume-invariant** (characterized empirically). Tested source volumes 0.09 → 0.30 (a 3.3× range) with `reverb_send` held at 0.95. Neural-metric deltas across the full range were ≤0.005 on every band — deeper than the stochastic noise floor. Tonotopic distribution was bit-identical across the range. **Conclusion: with `reverb_send ≥ 0.95`, source volume is a pure perceptual knob with no neural-spec cost.** Tune it on audibility grounds alone; the NMM's reading of the preset does not move.

**The 2+ sources alpha saturation — it's a discrete threshold, not a gradient.** When two or more sources are active with normal `reverb_send` values (0.20–0.50), combined alpha on Normal brain jumps ~0.03 above the anchor-only baseline and **stays there regardless of volume, color, or position**. Empirically verified: 1 source → α 0.577, 2 sources → α 0.613, and this jump is preserved whether the second source is at vol 0.05 or 0.20, whether it's Pink / Brown / Grey / Blue / SSN, whether placement is symmetric or asymmetric, front or rear. The transition happens between 1 and 2 sources as a step function. Cause: bilateral HRTF processing of multi-source decorrelated signals interacts with the AST-specialized left hemisphere — adding decorrelated per-ear streams pushes the right-hemi alpha basin deeper. The `reverb_send: 0.95+` technique is the *only* workaround that keeps sources "active for the hard floor" while keeping the direct-signal contribution low enough to preserve the anchor-only band powers.

**Environment is near-null for anchor-dominant presets.** Tested FocusRoom (1) vs OpenLounge (2) with an anchor-only SSN preset — band power deltas were ≤0.02 on every band. The `environment` parameter affects post-processing reverb (early reflections, room simulation), which runs *after* the bilateral JR model reads its input, so it doesn't meaningfully change the neural state for presets where the anchor does most of the work. Pick `environment` on perceptual grounds; it isn't a Layer 3 lever for anchor-dominant designs. It *does* matter for source-dominant designs where the per-source reverb path carries most of the energy, and we have not characterized that thoroughly.

**Why this rule matters: it saves you from the most expensive mistake in the framework.** I (the guide) learned this the hard way. A test run of the Shield spec spent two hours with Pink sources as a locked-in assumption, branched through 6 candidate source configurations, hit every structural pathology the guide now describes, and produced a mediocre compromise. A single Layer 0a sweep (4 anchor-only colors, 4 files, 80 seconds of measurement) then revealed that **SSN anchor at volume 1.0 with zero sources** delivered the same spec almost perfectly. The two-hour effort was wasted because the source-dominant paradigm was never questioned. Layer 0a catches this in minutes.

**Counterintuitive empirical findings** from that Shield exercise, worth remembering:

- **Pink's −3 dB/oct rolloff is not "concentrated in low bands"** — on log-spaced tonotopic bands, pink distributes *roughly evenly* (34/34/24/9 in our test). It's the most *uniform* color across the filterbank, not the darkest.
- **"Perceptually flat" Grey is tonotopically darker than pink.** Grey dumped **88%** of tonotopic energy into the Low (50–200 Hz) band because equal-loudness correction boosts LF to compensate for the ear's insensitivity there. "Perceptually flat" means "LF-boosted in signal space". Grey on Normal brain drove right-hemi band 0 (θ-high 8.7 Hz) to 77% theta — exactly the opposite of alpha-idle.
- **SSN concentrates 73% in Low-mid (200–800 Hz)**, which hits right-hemi band 1 (α-low 9.5 Hz, JR) and left-hemi band 2 (WC 14 Hz SMR) simultaneously. This produces R α 82% / L β 76% and a combined profile of α 58 / β 29 / θ 11 / δ 0 / γ 0 — a clean alpha-dominant + stable-beta state from pure anchor.
- **Which anchor color is "right" is a tonotopic-band targeting question, not a psychoacoustic naturalness question.** Arguments like "pink is 1/f so the brain treats it as background" are psychoacoustic and correct for that domain — but they don't predict which tonotopic bands the gammatone filterbank will feed, which is what the JR model actually sees. The two frames give different answers, and the tonotopic frame wins for NMM-score-driven design.

**Structural vs shapeable pathologies.** After building a candidate, measure at zero-point (no modulation, no movement, light reverb ~0.25) and classify what you see:

- **Shapeable** (advance to Layer 1): wrong arousal, too much theta, flat spectrum, mild asymmetry, dominant frequency off by a few Hz, weak entrainment. Later layers fix these.
- **Structural** (branch — do *not* advance): hemispheric split where left locks into one attractor and right into another via inhibitory coupling; HF tonotopic band below ~8% when you need speech masking (Pink's −3 dB/oct rolloff is fundamental, no amount of shaping fixes it); source-count/placement choices that make later shaping mathematically impossible. No later layer will fix these — they are properties of the carrier, not the treatment.

**Rule: Layer 0 is a branch point, not a single step.** Generate **2–4 structurally different Layer 0 candidates** (different carriers, different source counts, different symmetries), measure each at zero-point, and pick the one whose baseline has only shapeable pathologies. Iterating Layers 1–4 on a structurally broken foundation is the single most common way to waste time with this framework. The skill is in branching back to Layer 0 when the foundation is wrong, not in shaping harder.

**Candidate design guidelines:**

- **Carriers:** 3–5 sources for the broadband bed. **SSN (6)** is the best carrier *for the NMM's neural metrics* because its speech-shaped tonotopic profile (Low-mid 73% + Mid-high 20%) is the only color that simultaneously feeds right-hemi JR α (via Low-mid at band 1 = 9.5 Hz) AND left-hemi WC(14) β (via Mid-high at band 2). This dual-band coverage is structurally unique and makes SSN *structurally required* for goals with dual-band targets (Flow's α+β coupling, Shield's bilateral masking). **But SSN has a perceptual cost:** its speech-range energy (500 Hz–4 kHz) engages involuntary bottom-up attention — the brain keeps trying to parse it — causing cognitive fatigue over 5+ minute sessions. Use **Pink (1)** for habituation-first presets or when the spec allows relaxing β toward a cognitively restful profile. Use **Brown (2)** for LF-warm restful masking. Do not reach for **White (0)** reflexively; its uniform power spectrum is rarely optimal anymore.
- **Anchor:** low-volume non-localized baseline (0.03–0.10) to fill gaps the directional sources miss. **SSN at 0.05–0.10** for speech-band masking; **Pink (1) or Brown (2) at 0.03** for warmth. The color enum is `0=White, 1=Pink, 2=Brown, 3=Green, 4=Grey, 5=Black, 6=SSN, 7=Blue` (canonical source: `NoiseColor::from_u8` in `noise_generator_dsp/crates/core/src/lib.rs:176–188`).
- **Source count and symmetry:** more sources is not automatically better. **3 asymmetrically-placed sources often produce cleaner bilateral convergence than 4 symmetric sources**, because 65% contralateral routing pushes both hemispheres in the same direction. Symmetric passive placement + stochastic JR + inhibitory callosum is how you *get* hemispheric split — and that split is structural, not shapeable.
- **Master gain:** set for comfort. Does not affect the score.
- **No modulation, no movement.** Everything is Flat/Static at Layer 0. Reverb sits at a neutral ~0.25 placeholder; Layer 3 tunes it.

**Measurement goal at Layer 0:** not to match target bands (that's Layers 1–4's job), but to confirm the foundation has only pathologies you can shape. Check the per-hemisphere band breakdown in the `evaluate` diagnostic, not just the combined. If left and right hemispheres are converging on similar bands (even if those bands are wrong), you can shape from there. If they're locked into *different* attractors, branch.

#### Layer 1 — Spatial field (sonic depth + incidental delta)

- Place sources with stereo balance in mind. Slight asymmetry is welcome; hemispheric differentiation is a bonus from inhibitory callosal coupling, not a goal in itself.
- Add **figure-8 or pendulum movement** at speed 1.5–5 rad/s on 2–3 sources. Movement is doing triple duty: sonic interest, delta via HRTF envelope variation, and anti-habituation.
- Use `z < -2` for sources that should blend into ambience; `z > +1` for foreground character.
- **Measurement goal:** you've probably shifted mass toward slower bands and raised delta without trying. Quantify how much. If delta jumped from 2% to 15% just from movement, that's most of your slow-wave budget right there.

#### Layer 2 — One intentional band driver

Pick exactly **one** modulator aimed at the band you're steering. This is where goal-awareness finally enters the process — not before.

| You want | Put this on satellite_mod of one off-center source |
|---|---|
| Beta (focus, shield) | NeuralLfo 14–22 Hz, depth 0.4–0.6 |
| Alpha (flow, deep work) | **No dedicated driver exists.** JR is a damped oscillator at ~10 Hz — alpha is self-generating when arousal is in the 0.45–0.55 reverb range and both hemispheres converge on the attractor. Use reverb (Layer 3), not a 10 Hz AM. **10 Hz NeuralLfo does NOT drive alpha**: the 10 Hz modulation envelope, after cochlear envelope extraction + ASSR DC/AC separation (~40% attenuation at 10 Hz), lands at the theta-alpha boundary and acts as weak theta drive. Tested and confirmed. |
| Theta boundary | NeuralLfo 8 Hz — this is the one that actually breaks the alpha attractor on Normal |
| Theta harmonic | Breathing (kind 2), pattern 3 |
| Delta | **Do not use slow NeuralLfo.** ASSR strips ~70% of the AC at 5 Hz. Use reverb + movement instead. |
| Gamma | 40 Hz NeuralLfo, but accept gamma ceilings at ~5% max — the cortical model can't sustain it |

One driver, measured, understood. Three stacked drivers fighting each other is how you end up with incoherent presets that score well by accident.

**Why there is no alpha driver.** The Jansen-Rit cortical model is fundamentally a damped oscillator with a natural frequency near 10 Hz — the alpha attractor is *self-generating* when conditions allow it. An AM modulator at 10 Hz feeds 10 Hz envelope content into the cochlear filterbank, which after envelope extraction looks like a slow-wave signal to the cortical model. Then ASSR DC/AC separation attenuates it by ~40% (10 Hz sits between the fully-attenuated 5 Hz band and the near-full-transmission 40 Hz band). The result: driving "10 Hz" on satellite_mod at depth 0.20 produces *more theta*, not more alpha. If you want alpha, don't drive it — let the attractor do its job and use reverb to set the arousal that unlocks it.

#### Layer 3 — Reverb as the arousal knob

Reverb is **coarse steering**, not a finishing touch. It directly maps to the thalamic gate's arousal estimate, which decides whether low bands are even accessible.

| Reverb send | Arousal | Regime | Best for |
|---|---|---|---|
| 0.15–0.30 | High | Beta-locked | Focus, Shield, Isolation |
| 0.30–0.45 | Medium-high | Alpha-beta mix | Deep Work |
| 0.45–0.55 | Medium | Alpha-dominant | Flow |
| 0.55–0.70 | Low | Alpha-theta accessible | Meditation, Relaxation |
| 0.70+ | Very low | Theta-delta possible | Sleep onset |

**Rule:** whatever band you drove in Layer 2, reverb has to be in the matching arousal row or you're fighting the thalamic gate. This is usually the single biggest lever on the score.

#### Layer 4 — Resilience and texture

- **30 Hz NeuralLfo at depth 0.05** somewhere in the mix. Inaudible, resists habituation by providing micro-variation that keeps synaptic depression from eroding the response.
- **Stochastic bass at 3–5 spk/s** if you want natural rain character — it also disrupts slow-wave coherence without sounding artificial.
- **Measurement goal:** re-evaluate at **20 s, 60 s, and 300 s**. 60 s is the intermediate check; **300 s is the shippable test.** If the 60 s score is more than ~0.05 below the 20 s score, habituation is winning — add more temporal variation (another movement source, another stochastic element) until the gap closes. The 300 s test is the one that matches the user's 5-minute listen window and catches dynamics that 60 s misses entirely.

**Why 60 s isn't the final bar.** 60 s tells you whether habituation has started to erode the preset. 300 s tells you where the preset *stabilizes* after the synaptic depression mechanism has had time to fully equilibrate. These are different questions and they can give different answers:

- A preset that looks fine at 60 s can still drift meaningfully over the next 4 minutes.
- More interestingly, a preset that looks *wrong* at 60 s (slightly over a ceiling, slightly off a target) can **self-correct toward the spec** over 300 s if the dynamics cut the right way. 60 s hides this.

**Constructive habituation (observed pattern).** Under specific conditions, habituation pulls a preset *toward* its spec rather than *away* from it. The pattern: when the over-target basin is the strongly-driven one (e.g., a right-hemi α basin driven by a saturated tonotopic path) and the under-target basin is noise-sustained (e.g., a left-hemi β basin driven by stochastic JR σ=15), synaptic depression erodes the overshoot while the noise-driven side holds. Net drift is toward spec. Observed empirically: a Shield-target preset drifted from α 0.595 / β 0.251 at 20 s to α 0.565 / β 0.284 at 300 s — both bands moved *closer* to their ~0.50 / ~0.30 targets. This is the first case in our testing where habituation helped rather than hurt; the framework recommendation is to always run 300 s on presets that look "slightly over" at 20–60 s, because the habituation dynamics may be doing the final trim for you.

**The 60 s measurement can also reveal identity shifts.** A preset that looks weak at 20 s may be *habituation-stable* at 60 s — its 20 s → 60 s delta on the strongest goal is near zero while competing goals collapse. That stability is itself a distinct product property (critical for masking / isolation / sleep-onset goals, where "the listener forgets it's playing" is the success criterion). Don't dismiss a 60 s score just because its absolute value is lower — compare 20 s and 60 s on the *same* goal across the matrix. The goal with the smallest habituation-drop is often the preset's real identity, not the goal with the highest 20 s score.

#### Layer 5 — Let the preset tell you what it is

Now run the full matrix:

```bash
cargo run --release -- evaluate presets/my_preset.json --goal all --brain-type all
```

**Forget your original intent.** Look at the 9×5 table. One of three things happens:

1. **It lands where you wanted, clean across 2–3 related goals.** Ship it. Label it after the strongest fit — the adjacent goals it also covers are a feature, not a bug.
2. **It lands somewhere else entirely.** Do not force it back. You just discovered a preset for a different goal — rename it, save it, start a new stack for the original target.
3. **It splits: good on Normal, awful on ADHD (or vice versa).** Accept it as a single-brain-type preset, or clone and re-tune (usually: reduce modulation depth because ADHD sits near the bifurcation boundary and amplifies everything).

The matrix is a diagnostic, not a scorecard. Presets that lightly cover multiple adjacent cells beat presets that maximize a single cell.

**Naming discipline.** Name presets by their *design features*, not by the measured goal they landed closest to. `pink_static_habituation_v1.json` is better than `isolation_v1.json` because it describes what you built, not which column of the 9×5 matrix it scored best on. Goal-label filenames smuggle goal-first thinking back into a sound-first process — and they compress the preset's real identity (a full 9×5 matrix, often with habituation behavior that matters more than the absolute peak) down to a single label that hides the interesting information. A descriptive filename keeps the taxonomy honest and makes Layer 5's "let the preset tell you what it is" rule stick.

**A known tension worth naming.** The codebase's goal taxonomy (`focus`, `shield`, `flow`, `ignition`, `isolation`, …) is a goal-first artifact — it exists because the optimizer needs a fitness function. A sound-first process doesn't naturally produce presets that fit cleanly into these 9 bins; Layer 5 forces you to pick one at the end, which is a small betrayal of the framework's own philosophy. The honest answer is that a preset's identity is its full 9×5 matrix fingerprint at 60 s, not a single label. Use the labels where you must (CLI argument, goal-based UX), but store your mental model of the preset as the matrix.

#### Layer 6 — 5-minute listen test

Non-negotiable. The model cannot see: fatigue, pulsing artifacts, harshness, emotional tone, drift quality, stereo discomfort. A preset that works for 20 s of simulation but induces fatigue at 5 minutes is not a product. Trust your ears over any score above 0.70.

### When the optimizer is still useful

Don't abandon DE — use it correctly:

- **Warm-start only.** Always pass `--init-preset` pointing at your Layer-4 handcrafted base. DE is path-dependent; it stays local to your seed. Its job is to refine, not to discover from random init.
- **Short runs (40–60 generations).** Longer runs over-fit model quirks.
- **Manual sweeps beat DE for understanding.** "What does reverb do here?" — 5 `evaluate` calls at reverb 0.15, 0.30, 0.45, 0.60, 0.75 teach you more than any DE run.
- **Reserve DE for edge cases where hand design is hard.** Ignition on ADHD (gamma + high FHN against weak-inhibition cortex) is a legitimate optimizer target. Focus on Normal is not — you can hand-build that better than DE can.

### The grid is a research map, not a product taxonomy

9 goals × 5 brain types is a measurement grid for exploring what the model does. It is not a 45-SKU product line. Real users live in maybe three or four states — "focus", "wind down", "block the neighbors", "sleep" — and the presets they love are ones that degrade gracefully across those states, not ones that maximize any single cell. Design for the user's lived states, measure across the grid to confirm, label by actual effect rather than original intent.

---

## What the model actually measures

The pipeline is:

```
Audio --> Cochlear filterbank (32 gammatone channels, 4 tonotopic bands)
      --> Global max normalization (all bands normalized to the global max)
      --> 95th percentile FHN scaling
      --> [CET only] Slow/fast crossover at 10 Hz (1st-order leaky integrator + complementary HP)
      --> ASSR DC/AC separation on FAST path only (slow path bypasses ASSR — CET fix)
      --> [CET only] Recombine slow + (ASSR-attenuated) fast paths
      --> Band-dependent thalamic gate (arousal-driven input_offset shift)
      --> Bilateral Jansen-Rit cortical model (inhibitory corpus callosum coupling)
        -->  [CET only] Slow GABA_B parallel population (B_slow=10 mV, b_slow=5/s, τ≈200 ms)
      --> Wilson-Cowan adaptive frequency tracking (within +/-5 Hz Arnold tongue)
      --> FitzHugh-Nagumo single-neuron probe
      --> Habituation (synaptic depression, rate 0.0003, recovery 0.0001)
      --> Stochastic JR (Gaussian noise sigma=15, broadens spectrum)
      --> Carrier PLV against the LFO frequency (always)
      --> [CET only] Envelope-phase PLV against the slow-path drive (2-9 Hz Hilbert)
      --> Score against goal-specific targets
```

The `[CET only]` stages are gated behind `--cet` (Priority 13). Their effect is bitwise-zero when disabled — same regression-safety pattern as `--assr`, `--thalamic-gate`, and the stochastic/habituation flags.

The score is now 100% neural model — brightness was removed:

| Component | Weight | What it measures |
|-----------|:-:|---|
| **Band powers** | 65–80% | How well EEG band powers match the goal targets (per-goal; see table below) |
| **FHN firing** | 20–35% | Whether the single-neuron firing rate is in the target range |
| **Asymmetry penalty** | (subtractive) | Excessive L/R alpha asymmetry penalized for balance-wanting goals (meditation, relaxation, flow). Sleep ignores it. |
| **Carrier PLV bonus** | (additive) | Phase-Locking Value (Lachaux 1999) rewards entrainment to the carrier modulation frequency for goals that want a tone-driven response. Up to `10% × weight × PLV`. |
| **Envelope PLV bonus** (CET) | (additive) | Phase-locking of the EEG to the slow 2–9 Hz envelope of the auditory drive. Per Ding & Simon (2014) and Luo & Poeppel (2007) this is the cortical envelope tracking signal. Only computed when `--cet` is enabled. Up to `10% × envelope_weight × envelope_PLV`. |

The exact split is per-goal (`src/scoring.rs`):

| Goal | band / fhn | Carrier PLV | Envelope PLV (CET) | Asym threshold / max |
|---|:-:|:-:|:-:|:-:|
| Focus | 0.70 / 0.30 | 1.00 | 0.00 | 0.5 / 5% |
| DeepWork | 0.75 / 0.25 | 0.60 | 0.20 | 0.5 / 5% |
| Sleep | 0.65 / 0.35 | 0.00 | **0.80** | — / 0% |
| DeepRelaxation | 0.70 / 0.30 | 0.00 | **0.70** | 0.3 / 12% |
| Meditation | 0.65 / 0.35 | 0.30 | **0.60** | 0.2 / 15% |
| Isolation | 0.80 / 0.20 | 0.80 | 0.00 | 0.4 / 8% |
| Shield | 0.70 / 0.30 | 0.70 | 0.00 | 0.4 / 8% |
| Flow | 0.70 / 0.30 | 0.30 | 0.40 | 0.3 / 12% |
| Ignition | 0.70 / 0.30 | 1.00 | 0.00 | 0.6 / 3% |

The brightness term that used to contribute a free 10% is gone. Score is entirely driven by neural dynamics. The two PLV terms are additive on different perceptual axes — a preset can score on both simultaneously when its carrier locks the cortex AND its slow envelope is tracked.

**Critical note for relaxation/sleep design:** Sleep, DeepRelaxation, and Meditation previously had carrier PLV weight = 0% — they had **no entrainment reward channel at all**. With CET enabled (`--cet`), they now get an envelope-tracking reward channel weighted at 60–80%. This is the first mechanism by which these goals can be rewarded for genuine slow-rhythm cortical tracking, and it directly favors presets with slow NeuralLfo modulation (1–8 Hz) on broadband noise — exactly the "organic" sound classes (wind, surf, breath-paced pink) that the literature identifies as inducing relaxation through cortical envelope tracking rather than carrier entrainment.

---

## Score interpretation cheat sheet

| Score | Verdict | Practical meaning |
|---|---|---|
| 0.00-0.49 | POOR | Model is misbehaving — usually one band dominates >70% |
| 0.50-0.59 | OK | Acceptable for casual use, naive presets often land here |
| 0.60-0.69 | OK | Some intentional design is paying off |
| 0.70-0.79 | GOOD | Multiple mechanisms working together |
| 0.80-0.85 | GOOD | Probably near the realistic ceiling for most goals |
| > 0.85 | (rare) | Suspect overfitting to model quirks; verify subjectively |

**Important:** these are model scores, not human-perceived quality scores. A 0.50 pink-noise preset can feel more pleasant than a 0.80 optimizer preset. See "The score isn't perception" below.

---

## The 11 model changes and what they mean for preset design

Before diving into mechanisms, understand what changed — these invalidate many old assumptions.

### Global band normalization (replaces per-band)

Previously, each tonotopic band was normalized to its own max: `normalized_band = raw_band / max(band)`. This meant every band always hit 1.0 somewhere, erasing all spectral color differences. Brown and White produced identical neural inputs.

Now, all bands are normalized to the **global maximum** across all bands. If the High band peaks at 0.8 and Low peaks at 0.2, the model sees that difference. This means:

- **Color choice (of the anchor) matters neurally.** Different anchor colors produce dramatically different tonotopic distributions. Source colors don't — see the anchor-dominance rule above.
- **SSN is the only color** whose tonotopic profile feeds both right-hemi JR α (via Low-mid → band 1) AND left-hemi WC(14) β (via Mid-high → band 2). This makes SSN structurally required for goals with dual-band neural targets.
- **SSN has a psychoacoustic cost the NMM can't measure.** Speech-range energy (500 Hz–4 kHz) engages bottom-up attention circuits involuntarily — the brain keeps trying to parse it — causing cognitive fatigue over 5+ minute sessions. For goals where the neural spec doesn't strictly require SSN's dual-band profile, Pink or Brown anchors are cognitively more restful. For goals where SSN is required, use minimum viable volume.
- **Blue is no longer best for isolation.** SSN replaced it once the model could see spectral shape (and Blue saturates beta into gamma leakage).

### FHN 95th percentile scaling

Max normalization of EEG-to-FHN coupling was replaced with 95th percentile scaling. The old max-norm meant outlier spikes set the scale, compressing the dynamic range. Now the FHN firing rate responds to actual EEG amplitude differences between presets. A louder preset genuinely drives the FHN harder.

### Inhibitory bilateral coupling

The corpus callosum coupling changed from excitatory (+k) to inhibitory (-k). Previously, hemispheres synchronized — when one went alpha, the other followed. Now they **differentiate**: when one hemisphere is driven hard, it actively suppresses the same activity in the other hemisphere.

This means asymmetric placement creates **more** hemispheric contrast than before, not less. A source placed hard left doesn't just drive the right hemisphere — it also inhibits that pattern in the left hemisphere via the inhibitory coupling.

### Band-dependent thalamic gate

A new arousal-dependent input_offset shift was added. The thalamic gate computes arousal from environmental parameters:

- **Low arousal** (dark noise colors, high reverb, slow modulation rate) shifts input_offset downward on low-frequency bands, pushing bands 0-1 toward the JR bifurcation boundary where theta and delta oscillations become possible.
- **High arousal** (bright colors, dry/low reverb, fast modulation) keeps input_offset high, locking the model in beta.

The shift is band-dependent: full effect on bands 0-1 (Low, Low-mid), partial on band 2 (Mid-high), none on band 3 (High). This is why **reverb is now the primary lever for accessing slow-wave states** — it directly modulates the thalamic gate's arousal estimate.

### ASSR DC/AC separation

The Auditory Steady-State Response mechanism now separates the DC (mean level) and AC (modulation) components of band signals. ASSR attenuates only the AC component based on the preset's NeuralLfo frequencies:

- **40 Hz modulation**: near-full transmission (~100%)
- **5 Hz modulation**: only ~31% of the modulation envelope gets through

This means the old trick of "same-freq bass+sat at maximum depth" is **weakened for slow frequencies**. A 5 Hz NeuralLfo on both bass and satellite used to be devastating; now only 31% of that modulation envelope reaches the cortical model. The DC component (overall band energy) still passes through unchanged. Fast modulation frequencies (beta/gamma range) remain fully effective.

### Wilson-Cowan adaptive frequency tracking

The Wilson-Cowan oscillator model now tracks the input modulation frequency within a +/-5 Hz Arnold tongue instead of oscillating at a fixed natural frequency. If you drive it at 12 Hz and its natural frequency is 10 Hz, it will entrain to 12 Hz. This makes entrainment more realistic and means your target frequencies are more faithfully reproduced in the cortical output.

### Habituation (synaptic depression)

Enabled by default. Connectivity parameter C decreases over time with sustained activity (rate 0.0003) and recovers during low activity (rate 0.0001). This means:

- Sustained monotonic stimulation loses effectiveness over time
- Presets with temporal variation (modulation, movement) resist habituation better
- Long evaluation windows will show lower scores than short ones for static presets
- The 30 Hz NeuralLfo at depth 0.05 (inaudible texture frequency) provides resilience against habituation by adding micro-variation

### Stochastic JR

Gaussian noise (sigma=15) is added to JR input drive by default. This breaks the deterministic alpha attractor and broadens the EEG spectrum. The practical effect: the alpha floor on Normal brain type is lower than before — the model can be pushed further away from alpha than the old deterministic version allowed.

---

## Mechanisms that actually move the score

These are what we discovered through experimentation after the model updates, in rough order of impact.

### 1. Reverb as the thalamic gate lever (HUGE impact)

Reverb is no longer just "slow temporal structure" — it is the **primary control over the thalamic gate's arousal state**. This makes it the single most important parameter for determining which EEG regime the model enters.

| Reverb send | Arousal | Model regime | Best for |
|---|---|---|---|
| 0.15-0.30 | High | Beta-locked | Focus, Shield, Isolation |
| 0.30-0.45 | Medium-high | Alpha-beta mix | Deep Work |
| 0.45-0.55 | Medium | Alpha-dominant | Flow |
| 0.55-0.70 | Low | Alpha-theta accessible | Meditation, Relaxation |
| 0.70+ | Very low | Theta-delta possible | Sleep onset |

**Rule:** Set your reverb sends FIRST based on the goal's target bands, then tune everything else around that choice. Trying to force theta with low reverb, or beta with high reverb, fights the thalamic gate.

### 2. Noise color selection (now matters)

With global normalization, color choice directly shapes the neural input. Two important rules emerged from testing:

**The anchor is ~90% of the tonotopic profile. Sources barely shift it.** Empirical evidence across many tests:
- Pure SSN anchor 1.0 → tonotopic 4/73/20/2
- SSN anchor 0.85 + 3 Pink sources at 0.15 each → 4/76/19/0 (identical profile)
- SSN anchor 0.55 + 3 mixed-color sources at 0.30 each → 4/76/19/0 (identical profile)
- Brown anchor 0.60 + SSN source 0.20 → 79/18/3/0.3 (Brown profile, not SSN)
- Brown anchor 0.30 + SSN source 0.50 → 72/22/6/0.3 (still Brown profile)

**Even at source volumes up to 0.50 (more than half the anchor), source contribution to the tonotopic profile stays under ~10%.** This means: (a) **source colors are primarily a perceptual knob, not a neural one** — changing source colors shapes listening character but barely touches what the gammatone filterbank sees; (b) **to shift the neural tonotopic profile, you must change the anchor color**; (c) the anchor's absolute volume affects total loudness but not the proportional distribution across bands (normalization handles that).

**Perceptual masking cannot hide a neurally-required anchor color.** A natural intuition — "use a loud Brown bed to hide a quiet SSN layer" (the Trojan Horse approach) — fails on this NMM because the gammatone filterbank doesn't model human critical-band masking. Whichever color dominates the raw signal sum also dominates the NMM's neural reading. Empirically verified: you cannot get SSN's dual-band neural benefits while hiding its speech-range presence under a perceptually-dominant Brown bed. If your goal's neural spec requires SSN-profile tonotopic input, you have to accept SSN's perceptual character as well. The only knob that works is volume: SSN anchor at the minimum viable level (verified floor for Flow: 0.25) reduces perceptual dominance while preserving the neural spec.

**Color character notes:**

The enum is `0=White, 1=Pink, 2=Brown, 3=Green, 4=Grey, 5=Black, 6=SSN, 7=Blue` (canonical source: `NoiseColor::from_u8` in `noise_generator_dsp/crates/core/src/lib.rs:176–188`, with labels from `src/main.rs:1048`). **There is no Violet.** An earlier iteration of this guide listed Violet as index 4 — that was wrong; index 4 is Grey.

| Carrier | Strengths | Use case |
|---|---|---|
| **SSN (6)** — Speech-Shaped Noise | The *only* color whose tonotopic profile (Low-mid 73% + Mid-high 20%) feeds right-hemi JR α AND left-hemi WC(14) β simultaneously. Stable under source/driver perturbation — its α basin doesn't tip. **Psychoacoustic cost:** speech-range salience (500 Hz–4 kHz) engages bottom-up attention, causing cognitive fatigue in 5+ min sessions. | **Structurally required** for dual-band goals (Flow, Shield, Focus). Use minimum viable anchor volume (tested floor 0.25 for Flow on Normal brain). **Not recommended** as sole anchor for Sleep / Deep-Relaxation / Meditation — use Pink or Brown instead. |
| **Pink (1)** | Gentle 1/f rolloff, natural-sounding. Brain treats it as "no information" → fastest habituation. Tonotopically near-uniform on log-spaced bands (34/34/24/9). | Primary carrier when 1/f statistics are a requirement. **Fragile under perturbation** — α comes from a precarious right-hemi basin that collapses when drivers/sources are added. |
| **Brown (2)** | Deep bass, 1/f² rolloff. Tonotopically 68/17/9/6 — heavily LF-weighted. Vestibular grounding, perceptually important for ADHD (safety). | ADHD anchor at 0.03–0.05 for perceptual comfort. α 0.44 baseline is closest to "0.40 dominant" specs of any color, but likely fragile like Pink. |
| **Green (3)** | HF-weighted (3/50/42/6). Produces β 0.80 / α 0.14 baseline on Normal brain — beta-dominant, no alpha. | Rarely primary. Use if you need a beta-dominant base (e.g. a driverless Shield variant). |
| **Grey (4)** | Perceptually flat (equal-loudness corrected). Tonotopically HF-balanced (20/18/32/30). Baseline α 0.20 / β 0.47 / γ 0.10. | Moderate-beta base; γ leakage limits use. |
| **Black (5)** | LF-extreme (88/8/2/2). Baseline α 0.20 / θ 0.78 — deep slow-wave. | Sleep-onset / deep-relaxation candidate, never for beta goals. |
| **White (0)** | Bright, feeds High band strongly (tonotopically 4/17/43/35). Baseline β 0.82 / γ 0.16. | Rarely primary — beta saturates, gamma bleeds in. |
| **Blue (7)** | Most HF-extreme (2/7/38/53). The *only* color that produces a bilateral (non-AST-split) state on Normal brain — both hemis lock in beta. | Specialist use when bilateral beta is explicitly wanted; γ 0.21 leakage. |

**Key finding:** SSN is the best universal carrier *for the NMM's neural metrics* for two independent reasons — (1) it's the only color that feeds both the α and β oscillators simultaneously, and (2) it's stable under perturbation (its right-hemi α basin doesn't tip the way Pink's and Brown's do). **But "best for the NMM" is not "best for the listener".** SSN's speech-range energy triggers evolved speech-detection salience, producing cognitive fatigue that the neural metrics can't see. The tradeoff is real and irreducible on this NMM:

- **Dual-band goals (Flow, Shield, Focus)** → SSN anchor is structurally required. Minimize its volume (tested Flow floor: 0.25) and accept the cognitive-load tradeoff; consider limiting session duration to ~15–30 min rather than multi-hour use.
- **Single-band or slow-wave goals (Sleep, Meditation, Deep Relaxation)** → Pink or Brown anchor. Their fragile α basins don't survive Layer 2 perturbations, but these goals *don't need* Layer 2 drivers (they want passive accumulation), so the fragility doesn't bite. You get cognitively restful presets with matching neural targets.

For warmth at the anchor position in SSN-based presets, **Pink or Brown at 0.03–0.05** remain the right choices.

### 3. Asymmetric spatial placement (enhanced by inhibitory coupling)

The bilateral JR model has 65% contralateral input routing. With the new **inhibitory** corpus callosum coupling, asymmetric placement is even more powerful than before: a lateralized source both drives the contralateral hemisphere AND suppresses that pattern in the ipsilateral hemisphere. The hemispheres actively differentiate.

**Rule:** Always have at least one strongly-modulated source positioned off-center (e.g., x = +/-2 to +/-5).

**For goals that want balance** (meditation, relaxation): excessive asymmetry is now penalized by the alpha asymmetry scoring term. Design for moderate asymmetry — enough to break the alpha attractor, not so much that the penalty kicks in.

**For goals that ignore asymmetry** (sleep): go as asymmetric as you want.

### 4. NeuralLfo design patterns

The ASSR DC/AC separation and WC adaptive frequency tracking change how NeuralLfo works:

| Frequency | AC transmission | Neural effect | Audibility |
|---|---|---|---|
| 5 Hz | ~31% | Weak theta entrainment (most modulation stripped) | Audible wobble |
| 8 Hz | ~50% | Theta-alpha boundary. Produces theta on Normal brain (breakthrough). | Noticeable tremolo |
| 10 Hz | ~60% | Alpha entrainment | Moderate tremolo |
| 14-22 Hz | ~80% | Beta entrainment (still effective) | Subtle flutter |
| 30 Hz | ~95% | Inaudible texture. Provides habituation resilience at depth 0.05. | Imperceptible |
| 40 Hz | ~100% | Full gamma transmission (but gamma still hard to sustain) | Imperceptible |

**Key discovery:** 8 Hz NeuralLfo sits at the theta-alpha boundary and, combined with the thalamic gate at medium-low arousal, produces genuine theta on Normal brain type. This was the breakthrough for making theta accessible on Normal — previously considered nearly unreachable.

**30 Hz at depth 0.05** is now a standard addition to every preset. It's completely inaudible but provides micro-variation that resists habituation (synaptic depression).

### 5. Modulator kind selection

| Modulator | Best for | Why | Notes post-update |
|---|---|---|---|
| **Breathing (kind 2)** | Theta (4-8 Hz) | Slow envelope creates theta harmonics | Still effective, but now works WITH thalamic gate — pair with medium-high reverb |
| **SineLfo (kind 1) at 1.5-3 Hz** | Delta (0.5-4 Hz) | Direct entrainment | Weakened by ASSR DC/AC separation for slow freqs. DC component still passes. |
| **NeuralLfo (kind 4) at 14-22 Hz** | Beta (13-30 Hz) | Direct entrainment via satellite_mod | Still the primary beta driver. High AC transmission at these freqs. |
| **NeuralLfo (kind 4) at 8 Hz** | Theta-alpha boundary | Produces theta on Normal via WC tracking | New discovery. Medium AC transmission but sufficient with gate support. |
| **Stochastic (kind 3) at 3-5 spk/s** | Natural rain texture + broadband disruption | Random pulse trains at slow rate sound like rain on leaves | Best bass texture pattern. Disrupts slow-wave coherence without sounding artificial. |
| **NeuralLfo (kind 4) at 30 Hz, depth 0.05** | Habituation resilience | Inaudible texture frequency | Standard addition to all presets. |

### 6. Movement speed and pattern

Fast spatial movement (figure-8 or pendulum at speed 3-5 rad/s) creates:
- **Delta via HRTF variation** — the slow envelope changes from position shifts register as low-frequency content
- **Thalamic gate arousal modulation** — movement itself affects the gate's arousal estimate
- Organic-sounding spatial variation

Static or very slow movement (speed < 0.5) produces flat envelopes that feed the alpha resonance.

**Rule:** Use figure-8 or pendulum at speed 1.5-5 rad/s on at least 2-3 sources. Movement is a primary delta generator — we confirmed this when slowing a fast figure-8 (speed 4.5 to 0.6) crashed delta from 21% to 2.4%.

### 7. Source position and perceptibility (z-axis)

| z range | Audible role | Neural role |
|---|---|---|
| `z > +1` (in front of listener) | **Foreground** — individually localizable | Strong direct input, less reverb interaction |
| `z = 0 to -2` | Mid-distance | Balanced |
| `z < -2` (behind listener) | Background ambience | More reverb-mediated, blurrier modulation |

Sources placed deep behind (z < -3) blend into background texture. Sources in front (z > +1) become foreground characters.

### 8. Master gain and object volume

**`master_gain` does not affect the score.** It's a global scalar applied after normalization.

**Per-object volume DOES affect the score** because volumes are applied before global normalization. With global max normalization (not per-band), per-object volumes change the spectral balance of the mix, and the model now actually sees those spectral differences.

This is more consequential than before: changing the volume ratio between an SSN source and a Brown anchor genuinely shifts the neural input spectrum, because global normalization preserves those differences.

---

## Goals reference

### Original goals (updated behavior)

All goals now score without brightness. The band-power and FHN components are the entire score, plus asymmetry penalty and PLV bonus where applicable.

| Goal | Band target | Asymmetry penalty | PLV bonus | Key design lever |
|---|---|---|---|---|
| **Isolation** | Flat bands (each at 20%) | No | No | SSN carrier, moderate reverb, broadband modulation |
| **Focus** | Beta-dominant | No | Yes | Low reverb (high arousal gate), beta NeuralLfo |
| **Deep Work** | Alpha + theta | No | No | Medium reverb, 8 Hz NeuralLfo, Breathing |
| **Sleep** | Slow-wave dominant (delta + theta) | No | No | High reverb (low arousal gate), slow movement |
| **Deep Relaxation** | Slow-wave focus | Yes (penalizes imbalance) | No | High reverb, symmetric placement, Pink/Brown carriers |
| **Meditation** | Theta + alpha co-dominant | Yes (penalizes imbalance) | No | Medium-high reverb, 8 Hz NeuralLfo, balanced placement |

### New goals (added with model updates)

| Goal | Band target | Asymmetry penalty | PLV bonus | Key design lever |
|---|---|---|---|---|
| **Shield** | Beta-dominant, low theta, stable FHN | No | Yes | Very low reverb (0.15-0.20), multiple beta NeuralLfo drivers, SSN carrier |
| **Flow** | Alpha-dominant, rhythmic coherence | No | Yes | Medium reverb (0.40-0.50), 10 Hz NeuralLfo, Brown+Pink satellites for alpha:beta balance |
| **Ignition** | Gamma binding, high FHN (for ADHD activation) | No | Yes | Low reverb, 40 Hz NeuralLfo (full ASSR transmission), aggressive modulation depths |

**PLV coherence** (Lachaux 1999) measures true phase-locking between the driving modulation and the cortical response. Goals get a score bonus proportional to PLV, weighted by how much they value entrainment: Focus and Ignition (1.0) benefit most, then Isolation (0.8), Shield (0.7), DeepWork (0.6), and finally Meditation and Flow at 0.3. Sleep and Deep Relaxation get no PLV bonus — they want natural rhythms, not external locking. This rewards presets that create genuine neural entrainment versus those that just happen to produce the right average band powers.

---

## Model quirks and limitations

### The alpha attractor (weakened but still present)

The JR model is fundamentally a damped oscillator at ~10 Hz. The stochastic JR addition (sigma=15) broadens the spectrum and weakens the alpha attractor, but it doesn't eliminate it. For Normal brain type, you can now push alpha lower than the old ~35% floor, but it still resists going below ~25-30%.

The thalamic gate provides a new mechanism to escape alpha: by lowering arousal (high reverb, dark carriers), you shift the operating point toward bifurcation where theta/delta become accessible. This was not possible in the old model.

### Theta on Normal brain (now achievable)

The old guide said theta was "nearly unreachable on Normal brain." This is no longer true. The combination of:

1. **Thalamic gate** at low arousal (reverb 0.55+)
2. **8 Hz NeuralLfo** at the theta-alpha boundary
3. **Stochastic JR** breaking the deterministic alpha lock

...makes theta genuinely accessible on Normal. We measured 15-20% theta on Normal brain with these mechanisms combined. The old ceiling of ~5-7% is broken.

### Hemispheric specialization (AST) is hardcoded — the bilateral model does NOT produce symmetric cortical state

**This is probably the single most important thing to understand about the bilateral model**, and it's the finding that will cost you the most time if you miss it.

**Read `src/brain_type.rs:386–433`.** Every brain type's `BilateralParams` explicitly configures:

- **Left hemisphere:** bands 0–1 are Jansen-Rit at θ/α rates; **bands 2–3 are `WilsonCowan(14.0)` and `WilsonCowan(25.0)`** — SMR (sensorimotor rhythm) and beta oscillators. This is hardcoded. The left hemisphere *cannot not produce beta* when mid/high tonotopic bands are driven, because those bands are explicit beta oscillators. More drive = more beta.
- **Right hemisphere:** bands 0–3 are all Jansen-Rit at natural rates 8.7 / 9.5 / 10.6 / 11.3 Hz — a dedicated slow-to-alpha gradient. **10–12 Hz alpha lives on the right hemisphere in this NMM, not the left.**

This is the **Asymmetric Specialization Hypothesis (AST)** baked into the model: left hemisphere specializes in analytical/fast processing, right hemisphere specializes in holistic/slow processing. The model is doing exactly what the code says it should. The hemispheric "split" that a naive user will hit when they build a passive symmetric pink preset is not a bug and not stochastic noise — **it is the architecture working as specified.**

**Empirical consequences** (measured across 20 candidate × brain-type combinations on passive symmetric pink):

- Normal brain, 4 symmetric pink sources: Left 63% β / 13.6 Hz dom; Right 37% α / 7.97 Hz dom. *Not* a bug.
- Adding SSN anchor at 0.10–0.15 pushes left-hemi beta *higher* (63% → 77% → 80%), because SSN's speech-band energy feeds left bands 2–3 (WC oscillators) directly. Counterintuitive but correct given the architecture.
- HighAlpha, Anxious, Aging, ADHD all produce the same left-fast/right-slow pattern with different intensities. Normal has the *least* extreme right-hemi theta collapse (61%), which ironically makes it the best substrate for "bilateral alpha-ish" goals despite the left-beta lock.
- No combination of source count (2/3/4), symmetry (symmetric vs asymmetric vs diagonal), drive level, anchor color, or reverb (0.15–0.55) changes the left-beta / right-slow pattern on any brain type. Not shapeable by preset-level parameters.

**What this means for preset design:**

- **The "Combined" band powers in the diagnostic are a measurement artifact for bilateral-alpha goals**, because they average a beta-heavy left hemi with a theta/alpha-heavy right hemi and produce apparent theta-dominance that neither hemisphere actually has. For goals that want interhemispheric differentiation (Focus, Ignition, Shield-as-vigilance-plus-calm), **read the per-hemisphere breakdown, not the combined**.
- **A spec of "α 50% / β 30% / low θ" written without hemispheric qualification is usually describing a *unified* state** — which this NMM does not produce from passive input. Re-interpret such specs as "left hemisphere β-dominant + right hemisphere α-dominant" and evaluate per-hemisphere.
- **If you genuinely need a unified bilateral alpha state** (both hemispheres together, no split), you cannot get it from passive pink on any brain type. The only paths that *might* work (untested) are: (a) HighAlpha brain + aggressive Layer 2 driving to pull both hemispheres through the Arnold tongue simultaneously, (b) a model change to replace the WC(14)/WC(25) oscillators on the left hemisphere with JR at alpha rates, or (c) explicit per-hemisphere input routing that bypasses the bilateral architecture.

**Practical implication at Layer 0:** when you measure the zero-point and see left-fast / right-slow, **this is not a shapeable pathology and not a structural pathology — it is the foundation**. Do not iterate trying to "fix" it. Instead, decide whether your preset is a *differentiated-state* preset (embrace the split, design for each hemisphere separately) or a *right-hemi-lead* preset (treat the right hemisphere's alpha as the "main" output and accept left-hemi beta as background analytical tone). Both are legitimate design stances. Fighting the AST is not.

### Gamma is still essentially impossible

The fast inhibitory population can produce gamma in principle, but gamma stays at ~0.3% across all reasonable presets. The 40 Hz ASSR pathway transmits fully, but the cortical model can't sustain gamma oscillations. Don't optimize for gamma.

### Habituation changes long-run behavior

With synaptic depression enabled (rate 0.0003, recovery 0.0001), static presets lose effectiveness over time. A preset that scores 0.75 at 20 seconds may score 0.68 at 120 seconds if it lacks temporal variation.

**Counter-habituation strategies:**
- The 30 Hz NeuralLfo at depth 0.05 (standard on all presets)
- Movement (figure-8, pendulum) creates ongoing variation
- Multiple modulation sources at different rates prevent the model from adapting to any single frequency
- Stochastic modulators are inherently anti-habituation (random by nature)

### Brain-type-specific behavior

The same preset can score very differently across brain types:

| Brain type | input_offset | Operating regime |
|---|:-:|---|
| Normal | 175 | Alpha limit cycle (weakened by stochastic JR) |
| HighAlpha | 195 | Deep alpha |
| ADHD | 135 | Right at bifurcation boundary, hyper-sensitive |
| Aging | 165 | Slower dynamics |
| Anxious | 220 | Beta-biased |

**ADHD operates near the bifurcation boundary.** Small modulation moves it dramatically. ADHD presets need gentler modulation (depth 0.30-0.40) spread across more sources (4-5) rather than aggressive modulation from fewer sources.

### Score noise floor

The pipeline has stochastic dynamics (intentionally, with sigma=15 Gaussian noise in JR). Running the same preset twice produces scores that vary by +/-0.02. **Differences below 0.05 are not significant.**

### Length-of-evaluation and habituation

Short evaluations inflate scores because habituation hasn't kicked in. Long evaluations are more reliable but slower.

| Duration | Use case |
|---|---|
| 4-8 s | Optimizer search (fast iteration) |
| 12-20 s | Quick spot checks, Layer 0a anchor branch, Layer 0b source branch |
| 60 s | Intermediate check — habituation effects starting to become visible |
| **300 s** | **Shippable test** — matches the 5-minute listen window, catches constructive habituation drift, the actual bar for a final preset |
| 600+ s | Long-form fatigue test (rare, only when 300 s shows trend that hasn't stabilized) |

---

## The score isn't perception

The score measures how closely the simulated EEG band powers match a target distribution, plus FHN firing rate, with asymmetry and PLV modifiers. This is:

1. Computed from a simplified neural model (now richer with 11 improvements, but still simplified)
2. Targeting engineering constructs that approximate but don't replicate real cognitive states
3. Insensitive to audio quality, pleasantness, fatigue, attention, or context

Real human listeners care about completely different things:
- Spectral comfort (no harsh peaks, no muddy buildup)
- Stationarity (sudden changes break attention)
- Stereo balance (asymmetric output is jarring)
- Spatial naturalness (artifacts from extreme positions)
- Total broadband energy (louder masker = better masking)

A preset optimized purely for the score will frequently:
- Sound asymmetric (lateralized strategy for hemispheric differentiation)
- Have weird modulation artifacts (high-depth Breathing patterns audible as pulsing)
- Use modulation frequencies chosen for neural effect, not sonic quality
- Use silent "ghost" objects that contribute nothing audibly

**Workflow recommendation:** treat the score as a research signal, not a product metric. Use it to explore parameter space and identify mechanisms, then trust your ears for the final selection.

---

## Common tradeoffs and how to manage them

### Symmetry vs. score vs. asymmetry penalty

The optimizer prefers asymmetric placement because inhibitory coupling maximizes hemispheric differentiation. But:
- Real listeners prefer stereo-balanced sound
- Goals with asymmetry penalties (meditation, relaxation) actively score worse with extreme asymmetry

**Resolution:** Moderate asymmetry — keep the main lead source off-center, mirror lower-impact sources for stereo balance. For meditation/relaxation, verify the asymmetry penalty isn't eating your score.

### ASSR and slow-frequency modulation

The old "same-freq bass+sat at max depth" trick is weakened for slow frequencies by ASSR DC/AC separation. A 5 Hz modulator only gets 31% of its AC component through.

**Resolution:** For slow-wave driving, rely more on:
- **Reverb** (thalamic gate arousal) rather than slow NeuralLfo
- **Movement** (HRTF variation creates delta without going through ASSR)
- **Breathing modulators** (create theta harmonics via nonlinear interaction, partially bypassing ASSR)
- **8 Hz NeuralLfo** (higher frequency = better AC transmission, still produces theta via boundary effects)

### Modulation strength vs. sonic pleasantness

The fundamental tradeoff remains but the strategies have evolved:

**Resolution strategies:**
1. **Reverb softens audible modulators** while simultaneously controlling the thalamic gate
2. **Distance reduces perceptibility** — sources at z < -2 with high reverb blend into ambience
3. **Multiple gentle drivers > one aggressive driver** — especially critical for ADHD where 4-5 sources at depth 0.30-0.40 beats 2 sources at depth 0.90+
4. **30 Hz NeuralLfo at depth 0.05 is imperceptible** but provides habituation resilience
5. **Stochastic bass at 3-5 spk/s sounds like natural rain**, not clicks
6. **Brown+Pink satellites** produce the best alpha:beta balance for flow state

### Score chasing vs. shipping

Still true: marginal score improvements above 0.70 have essentially zero value to a real user. Set a threshold and stop.

---

## Designing brain-type pairs (Normal + ADHD)

### The pair pattern

```
Normal preset = base (4-5 sources)
ADHD preset   = base (same sources, adjusted modulation) + perceptual additions
```

### ADHD design principles (updated)

ADHD operates near the JR bifurcation boundary (input_offset=135). Everything is amplified. The key insight from our experiments:

**"Multiple gentle drivers > one aggressive driver."** ADHD amplifies everything, so:
- Use **moderate modulation** (depth 0.30-0.40) across 4-5 sources
- NOT maximum modulation from 2 sources
- Each source adds a little push; together they create the desired neural state without any single source being overwhelming

**Brown anchor for vestibular safety.** Brown noise in ADHD presets is a perceptual choice — the low-frequency rumble provides vestibular grounding that ADHD users report as calming. This is NOT a neural effect (Brown's bass content barely registers in the model with global normalization). Keep it at vol 0.03-0.05 as an anchor.

### Different goals reverse the brain-type difficulty

| Goal | Easier on | Why |
|---|---|---|
| Isolation | Normal | Wants flat bands; ADHD over-produces slow waves |
| Focus / Shield | Normal | Wants beta dominance; ADHD has weaker beta |
| Deep Work | **ADHD** | Wants alpha+theta; ADHD naturally produces both |
| Meditation | **ADHD** | Wants theta+alpha co-dominant; ADHD's natural state is close |
| Sleep | **ADHD** | Wants slow-wave dominant; ADHD over-produces this |
| Deep Relaxation | **ADHD** | Wants slow-wave focus; ADHD's strength |
| Ignition | **Neutral** | ADHD needs activation (high FHN, gamma) which fights its natural bias |

### Right-hemisphere theta-lock on ADHD

If you put Breathing pattern 3 on the lead source of an ADHD preset, the right hemisphere can lock at 60%+ theta while the left stays balanced. With inhibitory coupling, this differentiation is even more pronounced than before.

**Fix:** Add a source on the LEFT side (drives right hemi via 65% contralateral routing) with NeuralLfo at alpha frequency (10 Hz). The inhibitory coupling helps — driving the right hemisphere toward alpha also suppresses theta in the left hemisphere via the inhibitory bilateral link.

**Rule: to fix a hemisphere, drive it from the opposite side** (65% contralateral routing).

---

## Validating against distractions

After hitting your target score, validate resilience:

```bash
cargo run --release -- disturb presets/your_preset.json \
  --brain-type normal \
  --spike-time 5.0 --spike-gain 0.8 --duration 15
```

| Resilience Score | Verdict |
|---|---|
| 0.95-1.00 | Excellent — disturbance has no lasting effect |
| 0.85-0.94 | Good — brief impact, fast recovery |
| 0.70-0.84 | Marginal — noticeable but recovers |
| < 0.70 | Bad — preset is fragile |

Habituation interacts with resilience: a habituated model may actually recover faster from disturbance (the depression provides a reset). Test at both short (15s) and long (60s) windows.

---

## Key files in this repo

- `src/scoring.rs` — Goal definitions, band targets, scoring formulas. `Goal::evaluate_full()` combines band score + FHN score + asymmetry penalty + carrier PLV bonus + envelope PLV bonus (CET). `entrainment_weight()` and `envelope_entrainment_weight()` hold the per-goal weights for the two PLV terms; `asymmetry_penalty()` holds the L/R lateralization penalty. Read this to understand what each goal measures.
- `src/brain_type.rs` — Brain type parameter definitions (input_offset, input_scale, bilateral params).
- `src/neural/jansen_rit.rs` — The cortical model. Inhibitory bilateral coupling, stochastic noise (sigma=15), habituation (synaptic depression), and the slow GABA_B parallel population (CET 13b) all live here. The Wendling 4-population state `[y0..y7]` is the canonical core; an additional 2-state slow inhibitory population `[y_slow_0, y_slow_1]` is integrated alongside via RK4 when `b_slow_gain > 0`. EEG = `y[1] - y[2] - y[3] - y_slow_0`.
- `src/auditory/thalamic_gate.rs` — Band-dependent arousal shift. The reverb→arousal mapping. `band_offset_shifts()` returns `[100%, 70%, 20%, 0%]` of max reduction across bands 0-3.
- `src/auditory/assr.rs` — DC/AC separation logic. ASSR frequency-dependent attenuation of the modulation envelope. When CET is enabled, only acts on the FAST path (slow path bypasses ASSR — Priority 13a).
- `src/auditory/crossover.rs` — **CET 13a:** 1st-order leaky integrator LP at 10 Hz with complementary HP. Splits the band envelope into a slow path (≤10 Hz, the cortical envelope tracking band per Doelling 2014) and a fast path (>10 Hz, the carrier modulation that ASSR processes). LP and HP sum to within one ULP.
- `src/auditory/gammatone.rs` — 32-channel gammatone cochlear filterbank grouped into 4 tonotopic bands.
- `src/neural/wilson_cowan.rs` — Adaptive frequency tracking within ±5 Hz Arnold tongue.
- `src/neural/fhn.rs` — FitzHugh-Nagumo single-neuron probe. 95th percentile scaling is applied in `pipeline.rs` before driving FHN.
- `src/neural/performance.rs` — Performance vector (entrainment ratio, E/I stability, spectral centroid, carrier PLV, envelope PLV). `compute_plv()` measures phase-lock against a synthetic sinusoid at the LFO frequency; `compute_envelope_plv()` measures phase-lock against the Hilbert phase of the slow auditory envelope (CET 13c). Both use the 2–9 Hz CET-relevant bandpass.
- `src/pipeline.rs` — Audio → cochlea → neural. Global max normalization (across all 4 bands) replaces per-band. 95th percentile FHN scaling clamped to [-3, 3]. CET bifurcate-recombine logic in steps 5d/5e/5f.
- `src/optimizer/differential_evolution.rs` — The DE optimizer.
- `src/main.rs` — CLI entry point. `evaluate` ships `--assr`, `--thalamic-gate`, and `--cet` off by default; `optimize` always runs ASSR + thalamic gate, CET is off pending optimizer-side validation.

---

## A short bibliography of mechanisms

These are real phenomena that the model approximates:

- **Contralateral auditory dominance** (~65/35 split): Kimura 1961, Penhune et al. 1996, Hackett 2011
- **Auditory steady-state response to AM**: Picton et al. 2003 — shows that amplitude-modulated noise can entrain cortex at the modulation frequency. The DC/AC separation is inspired by the distinction between sustained response and frequency-following response.
- **Berger effect / alpha desynchronization**: Berger 1929, Pfurtscheller & Lopes da Silva 1999
- **Gammatone cochlear modeling**: Patterson et al. 1992, Holdsworth et al. 1988
- **Jansen-Rit neural mass model**: Jansen & Rit 1995; Wendling extension 2002 (adds fast inhibitory population)
- **Phase-Locking Value**: Lachaux et al. 1999 — measures inter-trial phase coherence. Used here to score entrainment quality, weighted per goal (see `entrainment_weight()` in `scoring.rs`).
- **Arnold tongue / entrainment**: Pikovsky et al. 2001 — frequency locking region for driven oscillators. The WC model tracks within +/-5 Hz of its natural frequency.
- **Stochastic resonance in noise**: Moss et al. 2004, Faisal et al. 2008
- **Synaptic depression / habituation**: Tsodyks & Markram 1997 — short-term synaptic plasticity. Implemented as connectivity reduction under sustained activity.
- **Thalamic gating of cortical input**: Sherman & Guillery 2006 — thalamus modulates cortical input based on arousal state.
- **Inhibitory callosal projections**: Bloom & Hynd 2005, Yazgan et al. 1995 — corpus callosum contains both excitatory and inhibitory fibers.
- **EEG band conventions**: Niedermeyer & Lopes da Silva 2005

The model collapses these into a stochastic simulator. Real brains have all of this plus orders of magnitude more — top-down attention, neuromodulation, individual variability, network dynamics — none of which are captured. Use the model as a hypothesis generator, not a ground truth.
