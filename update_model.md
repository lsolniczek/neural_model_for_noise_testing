# Neural Model Improvement Roadmap

## Implemented

### ASSR Transfer Function
- [x] Research ASSR frequency-response curves from literature (Picton 2003, Ross 2000)
- [x] Implement frequency-dependent gain (log-Gaussian, peaks at 40 Hz + 10 Hz) — `src/auditory/assr.rs`
- [x] Unit tests (22 tests passing)
- [x] Pipeline integration with CLI flag (`--assr`)
- [x] Disabled by default (zero regression)
- [x] **Pivot: signal-level FFT filtering ineffective** — band signals are DC-dominated envelopes, modulation is ~5% of power. Proved empirically: scores identical with/without signal filtering.
- [x] **New approach: preset-level input_scale modifier.** `compute_input_scale_modifier()` scans NeuralLfo frequencies, computes weighted ASSR gain, scales JR `input_scale`. Presets with 40 Hz modulation get full drive; 5 Hz gets ~0.31x drive.
- [x] Verified: Ignition (25 Hz NeuralLfo) gets +0.04 score improvement with ASSR enabled

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

## Priority 3: Decimation Anti-Aliasing (MEDIUM IMPACT, LOW EFFORT)

- [ ] **Problem:** 48:1 downsampling uses boxcar filter (`pipeline.rs:23-32`) with slow sidelobe roll-off (~13 dB first sidelobe). Content at 400-800 Hz aliases into theta range (4-8 Hz) in the decimated signal, creating false theta power.
- [ ] **Fix:** Apply Hann window before summation in decimate():
  ```rust
  let w = 0.5 * (1.0 - cos(2π * j / (factor-1)));
  ```
- [ ] **Ref:** Oppenheim AV, Schafer RW (2009). *Discrete-Time Signal Processing.* 3rd ed. Prentice Hall, Ch. 4.7 — boxcar (rectangular) window has -13 dB first sidelobe; Hann window achieves -31 dB, sufficient for 48:1 decimation.
- [ ] **Ref:** Crochiere RE, Rabiner LR (1983). *Multirate Digital Signal Processing.* Prentice Hall, Ch. 2 — establishes that decimation without adequate anti-aliasing introduces spectral folding proportional to the sidelobe level of the analysis window.
- [ ] Tests: verify theta content doesn't increase after decimation of pure high-frequency signal

## Priority 4: Bilateral Coupling Realism (MEDIUM IMPACT, HIGH EFFORT)

- [ ] **Problem:** Corpus callosum modeled as excitatory coupling (`jansen_rit.rs:698+`) when physiologically it's primarily inhibitory at the thalamic level. Fixed 10ms delay doesn't account for frequency-dependent callosal transmission.
- [ ] **Fix:** Replace with inhibitory coupling (negative offset modification on contralateral hemisphere). Make delay frequency-dependent.
- [ ] **Ref:** Innocenti GM (1986). "General organization of callosal connections in the cerebral cortex." In: Jones EG, Peters A (eds) *Cerebral Cortex,* vol 5. Plenum Press. — callosal projections are predominantly excitatory to inhibitory interneurons, creating net interhemispheric inhibition.
- [ ] **Ref:** Bloom JS, Hynd GW (2005). "The role of the corpus callosum in interhemispheric transfer of information: excitation or inhibition?" *Neuropsychol Rev* 15(2):59-71. — reviews evidence that callosal function is primarily inhibitory for frequency-specific processing.
- [ ] **Ref:** Aboitiz F, Scheibel AB, Fisher RS, Zaidel E (1992). "Fiber composition of the human corpus callosum." *Brain Res* 598(1-2):143-153. — callosal axon diameters vary (0.4-5 μm), giving conduction velocities of 3-60 m/s and frequency-dependent delays of 5-50 ms.
- [ ] This is a large architectural change — needs careful regression testing

## Priority 5: Wilson-Cowan Frequency Tracking (MEDIUM IMPACT, HIGH EFFORT)

