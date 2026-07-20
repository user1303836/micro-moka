# Micro Moka

[![GitHub Actions][gh-actions-badge]][gh-actions]
[![crates.io release][release-badge]][crate]
[![docs][docs-badge]][docs]
[![license][license-badge]](#license)

A fast, lightweight, single-threaded cache for Rust. Micro Moka is forked from
[Mini Moka][mini-moka-git] and intentionally omits async support, synchronization,
weighted eviction, and expiration. It provides a bounded cache using the
[SIEVE][sieve-paper] eviction policy.

```rust
use micro_moka::unsync::Cache;

let mut cache = Cache::new(10_000);
cache.insert("key", "value");
assert_eq!(cache.get(&"key"), Some(&"value"));
```

## Why Micro Moka?

**Efficient eviction.** SIEVE gives accessed entries a second chance using one
visited bit per entry and a persistent hand. Cache hits do not reorder a recency
list, keeping the read path small.

**Minimal overhead.** The only production dependency is `hashbrown`. There is no
async runtime, lock, atomic, custom allocator, or unsafe code in Micro Moka.

**Single-threaded by design.** If a cache belongs to one thread, such as in CLI
tools, WASM applications, game loops, compilers, or single-threaded servers, it
does not need to pay synchronization costs.

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
    .build();

cache.insert("key", 42);
assert_eq!(cache.get(&"key"), Some(&42));
assert!(cache.contains_key(&"key"));

// Inspect without affecting SIEVE eviction state.
assert_eq!(cache.peek(&"key"), Some(&42));

// Compute a missing value once.
assert_eq!(cache.get_or_insert_with("other", || 7), &7);

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

### Custom Hashers

The default hasher is `RandomState`, the same HashDoS-resistant default used by
`std::collections::HashMap`. A faster hasher can improve throughput when all keys
are trusted:

```rust,ignore
use micro_moka::unsync::Cache;

let mut cache: Cache<String, String, ahash::RandomState> = Cache::builder()
    .max_capacity(10_000)
    .build_with_hasher(ahash::RandomState::default());
```

## How SIEVE Works

Entries remain in insertion order. Each entry has one visited bit, and the cache
maintains a hand into the entry list:

1. A successful `get` marks the entry as visited without moving it.
2. When space is needed, the hand scans entries and clears visited bits.
3. The first unvisited entry is evicted.
4. The new entry is admitted unconditionally.

The hand persists between evictions, so the work of scanning visited entries is
amortized while cache hits remain constant-time table lookups plus one bit write.
`peek`, `contains_key`, and iteration do not set the visited bit.

## What Was Removed from Mini Moka

| Feature | Rationale |
|---------|-----------|
| Concurrent cache | Eliminates locks and atomics |
| Async support | Avoids a runtime dependency |
| Weight-based eviction | Every entry occupies one slot |
| TTL and TTI expiration | Avoids timer and maintenance overhead |
| W-TinyLFU admission | SIEVE uses a one-bit second-chance policy |

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

SIEVE was introduced by Yang et al. at NSDI 2024. Micro Moka was forked from
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
