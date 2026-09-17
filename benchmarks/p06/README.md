# P-06 replication benchmark

This frozen benchmark calibrates the default number of stochastic
realizations. Its manifest pins five preset fixtures by SHA-256, all five brain
profiles, all nine goals, the 3-second model configuration, DSP commit
`81b51fad005bb6522cbbf42ef08ca4a3c6c9ab06`, and a 64-replicate reference
panel.

Run it from the repository root:

```bash
cargo run --locked --release --bin p06_replication_benchmark -- --threads 4
```

For panel sizes 4, 8, 16 and 32, sixteen fixed subpanels are compared with the
64-replicate reference. A size passes only when its 95th-percentile absolute
mean error is at most 0.01, median Spearman rank correlation is at least 0.95,
and top-one/reference-inconclusive agreement is at least 95%. If none passes,
the contract selects 64.

The current run selected 64. At 32 realizations, rank and top-one criteria
passed, while the 95th-percentile absolute mean error was 0.01393 and missed
the 0.01 threshold. The summary also reports the empirical distribution of
sample variances for all paired preset differences on the 64-replicate panel.

Outputs are committed in `results/raw_scores.csv` and `results/summary.json`.
The raw rows are sorted independently of worker scheduling. Re-run this
benchmark after changes to the stochastic model, scoring in P-08, analysis
duration, input scale, seed tree, RNG, preset fixtures or DSP renderer.
