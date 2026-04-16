# Preset Instruction Guide: Composing Spatial Noise Presets for Focus, Relaxation, and Sleep

## 1) Purpose

This document is a practical, evidence-informed guide for composing **functional noise presets** (headphones / binaural / spatial render) for:

- Focus (fatigued neurotypical brain)
- Focus (ADHD profile)
- Relaxation
- Sleep

It combines results from psychoacoustics, auditory neuroscience, and soundscape research.  
Important: effects are typically **small-to-moderate and individual-dependent**; treat this as an optimization framework, not a medical intervention.

---

## 2) Quick Neuro Foundations

### 2.1 Brainwave target bands

| Band | Hz | Typical association |
|---|---:|---|
| Delta | 1–4 | Deep sleep |
| Theta | 4–8 | Drowsy, meditative |
| Alpha | 8–13 | Relaxed wakefulness |
| Beta | 13–30 | Alertness, focus |
| Gamma | 30–50 | High integration / cognitive binding |

### 2.2 ADHD vs fatigued neurotypical (design implications)

| Feature | ADHD profile | Fatigued neurotypical | Composition implication |
|---|---|---|---|
| Theta/Beta balance | Often elevated theta, reduced beta | Temporary alpha/theta drift | ADHD often needs stronger anti-habituation + stronger focus drive |
| Habituation speed | Faster | Slower | Use variation for ADHD (micro-drift, Lissajous, rate jitter) |
| Neural noise floor | Often functionally elevated | Typically normal | ADHD may prefer less broadband/high-frequency clutter |
| Best first target | Low-beta / beta / sometimes gamma | Alpha->low-beta ramp | Use gradual ramp for fatigue, stronger lock for ADHD |

---

## 3) Noise Colors: What to Use and Why

| Color | Spectral slope | Typical subjective effect | Practical use |
|---|---|---|---|
| White | Flat power/Hz | Bright, hissy | Short alerting layers only |
| Pink | -3 dB/oct | Balanced/natural | Best general-purpose base |
| Brown/Red | -6 dB/oct | Deep/soft/grounded | Good for calming + ADHD-friendly low-mid focus beds |
| Blue | +3 dB/oct | Sharp/airy | Use sparingly for "air" layers |
| Violet | +6 dB/oct | Very bright/fatiguing | Rarely useful therapeutically |
| Grey | Equal-loudness adjusted | Perceptually balanced | Excellent technical base if available |

**Rule of thumb:**
- Start with **Pink** for general use.
- For ADHD-focused scenes, test **Brown + Pink hybrids** first.
- Keep Blue/Violet as subtle canopy accents only.

---

## 4) Technique Comparison (when to choose what)

| Technique | Entrainment strength | Comfort | Needs headphones | Best use |
|---|---|---|---|---|
| Binaural Noise Tinting (BNT) | Medium | High | Yes | Long sessions, subtle entrainment |
| LFO/AM modulation | Medium-High | Medium | No | Practical robust entrainment |
| Isochronic tones | High | Low-Medium | No | Short strong state shifts |
| Stochastic "sparks" | Contextual | Medium | No | Attention reset / anti-habituation layer |

**Recommended hybrid (default):**
1. Colored noise base (Pink or Brown/Pink)
2. Subtle binaural tint for target band
3. Gentle AM/LFO at same target rate
4. Slow spatial motion + tiny rate jitter to reduce habituation

---

## 5) The Five Compositional Pillars

## Pillar 1 — Auditory Scene Analysis (stream separation)

The brain groups sounds by spectral similarity, timing, and location.  
If everything sits in same band + same position, the scene collapses to one mask.

**Rules**
- Separate layers in both **frequency** and **space**.
- Keep each layer’s role explicit (ground / organic / canopy / entrainment).

## Pillar 2 — Externalization (HRTF/Ambisonic over simple pan)

Simple stereo pan often feels in-head and fatiguing.

**Rules**
- Prefer HRTF or ambisonic render.
- Avoid long sessions with hard L/R-only static images.

## Pillar 3 — Distance & Peripersonal Space

