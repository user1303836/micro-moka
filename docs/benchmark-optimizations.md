# Benchmark-led optimizations

**Final selected-code results:** [benchmark-results.md](benchmark-results.md). This notebook retains historical experiments, including rejected implementations; older headline tables are not the final result.

This work targets broader measured wins against common single-threaded Rust caches without changing SIEVE semantics, HashDoS-resistant defaults, Rust 1.76 support, or the safe-Rust production dependency footprint.

## Retained implementation

- Add borrowed-key loading without hit-path key construction.
- Preserve table/slab/free-list storage on clear, including panic-safe cleanup.
- Make `Iter::count` constant-time. **Keep the original iterator layout and traversal.**

The more ambitious sparse traversal and folding prototypes below are **rejected**, not part of the final implementation. A second x86-64 host exposed up to 74% partial-iteration regressions in chunked folding; even smaller traversal changes destabilized other generated loops. The retained count-only change showed 159 ties, 19 wins and two small losses (5.5% and 7.2%) across 180 ARM iterator cases, without the large repeatable losses. Empty/trailing-hole traversal improvements are deferred, rather than purchased by regressing other iteration patterns.

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

The initial `benches/compare.rs` matrix ran 252 cases (three key shapes, two matched hashers, three capacities, fourteen operations), with nine rotating-order samples per library and a minimum five-millisecond measurement per sample. The initial A/A control, before any production optimization, has 240/252 candidate/baseline medians within 5%; eight apparent losses and four apparent wins demonstrate the noise/code-layout floor. The 5% band is descriptive, not a significance test. At this stage Micro Moka is more than 5% faster than every measured peer in 42/252 cases; this is a fixed synthetic matrix, not a representative market-share metric.

Raw control samples and allocation counts are in `benches/results/before.csv` and `allocations-before.csv`. Machine: Apple M4, aarch64 macOS, Rust 1.94.0, release LTO and one codegen unit. Baseline and candidate both have preallocated capacity. Competitors use the same hasher type/state; LRU policies naturally differ on churn/mixed workloads. Iteration measures a commutative value sum, not traversal order. Clear/refill measures a whole clear plus refill cycle (ns/cycle), whereas clear-empty measures repeated clearing of an already empty, previously populated cache. No allocator instrumentation is active in the timing executable.

```sh
python3 benches/run.py unique-run-name
# Narrow a diagnostic run without changing the workload implementation:
MM_FILTER=load MM_CAPACITIES=128,1024 MM_SAMPLES=9 python3 benches/run.py loader-check
cargo run --release --locked --manifest-path benches/Cargo.toml --bin allocations
cargo test --locked --manifest-path benches/Cargo.toml
```

`run.py` stores raw CSV, summary Markdown, source hashes, lockfile hash, revision, environment settings, and compiler/platform details. All results use separately compiled published Micro Moka 1.2.0 as the baseline. `hashlink` has no native loader in this harness; its best available get/insert composition is labeled as such here. The baseline/current adapters initially both use owned loading; subsequent implementation commits switch only the candidate adapter to the new borrowed API.

Both dependency graphs now pass `cargo audit --deny warnings`. The benchmark lock was refreshed to current releases, including LRU 0.18.4, hashlink 0.12.2, rand 0.10.3, and non-yanked chacha20 0.10.2. Adapter equivalence tests and allocation probes run in CI.

## Borrowed loading in isolation

`borrowed-only.csv` changes only the candidate loader API; clearing and iteration are unchanged. Across all twelve String loader-hit configurations, candidate median time is 0.379–0.873 times v1.2.0 (about 13–62% lower). Integer-key controls remain essentially unchanged. Loader-cycle cases do not show a >5% regression. Candidate allocation counts for 10,000 String hits are zero, versus 10,000 in v1.2.0; quick_cache, lru, and hashlink also avoid hit allocations.

The matched comparisons still show losses: hashlink is faster in the String loader-hit cases on this machine, while Micro Moka generally beats or approaches quick_cache/lru. This API change closes the allocation gap rather than establishing universal leadership. Three >5% apparent regressions are in unchanged `clear-empty` code, an allocator-sensitive control; the clear experiment will measure that path directly. Raw results, per-case medians, and provenance are checked in alongside the initial control. Tests verify single hashing, no cloning/loading on hits, unsized queries, owned-key/loader panic preservation, zero capacity, and SIEVE visitation/eviction.

## Allocation-preserving clearing

The first buffer-reuse prototype (`borrowed-clear`) eliminated allocations but still lost the empty-clear comparison to peers because of an unconditional out-of-line call. The retained implementation (`borrowed-clear-fast-empty`) adds an inline empty check and keeps destructor work out of line. On the 36 clear configurations, 23 medians improve over baseline by more than 5%, 13 are within 5%, and none regress by more than 5%; it beats the fastest peer by more than 5% in 29, with 7 ties. Full clear/refill cycles are mostly tied with v1.2 for String keys because destroying and reconstructing keys dominates; empty clears are dramatically cheaper. These are local descriptive medians, not latency guarantees.

Allocation tests assert zero new allocations during clear and exactly one deallocation per owned String key (no backing-buffer frees). New tests cover reuse of slab/free-list pointers through holes/refills, key-destructor panic, and repeated clearing after the last resident was removed. The existing value-destructor panic tests remain passing. The intermediate experiment is retained to show why simply swapping in a buffer-preserving implementation was not the final choice.

## Rejected iterator experiments

An unconditional `remaining == 0` check in `next` caused 30–40% dense-iteration regressions (`iterator-early-stop`), so that version was rejected. Linked iteration at 50% occupancy also lost badly to contiguous scanning (`iterator-density-half`). The intermediate checkpoint used links only at <=1/32 occupancy, with no new per-entry storage, and dispatches folds once rather than once per resident. Empty iteration and `count` are constant-time. Iteration order remains deliberately unspecified.

