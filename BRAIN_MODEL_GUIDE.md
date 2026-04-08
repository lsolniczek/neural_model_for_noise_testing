# Working with the Brain Model — Practical Guide

A field guide for designing presets against the bilateral Jansen-Rit + FHN neural simulator. This document captures what's load-bearing, what's noise, and where the model's preferences diverge from what real human listeners want.

---

## TL;DR for the impatient

1. **Trust the score directionally, not absolutely.** A 0.45 preset really is doing something different from a 0.75 preset, but the gap is exaggerated by ~5-10x compared to real EEG.
2. **Score below ~0.05 is noise.** Differences smaller than that are model jitter, not real improvements.
3. **Asymmetric stimulation scores higher than symmetric.** This is because the JR model has a strong alpha attractor that symmetric input feeds.
4. **Brightness is a free 10% of the score.** Use White, Grey, or Blue noise as your dominant carriers.
5. **Modulation type matters more than depth.** Different modulator kinds drive different EEG bands; cranking depth on the wrong kind doesn't help.
6. **Real human listeners care about pleasantness, not band flatness.** Trust your ears for the final selection.

---

## What the model actually measures

The pipeline is:

```
Audio → Cochlear filterbank (32 gammatone channels, 4 tonotopic bands)
      → Bilateral Jansen-Rit cortical model (left/right hemispheres)
      → FitzHugh-Nagumo single-neuron probe
      → Score against goal-specific targets
```

The score combines three components:

| Component | Weight | What it measures |
|-----------|:-:|---|
| **Band powers** | 72% (= 0.9 × 0.80) | How well EEG band powers match the goal targets |
| **FHN firing** | 18% (= 0.9 × 0.20) | Whether the single-neuron firing rate is in the target range |
| **Brightness** | 10% (= 0.1) | Spectral character of the audio (dark vs bright) |

For the **isolation** goal specifically, the band-power scoring uses a flat-deviation formula:

```
flatness = 1.0 - Σ|band - 0.20| / 2.0
```

i.e. all 5 bands (delta, theta, alpha, beta, gamma) should sit at exactly 20% each. Maximum flatness = 1.0 when every band is at 0.20.

---

## Score interpretation cheat sheet

| Score | Verdict | Practical meaning |
|---|---|---|
| 0.00–0.49 | POOR | Model is misbehaving — usually one band dominates >70% |
| 0.50–0.59 | OK | Acceptable for casual use, naive presets often land here |
| 0.60–0.69 | OK | Some intentional design is paying off |
| 0.70–0.79 | GOOD | Multiple mechanisms working together |
| 0.80–0.85 | GOOD | Probably near the realistic ceiling for most goals |
| > 0.85 | (rare) | Suspect overfitting to model quirks; verify subjectively |

**Important:** these are model scores, not human-perceived quality scores. A 0.50 pink-noise preset can feel more pleasant than a 0.80 optimizer preset. See "The score isn't perception" below.

---

## Mechanisms that actually move the score

These are what we discovered through experimentation, in rough order of impact.

### 1. Asymmetric spatial placement (HUGE impact)

The bilateral JR model has 65% contralateral input routing (sound on right → 65% of energy goes to left hemisphere). When you place a strongly-modulated source on **one side only**, the two hemispheres get driven into different oscillation modes:

- The contralateral hemisphere gets pulled out of its alpha attractor
- The ipsilateral hemisphere stays alpha-dominated
- The combined EEG has multiple frequencies → broader spectrum

Symmetric placement (same modulators on both sides) causes **both hemispheres to lock to alpha together**, crashing the score by 0.20+.

**Rule:** Always have at least one strongly-modulated source positioned off-center (e.g., x = ±2 to ±5).

### 2. Modulator kind selection

Each modulator kind drives different EEG bands through different mechanisms:

| Modulator | Best for | Why |
|---|---|---|
| **Breathing (kind 2)** | Theta (4-8 Hz) | Slow envelope curves create harmonic content in theta range via nonlinear interaction |
| **SineLfo (kind 1) at 1.5-3 Hz** | Delta (0.5-4 Hz) | Direct entrainment at fundamental |
| **NeuralLfo (kind 4) at 14-22 Hz** | Beta (13-30 Hz) | Direct entrainment, satellite_mod is most effective |
| **Stochastic (kind 3)** | Broadband temporal energy | Random pulse trains spread energy across bands |
| **Stochastic at slow rate (2-4 spikes/s, 250-350ms decay)** | Soft "rustling" texture | Sounds like distant rain on leaves rather than clicks — pleasant + still disrupts slow-wave entrainment |
| **NeuralLfo (kind 4) at 30+ Hz** | Gamma (mostly fails) | Model can't sustain gamma oscillations under Normal brain type |

