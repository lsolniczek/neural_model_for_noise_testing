# Relax & Meditation Preset Plan

## Goal

Create **4 preset pairs** (8 presets total) for slow-wave-heavy goals:

- 2 pairs of **Deep Relaxation** (`relax_normal_v1` + `relax_adhd_v1`, `relax_normal_v2` + `relax_adhd_v2`)
- 2 pairs of **Meditation** (`meditation_normal_v1` + `meditation_adhd_v1`, `meditation_normal_v2` + `meditation_adhd_v2`)

Each "v1" and "v2" should be sonically distinct from each other (different dominant colors, environments, modulation strategies, movement patterns) — same approach we used for `deepwork_normal_v1` vs `deepwork_normal_v2`.

---

## Goal targets (from `src/scoring.rs`)

### Deep Relaxation
- Delta: 5-40% (ideal **22%**)
- Theta: 18-52% (ideal **35%**)
- Alpha: 20-52% (ideal **36%**)
- Beta: 0-14% (ideal 3%)
- Gamma: 0-6% (ideal 1%)
- FHN firing rate: 1-6 sp/s
- Brightness target: **0.0** (very dark — Pink/Brown)
- Reference: Klimesch 1999, Niedermeyer 2005

### Meditation (focused-attention)
- Delta: 2-22% (ideal 8%)
- Theta: 25-56% (ideal **40%**)
- Alpha: 25-56% (ideal **40%**)
- Beta: 0-12% (ideal 3%)
- Gamma: 0-8% (ideal 2%)
- FHN firing rate: 1-6 sp/s
- ISI CV target: 0.28
- Brightness target: **0.0** (dark — Pink dominant)
- Reference: Lomas et al. 2015 meta-analysis (theta/alpha in meditation)

**Both goals are theta+alpha heavy and want dark/warm spectrum.** Meditation is even more theta-heavy and has tighter delta limits than Deep Relaxation.

---

## Key design question: which brain type is the base?

**For Isolation and Focus** (which we built first), we used "Normal as base, ADHD adds satellites" because:
- Normal naturally produces flat/beta-favorable bands
- ADHD over-produces slow waves which Isolation/Focus penalize
- Adding aggressive beta-drive satellites for ADHD could counter its slow-wave excess

**For Deep Work** we kept the same pattern (Normal base, ADHD adds alpha-drive satellites), and it worked — `deepwork_adhd_v2` hit 0.81 by maxing out same-freq alpha NeuralLfo on close foreground satellites.

**For Relax and Meditation, the natural fit reverses dramatically:**

| | Normal brain | ADHD brain |
|---|---|---|
| Natural state | Deep alpha attractor (input_offset 175), can't produce theta past ~5-7% | Hypersensitive bifurcation (input_offset 135), naturally produces 30-50% theta |
| Distance to relax/meditation targets | **Far** (theta 5% vs target 35-40%) | **Close** (theta 30-40% naturally) |
| Achievable score | ~0.55-0.65 ceiling | **0.75-0.85+ likely** |
| Effort needed | Aggressive theta drivers throughout | Minimal — just gentle structure |

So the question is: **start with Normal and add for ADHD, or start with ADHD and add for Normal?**

### Option A: Normal base + ADHD adds satellites (the Deep Work pattern)

**How it would work:**
- Build a Normal base that pushes as hard as possible toward theta (multiple Breathing pattern 3 sources, high reverb, fast spatial movement, Pink/Brown dominant)
- That base will probably hit ~0.55-0.62 on Normal (theta-limited as we already documented)
- On ADHD, the same base will likely **over-drive** theta past max (50%+) because ADHD amplifies the aggressive slow-wave drivers
- ADHD satellites would then need to ADD alpha drive (max same-freq NeuralLfo at 10 Hz) to pull theta down into the 25-46% range — same trick we used in `deepwork_adhd_v2`

**Pros:**
- Consistent with the Deep Work pair design pattern (easier to maintain mental model)
- "Same base + satellite toggle" is clean for the engine
- We already have tooling/experience for ADHD alpha-drive satellites

**Cons:**
- The Normal base will be theta-limited regardless of effort
- We're fighting against Normal's nature on the harder version
- Wasted effort: aggressive theta drivers in the base get partially neutralized by ADHD satellites

### Option B: ADHD base + Normal adds satellites (reversed pattern)

