#![warn(clippy::all)]
#![warn(rust_2018_idioms)]
#![deny(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_cfg))]

//! Micro Moka is a lightweight cache library for Rust. Micro Moka is a fork
//! of [Mini Moka][mini-moka-git], stripped down to the bare essentials.
//!
//! Micro Moka provides an in-memory, non-thread-safe cache implementation for
//! single thread applications.
//!
//! All cache implementations perform a best-effort bounding of the map using an
//! entry replacement algorithm to determine which entries to evict when the capacity
//! is exceeded.
//!
//! [moka-git]: https://github.com/moka-rs/moka
//! [mini-moka-git]: https://github.com/moka-rs/mini-moka
//! [caffeine-git]: https://github.com/ben-manes/caffeine
//!
//! # Features
//!
//! - A cache can be bounded by the maximum number of entries.
//! - Maintains good hit rate by using entry replacement algorithms inspired by
//!   [Caffeine][caffeine-git]:
//!     - Admission to a cache is controlled by the Least Frequently Used (LFU) policy.
//!     - Eviction from a cache is controlled by the Least Recently Used (LRU) policy.
//!
//! # Examples
//!
//! See the following document:
//!
//! - A not thread-safe, blocking cache for single threaded applications:
//!     - [`unsync::Cache`][unsync-cache-struct]
//!
//! [unsync-cache-struct]: ./unsync/struct.Cache.html
//!
//! # Minimum Supported Rust Versions
//!
//! This crate's minimum supported Rust versions (MSRV) are the followings:
//!
//! | Feature          | MSRV                       |
//! |:-----------------|:--------------------------:|
//! | default features | Rust 1.76.0 (Feb 8, 2024) |
//!
//! If only the default features are enabled, MSRV will be updated conservatively.
//! When using other features, MSRV might be updated more frequently, up to the
//! latest stable. In both cases, increasing MSRV is _not_ considered a
//! semver-breaking change.

pub(crate) mod common;
pub(crate) mod policy;
pub mod unsync;

pub use policy::Policy;

#[cfg(doctest)]
mod doctests {
    // https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html#include-items-only-when-collecting-doctests
    #[doc = include_str!("../README.md")]
    struct ReadMeDoctests;
}
