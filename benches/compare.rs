mod support;

use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::hint::black_box;
use std::time::{Duration, Instant};
use support::*;

const CASES: &[&str] = &[
    "get-hit",
    "get-miss",
    "update",
    "insert-cycle",
    "mixed-95",
    "load-hit",
    "load-cycle",
    "clear-refill",
    "clear-empty",
    "iter-dense",
    "iter-dense-for",
    "iter-density-1pct",
    "iter-density-6pct",
    "iter-density-quarter",
    "iter-density-half",
    "iter-front",
    "iter-back",
    "iter-empty",
    "iter-count",
];

struct Settings {
    samples: usize,
    duration: Duration,
    capacities: Vec<usize>,
    filter: String,
    libraries: String,
}

fn main() {
    let settings = Settings {
        samples: env("MM_SAMPLES", "9").parse().unwrap(),
        duration: Duration::from_millis(env("MM_MILLIS", "5").parse().unwrap()),
        capacities: env("MM_CAPACITIES", "128,1024,16384")
            .split(',')
            .map(|n| n.parse().unwrap())
            .collect(),
        filter: env("MM_FILTER", ""),
        libraries: env("MM_LIBS", "micro,baseline,quick_cache,lru,hashlink"),
    };
    assert!(settings.samples > 0 && !settings.duration.is_zero());
    assert!(settings.capacities.iter().all(|&c| c > 0));
    assert!(
        CASES.iter().any(|case| case.contains(&settings.filter)),
        "filter matches no workload"
    );
    assert!(
        settings.libraries.split(',').all(|name| [
            "micro",
            "baseline",
            "quick_cache",
            "lru",
            "hashlink"
        ]
        .contains(&name)),
        "unknown library"
    );
    println!("key,hasher,capacity,operation,library,sample,ns_per_op");
    run::<u64, _>("u64", 0, "sip", RandomState::new(), &settings);
    run::<u64, _>(
        "u64",
        0,
        "ahash",
        ahash::RandomState::with_seeds(1, 2, 3, 4),
        &settings,
    );
    for width in [24, 128] {
        let name = format!("string{width}");
        run::<String, _>(&name, width, "sip", RandomState::new(), &settings);
        run::<String, _>(
            &name,
            width,
            "ahash",
            ahash::RandomState::with_seeds(1, 2, 3, 4),
            &settings,
        );
    }
}

fn env(name: &str, default: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| default.into())
}

fn run<K: Key, S: BuildHasher + Clone>(
    name: &str,
    width: usize,
    hasher_name: &str,
    hasher: S,
    cfg: &Settings,
) {
    for &capacity in &cfg.capacities {
        let keys: Vec<K> = (0..capacity * 2).map(|i| K::make(i, width)).collect();
        let indices: Vec<usize> = (0..8192u64)
            .map(|i| {
                let mut x = i.wrapping_add(0x9e3779b97f4a7c15);
                x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
                x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
                ((x ^ (x >> 31)) as usize) % capacity
            })
            .collect();
        for &case in CASES {
            if !cfg.filter.is_empty() && !case.contains(&cfg.filter) {
                continue;
            }
            for sample in 0..cfg.samples {
                for offset in 0..5 {
                    // Rotate measured library order so one implementation is not always first.
                    let which = (sample + offset) % 5;
                    macro_rules! measure {
                        ($adapter:ident) => {{
                            let library = <$adapter<K, S> as Cache<K, S>>::NAME;
                            if cfg.libraries.split(',').any(|wanted| wanted == library) {
                                let ns = bench::<K, S, $adapter<K, S>>(capacity, &keys, &indices, case, hasher.clone(), cfg.duration);
                                println!("{name},{hasher_name},{capacity},{case},{library},{sample},{ns:.6}");
                            }
                        }};
                    }
                    match which {
                        0 => measure!(Current),
                        1 => measure!(Baseline),
                        2 => measure!(Quick),
                        3 => measure!(Lru),
                        _ => measure!(Hashlink),
                    }
                }
            }
        }
    }
}

#[inline(never)]
fn timed(mut batch: impl FnMut(), operations: usize, duration: Duration) -> f64 {
    batch();
    let start = Instant::now();
    let mut batches = 0u64;
    loop {
        batch();
        batches += 1;
        let elapsed = start.elapsed();
        if elapsed >= duration {
            return elapsed.as_secs_f64() * 1e9 / (batches as f64 * operations as f64);
        }
    }
}

