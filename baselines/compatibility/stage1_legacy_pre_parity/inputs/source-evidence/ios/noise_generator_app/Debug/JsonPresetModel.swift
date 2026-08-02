//
//  JsonPresetModel.swift
//  noise_generator_app
//
//  DEBUG-only: Codable model for loading preset JSON files.
//

#if DEBUG
import Foundation

struct JsonPresetModel: Decodable {
    let master_gain: Float
    let spatial_mode: Int?
    let source_count: Int?
    let anchor_color: Int
    let anchor_volume: Float
    let environment: Int
    let objects: [JsonObjectModel]
}

/// Wrapper for optimizer output format where preset is nested under a "preset" key.
struct JsonPresetWrapper: Decodable {
    let preset: JsonPresetModel
}

struct JsonObjectModel: Decodable {
    let active: Bool
    let color: Int
    let x: Float
    let y: Float
    let z: Float
    let volume: Float
    let reverb_send: Float
    let bass_mod: JsonModulator
    let satellite_mod: JsonModulator
    let movement: JsonMovement
    let tint_freq: Float?
    let tint_db: Float?
}

struct JsonModulator: Decodable {
    let kind: Int
    let param_a: Float
    let param_b: Float
    let param_c: Float
}

struct JsonMovement: Decodable {
    let kind: Int
    let radius: Float
    let speed: Float
    let phase: Float
    let depth_min: Float
    let depth_max: Float
    let reverb_min: Float
    let reverb_max: Float
}

// MARK: - Enum mapping (JSON 0-based → Swift enums)

extension JsonPresetModel {

    /// Maps JSON integer (0-based) to NoiseColor.
    static func noiseColor(from json: Int) -> NoiseColor {
        switch json {
        case 0: .white
        case 1: .pink
        case 2: .brown
        case 3: .green
        case 4: .grey
        case 5: .black
        case 6: .ssn
        case 7: .blue
        default: .white
        }
    }

    /// Maps JSON integer (0-based) to AcousticEnvironment.
    static func acousticEnvironment(from json: Int) -> AcousticEnvironment {
        switch json {
        case 0: .anechoicChamber
        case 1: .focusRoom
        case 2: .openLounge
        case 3: .vastSpace
        case 4: .deepSanctuary
        default: .anechoicChamber
        }
    }

    /// Maps JSON integer (0-based) to ModulatorKind.
    static func modulatorKind(from json: Int) -> ModulatorKind {
        switch json {
        case 0: .flat
        case 1: .sineLfo
        case 2: .breathing
        case 3: .stochastic
        case 4: .neuralLfo
        default: .flat
        }
    }

    /// Maps JSON integer (1-based) to MovementPattern.
    static func movementPattern(from json: Int) -> MovementPattern {
        switch json {
        case 1: .orbit
        case 2: .figureEight
        case 3: .randomWalk
        case 4: .depthBreathing
        case 5: .pendulum
        default: .orbit
        }
    }
}
#endif
