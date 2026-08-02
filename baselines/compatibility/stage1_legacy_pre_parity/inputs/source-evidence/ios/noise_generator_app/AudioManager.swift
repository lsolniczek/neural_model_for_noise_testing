#if os(iOS) || os(visionOS)
import Accelerate
import AVFoundation
import Observation

#if canImport(noise_generatorFFI)
import noise_generatorFFI
#endif

// Why @Observable instead of ObservableObject + @Published + Combine?
//
// The project build setting SWIFT_DEFAULT_ACTOR_ISOLATION = MainActor makes
// every type implicitly @MainActor. ObservableObject (from Combine) requires
// its objectWillChange publisher to be Sendable across threads, which clashes
// with that isolation and causes a compiler error.
//
// @Observable (available from iOS 17 / Swift 5.9, introduced in WWDC 2023) is
// the modern replacement. It is designed to work with Swift's actor model:
//   - No Combine import needed.
//   - No @Published on individual properties — all stored properties are
//     tracked automatically.
//   - Works correctly when the type is @MainActor-isolated.
//   - In views, use @State (for owned instances) or @Environment instead of
//     @StateObject / @EnvironmentObject.
@Observable
final class AudioManager {

    // MARK: - Observable state (drives the UI)

    /// True while the audio engine is running and noise is being produced.
    /// Because AudioManager is @Observable, SwiftUI views that read this
    /// property will re-render automatically when it changes — no @Published
    /// or objectWillChange.send() required.
    private(set) var isPlaying = false

    /// Whether the early-reflections room simulator is active.
    /// Mirrors the atomic value stored in the Rust engine.
    /// Default: `true` (enabled at startup).
    private(set) var earlyReflectionsEnabled: Bool = true

    /// Current room-size multiplier for the early-reflections module.
    /// Clamped to `[0.5, 2.0]` by the Rust engine.
    /// Default: `1.0` (reference room).
    private(set) var roomSize: Float = 1.0

    /// Current dry/wet blend for the early-reflections module.
    /// Clamped to `[0.0, 1.0]` by the Rust engine.
    /// Default: `0.3` (subtle blend — adds depth without colouring the noise).
    private(set) var reflectionsMix: Float = 0.3

    /// The current noise color target. The Rust engine crossfades to this color
    /// using an equal-power (cos/sin) crossfade over the configured fade
    /// duration (default 1.5 s). Default: `.white`.
    private(set) var noiseColor: NoiseColor = .white

    /// Whether the Base + Satellite spatial test mode is active.
    private(set) var isBaseSatelliteMode: Bool = false

    /// Which preset is currently loaded. Drives selection highlighting in the UI.
    private(set) var activePreset: ActivePreset = .normalShield1

    // MARK: - Private audio objects

    /// The AVAudioEngine graph: sourceNode → mainMixerNode → outputNode.
    /// We hold a strong reference so the engine is not deallocated while
    /// the app is running.
    private let audioEngine = AVAudioEngine()

    /// The source node feeds PCM buffers from the Rust engine into the
    /// AVAudioEngine graph. Stored as a property to prevent deallocation.
    private var sourceNode: AVAudioSourceNode?

    /// The Rust DSP engine that generates the actual audio samples.
    /// Created once and kept alive for the entire app lifetime.
    private let noiseEngine: NoiseEngine

    /// Raw opaque pointer to the NoiseEngine's inner DSP state, obtained
    /// once during setup via noise_generator_engine_ptr().
    ///
    /// This pointer is passed directly to noise_generator_render_into() on
    /// every render callback. It bypasses UniFFI's serialisation layer
    /// entirely, giving us a lock-free, allocation-free render path that is
    /// safe to call from the real-time audio thread.
    ///
    /// Its lifetime is bound to `noiseEngine` — it must not be used after
    /// the NoiseEngine object is deallocated (which is never, for this app).
    private let enginePtr: UnsafeRawPointer?

    /// Pre-allocated interleaved scratch buffer used inside the render closure.
    ///
    /// noise_generator_render_into() writes interleaved stereo samples
    /// [L₀, R₀, L₁, R₁, …] into this buffer. The render closure then
    /// de-interleaves them into the two planar (non-interleaved) channel
    /// buffers that AVAudioEngine requires.
    ///
    /// Allocated once here with capacity for maxScratchFrames × 2 floats,
    /// then reused on every render callback — zero heap allocations in the
    /// hot path.
    private let scratchBuffer: UnsafeMutablePointer<Float>

