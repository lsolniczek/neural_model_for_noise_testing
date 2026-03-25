/// Movement Controller — Spatial Trajectory Engine.
///
/// Rust port of the Swift `MovementController`. Drives spatial object
/// positions over time using deterministic patterns (orbit, figure-eight,
/// pendulum, depth-breathing) and a seeded random walk.
///
/// During evaluation the controller is stepped in discrete time increments
/// (matching the render chunk size) so that the rendered audio reflects
/// object motion through the HRTF pipeline.

use noice_generator_core::NoiseEngine;
use serde::{Deserialize, Serialize};

// ── Movement pattern enum ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MovementPattern {
    /// No movement — object stays at its initial position.
    Static,
    /// Circular orbit around the listener in the XZ plane.
    Orbit,
    /// Lemniscate (∞) path in the XZ plane.
    FigureEight,
    /// Brownian-style random walk constrained within a radius.
    RandomWalk,
    /// Sinusoidal depth oscillation along Z with reverb modulation.
    DepthBreathing,
    /// Arc-pendulum swinging in the XZ plane.
    Pendulum,
}

impl MovementPattern {
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => MovementPattern::Static,
            1 => MovementPattern::Orbit,
            2 => MovementPattern::FigureEight,
            3 => MovementPattern::RandomWalk,
            4 => MovementPattern::DepthBreathing,
            5 => MovementPattern::Pendulum,
            _ => MovementPattern::Static,
        }
    }

    pub fn to_u8(self) -> u8 {
        match self {
            MovementPattern::Static => 0,
            MovementPattern::Orbit => 1,
            MovementPattern::FigureEight => 2,
            MovementPattern::RandomWalk => 3,
            MovementPattern::DepthBreathing => 4,
            MovementPattern::Pendulum => 5,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MovementPattern::Static => "static",
            MovementPattern::Orbit => "orbit",
            MovementPattern::FigureEight => "figure-8",
            MovementPattern::RandomWalk => "random walk",
            MovementPattern::DepthBreathing => "depth breathing",
            MovementPattern::Pendulum => "pendulum",
        }
    }
}

// ── Per-object movement configuration (stored in preset JSON) ───────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovementConfig {
    /// Movement pattern type.
    pub kind: u8,
    /// Radius / amplitude of the movement (meters). Used by orbit, figure-eight,
    /// random walk (constraint radius), and pendulum (arm length).
    pub radius: f32,
    /// Speed multiplier — controls angular velocity or walk speed.
    /// For orbit/figure-eight/pendulum this is radians per second.
    /// For depth-breathing this is the breathing cycle frequency.
    pub speed: f32,
    /// Initial phase offset (radians). Lets multiple objects start at
    /// different points along the same trajectory.
    pub phase: f32,
    /// Depth-breathing: minimum Z distance. Also used as pendulum amplitude.
    pub depth_min: f32,
    /// Depth-breathing: maximum Z distance.
    pub depth_max: f32,
    /// Depth-breathing: minimum reverb send at closest point.
    pub reverb_min: f32,
    /// Depth-breathing: maximum reverb send at farthest point.
    pub reverb_max: f32,
}

impl Default for MovementConfig {
    fn default() -> Self {
        MovementConfig {
            kind: 0, // Static
            radius: 0.0,
            speed: 0.0,
            phase: 0.0,
            depth_min: 1.0,
            depth_max: 4.0,
            reverb_min: 0.05,
            reverb_max: 0.5,
        }
    }
}

impl MovementConfig {
    pub fn pattern(&self) -> MovementPattern {
        MovementPattern::from_u8(self.kind)
    }

    /// Clamp parameters to valid ranges.
    pub fn clamp(&mut self) {
        self.kind = self.kind.min(5);
        self.radius = self.radius.clamp(0.0, 5.0);
        self.speed = self.speed.clamp(0.0, 5.0);
        self.phase = self.phase.clamp(0.0, std::f32::consts::TAU);
        self.depth_min = self.depth_min.clamp(0.5, 5.0);
        self.depth_max = self.depth_max.clamp(0.5, 6.0);
        self.reverb_min = self.reverb_min.clamp(0.0, 1.0);
        self.reverb_max = self.reverb_max.clamp(0.0, 1.0);
    }
}

// ── Per-satellite runtime state ─────────────────────────────────────────────

struct SatelliteState {
    index: u32,
    pattern: MovementPattern,
    radius: f32,
    speed: f64,
    phase: f64,

    // Initial (base) position from preset — used by depth-breathing
    base_x: f32,
    base_y: f32,

    // Depth-breathing params
    min_z: f32,
    max_z: f32,
    min_reverb: f32,
    max_reverb: f32,

    // Random walk state
    rw_x: f32,
    rw_z: f32,
    rw_vel_x: f32,
    rw_vel_z: f32,
    /// Simple xorshift RNG state for deterministic random walk.
    rng_state: u64,
}

impl SatelliteState {
    /// Deterministic random float in [-1, 1] using xorshift64.
    fn rand_f32(&mut self) -> f32 {
        self.rng_state ^= self.rng_state << 13;
        self.rng_state ^= self.rng_state >> 7;
        self.rng_state ^= self.rng_state << 17;
        // Map to [-1, 1]
        (self.rng_state as i64 as f64 / i64::MAX as f64) as f32
    }
}

