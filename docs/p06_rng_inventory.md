# P-06 RNG inventory

This inventory is part of the reproducibility contract for NMM seed tree
revision `nmm_seed_tree_v1`. The root seed is never passed directly to a
consumer. Every production stream is derived from its complete typed address
with BLAKE3 context `com.noisegenerator.nmm.seed-tree.2026-09-05.v1`.

| Production consumer | Address | RNG / downstream contract | Status |
|---|---|---|---|
| Differential Evolution | `optimizer` | `ChaCha12Rng` (`rand_chacha 0.3.1`) | Seeded from the full 32-byte leaf |
| Dataset genome | `dataset_genome/sample_slot` | one `ChaCha12Rng` per original sample slot | Independent of scheduling and worker count |
| DSP renderer | `evaluation/panel/replicate/audio` | first 8 leaf bytes, little-endian, passed to `NoiseEngine::seeded` | DSP `dsp_seed_tree_v1`, revision `dsp_brown_hf_v2_binaural_beat_v1_seeded_v1` |
| Random-walk movement | `evaluation/panel/replicate/movement/object_slot` | one `ChaCha12Rng` per original preset slot | Phase is no longer used as entropy |
| Stochastic JR | `evaluation/panel/replicate/neural/hemisphere/band` | eight `ChaCha12Rng` streams; `box_muller_open01_v1` | left/right and bands 0–3 are distinct |
| Synthetic room impulse | `evaluation/panel/replicate/environment_rir` | `ChaCha12Rng` | Used by the legacy synthetic-RIR environments |
| Disturbance spike | `evaluation/panel/replicate/disturbance/hemisphere` | separate left/right `ChaCha12Rng` streams | Does not share state with JR |

The pinned DSP dependency is commit
`81b51fad005bb6522cbbf42ef08ca4a3c6c9ab06` (`v0.4.0`). Inside DSP, the
audio leaf is separated again into `anchor_left`, `anchor_right`,
`object_noise`, `object_bass_mod`, `object_satellite_mod`, and
`object_nature`, with original object slots in every object address. The two
anchor leaves implement the documented `dsp_correlation_v1` pair policy and
are not claimed to be independent channels.

The following fixed generators remain intentionally outside the schema-3
contract:

- `LegacyFixedV1` keeps the historical xorshift streams in JR, movement,
  synthetic RIR, and disturbance so schema-2 exports and the P-01 baseline
  replay their old semantics.
- `validate.rs` creates deterministic synthetic validation signals. Those
  samples are test fixtures whose outputs are protected by regression tests;
  they are not realizations of an evaluated preset.
- RNG snippets under `#[cfg(test)]` in crossover, performance, and surrogate
  tests create fixed test data only.
- Image-source room topology and deterministic filter/reverb coefficients are
  algorithm parameters. They do not represent a sampled room realization.
- FFT implementations and floating-point reductions are deterministic for the
  pinned canonical build, but are not random-number consumers.

Schema 2 always maps to `LegacyFixedV1`. Schema 3 records the run root, panel,
replicate index, NMM and DSP seed-tree revisions, ChaCha revision, normal
transform revision, and exact DSP source commit. A missing or mismatched
revision is a replay error.

The flat `seed_eval` CSV column stores the first 64 little-endian bits derived
from the typed evaluation address before selecting a consumer. It exists for
older readers only. The structured `(run_seed, panel, replicate_index,
seed_tree_revision)` columns are authoritative and collision-free within the
declared experiment contract.
