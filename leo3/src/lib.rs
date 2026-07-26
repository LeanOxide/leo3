#![cfg_attr(docsrs, feature(doc_cfg))]
//! # Leo3: Rust bindings for the Lean4 theorem prover
//!
//! Leo3 provides safe, ergonomic Rust bindings to Lean4, similar to how PyO3
//! provides Python bindings for Rust.
//!
//! ## Architecture
//!
//! Leo3 is organized into several layers:
//!
//! - **`leo3-ffi`**: Raw FFI bindings to Lean4's C API
//! - **`leo3-build-config`**: Build-time configuration and Lean4 detection
//! - **`leo3-macros`**: Procedural macros (`#[leanfn]`, etc.)
//! - **`leo3`** (this crate): Safe, high-level abstractions
//!
//! ## Core Concepts
//!
//! ### The `Lean<'l>` Token
//!
//! All interactions with Lean require a `Lean<'l>` token, which proves at
//! compile-time that the Lean runtime is initialized. This prevents accessing
//! Lean objects without proper initialization.
//!
//! ### Smart Pointers
//!
//! Leo3 provides several smart pointer types:
//!
//! - **`LeanBound<'l, T>`**: A Lean object bound to a `Lean<'l>` lifetime
//! - **`LeanRef<T>`**: An owned reference to a Lean object (can cross initialization boundaries)
//! - **`LeanBorrowed<'a, 'l, T>`**: A borrowed reference (zero-cost)
//! - **`LeanUnbound<T>`**: A thread-safe reference with automatic MT marking (`Send + Sync`)
//!
//! ### Reference Counting
//!
//! Lean4 uses reference counting for memory management. Leo3's smart pointers
//! automatically handle reference counting to prevent memory leaks and use-after-free.
//!
//! ### Thread Safety
//!
//! Lean4 uses a dual-mode reference counting system:
//! - **ST (Single-Threaded)**: Non-atomic, faster reference counting
//! - **MT (Multi-Threaded)**: Atomic reference counting for thread safety
//!
//! Use `LeanUnbound<T>` or `unbind_mt()` for objects that need to cross thread boundaries.
//!
//! ### Runtime Model
//!
//! Leo3 keeps Lean concurrency in three clearly separated lanes:
//!
//! - **Runtime worker**: `prepare_freethreaded_lean()` boots one long-lived
//!   worker thread that performs runtime/module initialization and serialized
//!   environment/meta operations.
//! - **Lean tasks**: `leo3::task` builds on Lean's native task manager for
//!   asynchronous computation after that worker has initialized the runtime.
//! - **User threads**: safe entry points such as `with_lean()` automatically
//!   attach the caller thread; lower-level code can attach explicitly with
//!   `leo3::sync::ensure_lean_thread()`.
//!
//! Blocking waits and polling-based waits are intentionally kept separate:
//! blocking APIs use Lean's native task waits, while combinators and futures
//! reuse one shared backoff-based polling helper for completion checks.
//!
//! ## Feature Flags
//!
//! Default features: **none**. The ungated core surface includes the runtime
//! token, smart pointers, type wrappers/conversions, closures, thunks, and
//! synchronization helpers. Optional subsystems are enabled explicitly:
//!
//! - **`macros`**: Procedural macros (`#[leanfn]`, `#[leanclass]`, `#[leanmodule]`)
//! - **`meta`**: `leo3::meta` metaprogramming APIs
//! - **`io`**: `leo3::io` helpers for Lean IO / filesystem / process utilities
//! - **`task`**: `leo3::task`, `leo3::task_combinators`, and `leo3::promise`
//! - **`module-loading`**: `leo3::module` dynamic shared-library loading
//! - **`tokio`**: Tokio bridge for Lean tasks (implies `task`)
//! - **`runtime-tests`**: Enables runtime-dependent integration tests
//!
//! ## Example
//!
//! ```rust,no_run
//! use leo3::prelude::*;
//!
//! fn main() -> LeanResult<()> {
//!     leo3::prepare_freethreaded_lean();
//!
//!     leo3::with_lean(|lean| {
//!         let n = LeanNat::from_usize(lean, 5)?;
//!         assert_eq!(LeanNat::to_usize(&n)?, 5);
//!         Ok(())
//!     })
//! }
//! ```

#![warn(missing_docs)]

pub use leo3_ffi as ffi;

#[cfg(doctest)]
#[doc = include_str!("../../README.md")]
mod readme_doctests {}

mod runtime;

