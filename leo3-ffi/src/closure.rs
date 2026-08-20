//! FFI bindings for Lean4 closure, thunk, and task operations
//!
//! Based on the closure/thunk/task functions from lean.h
//!
//! Note: `lean_alloc_closure` and related closure accessor functions are
//! implemented as inline functions in `inline.rs` (matching Lean4's static
//! inline implementations), rather than as extern declarations.

use crate::object::{b_lean_obj_arg, b_lean_obj_res, lean_obj_arg, lean_obj_res};
use libc::c_uint;

// ============================================================================
// Closure Functions
// ============================================================================
//
// NOTE: lean_alloc_closure, lean_closure_get, lean_closure_set, and other
// closure accessor functions are static inline in Lean4's lean.h.
// They are implemented in inline.rs.
//
// The lean_apply_* functions below ARE exported from libleanshared.so.

extern "C" {
    /// Apply a closure to an argument
    ///
    /// # Safety
    /// - `c` must be a valid closure object (consumed)
    /// - `a` is the argument to apply (consumed)
    pub fn lean_apply_1(c: lean_obj_arg, a: lean_obj_arg) -> lean_obj_res;

    /// Apply a closure to two arguments
    ///
    /// # Safety
    /// - `c` must be a valid closure object (consumed)
    /// - Both arguments are consumed
    pub fn lean_apply_2(c: lean_obj_arg, a1: lean_obj_arg, a2: lean_obj_arg) -> lean_obj_res;

    /// Apply a closure to three arguments
    pub fn lean_apply_3(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to four arguments
    pub fn lean_apply_4(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
        a4: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to five arguments
    pub fn lean_apply_5(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
        a4: lean_obj_arg,
        a5: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to six arguments
    pub fn lean_apply_6(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
        a4: lean_obj_arg,
        a5: lean_obj_arg,
        a6: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to seven arguments
    pub fn lean_apply_7(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
        a4: lean_obj_arg,
        a5: lean_obj_arg,
        a6: lean_obj_arg,
        a7: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to eight arguments
    pub fn lean_apply_8(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
        a4: lean_obj_arg,
        a5: lean_obj_arg,
        a6: lean_obj_arg,
        a7: lean_obj_arg,
        a8: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to nine arguments
    pub fn lean_apply_9(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
        a4: lean_obj_arg,
        a5: lean_obj_arg,
        a6: lean_obj_arg,
        a7: lean_obj_arg,
        a8: lean_obj_arg,
        a9: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to ten arguments
    pub fn lean_apply_10(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
        a4: lean_obj_arg,
        a5: lean_obj_arg,
        a6: lean_obj_arg,
        a7: lean_obj_arg,
        a8: lean_obj_arg,
        a9: lean_obj_arg,
        a10: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to eleven arguments
    pub fn lean_apply_11(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
        a4: lean_obj_arg,
        a5: lean_obj_arg,
        a6: lean_obj_arg,
        a7: lean_obj_arg,
        a8: lean_obj_arg,
        a9: lean_obj_arg,
        a10: lean_obj_arg,
        a11: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to twelve arguments
    pub fn lean_apply_12(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
        a4: lean_obj_arg,
        a5: lean_obj_arg,
        a6: lean_obj_arg,
        a7: lean_obj_arg,
        a8: lean_obj_arg,
        a9: lean_obj_arg,
        a10: lean_obj_arg,
        a11: lean_obj_arg,
        a12: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to thirteen arguments
    pub fn lean_apply_13(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
        a4: lean_obj_arg,
        a5: lean_obj_arg,
        a6: lean_obj_arg,
        a7: lean_obj_arg,
        a8: lean_obj_arg,
        a9: lean_obj_arg,
        a10: lean_obj_arg,
        a11: lean_obj_arg,
        a12: lean_obj_arg,
        a13: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to fourteen arguments
    pub fn lean_apply_14(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
        a4: lean_obj_arg,
        a5: lean_obj_arg,
        a6: lean_obj_arg,
        a7: lean_obj_arg,
        a8: lean_obj_arg,
        a9: lean_obj_arg,
        a10: lean_obj_arg,
        a11: lean_obj_arg,
        a12: lean_obj_arg,
        a13: lean_obj_arg,
        a14: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to fifteen arguments
    pub fn lean_apply_15(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
        a4: lean_obj_arg,
        a5: lean_obj_arg,
        a6: lean_obj_arg,
        a7: lean_obj_arg,
        a8: lean_obj_arg,
        a9: lean_obj_arg,
        a10: lean_obj_arg,
        a11: lean_obj_arg,
        a12: lean_obj_arg,
        a13: lean_obj_arg,
        a14: lean_obj_arg,
        a15: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to sixteen arguments
    pub fn lean_apply_16(
        c: lean_obj_arg,
        a1: lean_obj_arg,
        a2: lean_obj_arg,
        a3: lean_obj_arg,
        a4: lean_obj_arg,
        a5: lean_obj_arg,
        a6: lean_obj_arg,
        a7: lean_obj_arg,
        a8: lean_obj_arg,
        a9: lean_obj_arg,
        a10: lean_obj_arg,
        a11: lean_obj_arg,
        a12: lean_obj_arg,
        a13: lean_obj_arg,
        a14: lean_obj_arg,
        a15: lean_obj_arg,
        a16: lean_obj_arg,
    ) -> lean_obj_res;

    /// Apply a closure to n arguments (general case)
    ///
    /// # Safety
    /// - `c` must be a valid closure object (consumed)
    /// - `args` must be an array of `n` valid lean objects
    pub fn lean_apply_n(c: lean_obj_arg, n: c_uint, args: *mut lean_obj_arg) -> lean_obj_res;

    /// Apply a closure to multiple arguments
    ///
    /// # Safety
    /// - `c` must be a valid closure object (consumed)
    /// - `args` must be an array of `n` valid lean objects
    pub fn lean_apply_m(c: lean_obj_arg, n: c_uint, args: *mut lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// Thunk Functions
// ============================================================================
//
// NOTE: Most thunk functions are static inline in lean.h and are NOT exported
// from libleanshared.so. Only lean_thunk_get_core is exported. The inline
// implementations are provided in inline.rs:
// - lean_mk_thunk (creates a thunk from a closure)
// - lean_thunk_pure (creates a pre-evaluated thunk)
// - lean_thunk_get (gets value, forcing evaluation if needed)
// - lean_thunk_get_own (gets owned value)
//
// The extern declaration for lean_thunk_get_core is in inline.rs since it's
// used by the inline implementations.

// ============================================================================
// Task Functions (Async/Parallel Computation)
// ============================================================================

extern "C" {
    /// Initialize the task manager with default number of workers
    ///
    /// Must be called before using task functions
    pub fn lean_init_task_manager();

    /// Initialize the task manager with a specific number of workers
    ///
    /// # Safety
    /// - `num_workers` should be > 0
    pub fn lean_init_task_manager_using(num_workers: c_uint);

    /// Finalize the task manager
    ///
    /// Should be called before program exit
    pub fn lean_finalize_task_manager();

    /// Spawn a new task (core implementation)
    ///
    /// # Safety
    /// - `c` must be a valid closure object (consumed)
    /// - `prio` is the task priority
    /// - `keep_alive` indicates if the task should prevent thread pool shutdown
    pub fn lean_task_spawn_core(c: lean_obj_arg, prio: c_uint, keep_alive: bool) -> lean_obj_res;

    /// Get task result (blocking if not finished)
    ///
    /// # Safety
    /// - `t` must be a valid task object
    pub fn lean_task_get(t: b_lean_obj_arg) -> b_lean_obj_res;

    /// Map a function over a task (core implementation)
    ///
    /// # Safety
    /// - `f` must be a valid closure object (consumed)
    /// - `t` must be a valid task object (consumed)
    /// - `prio` is the priority for the new task
    /// - `sync` indicates if the task should be forced synchronously
    /// - `keep_alive` indicates if the task should prevent thread pool shutdown
    pub fn lean_task_map_core(
        f: lean_obj_arg,
        t: lean_obj_arg,
        prio: c_uint,
        sync: bool,
        keep_alive: bool,
    ) -> lean_obj_res;

    /// Bind a function over a task (core implementation)
    ///
    /// # Safety
    /// - `t` must be a valid task object (consumed)
    /// - `f` must be a valid closure object (consumed)
    /// - `prio` is the priority for the new task
    /// - `sync` indicates if the task should be forced synchronously
    /// - `keep_alive` indicates if the task should prevent thread pool shutdown
    pub fn lean_task_bind_core(
        t: lean_obj_arg,
        f: lean_obj_arg,
        prio: c_uint,
        sync: bool,
        keep_alive: bool,
    ) -> lean_obj_res;

    /// Create a pure task (already completed)
    ///
    /// # Safety
    /// - `v` is the value to wrap (consumed)
    pub fn lean_task_pure(v: lean_obj_arg) -> lean_obj_res;

    /// Check if cancellation was requested
    ///
    /// Returns true if a cancellation is pending
    pub fn lean_io_check_canceled_core() -> bool;

    /// Cancel a task
    ///
    /// # Safety
    /// - `t` must be a valid task object
    pub fn lean_io_cancel_core(t: b_lean_obj_arg);

    /// Get task state
    ///
    /// # Safety
    /// - `t` must be a valid task object
    ///   Returns: 0 = waiting, 1 = running, 2 = finished
    ///   (`LEAN_TASK_STATE_WAITING/RUNNING/FINISHED` in lean.h)
    pub fn lean_io_get_task_state_core(t: b_lean_obj_arg) -> u8;

}

// ============================================================================
// Promise Functions (IO-wrapped, exported from libleanshared)
// ============================================================================
//
// NOTE: The raw `lean_promise_new` / `lean_promise_resolve` are NOT exported
// from libleanshared.so, but the IO-wrapped versions below ARE exported with
// `LEAN_EXPORT` and can be called directly.

extern "C" {
    /// Create a new unresolved promise (IO-wrapped).
    ///
    /// Returns `IO (Except IO.Error (Promise α))`.
    /// The `obj_arg` parameter is the RealWorld token (consumed).
    ///
    /// # Safety
    /// - Lean task manager must be initialized
    pub fn lean_io_promise_new(world: lean_obj_arg) -> lean_obj_res;

    /// Resolve a promise with a value (IO-wrapped).
    ///
    /// Returns `IO (Except IO.Error Unit)`.
    ///
    /// # Safety
    /// - `value` is consumed (ownership transferred to the promise)
    /// - `promise` is borrowed
    /// - `world` is the RealWorld token (consumed)
    pub fn lean_io_promise_resolve(
        value: lean_obj_arg,
        promise: b_lean_obj_arg,
        world: lean_obj_arg,
    ) -> lean_obj_res;

    /// Get the task associated with a promise.
    ///
    /// Returns the `m_result` task object from the promise. The returned
    /// task object has its refcount incremented (caller owns the reference).
    ///
    /// # Safety
    /// - `promise` is borrowed (NOT consumed)
    pub fn lean_io_promise_result_opt(promise: b_lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// Reference Objects (Mutable Cells)
// ============================================================================

extern "C" {
    /// Allocate a reference object
    ///
    /// # Safety
    /// - `v` is the initial value (consumed)
    pub fn lean_alloc_ref(v: lean_obj_arg) -> lean_obj_res;

    /// Get the value from a reference
    ///
    /// # Safety
    /// - `r` must be a valid ref object
    pub fn lean_ref_get(r: b_lean_obj_arg) -> b_lean_obj_res;

    /// Set the value of a reference
    ///
    /// # Safety
    /// - `r` must be a valid ref object (consumed)
    /// - `v` is the new value (consumed)
    pub fn lean_ref_set(r: lean_obj_arg, v: lean_obj_arg) -> lean_obj_res;
}
