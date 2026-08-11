//! IO operations for Lean4.
//!
//! This module provides safe wrappers around Lean4's IO primitives,
//! including file operations, environment variables, and console I/O.
//!
//! # Example
//!
//! ```rust,ignore
//! use leo3::prelude::*;
//! use leo3::io::fs;
//!
//! fn main() -> LeanResult<()> {
//!     leo3::prepare_freethreaded_lean();
//!
//!     leo3::with_lean(|lean| {
//!         // Read a file
//!         let content = fs::read_file(lean, "config.txt")?;
//!         println!("Content: {}", content);
//!
//!         // Write a file
//!         fs::write_file(lean, "output.txt", "Hello, Lean!")?;
//!
//!         Ok(())
//!     })
//! }
//! ```

use crate::conversion::{FromLean, IntoLean};
use crate::err::LeanResult;
use crate::ffi;
use crate::instance::{LeanAny, LeanBound};
use crate::marker::Lean;
use std::marker::PhantomData;

pub mod console;
pub mod env;
pub mod error;
pub mod fs;
pub mod handle;

// These modules use IO primitives that may not be available on all platforms
// On Windows, some IO primitives are not exported from the Lean DLLs
#[cfg(not(target_os = "windows"))]
pub mod process;
#[cfg(not(target_os = "windows"))]
pub mod time;

pub use error::{IOError, IOResult};

/// A Lean IO computation that produces a value of type `T`.
///
/// This corresponds to Lean4's `IO T` type. It represents a computation
/// that performs IO and produces a value of type `T`, or fails with an `IO.Error`.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::LeanIO;
///
/// fn example<'l>(lean: Lean<'l>) -> LeanResult<()> {
///     // Create an IO computation
///     let io_computation: LeanIO<String> = /* ... */;
///
///     // Execute it
///     let result: String = io_computation.run()?;
///
///     Ok(())
/// }
/// ```
#[repr(transparent)]
pub struct LeanIO<'l, T> {
    /// The underlying Lean IO object (IO T)
    inner: LeanBound<'l, LeanIOAny>,
    _phantom: PhantomData<T>,
}

