//! # zan-sort
//!
//! A generic, hardware-optimized hybrid sorting library for Rust.
//! By abandoning traditional comparative algorithms in favor of O(N) arithmetic
//! routing and an `Ordex`-inspired parallel disjoint memory architecture,
//! it pushes modern DRAM bandwidth to its physical limits.

#![doc = include_str!("../README.md")]

pub mod core;
pub mod prelude;
