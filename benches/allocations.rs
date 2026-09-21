mod support;

use stats_alloc::{Region, StatsAlloc, INSTRUMENTED_SYSTEM};
use std::alloc::System;
use std::collections::hash_map::RandomState;
use std::hint::black_box;
use support::*;

#[global_allocator]
static ALLOCATOR: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;

fn check<C: Cache<String, RandomState>>() {
    let mut c = C::new(1024, RandomState::new());
    let keys: Vec<_> = (0..1024).map(|i| String::make(i, 24)).collect();
    for (i, key) in keys.iter().enumerate() {
        c.insert(key.clone(), i as u64);
    }
    let region = Region::new(ALLOCATOR);
    for i in 0..10000 {
        black_box(c.load(&keys[i % keys.len()]));
    }
    let hits = region.change();
    println!(
        "{},load-hit,{},{}",
        C::NAME,
        hits.allocations,
        hits.bytes_allocated
    );
    let region = Region::new(ALLOCATOR);
    c.clear();
    let clear = region.change();
    println!(
        "{},clear,{},{}",
        C::NAME,
        clear.allocations,
        clear.bytes_allocated
    );
    if C::NAME == "micro" {
        assert_eq!(hits.allocations, 0);
    }
    if C::NAME == "baseline" {
        assert_eq!(hits.allocations, 10000);
        assert_eq!(clear.allocations, 2);
    }
}

fn main() {
    println!("library,operation,allocations,bytes_allocated");
    check::<Current<_, _>>();
    check::<Baseline<_, _>>();
    check::<Quick<_, _>>();
    check::<Lru<_, _>>();
    check::<Hashlink<_, _>>();
}
