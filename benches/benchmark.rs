use std::collections::HashMap;
use std::hint::black_box;
use std::num::NonZeroUsize;
use std::time::Instant;

mod common;
use common::*;

const WARMUP_ITERS: usize = 500;
const MEASURE_ITERS: usize = 2_000;
const CAP: usize = 1_000;
const KEY_SPACE: usize = CAP * KEY_SPACE_MULTIPLIER;
const HIT_RATIO_OPS: usize = 1_000_000;
const ADMISSION_OUTCOME_SAMPLES: usize = 2_000;
const ADMISSION_CAP: usize = 10_000;
const ALL_HOT_SETUPS: usize = 200;
const ADMISSION_SCAN_LIMIT: u32 = 16;

fn main() {
    let zipf_workload = generate_zipf_workload(OPS_PER_ITER, 1.0, KEY_SPACE, SEED);
    let resident_workload = generate_uniform_workload(OPS_PER_ITER, CAP, SEED);
    let negative_workload = generate_uniform_workload(OPS_PER_ITER, CAP, SEED)
        .into_iter()
        .map(|key| key + CAP as u64)
        .collect::<Vec<_>>();
    let mixed_ops_95 = generate_mixed_ops(OPS_PER_ITER, 95, SEED);
    let mixed_ops_50 = generate_mixed_ops(OPS_PER_ITER, 50, SEED);

    println!("=== Throughput (Mops/sec) | cap={CAP} | Zipf s=1.0 ===");
    println!();
    println!("| Operation | micro-moka | quick-cache | lru | hashlink | mini-moka | sieve-cache | senba | hashmap |");
    println!("|---|---:|---:|---:|---:|---:|---:|---:|---:|");

    print_throughput_row("get", &zipf_workload, None);
    print_throughput_row("miss", &negative_workload, None);
    print_throughput_row("insert", &zipf_workload, None);
    print_throughput_row("update", &resident_workload, None);
    print_throughput_row("mixed 95/5", &zipf_workload, Some(&mixed_ops_95));
    print_throughput_row("mixed 50/50", &zipf_workload, Some(&mixed_ops_50));
    print_churn_row(&resident_workload);

    println!();
    println!("micro-moka, sieve-cache, and HashMap use HashDoS-resistant RandomState above; other competitors use their native faster defaults. senba uses Slot16.");
    println!(
        "micro-moka get with opt-in aHash: {:.1} Mops/sec",
        bench_get_micro_moka_ahash(&zipf_workload)
    );

    println!();
    println!("=== Hit Ratio (%) | cap={CAP} | {HIT_RATIO_OPS} ops ===");
    println!();
    println!("| Distribution | micro-moka | quick-cache | lru | hashlink | mini-moka | sieve-cache | senba |");
    println!("|---|---:|---:|---:|---:|---:|---:|---:|");

    for (name, s) in [
        ("Zipf s=0.7", 0.7),
        ("Zipf s=0.9", 0.9),
        ("Zipf s=1.0", 1.0),
        ("Zipf s=1.2", 1.2),
    ] {
        let workload = generate_zipf_workload(HIT_RATIO_OPS, s, KEY_SPACE, SEED);
        let ratios = measure_hit_ratios(&workload);
        println!(
            "| {name} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} |",
            ratios[0], ratios[1], ratios[2], ratios[3], ratios[4], ratios[5], ratios[6]
        );
    }

    let uniform_workload = generate_uniform_workload(HIT_RATIO_OPS, KEY_SPACE, SEED);
    let ratios = measure_hit_ratios(&uniform_workload);
    println!(
        "| Uniform | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} |",
        ratios[0], ratios[1], ratios[2], ratios[3], ratios[4], ratios[5], ratios[6]
    );

    print_admission_quality();
    print_admission_work();
}

// ---------------------------------------------------------------------------
// Throughput measurement
// ---------------------------------------------------------------------------

fn mops(iters: usize, elapsed: std::time::Duration) -> f64 {
    (iters as f64 * OPS_PER_ITER as f64) / elapsed.as_secs_f64() / 1_000_000.0
}

