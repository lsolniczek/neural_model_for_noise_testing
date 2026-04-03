/// Preset parameter space definition.
///
/// Maps the full NoiseEngine configuration into a flat f64 vector
/// that the optimizer can search over. Handles encoding/decoding
/// of mixed continuous and discrete parameters.

use crate::movement::MovementConfig;
use noice_generator_core::{
    AcousticEnvironment, ModulatorKind, NoiseColor, NoiseEngine,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const MAX_OBJECTS: usize = 8;

// ── Dimension count ─────────────────────────────────────────────────────────
// Global: master_gain(1) + spatial_mode(1) + source_count(1) + anchor_color(1)
//       + anchor_volume(1) + environment(1) = 6
// Per object (8): active(1) + color(1) + x(1) + y(1) + z(1) + volume(1)
//               + reverb_send(1) + bass_kind(1) + bass_a(1) + bass_b(1) + bass_c(1)
//               + sat_kind(1) + sat_a(1) + sat_b(1) + sat_c(1)
//               + mov_kind(1) + mov_radius(1) + mov_speed(1) + mov_phase(1)
//               + mov_depth_min(1) + mov_depth_max(1) + mov_reverb_min(1)
//               + mov_reverb_max(1) = 23
// Total: 6 + 8×23 = 190
pub const GENOME_LEN: usize = 6 + MAX_OBJECTS * 23;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModConfig {
    pub kind: u8, // 0=Flat, 1=SineLfo, 2=Breathing, 3=Stochastic, 4=NeuralLfo
    pub param_a: f32,
    pub param_b: f32,
    pub param_c: f32,
}

impl Default for ModConfig {
    fn default() -> Self {
        ModConfig {
            kind: 0,
            param_a: 0.0,
            param_b: 0.0,
            param_c: 0.0,
        }
    }
}

impl ModConfig {
    fn to_modulator_kind(&self) -> ModulatorKind {
        ModulatorKind::from_u8(self.kind)
    }

    /// Clamp parameters to valid ranges based on kind.
    fn clamp(&mut self) {
        self.kind = self.kind.min(4);
        match self.kind {
            1 => {
                // SineLfo: freq 0.01–2.0, depth 0.0–1.0
                self.param_a = self.param_a.clamp(0.01, 2.0);
                self.param_b = self.param_b.clamp(0.0, 1.0);
            }
            2 => {
                // Breathing: pattern_id 0–3, min_gain 0.0–1.0
                self.param_a = self.param_a.clamp(0.0, 3.0);
                self.param_b = self.param_b.clamp(0.0, 1.0);
            }
            3 => {
                // Stochastic: lambda 0.1–10, decay_ms 10–500, min_gain 0.05–0.5
                self.param_a = self.param_a.clamp(0.1, 10.0);
                self.param_b = self.param_b.clamp(10.0, 500.0);
                self.param_c = self.param_c.clamp(0.05, 0.5);
            }
            4 => {
                // NeuralLfo: freq 1.0–40.0 Hz, depth 0.0–1.0
                self.param_a = self.param_a.clamp(1.0, 40.0);
                self.param_b = self.param_b.clamp(0.0, 1.0);
            }
            _ => {} // Flat: no params
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectConfig {
    pub active: bool,
    pub color: u8, // 0–6
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub volume: f32,
    pub reverb_send: f32,
    pub bass_mod: ModConfig,
    pub satellite_mod: ModConfig,
    #[serde(default)]
    pub movement: MovementConfig,
}

impl Default for ObjectConfig {
    fn default() -> Self {
        ObjectConfig {
            active: false,
            color: 0,
            x: 0.0,
            y: 0.0,
            z: 1.0,
            volume: 1.0,
            reverb_send: 0.1,
            bass_mod: ModConfig::default(),
            satellite_mod: ModConfig::default(),
            movement: MovementConfig::default(),
        }
    }
}

impl ObjectConfig {
    fn clamp(&mut self) {
        self.color = self.color.min(6);
        self.x = self.x.clamp(-5.0, 5.0);
        self.y = self.y.clamp(-3.0, 3.0);
        self.z = self.z.clamp(-5.0, 5.0);
        self.volume = self.volume.clamp(0.0, 1.0);
        self.reverb_send = self.reverb_send.clamp(0.0, 1.0);
        self.bass_mod.clamp();
        self.satellite_mod.clamp();
        self.movement.clamp();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub master_gain: f32,
    pub spatial_mode: u8,  // 0=Stereo, 1=Immersive
    pub source_count: u32, // 2–8 (for Immersive)
    pub anchor_color: u8,  // 0–6
    pub anchor_volume: f32,
    pub environment: u8, // 0–4 (AcousticEnvironment)
    pub objects: Vec<ObjectConfig>,
}

impl Default for Preset {
    fn default() -> Self {
        Preset {
            master_gain: 0.8,
            spatial_mode: 1, // Immersive
            source_count: 4,
            anchor_color: 2, // Brown
            anchor_volume: 0.0,
            environment: 0,
            objects: (0..MAX_OBJECTS).map(|_| ObjectConfig::default()).collect(),
        }
    }
}

impl Preset {
    /// Clamp all parameters to valid ranges.
    pub fn clamp(&mut self) {
        self.master_gain = self.master_gain.clamp(0.1, 1.0);
        self.spatial_mode = self.spatial_mode.min(1);
        self.source_count = self.source_count.clamp(2, 8);
        self.anchor_color = self.anchor_color.min(6);
        self.anchor_volume = self.anchor_volume.clamp(0.0, 1.0);
        self.environment = self.environment.min(4);
        for obj in &mut self.objects {
            obj.clamp();
        }
    }

    /// Count active objects.
    pub fn active_object_count(&self) -> usize {
        self.objects.iter().filter(|o| o.active).count()
    }

    /// Apply this preset to a NoiseEngine instance.
    ///
    /// Spatial mode must be flushed via a render call before setting objects,
    /// because `apply_pending_config` re-syncs object params from inner state
    /// (resetting any pending object changes). We do a 1-frame render between
    /// the two phases to flush the config change.
    pub fn apply_to_engine(&self, engine: &Arc<NoiseEngine>) {
        engine.set_master_gain(self.master_gain);

        engine.set_source_count(self.source_count);

        engine.set_anchor_color(NoiseColor::from_u8(self.anchor_color));
        engine.set_anchor_volume(self.anchor_volume);

        let env = match self.environment {
            0 => AcousticEnvironment::AnechoicChamber,
            1 => AcousticEnvironment::FocusRoom,
            2 => AcousticEnvironment::OpenLounge,
            3 => AcousticEnvironment::VastSpace,
            _ => AcousticEnvironment::DeepSanctuary,
        };
        engine.set_acoustic_environment(env);

        // Flush spatial mode change so that apply_pending_config runs before
        // we set object params (otherwise it re-syncs and overwrites them).
        let _ = engine.render_audio(1);

        // Configure objects (now safe — config_dirty has been cleared)
        for (i, obj) in self.objects.iter().enumerate() {
            engine.set_object(
                i as u32,
                obj.active,
                NoiseColor::from_u8(obj.color),
                obj.x,
                obj.y,
                obj.z,
                obj.volume,
                obj.reverb_send,
            );
            engine.set_bass_modulator(
                i as u32,
                obj.bass_mod.to_modulator_kind(),
                obj.bass_mod.param_a,
                obj.bass_mod.param_b,
                obj.bass_mod.param_c,
            );
            engine.set_satellite_modulator(
                i as u32,
                obj.satellite_mod.to_modulator_kind(),
                obj.satellite_mod.param_a,
                obj.satellite_mod.param_b,
                obj.satellite_mod.param_c,
            );
        }
    }

    // ── Genome encoding/decoding ────────────────────────────────────────────

    /// Encode preset to a flat f64 vector for the optimizer.
    pub fn to_genome(&self) -> Vec<f64> {
        let mut g = Vec::with_capacity(GENOME_LEN);

        // Global params
        g.push(self.master_gain as f64);
        g.push(self.spatial_mode as f64);
        g.push(self.source_count as f64);
        g.push(self.anchor_color as f64);
        g.push(self.anchor_volume as f64);
        g.push(self.environment as f64);

        // Per-object params
        for obj in &self.objects {
            g.push(if obj.active { 1.0 } else { 0.0 });
            g.push(obj.color as f64);
            g.push(obj.x as f64);
            g.push(obj.y as f64);
            g.push(obj.z as f64);
            g.push(obj.volume as f64);
            g.push(obj.reverb_send as f64);
            g.push(obj.bass_mod.kind as f64);
            g.push(obj.bass_mod.param_a as f64);
            g.push(obj.bass_mod.param_b as f64);
            g.push(obj.bass_mod.param_c as f64);
            g.push(obj.satellite_mod.kind as f64);
            g.push(obj.satellite_mod.param_a as f64);
            g.push(obj.satellite_mod.param_b as f64);
            g.push(obj.satellite_mod.param_c as f64);
            g.push(obj.movement.kind as f64);
            g.push(obj.movement.radius as f64);
            g.push(obj.movement.speed as f64);
            g.push(obj.movement.phase as f64);
            g.push(obj.movement.depth_min as f64);
            g.push(obj.movement.depth_max as f64);
            g.push(obj.movement.reverb_min as f64);
            g.push(obj.movement.reverb_max as f64);
        }

        g
    }

    /// Decode from a flat f64 vector. Values are clamped to valid ranges.
    pub fn from_genome(g: &[f64]) -> Self {
        assert!(g.len() >= GENOME_LEN, "genome too short");

        let mut preset = Preset {
            master_gain: g[0] as f32,
            spatial_mode: g[1].round() as u8,
            source_count: g[2].round() as u32,
            anchor_color: g[3].round() as u8,
            anchor_volume: g[4] as f32,
            environment: g[5].round() as u8,
            objects: Vec::with_capacity(MAX_OBJECTS),
        };

        for i in 0..MAX_OBJECTS {
            let base = 6 + i * 23;
            let obj = ObjectConfig {
                active: g[base] > 0.5,
                color: g[base + 1].round() as u8,
                x: g[base + 2] as f32,
                y: g[base + 3] as f32,
                z: g[base + 4] as f32,
                volume: g[base + 5] as f32,
                reverb_send: g[base + 6] as f32,
                bass_mod: ModConfig {
                    kind: g[base + 7].round() as u8,
                    param_a: g[base + 8] as f32,
                    param_b: g[base + 9] as f32,
                    param_c: g[base + 10] as f32,
                },
                satellite_mod: ModConfig {
                    kind: g[base + 11].round() as u8,
                    param_a: g[base + 12] as f32,
                    param_b: g[base + 13] as f32,
                    param_c: g[base + 14] as f32,
                },
                movement: MovementConfig {
                    kind: g[base + 15].round() as u8,
                    radius: g[base + 16] as f32,
                    speed: g[base + 17] as f32,
                    phase: g[base + 18] as f32,
                    depth_min: g[base + 19] as f32,
                    depth_max: g[base + 20] as f32,
                    reverb_min: g[base + 21] as f32,
                    reverb_max: g[base + 22] as f32,
                },
            };
            preset.objects.push(obj);
        }

        preset.clamp();
        preset
    }

    /// Indices of discrete (integer-valued) genes in the genome.
    ///
    /// These genes encode categorical parameters (noise color, movement kind,
    /// modulator kind, etc.) and should be rounded to integers during
    /// optimisation so the DE algorithm doesn't waste budget exploring
    /// continuous values that map to the same discrete setting.
    pub fn discrete_gene_indices() -> Vec<usize> {
        let mut indices = Vec::new();
        // Global discrete params
        indices.push(1); // spatial_mode
        indices.push(2); // source_count
        indices.push(3); // anchor_color
        indices.push(5); // environment

        // Per-object discrete params
        for i in 0..MAX_OBJECTS {
            let base = 6 + i * 23;
            indices.push(base);      // active (0/1)
            indices.push(base + 1);  // color
            indices.push(base + 7);  // bass_mod.kind
            indices.push(base + 11); // satellite_mod.kind
            indices.push(base + 15); // movement.kind
        }
        indices
    }

    /// Parameter bounds: (min, max) for each gene.
    pub fn bounds() -> Vec<(f64, f64)> {
        let mut b = Vec::with_capacity(GENOME_LEN);

        // Global
        b.push((0.1, 1.0));   // master_gain
        b.push((0.0, 1.0));   // spatial_mode (discrete: 0 or 1)
        b.push((2.0, 8.0));   // source_count
        b.push((0.0, 6.0));   // anchor_color
        b.push((0.0, 1.0));   // anchor_volume
        b.push((0.0, 4.0));   // environment

        // Per-object (×8)
        for _ in 0..MAX_OBJECTS {
            b.push((0.0, 1.0));     // active
            b.push((0.0, 6.0));     // color
            b.push((-5.0, 5.0));    // x
            b.push((-3.0, 3.0));    // y
            b.push((-5.0, 5.0));    // z
            b.push((0.0, 1.0));     // volume
            b.push((0.0, 1.0));     // reverb_send
            b.push((0.0, 4.0));     // bass_mod.kind (0=Flat,1=SineLfo,2=Breathing,3=Stochastic,4=NeuralLfo)
            b.push((0.0, 40.0));    // bass_mod.param_a (max covers NeuralLfo 40 Hz)
            b.push((0.0, 1.0));     // bass_mod.param_b
            b.push((0.0, 0.5));     // bass_mod.param_c
            b.push((0.0, 4.0));     // sat_mod.kind
            b.push((0.0, 40.0));    // sat_mod.param_a
            b.push((0.0, 1.0));     // sat_mod.param_b
            b.push((0.0, 0.5));     // sat_mod.param_c
            b.push((0.0, 5.0));     // movement.kind
            b.push((0.0, 5.0));     // movement.radius
            b.push((0.0, 5.0));     // movement.speed
            b.push((0.0, 6.283));   // movement.phase
            b.push((0.5, 5.0));     // movement.depth_min
            b.push((0.5, 6.0));     // movement.depth_max
            b.push((0.0, 1.0));     // movement.reverb_min
            b.push((0.0, 1.0));     // movement.reverb_max
        }

        b
    }
}
