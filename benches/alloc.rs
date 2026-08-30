#![feature(slice_ptr_get)]

use bumpalo::Bump;
use criterion::{criterion_group, criterion_main, Criterion};
use recycled_bump::ReBump;
use std::{alloc::Layout, hint::black_box};


type Type = [u64;256];

#[repr(align(32))]
struct S(u8);

fn generic_bench(count: usize){
    let layout = Layout::new::<Type>();
    for _ in 0..count{
        let p = unsafe{ std::alloc::alloc(layout) };
        unsafe{
            p.write(0);
        }
    }
}

fn bumpalo_bench(bump: &mut Bump, count: usize){
    let layout = Layout::new::<Type>();
    for _ in 0..count{
        // bump.alloc([100u64;4]);
        let ptr = bump.alloc_layout(layout);
        unsafe{
            let p = ptr.as_ptr();
            p.write(0);
        }
    }
}

fn rebump_bench(bump: &mut ReBump, count: usize){
    let layout = Layout::new::<Type>();
    for _ in 0..count{
        let ptr = bump.allocate(layout).unwrap();
        unsafe{
            let p = ptr.as_mut_ptr();
            p.write(0);
        }
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    let count = 20_000;
    c.bench_function("rebump", |b| b.iter(|| rebump_bench(&mut ReBump::new(),black_box(count))));
    c.bench_function("bumpalo", |b| b.iter(|| bumpalo_bench(&mut Bump::new(), black_box(count))));
    c.bench_function("generic", |b| b.iter(|| generic_bench(black_box(count))));
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);