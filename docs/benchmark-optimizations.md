# Benchmark-led optimizations

This work targets broader measured wins against common single-threaded Rust caches without changing SIEVE semantics, HashDoS-resistant defaults, Rust 1.76 support, or the safe-Rust production dependency footprint.

## Comparison contract

- Baseline: released Micro Moka 1.2.0, independently of the candidate path dependency.
- Comparators: current published `quick_cache::unsync`, `lru`, and `hashlink`; retain the existing policy-quality suite for wider coverage.
- Use the same hasher configuration and resident capacity for like-for-like measurements. Keep default-hasher results separately labeled.
- Test integer and owned-string keys, multiple capacities, hits/misses, updates, admissions, loader hit/miss paths, clearing/refilling, and dense/sparse iteration.
- Measure each proposed change independently and in combination. Record raw repeated samples and allocation counts, including losing cases. Do not infer high-percentile latency from small samples.
- Regressions in ordinary lookups/updates, dense iteration, hit ratio, or panic safety are not acceptable merely to win an isolated microbenchmark.
- Run correctness tests and relevant comparisons before pushing implementation commits.

## Planned sequence

1. Establish comparison harness, baseline evidence, and benchmark dependency hygiene.
2. Evaluate borrowed-key loading with hit-only key-allocation avoidance.
3. Evaluate buffer-preserving clear, including destructor-panic consistency.
4. Evaluate iterator work reduction against dense and sparse controls.
5. Add permanent state-machine coverage, run individual/combined comparisons, and publish results and limitations here.

No performance claims are made by this initial planning commit. Results will be added as experiments complete.
