use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::time::Duration;

mod common;
use common::*;

macro_rules! bench_get_all {
    ($c:expr, $group_name:expr, $cap:expr, $workload:expr) => {{
        let mut group = $c.benchmark_group($group_name);
        group.throughput(Throughput::Elements($workload.len() as u64));

        group.bench_function("micro-moka", |b| {
            let mut cache = micro_moka::unsync::Cache::new($cap as u64);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for &key in &$workload {
                    black_box(cache.get(&key));
                }
            });
        });

        group.bench_function("quick-cache", |b| {
            let mut cache = quick_cache::unsync::Cache::new($cap);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for &key in &$workload {
                    black_box(cache.get(&key));
                }
            });
        });

        group.bench_function("lru", |b| {
            let mut cache = lru::LruCache::new(NonZeroUsize::new($cap).unwrap());
            for i in 0..$cap as u64 {
                cache.put(i, i);
            }
            b.iter(|| {
                for &key in &$workload {
                    black_box(cache.get(&key));
                }
            });
        });

        group.bench_function("hashlink", |b| {
            let mut cache = hashlink::LruCache::new($cap);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for &key in &$workload {
                    black_box(cache.get(&key));
                }
            });
        });

        group.bench_function("mini-moka", |b| {
            let mut cache = mini_moka::unsync::Cache::new($cap as u64);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for &key in &$workload {
                    black_box(cache.get(&key));
                }
            });
        });

        group.bench_function("hashmap", |b| {
            let mut cache = HashMap::with_capacity($cap);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for &key in &$workload {
                    black_box(cache.get(&key));
                }
            });
        });

        group.finish();
    }};
}

macro_rules! bench_insert_all {
    ($c:expr, $group_name:expr, $cap:expr, $workload:expr) => {{
        let mut group = $c.benchmark_group($group_name);
        group.throughput(Throughput::Elements($workload.len() as u64));

        group.bench_function("micro-moka", |b| {
            let mut cache = micro_moka::unsync::Cache::new($cap as u64);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for &key in &$workload {
                    cache.insert(key, key);
                }
            });
        });

        group.bench_function("quick-cache", |b| {
            let mut cache = quick_cache::unsync::Cache::new($cap);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for &key in &$workload {
                    cache.insert(key, key);
                }
            });
        });

        group.bench_function("lru", |b| {
            let mut cache = lru::LruCache::new(NonZeroUsize::new($cap).unwrap());
            for i in 0..$cap as u64 {
                cache.put(i, i);
            }
            b.iter(|| {
                for &key in &$workload {
                    cache.put(key, key);
                }
            });
        });

        group.bench_function("hashlink", |b| {
            let mut cache = hashlink::LruCache::new($cap);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for &key in &$workload {
                    cache.insert(key, key);
                }
            });
        });

        group.bench_function("mini-moka", |b| {
            let mut cache = mini_moka::unsync::Cache::new($cap as u64);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for &key in &$workload {
                    cache.insert(key, key);
                }
            });
        });

        group.bench_function("hashmap", |b| {
            let mut cache = HashMap::with_capacity($cap);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for &key in &$workload {
                    cache.insert(key, key);
                }
            });
        });

        group.finish();
    }};
}

