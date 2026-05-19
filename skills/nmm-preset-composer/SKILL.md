---
name: nmm-preset-composer
description: Use when designing, reviewing, or tuning NMM noise presets for goals such as Shield, ADHD focus, isolation, deep work, relaxation, or sleep. Applies literature-constrained, source-first preset composition; avoids optimizer-first workflows; and preserves HRTF/spatial rendering when anchor bypass is not allowed.
metadata:
  short-description: Compose and review NMM noise presets
---

# NMM Preset Composer

Use this skill for practical NMM preset work: reviewing preset JSON, composing source recipes, evaluating against brain types/goals, and explaining scientific limits. The default mode is conservative and product-oriented: do not claim clinical efficacy, do not optimize blindly, and do not treat a high NMM score as proof of real human benefit.

## Core Rules

- Start from the user goal and constraints. If the user says no anchor, keep `anchor_volume: 0.0` because the anchor bypasses directional source/HRTF composition and can feel internalized.
- Prefer source-only HRTF-routed designs for product presets unless the user explicitly allows anchor-based diffuse beds.
- Do not use the optimizer unless the user explicitly asks for optimizer use. Manual composition comes first; optimizer output is only a later diagnostic or polishing tool.
- Use `BRAIN_MODEL_GUIDE.md`, `colored-noise-recipes.md`, and current preset evaluation reports before inventing a recipe.
- Treat NMM results as proxy diagnostics: band balance, firing regularity, arousal, masking features, and failure modes. Never say the preset will treat ADHD, improve focus, or guarantee distraction resistance.
- Reject recipes that score by exploiting model shortcuts but violate product constraints, such as skull-centered anchor-only sound, excessive SSN fatigue, harsh white/blue content, or weird optimizer artifacts.

## Standard Workflow

1. Inspect the target preset JSON and relevant docs.
2. Confirm constraints: goal, brain type, duration, anchor allowed or forbidden, optimizer allowed or forbidden, and whether comfort or masking matters more than score.
3. Run a baseline evaluation before editing:

```bash
cargo run --release --bin neural_preset_optimizer -- evaluate <preset.json> --goal <goal> --brain-type <brain> --duration 10 --json-report reports/<name>_<goal>_<brain>_10s.json
```

4. Read the score diagnostically, not literally. Focus on failed bands, hemispheric split, FHN rate/CV, arousal estimate, tonotopic input, dominant frequency, and practical report.
5. Make one small recipe change at a time. Use `apply_patch` for repo file edits.
6. Re-evaluate the changed preset at 10s for quick feedback. Only run 60s or 300s after the 10s behavior is not structurally broken.
7. If a candidate causes hemispheric split, alpha collapse, beta lock, delta blow-up, or harsh tonotopic imbalance, revert or branch. Do not keep tuning a structurally broken foundation.
8. Summarize with current metrics, what improved, what regressed, scientific rationale, and what remains uncertain.

## Source-Only Shield Guidance

When `anchor_volume` must remain `0.0`, Shield should be built as a spatial mask, not an anchor shortcut.

- Use 4-6 active HRTF sources.
- Use pink as the main bed for broad, less fatiguing masking.
- Add green for mid-band environmental masking and texture.
- Add low-level white only if more activation or masking is needed; avoid making white the main bed unless targeting ADHD/focus specifically.
- Avoid SSN as the dominant source-only carrier unless tests show no hemispheric split. In this NMM, SSN can work as an anchor but may split badly through directional HRTF sources.
- Keep direct low-reverb sources moderate for neural drive; use high-reverb satellites for spatial envelopment with lower direct neural impact.
- Use only one intentional beta driver if needed: `satellite_mod.kind = 4`, `param_a` around `14-18`, `param_b` around `0.10-0.30`. If beta overshoots or alpha collapses, remove it.
- Use a tiny 30 Hz NeuralLfo only as anti-habituation if needed: depth around `0.03-0.06`.

Common source-only Shield failures:

- High delta with low beta: too low arousal, too much reverberant/diffuse slow basin, or right hemisphere collapsing into slow rhythm.
- Beta lock with alpha collapse: too much SSN/white/direct beta modulation or too much bright mid-high energy.
- Hemispheric split: directional sources are driving left/right attractors differently; reduce asymmetry, reduce SSN dominance, or return to pink/green.
- Good 4s but bad 10s/60s: transient score only; do not ship until longer windows are stable.

## ADHD Guidance

- ADHD profiles often amplify input. Prefer several gentle sources over one aggressive driver.
- White noise has literature support for some inattentive/ADHD groups through stochastic resonance, but it can impair already attentive users. Keep white controlled and user-adjustable.
- Moderate activation is safer than harsh brightness. Target comfort and sustained listenability first.
- For ADHD Shield, watch firing rate and ISI CV. If score rises but CV is chaotic or sound is harsh, do not call it successful.

## Literature Anchors

Use these as component-level support, not proof that the complete preset works:

- Söderlund et al. 2010, background white noise and inattentive children: https://behavioralandbrainfunctions.biomedcentral.com/articles/10.1186/1744-9081-6-55
- Söderlund et al. 2007, noise and ADHD cognitive performance: https://pubmed.ncbi.nlm.nih.gov/17683456/
- Hongisto 2005, speech intelligibility and work performance: https://pubmed.ncbi.nlm.nih.gov/16268835/
- Klimesch 1999, EEG alpha/theta and cognition/memory: https://pubmed.ncbi.nlm.nih.gov/10209231/
- Engel & Fries 2010, beta oscillations and maintained cognitive state: https://doi.org/10.1016/j.conb.2010.02.015

## Reporting Template

When reporting a preset review or edit, include:

- Files changed.
- Evaluation command and duration.
- Score by brain type and goal.
- Band powers: delta, theta, alpha, beta, gamma.
- Main failure modes.
- Whether anchor was used.
- Whether optimizer was used.
- Practical conclusion in plain language.
- Next step, only if a concrete next step exists.

Keep the conclusion honest. Use phrases like “the model says”, “proxy alignment”, and “component-supported design heuristic”. Avoid “this improves ADHD”, “this will focus users”, or “scientifically proven preset”.
