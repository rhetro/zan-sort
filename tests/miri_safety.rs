use std::fmt::Debug;
use zan_sort::prelude::*;

// ==========================================
// Soundness Verification Structures
// ==========================================

#[derive(Debug, Clone, PartialEq, Eq)]
struct DropItem {
    key: u32,
    // Heap allocation to trigger Miri on double-frees or memory leaks.
    _payload: Box<[u64; 4]>,
}

impl SortKey for DropItem {
    fn sort_key(&self) -> u64 {
        self.key as u64
    }
}

impl PartialOrd for DropItem {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DropItem {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key.cmp(&other.key)
    }
}

// Verifies sort correctness against the standard library baseline.
// Send bound is required for the parallel routing phase.
fn verify_sort<T: SortKey + Clone + Ord + Debug + Send>(mut data: Vec<T>) {
    let mut expected = data.clone();
    expected.sort_unstable();

    zan_sort(&mut data);

    assert_eq!(data, expected, "Sort failed for length {}", data.len());
}

// ==========================================
// Miri Safety Tests: Architectural Boundaries
// ==========================================

#[test]
fn miri_test_insertion_sort_boundary() {
    let sizes = [0, 1, 2, 15, 16, 17];
    for size in sizes {
        let data: Vec<u32> = (0..size).map(|i| (size - i) as u32).collect();
        verify_sort(data);
    }
}

#[test]
fn miri_test_drop_safety() {
    let size = 16;
    let data: Vec<DropItem> = (0..size)
        .map(|i| DropItem {
            key: (size - i) as u32,
            _payload: Box::new([i as u64; 4]),
        })
        .collect();
    verify_sort(data);
}

#[test]
fn miri_test_soa_bucketing_boundary() {
    let size = 5005;
    let data: Vec<u32> = (0..size).map(|i| (i * 17) % 1000).collect();
    verify_sort(data);
}

#[test]
fn miri_test_parallel_routing_boundary() {
    let size = 16385;
    let data: Vec<u32> = (0..size).map(|i| (i * 19) % 2000).collect();
    verify_sort(data);
}
