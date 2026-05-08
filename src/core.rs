use std::cmp;
use std::mem::MaybeUninit;
use std::thread;

/// The core trait of `zan-sort`.
/// It maps any arbitrary data type into a strictly ordered, 1-dimensional `u64` space.
/// In this algorithm, the `u64` representation is the absolute truth for ordering.
pub trait SortKey {
    fn sort_key(&self) -> u64;
}

// --- Primitive Type Implementations ---

impl SortKey for u32 {
    #[inline(always)]
    fn sort_key(&self) -> u64 {
        *self as u64
    }
}

impl SortKey for u64 {
    #[inline(always)]
    fn sort_key(&self) -> u64 {
        *self
    }
}

// --- Signed Integers ---
// Branchless two's complement mapping:
// Inverts the sign bit (XOR) to strictly align negative and positive values in the u64 space.
impl SortKey for i32 {
    #[inline(always)]
    fn sort_key(&self) -> u64 {
        (*self as u32 ^ 0x8000_0000) as u64
    }
}

impl SortKey for i64 {
    #[inline(always)]
    fn sort_key(&self) -> u64 {
        *self as u64 ^ 0x8000_0000_0000_0000
    }
}

// --- Floating-Point Numbers ---
// Branchless IEEE 754 bit-hack mapping:
// Uses arithmetic right shift (>>) to generate a sign mask without CPU branch-prediction stalls.
impl SortKey for f32 {
    #[inline(always)]
    fn sort_key(&self) -> u64 {
        let bits = self.to_bits();
        let sign_mask = ((bits as i32) >> 31) as u32;
        (bits ^ (sign_mask | 0x8000_0000)) as u64
    }
}

impl SortKey for f64 {
    #[inline(always)]
    fn sort_key(&self) -> u64 {
        let bits = self.to_bits();
        let sign_mask = ((bits as i64) >> 63) as u64;
        bits ^ (sign_mask | 0x8000_0000_0000_0000)
    }
}

/// Micro-optimized in-place insertion sort for extremely small arrays (N <= 16).
/// By utilizing raw `ptr::read` and `ptr::write` instead of `slice::swap`,
/// it coercers LLVM to perform register-level element shifting rather than memory-to-memory copies.
#[inline(always)]
pub fn custom_insertion_sort<T: SortKey>(arr: &mut [T]) {
    let len = arr.len();
    if len <= 1 {
        return;
    }

    let base_ptr = arr.as_mut_ptr();
    for i in 1..len {
        unsafe {
            let val_ptr = base_ptr.add(i);
            let val = std::ptr::read(val_ptr);
            let val_key = val.sort_key();
            let mut j = i;
            while j > 0 {
                let prev_ptr = base_ptr.add(j - 1);
                if (*prev_ptr).sort_key() > val_key {
                    std::ptr::write(base_ptr.add(j), std::ptr::read(prev_ptr));
                    j -= 1;
                } else {
                    break;
                }
            }
            std::ptr::write(base_ptr.add(j), val);
        }
    }
}

/// Helper function to sort overflow elements.
/// Similar to `custom_insertion_sort`, but specifically handles tuples containing chunk IDs.
#[inline(always)]
fn sort_overflow<T: SortKey>(arr: &mut [(usize, MaybeUninit<T>)]) {
    let len = arr.len();
    if len <= 1 {
        return;
    }

    let base_ptr = arr.as_mut_ptr();
    for i in 1..len {
        unsafe {
            let val_ptr = base_ptr.add(i);
            let val = std::ptr::read(val_ptr);
            let val_chunk = val.0;
            let val_key = val.1.assume_init_ref().sort_key();
            let mut j = i;
            while j > 0 {
                let prev_ptr = base_ptr.add(j - 1);
                let prev_chunk = (*prev_ptr).0;
                let prev_key = (*prev_ptr).1.assume_init_ref().sort_key();
                if prev_chunk > val_chunk || (prev_chunk == val_chunk && prev_key > val_key) {
                    std::ptr::write(base_ptr.add(j), std::ptr::read(prev_ptr));
                    j -= 1;
                } else {
                    break;
                }
            }
            std::ptr::write(base_ptr.add(j), val);
        }
    }
}

// --- Structure of Arrays (SoA) Definitions for the Micro/Mid Phase ---

#[repr(C, align(64))]
struct ChunkData<T> {
    data: [MaybeUninit<T>; 16],
}

impl<T> Default for ChunkData<T> {
    fn default() -> Self {
        unsafe { MaybeUninit::uninit().assume_init() }
    }
}

#[derive(Clone, Copy, Default)]
struct ChunkMeta {
    bitmap: u16,
    occupancy: u8,
    is_dirty: bool,
}