Sounds perceived as very close can raise arousal.

**Rules**
- Relaxation/sleep: keep key sources perceptually **outside ~1 m** bubble.
- Simulate distance using lower direct sound, more diffuse/reverb, mild HF rolloff.
- Avoid repeated looming (approach) motion in calming presets.

## Pillar 4 — Motion grammar

Motion can reduce habituation and steer arousal.

| Motion type | Suggested rate | Effect |
|---|---:|---|
| Static + micro-drift | very slow random | Low distraction, anti-habituation |
| Pendulum arc | 0.05–0.15 Hz | Gentle organic movement |
| Orbit | 0.1–0.25 Hz | Breath-compatible, immersive |
| Fast orbit | >1 Hz | Often fatiguing/disorienting |
| Lissajous/figure-8 | 0.1–0.2 Hz | Good for ADHD anti-habituation |

## Pillar 5 — Spectral-Spatial grid

Use a natural mapping:
- **Low frequencies** (20–250 Hz): below/center, distant, wide (grounding)
- **Mid frequencies** (250 Hz–2 kHz): lateral ear level, gently moving (texture/information)
- **High frequencies** (2 kHz+): elevated/diffuse, low level (air/space)

---

## 6) Actionable Composition Checklist

1. Pick target state + band (focus beta, relax alpha/theta, sleep delta).
2. Select base color (Pink default; Brown/Pink for ADHD testing).
3. Build 3-layer spectral-spatial grid (ground/mid/canopy).
4. Externalize with HRTF/ambisonics.
5. Set safe distances (relax/sleep farther than focus).
6. Add movement:
   - Focus: micro-drift or slow arcs
   - ADHD focus: Lissajous + slight rate jitter
   - Relax/sleep: slow orbit or mostly static
7. Add entrainment layer:
   - BNT (difference = target Hz)
   - Optional gentle AM/LFO at same target
8. Decorrelate similar layers (different seeds/phases) to avoid collapse.
9. Apply anti-habituation (tiny variation every 30–120 s).
10. Validate with objective + subjective metrics.

---

## 7) Preset Templates

## 7.1 Focus preset — fatigued neurotypical

**Intent:** lift from alpha/theta drift into stable low/high beta focus.

| Layer | Color | Band | Position | Distance | Movement | Notes |
|---|---|---|---|---|---|---|
| Ground | Brown | 20–220 Hz | 0° / -20° | ~3–4 m | Static | masks low rumble |
| Mid-L | Pink | 250–1200 Hz | -40° / 0° | ~2–2.5 m | Arc 0.08 Hz | gentle texture |
| Mid-R | Pink (decorrelated) | 300–1500 Hz | +40° / 0° | ~2–2.5 m | Arc 0.10 Hz | anti-fusion |
| Canopy | Filtered white/blue | 2–8 kHz | diffuse / +25° | 4 m+ | Static | very low level |
| Entrain core | BNT in pink | e.g., 500L/518R | Center | ~1.5–2 m | Micro-drift | 18 Hz target |

**Session progression (recommended):** 10 Hz for 5–8 min -> 14 Hz for 5–8 min -> 18 Hz sustained.

## 7.2 Focus preset — ADHD profile

**Intent:** stronger anti-habituation + stronger attentional lock.

| Layer | Color | Band | Position | Distance | Movement | Notes |
|---|---|---|---|---|---|---|
| Ground | Brown | 20–180 Hz | 0° / -20° | ~3 m | Static | reduce broadband clutter |
| Mid-L | Brown/Pink | 150–900 Hz | -55° / 0° | ~2 m | Lissajous 0.15 Hz | novelty without chaos |
| Mid-R | Brown/Pink (decorrelated) | 180–1100 Hz | +55° / 0° | ~2 m | Lissajous 0.12 Hz | phase offset |
| Canopy | Grey or soft pink-high | 1.5–6 kHz | diffuse / +20° | 3.5 m+ | Micro-drift | keep level conservative |
| Entrain core | BNT | e.g., 220L/240R | near center | ~1.2–1.8 m | ±5° drift | beta/gamma candidate |
| Optional pulse | AM/isochronic-light | 16–20 Hz | inherited | inherited | inherited | low depth, short blocks |

