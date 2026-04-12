# Research Report: Latest Findings Applicable to the NMM

## Context

Research survey of recent (2024-2026) neuroscience, computational, and math papers for findings that could improve the bilateral Jansen-Rit + FHN neural mass model used to evaluate and optimize colored noise presets for brain state induction. The model currently includes: gammatone filterbank, ASSR DC/AC separation, thalamic gate (heuristic + physiological HH TC cell), bilateral JR with inhibitory callosal coupling, Wilson-Cowan frequency tracking, FHN probe, CET with slow/fast crossover + GABA_B + envelope PLV, stochastic JR, and habituation.

---

## Category A: High-Relevance Findings (directly applicable to current architecture)

### A1. Dynamic Feedback Inhibitory Control (dFIC) for Jansen-Rit
**Paper:** Stasinski et al. (2024). "Homeodynamic feedback inhibition control in whole-brain simulations." *PLOS Comput Biol.* [Link](https://journals.plos.org/ploscompbiol/article?id=10.1371/journal.pcbi.1012595)

**What's new:** A synaptic-plasticity-inspired rule that dynamically adjusts inhibitory coupling weights in JR to reach desired dynamic regimes. Solves the problem of overexcitation from heavy-tailed network distributions.

**Applicability to our NMM:**
- Currently, our bilateral JR has fixed inhibitory coupling weight `k`. dFIC would let each hemisphere's inhibitory weight adapt dynamically based on ongoing activity, potentially solving the hardcoded AST left-beta/right-alpha split problem described in BRAIN_MODEL_GUIDE.md.
- Could replace or augment the fixed `k * delayed_contralateral` coupling with an adaptive weight that self-tunes toward biologically plausible E/I balance.
- **Priority: MEDIUM.** The AST split is structural and currently documented as "not shapeable." dFIC might make it partially shapeable, opening new preset design space for bilateral-alpha goals.

### A2. ASSR 40 Hz is Network Resonance, Not Simple Frequency Response
**Paper:** (2024). "Network resonance and the auditory steady state response." *Scientific Reports.* [Link](https://www.nature.com/articles/s41598-024-66697-4)

**What's new:** The large 40 Hz ASSR peak results from *decreased latency variability* (enhanced temporal consistency) of neural populations at 40 Hz, not from a simple bandpass gain. Subpopulations of A1 neurons exhibit resonance at 40 Hz specifically. Low-frequency ASSR components show latencies ~41-52 ms vs ~21-27 ms for >80 Hz.

**Applicability to our NMM:**
- Our ASSR is a scalar log-Gaussian peaked at 40 Hz + 10 Hz. This paper suggests the 40 Hz peak should be modeled as a *latency-consistency* effect rather than a simple gain effect. In practice, this means: at 40 Hz, the phase jitter of the cortical response should be minimal (high PLV), not just that the amplitude is maximized.
- Could improve our PLV predictions: currently PLV at 40 Hz may be unrealistically low because our JR model doesn't have the temporal consistency mechanism.
- **Priority: LOW-MEDIUM.** Interesting for Ignition (40 Hz gamma goal) but the current scalar ASSR works adequately.

### A3. GABAergic Inhibition Shapes 40 Hz ASSR
**Paper:** (2024). "40 Hz Steady-State Response in Human Auditory Cortex Is Shaped by Gabaergic Neuronal Inhibition." *J Neurosci* 44(24). [Link](https://www.jneurosci.org/content/44/24/e2029232024)

**What's new:** The decay time and amplitude of inhibitory postsynaptic currents (IPSC) on PV+ interneurons are critical for generating 40 Hz ASSRs. GABA parameters directly modulate the ASSR transfer function.

**Applicability to our NMM:**
- Our JR model uses fixed `b=50/s` (tau_i~20 ms) for the fast inhibitory population. This paper suggests that varying `b` based on brain type could produce more realistic 40 Hz responses. For ADHD (weaker inhibition), a longer IPSC decay -> weaker 40 Hz ASSR -> lower gamma PLV, which is exactly what ADHD EEG studies show.
- **Priority: MEDIUM.** Could improve brain-type differentiation for Ignition/gamma goals, particularly the ADHD brain type.

### A4. Pink Noise Reduces Neural Noise via Aperiodic EEG Slope -- Challenges Stochastic Resonance
**Paper:** Rijmen, Senoussi, Wiersema (2026). "Pink Noise and a Pure Tone Both Reduce 1/f Neural Noise in Adults With Elevated ADHD Traits." *J Attention Disorders.* [Link](https://journals.sagepub.com/doi/10.1177/10870547251357074)

**What's new:** Both pink noise AND a 100 Hz pure tone reduce the aperiodic (1/f) slope of EEG PSD in individuals with elevated ADHD traits, indicating decreased neural noise. This challenges the Moderate Brain Arousal (MBA) model -- stochastic resonance is NOT required for noise to benefit ADHD. The mechanism appears to be a general arousal/neural noise reduction effect, not specific to 1/f noise spectra.

**Applicability to our NMM:**
- Our ADHD brain type model uses `input_offset=135` (near bifurcation) + weaker inhibition. The stochastic resonance mechanism is implicitly modeled via stochastic JR (sigma=15). This paper suggests we should also model the *aperiodic slope change* -- i.e., how external noise affects the 1/f background of the simulated EEG.
- Currently our model doesn't track or score the aperiodic component of the PSD separately from the oscillatory peaks. Adding an aperiodic slope metric could improve ADHD preset scoring.
- **Priority: MEDIUM.** Novel metric (aperiodic slope) could be added to PerformanceVector.

### A5. Meta-Analysis: White/Pink Noise Helps ADHD (g=0.249), Hurts Neurotypicals (g=-0.212)
**Paper:** (2024). "Systematic Review and Meta-Analysis: Do White Noise and Pink Noise Help With Attention in ADHD?" *J Am Acad Child Adolesc Psychiatry.* [PMC](https://pmc.ncbi.nlm.nih.gov/articles/PMC11283987/)

**What's new:** k=13 studies, N=335: small but significant benefit for ADHD (g=0.249, p<.0001). Crucially, non-ADHD comparison groups showed *negative* effects (g=-0.212, p=.0036). No studies on brown noise despite its popularity.

**Applicability to our NMM:**
- Validates our brain-type-dependent scoring: the same preset should score differently on Normal vs ADHD. The finding that noise *hurts* neurotypicals suggests our Normal brain type should show diminishing returns or even penalties at high noise levels.
- Could inform a new scoring mechanism: "arousal mismatch penalty" -- if the preset's arousal level is already optimal for the brain type, additional stimulation degrades performance.
- **Priority: MEDIUM.** Validates the existing architecture but suggests a refinement to scoring for Normal brain type.

### A6. Biophysical Modeling of Thalamic Reticular Nucleus Subpopulations
**Paper:** (2024). "Biophysical modeling of thalamic reticular nucleus subpopulations and their differential contribution to network dynamics." *bioRxiv.* [Link](https://www.biorxiv.org/content/10.1101/2024.12.08.627399v1.full)

**What's new:** Different TRN subpopulations have distinct T-type Ca2+ expression and firing properties at 25C. MCMC-validated models capture the diversity of burst/tonic patterns within TRN.

**Applicability to our NMM:**
- Our Priority 9 physiological thalamic gate uses a single TC cell. This paper provides parameters for adding an RE (reticular) neuron, which was listed as a "Future" item. The RE neuron with mutual TC<->RE inhibition would capture the chaotic intermediate regime (Paul 2016) that's currently missing.
- **Priority: LOW.** Good parameter source for a v2 phys-gate upgrade, but the current TC-only model already produces the qualitative burst/tonic switch needed.

---

## Category B: Optimization & Math Improvements

### B1. L-SHADE: Adaptive DE with Population Reduction
**What it is:** L-SHADE (Tanabe & Fukunaga, 2014, but still state-of-art through 2025 benchmarks) adapts F and CR from successful mutation history and linearly reduces population size. Multiple 2024-2025 papers confirm 2-5x efficiency gains on high-dimensional problems vs fixed DE/rand/1/bin.

**Applicability:** Direct drop-in replacement for our DE/rand/1/bin optimizer. The success-history parameter adaptation would be especially useful because our 190-D landscape has heterogeneous sensitivity -- some parameters (reverb, color) dominate while others (z-position, phase) barely matter. L-SHADE would learn this automatically.
- **Priority: HIGH for surrogate work (P14).** When implementing the surrogate-assisted DE, switch the underlying DE variant to L-SHADE simultaneously.

### B2. Weighted Committee-Based Surrogate DE (WCBDEF)
**Paper:** (2025). "Weighted Committee-Based Surrogate-Assisted Differential Evolution Framework." *Int J Machine Learning & Cybernetics.* [Link](https://link.springer.com/article/10.1007/s13042-025-02632-x)

**What's new:** Uses an ensemble of surrogates (MLP + RBF) instead of a single MLP, with disagreement-based uncertainty as a screening filter. Outperforms single-surrogate top-K pre-screening for medium-scale expensive optimization.

**Applicability:** Our P14 plans a single MLP surrogate. An ensemble (e.g., 3 MLPs with different initializations) with disagreement as the "confidence" signal would be more robust. When surrogates disagree, validate with the real pipeline; when they agree, trust the pre-screening more aggressively.
- **Priority: MEDIUM.** Enhancement to P14 design.

### B3. Deep JR Parameter Inference (Tilwani & O'Reilly 2024)
**Paper:** Tilwani D, O'Reilly C (2024). "Benchmarking Deep Jansen-Rit Parameter Inference." *arXiv:2406.05002.* [Link](https://arxiv.org/abs/2406.05002)

**What's new:** Transformer, LSTM, CNN-BiLSTM, and SNPE all benchmarked for JR parameter estimation from simulated EEG. Key finding: synaptic gains and time constants are reliably estimable; connectivity constants are not. R^2 > 0.8 for identifiable parameters with ~100k training samples.

**Applicability:** Already cited in our P14 plan. Confirms our MLP architecture choice is reasonable. The finding about non-identifiable parameters is important: it means our surrogate should work well for parameters that map to synaptic/timing properties but may be noisy for pure-topology parameters.
- **Priority: Already planned (P14).** No action needed beyond what's specified.

### B4. Simulation-Based Inference (SBI/SNPE) for Neural Models
**What it is:** Train a neural density estimator on ~50-100K JR simulations, then get near-instant posterior over parameters for any target spectrum. The `sbi` Python package provides ready-made SNPE implementations.

**Applicability:** Long-term alternative to surrogate-assisted DE. Instead of optimizing iteratively, you'd specify a target PSD and get a posterior distribution over preset genomes in milliseconds. This would fundamentally change the workflow from "run optimizer for hours" to "specify target, get distribution of solutions."
- **Priority: LOW (v3+).** Requires significant rearchitecting. The surrogate-DE approach in P14 is the right near-term step.

---

## Category C: Model Architecture Improvements

### C1. Next-Generation Neural Mass Models with Electrical Synapses
**Paper:** Byrne, Avitabile, Coombes (2024). "Whole brain functional connectivity: Insights from next generation neural mass modelling incorporating electrical synapses." *PLOS Comput Biol.* [Link](https://journals.plos.org/ploscompbiol/article?id=10.1371/journal.pcbi.1012647)

**What's new:** Derived from exact mean-field reduction of spiking neuron networks. Can naturally incorporate gap junctions (electrical synapses). Shows that E->E coupling alone is insufficient for realistic functional connectivity -- gap junctions are needed.

**Applicability:** Already cited in our Priority 11 roadmap. Confirms that our inter-columnar coupling (when implemented) should include gap junctions, not just chemical synapses. For the bilateral callosal coupling, this means we might need both inhibitory chemical synapses (current `-k * delayed`) AND direct gap junction coupling (instantaneous, bidirectional).
- **Priority: LOW.** Relevant only when implementing P11 (multi-column network). Current bilateral model is adequate.

### C2. Cross-Frequency Coupling in Neural Mass Models
**Paper:** Chehelcheraghi et al. (2017, but still state-of-art). "A neural mass model of cross frequency coupling." *PLOS One.* [Link](https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0173776)

**What's new (to our model):** Shows how JR-type models naturally produce 5 types of cross-frequency coupling (PAC, PFC, AAC, AFC, FFC) depending on operating point. Below a critical noise threshold -> frequency modulation (PFC); above -> amplitude modulation (PAC/AAC).

**Applicability:** Our model already implicitly produces some CFC because we have multiple JR/WC oscillators at different frequencies. But we don't *measure* CFC. Adding a PAC metric (theta-phase x gamma-amplitude coupling) could be a new scoring dimension for meditation (where theta-gamma PAC is a known correlate of mindfulness).
- **Priority: MEDIUM-LOW.** New metric, not a model change. Would require implementing Modulation Index (Tort et al. 2010) in performance.rs.

### C3. Closed-Loop Auditory Stimulation Insights for Sleep Presets
**Literature:** Multiple papers (Ngo et al. 2013-2025) on phase-locked auditory stimulation during slow-wave sleep.

**Key finding:** Clicks timed to SO up-states prolong SO trains, enhance SO amplitudes, and phase-lock spindles. Mistimed clicks disrupt rather than enhance. The process is self-limiting (Ngo et al. 2015).

**Applicability:** Our sleep presets use continuous noise, not phase-locked pulses. But the insight about self-limiting dynamics is relevant: our habituation mechanism (Priority 8) already models this -- sustained stimulation loses effectiveness. The SO timing literature suggests that for a *future* real-time version, feeding back the listener's EEG phase could dramatically improve sleep preset effectiveness. For the current offline model, the insight validates that our CET envelope PLV metric (which rewards phase-locked slow-rhythm tracking) is on the right track.
- **Priority: INFORMATIONAL.** Validates CET architecture. Real-time closed-loop is beyond current scope.

---

## Category D: Validation & Scoring Refinements

### D1. Aperiodic (1/f) Slope as a New EEG Metric
**Papers:** Rijmen et al. (2026), Donoghue & Voytek (2021)

**What it measures:** The slope of the aperiodic (non-oscillatory) component of the EEG power spectrum. Steeper slope = less neural noise. ADHD shows shallower slopes; noise exposure steepens them.

**Applicability:** Currently our scoring uses only oscillatory band powers + FHN firing. The aperiodic slope is an orthogonal metric that captures the E/I balance at a population level (Gao et al. 2017 showed aperiodic slope correlates with E/I ratio). Adding it:
1. Compute the aperiodic slope from the simulated EEG PSD (fit 1/f^beta in the 2-40 Hz range)
2. Add to PerformanceVector as `aperiodic_slope`
3. Use in scoring: ADHD brain type should be rewarded for steeper slopes (noise is helping); Normal should be penalized for slopes that are too steep (overstimulation)
- **Priority: MEDIUM.** Novel metric, moderate implementation effort. Would improve ADHD scoring specifically.

### D2. Amplitude-Modulated Tones > Binaural Beats > Isochronic Tones for Entrainment
**Literature:** Multiple comparative studies (Schwarz & Taylor 2006, replicated through 2024)

**Key finding:** AM tones produce the strongest cortical entrainment, binaural beats the weakest. 2024 study confirmed isochronic tones modulate alpha/beta bands and effects persist post-stimulation.

**Applicability:** Validates our architecture: we use AM noise (NeuralLfo modulator), which is the strongest entrainment mechanism. Our model doesn't model binaural beats (which arise from interaural phase differences, not AM). This is correct -- we don't need binaural beat modeling because AM is more effective and is what the engine actually produces.
- **Priority: INFORMATIONAL.** Validates current approach. No action needed.

### D3. 40 Hz Gamma Stimulation -- Expanding Clinical Evidence
**Papers:** Chan et al. (2025), MIT 2025 review, multiple 2024-2025 clinical trials

**Key finding:** Daily 40 Hz audiovisual stimulation over 2 years showed reduced cognitive decline in mild AD patients. Preclinical: 37-53% A-beta reduction, improved synaptic plasticity. Robust 40 Hz EEG entrainment confirmed.

**Applicability:** Validates our Ignition goal (gamma-driven activation, 40 Hz NeuralLfo). The clinical evidence is strongest for 40 Hz specifically. Our model's ASSR gives ~100% AC transmission at 40 Hz, which is correct per this literature. Could motivate a new "therapeutic" goal specifically targeting 40 Hz for cognitive health.
- **Priority: LOW.** Validates Ignition. New therapeutic goal is a product decision, not a model improvement.

---

## Recommended Implementation Priorities

Based on this research, here are the highest-value improvements ranked by impact/effort:

### Tier 1 -- Proven, actionable now
1. **Brain-type-dependent GABA parameters** -- Vary fast inhibitory time constant `b` by brain type (ADHD: longer tau_i -> weaker 40 Hz ASSR; Anxious: shorter tau_i -> stronger beta). Currently `b=50/s` is fixed across all brain types. Evidence: HIGH (J Neuroscience 2024, decades of GABA-gamma research). Low implementation effort -- `b` is already a per-model parameter.
2. **L-SHADE + surrogate (as part of P14)** -- Do NOT switch to L-SHADE alone (premature convergence on 190-D). Implement L-SHADE together with the surrogate pre-screening from P14. Evidence: HIGH for the combined approach, MEDIUM for L-SHADE standalone.

### Tier 2 -- Promising, needs testing
3. **dFIC adaptive inhibitory coupling** -- Replace fixed bilateral coupling weight with dynamic feedback inhibitory control (Stasinski 2024). Could partially break the hardcoded AST left-beta/right-alpha pattern. Evidence: MEDIUM (proven in silico on whole-brain, untested on 2-node bilateral).
4. **Surrogate ensemble** -- Use 3 MLPs instead of 1 for P14 surrogate, with disagreement-based confidence for more robust pre-screening. Evidence: MEDIUM (benchmarked at 30-100D, not 190D).

### Tier 3 -- Future / needs more evidence
5. **RE neuron for physiological gate v2** -- Use the Dec 2024 bioRxiv TRN modeling paper for RE parameters. Evidence: LOW (preprint, 25C parameters).
6. **PAC metric** -- Add phase-amplitude coupling measurement (theta-phase x gamma-amplitude) for meditation scoring. Evidence: LOW-MEDIUM (real phenomenon but EEG measurement has artifact problems).
7. **SBI/SNPE** -- Long-term replacement for iterative optimization. Evidence: HIGH for the method, LOW for near-term practicality.

### Deferred -- insufficient evidence
8. ~~**Aperiodic slope metric**~~ -- Originally proposed as Tier 1. Downgraded after evidence review: the metric is actively debated, FOOOF decomposition has known confounds, developmental ADHD studies show contradictory findings, and a 2024 systematic review found "lack of clarity." Wait for methodological consensus before implementing.

---

## Evidence Quality Assessment

Critical evaluation of each finding: is it proven, preliminary, or speculative? Are there methodological concerns?

### A1. dFIC for Jansen-Rit (Stasinski 2024)
**Status: PROVEN IN SILICO, NOT VALIDATED IN VIVO.**
- Published in PLOS Comput Biol (peer-reviewed). The math is solid -- it's a well-defined plasticity rule applied to a well-understood model.
- Limitation: validated only against simulated fMRI data in The Virtual Brain framework, not against real EEG during auditory stimulation. The mechanism works in whole-brain simulations with structural connectome data -- our bilateral 2-hemisphere model is a much simpler topology. Whether dFIC helps in a 2-node system (vs 68+ node whole brain) is untested.
- **Confidence for our use: MEDIUM.** The algorithm is sound; applicability to our specific architecture needs empirical testing.

### A2. ASSR Network Resonance (2024)
**Status: PROVEN EMPIRICALLY.**
- Published in Scientific Reports. Based on laminar recordings in animal models + EEG in humans. The finding that 40 Hz ASSR reflects temporal consistency rather than simple gain is well-supported by convergent evidence.
- Limitation: Translating "temporal consistency" into a concrete change for our scalar ASSR model is non-trivial. The paper describes the phenomenon but doesn't prescribe a computational recipe.
- **Confidence for our use: HIGH for understanding, LOW for implementation.** Validates our ASSR peak placement, but changing from scalar gain to a consistency model is a significant redesign.

### A3. GABAergic Inhibition Shapes ASSR (2024)
**Status: PROVEN EMPIRICALLY.**
- Published in J Neuroscience (top-tier). Direct evidence from pharmacological manipulation and computational modeling. The link between GABA IPSC decay time and 40 Hz ASSR is well-established.
- No significant criticisms. This is a mature finding building on decades of GABA-gamma research.
- **Confidence for our use: HIGH.** Varying JR's `b` parameter by brain type is a straightforward, well-grounded change.

### A4. Pink Noise & Aperiodic Slope in ADHD (Rijmen 2026)
**Status: PRELIMINARY -- SIGNIFICANT METHODOLOGICAL CONCERNS.**
- N=69 neurotypical adults (not clinical ADHD), assessed via self-report ASRS questionnaire, not clinical diagnosis. This is a dimensional/trait study, not a clinical ADHD study.
- Only 3 conditions (silence, pink noise, pure tone) x 2 minutes each. Very short exposure windows.
- The aperiodic slope metric (FOOOF/specparam decomposition) itself has known confounds: periodic peaks at low frequencies can inflate aperiodic slope estimates (demonstrated in simulation studies, Frontiers 2025). Whether the slope change reflects genuine E/I rebalancing or an artifact of how periodic components are separated is debated.
- The pure-tone finding is the key novelty -- but 100 Hz is not "truly non-random" in the brain's processing (the auditory system generates considerable neural activity in response to a sustained tone). The authors' framing of "random vs non-random" oversimplifies how the auditory cortex processes sustained stimuli.
- **Confidence for our use: LOW-MEDIUM.** The finding is interesting but preliminary. Adding aperiodic slope as a metric carries risk of modeling a confound rather than a real phenomenon. Wait for replication with clinical ADHD samples before building scoring around this.

### A5. ADHD Noise Meta-Analysis (2024)
**Status: PROVEN -- STRONG EVIDENCE.**
- Published in J Am Acad Child Adolesc Psychiatry (top-tier). Rigorous systematic review with meta-analysis.
- k=13 studies, N=335. Effect sizes are small (g=0.249) but consistent -- heterogeneity was I^2 < 0.01 (essentially zero), which is remarkably low for a behavioral meta-analysis.
- Survived sensitivity tests, no publication bias detected.
- The negative effect in neurotypicals (g=-0.212) is also robust (I^2 minimal).
- Limitation: No brown noise studies exist. Effect sizes are small. Most studies used white noise; fewer used pink. Noise type was not a significant moderator (but this may be underpowered).
- **Confidence for our use: HIGH.** This is the strongest evidence in the entire report. Directly validates brain-type-dependent scoring.

### A6. TRN Subpopulation Modeling (2024)
**Status: PRELIMINARY (PREPRINT).**
- bioRxiv preprint, not yet peer-reviewed. Uses MCMC for parameter fitting, which is methodologically sound, but results at 25C (not physiological 37C) may not translate directly.
- **Confidence for our use: LOW.** Good parameter source but needs peer review. Our current TC-only phys-gate works adequately.

### B1. L-SHADE
**Status: PROVEN -- BUT WITH CAVEATS FOR OUR USE CASE.**
- L-SHADE is well-established (CEC competition winner). However, recent benchmarks identify a specific weakness: **premature convergence in high-dimensional spaces**. Our 190-D genome space is at the edge where L-SHADE's exploitation bias starts to hurt. The parameter adaptation promotes exploitation over exploration.
- For expensive optimization (our case: 100ms/eval), vanilla L-SHADE is "not competitive with hybrid methods" (per 2024 benchmarks). It needs a surrogate component to be effective -- which is exactly what our P14 plans.
- **Confidence for our use: MEDIUM.** L-SHADE alone may not be better than DE/rand/1/bin on 190-D. L-SHADE + surrogate pre-screening (P14) should be significantly better. Don't switch DE variant without the surrogate.

### B2. Surrogate Ensemble (WCBDEF 2025)
**Status: PROVEN IN BENCHMARKS.**
- Published in Int J Machine Learning & Cybernetics (peer-reviewed). Demonstrated on standard benchmark functions. The ensemble-with-disagreement approach is well-grounded theoretically (bias-variance tradeoff, uncertainty quantification).
- Limitation: Benchmarks used 30-100D problems, not 190D. Scaling behavior is unknown.
- **Confidence for our use: MEDIUM.** Sound approach, but untested at our dimensionality. Start with single MLP (P14 as designed), upgrade to ensemble if single MLP shows instability.

### B3. Deep JR Parameter Inference (Tilwani & O'Reilly 2024)
**Status: PROVEN IN SILICO -- PEER-REVIEWED (arXiv with code).**
- Systematic benchmarking study with open-source code on GitHub. The methodology is sound: they generate synthetic EEG from known JR parameters, train deep models, and measure recovery accuracy.
- Key caveat: all results are in silico (simulated EEG, not real). The finding that connectivity constants (C1-C4) are NOT reliably estimable is important -- it means our JR model has parameter degeneracies that any surrogate or inference method will struggle with.
- The R^2 > 0.8 claim is for *identifiable* parameters only (synaptic gains A/B, time constants a/b). For non-identifiable ones, R^2 drops to ~0.3-0.5.
- **Confidence for our use: HIGH for surrogate architecture guidance.** Confirms MLP is a reasonable choice. The identifiability finding is a genuine insight for P14 design.

### B4. Simulation-Based Inference (SBI/SNPE)
**Status: PROVEN METHODOLOGY, UNTESTED ON OUR PROBLEM.**
- SBI is a mature framework with a well-maintained Python package (`sbi`). Goncalves et al. 2020 (eLife) demonstrated it on neural mass models including JR-like systems. The math is rigorous (neural density estimation with theoretical guarantees).
- Limitation: SBI works best when the forward model is differentiable or when you have a very large simulation budget (50-100K samples). Our 190-D genome space with a 100ms/eval pipeline would require ~5000 seconds (~1.4 hours) for 50K samples -- feasible but not trivial. The bigger concern is that SBI assumes a fixed forward model; any change to our scoring function requires complete retraining.
- No significant doubts about the math or methodology. The question is purely one of engineering effort vs. benefit relative to simpler surrogate approaches.
- **Confidence for our use: HIGH for the method, LOW for near-term practicality.** Correct to defer to v3+.

### C1. Next-Generation NMMs with Electrical Synapses (Byrne 2024)
**Status: PROVEN MATHEMATICALLY, NOVEL FRAMEWORK.**
- Published in PLOS Comput Biol (peer-reviewed). The derivation from exact mean-field reduction is mathematically rigorous (Ott-Antonsen ansatz extended to include gap junctions). This is not speculative -- it's a formal mathematical result.
- Limitation: The model is validated against functional connectivity patterns in resting-state fMRI, not against EEG during auditory stimulation. The claim that "gap junctions are needed" is for whole-brain resting-state FC patterns -- it may not apply to our local bilateral auditory model where the coupling is a single callosal connection, not a whole connectome.
- The paper uses theta-neuron models (quadratic integrate-and-fire), not Jansen-Rit. Translating the gap junction result to JR requires a non-trivial model mapping.
- **Confidence for our use: LOW-MEDIUM.** The math is proven but the architecture is different from ours. Relevant only for P11 multi-column work, and even there would need adaptation.

### C3. Closed-Loop Auditory Stimulation for Sleep
**Status: PROVEN EMPIRICALLY -- STRONG EVIDENCE.**
- Multiple replicated studies (Ngo et al. 2013, 2015; Besedovsky et al. 2023). Phase-locked auditory stimulation during NREM enhancing slow oscillations and spindles is one of the most robust findings in sleep neuroscience.
- The self-limiting property (Ngo 2015, J Neuroscience) is well-established: continuous stimulation loses effectiveness after ~5-10 SO cycles, matching our habituation model's behavior qualitatively.
- Limitations: All studies use brief click stimuli (50 ms), not continuous noise. Our presets are continuous noise -- the mechanism is fundamentally different (envelope tracking vs. discrete phase reset). The closed-loop insight validates our CET direction conceptually but doesn't provide quantitative parameters for continuous-noise stimulation.
- **Confidence for our use: HIGH as conceptual validation, N/A for direct implementation.** We can't do closed-loop without real-time EEG feedback. But it validates that phase-aligned slow-rhythm stimulation is the right direction, which is what our CET envelope PLV scores.

### C2. Cross-Frequency Coupling / PAC
**Status: REAL PHENOMENON, MEASUREMENT IS PROBLEMATIC.**
- PAC (theta-gamma coupling) is a genuine neural phenomenon with strong empirical evidence in hippocampal recordings.
- However, PAC measurement from EEG has significant artifact problems: volume conduction, filtering artifacts (apparent oscillations from filtering non-sinusoidal waveforms), and spurious PAC from sharp transients. A key methodological paper showed that an "18 Hz oscillation" in PAC analysis was purely a filtering artifact.
- The CFC neural mass model (Chehelcheraghi 2017) demonstrates that JR can produce CFC in principle, but measuring it reliably from simulated EEG requires the same care as from real EEG.
- **Confidence for our use: LOW-MEDIUM.** The phenomenon is real but measuring it accurately from our simulated EEG is non-trivial. Implementation risk is moderate.

### D1. Aperiodic Slope as EEG Metric
**Status: ACTIVELY DEBATED -- SIGNIFICANT METHODOLOGICAL CONCERNS.**
- The interpretation that aperiodic slope = E/I balance is based on computational models (Gao et al. 2017), not direct measurement. Recent work (2024-2025) raises multiple concerns:
  1. FOOOF decomposition can misattribute periodic peaks (especially at spectrum edges) as aperiodic components, inflating slope estimates
  2. Task-related changes in spectral power can co-modulate with slope shifts, creating confounds
  3. Developmental studies show inconsistent findings -- some ADHD studies find steeper slopes (opposite to the E/I imbalance hypothesis)
  4. A 2024 systematic review concluded there is a "lack of clarity on if and to what extent aperiodic properties reflect clinical features"
- **Confidence for our use: LOW.** Do NOT build scoring around aperiodic slope until the methodological debate resolves. The metric is interesting for research/diagnostics but too unstable for a production scoring function.

### D2. AM > Binaural Beats > Isochronic for Entrainment
**Status: PROVEN.**
- Replicated across multiple studies since Schwarz & Taylor 2006. AM tones consistently produce stronger cortical entrainment than binaural beats. The 2024 isochronic tone study confirms intermediate effectiveness. Well-established finding.
- **Confidence for our use: HIGH.** Validates our AM-based architecture.

### D3. 40 Hz Gamma for Alzheimer's
**Status: STRONG PRECLINICAL, PRELIMINARY CLINICAL.**
- Preclinical evidence is strong (Nature, Cell publications). Clinical trials are ongoing but small (N=5 in the 2-year follow-up). Phase II/III trials are underway. The mechanism (microglial activation, amyloid clearance) is well-supported.
- Limitation for our use: We're a noise generator, not a medical device. Clinical efficacy claims require regulatory approval. Our Ignition goal can be validated against the EEG entrainment data (robust 40 Hz power increase confirmed), but not against therapeutic outcomes.
- **Confidence for our use: HIGH for 40 Hz entrainment as an EEG phenomenon. N/A for therapeutic claims.**

---

## Key Sources

- [Stasinski et al. 2024 -- dFIC for JR](https://journals.plos.org/ploscompbiol/article?id=10.1371/journal.pcbi.1012595)
- [Network resonance and ASSR 2024](https://www.nature.com/articles/s41598-024-66697-4)
- [40 Hz ASSR shaped by GABA 2024](https://www.jneurosci.org/content/44/24/e2029232024)
- [Rijmen et al. 2026 -- Pink noise & ADHD neural noise](https://journals.sagepub.com/doi/10.1177/10870547251357074)
- [ADHD noise meta-analysis 2024](https://pmc.ncbi.nlm.nih.gov/articles/PMC11283987/)
- [TRN subpopulation modeling 2024](https://www.biorxiv.org/content/10.1101/2024.12.08.627399v1.full)
- [Next-gen NMM with gap junctions 2024](https://journals.plos.org/ploscompbiol/article?id=10.1371/journal.pcbi.1012647)
- [CFC in neural mass models](https://journals.plos.org/plosone/article?id=10.1371/journal.pone.0173776)
- [Tilwani & O'Reilly 2024 -- Deep JR inference](https://arxiv.org/abs/2406.05002)
- [WCBDEF surrogate ensemble 2025](https://link.springer.com/article/10.1007/s13042-025-02632-x)
- [Binaural beats parametric investigation 2025](https://www.nature.com/articles/s41598-025-88517-z)
- [Gamma stimulation for AD 2025](https://alz-journals.onlinelibrary.wiley.com/doi/10.1002/alz.70792)
- [Auditory cortical entrainment to continuous sounds 2024](https://www.eneuro.org/content/11/3/ENEURO.0027-23.2024)
- [Stochastic resonance not required for ADHD benefit 2024](https://www.sciencedirect.com/science/article/abs/pii/S0028393224001763)
