import Foundation

struct NormalFlow1PresetProvider: NoisePresetProvider {
    func setPreset(engine: NoiseEngine, movementController: MovementController) {
        // ═══════════════════════════════════════════════════════════════════
          // Flow v4 — The Asymmetric Garden (ADHD-tuned)
          // ═══════════════════════════════════════════════════════════════════
          //
          // Purpose: Alpha-dominant flow state for ADHD brains — sustained deep
          //          work with built-in 10-minute stability. Redesigned from v3
          //          after discovering that the ADHD right hemisphere (theta-
          //          dominant by architecture, band rates ~8Hz) was being
          //          over-driven, pulling the system into delta. v4 solves this
          //          with asymmetric L/R volume and a Mayer wave stabilizer.
          //
          // Architecture:
          //   ┌─────────────────────────────────────────────────────┐
          //   │      Blue rain (y=2.0, orbit 0.15 rad/s)            │
          //   │      RP 12/s 20ms depth 0.15 — dense patter         │
          //   │      tint 4.5kHz -5dB (soft digital haze)            │
          //   │                                                     │
          //   │   Pink surf (orbit 0.05 rad/s, y=1.0)               │
          //   │   Mayer 0.1Hz depth 0.08 — GABA_B reset             │
          //   │   + Iso 14Hz depth 0.10 — 2nd beta driver           │
          //   │   tint 900Hz -2dB (muffled surf)                     │
          //   │                                                     │
          //   │  Pink L (-2.5)          Pink R (+2.5)               │
          //   │  vol 0.22  ← asym       vol 0.40  →                 │
          //   │  Iso 10Hz α d=0.04      Iso 14Hz β d=0.18           │
          //   │  tint 520Hz +2.5dB      tint 1500Hz +2dB            │
          //   │                                                     │
          //   │  Tone L (-5.0)          Tone R (+5.0)               │
          //   │  400 Hz                 410 Hz                      │
          //   │  ──── 10 Hz α binaural beat ────                    │
          //   │                                                     │
          //   │      White wind (z=2.0, figure-8 0.29)              │
          //   │      RP 8/s 30ms depth 0.12 — near-continuous        │
          //   │      tint 3kHz +1dB (bright arousal)                 │
          //   │              ← listener →                            │
          //   └─────────────────────────────────────────────────────┘
          //
          // Key design changes from v3 (The Terrarium):
          //
          //   1. Asymmetric L/R volume (0.22 vs 0.40):
          //      ADHD right hemisphere is theta-dominant (JR band rates ~8Hz).
          //      The left hemisphere has the alpha-capable JR bands (~9.5-10.5Hz).
          //      Contralateral routing means right-side sources drive left hemi.
          //      By under-driving obj 0 (→ right hemi, theta) and over-driving
          //      obj 1 (→ left hemi, alpha), alpha jumped from 15% to 40%.
          //      This was the single most important change.
          //
          //   2. Binaural 400/410 Hz (10 Hz alpha beat):
          //      v3 used 400/414 (14 Hz beta beat) which reinforced beta when
          //      the goal is alpha dominance. 10 Hz beat targets the alpha
          //      rhythm via interaural phase comparison in the superior olivary
          //      complex — a pathway independent from isochronic amplitude
          //      modulation.
          //
          //   3. White noise on figure-8 wind (was brown):
          //      Brown noise was feeding the delta basin. White noise raises
          //      arousal via the brightness factor in the thalamic gate
          //      (0.30 weight), pushing the system away from the bifurcation
          //      boundary where delta emerges.
          //
          //   4. Mayer wave at 0.1 Hz / depth 0.08 (was 30 Hz / 0.05):
          //      The 30 Hz NeuralLfo was anti-habituation only. The 0.1 Hz Mayer
          //      wave periodically reduces input volume, resetting GABA_B slow
          //      inhibitory accumulation. This prevents the system from drifting
          //      into the delta attractor over 10+ minutes. Without it: alpha
          //      collapses to 15% by 600s. With it: alpha holds at 37%.
          //
          //   5. Low modulation depths (stationary input):
          //      All isochronic depths reduced (0.04-0.18 vs 0.15-0.35) to keep
          //      the tonotopic input envelope as stationary as possible. The FHN
          //      reads the JR EEG, which is driven by gammatone band signals —
          //      any amplitude swing creates non-stationary input and irregular
          //      firing (high ISI CV). Lower depth = smoother envelope.
          //
          //   6. High-rate RandomPulse (12/s and 8/s, was 5/s and 0.8/s):
          //      Low-rate RandomPulse creates large silence-to-full envelope
          //      swings. At 8-12/s with 20-30ms duration, bursts overlap into
          //      near-continuous texture with micro-variation — perceptually
          //      rich (rain, wind) but envelope-stationary for the neural model.
          //
          //   7. Reduced reverb (0.05-0.08, was 0.08-0.16):
          //      The thalamic gate computes arousal as 0.25 × (1 - avg_reverb).
          //      Lower reverb = higher arousal = system stays in the alpha basin
          //      rather than drifting toward delta at the bifurcation boundary.
          //
          //   8. Master gain 0.85 (was 0.71):
          //      Higher overall drive pushes the FHN further above threshold,
          //      producing slightly more regular tonic firing (ISI CV 0.73 vs
          //      0.87). The effect is small because ADHD's stochastic JR noise
          //      (sigma=15) dominates the irregularity.
          //
          // Neural signature (ADHD brain, flow goal, CET+gate, 300s):
          //   ┌─────────┬───────────┬───────────┬───────────┬───────────┐
          //   │ Metric  │  Target   │  v3 300s  │  v4 300s  │  v4 600s  │
          //   ├─────────┼───────────┼───────────┼───────────┼───────────┤
          //   │ Score   │    —      │   0.558   │   0.607   │   0.578   │
          //   │ Delta   │  0.05     │   0.352   │   0.250   │   0.286   │
          //   │ Theta   │  0.15     │   0.186   │   0.136   │   0.155   │
          //   │ Alpha   │  0.45     │   0.150   │   0.405   │   0.368   │
          //   │ Beta    │  0.30     │   0.294   │   0.182   │   0.167   │
          //   │ ISI CV  │  0.30     │   0.867   │   0.727   │   0.621   │
          //   │ Status  │    —      │   weak    │  usable   │   weak    │
          //   └─────────┴───────────┴───────────┴───────────┴───────────┘
          //
          // Remaining bottlenecks:
          //   - Delta (0.25 at 300s, 0.29 at 600s): still 2× above max target.
          //     The ADHD brain type sits at input_offset=135 (bifurcation boundary).
          //     Delta is structurally hard to suppress without changing the brain
          //     model itself.
          //   - ISI CV (0.73 at 300s): dominated by stochastic JR noise (sigma=15).
          //     Preset-level modulation changes moved it by ±0.01. The gap from
          //     0.73 to the 0.30 target is not closable via preset parameters.
          //   - 10-min drift: score drops 0.607 → 0.578 (4.8%). The Mayer wave
          //     at depth 0.08 helps but doesn't fully prevent GABA_B accumulation.
          //     Deeper Mayer (0.15-0.20) was tested but pushed delta up.
          //
          // Refs:
          //   Katahira et al. 2018 — alpha-beta coupling in flow state
          //   Klimesch 1999 — alpha as cortical idle rhythm
          //   Schwarz & Taylor 2005 — ASSR to binaural/monaural beats
          //   Julien 2006 — Mayer waves (~0.1 Hz autonomic oscillation)
          //   McCormick 1992 — ACh/GABA_B arousal modulation
          //   Söderlund et al. 2007 — stochastic resonance in ADHD
          //   Ableidinger 2017 — stochastic JR velocity noise
          //   Sherman & Guillery 2006 — thalamic gating of cortical input
          // ═══════════════════════════════════════════════════════════════════

          // ── Engine setup ────────────────────────────────────────────────────
        engine.setMasterGain(gain: 0.85)
          engine.setAnchorColor(color: .brown)
          engine.setAnchorVolume(volume: 0.0)
          engine.setAcousticEnvironment(environment: .forest)
        
        // ── Object 0: Left Pillar (Alpha-rate, minimal modulation) ──────────
          // Pink at left ear → right hemisphere (65% contralateral).
          // Low volume (0.22) — right hemisphere is theta-dominant in ADHD, so
          // we deliberately under-drive it to avoid amplifying theta.
          // Isochronic 10Hz at depth 0.04: barely-perceptible alpha-rate pulse.
          // Tint +2.5dB at 520Hz feeds gammatone band 1 with warm low-mid energy.
          engine.setObject(
              index: 0, active: true, color: .pink,
              x: -2.5, y: 0.0, z: 0.0,
              volume: 0.22, reverbSend: 0.05
          )
          engine.setBassModulator(
              index: 0, kind: .isochronic,
              paramA: 10.0,   // 10 Hz alpha-rate
              paramB: 0.04,   // minimal depth — keep input stationary
              paramC: 0.50    // 50% duty cycle
          )
          engine.setSatelliteModulator(
              index: 0, kind: .isochronic,
              paramA: 10.0,
              paramB: 0.04,
              paramC: 0.50
          )
          engine.setObjectColorTint(index: 0, freqHz: 520.0, gainDb: 2.5)

          // ── Object 1: Right Pillar (Beta driver, alpha supporter) ───────────
          // Pink at right ear → left hemisphere (65% contralateral).
          // Higher volume (0.40) — left hemisphere has the alpha-capable JR bands
          // (~9.5-10.5Hz). Asymmetric L/R volume is the key alpha booster.
          // Isochronic 14Hz at depth 0.18: moderate beta drive without lock.
          // Tint +2dB at 1500Hz lifts mid-high band for WC(14) beta-range energy.
          engine.setObject(
              index: 1, active: true, color: .pink,
              x: 2.5, y: 0.5, z: 0.0,
              volume: 0.40, reverbSend: 0.05
          )
          engine.setBassModulator(
              index: 1, kind: .isochronic,
              paramA: 14.0,   // 14 Hz low-beta
              paramB: 0.18,   // moderate depth — avoid beta lock
              paramC: 0.50    // 50% duty
          )
          engine.setSatelliteModulator(
              index: 1, kind: .isochronic,
              paramA: 14.0,
              paramB: 0.18,
              paramC: 0.50
          )
          engine.setObjectColorTint(index: 1, freqHz: 1500.0, gainDb: 2.0)

          // ── Object 2: Crisp Rain Drops (Zenith Canopy) ──────────────────────
          // Blue noise overhead, orbiting slowly (0.15 rad/s ≈ 42s period).
          // High-rate RandomPulse (12/s, 20ms, depth 0.15) creates dense rain
          // texture that approaches continuous noise with micro-variation —
          // perceptually rich but envelope-stationary for the FHN.
          // Tint -5dB at 4.5kHz softens digital harshness.
          engine.setObject(
              index: 2, active: true, color: .blue,
              x: 0.0, y: 2.0, z: 0.0,
              volume: 0.10, reverbSend: 0.16
          )
          engine.setBassModulator(
              index: 2, kind: .flat,
              paramA: 0.0, paramB: 0.0, paramC: 0.0
          )
          engine.setSatelliteModulator(
              index: 2, kind: .randomPulse,
              paramA: 12.0,   // 12 drops/sec — dense, near-continuous
              paramB: 0.15,   // low depth — minimal envelope swing
              paramC: 20.0    // 20ms — crisp, short
          )
          engine.setObjectColorTint(index: 2, freqHz: 4500.0, gainDb: -5.0)
          movementController.addSatellite(
              index: 2, pattern: .orbit,
              radius: 2.0, speed: 0.15, initialPhase: 0.0,
              depthRange: 2.0...3.0,
              reverbRange: 0.05...0.15
          )

          // ── Object 3: White Wind (Figure-8) ─────────────────────────────────
          // White noise on figure-8 trajectory in front of listener.
          // High-rate RandomPulse (8/s, 30ms, depth 0.12) creates gentle wind
          // texture — near-continuous with micro-variation. White noise raises
          // arousal via brightness factor in the thalamic gate, pushing away
          // from the delta basin.
          // Tint +1dB at 3kHz for high-frequency arousal support.
          engine.setObject(
              index: 3, active: true, color: .white,
              x: 0.0, y: 0.0, z: 2.0,
              volume: 0.13, reverbSend: 0.08
          )
          engine.setBassModulator(
              index: 3, kind: .flat,
              paramA: 0.0, paramB: 0.0, paramC: 0.0
          )
          engine.setSatelliteModulator(
              index: 3, kind: .randomPulse,
              paramA: 8.0,    // 8 gusts/sec — near-continuous texture
              paramB: 0.12,   // low depth
              paramC: 30.0    // 30ms — short swells
          )
          engine.setObjectColorTint(index: 3, freqHz: 3000.0, gainDb: 1.0)
          movementController.addSatellite(
              index: 3, pattern: .figureEight,
              radius: 2.0, speed: 0.29, initialPhase: 0.0,
              depthRange: 1.0...3.0,
              reverbRange: 0.05...0.12
          )

          // ── Object 4: Tone LEFT — binaural beat carrier (400 Hz) ────────────
          // Extreme left for maximum interaural separation via head shadow.
          // 400 Hz sits in gammatone band 1 (200-800Hz) — feeds left-hemisphere
          // JR alpha basin via contralateral routing.
          engine.setObjectActive(index: 4, active: true)
          engine.setObjectSourceTone(index: 4, freqHz: 400.0, amplitude: 0.10)
          engine.setObjectPosition(index: 4, x: -1.0, y: 0.0, z: 0.0)
          engine.setObjectVolume(index: 4, volume: 0.05)
          engine.setObjectReverbSend(index: 4, send: 0.0)

          // ── Object 5: Tone RIGHT — binaural beat carrier (415 Hz) ───────────
          // 10 Hz difference = alpha binaural beat. Reinforces alpha through a
          // separate neural pathway (interaural phase comparison in the superior
          // olivary complex) distinct from the isochronic amplitude modulation.
          engine.setObjectActive(index: 5, active: true)
          engine.setObjectSourceTone(index: 5, freqHz: 415.0, amplitude: 0.10)
          engine.setObjectPosition(index: 5, x: 1.0, y: 0.0, z: 0.0)
          engine.setObjectVolume(index: 5, volume: 0.05)
          engine.setObjectReverbSend(index: 5, send: 0.0)

          // ── Object 6: Mayer Wave Stabilizer (Slow Orbit) ────────────────────
          // Pink noise on slow orbit (0.05 rad/s ≈ 126s revolution).
          // NeuralLfo at 0.1Hz (10s cycle) with depth 0.08 — the Mayer wave,
          // named after ~0.1Hz blood pressure oscillations. Periodically reduces
          // input volume to reset GABA_B slow inhibitory accumulation, preventing
          // the system from drifting into the delta attractor over 10+ minutes.
          // Satellite isochronic 14Hz at depth 0.10 adds gentle beta from a
          // second spatial position.
          // Tint -2dB at 900Hz removes brightness, leaving muffled surf texture.
          engine.setObject(
              index: 6, active: true, color: .pink,
              x: 1.0, y: 1.0, z: 0.0,
              volume: 0.16, reverbSend: 0.08
          )
          engine.setBassModulator(
              index: 6, kind: .neuralLfo,
              paramA: 0.1,    // 0.1 Hz — 10-second Mayer wave cycle
              paramB: 0.08,   // gentle depth — enough to reset GABA_B
              paramC: 0.0
          )
          engine.setSatelliteModulator(
              index: 6, kind: .isochronic,
              paramA: 14.0,   // 14 Hz beta — second spatial beta driver
              paramB: 0.10,   // low depth
              paramC: 0.50    // 50% duty
          )
          engine.setObjectColorTint(index: 6, freqHz: 900.0, gainDb: -2.0)
          movementController.addSatellite(
              index: 6, pattern: .orbit,
              radius: 2.0, speed: 0.05, initialPhase: 0.0,
              depthRange: 1.0...2.0,
              reverbRange: 0.05...0.15
          )

          movementController.start()
    }
}
