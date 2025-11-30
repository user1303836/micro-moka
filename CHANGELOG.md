# Micro Moka Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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