#[inline]
fn mixed_batch<K: Key, S: BuildHasher, C: Cache<K, S>>(c: &mut C, keys: &[K], indices: &[usize]) {
    let capacity = keys.len() / 2;
    for (op, &i) in indices.iter().enumerate() {
        if op % 20 == 0 {
            c.insert(black_box(&keys[capacity + i]).clone(), 7);
        } else {
            black_box(c.get(black_box(&keys[i])));
        }
    }
}

fn bench<K: Key, S: BuildHasher, C: Cache<K, S>>(
    capacity: usize,
    keys: &[K],
    indices: &[usize],
    case: &str,
    hasher: S,
    duration: Duration,
) -> f64 {
    let mut c = C::new(capacity, hasher);
    for (i, key) in keys[..capacity].iter().enumerate() {
        c.insert(key.clone(), i as u64);
    }
    assert_eq!(c.count(), capacity);
    let mut expected_count = capacity;
    let mut expected_sum = (0..capacity as u64).sum::<u64>();
    if case.starts_with("iter-") && !case.starts_with("iter-dense") {
        for (i, key) in keys[..capacity].iter().enumerate() {
            let keep = match case {
                "iter-front" | "iter-count" => i == 0,
                "iter-back" => i == capacity - 1,
                "iter-density-1pct" => i % 100 == 0,
                "iter-density-6pct" => i % 16 == 0,
                "iter-density-quarter" => i % 4 == 0,
                "iter-density-half" => i % 2 == 0,
                _ => false,
            };
            if !keep {
                c.remove(key);
                expected_count -= 1;
                expected_sum -= i as u64;
            }
        }
    }
    if case.starts_with("iter-") {
        assert_eq!(c.count(), expected_count);
        assert_eq!(c.sum(), expected_sum);
        assert_eq!(c.sum_for(), expected_sum);
    }
    if case == "mixed-95" {
        for _ in 0..64 {
            mixed_batch::<K, S, C>(&mut c, keys, indices);
        }
    }
    if case == "clear-empty" {
        c.clear();
    }
    let result = match case {
        "get-hit" => timed(
            || {
                for &i in indices {
                    black_box(c.get(black_box(&keys[i])));
                }
            },
            indices.len(),
            duration,
        ),
        "get-miss" => timed(
            || {
                for &i in indices {
                    black_box(c.get(black_box(&keys[capacity + i])));
                }
            },
            indices.len(),
            duration,
        ),
        "update" => timed(
            || {
                for &i in indices {
                    c.insert(black_box(&keys[i]).clone(), i as u64);
                }
            },
            indices.len(),
            duration,
        ),
        "insert-cycle" => timed(
            || {
                for key in keys {
                    c.insert(black_box(key).clone(), 7);
                }
            },
            keys.len(),
            duration,
        ),
        "mixed-95" => timed(
            || mixed_batch::<K, S, C>(&mut c, keys, indices),
            indices.len(),
            duration,
        ),
        "load-hit" => timed(
            || {
                for &i in indices {
                    black_box(c.load(black_box(&keys[i])));
                }
            },
            indices.len(),
            duration,
        ),
        "load-cycle" => timed(
            || {
                for key in keys {
                    black_box(c.load(black_box(key)));
                }
            },
            keys.len(),
            duration,
        ),
        "clear-refill" => timed(
            || {
                c.clear();
                for (i, key) in keys[..capacity].iter().enumerate() {
                    c.insert(black_box(key).clone(), i as u64);
                }
            },
            1,
            duration,
        ),
        "clear-empty" => timed(
            || {
                for _ in 0..256 {
                    black_box(&mut c).clear();
                }
            },
            256,
            duration,
        ),
        "iter-dense-for" => timed(
            || {
                for _ in 0..32 {
                    black_box(black_box(&c).sum_for());
                }
            },
            32,
            duration,
        ),
        "iter-count" => timed(
            || {
                for _ in 0..256 {
                    black_box(black_box(&c).count());
                }
            },
            256,
            duration,
        ),
        _ => timed(
            || {
                for _ in 0..32 {
                    black_box(black_box(&c).sum());
                }
            },
            32,
            duration,
        ),
    };
    black_box(c);
    result
}
