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
/// BISECT: leanclass block removed for macOS crash isolation.
