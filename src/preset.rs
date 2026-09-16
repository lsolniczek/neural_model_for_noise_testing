/// Preset parameter space definition.
///
/// Maps the full NoiseEngine configuration into a flat f64 vector
/// that the optimizer can search over. Handles encoding/decoding
/// of mixed continuous and discrete parameters.
use crate::movement::MovementConfig;
use noise_generator_core::{
    AcousticEnvironment, BinauralBeatConfig, Ear, ModulatorKind, NoiseColor, NoiseEngine,
    RoomGeometryPreset, RoomMode, WallMaterial,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub const MAX_OBJECTS: usize = 8;

// ── Dimension count ─────────────────────────────────────────────────────────
// Global: master_gain(1) + spatial_mode(1) + source_count(1) + anchor_color(1)
//       + anchor_volume(1) + environment(1) + binaural beat(5) = 11
// Per object (8): active(1) + color(1) + x(1) + y(1) + z(1) + volume(1)
//               + reverb_send(1) + bass_kind(1) + bass_a(1) + bass_b(1) + bass_c(1)
//               + sat_kind(1) + sat_a(1) + sat_b(1) + sat_c(1)
//               + mov_kind(1) + mov_radius(1) + mov_speed(1) + mov_phase(1)
//               + mov_depth_min(1) + mov_depth_max(1) + mov_reverb_min(1)
//               + mov_reverb_max(1) + tint_freq(1) + tint_db(1) = 25
// Total: 11 + 8×25 = 211
//
// NOTE: per-object `spread` is serialized in preset JSON and applied at runtime,
// but intentionally excluded from the optimizer genome for now. That keeps the
// preset / surrogate contract stable while we validate the DSP control.
pub const GENOME_LEN: usize = 11 + MAX_OBJECTS * 25;
pub const LEGACY_GENOME_LEN: usize = 6 + MAX_OBJECTS * 28;
pub const ANCHOR_COLOR_GENE_IDX: usize = 3;
pub const ANCHOR_VOLUME_GENE_IDX: usize = 4;
const BINAURAL_ENABLED_GENE_IDX: usize = 6;
const BINAURAL_LOWER_EAR_GENE_IDX: usize = 10;
const ROOM_MODE_MAX: u8 = 1;
const ROOM_PRESET_MAX: u8 = 3;
const WALL_MATERIAL_MAX: u8 = 6;
const OBJECT_POSITION_SPACE_MAX: u8 = 2;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomDimensionsConfig {
    pub width_m: f32,
    pub depth_m: f32,
    pub height_m: f32,
}

impl RoomDimensionsConfig {
    fn clamp(&mut self) {
        if self.width_m.is_finite() {
            self.width_m = self.width_m.clamp(1.5, 20.0);
        }
        if self.depth_m.is_finite() {
            self.depth_m = self.depth_m.clamp(1.5, 20.0);
        }
        if self.height_m.is_finite() {
            self.height_m = self.height_m.clamp(1.5, 10.0);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomMaterialsConfig {
    pub left: u8,
    pub right: u8,
    pub floor: u8,
    pub ceiling: u8,
    pub back: u8,
    pub front: u8,
}

impl RoomMaterialsConfig {
    fn clamp(&mut self) {
        self.left = self.left.min(WALL_MATERIAL_MAX);
        self.right = self.right.min(WALL_MATERIAL_MAX);
        self.floor = self.floor.min(WALL_MATERIAL_MAX);
        self.ceiling = self.ceiling.min(WALL_MATERIAL_MAX);
        self.back = self.back.min(WALL_MATERIAL_MAX);
        self.front = self.front.min(WALL_MATERIAL_MAX);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoomConfig {
    #[serde(default)]
    pub mode: u8, // 0=Legacy, 1=ImageSource
    #[serde(default)]
    pub preset: Option<u8>, // 0=Bedroom, 1=Studio, 2=Bathroom, 3=Hallway
    #[serde(default)]
    pub dimensions: Option<RoomDimensionsConfig>,
    #[serde(default)]
    pub reflectivity: Option<f32>,
    #[serde(default)]
    pub materials: Option<RoomMaterialsConfig>,
}

impl Default for RoomConfig {
    fn default() -> Self {
        Self {
            mode: 0,
            preset: None,
            dimensions: None,
            reflectivity: None,
            materials: None,
        }
    }
}

impl RoomConfig {
    fn clamp(&mut self) {
        self.mode = self.mode.min(ROOM_MODE_MAX);
        if let Some(preset) = &mut self.preset {
            *preset = (*preset).min(ROOM_PRESET_MAX);
        }
        if let Some(dimensions) = &mut self.dimensions {
            dimensions.clamp();
        }
        if let Some(reflectivity) = &mut self.reflectivity {
            if reflectivity.is_finite() {
                *reflectivity = reflectivity.clamp(0.0, 1.5);
            }
        }
        if let Some(materials) = &mut self.materials {
            materials.clamp();
        }
    }

    pub fn uses_image_source(&self) -> bool {
        self.mode == RoomMode::ImageSource as u8
    }

    fn room_mode(&self) -> RoomMode {
        RoomMode::from_u8(self.mode)
    }
}

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
        self.kind = self.kind.min(6);
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
            5 => {
                // Isochronic: freq 1.0–40.0 Hz, depth 0.0–1.0, duty cycle 0.2–0.8
                self.param_a = self.param_a.clamp(1.0, 40.0);
                self.param_b = self.param_b.clamp(0.0, 1.0);
                self.param_c = self.param_c.clamp(0.2, 0.8);
            }
            6 => {
                // RandomPulse: rate 0.1–20 bursts/s, depth 0.0–1.0, duration 20–500 ms
                self.param_a = self.param_a.clamp(0.1, 20.0);
                self.param_b = self.param_b.clamp(0.0, 1.0);
                self.param_c = self.param_c.clamp(20.0, 500.0);
            }
            _ => {} // Flat: no params
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BinauralBeatPresetConfig {
    pub enabled: bool,
    pub center_frequency_hz: f32,
    pub beat_frequency_hz: f32,
    pub gain_db: f32,
    /// `0=Left`, `1=Right`.
    pub lower_frequency_ear: u8,
}

impl Default for BinauralBeatPresetConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            center_frequency_hz: 400.0,
            beat_frequency_hz: 6.0,
            gain_db: -40.0,
            lower_frequency_ear: 0,
        }
    }
}

impl BinauralBeatPresetConfig {
    fn clamp(&mut self) {
        self.center_frequency_hz = self.center_frequency_hz.clamp(100.0, 1_000.0);
        self.beat_frequency_hz = self.beat_frequency_hz.clamp(0.0, 40.0);
        self.gain_db = self.gain_db.clamp(-80.0, -24.0);
        self.lower_frequency_ear = self.lower_frequency_ear.min(1);
    }

    fn as_engine_config(&self) -> BinauralBeatConfig {
        BinauralBeatConfig {
            enabled: self.enabled,
            center_frequency_hz: self.center_frequency_hz,
            beat_frequency_hz: self.beat_frequency_hz,
            gain_db: self.gain_db,
            lower_frequency_ear: if self.lower_frequency_ear == 1 {
                Ear::Right
            } else {
                Ear::Left
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectConfig {
    pub active: bool,
    pub color: u8, // 0–6
    /// Coordinate space: 0=WorldMeters, 1=RoomNormalized, 2=RoomMeters.
    #[serde(default)]
    pub position_space: u8,
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub volume: f32,
    pub reverb_send: f32,
    /// Per-object apparent source width / decorrelation. 0 = point source.
    #[serde(default)]
    pub spread: f32,
    pub bass_mod: ModConfig,
    pub satellite_mod: ModConfig,
    #[serde(default)]
    pub movement: MovementConfig,
    /// Per-object spectral tint: peak EQ frequency (Hz). 0 = disabled.
    #[serde(default)]
    pub tint_freq: f32,
    /// Per-object spectral tint: gain (dB). 0 = flat passthrough.
    #[serde(default)]
    pub tint_db: f32,
    /// Source kind: 0 = Noise (default), 1 = Tone (pure sine).
    #[serde(default, skip_serializing)]
    pub source_kind: u8,
    /// Tone frequency (Hz). Only used when source_kind = 1.
    #[serde(default = "default_tone_freq", skip_serializing)]
    pub tone_freq: f32,
    /// Tone amplitude (0.0–1.0). Only used when source_kind = 1.
    #[serde(default, skip_serializing)]
    pub tone_amplitude: f32,
}

fn default_tone_freq() -> f32 {
    200.0
}

impl Default for ObjectConfig {
    fn default() -> Self {
        ObjectConfig {
            active: false,
            color: 0,
            position_space: 0,
            x: 0.0,
            y: 0.0,
            z: 1.0,
            volume: 1.0,
            reverb_send: 0.1,
            spread: 0.0,
            bass_mod: ModConfig::default(),
            satellite_mod: ModConfig::default(),
            movement: MovementConfig::default(),
            tint_freq: 0.0,
            tint_db: 0.0,
            source_kind: 0,
            tone_freq: 200.0,
            tone_amplitude: 0.0,
        }
    }
}

impl ObjectConfig {
    fn clamp(&mut self) {
        self.color = self.color.min(7); // 0-7: White,Pink,Brown,Green,Grey,Black,SSN,Blue
        self.position_space = self.position_space.min(OBJECT_POSITION_SPACE_MAX);
        match self.position_space {
            1 => {
                self.x = self.x.clamp(-1.0, 1.0);
                self.y = self.y.clamp(-1.0, 1.0);
                self.z = self.z.clamp(-1.0, 1.0);
            }
            2 => {
                self.x = self.x.clamp(-10.0, 10.0);
                self.y = self.y.clamp(-5.0, 5.0);
                self.z = self.z.clamp(-10.0, 10.0);
            }
            _ => {
                self.x = self.x.clamp(-5.0, 5.0);
                self.y = self.y.clamp(-3.0, 3.0);
                self.z = self.z.clamp(-5.0, 5.0);
            }
        }
        self.volume = self.volume.clamp(0.0, 1.0);
        self.reverb_send = self.reverb_send.clamp(0.0, 1.0);
        self.spread = self.spread.clamp(0.0, 1.0);
        self.bass_mod.clamp();
        self.satellite_mod.clamp();
        self.movement.clamp();
        // Per-object color tint (DSP Priority 2b). freq=0 means disabled.
        if self.tint_freq > 0.0 {
            self.tint_freq = self.tint_freq.clamp(100.0, 8000.0);
            self.tint_db = self.tint_db.clamp(-6.0, 6.0);
        } else {
            self.tint_db = 0.0;
        }
        // Tone source
        self.source_kind = self.source_kind.min(1);
        if self.source_kind == 1 {
            self.tone_freq = self.tone_freq.clamp(20.0, 8000.0);
            self.tone_amplitude = self.tone_amplitude.clamp(0.0, 1.0);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "PresetWire")]
pub struct Preset {
    pub master_gain: f32,
    pub spatial_mode: u8,  // 0=Stereo, 1=Immersive
    pub source_count: u32, // 2–8 (for Immersive)
    pub anchor_color: u8,  // 0–6
    pub anchor_volume: f32,
    pub environment: u8, // 0–4 (AcousticEnvironment)
    #[serde(default)]
    pub room: RoomConfig,
    pub binaural_beat: BinauralBeatPresetConfig,
    pub objects: Vec<ObjectConfig>,
}

#[derive(Deserialize)]
struct PresetWire {
    pub master_gain: f32,
    pub spatial_mode: u8,
    pub source_count: u32,
    pub anchor_color: u8,
    pub anchor_volume: f32,
    pub environment: u8,
    #[serde(default)]
    pub room: RoomConfig,
    #[serde(default)]
    pub binaural_beat: Option<BinauralBeatPresetConfig>,
    pub objects: Vec<ObjectConfig>,
}

fn explicit_periodic_frequency(config: &ModConfig) -> Option<f32> {
    matches!(config.kind, 1 | 4 | 5)
        .then_some(config.param_a)
        .filter(|frequency| frequency.is_finite())
}

fn migrate_legacy_binaural_beat(objects: &[ObjectConfig]) -> BinauralBeatPresetConfig {
    let mut tones = objects.iter().enumerate().filter(|(_, object)| {
        object.active
            && object.volume > 0.0
            && object.source_kind == 1
            && object.tone_amplitude > 0.0
    });
    let Some((first_index, first)) = tones.next() else {
        return BinauralBeatPresetConfig::default();
    };

    let ignored: Vec<usize> = tones.map(|(index, _)| index).collect();
    if !ignored.is_empty() {
        eprintln!(
            "warning: migrated legacy Tone object {first_index} to the global binaural beat; ignored additional Tone object slots {ignored:?}"
        );
    }

    let mut migrated = BinauralBeatPresetConfig {
        enabled: true,
        center_frequency_hz: first.tone_freq,
        beat_frequency_hz: explicit_periodic_frequency(&first.satellite_mod)
            .or_else(|| explicit_periodic_frequency(&first.bass_mod))
            .unwrap_or(6.0),
        gain_db: -40.0,
        lower_frequency_ear: 0,
    };
    migrated.clamp();
    migrated
}

fn retire_legacy_tone_objects(objects: &mut [ObjectConfig]) {
    for object in objects {
        if object.source_kind == 1 {
            object.active = false;
            object.source_kind = 0;
            object.tone_freq = default_tone_freq();
            object.tone_amplitude = 0.0;
        }
    }
}

impl From<PresetWire> for Preset {
    fn from(wire: PresetWire) -> Self {
        let mut objects = wire.objects;
        let binaural_beat = wire
            .binaural_beat
            .unwrap_or_else(|| migrate_legacy_binaural_beat(&objects));
        retire_legacy_tone_objects(&mut objects);
        Self {
            master_gain: wire.master_gain,
            spatial_mode: wire.spatial_mode,
            source_count: wire.source_count,
            anchor_color: wire.anchor_color,
            anchor_volume: wire.anchor_volume,
            environment: wire.environment,
            room: wire.room,
            binaural_beat,
            objects,
        }
    }
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
            room: RoomConfig::default(),
            binaural_beat: BinauralBeatPresetConfig::default(),
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
        self.room.clamp();
        self.binaural_beat.clamp();
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
        engine.set_binaural_beat_config(self.binaural_beat.as_engine_config());

        let env = match self.environment {
            0 => AcousticEnvironment::AnechoicChamber,
            1 => AcousticEnvironment::FocusRoom,
            2 => AcousticEnvironment::OpenLounge,
            3 => AcousticEnvironment::VastSpace,
            _ => AcousticEnvironment::DeepSanctuary,
        };
        engine.set_acoustic_environment(env);
        engine.set_room_mode(self.room.room_mode());
        if let Some(preset) = self.room.preset {
            engine.set_room_preset(RoomGeometryPreset::from_u8(preset));
        }
        if let Some(dimensions) = &self.room.dimensions {
            engine.set_room_dimensions(dimensions.width_m, dimensions.depth_m, dimensions.height_m);
        }
        if let Some(reflectivity) = self.room.reflectivity {
            engine.set_room_reflectivity(reflectivity);
        }
        if let Some(materials) = &self.room.materials {
            engine.set_wall_materials(
                WallMaterial::from_u8(materials.left),
                WallMaterial::from_u8(materials.right),
                WallMaterial::from_u8(materials.floor),
                WallMaterial::from_u8(materials.ceiling),
                WallMaterial::from_u8(materials.back),
                WallMaterial::from_u8(materials.front),
            );
        }

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
            match obj.position_space {
                1 => engine.set_object_room_position(i as u32, obj.x, obj.y, obj.z),
                2 => engine.set_object_room_position_meters(i as u32, obj.x, obj.y, obj.z),
                _ => {}
            }
            engine.set_object_spread(i as u32, if obj.active { obj.spread } else { 0.0 });
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
            // Color tint (DSP Priority 2b): per-object spectral EQ.
            // freq=0 means disabled → set to default flat.
            if obj.tint_freq >= 100.0 && obj.tint_db.abs() > 0.01 {
                engine.set_object_color_tint(i as u32, obj.tint_freq, obj.tint_db);
            }
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
        g.push(if self.binaural_beat.enabled { 1.0 } else { 0.0 });
        g.push(self.binaural_beat.center_frequency_hz as f64);
        g.push(self.binaural_beat.beat_frequency_hz as f64);
        g.push(self.binaural_beat.gain_db as f64);
        g.push(self.binaural_beat.lower_frequency_ear as f64);

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
            g.push(encode_mod_param_b(
                obj.satellite_mod.kind,
                obj.satellite_mod.param_b,
            ));
            g.push(obj.satellite_mod.param_c as f64);
            g.push(obj.movement.kind as f64);
            g.push(obj.movement.radius as f64);
            g.push(obj.movement.speed as f64);
            g.push(obj.movement.phase as f64);
            g.push(obj.movement.depth_min as f64);
            g.push(obj.movement.depth_max as f64);
            g.push(obj.movement.reverb_min as f64);
            g.push(obj.movement.reverb_max as f64);
            // Color tint (DSP Priority 2b)
            g.push(obj.tint_freq as f64);
            g.push(obj.tint_db as f64);
        }

        g
    }

    /// Decode from a flat f64 vector. Values are clamped to valid ranges.
    /// Spread defaults to 0.0 for every object — see `from_genome_with_spread`
    /// when you need to preserve spread from a seed preset.
    pub fn from_genome(g: &[f64]) -> Self {
        Self::from_genome_with_spread(g, &[0.0_f32; MAX_OBJECTS])
    }

    /// Decode from a flat f64 vector while injecting per-slot spread values
    /// that are not part of the genome encoding. Used by the optimizer to
    /// preserve spread from a seed preset across genome roundtrips.
    pub fn from_genome_with_spread(g: &[f64], spread_per_slot: &[f32; MAX_OBJECTS]) -> Self {
        assert!(
            matches!(g.len(), GENOME_LEN | LEGACY_GENOME_LEN),
            "genome length must be {GENOME_LEN} or legacy {LEGACY_GENOME_LEN}, got {}",
            g.len()
        );
        let legacy = g.len() == LEGACY_GENOME_LEN;

        let mut preset = Preset {
            master_gain: g[0] as f32,
            spatial_mode: g[1].round() as u8,
            source_count: g[2].round() as u32,
            anchor_color: g[3].round() as u8,
            anchor_volume: g[4] as f32,
            environment: g[5].round() as u8,
            room: RoomConfig::default(),
            binaural_beat: if legacy {
                BinauralBeatPresetConfig::default()
            } else {
                BinauralBeatPresetConfig {
                    enabled: g[BINAURAL_ENABLED_GENE_IDX] > 0.5,
                    center_frequency_hz: g[7] as f32,
                    beat_frequency_hz: g[8] as f32,
                    gain_db: g[9] as f32,
                    lower_frequency_ear: g[BINAURAL_LOWER_EAR_GENE_IDX].round() as u8,
                }
            },
            objects: Vec::with_capacity(MAX_OBJECTS),
        };

        for i in 0..MAX_OBJECTS {
            let base = if legacy { 6 + i * 28 } else { 11 + i * 25 };
            let obj = ObjectConfig {
                active: g[base] > 0.5,
                color: g[base + 1].round() as u8,
                position_space: 0,
                x: g[base + 2] as f32,
                y: g[base + 3] as f32,
                z: g[base + 4] as f32,
                volume: g[base + 5] as f32,
                reverb_send: g[base + 6] as f32,
                spread: spread_per_slot[i],
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
                tint_freq: g[base + 23] as f32,
                tint_db: g[base + 24] as f32,
                source_kind: if legacy {
                    g[base + 25].round() as u8
                } else {
                    0
                },
                tone_freq: if legacy {
                    g[base + 26] as f32
                } else {
                    default_tone_freq()
                },
                tone_amplitude: if legacy { g[base + 27] as f32 } else { 0.0 },
            };
            preset.objects.push(obj);
        }

        if legacy {
            preset.binaural_beat = migrate_legacy_binaural_beat(&preset.objects);
            retire_legacy_tone_objects(&mut preset.objects);
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
        indices.push(BINAURAL_ENABLED_GENE_IDX);
        indices.push(BINAURAL_LOWER_EAR_GENE_IDX);

        // Per-object discrete params
        for i in 0..MAX_OBJECTS {
            let base = 11 + i * 25;
            indices.push(base); // active (0/1)
            indices.push(base + 1); // color
            indices.push(base + 7); // bass_mod.kind
            indices.push(base + 11); // satellite_mod.kind
            indices.push(base + 15); // movement.kind
        }
        indices
    }

    /// Parameter bounds: (min, max) for each gene.
    pub fn bounds() -> Vec<(f64, f64)> {
        let mut b = Vec::with_capacity(GENOME_LEN);

        // Global
        b.push((0.1, 1.0)); // master_gain
        b.push((0.0, 1.0)); // spatial_mode (discrete: 0 or 1)
        b.push((2.0, 8.0)); // source_count
        b.push((0.0, 6.0)); // anchor_color
        b.push((0.0, 1.0)); // anchor_volume
        b.push((0.0, 4.0)); // environment
        b.push((0.0, 1.0)); // binaural beat enabled
        b.push((100.0, 1000.0)); // binaural beat center frequency
        b.push((0.0, 40.0)); // binaural beat frequency difference
        b.push((-80.0, -24.0)); // binaural beat digital gain (dBFS)
        b.push((0.0, 1.0)); // lower-frequency ear (0=Left, 1=Right)

        // Per-object (×8)
        for _ in 0..MAX_OBJECTS {
            b.push((0.0, 1.0)); // active
            b.push((0.0, 6.0)); // color
            b.push((-5.0, 5.0)); // x
            b.push((-3.0, 3.0)); // y
            b.push((-5.0, 5.0)); // z
            b.push((0.0, 1.0)); // volume
            b.push((0.0, 1.0)); // reverb_send
            b.push((0.0, 6.0)); // bass_mod.kind (0=Flat,1=SineLfo,2=Breathing,3=Stochastic,4=NeuralLfo,5=Isochronic,6=RandomPulse)
            b.push((0.0, 40.0)); // bass_mod.param_a (max covers NeuralLfo/Isochronic 40 Hz)
            b.push((0.0, 1.0)); // bass_mod.param_b
            b.push((0.0, 0.8)); // bass_mod.param_c (covers Isochronic duty cycle 0.2–0.8)
            b.push((0.0, 6.0)); // sat_mod.kind (same range as bass)
            b.push((0.0, 40.0)); // sat_mod.param_a
            b.push((0.0, 1.0)); // sat_mod.param_b
            b.push((0.0, 0.8)); // sat_mod.param_c
            b.push((0.0, 5.0)); // movement.kind
            b.push((0.0, 5.0)); // movement.radius
            b.push((0.0, 5.0)); // movement.speed
            b.push((0.0, 6.283)); // movement.phase
            b.push((0.5, 5.0)); // movement.depth_min
            b.push((0.5, 6.0)); // movement.depth_max
            b.push((0.0, 1.0)); // movement.reverb_min
            b.push((0.0, 1.0)); // movement.reverb_max
                                // Color tint (DSP Priority 2b): per-object spectral EQ
            b.push((0.0, 8000.0)); // tint_freq (0 = disabled, 100-8000 when active)
            b.push((-6.0, 6.0)); // tint_db (-6 to +6 dB)
        }

        b
    }

    /// Optimizer bounds with the global anchor disabled.
    ///
    /// This freezes `anchor_volume` at 0.0 so every audible source is rendered
    /// through the normal object/HRTF path instead of the non-spatial anchor.
    /// `anchor_color` is also frozen to 0 to avoid wasting a discrete search
    /// dimension on a muted parameter.
    pub fn bounds_with_anchor_disabled() -> Vec<(f64, f64)> {
        let mut b = Self::bounds();
        b[ANCHOR_COLOR_GENE_IDX] = (0.0, 0.0);
        b[ANCHOR_VOLUME_GENE_IDX] = (0.0, 0.0);
        b
    }

    /// Force the anchor genes to a muted state inside a flat optimizer genome.
    pub fn disable_anchor_in_genome(genome: &mut [f64]) {
        assert!(genome.len() >= GENOME_LEN, "genome too short");
        genome[ANCHOR_COLOR_GENE_IDX] = 0.0;
        genome[ANCHOR_VOLUME_GENE_IDX] = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // GENOME_LEN
    // ---------------------------------------------------------------

    #[test]
    fn genome_len_is_211() {
        assert_eq!(GENOME_LEN, 11 + MAX_OBJECTS * 25);
        assert_eq!(GENOME_LEN, 211);
        assert_eq!(LEGACY_GENOME_LEN, 230);
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

    #[test]
    fn anchor_disabled_bounds_freeze_anchor_genes() {
        let b = Preset::bounds_with_anchor_disabled();
        assert_eq!(b[ANCHOR_COLOR_GENE_IDX], (0.0, 0.0));
        assert_eq!(b[ANCHOR_VOLUME_GENE_IDX], (0.0, 0.0));
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
        // 6 global + 8 * 5 per-object = 46
        let indices = Preset::discrete_gene_indices();
        assert_eq!(indices.len(), 6 + MAX_OBJECTS * 5);
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
            assert!((a - b).abs() < 1e-6, "Gene {i} differs: {a} vs {b}");
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
        preset.objects[0].bass_mod = ModConfig {
            kind: 1,
            param_a: 0.5,
            param_b: 0.8,
            param_c: 0.0,
        };
        preset.objects[0].satellite_mod = ModConfig {
            kind: 4,
            param_a: 10.0,
            param_b: 0.6,
            param_c: 0.0,
        };

        let genome = preset.to_genome();
        let decoded = Preset::from_genome(&genome);

        assert!(decoded.objects[0].active);
        assert_eq!(decoded.objects[0].color, 3);
        assert!((decoded.objects[0].volume - 0.75).abs() < 1e-5);
        assert_eq!(decoded.objects[0].bass_mod.kind, 1);
        assert!((decoded.objects[0].bass_mod.param_a - 0.5).abs() < 1e-5);
    }

    #[test]
    fn genome_roundtrip_preserves_global_binaural_beat() {
        let mut preset = Preset::default();
        preset.binaural_beat = BinauralBeatPresetConfig {
            enabled: true,
            center_frequency_hz: 432.0,
            beat_frequency_hz: 12.0,
            gain_db: -36.0,
            lower_frequency_ear: 1,
        };

        let decoded = Preset::from_genome(&preset.to_genome());
        assert_eq!(decoded.binaural_beat, preset.binaural_beat);
    }

    #[test]
    fn legacy_json_migrates_first_tone_and_uses_default_beat() {
        let preset: Preset = serde_json::from_str(include_str!(
            "../presets/normal_set_flow_v3_adhd_tuned_v3.json"
        ))
        .expect("legacy tone preset should migrate");

        assert!(preset.binaural_beat.enabled);
        assert_eq!(preset.binaural_beat.center_frequency_hz, 220.0);
        assert_eq!(preset.binaural_beat.beat_frequency_hz, 6.0);
        assert_eq!(preset.binaural_beat.gain_db, -40.0);
        assert_eq!(preset.binaural_beat.lower_frequency_ear, 0);
        assert!(!preset.objects[4].active);
        assert!(!preset.objects[5].active);
        assert!(preset.objects[4..=5]
            .iter()
            .all(|object| object.source_kind == 0 && object.tone_amplitude == 0.0));

        let saved = serde_json::to_string(&preset).expect("migrated preset should serialize");
        let reloaded: Preset = serde_json::from_str(&saved).expect("migrated preset should reload");
        assert_eq!(reloaded.binaural_beat, preset.binaural_beat);
        assert!(reloaded.objects[4..=5]
            .iter()
            .all(|object| !object.active && object.source_kind == 0));
    }

    #[test]
    fn legacy_migration_prefers_satellite_periodic_frequency() {
        let mut first = ObjectConfig::default();
        first.active = true;
        first.source_kind = 1;
        first.tone_freq = 300.0;
        first.tone_amplitude = 0.5;
        first.bass_mod.kind = 4;
        first.bass_mod.param_a = 10.0;
        first.satellite_mod.kind = 5;
        first.satellite_mod.param_a = 14.0;

        let migrated = migrate_legacy_binaural_beat(&[first]);
        assert_eq!(migrated.center_frequency_hz, 300.0);
        assert_eq!(migrated.beat_frequency_hz, 14.0);
    }

    #[test]
    fn new_json_omits_legacy_tone_fields() {
        let serialized = serde_json::to_value(Preset::default()).unwrap();
        assert!(serialized.get("binaural_beat").is_some());
        for object in serialized["objects"].as_array().unwrap() {
            assert!(object.get("source_kind").is_none());
            assert!(object.get("tone_freq").is_none());
            assert!(object.get("tone_amplitude").is_none());
        }
    }

    #[test]
    fn legacy_genome_migrates_tone_to_global_binaural_beat() {
        let mut genome = vec![0.0; LEGACY_GENOME_LEN];
        genome[0] = 0.8;
        genome[1] = 1.0;
        genome[2] = 2.0;
        let base = 6;
        genome[base] = 1.0;
        genome[base + 5] = 1.0;
        genome[base + 7] = 4.0;
        genome[base + 8] = 9.0;
        genome[base + 9] = 0.5;
        genome[base + 11] = 4.0;
        genome[base + 12] = 13.0;
        genome[base + 13] = 0.5;
        genome[base + 25] = 1.0;
        genome[base + 26] = 250.0;
        genome[base + 27] = 0.5;

        let migrated = Preset::from_genome(&genome);
        assert!(migrated.binaural_beat.enabled);
        assert_eq!(migrated.binaural_beat.center_frequency_hz, 250.0);
        assert_eq!(migrated.binaural_beat.beat_frequency_hz, 13.0);
        assert!(!migrated.objects[0].active);
        assert_eq!(migrated.objects[0].source_kind, 0);
        assert_eq!(migrated.objects[0].tone_amplitude, 0.0);
        assert_eq!(migrated.to_genome().len(), GENOME_LEN);
    }

    #[test]
    fn legacy_json_without_spread_defaults_to_zero() {
        let preset: Preset =
            serde_json::from_str(include_str!("../presets/normal_set_shield_v3.json"))
                .expect("legacy preset should deserialize without spread");
        assert!(preset.objects.iter().all(|obj| obj.spread == 0.0));
        assert!(preset.objects.iter().all(|obj| obj.position_space == 0));
        assert_eq!(preset.room, RoomConfig::default());
    }

    #[test]
    fn from_genome_drops_spread_but_with_spread_preserves_it() {
        // Regression: `from_genome` always zeroed spread because spread is
        // intentionally outside the optimizer genome. Without
        // `from_genome_with_spread`, an init-preset's spread values would be
        // silently lost when the optimizer roundtrips the seed through the
        // genome encoding.
        let mut preset = Preset::default();
        preset.objects[2].active = true;
        preset.objects[2].spread = 1.0;
        preset.objects[4].active = true;
        preset.objects[4].spread = 0.85;

        let genome = preset.to_genome();

        let decoded_default = Preset::from_genome(&genome);
        assert!(decoded_default.objects.iter().all(|obj| obj.spread == 0.0));

        let mut spread_per_slot = [0.0_f32; MAX_OBJECTS];
        for (i, obj) in preset.objects.iter().enumerate() {
            spread_per_slot[i] = obj.spread;
        }
        let decoded_with_spread = Preset::from_genome_with_spread(&genome, &spread_per_slot);
        assert!((decoded_with_spread.objects[2].spread - 1.0).abs() < 1e-6);
        assert!((decoded_with_spread.objects[4].spread - 0.85).abs() < 1e-6);
        assert_eq!(decoded_with_spread.objects[0].spread, 0.0);
    }

    #[test]
    fn from_genome_with_spread_clamps_out_of_range_values() {
        let preset = Preset::default();
        let genome = preset.to_genome();
        let mut spread = [0.0_f32; MAX_OBJECTS];
        spread[1] = 5.0;
        spread[3] = -2.0;
        let decoded = Preset::from_genome_with_spread(&genome, &spread);
        // Preset::clamp() runs at the end of from_genome_with_spread.
        assert_eq!(decoded.objects[1].spread, 1.0);
        assert_eq!(decoded.objects[3].spread, 0.0);
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
        assert!(
            (enc_min - 0.0).abs() < 1e-6,
            "decay_ms=10 should encode to ~0, got {enc_min}"
        );

        // Max: 500 → 1.0
        let enc_max = encode_mod_param_b(3, 500.0);
        assert!(
            (enc_max - 1.0).abs() < 1e-6,
            "decay_ms=500 should encode to ~1, got {enc_max}"
        );

        // Decode back
        let dec_min = decode_mod_param_b(3, 0.0);
        assert!(
            (dec_min - 10.0).abs() < 0.1,
            "genome=0 should decode to ~10, got {dec_min}"
        );

        let dec_max = decode_mod_param_b(3, 1.0);
        assert!(
            (dec_max - 500.0).abs() < 0.1,
            "genome=1 should decode to ~500, got {dec_max}"
        );
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
            param_a: 5.0,   // lambda
            param_b: 250.0, // decay_ms (midrange)
            param_c: 0.2,   // min_gain
        };

        let genome = preset.to_genome();

        // The genome param_b slot should be in [0, 1]
        let bass_param_b_idx = 11 + 9; // object base + bass param_b
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
    fn clamp_enforces_room_normalized_position_bounds() {
        let mut p = Preset::default();
        p.objects[0].position_space = 1;
        p.objects[0].x = 100.0;
        p.objects[0].y = -100.0;
        p.objects[0].z = 100.0;
        p.clamp();
        assert_eq!(p.objects[0].x, 1.0);
        assert_eq!(p.objects[0].y, -1.0);
        assert_eq!(p.objects[0].z, 1.0);
    }

    #[test]
    fn clamp_enforces_mod_params() {
        let mut p = Preset::default();
        p.objects[0].bass_mod = ModConfig {
            kind: 1,
            param_a: 100.0,
            param_b: -1.0,
            param_c: 0.0,
        };
        p.clamp();
        // SineLfo: freq clamped to [0.01, 2.0], depth to [0, 1]
        assert_eq!(p.objects[0].bass_mod.param_a, 2.0);
        assert_eq!(p.objects[0].bass_mod.param_b, 0.0);
    }

    #[test]
    fn clamp_enforces_spread_bounds() {
        let mut p = Preset::default();
        p.objects[0].spread = 2.0;
        p.clamp();
        assert_eq!(p.objects[0].spread, 1.0);

        p.objects[0].spread = -1.0;
        p.clamp();
        assert_eq!(p.objects[0].spread, 0.0);
    }

    // ---------------------------------------------------------------
    // from_genome with out-of-bounds values → clamp corrects
    // ---------------------------------------------------------------

    #[test]
    fn from_genome_clamps_out_of_bounds() {
        let mut genome = vec![999.0; GENOME_LEN];
        // Set discrete values to valid-ish ranges so they don't overflow u8
        genome[1] = 1.0; // spatial_mode
        genome[3] = 6.0; // anchor_color
        genome[5] = 4.0; // environment
        for i in 0..MAX_OBJECTS {
            let base = 11 + i * 25;
            genome[base + 1] = 6.0; // color
            genome[base + 7] = 0.0; // bass kind (Flat)
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

    #[test]
    fn disable_anchor_in_genome_zeros_anchor_fields() {
        let mut genome = Preset::default().to_genome();
        genome[ANCHOR_COLOR_GENE_IDX] = 6.0;
        genome[ANCHOR_VOLUME_GENE_IDX] = 0.73;
        Preset::disable_anchor_in_genome(&mut genome);
        assert_eq!(genome[ANCHOR_COLOR_GENE_IDX], 0.0);
        assert_eq!(genome[ANCHOR_VOLUME_GENE_IDX], 0.0);
        let decoded = Preset::from_genome(&genome);
        assert_eq!(decoded.anchor_color, 0);
        assert_eq!(decoded.anchor_volume, 0.0);
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

    #[test]
    fn apply_to_engine_sets_object_spread() {
        let engine = NoiseEngine::new(48_000, 0.8);
        let mut preset = Preset::default();
        preset.source_count = 2;
        preset.objects[0].active = true;
        preset.objects[0].spread = 0.65;
        preset.objects[1].active = false;
        preset.objects[1].spread = 0.4;

        preset.apply_to_engine(&engine);

        assert!((engine.object_spread(0) - 0.65).abs() < 1e-6);
        assert_eq!(engine.object_spread(1), 0.0);
    }

    #[test]
    fn apply_to_engine_sets_global_binaural_beat() {
        let engine = NoiseEngine::new(48_000, 0.8);
        let mut preset = Preset::default();
        preset.binaural_beat = BinauralBeatPresetConfig {
            enabled: true,
            center_frequency_hz: 440.0,
            beat_frequency_hz: 8.0,
            gain_db: -40.0,
            lower_frequency_ear: 0,
        };

        preset.apply_to_engine(&engine);

        let applied = engine.binaural_beat_config();
        assert!(applied.enabled);
        assert_eq!(applied.center_frequency_hz, 440.0);
        assert_eq!(applied.beat_frequency_hz, 8.0);
        assert_eq!(applied.gain_db, -40.0);
        assert_eq!(applied.lower_frequency_ear, Ear::Left);
    }

    #[test]
    fn apply_to_engine_sets_image_source_room_mode() {
        let engine = NoiseEngine::new(48_000, 0.8);
        let mut preset = Preset::default();
        preset.room.mode = RoomMode::ImageSource as u8;
        preset.room.preset = Some(2);

        preset.apply_to_engine(&engine);

        assert_eq!(engine.room_mode(), RoomMode::ImageSource);
    }
}
