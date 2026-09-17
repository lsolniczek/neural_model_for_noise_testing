//! Versioned, domain-separated random streams used by reproducible NMM runs.
//!
//! Every stream is derived directly from the run root and a typed full address.
//! Call order therefore cannot move another consumer onto a different stream.

use rand::SeedableRng;
use rand_chacha::ChaCha12Rng;
use serde::{Deserialize, Serialize};

pub const SEED_DERIVATION_REVISION: &str = "nmm_seed_tree_v1";
pub const SEED_TREE_CONTEXT: &str = "com.noisegenerator.nmm.seed-tree.2026-09-05.v1";
pub const RNG_ALGORITHM_REVISION: &str = "chacha12_rand_chacha_0.3.1";
pub const NORMAL_TRANSFORM_REVISION: &str = "box_muller_open01_v1";

const DOMAIN_OPTIMIZER: u64 = 1;
const DOMAIN_DATASET_GENOME: u64 = 2;
const DOMAIN_EVALUATION: u64 = 3;

const CONSUMER_AUDIO: u64 = 1;
const CONSUMER_MOVEMENT: u64 = 2;
const CONSUMER_NEURAL: u64 = 3;
const CONSUMER_ENVIRONMENT_RIR: u64 = 4;
const CONSUMER_DISTURBANCE: u64 = 5;

pub const MAX_OBJECT_SLOTS: u32 = 8;
pub const NEURAL_BANDS: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u64)]
pub enum SeedPanel {
    Direct = 1,
    Search = 2,
    Finalist = 3,
    Dataset = 4,
    Disturb = 5,
}

impl SeedPanel {
    const fn id(self) -> u64 {
        self as u64
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u64)]
pub enum Hemisphere {
    Left = 0,
    Right = 1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum SeedPolicy {
    #[default]
    LegacyFixedV1,
    DomainSeparatedV1 {
        run_seed: u64,
        panel: SeedPanel,
        replicate_index: u64,
    },
}

impl SeedPolicy {
    pub const fn domain_separated(run_seed: u64, panel: SeedPanel, replicate_index: u64) -> Self {
        Self::DomainSeparatedV1 {
            run_seed,
            panel,
            replicate_index,
        }
    }

    pub const fn evaluation_plan(self) -> Option<EvaluationSeedPlan> {
        match self {
            Self::LegacyFixedV1 => None,
            Self::DomainSeparatedV1 {
                run_seed,
                panel,
                replicate_index,
            } => Some(SeedTreeV1::new(run_seed).evaluation(panel, replicate_index)),
        }
    }

