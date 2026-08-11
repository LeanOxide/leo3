//! LeanClosure comprehensive operation tests for Leo3
//!
//! Exercises every public API on `LeanClosure` from `leo3::closure`:
//! - Creation: `from_raw_fn`, `from_fn1`..`from_fn6`, `with_captured`
//! - Application: `apply`, `apply_once`, `apply2`..`apply8` and their `_once`
//!   variants, `apply_n` / `apply_n_once` (including partial application)
//! - Type-converting calls: `call1`..`call4` (LeanNat/LeanString values round-trip)
//! - Introspection: `arity`, `num_fixed`, `remaining_arity`, `is_saturated`,
//!   `function_ptr`, `get_captured`
//! - Type checks: `is_closure`, `try_from_any`
//! - Error paths: invalid arity / captured counts, out-of-bounds captures

#![cfg(feature = "runtime-tests")]

use leo3::closure::LeanClosure;
use leo3::instance::LeanAny;
use leo3::prelude::*;
use std::ffi::c_void;

// ============================================================================
// Raw Lean calling-convention functions used by the closures under test
// ============================================================================

unsafe extern "C" fn identity_fn(x: *mut leo3::ffi::lean_object) -> *mut leo3::ffi::lean_object {
    x
}

unsafe extern "C" fn inc_nat(x: *mut leo3::ffi::lean_object) -> *mut leo3::ffi::lean_object {
    let n = leo3::ffi::inline::lean_unbox(x);
    leo3::ffi::inline::lean_box(n.wrapping_add(1))
}

/// Generates an n-ary function that sums its scalar arguments.
macro_rules! sum_n {
    ($name:ident, $($arg:ident),+) => {
        unsafe extern "C" fn $name($($arg: *mut leo3::ffi::lean_object),+) -> *mut leo3::ffi::lean_object {
            let mut total = 0usize;
            $(
                total = total.wrapping_add(leo3::ffi::inline::lean_unbox($arg));
            )+
            leo3::ffi::inline::lean_box(total)
        }
    };
}

sum_n!(add_nat, a, b);
sum_n!(sum3, a, b, c);
sum_n!(sum4, a, b, c, d);
sum_n!(sum5, a, b, c, d, e);
sum_n!(sum6, a, b, c, d, e, f);
sum_n!(sum7, a, b, c, d, e, f, g);
sum_n!(sum8, a, b, c, d, e, f, g, h);

unsafe extern "C" fn concat_str(
    a: *mut leo3::ffi::lean_object,
    b: *mut leo3::ffi::lean_object,
) -> *mut leo3::ffi::lean_object {
    // `lean_string_append` takes ownership of `a` and borrows `b`, matching
    // the closure calling convention.
    leo3::ffi::string::lean_string_append(a, b)
}

// ============================================================================
// Creation
// ============================================================================

#[test]
fn closure_ops_creation_from_fn1_to_fn6() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let c1 = LeanClosure::from_fn1(lean, identity_fn)?;
        let c2 = LeanClosure::from_fn2(lean, add_nat)?;
        let c3 = LeanClosure::from_fn3(lean, sum3)?;
        let c4 = LeanClosure::from_fn4(lean, sum4)?;
        let c5 = LeanClosure::from_fn5(lean, sum5)?;
        let c6 = LeanClosure::from_fn6(lean, sum6)?;

        assert_eq!(c1.arity(), 1);
        assert_eq!(c2.arity(), 2);
        assert_eq!(c3.arity(), 3);
        assert_eq!(c4.arity(), 4);
        assert_eq!(c5.arity(), 5);
        assert_eq!(c6.arity(), 6);

        for c in [&c1, &c2, &c3, &c4, &c5, &c6] {
            assert_eq!(c.num_fixed(), 0);
            assert_eq!(c.remaining_arity(), c.arity());
            assert!(!c.is_saturated());
        }

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn closure_ops_from_raw_fn_and_function_ptr() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = unsafe { LeanClosure::from_raw_fn(lean, add_nat as *mut c_void, 2, 0)? };

        assert_eq!(closure.arity(), 2);
        assert_eq!(closure.num_fixed(), 0);
        assert_eq!(closure.remaining_arity(), 2);
        assert!(!closure.is_saturated());

        // The raw function pointer is preserved.
        assert_eq!(closure.function_ptr(), add_nat as *mut c_void);

        // The closure is still callable.
        let out: LeanBound<LeanNat> = closure
            .apply2(
                LeanNat::from_usize(lean, 4)?.cast(),
                LeanNat::from_usize(lean, 5)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out)?, 9);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn closure_ops_from_raw_fn_errors() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // arity == 0 is rejected.
        let r = unsafe { LeanClosure::from_raw_fn(lean, identity_fn as *mut c_void, 0, 0) };
        assert!(r.is_err());

        // num_fixed >= arity is rejected.
        let r = unsafe { LeanClosure::from_raw_fn(lean, add_nat as *mut c_void, 2, 2) };
        assert!(r.is_err());
        let r = unsafe { LeanClosure::from_raw_fn(lean, add_nat as *mut c_void, 2, 3) };
        assert!(r.is_err());

        Ok(())
    });

    assert!(result.is_ok());
}

