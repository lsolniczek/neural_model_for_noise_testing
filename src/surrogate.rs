/// Surrogate model for fast approximate preset evaluation (Priority 14c).
///
/// A lightweight MLP that approximates the full simulation pipeline's
/// (genome, goal, brain_type, config_flags) → score mapping. Used for
/// pre-screening DE candidates before expensive real-pipeline validation.
///
/// # Architecture
///
/// Input (248 dims): genome[230] + goal one-hot[9] + brain_type one-hot[5] + config flags[4]
/// Hidden: Linear(248,256) → ReLU → Linear(256,256) → ReLU → Linear(256,128) → ReLU
/// Output: Linear(128,1) → Sigmoid → predicted score ∈ [0, 1]
///
/// # Inference
///
/// Hand-coded matmul+ReLU in pure Rust, no external dependencies. ~150k
/// multiply-adds per forward pass = ~5–20 µs on Apple Silicon. This is
/// ~10,000× faster than the real pipeline (~100 ms).
///
/// # Weight format
///
/// Weights are loaded from a flat f32 little-endian binary file with header:
///   [n_layers: u32, dim0: u32, dim1: u32, ..., dimN: u32]
/// followed by layer data in order: weights (row-major) then biases.
///
/// # Safety
///
/// The surrogate is NEVER used for final scoring. It only ranks
/// candidates inside the DE loop (Priority 14d). Only validated real
/// pipeline scores are allowed to replace DE parents, and the final
/// exported preset is always re-evaluated with `evaluate_preset()`.
/// When the `--surrogate` flag is off (default), this module is never
/// called.
///
/// # Refs
///
/// - Tilwani D, O'Reilly C (2024). "Benchmarking Deep Jansen-Rit Parameter
///   Inference." arXiv:2406.05002 — similar MLP for JR parameter inference.
/// - Tenne Y, Armfield SW (2009). "Surrogate-assisted DE." — top-K
///   pre-screening pattern.

use crate::brain_type::BrainType;
use crate::scoring::GoalKind;
use std::io;
use std::path::Path;

/// Number of genome dimensions (from `preset::GENOME_LEN`).
pub const GENOME_DIM: usize = crate::preset::GENOME_LEN;
/// Number of goal kinds (from `GoalKind::all().len()`).
pub const GOAL_DIM: usize = 9;
/// Number of brain types (from `BrainType::all().len()`).
pub const BRAIN_TYPE_DIM: usize = 5;
/// Config flags: assr, thalamic_gate, cet, phys_gate.
pub const CONFIG_DIM: usize = 4;
/// Total input dimension.
pub const INPUT_DIM: usize = GENOME_DIM + GOAL_DIM + BRAIN_TYPE_DIM + CONFIG_DIM;
/// Output dimension of the surrogate MLP.
pub const OUTPUT_DIM: usize = 1;
/// Trailing CSV columns after the genome values.
pub const CSV_METADATA_COLUMNS: [&str; 7] = [
    "goal_id",
    "brain_type_id",
    "assr",
    "thalamic_gate",
    "cet",
    "phys_gate",
    "score",
];

/// A dense (fully connected) layer: y = Wx + b.
#[derive(Debug, Clone)]
struct DenseLayer {
    weights: Vec<f32>,  // row-major [out_dim × in_dim]
    biases: Vec<f32>,   // [out_dim]
    in_dim: usize,
    out_dim: usize,
}

impl DenseLayer {
    /// Forward pass: y = W @ x + b, then apply activation.
    fn forward(&self, input: &[f32], output: &mut [f32]) {
        debug_assert_eq!(input.len(), self.in_dim);
        debug_assert_eq!(output.len(), self.out_dim);
        for o in 0..self.out_dim {
            let row_start = o * self.in_dim;
            let mut sum = self.biases[o];
            for i in 0..self.in_dim {
                sum += self.weights[row_start + i] * input[i];
            }
            output[o] = sum;
        }
    }
}

/// Surrogate MLP model.
pub struct SurrogateModel {
    layers: Vec<DenseLayer>,
}