// ── Movement controller ─────────────────────────────────────────────────────

pub struct MovementController {
    satellites: Vec<SatelliteState>,
}

impl MovementController {
    /// Create a movement controller from preset object configurations.
    ///
    /// `objects` provides the initial position and movement config for each object.
    /// Only objects with a non-static movement pattern are tracked.
    pub fn from_preset(preset: &crate::preset::Preset) -> Self {
        let mut satellites = Vec::new();

        for (i, obj) in preset.objects.iter().enumerate() {
            if !obj.active {
                continue;
            }
            let pattern = obj.movement.pattern();
            if pattern == MovementPattern::Static {
                continue;
            }

            let mv = &obj.movement;
            satellites.push(SatelliteState {
                index: i as u32,
                pattern,
                radius: mv.radius,
                speed: mv.speed as f64,
                phase: mv.phase as f64,
                base_x: obj.x,
                base_y: obj.y,
                min_z: mv.depth_min,
                max_z: mv.depth_max,
                min_reverb: mv.reverb_min,
                max_reverb: mv.reverb_max,
                rw_x: obj.x,
                rw_z: obj.z,
                rw_vel_x: 0.0,
                rw_vel_z: 0.0,
                // Seed RNG from object index + phase for determinism
                rng_state: 0xDEAD_BEEF_u64.wrapping_add(i as u64 * 7919)
                    .wrapping_add((mv.phase * 1000.0) as u64),
            });
        }

        MovementController { satellites }
    }

    /// Returns true if any objects have movement.
    pub fn has_movement(&self) -> bool {
        !self.satellites.is_empty()
    }

    /// Advance all satellites by `dt` seconds and update engine positions.
    pub fn tick(&mut self, dt: f64, engine: &NoiseEngine) {
        for sat in &mut self.satellites {
            sat.phase += dt * sat.speed;

            let (x, y, z) = match sat.pattern {
                MovementPattern::Static => continue,

                MovementPattern::Orbit => {
                    let x = sat.radius * (sat.phase as f32).cos();
                    let z = sat.radius * (sat.phase as f32).sin();
                    (x, 0.0_f32, z)
                }

                MovementPattern::FigureEight => {
                    let x = sat.radius * (sat.phase as f32).cos();
                    let z = sat.radius * (sat.phase as f32).sin() * (sat.phase as f32).cos();
                    (x, 0.0, z)
                }

                MovementPattern::RandomWalk => {
                    let accel_scale: f32 = 0.5;
                    sat.rw_vel_x += sat.rand_f32() * accel_scale * dt as f32;
                    sat.rw_vel_z += sat.rand_f32() * accel_scale * dt as f32;

                    // Dampen and clamp velocity
                    sat.rw_vel_x *= 0.98;
                    sat.rw_vel_z *= 0.98;
                    let max_vel: f32 = 0.5;
                    sat.rw_vel_x = sat.rw_vel_x.clamp(-max_vel, max_vel);
                    sat.rw_vel_z = sat.rw_vel_z.clamp(-max_vel, max_vel);

                    // Update position
                    sat.rw_x += sat.rw_vel_x * dt as f32;
                    sat.rw_z += sat.rw_vel_z * dt as f32;

                    // Constrain within radius, reflect on boundary
                    let dist = (sat.rw_x * sat.rw_x + sat.rw_z * sat.rw_z).sqrt();
                    if dist > sat.radius && dist > 0.0 {
                        sat.rw_x = sat.rw_x / dist * sat.radius;
                        sat.rw_z = sat.rw_z / dist * sat.radius;
                        sat.rw_vel_x *= -0.5;
                        sat.rw_vel_z *= -0.5;
                    }

                    (sat.rw_x, 0.0, sat.rw_z)
                }

                MovementPattern::DepthBreathing => {
                    let oscillation = (sat.phase as f32).sin();
                    let normalized = (oscillation + 1.0) / 2.0;

                    let x = sat.base_x;
                    let y = sat.base_y;
                    let z = sat.min_z + (sat.max_z - sat.min_z) * normalized;

                    // Modulate reverb send
                    let reverb = sat.min_reverb + (sat.max_reverb - sat.min_reverb) * normalized;
                    engine.set_object_reverb_send(sat.index, reverb);

                    (x, y, z)
                }

                MovementPattern::Pendulum => {
                    let amplitude: f32 = 0.8;
                    let current_angle = amplitude * (sat.phase as f32).sin();
                    let x = sat.radius * current_angle.sin();
                    let z = sat.radius * current_angle.cos();
                    (x, 0.0, z)
                }
            };

            engine.set_object_position(sat.index, x, y, z);
        }
    }

    /// Summary of active movements for display.
    pub fn movement_summary(&self) -> Vec<(u32, MovementPattern, f32, f32)> {
        self.satellites
            .iter()
            .map(|s| (s.index, s.pattern, s.radius, s.speed as f32))
            .collect()
    }
}
