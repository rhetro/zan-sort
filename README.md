# zan-sort

`zan-sort` is a hardware-oriented hybrid sorting engine for Rust achieving near-linear scaling. 

**Underlying Assumption:**
Sorting is slow not because of algorithmic complexity, but because typical implementations fail to align with CPU hardware behavior. Once the ordering rule is reduced to a single numeric key, the limiting factor becomes memory access patterns, not comparisons. `zan-sort` is designed to saturate the hardware.

## Key Architecture

The algorithm utilizes an adaptive pipeline designed to align with hardware memory hierarchies and physical CPU boundaries:

1. **Disjoint Parallel Routing (N > 16,384 / L3 & DRAM Bound):** Based on the philosophy of ensuring non-overlapping memory access, `zan-sort` dynamically scales the number of buckets according to input size. It employs a BAM-inspired Dynamic Precision Scaling technique to map the key space into 32 bits, eliminating `u128` emulation overhead. Multiple threads perform concurrent scatter writes via heap-allocated write-combining buffers (L1-aligned) into a unified buffer without locks or mutexes—**achieved safely by computing disjoint global prefix-sum offsets beforehand.**
2. **SoA Local Bucketing (5,000 < N <= 16,384 / L2 Cache Bound):** Maps elements into a local Structure of Arrays (`ChunkData` / `ChunkMeta`) using linear interpolation for O(1) routing. Collisions are resolved via bitwise operations (`trailing_zeros`) on the metadata bitmap. By keeping this SoA working set strictly within L2 cache boundaries (approx. 256KB–1MB), it entirely avoids main memory access penalties.
3. **L1 Cache Optimized Processing (16 < N <= 5,000):** When the dataset size fully fits within the L1 data cache (approx. 32KB), the fixed memory-allocation overhead of arithmetic routing exceeds the pure computational cost of comparative processing. In this zero-latency space, the engine transitions to `std::sort_unstable` to leverage highly optimized comparison-based logic.
4. **Register-Level Insertion Sort (N <= 16):** For micro-arrays, the engine utilizes raw `std::ptr::read` and `std::ptr::write` to encourage LLVM to perform register-level element shifting, reducing memory-to-memory copy overhead.
5. **Amortized Zero-Allocation (`Workspace`):** Internal processing utilizes a thread-local memory arena (`Workspace`). Buffers are allocated once per thread and reused, significantly reducing OS-level allocation lock contention during parallel execution.

## Hardware-Centric Philosophy

### DRAM Bandwidth Model: Single-Pass Routing

For large datasets (millions to hundreds of millions of elements), overall throughput is determined by DRAM bandwidth rather than computational complexity. In this regime, algorithms that require multiple full-memory passes cannot scale.

**Comparison-Based Sorting**
O(N log N) comparison algorithms perform repeated memory traversals and branch-dependent access patterns. Once DRAM bandwidth is saturated, additional comparisons do not contribute to throughput.

**Multi-Pass Radix Sorting**
Parallel radix algorithms require 2–4 complete memory passes depending on radix width. Each pass consumes DRAM bandwidth independently, causing total runtime to scale proportionally to the number of passes.

**Single-Pass Disjoint Routing**
`zan-sort` performs one global routing pass:
* Keys are projected into a 32-bit domain using Dynamic Precision Scaling
* Prefix-sum offsets define disjoint write regions for all threads
* Scatter operations use no shared mutable state
* Memory traffic follows the shortest path through the cache hierarchy
* DRAM bandwidth is saturated in a single pass

### Minimal Sufficient Structure

`zan-sort` is built around the minimal set of mechanisms required to saturate DRAM bandwidth under a single-key routing model. The architecture contains no auxiliary components beyond what is strictly necessary for full-bandwidth execution:

* One global routing pass
* A single prefix-sum phase
* Disjoint write regions
* **No** shared mutable state
* **No** atomics, locks, or synchronization barriers
* **No** multi-pass histogram construction
* **No** coordination between threads