pub mod closure;
pub mod conversion;
pub mod err;
pub mod external;
pub mod instance;
#[cfg(feature = "io")]
#[cfg_attr(docsrs, doc(cfg(feature = "io")))]
pub mod io;
pub mod marker;
#[cfg(feature = "meta")]
#[cfg_attr(docsrs, doc(cfg(feature = "meta")))]
pub mod meta;
#[cfg(feature = "module-loading")]
#[cfg_attr(docsrs, doc(cfg(feature = "module-loading")))]
pub mod module;
#[cfg(feature = "task")]
#[cfg_attr(docsrs, doc(cfg(feature = "task")))]
pub mod promise;
pub mod sync;
#[cfg(feature = "task")]
#[cfg_attr(docsrs, doc(cfg(feature = "task")))]
pub mod task;
#[cfg(feature = "task")]
#[cfg_attr(docsrs, doc(cfg(feature = "task")))]
pub mod task_combinators;
pub mod thunk;
pub mod types;
pub mod unbound;

#[cfg(feature = "tokio")]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
pub mod tokio_bridge;

// Re-export key types
pub use err::{KernelExceptionCode, LeanError, LeanResult};
pub use instance::{LeanBorrowed, LeanBound, LeanRef};
pub use marker::Lean;
pub use unbound::LeanUnbound;

#[cfg(feature = "macros")]
#[cfg_attr(docsrs, doc(cfg(feature = "macros")))]
pub use leo3_macros::{leanclass, leanfn, leanmodule, FromLean, IntoLean};

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::{Lean, LeanBorrowed, LeanBound, LeanError, LeanRef, LeanResult, LeanUnbound};

    // Re-export conversion traits and macros
    pub use crate::conversion::{FromLean, IntoLean};

    // Re-export smart conversion macros for automatic optimization
    pub use crate::{from_lean, to_lean};

    // Re-export commonly used types
    pub use crate::types::{
        LeanArray, LeanBool, LeanByteArray, LeanChar, LeanFloat, LeanFloat32, LeanISize, LeanInt,
        LeanInt16, LeanInt32, LeanInt64, LeanInt8, LeanList, LeanNat, LeanOption, LeanProd,
        LeanString, LeanUInt16, LeanUInt32, LeanUInt64, LeanUInt8, LeanUSize,
    };
    #[cfg(lean_4_22)]
    pub use crate::types::{LeanHashMap, LeanHashSet, LeanRBMap};

    #[cfg(feature = "macros")]
    pub use leo3_macros::{leanclass, leanfn, leanmodule, FromLean, IntoLean};

    // Re-export task combinators
    #[cfg(feature = "task")]
    pub use crate::task_combinators::{
        join, join_future, race, race_future, select, select_future, timeout, timeout_future,
        Either, JoinFuture, RaceFuture, SelectFuture, TimeoutError, TimeoutFuture,
    };

    // Re-export tokio bridge when enabled
    #[cfg(feature = "tokio")]
    pub use crate::tokio_bridge::lean_block_in_place;
}

/// Initialize the Lean runtime for standalone use.
///
/// This eagerly starts Leo3's long-lived runtime worker.
///
/// Safe caller-facing entry points such as [`with_lean`] already perform lazy
/// initialization as needed, so calling this function is optional. Use it when
/// you want to pay startup cost early or bootstrap runtime state before the
/// first `with_lean()` call.
///
/// This function is thread-safe and can be called multiple times - the
/// initialization will only happen once.
///
/// # Example
///
/// ```rust,no_run
/// use leo3::prelude::*;
///
/// fn main() -> LeanResult<()> {
///     leo3::prepare_freethreaded_lean();
///
///     leo3::with_lean(|lean| {
///         // Use Lean here
///         Ok(())
///     })
/// }
/// ```
pub fn prepare_freethreaded_lean() {
    // Spawn the long-lived worker thread and wait for it to complete all
    // Lean runtime initialization (lean_initialize_runtime_module,
    // lean_initialize_thread, and all ensure_*_initialized calls).
    //
    // This ensures that:
    // 1. All Lean module initialization happens on the worker thread
    // 2. The Once guards in ensure_*_initialized are triggered on the
    //    worker thread before any short-lived thread can trigger them
    // 3. lean_initialize_thread() is only called on the worker thread,
    //    which never exits, avoiding mimalloc's _mi_thread_done crash
    runtime::ensure_worker_initialized();
}

