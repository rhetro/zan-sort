use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use zan_sort::prelude::*;

// 1. Completely random (baseline)
fn gen_random(size: usize) -> Vec<u32> {
    let mut seed = 0xdeadbeef_u32;
    (0..size)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            seed
        })
        .collect()
}

// 2. Highly redundant identical key
fn gen_redundant(size: usize) -> Vec<u32> {
    (0..size).map(|i| (i / 5000) as u32).collect()
}

// 3. Skewed distribution (extreme concentration into specific buckets)
fn gen_skewed(size: usize) -> Vec<u32> {
    let mut seed = 0x87654321_u32;
    (0..size)
        .map(|i| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            // 90% concentration on the same value(42), with the remainder sparse
            if i % 10 == 0 {
                seed % 100_000
            } else {
                42
            }
        })
        .collect()
}

fn bench_fallback_robustness(c: &mut Criterion) {
    let size = 5_000_000;
    let mut group = c.benchmark_group("Fallback Robustness (5M elements)");
    group.sample_size(10);

    group.bench_function("Random Distribution", |b| {
        b.iter_batched(
            || gen_random(size),
            |mut d| zan_sort(&mut d),
            BatchSize::LargeInput,
        )
    });

    group.bench_function("Massive Redundant (Image-like)", |b| {
        b.iter_batched(
            || gen_redundant(size),
            |mut d| zan_sort(&mut d),
            BatchSize::LargeInput,
        )
    });

    group.bench_function("Skewed Distribution", |b| {
        b.iter_batched(
            || gen_skewed(size),
            |mut d| zan_sort(&mut d),
            BatchSize::LargeInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_fallback_robustness);
criterion_main!(benches);