    /// Maximum frame count the scratch buffer is sized for.
    /// AVAudioSourceNode callbacks are typically 256 or 512 frames on
    /// the built-in speaker, but Bluetooth codecs and AirPlay can request
    /// up to 4 096 frames per callback. Sizing for 4 096 covers all known
    /// iOS audio routes at a cost of only 32 KB (4096 × 2 × 4 bytes).
    private let maxScratchFrames = 4096

    /// The sample rate (Hz) that the Rust engine is currently using.
    /// Updated after a successful `setSampleRate` call on route change.
    private var engineSampleRate: Double

    /// Notification observers for route-change and engine-configuration-change.
    /// Stored so they can be removed in deinit.
    private var routeChangeObserver: NSObjectProtocol?
    private var configChangeObserver: NSObjectProtocol?

    /// Atomic mute flag shared between the main thread and the real-time
    /// audio thread. When non-zero the render closure outputs silence
    /// instead of DSP samples, preventing clicks caused by reading a
    /// partially-reconfigured preset state.
    ///
    /// Allocated as a raw pointer so the render closure can capture it
    /// without retaining `self`. Aligned 32-bit reads/writes are naturally
    /// atomic on ARM64 and x86_64 — no lock or barrier needed for a single
    /// writer (main thread) / single reader (audio thread) flag.
    private let muteFlag: UnsafeMutablePointer<Int32>

    /// Counter incremented (from the audio thread) every time the system
    /// requests more frames than `maxScratchFrames`. Read from the main
    /// thread via `scratchClampCount` for diagnostics.
    /// Uses a raw pointer for the same lock-free reasons as `muteFlag`.
    private let clampCounter: UnsafeMutablePointer<Int32>

    /// Number of times the render callback had to clamp `frameCount` to
    /// `maxScratchFrames`. A non-zero value means some audio buffers were
    /// partially zero-filled. Useful for diagnosing user reports of
    /// intermittent silence on Bluetooth/AirPlay routes.
    var scratchClampCount: Int32 { clampCounter.pointee }

    /// Movement controller for the Base + Satellite test mode.
    /// Animates the satellite position using CADisplayLink.
    private let movementController: MovementController

    // MARK: - Head tracking

    /// Observable head-tracking mode. Always equal to the coordinator's
    /// current mode — every mutation goes through `applyMode(_:persist:)`
    /// which updates the property, the coordinator, and (optionally)
    /// UserDefaults in one step. External callers use `setHeadTrackingMode`.
    private(set) var headTrackingMode: HeadOrientationCoordinator.Mode = .auto

    /// Observable simulated-drift algorithm selection.
    var simulationAlgorithm: HeadOrientationCoordinator.SimulationAlgorithmKind {
        headCoordinator.simulationAlgorithm
    }

    /// Coordinator that picks the motion source (AirPods vs simulated) and
    /// feeds orientation into the Rust engine.
    ///
    /// Tracking runs continuously while the app process is alive — including
    /// while backgrounded. The `audio` background mode keeps the process
    /// running, and `CMHeadphoneMotionManager` keeps delivering samples to
    /// that process. That's the whole point: users listen to noise with the
    /// screen off, and externalisation must follow their head.
    private let headCoordinator: HeadOrientationCoordinator

    /// True when the UI should display the head-tracking intro sheet (first
    /// launch, AirPods present, prompt not yet triggered). Cleared via
    /// `dismissHeadTrackingIntro(enableAirPods:)` once the user has chosen.
    private(set) var needsHeadTrackingIntro: Bool = false

    private static let introShownDefaultsKey = "headTracking.introShown"
    private static let preferenceDefaultsKey = "headTracking.preference"

    /// Reads the user's persisted head-tracking preference. Returns nil if
    /// the user hasn't made a choice yet (first launch).
    private static func loadHeadTrackingPreference() -> HeadOrientationCoordinator.Mode? {
        guard let raw = UserDefaults.standard.string(forKey: preferenceDefaultsKey) else {
            return nil
        }
        return HeadOrientationCoordinator.Mode(rawValue: raw)
    }

    // MARK: - Init

    init() {
        // ── Step 1: Configure AVAudioSession ─────────────────────────────────
        //
        // `.playback` category tells iOS that this app plays long-duration
        // audio even when the screen is locked or the silent switch is on.
        // Without this the audio stops the moment the screen dims or the user
        // flips the mute switch.
        //
        // We configure the session *before* reading sampleRate because the
        // hardware sample rate is only finalised once the session is active.
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(.playback, mode: .default)
            try session.setActive(true)
        } catch {
            print("AudioManager: AVAudioSession setup failed: \(error)")
        }