/// Execute a closure with access to the Lean runtime.
///
/// This provides a `Lean<'l>` token to the closure, which can be used to
/// create and manipulate Lean objects.
///
/// `with_lean()` is the canonical safe entry point for caller code. It ensures
/// both the shared runtime worker and the current thread are initialized before
/// constructing the `Lean<'l>` token.
///
/// # Example
///
/// ```rust,no_run
/// use leo3::prelude::*;
///
/// fn main() -> LeanResult<()> {
///     leo3::prepare_freethreaded_lean();
///
///     leo3::with_lean(|lean| {
///         let s = LeanString::mk(lean, "Hello, Lean!")?;
///         println!("{}", LeanString::cstr(&s)?);
///         Ok(())
///     })
/// }
/// ```
pub fn with_lean<F, R>(f: F) -> R
where
    F: for<'l> FnOnce(Lean<'l>) -> R,
{
    sync::ensure_lean_thread();
    let lean = unsafe { Lean::assume_initialized() };
    f(lean)
}

/// Execute a test with proper Lean thread lifecycle management.
///
/// This is specifically for testing - it ensures Lean is properly initialized
/// before running the test.
///
/// # Example
///
/// ```rust,no_run
/// use leo3::prelude::*;
///
/// fn main() -> LeanResult<()> {
///     leo3::test_with_lean(|lean| {
///         let s = LeanString::mk(lean, "Hello!")?;
///         assert!(!LeanString::cstr(&s)?.is_empty());
///         Ok(())
///     })
/// }
/// ```
pub fn test_with_lean<F, R>(f: F) -> R
where
    F: for<'l> FnOnce(Lean<'l>) -> R,
{
    with_lean(f)
}

/// Metadata about a Lean function (used by macros)
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeanPassingStyle {
    /// The wrapper owns the converted Rust value.
    Owned,
    /// The wrapper binds a Rust borrow to Lean-owned storage for the call.
    Borrowed,
}

/// Receiver metadata for a generated Lean binding.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeanBindingReceiver {
    /// A free function or static method with no receiver parameter.
    None,
    /// An `&self` receiver.
    Ref,
    /// An `&mut self` receiver.
    MutRef,
    /// A consuming `self` receiver.
    Owned,
}

/// High-level runtime semantics for a generated binding.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeanBindingSemantics {
    /// Plain input/output value semantics.
    Value,
    /// `&mut self` returning `Self`.
    MutatesSelf,
    /// `&mut self` returning `Prod Self R`.
    MutatesSelfWithValue,
}

/// The structured Lean-side shape known for a bound Rust type.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeanTypeShape {
    /// `()`
    Unit,
    /// Scalar primitives and similar direct-value wrappers.
    Scalar,
    /// Lean `String`.
    String,
    /// Lean `ByteArray`.
    ByteArray,
    /// Lean `Array`.
    Array,
    /// Lean `Option`.
    Option,
    /// Lean `Except`.
    Except,
    /// Lean `Prod`.
    Prod,
    /// A named Lean-visible type such as an external class.
    Named,
    /// The producer cannot currently lower this Rust type to a precise Lean shape.
    Unknown,
}

/// Structured type metadata for generated bindings.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeanTypeMetadata {
    /// Rust surface type as seen by the macro.
    pub rust: &'static str,
    /// Lean-visible type, when the producer can state it exactly.
    pub lean: Option<&'static str>,
    /// Structured type family for the binding.
    pub shape: LeanTypeShape,
}

/// Structured parameter metadata for generated bindings.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeanParameterMetadata {
    /// Parameter name in Rust.
    pub name: &'static str,
    /// Source/target type information for the parameter.
    pub ty: LeanTypeMetadata,
    /// Whether the wrapper binds the parameter as owned or borrowed Rust data.
    pub passing: LeanPassingStyle,
}

/// Structured metadata schema version for generated bindings.
#[doc(hidden)]
pub const LEO3_BINDING_SCHEMA_VERSION: u32 = 2;

/// Metadata about a Lean function (used by macros)
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeanFunctionMetadata {
    /// Metadata schema version.
    pub schema_version: u32,
    /// Rust function or method name.
    pub rust_name: &'static str,
    /// Name of the function in Lean.
    pub name: &'static str,
    /// Owning type name for methods, when applicable.
    pub owner: Option<&'static str>,
    /// Exported FFI symbol used by Lean.
    pub ffi_symbol: &'static str,
    /// Receiver style for class methods.
    pub receiver: LeanBindingReceiver,
    /// Parameter metadata in call order.
    pub params: &'static [LeanParameterMetadata],
    /// Return type metadata.
    pub return_type: LeanTypeMetadata,
    /// High-level call semantics for the binding.
    pub semantics: LeanBindingSemantics,
    /// Lean declaration text when the producer emits one directly.
    pub lean_decl: Option<&'static str>,
}