fn print_throughput_row(op: &str, workload: &[u64], mixed_ops: Option<&[bool]>) {
    let mm = match mixed_ops {
        Some(ops) => bench_mixed_micro_moka(workload, ops),
        None if op == "get" || op == "miss" => bench_get_micro_moka(workload),
        _ => bench_insert_micro_moka(workload),
    };
    let qc = match mixed_ops {
        Some(ops) => bench_mixed_quick_cache(workload, ops),
        None if op == "get" || op == "miss" => bench_get_quick_cache(workload),
        _ => bench_insert_quick_cache(workload),
    };
    let lr = match mixed_ops {
        Some(ops) => bench_mixed_lru(workload, ops),
        None if op == "get" || op == "miss" => bench_get_lru(workload),
        _ => bench_insert_lru(workload),
    };
    let hl = match mixed_ops {
        Some(ops) => bench_mixed_hashlink(workload, ops),
        None if op == "get" || op == "miss" => bench_get_hashlink(workload),
        _ => bench_insert_hashlink(workload),
    };
    let mk = match mixed_ops {
        Some(ops) => bench_mixed_mini_moka(workload, ops),
        None if op == "get" || op == "miss" => bench_get_mini_moka(workload),
        _ => bench_insert_mini_moka(workload),
    };
    let sc = match mixed_ops {
        Some(ops) => bench_mixed_sieve_cache(workload, ops),
        None if op == "get" || op == "miss" => bench_get_sieve_cache(workload),
        _ => bench_insert_sieve_cache(workload),
    };
    let sb = match mixed_ops {
        Some(ops) => bench_mixed_senba(workload, ops),
        None if op == "get" || op == "miss" => bench_get_senba(workload),
        _ => bench_insert_senba(workload),
    };
    let hm = match mixed_ops {
        Some(ops) => bench_mixed_hashmap(workload, ops),
        None if op == "get" || op == "miss" => bench_get_hashmap(workload),
        _ => bench_insert_hashmap(workload),
    };

    println!(
        "| {op} | {mm:.1} | {qc:.1} | {lr:.1} | {hl:.1} | {mk:.1} | {sc:.1} | {sb:.1} | {hm:.1} |",
    );
}

fn print_churn_row(workload: &[u64]) {
    let mm = bench_churn_micro_moka(workload);
    let qc = bench_churn_quick_cache(workload);
    let lr = bench_churn_lru(workload);
    let hl = bench_churn_hashlink(workload);
    let mk = bench_churn_mini_moka(workload);
    let sc = bench_churn_sieve_cache(workload);
    let sb = bench_churn_senba(workload);
    let hm = bench_churn_hashmap(workload);
    println!(
        "| del+put | {mm:.1} | {qc:.1} | {lr:.1} | {hl:.1} | {mk:.1} | {sc:.1} | {sb:.1} | {hm:.1} |",
    );
}

fn bench_churn_micro_moka(workload: &[u64]) -> f64 {
    let mut cache = micro_moka::unsync::Cache::new(CAP as u64);
    for key in 0..CAP as u64 {
        cache.insert(key, key);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.remove(&key));
            cache.insert(key, key);
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.remove(&key));
            cache.insert(key, key);
        }
    }
    2.0 * mops(MEASURE_ITERS, start.elapsed())
}

fn bench_churn_quick_cache(workload: &[u64]) -> f64 {
    let mut cache = quick_cache::unsync::Cache::new(CAP);
    for key in 0..CAP as u64 {
        cache.insert(key, key);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.remove(&key));
            cache.insert(key, key);
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.remove(&key));
            cache.insert(key, key);
        }
    }
    2.0 * mops(MEASURE_ITERS, start.elapsed())
}

fn bench_churn_lru(workload: &[u64]) -> f64 {
    let mut cache = lru::LruCache::new(NonZeroUsize::new(CAP).unwrap());
    for key in 0..CAP as u64 {
        cache.put(key, key);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.pop(&key));
            cache.put(key, key);
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.pop(&key));
            cache.put(key, key);
        }
    }
    2.0 * mops(MEASURE_ITERS, start.elapsed())
}