**Critical insight we learned the hard way:** the **Breathing modulator** (especially pattern 3, Wim Hof) is the only mechanism we found that reliably produces theta. Hand-tuned presets without Breathing topped out at theta ~8%; with Breathing they reach theta ~17-20%. Always include at least one Breathing modulator if you want theta.

### 3. Reverb send → slow temporal structure

The reverb tail (especially in larger environments) creates slowly-decaying envelope variation that the model interprets as delta-band activity. High reverb sends (0.6-0.9) on the main objects boost delta by 10-15%.

But: pairing high reverb with a small environment (FocusRoom, AnechoicChamber) preserves modulation integrity. Pairing with large environments (DeepSanctuary) smears out the modulators and hurts everything else.

**Sweet spot:** AnechoicChamber + reverb_send 0.6-0.8 on main objects. The "anechoic" name is misleading — the reverb_send still produces audible reverberation via the engine's late-reverb FDN.

### 4. Movement speed and pattern

Fast spatial movement (figure-8 or pendulum at speed 3-5 rad/s) creates Doppler-like amplitude variation that:
- Adds slow temporal richness (boosts delta/theta)
- Decorrelates the bilateral HRTF input (preserves asymmetry)
- Sounds organic rather than static

Static or very slow movement (speed < 0.5) produces flat envelopes that feed the alpha resonance.

**Rule:** Use figure-8 or pendulum at speed 1.5-5 rad/s on at least 2-3 sources.

**Critical sub-lesson:** Fast spatial movement on a source can be a primary delta generator, even on sources with no bass modulation. We discovered this the hard way when "improving" a working preset by slowing down a fast figure-8 movement (speed 4.5 → 0.6) on an SSN source — delta crashed from 21% to 2.4% even though the bass modulators on other sources were unchanged. The fast movement was producing slow envelope variation via HRTF/distance changes that the cochlear model interpreted as low-frequency content. **Don't slow down fast-moving sources without testing.**

Also avoid stacking conflicting slow modulators: e.g., a source with SineLfo bass at 2 Hz combined with depth-breathing movement (which itself creates slow distance modulation) can produce destructive interference that flattens the delta drive. Pick one slow modulation source per object — either temporal (bass mod) or spatial (movement), not both.

### 5. Source position and perceptibility (z-axis matters)

The z-axis (depth) controls both the neural model behavior **and** the perceived foreground/background mix:

| z range | Audible role | Neural role |
|---|---|---|
| `z > +1` (in front of listener) | **Foreground** — audible character, you can localize them | Strong direct input, less reverb interaction |
| `z = 0 to -2` | Mid-distance | Balanced |
| `z < -2` (behind listener) | Background ambience | More reverb-mediated, blurrier modulation |

Sources placed deep behind (z < -3) tend to blend into the background and contribute to the overall texture without being individually audible. Sources placed in front (z > +1) become foreground "characters" that the listener will recognize as discrete sounds.

**Practical implication:** When you want a source to be **perceptible** to the listener (e.g., the new beta driver in an ADHD pair), put it at `z ≥ +1`. When you want it to **blend invisibly** into the soundscape (e.g., the slow-wave generators in the base), put it at `z ≤ -2`.

### 6. Spectral content (color choice)

The model's tonotopic bands have very different sensitivities:

- **Low (50-200 Hz)**: barely 7% of energy in most White-noise presets, has minimal influence
- **Low-mid (200-800 Hz)**: dominates with 35-50%
- **Mid-high (800-3 kHz)**: contributes 20-35%
- **High (3-8 kHz)**: usually under 20%

This means **bass-heavy noise (Brown, Black) wastes energy** on a band that the model doesn't weight much. Bright noise (White, Blue, Grey) puts energy where the model can use it.

For isolation specifically, brightness directly contributes 10% of the score:

```
brightness_modifier = 0.3 + 0.7 × brightness
```

So going from brown noise (brightness ≈ 0.0) to white noise (brightness ≈ 0.95) is worth +0.066 score points just from this term alone.

### Same-frequency on both bass and satellite — the most powerful entrainment lever we found

When you want to drive a specific EEG band reliably, putting the same frequency NeuralLfo on **both bass_mod and satellite_mod** of one source — and pushing depths toward maximum — is the strongest entrainment mechanism we've found.

```
bass_mod:      NeuralLfo at 10 Hz, depth 1.0
satellite_mod: NeuralLfo at 10 Hz, depth 0.90
```

The two modulators combined act on the source's full spectrum, creating a coherent envelope at the target frequency that the cochlear filterbank passes through to all tonotopic bands.

**The depth scaling is dramatic.** We measured score progression on a Deep Work ADHD preset where the only change was the same-freq bass+sat depths on a single satellite source:

| Depth (bass / sat) | ADHD Deep Work score |
|---|:-:|
| 0.60 / 0.50 | 0.587 |
| 0.70 / 0.55 | 0.630 |
| 0.85 / 0.70 | 0.743 |
| 0.95 / 0.80 | 0.794 |
| **1.0 / 0.90** | **0.809** |

**That's a +0.22 score swing from depth alone.** Applying this to BOTH a left-positioned satellite AND an overhead satellite (each at max depth) pushed the score to **0.814** with all four lower bands hitting their ideal targets (delta 3%, theta 31%, alpha 55%, beta 10%) — essentially landing on the goal's nominal targets.

This works best when:
- The source's color has content in both <250 Hz and >250 Hz ranges (White, Pink, SSN, Grey work well; Green/Blue have less bass content so the bass mod is wasted)
- The target frequency is at the edge of an EEG band you want to lift (e.g., 10 Hz to drive alpha, 14 Hz to drive beta-edge)
- The brain type's natural attractor is *opposite* the target — this trick is most powerful when fighting against a natural bias (e.g., driving alpha on ADHD which over-produces theta)
- You're willing to use depth ≥ 0.85 — below that, the lever is weak

**Common gotcha**: setting `param_b` (depth) on a `satellite_mod` whose `kind` is 0 (Flat) does nothing. The mod must have `kind: 4` (NeuralLfo) explicitly set. Always set both fields when enabling a modulator.

**Result observation**: with both satellites at max same-freq alpha drive on ADHD, the model's dominant frequency shifted from 7.2 Hz (theta-locked) to 10.0 Hz (alpha) — meaning the neural simulation actually moved out of its theta attractor into the alpha regime, not just rebalanced the band powers.

### 7. Master gain and object volume — partial story

This needs nuance — the relationship is more subtle than "volume doesn't matter":

**`master_gain` truly does not affect the score.** It's a global scalar applied after all source mixing and the band-normalization step. Two presets identical except for master_gain produce identical neural simulation results.

**Per-object volume DOES affect the score, but indirectly.** Per-object volumes are applied *before* band normalization. So per-object volumes affect the **spectral balance** of the mixed audio reaching the cochlear filterbank — not the absolute amplitude (which gets normalized away), but the *ratio* of contributions from different sources.

For example: if you have a Pink lead at vol 0.85 and a White counter at vol 0.55, the mix is dominated by Pink's 1/f spectrum. If you change to Pink at 0.55 and White at 0.85, the mix is now dominated by White's flat spectrum. The resulting brightness, tonotopic distribution, and neural response are different — even though `master_gain` is unchanged.

**Practical implication:**
- Don't tune `master_gain` to chase score — it does nothing
- Per-object volumes ARE worth tuning, especially when sources have different colors. The volume ratios change the effective spectral mix
- For same-color presets (e.g., all White), per-object volume has minimal effect on the score (it just shifts the spatial mix slightly via HRTF)

The pipeline still does max-normalization per tonotopic band before feeding the JR model:
```rust
normalized_band = raw_band / max(band)  // Always [0, 1]
```
But this normalizes only the absolute amplitude — the relative spectral shape (which depends on per-object volumes when colors differ) is preserved.

---

## Model quirks and limitations

These are things to be aware of so you don't chase impossible targets.

### The alpha attractor

The Jansen-Rit model is fundamentally a damped oscillator at ~10 Hz. For Normal brain type, the input offset is 175 pulses/s (well above the 120-150 bifurcation), placing the model deep in the alpha limit-cycle regime. **No amount of audio input will fundamentally change this** — the model wants to oscillate at alpha.

What you *can* do is redistribute power away from alpha by:
- Driving other bands actively (NeuralLfo at 14-22 Hz pulls the left hemisphere into beta)
- Creating slow envelope variation (Breathing/SineLfo bass + reverb create delta/theta sidebands)
- Using asymmetric placement so the two hemispheres run different rhythms

But you'll never see alpha drop below ~30-40% for a Normal brain type, no matter what you do. **Stop trying after a few iterations** — accept the floor and optimize the rest.

### Theta is nearly unreachable on Normal brain

Just like gamma, **theta (4-8 Hz) is severely limited on Normal brain type**. The JR Normal model with input_offset=175 sits deep in the alpha attractor, and the strongest theta-driving mechanism we have (Breathing pattern 3 on the lead) only produces ~5-7% theta — far short of any goal that wants 15-30% theta (Deep Work, Meditation, Sleep onset).

This means goals with high theta targets are **fundamentally unreachable on Normal brain**:

| Goal | Theta target | Best achievable on Normal |
|---|:-:|:-:|
| Deep Work | 15-46% | ~5-7% |
| Meditation | 25-56% | ~5-7% |
| Sleep | 28-68% | ~5-7% |