// ============================================================================
// Application: apply / apply_once
// ============================================================================

#[test]
fn closure_ops_apply_inc_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, inc_nat)?;

        // LeanNat in, LeanNat out: 41 -> 42.
        let input = LeanNat::from_usize(lean, 41)?;
        let out: LeanBound<LeanNat> = closure.apply(input.cast()).cast();
        assert_eq!(LeanNat::to_usize(&out)?, 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn closure_ops_apply_reuses_closure_and_identity() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, inc_nat)?;

        // `apply` clones internally, so the same closure can be applied twice.
        let out1: LeanBound<LeanNat> = closure.apply(LeanNat::from_usize(lean, 1)?.cast()).cast();
        let out2: LeanBound<LeanNat> = closure.apply(LeanNat::from_usize(lean, 2)?.cast()).cast();
        assert_eq!(LeanNat::to_usize(&out1)?, 2);
        assert_eq!(LeanNat::to_usize(&out2)?, 3);

        // Identity closure round-trips the exact value.
        let ident = LeanClosure::from_fn1(lean, identity_fn)?;
        let value = LeanNat::from_usize(lean, 42)?;
        let out: LeanBound<LeanNat> = ident.apply(value.cast()).cast();
        assert_eq!(LeanNat::to_usize(&out)?, 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn closure_ops_apply_once() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, inc_nat)?;

        // apply_once consumes the closure: 9 -> 10.
        let out: LeanBound<LeanNat> = closure
            .apply_once(LeanNat::from_usize(lean, 9)?.cast())
            .cast();
        assert_eq!(LeanNat::to_usize(&out)?, 10);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn closure_ops_apply2_and_apply2_once() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn2(lean, add_nat)?;
        let out: LeanBound<LeanNat> = closure
            .apply2(
                LeanNat::from_usize(lean, 3)?.cast(),
                LeanNat::from_usize(lean, 7)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out)?, 10);

        let closure = LeanClosure::from_fn2(lean, add_nat)?;
        let out: LeanBound<LeanNat> = closure
            .apply2_once(
                LeanNat::from_usize(lean, 20)?.cast(),
                LeanNat::from_usize(lean, 22)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out)?, 42);

        Ok(())
    });

    assert!(result.is_ok());
}

// ============================================================================
// Application: apply3 .. apply8 and _once variants
// ============================================================================

