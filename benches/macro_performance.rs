use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use zan_sort::prelude::*;

// ==========================================
// Baseline Competitor: Safe Radix Sort (O(N))
// ==========================================
// A highly optimized, safe implementation of Base-256 LSD Radix Sort.
// Used as the absolute theoretical baseline for O(N) memory bandwidth saturation.
fn radix_sort_safe(data: &mut [u32]) {
    let n = data.len();
    if n <= 1 {
        return;
    }
    let mut buffer = vec![0u32; n];

    // 4 passes for 32-bit integers (Base 256)
    radix_pass(data, &mut buffer, 0);
    radix_pass(&buffer, data, 8);
    radix_pass(data, &mut buffer, 16);
    radix_pass(&buffer, data, 24); // Finally writes back to the original `data` slice
}

#[inline(always)]
fn radix_pass(source: &[u32], dest: &mut [u32], shift: u32) {
    let mut counts = [0usize; 256];
    for &v in source {
        counts[((v >> shift) & 0xFF) as usize] += 1;
    }

    let mut prefix_sums = [0usize; 256];
    let mut sum = 0;
    for i in 0..256 {
        prefix_sums[i] = sum;
        sum += counts[i];
    }

    for &v in source {
        let bucket = ((v >> shift) & 0xFF) as usize;
        dest[prefix_sums[bucket]] = v;
        prefix_sums[bucket] += 1;
    }
}

// ==========================================
// Test Data Generator
// ==========================================
// Uses a lightweight Xorshift PRNG to avoid external dependencies and overhead.
fn generate_random_data(size: usize, modulo: u32) -> Vec<u32> {
    let mut seed = 0xdeadbeef_u32;
    let mut rand = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    (0..size).map(|_| rand() % modulo).collect()
}

// ==========================================
// Benchmark Suite
// ==========================================
fn bench_macro_performance(c: &mut Criterion) {
    // Array sizes and their respective random upper bounds (modulo)
    let targets = [
        (5_000_000, 50_000_000),    // Formerly macro_performance
        (100_000_000, 200_000_000), // Formerly huge_performance
    ];

    for &(size, modulo) in &targets {
        let mut group = c.benchmark_group(format!("Macro-Scale Sort ({} elements)", size));

        // Reduce sample size due to the massive array processing time
        group.sample_size(10);

        // 1. Standard Library (O(N log N) Baseline)
        group.bench_function("std::sort_unstable", |b| {
            b.iter_batched(
                || generate_random_data(size, modulo),
                |mut data| {
                    data.sort_unstable();
                    black_box(data);
                },
                BatchSize::LargeInput,
            )
        });

        // 2. Safe Radix Sort (O(N) Competitor)
        group.bench_function("Radix Sort (Base 256)", |b| {
            b.iter_batched(
                || generate_random_data(size, modulo),
                |mut data| {
                    radix_sort_safe(&mut data);
                    black_box(data);
                },
                BatchSize::LargeInput,
            )
        });

        // 3. zan-sort (The O(N) Disjoint Parallel Engine)
        group.bench_function("zan-sort", |b| {
            b.iter_batched(
                || generate_random_data(size, modulo),
                |mut data| {
                    zan_sort(&mut data);
                    black_box(data);
                },
                BatchSize::LargeInput,
            )
        });

        group.finish();
    }
}

criterion_group!(benches, bench_macro_performance);
criterion_main!(benches);
