use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use zan_sort::prelude::*;

// ==========================================
// 1. Basic & Edge Case Tests
// ==========================================

#[test]
fn test_basic_sort() {
    let mut data = [5, 3, 8, 1, 9, 2, 7, 4, 6];
    zan_sort(&mut data);
    assert_eq!(data, [1, 2, 3, 4, 5, 6, 7, 8, 9]);
}

#[test]
fn test_empty_and_single() {
    let mut empty: [u32; 0] = [];
    zan_sort(&mut empty);
    assert_eq!(empty, []);

    let mut single = [42];
    zan_sort(&mut single);
    assert_eq!(single, [42]);
}

#[test]
fn test_identical_elements() {
    let mut data = vec![7u32; 100_000];
    zan_sort(&mut data);
    assert!(data.iter().all(|&x| x == 7));
}

#[test]
fn test_already_sorted_and_reversed() {
    let mut sorted_data: Vec<u32> = (0..100_000).collect();
    zan_sort(&mut sorted_data);
    assert!(sorted_data.windows(2).all(|w| w[0] <= w[1]));

    let mut reversed_data: Vec<u32> = (0..100_000).rev().collect();
    zan_sort(&mut reversed_data);
    assert!(reversed_data.windows(2).all(|w| w[0] <= w[1]));
}

#[test]
fn test_extreme_values() {
    let mut data = [u32::MAX, 0, 1, u32::MAX - 1, 500];
    let mut expected = data;
    expected.sort_unstable();

    zan_sort(&mut data);
    assert_eq!(data, expected);
}

#[test]
fn test_overflow_and_dirty_recovery() {
    // Tests the internal overflow buffer mechanism.
    // Pushing 20 reverse-ordered elements forces the 16-element SoA chunk
    // to overflow and triggers the internal dirty chunk recovery logic.
    let mut data: Vec<u32> = (1..=20).rev().collect();
    let expected: Vec<u32> = (1..=20).collect();

    zan_sort(&mut data);
    assert_eq!(data, expected);
}

// ==========================================
// 2. Generic Struct Sorting Tests
// ==========================================

// A rudimentary random number generator (Xorshift) for isolated testing without external crates.
fn xorshift32(seed: &mut u32) -> u32 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 17;
    *seed ^= *seed << 5;
    *seed
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HeavyItem {
    id: u32,
    name: String,
    payload: [u64; 4], // Simulating a memory-heavy struct
}

impl SortKey for HeavyItem {
    fn sort_key(&self) -> u64 {
        self.id as u64
    }
}

#[test]
fn test_heavy_struct_sort() {
    let mut seed = 0x87654321;
    let mut data: Vec<HeavyItem> = (0..5000)
        .map(|_| {
            let id = xorshift32(&mut seed) % 100_000;
            HeavyItem {
                id,
                name: format!("Item-{}", id),
                payload: [id as u64; 4],
            }
        })
        .collect();

    let mut expected = data.clone();
    expected.sort_unstable_by_key(|item| item.id);

    zan_sort(&mut data);
    assert!(data.windows(2).all(|w| w[0].id <= w[1].id));
}

// ==========================================
// 3. Memory Safety & Strict Drop Tracking
// ==========================================

#[derive(Debug)]
struct DropTracker {
    key: u32,
    drop_count: Arc<AtomicUsize>,
}

impl SortKey for DropTracker {
    fn sort_key(&self) -> u64 {
        self.key as u64
    }
}

impl Drop for DropTracker {
    fn drop(&mut self) {
        self.drop_count.fetch_add(1, Ordering::SeqCst);
    }
}

// Custom clone implementation to prepare test data safely.
impl Clone for DropTracker {
    fn clone(&self) -> Self {
        Self {
            key: self.key,
            drop_count: Arc::clone(&self.drop_count),
        }
    }
}

#[test]
fn test_memory_safety_drop_counts() {
    let drop_count = Arc::new(AtomicUsize::new(0));
    let num_elements = 50_000;

    {
        let mut seed = 0x11223344;
        let mut data: Vec<DropTracker> = (0..num_elements)
            .map(|_| DropTracker {
                key: xorshift32(&mut seed) % 1000,
                drop_count: Arc::clone(&drop_count),
            })
            .collect();

        // Perform the sort. This heavily utilizes unsafe pointer transfers.
        zan_sort(&mut data);

        // Verify the sort correctness.
        assert!(data.windows(2).all(|w| w[0].key <= w[1].key));

        // CRITICAL: No elements should have been dropped during the sorting process.
        // A non-zero count here indicates a memory leak or an unsafe double-free.
        assert_eq!(
            drop_count.load(Ordering::SeqCst),
            0,
            "Elements were dropped during the sort! (Memory leak or double free risk detected)"
        );
    } // `data` goes out of scope here, triggering normal drops.

    // Ensure that exactly `num_elements` were dropped, proving mathematical memory safety.
    assert_eq!(
        drop_count.load(Ordering::SeqCst),
        num_elements,
        "Drop count mismatch! Expected exactly {} drops.",
        num_elements
    );
}
