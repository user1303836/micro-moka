# Micro Moka

[![GitHub Actions][gh-actions-badge]][gh-actions]
[![crates.io release][release-badge]][crate]
[![docs][docs-badge]][docs]
[![license][license-badge]](#license)

A fast, lightweight, single-threaded cache for Rust. Forked from [Mini Moka][mini-moka-git] and stripped down to the essentials: no async, no concurrency overhead, no weight-based eviction, no TTL. Just a bounded cache with the [W-TinyLFU][tiny-lfu] eviction policy from [Caffeine][caffeine-git] -- the same algorithm that powers some of the highest hit-ratio caches in production.

The goal is simple: **be the fastest single-threaded cache in Rust**.

```rust
use micro_moka::unsync::Cache;

let mut cache = Cache::new(10_000);
cache.insert("key", "value");
assert_eq!(cache.get(&"key"), Some(&"value"));
```

## Why Micro Moka?

**W-TinyLFU is a better eviction policy.** Standard LRU caches are vulnerable to scan pollution -- a single sequential scan can flush the entire cache. W-TinyLFU uses a frequency sketch (Count-Min Sketch with 4-bit counters) to admit only entries that are likely to be accessed again, producing significantly higher hit ratios on real-world workloads compared to LRU, FIFO, or CLOCK.

**Minimal overhead.** One production dependency (`hashbrown`). No allocator, no async runtime, no parking lot, no unsafe code. Compiles fast, produces small binaries.

**Single-threaded by design.** No `Arc`, no `Mutex`, no atomic operations. If your cache lives on one thread -- CLI tools, WASM, game loops, compilers, single-threaded servers -- you pay zero synchronization cost.

## Installation

```toml
[dependencies]
micro-moka = "0.1"
```

## Usage

```rust
use micro_moka::unsync::Cache;

// Create with capacity
let mut cache: Cache<&str, i32> = Cache::new(1_000);

// Or use the builder
let mut cache: Cache<&str, i32> = Cache::builder()
    .max_capacity(1_000)
    .initial_capacity(100)
    .build();

// Insert and retrieve
cache.insert("key", 42);
assert_eq!(cache.get(&"key"), Some(&42));

// Check membership
assert!(cache.contains_key(&"key"));

// Remove entries
cache.invalidate(&"key");
let removed = cache.remove(&"other_key"); // returns Option<V>

// Bulk invalidation
cache.invalidate_entries_if(|_key, value| *value < 10);
cache.invalidate_all();

// Inspection
let count = cache.entry_count();
let policy = cache.policy();
println!("max capacity: {:?}", policy.max_capacity());

// Iteration (does not update access order)
for (key, value) in cache.iter() {
    println!("{}: {}", key, value);
}
```

### Custom Hashers

The default hasher is `RandomState` (SipHash). For cache-heavy workloads, a faster hasher like [aHash][ahash] can improve throughput substantially:

```rust,ignore
use micro_moka::unsync::Cache;

let mut cache: Cache<String, String, ahash::RandomState> = Cache::builder()
    .max_capacity(10_000)
    .build_with_hasher(ahash::RandomState::default());
```

## How W-TinyLFU Works

On every cache access, a probabilistic frequency counter (Count-Min Sketch) records how often keys are requested. When the cache is full and a new entry arrives:

1. The **candidate** (new entry) is compared against **victims** (least-recently-used entries from the eviction queue)
2. If the candidate's estimated frequency exceeds the victims', it is admitted and the victims are evicted
3. If not, the candidate is rejected -- preventing low-frequency entries from displacing popular ones

The frequency sketch uses 4-bit counters with periodic aging (halved when a sample threshold is reached), keeping memory usage bounded regardless of the key universe size. The sketch is only enabled once the cache reaches 50% capacity, avoiding overhead during warmup.

## Benchmarks

Measured with [criterion](https://crates.io/crates/criterion) 0.5 (LTO, 100 samples, fixed seed 42).
Source: `benches/` directory. Reproduce with `cargo bench`.

### Throughput (Mops/sec, higher is better)

Zipf s=1.0, capacity=1,000, key space=10,000:

| Operation | micro-moka | quick-cache | lru | hashlink | mini-moka | HashMap\* |
|-----------|-----------|-------------|-----|----------|-----------|----------|
| get | 52.2 | 685.2 | 429.8 | 357.7 | 33.8 | 242.2 |
| insert | 51.5 | 167.4 | 156.2 | 158.2 | 21.2 | 215.8 |
| mixed 95/5 | 56.9 | 534.2 | 377.9 | 389.6 | 32.4 | 210.2 |
| mixed 50/50 | 63.1 | 289.9 | 244.9 | 251.6 | 26.0 | 216.6 |

\*HashMap is unbounded (no eviction) -- shown as theoretical ceiling.

**Hasher note:** These results compare default configurations. micro-moka and HashMap use SipHash (DoS-resistant, slower); quick-cache uses ahash; lru and hashlink use foldhash. Using a [faster hasher](#custom-hashers) with micro-moka will significantly improve throughput. micro-moka is 1.5-2.4x faster than mini-moka across all workloads.

### Hit Ratio (%, higher is better)

1M operations, capacity=1,000, key space=10,000:

| Distribution | micro-moka | quick-cache | lru | hashlink | mini-moka |
|---|---|---|---|---|---|
| Zipf s=0.7 | 42.5 | 43.8 | 32.9 | 32.9 | 42.5 |
| Zipf s=0.9 | 63.9 | 64.6 | 55.6 | 55.6 | 63.6 |
| Zipf s=1.0 | 74.2 | 74.5 | 67.5 | 67.5 | 74.2 |
| Zipf s=1.2 | 89.3 | 89.4 | 86.1 | 86.1 | 89.3 |
| Uniform | 10.0 | 10.0 | 10.0 | 10.0 | 10.0 |

W-TinyLFU caches (micro-moka, quick-cache, mini-moka) achieve 7-10% higher hit ratios than LRU caches on skewed workloads. Under uniform access, all policies converge to the same hit ratio (capacity / key space).

## What Was Removed from Mini Moka

| Feature | Rationale |
|---------|-----------|
| `sync` module (concurrent cache) | Eliminates `DashMap`, locks, atomics |
| Async support | No runtime dependency |
| Weight-based eviction (`Weigher`) | All entries cost 1 slot; simpler admission |
| TTL / TTI expiration | No timer overhead; entries live until evicted or invalidated |
| `smallvec`, `tagptr`, `triomphe` dependencies | Simplified internals |

## Minimum Supported Rust Version

| Feature | MSRV |
|---------|------|
| Default | Rust 1.76.0 (Feb 8, 2024) |

MSRV follows a rolling 6-month policy. Bumping MSRV is not considered a semver-breaking change.

## Development

```bash
# All tests (including compile-fail and doc tests)
RUSTFLAGS='--cfg trybuild' cargo test --all-features

# Clippy (CI treats warnings as errors)
cargo clippy --lib --tests --all-features --all-targets -- -D warnings

# Format
cargo fmt --all -- --check

# Docs (nightly)
cargo +nightly -Z unstable-options --config 'build.rustdocflags="--cfg docsrs"' doc --no-deps

# Miri
cargo +nightly miri test
```

## Releases

Automated on merge to `main`. See [RELEASING.md](./RELEASING.md) for details.

## Credits

Architecture inspired by [Caffeine][caffeine-git] for Java. Thanks to Ben Manes and all Caffeine contributors.

Forked from [Mini Moka][mini-moka-git] by Tatsuya Kawano and the Moka contributors.

## License

MIT OR Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).

[gh-actions-badge]: https://github.com/user1303836/micro-moka/actions/workflows/CI.yml/badge.svg
[release-badge]: https://img.shields.io/crates/v/micro-moka.svg
[docs-badge]: https://docs.rs/micro-moka/badge.svg
[license-badge]: https://img.shields.io/crates/l/micro-moka.svg
[gh-actions]: https://github.com/user1303836/micro-moka/actions?query=workflow%3ACI
[crate]: https://crates.io/crates/micro-moka
[docs]: https://docs.rs/micro-moka
[mini-moka-git]: https://github.com/moka-rs/mini-moka
[caffeine-git]: https://github.com/ben-manes/caffeine
[tiny-lfu]: https://github.com/moka-rs/moka/wiki#admission-and-eviction-policies
[ahash]: https://crates.io/crates/ahash