/// Metadata about a Lean module generated by `#[leanmodule]`.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeanModuleMetadata {
    /// Metadata schema version.
    pub schema_version: u32,
    /// Module name exposed to Lean.
    pub name: &'static str,
    /// Functions implicitly exported from the module.
    pub exports: &'static [LeanFunctionMetadata],
    /// Nested submodule registrations discovered from inner modules.
    pub submodules: &'static [LeanSubmoduleMetadata],
}

/// Metadata about a nested submodule discovered inside a `#[leanmodule]`.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeanSubmoduleMetadata {
    /// Metadata schema version.
    pub schema_version: u32,
    /// Dot-separated nested module path (e.g. `Foo.Bar`).
    pub path: &'static str,
    /// Functions exported from this submodule.
    pub exports: &'static [LeanFunctionMetadata],
}

/// Metadata about a Lean external class generated by `#[leanclass]`.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LeanClassMetadata {
    /// Metadata schema version.
    pub schema_version: u32,
    /// Rust struct name.
    pub rust_name: &'static str,
    /// Lean-visible class name.
    pub lean_name: &'static str,
    /// Generated Lean `opaque` declaration.
    pub opaque_decl: &'static str,
    /// Generated Lean method declarations.
    pub methods_decl: &'static str,
    /// Structured method metadata.
    pub methods: &'static [LeanFunctionMetadata],
}

/// Shared helpers used by generated macro code.
#[doc(hidden)]
pub mod __private {
    use std::any::Any;

    use crate::{ffi, types::LeanString, Lean};

    /// Convert a caught Rust panic payload into a readable message.
    pub fn panic_payload_message(payload: &(dyn Any + Send)) -> String {
        if let Some(message) = payload.downcast_ref::<&'static str>() {
            format!("Rust panic in FFI: {}", message)
        } else if let Some(message) = payload.downcast_ref::<String>() {
            format!("Rust panic in FFI: {}", message)
        } else {
            "Rust panic in FFI".to_string()
        }
    }

    /// Build a Lean panic object with a best-effort message payload.
    pub unsafe fn lean_panic_message<'l>(
        lean: Lean<'l>,
        message: &str,
    ) -> ffi::object::lean_obj_res {
        let msg = match LeanString::mk(lean, message) {
            Ok(value) => value.into_ptr(),
            Err(_) => ffi::inline::lean_box(0),
        };

        ffi::object::lean_panic_fn(ffi::inline::lean_box(0), msg)
    }

    /// Execute an object-returning FFI body behind a panic boundary.
    pub unsafe fn ffi_panic_boundary<F>(f: F) -> ffi::object::lean_obj_res
    where
        F: FnOnce() -> crate::LeanResult<ffi::object::lean_obj_res>,
    {
        match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(f)) {
            Ok(Ok(ptr)) => ptr,
            Ok(Err(err)) => {
                let lean = Lean::assume_initialized();
                lean_panic_message(lean, &err.to_string())
            }
            Err(payload) => {
                let lean = Lean::assume_initialized();
                let message = panic_payload_message(payload.as_ref());
                lean_panic_message(lean, &message)
            }
        }
    }

    /// Abort the process after reporting a scalar-returning FFI boundary error.
    pub fn abort_ffi_boundary(message: &str) -> ! {
        eprintln!("{message}");
        std::process::abort()
    }

    /// Execute a scalar-returning FFI body behind a panic boundary.
    ///
    /// Scalar-returning Lean extern signatures cannot encode a Lean panic object,
    /// so the only sound failure behavior is to terminate explicitly after
    /// reporting the boundary error.
    pub fn scalar_u64_ffi_panic_boundary<F>(symbol: &str, f: F) -> u64
    where
        F: FnOnce() -> crate::LeanResult<u64>,
    {
        match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(f)) {
            Ok(Ok(value)) => value,
            Ok(Err(err)) => {
                abort_ffi_boundary(&format!("fatal scalar FFI error in {symbol}: {err}"))
            }
            Err(payload) => {
                let message = panic_payload_message(payload.as_ref());
                abort_ffi_boundary(&format!("fatal scalar FFI panic in {symbol}: {message}"))
            }
        }
    }
}
