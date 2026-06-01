//! Safe wrapper for Lean closures.
//!
//! This module provides [`LeanClosure`], a safe wrapper around Lean's closure
//! objects that supports inspection, application, and creation.
//!
//! # Example
//!
//! ```rust,ignore
//! use leo3::prelude::*;
//! use leo3::closure::LeanClosure;
//!
//! fn example<'l>(lean: Lean<'l>, closure: LeanClosure<'l>) -> LeanResult<()> {
//!     // Check arity
//!     println!("Arity: {}", closure.arity());
//!     println!("Remaining: {}", closure.remaining_arity());
//!
//!     // Apply arguments
//!     let result = closure.apply(42u64)?;
//!     Ok(())
//! }
//! ```
//!
//! # Creating Closures from Rust Functions
//!
//! You can create Lean closures that wrap Rust functions using [`LeanClosure::from_fn1`]
//! and similar methods. The Rust function must follow Lean's calling convention
//! (taking and returning raw `*mut lean_object` pointers).
//!
//! ```rust,ignore
//! use leo3::prelude::*;
//! use leo3::closure::LeanClosure;
//!
//! // A function compatible with Lean's calling convention
//! unsafe extern "C" fn double_nat(x: *mut lean_object) -> *mut lean_object {
//!     // ... implementation
//! }
//!
//! fn example<'l>(lean: Lean<'l>) -> LeanResult<LeanClosure<'l>> {
//!     // Create a closure wrapping the function
//!     LeanClosure::from_fn1(lean, double_nat)
//! }
//! ```

use crate::conversion::{FromLean, IntoLean};
use crate::err::{LeanError, LeanResult};
use crate::ffi;
use crate::instance::{LeanAny, LeanBound};
use crate::marker::Lean;
use std::ffi::c_void;

/// Marker type for Lean closure objects.
pub struct LeanClosureType {
    _private: (),
}

/// A safe wrapper around a Lean closure object.
///
/// `LeanClosure` provides:
/// - Inspection of closure arity and fixed arguments
/// - Safe application with type-checked arguments
/// - Partial application support
/// - Creation of closures from Rust functions
///
/// # Memory Management
///
/// Like other Lean objects, closures use reference counting. The `LeanClosure`
/// wrapper automatically manages reference counts through [`LeanBound`].
///
/// # Important: Closure Consumption
///
/// Lean's `lean_apply_*` functions consume the closure. This wrapper clones
/// the closure before application to allow multiple calls. For single-use
/// scenarios with better performance, use the `*_once` variants.
pub type LeanClosure<'l> = LeanBound<'l, LeanClosureType>;

/// Type alias for Lean-compatible function pointer taking 1 argument.
pub type LeanFn1 = unsafe extern "C" fn(*mut ffi::lean_object) -> *mut ffi::lean_object;

/// Type alias for Lean-compatible function pointer taking 2 arguments.
pub type LeanFn2 =
    unsafe extern "C" fn(*mut ffi::lean_object, *mut ffi::lean_object) -> *mut ffi::lean_object;

/// Type alias for Lean-compatible function pointer taking 3 arguments.
pub type LeanFn3 = unsafe extern "C" fn(
    *mut ffi::lean_object,
    *mut ffi::lean_object,
    *mut ffi::lean_object,
) -> *mut ffi::lean_object;

/// Type alias for Lean-compatible function pointer taking 4 arguments.
pub type LeanFn4 = unsafe extern "C" fn(
    *mut ffi::lean_object,
    *mut ffi::lean_object,
    *mut ffi::lean_object,
    *mut ffi::lean_object,
) -> *mut ffi::lean_object;

/// Type alias for Lean-compatible function pointer taking 5 arguments.
pub type LeanFn5 = unsafe extern "C" fn(
    *mut ffi::lean_object,
    *mut ffi::lean_object,
    *mut ffi::lean_object,
    *mut ffi::lean_object,
    *mut ffi::lean_object,
) -> *mut ffi::lean_object;

/// Type alias for Lean-compatible function pointer taking 6 arguments.
pub type LeanFn6 = unsafe extern "C" fn(
    *mut ffi::lean_object,
    *mut ffi::lean_object,
    *mut ffi::lean_object,
    *mut ffi::lean_object,
    *mut ffi::lean_object,
    *mut ffi::lean_object,
) -> *mut ffi::lean_object;

impl<'l> LeanClosure<'l> {
    // ========================================================================
    // Closure Creation
    // ========================================================================

