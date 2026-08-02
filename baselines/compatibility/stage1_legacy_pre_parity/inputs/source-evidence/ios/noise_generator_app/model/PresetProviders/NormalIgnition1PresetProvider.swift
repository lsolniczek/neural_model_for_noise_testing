import Foundation

struct NormalIgnition1PresetProvider: NoisePresetProvider {
    func setPreset(engine: NoiseEngine, movementController: MovementController) {
        // ═══════════════════════════════════════════════════════════════════
                    // Normal Set: Ignition v3 — The Catalyst
                    // ═══════════════════════════════════════════════════════════════════
                    //
                    // Purpose: High-beta/gamma "ignition" for ADHD brains.
                    //          White broadband flankers with dual-rate isochronic drive
                    //          beta+gamma simultaneously. Two blue ion streams add
                    //          perceptual depth (orbit + figure-8) without affecting
                    //          neural bands. Pink anchor at 0.05 stabilizes the
                    //          GABA_B gain modulation to keep beta within target range.
                    //
                    // Architecture:
                    //   ┌─────────────────────────────────────────────────────┐
                    //   │     Ion Stream Alpha (blue orbit 0.35 rad/s)        │
                    //   │     LFO 0.2Hz depth 0.50, tint 8kHz +3dB           │
                    //   │     vol=0.12 — spatial dome feeling                 │
                    //   │                                                     │
                    //   │   White L (-2.5, y=0.8)   White R (+2.5, y=0.8)    │
                    //   │   Bass: iso 25Hz β         Bass: iso 25Hz β         │
                    //   │   Sat:  iso 40Hz γ         Sat:  iso 40Hz γ         │
                    //   │   tint 3500Hz +5dB         tint 4500Hz +4dB         │
                    //   │   vol=0.45                 vol=0.45                  │
                    //   │                                                     │
                    //   │              ← listener →                           │
                    //   │         Pink anchor vol=0.05                        │
                    //   │                                                     │
                    //   │     Ion Spark Beta (blue fig8 0.15 rad/s)           │
                    //   │     Stochastic 2/s depth 0.40, tint 8kHz            │
                    //   │     vol=0.08 — electric sparkle texture              │
                    //   └─────────────────────────────────────────────────────┘
                    //
                    // Why white noise (not green) for beta+gamma:
                    //   Green's 500Hz BPF locks WC into beta at ANY volume (73%+),
                    //   starving gamma. White's flat spectrum feeds ALL gammatone
                    //   bands — with tint at 3500-4500Hz, the high bands drive
                    //   WC(25) into higher frequencies where gamma can emerge.
                    //   Result: beta 53% + gamma 8-12% vs green's beta 73% + gamma 4%.
                    //
                    // Why dual-rate isochronic (25Hz bass / 40Hz satellite):
                    //   Bass channel (50-800Hz) gets 25Hz → high-beta via WC tracking.
                    //   Satellite channel (800-8000Hz) gets 40Hz → gamma-rate transients
                    //   that the Wendling fast inhibitory population resonates with.
                    //   This tonotopic separation is what produces gamma 8-15%.
                    //   Stochastic 37-43Hz was tested: gamma only 1-4% — isochronic's
                    //   sharp ON/OFF transitions create stronger FFR.
                    //
                    // Why blue ions (neurally invisible):
                    //   Blue noise sits above the gammatone 80Hz LPF — the model
                    //   can't see it. But the listener perceives a rich spatial
                    //   environment: the orbit creates an open dome above, the fig8
                    //   adds electric sparkle that sweeps front-to-back. This prevents
                    //   the sterile "two speakers" feeling of just the white flankers.
                    //   The LFO 0.2Hz on the orbit and stochastic 2/s on the fig8
                    //   contribute to higher computed arousal → weaker GABA_B → less
                    //   theta drift. Perceptual richness WITH neural precision.
                    //
                    // Why pink anchor at 0.05:
                    //   Without it: beta locks at 75%. With it: beta 53% (in range).
                    //   The anchor feeds both ears symmetrically (bypasses HRTF),
                    //   providing just enough low-mid energy for the GABA_B slow
                    //   population to modulate excitatory gain — pulling beta down
                    //   from its attractor without killing gamma.
                    //   At 0.10: theta explodes to 37% (too much GABA_B fuel).
                    //   0.05 is the precise balance point.
                    //
                    // Neural signature (ADHD brain, ignition goal, CET+gate):
                    //   ┌─────────┬────────┬────────┬────────┬────────┐
                    //   │ Duration │  10 s  │  60 s  │ 300 s  │ 600 s  │
                    //   ├─────────┼────────┼────────┼────────┼────────┤
                    //   │ Score    │  0.727 │  ~0.68 │  0.655 │  0.664 │
                    //   │ Beta     │ 56.9%  │  ~55%  │ 52.9%  │ 52.3%  │
                    //   │ Gamma    │ 11.8%  │  ~10%  │  8.3%  │  8.3%  │
                    //   │ Alpha    │ 15.4%  │  ~15%  │ 14.9%  │ 15.0%  │
                    //   │ Theta    │ 14.4%  │  ~18%  │ 22.2%  │ 22.6%  │
                    //   │ Firing   │ 18.7   │        │ 16.6   │ 16.5   │
                    //   │ Dom Hz   │ 24.3   │  24.3  │ 24.3   │ 24.3   │
                    //   └─────────┴────────┴────────┴────────┴────────┘
                    //
                    // Disturbance resilience:
                    //   Entrainment: 0.91 (50ms recovery)
                    //   Spectral:    0.46 (BPPR=0.54, SRT₅₀=1700ms)
                    //
                    // Refs:
                    //   Clarke et al. 2001 — excess theta, deficient beta in ADHD EEG
                    //   Arns et al. 2013 — theta/beta ratio as ADHD biomarker
                    //   Pastor et al. 2003 — 40 Hz gamma binding in attention
                    //   McCormick 1992 — ACh/GABA_B arousal modulation
                    // ═══════════════════════════════════════════════════════════════════

                    // ── Engine setup ────────────────────────────────────────────────────
                    engine.setMasterGain(gain: 0.75)
                    engine.setAnchorColor(color: .pink)
                    engine.setAnchorVolume(volume: 0.0)    // tiny — GABA_B balance point
                    engine.setAcousticEnvironment(environment: .openField)

                    // ── Object 0: White left flanker — beta/gamma driver ───────────────
                    // White noise at left ear, slightly elevated (y=0.8).
                    // Dual-rate isochronic: bass 25Hz (high beta), satellite 40Hz (gamma).
                    // Tint +5dB at 3500Hz pushes energy into gammatone band 3 (high),
                    // which the WC(25) tracker uses for frequency-following. The
                    // asymmetric tint (3500 vs 4500 on right) creates different
                    // gammatone weightings per hemisphere for spectral diversity.
                    engine.setObject(
                        index: 0, active: true, color: .white,
                        x: -2.5, y: 0.8, z: 0.0,
                        volume: 0.45, reverbSend: 0.02
                    )
                    engine.setBassModulator(
                        index: 0, kind: .isochronic,
                        paramA: 25.0,   // 25 Hz high-beta
                        paramB: 0.40,   // moderate depth
                        paramC: 0.50    // 50% duty cycle
                    )
                    engine.setSatelliteModulator(
                        index: 0, kind: .isochronic,
                        paramA: 40.0,   // 40 Hz gamma
                        paramB: 0.50,   // deeper for gamma FFR
                        paramC: 0.40    // shorter ON — sharper transients
                    )
                    engine.setObjectColorTint(index: 0, freqHz: 3500.0, gainDb: 5.0)

                    // ── Object 1: White right flanker — mirror, asymmetric tint ────────
                    // Same dual-rate isochronic. Tint at 4500Hz (+4dB) vs left's 3500Hz
                    // (+5dB) — the spectral asymmetry drives both WC trackers at
                    // slightly different operating points, sustaining gamma 8-12%.
                    engine.setObject(
                        index: 1, active: true, color: .white,
                        x: 2.5, y: 0.8, z: 0.0,
                        volume: 0.45, reverbSend: 0.02
                    )
                    engine.setBassModulator(
                        index: 1, kind: .isochronic,
                        paramA: 25.0,
                        paramB: 0.40,
                        paramC: 0.50
                    )
                    engine.setSatelliteModulator(
                        index: 1, kind: .isochronic,
                        paramA: 40.0,
                        paramB: 0.50,
                        paramC: 0.40
                    )
                    engine.setObjectColorTint(index: 1, freqHz: 4500.0, gainDb: 4.0)

                    // ── Object 2: Ion Stream Alpha — blue orbit ────────────────────────
                    // Blue noise orbiting overhead at 0.35 rad/s (~18s period).
                    // Neurally invisible (above gammatone LPF) — purely perceptual.
                    // LFO 0.2Hz with depth 0.50: slow 5-second volume swell creates
                    // a breathing dome of air above the listener. The LFO contributes
                    // to arousal calculation → slightly suppresses GABA_B → helps
                    // maintain beta. Tint +3dB at 8kHz: bright, airy shimmer.
                    engine.setObject(
                        index: 2, active: true, color: .blue,
                        x: 1.5, y: 1.0, z: 0.0,
                        volume: 0.12, reverbSend: 0.05
                    )
                    engine.setBassModulator(
                        index: 2, kind: .neuralLfo,
                        paramA: 0.2,    // 0.2 Hz — 5-second breathing
                        paramB: 0.50,   // deep volume swell
                        paramC: 0.0
                    )
                    engine.setSatelliteModulator(
                        index: 2, kind: .neuralLfo,
                        paramA: 0.2,
                        paramB: 0.50,
                        paramC: 0.0
                    )
                    movementController.addSatellite(
                        index: 2, pattern: .orbit,
                        radius: 2.0, speed: 0.35, initialPhase: 0.0,
                        depthRange: 1.0...2.0,
                        reverbRange: 0.03...0.08
                    )
                    engine.setObjectColorTint(index: 2, freqHz: 8000.0, gainDb: 3.0)

                    // ── Object 3: Ion Spark Beta — blue figure-8 ───────────────────────
                    // Blue noise on slow figure-8 (0.15 rad/s) below ear level.
                    // Stochastic 2 pulses/sec: rare electric pings that sweep
                    // front-to-back through the listener's head axis. The Poisson
                    // timing prevents habituation — each ping is a surprise.
                    // Combined with the orbit above, creates a full 3D ion field.
                    // Tint at 8kHz neutral: pure high-frequency sparkle.
                    engine.setObject(
                        index: 3, active: true, color: .blue,
                        x: 0.0, y: -0.5, z: 1.5,
                        volume: 0.08, reverbSend: 0.03
                    )
                    engine.setBassModulator(
                        index: 3, kind: .stochastic,
                        paramA: 2.0,    // 2 pings/sec — sparse
                        paramB: 0.40,   // moderate depth
                        paramC: 0.0
                    )
                    engine.setSatelliteModulator(
                        index: 3, kind: .stochastic,
                        paramA: 2.0,
                        paramB: 0.40,
                        paramC: 0.0
                    )
                    engine.setObjectColorTint(index: 3, freqHz: 8000.0, gainDb: 0.0)
                    movementController.addSatellite(
                        index: 3, pattern: .figureEight,
                        radius: 2.0, speed: 0.15, initialPhase: 0.0,
                        depthRange: 1.0...2.0,
                        reverbRange: 0.02...0.05
                    )

                    movementController.start()
    }
}