impl SurrogateModel {
    /// Load a trained model from a binary weights file.
    ///
    /// File format (little-endian):
    ///   Header: n_layers (u32), then (n_layers + 1) dimension values (u32).
    ///   For a 3-hidden-layer network with dims [248, 256, 256, 128, 1],
    ///   the header is: [4, 248, 256, 256, 128, 1].
    ///
    ///   Body: for each layer i:
    ///     weights: dims[i+1] × dims[i] f32 values (row-major)
    ///     biases:  dims[i+1] f32 values
    pub fn load(path: &Path) -> io::Result<Self> {
        let data = std::fs::read(path)?;
        let mut cursor = 0;

        let read_u32 = |cursor: &mut usize| -> io::Result<u32> {
            if *cursor + 4 > data.len() {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "truncated header"));
            }
            let val = u32::from_le_bytes([data[*cursor], data[*cursor+1], data[*cursor+2], data[*cursor+3]]);
            *cursor += 4;
            Ok(val)
        };

        let read_f32 = |cursor: &mut usize| -> io::Result<f32> {
            if *cursor + 4 > data.len() {
                return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "truncated weights"));
            }
            let val = f32::from_le_bytes([data[*cursor], data[*cursor+1], data[*cursor+2], data[*cursor+3]]);
            *cursor += 4;
            Ok(val)
        };

        let n_layers = read_u32(&mut cursor)? as usize;
        if n_layers == 0 || n_layers > 10 {
            return Err(io::Error::new(io::ErrorKind::InvalidData,
                format!("invalid n_layers={n_layers}")));
        }

        let mut dims = Vec::with_capacity(n_layers + 1);
        for _ in 0..=n_layers {
            dims.push(read_u32(&mut cursor)? as usize);
        }

        if dims[0] != INPUT_DIM {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "stale surrogate weights: input dim {} does not match compiled INPUT_DIM {}. retrain the surrogate with a current generate-data CSV",
                    dims[0], INPUT_DIM
                ),
            ));
        }

        if *dims.last().unwrap_or(&0) != OUTPUT_DIM {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid surrogate weights: output dim {} does not match expected {}",
                    dims.last().copied().unwrap_or(0),
                    OUTPUT_DIM
                ),
            ));
        }

        let mut layers = Vec::with_capacity(n_layers);
        for i in 0..n_layers {
            let in_dim = dims[i];
            let out_dim = dims[i + 1];
            let n_weights = out_dim * in_dim;
            let mut weights = Vec::with_capacity(n_weights);
            for _ in 0..n_weights {
                weights.push(read_f32(&mut cursor)?);
            }
            let mut biases = Vec::with_capacity(out_dim);
            for _ in 0..out_dim {
                biases.push(read_f32(&mut cursor)?);
            }
            layers.push(DenseLayer { weights, biases, in_dim, out_dim });
        }

        if cursor != data.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "invalid surrogate weights: {} trailing bytes after layer data",
                    data.len() - cursor
                ),
            ));
        }

        Ok(SurrogateModel { layers })
    }

    /// Build the surrogate input vector from structured inputs.
    pub fn build_input(
        genome: &[f64],
        goal: GoalKind,
        brain_type: BrainType,
        assr: bool,
        thalamic_gate: bool,
        cet: bool,
        phys_gate: bool,
    ) -> Vec<f32> {
        assert_eq!(
            genome.len(),
            GENOME_DIM,
            "surrogate genome length mismatch: got {}, expected {}",
            genome.len(),
            GENOME_DIM
        );

        let mut input = Vec::with_capacity(INPUT_DIM);

        // Genome (normalized to [0,1] using bounds)
        let bounds = crate::preset::Preset::bounds();
        for (i, &val) in genome.iter().enumerate() {
            let (lo, hi) = bounds[i];
            let norm = if (hi - lo).abs() > 1e-10 {
                ((val - lo) / (hi - lo)).clamp(0.0, 1.0) as f32
            } else {
                0.5_f32
            };
            input.push(norm);
        }

        // Goal one-hot (9 dims)
        let goal_idx = GoalKind::all().iter().position(|&g| g == goal).unwrap_or(0);
        for i in 0..GOAL_DIM {
            input.push(if i == goal_idx { 1.0 } else { 0.0 });
        }

        // Brain type one-hot (5 dims)
        let bt_idx = BrainType::all().iter().position(|&b| b == brain_type).unwrap_or(0);
        for i in 0..BRAIN_TYPE_DIM {
            input.push(if i == bt_idx { 1.0 } else { 0.0 });
        }

        // Config flags (4 bools)
        input.push(if assr { 1.0 } else { 0.0 });
        input.push(if thalamic_gate { 1.0 } else { 0.0 });
        input.push(if cet { 1.0 } else { 0.0 });
        input.push(if phys_gate { 1.0 } else { 0.0 });

        input
    }

    /// Run the forward pass and return the predicted score ∈ [0, 1].
    pub fn predict(&self, input: &[f32]) -> f32 {
        if self.layers.is_empty() {
            return 0.5; // fallback
        }

        let mut current = input.to_vec();
        let mut next = Vec::new();

        for (i, layer) in self.layers.iter().enumerate() {
            next.resize(layer.out_dim, 0.0);
            layer.forward(&current, &mut next);

            if i < self.layers.len() - 1 {
                // Hidden layers: ReLU activation
                for v in &mut next {
                    *v = v.max(0.0);
                }
            } else {
                // Output layer: Sigmoid activation
                for v in &mut next {
                    *v = 1.0 / (1.0 + (-*v).exp());
                }
            }

            std::mem::swap(&mut current, &mut next);
        }

        current[0].clamp(0.0, 1.0)
    }

    /// Predict scores for a batch of inputs. Returns one score per input.
    pub fn predict_batch(&self, inputs: &[Vec<f32>]) -> Vec<f32> {
        inputs.iter().map(|inp| self.predict(inp)).collect()
    }

    /// Create a synthetic model for testing (random weights, correct shape).
    #[cfg(test)]
    fn synthetic(dims: &[usize], seed: u32) -> Self {
        let mut rng = seed;
        let mut next_f32 = || -> f32 {
            // Simple xorshift for reproducibility
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            ((rng as f32) / (u32::MAX as f32)) * 0.2 - 0.1 // small weights [-0.1, 0.1]
        };

        let mut layers = Vec::new();
        for i in 0..dims.len() - 1 {
            let in_dim = dims[i];
            let out_dim = dims[i + 1];
            let weights: Vec<f32> = (0..out_dim * in_dim).map(|_| next_f32()).collect();
            let biases: Vec<f32> = (0..out_dim).map(|_| next_f32()).collect();
            layers.push(DenseLayer { weights, biases, in_dim, out_dim });
        }
        SurrogateModel { layers }
    }

    /// Serialize this model to binary format (for test round-trip).
    #[cfg(test)]
    fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        let n_layers = self.layers.len() as u32;
        buf.extend_from_slice(&n_layers.to_le_bytes());

        // Dimensions header
        if let Some(first) = self.layers.first() {
            buf.extend_from_slice(&(first.in_dim as u32).to_le_bytes());
        }
        for layer in &self.layers {
            buf.extend_from_slice(&(layer.out_dim as u32).to_le_bytes());
        }

        // Layer data
        for layer in &self.layers {
            for &w in &layer.weights {
                buf.extend_from_slice(&w.to_le_bytes());
            }
            for &b in &layer.biases {
                buf.extend_from_slice(&b.to_le_bytes());
            }
        }
        buf
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    // ═══════════════════════════════════════════════════════════════
    // Weight loading
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn load_and_predict_round_trip() {
        // Create synthetic model, serialize, load back, verify same prediction.
        let model = SurrogateModel::synthetic(&[INPUT_DIM, 32, 16, 1], 42);
        let bytes = model.to_bytes();

        let tmp = std::env::temp_dir().join("test_surrogate_roundtrip.bin");
        std::fs::write(&tmp, &bytes).unwrap();

        let loaded = SurrogateModel::load(&tmp).unwrap();
        assert_eq!(loaded.layers.len(), model.layers.len());

        let input = vec![0.5_f32; INPUT_DIM];
        let pred_orig = model.predict(&input);
        let pred_loaded = loaded.predict(&input);
        assert_eq!(pred_orig.to_bits(), pred_loaded.to_bits(),
            "Round-trip prediction mismatch: {pred_orig} vs {pred_loaded}");

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn load_missing_file_returns_error() {
        let result = SurrogateModel::load(Path::new("/nonexistent/path/weights.bin"));
        assert!(result.is_err());
    }

    #[test]
    fn load_truncated_file_returns_error() {
        let tmp = std::env::temp_dir().join("test_surrogate_truncated.bin");
        let mut f = std::fs::File::create(&tmp).unwrap();
        f.write_all(&[1, 0, 0, 0]).unwrap(); // n_layers=1 but no dims
        drop(f);

        let result = SurrogateModel::load(&tmp);
        assert!(result.is_err());
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn load_rejects_stale_input_dimension() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(1_u32).to_le_bytes());
        bytes.extend_from_slice(&(208_u32).to_le_bytes());
        bytes.extend_from_slice(&(1_u32).to_le_bytes());
        bytes.extend_from_slice(&(0.25_f32).to_le_bytes());
        bytes.extend_from_slice(&(0.75_f32).to_le_bytes());

        let tmp = std::env::temp_dir().join("test_surrogate_stale_dim.bin");
        std::fs::write(&tmp, &bytes).unwrap();

        let result = match SurrogateModel::load(&tmp) {
            Ok(_) => panic!("stale surrogate weights should be rejected"),
            Err(err) => err,
        };
        assert_eq!(result.kind(), io::ErrorKind::InvalidData);
        let msg = result.to_string();
        assert!(msg.contains("stale surrogate weights"));
        assert!(msg.contains("retrain"));

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn bundled_surrogate_artifacts_load() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        for rel in [
            "surrogate_weights.bin",
            "surrogate_weights_small.bin",
            "surrogate_weights_med.bin",
        ] {
            let path = root.join(rel);
            let model = SurrogateModel::load(&path)
                .unwrap_or_else(|e| panic!("failed to load bundled artifact {}: {e}", path.display()));
            let input = vec![0.5_f32; INPUT_DIM];
            let score = model.predict(&input);
            assert!((0.0..=1.0).contains(&score), "bundled artifact {} produced invalid score {}", path.display(), score);
        }
    }

    // ═══════════════════════════════════════════════════════════════
    // Forward pass properties
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn output_is_in_zero_one() {
        let model = SurrogateModel::synthetic(&[INPUT_DIM, 64, 32, 1], 123);
        for seed in 0..20 {
            let input: Vec<f32> = (0..INPUT_DIM)
                .map(|i| ((i as f32 + seed as f32 * 7.3) % 1.0))
                .collect();
            let score = model.predict(&input);
            assert!(
                (0.0..=1.0).contains(&score),
                "Score {score} out of [0,1] for seed {seed}"
            );
        }
    }

    #[test]
    fn output_is_finite() {
        let model = SurrogateModel::synthetic(&[INPUT_DIM, 256, 256, 128, 1], 999);
        let input = vec![1.0_f32; INPUT_DIM]; // all-ones edge case
        let score = model.predict(&input);
        assert!(score.is_finite(), "Score must be finite, got {score}");

        let input_zero = vec![0.0_f32; INPUT_DIM];
        let score_zero = model.predict(&input_zero);
        assert!(score_zero.is_finite(), "Score must be finite for zero input, got {score_zero}");
    }

    #[test]
    fn batch_matches_individual() {
        let model = SurrogateModel::synthetic(&[INPUT_DIM, 32, 1], 77);
        let inputs: Vec<Vec<f32>> = (0..5)
            .map(|s| (0..INPUT_DIM).map(|i| ((i + s) as f32 * 0.01) % 1.0).collect())
            .collect();

        let batch_scores = model.predict_batch(&inputs);
        for (i, inp) in inputs.iter().enumerate() {
            let single = model.predict(inp);
            assert_eq!(
                single.to_bits(),
                batch_scores[i].to_bits(),
                "Batch[{i}] mismatch: single={single} batch={}",
                batch_scores[i]
            );
        }
    }

    #[test]
    fn different_inputs_produce_different_outputs() {
        let model = SurrogateModel::synthetic(&[INPUT_DIM, 64, 32, 1], 42);
        let input_a = vec![0.0_f32; INPUT_DIM];
        let mut input_b = vec![0.0_f32; INPUT_DIM];
        input_b[0] = 1.0; // change one genome value

        let score_a = model.predict(&input_a);
        let score_b = model.predict(&input_b);
        assert_ne!(
            score_a.to_bits(),
            score_b.to_bits(),
            "Different inputs should produce different scores"
        );
    }

    // ═══════════════════════════════════════════════════════════════
    // Input builder
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn build_input_correct_length() {
        let genome = vec![0.5_f64; GENOME_DIM];
        let input = SurrogateModel::build_input(
            &genome, GoalKind::Sleep, BrainType::Normal,
            true, true, false, false,
        );
        assert_eq!(input.len(), INPUT_DIM, "Input should be {INPUT_DIM} dims, got {}", input.len());
    }

    #[test]
    fn build_input_goal_one_hot_sums_to_one() {
        let genome = vec![0.5_f64; GENOME_DIM];
        let input = SurrogateModel::build_input(
            &genome, GoalKind::Focus, BrainType::Normal,
            true, true, false, false,
        );
        let goal_sum: f32 = input[GENOME_DIM..GENOME_DIM + GOAL_DIM].iter().sum();
        assert!(
            (goal_sum - 1.0).abs() < 1e-6,
            "Goal one-hot should sum to 1, got {goal_sum}"
        );
    }

    #[test]
    fn build_input_genome_normalized_to_unit() {
        let bounds = crate::preset::Preset::bounds();
        // Set genome to the midpoint of each bound
        let genome: Vec<f64> = bounds.iter().map(|(lo, hi)| (lo + hi) / 2.0).collect();
        let input = SurrogateModel::build_input(
            &genome, GoalKind::Sleep, BrainType::Normal,
            true, true, false, false,
        );
        for (i, &v) in input[..GENOME_DIM].iter().enumerate() {
            assert!(
                (v - 0.5).abs() < 0.01,
                "Midpoint genome should normalize to ~0.5, got {v} at dim {i}"
            );
        }
    }

    #[test]
    fn build_input_config_flags_correct() {
        let genome = vec![0.5_f64; GENOME_DIM];
        let input = SurrogateModel::build_input(
            &genome, GoalKind::Sleep, BrainType::Normal,
            true, false, true, false,
        );
        let flags_start = GENOME_DIM + GOAL_DIM + BRAIN_TYPE_DIM;
        assert_eq!(input[flags_start], 1.0, "assr should be 1.0");
        assert_eq!(input[flags_start + 1], 0.0, "thalamic_gate should be 0.0");
        assert_eq!(input[flags_start + 2], 1.0, "cet should be 1.0");
        assert_eq!(input[flags_start + 3], 0.0, "phys_gate should be 0.0");
    }

    // ═══════════════════════════════════════════════════════════════
    // Full architecture shape
    // ═══════════════════════════════════════════════════════════════

    #[test]
    fn production_architecture_shape() {
        // Verify the 248→256→256→128→1 architecture works end-to-end.
        let model = SurrogateModel::synthetic(&[INPUT_DIM, 256, 256, 128, 1], 42);
        assert_eq!(model.layers.len(), 4);
        assert_eq!(model.layers[0].in_dim, INPUT_DIM);
        assert_eq!(model.layers[0].out_dim, 256);
        assert_eq!(model.layers[3].out_dim, 1);

        let input = vec![0.5_f32; INPUT_DIM];
        let score = model.predict(&input);
        assert!((0.0..=1.0).contains(&score));
    }
}