**Practical implication**: For these goals, **the ADHD brain type achieves them more naturally** because ADHD's hypersensitive bifurcation (input_offset=135) over-produces slow waves. A Deep Work preset on ADHD typically scores 0.05-0.10 higher than the same preset on Normal because ADHD is naturally closer to the target band distribution.

If your product needs to support a slow-wave-heavy goal across brain types, accept that **Normal brain will plateau around 0.60-0.66** while ADHD can reach **0.80+** with the right satellite design (specifically, max same-freq bass+sat NeuralLfo at alpha frequency on close foreground satellites — see "Same-frequency on both bass and satellite" above). Design the perceptual quality first, scores second on Normal — but on ADHD, this technique can land you at goal-ideal band powers.

### Gamma is essentially impossible

The model's fast inhibitory population (Wendling extension) can produce gamma oscillations in principle, but in practice gamma stays at ~0.3% across all reasonable presets. This costs ~14% of the maximum possible isolation score (= 0.20 - 0.003 ≈ 0.20 deviation × 0.72 weight).

**Don't optimize for gamma.** It's not happening. Treat it as a fixed loss.

### Theoretical isolation ceiling

Given gamma ≈ 0 and alpha ≥ 35%, the realistic best-case math is:

```
delta=0.25, theta=0.20, alpha=0.35, beta=0.20, gamma=0.00
deviation = 0.05 + 0.00 + 0.15 + 0.00 + 0.20 = 0.40
flatness = 1.0 - 0.40/2.0 = 0.80

total = 0.9 × (0.80 × 0.80 + 0.20 × 1.0) + 0.1 × 1.0
      = 0.9 × 0.84 + 0.10 = 0.856
```

So **the realistic ceiling for isolation on Normal brain type is ~0.85**. Anything claiming higher is probably overfitting or measuring at a short evaluation window.

### Brain-type-specific behavior is dramatic

The same preset can score very differently across brain types because the underlying JR parameters (input_offset, input_scale, band_offsets, GABA gains) are tuned per profile:

| Brain type | input_offset | Operating regime |
|---|:-:|---|
| Normal | 175 | Deep alpha limit cycle |
| HighAlpha | 195 | Even deeper alpha |
| ADHD | 135 | Right at bifurcation boundary, hyper-sensitive |
| Aging | 165 | Slower dynamics |
| Anxious | 220 | Beta-biased |

**ADHD is fundamentally different** — it operates near the bifurcation boundary, which means small modulation moves it dramatically. ADHD presets need 40-50% less modulation depth than Normal presets. ADHD often gets natural delta/theta from the hypoaroused state and needs *suppression* of slow waves, not *promotion*.

**Practical implication:** A preset optimized for one brain type won't necessarily work for another. Build a pair (e.g., Normal + ADHD) by keeping the structural skeleton identical and varying only the modulation depths and rates.

### Score noise floor

The pipeline has chaotic-ish dynamics. Running the same preset twice produces scores that vary by ±0.02. Running at different durations produces drift of ±0.05. **Differences below 0.05 are not significant** — they're within the noise floor.

**Practical implication:** Don't agonize between a 0.78 and a 0.79 preset. They're the same. Pick the one that sounds better.

### Length-of-evaluation matters

Short evaluations (4-8 seconds) inflate scores because the JR model hasn't fully settled. Long evaluations (60+ seconds) are more reliable but slower.

| Duration | Use case |
|---|---|
| 4-8 s | Optimizer search (fast iteration) |
| 12-20 s | Quick spot checks |
| 60 s | Final validation, A/B comparison |

The optimizer typically runs at 12-20 s for speed; **always re-evaluate the winning preset at 60 s** before declaring victory.

---

## The score isn't perception

This deserves its own section because it's the biggest pitfall.

The score measures one narrow thing: **how closely the simulated EEG band powers match a target distribution**. This is:

1. Computed from a heavily simplified neural model
2. Targeting an engineering construct (e.g., "flat band powers") that doesn't correspond to any natural cognitive state
3. Insensitive to audio quality, pleasantness, fatigue, habituation, attention, or context
4. Indifferent to the L/R balance you actually hear (because of the band-normalization step)

Real human listeners care about completely different things:
- Spectral comfort (no harsh peaks, no muddy buildup)
- Stationarity (sudden changes break attention)
- Stereo balance (asymmetric output is jarring)
- Spatial naturalness (artifacts from extreme positions)
- Total broadband energy (the louder the masker, the better the masking)

