use criterion::{black_box, criterion_group, criterion_main, BatchSize, Criterion};
use zan_sort::core::zan_sort_into;
use zan_sort::prelude::*;

// WASM環境でよく扱われるRGBAピクセルデータを模した構造体
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C)]
struct RgbaPixel {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

// 輝度（Luminance）や特定の色チャンネルを絶対的なソートキーとする
impl SortKey for RgbaPixel {
    #[inline(always)]
    fn sort_key(&self) -> u64 {
        // 例: (R + G + B)の合計値をキーにする（簡易的な輝度ソート）
        (self.r as u64) + (self.g as u64) + (self.b as u64)
    }
}

// 軽量なXorshift PRNG
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

fn bench_wasm_simulation(c: &mut Criterion) {
    // 8K解像度相当 (約3300万ピクセル、約132MBの連続メモリ)
    let size = 33_177_600;
    let mut group = c.benchmark_group(format!("WASM Simulation ({} Pixels)", size));
    group.sample_size(10);

    // 1. 標準ライブラリ (O(N log N) / キャッシュミス多発)
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

    // 2. zan-sort (Sequential / Zero-allocation 単一パス)
    // WASMでの利用を想定し、zan_sort_into で所有権ごと処理する
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

criterion_group!(benches, bench_wasm_simulation);
criterion_main!(benches);
