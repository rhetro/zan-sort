# zan-sort

`zan-sort` is a hardware-oriented hybrid sorting engine for Rust achieving near-linear scaling across both multi-core native environments and single-threaded WebAssembly (WASM) targets. 

**Underlying Assumption:**
Sorting is slow not because of algorithmic complexity, but because typical implementations fail to align with CPU hardware behavior. Once the ordering rule is reduced to a single numeric key, the limiting factor becomes memory access patterns, not comparisons. `zan-sort` is designed to saturate the hardware.

## Key Architecture

The algorithm utilizes an adaptive pipeline designed to align with hardware memory hierarchies and physical CPU boundaries:

1. **Disjoint Routing (N > 16,384 / L3 & DRAM Bound):** Based on the philosophy of ensuring non-overlapping memory access, `zan-sort` dynamically scales the number of buckets according to input size. It employs a BAM-inspired Dynamic Precision Scaling technique to map the key space into 32 bits, eliminating `u128` emulation overhead. 
    * **Parallel Mode:** Multiple threads perform concurrent scatter writes via heap-allocated write-combining buffers into a unified buffer without locks—achieved safely by computing disjoint global prefix-sum offsets beforehand.
    * **Sequential Mode (WASM):** Operates on a single thread using a zero-allocation `MacroWorkspace` to entirely avoid linear memory growth penalties, maximizing raw DRAM bandwidth.
2. **SoA Local Bucketing (5,000 < N <= 16,384 / L2 Cache Bound):** Maps elements into a local Structure of Arrays (`ChunkData` / `ChunkMeta`) using linear interpolation for O(1) routing. Collisions are resolved via bitwise operations (`trailing_zeros`) on the metadata bitmap. By keeping this SoA working set strictly within L2 cache boundaries (approx. 256KB–1MB), it entirely avoids main memory access penalties.
3. **L1 Cache Optimized Processing (16 < N <= 5,000):** When the dataset size fully fits within the L1 data cache (approx. 32KB), the fixed memory-allocation overhead of arithmetic routing exceeds the pure computational cost of comparative processing. In this zero-latency space, the engine transitions to `std::sort_unstable` to leverage highly optimized comparison-based logic.
4. **Register-Level Insertion Sort (N <= 16):** For micro-arrays, the engine utilizes raw `std::ptr::read` and `std::ptr::write` to encourage LLVM to perform register-level element shifting, reducing memory-to-memory copy overhead.
5. **Amortized Zero-Allocation (`Workspace`):** Internal processing utilizes a thread-local memory arena (`Workspace`). Buffers are allocated once per thread (or once globally in sequential mode) and reused, significantly reducing OS-level allocation lock contention.

## Hardware-Centric Philosophy

### DRAM Bandwidth Model: Single-Pass Routing

For large datasets (millions to hundreds of millions of elements), overall throughput is determined by DRAM bandwidth rather than computational complexity. 

* **Comparison-Based Sorting:** O(N log N) algorithms perform repeated memory traversals and branch-dependent access patterns. 
* **Multi-Pass Radix Sorting:** Parallel radix algorithms require 2–4 complete memory passes depending on radix width. Each pass consumes DRAM bandwidth independently, causing total runtime to scale proportionally to the number of passes.
* **Single-Pass Disjoint Routing:** `zan-sort` performs one global routing pass. Keys are projected into a 32-bit domain, prefix-sum offsets define disjoint write regions, and memory traffic follows the shortest path through the cache hierarchy.

## The Absolute Truth: The `SortKey` Trait

`zan-sort` abandons the `std::cmp::Ord` trait entirely. It achieves distribution by relying on a single, absolute source of truth: a `u64` value. 

### Out-of-the-Box Support for Primitives
`zan-sort` provides branchless `SortKey` implementations for:
- **Integers:** `u32`, `u64`, `i32`, `i64` (using XOR bit-flips for signed integers).
- **Floats:** `f32`, `f64` (using IEEE 754 bit-hack mapping via arithmetic right shifts to generate sign masks without branch-prediction stalls).

> **Note on Strings:** `zan-sort` does not implement `SortKey` for `String`. Users must define their own projection (e.g., prefix bytes, custom encoding).

## Usage

```toml
[dependencies]
zan-sort = "0.2.2"
```

### ⚠️ Important Note on External Parallel Executors (Rayon, etc.)
`zan-sort` does **not** integrate with dynamic work-stealing runtimes like Rayon. Its parallel architecture is strictly deterministic: thread count, memory topology, and disjoint write regions are all computed completely upfront via static prefix-sum routing.