    pub const fn run_seed(self) -> Option<u64> {
        match self {
            Self::LegacyFixedV1 => None,
            Self::DomainSeparatedV1 { run_seed, .. } => Some(run_seed),
        }
    }
}

impl Hemisphere {
    const fn id(self) -> u64 {
        self as u64
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplicateIdentity {
    pub run_seed: u64,
    pub panel: SeedPanel,
    pub replicate_index: u64,
    pub seed_tree_revision: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeedTreeV1 {
    run_seed: u64,
}

impl SeedTreeV1 {
    pub const fn new(run_seed: u64) -> Self {
        Self { run_seed }
    }

    pub const fn run_seed(self) -> u64 {
        self.run_seed
    }

    pub fn optimizer_seed(self) -> [u8; 32] {
        self.derive(&[DOMAIN_OPTIMIZER])
    }

    pub fn optimizer_rng(self) -> ChaCha12Rng {
        ChaCha12Rng::from_seed(self.optimizer_seed())
    }

    pub fn dataset_genome_seed(self, sample_slot: u64) -> [u8; 32] {
        self.derive(&[DOMAIN_DATASET_GENOME, sample_slot])
    }

    pub fn dataset_genome_rng(self, sample_slot: u64) -> ChaCha12Rng {
        ChaCha12Rng::from_seed(self.dataset_genome_seed(sample_slot))
    }

    pub const fn evaluation(self, panel: SeedPanel, replicate_index: u64) -> EvaluationSeedPlan {
        EvaluationSeedPlan {
            tree: self,
            panel,
            replicate_index,
        }
    }

    fn derive(self, path: &[u64]) -> [u8; 32] {
        let mut material = Vec::with_capacity(16 + path.len() * 8);
        material.extend_from_slice(&self.run_seed.to_le_bytes());
        material.extend_from_slice(&(path.len() as u64).to_le_bytes());
        for element in path {
            material.extend_from_slice(&element.to_le_bytes());
        }
        blake3::derive_key(SEED_TREE_CONTEXT, &material)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EvaluationSeedPlan {
    tree: SeedTreeV1,
    panel: SeedPanel,
    replicate_index: u64,
}

impl EvaluationSeedPlan {
    pub fn identity(self) -> ReplicateIdentity {
        ReplicateIdentity {
            run_seed: self.tree.run_seed,
            panel: self.panel,
            replicate_index: self.replicate_index,
            seed_tree_revision: SEED_DERIVATION_REVISION.to_string(),
        }
    }

    /// Stable 64-bit compatibility token for legacy flat `seed_eval` columns.
    /// The structured identity remains authoritative for replay.
    pub fn compatibility_seed(self) -> u64 {
        seed_u64(
            self.tree
                .derive(&[DOMAIN_EVALUATION, self.panel.id(), self.replicate_index]),
        )
    }

    pub fn audio_seed(self) -> u64 {
        seed_u64(self.consumer_seed(CONSUMER_AUDIO, &[]))
    }

    pub fn movement_seed(self, object_slot: u32) -> [u8; 32] {
        assert!(
            object_slot < MAX_OBJECT_SLOTS,
            "object slot {object_slot} is outside 0..{MAX_OBJECT_SLOTS}"
        );
        self.consumer_seed(CONSUMER_MOVEMENT, &[object_slot as u64])
    }

    pub fn movement_rng(self, object_slot: u32) -> ChaCha12Rng {
        ChaCha12Rng::from_seed(self.movement_seed(object_slot))
    }

    pub fn neural_seed(self, hemisphere: Hemisphere, band: u8) -> [u8; 32] {
        assert!(
            band < NEURAL_BANDS,
            "neural band {band} is outside 0..{NEURAL_BANDS}"
        );
        self.consumer_seed(CONSUMER_NEURAL, &[hemisphere.id(), band as u64])
    }

    pub fn neural_rng(self, hemisphere: Hemisphere, band: u8) -> ChaCha12Rng {
        ChaCha12Rng::from_seed(self.neural_seed(hemisphere, band))
    }

    pub fn environment_rir_seed(self) -> [u8; 32] {
        self.consumer_seed(CONSUMER_ENVIRONMENT_RIR, &[])
    }

    pub fn environment_rir_rng(self) -> ChaCha12Rng {
        ChaCha12Rng::from_seed(self.environment_rir_seed())
    }

    pub fn disturbance_seed(self, hemisphere: Hemisphere) -> [u8; 32] {
        self.consumer_seed(CONSUMER_DISTURBANCE, &[hemisphere.id()])
    }

    pub fn disturbance_rng(self, hemisphere: Hemisphere) -> ChaCha12Rng {
        ChaCha12Rng::from_seed(self.disturbance_seed(hemisphere))
    }

    fn consumer_seed(self, consumer: u64, indices: &[u64]) -> [u8; 32] {
        let mut path = Vec::with_capacity(4 + indices.len());
        path.extend_from_slice(&[
            DOMAIN_EVALUATION,
            self.panel.id(),
            self.replicate_index,
            consumer,
        ]);
        path.extend_from_slice(indices);
        self.tree.derive(&path)
    }
}

pub fn seed_u64(seed: [u8; 32]) -> u64 {
    u64::from_le_bytes(seed[..8].try_into().expect("seed prefix is eight bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::RngCore;
    use std::collections::HashSet;

    fn hex(bytes: [u8; 32]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn seed_tree_v1_known_answers_are_stable() {
        let cases = [
            (
                0,
                "7c3e77e9b2ececed22fa471b0187080db1ffe1a965d52dbf699a02c63543d47e",
            ),
            (
                1,
                "cf396f0bb85f3ac331fd5f46f97c96224c3360fe531159dd368afd037a75737c",
            ),
            (
                42,
                "2798b4db8550d8166c64a232687be0cbbddb6810d6a0945a1770032d4f8f292d",
            ),
            (
                u64::MAX,
                "27ff2bfd34099fc1c310659a0bf8a7b606888c995b15c64a9a44dd9f397e48c4",
            ),
        ];
        for (run_seed, expected) in cases {
            let tree = SeedTreeV1::new(run_seed);
            assert_eq!(hex(tree.optimizer_seed()), expected, "run seed {run_seed}");
        }
    }

    #[test]
    fn every_documented_evaluation_address_is_distinct() {
        let plan = SeedTreeV1::new(42).evaluation(SeedPanel::Direct, 0);
        let mut values = HashSet::new();
        assert!(values.insert(plan.audio_seed().to_le_bytes().to_vec()));
        assert!(values.insert(plan.environment_rir_seed().to_vec()));
        for slot in 0..MAX_OBJECT_SLOTS {
            assert!(values.insert(plan.movement_seed(slot).to_vec()));
        }
        for hemisphere in [Hemisphere::Left, Hemisphere::Right] {
            assert!(values.insert(plan.disturbance_seed(hemisphere).to_vec()));
            for band in 0..NEURAL_BANDS {
                assert!(values.insert(plan.neural_seed(hemisphere, band).to_vec()));
            }
        }
        assert_eq!(values.len(), 20);
    }

    #[test]
    fn panels_and_replicates_are_domain_separated() {
        let tree = SeedTreeV1::new(42);
        let direct = tree.evaluation(SeedPanel::Direct, 0).audio_seed();
        let next = tree.evaluation(SeedPanel::Direct, 1).audio_seed();
        let search = tree.evaluation(SeedPanel::Search, 0).audio_seed();
        let finalist = tree.evaluation(SeedPanel::Finalist, 0).audio_seed();
        assert_ne!(direct, next);
        assert_ne!(direct, search);
        assert_ne!(search, finalist);
    }

    #[test]
    fn same_typed_address_replays_the_same_rng_stream() {
        let tree = SeedTreeV1::new(42);
        let mut first = tree
            .evaluation(SeedPanel::Direct, 3)
            .neural_rng(Hemisphere::Left, 2);
        let mut second = tree
            .evaluation(SeedPanel::Direct, 3)
            .neural_rng(Hemisphere::Left, 2);
        let a: Vec<u64> = (0..8).map(|_| first.next_u64()).collect();
        let b: Vec<u64> = (0..8).map(|_| second.next_u64()).collect();
        assert_eq!(a, b);
    }

    #[test]
    #[should_panic(expected = "object slot 8 is outside")]
    fn invalid_object_slot_is_rejected() {
        let _ = SeedTreeV1::new(0)
            .evaluation(SeedPanel::Direct, 0)
            .movement_seed(MAX_OBJECT_SLOTS);
    }

    #[test]
    #[should_panic(expected = "neural band 4 is outside")]
    fn invalid_neural_band_is_rejected() {
        let _ = SeedTreeV1::new(0)
            .evaluation(SeedPanel::Direct, 0)
            .neural_seed(Hemisphere::Left, NEURAL_BANDS);
    }
}
