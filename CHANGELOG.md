# Micro Moka Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.2.0] - 2026-07-20

### Added

- Added `Cache::try_insert`, a scan-budgeted admission path that returns a candidate when a full cache has no unvisited SIEVE victim within the configured budget.
- Added `CacheBuilder::admission_scan_limit` and `Policy::admission_scan_limit`; the default budget is 16 inspected entries and a zero budget freezes new-key admission while full.
- Added benchmark coverage for negative lookups, updates, delete/reinsert churn, request and synthetic byte hit ratios, short-scan resistance, admission rejection counts, and full-cache insertion percentiles.
- Added `docs/cache-positioning.md` with ecosystem research, methodology, representative results, limitations, and the rationale for Micro Moka's latency-density positioning.

### Changed

- Packed the SIEVE visited bit into a nonzero predecessor link, reducing the `u64` key/value slab slot from 40 to 32 bytes while preserving safe Rust and the vacant-slot niche.
- Reworked the README and crate documentation around exact versus scan-budgeted admission and documented where feature-rich Rust caches are a better fit.
- Added stable CI compilation and lint coverage for the opt-in benchmark target.

## [1.1.0] - 2026-07-20

### Added

- Made cache iterators exact-size and fused, providing accurate `size_hint()` and `len()` values even when the slab contains vacant slots.

### Changed

- Removed the unnecessary `Clone` requirement from custom cache hashers.
- Updated the crate to the Rust 2021 edition while retaining Rust 1.76.0 support.
- Removed the unused, test-only W-TinyLFU frequency sketch, its Kani workflow, empty legacy test harnesses, four unused development dependencies, and stale package assets.
- Updated the README and API documentation to describe the current SIEVE implementation and stable `1.x` installation.
- Modernized GitHub Actions and repaired the MSRV, lint, cross-compilation, Miri, and audit workflows.
- Updated all benchmark dependencies to their current releases.

### Fixed

- Kept cache accounting and eviction state consistent if an entry's destructor panics during invalidation, predicate invalidation, or eviction.
- Preserved both hash table and slab allocation capacity across `invalidate_all()`.
- Removed duplicate slab lookups from successful `get()` and `get_or_insert_with()` operations.

## [1.0.0] - 2026-04-07

### Changed

- Promoted to stable 1.0.0 release. The public API is now considered stable and will follow semantic versioning guarantees going forward.

## [0.1.22] - 2026-03-01

### Added

- Added `get_or_insert_with()` API that combines cache lookup and insertion with one hash computation and one fewer lookup than separate `get()` and `insert()` calls. The closure is only called on cache miss. On hit, the entry is marked as visited for SIEVE eviction.

## [0.1.21] - 2026-03-01

### Changed

- Replaced separate `find()` + `insert_unique()` double-probe on insert with a single `HashTable::entry()` call that walks the probe sequence once, eliminating a redundant slot-finding traversal on new-key inserts. Eviction is moved before the table probe so the `VacantEntry` borrow does not conflict with eviction's mutable access.

## [0.1.20] - 2026-03-01

### Changed

- Replaced W-TinyLFU eviction policy with SIEVE (NSDI 2024). The `get()` hot path now sets a single `visited = true` bit instead of performing a frequency sketch increment (~20-40 cycles for hash derivations, table lookups, conditional counter increments, and reset checks) and a deque move_to_back (~15-20 cycles for pointer reads/writes). On eviction, a hand pointer sweeps backward through the deque clearing visited bits until it finds an unvisited entry to evict. This eliminates the `FrequencySketch` from the get/insert paths entirely, removes all deque reordering on cache hits, and removes frequency-based admission decisions. New entries are always admitted.

### Removed

- Removed `FrequencySketch` (Count-Min Sketch) from the cache hot path. The module is retained for its own unit tests but is no longer used by the cache.
- Removed `sketch_capacity()` helper, `frequency_sketch_enabled` field, `should_enable_frequency_sketch()`, `enable_frequency_sketch()`, `do_enable_frequency_sketch()`, `admit()`, `AdmissionResult`, `remove_by_index()`, `evict_lru_entries()`, and `EVICTION_BATCH_SIZE`.

## [0.1.19] - 2026-03-01

### Changed

- Reduced FrequencySketch (Count-Min Sketch) depth from 4 to 2, halving the number of hash derivations and table accesses per `increment()` and `frequency()` call. Depth=2 provides ~75% confidence (vs 93.75% at depth=4), which is sufficient for cache admission decisions. Benchmarks show 7-18% throughput improvement with negligible hit rate impact (within 0.3pp of depth=4 across all distributions).

### Fixed

- Fixed `reset()` size correction formula to use `count >> 1` (divide by 2) instead of `count >> 2` (divide by 4), matching the depth=2 counter count per increment. The old formula underestimated the correction, causing `size` to remain inflated after reset and triggering aging cycles more frequently than intended.

## [0.1.18] - 2026-03-01

### Changed

- Moved frequency sketch increment in `get()` to after the hash table hit, so cache misses skip the ~40-cycle frequency counter update entirely. Added frequency sketch increment to `insert()` so that repeated insert attempts for uncached keys still build admission frequency, preserving W-TinyLFU admission quality.

## [0.1.17] - 2026-03-01

### Changed

- Moved W-TinyLFU admission check before slab allocation and hash table insertion so that rejected candidates return immediately with zero wasted work, eliminating a slab allocation, hash table insert, hash table remove, and slab deallocation per rejection.

## [0.1.16] - 2026-03-01

### Added

- Added benchmark suite comparing micro-moka against quick-cache, lru, hashlink, mini-moka, and std HashMap.
- Criterion throughput benchmarks across Zipf (s=1.0, s=0.7), uniform, and mixed (95/5, 50/50) workloads at cache sizes 100, 1,000, and 10,000.
- Standalone hit-ratio benchmark (1M ops, Zipf s=0.7/0.9/1.0/1.2, uniform).
- README updated with benchmark results tables.

## [0.1.15] - 2026-02-28

- Added test verifying `contains_key()` works with shared (`&self`) references, confirming the read-only signature enables callers to hold shared borrows while checking key existence.

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