impl<'l, T: 'l> LeanIO<'l, T> {
    /// Create a `LeanIO` from a Lean object.
    ///
    /// # Safety
    ///
    /// - `obj` must be a valid Lean IO object of type `IO T`
    #[inline]
    pub unsafe fn from_raw(obj: LeanBound<'l, LeanIOAny>) -> Self {
        LeanIO {
            inner: obj,
            _phantom: PhantomData,
        }
    }

    /// Lift a pure value into the IO monad.
    ///
    /// This corresponds to Lean's `pure` or `return` in the IO monad.
    /// Creates an IO computation that immediately returns the given value.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let io: LeanIO<i32> = LeanIO::pure(lean, 42)?;
    /// assert_eq!(io.run()?, 42);
    /// ```
    pub fn pure(lean: Lean<'l>, value: T) -> LeanResult<Self>
    where
        T: IntoLean<'l>,
    {
        unsafe {
            // Convert value to Lean
            let lean_value = value.into_lean(lean)?;

            // Create IO.pure function: λ _ => pure value
            // We need to create a closure that ignores the world token and returns (value, world)
            let pure_fn = create_io_pure(lean_value.into_ptr());

            Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, pure_fn)))
        }
    }

    /// Map a function over the result of an IO computation.
    ///
    /// This corresponds to Lean's `map` or `<$>` operator.
    /// Transforms the value produced by the IO computation.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let io: LeanIO<i32> = LeanIO::pure(lean, 21)?;
    /// let doubled: LeanIO<i32> = io.map(lean, |x| x * 2)?;
    /// assert_eq!(doubled.run()?, 42);
    /// ```
    pub fn map<U, F>(self, lean: Lean<'l>, f: F) -> LeanResult<LeanIO<'l, U>>
    where
        U: 'l,
        F: FnOnce(T) -> U + 'l,
        T: FromLean<'l>,
        U: IntoLean<'l>,
    {
        // Execute the IO computation
        let value = self.run()?;

        // Apply the function
        let result = f(value);

        // Wrap the result back into IO
        LeanIO::pure(lean, result)
    }

    /// Sequentially compose two IO computations, passing the result of the first to the second.
    ///
    /// This corresponds to Lean's `bind` or `>>=` operator.
    /// Chains IO computations together.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let io1: LeanIO<i32> = LeanIO::pure(lean, 21)?;
    /// let io2: LeanIO<String> = io1.bind(lean, |x| {
    ///     LeanIO::pure(lean, format!("Result: {}", x * 2))
    /// })?;
    /// assert_eq!(io2.run()?, "Result: 42");
    /// ```
    pub fn bind<U, F>(self, _lean: Lean<'l>, f: F) -> LeanResult<LeanIO<'l, U>>
    where
        U: 'l,
        F: FnOnce(T) -> LeanResult<LeanIO<'l, U>> + 'l,
        T: FromLean<'l>,
    {
        // Execute the first IO computation
        let value = self.run()?;

        // Apply the function to get the next IO computation
        f(value)
    }

    /// Sequence two IO computations, discarding the result of the first.
    ///
    /// This corresponds to Lean's `>>` operator.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let io1: LeanIO<()> = console::put_str_ln(lean, "Hello")?;
    /// let io2: LeanIO<i32> = LeanIO::pure(lean, 42)?;
    /// let result: LeanIO<i32> = io1.then(io2);
    /// ```
    pub fn then<U>(self, next: LeanIO<'l, U>) -> LeanIO<'l, U>
    where
        U: 'l,
    {
        unsafe {
            let lean = next.inner.lean_token();
            let io1_ptr = self.inner.into_ptr();
            let io2_ptr = next.inner.into_ptr();
            let seq_fn = create_io_seq(io1_ptr, io2_ptr);

            LeanIO::from_raw(LeanBound::from_owned_ptr(lean, seq_fn))
        }
    }

    /// Execute the IO computation and return the result.
    ///
    /// This corresponds to Lean's `unsafe_perform_io` or running IO in the main function.
    ///
    /// # Errors
    ///
    /// Returns an error if the IO computation fails.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let io: LeanIO<String> = /* ... */;
    /// let result: String = io.run()?;
    /// ```
    pub fn run(self) -> LeanResult<T>
    where
        T: FromLean<'l>,
    {
        let lean = self.inner.lean_token();

        // Execute IO with a RealWorld token
        unsafe {
            let world = ffi::io::lean_io_mk_world();
            let io_fn = self.inner.into_ptr();

            // Apply the IO function to the world token
            // IO functions have type: RealWorld → EStateM.Result IO.Error T
            // whose ok constructor is `ctor(0, 2, 0)` with fields
            // `(value, world)` — see `lean_io_result_mk_ok` in Lean's runtime.
            let result = ffi::closure::lean_apply_1(io_fn, world);

            // Check if result is an error
            if io_result_is_error(result) {
                // Extract error
                let err_ptr = io_result_get_error(result);
                let error = IOError::from_lean_io_error(lean, err_ptr);
                return Err(error.into());
            }

            // Extract the successful value: field 0 of the ok constructor.
            let value_ptr = ffi::object::lean_ctor_get(result, 0) as *mut ffi::lean_object;
            let value_bound: LeanBound<LeanAny> = LeanBound::from_owned_ptr(lean, value_ptr);

            // Convert to Rust type
            let typed: LeanBound<T::Source> = value_bound.cast();
            T::from_lean(&typed)
        }
    }

    /// Get the underlying Lean object.
    ///
    /// This consumes the `LeanIO` and returns the raw Lean object.
    #[inline]
    pub fn into_inner(self) -> LeanBound<'l, LeanIOAny> {
        self.inner
    }

    /// Get a reference to the underlying Lean object.
    #[inline]
    pub fn as_inner(&self) -> &LeanBound<'l, LeanIOAny> {
        &self.inner
    }
}

/// Marker type for IO computations.
///
/// This is used as the type parameter for `LeanBound` when wrapping IO objects.
pub struct LeanIOAny {
    _private: (),
}