fn bench_churn_hashlink(workload: &[u64]) -> f64 {
    let mut cache = hashlink::LruCache::new(CAP);
    for key in 0..CAP as u64 {
        cache.insert(key, key);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.remove(&key));
            cache.insert(key, key);
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.remove(&key));
            cache.insert(key, key);
        }
    }
    2.0 * mops(MEASURE_ITERS, start.elapsed())
}

fn bench_churn_mini_moka(workload: &[u64]) -> f64 {
    let mut cache = mini_moka::unsync::Cache::new(CAP as u64);
    for key in 0..CAP as u64 {
        cache.insert(key, key);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            cache.invalidate(&key);
            cache.insert(key, key);
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            cache.invalidate(&key);
            cache.insert(key, key);
        }
    }
    2.0 * mops(MEASURE_ITERS, start.elapsed())
}

fn bench_churn_sieve_cache(workload: &[u64]) -> f64 {
    let mut cache = sieve_cache::SieveCache::new(CAP).unwrap();
    for key in 0..CAP as u64 {
        cache.insert(key, key);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.remove(&key));
            cache.insert(key, key);
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.remove(&key));
            cache.insert(key, key);
        }
    }
    2.0 * mops(MEASURE_ITERS, start.elapsed())
}

fn bench_churn_senba(workload: &[u64]) -> f64 {
    let mut cache: senba::Cache<u64, u64, senba::Slot16> = senba::Cache::new(CAP);
    for key in 0..CAP as u64 {
        black_box(cache.insert(key, key));
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.remove(&key));
            black_box(cache.insert(key, key));
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.remove(&key));
            black_box(cache.insert(key, key));
        }
    }
    2.0 * mops(MEASURE_ITERS, start.elapsed())
}

fn bench_churn_hashmap(workload: &[u64]) -> f64 {
    let mut cache = HashMap::with_capacity(CAP);
    for key in 0..CAP as u64 {
        cache.insert(key, key);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.remove(&key));
            cache.insert(key, key);
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.remove(&key));
            cache.insert(key, key);
        }
    }
    2.0 * mops(MEASURE_ITERS, start.elapsed())
}

// -- micro-moka -------------------------------------------------------------

