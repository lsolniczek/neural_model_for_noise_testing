import Foundation

struct NormalShield1PresetProvider: NoisePresetProvider {
    func setPreset(engine: NoiseEngine, movementController: MovementController) {
        // ═══════════════════════════════════════════════════════════════════
          // ADHD Set: Shield v6 — The Room-Normalized Fortress
          // ═══════════════════════════════════════════════════════════════════
          //
          // Purpose:
          //   Speech masking / soft shielding for ADHD brain profile using the production
          //   room-normalized spatial API: `setObjectRoomPosition(...)`.
          //
          //   This version is retuned for normalized coordinates, where positions are
          //   relative room coordinates in approximately -1...+1. It should be used when
          //   Swift must call `setObjectRoomPosition` instead of world-meter
          //   `setObjectPosition`.
          //
          // Important:
          //   Do not reuse the world-meter v6 values with `setObjectRoomPosition`.
          //   Values such as ±2.5 would clip to ±1.0 and change the neural/acoustic result.
          //   This preset was tested after retuning for normalized room coordinates.
          //
          // Architecture:
          //   ┌──────────────────────────────────────────────────────────────┐
          //   │                 Overhead Pink Canopy                         │
          //   │                 x=0, y=+1.0, z=0                             │
          //   │                 vol=0.08, reverb=0.80, spread=1.0            │
          //   │                 tint 6kHz -4dB                               │
          //   │                                                              │
          //   │  Pink Side L                         Pink Side R             │
          //   │  x=-1.0, y=0, z=0                  x=+1.0, y=0, z=0          │
          //   │  vol=0.32, reverb=0.08             vol=0.32, reverb=0.08     │
          //   │  spread=0.25                       spread=0.25              │
          //   │                                                              │
          //   │                     ← listener →                             │
          //   │                                                              │
          //   │                 Green Front Activation                       │
          //   │                 x=0, y=0, z=+1.0                             │
          //   │                 vol=0.32, reverb=0.26, spread=0.85           │
          //   │                 tint 1.7kHz +3.5dB                           │
          //   │                                                              │
          //   │                 Brown Front Floor                            │
          //   │                 x=0, y=+0.75, z=+1.0                         │
          //   │                 vol=0.02, reverb=0.72, spread=0.95           │
          //   │                 tint 320Hz +1.5dB                            │
          //   │                                                              │
          //   │                 Rear Diffuse Pink Bed                        │
          //   │                 x=0, y=-1.0, z=-1.0                          │
          //   │                 vol=0.18, reverb=0.80, spread=1.0            │
          //   └──────────────────────────────────────────────────────────────┘
          //
          // Measured neural signature, ADHD + Shield goal:
          //   60s:  score=0.5278, delta=3.3%, theta=1.5%, alpha=67.9%, beta=23.9%
          //   300s: score=0.4691, delta=6.3%, theta=3.2%, alpha=64.6%, beta=22.8%
          //   600s: score=0.4914, delta=7.6%, theta=4.1%, alpha=63.1%, beta=22.2%
          //
          // Interpretation:
          //   Long-session EEG bands are inside target at 300s and 600s. The 60s window
          //   has transient alpha slightly above target, but long-session delta remains
          //   controlled, which was the main ADHD failure mode before retuning.
          //
          // Limitation:
          //   This is an NMM proxy result, not proof of human ADHD benefit or clinical
          //   efficacy.
          //
          // ═══════════════════════════════════════════════════════════════════

          engine.setMasterGain(gain: 0.65)
          engine.setAnchorColor(color: .pink)
          engine.setAnchorVolume(volume: 0.0)   // silent anchor; all masking is spatial
          engine.setAcousticEnvironment(environment: .forest)

          // Object 0: left pink side
          engine.setObjectActive(index: 0, active: true)
          engine.setObjectColor(index: 0, color: .pink)
          engine.setObjectRoomPosition(index: 0, x: -1.0, y: 0.0, z: 0.0)
          engine.setObjectVolume(index: 0, volume: 0.32)
          engine.setObjectReverbSend(index: 0, send: 0.08)
          engine.setObjectSpread(index: 0, spread: 0.25)
          engine.setBassModulator(index: 0, kind: .flat, paramA: 0.0, paramB: 0.0, paramC: 0.0)
          engine.setSatelliteModulator(index: 0, kind: .flat, paramA: 0.0, paramB: 0.0, paramC: 0.0)
          engine.setObjectColorTint(index: 0, freqHz: 900.0, gainDb: 0.5)

          // Object 1: right pink side
          engine.setObjectActive(index: 1, active: true)
          engine.setObjectColor(index: 1, color: .pink)
          engine.setObjectRoomPosition(index: 1, x: 1.0, y: 0.0, z: 0.0)
          engine.setObjectVolume(index: 1, volume: 0.32)
          engine.setObjectReverbSend(index: 1, send: 0.08)
          engine.setObjectSpread(index: 1, spread: 0.25)
          engine.setBassModulator(index: 1, kind: .flat, paramA: 0.0, paramB: 0.0, paramC: 0.0)
          engine.setSatelliteModulator(index: 1, kind: .flat, paramA: 0.0, paramB: 0.0, paramC: 0.0)
          engine.setObjectColorTint(index: 1, freqHz: 500.0, gainDb: 1.0)

          // Object 2: rear diffuse pink bed
          engine.setObjectActive(index: 2, active: true)
          engine.setObjectColor(index: 2, color: .pink)
          engine.setObjectRoomPosition(index: 2, x: 0.0, y: -1.0, z: -1.0)
          engine.setObjectVolume(index: 2, volume: 0.18)
          engine.setObjectReverbSend(index: 2, send: 0.80)
          engine.setObjectSpread(index: 2, spread: 1.0)
          engine.setBassModulator(index: 2, kind: .flat, paramA: 0.0, paramB: 0.0, paramC: 0.0)
          engine.setSatelliteModulator(index: 2, kind: .flat, paramA: 0.0, paramB: 0.0, paramC: 0.0)
          engine.setObjectColorTint(index: 2, freqHz: 500.0, gainDb: 1.0)

          // Object 3: overhead pink canopy
          engine.setObjectActive(index: 3, active: true)
          engine.setObjectColor(index: 3, color: .pink)
          engine.setObjectRoomPosition(index: 3, x: 0.0, y: 1.0, z: 0.0)
          engine.setObjectVolume(index: 3, volume: 0.08)
          engine.setObjectReverbSend(index: 3, send: 0.80)
          engine.setObjectSpread(index: 3, spread: 1.0)
          engine.setBassModulator(index: 3, kind: .flat, paramA: 0.0, paramB: 0.0, paramC: 0.0)
          engine.setSatelliteModulator(index: 3, kind: .flat, paramA: 0.0, paramB: 0.0, paramC: 0.0)
          engine.setObjectColorTint(index: 3, freqHz: 6000.0, gainDb: -4.0)

          // Object 4: green front activation layer
          engine.setObjectActive(index: 4, active: true)
          engine.setObjectColor(index: 4, color: .green)
          engine.setObjectRoomPosition(index: 4, x: 0.0, y: 0.0, z: 1.0)
          engine.setObjectVolume(index: 4, volume: 0.32)
          engine.setObjectReverbSend(index: 4, send: 0.26)
          engine.setObjectSpread(index: 4, spread: 0.85)
          engine.setBassModulator(index: 4, kind: .flat, paramA: 0.0, paramB: 0.0, paramC: 0.0)
          engine.setSatelliteModulator(index: 4, kind: .flat, paramA: 0.0, paramB: 0.0, paramC: 0.0)
          engine.setObjectColorTint(index: 4, freqHz: 1700.0, gainDb: 3.5)

          // Object 5: subtle brown floor
          engine.setObjectActive(index: 5, active: true)
          engine.setObjectColor(index: 5, color: .brown)
          engine.setObjectRoomPosition(index: 5, x: 0.0, y: 0.75, z: 1.0)
          engine.setObjectVolume(index: 5, volume: 0.02)
          engine.setObjectReverbSend(index: 5, send: 0.72)
          engine.setObjectSpread(index: 5, spread: 0.95)
          engine.setBassModulator(index: 5, kind: .flat, paramA: 0.0, paramB: 0.0, paramC: 0.0)
          engine.setSatelliteModulator(index: 5, kind: .flat, paramA: 0.0, paramB: 0.0, paramC: 0.0)
          engine.setObjectColorTint(index: 5, freqHz: 320.0, gainDb: 1.5)
    }
}
