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
Nor is it the only design with bounded admission work: Senba bounds each shard
at 64 entries and LRU has constant policy work.

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
same successful admission that exact SIEVE performs in one call. Both paths
inspect 10,001 residents in total in this constructed case; the budgeted path
splits that work into calls of at most 16 inspections and exposes every
deferred attempt to the caller.

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
- Admission outcomes: 2,000 independently built, full 10,000-entry caches.
  Exactly 1% have every resident visited. Accepted admissions and rejected
  attempts are reported separately with counts, rates, candidate disposition,
  and deterministic policy-scan work.
- Successful-admission work: 200 independently built full caches per Micro Moka
  path, with every resident visited. Every setup ends with the candidate
  resident. The budgeted path includes and counts every rejected retry through
  the final successful admission.
- Density: a production unit test fixes Micro Moka's `u64`/`u64` slab-slot
  layout at 32 bytes on supported CI targets. Senba's 16-byte stride is a public
  type-level contract.

Micro Moka, `sieve-cache`, and `HashMap` use `RandomState` in the native-default
throughput table. Other caches use their crate defaults; Senba uses `Slot16` and
xxh3. The extra Micro Moka aHash measurement isolates hasher cost. Policy
comparisons within Micro Moka use the same aHash state.

## Representative Results

Measured on an Apple M4 with 16 GiB RAM, macOS 15.7.3, Rust 1.94.0. Throughput
is millions of API operations per second. These are single-run local results;
rerun on the deployment target. Senba's README states that its SIMD lookup is
x86_64 AVX2-only, so this Apple Silicon run uses its scalar fallback.

### Native-default throughput

| Operation | micro-moka | quick-cache | lru | hashlink | mini-moka | sieve-cache | senba | HashMap |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Zipf get | 202.4 | 638.4 | 455.9 | 464.4 | 34.7 | 243.4 | 35.9 | 232.6 |
| Negative lookup | 224.1 | 716.1 | 706.6 | 764.6 | 44.8 | 274.2 | 34.2 | 256.9 |
| Zipf insert/update | 70.4 | 167.4 | 161.8 | 162.3 | 22.3 | 62.0 | 43.2 | 195.6 |
| Resident update | 118.8 | 302.5 | 332.2 | 301.8 | 23.5 | 196.5 | 41.3 | 213.9 |
| Mixed 95% read | 195.7 | 566.4 | 364.4 | 376.1 | 34.1 | 202.2 | 46.9 | 214.3 |
| Mixed 50% read | 127.9 | 295.1 | 250.2 | 253.7 | 26.3 | 94.3 | 46.4 | 215.1 |
| Delete + reinsert | 67.5 | 97.6 | 84.8 | 205.2 | 24.2 | 59.5 | 24.1 | 221.6 |

Micro Moka with opt-in aHash measured 611.9 million gets/s on the same Zipf
workload, 3.0 times its secure-default result. Cross-cache throughput claims
that do not account for hashers are not credible.

### Request and byte hit ratio

| Distribution | micro exact | quick-cache | lru | mini-moka | sieve-cache 1.1.6 | senba |
|---|---:|---:|---:|---:|---:|---:|
| Zipf 0.7 | 43.8% | 43.8% | 32.9% | 42.5% | 34.1% | 43.3% |
| Zipf 0.9 | 64.5% | 64.6% | 55.6% | 63.9% | 56.8% | 64.1% |
| Zipf 1.0 | 74.5% | 74.5% | 67.5% | 74.1% | 68.5% | 74.2% |
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

### Admission work and outcomes

One budgeted call in each of 2,000 independent cache setups, with exactly 1%
all-visited resident sets:

| Outcome/path | Count | Rate | Candidate state | Resident inspections |
|---|---:|---:|---|---:|
| Accepted attempt (successful admission) | 1,980 | 99.0% | Inserted | Exactly 1 |
| Rejected attempt | 20 | 1.0% | Returned; not inserted | Exactly 16 |

The 1% mix is deliberately constructed rather than an estimate of a production
rejection probability. Counts and rates make the outcomes explicit; no latency
quantile is inferred from either the 1,980 accepted samples or the 20 rejected
samples.

The all-hot experiment instead accounts for the work needed to reach the same
successful admission. Each row uses 200 independent, full 10,000-entry cache
setups with every resident visited:

| Micro Moka path | Calls per successful admission | Rejected attempts | Successful admissions | Resident inspections through success |
|---|---:|---:|---:|---:|
| Exact `insert` | Exactly 1 | 0 | 200 | Exactly 10,001 in one call |
| Budget-16 `try_insert`, retry through success | Exactly 626 (625 rejected + 1 accepted) | 125,000 | 200 | Exactly 10,001 total; at most 16 per call |

Across the 200 retry chains, 125,000 of 125,200 calls were rejected attempts
(99.840%) and 200 were accepted attempts that successfully admitted the
candidate (0.160%). Calls within a retry chain are sequential, not 125,200
independent samples. The value of budgeted admission is the hard per-call
inspection limit and explicit ownership-preserving rejection, not lower total
work to eventual success. Competitors remain in the throughput and hit-ratio
tables; this outcome section does not compare rejected attempts with successful
competitor insertions.

## Limitations and Rejected Directions

- Synthetic deterministic workloads expose controlled mechanisms but do not
  replace production traces. The suite omits allocator contention, large
  heap-owned values, backend miss cost, and multi-thread sharing.
- Throughput uses wall-clock measurement and is sensitive to timer, scheduler,
  compiler, hasher, and machine effects. The admission section therefore makes
  no per-operation nanosecond or tail-percentile claim.
- Fixed-size entry capacity can misrepresent memory use. Micro Moka does not
  account for heap allocations owned by keys/values and does not optimize byte
  hit ratio.
- A TinyLFU sketch was rejected for this positioning because it reintroduces
  counters, aging, multiple hash derivations, and admission comparisons already
  removed from Micro Moka's hot paths.
- Weighted eviction and expiration were rejected because they add per-entry
  weight/deadline state, clock reads, cleanup, and policy coupling. `quick_cache`,
  `mini-moka`, `moka`, `stretto`, and `foyer` serve those requirements.
- Sharding or fixed slots can bound unconditional admission work more tightly
  and improve density, as Senba demonstrates, but give up the chosen global
  policy, arbitrary entry size, or safe-Rust implementation.

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