    /// Create a closure from a raw function pointer and arity.
    ///
    /// This is a low-level method for advanced users. The function pointer must
    /// be compatible with Lean's calling convention.
    ///
    /// # Safety
    ///
    /// - `fun` must be a valid function pointer that follows Lean's calling convention
    /// - The function must have the correct signature for the given arity
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// unsafe extern "C" fn my_fn(x: *mut lean_object) -> *mut lean_object {
    ///     x // identity function
    /// }
    ///
    /// let closure = unsafe { LeanClosure::from_raw_fn(lean, my_fn as *mut c_void, 1, 0)? };
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `arity` is 0
    /// - `num_fixed` >= `arity`
    pub unsafe fn from_raw_fn(
        lean: Lean<'l>,
        fun: *mut c_void,
        arity: u32,
        num_fixed: u32,
    ) -> LeanResult<Self> {
        if arity == 0 {
            return Err(LeanError::Conversion(
                "Closure arity must be > 0".to_string(),
            ));
        }
        if num_fixed >= arity {
            return Err(LeanError::Conversion(format!(
                "num_fixed ({}) must be < arity ({})",
                num_fixed, arity
            )));
        }

        let ptr = ffi::inline::lean_alloc_closure(fun, arity, num_fixed);
        Ok(LeanBound::<LeanClosureType>::from_owned_ptr(lean, ptr))
    }

    /// Create a 1-arity closure from a Rust function.
    ///
    /// The function must follow Lean's calling convention: it takes ownership
    /// of its argument and returns an owned result.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// unsafe extern "C" fn inc(x: *mut lean_object) -> *mut lean_object {
    ///     let n = lean_unbox(x);
    ///     lean_box(n + 1)
    /// }
    ///
    /// let closure = LeanClosure::from_fn1(lean, inc)?;
    /// let result = closure.call1::<u64, u64>(5)?;
    /// assert_eq!(result, 6);
    /// ```
    pub fn from_fn1(lean: Lean<'l>, f: LeanFn1) -> LeanResult<Self> {
        // SAFETY: f is a correctly-typed function pointer for a 1-arity function
        unsafe { Self::from_raw_fn(lean, f as *mut c_void, 1, 0) }
    }

    /// Create a 2-arity closure from a Rust function.
    pub fn from_fn2(lean: Lean<'l>, f: LeanFn2) -> LeanResult<Self> {
        // SAFETY: f is a correctly-typed function pointer for a 2-arity function
        unsafe { Self::from_raw_fn(lean, f as *mut c_void, 2, 0) }
    }

    /// Create a 3-arity closure from a Rust function.
    pub fn from_fn3(lean: Lean<'l>, f: LeanFn3) -> LeanResult<Self> {
        // SAFETY: f is a correctly-typed function pointer for a 3-arity function
        unsafe { Self::from_raw_fn(lean, f as *mut c_void, 3, 0) }
    }

    /// Create a 4-arity closure from a Rust function.
    pub fn from_fn4(lean: Lean<'l>, f: LeanFn4) -> LeanResult<Self> {
        // SAFETY: f is a correctly-typed function pointer for a 4-arity function
        unsafe { Self::from_raw_fn(lean, f as *mut c_void, 4, 0) }
    }

    /// Create a 5-arity closure from a Rust function.
    pub fn from_fn5(lean: Lean<'l>, f: LeanFn5) -> LeanResult<Self> {
        // SAFETY: f is a correctly-typed function pointer for a 5-arity function
        unsafe { Self::from_raw_fn(lean, f as *mut c_void, 5, 0) }
    }

    /// Create a 6-arity closure from a Rust function.
    pub fn from_fn6(lean: Lean<'l>, f: LeanFn6) -> LeanResult<Self> {
        // SAFETY: f is a correctly-typed function pointer for a 6-arity function
        unsafe { Self::from_raw_fn(lean, f as *mut c_void, 6, 0) }
    }

