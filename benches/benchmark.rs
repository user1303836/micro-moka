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

fn main() {
    let zipf_workload = generate_zipf_workload(OPS_PER_ITER, 1.0, KEY_SPACE, SEED);
    let mixed_ops_95 = generate_mixed_ops(OPS_PER_ITER, 95, SEED);
    let mixed_ops_50 = generate_mixed_ops(OPS_PER_ITER, 50, SEED);

    println!("=== Throughput (Mops/sec) | cap={CAP} | Zipf s=1.0 ===");
    println!();
    println!(
        "| Operation | micro-moka | quick-cache | lru | hashlink | mini-moka | hashmap |"
    );
    println!("|-----------|-----------|-------------|-----|----------|-----------|---------|");

    print_throughput_row("get", &zipf_workload, None);
    print_throughput_row("insert", &zipf_workload, None);
    print_throughput_row("mixed 95/5", &zipf_workload, Some(&mixed_ops_95));
    print_throughput_row("mixed 50/50", &zipf_workload, Some(&mixed_ops_50));

    println!();
    println!("=== Hit Ratio (%) | cap={CAP} | {HIT_RATIO_OPS} ops ===");
    println!();
    println!("| Distribution | micro-moka | quick-cache | lru | hashlink | mini-moka |");
    println!("|---|---|---|---|---|---|");

    for (name, s) in [
        ("Zipf s=0.7", 0.7),
        ("Zipf s=0.9", 0.9),
        ("Zipf s=1.0", 1.0),
        ("Zipf s=1.2", 1.2),
    ] {
        let workload = generate_zipf_workload(HIT_RATIO_OPS, s, KEY_SPACE, SEED);
        let (mm, qc, lr, hl, mk) = measure_hit_ratios(&workload);
        println!("| {name} | {mm:.1} | {qc:.1} | {lr:.1} | {hl:.1} | {mk:.1} |");
    }

    let uniform_workload = generate_uniform_workload(HIT_RATIO_OPS, KEY_SPACE, SEED);
    let (mm, qc, lr, hl, mk) = measure_hit_ratios(&uniform_workload);
    println!("| Uniform | {mm:.1} | {qc:.1} | {lr:.1} | {hl:.1} | {mk:.1} |");
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
        None if op == "get" => bench_get_micro_moka(workload),
        _ => bench_insert_micro_moka(workload),
    };
    let qc = match mixed_ops {
        Some(ops) => bench_mixed_quick_cache(workload, ops),
        None if op == "get" => bench_get_quick_cache(workload),
        _ => bench_insert_quick_cache(workload),
    };
    let lr = match mixed_ops {
        Some(ops) => bench_mixed_lru(workload, ops),
        None if op == "get" => bench_get_lru(workload),
        _ => bench_insert_lru(workload),
    };
    let hl = match mixed_ops {
        Some(ops) => bench_mixed_hashlink(workload, ops),
        None if op == "get" => bench_get_hashlink(workload),
        _ => bench_insert_hashlink(workload),
    };
    let mk = match mixed_ops {
        Some(ops) => bench_mixed_mini_moka(workload, ops),
        None if op == "get" => bench_get_mini_moka(workload),
        _ => bench_insert_mini_moka(workload),
    };
    let hm = match mixed_ops {
        Some(ops) => bench_mixed_hashmap(workload, ops),
        None if op == "get" => bench_get_hashmap(workload),
        _ => bench_insert_hashmap(workload),
    };

    println!(
        "| {:<9} | {:>9.1} | {:>11.1} | {:>3.1} | {:>8.1} | {:>9.1} | {:>7.1} |",
        op, mm, qc, lr, hl, mk, hm,
    );
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

fn measure_hit_ratios(workload: &[u64]) -> (f64, f64, f64, f64, f64) {
    (
        hit_ratio_micro_moka(workload),
        hit_ratio_quick_cache(workload),
        hit_ratio_lru(workload),
        hit_ratio_hashlink(workload),
        hit_ratio_mini_moka(workload),
    )
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
