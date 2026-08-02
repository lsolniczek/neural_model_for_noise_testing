import Foundation

struct NormalReset1PresetProvider: NoisePresetProvider {
    func setPreset(engine: NoiseEngine, movementController: MovementController) {
        // ═══════════════════════════════════════════════════════════════════
                    // Normal Set: Reset — The Black Cocoon (Anxiety Reduction)
                    // ═══════════════════════════════════════════════════════════════════
                    //
                    // Purpose: Acute stress relief and anxiety reduction. Transitions
                    //          brain from fight-or-flight (beta) to rest-and-digest
                    //          (theta-alpha). Static dark cocoon with floating warm
                    //          breath guide. 10-20 minute sessions.
                    //
                    // Architecture:
                    //   ┌─────────────────────────────────────────────────────┐
                    //   │     Blue wash A (orbit 0.05, phase=0°)              │
                    //   │     LFO 0.03Hz atmospheric                          │
                    //   │                                                     │
                    //   │     Blue wash B (orbit 0.05, phase=180°)            │
                    //   │     counter-rotating                                │
                    //   │                                                     │
                    //   │   Black L (-2.0, z=1.5)   Black R (+2.0, z=-1.5)   │
                    //   │   STATIC                  STATIC                    │
                    //   │   vol=0.40, rev=0.55      vol=0.40, rev=0.55        │
                    //   │   tint 250Hz -3dB         tint 250Hz -3dB           │
                    //   │   (immovable dark walls)                            │
                    //   │                                                     │
                    //   │     Pink breath guide ←──pendulum──→                │
                    //   │     0.08 rad/s (~78s period)                        │
                    //   │     iso 6Hz depth 0.08 (theta anchor)               │
                    //   │     vol=0.20, rev=0.45, tint 300Hz +3dB             │
                    //   │                                                     │
                    //   │              ← listener →                           │
                    //   │         Anechoic environment                        │
                    //   └─────────────────────────────────────────────────────┘
                    //
                    // Design philosophy: "The walls don't move. You do."
                    //   The black cocoon is static — a stable, immovable dark presence
                    //   that wraps around the listener. The pink breath guide is the
                    //   only thing that moves: a gentle pendulum sway that feels like
                    //   rocking in a hammock. This separation creates a sense of being
                    //   held in a stable space while something warm floats around you.
                    //
                    // Why pendulum on pink (not black):
                    //   Moving the cocoon creates spatial instability that can increase
                    //   anxiety ("the walls are closing in"). Static cocoon + floating
                    //   guide reverses this: the container is safe, the guide is gentle.
                    //   Neural scores are nearly identical either way — the choice is
                    //   perceptual/psychological, not spectral.
                    //
                    // Why isochronic 6Hz depth 0.08 on breath guide:
                    //   At depth 0.08, barely perceptible — adds organic theta-rate
                    //   texture without audible pulsing. Provides phase reference for
                    //   the GABA_B slow population. 60% duty cycle softens the ON/OFF
                    //   transitions (gentler than 50%).
                    //
                    // Why anechoic + high per-object reverb:
                    //   Environment reverb is now pre-neural (Priority 19). Using
                    //   anechoic (env=0) avoids double-processing while per-object
                    //   reverb at 0.55 creates the diffuse cloud in the DSP engine.
                    //
                    // Neural signature (Normal brain, meditation goal, CET+gate):
                    //   ┌─────────┬────────┬────────┬────────┬────────┐
                    //   │ Duration │  10 s  │  60 s  │ 300 s  │ 600 s  │
                    //   ├─────────┼────────┼────────┼────────┼────────┤
                    //   │ Score    │ ~0.60  │ ~0.55  │  0.571 │  0.555 │
                    //   │ Theta    │ ~52%   │ ~57%   │ 55.9%  │ 57.6%  │
                    //   │ Alpha    │ ~36%   │ ~32%   │ 34.3%  │ 33.0%  │
                    //   │ Beta     │ ~6%    │ ~6%    │  5.7%  │  5.4%  │
                    //   │ Dom Hz   │  7.6   │  7.5   │  7.3   │  7.3   │
                    //   └─────────┴────────┴────────┴────────┴────────┘
                    //
                    // Refs:
                    //   Zaccaro et al. 2018 — breath-control and vagal tone
                    //   McCormick & Prince 1986 — ACh/GABA_B suppression
                    //   Steriade 1997 — GABA_B in low-arousal states
                    // ═══════════════════════════════════════════════════════════════════

                    // ── Engine setup ────────────────────────────────────────────────────
                    // Master gain at ceiling + per-object volume bumped from 0.40 to
                    // 0.55 on the two black objects — Black's near-pure sub-bass is
                    // far below the ear's sensitive range, so the preset would feel
                    // ~6 dB quieter than Shield otherwise. Pink and blue objects
                    // unchanged so the relative balance inside the cocoon is preserved.
                    engine.setMasterGain(gain: 1.0)
                    engine.setAnchorColor(color: .brown)
                    engine.setAnchorVolume(volume: 0.0)
                    engine.setAcousticEnvironment(environment: .openField)

                    // ── Object 0: Black cocoon left — static dark wall ─────────────────
                    // Black noise, front-left, completely static. No modulation, no
                    // movement. The immovable dark presence that forms half the cocoon.
                    // High reverb (0.55) dissolves it into a formless cloud via the
                    // per-object reverb bus. Tint -3dB at 250Hz cuts sub-bass rumble.
                    engine.setObject(
                        index: 0, active: true, color: .black,
                        x: -2.0, y: 0.0, z: 1.5,
                        volume: 0.55, reverbSend: 0.55
                    )
                    engine.setBassModulator(
                        index: 0, kind: .flat,
                        paramA: 0.0, paramB: 0.0, paramC: 0.0
                    )
                    engine.setSatelliteModulator(
                        index: 0, kind: .flat,
                        paramA: 0.0, paramB: 0.0, paramC: 0.0
                    )
                    engine.setObjectColorTint(index: 0, freqHz: 250.0, gainDb: -3.0)

                    // ── Object 1: Black cocoon right — static dark wall ────────────────
                    // Back-right, diagonal mirror of object 0. Together they create
                    // an asymmetric cocoon (front-left ↔ back-right diagonal) that
                    // envelops without symmetrical boredom. Both static — the container
                    // does not move.
                    engine.setObject(
                        index: 1, active: true, color: .black,
                        x: 2.0, y: 0.0, z: -1.5,
                        volume: 0.55, reverbSend: 0.55
                    )
                    engine.setBassModulator(
                        index: 1, kind: .flat,
                        paramA: 0.0, paramB: 0.0, paramC: 0.0
                    )
                    engine.setSatelliteModulator(
                        index: 1, kind: .flat,
                        paramA: 0.0, paramB: 0.0, paramC: 0.0
                    )
                    engine.setObjectColorTint(index: 1, freqHz: 250.0, gainDb: -3.0)

                    // ── Object 2: Pink breath guide — floating pendulum ────────────────
                    // The only moving noise element. Pendulum orbit at 0.08 rad/s
                    // (~78s period) creates a gentle rocking sensation — like being
                    // in a hammock. The pink noise with warm tint (+3dB at 300Hz)
                    // is the "warm light" floating through the dark cocoon.
                    //
                    // Isochronic 6Hz at depth 0.08: barely perceptible theta-rate
                    // texture. Provides a phase reference for GABA_B theta generation
                    // without audible pulsing. 60% duty cycle for soft transitions.
                    engine.setObject(
                        index: 2, active: true, color: .pink,
                        x: 0.0, y: 0.0, z: 1.5,
                        volume: 0.20, reverbSend: 0.45
                    )
                    engine.setBassModulator(
                        index: 2, kind: .isochronic,
                        paramA: 6.0,    // 6 Hz theta-rate
                        paramB: 0.08,   // barely perceptible
                        paramC: 0.60    // 60% duty — soft ON/OFF
                    )
                    engine.setSatelliteModulator(
                        index: 2, kind: .isochronic,
                        paramA: 6.0,
                        paramB: 0.08,
                        paramC: 0.60
                    )
                    engine.setObjectColorTint(index: 2, freqHz: 300.0, gainDb: 3.0)
                    movementController.addSatellite(
                        index: 2, pattern: .orbit,
                        radius: 1.5, speed: 0.08, initialPhase: 0.0,
                        depthRange: 1.0...2.0,
                        reverbRange: 0.30...0.50
                    )

                    // ── Object 3: Blue spatial wash A — atmospheric orbit ──────────────
                    // Neurally invisible. Ultra-slow orbit (0.05 rad/s ≈ 126s).
                    // LFO 0.03Hz (33s) adds atmospheric breathing at a different
                    // timescale than the pendulum. Tint -3dB at 6kHz softens highs.
                    engine.setObject(
                        index: 3, active: true, color: .blue,
                        x: 1.5, y: 1.0, z: 0.0,
                        volume: 0.06, reverbSend: 0.40
                    )
                    engine.setBassModulator(
                        index: 3, kind: .flat,
                        paramA: 0.0, paramB: 0.0, paramC: 0.0
                    )
                    engine.setSatelliteModulator(
                        index: 3, kind: .neuralLfo,
                        paramA: 0.03,   // 33-second atmospheric cycle
                        paramB: 0.12,
                        paramC: 0.0
                    )
                    engine.setObjectColorTint(index: 3, freqHz: 6000.0, gainDb: -3.0)
                    movementController.addSatellite(
                        index: 3, pattern: .orbit,
                        radius: 2.5, speed: 0.05, initialPhase: 0.0,
                        depthRange: 2.0...3.0,
                        reverbRange: 0.30...0.50
                    )

                    // ── Object 4: Blue spatial wash B — counter-rotating ───────────────
                    // Phase 180° from object 3. Two blue sources orbit in opposite
                    // directions above the cocoon — airy atmosphere that never repeats.
                    engine.setObject(
                        index: 4, active: true, color: .blue,
                        x: -1.5, y: 1.0, z: 0.0,
                        volume: 0.06, reverbSend: 0.40
                    )
                    engine.setBassModulator(
                        index: 4, kind: .flat,
                        paramA: 0.0, paramB: 0.0, paramC: 0.0
                    )
                    engine.setSatelliteModulator(
                        index: 4, kind: .neuralLfo,
                        paramA: 0.03,
                        paramB: 0.12,
                        paramC: 0.0
                    )
                    engine.setObjectColorTint(index: 4, freqHz: 6000.0, gainDb: -3.0)
                    movementController.addSatellite(
                        index: 4, pattern: .orbit,
                        radius: 2.5, speed: 0.05, initialPhase: 180.0,
                        depthRange: 2.0...3.0,
                        reverbRange: 0.30...0.50
                    )

                    movementController.start()
    }
}