#[test]
fn closure_ops_apply3_to_apply6() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let c3 = LeanClosure::from_fn3(lean, sum3)?;
        let out3: LeanBound<LeanNat> = c3
            .apply3(
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 2)?.cast(),
                LeanNat::from_usize(lean, 3)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out3)?, 6);

        let c4 = LeanClosure::from_fn4(lean, sum4)?;
        let out4: LeanBound<LeanNat> = c4
            .apply4(
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 2)?.cast(),
                LeanNat::from_usize(lean, 3)?.cast(),
                LeanNat::from_usize(lean, 4)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out4)?, 10);

        let c5 = LeanClosure::from_fn5(lean, sum5)?;
        let out5: LeanBound<LeanNat> = c5
            .apply5(
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 2)?.cast(),
                LeanNat::from_usize(lean, 3)?.cast(),
                LeanNat::from_usize(lean, 4)?.cast(),
                LeanNat::from_usize(lean, 5)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out5)?, 15);

        let c6 = LeanClosure::from_fn6(lean, sum6)?;
        let out6: LeanBound<LeanNat> = c6
            .apply6(
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 2)?.cast(),
                LeanNat::from_usize(lean, 3)?.cast(),
                LeanNat::from_usize(lean, 4)?.cast(),
                LeanNat::from_usize(lean, 5)?.cast(),
                LeanNat::from_usize(lean, 6)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out6)?, 21);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn closure_ops_apply7_and_apply8() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // No from_fn7/from_fn8 exist; create via from_raw_fn.
        let c7 = unsafe { LeanClosure::from_raw_fn(lean, sum7 as *mut c_void, 7, 0)? };
        assert_eq!(c7.arity(), 7);
        let out7: LeanBound<LeanNat> = c7
            .apply7(
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 2)?.cast(),
                LeanNat::from_usize(lean, 3)?.cast(),
                LeanNat::from_usize(lean, 4)?.cast(),
                LeanNat::from_usize(lean, 5)?.cast(),
                LeanNat::from_usize(lean, 6)?.cast(),
                LeanNat::from_usize(lean, 7)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out7)?, 28);

        let c8 = unsafe { LeanClosure::from_raw_fn(lean, sum8 as *mut c_void, 8, 0)? };
        assert_eq!(c8.arity(), 8);
        let out8: LeanBound<LeanNat> = c8
            .apply8(
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 2)?.cast(),
                LeanNat::from_usize(lean, 3)?.cast(),
                LeanNat::from_usize(lean, 4)?.cast(),
                LeanNat::from_usize(lean, 5)?.cast(),
                LeanNat::from_usize(lean, 6)?.cast(),
                LeanNat::from_usize(lean, 7)?.cast(),
                LeanNat::from_usize(lean, 8)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out8)?, 36);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn closure_ops_apply_once_variants_3_to_8() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let c3 = LeanClosure::from_fn3(lean, sum3)?;
        let out3: LeanBound<LeanNat> = c3
            .apply3_once(
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out3)?, 3);

        let c4 = LeanClosure::from_fn4(lean, sum4)?;
        let out4: LeanBound<LeanNat> = c4
            .apply4_once(
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out4)?, 4);

        let c5 = LeanClosure::from_fn5(lean, sum5)?;
        let out5: LeanBound<LeanNat> = c5
            .apply5_once(
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out5)?, 5);

        let c6 = LeanClosure::from_fn6(lean, sum6)?;
        let out6: LeanBound<LeanNat> = c6
            .apply6_once(
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out6)?, 6);

        let c7 = unsafe { LeanClosure::from_raw_fn(lean, sum7 as *mut c_void, 7, 0)? };
        let out7: LeanBound<LeanNat> = c7
            .apply7_once(
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out7)?, 7);

        let c8 = unsafe { LeanClosure::from_raw_fn(lean, sum8 as *mut c_void, 8, 0)? };
        let out8: LeanBound<LeanNat> = c8
            .apply8_once(
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
                LeanNat::from_usize(lean, 1)?.cast(),
            )
            .cast();
        assert_eq!(LeanNat::to_usize(&out8)?, 8);

        Ok(())
    });

    assert!(result.is_ok());
}

// ============================================================================
// Partial application
// ============================================================================

#[test]
fn closure_ops_partial_application() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn2(lean, add_nat)?;

        // Applying one argument to a 2-arity closure yields a new closure.
        let partial: LeanClosure<'_> = closure.apply(LeanNat::from_usize(lean, 5)?.cast()).cast();
        assert_eq!(partial.arity(), 2);
        assert_eq!(partial.num_fixed(), 1);
        assert_eq!(partial.remaining_arity(), 1);
        assert!(!partial.is_saturated());

        // The original closure is untouched and still full arity.
        assert_eq!(closure.num_fixed(), 0);

        // Finishing the partial application executes the function.
        let out: LeanBound<LeanNat> = partial.apply(LeanNat::from_usize(lean, 10)?.cast()).cast();
        assert_eq!(LeanNat::to_usize(&out)?, 15);

        // apply_once also returns a partial closure when under-applied.
        let closure2 = LeanClosure::from_fn2(lean, add_nat)?;
        let partial2: LeanClosure<'_> = closure2
            .apply_once(LeanNat::from_usize(lean, 6)?.cast())
            .cast();
        assert_eq!(partial2.remaining_arity(), 1);
        let out: LeanBound<LeanNat> = partial2.apply(LeanNat::from_usize(lean, 9)?.cast()).cast();
        assert_eq!(LeanNat::to_usize(&out)?, 15);

        Ok(())
    });

    assert!(result.is_ok());
}

