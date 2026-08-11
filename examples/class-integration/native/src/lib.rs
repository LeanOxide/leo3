//! Native side of the `class-integration` example.
//!
//! BISECT build: `#[leanclass]` block removed to isolate a macOS startup
//! crash. Lean module + free functions remain.

use leo3::prelude::*;

/// Free functions registered as the `ClassIntegration.Native` Lean module.
#[leanmodule(name = "ClassIntegration.Native")]
#[allow(unused_imports)]
mod native {
    use leo3::prelude::*;

    /// Scalar-only export: both parameters and the result cross the FFI
    /// boundary unboxed.
    #[leanfn(name = "ci_add")]
    pub fn add(a: u64, b: u64) -> u64 {
        a + b
    }

    /// Mixed export: `String` crosses boxed, `i32` crosses unboxed.
    #[leanfn(name = "ci_banner")]
    pub fn banner(name: String, count: i32) -> String {
        format!("{name} has {count} ticks")
    }

    /// Container export: `Vec<u64>` maps to Lean's `Array UInt64`.
    #[leanfn(name = "ci_sum")]
    pub fn sum(values: Vec<u64>) -> u64 {
        values.iter().sum()
    }
}
