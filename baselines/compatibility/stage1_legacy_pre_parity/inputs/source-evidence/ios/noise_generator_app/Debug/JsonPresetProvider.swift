//
//  JsonPresetProvider.swift
//  noise_generator_app
//
//  DEBUG-only: Loads a preset from a JSON file on disk and applies it
//  to the engine. Also prints the equivalent Swift PresetProvider code
//  to the Xcode console for copy-pasting into a production preset.
//

#if DEBUG
import Foundation

struct JsonPresetProvider: NoisePresetProvider {

    private let model: JsonPresetModel

    init(from url: URL) throws {
        let data = try Data(contentsOf: url)
        let decoder = JSONDecoder()
        // Support both flat format and optimizer wrapper ({"preset": {...}})
        if let wrapper = try? decoder.decode(JsonPresetWrapper.self, from: data) {
            model = wrapper.preset
        } else {
            model = try decoder.decode(JsonPresetModel.self, from: data)
        }
    }

    func setPreset(engine: NoiseEngine, movementController: MovementController) {
        engine.setMasterGain(gain: model.master_gain)
        if let sourceCount = model.source_count {
            engine.setSourceCount(sourceCount: UInt32(sourceCount))
        }
        engine.setAnchorColor(
            color: JsonPresetModel.noiseColor(from: model.anchor_color)
        )
        engine.setAnchorVolume(volume: model.anchor_volume)
        engine.setAcousticEnvironment(
            environment: JsonPresetModel.acousticEnvironment(from: model.environment)
        )

        for (i, obj) in model.objects.enumerated() {
            let idx = UInt32(i)

            engine.setObject(
                index: idx,
                active: obj.active,
                color: JsonPresetModel.noiseColor(from: obj.color),
                x: obj.x, y: obj.y, z: obj.z,
                volume: obj.volume,
                reverbSend: obj.reverb_send
            )

            engine.setBassModulator(
                index: idx,
                kind: JsonPresetModel.modulatorKind(from: obj.bass_mod.kind),
                paramA: obj.bass_mod.param_a,
                paramB: obj.bass_mod.param_b,
                paramC: obj.bass_mod.param_c
            )

            engine.setSatelliteModulator(
                index: idx,
                kind: JsonPresetModel.modulatorKind(from: obj.satellite_mod.kind),
                paramA: obj.satellite_mod.param_a,
                paramB: obj.satellite_mod.param_b,
                paramC: obj.satellite_mod.param_c
            )

            if let freq = obj.tint_freq, let db = obj.tint_db, freq > 0 {
                engine.setObjectColorTint(index: idx, freqHz: freq, gainDb: db)
            }

            if obj.active && obj.movement.kind > 0 {
                movementController.addSatellite(
                    index: idx,
                    pattern: JsonPresetModel.movementPattern(from: obj.movement.kind),
                    radius: obj.movement.radius,
                    speed: Double(obj.movement.speed),
                    initialPhase: Double(obj.movement.phase)
                )
            }
        }

        movementController.start()
        printGeneratedCode()
    }

    // MARK: - Code generation

    private func printGeneratedCode() {
        let sourceCountLine: String
        if let sc = model.source_count {
            sourceCountLine = "\n            engine.setSourceCount(sourceCount: \(sc))"
        } else {
            sourceCountLine = ""
        }

        var code = """
        import Foundation

        struct TODOPresetProvider: NoisePresetProvider {
            func setPreset(engine: NoiseEngine, movementController: MovementController) {
                engine.setMasterGain(gain: \(model.master_gain))\(sourceCountLine)
                engine.setAnchorColor(color: \(colorName(model.anchor_color)))
                engine.setAnchorVolume(volume: \(model.anchor_volume))
                engine.setAcousticEnvironment(environment: \(envName(model.environment)))

        """

        for (i, obj) in model.objects.enumerated() {
            code += """

                    // Object \(i)
                    engine.setObject(
                        index: \(i), active: \(obj.active), color: \(colorName(obj.color)),
                        x: \(obj.x), y: \(obj.y), z: \(obj.z),
                        volume: \(obj.volume), reverbSend: \(obj.reverb_send)
                    )
                    engine.setBassModulator(
                        index: \(i), kind: \(modName(obj.bass_mod.kind)),
                        paramA: \(obj.bass_mod.param_a), paramB: \(obj.bass_mod.param_b), paramC: \(obj.bass_mod.param_c)
                    )
                    engine.setSatelliteModulator(
                        index: \(i), kind: \(modName(obj.satellite_mod.kind)),
                        paramA: \(obj.satellite_mod.param_a), paramB: \(obj.satellite_mod.param_b), paramC: \(obj.satellite_mod.param_c)
                    )

            """

            if let freq = obj.tint_freq, let db = obj.tint_db, freq > 0 {
                code += """
                        engine.setObjectColorTint(index: \(i), freqHz: \(freq), gainDb: \(db))

                """
            }

            if obj.active && obj.movement.kind > 0 {
                code += """
                        movementController.addSatellite(
                            index: \(i), pattern: \(patternName(obj.movement.kind)),
                            radius: \(obj.movement.radius), speed: \(obj.movement.speed), initialPhase: \(obj.movement.phase)
                        )

                """
            }
        }

        code += """

                movementController.start()
            }
        }

        """

        print("═══════════════════════════════════════════════════════")
        print("  DEBUG: Generated PresetProvider from JSON")
        print("═══════════════════════════════════════════════════════")
        print(code)
        print("═══════════════════════════════════════════════════════")
    }

    // MARK: - Name helpers for code generation

    private func colorName(_ json: Int) -> String {
        switch json {
        case 0: ".white"; case 1: ".pink"; case 2: ".brown"; case 3: ".green"
        case 4: ".grey"; case 5: ".black"; case 6: ".ssn"; case 7: ".blue"
        default: ".white"
        }
    }

    private func envName(_ json: Int) -> String {
        switch json {
        case 0: ".anechoicChamber"; case 1: ".focusRoom"; case 2: ".openLounge"
        case 3: ".vastSpace"; case 4: ".deepSanctuary"
        default: ".anechoicChamber"
        }
    }

    private func modName(_ json: Int) -> String {
        switch json {
        case 0: ".flat"; case 1: ".sineLfo"; case 2: ".breathing"
        case 3: ".stochastic"; case 4: ".neuralLfo"
        default: ".flat"
        }
    }

    private func patternName(_ json: Int) -> String {
        switch json {
        case 1: ".orbit"; case 2: ".figureEight"; case 3: ".randomWalk"
        case 4: ".depthBreathing"; case 5: ".pendulum"
        default: ".orbit"
        }
    }
}
#endif
