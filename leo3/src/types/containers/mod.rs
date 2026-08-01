//! Container types for Lean4 standard library collections.
//!
//! This module provides Rust wrappers for Lean4's standard library container types,
//! including HashMap, RBMap (Red-Black Map), and HashSet.
//!
//! These wrappers use Lean's real runtime representation for an explicit, narrow
//! key matrix (`LeanNat`, `LeanInt`, `LeanString`, `LeanInt8`–`LeanInt64`,
//! and `LeanUInt8`–`LeanUInt64`).
//! The surface is available on the default feature set and requires Lean >= 4.22
//! (the `lean_4_22` cfg).

pub mod hashmap;
pub mod hashset;
pub mod rbmap;
mod symbols;

pub use hashmap::LeanHashMap;
pub use hashset::LeanHashSet;
pub use rbmap::LeanRBMap;
