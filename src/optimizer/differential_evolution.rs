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
    /// Display-only / legacy scalar. In legacy mode this is the value
    /// reported via `report_fitness` and used by the greedy comparator.
    /// In constrained mode it mirrors `neural_fitness` for backward
    /// compatibility with read-only callers (`mean_fitness`, etc.).
    pub fitness: f64,
    /// Priority 28 Phase 2: primary objective used by the ε-constrained
    /// comparator. Equal to `fitness` in legacy mode.
    pub neural_fitness: f64,
    /// Priority 28 Phase 2: aggregated comfort violation in [0, ~0.65].
    /// Always 0 in legacy mode (no comfort constraints applied), so the
    /// ε-comparator at any ε ≥ 0 reduces to fitness-only ranking.
    /// Unevaluated individuals carry `INFINITY` so they never dominate
    /// evaluated ones.
    pub violation: f64,
}

impl Individual {
    /// Construct an unevaluated placeholder. `fitness` and `neural_fitness`
    /// start at `NEG_INFINITY` (worst possible objective); `violation`
    /// starts at `INFINITY` (always infeasible until evaluated).
    fn unevaluated(genome: Vec<f64>) -> Self {
        Individual {
            genome,
            fitness: f64::NEG_INFINITY,
            neural_fitness: f64::NEG_INFINITY,
            violation: f64::INFINITY,
        }
    }
}

/// ε schedule for the Takahama & Sakai (2009) constrained DE comparator.
///
/// At generation `t`, the ε threshold is
///   `ε(t) = ε_0 · max(0, 1 − t / t_c)`     (linear decay)
/// per the Priority 28 §28f spec. Individuals with `violation ≤ ε(t)` are
/// considered ε-feasible. The schedule reaches 0 at `t_c`, after which
/// strict feasibility is required.
#[derive(Debug, Clone, Copy)]
struct EpsSchedule {
    /// Initial ε (typically the 70th percentile of initial-population
    /// violations, set externally via `enable_eps_constrained`).
    eps_0: f64,
    /// Generation at which ε first reaches 0. Typical: 0.5 · max_gens.
    t_c: usize,
}

/// Stagnation-triggered partial restart (Priority 28 §28g).
///
/// When the best fitness has not improved for `window` generations,
/// reseed the worst `fraction` of the population with uniform-random
/// genomes and `Individual::unevaluated()` state. The next generation's
/// trial replacement naturally re-evaluates them (any finite fitness
/// beats `NEG_INFINITY`). The current best individual is always
/// preserved (elitism).
///
/// Per Sallam et al. 2025 (ARRDE, arXiv:2511.18429): a simple restart
/// trigger materially mitigates premature convergence in L-SHADE-family
/// variants on high-dimensional spaces — the failure mode the priority
/// spec flags for our 230-D genome.
#[derive(Debug, Clone, Copy)]
struct StagnationConfig {
    window: usize,
    fraction: f64,
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
    /// Indices of discrete (integer-valued) dimensions to round after mutation.
    discrete_dims: Vec<usize>,
    /// Priority 28 Phase 2: optional ε-constrained schedule. `None` means
    /// legacy mode (greedy `>=` comparator on `fitness`). When set, the
    /// constrained comparator uses `current_eps()` for ranking.
    eps_schedule: Option<EpsSchedule>,
    /// Priority 28 Phase 3: when true, `generate_trials` redirects each
    /// trial's target to the nearest-genome parent (Thomsen 2004).
    crowding_enabled: bool,
    /// Priority 28 Phase 3: optional stagnation-triggered restart config.
    /// When set, `generate_trials` checks for stagnation between
    /// generations and reseeds the worst `fraction` of the population
    /// when stale.
    stagnation_config: Option<StagnationConfig>,
    /// Stagnation tracking — last best fitness observed at the start of
    /// `generate_trials`. Used to detect "no improvement this generation".
    stagnation_last_best: f64,
    /// Consecutive generations without improvement.
    stagnation_count: usize,
    /// How many times the stagnation restart has fired so far. Visible
    /// for tests and progress display; never decreases.
    stagnation_restart_count: usize,
}

impl DifferentialEvolution {
    #[inline]
    fn sample_within_bound(rng: &mut StdRng, lo: f64, hi: f64) -> f64 {
        if hi <= lo {
            lo
        } else {
            rng.sample(Uniform::new(lo, hi))
        }
    }

    /// Create a new DE optimizer.
    ///
    /// - `bounds`: (min, max) for each dimension
    /// - `pop_size`: population size (typically 5–10× the dimension count)
    /// - `f`: mutation scale (0.5–0.9 typical)
    /// - `cr`: crossover rate (0.7–0.9 typical)
    /// - `seed`: RNG seed for reproducibility
    pub fn new(bounds: Vec<(f64, f64)>, pop_size: usize, f: f64, cr: f64, seed: u64) -> Self {
        Self::with_discrete(bounds, pop_size, f, cr, seed, Vec::new())
    }

    /// Create a new DE optimizer with discrete dimension indices.
    ///
    /// Genes at `discrete_dims` indices are rounded to the nearest integer
    /// after mutation and crossover, preventing the algorithm from wasting
    /// evaluations on continuous values that map to the same discrete setting.
    pub fn with_discrete(
        bounds: Vec<(f64, f64)>,
        pop_size: usize,
        f: f64,
        cr: f64,
        seed: u64,
        discrete_dims: Vec<usize>,
    ) -> Self {
        let dim = bounds.len();
        let mut rng = StdRng::seed_from_u64(seed);

        // Initialise population with uniform random samples
        let population: Vec<Individual> = (0..pop_size)
            .map(|_| {
                let genome: Vec<f64> = bounds
                    .iter()
                    .map(|(lo, hi)| Self::sample_within_bound(&mut rng, *lo, *hi))
                    .collect();
                Individual::unevaluated(genome)
            })
            .collect();

        let best = Individual::unevaluated(vec![0.0; dim]);

        let mut de = DifferentialEvolution {
            population,
            best,
            bounds,
            f,
            cr,
            generation: 0,
            rng,
            discrete_dims,
            eps_schedule: None,
            crowding_enabled: false,
            stagnation_config: None,
            stagnation_last_best: f64::NEG_INFINITY,
            stagnation_count: 0,
            stagnation_restart_count: 0,
        };

        // Round discrete genes in the initial population
        de.round_discrete_all();
        de
    }

