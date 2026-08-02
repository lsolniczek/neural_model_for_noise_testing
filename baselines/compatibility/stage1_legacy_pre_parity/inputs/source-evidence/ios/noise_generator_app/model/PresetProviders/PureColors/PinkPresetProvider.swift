import Foundation

struct PinkPresetProvider: NoisePresetProvider {
    func setPreset(engine: NoiseEngine, movementController: MovementController) {
        // ═══════════════════════════════════════════════════════════════════
                    // Color Showcase Presets — Pure Noise Color Demos
                    // ═══════════════════════════════════════════════════════════════════
                    //
                    // 8 minimal presets, one per noise color. Each uses 6 static
                    // satellites placed at 60° azimuth intervals around the listener
                    // with alternating elevation (y = ±0.5) — a hexagonal-antiprism
                    // arrangement for full 3D spatial coverage. No anchor, no
                    // modulation, no movement, no tint, anechoic. Pure HRTF-processed
                    // color comparison.
                    //
                    // Hex layout (all presets identical, horizontal radius 2.5 m):
                    //   ┌─────────────────────────────────────────────────────┐
                    //   │              0° front  (y=+0.5)                     │
                    //   │                                                     │
                    //   │   300° FL (y=-0.5)              60° FR (y=-0.5)     │
                    //   │                                                     │
                    //   │              ← listener →                           │
                    //   │                                                     │
                    //   │   240° BL (y=+0.5)              120° BR (y=+0.5)    │
                    //   │                                                     │
                    //   │              180° back (y=-0.5)                     │
                    //   └─────────────────────────────────────────────────────┘
                    //
                    // Per-object volume scaled from 0.30 (old 4-object quad) to 0.20
                    // so total spatial energy across the 6 objects matches the old
                    // 4-object setup. Brown / Black still get bumps (0.30 / 0.33) to
                    // compensate for their sub-bass insensitivity.
                    //
                    // Color reference:
                    //   White (0): flat spectrum — equal energy all frequencies
                    //   Pink  (1): -3 dB/oct — natural 1/f, most "neutral"
                    //   Brown (2): -6 dB/oct — deep rumble, waterfall
                    //   Green (3): BPF 500Hz — narrow mid-range hum
                    //   Grey  (4): equal-loudness weighted — perceptually flat
                    //   Black (5): steep rolloff — very deep, dark sub-bass
                    //   SSN   (6): speech-shaped — mid-range, voice masking
                    //   Blue  (7): +3 dB/oct — bright, airy, high-frequency
                    // ═══════════════════════════════════════════════════════════════════

        // ── Showcase: Pink ──────────────────────────────────────────────────
                    // -3 dB/oct (1/f). The most natural noise — matches many
                    // environmental sounds (rain, wind, surf). Distributes roughly
                    // evenly across gammatone bands on a log scale (~34/34/24/9%).
                    // The baseline against which all other colors should be compared.
                    engine.setMasterGain(gain: 0.7)
                    engine.setAnchorColor(color: .pink)
                    engine.setAnchorVolume(volume: 0.0)
                    engine.setAcousticEnvironment(environment: .forest)

                    engine.setObject(index: 0, active: true, color: .pink,
                        x: 0.00, y: 0.5, z: 2.50, volume: 0.20, reverbSend: 0.05)
                    engine.setObject(index: 1, active: true, color: .pink,
                        x: 2.17, y: -0.5, z: 1.25, volume: 0.20, reverbSend: 0.05)
                    engine.setObject(index: 2, active: true, color: .pink,
                        x: 2.17, y: 0.5, z: -1.25, volume: 0.20, reverbSend: 0.05)
                    engine.setObject(index: 3, active: true, color: .pink,
                        x: 0.00, y: -0.5, z: -2.50, volume: 0.20, reverbSend: 0.05)
                    engine.setObject(index: 4, active: true, color: .pink,
                        x: -2.17, y: 0.5, z: -1.25, volume: 0.20, reverbSend: 0.05)
                    engine.setObject(index: 5, active: true, color: .pink,
                        x: -2.17, y: -0.5, z: 1.25, volume: 0.20, reverbSend: 0.05)
        (0..<5).forEach { engine.setObjectSpread(index: $0, spread: 0.2) }
    }
}