    /// Create a closure with pre-applied (captured) arguments.
    ///
    /// This creates a partially-applied closure. The `captured` arguments are
    /// stored in the closure and will be passed to the function when all
    /// remaining arguments are provided.
    ///
    /// # Safety
    ///
    /// - `fun` must be a valid function pointer that follows Lean's calling convention
    /// - The function must have the correct signature for the given arity
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // Create an "add" function
    /// unsafe extern "C" fn add(a: *mut lean_object, b: *mut lean_object) -> *mut lean_object {
    ///     let x = lean_unbox(a);
    ///     let y = lean_unbox(b);
    ///     lean_box(x + y)
    /// }
    ///
    /// // Create a partial application: add(5, _)
    /// let five = LeanNat::from_usize(lean, 5)?;
    /// let add5 = unsafe {
    ///     LeanClosure::with_captured(lean, add as *mut c_void, 2, vec![five.cast()])?
    /// };
    ///
    /// // Now add5 is a 1-arity closure
    /// assert_eq!(add5.remaining_arity(), 1);
    /// let result = add5.call1::<u64, u64>(10)?; // Returns 15
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - `arity` is 0
    /// - `captured.len()` >= `arity`
    pub unsafe fn with_captured(
        lean: Lean<'l>,
        fun: *mut c_void,
        arity: u32,
        captured: Vec<LeanBound<'l, LeanAny>>,
    ) -> LeanResult<Self> {
        let num_fixed = captured.len() as u32;
        if arity == 0 {
            return Err(LeanError::Conversion(
                "Closure arity must be > 0".to_string(),
            ));
        }
        if num_fixed >= arity {
            return Err(LeanError::Conversion(format!(
                "Too many captured arguments ({}) for arity ({})",
                num_fixed, arity
            )));
        }

        let ptr = ffi::inline::lean_alloc_closure(fun, arity, num_fixed);

        // Store the captured arguments
        for (i, arg) in captured.into_iter().enumerate() {
            ffi::inline::lean_closure_set(ptr, i as libc::c_uint, arg.into_ptr());
        }

        Ok(LeanBound::<LeanClosureType>::from_owned_ptr(lean, ptr))
    }

    // ========================================================================
    // Type Checking
    // ========================================================================

    /// Check if a Lean object is a closure.
    ///
    /// Returns `true` if the object is a closure, `false` otherwise.
    #[inline]
    pub fn is_closure(obj: &LeanBound<'l, LeanAny>) -> bool {
        unsafe {
            let ptr = obj.as_ptr();
            !ffi::inline::lean_is_scalar(ptr) && ffi::inline::lean_is_closure(ptr)
        }
    }

    /// Try to convert a `LeanBound<LeanAny>` to a `LeanClosure`.
    ///
    /// Returns `Some(closure)` if the object is a closure, `None` otherwise.
    #[inline]
    pub fn try_from_any(obj: LeanBound<'l, LeanAny>) -> Option<Self> {
        if Self::is_closure(&obj) {
            Some(obj.cast())
        } else {
            None
        }
    }

    // ========================================================================
    // Inspection
    // ========================================================================

    /// Get the total arity of this closure.
    ///
    /// This is the total number of arguments the underlying function expects,
    /// regardless of how many have already been applied.
    #[inline]
    pub fn arity(&self) -> u16 {
        unsafe {
            let closure = self.as_ptr() as *const ffi::inline::lean_closure_object;
            (*closure).m_arity
        }
    }

    /// Get the number of arguments already applied to this closure.
    ///
    /// This represents how many arguments have been captured through
    /// partial application.
    #[inline]
    pub fn num_fixed(&self) -> u16 {
        unsafe {
            let closure = self.as_ptr() as *const ffi::inline::lean_closure_object;
            (*closure).m_num_fixed
        }
    }

    /// Get the number of arguments still needed to fully apply this closure.
    ///
    /// Returns `arity() - num_fixed()`.
    #[inline]
    pub fn remaining_arity(&self) -> u16 {
        self.arity().saturating_sub(self.num_fixed())
    }

    /// Check if this closure is fully saturated (ready to be called).
    ///
    /// A saturated closure has all its arguments applied and can be
    /// called with no additional arguments.
    #[inline]
    pub fn is_saturated(&self) -> bool {
        self.remaining_arity() == 0
    }

    /// Get the function pointer from this closure.
    ///
    /// This returns the raw function pointer that will be called when
    /// the closure is fully applied. This is mainly useful for debugging
    /// or advanced interop scenarios.
    #[inline]
    pub fn function_ptr(&self) -> *mut c_void {
        unsafe { ffi::inline::lean_closure_fun(self.as_ptr()) }
    }

