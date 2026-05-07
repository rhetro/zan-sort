use proptest::prelude::*;
use zan_sort::prelude::*;

// ==========================================
// Property Testing: Heavy Struct Generator
// ==========================================

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct FuzzHeavyItem {
    key: u32,
    payload: [u64; 4],
}

impl SortKey for FuzzHeavyItem {
    fn sort_key(&self) -> u64 {
        self.key as u64
    }
}

// Instructs proptest on how to generate completely randomized FuzzHeavyItem instances.
prop_compose! {
    fn heavy_item_strategy()(
        key in any::<u32>(),
        p0 in any::<u64>(), p1 in any::<u64>(), p2 in any::<u64>(), p3 in any::<u64>()
    ) -> FuzzHeavyItem {
        FuzzHeavyItem { key, payload: [p0, p1, p2, p3] }
    }
}

// ==========================================
// Randomized Property Tests (Fuzzing)
// ==========================================

proptest! {
    // Execute 1,000 highly randomized test cases per run
    #![proptest_config(ProptestConfig::with_cases(1000))]

    // 1. Fuzzing pure u32 arrays (Lengths from 0 to 50,000)
    #[test]
    fn proptest_u32_random_arrays(mut data in prop::collection::vec(any::<u32>(), 0..50_000)) {
        let mut expected = data.clone();
        expected.sort_unstable(); // Treat the standard library as the absolute source of truth

        zan_sort(&mut data);

        // Verify exact equivalence
        prop_assert_eq!(data, expected);
    }

    // 2. Fuzzing complex generic structs (Lengths from 0 to 10,000)
    #[test]
    fn proptest_heavy_struct_random_arrays(mut data in prop::collection::vec(heavy_item_strategy(), 0..10_000)) {
        let original_len = data.len();

        zan_sort(&mut data);

        // Verify that the array length remained perfectly intact (Zero memory loss)
        prop_assert_eq!(data.len(), original_len);

        // Verify the ascending order property based strictly on the SortKey
        if !data.is_empty() {
            for i in 0..(data.len() - 1) {
                prop_assert!(
                    data[i].key <= data[i + 1].key,
                    "Sort failed at index {}: {} is not <= {}",
                    i, data[i].key, data[i + 1].key
                );
            }
        }
    }
}