A preset optimized purely for the score will frequently:
- Sound asymmetric (the optimizer's preferred lateralized strategy)
- Have weird modulation artifacts (high-depth Breathing patterns audible as pulsing)
- Sound thin (because Brown/bass colors hurt brightness)
- Use silent "ghost" objects that contribute nothing audibly but somehow tweak the score

**Workflow recommendation:** treat the score as a research signal, not a product metric. Use it to explore parameter space and identify mechanisms, then trust your ears for the final selection.

---

## Practical workflow

Based on what worked for us, here's a recommended process:

### 1. Establish a baseline

Start with a simple, safe preset:
- 3-4 White noise sources
- Spread across the listener (not all in one spot)
- Light SineLfo or NeuralLfo modulation
- Low to moderate reverb
- Master gain whatever sounds comfortable (it doesn't affect score)

Evaluate at 60 s. You'll typically land at 0.45-0.55 for isolation. This is your floor.

### 2. Add specific mechanisms one at a time

Only add one new mechanism per iteration so you can attribute score changes:

1. Add a **NeuralLfo at beta** (14-22 Hz) on satellite_mod of one off-center source → +0.05-0.10
2. Add a **SineLfo bass at 1.5-2 Hz** to a few sources → +0.05 (delta lift)
3. Add a **Breathing modulator** (kind 2, pattern 3) → +0.05-0.10 (theta lift)
4. Increase **reverb sends** to 0.6-0.85 → +0.03-0.05 (slow temporal structure)
5. Add **fast figure-8 or pendulum movement** → +0.02-0.04
6. Verify **brightness ≥ 0.6** (use White-dominant carriers) → +0.02-0.03

Each step should produce a measurable improvement. If a step produces nothing, the mechanism isn't engaging — try changing where it's placed or which modulator slot it occupies.

### 3. Use the optimizer as a discovery tool

The optimizer is good at finding non-obvious parameter combinations. Use it like this:

```bash
cargo run --release -- optimize --goal isolation --brain-type normal \
  --generations 80 --population 40 --duration 20 \
  --init-preset presets/your_best_handcrafted.json
```

The `--init-preset` flag is crucial — seeding from a good baseline helps the optimizer refine rather than re-discover.

**Caveat: DE is path-dependent.** Differential evolution gets stuck in local optima, and the basin of attraction is determined by the seed preset. Two runs seeded from different starting points will converge to different solutions, sometimes with score gaps of 0.10+. We observed:

| Seed | Score | Strategy DE found |
|---|:-:|---|
| NeuralLfo + max-depth preset | 0.74 | Beta-dominant (β=55%, θ=2%) |
| SineLfo + Breathing + reverb preset | 0.84 | Balanced (all bands except γ pass) |

If you're not getting the score you expect, **try seeding from a different starting point** rather than running more generations on the same seed. Running multiple short optimizer passes from different seeds is usually more productive than one long pass from a single seed.

After it finishes:
1. Re-evaluate at 60 s to verify the score isn't a short-window artifact
2. Inspect the result for **silent ghost objects** (volume = 0.0). Remove them — they're optimization artifacts.
3. Check brightness — if the optimizer chose a dark anchor (Brown), consider whether the +0.03 brightness loss is worth it
4. Listen to it. If it sounds unbalanced or weird, manually rebalance (see "Tradeoffs" below)

### 4. Validate against the real goal

The model score is one signal. Before declaring a preset done, ask:
- Does it sound pleasant for 5+ minutes of continuous listening?
- Does it actually mask environmental sound at normal listening volume?
- Does it feel restful or annoying?
- Does it work for the actual user (not just on the model)?

If the answers are bad, **lower-scoring presets are often better products**. A 0.65 preset that listeners actually use is more valuable than a 0.85 preset that they switch off after a minute.

---

## Common tradeoffs and how to manage them

### Symmetry vs. score

The optimizer prefers asymmetric placement because it breaks the alpha attractor. Real listeners prefer stereo-balanced sound.

**Resolution:** Keep the main asymmetric "lead" source (the one with Breathing or NeuralLfo) on one side, but mirror lower-impact sources to the opposite side for stereo balance. Expect to lose ~0.05 score versus the pure optimizer output.

### Anchor color choice — Green beats Brown for warmth

If you want an audible warm/character anchor without crashing the score, **prefer Green (id 3) over Brown (id 2)**:

| Anchor color | At vol 0.10 | Why |
|---|:-:|---|
| **Brown** (1/f² spectrum) | Score crashes from 0.75 → 0.53 | Brown's deep bass floods the Low tonotopic band, alpha jumps from 49% to 80% |
| **Green** (bandpass ~500 Hz) | Score holds at 0.70+ | Green's mid-range energy feeds Low-mid which the JR model uses for beta production. Alpha stays controlled (~45%) |

Green at 0.10 anchor volume:
- **Boosts beta** in both Normal (+15%) and ADHD (+6%) brain types
- Adds clearly audible mid-range warmth that listeners describe as "soft hum" or "drone"
- Drops brightness gently (0.62 → 0.58) instead of crashing it
- For ADHD specifically, Green pushed beta from 11% (WARN) to 17% (PASS) — solving a previously stubborn ADHD weakness

**Rule of thumb**: when you want an anchor color that's clearly audible, use Green at 0.05-0.15. Reserve Brown for very subtle warmth at vol ≤ 0.03.

### Brown/warmth vs. brightness score

Brown noise is comforting and natural-sounding but kills the brightness term and floods the Low tonotopic band, feeding alpha resonance.

**Resolution:** Use Brown only as the **anchor** at very low volume (0.05-0.10). Don't use Brown as a primary source. For warmth, prefer Pink (1) at moderate volume — it has more midrange energy than Brown.

### Color variety vs. spectral simplicity

The model rewards bright, broadband energy (favoring White). Real listeners get bored with pure White noise.

**Resolution:** Use White as the primary carrier (3-4 sources) but add 1-3 lower-volume "color spots" (Pink, Grey, SSN) for character. Keep colored sources at ≤1/3 the volume of the White carriers — the model is dominated by the loudest sources, so the colored ones add character without distorting the spectral balance.

### Optimizer "ghost" objects

The optimizer sometimes leaves objects with `active: true, volume: 0.0`. These contribute nothing audibly but somehow influence the modulator state machinery. They're optimization artifacts.

**Resolution:** Always clean up ghost objects manually. Set them to `active: false`. Re-evaluate to verify the score didn't move (usually it doesn't).

### Modulation strength vs. sonic pleasantness

There's a hard physical tradeoff here that you'll keep running into:

- **Aggressive modulators that move the score audibly affect the sound:**
  - High-depth NeuralLfo at beta range (>0.7 depth) creates a noticeable wobble/tremolo
  - Stochastic at high rate (>10 spikes/s) sounds clicky
  - Fast spatial movement (speed >3) creates audible "swooshing"
  - Breathing at full depth produces clearly perceptible amplitude pulsing
- **Subtle modulators that sound smooth don't move the score much:**
  - Low-depth NeuralLfo (<0.3) is almost inaudible but barely entrains the model
  - Slow movement (speed <0.5) is imperceptible
  - Static sources are pleasant but feed the alpha attractor
  - Flat modulators contribute nothing neurally

**Resolution strategies:**

1. **Reverb softens audible modulators.** A high reverb send (0.6+) blurs the temporal artifacts of NeuralLfo and Stochastic, making them less noticeable while preserving the neural effect. The reverb tail also adds slow envelope structure that helps delta.
2. **Distance reduces perceptibility.** Sources at z < -2 with high reverb feel like ambience; their modulation artifacts blend into the background.
3. **Multiple gentle drivers > one aggressive driver.** Two sources at moderate modulation depth often sound better than one with maxed-out modulation, even at the same total energy.
4. **Frequency choice matters for audibility.** NeuralLfo at 25-30 Hz is harder to perceive as wobble than 10-15 Hz (the human ear smooths fast tremolo into texture). Use upper-beta frequencies if you need stealth modulation.
5. **Stochastic at slow rate (2-4 spikes/s) sounds like soft rustling, not clicks.**

### Score chasing vs. shipping

It's easy to spend hours pushing from 0.78 to 0.82. The marginal value to a real user is essentially zero.

**Resolution:** Set a score threshold at the start (e.g., "≥ 0.70 GOOD verdict") and stop optimizing once you cross it. Spend the rest of your time on perceptual quality and product testing.

---

## Goal-specific brightness targets

Each goal applies a brightness modifier worth 10% of the total score. The targets are **very different per goal** and are easy to overlook:

| Goal | Brightness curve | Peak | Practical color |
|---|---|:-:|---|
| **Isolation** | Linear `0.3 + 0.7×brightness` | **1.0** (brighter = better) | White / Blue / Grey |
| **Focus** | Inverted-U around 0.55 | **0.55** | White-Pink mix |
| **Sleep** | `1.0 - 0.8×brightness` | **0.0** (darker = better) | Brown / Black |
| **Deep Relaxation** | `0.9 - 0.6×brightness` | **0.0** | Brown / Pink |
| **Meditation** | `0.85 - 0.5×brightness` | **0.0** | Pink / Brown |
| **Deep Work** | Inverted-U around 0.35 | **0.35** | Pink-dominant |

This explains why the same color works for some goals and not others:

- **White noise** is perfect for Isolation but terrible for Sleep / Deep Work / Deep Relaxation — it's too bright
- **Brown noise** is great for Sleep but kills Isolation (also crashes alpha)
- **Pink** is the universal middle-of-the-road — works decently for Deep Work, Meditation, and Deep Relaxation
- **Pink-dominant with one brighter source** hits Focus's 0.55 target naturally

**Practical implication**: when starting a new goal, **pick your dominant color based on the goal's brightness target** before tuning anything else. Trying to make White work for Sleep, or Brown work for Isolation, fights against a 10% built-in penalty you can't recover with neural tuning.

## Designing brain-type pairs (Normal + ADHD)

A common product requirement is to ship a preset that "works for both Normal and ADHD users." Don't try to satisfy both in one preset — the brain types have fundamentally different operating regimes and require opposite modulation strategies. Instead, build a **pair**: a base preset that works for one brain type, plus a "satellite layer" that adapts it for the other.

### The pair pattern

```
Normal preset = base (4 sources)
ADHD preset   = base (same 4 sources) + 1-2 added "satellite" sources
```

The base provides the structural skeleton (positioning, main modulators, environment, anchor). The satellites are activated only for the variant brain type. The engine can switch between presets by toggling the satellite source(s) on/off.

### Different goals reverse the brain-type difficulty

The Isolation goal we designed first happens to be **harder for ADHD than Normal** because ADHD over-produces slow waves while Isolation wants flat bands. But this is goal-specific:

| Goal | Easier on | Why |
|---|---|---|
| Isolation | Normal | Wants flat bands; ADHD over-produces slow waves |
| Focus | Normal | Wants beta dominance; ADHD has weaker beta |
| Deep Work | **ADHD** | Wants alpha+theta; ADHD naturally produces both |
| Meditation | **ADHD** | Wants theta+alpha co-dominant; ADHD's natural state is close |
| Sleep | **ADHD** | Wants slow-wave dominant; ADHD over-produces this |
| Deep Relaxation | **ADHD** | Wants slow-wave focus; ADHD's strength |

**Practical implication**: When designing a brain-type pair, identify which brain type is the "natural fit" for the goal first, then design the *harder* brain type's adaptation as the satellite layer. For Deep Work / Meditation / Sleep / Relaxation, this means **building for ADHD as the base and adding satellites for Normal** — the inverse of the Isolation pattern.

For Deep Work specifically, we observed Normal hitting a ~0.66 ceiling while ADHD reaches 0.67+ on the same base preset. Same architecture, different scores, because the goal aligns with ADHD's natural EEG distribution.

### Right-hemisphere theta-lock on ADHD with Breathing modulator

If you put Breathing pattern 3 (Wim Hof) on the lead source of an ADHD preset, you'll likely see the **right hemisphere lock at 60%+ theta** while the left hemisphere stays balanced. The bilateral split looks like:

```
Left (fast α/β):   theta 30%, alpha 30%, beta 28%  ← balanced
Right (slow δ/θ):  theta 64%, alpha 29%, beta  5%  ← LOCKED
Combined:          theta 50%, alpha 30%, beta 12%
```

This is because:
1. ADHD's right hemisphere has slow-wave bias (band_offsets pushed lower)
2. ADHD's hypersensitive bifurcation amplifies any slow modulation
3. Breathing pattern 3's 0.33 Hz cycle creates harmonic content that the right hemi locks onto

**Fix**: Add a satellite source on the **LEFT side** (which drives the right hemi via 65% contralateral routing) with NeuralLfo at alpha frequency (10 Hz). This drives the right hemisphere out of theta-lock toward alpha. We measured combined alpha jumping from 30% to 44% with this single satellite, and the dominant frequency shifting from 7.2 Hz (theta) to 10.0 Hz (alpha).

The pattern: **to fix a hemisphere, drive it from the opposite side** (because of the 65% contralateral routing).

### Why ADHD needs added energy for fast-band goals

For goals that want **alpha-light or beta-rich** brain states (Isolation, Focus), ADHD's slow-wave bias is a problem. ADHD's input_offset (135) sits right at the JR bifurcation boundary, so any modulation in the audio gets dramatically amplified. This means:

- **Normal-tuned slow modulators** (Breathing, SineLfo at 1.5-3 Hz) that produce ~12% theta in Normal will produce **~37% theta in ADHD** — runaway slow-wave excess
- The base preset's strengths for Normal (delta+theta promotion via slow modulation, reverb tails, etc.) become **liabilities for ADHD when targeting fast bands**

So an ADHD variant for Isolation/Focus needs to **inject opposing energy** to counter-balance the over-driven slow waves:

- **Beta drivers** — NeuralLfo at 18-25 Hz on satellite_mod, positioned on the right side (drives left hemisphere via 65% contralateral routing). The left hemisphere is fast/β-prone and accepts beta entrainment readily.
- **Broadband disruption** — Stochastic at moderate rate breaks up the rhythmic coherence of the slow waves
- **Foreground positioning** — put satellites at z > +1 so they sit above the base preset's background, audibly competing with it

For **slow-band goals** (Deep Work, Meditation, Sleep), the opposite is true: ADHD doesn't need slow-wave addition (it has plenty), it needs **alpha rebalancing** to bring its over-produced theta into the target range. See the right-hemisphere theta-lock fix above.

### The pleasantness vs strength tradeoff for ADHD

This deserves explicit warning: **subtle modulation does not work for ADHD**. Because ADHD amplifies modulation, any subtle effect gets swallowed by the slow-wave excess. You need either:

1. **Aggressive modulation** that audibly adds character (clicks, fast tremolo) — gets the score up but is sonically noticeable
2. **Multiple smaller drivers** spread across positions, each subtle but adding up — sounds smoother but uses more sources

Option 2 is usually what users want. The pattern that worked for us: **2 added sources, both at z > +1, one driving beta on the right, one adding texture on the left for stereo balance.** Each source individually is moderate, but together they pull delta from 37% to 20% on ADHD.

### Verify both directions

After designing a pair, evaluate both presets on **both brain types** to confirm:
- Normal preset on Normal brain: should score well (target: GOOD)
- Normal preset on ADHD brain: usually scores OK-to-GOOD (often around 0.70-0.74) because ADHD is more permissive
- ADHD preset on ADHD brain: should score better than Normal preset on ADHD (target: GOOD with higher score)
- ADHD preset on Normal brain: should not break (the satellite shouldn't actively hurt Normal)

The asymmetry of these results — Normal-on-ADHD often being decent — reflects that ADHD's wider parameter sensitivity means it tolerates a wider range of stimuli, while Normal is more selective about what flattens its EEG. Don't be surprised when a Normal preset accidentally scores fine on ADHD; the meaningful test is whether the dedicated ADHD variant scores **better** than the unmodified Normal preset.

---

## Validating against distractions

After hitting your target score, validate that the preset can actually withstand acoustic interruptions — the whole point of isolation is being resilient to disturbances. Use the `disturb` command:

```bash
cargo run --release -- disturb presets/your_preset.json \
  --brain-type normal \
  --spike-time 5.0 --spike-gain 0.8 --duration 15
```

This injects a 50ms acoustic spike at t=5s and measures how quickly the neural entrainment recovers. The key output is the **Resilience Score** (0-1):

- **0.95-1.00**: Excellent — disturbance has no lasting effect
- **0.85-0.94**: Good — brief impact, fast recovery
- **0.70-0.84**: Marginal — noticeable but recovers
- **< 0.70**: Bad — disturbance disrupts the desired state significantly

Both presets in our final pair (Normal and ADHD variants of `isolation_*_clean.json`) score 0.98-1.00 on resilience, meaning they fully recover from a strong acoustic spike within ~50ms. **A high isolation score with low resilience means the preset is fragile** — it works in steady state but breaks on the first noise. Always validate both metrics.

You can also test with stronger spikes (`--spike-gain 1.0 --spike-duration 0.2`) to stress-test against larger interruptions like a slammed door.

---

## Key files in this repo

- `src/scoring.rs` — Goal definitions, band targets, scoring formulas. Read this to understand what each goal actually measures.
- `src/brain_type.rs` — Brain type parameter definitions. The differences between Normal/ADHD/HighAlpha/etc. live here.
- `src/neural/jansen_rit.rs` — The cortical model. Look at `simulate_bilateral` to understand the L/R hemisphere routing.
- `src/pipeline.rs` — Audio → cochlea → neural. The `normalized_band = raw_band / max(band)` line at ~217 is the reason volume doesn't affect score.
- `src/optimizer/differential_evolution.rs` — The DE optimizer. Read this if you're tuning optimizer hyperparameters.

---

## A short bibliography of mechanisms (for your own reference)

These are real phenomena that the model approximates, in case you want to read primary sources:

- **Contralateral auditory dominance** (~65/35 split): Kimura 1961, Penhune et al. 1996, Hackett 2011
- **Auditory steady-state response to AM**: Picton et al. 2003 — shows that amplitude-modulated noise can entrain cortex at the modulation frequency
- **Berger effect / alpha desynchronization**: Berger 1929, Pfurtscheller & Lopes da Silva 1999
- **Gammatone cochlear modeling**: Patterson et al. 1992, Holdsworth et al. 1988
- **Jansen-Rit neural mass model**: Jansen & Rit 1995; Wendling extension 2002 (adds fast inhibitory population)
- **Stochastic resonance in noise**: Moss et al. 2004, Faisal et al. 2008
- **EEG band conventions**: Niedermeyer & Lopes da Silva 2005

The model collapses these into a deterministic simulator. Real brains have all of this plus orders of magnitude more — top-down attention, neuromodulation, individual variability, network dynamics — none of which are captured. Use the model as a hypothesis generator, not a ground truth.