macro_rules! bench_mixed_all {
    ($c:expr, $group_name:expr, $cap:expr, $workload:expr, $ops:expr) => {{
        let mut group = $c.benchmark_group($group_name);
        group.throughput(Throughput::Elements($workload.len() as u64));

        group.bench_function("micro-moka", |b| {
            let mut cache = micro_moka::unsync::Cache::new($cap as u64);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for (&key, &is_read) in $workload.iter().zip($ops.iter()) {
                    if is_read {
                        black_box(cache.get(&key));
                    } else {
                        cache.insert(key, key);
                    }
                }
            });
        });

        group.bench_function("quick-cache", |b| {
            let mut cache = quick_cache::unsync::Cache::new($cap);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for (&key, &is_read) in $workload.iter().zip($ops.iter()) {
                    if is_read {
                        black_box(cache.get(&key));
                    } else {
                        cache.insert(key, key);
                    }
                }
            });
        });

        group.bench_function("lru", |b| {
            let mut cache = lru::LruCache::new(NonZeroUsize::new($cap).unwrap());
            for i in 0..$cap as u64 {
                cache.put(i, i);
            }
            b.iter(|| {
                for (&key, &is_read) in $workload.iter().zip($ops.iter()) {
                    if is_read {
                        black_box(cache.get(&key));
                    } else {
                        cache.put(key, key);
                    }
                }
            });
        });

        group.bench_function("hashlink", |b| {
            let mut cache = hashlink::LruCache::new($cap);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for (&key, &is_read) in $workload.iter().zip($ops.iter()) {
                    if is_read {
                        black_box(cache.get(&key));
                    } else {
                        cache.insert(key, key);
                    }
                }
            });
        });

        group.bench_function("mini-moka", |b| {
            let mut cache = mini_moka::unsync::Cache::new($cap as u64);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for (&key, &is_read) in $workload.iter().zip($ops.iter()) {
                    if is_read {
                        black_box(cache.get(&key));
                    } else {
                        cache.insert(key, key);
                    }
                }
            });
        });

        group.bench_function("hashmap", |b| {
            let mut cache = HashMap::with_capacity($cap);
            for i in 0..$cap as u64 {
                cache.insert(i, i);
            }
            b.iter(|| {
                for (&key, &is_read) in $workload.iter().zip($ops.iter()) {
                    if is_read {
                        black_box(cache.get(&key));
                    } else {
                        cache.insert(key, key);
                    }
                }
            });
        });

        group.finish();
    }};
}

fn bench_get_zipf_s1_0(c: &mut Criterion) {
    for &cap in &CACHE_SIZES {
        let key_space = cap * KEY_SPACE_MULTIPLIER;
        let workload = generate_zipf_workload(OPS_PER_ITER, 1.0, key_space, SEED);
        bench_get_all!(c, format!("get/zipf_s1.0/cap_{cap}"), cap, workload);
    }
}

fn bench_get_zipf_s0_7(c: &mut Criterion) {
    for &cap in &CACHE_SIZES {
        let key_space = cap * KEY_SPACE_MULTIPLIER;
        let workload = generate_zipf_workload(OPS_PER_ITER, 0.7, key_space, SEED);
        bench_get_all!(c, format!("get/zipf_s0.7/cap_{cap}"), cap, workload);
    }
}

fn bench_get_uniform(c: &mut Criterion) {
    for &cap in &CACHE_SIZES {
        let key_space = cap * KEY_SPACE_MULTIPLIER;
        let workload = generate_uniform_workload(OPS_PER_ITER, key_space, SEED);
        bench_get_all!(c, format!("get/uniform/cap_{cap}"), cap, workload);
    }
}

fn bench_insert_zipf(c: &mut Criterion) {
    for &cap in &CACHE_SIZES {
        let key_space = cap * KEY_SPACE_MULTIPLIER;
        let workload = generate_zipf_workload(OPS_PER_ITER, 1.0, key_space, SEED);
        bench_insert_all!(c, format!("insert/zipf_s1.0/cap_{cap}"), cap, workload);
    }
}

fn bench_mixed_95_5(c: &mut Criterion) {
    for &cap in &CACHE_SIZES {
        let key_space = cap * KEY_SPACE_MULTIPLIER;
        let workload = generate_zipf_workload(OPS_PER_ITER, 1.0, key_space, SEED);
        let ops = generate_mixed_ops(OPS_PER_ITER, 95, SEED);
        bench_mixed_all!(c, format!("mixed/95_5/cap_{cap}"), cap, workload, ops);
    }
}

fn bench_mixed_50_50(c: &mut Criterion) {
    for &cap in &CACHE_SIZES {
        let key_space = cap * KEY_SPACE_MULTIPLIER;
        let workload = generate_zipf_workload(OPS_PER_ITER, 1.0, key_space, SEED);
        let ops = generate_mixed_ops(OPS_PER_ITER, 50, SEED);
        bench_mixed_all!(c, format!("mixed/50_50/cap_{cap}"), cap, workload, ops);
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3));
    targets =
        bench_get_zipf_s1_0,
        bench_get_zipf_s0_7,
        bench_get_uniform,
        bench_insert_zipf,
        bench_mixed_95_5,
        bench_mixed_50_50,
}

criterion_main!(benches);
