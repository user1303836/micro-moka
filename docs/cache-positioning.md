# Cache Positioning and Benchmarks

This report treats cache design as a multi-objective optimization problem. A
cache can improve one workload or operational axis while regressing another, so
the evidence below is a positioning argument rather than a universal ranking.
Results depend on key type, hasher, capacity ratio, access distribution, object
size, expiry needs, and whether admission rejection is acceptable.

Research and source inspection were performed on 2026-07-20. The benchmark was
rerun after correcting Micro Moka's SIEVE traversal to start at the oldest
resident and advance toward newer residents, as specified by the paper.

## Objectives and Tradeoffs

| Objective | What improves it | Typical cost |
|---|---|---|
| Average read/write latency | Fast hashing, compact lookup metadata, simple policy work | Weaker HashDoS protection, unsafe/SIMD specialization, or lower policy quality |
| Tail latency and predictability | Constant-time policies, bounded scans, sharding, admission rejection | Lower global policy fidelity or delayed/rejected candidates |
| Request and byte hit ratio | Better admission/eviction models, weights, more history | More metadata, hashing, counters, and maintenance work |
| Memory density | Fixed slots, packed metadata, compact indices | Entry-size limits, unsafe layout code, or wasted padding |
| Expiration and loading | TTL, TTI, variable expiry, background cleanup, loaders | Clock reads, scheduling/index state, dependencies, and a larger API |
| Operational simplicity | Single ownership, no workers, observable outcomes | No concurrency, automatic expiry, or built-in metrics |

Negative caching itself needs no specialized policy: store `Option<T>` or a
domain result enum. Negative lookup speed is nevertheless measured because a
miss-heavy workload can be a cache's dominant path.

## Rust Ecosystem Review

| Cache | Policy and relevant capabilities | Cost or tradeoff relative to Micro Moka |
|---|---|---|
| [`quick_cache` 0.7.0][quick-cache] | Modified CLOCK-Pro/S3-FIFO; sync and unsync; weighted capacity, ghost history, pinning, and lifecycle hooks | More features and policy state; its unsync resident representation includes atomic reference state and eviction can walk policy rings |
| [`moka` 0.12.15][moka] | Concurrent Window TinyLFU; weighted capacity, TTL, TTI, variable expiry, listeners, loaders, and async APIs | Broad operational surface and dependency footprint optimized for concurrent, feature-rich services |
| [`mini-moka` 0.10.3][mini-moka] | Sync and unsync Window TinyLFU; weighting, TTL, and TTI | Frequency sketch, multi-region policy, clocks, and expiry state trade throughput and density for features and policy quality |
| [`sieve-cache` 1.1.6][sieve-cache] | Published core single-threaded cache plus sync/sharded wrappers; vector nodes and a standard `HashMap` | Requires `K: Clone` and stores the key in both structures; the released source appends new nodes and starts at that newest node, so its measured policy is not reference SIEVE orientation |
| [`senba` 0.2.0][senba] | Single-threaded, sharded SIEVE-like policy; fixed 16/32/64-byte slots; at most 64 residents per shard; bit-parallel eviction and optional AVX2 lookup | Excellent density and bounded per-shard policy work, but uses substantial unsafe layout/SIMD code, requires Rust 1.85, defaults to non-HashDoS xxh3, and cannot directly store entries larger than 64 bytes |
| [`lru` 0.18.1][lru] | Strict O(1) LRU, map-like API, and `no_std` support | Relinks on hits and separately allocates pointer-linked nodes; no scan bypass |
| [`hashlink` 0.12.1][hashlink] | Linked hash map and LRU wrapper | Strong update/delete throughput; strict LRU rather than a scan-filtering admission path |
| [`stretto` 0.9.0][stretto] | Concurrent TinyLFU admission, sampled LFU eviction, weighted capacity, TTL, and metrics | Probabilistic and asynchronous policy machinery targets high-contention services |
| [`foyer` 0.22.3][foyer] | Concurrent in-memory S3-FIFO/LFU plus hybrid disk caching and observability | Designed for large concurrent and hybrid systems rather than a minimal single-owner cache |

The direct comparisons change the claim. Micro Moka is not the raw density
leader: Senba's `Slot16` stores a `u64`/`u64` pair in a 16-byte arena stride,
whereas Micro Moka's packed slab slot is 32 bytes before its hash-table index.
Nor is it the unconditional-admission tail-latency leader: Senba bounds each
shard at 64 entries and LRU has constant policy work.

The ecosystem is less well served at a different intersection: safe Rust,
Rust 1.76 support, arbitrary-size key/value storage, HashDoS-resistant defaults,
a global reference-SIEVE order, and an explicit per-attempt scan bound that
returns ownership of a deferred candidate. Among the reviewed caches, Micro
Moka is the only one combining those properties.

The [SIEVE paper][sieve] evaluates request and byte miss ratios on 1,559 traces.
It inserts at the newest end, starts the eviction hand at the oldest end, and
moves toward newer entries. The paper also notes that SIEVE is not universally
scan-resistant. Exact SIEVE keeps walking while entries are visited, so its work
is amortized but not bounded for one insertion. Micro Moka retains that exact
path and adds optional, observable admission backpressure.

