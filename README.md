# Micro Moka

[![GitHub Actions][gh-actions-badge]][gh-actions]
[![crates.io release][release-badge]][crate]
[![docs][docs-badge]][docs]
[![license][license-badge]](#license)

A predictable, memory-dense, single-threaded cache for Rust. Micro Moka is
forked from [Mini Moka][mini-moka-git] and uses the [SIEVE][sieve-paper]
eviction policy without locks, atomics, unsafe code, background work, or an
async runtime.

```rust
use micro_moka::unsync::Cache;

let mut cache = Cache::new(10_000);
cache.insert("key", "value");
assert_eq!(cache.get(&"key"), Some(&"value"));
```

## The Design Point

Cache design is a multi-objective problem: hit ratio, byte hit ratio, average
latency, tail latency, throughput, metadata density, expiration, weighting,
and concurrency all compete. Micro Moka deliberately targets one Pareto point:
single-owner caches that need small resident metadata and explicitly bounded
admission work.

- **Budgeted admission.** `try_insert` examines at most 16 SIEVE candidates by
  default. If all were recently visited, it returns the candidate instead of
  extending the sweep. The next attempt resumes at the saved hand. This bounds
  eviction-policy work, spreads a hot sweep across requests, and resists short
  one-hit scans.
- **Exact insertion remains available.** `insert` always admits a nonzero-
  capacity candidate and runs the original SIEVE sweep to completion. Choose
  exact admission when every new value must become resident.
- **Dense policy metadata.** The visited flag is packed into the nonzero
  predecessor link. On the supported targets, an `Option`-wrapped slab slot for
  `u64` keys and values is 32 bytes, down from 40 bytes in v1.1.0. This excludes
  hash-table storage and allocations owned by the key or value.
- **A small read path.** A hit performs one hash-table lookup and one visited-bit
  update. It never relinks an entry. A miss does not touch SIEVE metadata.
- **Operationally small.** The only production dependency is `hashbrown`.
  `RandomState` remains the HashDoS-resistant default; trusted-key workloads
  can opt into a faster hasher.

The benchmark suite measures hits, negative lookups, inserts, updates,
delete-and-reinsert churn, mixed workloads, request and byte hit ratios,
scan resistance, and full-cache insertion percentiles. See
[benchmark report](./docs/cache-positioning.md) for methodology, results, and limitations.

## Installation

```toml
[dependencies]
micro-moka = "1"
```

## Usage

```rust
use micro_moka::unsync::Cache;

let mut cache: Cache<&str, i32> = Cache::builder()
    .max_capacity(1_000)
    .initial_capacity(100)
    .admission_scan_limit(16)
    .build();

cache.insert("key", 42);
assert_eq!(cache.get(&"key"), Some(&42));
assert!(cache.contains_key(&"key"));

// Inspect without affecting SIEVE state.
assert_eq!(cache.peek(&"key"), Some(&42));

// Compute a missing value once using exact, always-admit insertion.
assert_eq!(cache.get_or_insert_with("other", || 7), &7);

// Bound eviction scanning and retain ownership if admission is deferred.
if let Err((key, value)) = cache.try_insert("candidate", 9) {
    assert_eq!((key, value), ("candidate", 9));
}

cache.invalidate(&"key");
let removed = cache.remove(&"other");
assert_eq!(removed, Some(7));

cache.insert("low", 1);
cache.insert("high", 100);
cache.invalidate_entries_if(|_key, value| *value < 10);

for (key, value) in cache.iter() {
    println!("{key}: {value}");
}

cache.invalidate_all();
assert_eq!(cache.entry_count(), 0);
```

### Choosing an Insertion Path

| Requirement | Method | Full, all-hot cache |
|---|---|---|
| Every candidate must become resident | `insert` | Sweeps until it finds a victim |
| Bound policy scan work | `try_insert` | Returns `Err((K, V))` after the configured budget |
| Load once and borrow the result | `get_or_insert_with` | Uses exact insertion |

`try_insert` always updates an existing key and always admits below capacity.
A scan limit of zero freezes admission of new keys while full. A larger limit
makes the policy approach exact SIEVE; a smaller limit protects hot residents
more aggressively and rejects more candidates. Rejection is observable through
the returned `Result`, so applications can count, retry, or bypass the cache
without enabling internal statistics.

### Custom Hashers

The default hasher is `RandomState`, the same HashDoS-resistant default used by
`std::collections::HashMap`. A faster hasher can materially improve throughput
when all keys are trusted:

```rust,ignore
use micro_moka::unsync::Cache;

let mut cache: Cache<String, String, ahash::RandomState> = Cache::builder()
    .max_capacity(10_000)
    .build_with_hasher(ahash::RandomState::default());
```

## How SIEVE and Budgeted Admission Work

Entries remain in insertion order. Each entry has one visited bit, and the cache
maintains a hand into the list:

1. A successful `get`, update, or `get_or_insert_with` hit marks the entry as
   visited without moving it.
2. Exact eviction clears visited bits while moving the hand backward and evicts
   the first unvisited entry.
3. Budgeted admission performs the same sweep for at most the configured number
   of entries.
4. If the budget expires, `try_insert` saves the hand and returns the candidate.
   Otherwise it evicts the unvisited victim and admits the candidate.

The exact sweep is amortized constant work, but a single insertion can inspect
the whole cache when every resident was visited. `try_insert` exists for callers
that care about that individual-operation tail.

`peek`, `contains_key`, and iteration do not set the visited bit.

## Ecosystem Fit

Micro Moka does not try to replace every Rust cache:

| Need | Better fit |
|---|---|
| Single-owner, entry-bounded, low-metadata cache with bounded admission work | Micro Moka `try_insert` |
| Single-owner cache where every candidate must be retained immediately | Micro Moka `insert` or [`lru`][lru-git] |
| Weighted capacity, S3-FIFO, pinning, or sync and unsync variants | [`quick_cache`][quick-cache-git] |
| TTL/TTI, per-entry expiry, listeners, async loading, or concurrent TinyLFU | [`moka`][moka-git] |
| Strict LRU with a broad map-like API | [`lru`][lru-git] or [`hashlink`][hashlink-git] |

Micro Moka intentionally omits weighted capacity and expiration. Those features
need more per-entry state, clock reads, cleanup machinery, or policy coupling and
would weaken this crate's latency-density position. Values may still be
`Option<T>` for negative caching, and callers can invalidate entries explicitly.

## Minimum Supported Rust Version

Micro Moka supports Rust 1.76.0 and uses the Rust 2021 edition. MSRV increases
are not considered semver-breaking changes.

## Development

```bash
cargo test --all-features
cargo clippy --lib --tests --all-features -- -D warnings
cargo fmt --all -- --check
cargo +nightly -Z unstable-options --config 'build.rustdocflags="--cfg docsrs"' doc --no-deps
cargo +nightly miri test --lib --all-features

# Benchmark dependencies are opt-in.
RUSTFLAGS='--cfg bench_deps' cargo bench --bench benchmark
```

## Releases

Merges to `main` are released automatically. See [RELEASING.md](./RELEASING.md).

## Credits

SIEVE was introduced by Zhang et al. at NSDI 2024. Micro Moka was forked from
[Mini Moka][mini-moka-git] by Tatsuya Kawano and the Moka contributors.

## License

MIT OR Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and
[LICENSE-APACHE](LICENSE-APACHE).

[gh-actions-badge]: https://github.com/user1303836/micro-moka/actions/workflows/CI.yml/badge.svg
[release-badge]: https://img.shields.io/crates/v/micro-moka.svg
[docs-badge]: https://docs.rs/micro-moka/badge.svg
[license-badge]: https://img.shields.io/crates/l/micro-moka.svg
[gh-actions]: https://github.com/user1303836/micro-moka/actions?query=workflow%3ACI
[crate]: https://crates.io/crates/micro-moka
[docs]: https://docs.rs/micro-moka
[mini-moka-git]: https://github.com/moka-rs/mini-moka
[sieve-paper]: https://www.usenix.org/conference/nsdi24/presentation/zhang-yazhuo
[quick-cache-git]: https://github.com/arthurprs/quick-cache
[moka-git]: https://github.com/moka-rs/moka
[lru-git]: https://github.com/jeromefroe/lru-rs
[hashlink-git]: https://github.com/djc/hashlink