**How it would work:**
- Build a minimal ADHD base that lets ADHD's natural slow waves emerge (gentle Pink/Brown sources, low modulation, comfortable reverb)
- That base will likely hit 0.75+ on ADHD with minimal effort (we saw ADHD's natural alignment with slow-wave goals)
- On Normal, the same base will UNDER-produce theta (alpha-locked at ~80%, theta at ~5%)
- Normal satellites would need to ADD aggressive theta drivers: multiple Breathing pattern 3 sources, high reverb, fast spatial movement (which we know drives delta but also theta-edge)

**Pros:**
- Matches each brain type's natural inclination
- ADHD version is minimal and clean (lower-effort design, fewer sources)
- Normal satellites are additive — they ADD slow-wave drivers rather than fighting alpha lock from inside the base

**Cons:**
- Inverts the pattern from Isolation/Focus/Deep Work pairs
- Normal version may still plateau (the satellites can only add so much before they distort the perceptual character)
- Different mental model for engine integration (ADHD is the "default" with Normal as the variant)

### Recommendation: Option B (ADHD base + Normal satellites)

**Rationale:**
1. ADHD's natural alignment with slow-wave goals means we'd be fighting Normal's nature in a Normal base. Better to fight Normal's nature only in the satellite layer where we have the most control.
2. Letting ADHD do what it does naturally produces a cleaner, sparser, more pleasant base preset.
3. The Normal satellites can be perceptual "richness" elements (extra sources with theta drive) that ADD to the base rather than overlap with it.
4. For Deep Relaxation/Meditation, the goals are about *unwinding* — the ADHD base should sound like a calm minimal soundscape, and the Normal satellites should add gentle activity (slow modulation, soft motion) that compensates for Normal's alpha-lock without making the sound busy.
5. Worst case: if Option B doesn't work well, we fall back to Option A and document the lesson.

**However**, this means the engine needs to understand "satellites added for Normal" not "satellites added for ADHD" — the inverse of the Isolation/Focus/Deep Work pairs. We should document this explicitly so future presets follow the right convention per goal-type.

---

## Build order

Build in this order so each step builds on the previous:

### Pair 1: Deep Relaxation v1 ("Forest Stillness")
- **Concept**: Pink-dominant, very quiet, OpenLounge environment for warm enveloping reverb. Static or imperceptible movement. Multiple Pink sources at different distances.
- **ADHD base** (`relax_adhd_v1`): 3-4 Pink sources, no aggressive modulation, gentle SineLfo at theta freq on 1-2 sources. Brown anchor at 0.10-0.15 for warmth. Target score: ≥ 0.75
- **Normal additions** (`relax_normal_v1`): Add 1-2 satellites with Breathing pattern 3 + high reverb + fast figure-8 movement to maximize theta drive. Target score: ≥ 0.55

### Pair 2: Deep Relaxation v2 ("Ocean Tide")
- **Concept**: Brown-dominant with Pink support. DeepSanctuary environment (RT60=2.5s) for cathedral-like depth. DepthBreathing movement on 1-2 sources for tidal feel.
- **ADHD base** (`relax_adhd_v2`): 3 sources, Brown lead with very slow SineLfo, Pink supports with DepthBreathing movement. Brown anchor 0.15. Target score: ≥ 0.75
- **Normal additions** (`relax_normal_v2`): Add 1-2 satellites with Breathing pattern 3 (slow tidal pulse), positioned in foreground to compete with the deep base. Target score: ≥ 0.55

### Pair 3: Meditation v1 ("Quiet Mind")
- **Concept**: Pink-dominant with subtle SSN element. FocusRoom environment (tight, intimate). Very slow movement for "settling" feel. Designed for breath-counting / focused-attention practice.
- **ADHD base** (`meditation_adhd_v1`): 3-4 sources, Pink lead with very gentle Breathing pattern 2 (Coherence 5-0-5-0, matches HRV training), 1 SSN source for ambience. Pink anchor 0.05-0.10. Target score: ≥ 0.75
- **Normal additions** (`meditation_normal_v1`): Add 2 satellites with Breathing pattern 3 + same-freq NeuralLfo at theta range (6-7 Hz) to push theta toward 15%+. Target score: ≥ 0.55

### Pair 4: Meditation v2 ("Open Sky")
- **Concept**: Mixed warm colors (Pink + Grey + Brown), DeepSanctuary environment for spaciousness, slow orbital movement for "expansive" feeling. Designed for open-monitoring practice.
- **ADHD base** (`meditation_adhd_v2`): 4 sources with mixed warm colors, slow orbit movement on 1-2 sources, Pink/Brown anchor. Target score: ≥ 0.75
- **Normal additions** (`meditation_normal_v2`): Add 1-2 satellites with stronger Breathing + fast figure-8 movement to drive theta. Target score: ≥ 0.55

---

## Per-pair workflow

For each pair, repeat this sequence:

1. **Design ADHD base** (3-4 sources, target ≥ 0.75 on ADHD)
   - Choose dominant color matching the v1/v2 concept
   - Set environment matching the concept
   - Add anchor (Pink or Brown at moderate volume)
   - Use gentle modulation that ADHD will naturally amplify into target bands
   - Test against ADHD goal, iterate
2. **Verify ADHD base on Normal** (expect ~0.45-0.55 — much lower)
   - Note which bands are off (theta will be low, alpha may be too high)
3. **Add Normal satellites** (1-2 satellites, target ≥ 0.55 on Normal)
   - Add aggressive theta drivers: Breathing pattern 3, slow SineLfo at 6-7 Hz, high reverb
   - Position satellites in foreground (z > 0) to compete with the base
   - Test against Normal goal, iterate
4. **Verify Normal satellites don't break ADHD** (ADHD score should stay ≥ 0.70 with satellites active — if it drops below, reduce satellite intensity)
5. **Match loudness** between Normal and ADHD versions using the loudness formula
6. **Test resilience** with the `disturb` command
7. **Document** what was added in the Normal version, with score progression

---

## Key constraints from prior lessons

(See BRAIN_MODEL_GUIDE.md for full context)

- **Theta on Normal is hard** — accept ~5-10% ceiling no matter what
- **Brown anchor at >0.10 crashes Isolation** — but for relax goals it's appropriate (we WANT alpha-feeding warmth)
- **Same-frequency bass+sat NeuralLfo at MAX depth** is the strongest single-band entrainment lever
- **Right hemisphere on ADHD locks to theta** when Breathing is on the lead — for relax goals this is GOOD, not bad
- **Per-object volume affects spectral balance** when colors differ
- **Master gain doesn't affect score** — only used for loudness calibration
- **Brightness peak for relax/meditation/sleep is 0.0** (very dark) — use Pink/Brown, avoid White/Blue/Grey
- **Position z > 0 = foreground** (audible character), z < -1 = background ambience
- **DepthBreathing movement** can conflict with bass modulators on the same object — pick one slow modulation source per object

---

## Open questions to resolve while building

1. Does the Normal-satellite-added approach actually work for relax goals, or do the satellites need to be too aggressive to be sonically pleasant? (If too aggressive, fall back to Option A: Normal base with theta drivers)
2. Should the Normal satellites also include alpha-frequency NeuralLfo at low depth to keep alpha in target range? (Normal already over-produces alpha, so this might not be needed)
3. For Meditation, is Breathing pattern 2 (Coherence 5-0-5-0) actually helpful, or does its 0.1 Hz cycle fall below the cochlear envelope filter cutoff?
4. Do we need to test against the Anxious brain type as a third leg of relax pairs? (Anxious is the brain that most needs relaxation but is hardest to relax)

---

## Files to create

```
presets/
  relax_adhd_v1.json         (build first)
  relax_normal_v1.json       (built on top of ADHD base)
  relax_adhd_v2.json         (different concept)
  relax_normal_v2.json
  meditation_adhd_v1.json
  meditation_normal_v1.json
  meditation_adhd_v2.json
  meditation_normal_v2.json
```

8 files, 4 pairs. Each pair shares the first N objects (the "base") and the Normal version adds objects N+1, N+2 as satellites with active modulation/movement.

---

## Definition of done

A pair is considered complete when:

- [ ] ADHD version scores ≥ 0.70 (target ≥ 0.75) on its target goal
- [ ] Normal version scores ≥ 0.55 (target ≥ 0.60) on its target goal
- [ ] ADHD score doesn't drop more than 0.05 when Normal satellites are toggled on (so the engine can switch via flag)
- [ ] Loudness between Normal and Normal/ADHD versions within 5% (use formula `mg × (sum_vol / √n + anchor)`)
- [ ] Resilience score ≥ 0.85 on both
- [ ] Sonically distinct from the v1/v2 sibling within the same goal
- [ ] Brightness near goal target (0.0-0.35 for these goals — measure and verify)