// ============================================================================
// Application: apply_n / apply_n_once
// ============================================================================

#[test]
fn closure_ops_apply_n() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn2(lean, add_nat)?;

        // Full application with a dynamic argument vector.
        let out: LeanBound<LeanNat> = closure
            .apply_n(vec![
                LeanNat::from_usize(lean, 30)?.cast(),
                LeanNat::from_usize(lean, 12)?.cast(),
            ])
            .cast();
        assert_eq!(LeanNat::to_usize(&out)?, 42);

        // Under-applying returns a partial closure.
        let partial: LeanClosure<'_> = closure
            .apply_n(vec![LeanNat::from_usize(lean, 7)?.cast()])
            .cast();
        assert_eq!(partial.remaining_arity(), 1);
        let out: LeanBound<LeanNat> = partial.apply(LeanNat::from_usize(lean, 8)?.cast()).cast();
        assert_eq!(LeanNat::to_usize(&out)?, 15);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn closure_ops_apply_n_once() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn2(lean, add_nat)?;

        let out: LeanBound<LeanNat> = closure
            .apply_n_once(vec![
                LeanNat::from_usize(lean, 20)?.cast(),
                LeanNat::from_usize(lean, 22)?.cast(),
            ])
            .cast();
        assert_eq!(LeanNat::to_usize(&out)?, 42);

        Ok(())
    });

    assert!(result.is_ok());
}

// ============================================================================
// with_captured (capturing Rust values)
// ============================================================================

#[test]
fn closure_ops_with_captured_nat() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Capture a Rust usize as a LeanNat: add(5, _).
        let five = LeanNat::from_usize(lean, 5)?;
        let add5 = unsafe {
            LeanClosure::with_captured(
                lean,
                add_nat as *mut c_void,
                2,
                vec![five.cast::<LeanAny>()],
            )?
        };

        assert_eq!(add5.arity(), 2);
        assert_eq!(add5.num_fixed(), 1);
        assert_eq!(add5.remaining_arity(), 1);
        assert_eq!(add5.function_ptr(), add_nat as *mut c_void);

        // The captured value round-trips through get_captured.
        let captured = add5.get_captured(0).expect("captured argument present");
        let captured_nat: LeanBound<LeanNat> = captured.cast();
        assert_eq!(LeanNat::to_usize(&captured_nat)?, 5);

        // Calling the closure computes add(5, 10) = 15.
        let out: LeanBound<LeanNat> = add5.apply(LeanNat::from_usize(lean, 10)?.cast()).cast();
        assert_eq!(LeanNat::to_usize(&out)?, 15);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn closure_ops_with_captured_string() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Capture a Rust &str as a LeanString: concat("Hello, ", _).
        let prefix = LeanString::mk(lean, "Hello, ")?;
        let concat = unsafe {
            LeanClosure::with_captured(
                lean,
                concat_str as *mut c_void,
                2,
                vec![prefix.cast::<LeanAny>()],
            )?
        };

        assert_eq!(concat.num_fixed(), 1);
        assert_eq!(concat.remaining_arity(), 1);

        // Captured string round-trips.
        let captured = concat.get_captured(0).expect("captured prefix present");
        let captured_str: LeanBound<LeanString> = captured.cast();
        assert_eq!(LeanString::cstr(&captured_str)?, "Hello, ");

        // Applying the remaining argument concatenates the strings.
        let out: LeanBound<LeanString> =
            concat.apply(LeanString::mk(lean, "World!")?.cast()).cast();
        assert_eq!(LeanString::cstr(&out)?, "Hello, World!");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn closure_ops_with_captured_errors() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let five = LeanNat::from_usize(lean, 5)?;

        // arity == 0 is rejected.
        let r = unsafe { LeanClosure::with_captured(lean, identity_fn as *mut c_void, 0, vec![]) };
        assert!(r.is_err());

        // num_fixed >= arity is rejected (2 captured for arity 2).
        let r = unsafe {
            LeanClosure::with_captured(
                lean,
                add_nat as *mut c_void,
                2,
                vec![five.clone().cast(), five.clone().cast()],
            )
        };
        assert!(r.is_err());

        // 3 captured for arity 2 is also rejected.
        let r = unsafe {
            LeanClosure::with_captured(
                lean,
                add_nat as *mut c_void,
                2,
                vec![
                    five.clone().cast(),
                    five.clone().cast(),
                    five.clone().cast(),
                ],
            )
        };
        assert!(r.is_err());

        // One captured arg for arity 3 is valid.
        let r = unsafe {
            LeanClosure::with_captured(lean, sum3 as *mut c_void, 3, vec![five.clone().cast()])
        };
        assert!(r.is_ok());

        Ok(())
    });

    assert!(result.is_ok());
}