The matrix now includes explicit `for` loops and four intermediate occupancies (approximately 1%, 6.25%, 25%, 50%), **342 configurations total**, preventing a one-resident-only optimization from hiding moderate-density costs. Direct slot folding and chunked folding were measured separately; the intermediate 64-slot chunk checkpoint showed broader improvements on ARM but was not portable enough to retain. In `iterator-final`, 137 of 180 iterator medians improve over v1.2 by >5%, 33 are within 5%, and **10 regress**. Regressions are at capacity 16,384, principally partially vacant slabs, up to about **17%**. Against the fastest peer: 61 wins, 70 ties, 49 losses. These remaining trade-offs must stay visible in the final comparison; this is not a zero-regression claim.

The exploratory CSVs record tested outcomes, not statistically established portable tuning constants. Full-factorial source-replayed comparisons are below; final reruns follow. Model tests independently check 1,050,000 mixed operations (reduced under Miri), along with table/slab/free-list/deque/hand/visited invariants and dense/sparse/clear/refill transitions.

## Full-factorial comparison

`benches/ablate.py` reconstructs v1.2 source and applies each subset of the three production patches in isolated, ignored build directories. Every variant passes debug/release model tests and adapter tests before measurement. The candidate package keeps version 1.3 so that the independent published 1.2 baseline can coexist. Borrowed-off variants use the owned loader in both the adapter and reference-model test. No production `cfg` switches or runtime benchmarking branches are introduced.

Initial factorial run: five rotating samples, >=3 ms/sample, 342 cases. Columns count medians >5% faster / within 5% / >5% slower, not statistically significant wins:

| Borrowed | Clear | Iterator | vs published 1.2 W/T/L | vs fastest peer W/T/L |
|---|---|---|---|---|
| off | off | off | 6 / 333 / 3 | 68 / 55 / 219 |
| on | off | off | 20 / 314 / 8 | 63 / 63 / 216 |
| off | on | off | 26 / 312 / 4 | 76 / 64 / 202 |
| off | off | on | 137 / 191 / 14 | 96 / 123 / 123 |
| on | on | off | 42 / 298 / 2 | 92 / 54 / 196 |
| on | off | on | 155 / 162 / 25 | 96 / 126 / 120 |
| off | on | on | 163 / 167 / 12 | 117 / 124 / 101 |
| on | on | on | 171 / 158 / 13 | 114 / 116 / 112 |

On the **original 252-case subset**, fastest-peer wins increase from 42 to 86 (103 additional cases within 5%), so the improvement is not just a consequence of adding iterator cases. On the expanded matrix, the combined checkpoint wins strictly against quick_cache in 222/342, lru in 204/342, and hashlink in 184/342; it does **not** beat the fastest of all three in most cases. The no-change control has nine >5% deviations, illustrating that minor results can move with code layout, random hash seeds, and machine load.

All raw samples, replay identities, per-variant source hashes and test logs are in `benches/results/factorial-*`. Reproduce with:

```sh
python3 benches/ablate.py historical-factorial --harness-ref bade8b4 --iterator-ref bade8b4
# Omit both overrides for the retained implementation and corrected harness.
# Bits are borrowed, clear, iterator; runs refuse to overwrite prior evidence.
python3 benches/ablate.py independent --variants 100,010,001,111
```

### Benchmark hardening after the factorial run

The mixed case means **95% lookups**, not a guaranteed 95% hit rate. Its resident set changes during initial churn, so final reruns add 64 unmeasured batches before timing rather than comparing short transient phases. Iteration setups now validate both their expected count and checksums before timing. These changes apply equally to every library. `run.py` also saves tracked source diffs for uncommitted experiments, and the new CI evidence job compares all libraries on x86-64 Linux. CI-host timings are paired exploratory evidence, not controlled hardware-performance guarantees.

### Equal-work correction for the evolving mixed trace

The second factorial run (`steady-factorial-*`) and two combined repeats (`combined-final`, `combined-repeat`) retained large mixed-case fluctuations despite 64 warm-up batches: one unchanged mixed path differed by 46% in a repeat. Warm-up alone does not guarantee a stationary resident set for this read-without-refill trace. An adaptive time window lets faster implementations process different trace prefixes, so these mixed-case rows do **not** establish like-for-like throughput regressions.

The corrected harness measures exactly **256 batches of 8,192 operations** for the mixed case, after the same warm-up/pilot work for every library. Other cases retain adaptive timing. The new test compares candidate and published-baseline resident contents after each equal-length mixed batch. In the initial corrected diagnostic (`mixed-fixed-operations`), all 18 mixed-case comparisons are within 5% of baseline. Earlier raw results are retained rather than silently replaced; final headline results use the corrected harness and retained count-only iterator, not the rejected sparse/chunked prototypes.

### Additional portability evidence

`linux-35605203376` is the equal-work run that rejected chunked folding: 34 cases regressed against 1.2, with partial-iteration losses up to 74%. `iterator-inlined-original-fold`, `iterator-conservative`, and `iterator-empty-prefix` also exposed repeatable partial-iteration costs. `iterator-count-only` preserves the original traversal and avoids those large losses. The raw unsuccessful experiments remain checked in; generated-evidence attributes keep them collapsed in GitHub's code diff.

`benches/footprint.py` measures three clean, offline, alternating-order builds of a small cache-using application, with source/manifests/checksums recorded. The intermediate implementation had the same 336,016-byte stripped executable size as baseline and approximately 2.08-second median clean builds. This is a representative application check, not a universal size or build-time guarantee; the retained implementation is remeasured separately.