This minimal structure is sufficient to reach the hardware limit. Any additional mechanism would not increase throughput, and any reduction would prevent bandwidth saturation.

### Sufficient as a Standalone Sorting Engine

`zan-sort` covers the full range of input sizes using a single routing architecture. No auxiliary sorting algorithms or external parallel frameworks are required:
* Micro-scale arrays use register-level insertion
* L1/L2-sized arrays use local bucketing or comparison fallback
* Large arrays use single-pass disjoint routing
* Parallel execution requires no custom thread orchestration
* No additional multi-pass or hybrid algorithms are needed

This structure makes `zan-sort` sufficient as a standalone sorting engine for datasets ranging from a handful of elements to hundreds of millions.

## The Absolute Truth: The `SortKey` Trait

`zan-sort` abandons the `std::cmp::Ord` trait entirely. It achieves distribution by relying on a single, absolute source of truth: a `u64` value. Any data type can be sorted as long as it implements the `SortKey` trait.

**This strict mapping guarantees O(N) routing behavior. There are no secondary comparative fallbacks.**

### Out-of-the-Box Support for Primitives
`zan-sort` provides branchless `SortKey` implementations for:
- **Integers:** `u32`, `u64`, `i32`, `i64` (using XOR bit-flips for signed integers).
- **Floats:** `f32`, `f64` (using IEEE 754 bit-hack mapping via arithmetic right shifts to generate sign masks without branch-prediction stalls).

> **Note on Strings:**
> `zan-sort` does not implement `SortKey` for `String`. Variable-length text cannot be mapped to a fixed 64-bit order without domain-specific rules. Users must define their own projection (e.g., prefix bytes, custom encoding).

## Usage

Add the dependency to your `Cargo.toml`:
```toml
[dependencies]
zan-sort = "0.1.0"
```