// ============================================================================
// IO.Result helpers
// ============================================================================

/// Lean4 `IO.Result` is represented as `Except IO.Error (T × RealWorld)`.
/// `Except.ok` has tag 0 and one field (value), `Except.error` has tag 1 and one field (error).
#[inline]
unsafe fn io_result_is_error(res: *mut ffi::lean_object) -> bool {
    ffi::object::lean_obj_tag(res) != 0
}

#[inline]
unsafe fn io_result_get_error(res: *mut ffi::lean_object) -> *mut ffi::lean_object {
    ffi::object::lean_ctor_get(res, 0) as *mut ffi::lean_object
}

/// Build the ok result of an IO action: `ctor(0, 2, 0)` with fields
/// `(value, world)` — the exact shape of `lean_io_result_mk_ok` in Lean's
/// runtime.
///
/// Used by `create_io_pure` and by the pure-Rust IO implementations in
/// `io::time` / `io::process` (which compute their values at run time instead
/// of capturing them).
/// Box a `u64` in the heap representation `LeanUInt64` uses (ctor tag 0,
/// no object fields, value in scalar slot 0) — the same shape Lean's IO
/// primitives return for `UInt64` results and the shape `FromLean for u64`
/// (`lean_ctor_get_uint64`) expects.
pub(crate) unsafe fn box_u64(value: u64) -> *mut ffi::lean_object {
    let obj = ffi::lean_alloc_ctor(0, 0, 1);
    ffi::object::lean_ctor_set_uint64(obj, 0, value);
    obj
}

pub(crate) unsafe fn io_ok_value_world(
    value: *mut ffi::lean_object,
    world: ffi::object::lean_obj_arg,
) -> ffi::object::lean_obj_res {
    #[cfg(not(lean_4_26))]
    {
        let r = ffi::lean_alloc_ctor(0, 2, 0);
        ffi::object::lean_ctor_set(r, 0, value);
        ffi::object::lean_ctor_set(r, 1, world);
        r
    }
    #[cfg(lean_4_26)]
    {
        // Lean 4.26+ erased the world from `EStateM.Result`: the ok
        // constructor carries only the value.
        let _ = world;
        let r = ffi::lean_alloc_ctor(0, 1, 0);
        ffi::object::lean_ctor_set(r, 0, value);
        r
    }
}

// ============================================================================
// IO Monad Combinators Implementation
// ============================================================================

/// Create an IO computation that returns a pure value.
///
/// Creates: λ world => Except.ok (value, world)
unsafe fn create_io_pure(value: *mut ffi::lean_object) -> *mut ffi::lean_object {
    // Create a closure that returns (value, world)
    // We'll use a simple approach: create a Lean closure
    // For now, we'll create the result directly as it's simpler
    // In a full implementation, we'd create a proper closure

    // Create a closure that takes world and returns Except.ok (value, world)
    // This is a simplified version - in practice we'd need to properly construct the closure
    let closure = ffi::inline::lean_alloc_closure(io_pure_impl as *mut std::ffi::c_void, 2, 1);
    ffi::inline::lean_closure_set(closure, 0, value);
    closure
}

/// Implementation function for IO.pure
extern "C" fn io_pure_impl(
    value: ffi::object::lean_obj_arg,
    world: ffi::object::lean_obj_arg,
) -> ffi::object::lean_obj_res {
    unsafe { io_ok_value_world(value, world) }
}

/// Create an IO computation that sequences two IO computations.
unsafe fn create_io_seq(
    io1: *mut ffi::lean_object,
    io2: *mut ffi::lean_object,
) -> *mut ffi::lean_object {
    let closure = ffi::inline::lean_alloc_closure(io_seq_impl as *mut std::ffi::c_void, 3, 2);
    ffi::inline::lean_closure_set(closure, 0, io1);
    ffi::inline::lean_closure_set(closure, 1, io2);
    closure
}

