use zan_sort::core::zan_sort_into;
use zan_sort::prelude::*;

// ==========================================
// 1. スワップ型API (zan_sort_into) のテスト
// ==========================================
#[test]
fn test_zan_sort_into_basic() {
    let data = vec![99, 42, 1, 7, 3, 100, 50];
    // 所有権を渡して、ソート済みのVecを直接受け取る
    let sorted = zan_sort_into(data);

    assert_eq!(sorted, vec![1, 3, 7, 42, 50, 99, 100]);
}

#[test]
fn test_zan_sort_into_macro_scale() {
    let size = 20_000;
    // 逆順の配列を生成 (Macro Phase / N > 16384 の発動を確認)
    let data: Vec<u32> = (0..size).map(|i| (size - i) as u32).collect();

    let sorted = zan_sort_into(data);

    assert!(sorted.windows(2).all(|w| w[0] <= w[1]));
    assert_eq!(sorted.len(), size as usize);
}

// ==========================================
// 2. sequentialモード専用: !Send 型のソートテスト
// ==========================================
// WASM環境などスレッドを立てない前提において、
// Rc などの `!Send` な型が含まれる構造体を安全にソートできるか検証します。
#[cfg(feature = "sequential")]
#[test]
fn test_non_send_type_sorting() {
    use std::cell::RefCell;
    use std::rc::Rc;

    #[derive(Clone)]
    struct NonSendItem {
        id: u32,
        // Rc を含めることで、意図的に T: Send 制約を満たさなくする
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

    // sequentialモードでは T: Send が不要なためコンパイルが通る
    zan_sort(&mut data);

    assert_eq!(data[0].id, 1);
    assert_eq!(data[1].id, 7);
    assert_eq!(data[2].id, 42);
}

// ==========================================
// 3. sequentialモード専用: 大規模ゼロアロケーションの確認
// ==========================================
#[cfg(feature = "sequential")]
#[test]
fn test_sequential_macro_routing_bounds() {
    // 境界値(16384)を超えるサイズで、MacroWorkspace が正しく
    // ルーティングとフォールバックを行えるかを検証
    let size = 35_000;
    let mut data: Vec<u32> = (0..size).map(|i| (i * 17) % 100_000).collect();
    let mut expected = data.clone();

    expected.sort_unstable();
    zan_sort(&mut data);

    assert_eq!(data, expected);
}
