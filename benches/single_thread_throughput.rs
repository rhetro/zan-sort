use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use zan_sort::core::zan_sort_into;
use zan_sort::prelude::*;

// Struct modeling RGBA pixel data commonly handled in WASM environments
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
struct RgbaPixel {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

// Uses luminance or specific color channels as an absolute sort key
impl SortKey for RgbaPixel {
    #[inline(always)]
    fn sort_key(&self) -> u64 {
        // Example: Use the sum of (R + G + B) as the key (simplified luminance sort)
        (self.r as u64) + (self.g as u64) + (self.b as u64)
    }
}

// Lightweight Xorshift PRNG
fn xorshift32(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

fn generate_image_buffer(pixel_count: usize) -> Vec<RgbaPixel> {
    let mut seed = 0xdeadbeef_u32;
    (0..pixel_count)
        .map(|_| {
            let val = xorshift32(&mut seed);
            RgbaPixel {
                r: (val & 0xFF) as u8,
                g: ((val >> 8) & 0xFF) as u8,
                b: ((val >> 16) & 0xFF) as u8,
                a: 255,
            }
        })
        .collect()
}

fn bench_single_thread_throughput(c: &mut Criterion) {
    // Equivalent to 8K resolution (~33.17 million pixels, ~132MB contiguous memory)
    let size = 33_177_600;
    let mut group = c.benchmark_group(format!("Single-Thread Throughput ({} Pixels)", size));
    group.sample_size(10);

    // 1. Standard library (O(N log N) / frequent cache misses)
    group.bench_function("std::sort_unstable_by_key", |b| {
        b.iter_batched(
            || generate_image_buffer(size),
            |mut data| {
                data.sort_unstable_by_key(|p| p.sort_key());
                black_box(data);
            },
            BatchSize::LargeInput,
        )
    });

    // 2. zan-sort (Sequential / Zero-allocation single pass)
    // Designed for WASM usage, taking full ownership via zan_sort_into
    group.bench_function("zan-sort (sequential / into)", |b| {
        b.iter_batched(
            || generate_image_buffer(size),
            |data| {
                let sorted = zan_sort_into(data);
                black_box(sorted);
            },
            BatchSize::LargeInput,
        )
    });

    group.finish();
}

criterion_group!(benches, bench_single_thread_throughput);
criterion_main!(benches);
