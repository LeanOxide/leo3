//! Thunk operation tests for Leo3
//!
//! Comprehensive coverage of the LeanThunk wrapper: type checking
//! (is_thunk, try_from_any), creation (new from a closure, pure),
//! evaluation (get, get_owned, get_cloned, is_pure), and combinators
//! (map, bind), including boundary values and lazy-evaluation semantics.

#![cfg(feature = "runtime-tests")]

use leo3::closure::LeanClosure;
use leo3::instance::LeanAny;
use leo3::prelude::*;
use leo3::thunk::LeanThunk;

// The closure stored in a thunk is applied to Lean's unit value
// (a boxed 0) when the thunk is forced, matching Lean's
// `Thunk.mk : (Unit → α) → Thunk α`.
unsafe extern "C" fn thunk_nat_42(
    _unit: *mut leo3::ffi::lean_object,
) -> *mut leo3::ffi::lean_object {
    leo3::ffi::inline::lean_box(42)
}

unsafe extern "C" fn thunk_nat_zero(
    _unit: *mut leo3::ffi::lean_object,
) -> *mut leo3::ffi::lean_object {
    leo3::ffi::inline::lean_box(0)
}

unsafe extern "C" fn thunk_nat_max_small(
    _unit: *mut leo3::ffi::lean_object,
) -> *mut leo3::ffi::lean_object {
    // Largest value that fits in the small-nat representation.
    leo3::ffi::inline::lean_box(usize::MAX >> 1)
}

/// Extract a usize from a LeanAny bound that is assumed to hold a small nat.
fn nat_value(v: &LeanBound<'_, LeanAny>) -> usize {
    unsafe { leo3::ffi::inline::lean_unbox(v.as_ptr()) }
}

