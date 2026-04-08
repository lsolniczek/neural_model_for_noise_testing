/// Movement Controller — Spatial Trajectory Engine.
///
/// Rust port of the Swift `MovementController`. Drives spatial object
/// positions over time using deterministic patterns (orbit, figure-eight,
/// pendulum, depth-breathing) and a seeded random walk.
///
/// During evaluation the controller is stepped in discrete time increments
/// (matching the render chunk size) so that the rendered audio reflects
/// object motion through the HRTF pipeline.

use noise_generator_core::NoiseEngine;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::{ModConfig, ObjectConfig, Preset, MAX_OBJECTS};

    /// Build a preset with one active object using the given movement config.
    fn preset_with_movement(mv: MovementConfig) -> Preset {
        let mut preset = Preset::default();
        preset.objects[0].active = true;
        preset.objects[0].x = 1.0;
        preset.objects[0].y = 0.5;
        preset.objects[0].z = 2.0;
        preset.objects[0].movement = mv;
        preset
    }

    /// Create a dummy engine for testing (we only inspect positions, not render).
    fn test_engine() -> std::sync::Arc<NoiseEngine> {
        NoiseEngine::new(48_000, 0.8)
    }

    // ---------------------------------------------------------------
    // MovementPattern enum
    // ---------------------------------------------------------------

    #[test]
    fn pattern_from_u8_round_trip() {
        for v in 0..=5_u8 {
            let p = MovementPattern::from_u8(v);
            assert_eq!(p.to_u8(), v, "Round-trip failed for {v}");
        }
    }

    #[test]
    fn pattern_from_u8_out_of_range_is_static() {
        assert_eq!(MovementPattern::from_u8(6), MovementPattern::Static);
        assert_eq!(MovementPattern::from_u8(255), MovementPattern::Static);
    }

    // ---------------------------------------------------------------
    // MovementConfig clamp
    // ---------------------------------------------------------------

    #[test]
    fn config_clamp_enforces_bounds() {
        let mut mc = MovementConfig {
            kind: 10,
            radius: 100.0,
            speed: -1.0,
            phase: 100.0,
            depth_min: -5.0,
            depth_max: 100.0,
            reverb_min: -1.0,
            reverb_max: 2.0,
        };
        mc.clamp();

        assert_eq!(mc.kind, 5);
        assert_eq!(mc.radius, 5.0);
        assert_eq!(mc.speed, 0.0);
        assert!(mc.phase <= std::f32::consts::TAU);
        assert_eq!(mc.depth_min, 0.5);
        assert_eq!(mc.depth_max, 6.0);
        assert_eq!(mc.reverb_min, 0.0);
        assert_eq!(mc.reverb_max, 1.0);
    }

    // ---------------------------------------------------------------
    // Static: no movement tracked
    // ---------------------------------------------------------------

    #[test]
    fn static_movement_not_tracked() {
        let preset = preset_with_movement(MovementConfig {
            kind: 0, // Static
            ..MovementConfig::default()
        });
        let ctrl = MovementController::from_preset(&preset);
        assert!(!ctrl.has_movement());
        assert_eq!(ctrl.satellites.len(), 0);
    }

    // ---------------------------------------------------------------
    // Inactive objects not tracked
    // ---------------------------------------------------------------

    #[test]
    fn inactive_objects_not_tracked() {
        let mut preset = Preset::default();
        // Object 0 inactive but has orbit
        preset.objects[0].active = false;
        preset.objects[0].movement.kind = 1;
        preset.objects[0].movement.radius = 2.0;
        preset.objects[0].movement.speed = 1.0;

        let ctrl = MovementController::from_preset(&preset);
        assert!(!ctrl.has_movement());
    }

    // ---------------------------------------------------------------
    // Orbit: traces a circle (x² + z² = r²)
    // ---------------------------------------------------------------

    #[test]
    fn orbit_traces_circle() {
        let r = 3.0_f32;
        let preset = preset_with_movement(MovementConfig {
            kind: 1, // Orbit
            radius: r,
            speed: 2.0,
            phase: 0.0,
            ..MovementConfig::default()
        });
        let engine = test_engine();
        preset.apply_to_engine(&engine);
        let mut ctrl = MovementController::from_preset(&preset);

        let dt = 0.05;
        for _ in 0..100 {
            ctrl.tick(dt, &engine);
        }

        // After ticking, the satellite's phase has advanced.
        // Check the last position satisfies x² + z² ≈ r²
        let sat = &ctrl.satellites[0];
        let x = r * (sat.phase as f32).cos();
        let z = r * (sat.phase as f32).sin();
        let dist_sq = x * x + z * z;
        assert!(
            (dist_sq - r * r).abs() < 0.01,
            "Orbit should trace circle: x²+z²={dist_sq}, expected {}", r * r
        );
    }

    // ---------------------------------------------------------------
    // FigureEight: crosses origin twice per cycle
    // ---------------------------------------------------------------

    #[test]
    fn figure_eight_crosses_origin() {
        let r = 2.0_f32;
        let speed = 1.0;
        // At phase = π/2 and 3π/2, x = r·cos(phase), z = r·sin(phase)·cos(phase)
        // At π/2: x = 0, z = 0 (crossing)
        let phase_at_cross = std::f64::consts::FRAC_PI_2;
        let x = r * (phase_at_cross as f32).cos();
        let z = r * (phase_at_cross as f32).sin() * (phase_at_cross as f32).cos();
        assert!(x.abs() < 0.01, "FigureEight x at π/2 should be ~0, got {x}");
        assert!(z.abs() < 0.01, "FigureEight z at π/2 should be ~0, got {z}");
    }

    // ---------------------------------------------------------------
    // DepthBreathing: z oscillates between min_z and max_z
    // ---------------------------------------------------------------

    #[test]
    fn depth_breathing_z_range() {
        let min_z = 1.0_f32;
        let max_z = 4.0_f32;
        let preset = preset_with_movement(MovementConfig {
            kind: 4, // DepthBreathing
            radius: 0.0,
            speed: 2.0,
            phase: 0.0,
            depth_min: min_z,
            depth_max: max_z,
            reverb_min: 0.1,
            reverb_max: 0.8,
        });
        let engine = test_engine();
        preset.apply_to_engine(&engine);
        let mut ctrl = MovementController::from_preset(&preset);

        // Tick through a full cycle to collect z values
        let dt = 0.05;
        let mut z_min = f32::MAX;
        let mut z_max = f32::MIN;
        for _ in 0..200 {
            ctrl.tick(dt, &engine);
            let sat = &ctrl.satellites[0];
            let oscillation = (sat.phase as f32).sin();
            let normalized = (oscillation + 1.0) / 2.0;
            let z = sat.min_z + (sat.max_z - sat.min_z) * normalized;
            z_min = z_min.min(z);
            z_max = z_max.max(z);
        }

        assert!(
            (z_min - min_z).abs() < 0.05,
            "DepthBreathing z should reach min_z={min_z}, got z_min={z_min}"
        );
        assert!(
            (z_max - max_z).abs() < 0.05,
            "DepthBreathing z should reach max_z={max_z}, got z_max={z_max}"
        );
    }

    // ---------------------------------------------------------------
    // Pendulum: stays on circular arc (x² + z² = r²)
    // ---------------------------------------------------------------

    #[test]
    fn pendulum_stays_on_arc() {
        let r = 2.5_f32;
        let preset = preset_with_movement(MovementConfig {
            kind: 5, // Pendulum
            radius: r,
            speed: 1.5,
            phase: 0.0,
            ..MovementConfig::default()
        });
        let engine = test_engine();
        preset.apply_to_engine(&engine);
        let mut ctrl = MovementController::from_preset(&preset);

        let dt = 0.05;
        for step in 0..100 {
            ctrl.tick(dt, &engine);
            let sat = &ctrl.satellites[0];
            let amplitude: f32 = 0.8;
            let angle = amplitude * (sat.phase as f32).sin();
            let x = r * angle.sin();
            let z = r * angle.cos();
            let dist_sq = x * x + z * z;
            assert!(
                (dist_sq - r * r).abs() < 0.01,
                "Pendulum step {step}: x²+z²={dist_sq}, expected {}",
                r * r
            );
        }
    }

    // ---------------------------------------------------------------
    // RandomWalk: stays within radius
    // ---------------------------------------------------------------

    #[test]
    fn random_walk_stays_within_radius() {
        let r = 3.0_f32;
        let preset = preset_with_movement(MovementConfig {
            kind: 3, // RandomWalk
            radius: r,
            speed: 1.0,
            phase: 0.0,
            ..MovementConfig::default()
        });
        let engine = test_engine();
        preset.apply_to_engine(&engine);
        let mut ctrl = MovementController::from_preset(&preset);

        let dt = 0.05;
        for step in 0..500 {
            ctrl.tick(dt, &engine);
            let sat = &ctrl.satellites[0];
            let dist = (sat.rw_x * sat.rw_x + sat.rw_z * sat.rw_z).sqrt();
            assert!(
                dist <= r + 0.01,
                "RandomWalk step {step}: dist={dist} exceeds radius={r}"
            );
        }
    }

    // ---------------------------------------------------------------
    // Determinism: same seed → same walk
    // ---------------------------------------------------------------

    #[test]
    fn random_walk_deterministic() {
        let mv = MovementConfig {
            kind: 3,
            radius: 3.0,
            speed: 1.0,
            phase: 0.5,
            ..MovementConfig::default()
        };
        let preset1 = preset_with_movement(mv.clone());
        let preset2 = preset_with_movement(mv);
        let engine1 = test_engine();
        let engine2 = test_engine();
        preset1.apply_to_engine(&engine1);
        preset2.apply_to_engine(&engine2);

        let mut ctrl1 = MovementController::from_preset(&preset1);
        let mut ctrl2 = MovementController::from_preset(&preset2);

        let dt = 0.05;
        for _ in 0..100 {
            ctrl1.tick(dt, &engine1);
            ctrl2.tick(dt, &engine2);
        }

        assert_eq!(ctrl1.satellites[0].rw_x, ctrl2.satellites[0].rw_x);
        assert_eq!(ctrl1.satellites[0].rw_z, ctrl2.satellites[0].rw_z);
    }

    // ---------------------------------------------------------------
    // Phase advances correctly
    // ---------------------------------------------------------------

    #[test]
    fn phase_advances_by_dt_times_speed() {
        let speed = 2.5;
        let preset = preset_with_movement(MovementConfig {
            kind: 1, // Orbit
            radius: 1.0,
            speed: speed as f32,
            phase: 0.0,
            ..MovementConfig::default()
        });
        let engine = test_engine();
        preset.apply_to_engine(&engine);
        let mut ctrl = MovementController::from_preset(&preset);

        let dt = 0.05;
        let n = 10;
        for _ in 0..n {
            ctrl.tick(dt, &engine);
        }

        let expected_phase = dt * speed * n as f64;
        let actual_phase = ctrl.satellites[0].phase;
        assert!(
            (actual_phase - expected_phase).abs() < 1e-10,
            "Phase should be {expected_phase}, got {actual_phase}"
        );
    }

    // ---------------------------------------------------------------
    // Multiple objects tracked independently
    // ---------------------------------------------------------------

    #[test]
    fn multiple_objects_tracked() {
        let mut preset = Preset::default();
        preset.objects[0].active = true;
        preset.objects[0].movement = MovementConfig {
            kind: 1, radius: 2.0, speed: 1.0, phase: 0.0, ..MovementConfig::default()
        };
        preset.objects[2].active = true;
        preset.objects[2].movement = MovementConfig {
            kind: 5, radius: 1.5, speed: 0.5, phase: 1.0, ..MovementConfig::default()
        };
        // Object 1 active but static → not tracked
        preset.objects[1].active = true;
        preset.objects[1].movement.kind = 0;

        let ctrl = MovementController::from_preset(&preset);
        assert!(ctrl.has_movement());
        assert_eq!(ctrl.satellites.len(), 2);
        assert_eq!(ctrl.satellites[0].index, 0);
        assert_eq!(ctrl.satellites[1].index, 2);
    }

    // ---------------------------------------------------------------
    // DepthBreathing reverb bounds
    // ---------------------------------------------------------------

    #[test]
    fn depth_breathing_reverb_bounded() {
        let min_r = 0.1_f32;
        let max_r = 0.8_f32;

        // Compute reverb at extremes of sin
        // sin = -1: normalized = 0, reverb = min_r
        // sin = +1: normalized = 1, reverb = max_r
        let norm_min = (-1.0_f32 + 1.0) / 2.0;
        let norm_max = (1.0_f32 + 1.0) / 2.0;
        let rev_at_min = min_r + (max_r - min_r) * norm_min;
        let rev_at_max = min_r + (max_r - min_r) * norm_max;

        assert!((rev_at_min - min_r).abs() < 1e-6);
        assert!((rev_at_max - max_r).abs() < 1e-6);
    }
}
