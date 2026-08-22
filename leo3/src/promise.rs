//! Safe wrapper for Lean promises (manually-resolvable async values).
//!
//! This module provides [`LeanPromise`], a safe wrapper around Lean's promise
//! objects that allow manual resolution of async computations.
//!
//! # Example
//!
//! ```rust,no_run
//! use leo3::prelude::*;
//! use leo3::promise::LeanPromise;
//! use leo3::task::{finalize_task_manager, init_task_manager};
//!
//! fn main() -> LeanResult<()> {
//!     leo3::prepare_freethreaded_lean();
//!     init_task_manager();
//!
//!     leo3::with_lean(|lean| {
//!         let promise = LeanPromise::<LeanNat>::new(lean)?;
//!         let task = promise.task();
//!         let value = LeanNat::from_usize(lean, 42)?;
//!         promise.resolve(value)?;
//!         assert_eq!(LeanNat::to_usize(&task.get_cloned())?, 42);
//!         Ok::<(), LeanError>(())
//!     })?;
//!
//!     finalize_task_manager();
//!     Ok(())
//! }
//! ```

use crate::err::{LeanError, LeanResult};
use crate::ffi;
use crate::instance::{LeanAny, LeanBound};
use crate::marker::Lean;
use crate::runtime::with_worker;
use crate::task::LeanTask;
use std::marker::PhantomData;

/// Marker type for Lean promise objects.
pub struct LeanPromiseType<T = LeanAny> {
    _marker: PhantomData<T>,
}

/// A safe wrapper around a Lean promise (manually-resolvable async value).
///
/// `LeanPromise` allows you to create an async computation that can be
/// resolved later with a value. This is useful for bridging synchronous
/// and asynchronous code, or for implementing custom async patterns.
///
/// # Example Flow
///
/// 1. Create a promise with [`LeanPromise::new`]
/// 2. Get the associated task with [`LeanPromise::task`]
/// 3. Pass the task to code that needs the result asynchronously
/// 4. When the value is ready, call [`LeanPromise::resolve`]
/// 5. The task automatically completes with the resolved value
///
/// # Memory Management
///
/// Like other Lean objects, promises use reference counting. The `LeanPromise`
/// wrapper automatically manages reference counts through [`LeanBound`].
///
/// # Important
///
/// - Each promise can only be resolved once
/// - Resolving consumes the promise
/// - The associated task will block until the promise is resolved
pub type LeanPromise<'l, T = LeanAny> = LeanBound<'l, LeanPromiseType<T>>;

impl<'l, T> LeanPromise<'l, T> {
    // ========================================================================
    // Type Checking
    // ========================================================================

    /// Check if a Lean object is a promise.
    ///
    /// Returns `true` if the object is a promise, `false` otherwise.
    #[inline]
    pub fn is_promise(obj: &LeanBound<'l, LeanAny>) -> bool {
        unsafe {
            let ptr = obj.as_ptr();
            !ffi::inline::lean_is_scalar(ptr) && ffi::inline::lean_is_promise(ptr)
        }
    }

    /// Try to convert a `LeanBound<LeanAny>` to a `LeanPromise`.
    ///
    /// Returns `Some(promise)` if the object is a promise, `None` otherwise.
    #[inline]
    pub fn try_from_any(obj: LeanBound<'l, LeanAny>) -> Option<Self> {
        if Self::is_promise(&obj) {
            Some(obj.cast())
        } else {
            None
        }
    }

    // ========================================================================
    // Creation
    // ========================================================================

    /// Create a new unresolved promise.
    ///
    /// The promise can be resolved later with [`resolve`](Self::resolve).
    ///
    /// # Errors
    ///
    /// Returns `LeanError` if the Lean task manager is not initialized or
    /// the IO operation fails.
    pub fn new(lean: Lean<'l>) -> LeanResult<Self> {
        let result_ptr = with_worker(move || unsafe {
            #[cfg(lean_4_26)]
            {
                // Lean >= 4.26: lean_io_promise_new takes no arguments and
                // returns the raw promise object directly (no IO wrapping).
                let result = ffi::closure::lean_io_promise_new();
                Ok::<_, LeanError>(result)
            }
            #[cfg(not(lean_4_26))]
            {
                // Lean < 4.26: lean_io_promise_new returns
                // IO (Except IO.Error (Promise α)).
                let world = ffi::io::lean_io_mk_world();
                let result = ffi::closure::lean_io_promise_new(world);
                if ffi::io::lean_io_result_is_ok(result) {
                    Ok(ffi::io::lean_io_result_take_value(result))
                } else {
                    ffi::lean_dec(result);
                    Err(LeanError::other(
                        "Promise::new: lean_io_promise_new failed (is the task manager initialized?)",
                    ))
                }
            }
        })?;
        Ok(unsafe { LeanBound::from_owned_ptr(lean, result_ptr) })
    }

    // ========================================================================
    // Resolution
    // ========================================================================

    /// Resolve this promise with a value.
    ///
    /// This completes the associated task with the given value.
    ///
    /// # Important
    ///
    /// - The value is consumed and transferred to the waiting task
    /// - The promise itself is consumed (it can only be resolved once)
    ///
    /// # Errors
    ///
    /// Returns `LeanError` if the IO operation fails.
    pub fn resolve(self, value: LeanBound<'l, T>) -> LeanResult<()> {
        let value_ptr = value.into_ptr();
        let promise_ptr = self.as_ptr();
        with_worker(move || unsafe {
            #[cfg(lean_4_26)]
            {
                // Lean >= 4.26: no world argument; returns lean_box(0)
                // (unit scalar), no IO wrapping.
                let result = ffi::closure::lean_io_promise_resolve(value_ptr, promise_ptr);
                let _ = result;
                Ok(())
            }
            #[cfg(not(lean_4_26))]
            {
                // Lean < 4.26: result is IO (Except IO.Error Unit).
                let world = ffi::io::lean_io_mk_world();
                // value is consumed, promise is borrowed
                let result = ffi::closure::lean_io_promise_resolve(value_ptr, promise_ptr, world);
                if ffi::io::lean_io_result_is_ok(result) {
                    ffi::lean_dec(result);
                    Ok(())
                } else {
                    ffi::lean_dec(result);
                    Err(LeanError::other(
                        "Promise::resolve: lean_io_promise_resolve failed",
                    ))
                }
            }
        })
    }

    // ========================================================================
    // Task Access
    // ========================================================================

    /// Get the task associated with this promise.
    ///
    /// The returned task will complete when this promise is resolved.
    /// Multiple calls return independent references to the same underlying task.
    pub fn task(&self) -> LeanTask<'l, T> {
        unsafe {
            // lean_io_promise_result_opt borrows the promise and returns an
            // owned reference to the underlying task (with lean_inc_ref
            // inside the C implementation).
            let lean = self.lean_token();
            let task_ptr = ffi::closure::lean_io_promise_result_opt(self.as_ptr());
            LeanBound::from_owned_ptr(lean, task_ptr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_promise_type_size() {
        // LeanPromise should be pointer-sized (same as LeanBound)
        assert_eq!(
            std::mem::size_of::<LeanPromise<LeanAny>>(),
            std::mem::size_of::<*mut ()>()
        );
    }
}
