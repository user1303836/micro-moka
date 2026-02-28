# Micro Moka Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.14] - 2026-02-28

### Added

- Added `peek()` method to `Cache` that returns `Option<&V>` via a shared `&self` reference without updating the frequency sketch or deque position, enabling non-promoting cache reads that do not influence eviction decisions.

## [0.1.13] - 2026-02-28

### Changed

- Added `[profile.release]` with `lto = true` and `codegen-units = 1` to enable full link-time optimization and single codegen unit for release builds.
- Added `[profile.bench]` with the same LTO and codegen-units settings to ensure benchmarks are built with maximum optimization.

## [0.1.12] - 2026-02-28

### Changed

- Updated README to reflect slab-indexed architecture: single dependency (`hashbrown`), no unsafe code, removed stale Miri deque command.

## [0.1.11] - 2026-02-28

### Changed

- Replaced `Rc<K>`-wrapped key storage and pointer-based doubly-linked deque with a slab-based index architecture (`Vec<Option<SlabEntry<K, V>>>` + `HashTable<u32>`), eliminating all reference counting, all `Box<DeqNode>` heap allocations, and all unsafe pointer manipulation.
- Switched from `hashbrown::HashMap<Rc<K>, ValueEntry<K, V>>` to `hashbrown::HashTable<u32>` with slab indices, so keys live in a single contiguous location (the slab) with no duplication.
- Embedded deque prev/next links as `u32` indices directly in slab entries, replacing the unsafe `NonNull<DeqNode>` linked list with simple index arithmetic.
- Removed `triomphe` dependency (no longer needed without `Rc`/deque node allocations).
- Removed `hashbrown` features `raw-entry` and `equivalent` (no longer needed with `HashTable`).
- Improved `contains_key` to take `&self` instead of `&mut self`.

### Removed

- Removed `src/unsync/deques.rs` and `src/common/deque.rs` (replaced by inline `IndexDeque`).
- Removed all `Rc<K>`, `NonNull`, `DeqNode`, `KeyHashDate`, `ValueEntry`, and `EntryInfo` types.

## [0.1.10] - 2026-02-28

### Changed

- Deferred `evict_lru_entries()` behind a fast capacity check so it only runs when the cache is actually over capacity, eliminating unnecessary work on every sub-capacity insert during warmup and normal operation.
- Marked `evict_lru_entries()` as `#[cold] #[inline(never)]` since it now exclusively handles the rare over-capacity path.
- Removed redundant unconditional `evict_lru_entries()` calls from `invalidate()` and `remove()`, which can never cause the cache to exceed capacity.

## [0.1.9] - 2026-02-28

### Changed

- Rewrote README to reflect current project state and positioning as the fastest single-threaded cache in Rust.

## [0.1.8] - 2026-02-28

### Changed

- Added `#[inline]` to hot path public API functions (`get`, `insert`, `contains_key`, `invalidate`, `remove`) and core private helpers (`record_hit`, `has_enough_capacity`, `weights_to_evict`, `handle_update`) to enable cross-crate inlining for downstream users.
- Added `#[inline]` to frequency sketch hot path methods (`frequency`, `increment`, `index_of`, `increment_at`) for better inlining in the admission/eviction loop.
- Split `evict_lru_entries()` into a thin `#[inline]` early-return check and a `#[cold] #[inline(never)]` inner body (`do_evict_lru_entries`) so the common no-eviction path stays in the instruction cache.
- Marked `invalidate_all()` and `invalidate_entries_if()` as `#[cold] #[inline(never)]` to keep infrequent bulk operations out of the hot instruction cache.

## [0.1.7] - 2026-02-28

### Changed

- Replaced three-deque `Deques<K>` struct (window, probation, protected) with a single `Deque<KeyHashDate<K>>`, eliminating ~80 bytes of unused deque overhead per cache instance.
- Removed `CacheRegion` enum and all tagged-pointer (`TagNonNull`) dispatch from access-order operations, simplifying every cache hit/insert/evict path.
- Removed `#[repr(align(4))]` from `DeqNode` now that 2-bit tag encoding is no longer needed.
- Removed `tagptr` dependency.
- Eliminated redundant hash computations in cache operations by switching to `hashbrown::HashMap` and using its `raw_entry` API to compute each key's hash exactly once per `get()` or `insert()` call, instead of 2-3 times.

## [0.1.6] - 2026-02-27

### Fixed

- Fixed release publish automation to avoid crates.io API pre-checks that can return 403 under data-access policy enforcement.
- Made publish step idempotent by treating "already uploaded" responses from `cargo publish` as success.

## [0.1.5] - 2026-02-27

### Added

- Added automated release and publish workflows:
  - PR-time release readiness checks (version bump, changelog entry, `cargo publish --dry-run`).
  - Post-merge publish to crates.io, version tag creation, and GitHub release creation.

## [0.1.4] - 2026-02-27

### Changed

- Simplified unsync cache hot paths for weight=1 admission and read operations.
- Reduced dependency surface by removing `smallvec`.

### Fixed

- Replaced test-only unsafe `transmute` in frequency sketch tests with `u32::from_ne_bytes`.

## [0.1.3] - 2026-02-26

### Fixed

- Preserved `HashMap` capacity across `unsync::Cache::invalidate_all` without transiently doubling peak allocation by restoring capacity in a second phase after dropping the old map.
- Replaced a test-only unsafe `transmute` in `frequency_sketch` with `u32::from_ne_bytes`.

## [0.1.2] - 2026-02-25

### Fixed

- Fixed `unsync::Cache::remove` and `unsync::Cache::invalidate` so they decrement `entry_count`, preventing stale capacity tracking and incorrect admission/rejection after manual removals.
- Hardened deque membership checks in debug builds to catch wrong-deque / stale-node misuse around unsafe pointer operations, and added regression tests for the false-positive case.
- Made `unsync::Cache::invalidate_all` panic-safer by resetting internal state before dropping the old map, so a panicking `Drop` leaves the cache in a consistent empty state.

## [0.1.0] - 2025-11-29

### Added

- Initial release of **Micro Moka**, a lightweight, single-threaded cache library for Rust.
- Forked from [Mini Moka](https://github.com/moka-rs/mini-moka) v0.11.0.
- Retains the high-performance **W-TinyLFU** eviction policy (Window Tiny Least Frequently Used).
- Supports bounded capacity (maximum number of entries).

### Changed

- Renamed package to `micro-moka`.
- **Removed Concurrency:** `sync` module and `DashMap` dependency removed. Strictly single-threaded (`unsync` only).
- **Removed Weight Support:** All items have an implicit weight of 1. `Weigher` trait and logic removed.
- **Removed Expiration:** Time-to-live (TTL) and Time-to-idle (TTI) policies removed. `time` module removed.
- Updated documentation and examples to reflect the new lightweight nature.