**Cycle recommendation:** 20–30 min blocks with 2–5 min reset.

## 7.3 Deep relaxation preset

- Base: Pink + Brown
- Target: 8 -> 6 Hz (alpha to theta)
- Movement: very slow orbit 0.08–0.12 Hz
- Distance: mostly 3–6 m
- No looming approaches

## 7.4 Sleep preset

- Base: Brown-heavy with low pink canopy
- Target: 4 -> 2 Hz (theta to delta, gentle transition)
- Movement: near-static or extremely slow drift
- Distance: distant/diffuse only
- Avoid bright high-frequency components

## 7.5 Blank preset schema

Use this template for new presets:

| Layer | Color | Band | Position (az/el) | Distance | Movement | Entrain role | Notes |
|---|---|---|---|---|---|---|---|
| Ground |  |  |  |  |  |  |  |
| Mid-L |  |  |  |  |  |  |  |
| Mid-R |  |  |  |  |  |  |  |
| Canopy |  |  |  |  |  |  |  |
| Core |  |  |  |  |  |  |  |

---

## 8) NNM Testing Priorities (starting point)

1. **Color sweep at fixed target**: Pink vs Brown vs Pink/Brown hybrid.
2. **Movement sweep**: static, micro-drift, arc, orbit, Lissajous.
3. **Distance sweep**: near vs mid vs far for same spectrum.
4. **Technique ablation**: noise only, +BNT, +AM, +hybrid.
5. **Habituation analysis**: sustained 45–60 min session dynamics.
6. **Population split**: ADHD-labeled vs non-ADHD fatigued cohort.

Suggested metrics:
- EEG: target-band power, theta/beta ratio, ASSR/FFR proxies
- Behavioral: reaction time, CPT-style attention tasks, error rate
- Physiology: HRV, skin conductance
- Subjective: fatigue, focus quality, comfort, irritation

---

## 9) Parameter Reference

| Parameter | Start range |
|---|---|
| BNT carrier freq | 180–700 Hz (often most practical <=1 kHz) |
| BNT difference (focus) | 14–20 Hz (test 18 Hz first) |
| BNT difference (relax) | 6–10 Hz |
| BNT difference (sleep) | 2–4 Hz |
| Tint boost per ear | subtle (typically +1 to +6 dB local emphasis) |
| AM/LFO depth | gentle for long sessions |
| Orbit speed (calm) | 0.1–0.25 Hz |
| Fast-motion caution | >1 Hz often uncomfortable |

---

## 10) Core References (starting bibliography)

- Bregman, A. S. (1990). *Auditory Scene Analysis.*
- Blauert, J. (1997). *Spatial Hearing.*
- Oster, G. (1973). Auditory beats in the brain.
- Schwarz, D. W. F., & Taylor, P. (2005). ASSR to binaural/monaural beats.
- Gao, X., et al. (2014). EEG activity to binaural beat stimulation.
- Jirakittayakorn, N., & Wongsawat, Y. (2017). Theta responses to binaural beats.
- Garcia-Argibay, M., Santed, M. A., & Reales, J. M. (2019). Meta-analysis of binaural beats.
- Chaieb, L., et al. (2015). Auditory beat stimulation and cognition/mood.
- Söderlund, G., Sikström, S., & Smart, A. (2007). Noise benefits in ADHD.
- Arns, M., et al. (2013). Theta/beta ratio research in ADHD.
- Zahorik, P. (2002). Auditory distance perception in virtual acoustics.
- Tajadura-Jiménez, A., et al. (peripersonal space / spatial-emotional auditory work).

---

## 11) Safety, Ethics, and Practical Constraints

- Keep safe listening levels and session duration limits.
- Avoid using this as a substitute for medical ADHD care.
- Stop immediately if dizziness, headache, anxiety spike, or nausea appears.
- Validate every preset with objective metrics and user comfort, not theory alone.
