use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use rayon::prelude::*;
use zan_sort::prelude::*;

// ==========================================
// Parallel MSD Radix Sort: O(N) baseline
// ==========================================
pub fn parallel_radix_sort(data: &mut [u32]) {
    msd_radix_sort_par(data, 3);
}

fn msd_radix_sort_par(data: &mut [u32], byte_idx: isize) {
    // Fallback to comparison sort on small subarrays or max depth
    if data.len() <= 32768 || byte_idx < 0 {
        data.par_sort_unstable();
        return;
    }

    // Step 1: Histogram construction
    let mut counts = [0_usize; 256];
    for &x in data.iter() {
        let b = ((x >> (byte_idx * 8)) & 0xFF) as usize;
        counts[b] += 1;
    }

    // Step 2: Prefix-sum offset calculation
    let mut offsets = [0_usize; 256];
    let mut sum = 0;
    for i in 0..256 {
        offsets[i] = sum;
        sum += counts[i];
    }

    // Step 3: Single-threaded scattering into auxiliary buffer
    let mut aux = vec![0_u32; data.len()];
    for &x in data.iter() {
        let b = ((x >> (byte_idx * 8)) & 0xFF) as usize;
        aux[offsets[b]] = x;
        offsets[b] += 1;
    }
    data.copy_from_slice(&aux);

    // Step 4: Subslice generation
    let mut slices = Vec::new();
    let mut rem = &mut data[..];
    for &count in counts.iter() {
        let (left, right) = rem.split_at_mut(count);
        if count > 0 {
            slices.push(left);
        }
        rem = right;
    }

    // Step 5: Recursive parallel sort via Rayon
    slices.into_par_iter().for_each(|slice| {
        msd_radix_sort_par(slice, byte_idx - 1);
    });
}

// ==========================================
// Test Data Generator (Xorshift)
// ==========================================
fn generate_random_data(size: usize) -> Vec<u32> {
    let mut seed = 0xdeadbeef_u32;
    let mut rand = || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed
    };
    (0..size).map(|_| rand() % 200_000_000).collect()
}

// ==========================================
// Benchmark Suite: Parallel Performance
// ==========================================
fn bench_parallel_performance(c: &mut Criterion) {
    let size = 100_000_000;

    // Hardware constraint: Execution assumes OS-level resource pinning (e.g., taskset)
    let mut group = c.benchmark_group(format!("Parallel Sort ({} elements)", size));
    group.sample_size(10);

    // Target 1: O(N log N) parallel comparison baseline
    group.bench_function("rayon::par_sort_unstable", |b| {
        b.iter_batched(
            || generate_random_data(size),
            |mut data| {
                data.par_sort_unstable();
                black_box(data);
            },
            BatchSize::LargeInput,
        )
    });

    // Target 2: O(N) parallel radix baseline
    group.bench_function("parallel_radix_sort", |b| {
        b.iter_batched(
            || generate_random_data(size),
            |mut data| {
                parallel_radix_sort(&mut data);
                black_box(data);
            },
            BatchSize::LargeInput,
        )
    });

    // Target 3: zan-sort (saturates available hardware parallelism internally)
    group.bench_function("zan_sort", |b| {
        b.iter_batched(
            || generate_random_data(size),
            |mut data| {
                zan_sort(&mut data);
                black_box(data);
            },
            BatchSize::LargeInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_parallel_performance);
criterion_main!(benches);
