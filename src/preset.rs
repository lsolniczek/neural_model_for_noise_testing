/// Preset parameter space definition.
///
/// Maps the full NoiseEngine configuration into a flat f64 vector
/// that the optimizer can search over. Handles encoding/decoding
/// of mixed continuous and discrete parameters.

use crate::movement::MovementConfig;
use noise_generator_core::{
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

/// Stochastic decay_ms range [10, 500] doesn't fit the shared genome param_b
/// bounds [0, 1]. These helpers remap between the two spaces so the optimizer
/// can explore the full range without widening bounds for other modulator kinds.
const STOCHASTIC_DECAY_MIN: f64 = 10.0;
const STOCHASTIC_DECAY_RANGE: f64 = 490.0; // 500 - 10

fn encode_mod_param_b(kind: u8, param_b: f32) -> f64 {
    if kind == 3 {
        // Stochastic: remap decay_ms [10, 500] → [0, 1]
        ((param_b as f64 - STOCHASTIC_DECAY_MIN) / STOCHASTIC_DECAY_RANGE).clamp(0.0, 1.0)
    } else {
        param_b as f64
    }
}

fn decode_mod_param_b(kind: u8, genome_val: f64) -> f32 {
    if kind == 3 {
        // Stochastic: remap [0, 1] → decay_ms [10, 500]
        (STOCHASTIC_DECAY_MIN + genome_val.clamp(0.0, 1.0) * STOCHASTIC_DECAY_RANGE) as f32
    } else {
        genome_val as f32
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
            g.push(encode_mod_param_b(obj.bass_mod.kind, obj.bass_mod.param_b));
            g.push(obj.bass_mod.param_c as f64);
            g.push(obj.satellite_mod.kind as f64);
            g.push(obj.satellite_mod.param_a as f64);
            g.push(encode_mod_param_b(obj.satellite_mod.kind, obj.satellite_mod.param_b));
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
                bass_mod: {
                    let bk = g[base + 7].round() as u8;
                    ModConfig {
                        kind: bk,
                        param_a: g[base + 8] as f32,
                        param_b: decode_mod_param_b(bk, g[base + 9]),
                        param_c: g[base + 10] as f32,
                    }
                },
                satellite_mod: {
                    let sk = g[base + 11].round() as u8;
                    ModConfig {
                        kind: sk,
                        param_a: g[base + 12] as f32,
                        param_b: decode_mod_param_b(sk, g[base + 13]),
                        param_c: g[base + 14] as f32,
                    }
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

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // GENOME_LEN
    // ---------------------------------------------------------------

    #[test]
    fn genome_len_is_190() {
        assert_eq!(GENOME_LEN, 6 + MAX_OBJECTS * 23);
        assert_eq!(GENOME_LEN, 190);
    }

    // ---------------------------------------------------------------
    // bounds length
    // ---------------------------------------------------------------

    #[test]
    fn bounds_length_matches_genome() {
        let b = Preset::bounds();
        assert_eq!(b.len(), GENOME_LEN);
    }

    #[test]
    fn bounds_min_less_than_max() {
        for (i, (lo, hi)) in Preset::bounds().iter().enumerate() {
            assert!(lo <= hi, "Gene {i}: min {lo} > max {hi}");
        }
    }

    // ---------------------------------------------------------------
    // discrete_gene_indices
    // ---------------------------------------------------------------

    #[test]
    fn discrete_indices_within_genome() {
        let indices = Preset::discrete_gene_indices();
        for &idx in &indices {
            assert!(idx < GENOME_LEN, "Discrete index {idx} >= GENOME_LEN");
        }
    }

    #[test]
    fn discrete_indices_count() {
        // 4 global + 8 * 5 per-object = 44
        let indices = Preset::discrete_gene_indices();
        assert_eq!(indices.len(), 4 + MAX_OBJECTS * 5);
    }

    // ---------------------------------------------------------------
    // to_genome / from_genome round-trip
    // ---------------------------------------------------------------

    #[test]
    fn genome_roundtrip_default_preset() {
        let original = Preset::default();
        let genome = original.to_genome();
        assert_eq!(genome.len(), GENOME_LEN);

        let decoded = Preset::from_genome(&genome);
        let re_encoded = decoded.to_genome();

        // After clamp, re-encoding should give identical genome
        for (i, (a, b)) in genome.iter().zip(re_encoded.iter()).enumerate() {
            assert!(
                (a - b).abs() < 1e-6,
                "Gene {i} differs: {a} vs {b}"
            );
        }
    }

    #[test]
    fn genome_roundtrip_active_objects() {
        let mut preset = Preset::default();
        preset.objects[0].active = true;
        preset.objects[0].color = 3;
        preset.objects[0].volume = 0.75;
        preset.objects[0].x = 2.0;
        preset.objects[0].y = -1.5;
        preset.objects[0].z = 3.0;
        preset.objects[0].bass_mod = ModConfig { kind: 1, param_a: 0.5, param_b: 0.8, param_c: 0.0 };
        preset.objects[0].satellite_mod = ModConfig { kind: 4, param_a: 10.0, param_b: 0.6, param_c: 0.0 };

        let genome = preset.to_genome();
        let decoded = Preset::from_genome(&genome);

        assert!(decoded.objects[0].active);
        assert_eq!(decoded.objects[0].color, 3);
        assert!((decoded.objects[0].volume - 0.75).abs() < 1e-5);
        assert_eq!(decoded.objects[0].bass_mod.kind, 1);
        assert!((decoded.objects[0].bass_mod.param_a - 0.5).abs() < 1e-5);
    }

    // ---------------------------------------------------------------
    // Stochastic param_b encode/decode
    // ---------------------------------------------------------------

    #[test]
    fn stochastic_param_b_encode_decode_roundtrip() {
        // decay_ms = 255 (midpoint) → genome ≈ 0.5 → back to 255
        let decay_ms = 255.0_f32;
        let encoded = encode_mod_param_b(3, decay_ms);
        assert!(
            encoded >= 0.0 && encoded <= 1.0,
            "Encoded stochastic param_b should be in [0, 1], got {encoded}"
        );

        let decoded = decode_mod_param_b(3, encoded);
        assert!(
            (decoded - decay_ms).abs() < 0.1,
            "Stochastic param_b roundtrip: {decay_ms} → {encoded:.4} → {decoded}"
        );
    }

    #[test]
    fn stochastic_param_b_at_boundaries() {
        // Min: 10 → 0.0
        let enc_min = encode_mod_param_b(3, 10.0);
        assert!((enc_min - 0.0).abs() < 1e-6, "decay_ms=10 should encode to ~0, got {enc_min}");

        // Max: 500 → 1.0
        let enc_max = encode_mod_param_b(3, 500.0);
        assert!((enc_max - 1.0).abs() < 1e-6, "decay_ms=500 should encode to ~1, got {enc_max}");

        // Decode back
        let dec_min = decode_mod_param_b(3, 0.0);
        assert!((dec_min - 10.0).abs() < 0.1, "genome=0 should decode to ~10, got {dec_min}");

        let dec_max = decode_mod_param_b(3, 1.0);
        assert!((dec_max - 500.0).abs() < 0.1, "genome=1 should decode to ~500, got {dec_max}");
    }

    #[test]
    fn non_stochastic_param_b_passes_through() {
        // For kind != 3, encode/decode should be identity (f32↔f64 rounding aside)
        for kind in [0_u8, 1, 2, 4] {
            let val = 0.73_f32;
            let encoded = encode_mod_param_b(kind, val);
            assert!(
                (encoded - val as f64).abs() < 1e-6,
                "kind={kind}: non-stochastic should pass through, got {encoded}"
            );
            let decoded = decode_mod_param_b(kind, encoded);
            assert!(
                (decoded - val).abs() < 1e-5,
                "kind={kind}: decode should pass through, got {decoded}"
            );
        }
    }

    #[test]
    fn stochastic_mod_full_genome_roundtrip() {
        let mut preset = Preset::default();
        preset.objects[0].active = true;
        preset.objects[0].bass_mod = ModConfig {
            kind: 3,
            param_a: 5.0,     // lambda
            param_b: 250.0,   // decay_ms (midrange)
            param_c: 0.2,     // min_gain
        };

        let genome = preset.to_genome();

        // The genome param_b slot should be in [0, 1]
        let bass_param_b_idx = 6 + 0 * 23 + 9; // base + 9
        assert!(
            genome[bass_param_b_idx] >= 0.0 && genome[bass_param_b_idx] <= 1.0,
            "Stochastic genome param_b should be [0,1], got {}",
            genome[bass_param_b_idx]
        );

        let decoded = Preset::from_genome(&genome);
        assert_eq!(decoded.objects[0].bass_mod.kind, 3);
        assert!(
            (decoded.objects[0].bass_mod.param_b - 250.0).abs() < 1.0,
            "Stochastic decay_ms should roundtrip: got {}",
            decoded.objects[0].bass_mod.param_b
        );
    }

    // ---------------------------------------------------------------
    // clamp enforces bounds
    // ---------------------------------------------------------------

    #[test]
    fn clamp_enforces_master_gain_bounds() {
        let mut p = Preset::default();
        p.master_gain = 2.0;
        p.clamp();
        assert_eq!(p.master_gain, 1.0);

        p.master_gain = 0.0;
        p.clamp();
        assert_eq!(p.master_gain, 0.1);
    }

    #[test]
    fn clamp_enforces_color_bounds() {
        let mut p = Preset::default();
        p.anchor_color = 10;
        p.clamp();
        assert_eq!(p.anchor_color, 6);
    }

    #[test]
    fn clamp_enforces_object_position_bounds() {
        let mut p = Preset::default();
        p.objects[0].x = 100.0;
        p.objects[0].y = -100.0;
        p.clamp();
        assert_eq!(p.objects[0].x, 5.0);
        assert_eq!(p.objects[0].y, -3.0);
    }

    #[test]
    fn clamp_enforces_mod_params() {
        let mut p = Preset::default();
        p.objects[0].bass_mod = ModConfig { kind: 1, param_a: 100.0, param_b: -1.0, param_c: 0.0 };
        p.clamp();
        // SineLfo: freq clamped to [0.01, 2.0], depth to [0, 1]
        assert_eq!(p.objects[0].bass_mod.param_a, 2.0);
        assert_eq!(p.objects[0].bass_mod.param_b, 0.0);
    }

    // ---------------------------------------------------------------
    // from_genome with out-of-bounds values → clamp corrects
    // ---------------------------------------------------------------

    #[test]
    fn from_genome_clamps_out_of_bounds() {
        let mut genome = vec![999.0; GENOME_LEN];
        // Set discrete values to valid-ish ranges so they don't overflow u8
        genome[1] = 1.0;  // spatial_mode
        genome[3] = 6.0;  // anchor_color
        genome[5] = 4.0;  // environment
        for i in 0..MAX_OBJECTS {
            let base = 6 + i * 23;
            genome[base + 1] = 6.0;  // color
            genome[base + 7] = 0.0;  // bass kind (Flat)
            genome[base + 11] = 0.0; // sat kind (Flat)
            genome[base + 15] = 0.0; // movement kind (Static)
        }

        let p = Preset::from_genome(&genome);
        assert!(p.master_gain <= 1.0);
        assert!(p.source_count <= 8);
        for obj in &p.objects {
            assert!(obj.volume <= 1.0);
            assert!(obj.x <= 5.0);
        }
    }

    // ---------------------------------------------------------------
    // Default preset structure
    // ---------------------------------------------------------------

    #[test]
    fn default_preset_has_max_objects() {
        let p = Preset::default();
        assert_eq!(p.objects.len(), MAX_OBJECTS);
    }

    #[test]
    fn default_preset_no_active_objects() {
        let p = Preset::default();
        assert_eq!(p.active_object_count(), 0);
    }
}