    /// Replace the population with perturbations of a seed genome.
    ///
    /// The seed itself is placed at index 0. The rest of the population is
    /// generated by perturbing each gene by ±perturbation_frac of its range,
    /// clamped to bounds.
    pub fn seed_from_genome(&mut self, seed: &[f64], perturbation_frac: f64) {
        for (i, ind) in self.population.iter_mut().enumerate() {
            if i == 0 {
                ind.genome = seed.to_vec();
            } else {
                ind.genome = seed
                    .iter()
                    .zip(self.bounds.iter())
                    .map(|(g, (lo, hi))| {
                        let range = hi - lo;
                        let delta = self
                            .rng
                            .sample(Uniform::new(-perturbation_frac, perturbation_frac))
                            * range;
                        if range <= 0.0 {
                            *lo
                        } else {
                            (g + delta).clamp(*lo, *hi)
                        }
                    })
                    .collect();
            }
            // Reset to "unevaluated" so the next pending_evaluations() pass
            // picks them up. Both legacy and constrained-mode bookkeeping
            // share this reset because pending_evaluations checks `fitness`.
            ind.fitness = f64::NEG_INFINITY;
            ind.neural_fitness = f64::NEG_INFINITY;
            ind.violation = f64::INFINITY;
        }
        self.round_discrete_all();
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

    /// Report fitness for an individual (legacy mode).
    ///
    /// Sets `neural_fitness = fitness` and `violation = 0.0` so the
    /// ε-comparator at any ε ≥ 0 reduces to fitness-only ranking; this
    /// preserves identical legacy behavior for callers that never enable
    /// constrained mode.
    pub fn report_fitness(&mut self, index: usize, fitness: f64) {
        self.population[index].fitness = fitness;
        self.population[index].neural_fitness = fitness;
        self.population[index].violation = 0.0;
        if fitness > self.best.fitness {
            self.best = self.population[index].clone();
        }
    }

    // ─────────────────────────────────────────────────────────────────────
    // Priority 28 Phase 2 — ε-constrained DE API
    //
    // Per Takahama & Sakai (2009), "Constrained Optimization by ε
    // Constrained Differential Evolution with Dynamic ε-Level Control."
    // The legacy methods above continue to work; constrained mode is
    // opt-in via `enable_eps_constrained` and the `report_*_constrained`
    // entry points.
    // ─────────────────────────────────────────────────────────────────────

    /// Activate ε-constrained mode with a linear decay schedule.
    ///
    /// `eps_0` should be set from the violation distribution of the
    /// initial population — typical: the 70th percentile (Priority 28
    /// §28f). `t_c` is the generation at which ε first reaches 0; a
    /// common choice is half the maximum generation budget.
    ///
    /// **Recomputes the incumbent** under the new ε. This matters when
    /// the population was evaluated via `report_constrained` *before*
    /// the schedule was enabled: at that point `current_eps()` returned
    /// 0 and `best` tracked the strict-feasible best. Some non-strict
    /// individuals may have higher fitness AND ε-feasible violation,
    /// and they would otherwise be silently masked until they happened
    /// to be touched by a trial replacement (potentially never, if
    /// crowding selection routes trials elsewhere). The
    /// stagnation-tracking baseline is also reset so the first
    /// stagnation check measures progress relative to the recomputed
    /// incumbent — not the stale strict-feasible one.
    pub fn enable_eps_constrained(&mut self, eps_0: f64, t_c: usize) {
        assert!(eps_0.is_finite() && eps_0 >= 0.0, "eps_0 must be finite and non-negative, got {eps_0}");
        assert!(t_c > 0, "t_c must be > 0");
        self.eps_schedule = Some(EpsSchedule { eps_0, t_c });
        self.recompute_best_under_current_eps();
        self.stagnation_last_best = self.best.fitness;
        self.stagnation_count = 0;
    }

    /// Re-derive `best` by scanning the full population under the
    /// currently-effective ε. Used after `enable_eps_constrained` so
    /// that the cached incumbent reflects the schedule, and exposed
    /// publicly for callers that mutate population state outside the
    /// standard `report_*` paths.
    pub fn recompute_best_under_current_eps(&mut self) {
        let eps = self.current_eps();
        // Start from the first population member that has been evaluated
        // (finite neural_fitness). If none, leave `best` untouched.
        let mut new_best: Option<Individual> = None;
        for ind in &self.population {
            if !ind.neural_fitness.is_finite() {
                continue;
            }
            new_best = Some(match new_best.take() {
                None => ind.clone(),
                Some(curr) => {
                    if constrained_better(ind, &curr, eps) {
                        ind.clone()
                    } else {
                        curr
                    }
                }
            });
        }
        if let Some(b) = new_best {
            self.best = b;
        }
    }

    /// True if the optimizer is in ε-constrained mode.
    pub fn is_constrained(&self) -> bool {
        self.eps_schedule.is_some()
    }

    /// Current ε threshold for the comparator. Returns 0.0 in legacy
    /// mode (which combined with `violation = 0.0` for legacy reports
    /// makes the constrained comparator equivalent to fitness-only).
    pub fn current_eps(&self) -> f64 {
        match &self.eps_schedule {
            None => 0.0,
            Some(s) => {
                if self.generation >= s.t_c {
                    0.0
                } else {
                    let frac = 1.0 - (self.generation as f64 / s.t_c as f64);
                    s.eps_0 * frac.max(0.0)
                }
            }
        }
    }

    /// Suggest `eps_0` from the current population's violation
    /// distribution. Returns the value at the given quantile (0.0 = min,
    /// 1.0 = max) over individuals with finite violation. Defaults to
    /// `f64::INFINITY` when the population has no evaluated individuals.
    pub fn suggest_eps_from_population(&self, quantile: f64) -> f64 {
        let q = quantile.clamp(0.0, 1.0);
        let mut violations: Vec<f64> = self
            .population
            .iter()
            .map(|ind| ind.violation)
            .filter(|v| v.is_finite())
            .collect();
        if violations.is_empty() {
            return f64::INFINITY;
        }
        violations.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((violations.len() - 1) as f64 * q).round() as usize;
        violations[idx.min(violations.len() - 1)]
    }

    /// Report `(neural_fitness, violation)` for an individual (constrained
    /// mode). Updates `best` using the ε-constrained comparator.
    pub fn report_constrained(&mut self, index: usize, neural_fitness: f64, violation: f64) {
        self.population[index].fitness = neural_fitness;
        self.population[index].neural_fitness = neural_fitness;
        self.population[index].violation = violation.max(0.0);
        let eps = self.current_eps();
        if constrained_better(&self.population[index], &self.best, eps) {
            self.best = self.population[index].clone();
        }
    }

    /// Report a constrained trial result. Replaces the parent at
    /// `target_index` iff the trial is "at least as good" under the
    /// ε-comparator (matches the `>=` semantics of the legacy
    /// `report_trial_result`).
    pub fn report_trial_constrained(
        &mut self,
        target_index: usize,
        trial_genome: Vec<f64>,
        neural_fitness: f64,
        violation: f64,
    ) {
        let eps = self.current_eps();
        let trial = Individual {
            genome: trial_genome,
            fitness: neural_fitness,
            neural_fitness,
            violation: violation.max(0.0),
        };
        // ">= " semantics: replace unless the parent strictly dominates.
        if !constrained_better(&self.population[target_index], &trial, eps) {
            self.population[target_index] = trial.clone();
            if constrained_better(&trial, &self.best, eps) {
                self.best = trial;
            }
        }
    }

    /// Inspect one population member by index. Used by caller-side logging.
    pub fn individual(&self, index: usize) -> &Individual {
        &self.population[index]
    }

    /// Whether a legacy-mode trial would replace the current parent.
    pub fn trial_would_replace(&self, target_index: usize, trial_fitness: f64) -> bool {
        trial_fitness >= self.population[target_index].fitness
    }

    /// Whether a constrained-mode trial would replace the current parent.
    pub fn trial_would_replace_constrained(
        &self,
        target_index: usize,
        neural_fitness: f64,
        violation: f64,
    ) -> bool {
        let trial = Individual {
            genome: Vec::new(),
            fitness: neural_fitness,
            neural_fitness,
            violation: violation.max(0.0),
        };
        !constrained_better(&self.population[target_index], &trial, self.current_eps())
    }

    /// Best **strictly feasible** individual (violation ≤ `STRICT_FEAS_TOL`),
    /// or `None` if no evaluated individual is strictly feasible.
    ///
    /// This is the function `main.rs` uses to pick the final preset in
    /// constrained mode. The `Option` return makes the "no feasible
    /// candidate" case explicit — previously this function silently
    /// returned the lowest-violation infeasible individual, which
    /// contradicted the docs and could ship an unfeasible preset as
    /// "the best feasible" if the population happened to be entirely
    /// infeasible. (Caught by external code review 2026-05-02.)
    ///
    /// In legacy mode every evaluated individual has `violation = 0`,
    /// so this is identical to `Some(best())`.
    pub fn best_strict(&self) -> Option<&Individual> {
        const STRICT_FEAS_TOL: f64 = 1e-9;
        let mut candidate: Option<&Individual> = None;
        // Consider the cached `best` first; it was tracked under the
        // active ε which may be > 0, so it is not necessarily strictly
        // feasible.
        if self.best.neural_fitness.is_finite()
            && self.best.violation.is_finite()
            && self.best.violation <= STRICT_FEAS_TOL
        {
            candidate = Some(&self.best);
        }
        for ind in &self.population {
            if !ind.neural_fitness.is_finite() || !ind.violation.is_finite() {
                continue;
            }
            if ind.violation > STRICT_FEAS_TOL {
                continue;
            }
            candidate = Some(match candidate {
                None => ind,
                Some(c) => {
                    if ind.neural_fitness > c.neural_fitness {
                        ind
                    } else {
                        c
                    }
                }
            });
        }
        candidate
    }

    // ─────────────────────────────────────────────────────────────────────
    // Priority 28 Phase 3 — DE diversification API
    //
    // Two opt-in features that target the documented premature-convergence
    // failure of vanilla DE/rand/1/bin on the 230-D preset genome:
    //   - Crowding selection (Thomsen 2004): redirect each trial's
    //     replacement target to the nearest-genome parent. Maintains
    //     niches across the search space without sacrificing convergence.
    //   - Stagnation-triggered partial restart (Sallam 2025 ARRDE): when
    //     the best fitness has not improved for `window` generations,
    //     reseed the worst `fraction` of the population.
    //
    // Both features are independent and composable. Default off; legacy
    // mode (no enable calls) is bit-identical to pre-Phase-3 behavior.
    // ─────────────────────────────────────────────────────────────────────

    /// Activate crowding selection (Thomsen 2004).
    pub fn enable_crowding_selection(&mut self) {
        self.crowding_enabled = true;
    }

    /// True if crowding selection is currently enabled.
    pub fn is_crowding_enabled(&self) -> bool {
        self.crowding_enabled
    }

    /// Activate stagnation-triggered partial restart.
    ///
    /// `window`: minimum number of consecutive no-improvement generations
    /// before the restart fires. Typical: 10–20 for the 230-D preset
    /// genome at 100 generations.
    /// `fraction`: fraction of the population to reseed when the trigger
    /// fires (0.0 to 1.0). Typical: 0.20–0.40. The current best is always
    /// preserved (elitism), even if it falls within the reset slice.
    pub fn enable_stagnation_restart(&mut self, window: usize, fraction: f64) {
        assert!(window > 0, "stagnation window must be > 0");
        assert!(
            (0.0..=1.0).contains(&fraction),
            "stagnation fraction must be in [0, 1], got {fraction}"
        );
        self.stagnation_config = Some(StagnationConfig { window, fraction });
        self.stagnation_last_best = self.best.fitness;
        self.stagnation_count = 0;
    }

    /// True if stagnation-triggered restart is currently enabled.
    pub fn is_stagnation_restart_enabled(&self) -> bool {
        self.stagnation_config.is_some()
    }

    /// Generations of no-improvement counted so far. Visible for tests
    /// and progress display.
    pub fn stagnation_count(&self) -> usize {
        self.stagnation_count
    }

    /// How many times the stagnation restart has fired so far.
    pub fn stagnation_restart_count(&self) -> usize {
        self.stagnation_restart_count
    }

    /// Squared normalized Euclidean distance between two genomes, scaling
    /// each dimension by its bound range. Returns 0 for genomes identical
    /// after normalisation. Caller is responsible for `genome.len() ==
    /// self.bounds.len()`.
    fn normalized_distance_sq(&self, a: &[f64], b: &[f64]) -> f64 {
        let mut sum_sq = 0.0;
        for ((ai, bi), (lo, hi)) in a.iter().zip(b.iter()).zip(self.bounds.iter()) {
            let span = (hi - lo).max(1e-12);
            let d = (ai - bi) / span;
            sum_sq += d * d;
        }
        sum_sq
    }

    /// Index of the population member whose genome is closest to `trial`.
    /// Used in crowding-DE selection.
    fn nearest_parent_index(&self, trial: &[f64]) -> usize {
        let mut best_i = 0;
        let mut best_dist = f64::INFINITY;
        for (i, ind) in self.population.iter().enumerate() {
            let d = self.normalized_distance_sq(trial, &ind.genome);
            if d < best_dist {
                best_dist = d;
                best_i = i;
            }
        }
        best_i
    }

    /// Apply the stagnation restart if the trigger condition is met.
    /// Internal — called at the start of each `generate_trials`. The
    /// current best individual is preserved (elitism); only members of
    /// the worst-fitness slice are reseeded.
    fn maybe_apply_stagnation_restart(&mut self) {
        let cfg = match self.stagnation_config {
            Some(c) => c,
            None => return,
        };
        // Only react after at least one full generation has produced data.
        if self.generation == 0 {
            return;
        }
        let current_best = self.best.fitness;
        let improved = current_best.is_finite()
            && (current_best - self.stagnation_last_best).abs() > 1e-9
            && current_best > self.stagnation_last_best;
        if improved {
            self.stagnation_last_best = current_best;
            self.stagnation_count = 0;
            return;
        }
        self.stagnation_count += 1;
        if self.stagnation_count < cfg.window {
            return;
        }

        // Stagnation trigger fired — reseed worst `fraction` of the pop,
        // preserving the elite. Worst is defined by `fitness` so that the
        // policy is consistent across legacy and constrained modes
        // (constrained mode mirrors fitness = neural_fitness).
        self.stagnation_count = 0;
        self.stagnation_last_best = current_best;
        self.stagnation_restart_count += 1;

        let pop_size = self.population.len();
        if pop_size == 0 {
            return;
        }
        let mut indices: Vec<usize> = (0..pop_size).collect();
        indices.sort_by(|&x, &y| {
            self.population[x]
                .fitness
                .partial_cmp(&self.population[y].fitness)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let elite_index = indices[indices.len() - 1]; // highest fitness
        let reset_n = ((pop_size as f64) * cfg.fraction).ceil() as usize;
        let reset_n = reset_n.min(pop_size);
        for &i in indices.iter().take(reset_n) {
            if i == elite_index {
                continue; // elitism: never reset the current best
            }
            let genome: Vec<f64> = self
                .bounds
                .iter()
                .map(|(lo, hi)| Self::sample_within_bound(&mut self.rng, *lo, *hi))
                .collect();
            self.population[i] = Individual::unevaluated(genome);
        }
        // Re-round discrete genes after randomization.
        self.round_discrete_all();
    }

    /// Run one generation of DE. Returns trial vectors to evaluate.
    ///
    /// Call `report_trial_results()` after evaluating each trial.
    pub fn generate_trials(&mut self) -> Vec<(usize, Vec<f64>)> {
        // Priority 28 Phase 3: stagnation restart fires here so the
        // freshly randomised individuals participate in this generation's
        // trial generation as random parents (improving diversity in the
        // donor pool from the very next iteration).
        self.maybe_apply_stagnation_restart();

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
                        + self.f * (self.population[b].genome[j] - self.population[c].genome[j]);
                    trial[j] = self.clamp_to_bounds(j, mutant);
                } else {
                    trial[j] = self.population[i].genome[j];
                }
            }

            self.round_discrete(&mut trial);
            // Priority 28 Phase 3 (Thomsen 2004): in crowding mode, the
            // trial competes against its nearest-genome parent rather
            // than the parent it was generated from. The lookup happens
            // here so the caller's `target_idx` already reflects the
            // actual replacement target — `report_trial_result` does not
            // need to know about diversification.
            let target = if self.crowding_enabled {
                self.nearest_parent_index(&trial)
            } else {
                i
            };
            trials.push((target, trial));
        }

        self.generation += 1;
        trials
    }

