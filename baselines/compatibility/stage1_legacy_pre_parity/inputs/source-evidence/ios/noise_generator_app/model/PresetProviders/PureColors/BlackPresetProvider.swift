import Foundation

struct BlackPresetProvider: NoisePresetProvider {
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

        // ── Showcase: Black ─────────────────────────────────────────────────
                    // Very steep low-pass rolloff. Almost pure sub-bass — sounds like
                    // the deepest rumble imaginable, below what most speakers can
                    // reproduce. Less mid-range than brown, which means less beta
                    // contamination. Best for theta/delta targets where even brown
                    // feeds too much beta (Reset preset uses black for this reason).
                    // Master gain at ceiling + per-object volume of 0.33 (vs 0.20
                    // for spectrally-bright colors) — Black's near-pure sub-bass is
                    // so far outside the ear's sensitive band that it would otherwise
                    // sound ~12+ dB quieter than White at the same RMS. Total
                    // spatial energy = 6 × 0.33 ≈ 2.0, matching the old quad's
                    // 4 × 0.50.
                    engine.setMasterGain(gain: 1)
                    engine.setAnchorColor(color: .black)
                    engine.setAnchorVolume(volume: 0.0)
                    engine.setAcousticEnvironment(environment: .forest)

                    engine.setObject(index: 0, active: true, color: .black,
                        x: 0.00, y: 0.5, z: 2.50, volume: 0.5, reverbSend: 0.05)
                    engine.setObject(index: 1, active: true, color: .black,
                        x: 2.17, y: -0.5, z: 1.25, volume: 0.5, reverbSend: 0.05)
                    engine.setObject(index: 2, active: true, color: .black,
                        x: 2.17, y: 0.5, z: -1.25, volume: 0.5, reverbSend: 0.05)
                    engine.setObject(index: 3, active: true, color: .black,
                        x: 0.00, y: -0.5, z: -2.50, volume: 0.5, reverbSend: 0.05)
                    engine.setObject(index: 4, active: true, color: .black,
                        x: -2.17, y: 0.5, z: -1.25, volume: 0.5, reverbSend: 0.05)
                    engine.setObject(index: 5, active: true, color: .black,
                        x: -2.17, y: -0.5, z: 1.25, volume: 0.5, reverbSend: 0.05)
        (0..<5).forEach { engine.setObjectSpread(index: $0, spread: 0.2) }
    }
}