/// A thread-local, zero-allocation memory arena.
/// Avoids OS-level `malloc`/`free` calls within hot execution loops by reusing vectors.
struct Workspace<T> {
    datas: Vec<ChunkData<T>>,
    metas: Vec<ChunkMeta>,
    overflow: Vec<(usize, MaybeUninit<T>)>,
}

impl<T> Workspace<T> {
    fn new() -> Self {
        Self {
            datas: Vec::new(),
            metas: Vec::new(),
            overflow: Vec::new(),
        }
    }

    #[inline(always)]
    fn prepare(&mut self, c: usize) {
        self.metas.clear();
        self.metas.resize(c, ChunkMeta::default());
        self.datas.clear();
        self.datas.reserve(c);
        unsafe {
            self.datas.set_len(c);
        }
        self.overflow.clear();
    }
}

/// The core O(N) arithmetic routing algorithm for mid-scale localized processing.
/// Maps elements into an SoA structure (ChunkData / ChunkMeta) using 128-bit linear interpolation.
fn zan_sort_local<T: SortKey>(data: &mut [T], min_key: u64, max_key: u64, ws: &mut Workspace<T>) {
    let n = data.len();
    if n <= 1 {
        return;
    }
    let range = max_key.saturating_sub(min_key);
    if range == 0 {
        return;
    }

    let c = cmp::max(1, n / 4);
    let m = (c * 16 - 1) as u64;
    // Pre-calculate the routing multiplier to avoid division in the loop
    let multiplier = (((m as u128) << 32) / (range as u128)) as u64;

    ws.prepare(c);
    let metas = &mut ws.metas;
    let datas = &mut ws.datas;
    let overflow = &mut ws.overflow;

    // Phase 1: Arithmetic O(1) Routing
    for i in 0..n {
        unsafe {
            let v = std::ptr::read(data.as_ptr().add(i));
            let v_key = v.sort_key();
            let v_diff = v_key - min_key;
            let i_v = ((v_diff as u128 * multiplier as u128) >> 32) as usize;

            let chunk_id = cmp::min(i_v >> 4, c - 1);
            let offset = i_v & 15;

            let meta = &mut metas[chunk_id];
            let data_chunk = &mut datas[chunk_id];

            if meta.occupancy < 16 {
                let bit = 1 << offset;
                // Collision resolution using bit manipulation
                if (meta.bitmap & bit) == 0 {
                    data_chunk.data[offset].write(v);
                    meta.bitmap |= bit;
                    meta.occupancy += 1;
                } else {
                    meta.is_dirty = true;
                    let empty_offset = (!meta.bitmap).trailing_zeros() as usize;
                    data_chunk.data[empty_offset].write(v);
                    meta.bitmap |= 1 << empty_offset;
                    meta.occupancy += 1;
                }
            } else {
                // If a 16-element chunk is full, push to the fallback overflow buffer
                overflow.push((chunk_id, MaybeUninit::new(v)));
            }
        }
    }

    if overflow.len() > 1 {
        sort_overflow(overflow);
    }

    // Phase 2: Sequential Write-back & Micro-sorting
    let mut overflow_idx = 0;
    let mut write_ptr = 0;

    for id in 0..c {
        let meta = &metas[id];
        let data_chunk = &mut datas[id];
        let has_overflow = overflow_idx < overflow.len() && overflow[overflow_idx].0 == id;

        if meta.occupancy == 0 && !has_overflow {
            continue;
        }

        let mut local: [MaybeUninit<T>; 16] = unsafe { MaybeUninit::uninit().assume_init() };
        let mut local_len = 0;
        let mut bmp = meta.bitmap;

        // Extract occupied elements densely via trailing zeros
        while bmp != 0 {
            let offset = bmp.trailing_zeros() as usize;
            unsafe {
                local[local_len].write(data_chunk.data[offset].assume_init_read());
            }
            local_len += 1;
            bmp &= bmp - 1;
        }

        // Sort dirty chunks (where collisions forced elements out of exact alignment)
        if meta.is_dirty && local_len > 1 {
            unsafe {
                let slice = std::slice::from_raw_parts_mut(local.as_mut_ptr() as *mut T, local_len);
                custom_insertion_sort(slice);
            }
        }

        // Merge local chunk data with potential overflow elements, writing back to the original slice
        let base = data.as_mut_ptr();
        if !has_overflow {
            unsafe {
                let dst = base.add(write_ptr);
                let src = local.as_ptr() as *const T;
                std::ptr::copy_nonoverlapping(src, dst, local_len);
            }
            write_ptr += local_len;
        } else {
            let mut l_idx = 0;
            loop {
                let has_local = l_idx < local_len;
                let has_over = overflow_idx < overflow.len() && overflow[overflow_idx].0 == id;

                if has_local && has_over {
                    unsafe {
                        let l_key = (*(local.as_ptr().add(l_idx) as *const T)).sort_key();
                        let o_key = overflow[overflow_idx].1.assume_init_ref().sort_key();
                        if l_key <= o_key {
                            let l_val = local.as_ptr().add(l_idx).cast::<T>().read();
                            base.add(write_ptr).write(l_val);
                            l_idx += 1;
                        } else {
                            let o_val = overflow[overflow_idx].1.assume_init_read();
                            base.add(write_ptr).write(o_val);
                            overflow_idx += 1;
                        }
                    }
                    write_ptr += 1;
                } else if has_local {
                    unsafe {
                        let l_val = local.as_ptr().add(l_idx).cast::<T>().read();
                        base.add(write_ptr).write(l_val);
                    }
                    l_idx += 1;
                    write_ptr += 1;
                } else if has_over {
                    unsafe {
                        let o_val = overflow[overflow_idx].1.assume_init_read();
                        base.add(write_ptr).write(o_val);
                    }
                    overflow_idx += 1;
                    write_ptr += 1;
                } else {
                    break;
                }
            }
        }
    }
}

