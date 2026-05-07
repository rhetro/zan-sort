//! The `zan-sort` Prelude
//!
//! This module provides a convenient way to import the core traits and sorting
//! functions required to leverage the high-performance sorting engine.
//!
//! By glob-importing this module, you bring `SortKey` and the primary sorting
//! routines into scope immediately, enabling out-of-the-box usage.
//!
//! # Example
//!
//! ```rust
//! use zan_sort::prelude::*;
//!
//! let mut data = vec![99u32, 42, 1, 7, 3];
//! zan_sort(&mut data);
//! ```

pub use crate::core::{custom_insertion_sort, zan_sort, SortKey};
