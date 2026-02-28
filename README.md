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

**Minimal overhead.** Two production dependencies (`hashbrown`, `triomphe`). No allocator, no async runtime, no parking lot. Compiles fast, produces small binaries.

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

## What Was Removed from Mini Moka

| Feature | Rationale |
|---------|-----------|
| `sync` module (concurrent cache) | Eliminates `DashMap`, locks, atomics |
| Async support | No runtime dependency |
| Weight-based eviction (`Weigher`) | All entries cost 1 slot; simpler admission |
| TTL / TTI expiration | No timer overhead; entries live until evicted or invalidated |
| `smallvec`, `tagptr` dependencies | Simplified internals |

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

# Miri (verify unsafe deque code)
cargo +nightly miri test deque
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