fn bench_get_micro_moka(workload: &[u64]) -> f64 {
    let mut cache = micro_moka::unsync::Cache::new(CAP as u64);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_get_micro_moka_ahash(workload: &[u64]) -> f64 {
    let mut cache = micro_moka::unsync::Cache::builder()
        .max_capacity(CAP as u64)
        .initial_capacity(CAP)
        .build_with_hasher(ahash::RandomState::new());
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_insert_micro_moka(workload: &[u64]) -> f64 {
    let mut cache = micro_moka::unsync::Cache::new(CAP as u64);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            cache.insert(key, key);
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            cache.insert(key, key);
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_mixed_micro_moka(workload: &[u64], ops: &[bool]) -> f64 {
    let mut cache = micro_moka::unsync::Cache::new(CAP as u64);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                cache.insert(key, key);
            }
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                cache.insert(key, key);
            }
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

// -- quick-cache -------------------------------------------------------------

fn bench_get_quick_cache(workload: &[u64]) -> f64 {
    let mut cache = quick_cache::unsync::Cache::new(CAP);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_insert_quick_cache(workload: &[u64]) -> f64 {
    let mut cache = quick_cache::unsync::Cache::new(CAP);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            cache.insert(key, key);
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            cache.insert(key, key);
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_mixed_quick_cache(workload: &[u64], ops: &[bool]) -> f64 {
    let mut cache = quick_cache::unsync::Cache::new(CAP);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                cache.insert(key, key);
            }
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                cache.insert(key, key);
            }
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

// -- lru ---------------------------------------------------------------------

fn bench_get_lru(workload: &[u64]) -> f64 {
    let mut cache = lru::LruCache::new(NonZeroUsize::new(CAP).unwrap());
    for i in 0..CAP as u64 {
        cache.put(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_insert_lru(workload: &[u64]) -> f64 {
    let mut cache = lru::LruCache::new(NonZeroUsize::new(CAP).unwrap());
    for i in 0..CAP as u64 {
        cache.put(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            cache.put(key, key);
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            cache.put(key, key);
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_mixed_lru(workload: &[u64], ops: &[bool]) -> f64 {
    let mut cache = lru::LruCache::new(NonZeroUsize::new(CAP).unwrap());
    for i in 0..CAP as u64 {
        cache.put(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                cache.put(key, key);
            }
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                cache.put(key, key);
            }
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

// -- hashlink ----------------------------------------------------------------

fn bench_get_hashlink(workload: &[u64]) -> f64 {
    let mut cache = hashlink::LruCache::new(CAP);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_insert_hashlink(workload: &[u64]) -> f64 {
    let mut cache = hashlink::LruCache::new(CAP);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            cache.insert(key, key);
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            cache.insert(key, key);
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_mixed_hashlink(workload: &[u64], ops: &[bool]) -> f64 {
    let mut cache = hashlink::LruCache::new(CAP);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                cache.insert(key, key);
            }
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                cache.insert(key, key);
            }
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

// -- mini-moka ---------------------------------------------------------------

fn bench_get_mini_moka(workload: &[u64]) -> f64 {
    let mut cache = mini_moka::unsync::Cache::new(CAP as u64);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_insert_mini_moka(workload: &[u64]) -> f64 {
    let mut cache = mini_moka::unsync::Cache::new(CAP as u64);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            cache.insert(key, key);
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            cache.insert(key, key);
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_mixed_mini_moka(workload: &[u64], ops: &[bool]) -> f64 {
    let mut cache = mini_moka::unsync::Cache::new(CAP as u64);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                cache.insert(key, key);
            }
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                cache.insert(key, key);
            }
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

// -- sieve-cache -------------------------------------------------------------

fn bench_get_sieve_cache(workload: &[u64]) -> f64 {
    let mut cache = sieve_cache::SieveCache::new(CAP).unwrap();
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_insert_sieve_cache(workload: &[u64]) -> f64 {
    let mut cache = sieve_cache::SieveCache::new(CAP).unwrap();
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.insert(key, key));
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.insert(key, key));
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_mixed_sieve_cache(workload: &[u64], ops: &[bool]) -> f64 {
    let mut cache = sieve_cache::SieveCache::new(CAP).unwrap();
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                black_box(cache.insert(key, key));
            }
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                black_box(cache.insert(key, key));
            }
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

// -- senba -------------------------------------------------------------------

fn bench_get_senba(workload: &[u64]) -> f64 {
    let mut cache: senba::Cache<u64, u64, senba::Slot16> = senba::Cache::new(CAP);
    for i in 0..CAP as u64 {
        black_box(cache.insert(i, i));
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_insert_senba(workload: &[u64]) -> f64 {
    let mut cache: senba::Cache<u64, u64, senba::Slot16> = senba::Cache::new(CAP);
    for i in 0..CAP as u64 {
        black_box(cache.insert(i, i));
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.insert(key, key));
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.insert(key, key));
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_mixed_senba(workload: &[u64], ops: &[bool]) -> f64 {
    let mut cache: senba::Cache<u64, u64, senba::Slot16> = senba::Cache::new(CAP);
    for i in 0..CAP as u64 {
        black_box(cache.insert(i, i));
    }
    for _ in 0..WARMUP_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                black_box(cache.insert(key, key));
            }
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                black_box(cache.insert(key, key));
            }
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

// -- hashmap (unbounded baseline) --------------------------------------------

fn bench_get_hashmap(workload: &[u64]) -> f64 {
    let mut cache = HashMap::with_capacity(CAP);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            black_box(cache.get(&key));
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_insert_hashmap(workload: &[u64]) -> f64 {
    let mut cache = HashMap::with_capacity(CAP);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for &key in workload {
            cache.insert(key, key);
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for &key in workload {
            cache.insert(key, key);
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

fn bench_mixed_hashmap(workload: &[u64], ops: &[bool]) -> f64 {
    let mut cache = HashMap::with_capacity(CAP);
    for i in 0..CAP as u64 {
        cache.insert(i, i);
    }
    for _ in 0..WARMUP_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                cache.insert(key, key);
            }
        }
    }
    let start = Instant::now();
    for _ in 0..MEASURE_ITERS {
        for (&key, &is_read) in workload.iter().zip(ops.iter()) {
            if is_read {
                black_box(cache.get(&key));
            } else {
                cache.insert(key, key);
            }
        }
    }
    mops(MEASURE_ITERS, start.elapsed())
}

// ---------------------------------------------------------------------------
// Hit ratio measurement
// ---------------------------------------------------------------------------

fn measure_hit_ratios(workload: &[u64]) -> [f64; 7] {
    [
        hit_ratio_micro_moka(workload),
        hit_ratio_quick_cache(workload),
        hit_ratio_lru(workload),
        hit_ratio_hashlink(workload),
        hit_ratio_mini_moka(workload),
        hit_ratio_sieve_cache(workload),
        hit_ratio_senba(workload),
    ]
}

fn hit_ratio_micro_moka(workload: &[u64]) -> f64 {
    let mut cache = micro_moka::unsync::Cache::new(CAP as u64);
    let mut hits = 0u64;
    for &key in workload {
        if cache.get(&key).is_some() {
            hits += 1;
        } else {
            cache.insert(key, key);
        }
    }
    hits as f64 / workload.len() as f64 * 100.0
}

fn hit_ratio_quick_cache(workload: &[u64]) -> f64 {
    let mut cache = quick_cache::unsync::Cache::new(CAP);
    let mut hits = 0u64;
    for &key in workload {
        if cache.get(&key).is_some() {
            hits += 1;
        } else {
            cache.insert(key, key);
        }
    }
    hits as f64 / workload.len() as f64 * 100.0
}

fn hit_ratio_lru(workload: &[u64]) -> f64 {
    let mut cache = lru::LruCache::new(NonZeroUsize::new(CAP).unwrap());
    let mut hits = 0u64;
    for &key in workload {
        if cache.get(&key).is_some() {
            hits += 1;
        } else {
            cache.put(key, key);
        }
    }
    hits as f64 / workload.len() as f64 * 100.0
}

fn hit_ratio_hashlink(workload: &[u64]) -> f64 {
    let mut cache = hashlink::LruCache::new(CAP);
    let mut hits = 0u64;
    for &key in workload {
        if cache.get(&key).is_some() {
            hits += 1;
        } else {
            cache.insert(key, key);
        }
    }
    hits as f64 / workload.len() as f64 * 100.0
}

fn hit_ratio_mini_moka(workload: &[u64]) -> f64 {
    let mut cache = mini_moka::unsync::Cache::new(CAP as u64);
    let mut hits = 0u64;
    for &key in workload {
        if cache.get(&key).is_some() {
            hits += 1;
        } else {
            cache.insert(key, key);
        }
    }
    hits as f64 / workload.len() as f64 * 100.0
}

fn hit_ratio_sieve_cache(workload: &[u64]) -> f64 {
    let mut cache = sieve_cache::SieveCache::new(CAP).unwrap();
    let mut hits = 0u64;
    for &key in workload {
        if cache.get(&key).is_some() {
            hits += 1;
        } else {
            cache.insert(key, key);
        }
    }
    hits as f64 / workload.len() as f64 * 100.0
}

fn hit_ratio_senba(workload: &[u64]) -> f64 {
    let mut cache: senba::Cache<u64, u64, senba::Slot16> = senba::Cache::new(CAP);
    let mut hits = 0u64;
    for &key in workload {
        if cache.get(&key).is_some() {
            hits += 1;
        } else {
            black_box(cache.insert(key, key));
        }
    }
    hits as f64 / workload.len() as f64 * 100.0
}

fn print_admission_quality() {
    println!();
    println!("=== SIEVE Admission Quality (%) | cap={CAP} | {HIT_RATIO_OPS} ops ===");
    println!();
    println!("| Workload | exact SIEVE | budget 1 | budget 4 | budget 16 | budget 64 |");
    println!("|---|---:|---:|---:|---:|---:|");

    for (name, workload) in [
        (
            "Zipf s=0.7",
            generate_zipf_workload(HIT_RATIO_OPS, 0.7, KEY_SPACE, SEED),
        ),
        (
            "Zipf s=1.0",
            generate_zipf_workload(HIT_RATIO_OPS, 1.0, KEY_SPACE, SEED),
        ),
        (
            "Zipf s=1.2",
            generate_zipf_workload(HIT_RATIO_OPS, 1.2, KEY_SPACE, SEED),
        ),
        (
            "Uniform",
            generate_uniform_workload(HIT_RATIO_OPS, KEY_SPACE, SEED),
        ),
    ] {
        let exact = hit_ratio_exact_ahash(&workload);
        let b1 = hit_ratio_budgeted(&workload, 1).0;
        let b4 = hit_ratio_budgeted(&workload, 4).0;
        let b16 = hit_ratio_budgeted(&workload, 16).0;
        let b64 = hit_ratio_budgeted(&workload, 64).0;
        println!("| {name} | {exact:.2} | {b1:.2} | {b4:.2} | {b16:.2} | {b64:.2} |");
    }

    let scan = generate_scan_resistance_workload();
    let exact = hit_ratio_exact_ahash(&scan);
    let (b1, r1) = hit_ratio_budgeted(&scan, 1);
    let (b4, r4) = hit_ratio_budgeted(&scan, 4);
    let (b16, r16) = hit_ratio_budgeted(&scan, 16);
    let (b64, r64) = hit_ratio_budgeted(&scan, 64);
    println!("| Hot set + scans | {exact:.2} | {b1:.2} | {b4:.2} | {b16:.2} | {b64:.2} |");
    println!("Rejected candidates on hot-set workload: b1={r1}, b4={r4}, b16={r16}, b64={r64}");

    let weighted = generate_zipf_workload(HIT_RATIO_OPS, 1.0, KEY_SPACE, SEED);
    let (exact_requests, exact_bytes) = weighted_hit_ratio(&weighted, None);
    let (budget_requests, budget_bytes) = weighted_hit_ratio(&weighted, Some(16));
    println!();
    println!("Synthetic 1-1024 byte objects, Zipf s=1.0:");
    println!("exact request={exact_requests:.2}%, byte={exact_bytes:.2}%");
    println!("budget 16 request={budget_requests:.2}%, byte={budget_bytes:.2}%");
}

fn hit_ratio_budgeted(workload: &[u64], scan_limit: u32) -> (f64, u64) {
    let mut cache = micro_moka::unsync::Cache::builder()
        .max_capacity(CAP as u64)
        .initial_capacity(CAP)
        .admission_scan_limit(scan_limit)
        .build_with_hasher(ahash::RandomState::new());
    let mut hits = 0u64;
    let mut rejected = 0u64;
    for &key in workload {
        if cache.get(&key).is_some() {
            hits += 1;
        } else if cache.try_insert(key, key).is_err() {
            rejected += 1;
        }
    }
    (hits as f64 / workload.len() as f64 * 100.0, rejected)
}

fn hit_ratio_exact_ahash(workload: &[u64]) -> f64 {
    let mut cache = micro_moka::unsync::Cache::builder()
        .max_capacity(CAP as u64)
        .initial_capacity(CAP)
        .build_with_hasher(ahash::RandomState::new());
    let mut hits = 0u64;
    for &key in workload {
        if cache.get(&key).is_some() {
            hits += 1;
        } else {
            cache.insert(key, key);
        }
    }
    hits as f64 / workload.len() as f64 * 100.0
}

fn generate_scan_resistance_workload() -> Vec<u64> {
    let mut workload = Vec::with_capacity(HIT_RATIO_OPS);
    workload.extend(0..CAP as u64);
    workload.extend(0..CAP as u64);

    const SCAN_BURST: usize = 32;
    let round_len = CAP + SCAN_BURST;
    let rounds = (HIT_RATIO_OPS - workload.len()) / round_len;
    for round in 0..rounds {
        let scan_start = KEY_SPACE as u64 + (round * SCAN_BURST) as u64;
        workload.extend(scan_start..scan_start + SCAN_BURST as u64);
        workload.extend(0..CAP as u64);
    }
    workload.resize(HIT_RATIO_OPS, 0);
    workload
}

fn weighted_hit_ratio(workload: &[u64], scan_limit: Option<u32>) -> (f64, f64) {
    let mut cache = micro_moka::unsync::Cache::builder()
        .max_capacity(CAP as u64)
        .initial_capacity(CAP)
        .admission_scan_limit(scan_limit.unwrap_or(16))
        .build_with_hasher(ahash::RandomState::new());
    let mut request_hits = 0u64;
    let mut hit_bytes = 0u64;
    let mut requested_bytes = 0u64;
    for &key in workload {
        let bytes = 1 + key.wrapping_mul(0x9e37_79b9_7f4a_7c15) % 1_024;
        requested_bytes += bytes;
        if cache.get(&key).is_some() {
            request_hits += 1;
            hit_bytes += bytes;
        } else if scan_limit.is_some() {
            let _ = cache.try_insert(key, key);
        } else {
            cache.insert(key, key);
        }
    }
    (
        request_hits as f64 / workload.len() as f64 * 100.0,
        hit_bytes as f64 / requested_bytes as f64 * 100.0,
    )
}

fn print_admission_work() {
    let outcomes = admission_attempt_outcomes_micro();

    assert_eq!(outcomes.accepted, ADMISSION_OUTCOME_SAMPLES * 99 / 100);
    assert_eq!(outcomes.rejected, ADMISSION_OUTCOME_SAMPLES / 100);

    println!();
    println!(
        "=== Full-cache budgeted admission outcomes | cap={ADMISSION_CAP} | scan limit={ADMISSION_SCAN_LIMIT} ==="
    );
    println!();
    println!(
        "Every 100th independent cache setup has all residents visited; the others have an unvisited oldest resident."
    );
    println!("| Outcome/path | Count | Rate | Candidate state | Resident inspections |");
    println!("|---|---:|---:|---|---:|");
    println!(
        "| Accepted attempt (successful admission) | {} | {:.1}% | inserted | exactly 1 |",
        outcomes.accepted,
        outcomes.accepted as f64 / ADMISSION_OUTCOME_SAMPLES as f64 * 100.0
    );
    println!(
        "| Rejected attempt | {} | {:.1}% | returned; not inserted | exactly {ADMISSION_SCAN_LIMIT} |",
        outcomes.rejected,
        outcomes.rejected as f64 / ADMISSION_OUTCOME_SAMPLES as f64 * 100.0
    );

    let exact_successes = all_hot_success_micro_exact();
    let attempt_counts = all_hot_success_micro_budgeted();
    let rejected_attempts = attempt_counts
        .iter()
        .map(|attempts| attempts - 1)
        .sum::<u64>();
    let successful_admissions = attempt_counts.len() as u64;
    let total_attempts = rejected_attempts + successful_admissions;
    let expected_attempts = ADMISSION_CAP as u64 / ADMISSION_SCAN_LIMIT as u64 + 1;

    assert_eq!(ADMISSION_CAP as u32 % ADMISSION_SCAN_LIMIT, 0);
    assert_eq!(exact_successes, ALL_HOT_SETUPS);
    assert!(attempt_counts
        .iter()
        .all(|&attempts| attempts == expected_attempts));

    println!();
    println!(
        "=== Successful full-cache admission work | cap={ADMISSION_CAP} | all residents visited ==="
    );
    println!();
    println!("| Micro Moka path | Independent cache setups | Calls per successful admission | Rejected attempts | Successful admissions | Resident inspections through success |");
    println!("|---|---:|---:|---:|---:|---:|");
    println!(
        "| Exact `insert` | {exact_successes} | exactly 1 | 0 | {exact_successes} | exactly {} in one call |",
        ADMISSION_CAP + 1
    );
    println!(
        "| Budget-{ADMISSION_SCAN_LIMIT} `try_insert`, retry through success | {} | exactly {expected_attempts} ({} rejected + 1 accepted) | {rejected_attempts} | {successful_admissions} | exactly {} total; at most {ADMISSION_SCAN_LIMIT} per call |",
        attempt_counts.len(),
        expected_attempts - 1,
        ADMISSION_CAP + 1
    );
    println!(
        "Budget-{ADMISSION_SCAN_LIMIT} retry-chain attempt outcomes: rejected={rejected_attempts} ({:.3}%), accepted={successful_admissions} ({:.3}%).",
        rejected_attempts as f64 / total_attempts as f64 * 100.0,
        successful_admissions as f64 / total_attempts as f64 * 100.0
    );
}

struct AdmissionAttemptOutcomes {
    accepted: usize,
    rejected: usize,
}

fn admission_attempt_outcomes_micro() -> AdmissionAttemptOutcomes {
    let mut accepted = 0;
    let mut rejected = 0;
    for sample in 0..ADMISSION_OUTCOME_SAMPLES {
        let mut cache = micro_moka::unsync::Cache::builder()
            .max_capacity(ADMISSION_CAP as u64)
            .initial_capacity(ADMISSION_CAP)
            .admission_scan_limit(ADMISSION_SCAN_LIMIT)
            .build_with_hasher(ahash::RandomState::new());
        for key in 0..ADMISSION_CAP as u64 {
            cache.insert(key, key);
        }
        if sample % 100 == 0 {
            for key in 0..ADMISSION_CAP as u64 {
                black_box(cache.get(&key));
            }
        }
        let candidate = ADMISSION_CAP as u64 + sample as u64;
        let result = black_box(cache.try_insert(candidate, candidate));
        if result.is_ok() {
            accepted += 1;
            assert!(cache.contains_key(&candidate));
        } else {
            rejected += 1;
            assert_eq!(result, Err((candidate, candidate)));
            assert!(!cache.contains_key(&candidate));
        }
        assert_eq!(cache.entry_count(), ADMISSION_CAP as u64);
    }
    AdmissionAttemptOutcomes { accepted, rejected }
}

fn all_hot_success_micro_exact() -> usize {
    let mut successes = 0;
    for sample in 0..ALL_HOT_SETUPS {
        let mut cache = micro_moka::unsync::Cache::builder()
            .max_capacity(ADMISSION_CAP as u64)
            .initial_capacity(ADMISSION_CAP)
            .build_with_hasher(ahash::RandomState::new());
        for key in 0..ADMISSION_CAP as u64 {
            cache.insert(key, key);
        }
        for key in 0..ADMISSION_CAP as u64 {
            black_box(cache.get(&key));
        }
        let candidate = ADMISSION_CAP as u64 + sample as u64;
        cache.insert(candidate, candidate);
        assert!(cache.contains_key(&candidate));
        successes += 1;
    }
    successes
}

fn all_hot_success_micro_budgeted() -> Vec<u64> {
    let mut attempt_counts = Vec::with_capacity(ALL_HOT_SETUPS);
    for sample in 0..ALL_HOT_SETUPS {
        let mut cache = micro_moka::unsync::Cache::builder()
            .max_capacity(ADMISSION_CAP as u64)
            .initial_capacity(ADMISSION_CAP)
            .admission_scan_limit(ADMISSION_SCAN_LIMIT)
            .build_with_hasher(ahash::RandomState::new());
        for key in 0..ADMISSION_CAP as u64 {
            cache.insert(key, key);
        }
        for key in 0..ADMISSION_CAP as u64 {
            black_box(cache.get(&key));
        }
        let candidate = ADMISSION_CAP as u64 + sample as u64;
        let mut attempts = 0u64;
        loop {
            attempts += 1;
            if cache.try_insert(candidate, candidate).is_ok() {
                break;
            }
        }
        attempt_counts.push(attempts);
        assert!(cache.contains_key(&candidate));
    }
    attempt_counts
}
