# Retained optimizations: results and limits

PR #40 keeps three small changes: borrowed-key loading, allocation-preserving clear, and constant-time iterator counting. It **does not establish general benchmark leadership**. More aggressive iterator changes were rejected after cross-platform regressions.

## What is retained

- `get_or_insert_with_ref(&Q, F)`: hashes once; hits neither allocate an owned key nor invoke the loader. Miss construction precedes eviction. Existing owned-loading and zero-capacity semantics remain unchanged.
- `invalidate_all()`: keeps table, slab and free-list buffers on normal completion. An inline empty check avoids unnecessary work. Destructor panics leave an empty, usable cache; slab storage may be released during unwinding.
- `Iter::count()`: returns the tracked remaining length. **Iterator representation, `next`, and traversal are unchanged.** Sparse traversal still scans vacant slots.

Production stays safe Rust with only hashbrown 0.16.1 and Rust 1.76 support. No policy changes, new production dependencies, or runtime feature switches.

## Paired comparisons

342 synthetic configurations: u64/String24/String128 keys, matched SipHash/aHash, capacities 128/1,024/16,384, and 19 operations. Comparators are published Micro Moka 1.2.0, quick_cache 0.7.0, lru 0.18.4, and hashlink 0.12.2. Values are u64; these are not representative real-world traces or large-value measurements.

Nine rotating-order samples, >=5 ms/sample except the evolving mixed trace, which uses exactly 256 measured batches of 8,192 operations after equal warm-up. Mixed means 95% lookups, not 95% hits. Clear/refill and iterator traversal are measured per complete cycle, not per resident. No allocation instrumentation runs in the timing process. The 1.2 loader baseline uses its native owned-key API; callers returning copied/cloned values could already compose `get` plus insertion to avoid hit-key construction. The new API offers direct reference-returning borrowed loading without that workaround.

**W/T/L** counts medians >5% faster / within 5% / >5% slower. The bands are descriptive, not statistical significance or equivalence. Results below use the retained production source at `f628e43`.

| Run | vs Micro Moka 1.2 | vs quick_cache | vs lru | vs hashlink | vs fastest peer |
|---|---|---|---|---|---|
| ARM/macOS | 54 / 284 / 4 | 149 / 58 / 135 | 179 / 59 / 104 | 164 / 63 / 115 | 86 / 76 / 180 |
| ARM repeat | 58 / 278 / 6 | 151 / 58 / 133 | 186 / 56 / 100 | 161 / 65 / 116 | 84 / 80 / 178 |
| x86-64/Linux CI | 64 / 273 / 5 | 121 / 65 / 156 | 162 / 50 / 130 | 142 / 59 / 141 | 76 / 89 / 177 |

The corresponding ARM no-change control has 331/342 comparisons within 5%, seven apparent wins and four apparent losses. Against the fastest peer, strict wins improve from 58 to 86 on ARM. Restricting to the original 252-case operation set, wins improve from 40 to 60; added density cases do not explain the gain. Nevertheless, Micro Moka still loses to the fastest peer in roughly half this matrix.

### Useful gains

- String loader-hit medians are **15–62% lower** than 1.2 on ARM, **7–52% lower** on the Linux host.
- For 10,000 String loader hits: **zero candidate allocations**, versus 10,000 in 1.2.
- Clearing the allocation probe's preallocated String cache: **zero new allocations and no backing-buffer frees**, versus two replacement allocations in 1.2. Key destruction remains necessary.
- Iterator counting becomes O(1), including after partial consumption; this does not make ordinary sparse traversal O(1).

### Losses and limitations

The two final ARM runs record four and six >5% baseline losses. The recurring one is u64/aHash loading under churn, about 6%; other losses vary between runs and include mixed traffic, lookup and String clear/refill cases. The Linux run records five losses, including **11–13% slower borrowed-loader hits for u64/SipHash**. The new borrowed API is not universally faster: use the unchanged owned loader when cheap owned/Copy keys are already available.

No large repeatable dense/partially vacant iteration regressions remain in these retained-version runs. This is not a zero-regression or portable speed guarantee. Full per-case tables, including every losing row, are linked below. CI machines are shared, compiler/allocator/code layout matters, and no p99 latency claim is made.

## Independent changes and interactions

`retained-factorial-*` replays every subset from v1.2 source, with five rotating samples and >=3 ms adaptive windows (fixed work for the mixed trace). Each variant passes debug/release model tests and benchmark adapter tests before timing. Bits are **borrowed / clear / count**.

| Bits | vs published 1.2 W/T/L | vs fastest peer W/T/L |
|---|---|---|
| 000 | 7 / 331 / 4 | 58 / 65 / 219 |
| 100 | 27 / 296 / 19 | 68 / 53 / 221 |
| 010 | 34 / 300 / 8 | 82 / 69 / 191 |
| 001 | 28 / 297 / 17 | 71 / 73 / 198 |
| 110 | 46 / 285 / 11 | 88 / 55 / 199 |
| 101 | 46 / 275 / 21 | 59 / 80 / 203 |
| 011 | 58 / 268 / 16 | 88 / 69 / 185 |
| 111 | 57 / 282 / 3 | 82 / 79 / 181 |

Some subset runs fluctuate substantially even on unchanged paths, especially the old allocating clear. Do not interpret these aggregate counts as additive causal effects. Allocation assertions and API/model tests establish the mechanisms; repeated paired timings bound the performance claims.

## Rejected changes and benchmark correction

Early-stop checks, sparse/dense dispatch and specialized folds were explored, then removed. They produced attractive isolated results but repeatable regressions elsewhere; chunked folding lost up to **74%** on one x86-64 CI host. Even empty-iterator shortcuts affected generated traversal loops. Keeping the three-line `count` override was preferable to retaining platform-sensitive tuning.

An adaptive-duration mixed trace initially compared different evolving resident sets. Warm-up alone did not fix that. The final harness uses equal measured operation counts and tests candidate/published-baseline state equality after equal-length trace prefixes. Earlier results remain archived and are not used for the final headline table.

## Correctness, footprint and reproduction

- 66 unit tests and 11 runnable doctests pass in debug/release; formatting and root/benchmark Clippy pass.
- Deterministic reference model: 1,050,000 mixed operations, colliding hashes, seven capacities and five admission budgets, plus table/slab/free-list/deque/hand/visited invariants. Miri uses a smaller corpus.
- CI passes Rust 1.76/stable/beta/nightly, full-library Miri, four Linux cross targets, allocation assertions and both dependency audits with warnings denied.
- A small representative application has the same **336,016-byte** stripped executable as baseline. Three alternating-order, clean offline builds have median times 2.15 s candidate / 2.34 s baseline. This small sample is not a general compile-speed claim.

```sh
python3 benches/run.py new-run
python3 benches/ablate.py new-factorial
python3 benches/footprint.py new-footprint
cargo run --release --locked --manifest-path benches/Cargo.toml --bin allocations
```

Evidence:

- [ARM primary](../benches/results/retained-final.md), [repeat](../benches/results/retained-repeat.md), [Linux](../benches/results/linux-35608797035.md). Adjacent CSV/JSON files contain raw samples, settings and source hashes.
- [Allocation counts](../benches/results/allocations-retained.csv), [build/size probe](../benches/results/retained-footprint.json).
- Factorial source identities, per-file hashes and test logs: `benches/results/retained-factorial-*`.
- [Experiment notebook and rejected variants](benchmark-optimizations.md).
- [Matching-source Linux benchmark CI](https://github.com/user1303836/micro-moka/actions/runs/35608797035).
