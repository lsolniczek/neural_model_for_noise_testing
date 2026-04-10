# Neural Model Improvement Roadmap

## Priority 1: ASSR Transfer Function
- [x] Research ASSR frequency-response curves from literature
- [x] Implement frequency-dependent gain (log-Gaussian, peaks at 40 Hz + 10 Hz)
- [x] Unit tests (16 tests passing)
- [x] Integration tests (verified component has effect on modulated signals)
- [x] Pipeline integration with CLI flags (--assr, --thalamic-gate)
- [x] Disabled by default (zero regression on 236 existing tests)
- [ ] **BLOCKED: Envelope extraction needed.** Current implementation applies ASSR to raw band signals, but modulation is ~5% of total broadband noise power — too small to shift neural model. Need to extract modulation envelope from band signals before applying ASSR, then reconstruct. This is how real ASSR works (envelope following response).
- [ ] Implement Hilbert transform or analytic signal for envelope extraction
- [ ] Apply ASSR to extracted envelope, not raw signal
- [ ] Re-evaluate all presets with envelope-based ASSR

## Priority 1b: Envelope Extraction (unblocks ASSR + Thalamic Gate)
- [ ] Implement Hilbert transform or analytic signal to extract modulation envelope from each band signal
- [ ] The proper fix: extract the modulation envelope from each band signal first, apply ASSR to the envelope, then reconstruct. That's the way real ASSR works — it measures the envelope following response (EFR), not the raw signal.
- [ ] Architecture: `band_signal → Hilbert → envelope → ASSR filter → modulated_envelope → reconstruct → neural model`
- [ ] The envelope captures amplitude modulation (NeuralLfo, Breathing, Stochastic) as the dominant signal, removing the broadband noise carrier that drowns out the filter's effect
- [ ] Unit tests for envelope extraction (known modulated sinusoid → correct envelope)
- [ ] Integration tests verifying ASSR and thalamic gate now measurably change preset scores
- [ ] This is the critical path — both ASSR and thalamic gate are implemented and tested but ineffective without this step

## Priority 2: Wilson-Cowan on All Bands
- [ ] Add new brain type profiles with WC on bands 0-1 for relaxation goals
- [ ] WC(2.0) on band 0 for delta oscillation
- [ ] WC(5.0) on band 1 for theta oscillation
- [ ] Keep JR on bands 0-1 for focus/isolation profiles (alpha attractor is correct there)
- [ ] Create "Relaxation" and "Sleep" brain type variants
- [ ] Compare preset scores before/after

## Priority 3: Thalamic Gate
- [x] Design thalamic relay model (arousal-dependent sigmoid crossover at 10 Hz)
- [x] Implement arousal computation from preset properties (brightness, reverb, modulation speed, movement)
- [x] Unit tests (16 tests passing)
- [x] Pipeline integration with CLI flag (--thalamic-gate)
- [x] Disabled by default (zero regression)
- [ ] **BLOCKED: Same envelope extraction issue as ASSR.** Gate applies to raw band signals where modulation is a tiny fraction. Needs envelope-based processing to be effective.
- [ ] Consider: thalamic gate may be better modeled as a JR input_offset modifier (shifting the cortical model's operating point) rather than a signal filter

## Priority 4: Entrainment Coherence Scoring
- [ ] Replace or supplement band-power scoring with phase-locking value (PLV)
- [ ] Measure coherence between modulation frequency and neural oscillation
- [ ] Weight scoring by entrainment strength, not just band power magnitude
- [ ] Adjust goal targets for coherence-based metrics

## Priority 5: EEG Validation
- [ ] Design experiment protocol (presets × participants × conditions)
- [ ] Record EEG during preset playback
- [ ] Compare model predictions with actual EEG band changes
- [ ] Calibrate model parameters to minimize prediction error
- [ ] Publish findings

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

### Noise & Cognitive Performance
- Rausch VH, Bauch EM, Bunzeck N (2014). "White noise improves learning by modulating activity in dopaminergic midbrain regions and right superior temporal sulcus." *J Cogn Neurosci* 26(7):1469-1480.
- Söderlund G, Sikström S, Loftesnes JM, Sonuga-Barke EJ (2010). "The effects of background white noise on memory performance in inattentive school children." *Behav Brain Funct* 6:55.
