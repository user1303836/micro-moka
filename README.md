# Micro Moka

[![GitHub Actions][gh-actions-badge]][gh-actions]
[![crates.io release][release-badge]][crate]
[![docs][docs-badge]][docs]

<!-- [![coverage status][coveralls-badge]][coveralls] -->

[![license][license-badge]](#license)

Micro Moka is a lightweight, single-threaded cache library for Rust. It is a specialized fork of [Mini Moka][mini-moka-git], engineered for use cases where binary size, compile times, and simplicity are paramount, while maintaining the high hit-ratio of the W-TinyLFU eviction policy.

Micro Moka provides a non-thread-safe cache implementation for single thread applications. All caches perform a best-effort bounding of a hash map using an entry replacement algorithm to determine which entries to evict when the capacity is exceeded.

[gh-actions-badge]: https://github.com/user1303836/micro-moka/actions/workflows/CI.yml/badge.svg
[release-badge]: https://img.shields.io/crates/v/micro-moka.svg
[docs-badge]: https://docs.rs/micro-moka/badge.svg
[license-badge]: https://img.shields.io/crates/l/micro-moka.svg

<!-- [fossa-badge]: https://app.fossa.com/api/projects/git%2Bgithub.com%2Fmoka-rs%2Fmini-moka.svg?type=shield -->

[gh-actions]: https://github.com/user1303836/micro-moka/actions?query=workflow%3ACI
[crate]: https://crates.io/crates/micro-moka
[docs]: https://docs.rs/micro-moka
[deps-rs]: https://deps.rs/repo/github/user1303836/micro-moka
[moka-git]: https://github.com/moka-rs/moka
[mini-moka-git]: https://github.com/moka-rs/mini-moka
[caffeine-git]: https://github.com/ben-manes/caffeine

## Key Features

- **Minimal Footprint:** Stripped of all async, concurrent, and heavy logic. Ideal for CLIs, WASM, and environments where binary size matters.
- **Tiny Dependency Tree:** Minimal dependencies (`smallvec`, `tagptr`, `triomphe`). No `parking_lot` or async runtimes.
- **Smart Eviction:** Uses W-TinyLFU (LFU admission + LRU eviction) to maintain a near-optimal hit ratio, significantly outperforming standard LRU caches.
- **Bounded Capacity:** Caches are strictly bounded by a maximum number of entries.

<!--
Mini Moka provides a rich and flexible feature set while maintaining high hit ratio
and a high level of concurrency for concurrent access. However, it may not be as fast
as other caches, especially those that focus on much smaller feature sets.

If you do not need features like: time to live, and size aware eviction, you may want
to take a look at the [Quick Cache][quick-cache] crate.
-->

[tiny-lfu]: https://github.com/moka-rs/moka/wiki#admission-and-eviction-policies

<!-- [quick-cache]: https://crates.io/crates/quick_cache -->

## Change Log

- [CHANGELOG.md](./CHANGELOG.md)

## Table of Contents

- [Micro Moka](#micro-moka)
  - [Features](#features)
  - [Change Log](#change-log)
  - [Table of Contents](#table-of-contents)
  - [Usage](#usage)
  - [Example: Basic Usage](#example-basic-usage)
  - [Minimum Supported Rust Versions](#minimum-supported-rust-versions)
  - [Developing Micro Moka](#developing-micro-moka)
  - [Releasing](#releasing)
  - [Credits](#credits)
    - [Caffeine](#caffeine)
  - [License](#license)

## Usage

Add this to your `Cargo.toml`:

```toml
[dependencies]
micro_moka = "0.1"
```

## Example: Basic Usage

Cache entries are manually added using `insert` method, and are stored in the cache
until either evicted or manually invalidated.

```rust
use micro_moka::unsync::Cache;

fn main() {
    // Create a cache that can store up to 10,000 entries.
    let mut cache = Cache::new(10_000);

    // Insert an entry.
    cache.insert("my_key", "my_value");

    // Get the entry.
    // get() returns Option<&V>, a reference to the stored value.
    if let Some(value) = cache.get(&"my_key") {
        println!("value: {}", value);
    }

    // Invalidate the entry.
    cache.invalidate(&"my_key");
}
```

## Minimum Supported Rust Versions

Micro Moka's minimum supported Rust versions (MSRV) are the followings:

| Feature          |           MSRV            |
| :--------------- | :-----------------------: |
| default features | Rust 1.76.0 (Feb 8, 2024) |

It will keep a rolling MSRV policy of at least 6 months. If only the default features
are enabled, MSRV will be updated conservatively. When using other features, MSRV
might be updated more frequently, up to the latest stable. In both cases, increasing
MSRV is _not_ considered a semver-breaking change.

## Developing Micro Moka

**Running All Tests**

To run all tests including doc tests on the README, use the following command:

```console
$ RUSTFLAGS='--cfg trybuild' cargo test --all-features
```

**Generating the Doc**

```console
$ cargo +nightly -Z unstable-options --config 'build.rustdocflags="--cfg docsrs"' \
    doc --no-deps
```

## Releasing

Releases are automated from merges into `main`.

- See [RELEASING.md](./RELEASING.md) for one-time setup.
- Every PR to `main` must bump `Cargo.toml` version and add the matching changelog section.
- On merge, GitHub Actions publishes to crates.io, creates `v<version>` tag, and creates a GitHub release.

## Credits

### Caffeine

Micro Moka's architecture is heavily inspired by the [Caffeine][caffeine-git] library
for Java. Thanks go to Ben Manes and all contributors of Caffeine.

## License

Micro Moka is distributed under either of

- The MIT license
- The Apache License (Version 2.0)

at your option.

See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE) for details.
