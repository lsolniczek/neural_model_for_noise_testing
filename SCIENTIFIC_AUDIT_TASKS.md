# Scientific Audit — Fix Tasks

## High Priority

- [x] Fix bilateral hemisphere labels: Left = "α/β", Right = "δ/θ" (currently mislabeled as θ/γ and δ/β)
  - Files: `src/main.rs`, `src/brain_type.rs`, `src/pipeline.rs`
- [x] Raise ADHD band_offsets: low band to ~100-110 so spontaneous theta emerges without input
  - Changed: tonotopic `[80,75,70,65]` → `[110,95,80,70]`, bilateral left `→ [110,90,75,65]`, right `→ [100,85,70,60]`
  - Also raised global ADHD `input_offset` from 75 → 95
  - Files: `src/brain_type.rs` — `params()`, `tonotopic_params()`, `bilateral_params()`

## Medium Priority

- [ ] Raise Aging callosal coupling from 0.05 to 0.07
  - Literature supports ~20-30% reduction from normal (0.10), not 50%
  - File: `src/brain_type.rs` — `bilateral_params()` Aging section
- [ ] Acknowledge gamma limitation in scoring
  - JR model max frequency ~17 Hz (rates 140/70) — cannot produce 30+ Hz gamma
  - Option A: Remove gamma from Focus scoring targets (set weight to 0 or min/ideal/max to 0)
  - Option B: Add comment documenting the limitation, accept gamma score as always near-zero
  - File: `src/scoring.rs` — Focus goal gamma target

## Low Priority

- [ ] Make FHN time_scale brain-type-dependent
  - Currently hardcoded at 300 for all brain types
  - ADHD: ~350 (faster dopamine-driven temporal processing)
  - Aging: ~250 (slower dynamics)
  - File: `src/neural/fhn.rs` — add `time_scale` to `FhnParams` in `src/brain_type.rs`
- [ ] Consider adjusting Anxious b_gain from 19 to 20-21
  - Current model uses reduced inhibition; some anxiety phenotypes show increased GABAergic tone
  - Hyperexcitability could be driven more by a_gain=3.5 and c=145 alone
  - File: `src/brain_type.rs` — Anxious params
