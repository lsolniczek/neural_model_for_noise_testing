All Models Are Wrong, But Some Are Useful

How we use computational neuroscience and advanced DSP to design noise — and why we don't pretend it's perfect

"All models are wrong, but some are useful."
— George E.P. Box, Science and Statistics (1976)

Box was talking about statistical models, but the line lands harder in neuroscience. Every model of the brain is wrong. The brain has roughly 86 billion neurons, each with thousands of synaptic connections, shaped by genetics, experience, sleep, caffeine, mood, and whether you had an argument this morning. No simulation captures that. None ever will.

And yet — models are how we got most of what we know about how the brain works. Neuroscientists have spent decades building mathematical frameworks that describe how populations of neurons behave: how excitatory and inhibitory circuits interact, how cortical rhythms emerge, how the thalamus gates sensory information depending on arousal. These models don't describe your brain. They describe how brains in general tend to work — and they've been validated against thousands of EEG recordings in peer-reviewed research since the 1990s.

We built our noise design pipeline on top of these models. Here's what that means for your ability to do deep work, and — just as importantly — what it doesn't.

What most noise apps do

Someone with good ears picks a sound. Maybe they A/B test it with a focus group. They call it "Focus Noise" or "Deep Sleep" and ship it. The entire design process lives in subjective preference: does this sound feel calming?

That's not nothing. Human intuition about sound is real. But it doesn't answer the question we care about: what is the acoustic stimulus actually doing to cortical dynamics to keep you in a state of deep focus?

What we do instead

We built a simulation pipeline. It takes a noise preset — a complete spatial audio scene with colored noise sources, modulation patterns, spatial movement, and room acoustics — and runs it through a chain of biophysical models drawn from the computational neuroscience literature.

The pipeline doesn't just process sound — it sculpts an acoustic environment designed to reduce cognitive fatigue. The output isn't "this sounds relaxing." It's a quantitative prediction: What EEG band power distribution does this stimulus produce? What happens to neural firing regularity? Does the cortex phase-lock to the modulation frequency, or ignore it?

These are the same measurements a neuroscience lab would extract from a real EEG recording. We're predicting them computationally, from the acoustic signal alone.

Then we optimize. An evolutionary search algorithm explores tens of thousands of possible noise configurations, each evaluated by the neural simulation. The result isn't a preset that sounds like it should help you focus — it's one where our models predict the cortical response matches the exact spectral signature that researchers associate with deep work.

The pipeline: what's inside (and what makes it non-trivial)

We won't detail the full architecture — it's our core IP — but we can describe the class of science it's built on.

Neural mass models. The backbone is a mathematical framework that simulates how populations of cortical neurons interact. Four distinct neural populations — excitatory pyramidal cells, excitatory interneurons, slow inhibitory interneurons, and fast inhibitory interneurons — coupled through nonlinear transfer functions. This produces output with realistic spectral properties across all five clinical EEG bands (delta to gamma).

Biophysical auditory pathway. Sound doesn't arrive at the cortex as raw audio. The auditory system filters it. We model this transfer function so that the neural simulation receives input that reflects what the cortex actually sees, not just the waveform that hits the eardrum.

Thalamic gating with ion-channel dynamics. The thalamus is the brain's sensory gatekeeper. As arousal drops, it switches from tonic to burst mode — driven by T-type calcium channels. We model this gate using physiological mechanisms, meaning our simulation's access to slow-wave bands is governed by real biology, not a linear volume knob.

Bilateral hemispheric simulation. We simulate both hemispheres, connected by inhibitory coupling that mirrors the corpus callosum. This lets us model — and penalize, or reward — hemispheric asymmetry in the cortical response.

Dual-pathway cortical envelope tracking. The brain synchronizes to external stimuli through fast entrainment to carrier frequencies and slow cortical tracking of the stimulus envelope. We model both pathways to maximize phase-locking for focused states.

Spiking dynamics and E/I balance. Beyond spectral power, we model the balance between excitatory and inhibitory cortical activity — a ratio that's increasingly recognized as central to maintaining cognitive load during deep work.

Five brains, not one

Neural dynamics vary. Baseline alpha power differs between meditators and non-meditators. Cortical inhibition is characteristically weaker in ADHD. Beta activity is elevated in anxiety.

Our model parameterizes five distinct neurological profiles, each reflecting documented differences from the clinical literature. A preset that scores well for a neurotypical baseline may score differently for an anxiety-profile brain.

This isn't personalization (we don't read your mind). It's an acknowledgment that "the average brain" is a simplification. By designing presets that account for these variances, we build a broader, more robust foundation for deep work across different neurotypes.

Resilience: Protecting your flow from the real world

A good preset doesn't just help you reach deep work — it protects you when the real world intrudes. We built a disturbance testing framework inspired by ERD/ERS methodology from the clinical EEG literature.

We inject controlled acoustic spikes into the simulation: What happens when a dog barks, a Slack notification chimes, or a door slams? How much do band powers drop? How fast does your spectral profile recover?

These metrics tell us how robust a preset is. We design our audio to ensure that when someone drops a mug in the kitchen, you don't lose your flow.

The reality check: Maps vs. Territories

Are we running clinical EEG trials on our users? No. We don't claim to read your mind or guarantee a specific EEG pattern in you on this Tuesday. Individual variation is vast, and no simulation captures it perfectly.

Instead, we stand on the shoulders of giants. The entire pipeline is a simulation predicting what should happen, based on mathematical frameworks validated across thousands of peer-reviewed studies by the scientific community.

What we do claim:

We aren't offering magic. We are offering an evidence-based approximation. Our models capture general neural dynamics. When our engine predicts that a specific pattern of acoustic modulation will drive alpha-band power up for Deep Work, that prediction is grounded in the same mathematics that neuroscientists use every day.

It’s an approach pointed strictly in the direction of decades of peer-reviewed research, rather than the direction of "this pink noise sounds kinda chill."

The honest pitch

Every preset we ship has been:

Optimized against biophysical neural models from the computational neuroscience literature

Stress-tested for resilience to real-world acoustic disruption

Scored on quantitative neuroscience metrics to maximize deep work potential

Is it a perfect replica of your brain? No. Is it a ruthlessly effective tool for focus? We think the science says yes.

Box's line is a reminder that usefulness doesn't require perfection. A weather model that's wrong about whether it will rain at 3:17 PM on your street is still useful if it reliably distinguishes rainy days from sunny ones. Our model is an imperfect simulation of the brain, but it reliably distinguishes acoustic configurations that drive flow from ones that cause fatigue.

All models are wrong. We built one that helps you do your best work.

We design noise for brains, not just ears. And we're honest about the gap between the two.