        // ── Step 2: Read the hardware sample rate ─────────────────────────────
        //
        // `session.sampleRate` returns the rate the hardware is currently
        // running at — typically 44 100 Hz or 48 000 Hz depending on the
        // device and any connected audio accessories.
        //
        // We must pass the exact same value to NoiseEngine so that the Rust
        // DSP uses the correct sample rate when computing:
        //   - Biquad filter coefficients (notch frequencies scale with Fs)
        //   - LFO phase increments (LFO rates are in Hz; Fs converts them to
        //     per-sample increments)
        // A mismatch would shift every LFO frequency and filter centre
        // proportionally — e.g. at 44 100 instead of 48 000 Hz, the LFOs
        // would run ~9 % faster than intended.
        let hardwareSampleRate = UInt32(session.sampleRate)
        engineSampleRate = session.sampleRate

        // ── Step 3: Instantiate the Rust noise engine ─────────────────────────
        //
        // masterGain: 0.7
        //   The Rust engine applies tanh(x) soft clipping as the final stage.
        //   At gain = 1.0 the peak output is ≈ 0.76 (tanh of the LFO
        //   amplitude peak of 1.0). That is already comfortably below 0 dBFS,
        //   so 1.0 would not clip. We use 0.7 because:
        //     - It is a comfortable loudness for a background noise use case —
        //       audible and effective at masking distractions without being
        //       fatiguing at normal listening volumes.
        //     - It leaves a clear safety margin below the tanh shoulder,
        //       ensuring the soft clipper never meaningfully colours the sound.
        //   Valid range is [0.0, 1.0]; the Rust layer silently clamps anything
        //   outside that range.
        let engine = NoiseEngine(sampleRate: UInt32(hardwareSampleRate), masterGain: 0.7)
        engine.setCrossfeedEnabled(enabled: true)
        engine.setCrossfeedStrength(strength: 0.4)
        engine.setReverbMode(mode: .sparseMultibandVelvet)
        engine.setRoomMode(mode: .outdoor)
        noiseEngine = engine
        movementController = MovementController(engine: noiseEngine)
        headCoordinator = HeadOrientationCoordinator(engine: noiseEngine)
        
        // ── Step 5: Obtain the raw engine pointer for the C-FFI render path ────
        //
        // noise_generator_engine_ptr() extracts the raw opaque pointer to the
        // NoiseEngine's inner DSP state from the UniFFI-managed object.
        //
        // We call this once here (on the main/setup thread) and store it as a
        // constant. From this point on, the render closure uses only this
        // pointer — it never touches the Swift NoiseEngine object directly.
        //
        // Why not use noiseEngine directly in the render block?
        //   Calling any UniFFI method (e.g. renderAudio) from the audio thread
        //   allocates a Vec<f32> on every callback — strictly forbidden on a
        //   real-time thread. The C-FFI path writes directly into Core Audio's
        //   pre-allocated buffer with no heap allocation and no mutex.
        enginePtr = noise_generator_engine_ptr(engine.uniffiClonePointer())

        // ── Step 6: Pre-allocate the interleaved scratch buffer ───────────────
        //
        // noise_generator_render_into() writes interleaved stereo samples into
        // a caller-supplied buffer. We size it for maxScratchFrames × 2 floats.
        //
        // Allocating here (once, on the setup thread) means the render closure
        // never calls malloc. The buffer is deallocated in deinit.
        scratchBuffer = UnsafeMutablePointer<Float>.allocate(capacity: maxScratchFrames * 2)

        // ── Step 6b: Allocate the atomic mute flag and clamp counter ────────
        muteFlag = UnsafeMutablePointer<Int32>.allocate(capacity: 1)
        muteFlag.initialize(to: 0)
        clampCounter = UnsafeMutablePointer<Int32>.allocate(capacity: 1)
        clampCounter.initialize(to: 0)

        // ── Step 7: Build the AVAudioFormat ───────────────────────────────────
        //
        // .pcmFormatFloat32
        //   The Rust engine produces 32-bit IEEE 754 samples.
        //   Using the matching type means zero sample conversion cost.
        //
        // channels: 2 (stereo)
        //   The Rust engine always produces independent L and R channels.
        //
        // interleaved: false  ← this is the critical choice
        //   AVAudioEngine's mixer node (and the underlying Audio Unit bus
        //   system on iOS) only accepts NON-interleaved (planar) input.
        //   Passing interleaved: true causes error -10868
        //   (kAudioUnitErr_FormatNotSupported) when connecting the source node
        //   to the mixer and crashes the engine at start.
        //
        //   With non-interleaved format, AVAudioEngine provides two separate
        //   Float buffers in the AudioBufferList — one per channel. We must
        //   de-interleave the Rust output (L₀R₀L₁R₁…) from the scratch buffer
        //   into [L₀L₁…] and [R₀R₁…] ourselves, but that is a simple strided
        //   copy and has negligible cost.
        guard let audioFormat = AVAudioFormat(
            commonFormat: .pcmFormatFloat32,
            sampleRate: session.sampleRate, // Double, as AVAudioFormat expects
            channels: 2,
            interleaved: false              // required by AVAudioEngine's mixer
        ) else {
            print("AudioManager: could not create AVAudioFormat")
            return
        }