#[test]
fn test_is_thunk_false_on_non_thunk() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let n = LeanNat::from_usize(lean, 42)?;
        let any: LeanBound<'_, LeanAny> = n.cast();
        assert!(!LeanThunk::<LeanAny>::is_thunk(&any));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_is_thunk_false_on_closure() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // A closure is a heap object but must not be classified as a thunk.
        let closure = LeanClosure::from_fn1(lean, thunk_nat_42)?;
        let any: LeanBound<'_, LeanAny> = closure.cast();
        assert!(!LeanThunk::<LeanAny>::is_thunk(&any));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_is_thunk_true_on_real_thunk() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, thunk_nat_42)?;
        let thunk: LeanThunk<'_, LeanAny> = LeanThunk::new(closure);
        let any: LeanBound<'_, LeanAny> = thunk.cast();
        assert!(LeanThunk::<LeanAny>::is_thunk(&any));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_try_from_any_none_for_non_thunk() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let n = LeanNat::from_usize(lean, 42)?;
        let any: LeanBound<'_, LeanAny> = n.cast();
        assert!(LeanThunk::<LeanAny>::try_from_any(any).is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_try_from_any_some_for_real_thunk() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, thunk_nat_42)?;
        let thunk: LeanThunk<'_, LeanAny> = LeanThunk::new(closure);
        let any: LeanBound<'_, LeanAny> = thunk.cast();

        let recovered = LeanThunk::<LeanAny>::try_from_any(any).expect("recover thunk");
        let value = recovered.get().to_owned();
        assert_eq!(nat_value(&value), 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_new_thunk_lazy_until_get() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, thunk_nat_42)?;
        let thunk: LeanThunk<'_, LeanNat> = LeanThunk::new(closure);

        // Not evaluated yet.
        assert!(!thunk.is_pure());

        // Forcing evaluates the closure and yields the value.
        let value = thunk.get().to_owned();
        assert_eq!(LeanNat::to_usize(&value)?, 42);

        // Now evaluated and cached.
        assert!(thunk.is_pure());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_get_owned_consumes_thunk() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, thunk_nat_42)?;
        let thunk: LeanThunk<'_, LeanAny> = LeanThunk::new(closure);

        let value = thunk.get_owned();
        assert_eq!(nat_value(&value), 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_get_cloned_does_not_consume() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, thunk_nat_42)?;
        let thunk: LeanThunk<'_, LeanNat> = LeanThunk::new(closure);

        let a = thunk.get_cloned();
        let b = thunk.get_cloned();
        assert_eq!(LeanNat::to_usize(&a)?, 42);
        assert_eq!(LeanNat::to_usize(&b)?, 42);

        // The thunk survives get_cloned and is now evaluated.
        assert!(thunk.is_pure());
        let c = thunk.get().to_owned();
        assert_eq!(LeanNat::to_usize(&c)?, 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_get_caches_value() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, thunk_nat_42)?;
        let thunk: LeanThunk<'_, LeanNat> = LeanThunk::new(closure);

        let v1 = LeanNat::to_usize(&thunk.get().to_owned())?;
        assert_eq!(v1, 42);
        assert!(thunk.is_pure());

        let v2 = LeanNat::to_usize(&thunk.get().to_owned())?;
        assert_eq!(v2, 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_pure_thunk_constructs_evaluated_thunk() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let thunk: LeanThunk<'_, LeanNat> = LeanThunk::pure(LeanNat::from_usize(lean, 99)?);

        // A pure thunk is already evaluated.
        assert!(thunk.is_pure());
        let value = thunk.get().to_owned();
        assert_eq!(LeanNat::to_usize(&value)?, 99);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_pure_thunk_detected_as_thunk() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let thunk: LeanThunk<'_, LeanNat> = LeanThunk::pure(LeanNat::from_usize(lean, 7)?);
        let any: LeanBound<'_, LeanAny> = thunk.cast();

        assert!(LeanThunk::<LeanAny>::is_thunk(&any));
        assert!(LeanThunk::<LeanAny>::try_from_any(any).is_some());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_map_transforms_value() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, thunk_nat_42)?;
        let thunk: LeanThunk<'_, LeanNat> = LeanThunk::new(closure);

        let mapped = thunk.map(|v| {
            let n = LeanNat::to_usize(&v).unwrap();
            let lean = v.lean_token();
            LeanNat::from_usize(lean, n + 1).unwrap().cast()
        });

        // map eagerly evaluates the source and wraps the result in a pure thunk.
        assert!(mapped.is_pure());
        let value = mapped.get().to_owned();
        let nat: LeanBound<'_, LeanNat> = value.cast();
        assert_eq!(LeanNat::to_usize(&nat)?, 43);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_map_on_pure_thunk() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let thunk: LeanThunk<'_, LeanNat> = LeanThunk::pure(LeanNat::from_usize(lean, 5)?);

        let mapped = thunk.map(|v| {
            let n = LeanNat::to_usize(&v).unwrap();
            let lean = v.lean_token();
            LeanNat::from_usize(lean, n + 1).unwrap().cast()
        });

        let value = mapped.get().to_owned();
        let nat: LeanBound<'_, LeanNat> = value.cast();
        assert_eq!(LeanNat::to_usize(&nat)?, 6);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bind_chains_thunks() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, thunk_nat_42)?;
        let thunk: LeanThunk<'_, LeanNat> = LeanThunk::new(closure);

        // bind evaluates the source and returns the thunk produced by f.
        let bound = thunk.bind(|v| {
            let n = LeanNat::to_usize(&v).unwrap();
            let lean = v.lean_token();
            let val = LeanNat::from_usize(lean, n * 2).unwrap();
            LeanThunk::pure(val.cast())
        });

        let value = bound.get().to_owned();
        let nat: LeanBound<'_, LeanNat> = value.cast();
        assert_eq!(LeanNat::to_usize(&nat)?, 84);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_zero_value_boundary() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, thunk_nat_zero)?;
        let thunk: LeanThunk<'_, LeanNat> = LeanThunk::new(closure);

        let value = thunk.get().to_owned();
        assert_eq!(LeanNat::to_usize(&value)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_max_small_nat_boundary() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, thunk_nat_max_small)?;
        let thunk: LeanThunk<'_, LeanNat> = LeanThunk::new(closure);

        let value = thunk.get().to_owned();
        assert_eq!(LeanNat::to_usize(&value)?, usize::MAX >> 1);

        Ok(())
    });

    assert!(result.is_ok());
}