    /// Report trial evaluation results (legacy mode). Replaces parent
    /// when `trial_fitness >= parent.fitness`.
    pub fn report_trial_result(
        &mut self,
        target_index: usize,
        trial_genome: Vec<f64>,
        trial_fitness: f64,
    ) {
        if trial_fitness >= self.population[target_index].fitness {
            self.population[target_index] = Individual {
                genome: trial_genome,
                fitness: trial_fitness,
                neural_fitness: trial_fitness,
                violation: 0.0,
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
        let valid: Vec<f64> = self
            .population
            .iter()
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
        let valid: Vec<f64> = self
            .population
            .iter()
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

    /// Round discrete genes in a single genome vector.
    fn round_discrete(&self, genome: &mut [f64]) {
        for &d in &self.discrete_dims {
            genome[d] = genome[d].round();
        }
    }

    /// Round discrete genes in the entire population.
    fn round_discrete_all(&mut self) {
        let dims: Vec<usize> = self.discrete_dims.clone();
        for ind in &mut self.population {
            for &d in &dims {
                ind.genome[d] = ind.genome[d].round();
            }
        }
    }

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

    /// Clamp value to the bounds for the given dimension.
    fn clamp_to_bounds(&self, dim: usize, value: f64) -> f64 {
        let (lo, hi) = self.bounds[dim];
        value.clamp(lo, hi)
    }
}

// ─────────────────────────────────────────────────────────────────────────
// Priority 28 Phase 2 — ε-constrained comparator
//
// Per Takahama & Sakai (2009), the ε-constrained comparison defines the
// strict preference relation `<_ε` on (fitness, violation) pairs:
//
//   if both ε-feasible:     prefer higher fitness
//   if exactly one feasible: prefer the feasible one
//   if both infeasible:     prefer lower violation
//
// Equality at any tier is "not preferred" (the trial-replacement code uses
// `!constrained_better(parent, trial, eps)` to honour the legacy `>=`
// replacement semantics: a tie still replaces the parent).
// ─────────────────────────────────────────────────────────────────────────

/// Returns true iff `a` is *strictly* better than `b` under the
/// ε-constrained comparator. NaN handling: any NaN comparison returns
/// false (so degenerate input cannot displace a valid `best`).
fn constrained_better(a: &Individual, b: &Individual, eps: f64) -> bool {
    if a.violation.is_nan() || b.violation.is_nan() {
        return false;
    }
    if a.neural_fitness.is_nan() {
        return false;
    }
    let a_feas = a.violation <= eps;
    let b_feas = b.violation <= eps;
    match (a_feas, b_feas) {
        (true, true) => a.neural_fitness > b.neural_fitness,
        (true, false) => true,
        (false, true) => false,
        (false, false) => a.violation < b.violation,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_bounds(dim: usize) -> Vec<(f64, f64)> {
        vec![(-5.0, 5.0); dim]
    }

    // ---------------------------------------------------------------
    // Construction
    // ---------------------------------------------------------------

    #[test]
    fn population_has_correct_size() {
        let de = DifferentialEvolution::new(simple_bounds(3), 20, 0.8, 0.9, 42);
        assert_eq!(de.population.len(), 20);
    }

    #[test]
    fn initial_population_within_bounds() {
        let bounds = vec![(-2.0, 3.0), (0.0, 10.0), (-1.0, 1.0)];
        let de = DifferentialEvolution::new(bounds.clone(), 50, 0.8, 0.9, 42);
        for ind in &de.population {
            for (j, &g) in ind.genome.iter().enumerate() {
                let (lo, hi) = bounds[j];
                assert!(g >= lo && g <= hi, "Gene {j} = {g} out of [{lo}, {hi}]");
            }
        }
    }

    #[test]
    fn fixed_width_bounds_are_supported() {
        let bounds = vec![(0.0, 0.0), (-2.0, 3.0), (1.5, 1.5)];
        let mut de = DifferentialEvolution::new(bounds.clone(), 12, 0.8, 0.9, 42);
        for ind in &de.population {
            assert_eq!(ind.genome[0], 0.0);
            assert_eq!(ind.genome[2], 1.5);
        }
        de.seed_from_genome(&[0.0, 0.7, 1.5], 0.2);
        for ind in &de.population {
            assert_eq!(ind.genome[0], 0.0);
            assert_eq!(ind.genome[2], 1.5);
        }
        de.enable_stagnation_restart(1, 0.5);
        de.generation = 1;
        de.stagnation_count = 1;
        de.maybe_apply_stagnation_restart();
        for ind in &de.population {
            assert_eq!(ind.genome[0], 0.0);
            assert_eq!(ind.genome[2], 1.5);
        }
    }

    #[test]
    fn initial_fitness_is_neg_infinity() {
        let de = DifferentialEvolution::new(simple_bounds(3), 10, 0.8, 0.9, 42);
        for ind in &de.population {
            assert_eq!(ind.fitness, f64::NEG_INFINITY);
        }
    }

    #[test]
    fn best_starts_at_neg_infinity() {
        let de = DifferentialEvolution::new(simple_bounds(3), 10, 0.8, 0.9, 42);
        assert_eq!(de.best().fitness, f64::NEG_INFINITY);
    }

    // ---------------------------------------------------------------
    // Discrete gene rounding
    // ---------------------------------------------------------------

    #[test]
    fn discrete_genes_are_integers() {
        let bounds = vec![(0.0, 5.0), (-3.0, 3.0), (0.0, 4.0)];
        let discrete = vec![0, 2]; // genes 0 and 2 are discrete
        let de = DifferentialEvolution::with_discrete(bounds, 30, 0.8, 0.9, 42, discrete);

        for ind in &de.population {
            assert!(
                (ind.genome[0] - ind.genome[0].round()).abs() < 1e-10,
                "Discrete gene 0 = {} should be integer",
                ind.genome[0]
            );
            assert!(
                (ind.genome[2] - ind.genome[2].round()).abs() < 1e-10,
                "Discrete gene 2 = {} should be integer",
                ind.genome[2]
            );
        }
    }

    // ---------------------------------------------------------------
    // report_fitness updates best
    // ---------------------------------------------------------------

    #[test]
    fn report_fitness_updates_best() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        de.report_fitness(3, 0.75);
        assert_eq!(de.best().fitness, 0.75);

        de.report_fitness(7, 0.90);
        assert_eq!(de.best().fitness, 0.90);

        // Lower fitness doesn't replace best
        de.report_fitness(1, 0.50);
        assert_eq!(de.best().fitness, 0.90);
    }

    // ---------------------------------------------------------------
    // generate_trials produces correct count
    // ---------------------------------------------------------------

    #[test]
    fn trials_count_equals_pop_size() {
        let mut de = DifferentialEvolution::new(simple_bounds(3), 15, 0.8, 0.9, 42);
        // Must evaluate initial pop first
        for i in 0..15 {
            de.report_fitness(i, 0.5);
        }
        let trials = de.generate_trials();
        assert_eq!(trials.len(), 15);
    }

    #[test]
    fn trials_within_bounds() {
        let bounds = vec![(-2.0, 3.0), (0.0, 10.0), (-1.0, 1.0)];
        let mut de = DifferentialEvolution::new(bounds.clone(), 20, 0.8, 0.9, 42);
        for i in 0..20 {
            de.report_fitness(i, 0.5);
        }

        let trials = de.generate_trials();
        for (idx, trial) in &trials {
            for (j, &g) in trial.iter().enumerate() {
                let (lo, hi) = bounds[j];
                assert!(
                    g >= lo && g <= hi,
                    "Trial for target {idx}, gene {j} = {g} out of [{lo}, {hi}]"
                );
            }
        }
    }

    // ---------------------------------------------------------------
    // Greedy selection: trial replaces parent if >=
    // ---------------------------------------------------------------

    #[test]
    fn greedy_selection_replaces_on_equal() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        de.report_fitness(0, 0.5);

        let new_genome = vec![1.0, 2.0];
        de.report_trial_result(0, new_genome.clone(), 0.5); // equal fitness

        // Should replace (>= selection)
        assert_eq!(de.population[0].genome, new_genome);
    }

    #[test]
    fn greedy_selection_keeps_parent_if_worse() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        de.report_fitness(0, 0.5);

        let old_genome = de.population[0].genome.clone();
        de.report_trial_result(0, vec![9.0, 9.0], 0.3); // worse

        assert_eq!(de.population[0].genome, old_genome);
        assert_eq!(de.population[0].fitness, 0.5);
    }

    #[test]
    fn skipped_trial_report_keeps_parent_unchanged() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 6, 0.8, 0.9, 42);
        for i in 0..6 {
            de.report_fitness(i, 0.4 + i as f64 * 0.01);
        }