        // ── Step 8: Create AVAudioSourceNode ──────────────────────────────────
        //
        // AVAudioSourceNode is a "pull" source node. AVAudioEngine calls the
        // render closure on a real-time audio thread every time it needs a new
        // buffer of samples. The closure must not:
        //   - Allocate heap memory (no Array literals, no Swift strings)
        //   - Call Objective-C runtime methods
        //   - Acquire a contended lock
        //
        // We satisfy all three constraints:
        //   - noise_generator_render_into() is allocation-free (writes into the
        //     pre-allocated scratchBuffer).
        //   - It is lock-free (uses only atomic loads internally).
        //   - The subsequent de-interleave loop is pure pointer arithmetic.
        //
        // Closure parameters:
        //   isSilence       — inout Bool; set to true to signal an all-zero
        //                     buffer without writing data. We always produce
        //                     real samples so we leave it false.
        //   timestamp       — AudioTimeStamp of the first sample; unused for
        //                     a generative source.
        //   frameCount      — number of frames requested. One frame = one L+R
        //                     sample pair. Varies per callback; typically 512.
        //   audioBufferList — UnsafeMutablePointer to the AudioBufferList we
        //                     must fill. For non-interleaved stereo this
        //                     contains two AudioBuffers: index 0 = left channel,
        //                     index 1 = right channel.
        let capturedEnginePtr    = enginePtr
        let capturedScratch      = scratchBuffer
        let capturedMaxFrames    = maxScratchFrames
        let capturedMuteFlag     = muteFlag
        let capturedClampCounter = clampCounter

        let node = AVAudioSourceNode(format: audioFormat) { _, _, frameCount, audioBufferList -> OSStatus in

            // ── Guard: mute flag — output silence during preset switches ─────
            //
            // When the main thread is reconfiguring the Rust engine (applying a
            // new preset), the mute flag is set to 1. The render closure checks
            // it and outputs silence to avoid reading partially-configured state.
            if capturedMuteFlag.pointee != 0 {
                let abl = UnsafeMutableAudioBufferListPointer(audioBufferList)
                for buf in abl {
                    if let data = buf.mData {
                        memset(data, 0, Int(buf.mDataByteSize))
                    }
                }
                return noErr
            }

            // ── Guard: engine pointer must be valid ──────────────────────────────
            //
            // enginePtr is obtained once during init via
            // noise_generator_engine_ptr(). If the call failed (returned nil),
            // we must not pass a null pointer to noise_generator_render_into()
            // — that would be undefined behaviour (likely EXC_BAD_ACCESS).
            // Instead, zero-fill the output buffers so the DAC plays silence.
            guard let enginePtr = capturedEnginePtr else {
                let abl = UnsafeMutableAudioBufferListPointer(audioBufferList)
                for buf in abl {
                    if let data = buf.mData {
                        memset(data, 0, Int(buf.mDataByteSize))
                    }
                }
                return noErr
            }

            // ── Render into the pre-allocated interleaved scratch buffer ───────
            //
            // noise_generator_render_into() is the lock-free, allocation-free
            // C-FFI render path exported by the XCFramework alongside the
            // UniFFI symbols. It writes `frameCount` stereo frames directly
            // into `capturedScratch` as interleaved samples:
            //   [L₀, R₀, L₁, R₁, … Lₙ₋₁, Rₙ₋₁]
            // Total elements written = frameCount × 2.
            //
            // No heap allocation. No mutex. No UniFFI serialisation overhead.
            // Clamp to the scratch buffer capacity to prevent a heap overflow
            // if AVAudioEngine requests more than maxScratchFrames frames (e.g.
            // during internal reconfiguration on pause/resume), which would
            // corrupt adjacent heap memory and cause EXC_BAD_ACCESS.
            let safeFrameCount = min(frameCount, UInt32(capturedMaxFrames))
            if safeFrameCount < frameCount {
                // Increment the clamp counter so the main thread can detect
                // that we had to drop frames. Lock-free: single writer
                // (audio thread), single reader (main thread).
                capturedClampCounter.pointee &+= 1
            }
            noise_generator_render_into(enginePtr, capturedScratch, safeFrameCount)

            let frameCountInt = Int(safeFrameCount)

            // ── De-interleave into the two planar channel buffers ─────────────
            //
            // AVAudioEngine provides two separate Float buffers in the
            // AudioBufferList for non-interleaved (planar) format:
            //   ablPointer[0].mData — left  channel plane
            //   ablPointer[1].mData — right channel plane
            //
            // We scatter the interleaved scratch output into these planes.
            // scratchBuffer[2*i]   = left  sample for frame i
            // scratchBuffer[2*i+1] = right sample for frame i
            let totalFrames = Int(frameCount)
            let ablPointer = UnsafeMutableAudioBufferListPointer(audioBufferList)

            guard ablPointer.count >= 2,
                  let leftData  = ablPointer[0].mData?.bindMemory(to: Float.self, capacity: totalFrames),
                  let rightData = ablPointer[1].mData?.bindMemory(to: Float.self, capacity: totalFrames)
            else {
                // Zero-fill all buffers so the DAC plays silence instead of
                // whatever garbage was left in the buffer.
                let abl = UnsafeMutableAudioBufferListPointer(audioBufferList)
                for buf in abl {
                    if let data = buf.mData {
                        memset(data, 0, Int(buf.mDataByteSize))
                    }
                }
                return noErr
            }

            AudioManager.deinterleaveAndZeroFill(
                scratch: capturedScratch,
                renderedFrames: frameCountInt,
                totalFrames: totalFrames,
                left: leftData,
                right: rightData
            )

            return noErr
        }

