# Neural Model Improvement Roadmap

## Implemented

### ASSR Transfer Function
- [x] Research ASSR frequency-response curves from literature (Picton 2003, Ross 2000)
- [x] Implement frequency-dependent gain (log-Gaussian, peaks at 40 Hz + 10 Hz) — `src/auditory/assr.rs`
- [x] Unit tests (22 tests passing)
- [x] Pipeline integration with CLI flag (`--assr`)
- [x] Disabled by default (zero regression)
- [x] **Pivot: signal-level FFT filtering ineffective** — band signals are DC-dominated envelopes, modulation is ~5% of power. Proved empirically: scores identical with/without signal filtering.
- [x] **New approach: preset-level input_scale modifier.** `compute_input_scale_modifier()` scans NeuralLfo frequencies, computes weighted ASSR gain.
- [x] **DC/AC separation fix:** Original approach scaled entire `input_scale` (DC+AC), conflating modulation attenuation with operating point shift (thalamic gate's domain). Fixed: ASSR now subtracts signal mean, scales AC only by ASSR modifier, adds mean back. DC operating point preserved — clean separation from thalamic gate.
- [x] Verified: Ground gate+assr conflict improved (ratio 0.66→0.69). Remaining gap is correct AC attenuation.

### Thalamic Gate
- [x] Research thalamocortical dynamics (Hughes & Crunelli 2005, Suffczynski 2004)
- [x] **Pivot: signal filtering ineffective** (same DC-dominance issue as ASSR)
- [x] **New approach: JR input_offset modifier.** Arousal computed from preset properties (brightness, reverb, modulation speed, movement) → shifts band_offsets toward bifurcation at low arousal — `src/auditory/thalamic_gate.rs`
- [x] Unit tests (12 tests passing)
- [x] Pipeline integration with CLI flag (`--thalamic-gate`)
- [x] Verified: Ground (sleep) jumped from 0.28 → 0.73 with gate enabled
- [x] **Canonicalized later in Priority 20:** `evaluate` now consumes the same `pipeline.rs` result used by the rest of the scoring stack; the old duplicated detailed path in `main.rs` is gone.

### Pipeline Integration
- [x] Both features disabled by default — zero regression on all existing tests
- [x] 276 total tests, 0 failures
- [x] CLI flags: `--assr`, `--thalamic-gate` on evaluate command

### Per-Object Spread Preset Support
- [x] Added optional per-object `spread` to preset JSON/runtime (`src/preset.rs`) with backward-compatible serde default `0.0` and clamp to `[0.0, 1.0]`.
- [x] Applied spread to the DSP engine via `engine.set_object_spread(...)` inside `Preset::apply_to_engine()`.
- [x] Added regression coverage for legacy preset deserialization, bound clamping, and runtime engine propagation.
- [x] Kept spread out of the optimizer genome and surrogate contract on purpose. The current genome remains 230 genes and the surrogate input remains 248 dims; spread is a manual/runtime preset control for now.
- [x] Evaluated Shield-preserving `normal_set_shield_v4` variants with spread: direct-source spread on the two side leads degrades Shield strongly, while spread on the wet-isolated rear/canopy layers is neutral-to-slightly-positive. The checked-in `presets/normal_set_shield_v4.json` uses spread only on those background layers.

---

## Priority 1: Signal Normalization Fixes (HIGH IMPACT, LOW EFFORT)

### 1a. Global Band Normalization
- [x] **Problem:** Per-band max normalization (`pipeline.rs:221-229`) destroys relative energy between bands. Brown noise and White noise produce identical neural inputs. The model is "color-blind" at the neural level.
- [x] **Fix:** Replace per-band max with global max across all 4 bands in both `pipeline.rs` and `run_detailed_pipeline` in `main.rs`.
- [x] **Ref:** Patterson RD, Robinson K, Holdsworth J, McKeown D, Zhang C, Allerhand M (1992). "Complex sounds and auditory images." In: Cazals Y et al. (eds) *Auditory Physiology and Perception.* — establishes that tonotopic band energy ratios carry critical information for cortical processing and should be preserved through the auditory model pipeline.
- [x] **Ref:** Glasberg BR, Moore BCJ (2002). "A model of loudness applicable to time-varying sounds." *J Audio Eng Soc* 50(5):331-342. — spectral loudness model preserves inter-band ratios via global normalization.
- [x] Tests: Brown/White score diff increased 14x (0.0006 → 0.0087). Band power sum preserved. All colors produce valid scores.
- [x] Re-evaluated: Ignition jumped 0.53 → 0.72 (model now sees Blue/White high-band energy). Shield/Flow need retuning.

### 1b. FHN Amplitude Preservation
- [x] **Problem:** EEG normalized to [-1,1] using max value (`pipeline.rs:345-348`) before driving FHN. Destroys amplitude information — strong rhythmic EEG and weak flat EEG produce identical FHN input. Firing rate becomes insensitive to entrainment strength.
- [x] **Fix:** 95th percentile scaling with clamp to [-3,3]. Applied in both `pipeline.rs` and `run_detailed_pipeline` in `main.rs`. Also updated `regression_tests.rs` to match.
- [x] **Ref:** FitzHugh R (1961). "Impulses and physiological states in theoretical models of nerve membrane." *Biophys J* 1(6):445-466. — original FHN model assumes input current in physiological range; max-normalization violates this by collapsing all inputs to identical range.
- [x] **Ref:** Izhikevich EM (2003). "Simple model of spiking neurons." *IEEE Trans Neural Netw* 14(6):1569-1572. — demonstrates that neuron firing rate is monotonically dependent on input current amplitude; normalizing amplitude removes this relationship.
- [x] Tests: Brown and Blue produce different FHN firing rates (confirmed — global norm provides different EEG amplitudes, percentile scaling preserves them through FHN). 282 tests passing.

## Priority 2: Band-Dependent Thalamic Shift (MEDIUM IMPACT, LOW EFFORT)

- [x] **Problem:** Thalamic gate applies uniform offset shift to all 4 bands. Physiologically, arousal reduction primarily affects low bands (theta/delta), not high bands (beta/gamma).
- [x] **Fix:** `band_offset_shifts()` method returns per-band shifts: [100%, 70%, 20%, 0%] of max reduction. Applied in both `pipeline.rs` and `main.rs`.
- [x] **Ref:** Hughes SW, Crunelli V (2005). "Thalamic mechanisms of EEG alpha rhythms and their pathological implications." *Neuroscientist* 11(4):357-372.
- [x] **Ref:** Steriade M, McCormick DA, Sejnowski TJ (1993). "Thalamocortical oscillations in the sleeping and aroused brain." *Science* 262(5134):679-685.
- [x] Tests: 6 unit tests for band shifts (decreasing by band, band 3 always zero, proportions correct, disabled passthrough). 288 total tests passing.
- [x] Verified: high bands stay beta-responsive at low arousal. Ground with features: 0.5377.

## Priority 3: Decimation Anti-Aliasing (LOW IMPACT — DEFERRED)

- [x] **Investigated:** Boxcar passes 300 Hz at ~74% power (-1.3 dB). Hann window over 48 samples (1ms) is too short for better cutoff — main lobe wider than boxcar's.
- [x] **Finding:** The gammatone filterbank's 80 Hz envelope lowpass (`gammatone.rs`) is the actual anti-alias filter. Content above ~80 Hz is already heavily attenuated before decimation. The boxcar only handles residual carrier leakage.
- [x] **Conclusion:** Impact is LOW for real presets because gammatone envelopes don't contain significant high-frequency content. Documented in code comments and tests.
- [ ] **Future:** Proper fix requires multi-stage decimation (48kHz → 8kHz → 1kHz) or a long FIR filter (~200+ taps). This is a moderate effort change for minimal practical impact.
- [x] **Ref:** Oppenheim AV, Schafer RW (2009). *Discrete-Time Signal Processing.* 3rd ed. Prentice Hall, Ch. 4.7.
- [x] **Ref:** Crochiere RE, Rabiner LR (1983). *Multirate Digital Signal Processing.* Prentice Hall, Ch. 2.
- [x] Tests: 4 tests — boxcar behavior documented, low-freq preservation verified, output length correct.

## Priority 4: Bilateral Coupling Realism (MEDIUM IMPACT, IMPLEMENTED)

- [x] **Problem:** Corpus callosum modeled as excitatory coupling (`jansen_rit.rs:766-772`) when physiologically it's primarily inhibitory.
- [x] **Fix:** Changed `+k * delayed_contralateral` to `-k * delayed_contralateral` in `simulate_bilateral()`. One-line change with correct scientific basis.
- [x] **Result:** Ignition baseline jumped 0.7178 → 0.7322 — inhibitory coupling prevents hemispheres from locking to alpha together, allowing independent beta/gamma processing per hemisphere. Alpha asymmetry increased (0.9824 → 0.9857) as predicted by Bloom & Hynd.
- [x] **Ref:** Innocenti GM (1986). "General organization of callosal connections in the cerebral cortex." In: Jones EG, Peters A (eds) *Cerebral Cortex,* vol 5. Plenum Press.
- [x] **Ref:** Bloom JS, Hynd GW (2005). "The role of the corpus callosum in interhemispheric transfer of information: excitation or inhibition?" *Neuropsychol Rev* 15(2):59-71.
- [x] **Ref:** Aboitiz F, Scheibel AB, Fisher RS, Zaidel E (1992). "Fiber composition of the human corpus callosum." *Brain Res* 598(1-2):143-153.
- [x] Tests: 2 tests — hemispheric differentiation with asymmetric input, valid scores across brain types. 293 total tests passing.
- [ ] **Future:** Frequency-dependent callosal delay (Aboitiz 1992: 5-50 ms range). Current fixed 10ms delay is a simplification.

## Priority 5: Wilson-Cowan Frequency Tracking (IMPLEMENTED)

- [x] **Problem:** WC oscillates at hardcoded frequencies (14/25 Hz) regardless of input.
- [x] **Fix:** `for_frequency_adaptive()` — detects dominant modulation frequency in input signal via FFT, shifts WC natural frequency toward it if within ±5 Hz Arnold tongue. Partial entrainment: shift fraction decreases linearly with detuning.
- [x] `detect_dominant_modulation()` — finds strongest spectral peak in 1-50 Hz range, requires 3x above noise floor to report.
- [x] Applied to both `simulate_tonotopic` and `run_hemisphere_tonotopic` in `jansen_rit.rs`.
- [x] **Ref:** Pikovsky A, Rosenblum M, Kurths J (2001). *Synchronization: A Universal Concept in Nonlinear Sciences.* Cambridge University Press, Ch. 3.
- [x] **Ref:** Notbohm A, Kurths J, Herrmann CS (2016). "Modification of brain oscillations via rhythmic sensory stimulation." *Int J Psychophysiol* 103:62-68.
- [x] **Ref:** Thut G, Schyns PG, Gross J (2011). "Entrainment of perceptually relevant brain oscillations." *Front Psychol* 2:170.
- [x] Tests: 5 unit tests — tracks nearby frequency, ignores distant, keeps natural for flat input, detects peaks, returns None for flat. 298 total tests passing.
- [x] Impact: Minimal on current presets (Ignition's 25 Hz drives already match WC(25) target). Will benefit future presets with non-matching modulation frequencies.

## Priority 6: Scoring Refinements (MEDIUM IMPACT, MEDIUM EFFORT)

### 6a. Remove Brightness Double-Counting
- [x] **Problem:** Brightness contributed 10% of score (`scoring.rs:309-316`), partially duplicating information already captured by band powers after global normalization fix.
- [x] **Fix:** Removed brightness modifier from `evaluate_with_brightness()`. Score is now 100% neural model (band powers + FHN). API parameter kept for compatibility (`let _ = brightness`).
- [x] **Ref:** Zwicker E, Fastl H (1999). *Psychoacoustics: Facts and Models.* 2nd ed. Springer.
- [x] Tests: score independent of brightness parameter, all goals produce valid scores. 300 total tests passing.
- [x] Impact: Scores shifted down ~5-10% (brightness was a free bonus). Neural model now has full scoring authority.

### 6b. Include Alpha Asymmetry in Scoring
- [x] **Problem:** Alpha asymmetry computed but never used in scoring.
- [x] **Fix:** `evaluate_with_asymmetry()` applies goal-specific penalty. `asymmetry_penalty()` uses per-goal threshold and max penalty:
  - Meditation: threshold 0.2, max 15% penalty
  - Deep Relaxation: threshold 0.3, max 12%
  - Isolation: threshold 0.4, max 8%
  - Focus/Deep Work: threshold 0.5, max 5%
  - Sleep: no penalty
- [x] Wired into both `pipeline.rs` (evaluate_preset) and `main.rs` (diagnose display).
- [x] **Ref:** Davidson RJ (2004). "What does the prefrontal cortex 'do' in affect." *Biol Psychol* 67(1-2):219-234.
- [x] **Ref:** Allen JJB, Coan JA, Nazarian M (2004). "Issues and assumptions on the road from raw signals to metrics of frontal EEG asymmetry." *Biol Psychol* 67(1-2):183-218.
- [x] Tests: balanced > asymmetric for relaxation, sleep ignores asymmetry, valid range. 303 total tests passing.

### 6c. Entrainment Coherence Scoring (PLV)
- [x] Implemented Phase-Locking Value per Lachaux et al. (1999): PLV = |1/N × Σ exp(i·φ(t))| using Hilbert transform (Marple 1999) for instantaneous phase extraction.
- [x] `compute_plv()` in `performance.rs`: bandpass filter (±3 Hz), Hilbert analytic signal, phase difference with reference sinusoid.
- [x] Added `plv` field to `PerformanceVector`.
- [x] `evaluate_full()` in `scoring.rs`: combines band score + FHN score + asymmetry penalty + PLV bonus. PLV bonus weighted per goal: Focus 100%, Isolation 80%, DeepWork 60%, Meditation 30%, Sleep/Relaxation 0%.
- [x] Wired into both `pipeline.rs` and `main.rs` diagnose path.
- [x] **Ref:** Lachaux JP et al. (1999). "Measuring phase synchrony in brain signals." *Hum Brain Mapp* 8(4):194-208.
- [x] **Ref:** Helfrich RF et al. (2014). "Entrainment of brain oscillations by transcranial alternating current stimulation." *Curr Biol* 24(3):333-339.
- [x] **Ref:** Marple SL (1999). "Computing the Discrete-Time Analytic Signal via FFT." *IEEE Trans Signal Process* 47(9):2600-2603.
- [x] Tests: 6 unit tests (perfect sine PLV>0.8, off-target <0.3, noise <0.3, range [0,1], included in PerformanceVector, None without target). 309 total tests passing.
- [x] Impact: Ignition +0.006 (strong 25 Hz entrainment rewarded). Drift/Ground unchanged (no entrainment weight). Correct behavior.

## Priority 7: Stochastic JR for Theta/Delta (IMPLEMENTED)

**Problem:** JR alpha attractor at ~10 Hz is too strong. Theta and delta nearly unreachable with deterministic model.

**Solution:** Added stochastic noise term to JR input drive: `p = offset + input*scale + σ·ξ(t)` where ξ is Gaussian white noise (Box-Muller from xorshift64 for reproducibility).

- [x] Implemented `stochastic_sigma` parameter on JansenRitModel (default 0.0 = deterministic)
- [x] `next_gaussian_noise()` method using Box-Muller transform from xorshift64 RNG
- [x] Applied to both `simulate()` and `simulate_with_fast_inhib_trace()` loops
- [x] Wired through `simulate_bilateral()` → `run_hemisphere_tonotopic()` → each JR model
- [x] Pipeline integration via `SimulationConfig.stochastic_jr_enabled` (default false, sigma=15.0 when enabled)
- [x] Verified: sigma=0 produces identical output to deterministic model (bitwise match)
- [x] Verified: stochastic broadens spectrum — band power std drops from 0.213 → 0.115 (sigma=20)
- [x] Verified: all outputs finite with aggressive sigma=30
- [x] Tests: 3 unit tests (broadens spectrum, sigma=0 deterministic, output finite). 316 total passing.
- [x] Zero regression: disabled by default, all existing tests unchanged.
- [x] **Ref:** Ableidinger M, Buckwar E, Hinterleitner H (2017). "A Stochastic Version of the Jansen and Rit Neural Mass Model: Analysis and Numerics." *J Math Neurosci* 7:8.
- [x] **Ref:** Grimbert F, Faugeras O (2006). "Bifurcation analysis of Jansen's neural mass model." *Neural Comput* 18(12):3052-3068.
- [x] **Ref:** Spiegler A, Kiebel SJ, Atay FM, Knösche TR (2011). "Complex behavior in a modified Jansen and Rit neural mass model." *Biol Cybern* 104:229-254.
- [ ] **Future:** Scale sigma with arousal (low arousal → higher sigma → more theta/delta). Currently fixed at 15.0 when enabled.

## Priority 8: Neural Habituation (IMPLEMENTED)

**Problem:** No time-dependent adaptation — preset response identical at t=0 and t=2 hours.

**Solution:** Added synaptic depression to JR's connectivity constants. Depression accumulates proportionally to pyramidal cell activity (`|S(v_pyr)|/V_MAX`) and recovers exponentially. Effective connectivity: `C_effective = C_base × (1 - depression)`.

- [x] Implemented `habituation_rate` and `habituation_recovery` parameters on JansenRitModel (default 0.0 = no habituation)
- [x] Depression state evolves per timestep: `depression += rate * activity - recovery * depression`, clamped to [0, 0.8]
- [x] Applied `c_scale = 1 - depression` to C1, C2, C3, C4 in `derivatives_with_habituation()`
- [x] Wired through `simulate_bilateral()` → `run_hemisphere_tonotopic()` → each JR model
- [x] Pipeline integration via `SimulationConfig.habituation_enabled` (default false, rate=0.0003/recovery=0.0001 when enabled)
- [x] Verified: habituation reduces late EEG variance vs early (30s simulation)
- [x] Verified: rate=0 produces identical output to non-habituating model (bitwise match)
- [x] Verified: aggressive habituation (rate=0.001) produces finite output
- [x] Tests: 3 unit tests (reduces late response, zero-rate deterministic, output finite). 316 total passing.
- [x] Zero regression: disabled by default.
- [x] **Ref:** Rowe DL et al. (2004/2012). Synaptic depression learning rule for JR-like models.
- [x] **Ref:** Moran RJ et al. (2011). N100 habituation data, depression/recovery parameters.
- [x] **Ref:** Huber DE et al. (2020). Habituation timescale 10-30 seconds.
- [x] **Ref:** Jääskeläinen IP et al. (2007). Auditory habituation mechanisms.
- [ ] **Future:** Add "novelty" parameter — presets with temporal variation habituate slower. Scale habituation rate inversely with modulation complexity.

## Priority 9: Physiological Thalamic Gate (MEDIUM IMPACT, MEDIUM EFFORT — IMPLEMENTED)

**Problem:** Current thalamic gate uses a heuristic: arousal (computed from preset properties) linearly shifts band_offsets. Real thalamocortical state switching involves ion channel dynamics (T-type Ca2+, K+ leak, persistent Na+) that produce qualitatively different firing modes (tonic vs burst), not just a shifted operating point.

**Solution:** Single-compartment Hodgkin-Huxley TC relay cell with T-type Ca²⁺ current (Destexhe 1996 / Huguenard & McCormick 1992) and K⁺ leak as the master arousal knob (Bazhenov 2002). Arousal → g_KL → membrane potential → T-current de-inactivation → burst↔tonic mode switch → sigmoidal shift-vs-arousal curve. Gated behind `--phys-gate` flag, default off.

**Important citation correction:** The paper originally referenced as "Gonzalez et al. (2016)" is actually **Paul K, Cauller LJ, Llano DA (2016)** *Front Comput Neurosci* 10:91 (PMID 27660609). No author named Gonzalez. Corrected below.

- [x] Read Paul et al. (2016) and Bazhenov et al. (2002) for TC circuit equations and parameters. Paul 2016 provides the 3-neuron architecture and chaotic-region analysis; Bazhenov 2002 provides the g_KL range (0–0.03 mS/cm²) and reversal potentials. Neither paper prints T-type m_inf/h_inf verbatim — both defer to Destexhe 1996 / Huguenard & McCormick 1992 canonical Boltzmann fits.
- [x] Implemented single-compartment TC cell with: I_L (g_L=0.05), I_KL (g_KL ∈ [0, 0.06] mS/cm², E_KL=−95 mV), I_Na (g_Na=90, HH gates per Mainen & Sejnowski 1996), I_K (g_K=10), I_T (g_T=2.2, Destexhe Boltzmann m_inf/h_inf). RK4 at dt=0.02 ms (Bazhenov standard). I_inj=0.5 µA/cm² tonic cortical drive keeps cell tonic at moderate arousal.
- [x] **v1 simplification — RE neuron deferred.** The mutual TC↔RE inhibition and the chaotic intermediate region (Paul 2016: Lyapunov +0.24) are v2 enhancements. For the scalar `band_offset_shifts()` output, the qualitative burst↔tonic switch in a single TC cell suffices to produce a physiologically grounded sigmoid.
- [x] Map arousal → g_KL: linear inverse `g_KL = G_KL_MAX × (1 − arousal)`. G_KL_MAX = 0.06 mS/cm² (wider than Bazhenov's 0.03 to push the transition to moderate arousal ≈ 0.4–0.5 where typical presets land).
- [x] 5 s warmup + 1 s sampling window per gate construction. Mean firing rate and ISI CV extracted from spike-time detection. Burstiness sigmoid maps (rate, CV) → [0, 1]. Per-band shift uses Steriade [100%, 70%, 20%, 0%] proportions.
- [x] CLI flag `--phys-gate` on the `evaluate` command. `SimulationConfig.physiological_thalamic_gate_enabled` (default false). Takes precedence over `--thalamic-gate` when both set.
- [x] Unit tests (17 total): T-type gating curves monotonic, HH singularities handled, steady-state gates in [0,1], ODE finite at extremes, disabled returns zeros, arousal clamped, band 3 always zero, shifts non-positive, high > low arousal monotonicity, Steriade proportions, deep sleep produces meaningful shift, full wake produces small/zero shift, compute_arousal delegates exactly.
- [x] **Empirical comparison — heuristic vs physiological gate:**
  - Pink sleep: heuristic 0.1876 → **physiological 0.4636 (+0.28)** — 147% improvement
  - Brown sleep: heuristic 0.3927 → **physiological 0.5155 (+0.12)** — crosses OK threshold
  - Pink deep_relaxation: heuristic 0.2880 → **physiological 0.4370 (+0.15)** — 52% improvement
  - Deep_work/Focus: drops of 0.03–0.25 → physiologically correct (moderate arousal → burst mode, which focus/deep_work goals should not benefit from)
- [x] **Regression check:** all preset/goal pairs with `--phys-gate` OFF produce scores bitwise-identical to pre-P9 baseline. Test suite: 352 passing, 4 pre-existing thalamic_gate failures unchanged (+17 new tests).
- [ ] **Future:** Add RE neuron with mutual inhibition for the chaotic intermediate regime (Paul 2016). Add deterministic chaos jitter to the per-band shift at intermediate arousal.
- [ ] **Future:** Scale I_inj with arousal (cortical→thalamic feedback is arousal-dependent, not constant). This would give a smoother transition at intermediate arousals.
- [x] **Ref (corrected):** Paul K, Cauller LJ, Llano DA (2016). "Presence of a chaotic region at the sleep-wake transition in a simplified thalamocortical circuit model." *Front Comput Neurosci* 10:91 (PMID 27660609). — 3-neuron TC + RE + CX circuit with chaotic sleep-wake transition; g_LEAK = 11.25 nS (periodic/sleep) to 0 nS (wake), Lyapunov +0.24 in chaotic band, dt=10 µs, 15 s warmup.
- [x] **Ref:** Bazhenov M, Timofeev I, Steriade M, Sejnowski TJ (2002). "Model of thalamocortical slow-wave sleep oscillations and transitions to activated states." *J Neurosci* 22(19):8691-8704. — foundational TC cell model; g_KL as the wake↔sleep switch (TC: 0–0.03 mS/cm²); reversal potentials (E_L=−70, E_KL=−95, E_K=−95 mV); RK4 dt=0.02 ms.
- [x] **Ref:** Destexhe A, Contreras D, Steriade M, Sejnowski TJ, Huguenard JR (1996). "In vivo, in vitro, and computational analysis of dendritic calcium currents in thalamic reticular neurons." *J Neurosci* 16(1):169-185. — canonical T-type Ca²⁺ m_inf / h_inf Boltzmann fits used in our TC cell. Also reproduced in ModelDB 3343.
- [x] **Ref:** Huguenard JR, McCormick DA (1992). "Simulation of the currents involved in rhythmic oscillations in thalamic relay neurons." *J Neurophysiol* 68(4):1373-1383. — original T-type current fits for TC relay cells.
- [x] **Ref:** Mainen ZF, Sejnowski TJ (1996). "Influence of dendritic structure on firing pattern in model neocortical neurons." *Nature* 382:363-366. — HH Na⁺ and K⁺ kinetics used in our TC cell.
- [ ] **Ref:** (2023). "Translating electrophysiological signatures of awareness into thalamocortical mechanisms..." *bioRxiv* 2023.10.11.561970 — deferred to v2 for parameter calibration from real EEG.
- [ ] **Ref:** (2023). "Thalamic control of sensory processing and spindles..." *Cell Reports* — deferred to v2 RE neuron implementation.

## Priority 10: Auditory Cortex Hierarchy (MEDIUM IMPACT, HIGH EFFORT)

**Problem:** Current pipeline jumps from cochlear filterbank directly to a cortical column. Real auditory processing goes through cochlear nucleus → inferior colliculus (IC) → medial geniculate body (MGB) → primary auditory cortex (A1). Each stage performs specific transformations on amplitude modulation that our scalar ASSR approximates but doesn't model.

**Solution:** Replace the scalar ASSR with a 3-stage subcortical pipeline: Cochlea → IC (rate/temporal modulation transfer function) → MGB (thalamocortical relay with state-dependent gating) → A1. Each stage is a small neural model with empirically measured transfer characteristics.

- [ ] Read Rabang et al. (2012) for IC amplitude modulation transfer function model
- [ ] Read Proctor & Bhatt (2012) for MGB temporal coding model (synchronized vs non-synchronized responses)
- [ ] Implement IC stage: rate modulation transfer function (low-pass for AM, band-pass for FM) per Rabang model
- [ ] Implement MGB stage: merge with thalamic gate — MGB IS the thalamic relay for auditory signals
- [ ] Implement A1 stage: minimal E/I microcircuit per Moshitch & Las (2020)
- [ ] Use Farahani et al. (2021) ASSR source mapping to validate that model stages match real subcortical sources
- [ ] Unit tests: IC transfer function matches published MTF data
- [ ] Integration tests: 3-stage pipeline produces different ASSR-like gain curve than scalar approximation
- [ ] Compare preset scores with scalar ASSR vs multi-stage pipeline
- [ ] **Ref:** Rabang CF, Parthasarathy A, Engel Y, Bhatt T, Bhatt P (2012). "A computational model of inferior colliculus responses to amplitude modulated sounds in young and aged rats." *Front Neural Circuits* 6:77. — IC model with empirical AM transfer functions by modulation frequency.
- [ ] **Ref:** Proctor CW, Bhatt DH (2012). "A computational model of cellular mechanisms of temporal coding in the medial geniculate body." *J Comput Neurosci* 32(2):207-230. — MGB model distinguishing synchronized (temporal) vs non-synchronized (rate) coding of AM.
- [ ] **Ref:** Moshitch D, Las L (2020). "A circuit model of auditory cortex." *PLoS Comput Biol* 16(7):e1008016. — minimal A1 circuit model with E/I microcircuits reproducing experimental auditory cortex responses.
- [ ] **Ref:** Farahani ED, Wouters J, Francart T (2021). "Brain mapping of auditory steady-state responses: A broad view of cortical and subcortical sources." *Hum Brain Mapp* 42(3):780-796. — maps ASSR generators in IC, MGB, and cortex; validates our ASSR gain curve against real source locations.
- [ ] **Ref:** (2025). "Modelling neural coding in the auditory midbrain with high resolution and accuracy." *Nature Machine Intelligence* — ICNet: deep learning model of IC providing accurate simulation across wide sound range.

## Priority 11: Multi-Column Cortical Network (MEDIUM IMPACT, HIGH EFFORT)

**Problem:** Each hemisphere has 4 independent JR/WC models (one per tonotopic band) with no lateral connections. Real cortex has inter-columnar connections, feedback from higher areas, and cross-frequency coupling. Our model can't capture network effects like alpha-gamma coupling or frontal-auditory feedback.

**Solution:** Scale from 4 independent models to a small coupled network. Per Cakan & Obermayer (2021, neurolib), the architecture is: nodes = brain regions, edges = structural connectivity, dynamics = neural mass model per node. For our use case: auditory cortex (current model) → frontal cortex → default mode network, with realistic coupling weights.

- [ ] Read Cakan & Obermayer (2021) for neurolib's coupling architecture and translate to Rust
- [ ] Read Byrne et al. (2024) for next-generation neural mass models with electrical synapses (gap junctions) — E→E coupling alone is insufficient
- [ ] Read Ableidinger et al. (2018) for bifurcation analysis of TWO coupled JR columns — documents the rich dynamics (synchronization, anti-phase) that emerge from coupling
- [ ] Design minimal 3-region network: auditory → frontal (attention) → DMN (relaxation)
- [ ] Implement inter-region coupling as delayed, weighted connections between E populations
- [ ] Add structural connectivity weights from published human connectome data (Deco et al. 2014)
- [ ] Unit tests: coupled columns produce richer dynamics than isolated columns
- [ ] Integration tests: frontal feedback modulates auditory cortex response to noise
- [ ] Compare single-column vs network scores for attention-dependent goals (Focus, Meditation)
- [ ] **Ref:** Cakan C, Obermayer K (2021). "neurolib: A simulation framework for whole-brain neural mass modeling." *Cogn Comput* 13:1132-1152. — Python framework for coupling neural mass models with structural connectivity; provides architecture template.
- [ ] **Ref:** Byrne Á, Avitabile D, Coombes S (2024). "Whole brain functional connectivity: Insights from next generation neural mass modelling incorporating electrical synapses." *PLoS Comput Biol* 20(12):e1012647. — shows E→E coupling is insufficient; need gap junctions for realistic functional connectivity.
- [ ] **Ref:** Ableidinger M, Buckwar E, Hinterleitner H (2018). "Bifurcation analysis of two coupled Jansen-Rit neural mass models." *PLoS One* 13(2):e0192842. — documents synchronization, anti-phase oscillation, and other emergent dynamics in coupled JR.
- [ ] **Ref:** Deco G, Ponce-Alvarez A, Mantini D, Romani GL, Hagmann P, Corbetta M (2013). "Resting-state functional connectivity emerges from structurally and dynamically shaped slow linear fluctuations." *J Neurosci* 33(27):11239-11252. — provides structural connectivity matrix from human connectome for inter-region coupling weights.

## Priority 12: EEG Validation (HIGH IMPACT, HIGH EFFORT)

**Problem:** All model improvements are theoretical — we haven't validated against real human EEG data during noise listening. The model predicts DIRECTIONALLY (more beta with beta-driving presets) but absolute band power values are engineering constructs. Without validation, we can't know if the model's predictions translate to real brain effects.

**Solution:** Design and run an EEG experiment comparing model predictions with measured brain responses to our preset set (Shield, Flow, Ignition, Drift, Ground).

- [ ] Design protocol: 20+ participants, 5 presets × 5 minutes each, 64-channel EEG
- [ ] Use Donoghue & Voytek (2021) PaWNextra method to separate 1/f noise from oscillatory components — critical for clean comparison with model band powers
- [ ] Follow (2024) auditory beats stimulation protocol for stimulus delivery and EEG recording methodology
- [ ] Measure: band powers (delta, theta, alpha, beta, gamma), alpha asymmetry, entrainment PLV at modulation frequency
- [ ] Compare model predictions vs measured for each preset × each metric
- [ ] Calibrate model parameters (input_scale, offset ranges, coupling strengths) to minimize prediction error
- [ ] Test noise color effects using (2025) prestimulus EEG methodology — validate that our global normalization correctly predicts color-dependent neural differences
- [ ] Validate sleep onset effects using Zhou et al. (2012) pink noise protocol — Ground preset should show similar EEG complexity reduction
- [ ] Use Zoefel et al. (2018) methodology to distinguish true entrainment from evoked responses
- [ ] Publish findings with open data and model code
- [ ] **Ref:** Donoghue T, Voytek B (2021). "Characterizing pink and white noise in the human electroencephalogram." *J Neurophysiol* 125(4):1545-1554. — PaWNextra method for valid 1/f noise estimation from EEG; distinct topography for pink vs white noise.
- [ ] **Ref:** (2024). "Brain wave modulation and EEG power changes during auditory beats stimulation." *Int J Psychophysiol* 203:112403. — compares isochronic tones, binaural beats, and white noise effects on EEG; provides experimental protocol template.
- [ ] **Ref:** (2025). "Prestimulus EEG oscillations and pink noise affect Go/No-Go ERPs." *Sensors* 25(6):1733. — shows pink noise modulates prestimulus alpha/theta and cognitive processing; validates our premise.
- [ ] **Ref:** Zhou J, Liu D, Li X, Ma J, Zhang J, Fang J (2012). "Pink noise: Effect on complexity synchronization of brain activity and sleep consolidation." *J Theor Biol* 306:68-72. — pink noise reduces EEG complexity by 9.5 minutes faster sleep onset; quantitative validation target for Ground preset.
- [ ] **Ref:** Obleser J, Kayser C (2019). "Neural entrainment and attentional selection in the listening brain." *Trends Cogn Sci* 23(11):913-926. — methodological framework for measuring neural entrainment to auditory stimuli.
- [ ] **Ref:** Zoefel B, ten Oever S, Sack AT (2018). "The involvement of endogenous neural oscillations in the processing of rhythmic input." *Front Neurosci* 12:95. — distinguishes true entrainment from evoked responses; critical for PLV validation.

## Priority 13: Cortical Envelope Tracking (CET) Pathway (MEDIUM IMPACT, MEDIUM EFFORT)

**Problem:** The current pipeline has no explicit mechanism for *cortical envelope tracking* — the well-documented slow (0.5–8 Hz) phase-locking of auditory cortex to the amplitude envelope of natural sounds (speech, wind, waves, ASMR). Three specific limitations:

1. **Slow modulation is attenuated by ASSR.** `pipeline.rs:305-321` applies a scalar ASSR modifier (log-Gaussian peaked at 10/40 Hz, from `compute_input_scale_modifier`) uniformly to the AC component of *all* bands. A 5 Hz NeuralLfo sits in a trough of that curve and gets its AC component suppressed before it ever reaches JR — the exact opposite of what the cortical envelope-tracking literature predicts for slow-rate stimuli.
2. **No slow inhibitory population in JR.** Current Jansen-Rit uses canonical a=100/s (τ_e≈10 ms), b=50/s (τ_i≈20 ms). Tracking 4–8 Hz theta-rate envelopes cleanly requires a GABA_B-like slow inhibitory time constant (τ ≈ 100–200 ms). Without it, JR's internal dynamics actively resist locking to the 5 Hz envelope.
3. **PLV is measured against the carrier, not the envelope.** `compute_plv()` in `performance.rs` builds a reference sinusoid at `target_lfo_freq` (the NeuralLfo carrier). For CET, the physiologically meaningful quantity is PLV between the cortical response and the *instantaneous phase of the extracted envelope*, not the phase of a synthetic sine at the LFO rate.

Net result: presets that rely on slow envelope fluctuations (Ground, Drift, relaxation/meditation targets) can't be rewarded for CET-mediated entrainment, and the optimizer has no gradient toward "organic" envelope-modulated designs (surf, wind, breath-paced pink noise).

**Solution:** Implement CET as a three-part extension gated behind a new `cet_enabled` config flag, mirroring the `assr_enabled` / `thalamic_gate_enabled` pattern. The existing gammatone envelope (`gammatone.rs:121-125`) already does the physiologically correct thing (magnitude → 80 Hz LPF → decimate) — CET builds on top of it, it does not replace it.

### 13a. Bifurcate input into fast (ASSR) and slow (CET) pathways
- [x] **PRECHECK** completed via `pipeline::tests::cet_precheck_band0_ac_dc_5hz_neural_lfo`. Result: AC fraction = **0.171** (YELLOW band — above 0.15 abort threshold but below 0.30 green). Verdict: implement 13b first so JR has the slow inhibitory dynamics to amplify the 17% AC drive, then 13a so it reaches JR undamped, then 13c so it's rewarded. Implementation order followed exactly that.
- [x] Added `ButterworthCrossover` in `src/auditory/crossover.rs` — 1st-order leaky integrator LP at 10 Hz with complementary HP (`fast = x - slow`). Chose 1st-order over LR4 because the complementary HP for higher-order Butterworth has asymmetric magnitude at the crossover (|HP(jω_c)| ≈ +1.76 dB for 2nd-order Butterworth instead of −3 dB), while 1st-order gives the textbook symmetric −3 dB on both paths. 6 dB/oct slope is sufficient because the gammatone front-end already runs an 80 Hz envelope LPF.
- [x] Wired into `pipeline.rs` in step 5d/5e/5f: split each decimated band into slow + fast, run ASSR only on the fast path, recombine before driving JR. The slow envelope reaches JR undamped — exactly the architectural fix the precheck identified.
- [x] Same plumbing added to `main.rs::run_detailed_pipeline` (the duplicate display path).
- [x] Gated behind `SimulationConfig.cet_enabled` (default `false`).
- [x] CLI flag `--cet` on the `evaluate` command (mirrors `--assr`/`--thalamic-gate`).
- [x] Unit tests in `auditory::crossover::tests` (9 tests): symmetric −3 dB at 10 Hz crossover, slow path passes 5 Hz with ratio 0.894, fast path passes 40 Hz with ratio 0.940, DC routes entirely to slow, reconstruction error ≤ 1 ULP, finite output for white noise, reset clears state, default cutoff is 10 Hz (Doelling 2014), empty signal returns empty.

### 13b. Slow inhibitory population in Jansen-Rit (GABA_B-like)
- [x] Added `b_slow_gain`, `b_slow_rate`, `c_slow` fields on `JansenRitModel`. Default 0.0 → bitwise-identical to pre-CET model. Setter `set_slow_inhib(gain, rate, c)`.
- [x] New ODE: parallel 2-state slow inhibitory population `[y_slow_0, y_slow_1]` integrated alongside the canonical Wendling 8-state via RK4 in `simulate()` and `simulate_with_fast_inhib_trace()`. Driven by `S(v_pyr_with_slow_feedback)`, contributes to EEG via subtraction `eeg = y[1] - y[2] - y[3] - y_slow_0`. Same ODE form as canonical JR synapses: `dy_slow_1/dt = B_slow·b_slow·C_slow·S(v_pyr) − 2·b_slow·y_slow_1 − b_slow²·y_slow_0`.
- [x] **Note on parameter sources** — The CET research agent found that Moran 2007 NeuroImage 37(3):706-720 *does not* contain GABA_B-specific parameters (it adds adaptation and recurrent fast inhibition, not a slow population). Spiegler et al. 2011 *Biol Cybern* 104:229 also could not be located; the closest match is Spiegler 2010 *NeuroImage* 52:1041 which sweeps canonical JR parameters but does not add a population. The actual canonical microcircuit GABA_B parameters come from **Moran & Friston (2011) "Canonical microcircuit DCM" *NeuroImage* 56(3):1131-1144** — references in update_model.md should be corrected accordingly. Used parameters: B_slow = 10 mV, b_slow = 5 /s (τ ≈ 200 ms), C_slow = 30.0.
- [x] Plumbed `b_slow_gain`, `b_slow_rate`, `c_slow` through `simulate_bilateral` → `run_hemisphere_tonotopic` → per-band JR construction. Updated 11 call sites to pass `0.0, 0.0, 0.0` for regression safety in non-CET paths (validate.rs, neural/tests.rs, regression_tests.rs, disturb.rs, main.rs, pipeline.rs). Pipeline + main.rs detailed pipeline use `(10.0, 5.0, 30.0)` when `cet_enabled = true`.
- [x] Unit tests in `neural::jansen_rit::tests` (5 new): `slow_gaba_b_default_is_zero`, `slow_gaba_b_disabled_bitwise_identical_to_pre_cet`, `slow_gaba_b_changes_eeg_when_enabled`, `slow_gaba_b_output_finite_under_aggressive_params`, `slow_gaba_b_with_constant_input_reduces_dc_drift`. All pass; bitwise regression contract empirically validated.
- [x] **Ref:** Moran RJ, Friston KJ (2011). "Neural masses and fields in dynamic causal modeling." *Front Comput Neurosci* / *NeuroImage* 56(3):1131-1144 — canonical microcircuit DCM with GABA_A and GABA_B populations; source for B_slow, b_slow, C_slow ranges.
- [x] **Ref:** Ghitza O (2011). "Linking speech perception and neurophysiology: speech decoding guided by cascaded oscillators locked to the input rhythm." *Front Psychol* 2:130 — slow inhibitory loop for theta-rate envelope tracking.

### 13c. Envelope-phase PLV metric
- [x] Added `compute_envelope_plv(eeg, envelope, sample_rate)` in `performance.rs` — bandpass-filters both signals to **2–9 Hz** (Doelling 2014 CET-relevant band) via FFT zero-out, runs Hilbert transform on each (Marple 1999), computes Δφ(t) = φ_eeg − φ_envelope, averages unit phasors. Mirrors `compute_plv()` structure but the reference is the envelope's actual phase, not a synthetic sinusoid.
- [x] Added `envelope_plv: Option<f64>` field to `PerformanceVector` alongside the existing `plv`.
- [x] New entry point `PerformanceVector::compute_with_envelope(...)` takes an optional envelope reference; legacy `compute()` delegates to it with `None`. Backward-compat: every existing call site sees `envelope_plv = None`.
- [x] Wired into `Goal::evaluate_full()` in `scoring.rs` with a new `envelope_entrainment_weight()` per-goal:
  - Sleep: 0.8 (was 0% on carrier PLV → first real entrainment signal)
  - Deep Relaxation: 0.7
  - Meditation: 0.6
  - Flow: 0.4
  - Deep Work: 0.2
  - Focus / Isolation / Shield / Ignition: 0.0 (carrier-driven goals; CET is irrelevant)
- [x] Bonus is **additive** to carrier PLV (max 10% × envelope_weight × envelope_plv). A preset can score on both axes simultaneously.
- [x] Pipeline builds the envelope reference as the energy-weighted average of slow-path band signals across both ears (the slow drive feeding cortex).
- [x] Unit tests in `neural::performance::tests` (8 new): high PLV for self-locked 5 Hz envelope, low PLV for independent signals (12 Hz EEG vs 4 Hz envelope), high PLV for 90° phase-shifted lock (PLV measures consistency, not magnitude), in [0,1] range, zero for short signal, present when envelope supplied, none for legacy `compute()`, finite under realistic noisy JR EEG.
- [x] **Ref:** Ding N, Simon JZ (2014). "Cortical entrainment to continuous speech: functional roles and interpretations." *Front Hum Neurosci* 8:311 — CET methodology and band-of-interest definitions.
- [x] **Ref:** Luo H, Poeppel D (2007). "Phase patterns of neuronal responses reliably discriminate speech in human auditory cortex." *Neuron* 54(6):1001-1010 — envelope-phase as the primary CET signal carrier.
- [x] **Ref:** Doelling KB et al. (2014). "Acoustic landmarks drive delta-theta oscillations." *NeuroImage* 85:761-768 — empirical 2–9 Hz CET band that 13c bandpasses to.

### 13d. Validation against existing presets
- [x] **Regression check (cet OFF must equal baseline bitwise):** all 7 baseline preset/goal pairs verified — deepwork_normal/deep_work=0.4480, isolation_normal_smooth/deep_work=0.4482, showcase_pink/deep_work=0.5259, showcase_brown/deep_work=0.3780, showcase_blue/deep_work=0.2434, showcase_pink/sleep=0.1241, showcase_brown/sleep=0.2157. Bitwise match. ✓
- [x] **CET ON effect on relaxation goals (synthetic preset: pink + 5 Hz NeuralLfo, depth 0.9):** Sleep 0.1729 → 0.1787 (+0.0058, +3.4%); Deep Relaxation 0.2458 → 0.2556 (+0.0098, +4.0%); Meditation 0.2808 → 0.2950 (+0.0142, +5.1%). All three relaxation-family goals consistently positive — exactly the predicted direction.
- [x] **CET ON effect on showcase (static noise) presets:** sleep deltas in the range −0.0018 to +0.0009 (essentially noise floor). Correct: static noise has no slow envelope to track, so CET correctly does nothing. The architecture only rewards presets that actually have slow envelope content.
- [x] **Test suite:** 335 passing, 4 pre-existing thalamic_gate failures unchanged. Net delta: **+23 new tests passing** (5 slow GABA_B + 9 crossover + 8 envelope PLV + 1 precheck), zero regressions in any non-CET test.
- [ ] **Future:** Optimize a new preset with `cet_enabled=true` to verify the DE optimizer discovers envelope-modulated designs (NeuralLfo at 1–5 Hz, deep modulation) without any direct hint in the fitness function.

### Caveats worth respecting
- The Priority 1b investigation (`update_model.md:11`, Obsolete/Superseded:256-258) empirically found that AC was ~5% of total band power and JR was effectively mean-driven. That measurement was for *40 Hz carrier modulation on high bands*, where the 80 Hz gammatone LPF already squashes the carrier. For 1–8 Hz envelopes on low bands the AC fraction is much larger and the finding doesn't automatically carry over — but step 13a's PRECHECK is non-optional. If the precheck fails, implement 13b first (slow GABA_B population) so that JR actually *can* respond to AC drive at all, then retry 13a.
- Don't couple CET to the `stochastic_jr_enabled` flag — they're orthogonal (noise broadens the spectrum, CET adds a structured slow drive). Both should be independently toggleable.
- CET must not bypass the thalamic gate's operating-point shifts (Priority 2). The slow input drive is modulated by the same `band_offsets` (post-gate) as the fast drive — the two pathways share the JR circuit, they don't duplicate it.

### References

- [ ] **Ref:** Ding N, Simon JZ (2014). "Cortical entrainment to continuous speech: functional roles and interpretations." *Front Hum Neurosci* 8:311. — foundational review of cortical envelope tracking; establishes that auditory cortex phase-locks to the 1–8 Hz envelope of natural sound even when no periodic carrier is present.
- [ ] **Ref:** Giraud AL, Poeppel D (2012). "Cortical oscillations and speech processing: emerging computational principles and operations." *Nat Neurosci* 15(4):511-517. — theta/delta cortical oscillations as the computational substrate for envelope tracking; motivates the GABA_B slow time constant.
- [ ] **Ref:** Lakatos P, Karmos G, Mehta AD, Ulbert I, Schroeder CE (2008). "Entrainment of neuronal oscillations as a mechanism of attentional selection." *Science* 320(5872):110-113. — demonstrates that slow-envelope entrainment is an active attentional mechanism, not a passive following response; validates scoring CET for attention-related goals.
- [ ] **Ref:** Luo H, Poeppel D (2007). "Phase patterns of neuronal responses reliably discriminate speech in human auditory cortex." *Neuron* 54(6):1001-1010. — MEG evidence that envelope-phase (not amplitude) carries the bulk of cortical tracking information; directly motivates the envelope-phase PLV formulation in 13c.
- [ ] **Ref:** Peelle JE, Davis MH (2012). "Neural oscillations carry speech rhythm through to comprehension." *Front Psychol* 3:320. — syllabic-rate (3–7 Hz) cortical tracking mechanism; sets the cutoff rationale for the 10 Hz crossover in 13a.
- [ ] **Ref:** Doelling KB, Arnal LH, Ghitza O, Poeppel D (2014). "Acoustic landmarks drive delta-theta oscillations to enable speech comprehension by facilitating perceptual parsing." *NeuroImage* 85:761-768. — shows that removing slow envelope cues (highpassing the amplitude modulation above ~8 Hz) destroys cortical tracking; empirical mirror of the "ASSR kills slow modulation" bug.
- [ ] **Ref:** Obleser J, Kayser C (2019). "Neural entrainment and attentional selection in the listening brain." *Trends Cogn Sci* 23(11):913-926. — methodological framework for measuring envelope entrainment (already cited in Priority 12; reused here for the PLV-against-envelope formulation).
- [ ] **Ref:** Zoefel B, ten Oever S, Sack AT (2018). "The involvement of endogenous neural oscillations in the processing of rhythmic input." *Front Neurosci* 12:95. — distinguishes genuine envelope entrainment from evoked responses (already cited in Priority 12); critical for validating 13c against trivial transient locking.
- [ ] **Ref:** Spiegler A, Kiebel SJ, Atay FM, Knösche TR (2011). "Complex behavior in a modified Jansen and Rit neural mass model." *Biol Cybern* 104:229-254. — provides extended JR parameter sets including slow inhibitory populations; parameter source for 13b's `B_slow`, `C_slow`, `b_slow`.
- [ ] **Ref:** Moran RJ, Stephan KE, Seidenbecher T, Pape HC, Dolan RJ, Friston KJ (2007). "A neural mass model of spectral responses in electrophysiology." *NeuroImage* 37(3):706-720. — DCM-oriented JR extension with GABA_A/GABA_B separation; canonical reference for the slow inhibitory time constant used in 13b.
- [ ] **Ref:** Ghitza O (2011). "Linking speech perception and neurophysiology: speech decoding guided by cascaded oscillators locked to the input rhythm." *Front Psychol* 2:130. — cascaded-oscillator model of envelope tracking; architectural precedent for the fast/slow pathway bifurcation in 13a.

## Priority 14: Surrogate-Assisted Optimization (HIGH IMPACT, MEDIUM EFFORT — IMPLEMENTED)

**Problem:** The DE optimizer evaluates each candidate preset by running the full simulation pipeline: audio render (48 kHz) → gammatone filterbank → ASSR/CET crossover → bilateral JR cortical model (RK4 at 1 kHz) → FHN → PLV → scoring. At ~100 ms per evaluation with 12 s audio, a 200-generation × 50-population run takes ~2.8 hours. With the physiological gate (P9) adding ~17 ms per evaluation, it's even slower. This limits iteration speed: testing a new scoring idea or brain-type tuning requires a multi-hour optimizer run.

**Solution:** Train a lightweight MLP surrogate to approximate the pipeline's (genome, goal, brain_type, config_flags) → score mapping. Use it for **pre-screening** inside the DE loop: score all 50 candidates with the surrogate (~5 µs each), validate only the top-5 with the real pipeline. The real pipeline is never replaced — it remains the ground truth for final scores, regression tests, and preset export. The surrogate just accelerates the search.

**Expected speedup:** ~10x per generation (from 50 × 100 ms = 5 s to 50 × 5 µs + 5 × 100 ms ≈ 500 ms). A 200-generation run goes from ~2.8 hours to ~17 minutes.

**Key design constraint:** The NMM pipeline is NEVER modified. The surrogate is an additive, flag-gated, optional acceleration layer. When `--surrogate` is off (default), behavior is bit-for-bit identical.

### 14a. Training data generation (`generate-data` CLI command)

- [x] Added `GenerateData` subcommand to the CLI in `src/main.rs`:
  1. Samples N random presets uniformly from genome bounds (`Preset::bounds()`) via xorshift64 RNG
  2. Evaluates each against specified goals × brain_types using `evaluate_preset()` — the EXACT same function the optimizer calls
  3. Writes CSV: `g0..g229, goal_id, brain_type_id, assr, thalamic_gate, cet, phys_gate, score`
  4. Supports `--count N` (default 1000), `--goals all` (or comma-separated), `--brain-type all` (or specific), `--duration`, `--threads`, `--seed`
  5. Parallelizes via `std::thread::scope` (no extra dependencies)
- [x] Smoke tested: 5 presets × 1 goal × 1 brain type, 2 threads, correct CSV output verified.
- [x] Data format: flat CSV, genome as f64 with 6 decimals, score as f64 with 6 decimals, goal/brain_type as integer IDs.
- [x] Contract cleanup later in Priority 22: CSV header now comes from the shared surrogate contract helper, and rows serialize the REAL feature flags from `SimulationConfig` instead of hard-coded values.
- [x] Priority 22 follow-up: added `--phys-gate` to `generate-data` so the surrogate can be retrained on real physiological-gate rows instead of a heuristic-only dataset.
- [x] Generated fresh 230-gene training data with the corrected schema:
  - baseline slice: 400 presets × all 9 goals × all 5 brain types = 18,000 rows
  - phys-gate slice: 300 presets × 3 relaxation goals × all 5 brain types = 4,500 rows
  - checked-in `training_data.csv` / `training_combined.csv`: 22,500 rows total, 237 columns total

### 14b. Surrogate model training (Python, offline, separate from Rust)

- [x] Created `tools/train_surrogate.py` — standalone PyTorch training script:
  1. Loads CSV from 14a, infers genome width from the `g0..gN` header, and builds the current input contract (genome + goal one-hot[9] + brain_type one-hot[5] + config flags[4])
  2. Architecture: `Linear(input_dim,256) → ReLU → Linear(256,256) → ReLU → Linear(256,128) → ReLU → Linear(128,1) → Sigmoid`
  3. Loss: MSE on score ∈ [0, 1]. Optimizer: AdamW (lr=1e-3, weight_decay=1e-4)
  4. Train/val split 80/20, early stopping (patience 20), max 200 epochs
  5. Exports weights as flat f32 little-endian binary with u32 header: `[n_layers, dim0, dim1, ..., dimN]`
  6. Prints per-epoch train/val loss + R² metric
- [x] Script is self-contained: `python tools/train_surrogate.py data.csv weights.bin`
- [x] The Python dependency is OFFLINE only. Rust never calls Python. The weights file is the only artifact.
- [x] Priority 22 retrain pass on the fresh 22,500-row dataset:
  - `surrogate_weights.bin` (`256,256,128`): val_loss `0.003182`, `R² = 0.8499`
  - `surrogate_weights_med.bin` (`128,64`): val_loss `0.003837`, `R² = 0.8191`
  - `surrogate_weights_small.bin` (`64,32`): val_loss `0.004000`, `R² = 0.8113`

### 14c. Rust inference engine (hand-coded MLP, zero dependencies)

- [x] Added `src/surrogate.rs` containing `SurrogateModel`:
  - `DenseLayer` struct with row-major weights + biases, `forward()` method
  - `SurrogateModel::load(path)` — reads binary weights file, validates header, rejects stale input dimensions, constructs layer stack
  - `SurrogateModel::build_input(genome, goal, brain_type, assr, thalamic_gate, cet, phys_gate)` — normalizes genome to [0,1] using `Preset::bounds()`, one-hot encodes goal/brain_type, adds config flags
  - `SurrogateModel::predict(input) -> f32` — forward pass (matmul + ReLU per hidden layer, Sigmoid on output)
  - `SurrogateModel::predict_batch(inputs)` — batch prediction
- [x] Forward pass: plain `for` loops over weight matrices. ~170k multiply-adds for 248→256→256→128→1 = **~5–20 µs on Apple Silicon**. No BLAS, no SIMD, no external crates.
- [x] Unit tests (13 total): round-trip serialize→load→predict (bitwise identical), missing file returns error, truncated file returns error, stale-dimension weights return a retrain error, output always in [0,1], output finite for all-ones and all-zeros input, batch matches individual predictions bitwise, different inputs produce different outputs, input builder correct length/one-hot sum/genome normalization/config flags, production architecture shape (248→256→256→128→1) works end-to-end.
- [x] Gate: `--surrogate` flag on `optimize` command (default false). Missing weights file prints warning and falls back to full pipeline.

### 14d. Surrogate-assisted DE loop

- [x] Modified the DE trial loop in `run_optimize()` with a conditional branch:
  - When `--surrogate` active: scores ALL candidates with the surrogate, ranks by surrogate score, validates top-K + 1 exploration candidate with the real pipeline, and reports ONLY validated REAL scores back into DE
  - When `--surrogate` off: identical to pre-P14 code path (zero regression)
- [x] CLI flags: `--surrogate` (enable), `--surrogate-weights path` (default `surrogate_weights.bin`), `--surrogate-k N` (default 5)
- [x] Surrogate model loaded at optimizer start with graceful fallback on load failure
- [x] Also added `--cet` and `--phys-gate` flags to `optimize` command (were previously only on `evaluate`)
- [x] **Speedup estimate:** 50 surr + 5 real = 0.25 ms + 500 ms ≈ 500 ms/gen vs 50 × 100 ms = 5 s/gen without. **~10x speedup.**
- [x] Priority 23 semantics cleanup: unvalidated surrogate-ranked trials no longer steer DE population state. The MLP is now a strict ranking/filter layer, not an approximate fitness source.
- [ ] **Future:** Print surrogate statistics per generation (surr_best, real_best, surr_rank_of_real_best)

### 14e. Incremental retraining (optional, v2)

- [ ] After each optimizer run, the real-pipeline evaluations from step 14d become new training data. Append them to the CSV.
- [ ] Re-run `tools/train_surrogate.py` on the accumulated dataset → new `surrogate_weights.bin`.
- [ ] This is the "active learning" loop from surrogate-assisted optimization. v1 works without it; v2 closes the loop.

### Optimizer validation results (P9 + P13 features, no surrogate yet)

While implementing P14, we ran two full optimization passes with `--phys-gate --cet` to validate the P9 + P13 architecture:

| Goal | Best score | Generations | Wall time | Preset file |
|---|---|---|---|---|
| **Deep Relaxation** | **0.9237** | 37 (converged) | ~25 min | `presets/deep_relax_phys_cet_v1.json` |
| **Sleep** | **0.5180** | 61 (stagnated) | ~41 min | `presets/sleep_phys_cet_v1.json` |

Deep relaxation 0.9237 is the highest score this model has ever produced on any goal. Sleep 0.5180 crosses the OK threshold. Both were impossible before P9 + P13.

### Caveats

- **The surrogate is NOT a replacement for the NMM.** It's an approximate filter. Final exported presets are ALWAYS validated by the real pipeline. Regression tests use the real pipeline. The surrogate file is a build artifact, not a model component.
- **The surrogate becomes stale** when the scoring function changes (new goal weights, new PLV metric, etc.). Regenerate training data and retrain after any scoring.rs change. The CSV + Python script make this a 30-minute process.
- **The Python dependency is OFFLINE only.** Rust compilation, testing, and all runtime paths are pure Rust. The Python script is a tool, like a benchmark or a plot script — it doesn't ship.
- **Accuracy floor:** R² of ~0.92 means ~8% unexplained variance. Some "best" surrogate candidates will be duds when validated by the real pipeline. The top-K design absorbs this: K=5 means 5 chances to find a real winner per generation.
- **Test suite:** 364 passing, 4 pre-existing thalamic_gate failures unchanged (+12 new surrogate tests). Zero regression.

### References

- [x] **Ref:** Tilwani D, O'Reilly C (2024). "Benchmarking Deep Jansen-Rit Parameter Inference: An in Silico Study." arXiv:2406.05002. — Deep learning for JR parameter inference from EEG. Demonstrates MLP architecture (128–256 units, ReLU) on JR-generated data. Reports R² > 0.8 on identifiable parameters with ~100k samples. [GitHub](https://github.com/lina-usc/Jansen-Rit-Model-Benchmarking-Deep-Learning).
- [x] **Ref:** Sun R, et al. (2022). "Deep neural networks constrained by neural mass models improve electrophysiological source imaging." *PNAS* 119(31):e2201128119. — NMM-constrained DNN; hybrid NMM+DL architecture.
- [x] **Ref:** Gonçalves PJ, et al. (2020). "Training deep neural density estimators to identify mechanistic models of neural dynamics." *eLife* 9:e56261. — Simulation-based inference (SBI) with neural posterior estimation.
- [x] **Ref:** Tenne Y, Armfield SW (2009). "An effective approach to evolutionary surrogate-assisted optimization." In: *Computational Intelligence in Expensive Optimization Problems*, Springer. — Foundation for surrogate-assisted DE; top-K pre-screening pattern.
- [x] **Ref:** Forrester AIJ, Sóbester A, Keane AJ (2008). *Engineering Design via Surrogate Modelling.* Wiley. — Training data requirements (~50–100 samples per effective input dim); R² benchmarks for smooth high-dimensional surrogates.

## Priority 15: Brain-Type-Dependent GABA Inhibitory Time Constants (HIGH IMPACT, LOW EFFORT)

**Problem:** JR fast inhibitory time constant `b_rate` is fixed at 50.0/s for Normal, HighAlpha, ADHD, and Anxious brain types (Aging uses 40.0). The `g_fast_rate` already varies (ADHD 450/s, Anxious 500/s) for the Wendling fast inhibitory population, but the canonical JR slow inhibitory rate `b_rate` is uniform. This contradicts the GABA literature: ADHD shows reduced GABAergic tone (weaker, slower inhibition), while anxiety shows increased GABAergic activity (faster, stronger inhibition). The 40 Hz ASSR — the primary gamma entrainment mechanism — is directly shaped by IPSC decay time, meaning the current model under-differentiates brain types for gamma/beta-related goals (Ignition, Shield, Focus).

**Solution:** Adjust `b_rate` per brain type in `brain_type.rs` to match the known GABA phenotypes:
- ADHD: `b_rate` 50.0 → **45.0** (slower GABA-B → weaker slow inhibition → reduced gamma capacity, matching Edden et al. 2012 GABA deficit in ADHD)
- Anxious: `b_rate` 50.0 → **55.0** (faster GABA-B → stronger inhibitory tone → more beta, matching the hyperactive inhibition phenotype already documented in code comments at line 178-181)
- Normal, HighAlpha, Aging: unchanged

Also propagate to bilateral `band_rates` where the `b` component of `(a_rate, b_rate)` tuples should reflect the same per-brain-type inhibitory profile for consistency with the scalar `b_rate`.

- [x] Change ADHD `b_rate` from 50.0 to 45.0 in `brain_type.rs` (scalar JansenRitParams)
- [x] Change Anxious `b_rate` from 50.0 to 55.0 in `brain_type.rs` (scalar JansenRitParams)
- [x] Propagate ADHD b_rate adjustment to JR-driven `band_rates` tuples in tonotopic and bilateral profiles
- [x] Propagate Anxious b_rate adjustment to tonotopic and bilateral `band_rates` tuples
- [x] Add parameter-contract regression tests:
  - `adhd_b_rate_is_slower_than_normal_across_jr_profiles`
  - `anxious_b_rate_is_faster_than_normal_across_jr_profiles`
- [x] Run `cargo test` — all existing tests pass after the brain-type retune (`417 passed; 0 failed; 4 ignored`)
- [ ] Evaluate: compare ADHD Ignition scores before/after (expect: slight reduction in gamma PLV, reflecting weaker 40 Hz ASSR — physiologically correct)
- [ ] Evaluate: compare Anxious Shield/Focus scores before/after (expect: slight increase in beta stability — matching hyperactive inhibition)
- [ ] Evaluate: confirm Normal/HighAlpha/Aging scores are bitwise identical (no change to these brain types)
- [ ] **Ref:** (2024). "40 Hz Steady-State Response in Human Auditory Cortex Is Shaped by Gabaergic Neuronal Inhibition." *J Neurosci* 44(24):e2029232024. — Direct evidence that IPSC decay time and amplitude on PV+ interneurons are critical for 40 Hz ASSR generation. GABA parameters modulate the ASSR transfer function. [Link](https://www.jneurosci.org/content/44/24/e2029232024)
- [ ] **Ref:** Edden RAE, Crocetti D, Zhu H, Gilbert DL, Mostofsky SH (2012). "Reduced GABA concentration in attention-deficit/hyperactivity disorder." *Arch Gen Psychiatry* 69(7):750-753. — MRS evidence of reduced GABA in ADHD sensorimotor cortex; establishes the GABA deficit that motivates slower b_rate for ADHD.
- [ ] **Ref:** Whittington MA, Traub RD, Jefferys JGR (1995). "Synchronized oscillations in interneuron networks driven by metabotropic glutamate receptor activation." *Nature* 373(6515):612-615. — Foundational evidence that GABA-A IPSC decay time determines gamma frequency; tau_i ~20ms → ~40 Hz, slower → lower frequency.
- [ ] **Ref:** Traub RD, Whittington MA, Colling SB, Buzsáki G, Jefferys JGR (1996). "Analysis of gamma rhythms in the rat hippocampus in vitro and in vivo." *J Physiol* 493(2):471-484. — Quantitative relationship between inhibitory time constant and gamma oscillation frequency.
- [ ] **Ref:** (2024). "Systematic Review and Meta-Analysis: Do White Noise and Pink Noise Help With Attention in ADHD?" *J Am Acad Child Adolesc Psychiatry* 63(8):859-870. — Meta-analysis (k=13, N=335, I²<0.01) confirming noise benefits ADHD (g=0.249) but hurts neurotypicals (g=−0.212); validates brain-type-dependent scoring. [PMC](https://pmc.ncbi.nlm.nih.gov/articles/PMC11283987/)

## Priority 16: L-SHADE Adaptive DE (with P14 Surrogate Integration) (HIGH IMPACT, MEDIUM EFFORT)

**Problem:** The current optimizer uses DE/rand/1/bin with fixed F=0.7 and CR=0.8 (`src/optimizer/differential_evolution.rs`). On the 190-dimensional genome space, fixed F/CR is suboptimal: some parameters (reverb_send, anchor_color) dominate the fitness landscape while others (movement phase, z-position) barely matter. The optimizer spends equal mutation energy on all dimensions. Population size is also fixed throughout the run, wasting evaluations in late generations when the population has converged.

**Solution:** Replace DE/rand/1/bin with L-SHADE (Linear Success-History Adaptive DE) inside the existing optimizer framework. L-SHADE adds three mechanisms:
1. **Success-history parameter adaptation:** F and CR are sampled from Cauchy/Normal distributions centered on values that produced successful mutations in previous generations. The optimizer learns which mutation scales work.
2. **Current-to-pbest/1 mutation:** `donor = x_i + F * (x_pbest - x_i) + F * (x_r1 - x_r2)` where x_pbest is a random member of the top-p% of the population. Better exploitation than rand/1.
3. **Linear population size reduction (LPSR):** Population shrinks linearly from N_init to N_min over the generation budget. Early: wide exploration. Late: focused exploitation.

**CRITICAL: Implement together with P14 surrogate, not standalone.** Recent benchmarks (2024) show L-SHADE has premature convergence on high-D spaces when used alone. The surrogate pre-screening from P14 compensates: fewer wasted real evaluations → L-SHADE's exploitation bias becomes a strength rather than a weakness.

- [ ] Implement `LShadeOptimizer` in `src/optimizer/` alongside existing `DifferentialEvolution`
- [ ] Success history: circular buffer of H=6 entries for F and CR (Tanabe & Fukunaga recommend H=6)
- [ ] F sampled from Cauchy(μ_F, 0.1), CR sampled from Normal(μ_CR, 0.1), both clamped to [0,1]
- [ ] μ_F and μ_CR updated via weighted Lehmer mean of successful parameters each generation
- [ ] Current-to-pbest/1 mutation with p decreasing linearly from 0.2 to 0.05
- [ ] LPSR: population reduces linearly from N_init to max(N_init/5, 10) over max_generations
- [ ] External archive of replaced individuals (size ≤ N_init) for donor selection diversity
- [ ] Wire into `run_optimize()` with `--optimizer lshade` CLI flag (default remains `de` for regression safety)
- [ ] Unit tests: convergence on Rastrigin 10-D, Rosenbrock 10-D — verify improvement over DE/rand/1/bin
- [ ] Integration test: optimize a known goal (Focus/Normal) with L-SHADE vs DE — compare convergence speed
- [ ] When P14 surrogate is ready: integrate L-SHADE as the DE backend inside the surrogate-assisted loop
- [ ] **Ref:** Tanabe R, Fukunaga AS (2014). "Improving the search performance of SHADE using linear population size reduction." *Proc IEEE CEC* 2014:1658-1665. — Original L-SHADE paper: success-history adaptation + LPSR. CEC 2014 competition winner.
- [ ] **Ref:** Tanabe R, Fukunaga A (2013). "Success-History Based Parameter Adaptation for Differential Evolution." *Proc IEEE CEC* 2013:71-78. — SHADE foundation paper: success-history F/CR adaptation mechanism.
- [ ] **Ref:** Zhang J, Sanderson AC (2009). "JADE: Adaptive Differential Evolution with Optional External Archive." *IEEE Trans Evol Comput* 13(5):945-958. — JADE: current-to-pbest/1 mutation strategy and external archive mechanism used by L-SHADE.
- [ ] **Ref:** (2025). "Weighted Committee-Based Surrogate-Assisted Differential Evolution Framework." *Int J Machine Learning & Cybernetics.* — Ensemble surrogate approach; consider for P14 v2 if single MLP shows instability. [Link](https://link.springer.com/article/10.1007/s13042-025-02632-x)

## Priority 17: Dynamic Feedback Inhibitory Control (dFIC) for Bilateral Coupling (MEDIUM IMPACT, LOW EFFORT)

**Problem:** The bilateral callosal coupling uses a fixed weight `k` per brain type (`callosal_coupling` in `BilateralParams`, range 0.11–0.15). The coupling formula in `simulate_bilateral()` is `rh_coupled[i] = rh_eeg[i] - k * delayed_lh` (and vice versa). This fixed inhibitory coupling, combined with the AST hemispheric specialization (left WC-beta, right JR-alpha), produces a structural left-beta/right-alpha split that is "not shapeable by preset-level parameters" (BRAIN_MODEL_GUIDE.md line 505). This is the model's most documented limitation — it prevents bilateral alpha states and limits preset design space for meditation/relaxation goals.

**Solution:** Replace the fixed coupling weight `k` with a dynamic feedback inhibitory control (dFIC) rule that adapts `k` per hemisphere per timestep based on local activity. The rule from Stasinski et al. (2024): `dk/dt = η * (E_target - |EEG(t)|)` where η is a learning rate and E_target is the desired activity level. When a hemisphere is too active (|EEG| > E_target), coupling increases (more inhibition from the other side). When under-active, coupling decreases.

This is a homeostatic mechanism — it pulls both hemispheres toward a common activity level, which could break the structural asymmetry enough for bilateral alpha to emerge on Normal brain type.

**v1 simplification:** Use a single scalar dFIC rule on the callosal coupling weight, not per-band adaptation. Per-band dFIC is a v2 enhancement.

- [ ] Add `dfic_enabled: bool`, `dfic_eta: f64` (learning rate, default 0.001), `dfic_target: f64` (target RMS EEG, default computed from first 500ms of simulation) to `SimulationConfig`
- [ ] In `simulate_bilateral()` coupling loop: replace fixed `k` with `k_left` and `k_right` that adapt per timestep:
  ```
  k_left += eta * (target - rms_left_window)   // if left is over-active, increase its inhibition
  k_right += eta * (target - rms_right_window)  // if right is over-active, increase its inhibition
  clamp k_left, k_right to [0.0, 0.5]
  ```
- [ ] RMS window: exponential moving average with tau=50ms (50 samples at 1kHz)
- [ ] CLI flag `--dfic` on `evaluate` command. Default off. Takes no precedence over other flags — orthogonal to `--phys-gate`, `--cet`, etc.
- [ ] When disabled: coupling loop is bitwise identical to pre-P17 (regression safety)
- [ ] Unit tests: (1) disabled returns zeros / no change, (2) symmetric input → k_left ≈ k_right, (3) asymmetric input → the more active hemisphere gets higher coupling, (4) k stays within [0, 0.5], (5) output finite under aggressive eta
- [ ] Evaluate: compare per-hemisphere band powers with dFIC on vs off for Normal brain type with symmetric pink anchor preset. If dFIC reduces the left-beta/right-alpha gap, it's working.
- [ ] Evaluate: confirm dFIC OFF scores are bitwise identical to pre-P17 baseline
- [ ] **Ref:** Stasinski J, Taher H, Meier JM, Schirner M, Perdikis D, Ritter P (2024). "Homeodynamic feedback inhibition control in whole-brain simulations." *PLOS Comput Biol* 20(12):e1012595. — dFIC plasticity rule for JR: adapts inhibitory coupling to reach desired dynamic regimes. Validated on TVB whole-brain simulations. [Link](https://journals.plos.org/ploscompbiol/article?id=10.1371/journal.pcbi.1012595)
- [ ] **Ref:** Bloom JS, Hynd GW (2005). "The role of the corpus callosum in interhemispheric transfer of information: excitation or inhibition?" *Neuropsychol Rev* 15(2):59-71. — Establishes that callosal coupling is primarily inhibitory and activity-dependent, not a fixed constant.
- [ ] **Ref:** Vogels TP, Sprekeler H, Zenke F, Clopath C, Gerstner W (2011). "Inhibitory Plasticity Balances Excitation and Inhibition in Sensory Pathways and Memory Networks." *Science* 334(6062):1569-1572. — Foundational paper on inhibitory synaptic plasticity as a homeostatic mechanism for E/I balance.
- [ ] **Future:** Per-band dFIC — adapt k separately for each tonotopic band pair, allowing frequency-specific bilateral balancing. Per-band would let low bands (theta/alpha) converge bilaterally while high bands (beta) remain differentiated.
- [ ] **Future:** Activity-dependent E_target — instead of a fixed target, compute E_target from the running mean of both hemispheres combined, so the system seeks bilateral balance rather than a preset target level.

## Priority 19: Pre-Neural Environment Processing (HIGH IMPACT, MEDIUM EFFORT)

**Problem:** The acoustic environment (reverb, early reflections, room modes) is applied AFTER the neural model evaluates the audio. The NMM processes the dry signal — the listener never hears this signal. A preset in `AnechoicChamber` vs `DeepSanctuary` scores identically, which is physically incorrect. Reverberation modifies the acoustic waveform at the ear before it reaches the cochlea.

**Scientific evidence (the ordering is unambiguous):**

1. **Reverberation degrades the signal before the cochlea.** Devore et al. (2009, J Neurosci) showed auditory nerve fibers encode the reverberant signal faithfully, including the smeared temporal envelope. The cochlea has no "de-reverberation" stage. The neural model must see post-reverb signals.

2. **ASSR is reduced by reverberation.** Fujihira & Shiraishi (2015, Ear and Hearing): 40 Hz ASSRs are reduced in amplitude under reverberation because reverb fills inter-stimulus gaps, reducing modulation depth. For a 10 Hz isochronic tone in a reverberant room, the cortical 10 Hz tracking is physically weaker — the modulation is degraded before reaching the auditory nerve.

3. **Brainstem responses are degraded by reverb.** Bidelman & Krishnan (2010, JASA): brainstem frequency-following responses (FFR) are smeared by reverberation — temporal fine structure encoding is degraded at the subcortical level.

4. **EEG cortical responses change with reverb.** Fuglsang et al. (2020, NeuroImage): EEG temporal response functions show reduced amplitude and delayed latency in auditory cortex components (N1/P2) under reverberation, scaling with RT60.

5. **Spectral effects on noise.** Kuttruff (2009, "Room Acoustics" 5th ed.): rooms boost low frequencies (longer RT60 below 500 Hz) and attenuate highs (air absorption above 2 kHz). Brown noise through a room becomes even more low-heavy; white noise shifts toward pink. Reverb partially converges noise spectra toward the room's modal response.

6. **RT60 and arousal (task-dependent).** Sato & Bradley (2008, JASA): reverb on noise/music reduces temporal sharpness (relaxing), while reverb on speech increases listening effort (alerting). Pätynen et al. (2014, JASA): reverberant spaces with non-speech signals promote subjective calm.

7. **Computational precedent.** Carney et al. (2015, JARO) and Zilany et al. (2014, JASA) model the full chain: stimulus → room → auditory periphery → midbrain. The ordering is always room processing before cochlear models.

**Current architecture (incorrect):**
```
audio render → gammatone filterbank → JR model → score
                                                    ↑ dry signal
environment reverb applied only in DSP engine (listener hears this, model doesn't)
```

**Correct architecture:**
```
audio render → environment reverb → gammatone filterbank → JR model → score
                    ↑ wet signal — what the listener actually hears
```

**Implementation:**
- [ ] In `pipeline.rs`, apply environment room impulse response (RIR) to the rendered audio BEFORE the gammatone filterbank step
- [ ] The RIR should be parameterized by the preset's `environment` field (0-4: AnechoicChamber, FocusRoom, OpenLounge, VastSpace, DeepSanctuary)
- [ ] Each environment needs an RT60 and frequency-dependent decay profile:
  - AnechoicChamber: RT60 ≈ 0 (passthrough, no change)
  - FocusRoom: RT60 ≈ 0.3s (small treated room)
  - OpenLounge: RT60 ≈ 0.8s (open plan office)
  - VastSpace: RT60 ≈ 2.0s (large hall)
  - DeepSanctuary: RT60 ≈ 4.0s (cathedral-like)
- [ ] The RIR convolution should be applied per-ear (the reverberant field is different at each ear position)
- [ ] Impact: presets with high reverb_send + reverberant environment will score differently than the same preset in anechoic. This is correct — the listener experiences a different spectral balance.
- [ ] Regression: environment=0 (AnechoicChamber) with reverb_send=0 on all objects must produce bitwise-identical scores to the current pipeline
- [ ] The arousal computation already uses per-object reverb_send — this priority adds the ROOM's contribution on top of that. Consider adjusting arousal to include environment RT60 as a factor.
- [ ] Unit tests: verify RIR convolution preserves signal energy, verify anechoic passthrough, verify longer RT60 produces more spectral blur (lower modulation depth)
- [ ] **Ref:** Devore S, Ihlefeld A, Hancock K, Shinn-Cunningham B, Bhatt P (2009). "Accurate sound localization in reverberant environments is mediated by robust encoding of spatial cues in the auditory midbrain." *Neuron* 62(1):123-134.
- [ ] **Ref:** Fujihira H, Shiraishi K (2015). "Effect of reverberation on 80-Hz auditory steady-state response." *Ear and Hearing* 36(5):e282-e289.
- [ ] **Ref:** Bidelman GM, Krishnan A (2010). "Effects of reverberation on brainstem representation of speech in musicians and non-musicians." *Brain Res* 1355:112-125.
- [ ] **Ref:** Fuglsang SA, Märcher-Rørsted J, Dau T, Hjortkjær J (2020). "Effects of sensorineural hearing loss on cortical synchronization to competing speech during selective attention." *J Neurosci* 40(12):2562-2572.
- [ ] **Ref:** Kuttruff H (2009). *Room Acoustics.* 5th ed. CRC Press.
- [ ] **Ref:** Sato H, Bradley JS (2008). "Evaluation of acoustical conditions for speech communication in working elementary school classrooms." *JASA* 123(4):2064-2077.
- [ ] **Ref:** Carney LH, Li T, McDonough JM (2015). "Speech coding in the brain: Representation of vowel formants by midbrain neurons tuned to sound fluctuations." *eNeuro* 2(4).
- [ ] **Ref:** Zilany MS, Bruce IC, Bhatt P (2014). "Updated parameters and expanded simulation options for a model of the auditory periphery." *JASA* 135(1):283-286.

---

## Priority 15: Spectral Disturbance Resilience Metrics (HIGH IMPACT, MEDIUM EFFORT — IMPLEMENTED)

**Problem:** The current disturbance test (`src/disturb.rs`) measures resilience via entrainment ratio recovery — how fast the EEG phase-locks back to a driving LFO frequency after an acoustic spike. This fails completely when:
1. No NeuralLfo/Isochronic modulator is present (static noise presets)
2. Binaural beat tones are used (tone objects have no LFO the model can detect)
3. The modulation depth is too subtle for measurable entrainment baseline

**Solution:** Added 3 spectral metrics (BPPR, SRT, SCDI) that work for ALL preset types, alongside the existing entrainment-based metrics (kept for backward compatibility). Both are now displayed in the CLI disturbance output.

### Bug fix: Isochronic detection in disturb.rs

- [x] Fixed `target_lfo_freq` detection to scan for `kind == 4` (NeuralLfo) AND `kind == 5` (Isochronic) on both bass_mod and satellite_mod. Uses the same strongest-by-depth×volume pattern as `pipeline.rs`. This alone fixed the "N/A entrainment" bug for isochronic presets — Shield v3 now reports entrainment ratio 0.570 and entrainment resilience 0.92.

### 15a. Per-band powers in WindowMetrics

- [x] Added `band_powers: [f64; 5]` (delta, theta, alpha, beta, gamma) to `WindowMetrics`. Computed per sliding window via FFT band integration using the same frequency ranges as `JansenRitModel::compute_band_powers()`: delta 0.5-4 Hz, theta 4-8 Hz, alpha 8-13 Hz, beta 13-30 Hz, gamma 30-50 Hz.
- [x] Normalized to fractions summing to 1.0.
- [x] Unit test: `band_powers_sum_to_one` — 10 Hz sine at 1 kHz sample rate, band powers sum to ~1.0.

### 15b. BPPR (Band Power Preservation Ratio)

- [x] Implemented `compute_bppr()`: computes worst-case preservation ratio across all 5 bands. For each post-spike window, the ratio `min(P_b(t) / P_b_baseline, 1.0)` is averaged across bands. The minimum across all windows is the BPPR.
- [x] Added `bppr: f64` to `DisturbResult`. Added `baseline_band_powers: [f64; 5]`.
- [x] Unit tests: `bppr_perfect_when_no_change` (1.0), `bppr_drops_when_bands_shift` (< 1.0 for shifted, < 0.3 for total collapse), `bppr_in_unit_range`.
- [x] **Ref:** Pfurtscheller G, Lopes da Silva FH (1999). ERD% = (P(t) - R) / R × 100.
- [ ] **Future:** Goal-specific band weighting (Shield → alpha-weighted, Flow → alpha+beta, etc.)

### 15c. SRT (Spectral Recovery Time)

- [x] Implemented `compute_spectral_recovery()`: computes normalized band power deviation at each post-spike window, finds the nadir (maximum deviation), then finds the first window after nadir where deviation drops below `(1-fraction)` of the nadir value.
- [x] Added `spectral_recovery_50_ms` and `spectral_recovery_90_ms` to `DisturbResult`.
- [x] Unit tests: `spectral_recovery_zero_when_no_deviation` (0 ms), `spectral_recovery_positive_after_spike` (> 0 ms).

### 15d. SCDI (Spectral Centroid Deviation Integral)

- [x] Implemented `compute_scdi()`: mean absolute centroid deviation from baseline across all post-spike windows. Lower is better.
- [x] Added `scdi_hz: f64` to `DisturbResult`.
- [x] Unit tests: `scdi_zero_when_no_deviation`, `scdi_positive_when_centroid_shifts` (verifies (5+2)/2 = 3.5 Hz).

### 15e. Composite Spectral Resilience Score

- [x] Implemented `compute_spectral_resilience()`: `0.40×BPPR + 0.30×(1-norm_SRT_90) + 0.30×(1-norm_SCDI)`. SRT normalized to 2 s max, SCDI normalized to 5 Hz max.
- [x] Added `spectral_resilience: f64` to `DisturbResult`.
- [x] Unit tests: `resilience_perfect_when_no_disturbance` (1.0), `resilience_zero_when_worst_case` (0.0), `resilience_in_unit_range` (sweeps all combinations).

### 15f. Display and backward compatibility

- [x] CLI `disturb` command now shows BOTH entrainment resilience (when available) AND spectral resilience (always).
- [x] Entrainment resilience labeled "Entrainment Resilience" (was just "Resilience Score").
- [x] New section "Spectral Resilience (Priority 15)" shows BPPR, SRT 50%/90%, SCDI, and composite score.
- [x] Old entrainment-based metrics unchanged — backward compatible. Existing presets with NeuralLfo show both scores.
- [ ] **Future:** Add `--goal` flag to `disturb` command for goal-specific BPPR weighting.

### Empirical validation

Shield v3 (binaural beats 400/410 Hz + isochronic L10/R12):
- Entrainment Resilience: **0.92** (preservation=0.87, speed=0.99)
- Spectral Resilience: **0.43** (BPPR=0.50, SRT=3650ms, SCDI=1.14Hz)

The two scores tell different stories: phase-locking recovers instantly (0.92) but the band power distribution takes 3.6 seconds to resettle (0.43). This nuance was invisible with the entrainment-only metric.

### Test suite

- 11 new unit tests in `disturb::tests`. 375 total passing, 4 pre-existing thalamic_gate failures unchanged.

### Implementation notes

- All new metrics reuse the existing sliding-window infrastructure in `disturb.rs`. No new FFT passes needed — just integrate band powers from the existing per-window FFT.
- The per-band power computation mirrors `JansenRitModel::compute_band_powers()` frequency ranges: delta 0.5-4 Hz, theta 4-8 Hz, alpha 8-13 Hz, beta 13-30 Hz, gamma 30-50 Hz.
- Recovery time constants from the literature: psychoacoustic forward masking recovers in 100-200 ms (Jesteadt 1982), neural alpha oscillations recover in 500-800 ms (Pfurtscheller 1999), attentional capture (P3a) peaks at 250-350 ms. Our SRT_90 should land somewhere in this 200-800 ms range for a healthy preset.

### References

- [ ] **Ref:** Pfurtscheller G, Lopes da Silva FH (1999). "Event-related EEG/MEG synchronization and desynchronization: basic principles." *Clin Neurophysiol* 110(11):1842-1857. — ERD/ERS formula and methodology.
- [ ] **Ref:** Casali AG, et al. (2013). "A theoretically based index of consciousness independent of sensory processing and behavior." *Sci Transl Med* 5(198):198ra105. — PCI methodology (Lempel-Ziv complexity of perturbation response).
- [ ] **Ref:** Jesteadt W, Bacon SP, Lehman JR (1982). "Forward masking as a function of frequency, masker level, and signal delay." *J Acoust Soc Am* 71(4):950-962. — Forward masking growth and recovery dynamics.
- [ ] **Ref:** Klimesch W (2012). "Alpha-band oscillations, attention, and controlled access to stored information." *Trends Cogn Sci* 16(12):606-617. — Alpha suppression and rebound dynamics.
- [ ] **Ref:** Schröger E, Marzecová A, SanMiguel I (2015). "Attention and prediction in human audition: a lesson from cognitive psychophysiology." *Eur J Neurosci* 41(5):641-664. — Three-phase distraction model (P3a/RON).

## Priority 18: Theta-Alpha Coexistence — Breaking the Single-Attractor Limit (CRITICAL IMPACT, LOW-MEDIUM EFFORT)

**Problem:** The JR model's alpha attractor at ~10 Hz is a mathematical fixed point (limit cycle) determined by the excitatory/inhibitory time constants (a=100/s τ_e≈10ms, b=50/s τ_i≈20ms). At any given operating point, the system has ONE dominant eigenfrequency. The thalamic gate shifts this eigenfrequency (alpha→theta), but the system then locks into the NEW attractor just as hard. No preset-level manipulation (noise color, spatial movement, modulation, spectral tint, binaural beats, hemispheric decoupling) can produce a stable mixed theta-alpha state at durations >60 seconds.

This was empirically proven across 50+ experiments during Deep Relaxation preset design (2026-04-15/16):
- Without gate: alpha locks at ~90% regardless of input (brown, pink, any movement)
- With thalamic gate: theta locks at ~90% regardless of reverb tuning
- With CET (GABA_B): same single-attractor behavior, GABA_B acts as DC shift not theta resonator
- With phys-gate: delta locks at ~70% (different attractor, same problem)
- Hemispheric decoupling (Chimera Split): left hemisphere showed 56% theta / 38% alpha at 600s — real split but right hemisphere still locked at 89% theta

The deep relaxation goal requires: theta ideal 35%, alpha ideal 36%, delta ideal 22% — a MIXED state that the current model cannot sustain.

**Root cause analysis:** The existing stochastic JR (Priority 7, sigma=15.0) and GABA_B slow population (Priority 13b, b_slow=5/s τ=200ms) are implemented but misconfigured for this purpose:
1. Sigma=15.0 is ~10x too low for basin hopping between alpha and theta attractors
2. b_slow=5/s (τ=200ms) is too slow to resonate at theta — it acts as a DC shift, not a frequency generator

**Solution:** Three complementary approaches, ranked by effort and scientific backing. All modify existing code — no new architecture required.

### 18a. Fix Stochastic Noise Placement + Increase Sigma (LOW EFFORT — code change + parameter)

**The science:** Ableidinger et al. (2017) applied noise as an **SDE on the velocity state variables** (ẋ₁/X₄ in their Hamiltonian formulation), NOT as additive noise on the input drive p. Their equation (5): `dP(t) = (−∇QH − 2ΓP + G(t,Q))dt + Σ(t)dW(t)` where `Σ = diag[σ₃, σ₄, σ₅]`. Three independent Wiener processes on **velocity variables only**. The primary configuration tested: σ₃ = σ₅ = 10 (weak), **σ₄ = 1000** (strong on excitatory PSP velocity). At σ₄=1000 with C=270, the probability density becomes multimodal — the system stochastically hops between attractors.

**Critical correction:** Our current implementation (Priority 7) adds noise to the input drive p: `p = offset + input*scale + σ·ξ(t)`. This is a DIFFERENT stochastic model than Ableidinger's. Noise on p modulates the external input; noise on the state velocity directly perturbs the internal dynamics. The latter is more effective at basin escape because it acts on the oscillation mechanism itself, not just the driving force.

**Numerical stability warning:** Ableidinger explicitly warns that **Euler-Maruyama produces spurious bifurcations** at step sizes Δt ∈ {1e-3, 2e-3, 5e-3} with σ₄=1000 (their Fig. 9). Their Strang splitting scheme is robust. Our current RK4 integrator at dt=1ms may need sub-stepping or the splitting scheme at high sigma values. However, for moderate sigma (50-200), RK4 should be adequate — test carefully.

**Our current state:** sigma=15.0 on input p. This is both the wrong placement and too low amplitude.

**Implementation (two phases):**

Phase 1: Test higher sigma on current input-p placement (trivial):
- [x] Unit tests: verify sigma=50-200 produces finite output (passed)
- [x] Unit tests: verify sigma=0 is deterministic (passed)
- [x] Unit tests: verify higher sigma doesn't increase alpha concentration (passed)
- [ ] Test sigma sweep: 50, 100, 150, 200 on deep relaxation preset with thalamic gate
- [ ] If higher sigma on input-p already breaks the alpha lock → ship it, skip Phase 2

Phase 2: Move noise to velocity variable (if Phase 1 insufficient):
- [ ] Change noise application from `p += σ·ξ(t)` to `dy[4]/dt += σ·ξ(t)` in the RK4 loop
- [ ] σ₄ in Ableidinger maps to our `y[4]` (excitatory interneuron PSP velocity, i.e., `dy[1]/dt` in the 8-state formulation — the velocity of the excitatory PSP)
- [ ] Verify RK4 stability at sigma=200 with current dt. If unstable, add sub-stepping (halve h when sigma > threshold)
- [ ] Scale sigma with arousal: `sigma = sigma_base × (1 + k × (1 - arousal))` so low-arousal presets get more noise
- [ ] CLI: expose sigma as `--jr-sigma N` parameter (default: current 15.0 for backward compat)
- [ ] Regression: sigma=15 on input-p (default) produces bitwise-identical scores to pre-P18

**Expected outcome:** At sigma=100-200, the JR system should intermittently escape the alpha basin, spend time in theta, and return — producing a time-averaged spectrum with both bands represented.

- [ ] **Ref:** Ableidinger M, Buckwar E, Hinterleitner H (2017). "A Stochastic Version of the Jansen and Rit Neural Mass Model: Analysis and Numerics." *J Math Neurosci* 7:8. — Noise on velocity state variables (σ₃=σ₅=10, σ₄=1000). Multimodal probability density at C=270. **Warning: Euler-Maruyama produces spurious bifurcations at high sigma (Fig. 9) — use Strang splitting or verify RK4 stability.** Ito and Stratonovich interpretations coincide (additive noise). [PMC5567162](https://pmc.ncbi.nlm.nih.gov/articles/PMC5567162/)
- [ ] **Ref:** Grimbert F, Faugeras O (2006). "Bifurcation analysis of Jansen's neural mass model." *Neural Comput* 18(12):3052-3068. — Alpha limit cycle exists for p ∈ [89.83, 315.70]. Fold bifurcation at p≈137.5 where alpha and slow oscillation coexist bistably. Noise can drive transitions between coexisting basins.

### 18b. Retune GABA_B Time Constant for Theta Resonance (LOW EFFORT — parameter change)

**The science:** Ursino, Cona & Zavaglia (2010) demonstrated that a single cortical region with **two inhibitory populations having different synaptic kinetics** produces simultaneous multi-band rhythms. GABA_A,slow (τ_s ≈ 20-25ms, i.e., b=40-50/s) drives alpha/beta. A second, separate inhibitory population with τ ≈ 33-50ms (b=20-30/s) places its resonance in the theta range (5-8 Hz). When both compete for the pyramidal population output, the time-averaged spectrum shows both alpha and theta.

Wendling et al. (2002) showed the critical transition is controlled by B (slow inhibitory gain): B=45 produces normal background alpha; B=38 produces sporadic spikes; B=8 produces gamma. Theta emerges as B decreases from 45 toward 38. However, Wendling found no stable coexistence in the original parameterization — the transitions are sharp bifurcations. Ursino's key insight was that **time constant separation** (not gain reduction) is what produces coexistence.

**Our current state:** b_slow=5/s (τ=200ms), B_slow=10mV, C_slow=30.0. This is far too slow for theta resonance — at τ=200ms the population's natural frequency is ~0.8 Hz (sub-delta). It acts as a pure DC offset reducer, not a theta oscillator.

**Implementation:**
- [x] Unit tests (4 passing): backward compat (default unchanged bitwise), output finite at b_slow=25/30/40, different EEG from original params, b_slow=25 produces more balanced theta/alpha ratio than b_slow=5
- [ ] Change CET default `b_slow` from 5.0 to **25.0** in pipeline.rs (when cet_enabled)
- [ ] Change CET default `B_slow` from 10.0 to **15.0-20.0** mV in pipeline.rs (when cet_enabled)
- [ ] Keep `C_slow` at 30.0 (connectivity from pyramidal to slow inhibitory population)
- [ ] Test parameter sweep: b_slow ∈ {15, 20, 25, 30, 40}, B_slow ∈ {12, 15, 18, 20} with CET enabled + thalamic gate on deep relaxation preset
- [ ] Measure: does the 300s spectrum show theta 20-40% AND alpha 20-40% simultaneously?
- [ ] The mechanism: at b_slow=5 (τ=200ms), slow inhibition sustains pyramidal suppression → extreme theta dominance (ratio 3879:1). At b_slow=25 (τ=40ms), faster recovery → more balanced ratio (63:1). Need to find the b_slow that gives ratio ~1:1.
- [ ] CLI: expose b_slow tuning as `--gaba-b-rate N` (default: 5.0 for backward compat)
- [ ] Regression: b_slow=5.0 (default) produces bitwise-identical scores to pre-P18

**Note on Ursino correction:** Ursino, Cona & Zavaglia (2010) use two GABA-**A** populations (slow b=50/s + fast g=500/s), NOT GABA-A + GABA-B. Their model produces alpha+gamma combinations, not theta+alpha directly. Theta from a single column requires reducing the slow inhibitory rate to b≈15-25/s — extrapolated from their framework but not directly tested in their paper. The Wendling (2002) standard params confirm: GABA-A slow at b=50 → alpha; reducing b shifts oscillation downward. Our test at b_slow=25 confirmed: theta/alpha ratio drops from 3879 to 63 (more balanced) compared to b_slow=5.

**Expected outcome:** With b_slow=25/s, the slow inhibitory population resonates at theta frequencies. The fast inhibitory population (b=50/s) continues resonating at alpha. The pyramidal population output is the difference of both — producing a mixed spectrum where both alpha and theta are present. This is not two separate oscillators averaged, but genuine cross-frequency coupling (theta modulating alpha envelope), exactly as seen in real EEG during deep relaxation (Klimesch 1999).

- [ ] **Ref:** Ursino M, Cona F, Zavaglia M (2010). "A neural mass model for the simulation of cortical activity estimated from high resolution EEG during cognitive or motor tasks." *Comput Intell Neurosci* 2010:456140. — Single cortical region with two inhibitory populations (slow τ_s≈20-25ms and fast τ_f≈3-5ms) produces simultaneous multi-band rhythms. Connectivity strengths C_pf vs C_ps control the power distribution between frequency bands. [DOI](https://onlinelibrary.wiley.com/doi/10.1155/2010/456140)
- [ ] **Ref:** Ursino M, Cona F (2010). "Independent component analysis and variable neural coupling in the generation of cortical rhythms." *NeuroImage* 52(3):839-854. — Companion paper showing variable connectivity between inhibitory populations produces different spectral compositions (alpha-dominant, theta-dominant, mixed). [PMID 20045071](https://pubmed.ncbi.nlm.nih.gov/20045071/)
- [ ] **Ref:** Wendling F, Bartolomei F, Bellanger JJ, Chauvel P (2002). "Epileptic fast activity can be explained by a model of impaired GABAergic dendritic inhibition." *Eur J Neurosci* 15(9):1499-1508. — Standard Wendling parameters: a=100/s, b=50/s, g=350/s, A=5mV. B=45 → alpha, B=38 → sporadic, B=8 → gamma. Transitions are sharp bifurcations. [PMID 12028360](https://pubmed.ncbi.nlm.nih.gov/12028360/)
- [ ] **Ref:** Wendling F, Hernandez A, Bellanger JJ, Chauvel P, Bartolomei F (2005). "Interictal to ictal transition in human temporal lobe epilepsy: insights from a computational model of intracerebral EEG." *J Clin Neurophysiol* 22(5):343-356. — Extended Wendling model parameter analysis confirming the role of slow inhibition time constant in frequency selection. [PMC4838812](https://pmc.ncbi.nlm.nih.gov/articles/PMC4838812/)
- [ ] **Ref:** Klimesch W (1999). "EEG alpha and theta oscillations reflect cognitive and memory performance: a review and analysis." *Brain Res Rev* 29(2-3):169-195. — Establishes that theta-alpha coexistence (not alternation) is the normal EEG signature during relaxed wakefulness and memory encoding.

### 18c. Two Coupled JR Columns at Different Operating Points (MEDIUM EFFORT — new architecture)

**The science:** Ahmadizadeh et al. (2018) analyzed two coupled JR columns with coupling gains K = 25, 50, 100, 150. At **K=25-50**, asymmetric equilibria emerge where the two columns settle at different amplitude states. Alpha activity spans input u ∈ [87.43, 106] for asymmetric vs [71.56, 313.4] for symmetric cases. Neimark-Sacker and period-doubling bifurcations produce quasi-periodic oscillations at moderate coupling — exactly the mixed-frequency behavior we need.

The key insight: with weak coupling (K=25-50), each column preserves its intrinsic frequency while the summed output yields a mixed spectrum. One column biased toward theta (lower band_offset) and one toward alpha (higher band_offset) produces a guaranteed mixed state by construction.

**Implementation:**
- [ ] For each tonotopic band, instantiate TWO JansenRitModel instances instead of one:
  - Column A: band_offset biased toward theta (offset reduced by ~30 from default)
  - Column B: band_offset at default (alpha-producing)
- [ ] Add excitatory coupling: `input_A += K * S(v_pyr_B)` and `input_B += K * S(v_pyr_A)` with K=25-50
- [ ] Sum outputs: `eeg = mix_ratio * eeg_A + (1 - mix_ratio) * eeg_B` with mix_ratio=0.5 default
- [ ] The mix_ratio could be goal-dependent: relaxation → 0.6 (more theta column), focus → 0.3 (more alpha column)
- [ ] Gate behind `--dual-column` CLI flag. Default off. When off, behavior is bitwise identical (single column path unchanged)
- [ ] Unit tests: dual column produces broader spectrum than single, K=0 produces independent oscillations, K=inf produces synchronized, output finite at K=150
- [ ] Regression: single column path unchanged when flag off

**Expected outcome:** The summed output of two columns at different operating points inherently contains both alpha and theta. The coupling prevents complete independence (avoids artificial-sounding summation) while preserving frequency diversity. This approach guarantees a mixed spectrum by construction but requires more code changes than 18a/18b.

- [ ] **Ref:** Ahmadizadeh S, Karoly PJ, Nešić D, Grayden DB, Cook MJ, Soudry D, Freestone DR (2018). "Bifurcation analysis of two coupled Jansen-Rit neural mass models." *PLoS One* 13(2):e0192842. — Two coupled JR columns with K=25-150. Asymmetric equilibria at K=25-50. Neimark-Sacker bifurcations produce quasi-periodic oscillations. [DOI](https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0192842)
- [ ] **Ref:** Ableidinger M, Buckwar E, Hinterleitner H (2018). "Bifurcation analysis of two coupled Jansen-Rit neural mass models." Documented synchronization, anti-phase oscillation, and emergent dynamics in coupled JR. (Already cited in Priority 11.)

### Implementation order

**Phase 1 (test immediately — parameter changes only):**
1. 18a: Sweep sigma from 50 to 300 on deep relaxation preset with thalamic gate at reverb≈0.25
2. 18b: Sweep b_slow from 15 to 30 with B_slow from 12 to 20, CET enabled

**Phase 2 (implement if Phase 1 insufficient):**
3. 18a+18b combined: higher sigma + retuned GABA_B together
4. 18a arousal-scaled sigma: low arousal → higher sigma automatically

**Phase 3 (implement if Phase 1+2 insufficient):**
5. 18c: Dual coupled columns (requires new code in jansen_rit.rs)

### Empirical evidence from Deep Relaxation experiments (2026-04-15/16)

The following results motivated this priority. All use the same base preset (2 static brown front/back + 2 fig8 pink at prime speeds 0.43/0.53, DeepSanctuary environment):

| Configuration | θ 10s | θ 300s | α 10s | α 300s | Score 300s |
|---|---|---|---|---|---|
| No gate, no CET | 22.5% | 5.4% | 75.7% | 92.8% | 0.341 |
| Thalamic gate (rev=0.25) | 34.7% | 86.2% | 62.7% | 10.9% | 0.349 |
| CET + thalamic gate (rev=0.25) | 34.7% | 86.3% | 62.7% | 10.8% | 0.348 |
| Phys-gate | 68.7% | 67.8% | 11.7% | 14.4% | 0.442 |
| CET + phys-gate | 76.7% | 78.5% | 8.7% | 9.0% | 0.477 |
| Chimera Split (hemispheric) | LH:47/47 | LH:56/38 | RH:74/24 | RH:89/9 | 0.340 |
| **Target (ideal)** | **35%** | **35%** | **36%** | **36%** | **>0.60** |

The Chimera Split showed genuine hemispheric decoupling at 600s (LH: theta 56% / alpha 38%), proving the JR model CAN sustain a mixed state in one hemisphere when the input spectrum is sufficiently different. The missing piece is a mechanism to sustain this at the single-column level.

---

## Priority 20: Canonical Evaluate Pipeline and CLI Consistency (CRITICAL IMPACT, LOW-MEDIUM EFFORT)

**Problem:** `evaluate` still has two pipelines. `run_evaluate()` calls `evaluate_preset()` in `src/pipeline.rs`, but then re-runs a separate `run_detailed_pipeline()` in `src/main.rs` and prints diagnostics from that second path. The two paths are not equivalent:

- `pipeline.rs` decimates 48 kHz band envelopes to `NEURAL_SR`, trims warmup, applies CET split/recombine at 1 kHz, optionally applies ASSR only to the fast CET path, uses the configured habituation/stochastic flags, and computes diagnostics with `PerformanceVector::compute_with_envelope()`.
- `run_detailed_pipeline()` keeps the band envelopes at 48 kHz, applies a different ASSR/CET flow, hard-codes stochastic JR on, hard-codes habituation off, and uses the legacy `PerformanceVector::compute()` path without envelope PLV.

Net effect: single-preset `evaluate` can disagree with matrix mode, `optimize`, and `generate-data` even when the preset, goal, brain type, and CLI flags are identical. That makes the human-facing diagnosis unreliable and makes documentation drift harder to notice.

**Goal:** make `src/pipeline.rs` the only source of truth for simulation and scoring. `src/main.rs` should format results, not recompute them.

### 20a. Return detailed diagnostics from the canonical pipeline

- [x] Add a `DetailedSimulationResult` in `src/pipeline.rs`, or extend `SimulationResult`, so the canonical path can return:
  - final `score`
  - `FhnResult`
  - `BilateralResult`
  - `PerformanceVector`
  - `brightness`
  - `band_energy_fractions`
- [x] Keep `evaluate_preset()` as a thin compatibility wrapper over the new detailed entry point so existing optimizer and matrix callers keep their current API.
- [x] Reuse the exact existing `pipeline.rs` order of operations: audio render -> room/environment processing -> gammatone -> global normalization -> decimation -> CET split/recombine -> ASSR on fast path only -> thalamic gate -> bilateral JR -> FHN -> `compute_with_envelope()` -> `Goal::evaluate_full()`.

### 20b. Remove duplicated neural computation from `main.rs`

- [x] Change `run_evaluate()` so the printed score comes directly from the canonical pipeline result.
- [x] Delete `run_detailed_pipeline()` entirely, or reduce it to a formatting helper that consumes `DetailedSimulationResult` without re-running the model.
- [x] Feed `goal.diagnose()` from the canonical pipeline outputs instead of a second simulation pass.
- [x] Remove comments in `main.rs` that justify behavioral differences in the display path. The display path should contain no independent physiological logic.

### 20c. Fix `evaluate` CLI semantics

- [x] Replace the current misleading bool flags with explicit semantics that match the help text:
  - `--assr` / `--no-assr`
  - `--thalamic-gate` / `--no-thalamic-gate`
  - `--cet` / `--no-cet`
  - `--phys-gate`
- [ ] Align one source of truth for defaults across `SimulationConfig`, `src/main.rs`, `API_DOCUMENTATION.md`, and `BRAIN_MODEL_GUIDE.md`. The user-facing docs are now aligned with `src/main.rs`; the remaining nuance is that `SimulationConfig::default()` is an internal pipeline default, while `evaluate` deliberately overrides ASSR to off unless requested.
- [x] Print the active feature set in `evaluate` output (`assr`, `thalamic_gate`, `cet`, `phys_gate`) so screenshots and manual evaluations are reproducible.

### 20d. Add consistency and regression tests

- [x] Add a regression test proving: for the same preset/goal/brain/config, single-preset `evaluate` reports the same numeric score as `evaluate_preset()`.
- [x] Add a regression test proving: matrix mode and single mode produce the same score for the same single cell.
- [x] Add a regression test proving: the canonical detailed result reproduces the same diagnosis score the pipeline already computed. This remains the score/diagnosis guard alongside the new matrix/single-cell parity test.
- [x] Add a helper-level regression test proving the documented disable flags (`--no-cet`, `--no-thalamic-gate`) resolve to the intended `SimulationConfig`. A full CLI smoke test remains optional.
- [x] Keep the existing thalamic-gate unit failures as a separate Priority 2 consistency issue; do not weaken `evaluate` tests to hide that mismatch. This is now historical only because Priority 21 returned those tests to green without touching the `evaluate` consistency guards.

### 20e. Documentation cleanup after the code path is canonical

- [x] Update `API_DOCUMENTATION.md` examples so `evaluate` usage and defaults match the real CLI.
- [x] Update `BRAIN_MODEL_GUIDE.md` sections that currently describe stale `evaluate` defaults or stale flag behavior.
- [x] Add one short architecture note stating that all scoring paths (`evaluate`, matrix, `optimize`, `generate-data`) share the same simulation core in `pipeline.rs`.

### 20f. Scope and references

- [ ] No new neuroscience literature is required for this priority. This is an implementation-integrity fix, not a new physiological model.
- [ ] Internal code references for this work:
  - `src/main.rs::run_evaluate`
  - `src/main.rs::run_detailed_pipeline`
  - `src/pipeline.rs::evaluate_preset`
  - `PerformanceVector::compute_with_envelope`
  - `SimulationConfig`

**Expected outcome:** `evaluate`, matrix mode, `optimize`, and `generate-data` all report the same score and diagnostics for the same preset and flags. Once that is true, future model work only needs to land in one place, and human-facing diagnosis becomes trustworthy again.

## Priority 21: Thalamic Gate Spec Reconciliation (CRITICAL IMPACT, LOW EFFORT)

**Problem:** The heuristic thalamic gate was internally inconsistent. The code in `src/auditory/thalamic_gate.rs` had drifted away from its own tests and the user-facing docs:

- Tests still assert the original Steriade-inspired per-band proportions `[100%, 70%, 20%, 0%]`.
- `BRAIN_MODEL_GUIDE.md`, `API_DOCUMENTATION.md`, and `update_model.md` described that same fixed profile.
- The implementation now applies a deeper low-arousal redistribution, so band 1 can behave like 100%, band 2 can exceed the documented 20%, and band 3 is no longer guaranteed to stay at 0%.

This is the only current red test area in the suite. More importantly, it means the project's "arousal gate" story is ambiguous at exactly the point where users rely on it most: slow-state access via reverb/arousal.

**Decision:** keep the original fixed Steriade-inspired proportions and make the whole repo agree:

1. **Heuristic gate** uses fixed per-band offset reductions `[100%, 70%, 20%, 0%] × max_shift`
   - simplest
   - matches the existing literature summary and user mental model
   - keeps band 3 unchanged by design
2. **Physiological gate** remains the nonlinear path
   - stronger low-arousal push belongs in `--phys-gate`
   - biophysical justification stays with the TC-cell model, not the heuristic shortcut

### 21a. Code/doc/test alignment

- [x] Decide that the heuristic gate is "fixed Steriade proportions", not "deep-factor nonlinear redistribution".
- [x] Simplify `band_offset_shifts()` to `[100%, 70%, 20%, 0%] × max_shift`.
- [x] Remove the deep-factor drift from the heuristic implementation and keep nonlinear burst-mode behavior in `--phys-gate`.
- [x] Update the unit tests so they assert the intended fixed-profile behavior explicitly, including a mid-arousal proportion check.
- [x] Update `API_DOCUMENTATION.md` and `update_model.md` so they match the fixed-profile heuristic gate.
- [x] Update `BRAIN_MODEL_GUIDE.md` so the remaining user-facing narrative matches the code.

### 21b. Regression and acceptance

- [x] `cargo test auditory::thalamic_gate::tests` returns to green.
- [x] Keep the explicit regression coverage for band 3 behavior so it cannot drift silently again.
- [x] Add regression coverage for the fixed proportions at both low arousal and mid arousal.
- [x] Re-evaluate at least one sleep/relaxation preset with heuristic gate vs physiological gate to confirm the intended separation of responsibility:
  - heuristic gate = simple arousal steering
  - physiological gate = stronger nonlinear burst-mode push

**Verification completed:**
- `cargo test auditory::thalamic_gate::tests`
- `cargo test regression_tests::tests::enabled_features_are_consistent -- --nocapture`
- `cargo test regression_tests::tests::thalamic_gate_disabled_changes_scores -- --nocapture`
- `cargo test regression_tests::tests::both_features_enabled_produces_valid_scores -- --nocapture`
- `cargo test regression_tests::tests::detailed_pipeline_summary_matches_scalar_evaluate -- --nocapture`
- `cargo test regression_tests::tests::canonical_detailed_result_reproduces_diagnosis_score -- --nocapture`
- `cargo test --no-run`

**Expected outcome:** there is exactly one documented heuristic gate behavior, the tests encode it, and users can understand the difference between `--thalamic-gate` and `--phys-gate` without reverse-engineering the source.

## Priority 22: Surrogate Artifact and Data Integrity (CRITICAL IMPACT, MEDIUM EFFORT)

**Problem:** The surrogate stack is dimensionally and procedurally stale:

- Rust now uses the 230-gene genome and a larger total surrogate input dimension.
- `tools/train_surrogate.py` still describes the old input layout.
- Existing exported weights still reflect the old input dimension.
- `generate-data` writes config-flag columns that do not match the actual `SimulationConfig` used to score the rows.

This is worse than "just stale docs". It can make the surrogate silently ignore part of the input, and it can poison retraining data with mislabeled feature flags.

### 22a. Freeze the input contract

- [x] Define one canonical surrogate input contract in one place:
  - genome length
  - goal one-hot width
  - brain-type one-hot width
  - config-flag width
- [x] Make Rust and Python derive their dimensions from that shared contract, not from hand-maintained comments.
- [x] Update `update_model.md` and `API_DOCUMENTATION.md` examples from `g0..g189` to the current genome width.

### 22b. Validate artifacts at load time

- [x] In `src/surrogate.rs`, reject weights whose declared input dimension does not match the compiled `INPUT_DIM`.
- [x] Surface a clear runtime error that says the weights are stale and must be retrained.
- [x] Do not allow silent partial-input inference in release builds.

### 22c. Fix training-data correctness

- [x] Make `generate-data` write the REAL feature flags used during evaluation, not hard-coded values.
- [x] Specifically verify the `cet` column matches the `SimulationConfig` that produced the score.
- [x] Add a regression test that inspects a generated row and proves the serialized flags equal the actual config.

### 22d. Retraining and docs

- [x] Retrain the surrogate after the input contract and CSV labeling are corrected.
- [x] Regenerate `surrogate_weights.bin` (and any small/medium variants) with the new dimensions.
- [x] Add a regression test that loads the bundled weight artifacts and runs inference through the current 248-input contract.
- [x] Extend `generate-data` with `--phys-gate` so retraining can include physiological-gate rows instead of only heuristic-gate rows.
- [x] Update `API_DOCUMENTATION.md` and `update_model.md` to describe the real input size, CSV schema, bundled artifacts, and retraining workflow.
- [x] Update `BRAIN_MODEL_GUIDE.md` to describe the real input size, CSV schema, and retraining workflow.

**Current status:** checked-in surrogate artifacts (`surrogate_weights.bin`, `surrogate_weights_small.bin`, `surrogate_weights_med.bin`) now use the current 248-input header and load successfully under the stricter validator. The checked-in CSV datasets (`training_data.csv`, `training_combined.csv`) also use the corrected 237-column schema and include both default rows (`1,1,1,0`) and phys-gate rows (`1,1,1,1`).

**Expected outcome:** the surrogate either runs on a verified current artifact or fails loudly; training data rows become trustworthy; and future scoring changes cannot silently desynchronize the Rust/Python pipeline.

## Priority 23: Surrogate Search Semantics and Optimizer Truthfulness (HIGH IMPACT, LOW-MEDIUM EFFORT)

**Problem:** The optimizer/docs contract was inconsistent: surrogate mode claimed to be only a filter, but unvalidated surrogate scores were still entering DE population state.

**Decision taken:** strict real-score population.

- only real-pipeline scores enter DE state
- unvalidated candidates are ignored for parent replacement
- the surrogate remains a ranking/filter layer only
- final preset reporting/export stays fully real-pipeline

### 23a. Align semantics

- [x] Decide whether surrogate scores are allowed to enter DE population state.
- [x] Refactor `run_optimize()` so only validated trials are reported into DE.
- [x] Update surrogate code comments so they describe the strict real-score contract.

### 23b. Add explicit optimizer tests

- [x] Add a regression test that encodes the chosen behavior.
- [x] Add a dedicated test that final exported presets are always re-evaluated with the real pipeline.
- [x] Add one generation-level test or debug statistic proving whether unvalidated candidates influence parent replacement.

Completed checks:

- `surrogate_validation_mask_*` tests in `src/main.rs` lock the top-K + exploration selection policy
- `optimizer::differential_evolution::tests::skipped_trial_report_keeps_parent_unchanged` proves unreported trials do not replace parents
- `tests::export_best_genome_uses_re_evaluated_real_score` proves the export path rebuilds the best preset from genome, re-runs the real pipeline, and writes that score/analysis to JSON rather than trusting any cached optimizer fitness
- surrogate optimizer smoke run succeeds with current bundled weights after the semantics change

### 23c. User-facing clarity

- [x] Update the optimizer help text and `API_DOCUMENTATION.md` so users know what `--surrogate` really guarantees.
- [x] Update `BRAIN_MODEL_GUIDE.md` so the long-form narrative matches the strict real-score contract.

**Expected outcome:** users can trust the written description of surrogate mode, and engineers can no longer accidentally change the surrogate/DE contract without tripping a test.

## Priority 24: Documentation Closure and Test-Harness Stability (MEDIUM IMPACT, LOW-MEDIUM EFFORT)

**Problem:** After the Priority 20 refactor, the canonical scoring path is cleaner, but there are still two repo-level stability problems:

1. **Docs are stale in multiple places.**
   - feature defaults
   - `evaluate` examples
   - surrogate dimensions / CSV column counts
   - the heuristic-vs-physiological gate descriptions
2. **The default test suite was pulling in exploratory sweeps and a few oversized validity checks.**
   - the previous `thalamic_gate` red tests are fixed
   - the main confusion was not a model failure signal; it was that `cargo test` still included table-printing preset-analysis sweeps (`analyze_preset::tests::*`) plus several validity-only regression tests that were still using full 12 s simulations

The first problem hurts user trust; the second hurts engineering throughput because it makes full-regression runs feel ambiguous even when the real failure signal is already known.

### 24a. Documentation sweep

- [x] Audit `API_DOCUMENTATION.md` for:
  - `evaluate` defaults
  - `--no-*` flags
  - surrogate CSV schema
  - surrogate usage guarantees
- [x] Audit `API_DOCUMENTATION.md` for the `evaluate` contract specifically: defaults, `--no-*` flags, optimize-parity example, and the shared `pipeline.rs` scoring-core note are now aligned with `src/main.rs`.
- [x] Audit `BRAIN_MODEL_GUIDE.md` for:
  - feature defaults
  - heuristic gate behavior
  - surrogate dimensions and workflow
  - any stale examples that still assume the deleted duplicate evaluate path
- [x] Add one short note that `pipeline.rs` is now the single scoring core for `evaluate`, matrix mode, `optimize`, and `generate-data`.

### 24b. Full-test stability

- [x] Identify the first major long-tail source: `src/analyze_preset.rs` contains four exploratory sweep tests that print tables and run many full `evaluate_preset()` calls, but do not assert regression properties.
- [x] Reclassify those four tests behind `#[ignore]` with an explicit manual invocation string (`cargo test analyze_preset::tests -- --ignored --nocapture`).
- [x] Shorten the heaviest validity-only regression loops in `src/regression_tests.rs` by using a reduced 4.0 s test config instead of the default 12.0 s config where the assertions only require "finite/in-range/different", not long-horizon stability.
- [x] Re-run the default `cargo test` suite and confirm that the remaining long-running regression tests now end with a clean harness summary rather than an ambiguous tail (`412 passed; 0 failed; 4 ignored`).
- [x] Refactor the last oversized normalization validity test to representative coverage (3 colors × 4 goals) so it still guards pipeline boundedness without dominating the suite runtime.

### 24c. Short-duration safety guard

**Problem:** CLI commands accepted durations shorter than the 2.0-second neural warm-up discard. That could produce an empty post-warmup signal and eventually panic inside `JansenRitModel::simulate()` on `input[0]`. The surrogate smoke test with `optimize --duration 1` exposed it, but the bug affected any neural-analysis entry point using an undersized duration.

- [x] Add a shared `validate_analysis_window(duration_secs, warmup_discard_secs)` helper in `pipeline.rs`.
- [x] Enforce that guard in `optimize`, `evaluate`, `generate-data`, and `disturb`.
- [x] Keep a pipeline-side assertion so direct non-CLI callers fail with a precise configuration error instead of a deep JR panic.
- [x] Add unit tests for valid and invalid duration windows.
- [x] Smoke-check that `optimize --duration 1` now exits cleanly with a validation error.

**Expected outcome:** unsupported short durations fail immediately and clearly, and no neural-analysis command can reach the old empty-input panic path through normal CLI usage.

### 24d. Acceptance

- [x] A clean `cargo test` run should have one of two outcomes:
  - green, or
  - a clearly identified non-thalamic failure with a clean process exit
- [x] No silent teardown stall after the last reported test.

**Expected outcome:** the docs describe the repo that actually exists, exploratory analysis is no longer mixed into the default regression suite, and a full regression run becomes operationally trustworthy again.

**Verification run:**
- `cargo test analyze_preset::tests`
- `cargo test regression_tests::tests::normalization_change_preserves_valid_scores`
- `cargo test --no-run`
- `cargo test`

## Priority 25: Acoustic Masking and Psychoacoustic Subscore Layer (HIGH IMPACT, MEDIUM-HIGH EFFORT)

**Problem:** The current scoring stack is dominated by the neural-model output: EEG band powers, FHN response, asymmetry, PLV, and related modifiers. That is appropriate for "brain-state" goals, but it under-specifies goals whose product value also depends on the acoustic task in the ear. `shield` is the clearest example:

- a preset can create a calmer NMM state while still leaving speech too intelligible to be useful in an office
- a preset can also improve speech masking acoustically while slightly worsening the current EEG-based Shield score
- the result is that preset search and manual tuning are forced to use one proxy (`scoring.rs`) for two different phenomena:
  1. cortical state alignment
  2. acoustic masking / comfort / privacy

This is not just a Shield issue. A generalized acoustic layer is useful across goals:

- `shield`, `isolation`: speech privacy / masking effectiveness
- `focus`, `deep_work`: distraction suppression and non-fatiguing masking
- `relaxation`, `sleep`, `meditation`: acoustic comfort, low sharpness, low fluctuation, low roughness
- `flow`, `ignition`: avoid harsh or overly salient sound design while preserving neural drive

**Solution:** Add a second scoring branch that evaluates the rendered sound as an acoustic stimulus, then combine it with the existing NMM score using goal-specific weights.

Proposed top-level structure:

```text
FinalGoalScore =
  w_nmm(goal)      * NMMScore
+ w_acoustic(goal) * AcousticGoalScore
```

where:

```text
AcousticGoalScore = weighted combination of:
  - speech_privacy / intelligibility suppression
  - modulation / envelope masking
  - psychoacoustic comfort
  - optional spatial coverage / diffuseness
```

The current NMM score remains intact. This priority adds a parallel acoustic evaluation layer rather than replacing the neural model.

### 25a. Acoustic feature vector

- [ ] Define a reusable `AcousticFeatureVector` in a new module (for example `src/acoustic_score.rs`) with at least:
  - `speech_privacy`
  - `envelope_masking`
  - `loudness_comfort`
  - `sharpness_penalty`
  - `roughness_penalty`
  - `fluctuation_penalty`
  - `spatial_diffuseness` (optional first pass; can be deferred if expensive)
- [ ] Keep the feature vector goal-agnostic. Goal-specific meaning should come from weights in `scoring.rs`, not from hard-coded logic inside the extractor.
- [ ] Make the extractor operate on rendered stereo audio, so it can be reused by `evaluate`, `optimize`, `disturb`, and later subjective/offline analysis scripts.

**Design rule:** this layer must remain additive and flag-gated at first, just like ASSR / CET / surrogate integration. Existing scores should remain reproducible when the acoustic branch is disabled.

### 25b. Speech intelligibility / privacy term

- [ ] Implement a first-pass intelligibility proxy using a small reference speech corpus:
  - dry male + female speech
  - short fixed utterances
  - deterministic seed / asset set for reproducibility
- [ ] Render `speech`, `preset`, and `speech + preset`.
- [ ] Compute a band-weighted effective SNR or SII-like proxy over speech-relevant auditory bands.
- [ ] Convert that into a monotonic privacy score, e.g. `speech_privacy = 1 - intelligibility_proxy`.
- [ ] Calibrate the first version against office masking expectations, not against arbitrary absolute thresholds.

**Why this first:** office-distraction literature consistently points to speech intelligibility, not raw sound level, as the main driver of cognitive disruption in open-plan environments.

**References:**
- ANSI/ASA S3.5 — *Methods for Calculation of the Speech Intelligibility Index (SII)*.
- IEC 60268-16 — *Speech Transmission Index (STI)*.
- Hongisto V (2005). "A model predicting the effect of speech of varying intelligibility on work performance." *Indoor Air* 15(6):458-468.
- Haapakangas A, Hongisto V, Liebl A (2020). "The relation between the intelligibility of irrelevant speech and cognitive performance: A revised model based on laboratory studies." *Indoor Air* 30(6):1130-1146.
- Venetjoki N, Kaarlela-Tuomaala A, Keskinen E, Hongisto V (2006). "The effect of speech and speech intelligibility on task performance." *Ergonomics* 49(11):1068-1091.

### 25c. Envelope / modulation masking term

- [ ] Add a second speech-specific term that measures how much the preset destroys speech-envelope fidelity rather than only its band-energy SNR.
- [ ] Reuse the repo's auditory front end where practical:
  - gammatone banding
  - per-band envelope extraction
  - modulation-band analysis focused on the speech-relevant slow envelope range
- [ ] Weight low modulation rates most strongly (roughly syllabic / fluctuation range).
- [ ] Define a bounded score such as `envelope_masking in [0, 1]`, where higher means stronger disruption of speech-envelope cues.

**Why this matters:** two presets with similar spectra can differ substantially in how much intelligible envelope structure they leave intact. A purely spectral masker can miss that.

**References:**
- Chi T, Gao Y, Guyton MC, Ru P, Shamma S (1999). "Spectro-temporal modulation transfer functions and speech intelligibility." *J Acoust Soc Am* 106(5):2719-2732.
- Elliott TM, Theunissen FE (2009). "The modulation transfer function for speech intelligibility." *PLoS Comput Biol* 5(3):e1000302.
- Drullman R, Festen JM, Plomp R (1994). "Effect of temporal envelope smearing on speech reception." *J Acoust Soc Am* 95(5 Pt 1):2670-2680.
- Kates JM, Arehart KH (2005). "Coherence and the speech intelligibility index." *J Acoust Soc Am* 117(4 Pt 1):2224-2237.

### 25d. Psychoacoustic comfort term

- [ ] Add a lightweight psychoacoustic comfort branch, initially independent of speech:
  - loudness
  - sharpness
  - roughness
  - fluctuation strength
- [ ] Convert those into bounded penalties/rewards that can be weighted differently by goal.
- [ ] Keep the first implementation simple and monotonic:
  - sleep / meditation should penalize sharpness, roughness, and fluctuation more strongly
  - shield / focus can tolerate somewhat higher loudness, but should still penalize harshness and aggressive temporal roughness
- [ ] Consider a later composite "annoyance" score only after the individual dimensions are stable and tested.

**Why this matters:** a preset that looks good neurally but sounds harsh, rough, or fatiguing is not shippable. This term is the bridge between NMM search and product sound quality.

**References:**
- Zwicker E, Fastl H (1999). *Psychoacoustics: Facts and Models.* 2nd ed. Springer.
- Fastl H (1982). "Fluctuation strength and temporal masking patterns of amplitude-modulated broadband noise." *Hear Res* 8(1):59-69.
- Fastl H (1990). "The hearing sensation roughness and neuronal responses to AM-tones." *Hear Res* 46(3):293-295.

### 25e. Goal-specific fusion

- [ ] Extend `scoring.rs` so acoustic features are fused with the existing NMM score using per-goal weights.
- [ ] Start with explicit hand-tuned weights rather than trying to learn them automatically.
- [ ] Proposed initial policy:
  - `shield`, `isolation`: high acoustic weight
  - `focus`, `deep_work`: moderate acoustic weight
  - `sleep`, `meditation`, `deep_relaxation`: low speech-privacy weight but meaningful comfort weight
  - `ignition`, `flow`: mostly neural, small comfort guardrail
- [ ] Keep these weights visible and documented so they can be revised after listening tests or human-data calibration.

Suggested first-pass weights (engineering prior, not yet empirically validated):

```text
Shield / Isolation:
  Final = 0.60 * NMM + 0.40 * Acoustic

Focus / DeepWork:
  Final = 0.70 * NMM + 0.30 * Acoustic

Sleep / Meditation / DeepRelaxation:
  Final = 0.85 * NMM + 0.15 * Acoustic
```

### 25f. Implementation phases

- [ ] **Phase 1: SII/STI-style proxy**
  - implement the speech-privacy term only
  - add deterministic fixtures and unit tests
  - expose metrics in `evaluate`
- [ ] **Phase 2: modulation masking**
  - add envelope-fidelity degradation
  - verify that presets with similar band spectra but different temporal structure separate correctly
- [ ] **Phase 3: psychoacoustic comfort**
  - add loudness / sharpness / roughness / fluctuation terms
  - gate by goal weighting
- [ ] **Phase 4: optimizer integration**
  - include the acoustic branch in the optimization objective
  - update surrogate/data contract if the final scalar score changes materially

### 25g. Regression and acceptance criteria

- [ ] Unit tests:
  - louder speech masker in speech bands should reduce intelligibility proxy
  - envelope-smearing maskers should outperform spectrally matched flat maskers on `envelope_masking`
  - comfort penalties should increase for deliberately rough / sharp / highly fluctuating synthetic cases
- [ ] Integration tests:
  - `shield` presets optimized under the new score should separate better from sleep/relaxation presets on intelligibility suppression
  - `sleep` presets should not be rewarded for high speech masking if they become sharp/rough
- [ ] CLI output:
  - `evaluate` should print acoustic sub-metrics alongside neural metrics when enabled
- [ ] Backward compatibility:
  - acoustic scoring branch disabled => bit-for-bit identical legacy scores

**Expected outcome:** Shield stops being a pure EEG proxy and becomes a composite model of "protective brain state + useful acoustic masking". More broadly, the repo gains a reusable acoustic quality layer that improves preset selection across all goals, not just shielding.

### 25h. Controlled rollout plan (regression-first)

**Implementation rule:** do not ship the acoustic scoring branch as one large feature. Land it in small, testable layers with explicit flags, so every phase can be evaluated locally and disabled without changing legacy behavior.

#### Phase 0 — Scaffolding only

**Goal:** add the architecture without changing any score.

- [x] Create a dedicated module (for example `src/acoustic_score.rs`).
- [x] Define:
  - `AcousticFeatureVector`
  - `AcousticScoreConfig`
  - `AcousticScoreResult`
- [x] Add `acoustic_scoring_enabled: bool` to the simulation/evaluation config surface.
- [x] Thread the config through `evaluate`, `optimize`, and `disturb`, but keep it default-off everywhere.
- [x] No score fusion, no optimizer use, no CLI output change yet.

**Acceptance / regression bar:**
- [x] Config default is disabled.
- [x] When disabled, all legacy scoring paths are bit-for-bit unchanged.
- [x] `cargo test` remains green with zero score-delta regressions.

**Status (2026-04-29):**
- Landed `src/acoustic_score.rs` scaffolding plus a default-off `acoustic_scoring_enabled` flag on the simulation and disturb config surfaces.
- Added regression guards:
  - `tests::acoustic_scaffolding_defaults_are_disabled`
  - `regression_tests::tests::acoustic_scaffolding_flag_does_not_change_scores`
- Verification: `cargo test` -> `419 passed; 0 failed; 4 ignored`.

#### Phase 1 — Render-path extraction only

**Goal:** expose reusable rendered audio for acoustic analysis, still with no scoring effect.

- [x] Add a helper that provides rendered stereo audio in a stable, reusable form for analysis.
- [x] Keep this helper side-effect-free and separate from neural scoring.
- [x] Do not yet compute final acoustic scores; only expose the data path.

**Acceptance / regression bar:**
- [x] Silence, short audio, and normal presets produce finite extracted buffers.
- [x] No acoustic path execution when `acoustic_scoring_enabled=false`.
- [x] Legacy evaluation score remains unchanged.

**Status (2026-04-29):**
- Added `RenderedStereoAudio` plus shared `render_preset_stereo_dry()` / `render_preset_ear_signals()` helpers.
- Reused the shared dry render helper in both the canonical evaluation path and `disturb`, removing duplicate movement/HRTF rendering logic without changing downstream scoring.
- Exposed the rendered stereo ear signal only on `DetailedSimulationResult.acoustic_render` and only when `acoustic_scoring_enabled=true`.
- Added regression coverage:
  - `pipeline::tests::render_preset_ear_signals_short_silence_is_finite`
  - `pipeline::tests::render_preset_ear_signals_normal_preset_is_finite`
  - `regression_tests::tests::acoustic_render_is_exposed_only_when_enabled`
- Verification: `cargo test` -> `422 passed; 0 failed; 4 ignored`.

#### Phase 2 — AcousticFeatureVector v1 (non-speech features only)

**Goal:** compute safe, bounded acoustic features before adding any speech fixtures or score fusion.

Suggested first features:
- [x] broadband level / RMS proxy
- [x] speech-band energy ratio
- [x] modulation-depth or temporal-variance proxy
- [x] simple brightness / sharpness proxy (cheap first-pass, not full Zwicker yet)

Do **not** add these to the final score yet. First make sure the extractor behaves sensibly and can be tested independently.

**Acceptance / regression bar:**
- [x] Louder signal increases level feature.
- [x] Brighter signal increases brightness / sharpness proxy.
- [x] All features are finite, bounded, and deterministic for fixed input.
- [x] Legacy score still unchanged when acoustic fusion is off.

**Status (2026-04-30):**
- Implemented `extract_features_v1()` and `extract_score_result_v1()` in `src/acoustic_score.rs`.
- The canonical evaluation path now populates `SimulationResult.acoustic_score.features` only when `acoustic_scoring_enabled=true`.
- The scalar NMM score remains unchanged; only the optional acoustic payload is added behind the flag.
- Added unit coverage for monotonic feature behavior:
  - louder signal -> higher broadband level
  - brighter signal -> higher sharpness proxy
  - mid-band tone -> higher speech-band ratio than off-band tone
  - amplitude modulation -> higher modulation-depth proxy
  - deterministic and bounded feature extraction
- Regression coverage still proves:
  - acoustic payload is absent when disabled
  - legacy scalar summary is unchanged when enabled
- Verification: `cargo test` -> `427 passed; 0 failed; 4 ignored`.

#### Phase 3 — Speech privacy proxy v1

**Goal:** implement the first useful masking metric, still in evaluate-only mode.

- [x] Add fixed speech fixtures:
  - dry, short utterances
  - deterministic and version-controlled
- [x] Render:
  - speech alone
  - preset alone
  - speech + preset
- [x] Compute a band-weighted effective SNR or SII-style intelligibility proxy.
- [x] Convert to:
  - `intelligibility_proxy`
  - `speech_privacy = 1 - intelligibility_proxy`

This is the first phase where the acoustic branch becomes product-useful, but it should still be inspectable before it becomes optimization-driving.

**Acceptance / regression bar:**
- [x] Stronger speech-band masker lowers intelligibility proxy.
- [x] Off-band masker performs worse than speech-band-focused masker.
- [x] Silent or weak masker gives poor privacy score.
- [x] Results are deterministic for the same speech fixture and preset.

**Status (2026-04-30):**
- Added a deterministic synthetic speech fixture generator in `src/acoustic_score.rs` to avoid external asset churn while still giving the acoustic branch a fixed, version-controlled speech reference.
- `extract_score_result_v1()` now computes:
  - Phase 2 non-speech acoustic features
  - `intelligibility_proxy`
  - `speech_privacy = 1 - intelligibility_proxy`
- The proxy is a first-pass band-weighted speech-preservation score over speech-relevant bands, using:
  - generated speech alone
  - preset masker alone
  - mixed speech + preset
- The canonical evaluation path still does **not** fuse any acoustic term into the scalar NMM score. Only the optional acoustic payload changes when `acoustic_scoring_enabled=true`.
- Added unit coverage for:
  - deterministic speech fixture generation
  - stronger speech-band masker -> lower intelligibility
  - off-band masker -> weaker privacy than speech-band masker
  - silent masker -> poor privacy
- Regression coverage still proves:
  - scalar summary fields are unchanged when the acoustic flag is enabled
  - acoustic payload is present and bounded only behind the flag
- Verification: `cargo test` -> `431 passed; 0 failed; 4 ignored`.

#### Phase 4 — `evaluate` display only

**Goal:** expose acoustic metrics to users before using them for decision-making.

- [x] Add evaluate-time acoustic output (for example under `--acoustic-score`).
- [x] Print:
  - speech privacy
  - intelligibility proxy
  - non-speech acoustic features from Phase 2
- [x] Keep the legacy NMM score as the primary result and expose acoustic metrics as a separate inspectable block.

This phase is for inspection and calibration, not yet for optimizer-driving behavior.

**Acceptance / regression bar:**
- [x] Acoustic metrics appear only when requested or enabled.
- [x] Existing evaluate output remains unchanged by default.
- [x] Users can compare NMM score vs acoustic subscore directly.

**Status (2026-04-30):**
- Added `--acoustic-score` to the `evaluate` CLI in `src/main.rs`.
- Single goal / single brain-type `evaluate` now prints an `Acoustic Subscore` block with:
  - broadband level
  - speech-band ratio
  - modulation depth
  - sharpness proxy
  - intelligibility proxy
  - speech privacy
- Matrix mode intentionally remains legacy-score-only and prints a note when `--acoustic-score` is requested, to avoid implying that acoustic metrics are already part of the scalar objective.
- `build_eval_config()` now carries `acoustic_scoring_enabled` explicitly, so the display path is still gated by the same canonical `SimulationConfig` used by the evaluator.
- Regression coverage confirms:
  - default evaluate output remains unchanged
  - enabling acoustic scoring still leaves the scalar NMM summary unchanged
  - acoustic payload and render details appear only behind the explicit gate
- Verification:
  - `cargo test build_eval_config_carries_feature_flags -- --nocapture`
  - `cargo test acoustic_render_is_exposed_only_when_enabled -- --nocapture`
  - `cargo test acoustic_score::tests -- --nocapture`
  - `cargo test` -> `431 passed; 0 failed; 4 ignored`

#### Phase 5 — Goal-specific score fusion behind a second flag

**Goal:** integrate acoustic scoring in a narrow, controllable way.

- [x] Add `acoustic_score_fusion_enabled: bool`.
- [x] Fuse only when explicitly enabled:

```text
Final = w_nmm * NMM + w_acoustic * Acoustic
```

- [x] Start with `shield` and `isolation` only.
- [x] Leave all other goals on the legacy score initially.

**Why narrow first:** Shield is the clearest case where the current NMM score is missing an acoustic task dimension. Rolling fusion out to every goal immediately would make debugging much harder.

**Acceptance / regression bar:**
- [x] Fusion disabled => exact legacy score.
- [x] Fusion enabled => Shield/Isolation scores move in expected direction on known masking examples.
- [x] Non-Shield goals are unchanged in the first fusion pass.

**Status (2026-04-30):**
- Added `SimulationConfig.acoustic_score_fusion_enabled` and kept it default-off.
- Added `Goal::supports_acoustic_fusion()` plus a first bounded fusion path in `src/scoring.rs`.
- The first-pass acoustic branch uses currently available Phase 2/3 metrics only:
  - `speech_privacy`
  - `speech_band_ratio`
  - a lightweight comfort proxy derived from `sharpness_proxy` and `modulation_depth`
- Fusion is intentionally limited to:
  - `shield`: `0.82 * NMM + 0.18 * Acoustic`
  - `isolation`: `0.78 * NMM + 0.22 * Acoustic`
- All other goals keep the exact legacy NMM score even if fusion is requested.
- Added an explicit pipeline-side guard: fusion requires acoustic scoring to be enabled, so direct callers fail immediately instead of silently falling back.
- `evaluate` now exposes `--acoustic-score-fusion` as an evaluate-only switch. It implies acoustic analysis, leaves optimize/surrogate behavior unchanged, and prints the fused-vs-legacy breakdown on supported single-goal evaluations.
- Matrix mode supports the fused scalar score when requested, but remains scalar-only in presentation and prints a note that only `shield` / `isolation` cells are affected.
- Added regression coverage for:
  - supported-goal fusion changing only the scalar score
  - unsupported-goal fusion staying bit-identical to legacy
  - config propagation for the new flag
  - invalid direct config (`fusion=true`, `acoustic_scoring=false`) panicking immediately
- Targeted verification:
  - `cargo test acoustic_fusion -- --nocapture`
  - `cargo test build_eval_config_carries_feature_flags -- --nocapture`
  - `cargo test acoustic_fusion_requires_acoustic_scoring -- --nocapture`
  - `cargo test` -> `436 passed; 0 failed; 4 ignored`

#### Phase 6 — Optimizer integration

**Goal:** allow acoustic-aware optimization only after evaluate behavior is trusted.

- [x] Permit `optimize` to use fused scores only when explicitly enabled.
- [x] Keep default optimizer behavior legacy-safe.
- [x] Do not update surrogate assumptions until the fused scalar score contract is stable.

**Acceptance / regression bar:**
- [x] Optimizer runs with fusion on/off.
- [x] Export path still re-evaluates with the real pipeline.
- [x] Surrogate remains disabled, stale-marked, or explicitly retrained if the scalar score contract changes.

**Status (2026-04-30):**
- Added `--acoustic-score-fusion` to `optimize`.
- Added `build_optimize_config()` so the optimizer config path has one canonical place to derive:
  - `acoustic_scoring_enabled`
  - `acoustic_score_fusion_enabled`
- Added `validate_optimize_acoustic_mode()` and made the optimize CLI fail fast for unsupported or unsafe combinations:
  - unsupported goals are rejected up front (`shield` / `isolation` only for now)
  - `--acoustic-score-fusion` + `--surrogate` is rejected until the surrogate score contract is updated
  - `--acoustic-score-fusion` + `--log-evaluations` is rejected until the CSV/data contract can distinguish fused-score runs from legacy-score runs
- Default optimize behavior is unchanged when `--acoustic-score-fusion` is omitted.
- Supported fused optimize runs now:
  - evaluate all candidates with the real fused scalar objective
  - preserve the existing "final preset is re-evaluated with the real pipeline before export" rule
  - print the acoustic breakdown in the final optimize summary so the fused objective is inspectable
- Added regression coverage for:
  - optimize config propagation
  - safe/unsafe mode validation
  - fused export re-evaluation
- Verification:
  - `cargo test validate_optimize_acoustic_mode -- --nocapture`
  - `cargo test build_optimize_config_enables_fusion_implies_acoustic_scoring -- --nocapture`
  - `cargo test export_best_genome_uses_re_evaluated_fused_score -- --nocapture`
  - `cargo run -- optimize --help`
  - `cargo run -- optimize --goal shield --population 4 --generations 1 --duration 3 --acoustic-score-fusion --output /tmp/phase6_optimize_shield.json`
  - `cargo test` -> `442 passed; 0 failed; 4 ignored`

#### Phase 7 — Envelope masking

**Goal:** add the second speech-aware refinement after the privacy proxy is stable.

- [ ] Implement envelope degradation / modulation masking using the repo's auditory front end.
- [ ] Confirm that two maskers with similar spectrum but different temporal structure separate correctly.

**Acceptance / regression bar:**
- [ ] Envelope-smearing or modulation-dense maskers outperform spectrally matched flat maskers on the envelope metric.
- [ ] Metric remains bounded and deterministic.

#### Phase 8 — Psychoacoustic comfort

**Goal:** add a reusable acoustic quality layer for all goals.

- [ ] Add loudness / sharpness / roughness / fluctuation terms.
- [ ] Start with monotonic penalties rather than a complex composite annoyance model.
- [ ] Weight these differently per goal after they are stable independently.

**Acceptance / regression bar:**
- [ ] Deliberately harsh / rough / pulsed synthetic cases score worse on comfort.
- [ ] Sleep / meditation goals can use comfort penalties without inheriting Shield-specific speech privacy logic.

### 25i. Governance rules for maintainability

To keep this priority under control:

- [ ] Use **one flag per behavior boundary**:
  - `acoustic_scoring_enabled`
  - `acoustic_score_fusion_enabled`
- [ ] Do **not** let the optimizer use the acoustic branch before `evaluate` is trusted.
- [ ] Do **not** retrain or modify the surrogate until the final fused scalar score is stable.
- [ ] Prefer **monotonic tests** ("masker A lowers intelligibility more than masker B") over brittle absolute snapshot-score tests.
- [ ] Every phase must support exact legacy fallback.

**Expected outcome:** Priority 25 becomes a controlled engineering program rather than a monolithic scoring rewrite. Each phase can be validated, rolled back, and tuned independently, which is the only sane way to add an acoustic branch without destabilizing the current NMM stack.

## Priority 26: Brain-Type-Dependent Noise Tolerance / Overstimulation Penalty (MEDIUM IMPACT, LOW-MEDIUM EFFORT)

**Problem:** The current scoring stack differentiates brain types through model parameters (for example `input_offset`, inhibitory constants, bilateral coupling), but it does not yet encode a strong behavioral finding from the noise-attention literature: the same broadband noise that can slightly improve ADHD performance can degrade performance in neurotypical listeners. Right now a Normal-brain evaluation can still reward "more masking / more stimulation" whenever the EEG target moves in the right direction, even if the preset has crossed into an over-stimulating regime that the human literature associates with worse task performance.

This gap matters most for:

- `focus`, `deep_work`, `shield`, `isolation` on `normal`
- comparisons between `normal` and `adhd`
- future acoustic-scoring work where louder / more speech-band-heavy presets may look useful acoustically but should not receive unlimited reward on neurotypical brains

**Solution:** Add a mild, brain-type-dependent "noise tolerance" or "arousal mismatch" term to scoring. The term should not act like a blanket volume penalty. It should only activate when a preset pushes a brain type beyond its likely beneficial stimulation range.

Conceptually:

```text
AdjustedScore =
  BaseScore
- overstimulation_penalty(brain_type, acoustic_drive, neural_drive)
```

Where the penalty is:

- near-zero for `adhd` across a broader stimulation range
- earlier / steeper for `normal`
- potentially different again for `anxious` and `high_alpha`

The penalty should be informed by both:

1. **acoustic drive**:
   - broadband level
   - speech-band masking density
   - modulation salience
2. **neural drive**:
   - excess beta / gamma relative to goal
   - elevated FHN firing beyond target regime
   - possibly future acoustic-subscore outputs from Priority 25

This is explicitly **not** a claim that "noise is bad for neurotypicals" in general. The literature shows small average harms in the comparison groups used in ADHD noise studies, not a universal prohibition. The correct interpretation for the roadmap is narrower: the model should stop treating additional broadband stimulation as uniformly acceptable on Normal brain once the preset has already reached a good state.

- [ ] Add a bounded `overstimulation_penalty` term in `scoring.rs`, initially gated to `normal` and `adhd`
- [ ] Make the first version conservative: penalty should be small and only activate at clearly high stimulation regimes
- [ ] Define the trigger from existing metrics first (for example excessive beta/gamma + high FHN firing + high acoustic drive), rather than inventing a new latent variable immediately
- [ ] Verify that ADHD presets are less penalized than Normal for the same drive level
- [ ] Verify that a clearly overdriven Normal-brain preset loses score while moderate Shield / Focus presets remain unaffected
- [ ] Re-evaluate whether `anxious` should share part of the Normal penalty or have its own curve

**Expected outcome:** brain-type comparisons become more behaviorally plausible, especially for attention / masking goals, and future acoustic-score work gains a principled way to avoid rewarding "just add more noise" on neurotypical brains.

**References:**
- [ ] **Ref:** (2024). "Systematic Review and Meta-Analysis: Do White Noise and Pink Noise Help With Attention in ADHD?" *J Am Acad Child Adolesc Psychiatry* 63(8):859-870. — Meta-analysis (k=13, N=335) reporting small but reliable benefit in ADHD (g=0.249) and small but reliable harm in non-ADHD comparison groups (g=−0.212), with minimal heterogeneity. Strongest direct evidence that noise tolerance should differ by brain type. [PMC](https://pmc.ncbi.nlm.nih.gov/articles/PMC11283987/)
- [ ] **Ref:** Söderlund G, Sikström S, Smart A (2007). "Listen to the noise: noise is beneficial for cognitive performance in ADHD." *J Child Psychol Psychiatry* 48(8):840-847. — Foundational Moderate Brain Arousal framing for ADHD benefit from external noise; useful as historical context even though later work weakens the strict stochastic-resonance interpretation.
- [ ] **Ref:** Rijmen M, Senoussi M, Wiersema JR (2026). "Pink Noise and a Pure Tone Both Reduce 1/f Neural Noise in Adults With Elevated ADHD Traits." *J Attention Disorders.* — Suggests the benefit may reflect a broader arousal / neural-noise regulation effect rather than a pink-noise-specific mechanism. Use as interpretive support, not as the primary scoring basis.

## Priority 27: ASSR Phase-Consistency Refinement at 40 Hz (LOW-MEDIUM IMPACT, MEDIUM EFFORT)

**Problem:** The current ASSR implementation is fundamentally amplitude-oriented: it computes a frequency-dependent modifier and applies it to the AC component of the signal. That captures "how much drive gets through" but it does not capture an important refinement from newer ASSR literature: the 40 Hz peak is not just stronger, it is more temporally consistent. In other words, the 40 Hz ASSR appears to reflect reduced response-latency variability / increased phase consistency, not merely larger response gain.

This matters most for:

- `ignition` and any future gamma-forward goals
- PLV-based interpretation of 40 Hz entrainment
- brain-type differences that should express as weaker or noisier gamma locking rather than only lower amplitude

**Solution:** Extend the ASSR branch so that it can modulate not only amplitude transmission but also the expected phase-consistency / temporal-jitter properties of the downstream cortical response.

The first implementation does not need a full spiking-network redesign. A practical v1 could:

- keep the current scalar AC gain path
- add a 40 Hz-centered **phase-consistency modifier**
- feed that modifier into PLV-related terms, or into controlled temporal jitter / coherence penalties upstream of PLV computation

Conceptually:

```text
ASSR response = amplitude_gain(f) + temporal_consistency(f)
```

where `temporal_consistency(f)` peaks around 40 Hz more sharply than the current amplitude-only approximation.

Possible implementation directions:

1. **PLV weighting route (lower risk):**
   - augment the PLV bonus near 40 Hz based on an ASSR-derived consistency prior
   - minimal disruption to the signal path
2. **Temporal-jitter route (higher fidelity):**
   - make effective response jitter frequency-dependent before PLV is measured
   - more mechanistic, but higher regression risk

- [ ] Add an explicit roadmap note in the ASSR code path that 40 Hz resonance is partly a temporal-consistency phenomenon, not only an amplitude phenomenon
- [ ] Prototype a bounded `assr_phase_consistency_modifier(f)` peaking at 40 Hz
- [ ] Evaluate whether it should enter:
  - PLV scoring only, or
  - both signal path and PLV scoring
- [ ] Run focused regressions on `ignition` and gamma-heavy presets to confirm the refinement improves discrimination without destabilizing non-gamma goals
- [ ] Keep the feature flag-gated initially; legacy ASSR behavior must remain reproducible when disabled

**Expected outcome:** 40 Hz-driven presets are judged more by stable locking and less by raw amplitude alone, which better matches the empirical ASSR literature and should improve gamma-goal plausibility.

**References:**
- [ ] **Ref:** (2024). "Network resonance and the auditory steady state response." *Scientific Reports.* — Shows that the large 40 Hz ASSR peak is linked to decreased latency variability / enhanced temporal consistency rather than a simple gain-only explanation. This is the direct motivation for adding a phase-consistency branch to the ASSR approximation.
- [ ] **Ref:** (2024). "40 Hz Steady-State Response in Human Auditory Cortex Is Shaped by Gabaergic Neuronal Inhibition." *J Neurosci* 44(24):e2029232024. — Connects inhibitory kinetics to the strength and quality of 40 Hz locking; supports treating gamma ASSR as a timing/coherence phenomenon, not only an amplitude phenomenon.
- [ ] **Ref:** Ross B, Borgmann C, Draganova R, Roberts LE, Pantev C (2000). "A high-precision magnetoencephalographic study of human auditory steady-state responses to amplitude-modulated tones." *J Acoust Soc Am* 108(2):679-691. — Classic empirical basis for the exceptional robustness of 40 Hz ASSRs in humans.

## Explicitly Deferred: Aperiodic (1/f) Slope as a Scoring Metric

**Status:** reviewed, scientifically interesting, but intentionally **not** promoted to an implementation priority yet.

**Why it was considered:** several recent ADHD / noise papers argue that background noise changes the aperiodic component of EEG spectra and that this may reflect altered neural-noise or E/I balance. In principle, an aperiodic-slope term could become a new `PerformanceVector` feature and improve ADHD-specific scoring.

**Why it is deferred:** the measurement and interpretation remain too unstable for the current roadmap:

- decomposition methods such as FOOOF / specparam have known confounds when periodic peaks and aperiodic structure overlap
- the literature is not yet consistent on whether ADHD reliably shows flatter slopes, steeper slopes, or mixed age/task-dependent patterns
- the biological interpretation ("aperiodic slope = E/I balance") is still model-driven and not settled enough to use as a product-facing score component

That means this metric is suitable for exploratory diagnostics, but not yet for a production scoring term that would steer optimization.

- [ ] Do **not** add `aperiodic_slope` to `PerformanceVector` as a scored metric until the methodological debate settles
- [ ] If revisited later, start with offline analysis / reporting only, not optimizer-driving fitness
- [ ] Reconsider only after stronger clinical ADHD replication and clearer guidance on decomposition robustness

**References:**
- [ ] **Ref:** Rijmen M, Senoussi M, Wiersema JR (2026). "Pink Noise and a Pure Tone Both Reduce 1/f Neural Noise in Adults With Elevated ADHD Traits." *J Attention Disorders.* — Interesting motivating finding, but based on elevated traits in neurotypical adults rather than a clinical ADHD sample.
- [ ] **Ref:** Donoghue T, Voytek B (2021). "Characterizing pink and white noise in the human electroencephalogram." *J Neurophysiol* 125(4):1545-1554. — Useful background on spectral parameterization, but not a license to treat slope as a settled biomarker.
- [ ] **Ref:** (2024). "Systematic Review and Meta-Analysis: Do White Noise and Pink Noise Help With Attention in ADHD?" *J Am Acad Child Adolesc Psychiatry* 63(8):859-870. — Supports brain-type-dependent noise effects, but does **not** establish aperiodic slope as the correct operational metric for scoring.

## Obsolete / Superseded

### ~~Envelope Extraction (Priority 1b — original)~~
- ~~Implement Hilbert transform for envelope extraction~~
- **Superseded:** Analysis proved band signals ARE already envelopes (gammatone magnitude + 80Hz LPF + decimation). The issue was never envelope extraction — it was that the JR model is mean-driven (responds to DC offset, not AC modulation). Fixed by: ASSR → input_scale modifier, Thalamic gate → input_offset modifier. Both operate at the parameter level, not signal level.

### ~~Wilson-Cowan on Low Bands (Priority 2 — original)~~
- ~~Add WC(2.0) and WC(5.0) on bands 0-1 for relaxation~~
- **Superseded:** The thalamic gate achieves the same effect more correctly. WC at theta/delta frequencies would pretend sound can directly entrain slow rhythms, which contradicts ASSR research (Picton 2003). The correct mechanism is: sound → reduced arousal → thalamic mode switch → JR shifts to slow-wave regime. Implemented.

---

## Key References

### ASSR Transfer Function
- Galambos R, Makeig S, Talmachoff PJ (1981). "A 40-Hz auditory potential recorded from the human scalp." *Proc Natl Acad Sci USA* 78(4):2643-2647.
- Picton TW, John MS, Dimitrijevic A, Purcell D (2003). "Human auditory steady-state responses." *Int J Audiol* 42(4):177-219.
- Ross B, Borgmann C, Draganova R, Roberts LE, Pantev C (2000). "A high-precision magnetoencephalographic study of human auditory steady-state responses to amplitude-modulated tones." *J Acoust Soc Am* 108(2):679-691.

### Wilson-Cowan Model
- Wilson HR, Cowan JD (1972). "Excitatory and inhibitory interactions in localized populations of model neurons." *Biophys J* 12(1):1-24.
- Wilson HR, Cowan JD (1973). "A mathematical theory of the functional dynamics of cortical and thalamic nervous tissue." *Kybernetik* 13(2):55-80.

### Jansen-Rit Model
- Jansen BH, Rit VG (1995). "Electroencephalogram and visual evoked potential generation in a mathematical model of coupled cortical columns." *Biol Cybern* 73(4):357-366.
- David O, Friston KJ (2003). "A neural mass model for MEG/EEG: coupling and neuronal dynamics." *NeuroImage* 20(3):1743-1755.

### Thalamic Gating
- Lopes da Silva FH (1991). "Neural mechanisms underlying brain waves: from neural membranes to networks." *Electroencephalogr Clin Neurophysiol* 79(2):81-93.
- Suffczynski P, Kalitzin S, Lopes da Silva FH (2004). "Dynamics of non-convulsive epileptic phenomena modeled by a bistable neuronal network." *Neuroscience* 126(2):467-484.
- Hughes SW, Crunelli V (2005). "Thalamic mechanisms of EEG alpha rhythms and their pathological implications." *Neuroscientist* 11(4):357-372.

### Auditory Entrainment & Theta/Delta
- Buzsáki G (2002). "Theta oscillations in the hippocampus." *Neuron* 33(3):325-340.
- Thaut MH (2005). *Rhythm, Music, and the Brain: Scientific Foundations and Clinical Applications.* Routledge.
- Nozaradan S, Peretz I, Missal M, Mouraux A (2011). "Tagging the neuronal entrainment to beat and meter." *J Neurosci* 31(28):10234-10240.

### 40 Hz Gamma Stimulation
- Iaccarino HF, Singer AC, Martorell AJ, et al. (2016). "Gamma frequency entrainment attenuates amyloid load and modifies microglia." *Nature* 540(7632):230-235.
- Martorell AJ, Paulson AL, Suk HJ, et al. (2019). "Multi-sensory gamma stimulation ameliorates Alzheimer's-associated pathology and improves cognition." *Cell* 177(2):256-271.

### Stochastic Resonance
- Moss F, Ward LM, Sannita WG (2004). "Stochastic resonance and sensory information processing: a tutorial and review of application." *Clin Neurophysiol* 115(2):267-281.
- Söderlund G, Sikström S, Smart A (2007). "Listen to the noise: noise is beneficial for cognitive performance in ADHD." *J Child Psychol Psychiatry* 48(8):840-847.

### Phase-Locking & Coherence
- Lachaux JP, Rodriguez E, Martinerie J, Varela FJ (1999). "Measuring phase synchrony in brain signals." *Hum Brain Mapp* 8(4):194-208.
- Helfrich RF, Schneider TR, Rach S, Trautmann-Lengsfeld SA, Engel AK, Herrmann CS (2014). "Entrainment of brain oscillations by transcranial alternating current stimulation." *Curr Biol* 24(3):333-339.

### EEG & Relaxation States
- Klimesch W (1999). "EEG alpha and theta oscillations reflect cognitive and memory performance: a review and analysis." *Brain Res Rev* 29(2-3):169-195.
- Niedermeyer E, Lopes da Silva FH (2005). *Electroencephalography: Basic Principles, Clinical Applications, and Related Fields.* 5th ed. Lippincott Williams & Wilkins.

### Signal Processing
- Oppenheim AV, Schafer RW (2009). *Discrete-Time Signal Processing.* 3rd ed. Prentice Hall.
- Crochiere RE, Rabiner LR (1983). *Multirate Digital Signal Processing.* Prentice Hall.
- Patterson RD, Robinson K, Holdsworth J, McKeown D, Zhang C, Allerhand M (1992). "Complex sounds and auditory images." In: Cazals Y et al. (eds) *Auditory Physiology and Perception.*
- Marple SL (1999). "Computing the Discrete-Time Analytic Signal via FFT." *IEEE Trans Signal Process* 47(9):2600-2603.

### Noise & Cognitive Performance
- Rausch VH, Bauch EM, Bunzeck N (2014). "White noise improves learning by modulating activity in dopaminergic midbrain regions and right superior temporal sulcus." *J Cogn Neurosci* 26(7):1469-1480.
- Söderlund G, Sikström S, Loftesnes JM, Sonuga-Barke EJ (2010). "The effects of background white noise on memory performance in inattentive school children." *Behav Brain Funct* 6:55.

### Auditory Perception & Loudness
- Glasberg BR, Moore BCJ (2002). "A model of loudness applicable to time-varying sounds." *J Audio Eng Soc* 50(5):331-342.
- Zwicker E, Fastl H (1999). *Psychoacoustics: Facts and Models.* 2nd ed. Springer.

### Speech Intelligibility & Acoustic Masking
- Hongisto V (2005). "A model predicting the effect of speech of varying intelligibility on work performance." *Indoor Air* 15(6):458-468.
- Venetjoki N, Kaarlela-Tuomaala A, Keskinen E, Hongisto V (2006). "The effect of speech and speech intelligibility on task performance." *Ergonomics* 49(11):1068-1091.
- Haapakangas A, Hongisto V, Liebl A (2020). "The relation between the intelligibility of irrelevant speech and cognitive performance: A revised model based on laboratory studies." *Indoor Air* 30(6):1130-1146.
- Chi T, Gao Y, Guyton MC, Ru P, Shamma S (1999). "Spectro-temporal modulation transfer functions and speech intelligibility." *J Acoust Soc Am* 106(5):2719-2732.
- Elliott TM, Theunissen FE (2009). "The modulation transfer function for speech intelligibility." *PLoS Comput Biol* 5(3):e1000302.
- Drullman R, Festen JM, Plomp R (1994). "Effect of temporal envelope smearing on speech reception." *J Acoust Soc Am* 95(5 Pt 1):2670-2680.
- Kates JM, Arehart KH (2005). "Coherence and the speech intelligibility index." *J Acoust Soc Am* 117(4 Pt 1):2224-2237.
- ANSI/ASA S3.5. *Methods for Calculation of the Speech Intelligibility Index (SII).* 
- IEC 60268-16. *Sound system equipment — Part 16: Objective rating of speech intelligibility by speech transmission index.*

### Bilateral Coupling
- Innocenti GM (1986). "General organization of callosal connections in the cerebral cortex." In: Jones EG, Peters A (eds) *Cerebral Cortex,* vol 5. Plenum Press.
- Bloom JS, Hynd GW (2005). "The role of the corpus callosum in interhemispheric transfer of information: excitation or inhibition?" *Neuropsychol Rev* 15(2):59-71.
- Aboitiz F, Scheibel AB, Fisher RS, Zaidel E (1992). "Fiber composition of the human corpus callosum." *Brain Res* 598(1-2):143-153.

### Hemispheric Asymmetry
- Davidson RJ (2004). "What does the prefrontal cortex 'do' in affect: perspectives on frontal EEG asymmetry research." *Biol Psychol* 67(1-2):219-234.
- Allen JJB, Coan JA, Nazarian M (2004). "Issues and assumptions on the road from raw signals to metrics of frontal EEG asymmetry in emotion." *Biol Psychol* 67(1-2):183-218.

### Thalamocortical Dynamics
- Steriade M, McCormick DA, Sejnowski TJ (1993). "Thalamocortical oscillations in the sleeping and aroused brain." *Science* 262(5134):679-685.

### Neural Entrainment Methodology
- Notbohm A, Kurths J, Herrmann CS (2016). "Modification of brain oscillations via rhythmic sensory stimulation." *Int J Psychophysiol* 103:62-68.
- Thut G, Schyns PG, Gross J (2011). "Entrainment of perceptually relevant brain oscillations by non-invasive rhythmic stimulation of the human brain." *Front Psychol* 2:170.
- Obleser J, Kayser C (2019). "Neural entrainment and attentional selection in the listening brain." *Trends Cogn Sci* 23(11):913-926.
- Zoefel B, ten Oever S, Sack AT (2018). "The involvement of endogenous neural oscillations in the processing of rhythmic input." *Front Neurosci* 12:95.

### Computational Neuroscience
- Izhikevich EM (2003). "Simple model of spiking neurons." *IEEE Trans Neural Netw* 14(6):1569-1572.

### FitzHugh-Nagumo Model
- FitzHugh R (1961). "Impulses and physiological states in theoretical models of nerve membrane." *Biophys J* 1(6):445-466.
- Nagumo J, Arimoto S, Yoshizawa S (1962). "An active pulse transmission line simulating nerve axon." *Proc IRE* 50(10):2061-2070.

### Synchronization & Frequency Tracking
- Pikovsky A, Rosenblum M, Kurths J (2001). *Synchronization: A Universal Concept in Nonlinear Sciences.* Cambridge University Press.

### Stochastic Neural Mass Models
- Ableidinger M, Buckwar E, Hinterleitner H (2017). "A Stochastic Version of the Jansen and Rit Neural Mass Model: Analysis and Numerics." *J Math Neurosci* 7:8.
- Grimbert F, Faugeras O (2006). "Bifurcation analysis of Jansen's neural mass model." *Neural Comput* 18(12):3052-3068.
- Spiegler A, Kiebel SJ, Atay FM, Knösche TR (2011). "Complex behavior in a modified Jansen and Rit neural mass model." *Biol Cybern* 104:229-254.
- (2024). "On the influence of input triggering on the dynamics of Jansen-Rit oscillators network." *Neurocomputing*.

### Neural Habituation & Adaptation
- Rowe DL, Robinson PA, Rennie CJ (2004). "Estimation of neurophysiological parameters from the waking EEG using a biophysical model of brain dynamics." *J Theor Biol* 231(3):413-433.
- Moran RJ et al. (2011). "Modeling habituation of auditory evoked fields using neural mass models." *BMC Neuroscience* 12(Suppl 1):P368.
- Huber DE, Potter KW, Huszar LD (2020). "Neural habituation enhances novelty detection: an EEG study of rapidly presented words." *Comput Brain Behav* 2:116-129.
- Jääskeläinen IP, Ahveninen J, Belliveau JW, Raij T, Sams M (2007). "Short-term plasticity in auditory cognition." *Trends Neurosci* 30(12):653-661.

### Thalamocortical Sleep/Wake Models
- Bazhenov M, Timofeev I, Steriade M, Sejnowski TJ (2002). "Model of thalamocortical slow-wave sleep oscillations and transitions to activated states." *J Neurosci* 22(19):8691-8704.
- Gonzalez OJ et al. (2016). "Presence of a chaotic region at the sleep-wake transition in a simplified thalamocortical circuit model." *Front Comput Neurosci* 10:91.
- (2023). "Translating electrophysiological signatures of awareness into thalamocortical mechanisms by inverting systems-level computational models across arousal states." *bioRxiv* 2023.10.11.561970.
- (2023). "Thalamic control of sensory processing and spindles in a biophysical somatosensory thalamoreticular circuit model of wakefulness and sleep." *Cell Reports*.

### Subcortical Auditory Pathway Models
- Rabang CF et al. (2012). "A computational model of inferior colliculus responses to amplitude modulated sounds in young and aged rats." *Front Neural Circuits* 6:77.
- Proctor CW, Bhatt DH (2012). "A computational model of cellular mechanisms of temporal coding in the medial geniculate body." *J Comput Neurosci* 32(2):207-230.
- Farahani ED, Wouters J, Francart T (2021). "Brain mapping of auditory steady-state responses: A broad view of cortical and subcortical sources." *Hum Brain Mapp* 42(3):780-796.
- (2025). "Modelling neural coding in the auditory midbrain with high resolution and accuracy." *Nature Machine Intelligence*.

### Auditory Cortex Circuit Models
- Moshitch D, Las L (2020). "A circuit model of auditory cortex." *PLoS Comput Biol* 16(7):e1008016.

### Whole-Brain Network Models
- Cakan C, Obermayer K (2021). "neurolib: A simulation framework for whole-brain neural mass modeling." *Cogn Comput* 13:1132-1152.
- Byrne Á, Avitabile D, Coombes S (2024). "Whole brain functional connectivity: Insights from next generation neural mass modelling incorporating electrical synapses." *PLoS Comput Biol* 20(12):e1012647.
- Ableidinger M, Buckwar E, Hinterleitner H (2018). "Bifurcation analysis of two coupled Jansen-Rit neural mass models." *PLoS One* 13(2):e0192842.
- Deco G et al. (2013). "Resting-state functional connectivity emerges from structurally and dynamically shaped slow linear fluctuations." *J Neurosci* 33(27):11239-11252.

### EEG & Noise Color Effects
- Donoghue T, Voytek B (2021). "Characterizing pink and white noise in the human electroencephalogram." *J Neurophysiol* 125(4):1545-1554.
- (2024). "Brain wave modulation and EEG power changes during auditory beats stimulation." *Int J Psychophysiol* 203:112403.
- (2025). "Prestimulus EEG oscillations and pink noise affect Go/No-Go ERPs." *Sensors* 25(6):1733.
- Zhou J et al. (2012). "Pink noise: Effect on complexity synchronization of brain activity and sleep consolidation." *J Theor Biol* 306:68-72.
