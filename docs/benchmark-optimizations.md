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

## Harness and initial control

`benches/compare.rs` runs 252 cases (three key shapes, two matched hashers, three capacities, fourteen operations), with nine rotating-order samples per library and a minimum five-millisecond measurement per sample. The initial A/A control, before any production optimization, has 240/252 candidate/baseline medians within 5%; eight apparent losses and four apparent wins demonstrate the noise/code-layout floor. The 5% band is descriptive, not a significance test. At this stage Micro Moka is more than 5% faster than every measured peer in 42/252 cases; this is a fixed synthetic matrix, not a representative market-share metric.

Raw control samples and allocation counts are in `benches/results/before.csv` and `allocations-before.csv`. Machine: Apple M4, aarch64 macOS, Rust 1.94.0, release LTO and one codegen unit. Baseline and candidate both have preallocated capacity. Competitors use the same hasher type/state; LRU policies naturally differ on churn/mixed workloads. Iteration measures a commutative value sum, not traversal order. Clear/refill measures a whole clear plus refill cycle (ns/cycle), whereas clear-empty measures repeated clearing of an already empty, previously populated cache. No allocator instrumentation is active in the timing executable.

```sh
python3 benches/run.py unique-run-name
# Narrow a diagnostic run without changing the workload implementation:
MM_FILTER=load MM_CAPACITIES=128,1024 MM_SAMPLES=9 python3 benches/run.py loader-check
cargo run --release --locked --manifest-path benches/Cargo.toml --bin allocations
cargo test --locked --manifest-path benches/Cargo.toml
```

`run.py` stores raw CSV, summary Markdown, source hashes, lockfile hash, revision, environment settings, and compiler/platform details. All results use separately compiled published Micro Moka 1.2.0 as the baseline. `hashlink` has no native loader in this harness; its best available get/insert composition is labeled as such here. The baseline/current adapters initially both use owned loading; subsequent implementation commits switch only the candidate adapter to the new borrowed API.

Both dependency graphs now pass `cargo audit --deny warnings`. The benchmark lock was refreshed to current releases, including LRU 0.18.4, hashlink 0.12.2, rand 0.10.3, and non-yanked chacha20 0.10.2. Adapter equivalence tests and allocation probes run in CI. ## Borrowed loading in isolation

`borrowed-only.csv` changes only the candidate loader API; clearing and iteration are unchanged. Across all twelve String loader-hit configurations, candidate median time is 0.379–0.873 times v1.2.0 (about 13–62% lower). Integer-key controls remain essentially unchanged. Loader-cycle cases do not show a >5% regression. Candidate allocation counts for 10,000 String hits are zero, versus 10,000 in v1.2.0; quick_cache, lru, and hashlink also avoid hit allocations.

The matched comparisons still show losses: hashlink is faster in the String loader-hit cases on this machine, while Micro Moka generally beats or approaches quick_cache/lru. This API change closes the allocation gap rather than establishing universal leadership. Three >5% apparent regressions are in unchanged `clear-empty` code, an allocator-sensitive control; the clear experiment will measure that path directly. Raw results, per-case medians, and provenance are checked in alongside the initial control. Tests verify single hashing, no cloning/loading on hits, unsized queries, owned-key/loader panic preservation, zero capacity, and SIEVE visitation/eviction.
