#![feature(slice_ptr_get)]

use bumpalo::Bump;
use criterion::{criterion_group, criterion_main, Criterion};
use bump_recycle::ReBump;
use std::{alloc::Layout, hint::black_box, ptr::NonNull};

fn bumpalo_bench<Type>(bump: &mut Bump, count: usize){
    let layout = Layout::new::<Type>();
    for _ in 0..count{
        // 1. Allocate
        let ptr = bump.alloc_layout(layout);
        unsafe{
            let p = ptr.as_ptr();
            p.write(0);
        }

        // 2. bump does not have deallocate - just skip this step.

        // 3. Allocate
        let ptr = bump.alloc_layout(layout);
        unsafe{
            let p = ptr.as_ptr();
            p.write(0);
        }
    }
}

fn rebump_bench<Type>(bump: &mut ReBump, count: usize){
    let layout = Layout::new::<Type>();
    for _ in 0..count{
        // 1. Allocate
        let ptr = bump.allocate(layout).unwrap();
        unsafe{
            let p = ptr.as_mut_ptr();
            p.write(0);
        }

        // 2. Deallocate
        unsafe{ bump.deallocate(NonNull::new_unchecked(ptr.as_mut_ptr()), layout); }

        // 1. Allocate
        let ptr = bump.allocate(layout).unwrap();
        unsafe{
            let p = ptr.as_mut_ptr();
            p.write(0);
        }
    }
}

fn criterion_benchmark(c: &mut Criterion) {
    fn bench_type<T>(c: &mut Criterion){
        let count = 20_000;
        c.bench_function(
            &format!("rebump {:}", std::any::type_name::<T>()),
            |b| b.iter(|| rebump_bench::<T>(&mut ReBump::new(),black_box(count))));
        c.bench_function(
            &format!("bumpalo {:}", std::any::type_name::<T>()),
            |b| b.iter(|| bumpalo_bench::<T>(&mut Bump::new(), black_box(count))));
    }

    bench_type::<[u64;1]>(c);
    bench_type::<[u64;2]>(c);
    bench_type::<[u64;4]>(c);
    bench_type::<[u64;8]>(c);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);