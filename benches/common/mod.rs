#![allow(dead_code)]

use rand::distr::Distribution;
use rand::rngs::StdRng;
use rand::{RngExt, SeedableRng};
use rand_distr::Zipf;

pub const SEED: u64 = 42;
pub const CACHE_SIZES: [usize; 3] = [100, 1_000, 10_000];
pub const KEY_SPACE_MULTIPLIER: usize = 10;
pub const OPS_PER_ITER: usize = 10_000;

pub fn generate_zipf_workload(n: usize, s: f64, num_keys: usize, seed: u64) -> Vec<u64> {
    let mut rng = StdRng::seed_from_u64(seed);
    let zipf = Zipf::new(num_keys as f64, s).unwrap();
    (0..n)
        .map(|_| {
            let v: f64 = zipf.sample(&mut rng);
            v as u64 - 1
        })
        .collect()
}

pub fn generate_uniform_workload(n: usize, num_keys: usize, seed: u64) -> Vec<u64> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..n)
        .map(|_| rng.random_range(0..num_keys as u64))
        .collect()
}

pub fn generate_mixed_ops(n: usize, read_pct: u8, seed: u64) -> Vec<bool> {
    let mut rng = StdRng::seed_from_u64(seed.wrapping_add(1));
    (0..n)
        .map(|_| rng.random_range(0..100u8) < read_pct)
        .collect()
}
