# Cache Positioning and Benchmarks

This document records the evidence behind Micro Moka's positioning as a
predictable, memory-dense, single-threaded cache. It is a benchmark report, not
a universal ranking: cache results depend on key type, hasher, capacity ratio,
access distribution, object size, expiry needs, and whether admission rejection
is acceptable.

## Ecosystem Research

Research was performed on 2026-07-20 against the repositories and released
versions below.

| Cache | Policy and relevant capabilities | Cost or tradeoff relative to Micro Moka |
|---|---|---|
| [`quick_cache` 0.7.0][quick-cache] | Modified CLOCK-Pro/S3-FIFO; sync and unsync; weighted capacity, ghost history, pinning, lifecycle hooks | More policy state and features; its unsync resident still uses an atomic reference counter, and eviction can walk hot/cold rings |
| [`moka` 0.12.15][moka] | Concurrent Window TinyLFU; weighted capacity, TTL, TTI, variable expiry, listeners and loading APIs | Much broader operational surface and dependency footprint; optimized for concurrency and feature completeness |
| [`mini-moka` 0.10.3][mini-moka] | Window TinyLFU; sync and unsync | Frequency-sketch and multi-region policy work buys higher Zipf hit ratio at substantially lower single-thread throughput in this suite |
| [`lru` 0.18.1][lru] | Strict O(1) LRU, map-like API, `no_std` support | Relinks on hits and uses a separately allocated pointer-linked node per entry; no scan resistance |
| [`hashlink` 0.12.1][hashlink] | Linked hash map and LRU wrapper | Strong general-purpose update/delete throughput; strict LRU rather than a scan-resistant policy |
| [`stretto` 0.9.0][stretto] | Concurrent TinyLFU admission plus sampled LFU eviction; weighted capacity, TTL and metrics | Probabilistic frequency structures and asynchronous policy machinery target high-contention services |
| [`foyer` 0.22][foyer] | Concurrent in-memory S3-FIFO/LFU plus hybrid disk caching and observability | Designed for large concurrent and hybrid systems rather than a minimal single-owner cache |

Source inspection matters here. For example, `quick_cache` stores slab tokens in
a `HashTable`, resident/placeholder/ghost variants in a linked slab, three ring
heads, hot/cold weights, and an `AtomicU16` reference counter. `lru` stores
`NonNull` pointers in its hash map and allocates each key/value node separately.
Micro Moka stores one `u32` slab index per table bucket and keeps the key, value,
hash, packed visited/predecessor word, and successor in one contiguous slot.

The [SIEVE paper][sieve] frames cache policy around efficiency and throughput,
evaluates request and byte miss ratios on 1,559 traces, and reports one visited
bit plus a hand on top of queue links. Its exact algorithm continues walking
while entries are visited. That work is amortized, but neither the paper's
pseudocode nor the reference `libCacheSim` implementation places a bound on one
eviction sweep. Micro Moka's `try_insert` fills that tail-latency gap by exposing
a deterministic scan budget and an observable rejection result.

## Chosen Pareto Point

The project does not attempt to beat every cache on every axis. The implemented
point combines:

1. a normal exact-SIEVE path for callers that require unconditional admission;
2. a scan-budgeted path that inspects at most `admission_scan_limit` entries and
   preserves ownership of a rejected candidate;
3. a persistent hand so rejected attempts deamortize, rather than restart, a hot
   sweep;
4. a packed nonzero predecessor/visited word so `Option<SlabEntry<u64, u64>>`
   occupies 32 bytes rather than v1.1.0's 40 bytes;
5. no new production dependency, unsafe block, clock read, atomic, background
   worker, or expiry index.

This is synergistic rather than a latency-only optimization. Rejecting a
candidate when the budget is exhausted both caps policy work and protects the
recently visited resident set from short scans. Packing metadata reduces memory
and improves the number of entries that fit in a CPU cache line working set.

The default budget of 16 was selected because the synthetic Zipf and uniform
traces below remain within 0.03 percentage points of exact SIEVE while the
all-hot path is bounded to 16 inspected entries. Budget 1 increased Zipf hit
ratio slightly but rejects more aggressively; budget 64 adapted more readily but
protected less of the hot set during short scans.

## Method

Run the complete suite with:

```bash
RUSTFLAGS='--cfg bench_deps' cargo bench --bench benchmark
```

The committed harness uses deterministic seed 42 and compares Micro Moka with
`quick_cache`, `lru`, `hashlink`, `mini-moka`, and an unbounded `HashMap`.

- Throughput: capacity 1,000, key space 10,000, 10,000 operations per batch,
  500 warm-up batches, and 2,000 measured batches.
- Hit ratio: 1,000,000 requests at capacity 1,000 over Zipf and uniform traces.
- Short-scan resistance: a warmed 1,000-entry hot set alternates with bursts of
  32 unique keys.
- Byte hit ratio: deterministic synthetic object sizes from 1 to 1,024 bytes.
- Tail latency: 2,000 independently constructed caches at capacity 10,000; 1%
  of samples visit every resident before timing one full-cache insert. The
  harness reports p50, p99, p99.9 and max from `Instant` samples.