// --- Public API ---

/// High-performance, $O(N)$ generic hybrid sort.
/// It dynamically scales from standard `pdqsort` (for mid-sized arrays) up to
/// `Ordex`-inspired lock-free parallel arithmetic routing for massive arrays.
pub fn zan_sort<T: SortKey + Send>(data: &mut [T]) {
    let n = data.len();
    if n <= 1 {
        return;
    }

    // Dynamic Entry Point / Threshold Detection
    #[cfg(not(feature = "pure"))]
    {
        if n <= 16 {
            custom_insertion_sort(data);
            return;
        } else if n <= 5000 {
            data.sort_unstable_by_key(|item| item.sort_key());
            return;
        }
    }

    #[cfg(feature = "pure")]
    {
        if n <= 16 {
            custom_insertion_sort(data);
            return;
        }
    }

    // Determine the global bounds
    let mut min_key = u64::MAX;
    let mut max_key = u64::MIN;
    for item in data.iter() {
        let key = item.sort_key();
        if key < min_key {
            min_key = key;
        }
        if key > max_key {
            max_key = key;
        }
    }

    if min_key == max_key {
        return;
    }

    // Route mid-scale data to single-threaded SoA bucketing; larger datasets fall through to the parallel Macro Phase.
    if n <= 16384 {
        let mut ws = Workspace::new();
        zan_sort_local(data, min_key, max_key, &mut ws);
        return;
    }

    // --- Macro Phase: Dynamic Multi-Bucket Routing ---

    // Clamp minimum buckets to 16 to prevent over-partitioning
    let target_num_buckets = (n / 32768).next_power_of_two().clamp(16, 16384);
    let num_buckets = target_num_buckets;

    let range = max_key.saturating_sub(min_key);
    let shift_bits = if range > (u32::MAX as u64) {
        64 - range.leading_zeros() - 32
    } else {
        0
    };
    let scaled_range = range >> shift_bits;
    let multiplier = ((num_buckets as u64) << 32) / (scaled_range + 1);

    let num_threads = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let chunk_size = n.div_ceil(num_threads);

    // Step 1: Parallel Local Histograms
    let mut local_counts = vec![vec![0usize; num_buckets]; num_threads];
    thread::scope(|s| {
        for (chunk, counts) in data.chunks_mut(chunk_size).zip(local_counts.iter_mut()) {
            s.spawn(move || {
                for item in chunk {
                    let v_diff = item.sort_key() - min_key;
                    let scaled_diff = v_diff >> shift_bits;
                    let bucket = ((scaled_diff * multiplier) >> 32) as usize;
                    counts[bucket] += 1;
                }
            });
        }
    });

    // Step 2: Global Prefix Sums
    let mut bucket_offsets = vec![0usize; num_buckets];
    let mut local_offsets = vec![vec![0usize; num_buckets]; num_threads];
    let mut global_counts = vec![0usize; num_buckets];
    let mut sum = 0;

    for b in 0..num_buckets {
        bucket_offsets[b] = sum;
        for t in 0..num_threads {
            local_offsets[t][b] = sum;
            sum += local_counts[t][b];
            global_counts[b] += local_counts[t][b];
        }
    }

    let mut buffer: Vec<MaybeUninit<T>> = Vec::with_capacity(n);
    unsafe {
        buffer.set_len(n);
    }

    let data_ptr = data.as_mut_ptr() as usize;
    let buffer_ptr = buffer.as_mut_ptr() as usize;

    // Step 3: Lock-free Parallel Scatter with Heap-allocated Local Buffers
    thread::scope(|s| {
        for (t_id, mut offsets) in local_offsets.into_iter().enumerate() {
            let chunk_start = t_id * chunk_size;
            let chunk_end = cmp::min(chunk_start + chunk_size, n);

            s.spawn(move || unsafe {
                let d_ptr = data_ptr as *mut T;
                let b_ptr = buffer_ptr as *mut MaybeUninit<T>;

                const BUF_SIZE: usize = 16;
                // Allocate physical memory directly to bypass initialization overhead and Clone bounds.
                let mut local_buf: Vec<[MaybeUninit<T>; BUF_SIZE]> =
                    Vec::with_capacity(num_buckets);
                local_buf.set_len(num_buckets);

                let mut local_idx = vec![0usize; num_buckets];

                for i in chunk_start..chunk_end {
                    let v_ptr = d_ptr.add(i);
                    let v_key = (*v_ptr).sort_key();
                    let v_diff = v_key - min_key;
                    let scaled_diff = v_diff >> shift_bits;
                    let bucket = ((scaled_diff * multiplier) >> 32) as usize;

                    let idx = local_idx[bucket];
                    local_buf[bucket][idx] = std::ptr::read(v_ptr as *const MaybeUninit<T>);
                    local_idx[bucket] = idx + 1;

                    if idx + 1 == BUF_SIZE {
                        let dst = b_ptr.add(offsets[bucket]);
                        std::ptr::copy_nonoverlapping(local_buf[bucket].as_ptr(), dst, BUF_SIZE);
                        offsets[bucket] += BUF_SIZE;
                        local_idx[bucket] = 0;
                    }
                }

                for b in 0..num_buckets {
                    let remain = local_idx[b];
                    if remain > 0 {
                        let dst = b_ptr.add(offsets[b]);
                        std::ptr::copy_nonoverlapping(local_buf[b].as_ptr(), dst, remain);
                        offsets[b] += remain;
                    }
                }
            });
        }
    });

    // Step 4: Ahead-of-Time Allocation & Parallel Recursive Sort
    let buckets_per_thread = num_buckets.div_ceil(num_threads);

    let workspaces: Vec<Workspace<T>> = (0..num_threads)
        .map(|t_id| {
            let start_b = t_id * buckets_per_thread;
            let end_b = cmp::min(start_b + buckets_per_thread, num_buckets);
            let max_bucket_count = (start_b..end_b)
                .map(|b| global_counts[b])
                .max()
                .unwrap_or(0);

            let mut ws = Workspace::new();
            if max_bucket_count > 0 {
                ws.prepare(cmp::max(1, max_bucket_count / 4));
            }
            ws
        })
        .collect();

    let mut ws_iter = workspaces.into_iter();

    thread::scope(|s| {
        for t_id in 0..num_threads {
            let start_b = t_id * buckets_per_thread;
            let end_b = cmp::min(start_b + buckets_per_thread, num_buckets);
            #[allow(unused_mut, unused_variables)]
            let mut ws = ws_iter.next().unwrap();
            let g_counts = &global_counts;
            let b_offsets = &bucket_offsets;

            s.spawn(move || unsafe {
                let d_ptr = data_ptr as *mut T;
                let b_ptr = buffer_ptr as *mut MaybeUninit<T>;

                for b in start_b..end_b {
                    let count = g_counts[b];
                    if count == 0 {
                        continue;
                    }

                    let offset = b_offsets[b];
                    let block_ptr = b_ptr.add(offset) as *mut T;
                    let block = std::slice::from_raw_parts_mut(block_ptr, count);

                    if count <= 16 {
                        custom_insertion_sort(block);
                    } else {
                        #[cfg(not(feature = "pure"))]
                        {
                            if count <= 5000 {
                                block.sort_unstable_by_key(|item| item.sort_key());
                                std::ptr::copy_nonoverlapping(block_ptr, d_ptr.add(offset), count);
                                continue;
                            }
                        }

                        // Pure arithmetic routing fallback
                        let (mut l_min, mut l_max) = (u64::MAX, u64::MIN);
                        for item in block.iter() {
                            let key = item.sort_key();
                            if key < l_min {
                                l_min = key;
                            }
                            if key > l_max {
                                l_max = key;
                            }
                        }
                        if l_min != l_max {
                            zan_sort_local(block, l_min, l_max, &mut ws);
                        }
                    }

                    std::ptr::copy_nonoverlapping(block_ptr, d_ptr.add(offset), count);
                }
            });
        }
    });
}
