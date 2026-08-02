import Foundation

struct NormalRelax1PresetProvider: NoisePresetProvider {
    func setPreset(engine: NoiseEngine, movementController: MovementController) {
        // ═══════════════════════════════════════════════════════════════════
                    // Normal Set: Deep Relax — The Gravity Blanket
                    // ═══════════════════════════════════════════════════════════════════
                    //
                    // Purpose: Deep relaxation / parasympathetic activation.
                    //          Theta-alpha coexistence — conscious but fully detached.
                    //          Brown cocoon for warmth, pink orbiting satellites for
                    //          gentle spatial variation that sustains the mixed state.
                    //
                    // Architecture:
                    //   ┌─────────────────────────────────────────────────────┐
                    //   │                                                     │
                    //   │     Pink orbit A (speed 0.43)                       │
                    //   │     6Hz LFO depth 0.08                              │
                    //   │     tint 150Hz +5dB (warm bass)                     │
                    //   │     rev=0.55                                        │
                    //   │                                                     │
                    //   │   Brown front (z=2.0)     Brown back (z=-2.0)       │
                    //   │   vol=0.50, rev=0.60      vol=0.50, rev=0.60        │
                    //   │   STATIC                  STATIC                    │
                    //   │                                                     │
                    //   │              ← listener →                           │
                    //   │                                                     │
                    //   │     Pink orbit B (speed 0.53)                       │
                    //   │     6Hz LFO depth 0.08                              │
                    //   │     tint 150Hz +5dB (warm bass)                     │
                    //   │     rev=0.55                                        │
                    //   │                                                     │
                    //   │         Anechoic environment                        │
                    //   └─────────────────────────────────────────────────────┘
                    //
                    // This preset exploits the GABA_B gain modulation fix (Priority 18).
                    // The corrected model produces genuine theta-alpha coexistence:
                    //   θ=56% + α=34% at 300s — a mixed state that was impossible
                    //   in the old model (which locked to either α=90% or θ=90%).
                    //
                    // Why orbit instead of figure-8:
                    //   Neural scores are identical — the gammatone/JR model doesn't
                    //   distinguish orbit from fig8 at these speeds. But orbit is
                    //   perceptually more soothing: the pink noise circles smoothly
                    //   around the head like a warm current, while fig8 crosses the
                    //   midline creating a more dynamic, less restful sensation.
                    //
                    // Why prime-speed ratio (0.43 / 0.53):
                    //   These orbit speeds never synchronize — the two pink streams
                    //   would need hours to align at the same azimuth. The auditory
                    //   cortex can't predict when they'll overlap, preventing
                    //   habituation. The continuous unpredictable HRTF variation
                    //   sustains the transient theta that GABA_B then maintains.
                    //
                    // Why anechoic (not DeepSanctuary):
                    //   With Priority 19 (pre-neural RIR), DeepSanctuary's reverb
                    //   actually reduces theta (smears temporal modulation → more
                    //   alpha-idle). For deep relaxation, we WANT maximum theta.
                    //   Anechoic gives the cleanest theta signal. Per-object reverb
                    //   at 0.55-0.60 still creates a diffuse cloud via the DSP engine.
                    //
                    // Why brown front/back:
                    //   Brown's -6dB/oct concentrates energy in gammatone band 0
                    //   (50-200Hz), feeding the right hemisphere's theta generator
                    //   via GABA_B modulation. Static placement signals safety.
                    //
                    // Neural signature (Normal brain, deep_relaxation goal, CET+gate):
                    //   ┌─────────┬────────┬────────┬────────┬────────┐
                    //   │ Duration │  10 s  │  60 s  │ 300 s  │ 600 s  │
                    //   ├─────────┼────────┼────────┼────────┼────────┤
                    //   │ Score    │  0.487 │ ~0.50  │  0.517 │  0.490 │
                    //   │ Theta    │ 49.5%  │ ~53%   │ 55.6%  │ 58.7%  │
                    //   │ Alpha    │ 34.2%  │ ~35%   │ 33.6%  │ 31.0%  │
                    //   │ Beta     │ 10.3%  │ ~8%    │  6.2%  │  6.0%  │
                    //   │ Delta    │  5.5%  │ ~4%    │  4.2%  │  3.8%  │
                    //   │ Dom Hz   │  8.2   │  7.5   │  7.3   │  7.3   │
                    //   └─────────┴────────┴────────┴────────┴────────┘
                    //
                    // Refs:
                    //   Klimesch 1999 — theta-alpha coexistence in relaxed wakefulness
                    //   McCormick & Prince 1986 — ACh/GABA_B suppression
                    //   Steriade 1997 — GABA_B conductance 2-3x in NREM vs wake
                    //   Moran & Friston 2011 — canonical microcircuit GABA_B
                    // ═══════════════════════════════════════════════════════════════════

                    // ── Engine setup ────────────────────────────────────────────────────
                    engine.setMasterGain(gain: 0.7)
                    engine.setAnchorColor(color: .brown)
                    engine.setAnchorVolume(volume: 0.0)
                    engine.setAcousticEnvironment(environment: .openField)

                    // ── Object 0: Brown front — static warmth ──────────────────────────
                    // Brown -6dB/oct in front. High reverb (0.60) blurs the source
                    // into a warm cloud. Static placement: no HRTF variation, no
                    // arousal from movement. The low-frequency energy feeds GABA_B
                    // theta generation through the slow inhibitory gain modulation.
                    engine.setObject(
                        index: 0, active: true, color: .brown,
                        x: 0.0, y: 0.0, z: 2.0,
                        volume: 0.50, reverbSend: 0.60
                    )
                    engine.setBassModulator(
                        index: 0, kind: .flat,
                        paramA: 0.0, paramB: 0.0, paramC: 0.0
                    )
                    engine.setSatelliteModulator(
                        index: 0, kind: .flat,
                        paramA: 0.0, paramB: 0.0, paramC: 0.0
                    )

                    // ── Object 1: Brown back — completes the cocoon ────────────────────
                    // Mirror of front, behind the listener. Front/back axis of warm
                    // bass envelops symmetrically — no sound sneaking up from behind.
                    engine.setObject(
                        index: 1, active: true, color: .brown,
                        x: 0.0, y: 0.0, z: -2.0,
                        volume: 0.50, reverbSend: 0.60
                    )
                    engine.setBassModulator(
                        index: 1, kind: .flat,
                        paramA: 0.0, paramB: 0.0, paramC: 0.0
                    )
                    engine.setSatelliteModulator(
                        index: 1, kind: .flat,
                        paramA: 0.0, paramB: 0.0, paramC: 0.0
                    )

                    // ── Object 2: Pink orbit A — prime-speed theta nudge ───────────────
                    // Pink noise orbiting at 0.43 rad/s (~14.6s period). The continuous
                    // circular HRTF variation prevents alpha lock during the initial
                    // transient, creating a window for GABA_B to establish theta.
                    // NeuralLfo 6Hz at depth 0.08: subtle theta-rate modulation.
                    // Tint +5dB at 150Hz: warm bass boost that feeds the theta pathway
                    // gently without brown's extreme -6dB/oct rolloff.
                    // High reverb (0.55) with depth variation during orbit (1-3m).
                    engine.setObject(
                        index: 2, active: true, color: .pink,
                        x: -2.5, y: 0.0, z: 0.5,
                        volume: 0.45, reverbSend: 0.55
                    )
                    engine.setBassModulator(
                        index: 2, kind: .neuralLfo,
                        paramA: 6.0,    // 6 Hz theta-rate
                        paramB: 0.08,   // very subtle
                        paramC: 0.0
                    )
                    engine.setSatelliteModulator(
                        index: 2, kind: .neuralLfo,
                        paramA: 6.0,
                        paramB: 0.08,
                        paramC: 0.0
                    )
                    engine.setObjectColorTint(index: 2, freqHz: 150.0, gainDb: 5.0)
                    movementController.addSatellite(
                        index: 2, pattern: .orbit,
                        radius: 3.0, speed: 0.43, initialPhase: 0.0,
                        depthRange: 1.0...3.0,
                        reverbRange: 0.35...0.65
                    )

                    // ── Object 3: Pink orbit B — asynchronous companion ────────────────
                    // Speed 0.53 rad/s (prime ratio to 0.43 — never synchronizes).
                    // Phase offset 137° (golden angle). The two orbits circle in the
                    // same direction but at different speeds, creating continuously
                    // evolving spatial overlap patterns. The auditory cortex can't
                    // predict when they'll coincide — gentle, ongoing surprise.
                    engine.setObject(
                        index: 3, active: true, color: .pink,
                        x: 2.5, y: 0.0, z: 0.5,
                        volume: 0.45, reverbSend: 0.55
                    )
                    engine.setBassModulator(
                        index: 3, kind: .neuralLfo,
                        paramA: 6.0,
                        paramB: 0.08,
                        paramC: 0.0
                    )
                    engine.setSatelliteModulator(
                        index: 3, kind: .neuralLfo,
                        paramA: 6.0,
                        paramB: 0.08,
                        paramC: 0.0
                    )
                    engine.setObjectColorTint(index: 3, freqHz: 150.0, gainDb: 5.0)
                    movementController.addSatellite(
                        index: 3, pattern: .orbit,
                        radius: 3.0, speed: 0.53, initialPhase: 137.0,
                        depthRange: 1.0...3.0,
                        reverbRange: 0.35...0.65
                    )

                    movementController.start()
    }
}