        sourceNode = node

        // ── Step 9: Wire the graph ────────────────────────────────────────────
        //
        // AVAudioEngine uses a simple directed graph.
        // sourceNode → mainMixerNode → outputNode (hardware speaker/headphones)
        //
        // The mainMixerNode handles volume mixing; the outputNode drives the
        // hardware. We pass `audioFormat` to the connect call so AVAudioEngine
        // knows the format on this bus without having to infer it.
        audioEngine.attach(node)
        audioEngine.connect(node, to: audioEngine.mainMixerNode, format: audioFormat)

        // ── Step 10: Start the engine ─────────────────────────────────────────
        //
        // prepare() pre-allocates internal buffers. Optional, but reduces the
        // risk of a glitch or underrun on the very first audio callback.
        //
        // start() is the last step; once it returns, the render closure will
        // be called continuously by the audio thread for as long as the engine
        // is running — which is the entire app lifetime.
        audioEngine.prepare()
        do {
            // Start the engine silently. AudioTransitionManager will fade in
            // from 0 → 1 when the user first hits Play.
            audioEngine.mainMixerNode.outputVolume = 0.0
            try audioEngine.start()
            // isPlaying intentionally left false — user must press Play.
        } catch {
            print("AudioManager: AVAudioEngine start failed: \(error)")
        }

        // ── Step 11: Register for route-change and config-change notifications ─
        setupRouteChangeHandling()
        setupConfigurationChangeHandling()

        // ── Step 12: Start head tracking ─────────────────────────────────────
        //
        // First-launch flow:
        //   - If the intro hasn't been dismissed yet → start in `.simulated`
        //     (no prompt, immediate externalisation) and flag the UI to show
        //     the intro sheet. On Enable we transition to `.auto`, which
        //     either fires the iOS motion-permission prompt (AirPods present)
        //     or silently falls back to simulated (no AirPods).
        //   - On subsequent launches → load the persisted preference and
        //     apply it. If nothing is persisted (corner case), default to
        //     `.auto`. iOS caches the auth answer, so no surprise prompt.
        let introAlreadyShown = UserDefaults.standard.bool(forKey: Self.introShownDefaultsKey)

        if !introAlreadyShown {
            needsHeadTrackingIntro = true
            applyMode(.simulated, persist: false)
        } else {
            let preference = Self.loadHeadTrackingPreference() ?? .auto
            applyMode(preference, persist: false)
        }

