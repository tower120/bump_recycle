#![feature(allocator_api)]

use bumpalo::Bump;
use criterion::{criterion_group, criterion_main, Criterion};
use bump_recycle::ReBump;
use std::{alloc::Layout, hint::black_box};

fn generic_bench(count: usize){
    let layout = Layout::new::<[u64;4]>();
    for _ in 0..count{
        unsafe{ std::alloc::alloc(layout); }
    }
}

fn bumpalo_bench(bump: &mut Bump, count: usize){
    let mut vec: Vec<usize, &Bump> = Vec::new_in(bump);
    for i in 0..count{
        vec.push(i);
    }
    vec.clear();
    for i in 0..count{
        vec.push(i);
    }
}

fn rebump_bench(bump: &mut ReBump, count: usize){
    let mut vec: Vec<usize, &ReBump> = Vec::new_in(bump);
    for i in 0..count{
        vec.push(i);
    }
    vec.clear();
    for i in 0..count{
        vec.push(i);
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let count = 20_000;
    c.bench_function("rebump", |b| b.iter(|| rebump_bench(&mut ReBump::new(),black_box(count))));
    c.bench_function("bumpalo", |b| b.iter(|| bumpalo_bench(&mut Bump::new(), black_box(count))));
    // c.bench_function("generic", |b| b.iter(|| generic_bench(black_box(count))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);