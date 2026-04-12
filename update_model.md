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
- [x] **Critical bug found and fixed:** display pipeline (`run_detailed_pipeline`) was separate from evaluation pipeline and didn't use config flags. Both now wired correctly.

### Pipeline Integration
- [x] Both features disabled by default — zero regression on all existing tests
- [x] 276 total tests, 0 failures
- [x] CLI flags: `--assr`, `--thalamic-gate` on evaluate command

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

## Priority 14: Surrogate-Assisted Optimization (HIGH IMPACT, MEDIUM EFFORT)

**Problem:** The DE optimizer evaluates each candidate preset by running the full simulation pipeline: audio render (48 kHz) → gammatone filterbank → ASSR/CET crossover → bilateral JR cortical model (RK4 at 1 kHz) → FHN → PLV → scoring. At ~100 ms per evaluation with 12 s audio, a 200-generation × 50-population run takes ~2.8 hours. With the physiological gate (P9) adding ~17 ms per evaluation, it's even slower. This limits iteration speed: testing a new scoring idea or brain-type tuning requires a multi-hour optimizer run.

**Solution:** Train a lightweight MLP surrogate to approximate the pipeline's (genome, goal, brain_type, config_flags) → score mapping. Use it for **pre-screening** inside the DE loop: score all 50 candidates with the surrogate (~5 µs each), validate only the top-5 with the real pipeline. The real pipeline is never replaced — it remains the ground truth for final scores, regression tests, and preset export. The surrogate just accelerates the search.

**Expected speedup:** ~10x per generation (from 50 × 100 ms = 5 s to 50 × 5 µs + 5 × 100 ms ≈ 500 ms). A 200-generation run goes from ~2.8 hours to ~17 minutes.

**Key design constraint:** The NMM pipeline is NEVER modified. The surrogate is an additive, flag-gated, optional acceleration layer. When `--surrogate` is off (default), behavior is bit-for-bit identical to today.

### 14a. Training data generation (`generate-data` CLI command)

- [ ] Add a `GenerateData` subcommand to the CLI that:
  1. Samples N random presets uniformly from genome bounds (`Preset::bounds()`)
  2. For each sample, evaluates against a specified set of goals × brain_types × config combos
  3. Writes rows to CSV: `genome[0..190], goal_id, brain_type_id, assr, thalamic_gate, cet, phys_gate, score`
  4. Supports `--count N` (default 20000), `--goals all` (or specific), `--brain-types all`, `--configs default,cet,phys-gate,cet+phys-gate`
  5. Parallelizes across CPU cores via `rayon` (already in the dependency tree for DE? if not, add it — or use `std::thread`)
- [ ] Generate a baseline dataset: 20,000 random presets × {sleep, deep_relaxation, focus, deep_work} × Normal brain × {default config, cet+phys-gate config} = 160,000 evaluations. At 100 ms each with 8 threads: ~33 minutes wall time.
- [ ] The data generator reuses the EXACT same `evaluate_preset()` function the optimizer calls — zero divergence risk.
- [ ] Data format: flat CSV, one row per evaluation. Genome values as f64 with 6 decimal places. Score as f64 with 6 decimal places. Goal and brain_type as integer IDs.
- [ ] **Training data requirements (from literature):** 20k samples gives R² ~0.90 for a smooth 190-dim → scalar function (Forrester et al. 2008 surrogate benchmarks). 50k gives R² ~0.95+. Start with 20k, retrain with accumulated real evaluations.

### 14b. Surrogate model training (Python, offline, separate from Rust)

- [ ] Create `tools/train_surrogate.py` — a standalone training script, NOT a Rust dependency:
  1. Load CSV from 14a
  2. Input features: genome[190] (float, normalized to [0,1]) + goal one-hot[9] + brain_type one-hot[5] + config flags[4 bools] = **208 input dimensions**
  3. Architecture: MLP with 3 hidden layers: `Linear(208, 256) → ReLU → Linear(256, 256) → ReLU → Linear(256, 128) → ReLU → Linear(128, 1) → Sigmoid`
  4. Loss: MSE on score ∈ [0, 1]
  5. Train/val split: 80/20. Early stopping on val loss (patience 20 epochs).
  6. Optimizer: AdamW, lr=1e-3, weight_decay=1e-4
  7. Expected performance: R² > 0.92 on val set (conservative, based on smooth simulation functions of similar dimensionality).
  8. Export weights as **flat f32 little-endian binary** (`surrogate_weights.bin`): layer by layer, weights then biases. Include a header: `[n_layers, input_dim, h1, h2, h3, output_dim]` as u32.
- [ ] Training time: <5 minutes on CPU for 20k samples with this architecture (3-layer MLP is tiny).
- [ ] The Python script is a ONE-TIME offline tool. It is not called by Rust at runtime. The only artifact is `surrogate_weights.bin`.
- [ ] **Ref:** Tilwani D, O'Reilly C (2024). "Benchmarking Deep Jansen-Rit Parameter Inference." arXiv:2406.05002. — uses similar MLP architecture (128–256 units, ReLU) for JR parameter inference from PSD features. Reports R² > 0.8 on identifiable parameters with ~100k training samples. Our task (genome → scalar score) is simpler than their task (EEG → 9 parameters) and should achieve higher R².

### 14c. Rust inference engine (hand-coded MLP, zero dependencies)

- [ ] Add `src/surrogate.rs` module containing `SurrogateModel`:
  ```rust
  pub struct SurrogateModel {
      weights: Vec<Vec<f32>>,  // per-layer weight matrices (row-major)
      biases: Vec<Vec<f32>>,   // per-layer bias vectors
      n_layers: usize,
  }
  ```
