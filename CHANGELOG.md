# Micro Moka Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
