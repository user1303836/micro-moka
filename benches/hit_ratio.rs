use std::num::NonZeroUsize;

mod common;
use common::*;

fn main() {
    let cap = 1_000usize;
    let key_space = 10_000usize;
    let num_ops = 1_000_000usize;

    println!(
        "Hit Ratio Comparison (capacity={cap}, key_space={key_space}, ops={num_ops}, seed={SEED})"
    );
    println!();
    println!("| Distribution | micro-moka | quick-cache | lru | hashlink | mini-moka |");
    println!("|---|---|---|---|---|---|");

    for (name, s) in [
        ("Zipf s=0.7", 0.7),
        ("Zipf s=0.9", 0.9),
        ("Zipf s=1.0", 1.0),
        ("Zipf s=1.2", 1.2),
    ] {
        let workload = generate_zipf_workload(num_ops, s, key_space, SEED);
        let (mm, qc, lr, hl, mk) = measure_all(&workload, cap);
        println!("| {name} | {mm:.1}% | {qc:.1}% | {lr:.1}% | {hl:.1}% | {mk:.1}% |");
    }

    let workload = generate_uniform_workload(num_ops, key_space, SEED);
    let (mm, qc, lr, hl, mk) = measure_all(&workload, cap);
    println!("| Uniform | {mm:.1}% | {qc:.1}% | {lr:.1}% | {hl:.1}% | {mk:.1}% |");
}

fn measure_all(workload: &[u64], cap: usize) -> (f64, f64, f64, f64, f64) {
    (
        hit_ratio_micro_moka(workload, cap),
        hit_ratio_quick_cache(workload, cap),
        hit_ratio_lru(workload, cap),
        hit_ratio_hashlink(workload, cap),
        hit_ratio_mini_moka(workload, cap),
    )
}

fn hit_ratio_micro_moka(workload: &[u64], cap: usize) -> f64 {
    let mut cache = micro_moka::unsync::Cache::new(cap as u64);
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

fn hit_ratio_quick_cache(workload: &[u64], cap: usize) -> f64 {
    let mut cache = quick_cache::unsync::Cache::new(cap);
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

fn hit_ratio_lru(workload: &[u64], cap: usize) -> f64 {
    let mut cache = lru::LruCache::new(NonZeroUsize::new(cap).unwrap());
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

fn hit_ratio_hashlink(workload: &[u64], cap: usize) -> f64 {
    let mut cache = hashlink::LruCache::new(cap);
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

fn hit_ratio_mini_moka(workload: &[u64], cap: usize) -> f64 {
    let mut cache = mini_moka::unsync::Cache::new(cap as u64);
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
