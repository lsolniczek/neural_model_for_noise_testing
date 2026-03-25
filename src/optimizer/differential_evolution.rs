/// Differential Evolution (DE/rand/1/bin) optimizer.
///
/// Well-suited for the mixed continuous/discrete parameter space of noise
/// presets. Population-based, gradient-free, handles non-convex landscapes.

use rand::prelude::*;
use rand_distr::Uniform;

/// Result of one evaluation.
#[derive(Clone)]
pub struct Individual {
    pub genome: Vec<f64>,
    pub fitness: f64,
}

pub struct DifferentialEvolution {
    /// Population of candidate solutions.
    population: Vec<Individual>,
    /// Best individual found so far.
    best: Individual,
    /// Parameter bounds: (min, max) per dimension.
    bounds: Vec<(f64, f64)>,
    /// Mutation scale factor.
    f: f64,
    /// Crossover probability.
    cr: f64,
    /// Current generation.
    generation: usize,
    /// RNG
    rng: StdRng,
}

impl DifferentialEvolution {
    /// Create a new DE optimizer.
    ///
    /// - `bounds`: (min, max) for each dimension
    /// - `pop_size`: population size (typically 5–10× the dimension count)
    /// - `f`: mutation scale (0.5–0.9 typical)
    /// - `cr`: crossover rate (0.7–0.9 typical)
    /// - `seed`: RNG seed for reproducibility
    pub fn new(
        bounds: Vec<(f64, f64)>,
        pop_size: usize,
        f: f64,
        cr: f64,
        seed: u64,
    ) -> Self {
        let dim = bounds.len();
        let mut rng = StdRng::seed_from_u64(seed);

        // Initialise population with uniform random samples
        let population: Vec<Individual> = (0..pop_size)
            .map(|_| {
                let genome: Vec<f64> = bounds
                    .iter()
                    .map(|(lo, hi)| rng.sample(Uniform::new(*lo, *hi)))
                    .collect();
                Individual {
                    genome,
                    fitness: f64::NEG_INFINITY,
                }
            })
            .collect();

        let best = Individual {
            genome: vec![0.0; dim],
            fitness: f64::NEG_INFINITY,
        };

        DifferentialEvolution {
            population,
            best,
            bounds,
            f,
            cr,
            generation: 0,
            rng,
        }
    }

    /// Get all individuals that need evaluation (fitness == NEG_INFINITY).
    pub fn pending_evaluations(&self) -> Vec<(usize, Vec<f64>)> {
        self.population
            .iter()
            .enumerate()
            .filter(|(_, ind)| ind.fitness == f64::NEG_INFINITY)
            .map(|(i, ind)| (i, ind.genome.clone()))
            .collect()
    }

    /// Report fitness for an individual.
    pub fn report_fitness(&mut self, index: usize, fitness: f64) {
        self.population[index].fitness = fitness;
        if fitness > self.best.fitness {
            self.best = self.population[index].clone();
        }
    }

    /// Run one generation of DE. Returns trial vectors to evaluate.
    ///
    /// Call `report_trial_results()` after evaluating each trial.
    pub fn generate_trials(&mut self) -> Vec<(usize, Vec<f64>)> {
        let pop_size = self.population.len();
        let dim = self.bounds.len();
        let mut trials = Vec::with_capacity(pop_size);

        for i in 0..pop_size {
            // Select three distinct random individuals (not i)
            let (a, b, c) = self.pick_three(i);

            // Mutation: donor = a + F * (b - c)
            let mut trial = vec![0.0; dim];
            let j_rand = self.rng.gen_range(0..dim);

            for j in 0..dim {
                // Binomial crossover
                if self.rng.gen::<f64>() < self.cr || j == j_rand {
                    let mutant = self.population[a].genome[j]
                        + self.f
                            * (self.population[b].genome[j] - self.population[c].genome[j]);
                    // Bounce-back boundary handling
                    trial[j] = self.bounce_back(j, mutant);
                } else {
                    trial[j] = self.population[i].genome[j];
                }
            }

            trials.push((i, trial));
        }

        self.generation += 1;
        trials
    }

    /// Report trial evaluation results. Replaces parent if trial is better.
    pub fn report_trial_result(&mut self, target_index: usize, trial_genome: Vec<f64>, trial_fitness: f64) {
        if trial_fitness >= self.population[target_index].fitness {
            self.population[target_index] = Individual {
                genome: trial_genome,
                fitness: trial_fitness,
            };
            if trial_fitness > self.best.fitness {
                self.best = self.population[target_index].clone();
            }
        }
    }

    /// Best individual found so far.
    pub fn best(&self) -> &Individual {
        &self.best
    }

    pub fn generation(&self) -> usize {
        self.generation
    }

    /// Mean fitness of current population.
    pub fn mean_fitness(&self) -> f64 {
        let valid: Vec<f64> = self.population.iter()
            .filter(|i| i.fitness > f64::NEG_INFINITY)
            .map(|i| i.fitness)
            .collect();
        if valid.is_empty() {
            return 0.0;
        }
        valid.iter().sum::<f64>() / valid.len() as f64
    }

    /// Fitness standard deviation of current population.
    pub fn fitness_std(&self) -> f64 {
        let valid: Vec<f64> = self.population.iter()
            .filter(|i| i.fitness > f64::NEG_INFINITY)
            .map(|i| i.fitness)
            .collect();
        if valid.len() < 2 {
            return 0.0;
        }
        let mean = valid.iter().sum::<f64>() / valid.len() as f64;
        let var = valid.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / valid.len() as f64;
        var.sqrt()
    }

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn pick_three(&mut self, exclude: usize) -> (usize, usize, usize) {
        let pop_size = self.population.len();
        let mut a = exclude;
        while a == exclude {
            a = self.rng.gen_range(0..pop_size);
        }
        let mut b = exclude;
        while b == exclude || b == a {
            b = self.rng.gen_range(0..pop_size);
        }
        let mut c = exclude;
        while c == exclude || c == a || c == b {
            c = self.rng.gen_range(0..pop_size);
        }
        (a, b, c)
    }

    /// Bounce-back boundary handling: if mutant is out of bounds,
    /// reflect it back from the boundary.
    fn bounce_back(&self, dim: usize, value: f64) -> f64 {
        let (lo, hi) = self.bounds[dim];
        if value < lo {
            lo + self.rng.clone().gen::<f64>() * (hi - lo) * 0.1
        } else if value > hi {
            hi - self.rng.clone().gen::<f64>() * (hi - lo) * 0.1
        } else {
            value
        }
    }
}