        let before = de.population[0].clone();
        let _trials = de.generate_trials();

        assert_eq!(de.population[0].genome, before.genome);
        assert_eq!(de.population[0].fitness, before.fitness);
    }

    // ---------------------------------------------------------------
    // Convergence on a simple 1D function
    // ---------------------------------------------------------------

    #[test]
    fn converges_on_1d_quadratic() {
        // Maximize f(x) = -(x-3)^2 + 10.  Optimum at x=3, f=10.
        let bounds = vec![(0.0, 6.0)];
        let mut de = DifferentialEvolution::new(bounds, 10, 0.8, 0.9, 42);

        // Evaluate initial population
        for (i, genome) in de.pending_evaluations() {
            let x = genome[0];
            let fitness = -(x - 3.0).powi(2) + 10.0;
            de.report_fitness(i, fitness);
        }

        // Run 50 generations
        for _ in 0..50 {
            let trials = de.generate_trials();
            for (target, trial) in trials {
                let x = trial[0];
                let fitness = -(x - 3.0).powi(2) + 10.0;
                de.report_trial_result(target, trial, fitness);
            }
        }

        let best = de.best();
        assert!(
            (best.genome[0] - 3.0).abs() < 0.1,
            "Should converge near x=3, got x={:.4}",
            best.genome[0]
        );
        assert!(
            best.fitness > 9.9,
            "Should achieve fitness near 10, got {:.4}",
            best.fitness
        );
    }

    // ---------------------------------------------------------------
    // Determinism: same seed → same results
    // ---------------------------------------------------------------

    #[test]
    fn deterministic_with_same_seed() {
        let make_de = || {
            let mut de = DifferentialEvolution::new(simple_bounds(3), 10, 0.8, 0.9, 123);
            for i in 0..10 {
                de.report_fitness(i, i as f64 * 0.1);
            }
            de.generate_trials()
        };

        let trials1 = make_de();
        let trials2 = make_de();

        for ((i1, t1), (i2, t2)) in trials1.iter().zip(trials2.iter()) {
            assert_eq!(i1, i2);
            assert_eq!(t1, t2);
        }
    }

    // ---------------------------------------------------------------
    // seed_from_genome
    // ---------------------------------------------------------------

    #[test]
    fn seed_from_genome_places_seed_at_index_0() {
        let mut de = DifferentialEvolution::new(simple_bounds(3), 10, 0.8, 0.9, 42);
        let seed = vec![1.0, 2.0, 3.0];
        de.seed_from_genome(&seed, 0.1);

        assert_eq!(de.population[0].genome, seed);
    }

    #[test]
    fn seed_from_genome_resets_fitness() {
        let mut de = DifferentialEvolution::new(simple_bounds(3), 10, 0.8, 0.9, 42);
        de.report_fitness(0, 0.9);
        de.seed_from_genome(&vec![0.0; 3], 0.1);

        for ind in &de.population {
            assert_eq!(ind.fitness, f64::NEG_INFINITY);
        }
    }

    // ---------------------------------------------------------------
    // mean_fitness and fitness_std
    // ---------------------------------------------------------------

    #[test]
    fn mean_fitness_excludes_unevaluated() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        de.report_fitness(0, 0.4);
        de.report_fitness(1, 0.6);
        // 8 others are NEG_INFINITY

        let mean = de.mean_fitness();
        assert!((mean - 0.5).abs() < 1e-10, "Mean should be 0.5, got {mean}");
    }

    #[test]
    fn fitness_std_zero_for_uniform_pop() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 5, 0.8, 0.9, 42);
        for i in 0..5 {
            de.report_fitness(i, 0.7);
        }
        assert!(
            de.fitness_std() < 1e-10,
            "Uniform fitness should have std=0"
        );
    }

    #[test]
    fn fitness_std_positive_for_varied_pop() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 4, 0.8, 0.9, 42);
        de.report_fitness(0, 0.2);
        de.report_fitness(1, 0.4);
        de.report_fitness(2, 0.6);
        de.report_fitness(3, 0.8);
        assert!(de.fitness_std() > 0.1);
    }

    // ---------------------------------------------------------------
    // Generation counter
    // ---------------------------------------------------------------

    #[test]
    fn generation_increments() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        assert_eq!(de.generation(), 0);

        for i in 0..10 {
            de.report_fitness(i, 0.5);
        }
        let _ = de.generate_trials();
        assert_eq!(de.generation(), 1);

        let _ = de.generate_trials();
        assert_eq!(de.generation(), 2);
    }

    // ---------------------------------------------------------------
    // pending_evaluations
    // ---------------------------------------------------------------

    #[test]
    fn pending_evaluations_initially_all() {
        let de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        assert_eq!(de.pending_evaluations().len(), 10);
    }

    #[test]
    fn pending_evaluations_shrinks_after_report() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        de.report_fitness(0, 0.5);
        de.report_fitness(1, 0.5);
        assert_eq!(de.pending_evaluations().len(), 8);
    }

    // ───────────────────────────────────────────────────────────────────
    // Priority 28 Phase 2 — ε-constrained DE tests
    //
    // Per Takahama & Sakai (2009), the comparator's three-tier behaviour
    // (feasible-first / fitness / lowest-violation) is the primary
    // contract these tests pin. Linear ε-decay, init-from-population
    // ε₀ helper, and the legacy↔constrained equivalence in legacy mode
    // are the secondary contracts.
    // ───────────────────────────────────────────────────────────────────

    fn make_individual(neural_fitness: f64, violation: f64) -> Individual {
        Individual {
            genome: vec![0.0; 1],
            fitness: neural_fitness,
            neural_fitness,
            violation,
        }
    }

    #[test]
    fn constrained_compare_both_feasible_prefers_higher_fitness() {
        let a = make_individual(1.0, 0.05);
        let b = make_individual(0.8, 0.05);
        // ε = 0.10 → both feasible. Higher fitness wins.
        assert!(constrained_better(&a, &b, 0.10));
        assert!(!constrained_better(&b, &a, 0.10));
    }

    #[test]
    fn constrained_compare_feasible_beats_infeasible() {
        let feasible = make_individual(0.1, 0.05);   // low fitness, low violation
        let infeasible = make_individual(0.99, 0.5); // high fitness, high violation
        // ε = 0.10 → feasible (0.05 ≤ 0.10) beats infeasible (0.5 > 0.10)
        // even though the infeasible one has higher fitness.
        assert!(constrained_better(&feasible, &infeasible, 0.10));
        assert!(!constrained_better(&infeasible, &feasible, 0.10));
    }

    #[test]
    fn constrained_compare_both_infeasible_prefers_lower_violation() {
        let a = make_individual(0.5, 0.20);
        let b = make_individual(0.9, 0.30);
        // ε = 0.10 → both infeasible. Lower-violation wins regardless of fitness.
        assert!(constrained_better(&a, &b, 0.10));
        assert!(!constrained_better(&b, &a, 0.10));
    }

    #[test]
    fn constrained_compare_strict_inequality_handles_ties() {
        let a = make_individual(0.5, 0.05);
        let b = make_individual(0.5, 0.05);
        // Tie → neither is strictly better.
        assert!(!constrained_better(&a, &b, 0.10));
        assert!(!constrained_better(&b, &a, 0.10));
    }

    #[test]
    fn constrained_compare_nan_returns_false() {
        let nan_v = make_individual(0.5, f64::NAN);
        let nan_f = make_individual(f64::NAN, 0.05);
        let normal = make_individual(0.5, 0.05);
        assert!(!constrained_better(&nan_v, &normal, 0.10));
        assert!(!constrained_better(&normal, &nan_v, 0.10));
        assert!(!constrained_better(&nan_f, &normal, 0.10));
    }

    // ── ε schedule ─────────────────────────────────────────────────────

    #[test]
    fn current_eps_zero_in_legacy_mode() {
        let de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        assert!(!de.is_constrained());
        assert_eq!(de.current_eps(), 0.0);
    }

    #[test]
    fn current_eps_decays_linearly_to_zero() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        de.enable_eps_constrained(0.10, 20);
        assert!(de.is_constrained());

        // gen 0 → ε = 0.10
        assert!((de.current_eps() - 0.10).abs() < 1e-10);

        // Advance the generation counter directly (avoids running trials).
        // Generation 10 (half way) → ε = 0.05
        for i in 0..10 {
            de.report_fitness(i, 0.5);
        }
        let _ = de.generate_trials(); // generation = 1
        for _ in 0..9 {
            let _ = de.generate_trials(); // generation = 10
        }
        assert_eq!(de.generation(), 10);
        assert!((de.current_eps() - 0.05).abs() < 1e-10, "ε at half-way should be 0.05, got {}", de.current_eps());

        // At T_c → ε = 0
        for _ in 0..10 {
            let _ = de.generate_trials();
        }
        assert_eq!(de.generation(), 20);
        assert_eq!(de.current_eps(), 0.0);

        // Past T_c → still 0
        let _ = de.generate_trials();
        assert_eq!(de.current_eps(), 0.0);
    }

    #[test]
    #[should_panic(expected = "eps_0 must be finite")]
    fn enable_eps_constrained_rejects_nan() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        de.enable_eps_constrained(f64::NAN, 20);
    }

    #[test]
    #[should_panic(expected = "t_c must be > 0")]
    fn enable_eps_constrained_rejects_zero_t_c() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        de.enable_eps_constrained(0.10, 0);
    }

    // ── suggest_eps_from_population ─────────────────────────────────────

    #[test]
    fn suggest_eps_returns_infinity_when_population_unevaluated() {
        let de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        // Population is initialized with violation = INFINITY → all
        // values are filtered out as non-finite.
        let eps = de.suggest_eps_from_population(0.7);
        assert_eq!(eps, f64::INFINITY);
    }

    #[test]
    fn suggest_eps_returns_quantile_of_population_violations() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        // Populate violations 0.0, 0.1, 0.2, ..., 0.9 deterministically.
        for i in 0..10 {
            de.report_constrained(i, 0.5, i as f64 * 0.1);
        }
        let med = de.suggest_eps_from_population(0.5);
        assert!((med - 0.5).abs() < 1e-10, "median violation should be 0.5, got {med}");
        let p70 = de.suggest_eps_from_population(0.7);
        assert!((p70 - 0.6).abs() < 1e-10, "70th percentile should be 0.6, got {p70}");
        assert_eq!(de.suggest_eps_from_population(0.0), 0.0);
        assert!((de.suggest_eps_from_population(1.0) - 0.9).abs() < 1e-10);
    }

    // ── report_constrained / report_trial_constrained ───────────────────

    /// Regression: before the fix, `enable_eps_constrained` left the
    /// cached `best` as the strict-feasible (ε=0) winner from initial-
    /// population reporting. After the fix, the cached `best` must be
    /// re-derived under the new ε so that high-fitness, slightly-
    /// infeasible candidates aren't silently masked.
    ///
    /// Scenario: A is high-fitness with v=0.10, B is moderate-fitness
    /// with v=0 (strict-feasible). Init reporting (ε=0) picks B. Then
    /// `enable_eps_constrained(0.20, _)` should re-promote A because A
    /// is ε-feasible at ε=0.20 and has higher fitness.
    #[test]
    fn enable_eps_constrained_recomputes_incumbent_under_new_eps() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 4, 0.8, 0.9, 42);
        // Report population BEFORE enabling the schedule (mirrors the
        // production main.rs sequence: init pop → enable_eps_constrained).
        de.report_constrained(0, 0.90, 0.10); // A — high fitness, mild infeasible
        de.report_constrained(1, 0.50, 0.0);  // B — moderate, strict feasible
        de.report_constrained(2, 0.20, 0.30); // far infeasible
        de.report_constrained(3, 0.10, 0.40); // worst

        // Pre-schedule: ε=0 → only B is feasible → best should be B.
        assert!((de.best().neural_fitness - 0.50).abs() < 1e-12);

        // Enabling the schedule must re-promote A.
        de.enable_eps_constrained(0.20, 50);
        assert!(
            (de.best().neural_fitness - 0.90).abs() < 1e-12,
            "best must be recomputed under new ε; expected A (0.90), got {}",
            de.best().neural_fitness
        );
        assert!((de.best().violation - 0.10).abs() < 1e-12);

        // Stagnation baseline must also follow the recomputed incumbent
        // (not the stale strict-feasible one), so the first stagnation
        // check doesn't immediately fire on a phantom "regression" from
        // the strict best down to the new best.
        assert!((de.stagnation_last_best - 0.90).abs() < 1e-12);
        assert_eq!(de.stagnation_count(), 0);
    }

    #[test]
    fn report_constrained_updates_best_under_eps_comparator() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 5, 0.8, 0.9, 42);
        de.enable_eps_constrained(0.20, 50);

        de.report_constrained(0, 0.10, 0.05); // feasible-low, low fitness
        de.report_constrained(1, 0.99, 0.30); // infeasible, high fitness
        de.report_constrained(2, 0.50, 0.15); // feasible, mid fitness
        de.report_constrained(3, 0.90, 0.10); // feasible, highest fitness
        de.report_constrained(4, 0.95, 0.40); // infeasible, very high fitness

        // ε = 0.20 → feasible: 0, 2, 3. Among these, idx 3 has the highest
        // neural_fitness (0.90).
        let best = de.best();
        assert_eq!(best.neural_fitness, 0.90);
        assert!(best.violation <= 0.20);
    }

    #[test]
    fn report_trial_constrained_replaces_parent_when_strictly_dominated() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 4, 0.8, 0.9, 42);
        de.enable_eps_constrained(0.20, 50);
        // Set up a low-fitness feasible parent.
        de.report_constrained(0, 0.30, 0.05);

        // Trial: higher fitness, also feasible → replaces.
        de.report_trial_constrained(0, vec![1.0, 1.0], 0.60, 0.05);
        let p = &de.population[0];
        assert!((p.neural_fitness - 0.60).abs() < 1e-10);
        assert!((p.violation - 0.05).abs() < 1e-10);
    }

    #[test]
    fn report_trial_constrained_keeps_parent_when_strictly_worse() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 4, 0.8, 0.9, 42);
        de.enable_eps_constrained(0.20, 50);
        de.report_constrained(0, 0.60, 0.05);

        // Infeasible trial with higher fitness → must NOT replace a
        // feasible parent.
        de.report_trial_constrained(0, vec![9.0, 9.0], 0.95, 0.40);
        assert!((de.population[0].neural_fitness - 0.60).abs() < 1e-10);
    }

    #[test]
    fn report_trial_constrained_replaces_on_tie() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 4, 0.8, 0.9, 42);
        de.enable_eps_constrained(0.20, 50);
        de.report_constrained(0, 0.50, 0.05);

        // Identical (fitness, violation) → tie → replace (matches the
        // legacy `>=` semantics of `report_trial_result`).
        let new_genome = vec![3.0, 4.0];
        de.report_trial_constrained(0, new_genome.clone(), 0.50, 0.05);
        assert_eq!(de.population[0].genome, new_genome);
    }

    #[test]
    fn report_trial_constrained_prefers_lower_violation_when_both_infeasible() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 4, 0.8, 0.9, 42);
        de.enable_eps_constrained(0.05, 50); // tight ε → most things infeasible
        de.report_constrained(0, 0.20, 0.30); // infeasible parent

        // Trial: lower violation (still infeasible at ε=0.05), lower
        // fitness — must still replace because lower-violation wins among
        // infeasible pairs.
        de.report_trial_constrained(0, vec![1.0, 1.0], 0.10, 0.15);
        assert!((de.population[0].violation - 0.15).abs() < 1e-10);
    }

    // ── best_strict ─────────────────────────────────────────────────────

    #[test]
    fn best_strict_returns_strictly_feasible_individual() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 4, 0.8, 0.9, 42);
        de.enable_eps_constrained(0.20, 50);
        // High-fitness infeasible (0.95, violation 0.10) — feasible at
        // ε=0.20 but NOT strictly feasible (violation > 0).
        de.report_constrained(0, 0.95, 0.10);
        // Strictly feasible at moderate fitness.
        de.report_constrained(1, 0.50, 0.0);

        // best() under current ε accepts (0, 0.95, 0.10).
        assert!((de.best().neural_fitness - 0.95).abs() < 1e-10);
        // best_strict() requires violation ≤ 1e-9; only individual 1 qualifies.
        let strict = de.best_strict().expect("at least one strict-feasible exists");
        assert!((strict.neural_fitness - 0.50).abs() < 1e-10);
    }

    /// **Review fix 2026-05-02**: best_strict must return None when the
    /// entire population is infeasible. Previously it silently returned
    /// the lowest-violation infeasible individual, which contradicted
    /// the function's documented semantics and could ship an unfeasible
    /// preset as "the strict best".
    #[test]
    fn best_strict_returns_none_when_no_strict_feasible_exists() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 4, 0.8, 0.9, 42);
        de.enable_eps_constrained(0.50, 50);
        // Every individual infeasible.
        de.report_constrained(0, 0.90, 0.20);
        de.report_constrained(1, 0.50, 0.10);
        de.report_constrained(2, 0.20, 0.40);
        de.report_constrained(3, 0.10, 0.50);
        assert!(
            de.best_strict().is_none(),
            "best_strict must be None when no individual has violation ≤ 1e-9"
        );
        // best() under current ε returns the highest-fitness ε-feasible.
        assert!(de.best().neural_fitness.is_finite());
    }

    /// best_strict tolerates tiny FP-noise violations (≤ 1e-9) as
    /// strictly feasible. Useful because the legacy `report_fitness`
    /// path sets `violation = 0.0` exactly, but other paths could
    /// produce values like 1e-15 from floating-point arithmetic.
    #[test]
    fn best_strict_accepts_tiny_fp_noise_violations() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 3, 0.8, 0.9, 42);
        de.report_constrained(0, 0.30, 0.0);    // exactly 0
        de.report_constrained(1, 0.80, 1e-15);  // FP-noise
        de.report_constrained(2, 0.20, 0.10);   // genuinely infeasible
        let strict = de.best_strict().expect("two of three are strict-feasible");
        assert!(
            (strict.neural_fitness - 0.80).abs() < 1e-10,
            "best_strict should pick the higher-fitness FP-noise-feasible individual"
        );
    }

    // ── Convergence on a synthetic constrained problem ──────────────────

    /// Synthetic 2D problem: maximize f(x, y) = x + y subject to
    /// constraint x + y ≤ 1 (encoded as violation = max(0, x+y-1)).
    /// Optimum is anywhere on the boundary x+y = 1, where f = 1.
    /// Without the constraint, the unbounded optimum is at the upper
    /// corner (5, 5) with f = 10.
    #[test]
    fn constrained_de_respects_feasibility_at_convergence() {
        let bounds = vec![(0.0, 5.0), (0.0, 5.0)];
        let mut de = DifferentialEvolution::new(bounds, 30, 0.8, 0.9, 12345);

        // Initialise with a wide ε so most of the population can compete
        // by fitness early, then tighten it. Tightens to 0 by gen 80
        // (which is >> the 200 generations we run).
        de.enable_eps_constrained(2.0, 80);

        // Initial-population evaluation.
        for (i, genome) in de.pending_evaluations() {
            let x = genome[0];
            let y = genome[1];
            let fitness = x + y;
            let violation = (x + y - 1.0).max(0.0);
            de.report_constrained(i, fitness, violation);
        }

        // Run the constrained DE.
        for _ in 0..200 {
            let trials = de.generate_trials();
            for (target, trial) in trials {
                let x = trial[0];
                let y = trial[1];
                let fitness = x + y;
                let violation = (x + y - 1.0).max(0.0);
                de.report_trial_constrained(target, trial, fitness, violation);
            }
        }

        let best = de
            .best_strict()
            .expect("the synthetic problem has feasible interior; convergence should find it");
        let x = best.genome[0];
        let y = best.genome[1];
        let f = x + y;
        // Best strictly feasible should be near the boundary x+y = 1.
        assert!(
            best.violation <= 1e-6,
            "best_strict must be feasible, got violation = {:.6}",
            best.violation
        );
        assert!(
            f >= 0.95 && f <= 1.0 + 1e-6,
            "best fitness should approach 1.0 from below, got f = {f:.4} (x={x:.4}, y={y:.4})"
        );
    }

    /// Same problem in legacy mode (no constraint) — must converge to
    /// the unbounded optimum near (5, 5) with f ≈ 10.
    #[test]
    fn legacy_de_unconstrained_converges_to_unbounded_optimum() {
        let bounds = vec![(0.0, 5.0), (0.0, 5.0)];
        let mut de = DifferentialEvolution::new(bounds, 30, 0.8, 0.9, 12345);
        // No enable_eps_constrained → legacy mode.

        for (i, genome) in de.pending_evaluations() {
            let f = genome[0] + genome[1];
            de.report_fitness(i, f);
        }
        for _ in 0..200 {
            let trials = de.generate_trials();
            for (target, trial) in trials {
                let f = trial[0] + trial[1];
                de.report_trial_result(target, trial, f);
            }
        }
        let best = de.best();
        assert!(
            best.fitness > 9.5,
            "legacy DE should approach 10, got {:.3}",
            best.fitness
        );
    }

    /// Legacy-mode bit-identity: report_fitness + report_trial_result must
    /// produce the same trajectory as before Phase 2 (set neural_fitness =
    /// fitness, violation = 0). Same seed → same sequence of best/mean.
    #[test]
    fn legacy_mode_reproduces_pre_phase2_trajectory() {
        let bounds = vec![(0.0, 5.0), (0.0, 5.0)];
        let mut de = DifferentialEvolution::new(bounds.clone(), 20, 0.8, 0.9, 99);

        // Sphere function, maximize.
        let eval = |g: &[f64]| -(g[0] - 2.0).powi(2) - (g[1] - 3.0).powi(2);

        for (i, genome) in de.pending_evaluations() {
            de.report_fitness(i, eval(&genome));
        }
        let mut history = Vec::new();
        for _ in 0..30 {
            let trials = de.generate_trials();
            for (target, trial) in trials {
                let f = eval(&trial);
                de.report_trial_result(target, trial, f);
            }
            history.push((de.best().fitness, de.mean_fitness(), de.fitness_std()));
        }

        // Re-run with the same seed.
        let mut de2 = DifferentialEvolution::new(bounds, 20, 0.8, 0.9, 99);
        for (i, genome) in de2.pending_evaluations() {
            de2.report_fitness(i, eval(&genome));
        }
        let mut history2 = Vec::new();
        for _ in 0..30 {
            let trials = de2.generate_trials();
            for (target, trial) in trials {
                let f = eval(&trial);
                de2.report_trial_result(target, trial, f);
            }
            history2.push((de2.best().fitness, de2.mean_fitness(), de2.fitness_std()));
        }

        assert_eq!(history, history2, "legacy DE must be deterministic at fixed seed");
        // And every Individual must have neural_fitness == fitness, violation == 0.
        for ind in &de.population {
            assert_eq!(ind.neural_fitness, ind.fitness);
            assert_eq!(ind.violation, 0.0);
        }
    }

    // ───────────────────────────────────────────────────────────────────
    // Priority 28 Phase 3 — DE diversification tests
    //
    // Crowding-DE (Thomsen 2004): redirect each trial's target to its
    // nearest-genome parent. Stagnation restart (Sallam 2025 ARRDE):
    // reseed worst fraction when the best fitness stalls. Both opt-in.
    // ───────────────────────────────────────────────────────────────────

    // ── Crowding-DE selection ──────────────────────────────────────────

    #[test]
    fn crowding_disabled_by_default() {
        let de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        assert!(!de.is_crowding_enabled());
    }

    #[test]
    fn enable_crowding_selection_flips_flag() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        de.enable_crowding_selection();
        assert!(de.is_crowding_enabled());
    }

    /// In crowding mode, each returned target is the nearest-parent
    /// index (under normalised Euclidean distance), not the parent the
    /// trial was generated from. Verify by direct distance recomputation.
    #[test]
    fn crowding_redirects_target_to_nearest_parent() {
        let mut de = DifferentialEvolution::new(simple_bounds(3), 12, 0.8, 0.9, 7);
        for i in 0..12 {
            de.report_fitness(i, i as f64 * 0.1);
        }
        de.enable_crowding_selection();
        let trials = de.generate_trials();

        for (target_idx, trial_genome) in &trials {
            // Recompute the nearest parent locally and compare.
            let mut best_i = 0;
            let mut best_d = f64::INFINITY;
            for (i, ind) in de.population.iter().enumerate() {
                let d: f64 = trial_genome
                    .iter()
                    .zip(ind.genome.iter())
                    .zip(de.bounds.iter())
                    .map(|((a, b), (lo, hi))| {
                        let span = (hi - lo).max(1e-12);
                        let dx = (a - b) / span;
                        dx * dx
                    })
                    .sum();
                if d < best_d {
                    best_d = d;
                    best_i = i;
                }
            }
            assert_eq!(
                *target_idx, best_i,
                "crowding mode must redirect trial to nearest parent"
            );
        }
    }

    /// Without crowding, target_idx == i (the loop variable used during
    /// trial generation). This is the classical DE/rand/1/bin selection.
    #[test]
    fn crowding_off_keeps_target_equal_to_i() {
        let mut de = DifferentialEvolution::new(simple_bounds(3), 8, 0.8, 0.9, 7);
        for i in 0..8 {
            de.report_fitness(i, i as f64 * 0.1);
        }
        let trials = de.generate_trials();
        for (i, (target_idx, _)) in trials.iter().enumerate() {
            assert_eq!(
                *target_idx, i,
                "legacy DE must use parent-by-index selection"
            );
        }
    }

    #[test]
    fn crowding_does_not_change_trial_count() {
        let mut de = DifferentialEvolution::new(simple_bounds(3), 15, 0.8, 0.9, 7);
        for i in 0..15 {
            de.report_fitness(i, 0.5);
        }
        de.enable_crowding_selection();
        let trials = de.generate_trials();
        assert_eq!(trials.len(), 15, "trial count must equal population size");
    }

    /// Crowding redirect: every trial's normalised distance to its
    /// reported `target_idx` must be the minimum across the population.
    /// Uses the smallest pop_size DE/rand/1/bin can support (4) so the
    /// `pick_three(i)` rejection sampler always terminates.
    #[test]
    fn crowding_redirect_picks_minimum_distance_target() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 4, 0.8, 0.9, 13);
        for i in 0..4 {
            de.report_fitness(i, 0.5);
        }
        de.enable_crowding_selection();
        let trials = de.generate_trials();
        assert_eq!(trials.len(), 4);
        for (target_idx, trial) in &trials {
            // `target_idx` must be the global argmin across all parents.
            let target_dist: f64 = trial
                .iter()
                .zip(de.population[*target_idx].genome.iter())
                .zip(de.bounds.iter())
                .map(|((t, p), (lo, hi))| {
                    let span = (hi - lo).max(1e-12);
                    let d = (t - p) / span;
                    d * d
                })
                .sum();
            for (other_idx, ind) in de.population.iter().enumerate() {
                if other_idx == *target_idx {
                    continue;
                }
                let other_dist: f64 = trial
                    .iter()
                    .zip(ind.genome.iter())
                    .zip(de.bounds.iter())
                    .map(|((t, p), (lo, hi))| {
                        let span = (hi - lo).max(1e-12);
                        let d = (t - p) / span;
                        d * d
                    })
                    .sum();
                assert!(
                    target_dist <= other_dist,
                    "target {target_idx} (d²={target_dist:.6}) must be closer than {other_idx} (d²={other_dist:.6})"
                );
            }
        }
    }

    // ── Stagnation-triggered partial restart ───────────────────────────

    #[test]
    fn stagnation_restart_disabled_by_default() {
        let de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        assert!(!de.is_stagnation_restart_enabled());
        assert_eq!(de.stagnation_count(), 0);
    }

    #[test]
    #[should_panic(expected = "stagnation window must be > 0")]
    fn stagnation_restart_rejects_zero_window() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        de.enable_stagnation_restart(0, 0.30);
    }

    #[test]
    #[should_panic(expected = "stagnation fraction must be in")]
    fn stagnation_restart_rejects_negative_fraction() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        de.enable_stagnation_restart(5, -0.1);
    }

    #[test]
    #[should_panic(expected = "stagnation fraction must be in")]
    fn stagnation_restart_rejects_fraction_above_one() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        de.enable_stagnation_restart(5, 1.5);
    }

    /// Restart should NOT fire when the best fitness improves between
    /// generations — the stagnation counter must reset on improvement.
    #[test]
    fn stagnation_restart_does_not_fire_on_improvement() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 8, 0.8, 0.9, 42);
        for i in 0..8 {
            de.report_fitness(i, 0.0);
        }
        de.enable_stagnation_restart(3, 0.30);

        // Generate trials; in the standard mode trials go through
        // report_trial_result. Push artificially better fitnesses each
        // generation so improvement is detected.
        for k in 0..10 {
            let trials = de.generate_trials();
            for (target, trial) in trials {
                de.report_trial_result(target, trial, k as f64 + 1.0);
            }
            // Each generation improves, so stagnation_count should stay 0.
            assert_eq!(
                de.stagnation_count(),
                0,
                "stagnation count must remain 0 while best improves"
            );
        }
    }

    /// Restart must fire after `window` consecutive no-improvement
    /// generations. The current best individual is preserved (elitism)
    /// across the restart even when the worst-slice reseed would
    /// otherwise pick it off.
    #[test]
    fn stagnation_restart_fires_after_window_and_preserves_elite() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 10, 0.8, 0.9, 42);
        // Initial fitnesses: spread with a clear winner at idx 9.
        for i in 0..10 {
            de.report_fitness(i, i as f64);
        }
        de.enable_stagnation_restart(2, 0.40);
        let elite_fitness = de.best().fitness;
        assert_eq!(de.stagnation_restart_count(), 0);

        // Drive several stagnant generations: every trial reports fitness
        // 0.0 (worse than the elite at fitness 9.0), so the best never
        // moves. Restart must fire at least once because window=2.
        for _ in 0..6 {
            let trials = de.generate_trials();
            for (target, trial) in trials {
                de.report_trial_result(target, trial, 0.0);
            }
        }

        // Best fitness must still equal the original elite — elitism is
        // honoured across the restart(s).
        let after_best = de.best();
        assert_eq!(after_best.fitness, elite_fitness, "elite fitness preserved");

        // Observable proof that the restart triggered.
        assert!(
            de.stagnation_restart_count() >= 1,
            "restart must fire at least once after `window` stagnant gens, got {} fires",
            de.stagnation_restart_count()
        );
    }

    /// Across a long stagnation horizon, the restart counter increases
    /// monotonically — one fire per `window` generations of no
    /// improvement.
    #[test]
    fn stagnation_restart_counter_increments_each_trigger() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 8, 0.8, 0.9, 42);
        for i in 0..8 {
            de.report_fitness(i, i as f64);
        }
        de.enable_stagnation_restart(3, 0.30);
        let mut prev = de.stagnation_restart_count();
        let mut total_fires = 0;
        for _ in 0..15 {
            let trials = de.generate_trials();
            for (target, trial) in trials {
                de.report_trial_result(target, trial, 0.0);
            }
            let now = de.stagnation_restart_count();
            assert!(now >= prev, "restart counter must be monotone non-decreasing");
            if now > prev {
                total_fires += 1;
            }
            prev = now;
        }
        // 15 stagnant gens / window=3 ⇒ approximately 5 restarts; allow
        // some slack since the very first generation does not check.
        assert!(
            total_fires >= 3,
            "expected ≥ 3 restarts in 15 stagnant gens with window=3, got {total_fires}"
        );
    }

    /// The reseeded genomes after a restart must lie within bounds.
    #[test]
    fn stagnation_restart_keeps_genomes_within_bounds() {
        let bounds = vec![(-2.0, 3.0), (0.0, 10.0), (-1.0, 1.0)];
        let mut de = DifferentialEvolution::new(bounds.clone(), 10, 0.8, 0.9, 42);
        for i in 0..10 {
            de.report_fitness(i, 0.0);
        }
        de.enable_stagnation_restart(1, 0.50);

        for _ in 0..3 {
            let trials = de.generate_trials();
            for (target, trial) in trials {
                de.report_trial_result(target, trial, 0.0);
            }
        }

        for ind in &de.population {
            for (j, &g) in ind.genome.iter().enumerate() {
                let (lo, hi) = bounds[j];
                assert!(
                    g >= lo && g <= hi,
                    "restart-reseeded gene {j} = {g} out of [{lo}, {hi}]"
                );
            }
        }
    }

    /// Restart must respect the discrete-gene rounding contract.
    #[test]
    fn stagnation_restart_preserves_discrete_rounding() {
        let bounds = vec![(0.0, 5.0), (-3.0, 3.0), (0.0, 4.0)];
        let discrete = vec![0, 2];
        let mut de = DifferentialEvolution::with_discrete(bounds, 10, 0.8, 0.9, 42, discrete);
        for i in 0..10 {
            de.report_fitness(i, 0.0);
        }
        de.enable_stagnation_restart(1, 0.50);

        for _ in 0..3 {
            let trials = de.generate_trials();
            for (target, trial) in trials {
                de.report_trial_result(target, trial, 0.0);
            }
        }

        for ind in &de.population {
            assert!(
                (ind.genome[0] - ind.genome[0].round()).abs() < 1e-10,
                "discrete gene 0 must remain integer after restart, got {}",
                ind.genome[0]
            );
            assert!(
                (ind.genome[2] - ind.genome[2].round()).abs() < 1e-10,
                "discrete gene 2 must remain integer after restart, got {}",
                ind.genome[2]
            );
        }
    }

    /// Restart must not fire in the very first generation — there is
    /// no "previous best" to compare against yet.
    #[test]
    fn stagnation_restart_does_not_fire_on_generation_zero() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 6, 0.8, 0.9, 42);
        for i in 0..6 {
            de.report_fitness(i, i as f64);
        }
        de.enable_stagnation_restart(1, 0.50);
        assert_eq!(de.generation(), 0);

        let snapshot: Vec<Vec<f64>> = de.population.iter().map(|i| i.genome.clone()).collect();
        let _ = de.generate_trials();
        // After generate_trials runs, the maybe_apply_stagnation_restart
        // hook runs at the start; at gen 0 it is a no-op. The genomes
        // must not have been reseeded.
        for (a, b) in de.population.iter().zip(snapshot.iter()) {
            assert_eq!(&a.genome, b, "no reseeding allowed at gen 0");
        }
    }

    // ── Composability of crowding + stagnation ─────────────────────────

    #[test]
    fn crowding_and_stagnation_compose_without_panic() {
        let mut de = DifferentialEvolution::new(simple_bounds(2), 12, 0.8, 0.9, 42);
        for i in 0..12 {
            de.report_fitness(i, 0.0);
        }
        de.enable_crowding_selection();
        de.enable_stagnation_restart(2, 0.30);

        // Run a handful of generations with both features active.
        for _ in 0..6 {
            let trials = de.generate_trials();
            for (target, trial) in trials {
                de.report_trial_result(target, trial, 0.0);
            }
        }
        // Sanity: bounds + finite fitness + at least the elite preserved.
        assert!(de.best().fitness.is_finite());
    }

    // ── Legacy bit-identity guard ──────────────────────────────────────

    /// Pin: with neither crowding nor stagnation enabled, two seeded
    /// runs through `generate_trials` + `report_trial_result` produce
    /// identical trajectories. This is the strongest no-regression
    /// guarantee against silent drift introduced by the new code paths.
    #[test]
    fn legacy_de_bit_identical_with_diversification_off() {
        let bounds = vec![(-3.0, 3.0); 4];
        let eval = |g: &[f64]| -g.iter().map(|x| (x - 1.0).powi(2)).sum::<f64>();

        let run = || -> Vec<(f64, f64, f64)> {
            let mut de = DifferentialEvolution::new(bounds.clone(), 20, 0.8, 0.9, 271828);
            for (i, genome) in de.pending_evaluations() {
                de.report_fitness(i, eval(&genome));
            }
            let mut history = Vec::new();
            for _ in 0..40 {
                let trials = de.generate_trials();
                for (target, trial) in trials {
                    de.report_trial_result(target, trial.clone(), eval(&trial));
                }
                history.push((de.best().fitness, de.mean_fitness(), de.fitness_std()));
            }
            history
        };

        let h1 = run();
        let h2 = run();
        assert_eq!(h1, h2, "legacy DE must be deterministic at fixed seed");
        // Convergence sanity: at least within 0.1 of optimum (1, 1, 1, 1).
        assert!(h1.last().unwrap().0 > -0.1);
    }

    /// Crowding ON / stagnation OFF must still be deterministic at a
    /// fixed seed. Same eval function as the legacy run; results will
    /// differ from legacy because targets are redirected, but the run
    /// itself must reproduce exactly when re-run.
    #[test]
    fn crowding_mode_is_deterministic_at_fixed_seed() {
        let bounds = vec![(-3.0, 3.0); 4];
        let eval = |g: &[f64]| -g.iter().map(|x| (x - 1.0).powi(2)).sum::<f64>();

        let run = || -> Vec<f64> {
            let mut de = DifferentialEvolution::new(bounds.clone(), 20, 0.8, 0.9, 271828);
            de.enable_crowding_selection();
            for (i, genome) in de.pending_evaluations() {
                de.report_fitness(i, eval(&genome));
            }
            let mut history = Vec::new();
            for _ in 0..40 {
                let trials = de.generate_trials();
                for (target, trial) in trials {
                    de.report_trial_result(target, trial.clone(), eval(&trial));
                }
                history.push(de.best().fitness);
            }
            history
        };
        let h1 = run();
        let h2 = run();
        assert_eq!(h1, h2, "crowding-DE must be deterministic at fixed seed");
    }
}
