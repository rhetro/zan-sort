use zan_sort::core::zan_sort_into;

#[cfg(feature = "sequential")]
use zan_sort::prelude::*;

// 1. Swap-based API (zan_sort_into) Tests
#[test]
fn test_zan_sort_into_basic() {
    let data = vec![99, 42, 1, 7, 3, 100, 50];
    // Take ownership and directly receive the sorted Vec
    let sorted = zan_sort_into(data);

    assert_eq!(sorted, vec![1, 3, 7, 42, 50, 99, 100]);
}

#[test]
fn test_zan_sort_into_macro_scale() {
    let size = 20_000;
    // Generate a reverse-ordered vector (Verify Macro Phase / N > 16384 execution)
    let data: Vec<u32> = (0..size).map(|i| (size - i) as u32).collect();

    let sorted = zan_sort_into(data);

    assert!(sorted.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(sorted.len(), size as usize);
}

// ==========================================
// 2. Sequential-mode only: !Send type sorting test
// ==========================================
// Verifies that structures containing `!Send` types (e.g., Rc) can be safely sorted
// in environments like WASM where multi-threading is not assumed.
#[cfg(feature = "sequential")]
#[test]
fn test_non_send_type_sorting() {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct NonSendItem {
        id: u32,
        // Include Rc to intentionally violate the T: Send bound
        _heavy_dom_node: Rc<RefCell<String>>,
    }

    impl SortKey for NonSendItem {
        fn sort_key(&self) -> u64 {
            self.id as u64
        }
    }

    let mut data = vec![
        NonSendItem {
            id: 42,
            _heavy_dom_node: Rc::new(RefCell::new("Node A".into())),
        },
        NonSendItem {
            id: 1,
            _heavy_dom_node: Rc::new(RefCell::new("Node B".into())),
        },
        NonSendItem {
            id: 7,
            _heavy_dom_node: Rc::new(RefCell::new("Node C".into())),
        },
    ];

    // Compiles successfully in sequential mode as T: Send is not required
    zan_sort(&mut data);

    assert_eq!(data[0].id, 1);
    assert_eq!(data[1].id, 7);
    assert_eq!(data[2].id, 42);
}

// ==========================================
// 3. Sequential-mode only: Large-scale zero-allocation verification
// ==========================================
#[cfg(feature = "sequential")]
#[test]
fn test_sequential_macro_routing_bounds() {
    // Verify that MacroWorkspace correctly performs routing and fallback
    // for sizes exceeding the threshold (16384).
    let size = 35_000;
    let mut data: Vec<u32> = (0..size).map(|i| (i * 17) % 100_000).collect();
    let mut expected = data.clone();

    expected.sort_unstable();
    zan_sort(&mut data);

    assert_eq!(data, expected);
}