- [ ] **Problem:** WC oscillates at hardcoded frequencies (14/25 Hz) regardless of input. Real cortical oscillations track input frequency — driving at 20 Hz should produce 20 Hz output. Current WC always outputs 25 Hz.
- [ ] **Fix:** Replace fixed-frequency WC with input-dependent frequency tracking. Options: Kuramoto oscillator, adaptive WC with input-dependent tau, or phase-locking loop.
- [ ] **Ref:** Pikovsky A, Rosenblum M, Kurths J (2001). *Synchronization: A Universal Concept in Nonlinear Sciences.* Cambridge University Press. — Chapter 3 covers forced synchronization of nonlinear oscillators, the mathematical framework for entrainment.
- [ ] **Ref:** Notbohm A, Kurths J, Herrmann CS (2016). "Modification of brain oscillations via rhythmic sensory stimulation." *Int J Psychophysiol* 103:62-68. — demonstrates that cortical oscillators follow external driving frequencies within an Arnold tongue (entrainment region), not just at their natural frequency.
- [ ] **Ref:** Thut G, Schyns PG, Gross J (2011). "Entrainment of perceptually relevant brain oscillations by non-invasive rhythmic stimulation of the human brain." *Front Psychol* 2:170. — evidence that cortical oscillation frequency shifts toward the stimulus frequency within ±2-3 Hz of natural frequency.
- [ ] Consider: may be better to use JR for all bands and accept 20 Hz ceiling, since the thalamic gate now handles slow-wave production

## Priority 6: Scoring Refinements (MEDIUM IMPACT, MEDIUM EFFORT)

### 6a. Remove Brightness Double-Counting
- [ ] **Problem:** Brightness contributes 10% of score (`scoring.rs:309-316`) but partially duplicates information already in band powers. Biases optimizer toward bright noise even when neural entrainment is suboptimal.
- [ ] **Fix:** Replace brightness term with goal-specific spectral match derived from band energy fractions. Or remove and increase band_weight to 1.0.
- [ ] **Ref:** Zwicker E, Fastl H (1999). *Psychoacoustics: Facts and Models.* 2nd ed. Springer. — spectral loudness and brightness are perceptual constructs derived from the same cochlear excitation pattern that already feeds the neural model; including both double-counts.

### 6b. Include Alpha Asymmetry in Scoring
- [ ] **Problem:** Alpha asymmetry is computed (`pipeline.rs:388`) but never used in scoring. Hemispheric balance matters for meditation, relaxation, and deep work goals.
- [ ] **Fix:** Add asymmetry as a soft penalty for goals that want balanced states.
- [ ] **Ref:** Davidson RJ (2004). "What does the prefrontal cortex 'do' in affect: perspectives on frontal EEG asymmetry research." *Biol Psychol* 67(1-2):219-234. — frontal alpha asymmetry is a reliable marker of approach/withdrawal motivation; left-dominant alpha suppression correlates with positive affect (relevant for relaxation goals).
- [ ] **Ref:** Allen JJB, Coan JA, Nazarian M (2004). "Issues and assumptions on the road from raw signals to metrics of frontal EEG asymmetry in emotion." *Biol Psychol* 67(1-2):183-218. — methodological framework for scoring hemispheric asymmetry.

### 6c. Entrainment Coherence Scoring
- [ ] Replace or supplement band-power scoring with phase-locking value (PLV)
- [ ] Measure coherence between modulation frequency and neural oscillation
- [ ] Weight scoring by entrainment strength, not just band power magnitude
- [ ] **Ref:** Lachaux JP, Rodriguez E, Martinerie J, Varela FJ (1999). "Measuring phase synchrony in brain signals." *Hum Brain Mapp* 8(4):194-208. — defines PLV as |1/N × Σ exp(iφ(t))| where φ is the instantaneous phase difference; PLV=1 means perfect phase-locking (entrainment), PLV=0 means no entrainment.
- [ ] **Ref:** Helfrich RF, Schneider TR, Rach S, Trautmann-Lengsfeld SA, Engel AK, Herrmann CS (2014). "Entrainment of brain oscillations by transcranial alternating current stimulation." *Curr Biol* 24(3):333-339. — applies PLV to measure entrainment from external periodic stimulation; directly analogous to our auditory modulation → cortical oscillation pipeline.

## Priority 7: EEG Validation (HIGH IMPACT, HIGH EFFORT)

- [ ] Design experiment protocol (presets × participants × conditions)
- [ ] Record EEG during preset playback
- [ ] Compare model predictions with actual EEG band changes
- [ ] Calibrate model parameters to minimize prediction error
- [ ] Publish findings
- [ ] **Ref:** Obleser J, Kayser C (2019). "Neural entrainment and attentional selection in the listening brain." *Trends Cogn Sci* 23(11):913-926. — methodological framework for measuring neural entrainment to auditory stimuli via EEG; defines best practices for stimulus-response coherence analysis.
- [ ] **Ref:** Zoefel B, ten Oever S, Sack AT (2018). "The involvement of endogenous neural oscillations in the processing of rhythmic input: more than a regular repetition of evoked neural responses." *Front Neurosci* 12:95. — distinguishes true entrainment from evoked responses in EEG; critical for validating whether our model captures genuine oscillatory coupling vs. stimulus-locked artifacts.

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
