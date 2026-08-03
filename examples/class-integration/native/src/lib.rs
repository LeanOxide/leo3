//! Native side of the `class-integration` example.
//!
//! Demonstrates Leo3's macro pipeline end to end:
//!
//! - `#[leanclass]` exposes `Account` as a Lean external object, with static
//!   constructors, `&self` / `&mut self` methods and `#[getter]` / `#[setter]`
//!   accessors;
//! - `#[leanfn]` exports free functions;
//! - `#[leanmodule]` registers them under the `ClassIntegration.Native`
//!   Lean module path.
//!
//! The macros embed structured binding metadata into the compiled cdylib.
//! `leo3-codegen` reads that metadata and emits the matching Lean `extern`
//! declarations (see `../gen-lean.sh`); nothing on the Lean side is written
//! by hand.

use leo3::prelude::*;

/// A minimal bank account used to demonstrate external objects.
#[derive(Clone)]
#[leanclass]
pub struct Account {
    owner: String,
    balance: i64,
}

#[leanclass]
impl Account {
    /// Static constructor. Lean sees `String → Account`.
    pub fn new(owner: String) -> Self {
        Account { owner, balance: 0 }
    }

    /// Shared accessor. Lean sees `Account → Int64` (unboxed result).
    #[getter]
    pub fn balance(&self) -> i64 {
        self.balance
    }

    /// Shared accessor returning an owned string.
    #[getter]
    pub fn owner(&self) -> String {
        self.owner.clone()
    }

    /// Mutating accessor. Lean sees `Account → Int64 → Account`;
    /// the update is copy-on-write from Lean's pure point of view.
    #[setter]
    pub fn set_balance(&mut self, balance: i64) {
        self.balance = balance;
    }

    /// `&mut self` returning `()`: Lean sees `Account → Int64 → Account`.
    pub fn deposit(&mut self, amount: i64) {
        self.balance += amount;
    }

    /// `&mut self` returning a value: Lean sees
    /// `Account → Int64 → Prod Account Bool`, so the updated object and the
    /// result are both preserved.
    pub fn withdraw(&mut self, amount: i64) -> bool {
        if amount > 0 && amount <= self.balance {
            self.balance -= amount;
            true
        } else {
            false
        }
    }

    /// `&self` returning a formatted string.
    pub fn describe(&self) -> String {
        format!("{} (balance: {})", self.owner, self.balance)
    }
}

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