Calling the default `zan_sort` inside any external parallel executor (e.g., Rayon's `par_iter_mut`) will cause severe thread explosion and destroy hardware cache locality.

If you are forced to operate within an external worker pool, or are targeting strictly single-threaded environments like WebAssembly, use the `sequential` feature flag. **This does not make `zan-sort` "compatible" with Rayon.** It simply disables `zan-sort`'s internal topology generation, forcing it to run deterministically on the current thread and ignoring the external dynamic executor entirely.

### Swap-based API (`zan_sort_into`)
For environments where zero-copy ownership transfers are preferred (like WebAssembly bindings), `zan-sort` provides a swap-based API to avoid unnecessary write-back copies:

```rust
# use zan_sort::core::zan_sort_into;
# let my_data_vec = vec![5, 2, 8, 1];
let sorted_data = zan_sort_into(my_data_vec);
```

### Sorting Custom Structs

```rust
use zan_sort::prelude::*;

#[derive(Debug)]
struct User {
    id: u32,
    name: String,
}

impl SortKey for User {
    fn sort_key(&self) -> u64 {
        let mut bytes = [0u8; 8];
        let name_bytes = self.name.as_bytes();
        let len = name_bytes.len().min(8);
        bytes[..len].copy_from_slice(&name_bytes[..len]);
        u64::from_be_bytes(bytes)
    }
}
```

## Feature Flags

* **Default (Practical Mode):** Safely falls back to `std::slice::sort_unstable_by_key` for 16 < N <= 5000.
* **`sequential`:** Disables OS-level threading (`std::thread`) and removes the `Send` bound. Enables the WASM-optimized single-pass macro routing using `MacroWorkspace`. Enforces a zero-allocation single-pass scatter optimized for WebAssembly, Rayon worker pools, and single-threaded environments.
* **`pure`:** Disables standard library fallbacks, relying purely on arithmetic routing for all sizes `N > 16`.

## Benchmark Results

## Benchmark Results

### 1. Single-Thread Large Payload Throughput (Linear Memory Optimization)
Evaluates single-threaded throughput targeting massive contiguous payloads (modeling an 8K RGBA image buffer, ~132MB continuous memory). Proves the architectural dominance of single-pass routing over O(N log N) cache-thrashing in constrained single-core and WebAssembly execution models.

* **Target:** 33,177,600 Elements (`RgbaPixel` Struct)
* **Resource Constraint:** Single Thread (`sequential` feature / `zan_sort_into`)

| Algorithm | Execution Time | vs std |
|:---|:---:|:---:|
| `std::sort_unstable_by_key` | ~663.67 ms | Baseline |
| **`zan-sort (sequential / into)`** | **~161.72 ms** | **+75.6% Faster (4.10x)** |

### 2. Parallel Architecture Scaling (Hardware Saturation)
Evaluates architectural superiority when completely restricted to the same CPU resources.

* **Target:** 100,000,000 Elements (`u32`, Highly randomized)
* **Resource Constraint:** Exactly 8 Cores Allocated

| Algorithm | Complexity | Paradigm | Time | vs Rayon | vs Radix |
|:---|:---:|:---:|:---:|:---:|:---:|
| `rayon::par_sort_unstable` | O(N log N) | Parallel Compare | 954 ms | Baseline | - |
| `parallel_radix_sort` | O(N) | Parallel MSD Radix | 1.33 s | -39.4% | - |
| **`zan-sort`** | **O(N)** | **Disjoint Parallel Routing** | **678 ms** | **+28.9%** | **+49.0%** |

### 3. Absolute Throughput (Standard Replacement)
Real-world performance gains replacing standard single-threaded sorting.

| Array Size (N) | `std::sort_unstable` | `Radix Sort` (Base 256) | `zan-sort` (Default) |
|:---:|:---:|:---:|:---:|
| 16 | 342 ns | - | **279 ns** |
| 10,000 | 194.43 µs | - | **184.95 µs** |
| 5,000,000 | 154.80 ms | 118.91 ms | **34.80 ms** |
| 100,000,000 | 3.49 s | 2.16 s | **683.4 ms** |

## Safety & Miri

All unsafe code paths (`ptr::read` / `ptr::write`) are covered by dedicated Miri tests verifying the absence of undefined behavior (use-after-free, double free, invalid pointer reads). The parallel routing phase operates entirely on disjoint prefix-sum offsets without shared mutable state, rendering it race-free by construction.