/// Implementation function for IO sequencing
extern "C" fn io_seq_impl(
    io1: ffi::object::lean_obj_arg,
    io2: ffi::object::lean_obj_arg,
    world: ffi::object::lean_obj_arg,
) -> ffi::object::lean_obj_res {
    unsafe {
        // Execute first IO
        let result1 = ffi::closure::lean_apply_1(io1, world);

        // Check if it's an error
        if io_result_is_error(result1) {
            return result1;
        }

        // Execute the second IO with the (possibly threaded) world token.
        #[cfg(not(lean_4_26))]
        {
            // The ok constructor carries the world in field 1.
            let world_ptr = ffi::object::lean_ctor_get(result1, 1) as *mut ffi::lean_object;
            ffi::closure::lean_apply_1(io2, world_ptr)
        }
        #[cfg(lean_4_26)]
        {
            // 4.26+ erased the world from results; reuse the caller's token.
            let _ = result1;
            ffi::closure::lean_apply_1(io2, world)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_type_size() {
        // LeanIO should be pointer-sized
        assert_eq!(
            std::mem::size_of::<LeanIO<String>>(),
            std::mem::size_of::<*mut ()>()
        );
    }

    #[test]
    fn test_io_pure() {
        crate::prepare_freethreaded_lean();
        let _ = crate::with_lean(|lean| {
            let io = LeanIO::pure(lean, 42)?;
            let result = io.run()?;
            assert_eq!(result, 42);
            Ok::<(), crate::err::LeanError>(())
        });
    }

    #[test]
    fn test_io_map() {
        crate::prepare_freethreaded_lean();
        let _ = crate::with_lean(|lean| {
            let io = LeanIO::pure(lean, 21)?;
            let doubled = io.map(lean, |x| x * 2)?;
            let result = doubled.run()?;
            assert_eq!(result, 42);
            Ok::<(), crate::err::LeanError>(())
        });
    }

    #[test]
    fn test_io_map_string() {
        crate::prepare_freethreaded_lean();
        let _ = crate::with_lean(|lean| {
            let io = LeanIO::pure(lean, 21)?;
            let formatted = io.map(lean, |x| format!("Value: {}", x))?;
            let result = formatted.run()?;
            assert_eq!(result, "Value: 21");
            Ok::<(), crate::err::LeanError>(())
        });
    }

    #[test]
    fn test_io_bind() {
        crate::prepare_freethreaded_lean();
        let _ = crate::with_lean(|lean| {
            let io1 = LeanIO::pure(lean, 21)?;
            let io2 = io1.bind(lean, |x| LeanIO::pure(lean, x * 2))?;
            let result = io2.run()?;
            assert_eq!(result, 42);
            Ok::<(), crate::err::LeanError>(())
        });
    }

    #[test]
    fn test_io_bind_string() {
        crate::prepare_freethreaded_lean();
        let _ = crate::with_lean(|lean| {
            let io1 = LeanIO::pure(lean, 21)?;
            let io2 = io1.bind(lean, |x| LeanIO::pure(lean, format!("Result: {}", x * 2)))?;
            let result = io2.run()?;
            assert_eq!(result, "Result: 42");
            Ok::<(), crate::err::LeanError>(())
        });
    }

    #[test]
    fn test_io_map_chain() {
        crate::prepare_freethreaded_lean();
        let _ = crate::with_lean(|lean| {
            let io = LeanIO::pure(lean, 10)?;
            let result = io
                .map(lean, |x| x + 5)?
                .map(lean, |x| x * 2)?
                .map(lean, |x| x - 10)?
                .run()?;
            assert_eq!(result, 20); // (10 + 5) * 2 - 10 = 20
            Ok::<(), crate::err::LeanError>(())
        });
    }

    #[test]
    fn test_io_bind_chain() {
        crate::prepare_freethreaded_lean();
        let _ = crate::with_lean(|lean| {
            let io = LeanIO::pure(lean, 10)?;
            let result = io
                .bind(lean, |x| LeanIO::pure(lean, x + 5))?
                .bind(lean, |x| LeanIO::pure(lean, x * 2))?
                .bind(lean, |x| LeanIO::pure(lean, x - 10))?
                .run()?;
            assert_eq!(result, 20); // (10 + 5) * 2 - 10 = 20
            Ok::<(), crate::err::LeanError>(())
        });
    }
}
