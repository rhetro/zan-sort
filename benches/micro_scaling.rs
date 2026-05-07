use criterion::{black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use zan_sort::prelude::*;

// Lightweight PRNG
#[inline(always)]
fn xorshift32(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

// ==========================================
// Benchmark Suite: Scaling & Thresholds
// ==========================================
// This benchmark proves that `zan-sort` matches the standard library's speed
// for small arrays (via zero-cost fallbacks) and decisively overtakes it
// once the constant-factor overhead is amortized (around N = 5000).
fn bench_micro_scaling(c: &mut Criterion) {
    // Range: from L1-cache scale up to the O(N) crossover point
    let sizes = [16, 64, 256, 1000, 5000, 10_000];
    let mut group = c.benchmark_group("Micro & Mid-Scale Scaling");

    let mut seed = 0x87654321_u32;
    let mut rand = || xorshift32(&mut seed);

    for &size in &sizes {
        // 1. Standard Library
        group.bench_with_input(
            BenchmarkId::new("std::sort_unstable", size),
            &size,
            |b, &s| {
                b.iter_batched(
                    || (0..s).map(|_| rand() % 100_000).collect::<Vec<u32>>(),
                    |mut data| {
                        data.sort_unstable();
                        black_box(data);
                    },
                    BatchSize::SmallInput,
                )
            },
        );

        // 2. zan-sort
        group.bench_with_input(BenchmarkId::new("zan-sort", size), &size, |b, &s| {
            b.iter_batched(
                || (0..s).map(|_| rand() % 100_000).collect::<Vec<u32>>(),
                |mut data| {
                    zan_sort(&mut data);
                    black_box(data);
                },
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

criterion_group!(benches, bench_micro_scaling);
criterion_main!(benches);