// ============================================================================
// get_captured bounds
// ============================================================================

#[test]
fn closure_ops_get_captured_out_of_bounds() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // A closure with no fixed arguments has nothing to fetch.
        let closure = LeanClosure::from_fn1(lean, inc_nat)?;
        assert!(closure.get_captured(0).is_none());

        let five = LeanNat::from_usize(lean, 5)?;
        let add5 = unsafe {
            LeanClosure::with_captured(lean, add_nat as *mut c_void, 2, vec![five.cast()])?
        };

        // Valid index 0 yields the value; index >= num_fixed yields None.
        assert!(add5.get_captured(0).is_some());
        assert!(add5.get_captured(1).is_none());
        assert!(add5.get_captured(100).is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

// ============================================================================
// Type-converting calls: call1 .. call4
// ============================================================================

#[test]
fn closure_ops_call1() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, inc_nat)?;

        // u32 round-trip: 41 -> 42 (UInt32 uses the scalar encoding, so the
        // raw `lean_unbox`-based closure can read it).
        let out: u32 = closure.call1(41u32)?;
        assert_eq!(out, 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn closure_ops_call2_strings_and_nats() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // String concatenation: "Hello, " + "World!" = "Hello, World!".
        let concat = LeanClosure::from_fn2(lean, concat_str)?;
        let out: String =
            concat.call2::<String, String, String>("Hello, ".to_string(), "World!".to_string())?;
        assert_eq!(out, "Hello, World!");

        // u32 addition: 3 + 7 = 10.
        let add = LeanClosure::from_fn2(lean, add_nat)?;
        let out: u32 = add.call2(3u32, 7u32)?;
        assert_eq!(out, 10);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn closure_ops_call3() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn3(lean, sum3)?;
        let out: u32 = closure.call3(1u32, 2u32, 3u32)?;
        assert_eq!(out, 6);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn closure_ops_call4() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn4(lean, sum4)?;
        let out: u32 = closure.call4(1u32, 2u32, 3u32, 4u32)?;
        assert_eq!(out, 10);

        Ok(())
    });

    assert!(result.is_ok());
}

// ============================================================================
// Type checks: is_closure / try_from_any
// ============================================================================

#[test]
fn closure_ops_is_closure_and_try_from_any() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // A real closure converts both ways.
        let closure = LeanClosure::from_fn1(lean, inc_nat)?;
        let any: LeanBound<LeanAny> = closure.cast();
        assert!(LeanClosure::is_closure(&any));

        let recovered = LeanClosure::try_from_any(any);
        assert!(recovered.is_some());
        let recovered = recovered.unwrap();
        assert_eq!(recovered.arity(), 1);
        let out: u32 = recovered.call1(41u32)?;
        assert_eq!(out, 42);

        // A natural number is neither a closure nor convertible.
        let n = LeanNat::from_usize(lean, 42)?;
        let any_n: LeanBound<LeanAny> = n.cast();
        assert!(!LeanClosure::is_closure(&any_n));
        assert!(LeanClosure::try_from_any(any_n).is_none());

        Ok(())
    });

    assert!(result.is_ok());
}