## Implemented Pareto Point

The v1.2 design combines:

1. `insert`, an exact/global SIEVE path for unconditional admission;
2. `try_insert`, which examines at most `admission_scan_limit` residents and
   returns `Err((K, V))` without losing the candidate when the budget expires;
3. a persistent hand, so retries continue a sweep instead of restarting it;
4. a packed nonzero predecessor/visited word, reducing
   `Option<SlabEntry<u64, u64>>` from 40 to 32 bytes;
5. no new production dependency, unsafe block, clock read, atomic, background
   worker, expiry index, or internal metrics object.

Budgeted admission is not a faster successful insertion. It is a scheduling and
backpressure primitive: each call has bounded policy work, while the caller
chooses whether to retry, bypass the cache, or drop/count the candidate. On a
fully visited 10,000-entry cache, budget 16 requires 626 calls to achieve the
same successful admission that exact SIEVE performs in one call. Total measured
policy time is similar; the work is split into predictable pieces.

The default of 16 is a workload-informed compromise, not a universal optimum.
Across the deterministic Zipf and uniform workloads below it stays within 0.03
percentage points of exact SIEVE. On the hot-set workload, 32-key scan bursts
are rejected before they can displace the repeatedly revisited residents. A
budget of 64 admits enough of that scan to collapse the benefit.

## Reproducible Method

Run the complete suite with:

```bash
cargo run --release --locked --manifest-path benches/Cargo.toml
```

The committed `benches/Cargo.lock` fixes direct and transitive versions without
placing benchmark-only, Rust 1.85 dependencies in the library's Rust 1.76
dependency graph. All evidence-bearing direct dependencies are also pinned
exactly in `benches/Cargo.toml`:
`quick_cache` 0.7.0, `lru` 0.18.1, `hashlink` 0.12.1, `mini-moka` 0.10.3,
`sieve-cache` 1.1.6, `senba` 0.2.0, `rand` 0.10.2, `rand_distr` 0.6.0, and
`ahash` 0.8.12. The harness uses deterministic seed 42.

- Throughput: capacity 1,000, key space 10,000, 10,000 operations per batch,
  500 warm-up batches, and 2,000 measured batches.
- Hit ratio: 1,000,000 requests at capacity 1,000 over Zipf and uniform traces.
- Scan filtering: a warmed 1,000-entry hot set alternates with 32 unique keys.
- Byte hit ratio: deterministic synthetic object sizes from 1 to 1,024 bytes;
  object size is measured but does not influence entry-bounded eviction.
- Admission-attempt latency: 2,000 independently built, full 10,000-entry
  caches. Exactly 1% have every resident visited. Accepted and rejected
  outcomes are reported separately with counts and rates.
- Matched successful admission: 200 independently built full caches with every
  resident visited. Every row ends with the candidate resident. The budgeted
  row includes all retries through success.
- Density: a production unit test fixes Micro Moka's `u64`/`u64` slab-slot
  layout at 32 bytes on supported CI targets. Senba's 16-byte stride is a public
  type-level contract.

Micro Moka, `sieve-cache`, and `HashMap` use `RandomState` in the native-default
throughput table. Other caches use their crate defaults; Senba uses `Slot16` and
xxh3. The extra Micro Moka aHash measurement isolates hasher cost. Policy
comparisons within Micro Moka use the same aHash state.

## Representative Results

Measured on an Apple M4 with 16 GiB RAM, macOS 15.7.3, Rust 1.94.0. Times are
nanoseconds and throughput is millions of API operations per second. These are
single-run local results; rerun on the deployment target. Senba's README states
that its SIMD lookup is x86_64 AVX2-only, so this Apple Silicon run uses its
scalar fallback.

### Native-default throughput

| Operation | micro-moka | quick-cache | lru | hashlink | mini-moka | sieve-cache | senba | HashMap |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Zipf get | 200.5 | 640.1 | 449.8 | 452.2 | 34.4 | 231.2 | 45.3 | 240.5 |
| Negative lookup | 212.8 | 665.4 | 672.7 | 699.0 | 47.2 | 268.5 | 32.7 | 243.3 |
| Zipf insert/update | 71.2 | 176.2 | 164.1 | 160.3 | 22.2 | 65.4 | 40.5 | 205.5 |
| Resident update | 82.6 | 330.5 | 329.3 | 316.9 | 25.0 | 194.4 | 39.0 | 209.8 |
| Mixed 95% read | 191.8 | 560.9 | 362.3 | 370.5 | 34.3 | 195.1 | 46.1 | 212.2 |
| Mixed 50% read | 125.9 | 300.8 | 244.0 | 253.2 | 26.5 | 91.1 | 46.4 | 209.8 |
| Delete + reinsert | 67.1 | 100.9 | 72.2 | 208.0 | 23.9 | 59.2 | 23.8 | 223.9 |

Micro Moka with opt-in aHash measured 611.5 million gets/s on the same Zipf
workload, 3.0 times its secure-default result. Cross-cache throughput claims
that do not account for hashers are not credible.

