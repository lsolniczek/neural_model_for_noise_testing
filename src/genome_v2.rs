//! Versioned mixed-variable preset genome used by P-10.
//!
//! The JSON preset remains the public interchange format.  This module only
//! defines the optimizer representation: nominal values are never treated as
//! numbers, and parameters belonging to an inactive branch are canonicalized
//! to zero and ignored by mutation and distance calculations.

use crate::movement::MovementConfig;
use crate::preset::{ModConfig, ObjectConfig, Preset, MAX_OBJECTS};

pub const MIXED_GENOME_SCHEMA: &str = "mixed-v2";
pub const MIXED_GENOME_DIM: usize = 423;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneKind {
    Continuous,
    Nominal { categories: u8 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Condition {
    pub controller: usize,
    pub value: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneSpec {
    pub kind: GeneKind,
    pub conditions: Vec<Condition>,
    /// Optional disjunction evaluated in addition to `conditions`.
    pub any_of: Vec<Condition>,
    pub canonical: f64,
}

impl GeneSpec {
    pub fn active(&self, genome: &[f64]) -> bool {
        self.conditions
            .iter()
            .all(|c| genome[c.controller].round() as u8 == c.value)
            && (self.any_of.is_empty()
                || self
                    .any_of
                    .iter()
                    .any(|c| genome[c.controller].round() as u8 == c.value))
    }

    pub fn bounds(&self) -> (f64, f64) {
        match self.kind {
            GeneKind::Continuous => (0.0, 1.0),
            GeneKind::Nominal { categories } => (0.0, f64::from(categories - 1)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MixedGenomeContext {
    pub room: crate::preset::RoomConfig,
    pub position_space_per_slot: [u8; MAX_OBJECTS],
}

impl Default for MixedGenomeContext {
    fn default() -> Self {
        Self {
            room: crate::preset::RoomConfig::default(),
            position_space_per_slot: [0; MAX_OBJECTS],
        }
    }
}

fn norm(value: f32, lo: f32, hi: f32) -> f64 {
    ((value - lo) / (hi - lo)).clamp(0.0, 1.0) as f64
}

fn denorm(value: f64, lo: f32, hi: f32) -> f32 {
    lo + value.clamp(0.0, 1.0) as f32 * (hi - lo)
}

fn push_cont(specs: &mut Vec<GeneSpec>, conditions: &[Condition]) -> usize {
    let index = specs.len();
    specs.push(GeneSpec {
        kind: GeneKind::Continuous,
        conditions: conditions.to_vec(),
        any_of: Vec::new(),
        canonical: 0.0,
    });
    index
}

fn push_nominal(specs: &mut Vec<GeneSpec>, categories: u8, conditions: &[Condition]) -> usize {
    let index = specs.len();
    specs.push(GeneSpec {
        kind: GeneKind::Nominal { categories },
        conditions: conditions.to_vec(),
        any_of: Vec::new(),
        canonical: 0.0,
    });
    index
}

fn push_cont_any(
    specs: &mut Vec<GeneSpec>,
    conditions: &[Condition],
    any_of: &[Condition],
) -> usize {
    let index = specs.len();
    specs.push(GeneSpec {
        kind: GeneKind::Continuous,
        conditions: conditions.to_vec(),
        any_of: any_of.to_vec(),
        canonical: 0.0,
    });
    index
}

#[derive(Debug, Clone)]
struct ModIndices {
    kind: usize,
    sine_frequency: usize,
    sine_depth: usize,
    breathing_pattern: usize,
    breathing_min_gain: usize,
    stochastic_rate: usize,
    stochastic_decay: usize,
    stochastic_min_gain: usize,
    neural_frequency: usize,
    neural_depth: usize,
    isochronic_frequency: usize,
    isochronic_depth: usize,
    isochronic_duty: usize,
    random_pulse_rate: usize,
    random_pulse_depth: usize,
    random_pulse_duration: usize,
}

#[derive(Debug, Clone)]
struct MovementIndices {
    kind: usize,
    radius: usize,
    speed: usize,
    phase: usize,
    depth_min: usize,
    depth_max: usize,
    reverb_min: usize,
    reverb_max: usize,
}

#[derive(Debug, Clone)]
struct ObjectIndices {
    active: usize,
    color: usize,
    x: usize,
    y: usize,
    z: usize,
    volume: usize,
    reverb_send: usize,
    spread: usize,
    bass: ModIndices,
    satellite: ModIndices,
    movement: MovementIndices,
    tint_enabled: usize,
    tint_frequency: usize,
    tint_sign: usize,
    tint_magnitude: usize,
}

#[derive(Debug, Clone)]
pub struct PresetGenomeV2 {
    specs: Vec<GeneSpec>,
    master_gain: usize,
    environment: usize,
    binaural_enabled: usize,
    binaural_center: usize,
    binaural_beat: usize,
    binaural_gain: usize,
    binaural_lower_ear: usize,
    objects: Vec<ObjectIndices>,
}

impl Default for PresetGenomeV2 {
    fn default() -> Self {
        Self::new()
    }
}

impl PresetGenomeV2 {
    pub fn new() -> Self {
        let mut specs = Vec::new();
        let master_gain = push_cont(&mut specs, &[]);
        let environment = push_nominal(&mut specs, 5, &[]);
        let binaural_enabled = push_nominal(&mut specs, 2, &[]);
        let binaural_on = [Condition {
            controller: binaural_enabled,
            value: 1,
        }];
        let binaural_center = push_cont(&mut specs, &binaural_on);
        let binaural_beat = push_cont(&mut specs, &binaural_on);
        let binaural_gain = push_cont(&mut specs, &binaural_on);
        let binaural_lower_ear = push_nominal(&mut specs, 2, &binaural_on);

        let mut objects = Vec::with_capacity(MAX_OBJECTS);
        for _ in 0..MAX_OBJECTS {
            let active = push_nominal(&mut specs, 2, &[]);
            let object_on = [Condition {
                controller: active,
                value: 1,
            }];
            let color = push_nominal(&mut specs, 8, &object_on);
            let x = push_cont(&mut specs, &object_on);
            let y = push_cont(&mut specs, &object_on);
            let z = push_cont(&mut specs, &object_on);
            let volume = push_cont(&mut specs, &object_on);
            let reverb_send = push_cont(&mut specs, &object_on);
            let spread = push_cont(&mut specs, &object_on);
            let bass = Self::build_mod(&mut specs, &object_on);
            let satellite = Self::build_mod(&mut specs, &object_on);
            let movement = Self::build_movement(&mut specs, &object_on);
            let tint_enabled = push_nominal(&mut specs, 2, &object_on);
            let tint_on = [
                Condition {
                    controller: active,
                    value: 1,
                },
                Condition {
                    controller: tint_enabled,
                    value: 1,
                },
            ];
            let tint_frequency = push_cont(&mut specs, &tint_on);
            let tint_sign = push_nominal(&mut specs, 2, &tint_on);
            let tint_magnitude = push_cont(&mut specs, &tint_on);
            objects.push(ObjectIndices {
                active,
                color,
                x,
                y,
                z,
                volume,
                reverb_send,
                spread,
                bass,
                satellite,
                movement,
                tint_enabled,
                tint_frequency,
                tint_sign,
                tint_magnitude,
            });
        }

        let schema = Self {
            specs,
            master_gain,
            environment,
            binaural_enabled,
            binaural_center,
            binaural_beat,
            binaural_gain,
            binaural_lower_ear,
            objects,
        };
        debug_assert_eq!(schema.specs.len(), MIXED_GENOME_DIM);
        schema
    }

    fn build_mod(specs: &mut Vec<GeneSpec>, parent: &[Condition]) -> ModIndices {
        let kind = push_nominal(specs, 7, parent);
        let branch = |value| {
            let mut conditions = parent.to_vec();
            conditions.push(Condition {
                controller: kind,
                value,
            });
            conditions
        };
        let sine = branch(1);
        let breathing = branch(2);
        let stochastic = branch(3);
        let neural = branch(4);
        let isochronic = branch(5);
        let random_pulse = branch(6);
        ModIndices {
            kind,
            sine_frequency: push_cont(specs, &sine),
            sine_depth: push_cont(specs, &sine),
            breathing_pattern: push_nominal(specs, 4, &breathing),
            breathing_min_gain: push_cont(specs, &breathing),
            stochastic_rate: push_cont(specs, &stochastic),
            stochastic_decay: push_cont(specs, &stochastic),
            stochastic_min_gain: push_cont(specs, &stochastic),
            neural_frequency: push_cont(specs, &neural),
            neural_depth: push_cont(specs, &neural),
            isochronic_frequency: push_cont(specs, &isochronic),
            isochronic_depth: push_cont(specs, &isochronic),
            isochronic_duty: push_cont(specs, &isochronic),
            random_pulse_rate: push_cont(specs, &random_pulse),
            random_pulse_depth: push_cont(specs, &random_pulse),
            random_pulse_duration: push_cont(specs, &random_pulse),
        }
    }

    fn build_movement(specs: &mut Vec<GeneSpec>, parent: &[Condition]) -> MovementIndices {
        let kind = push_nominal(specs, 6, parent);
        let any = |values: &[u8]| {
            values
                .iter()
                .map(|&value| Condition {
                    controller: kind,
                    value,
                })
                .collect::<Vec<_>>()
        };
        MovementIndices {
            kind,
            radius: push_cont_any(specs, parent, &any(&[1, 2, 3, 5])),
            speed: push_cont_any(specs, parent, &any(&[1, 2, 3, 4, 5])),
            phase: push_cont_any(specs, parent, &any(&[1, 2, 4, 5])),
            depth_min: push_cont_any(specs, parent, &any(&[4, 5])),
            depth_max: push_cont_any(specs, parent, &any(&[4])),
            reverb_min: push_cont_any(specs, parent, &any(&[4])),
            reverb_max: push_cont_any(specs, parent, &any(&[4])),
        }
    }

    pub fn specs(&self) -> &[GeneSpec] {
        &self.specs
    }

    pub fn context_from_preset(&self, preset: &Preset) -> MixedGenomeContext {
        let mut context = MixedGenomeContext {
            room: preset.room.clone(),
            ..MixedGenomeContext::default()
        };
        for (i, object) in preset.objects.iter().take(MAX_OBJECTS).enumerate() {
            context.position_space_per_slot[i] = object.position_space.min(2);
        }
        context
    }

    pub fn encode(&self, preset: &Preset) -> Vec<f64> {
        let mut genome = vec![0.0; self.specs.len()];
        genome[self.master_gain] = norm(preset.master_gain, 0.1, 1.0);
        genome[self.environment] = f64::from(preset.environment.min(4));
        genome[self.binaural_enabled] = f64::from(preset.binaural_beat.enabled);
        genome[self.binaural_center] =
            norm(preset.binaural_beat.center_frequency_hz, 100.0, 1000.0);
        genome[self.binaural_beat] = norm(preset.binaural_beat.beat_frequency_hz, 0.0, 40.0);
        genome[self.binaural_gain] = norm(preset.binaural_beat.gain_db, -80.0, -24.0);
        genome[self.binaural_lower_ear] =
            f64::from(preset.binaural_beat.lower_frequency_ear.min(1));

        for (slot, idx) in self.objects.iter().enumerate() {
            let object = preset.objects.get(slot).cloned().unwrap_or_default();
            genome[idx.active] = f64::from(object.active);
            genome[idx.color] = f64::from(object.color.min(7));
            let (xlo, xhi, ylo, yhi, zlo, zhi) = position_ranges(object.position_space);
            genome[idx.x] = norm(object.x, xlo, xhi);
            genome[idx.y] = norm(object.y, ylo, yhi);
            genome[idx.z] = norm(object.z, zlo, zhi);
            genome[idx.volume] = norm(object.volume, 0.0, 1.0);
            genome[idx.reverb_send] = norm(object.reverb_send, 0.0, 1.0);
            genome[idx.spread] = norm(object.spread, 0.0, 1.0);
            Self::encode_mod(&mut genome, &idx.bass, &object.bass_mod);
            Self::encode_mod(&mut genome, &idx.satellite, &object.satellite_mod);
            Self::encode_movement(&mut genome, &idx.movement, &object.movement);
            let tint_enabled = object.tint_freq >= 100.0 && object.tint_db.abs() > 0.01;
            genome[idx.tint_enabled] = f64::from(tint_enabled);
            genome[idx.tint_frequency] = norm(object.tint_freq.max(100.0), 100.0, 8000.0);
            genome[idx.tint_sign] = f64::from(object.tint_db >= 0.0);
            genome[idx.tint_magnitude] = norm(object.tint_db.abs(), 0.0, 6.0);
        }
        self.canonicalize(&mut genome);
        genome
    }

    fn encode_mod(genome: &mut [f64], idx: &ModIndices, config: &ModConfig) {
        let kind = config.kind.min(6);
        genome[idx.kind] = f64::from(kind);
        match kind {
            1 => {
                genome[idx.sine_frequency] = norm(config.param_a, 0.01, 2.0);
                genome[idx.sine_depth] = norm(config.param_b, 0.0, 1.0);
            }
            2 => {
                genome[idx.breathing_pattern] =
                    f64::from(config.param_a.round().clamp(0.0, 3.0) as u8);
                genome[idx.breathing_min_gain] = norm(config.param_b, 0.0, 1.0);
            }
            3 => {
                genome[idx.stochastic_rate] = norm(config.param_a, 0.1, 10.0);
                genome[idx.stochastic_decay] = norm(config.param_b, 10.0, 500.0);
                genome[idx.stochastic_min_gain] = norm(config.param_c, 0.05, 0.5);
            }
            4 => {
                genome[idx.neural_frequency] = norm(config.param_a, 1.0, 40.0);
                genome[idx.neural_depth] = norm(config.param_b, 0.0, 1.0);
            }
            5 => {
                genome[idx.isochronic_frequency] = norm(config.param_a, 1.0, 40.0);
                genome[idx.isochronic_depth] = norm(config.param_b, 0.0, 1.0);
                genome[idx.isochronic_duty] = norm(config.param_c, 0.2, 0.8);
            }
            6 => {
                genome[idx.random_pulse_rate] = norm(config.param_a, 0.1, 20.0);
                genome[idx.random_pulse_depth] = norm(config.param_b, 0.0, 1.0);
                genome[idx.random_pulse_duration] = norm(config.param_c, 20.0, 500.0);
            }
            _ => {}
        }
    }

    fn encode_movement(genome: &mut [f64], idx: &MovementIndices, movement: &MovementConfig) {
        genome[idx.kind] = f64::from(movement.kind.min(5));
        genome[idx.radius] = norm(movement.radius, 0.0, 5.0);
        genome[idx.speed] = norm(movement.speed, 0.0, 5.0);
        genome[idx.phase] = norm(movement.phase, 0.0, std::f32::consts::TAU);
        genome[idx.depth_min] = norm(movement.depth_min, 0.5, 5.0);
        genome[idx.depth_max] = norm(movement.depth_max, 0.5, 6.0);
        genome[idx.reverb_min] = norm(movement.reverb_min, 0.0, 1.0);
        genome[idx.reverb_max] = norm(movement.reverb_max, 0.0, 1.0);
    }

    pub fn decode(&self, genome: &[f64], context: &MixedGenomeContext) -> Result<Preset, String> {
        if genome.len() != self.specs.len() {
            return Err(format!(
                "mixed-v2 genome length mismatch: expected {}, got {}",
                self.specs.len(),
                genome.len()
            ));
        }
        let mut canonical = genome.to_vec();
        self.canonicalize(&mut canonical);
        let mut preset = Preset::default();
        preset.master_gain = denorm(canonical[self.master_gain], 0.1, 1.0);
        preset.spatial_mode = 1;
        preset.anchor_color = 0;
        preset.anchor_volume = 0.0;
        preset.environment = canonical[self.environment].round() as u8;
        preset.room = context.room.clone();
        preset.binaural_beat.enabled = canonical[self.binaural_enabled] > 0.5;
        preset.binaural_beat.center_frequency_hz =
            denorm(canonical[self.binaural_center], 100.0, 1000.0);
        preset.binaural_beat.beat_frequency_hz = denorm(canonical[self.binaural_beat], 0.0, 40.0);
        preset.binaural_beat.gain_db = denorm(canonical[self.binaural_gain], -80.0, -24.0);
        preset.binaural_beat.lower_frequency_ear = canonical[self.binaural_lower_ear].round() as u8;
        preset.objects.clear();

        for (slot, idx) in self.objects.iter().enumerate() {
            let active = canonical[idx.active] > 0.5;
            let space = context.position_space_per_slot[slot];
            let (xlo, xhi, ylo, yhi, zlo, zhi) = position_ranges(space);
            let tint_enabled = canonical[idx.tint_enabled] > 0.5;
            let magnitude = denorm(canonical[idx.tint_magnitude], 0.0, 6.0);
            let tint_db = if !tint_enabled {
                0.0
            } else if canonical[idx.tint_sign] > 0.5 {
                magnitude
            } else {
                -magnitude
            };
            preset.objects.push(ObjectConfig {
                active,
                color: canonical[idx.color].round() as u8,
                position_space: space,
                x: denorm(canonical[idx.x], xlo, xhi),
                y: denorm(canonical[idx.y], ylo, yhi),
                z: denorm(canonical[idx.z], zlo, zhi),
                volume: denorm(canonical[idx.volume], 0.0, 1.0),
                reverb_send: denorm(canonical[idx.reverb_send], 0.0, 1.0),
                spread: denorm(canonical[idx.spread], 0.0, 1.0),
                bass_mod: Self::decode_mod(&canonical, &idx.bass),
                satellite_mod: Self::decode_mod(&canonical, &idx.satellite),
                movement: Self::decode_movement(&canonical, &idx.movement),
                tint_freq: if tint_enabled {
                    denorm(canonical[idx.tint_frequency], 100.0, 8000.0)
                } else {
                    0.0
                },
                tint_db,
                source_kind: 0,
                tone_freq: 200.0,
                tone_amplitude: 0.0,
            });
        }
        preset.source_count = preset.active_object_count() as u32;
        preset.clamp();
        // Preset::clamp retains its historical minimum of two. The DSP object
        // activation flags are authoritative in V2, so restore the derived
        // count after clamping.
        preset.source_count = preset.active_object_count() as u32;
        Ok(preset)
    }

    fn decode_mod(genome: &[f64], idx: &ModIndices) -> ModConfig {
        let kind = genome[idx.kind].round() as u8;
        let (param_a, param_b, param_c) = match kind {
            1 => (
                denorm(genome[idx.sine_frequency], 0.01, 2.0),
                denorm(genome[idx.sine_depth], 0.0, 1.0),
                0.0,
            ),
            2 => (
                genome[idx.breathing_pattern].round() as f32,
                denorm(genome[idx.breathing_min_gain], 0.0, 1.0),
                0.0,
            ),
            3 => (
                denorm(genome[idx.stochastic_rate], 0.1, 10.0),
                denorm(genome[idx.stochastic_decay], 10.0, 500.0),
                denorm(genome[idx.stochastic_min_gain], 0.05, 0.5),
            ),
            4 => (
                denorm(genome[idx.neural_frequency], 1.0, 40.0),
                denorm(genome[idx.neural_depth], 0.0, 1.0),
                0.0,
            ),
            5 => (
                denorm(genome[idx.isochronic_frequency], 1.0, 40.0),
                denorm(genome[idx.isochronic_depth], 0.0, 1.0),
                denorm(genome[idx.isochronic_duty], 0.2, 0.8),
            ),
            6 => (
                denorm(genome[idx.random_pulse_rate], 0.1, 20.0),
                denorm(genome[idx.random_pulse_depth], 0.0, 1.0),
                denorm(genome[idx.random_pulse_duration], 20.0, 500.0),
            ),
            _ => (0.0, 0.0, 0.0),
        };
        ModConfig {
            kind,
            param_a,
            param_b,
            param_c,
        }
    }

    fn decode_movement(genome: &[f64], idx: &MovementIndices) -> MovementConfig {
        MovementConfig {
            kind: genome[idx.kind].round() as u8,
            radius: denorm(genome[idx.radius], 0.0, 5.0),
            speed: denorm(genome[idx.speed], 0.0, 5.0),
            phase: denorm(genome[idx.phase], 0.0, std::f32::consts::TAU),
            depth_min: denorm(genome[idx.depth_min], 0.5, 5.0),
            depth_max: denorm(genome[idx.depth_max], 0.5, 6.0),
            reverb_min: denorm(genome[idx.reverb_min], 0.0, 1.0),
            reverb_max: denorm(genome[idx.reverb_max], 0.0, 1.0),
        }
    }

    pub fn canonicalize(&self, genome: &mut [f64]) {
        assert_eq!(genome.len(), self.specs.len());
        for (value, spec) in genome.iter_mut().zip(&self.specs) {
            let (lo, hi) = spec.bounds();
            *value = value.clamp(lo, hi);
            if matches!(spec.kind, GeneKind::Nominal { .. }) {
                *value = value.round();
            }
        }
        // Conditions are ordered after their controllers, so one forward pass
        // is sufficient for all global/object/modulator branches.
        for i in 0..genome.len() {
            if !self.specs[i].active(genome) {
                genome[i] = self.specs[i].canonical;
            }
        }
    }

    pub fn canonical_bytes(&self, genome: &[f64]) -> Vec<u8> {
        let mut canonical = genome.to_vec();
        self.canonicalize(&mut canonical);
        canonical.iter().flat_map(|v| v.to_le_bytes()).collect()
    }
}

fn position_ranges(space: u8) -> (f32, f32, f32, f32, f32, f32) {
    match space {
        1 => (-1.0, 1.0, -1.0, 1.0, -1.0, 1.0),
        2 => (-10.0, 10.0, -5.0, 5.0, -10.0, 10.0),
        _ => (-5.0, 5.0, -3.0, 3.0, -5.0, 5.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_active_blue_spread_and_random_pulse_duration() {
        let schema = PresetGenomeV2::new();
        assert_eq!(schema.specs().len(), MIXED_GENOME_DIM);
        let mut preset = Preset::default();
        preset.objects[0].active = true;
        preset.objects[0].color = 7;
        preset.objects[0].position_space = 2;
        preset.objects[0].spread = 0.67;
        preset.objects[0].bass_mod = ModConfig {
            kind: 6,
            param_a: 8.0,
            param_b: 0.4,
            param_c: 420.0,
        };
        preset.room.mode = 1;
        let context = schema.context_from_preset(&preset);
        let decoded = schema.decode(&schema.encode(&preset), &context).unwrap();
        assert_eq!(decoded.objects[0].color, 7);
        assert_eq!(decoded.objects[0].position_space, 2);
        assert_eq!(decoded.room, preset.room);
        assert!((decoded.objects[0].spread - 0.67).abs() < 1e-5);
        assert!((decoded.objects[0].bass_mod.param_c - 420.0).abs() < 1e-3);
        assert_eq!(decoded.source_count, 1);
        assert_eq!(decoded.spatial_mode, 1);
        assert_eq!(decoded.anchor_volume, 0.0);
    }

    #[test]
    fn inactive_fields_are_canonical_and_hash_identical() {
        let schema = PresetGenomeV2::new();
        let preset = Preset::default();
        let a = schema.encode(&preset);
        let mut b = a.clone();
        b[schema.objects[0].color] = 7.0;
        b[schema.objects[0].spread] = 0.9;
        b[schema.objects[0].bass.kind] = 6.0;
        assert_eq!(schema.canonical_bytes(&a), schema.canonical_bytes(&b));
    }

    #[test]
    fn every_modulator_branch_decodes_inside_dsp_ranges() {
        let schema = PresetGenomeV2::new();
        let context = MixedGenomeContext::default();
        for kind in 0..=6 {
            let mut genome = schema.encode(&Preset::default());
            let idx = &schema.objects[0];
            genome[idx.active] = 1.0;
            genome[idx.bass.kind] = f64::from(kind);
            for value in &mut genome {
                if *value == 0.0 {
                    *value = 0.73;
                }
            }
            genome[idx.active] = 1.0;
            genome[idx.bass.kind] = f64::from(kind);
            let decoded = schema.decode(&genome, &context).unwrap();
            let m = &decoded.objects[0].bass_mod;
            assert_eq!(m.kind, kind);
            if kind == 2 {
                assert!((0.0..=3.0).contains(&m.param_a));
            }
            if kind == 3 {
                assert!((10.0..=500.0).contains(&m.param_b));
            }
            if kind == 6 {
                assert!((20.0..=500.0).contains(&m.param_c));
            }
        }
    }

    #[test]
    fn every_gene_has_an_active_witness_that_changes_the_decoded_preset() {
        let schema = PresetGenomeV2::new();
        let context = MixedGenomeContext::default();
        for (index, target) in schema.specs().iter().enumerate() {
            let mut before = schema
                .specs()
                .iter()
                .map(|spec| match spec.kind {
                    GeneKind::Continuous => 0.25,
                    GeneKind::Nominal { .. } => 0.0,
                })
                .collect::<Vec<_>>();
            for condition in &target.conditions {
                before[condition.controller] = f64::from(condition.value);
            }
            if let Some(condition) = target.any_of.first() {
                before[condition.controller] = f64::from(condition.value);
            }
            schema.canonicalize(&mut before);
            assert!(target.active(&before), "gene {index} has no active witness");

            let mut after = before.clone();
            after[index] = match target.kind {
                GeneKind::Continuous => {
                    if before[index] < 0.5 {
                        0.75
                    } else {
                        0.0
                    }
                }
                GeneKind::Nominal { categories } => {
                    f64::from((before[index].round() as u8 + 1) % categories)
                }
            };
            schema.canonicalize(&mut after);
            let first = schema.decode(&before, &context).unwrap();
            let second = schema.decode(&after, &context).unwrap();
            assert_ne!(
                serde_json::to_vec(&first).unwrap(),
                serde_json::to_vec(&second).unwrap(),
                "gene {index} changes no decoded preset field"
            );
        }
    }
}