### ⚠️ Important Note on Threading
`zan-sort` automatically detects available CPU cores (`std::thread::available_parallelism()`) and aggressively saturates them via an internal parallel architecture. 
**Do NOT call `zan_sort` from within an external parallel iterator (e.g., Rayon's `par_iter_mut`).** Doing so will cause severe thread explosion, resulting in extreme context-switching overhead, performance degradation, and potential instability.

### Sorting Custom Structs
Due to Rust's Orphan Rule, implement `SortKey` on your custom structs. For complex data, you must manually encode multiple fields or prefix bytes into a single `u64`.

```rust
use zan_sort::prelude::*;

#[derive(Debug)]
struct User {
    id: u32,
    name: String,
}

impl SortKey for User {
    fn sort_key(&self) -> u64 {
        // Evaluate the first 8 bytes of the name as the absolute truth
        let mut bytes = [0u8; 8];
        let name_bytes = self.name.as_bytes();
        let len = name_bytes.len().min(8);
        bytes[..len].copy_from_slice(&name_bytes[..len]);
        u64::from_be_bytes(bytes)
    }
}

fn main() {
    let mut data = vec![
        User { id: 42, name: String::from("rust") },
        User { id: 7,  name: String::from("apple") },
        User { id: 99, name: String::from("zan") },
    ];

    zan_sort(&mut data);
}
```

### Performance Tip: Sorting Large Structs (Index Sorting)

`zan-sort` maintains its routing architecture strictly within CPU cache lines. Sorting massive structs (e.g., hundreds of bytes each) directly will saturate the cache with physical memory movement, degrading the engine's advantages.

For heavy payloads, **do not sort the data directly**. Extract a minimal index array, sort it, and use it to access your data.

```rust
use zan_sort::prelude::*;

struct HeavyPayload {
    id: u32,
    _data: [u64; 32],
}

#[derive(Clone, Copy)]
struct IndexPair {
    key: u64,
    original_idx: usize,
}

impl SortKey for IndexPair {
    fn sort_key(&self) -> u64 { self.key }
}

fn sort_heavy_data(payloads: &mut [HeavyPayload]) {
    let mut indices: Vec<IndexPair> = payloads
        .iter()
        .enumerate()
        .map(|(i, p)| IndexPair {
            key: p.id as u64,
            original_idx: i,
        })
        .collect();

    zan_sort(&mut indices);
    // Use `indices` to access payloads in sorted order.
}
```

## Feature Flags

* **Default (Practical Mode):** For 16 < N <= 5000, the engine safely falls back to `std::slice::sort_unstable_by_key` to avoid arithmetic routing overhead in the L1 cache.
* **`features = ["pure"]`:** Disables standard library fallbacks, relying purely on arithmetic routing for all sizes `N > 16` (`N <= 16` remains register-level insertion sort). Useful for extreme performance testing or specialized environments.

## Benchmark Results

* **Hardware:** Links International LC7430-16/512-W11Pro
  - CPU: AMD Ryzen 5 7430U (6C/12T, 2.30GHz)
  - RAM: 16GB DDR4-3200
  
### 1. Parallel Architecture Scaling (Hardware Saturation)
Evaluates architectural superiority when completely restricted to the same CPU resources (8 Cores via OS-level `taskset`). This proves that `zan-sort`'s performance is driven by memory optimization, not just thread count.

* **Target:** 100,000,000 Elements (`u32`, Highly randomized)
* **Resource Constraint:** Exactly 8 Cores Allocated

| Algorithm | Complexity | Paradigm | Time | vs Rayon | vs Radix |
|:---|:---:|:---:|:---:|:---:|:---:|
| `rayon::par_sort_unstable` | O(N log N) | Parallel Compare | 954 ms | Baseline | - |
| `parallel_radix_sort` | O(N) | Parallel MSD Radix | 1.33 s | -39.4% | - |
| **`zan-sort`** | **O(N)** | **Disjoint Parallel Routing** | **678 ms** | **+28.9%** | **+49.0%** |

### 2. Absolute Throughput (Standard Replacement)
Shows real-world performance gains when directly replacing standard single-threaded sorting on massive datasets.

| Array Size (N) | `std::sort_unstable` | `Radix Sort` (Base 256) | `zan-sort` (Default) |
|:---:|:---:|:---:|:---:|
| 16 | 342 ns | - | **279 ns** |
| 64 | **733 ns** | - | 735 ns |
| 1,000 | **15.69 µs** | - | 15.80 µs |
| 5,000 | 91.17 µs | - | **91.07 µs** |
| 10,000 | 194.43 µs | - | **184.95 µs** |
| 5,000,000 | 154.80 ms | 118.91 ms | **34.80 ms** |
| 100,000,000 | 3.49 s | 2.16 s | **683.4 ms** |

## Memory Usage Trade-off

`zan-sort` is not an in-place algorithm. It uses O(N) temporary buffers inside each thread (`Workspace`) to achieve lock-free parallel routing and DRAM-bandwidth saturation. This is a deliberate architectural trade-off: higher memory usage in exchange for significantly higher throughput on large datasets.

## Safety & Miri

`zan-sort` uses `unsafe` (`ptr::read` / `ptr::write`) in a few performance-critical paths. All unsafe code paths are covered by dedicated Miri tests:

```bash
MIRIFLAGS="-Zmiri-disable-isolation -Zmiri-permissive-provenance" cargo +nightly miri test --test miri_safety
```

Miri verifies the absence of undefined behavior (use-after-free, double free, invalid pointer reads) across all single-threaded executions of these paths. 

The parallel routing phase (N > 16,384) is executed under Miri’s threading model, which emulates interleaved multi-threaded execution. Miri does not run OS-level parallel threads, but it fully checks all memory accesses for undefined behavior. Since the algorithm uses disjoint prefix-sum offsets and never shares mutable state between threads, the parallel routing phase is race-free by construction.

## License

This project is licensed under either of

* Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or [http://www.apache.org/licenses/LICENSE-2.0](http://www.apache.org/licenses/LICENSE-2.0))
* MIT license ([LICENSE-MIT](LICENSE-MIT) or [http://opensource.org/licenses/MIT](http://opensource.org/licenses/MIT))

at your option.