### Request and byte hit ratio

| Distribution | micro exact | quick-cache | lru | mini-moka | sieve-cache 1.1.6 | senba |
|---|---:|---:|---:|---:|---:|---:|
| Zipf 0.7 | 43.8% | 43.8% | 32.9% | 42.5% | 34.1% | 43.3% |
| Zipf 0.9 | 64.5% | 64.6% | 55.6% | 63.9% | 56.8% | 64.1% |
| Zipf 1.0 | 74.5% | 74.5% | 67.5% | 74.0% | 68.5% | 74.2% |
| Zipf 1.2 | 89.3% | 89.4% | 86.1% | 89.3% | 86.6% | 89.2% |
| Uniform | 10.0% | 10.0% | 10.0% | 10.0% | 10.0% | 10.0% |

The released `sieve-cache` 1.1.6 result must not be interpreted as exact-SIEVE
quality because its inspected source traverses from the newest end. Micro Moka's
corrected exact policy now matches quick-cache on Zipf 1.0 in this synthetic
suite; neither result predicts every production trace.

| Workload | micro exact | micro budget 16 |
|---|---:|---:|
| Zipf 0.7 | 43.77% | 43.80% |
| Zipf 1.0 | 74.50% | 74.52% |
| Zipf 1.2 | 89.30% | 89.31% |
| Uniform | 9.97% | 9.98% |
| Hot set + 32-key scans | 0.11% | 96.81% |

Budget 16 rejected 30,944 candidates on the hot-set/scan workload. That result
is deliberate bypass, not free hit-ratio improvement: callers must have a valid
fallback for rejected values. Budget 64 rejected only 15 candidates and matched
exact SIEVE's 0.11% on this workload.

With synthetic 1-1,024-byte objects on Zipf 1.0, exact SIEVE measured 74.50%
request hit ratio and 64.12% byte hit ratio; budget 16 measured 74.52% and
64.18%. Micro Moka is not weight-aware, so these byte results are observations,
not a weighted-eviction capability.

### Admission latency and outcomes

One budgeted call, with 1% all-visited resident sets:

| Outcome | Count | Rate | p50 | p99 | p99.9 | max |
|---|---:|---:|---:|---:|---:|---:|
| Accepted | 1,980 | 99.0% | 42 | 84 | 125 | 167 |
| Rejected | 20 | 1.0% | 41 | 84 | 84 | 125 |

This table does not compare rejected work with successful competitor inserts.
The matched-outcome table below times eventual success after every resident has
been visited:

| Cache/path | p50 | p99 | p99.9 | max |
|---|---:|---:|---:|---:|
| Micro Moka exact | 12,959 | 18,333 | 18,791 | 19,750 |
| Micro Moka budget 16, retry through success | 12,375 | 21,959 | 22,708 | 22,959 |
| quick-cache | 23,709 | 25,458 | 26,208 | 26,375 |
| sieve-cache 1.1.6 | 5,042 | 9,625 | 10,375 | 11,083 |
| senba Slot16 | 42 | 208 | 584 | 45,875 |
| lru | 41 | 83 | 83 | 83 |

Budget 16 took exactly 626 attempts in every matched sample. Its value is the
hard per-call inspection limit and explicit rejection, not lower total latency
to eventual success. Senba and LRU are better choices when unconditional
admission tail latency dominates and their other tradeoffs are acceptable.

## Limitations and Rejected Directions

- Synthetic deterministic workloads expose controlled mechanisms but do not
  replace production traces. The suite omits allocator contention, large
  heap-owned values, backend miss cost, and multi-thread sharing.
- Per-operation `Instant` measurement has timer overhead and limited resolution.
  Percentiles are comparative evidence, not universal service objectives; the
  rejected distribution has only 20 samples.
- Fixed-size entry capacity can misrepresent memory use. Micro Moka does not
  account for heap allocations owned by keys/values and does not optimize byte
  hit ratio.
- A TinyLFU sketch was rejected for this positioning because it reintroduces
  counters, aging, multiple hash derivations, and admission comparisons already
  removed from Micro Moka's hot paths.
- Weighted eviction and expiration were rejected because they add per-entry
  weight/deadline state, clock reads, cleanup, and policy coupling. `quick_cache`,
  `mini-moka`, `moka`, `stretto`, and `foyer` serve those requirements.
- Sharding or fixed slots could beat Micro Moka on density and admission tail,
  as Senba demonstrates, but would give up the chosen global policy, arbitrary
  entry size, or safe-Rust implementation.

[sieve]: https://www.usenix.org/conference/nsdi24/presentation/zhang-yazhuo
[quick-cache]: https://github.com/arthurprs/quick-cache
[moka]: https://github.com/moka-rs/moka
[mini-moka]: https://github.com/moka-rs/mini-moka
[sieve-cache]: https://github.com/jedisct1/rust-sieve-cache
[senba]: https://github.com/saka1/senba-cache
[lru]: https://github.com/jeromefroe/lru-rs
[hashlink]: https://github.com/djc/hashlink
[stretto]: https://github.com/al8n/stretto
[foyer]: https://github.com/foyer-rs/foyer