        // ── Step 13: Apply the default preset ────────────────────────────────
        //
        // The Rust engine starts in a constructor-default state — its built-in
        // anchor is active at the default volume (~loud), no spatial objects
        // are configured, and master gain is whatever we passed to `init`
        // (0.7), NOT the per-preset gain encoded in the `setPreset` call.
        //
        // Every `NoisePresetProvider.setPreset` explicitly silences the anchor
        // (`setAnchorVolume(volume: 0.0)`) and routes audio through spatial
        // objects with their own per-preset master gain. Without this final
        // call the engine would render its default anchor on first Play,
        // which is noticeably louder than any of the configured presets — the
        // user's reported "first preset sounds louder than the rest" bug.
        //
        // Mirrors `AudioManagerMac.init` which already does this. `applyPreset`
        // uses `muteFlag` to prevent any click during reconfiguration, and the
        // mixer is at volume 0 here anyway so nothing is audible until the
        // user hits Play.
        applyPreset(.normalShield1)
    }

    // MARK: - Route & configuration change handling

    private func setupRouteChangeHandling() {
        routeChangeObserver = NotificationCenter.default.addObserver(
            forName: AVAudioSession.routeChangeNotification,
            object: AVAudioSession.sharedInstance(),
            queue: .main
        ) { [weak self] notification in
            self?.handleRouteChange(notification)
        }
    }

    private func setupConfigurationChangeHandling() {
        configChangeObserver = NotificationCenter.default.addObserver(
            forName: .AVAudioEngineConfigurationChange,
            object: audioEngine,
            queue: .main
        ) { [weak self] _ in
            self?.handleConfigurationChange()
        }
    }

    /// Called when the audio route changes (headphones plugged/unplugged,
    /// Bluetooth connected/disconnected, etc.).
    ///
    /// Three things can go wrong:
    /// 1. The engine may have stopped — we restart it.
    /// 2. The hardware sample rate may have changed — we call
    ///    `noiseEngine.setSampleRate()` so the Rust DSP recalculates all
    ///    filter coefficients, delay buffers, and modulator timing.
    /// 3. AirPods may have just connected or disconnected — re-evaluate
    ///    the head-tracking source if we're in `.auto` mode.
    private func handleRouteChange(_ notification: Notification) {
        guard let info = notification.userInfo,
              let reasonValue = info[AVAudioSessionRouteChangeReasonKey] as? UInt,
              let reason = AVAudioSession.RouteChangeReason(rawValue: reasonValue)
        else { return }

        guard AudioManager.shouldHandleRouteChange(reason: reason) else { return }

        // Update the Rust engine's sample rate if the hardware rate changed.
        // setSampleRate() recalculates all filter coefficients, delay buffers,
        // and modulator timing on the next render block. A brief ~1 ms transient
        // may occur as filter state is reset.
        let currentRate = AVAudioSession.sharedInstance().sampleRate
        if AudioManager.isSampleRateMismatched(initial: engineSampleRate, current: currentRate) {
            print("AudioManager: hardware sample rate changed from "
                  + "\(engineSampleRate) to \(currentRate) Hz — updating Rust engine.")
            noiseEngine.setSampleRate(sampleRate: UInt32(currentRate))
            engineSampleRate = currentRate
        }

        // Re-evaluate the head-tracking source. If AirPods just connected,
        // swap from simulated to real tracking; if they just disconnected,
        // swap the other way. No-op outside `.auto`.
        headCoordinator.refreshAutoSource()

        // Restart the engine if it stopped due to the route change.
        // Guard: skip if AudioTransitionManager paused us due to an
        // interruption — let the interruption-end handler resume instead.
        guard isPlaying else { return }
        if !audioEngine.isRunning {
            restartEngineWithMute()
        }
    }

    /// Called when iOS invalidates the audio graph (e.g. media server reset,
    /// hardware reconfiguration). The engine is stopped at this point and
    /// must be restarted to resume audio output.
    private func handleConfigurationChange() {
        print("AudioManager: AVAudioEngine configuration changed — restarting.")
        guard isPlaying else { return }
        if !audioEngine.isRunning {
            restartEngineWithMute()
        }
    }

    /// Restarts the engine with mute flag protection to prevent clicks from
    /// the render callback reading partially-ready state.
    private func restartEngineWithMute() {
        do {
            muteFlag.pointee = 1
            audioEngine.mainMixerNode.outputVolume = 0.0
            try AVAudioSession.sharedInstance().setActive(true)
            audioEngine.prepare()
            try audioEngine.start()
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) { [weak self] in
                guard let self else { return }
                self.muteFlag.pointee = 0
                self.audioEngine.mainMixerNode.outputVolume = self.isPlaying ? 1.0 : 0.0
            }
        } catch {
            print("AudioManager: engine restart failed: \(error)")
            muteFlag.pointee = 0
        }
    }

    // MARK: - Head tracking

    /// Change the head-tracking mode. Persists the choice so it sticks
    /// across app launches. Coordinator swaps sources internally.
    func setHeadTrackingMode(_ mode: HeadOrientationCoordinator.Mode) {
        applyMode(mode, persist: true)
    }

    /// Single chokepoint for mode mutations — keeps the @Observable property,
    /// the coordinator, and persisted UserDefaults in lockstep. Pass
    /// `persist: false` for transient internal transitions (e.g. starting
    /// in `.simulated` for the intro period before the user has chosen).
    private func applyMode(_ mode: HeadOrientationCoordinator.Mode, persist: Bool) {
        headTrackingMode = mode
        headCoordinator.setMode(mode)
        if persist {
            UserDefaults.standard.set(mode.rawValue, forKey: Self.preferenceDefaultsKey)
        }
    }

    /// Capture the current yaw as the new zero.
    func recenterHead() {
        headCoordinator.recenter()
    }

    /// Switch the simulated-drift algorithm. Live-swap if simulated is
    /// currently active; otherwise takes effect next time we fall back to
    /// simulated.
    func setSimulationAlgorithm(_ kind: HeadOrientationCoordinator.SimulationAlgorithmKind) {
        headCoordinator.setSimulationAlgorithm(kind)
    }

    /// Called when the app becomes active after the user may have changed
    /// Motion permission in Settings.
    func retryHeadTrackingAfterExternalPermissionChange() {
        headCoordinator.retryAirPodsAfterExternalPermissionChange()
    }

    /// Called by the UI when the user dismisses the head-tracking intro.
    /// - Parameter enableAirPods: `true` to opt in (persists `.auto`, which
    ///   triggers the iOS motion-permission prompt on the next start).
    ///   `false` to stay on simulated drift (persists `.simulated`).
    /// Either way the intro is marked shown so it doesn't appear again.
    func dismissHeadTrackingIntro(enableAirPods: Bool) {
        UserDefaults.standard.set(true, forKey: Self.introShownDefaultsKey)
        needsHeadTrackingIntro = false
        setHeadTrackingMode(enableAirPods ? .auto : .simulated)
    }

    #if DEBUG
    var debugHeadYaw: Float   { noiseEngine.headYaw() }
    var debugHeadPitch: Float { noiseEngine.headPitch() }
    var debugHeadRoll: Float  { noiseEngine.headRoll() }
    var debugHeadOrientationActive: Bool { noiseEngine.headOrientationActive() }
    #endif

    // MARK: - Global volume control (used by AudioTransitionManager)

    /// Current output volume of the main mixer (0.0 – 1.0).
    var outputVolume: Float {
        audioEngine.mainMixerNode.outputVolume
    }

    /// Sets the output volume of the main mixer node.
    /// Called from AudioTransitionManager's render-loop tick — must be cheap.
    func setOutputVolume(_ volume: Float) {
        audioEngine.mainMixerNode.outputVolume = volume
    }

    /// Stops the audio engine, releasing the render thread and saving battery.
    /// Uses `stop()` rather than `pause()` so that the real-time audio thread
    /// is fully torn down during idle — important for battery life when the
    /// user pauses for hours.
    /// Safe to call while volume is already at 0.
    func pauseEngine() throws {
        audioEngine.stop()
        isPlaying = false
    }

    /// Resumes the audio engine after a stop.
    /// `prepare()` is called to re-allocate internal buffers that `stop()`
    /// released, then `start()` resumes the render callbacks.
    func resumeEngine() throws {
        audioEngine.prepare()
        try audioEngine.start()
        isPlaying = true
    }

    // MARK: - Preset dispatch

    /// Unified dispatcher — maps an `ActivePreset` value to the correct
    /// apply method. Used by `NowPlayingManager` for remote-command preset
    /// switching so callers don't need to switch on the enum themselves.
    ///
    /// Sets the atomic mute flag before reconfiguring the Rust engine so
    /// the render closure outputs silence during the transition, then
    /// clears the flag when done. This prevents the audio thread from
    /// reading partially-configured state (which would produce clicks).
    func applyPreset(_ preset: ActivePreset) {
        muteFlag.pointee = 1
        defer { muteFlag.pointee = 0 }
        
        clearCurrentPreset()
        
        preset.presetProvider.setPreset(
            engine: noiseEngine,
            movementController: movementController
        )
        
        activePreset = preset
    }
    
    private func clearCurrentPreset() {
        resetPresetState(
            engine: noiseEngine,
            movementController: movementController
        )
    }

    /// Advances to the next preset in declaration order (wraps around).
    func nextPreset() {
        let all = ActivePreset.allCases
        guard let idx = all.firstIndex(of: activePreset) else { return }
        applyPreset(all[(idx + 1) % all.count])
    }

    /// Moves to the previous preset in declaration order (wraps around).
    func previousPreset() {
        let all = ActivePreset.allCases
        guard let idx = all.firstIndex(of: activePreset) else { return }
        applyPreset(all[(idx - 1 + all.count) % all.count])
    }

    // MARK: - Route change helpers (extracted for testability)

    /// Returns `true` if the given route-change reason is one that can
    /// affect our audio playback and should trigger an engine restart check.
    static func shouldHandleRouteChange(
        reason: AVAudioSession.RouteChangeReason
    ) -> Bool {
        switch reason {
        case .newDeviceAvailable, .oldDeviceUnavailable, .override,
             .categoryChange, .routeConfigurationChange:
            return true
        default:
            return false
        }
    }

    /// Returns `true` if the current hardware sample rate differs from
    /// the rate the Rust engine was initialised with. A mismatch means
    /// filter coefficients and LFO rates will be slightly off.
    static func isSampleRateMismatched(
        initial: Double,
        current: Double
    ) -> Bool {
        initial != current
    }

    // MARK: - Render helpers (extracted for testability)

    /// De-interleaves stereo samples from `scratch` into planar `left`/`right`
    /// buffers, and zero-fills any remaining frames beyond `renderedFrames`
    /// up to `totalFrames`.
    ///
    /// This is the pure-logic core of the AVAudioSourceNode render callback,
    /// extracted as a static method so it can be unit-tested without an audio
    /// engine.
    ///
    /// - Parameters:
    ///   - scratch: Interleaved stereo buffer [L₀,R₀,L₁,R₁,…]. Must contain
    ///     at least `renderedFrames * 2` elements.
    ///   - renderedFrames: Number of frames actually produced by the DSP engine
    ///     (may be less than `totalFrames` if clamped to scratch capacity).
    ///   - totalFrames: Number of frames the system requested (= size of
    ///     `left` and `right` buffers).
    ///   - left: Destination buffer for the left channel (planar).
    ///   - right: Destination buffer for the right channel (planar).
    static func deinterleaveAndZeroFill(
        scratch: UnsafePointer<Float>,
        renderedFrames: Int,
        totalFrames: Int,
        left: UnsafeMutablePointer<Float>,
        right: UnsafeMutablePointer<Float>
    ) {
        if renderedFrames > 0 {
            // ── Bulk de-interleave with vDSP ─────────────────────────────────
            //
            // vDSP_vsadd with stride 2 is the idiomatic way to scatter an
            // interleaved buffer into two planar buffers. We add 0.0 to each
            // element (identity operation) purely to get the strided copy.
            //
            // scratch layout:  [L₀, R₀, L₁, R₁, …]
            //   Left  channel: starts at scratch[0], stride 2
            //   Right channel: starts at scratch[1], stride 2
            var zero: Float = 0
            // Left channel: scratch[0], scratch[2], scratch[4], …
            vDSP_vsadd(scratch,      2, &zero, left,  1, vDSP_Length(renderedFrames))
            // Right channel: scratch[1], scratch[3], scratch[5], …
            vDSP_vsadd(scratch + 1,  2, &zero, right, 1, vDSP_Length(renderedFrames))

            // ── Bulk NaN/Inf replacement ─────────────────────────────────────
            //
            // Replace any non-finite samples with 0.0. vDSP_vclip clamps all
            // values to [lo, hi]; NaN fails both comparisons and is replaced
            // with 0.0 by vDSP. +/-Inf gets clamped to +/-1.0.
            var lo: Float = -1.0
            var hi: Float =  1.0
            vDSP_vclip(left,  1, &lo, &hi, left,  1, vDSP_Length(renderedFrames))
            vDSP_vclip(right, 1, &lo, &hi, right, 1, vDSP_Length(renderedFrames))
        }

        // Zero-fill any frames beyond what the scratch buffer could hold.
        // Without this, the system would play uninitialised memory as audio,
        // producing clicks and artifacts — especially on Bluetooth/AirPlay
        // routes that request large buffer sizes (up to 4096 frames).
        let remaining = totalFrames - renderedFrames
        if remaining > 0 {
            let bytes = remaining * MemoryLayout<Float>.size
            memset(left  + renderedFrames, 0, bytes)
            memset(right + renderedFrames, 0, bytes)
        }
    }

    // MARK: - AudioManagerProtocol conformance

    // All required methods and properties (isPlaying, activePreset, outputVolume,
    // pauseEngine, resumeEngine, setOutputVolume, applyPreset, nextPreset,
    // previousPreset) are already declared above — no forwarding needed.

    // MARK: - Deinit

    deinit {
        // Stop the engine and detach the source node before any pointers
        // are freed. This ensures the render closure is no longer being
        // called when we deallocate enginePtr's backing memory.
        audioEngine.stop()
        if let node = sourceNode {
            audioEngine.detach(node)
        }

        if let observer = routeChangeObserver {
            NotificationCenter.default.removeObserver(observer)
        }
        if let observer = configChangeObserver {
            NotificationCenter.default.removeObserver(observer)
        }
        muteFlag.deinitialize(count: 1)
        muteFlag.deallocate()
        clampCounter.deinitialize(count: 1)
        clampCounter.deallocate()
        scratchBuffer.deallocate()
    }
}

// MARK: - Protocol conformance
extension AudioManager: AudioManagerProtocol {}
#endif