- Density: a production unit test fixes the u64/u64 slab-slot layout at 32 bytes
  on supported CI targets.

Native-default throughput is intentionally labeled as such. Micro Moka and
`HashMap` use HashDoS-resistant `RandomState`; the other benchmarked caches use
their faster crate defaults. An additional Micro Moka row uses opt-in `aHash` to
show the impact of hasher choice. Policy comparisons within Micro Moka use the
same hasher.

## Representative Results

Measured on an Apple M4 with 16 GiB RAM, macOS 15.7.3, Rust 1.94.0. Times are
nanoseconds and throughput is millions of API operations per second. These are
single-run local results; rerun on the deployment target before making a sizing
decision.

### Native-default throughput

| Operation | micro-moka | quick-cache | lru | hashlink | mini-moka | HashMap |
|---|---:|---:|---:|---:|---:|---:|
| Zipf get | 167.7 | 618.8 | 442.4 | 378.0 | 32.5 | 232.5 |
| Negative lookup | 212.4 | 682.1 | 885.7 | 810.1 | 48.7 | 264.4 |
| Zipf insert/update mix | 45.4 | 139.2 | 97.3 | 117.0 | 20.2 | 183.1 |
| Resident update | 117.4 | 329.0 | 346.9 | 316.6 | 21.0 | 205.0 |
| Mixed 95% read | 160.6 | 522.7 | 380.6 | 398.2 | 33.1 | 209.4 |
| Mixed 50% read | 99.5 | 303.3 | 233.9 | 241.8 | 26.1 | 214.0 |
| Delete + reinsert | 65.5 | 98.2 | 75.0 | 203.6 | 22.5 | 213.8 |

Micro Moka with opt-in `aHash` measured 578.5 M gets/s on the same Zipf get
workload. The 3.4x difference from its secure default is why cross-cache claims
that ignore hashers are not credible.

### Request hit ratio

| Distribution | micro exact | micro budget 16 | quick-cache | lru | mini-moka |
|---|---:|---:|---:|---:|---:|
| Zipf 0.7 | 34.53% | 34.52% | 43.8% | 32.9% | 42.5% |
| Zipf 1.0 | 68.87% | 68.85% | 74.5% | 67.5% | 74.1% |
| Zipf 1.2 | 86.78% | 86.79% | 89.4% | 86.1% | 89.3% |
| Uniform | 9.99% | 9.99% | 10.0% | 10.0% | 10.0% |
| Hot set + 32-key scans | 90.62% | 96.81% | — | — | — |

On the Zipf 1.0 trace with synthetic object sizes, exact SIEVE measured 68.87%
request hit ratio and 56.43% byte hit ratio; budget 16 measured 68.85% and
56.39%. Micro Moka remains entry-bounded and does not use object size when
choosing a victim, so byte hit ratio can diverge from request hit ratio.

### Full-cache insertion latency

| Cache/path | p50 | p99 | p99.9 | max |
|---|---:|---:|---:|---:|
| Micro Moka exact SIEVE | 42 | 84 | 8,416 | 14,583 |
| Micro Moka budget 16 | 42 | 84 | 125 | 167 |
| quick-cache | 41 | 84 | 24,125 | 24,917 |
| lru | 41 | 42 | 84 | 167 |

The key result is structural: `try_insert` cannot inspect more than the configured
number of SIEVE entries. The measured p99.9 reduction confirms that the bound is
visible end to end on this workload, while strict LRU remains an excellent choice
when scan resistance and admission control are unnecessary.

## Limitations and Rejected Directions

- The synthetic harness is deterministic and reproducible but is not a
  substitute for production traces. It does not model allocator contention,
  large heap-owned values, backend miss cost, or multi-thread sharing.
- Per-operation `Instant` measurement has timer overhead and limited resolution.
  Percentiles should be treated as comparative evidence, not universal service
  level objectives.
- SIEVE is not the hit-ratio leader on these Zipf traces. S3-FIFO and TinyLFU
  spend more state and policy work to gain 2.5-9.3 percentage points.
- A TinyLFU sketch was rejected for this positioning because it reintroduces
  counters, aging, multiple hash derivations, and an admission comparison on hot
  paths already removed from Micro Moka.
- Weighted eviction and expiration were rejected because they add per-entry
  weight/deadline state and cleanup or clock machinery. `quick_cache`, `moka`,
  `stretto`, and `foyer` already serve those requirements well.
- Negative caching does not need a specialized structure: use `Option<T>` (or a
  domain result enum) as the value. Negative-lookup throughput is measured
  explicitly above.

[sieve]: https://www.usenix.org/conference/nsdi24/presentation/zhang-yazhuo
[quick-cache]: https://github.com/arthurprs/quick-cache
[moka]: https://github.com/moka-rs/moka
[mini-moka]: https://github.com/moka-rs/mini-moka
[lru]: https://github.com/jeromefroe/lru-rs
[hashlink]: https://github.com/djc/hashlink
[stretto]: https://github.com/al8n/stretto
[foyer]: https://github.com/foyer-rs/foyer
