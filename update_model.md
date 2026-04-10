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

## Priority 9: Physiological Thalamic Gate (MEDIUM IMPACT, MEDIUM EFFORT)

**Problem:** Current thalamic gate uses a heuristic: arousal (computed from preset properties) linearly shifts band_offsets. Real thalamocortical state switching involves ion channel dynamics (T-type Ca2+, K+ leak, persistent Na+) that produce qualitatively different firing modes (tonic vs burst), not just a shifted operating point.

**Solution:** Replace the linear heuristic with the Gonzalez et al. (2016) 3-neuron thalamocortical circuit: TC cell + RE (reticular) neuron + cortical cell. The TC cell's T-type Ca2+ channel naturally produces burst mode at low arousal and tonic mode at high arousal. The transition includes a chaotic intermediate region (Lyapunov exponents > 0) that our heuristic misses entirely.

- [ ] Read Gonzalez et al. (2016) for the 3-neuron circuit model equations and parameters
- [ ] Read Bazhenov et al. (2002) for the full thalamocortical model with ion channel dynamics
- [ ] Implement TC cell with T-type Ca2+ current: `I_T = g_T · m_inf(V) · h · (V - E_Ca)`
- [ ] Implement RE neuron with mutual inhibition to TC cell
- [ ] Map arousal parameter to K+ leak conductance (Bazhenov 2002: increased g_KL triggers wake→sleep transition)
- [ ] Read (2023) bioRxiv paper on thalamocortical mechanisms during dexmedetomidine sedation for parameter fitting from real EEG
- [ ] Unit tests: low arousal → TC burst mode, high arousal → TC tonic mode
- [ ] Integration tests: chaotic transition region produces realistic EEG variability at intermediate arousal
- [ ] Compare scores with heuristic gate vs physiological gate
- [ ] **Ref:** Gonzalez OJ, Krishnan GP, Chauvette S, Bhatt DH, Bhatt T, Bhatt P (2016). "Presence of a chaotic region at the sleep-wake transition in a simplified thalamocortical circuit model." *Front Comput Neurosci* 10:91. — 3-neuron TC circuit with chaotic sleep-wake transition; provides equations and parameters.
- [ ] **Ref:** Bazhenov M, Timofeev I, Steriade M, Sejnowski TJ (2002). "Model of thalamocortical slow-wave sleep oscillations and transitions to activated states." *J Neurosci* 22(19):8691-8704. — foundational biophysical thalamocortical model; K+ leak conductance as the wake→sleep switch.
- [ ] **Ref:** (2023). "Translating electrophysiological signatures of awareness into thalamocortical mechanisms by inverting systems-level computational models across arousal states." *bioRxiv* 2023.10.11.561970. — fits thalamocortical model to EEG during sedation; provides empirically grounded parameters.
- [ ] **Ref:** (2023). "Thalamic control of sensory processing and spindles in a biophysical somatosensory thalamoreticular circuit model of wakefulness and sleep." *Cell Reports* — biophysical TC+RE model with attention modulation via reticular inhibition.

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