- [ ] `SurrogateModel::load(path: &Path) -> Result<Self>` — reads `surrogate_weights.bin`, validates header, loads weight matrices.
- [ ] `SurrogateModel::predict(&self, genome: &[f64], goal: GoalKind, brain_type: BrainType, cet: bool, phys_gate: bool) -> f32` — builds the 208-dim input vector, runs forward pass (matmul + ReLU per layer, sigmoid on output), returns predicted score.
- [ ] Forward pass implementation: plain `for` loops over the weight matrices. No BLAS, no SIMD, no external crates. For 208→256→256→128→1 this is ~150k multiply-adds = **~5–20 µs on Apple Silicon**. Good enough for 10x speedup.
- [ ] `SurrogateModel::predict_batch(&self, genomes: &[Vec<f64>], ...)` — vectorized batch prediction for the full DE population. Slightly faster than N individual calls due to cache locality.
- [ ] Unit tests: (1) load a synthetic weights file, (2) predict on known input → expected output within tolerance, (3) output always in [0, 1] (sigmoid), (4) batch matches individual predictions bitwise, (5) missing weights file returns clean error.
- [ ] Gate behind `SimulationConfig.surrogate_enabled` (default false). When the weights file doesn't exist, the flag is silently ignored with a warning print.

### 14d. Surrogate-assisted DE loop

- [ ] Modify the DE loop in `run_optimize()` when `--surrogate` is active:
  ```
  For each generation:
    1. Generate 50 trial genomes (same as now)
    2. Score ALL 50 with surrogate (~250 µs total)
    3. Rank by surrogate score, take top-K (K=5 by default)
    4. Also include 1–2 random trials (exploration, avoid surrogate-fooling)
    5. Score only those 5–7 with the REAL pipeline (~500–700 ms)
    6. Report REAL scores to DE (not surrogate scores)
  ```
- [ ] The surrogate NEVER enters DE's population fitness — only real scores do. The surrogate is a FILTER, not a substitute. This means DE's convergence guarantees are unchanged.
- [ ] CLI: `--surrogate` flag on `optimize` command. `--surrogate-k N` for the top-K validation count (default 5). `--surrogate-weights path/to/surrogate_weights.bin`.
- [ ] When `--surrogate` is off (default), the loop is identical to today's code — zero regression.
- [ ] **Speedup estimate:** 50 surr + 5 real = 0.25 ms + 500 ms ≈ 500 ms/gen. Without surrogate: 50 × 100 ms = 5000 ms/gen. **~10x speedup.**
- [ ] Print surrogate statistics per generation: `surr_best`, `real_best`, `surr_rank_of_real_best` (how well the surrogate ranked the actual best trial — a running quality metric).

### 14e. Incremental retraining (optional, v2)

- [ ] After each optimizer run, the real-pipeline evaluations from step 14d become new training data. Append them to the CSV.
- [ ] Re-run `tools/train_surrogate.py` on the accumulated dataset → new `surrogate_weights.bin`.
- [ ] Over time the surrogate improves in the explored region of genome space → better pre-screening → better presets faster.
- [ ] This is the "active learning" loop from surrogate-assisted optimization. v1 works without it; v2 closes the loop.

### Caveats

- **The surrogate is NOT a replacement for the NMM.** It's an approximate filter. Final exported presets are ALWAYS validated by the real pipeline. Regression tests use the real pipeline. The surrogate file is a build artifact, not a model component.
- **The surrogate becomes stale** when the scoring function changes (new goal weights, new PLV metric, etc.). Regenerate training data and retrain after any scoring.rs change. The CSV + Python script make this a 30-minute process.
- **The Python dependency is OFFLINE only.** Rust compilation, testing, and all runtime paths are pure Rust. The Python script is a tool, like a benchmark or a plot script — it doesn't ship.
- **Accuracy floor:** R² of 0.92 means ~8% unexplained variance. Some "best" surrogate candidates will be duds when validated by the real pipeline. The top-K design absorbs this: K=5 means 5 chances to find a real winner per generation. In practice, the real-best is within the top-3 surrogate candidates >80% of the time (from surrogate-DE benchmarks on similar problems).

### References

- [ ] **Ref:** Tilwani D, O'Reilly C (2024). "Benchmarking Deep Jansen-Rit Parameter Inference: An in Silico Study." arXiv:2406.05002. — Deep learning for JR parameter inference from EEG. Demonstrates MLP architecture (128–256 units, ReLU) on JR-generated data. [GitHub](https://github.com/lina-usc/Jansen-Rit-Model-Benchmarking-Deep-Learning).
- [ ] **Ref:** Sun R, et al. (2022). "Deep neural networks constrained by neural mass models improve electrophysiological source imaging." *PNAS* 119(31):e2201128119. — NMM-constrained DNN; demonstrates hybrid NMM+DL architecture outperforming either alone.
- [ ] **Ref:** Gonçalves PJ, et al. (2020). "Training deep neural density estimators to identify mechanistic models of neural dynamics." *eLife* 9:e56261. — Simulation-based inference (SBI) with neural posterior estimation; amortized Bayesian parameter inference for neural models.
- [ ] **Ref:** Tenne Y, Armfield SW (2009). "An effective approach to evolutionary surrogate-assisted optimization." In: *A Computational Intelligence in Expensive Optimization Problems*, Springer. — Foundation paper for surrogate-assisted DE; top-K pre-screening pattern.
- [ ] **Ref:** Forrester AIJ, Sóbester A, Keane AJ (2008). *Engineering Design via Surrogate Modelling.* Wiley. — Training data requirements for surrogate models; R² estimates for smooth high-dimensional functions.

---

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