    /// Get a captured (fixed) argument by index.
    ///
    /// Returns `None` if the index is out of bounds.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// // If closure was created with captured args
    /// if let Some(first_arg) = closure.get_captured(0) {
    ///     println!("First captured arg: {:?}", first_arg);
    /// }
    /// ```
    pub fn get_captured(&self, index: usize) -> Option<LeanBound<'l, LeanAny>> {
        if index >= self.num_fixed() as usize {
            return None;
        }
        unsafe {
            let ptr = ffi::inline::lean_closure_get(self.as_ptr(), index as libc::c_uint);
            if ptr.is_null() {
                None
            } else {
                // Increment ref count since we're borrowing
                ffi::inline::lean_inc(ptr as *mut _);
                Some(LeanBound::<LeanAny>::from_owned_ptr(
                    self.lean_token(),
                    ptr as *mut _,
                ))
            }
        }
    }

    // ========================================================================
    // Application (returns LeanAny - caller casts as needed)
    // ========================================================================

    /// Apply this closure to one argument, returning a new closure or result.
    ///
    /// If the closure needs more than one argument, this returns a new
    /// partially-applied closure. If this was the last argument needed,
    /// the closure is executed and the result is returned.
    ///
    /// # Note
    ///
    /// This clones the closure before application. For single-use scenarios,
    /// use [`apply_once`](Self::apply_once) for better performance.
    pub fn apply(&self, arg: LeanBound<'l, LeanAny>) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();
        let closure_clone = self.clone();

        unsafe {
            let result = ffi::closure::lean_apply_1(closure_clone.into_ptr(), arg.into_ptr());
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to one argument, consuming the closure.
    ///
    /// This is more efficient than [`apply`](Self::apply) when you don't
    /// need to reuse the closure.
    pub fn apply_once(self, arg: LeanBound<'l, LeanAny>) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();

        unsafe {
            let result = ffi::closure::lean_apply_1(self.into_ptr(), arg.into_ptr());
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to two arguments.
    pub fn apply2(
        &self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();
        let closure_clone = self.clone();

        unsafe {
            let result =
                ffi::closure::lean_apply_2(closure_clone.into_ptr(), a.into_ptr(), b.into_ptr());
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to two arguments, consuming the closure.
    ///
    /// This is more efficient than [`apply2`](Self::apply2) when you don't
    /// need to reuse the closure.
    pub fn apply2_once(
        self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();

        unsafe {
            let result =
                ffi::closure::lean_apply_2(self.into_ptr(), a.into_ptr(), b.into_ptr());
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to three arguments.
    pub fn apply3(
        &self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
        c: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();
        let closure_clone = self.clone();

        unsafe {
            let result = ffi::closure::lean_apply_3(
                closure_clone.into_ptr(),
                a.into_ptr(),
                b.into_ptr(),
                c.into_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to three arguments, consuming the closure.
    ///
    /// This is more efficient than [`apply3`](Self::apply3) when you don't
    /// need to reuse the closure.
    pub fn apply3_once(
        self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
        c: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();

        unsafe {
            let result = ffi::closure::lean_apply_3(
                self.into_ptr(),
                a.into_ptr(),
                b.into_ptr(),
                c.into_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to four arguments.
    pub fn apply4(
        &self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
        c: LeanBound<'l, LeanAny>,
        d: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();
        let closure_clone = self.clone();

        unsafe {
            let result = ffi::closure::lean_apply_4(
                closure_clone.into_ptr(),
                a.into_ptr(),
                b.into_ptr(),
                c.into_ptr(),
                d.into_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to four arguments, consuming the closure.
    ///
    /// This is more efficient than [`apply4`](Self::apply4) when you don't
    /// need to reuse the closure.
    pub fn apply4_once(
        self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
        c: LeanBound<'l, LeanAny>,
        d: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();

        unsafe {
            let result = ffi::closure::lean_apply_4(
                self.into_ptr(),
                a.into_ptr(),
                b.into_ptr(),
                c.into_ptr(),
                d.into_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to five arguments.
    #[allow(clippy::too_many_arguments)]
    pub fn apply5(
        &self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
        c: LeanBound<'l, LeanAny>,
        d: LeanBound<'l, LeanAny>,
        e: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();
        let closure_clone = self.clone();

        unsafe {
            let result = ffi::closure::lean_apply_5(
                closure_clone.into_ptr(),
                a.into_ptr(),
                b.into_ptr(),
                c.into_ptr(),
                d.into_ptr(),
                e.into_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to five arguments, consuming the closure.
    ///
    /// This is more efficient than [`apply5`](Self::apply5) when you don't
    /// need to reuse the closure.
    #[allow(clippy::too_many_arguments)]
    pub fn apply5_once(
        self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
        c: LeanBound<'l, LeanAny>,
        d: LeanBound<'l, LeanAny>,
        e: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();

        unsafe {
            let result = ffi::closure::lean_apply_5(
                self.into_ptr(),
                a.into_ptr(),
                b.into_ptr(),
                c.into_ptr(),
                d.into_ptr(),
                e.into_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to six arguments.
    #[allow(clippy::too_many_arguments)]
    pub fn apply6(
        &self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
        c: LeanBound<'l, LeanAny>,
        d: LeanBound<'l, LeanAny>,
        e: LeanBound<'l, LeanAny>,
        f: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();
        let closure_clone = self.clone();

        unsafe {
            let result = ffi::closure::lean_apply_6(
                closure_clone.into_ptr(),
                a.into_ptr(),
                b.into_ptr(),
                c.into_ptr(),
                d.into_ptr(),
                e.into_ptr(),
                f.into_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to six arguments, consuming the closure.
    ///
    /// This is more efficient than [`apply6`](Self::apply6) when you don't
    /// need to reuse the closure.
    #[allow(clippy::too_many_arguments)]
    pub fn apply6_once(
        self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
        c: LeanBound<'l, LeanAny>,
        d: LeanBound<'l, LeanAny>,
        e: LeanBound<'l, LeanAny>,
        f: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();

        unsafe {
            let result = ffi::closure::lean_apply_6(
                self.into_ptr(),
                a.into_ptr(),
                b.into_ptr(),
                c.into_ptr(),
                d.into_ptr(),
                e.into_ptr(),
                f.into_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to seven arguments.
    #[allow(clippy::too_many_arguments)]
    pub fn apply7(
        &self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
        c: LeanBound<'l, LeanAny>,
        d: LeanBound<'l, LeanAny>,
        e: LeanBound<'l, LeanAny>,
        f: LeanBound<'l, LeanAny>,
        g: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();
        let closure_clone = self.clone();

        unsafe {
            let result = ffi::closure::lean_apply_7(
                closure_clone.into_ptr(),
                a.into_ptr(),
                b.into_ptr(),
                c.into_ptr(),
                d.into_ptr(),
                e.into_ptr(),
                f.into_ptr(),
                g.into_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to seven arguments, consuming the closure.
    ///
    /// This is more efficient than [`apply7`](Self::apply7) when you don't
    /// need to reuse the closure.
    #[allow(clippy::too_many_arguments)]
    pub fn apply7_once(
        self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
        c: LeanBound<'l, LeanAny>,
        d: LeanBound<'l, LeanAny>,
        e: LeanBound<'l, LeanAny>,
        f: LeanBound<'l, LeanAny>,
        g: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();

        unsafe {
            let result = ffi::closure::lean_apply_7(
                self.into_ptr(),
                a.into_ptr(),
                b.into_ptr(),
                c.into_ptr(),
                d.into_ptr(),
                e.into_ptr(),
                f.into_ptr(),
                g.into_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to eight arguments.
    #[allow(clippy::too_many_arguments)]
    pub fn apply8(
        &self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
        c: LeanBound<'l, LeanAny>,
        d: LeanBound<'l, LeanAny>,
        e: LeanBound<'l, LeanAny>,
        f: LeanBound<'l, LeanAny>,
        g: LeanBound<'l, LeanAny>,
        h: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();
        let closure_clone = self.clone();

        unsafe {
            let result = ffi::closure::lean_apply_8(
                closure_clone.into_ptr(),
                a.into_ptr(),
                b.into_ptr(),
                c.into_ptr(),
                d.into_ptr(),
                e.into_ptr(),
                f.into_ptr(),
                g.into_ptr(),
                h.into_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to eight arguments, consuming the closure.
    ///
    /// This is more efficient than [`apply8`](Self::apply8) when you don't
    /// need to reuse the closure.
    #[allow(clippy::too_many_arguments)]
    pub fn apply8_once(
        self,
        a: LeanBound<'l, LeanAny>,
        b: LeanBound<'l, LeanAny>,
        c: LeanBound<'l, LeanAny>,
        d: LeanBound<'l, LeanAny>,
        e: LeanBound<'l, LeanAny>,
        f: LeanBound<'l, LeanAny>,
        g: LeanBound<'l, LeanAny>,
        h: LeanBound<'l, LeanAny>,
    ) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();

        unsafe {
            let result = ffi::closure::lean_apply_8(
                self.into_ptr(),
                a.into_ptr(),
                b.into_ptr(),
                c.into_ptr(),
                d.into_ptr(),
                e.into_ptr(),
                f.into_ptr(),
                g.into_ptr(),
                h.into_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to a dynamic number of arguments.
    ///
    /// The arguments are consumed.
    pub fn apply_n(&self, args: Vec<LeanBound<'l, LeanAny>>) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();
        let closure_clone = self.clone();

        let mut arg_ptrs: Vec<*mut ffi::lean_object> =
            args.into_iter().map(|arg| arg.into_ptr()).collect();

        unsafe {
            let result = ffi::closure::lean_apply_n(
                closure_clone.into_ptr(),
                arg_ptrs.len() as libc::c_uint,
                arg_ptrs.as_mut_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Apply this closure to a dynamic number of arguments, consuming the closure.
    pub fn apply_n_once(self, args: Vec<LeanBound<'l, LeanAny>>) -> LeanBound<'l, LeanAny> {
        let lean = self.lean_token();

        let mut arg_ptrs: Vec<*mut ffi::lean_object> =
            args.into_iter().map(|arg| arg.into_ptr()).collect();

        unsafe {
            let result = ffi::closure::lean_apply_n(
                self.into_ptr(),
                arg_ptrs.len() as libc::c_uint,
                arg_ptrs.as_mut_ptr(),
            );
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    // ========================================================================
    // Type-Converting Call Methods
    // ========================================================================

    /// Call this closure with 1 argument, converting types automatically.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let result: u64 = closure.call1::<u64, u64>(42)?;
    /// ```
    pub fn call1<A, R>(&self, a: A) -> LeanResult<R>
    where
        A: IntoLean<'l> + 'l,
        R: FromLean<'l> + 'l,
    {
        let lean = self.lean_token();
        let a_obj = a.into_lean(lean)?.cast::<LeanAny>();
        let result = self.apply(a_obj);
        R::from_lean(&result.cast())
    }

    /// Call this closure with 2 arguments, converting types automatically.
    pub fn call2<A, B, R>(&self, a: A, b: B) -> LeanResult<R>
    where
        A: IntoLean<'l> + 'l,
        B: IntoLean<'l> + 'l,
        R: FromLean<'l> + 'l,
    {
        let lean = self.lean_token();
        let a_obj = a.into_lean(lean)?.cast::<LeanAny>();
        let b_obj = b.into_lean(lean)?.cast::<LeanAny>();
        let result = self.apply2(a_obj, b_obj);
        R::from_lean(&result.cast())
    }

    /// Call this closure with 3 arguments, converting types automatically.
    pub fn call3<A, B, C, R>(&self, a: A, b: B, c: C) -> LeanResult<R>
    where
        A: IntoLean<'l> + 'l,
        B: IntoLean<'l> + 'l,
        C: IntoLean<'l> + 'l,
        R: FromLean<'l> + 'l,
    {
        let lean = self.lean_token();
        let a_obj = a.into_lean(lean)?.cast::<LeanAny>();
        let b_obj = b.into_lean(lean)?.cast::<LeanAny>();
        let c_obj = c.into_lean(lean)?.cast::<LeanAny>();
        let result = self.apply3(a_obj, b_obj, c_obj);
        R::from_lean(&result.cast())
    }

    /// Call this closure with 4 arguments, converting types automatically.
    pub fn call4<A, B, C, D, R>(&self, a: A, b: B, c: C, d: D) -> LeanResult<R>
    where
        A: IntoLean<'l> + 'l,
        B: IntoLean<'l> + 'l,
        C: IntoLean<'l> + 'l,
        D: IntoLean<'l> + 'l,
        R: FromLean<'l> + 'l,
    {
        let lean = self.lean_token();
        let a_obj = a.into_lean(lean)?.cast::<LeanAny>();
        let b_obj = b.into_lean(lean)?.cast::<LeanAny>();
        let c_obj = c.into_lean(lean)?.cast::<LeanAny>();
        let d_obj = d.into_lean(lean)?.cast::<LeanAny>();
        let result = self.apply4(a_obj, b_obj, c_obj, d_obj);
        R::from_lean(&result.cast())
    }

    // ========================================================================
    // Helper Methods
    // ========================================================================

    /// Check that the closure can accept the given number of arguments.
    #[allow(dead_code)]
    fn check_arity(&self, n: usize) -> LeanResult<()> {
        let remaining = self.remaining_arity() as usize;
        if remaining != n {
            return Err(LeanError::Conversion(format!(
                "Closure expects {} arguments, got {}",
                remaining, n
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_closure_type_size() {
        // LeanClosure should be pointer-sized (same as LeanBound)
        assert_eq!(
            std::mem::size_of::<LeanClosure>(),
            std::mem::size_of::<*mut ()>()
        );
    }
}
