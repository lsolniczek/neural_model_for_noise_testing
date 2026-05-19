# Stage 8 Practical-A: Goal Semantics and Practical Evaluation Reports

## Objective

Make the NMM easier to use as a practical preset-design tool by attaching an explicit, machine-readable explanation to every goal and surfacing that explanation in evaluation outputs.

This stage is about **clarity**, not score changes.

## Why this is the next step

The current evaluator already computes rich diagnostics, but users still have to infer too much:

- what `shield` is actually trying to optimize,
- which outputs are product heuristics versus neural proxies,
- which claims the score does **not** establish,
- and how to interpret a good score safely.

For a practical NMM, this is more important now than adding another external-validation layer.

## Required outcome

For every `GoalKind`, the codebase should define one explicit semantics contract:

```text
goal_id
plain_language_purpose
product_objective
primary_neural_proxies
primary_acoustic_proxies
best_use_cases
unsupported_claims
evidence_level
```

The exact field names may differ, but all ideas above must be represented.

## Scope

### In scope

1. Add a typed goal-semantics structure in Rust.
2. Define semantics for all existing goals:
   - `focus`
   - `deep_work`
   - `sleep`
   - `deep_relaxation`
   - `meditation`
   - `isolation`
   - `shield`
   - `flow`
   - `ignition`
3. Surface those semantics in:
   - CLI `evaluate` output for single-goal runs,
   - machine-readable exported analysis metadata where practical,
   - documentation.
4. Add tests proving every goal has a complete semantics entry.

### Out of scope

1. No score changes.
2. No target-range retuning.
3. No goal renaming.
4. No new brain types.
5. No EEG/public-data changes.
6. No runtime default changes.

## Required semantics principles

### 1. Separate product objective from biological claim

Examples:

- `shield`
  - product objective: sustained masking-friendly focus support
  - not a validated claim that it improves human focus or blocks distraction in all users
- `isolation`
  - product objective: acoustic privacy / masking
  - not a validated claim of cognitive improvement
- `sleep`
  - product objective: sleep-onset friendliness / low-arousal profile
  - not a validated claim of slow-wave enhancement or memory improvement
- `ignition`
  - product objective: high-activation exploratory profile
  - not a validated ADHD treatment claim

### 2. Call current goals what they are

The current goal layer is a mix of:

- model heuristics,
- useful product objectives,
- and physiologically inspired proxies.

The semantics layer should say that plainly.

### 3. Keep evidence labels conservative

Recommended initial labels:

- `practical_model_heuristic`
- `component_supported_but_not_goal_validated`
- `requires_human_validation_for_efficacy_claim`

Do not use `validated` unless the repo already has the evidence to support it.

## Required implementation work

1. Add a reusable semantics struct, likely near `GoalKind` / `Goal` in `src/scoring.rs`.
2. Add a total mapping:
   - every `GoalKind` must return exactly one semantics entry.
3. Surface semantics in single-goal `evaluate` output:
   - short purpose line,
   - what the score means,
   - what it does not prove.
4. Add semantics into a machine-readable output surface:
   - preferred: export/evaluation JSON metadata if an appropriate structure already exists,
   - acceptable fallback: add a dedicated serialization helper and document where it is consumed.
5. Update docs:
   - `API_DOCUMENTATION.md`
   - optionally `BRAIN_MODEL_GUIDE.md` if needed for cross-reference.

## Required tests

1. Every `GoalKind::all()` entry has semantics.
2. No semantics field is empty.
3. Unsupported-claim lists are non-empty for every goal.
4. `sleep` explicitly says it does not establish slow-wave enhancement / memory benefit.
5. `shield` and `isolation` explicitly separate masking/product behavior from human cognitive efficacy.
6. Single-goal evaluate output includes semantics text.
7. Existing scores remain unchanged for representative presets.

## Acceptance criteria

Stage 8 Practical-A is complete when:

1. A caller can inspect any goal and immediately understand what it optimizes.
2. CLI output no longer makes the user infer product intent from target bands alone.
3. Unsupported claims are explicit.
4. Goal semantics are test-covered.
5. No scalar score or runtime behavior changed.

## Pseudocode

```rust
struct GoalSemantics {
    goal: GoalKind,
    plain_language_purpose: &'static str,
    product_objective: &'static str,
    primary_neural_proxies: &'static [&'static str],
    primary_acoustic_proxies: &'static [&'static str],
    best_use_cases: &'static [&'static str],
    unsupported_claims: &'static [&'static str],
    evidence_level: &'static str,
}

impl GoalKind {
    pub fn semantics(self) -> GoalSemantics {
        match self {
            GoalKind::Shield => GoalSemantics { ... },
            ...
        }
    }
}
```

CLI example:

```text
Goal meaning:
  Purpose: sustained masking-friendly focus support
  Score means: the preset matches the model's Shield proxies
  Does not prove: human focus benefit, ADHD treatment, universal distraction resistance
```

## Recommended implementation order

1. Add `GoalSemantics`.
2. Fill all nine goals.
3. Add tests for completeness and specific unsupported claims.
4. Surface semantics in single-goal evaluate output.
5. Add machine-readable serialization path.
6. Update docs last.
