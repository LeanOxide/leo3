//! Option operation tests for Leo3
//!
//! Comprehensive coverage of the LeanOption wrapper: none/some creation,
//! inspection, value extraction, and the full combinator surface
//! (getD, map, bind, filter, toList, merge, or, isEqSome, all, any, join)
//! including edge paths (none branches, boundary values, and error paths).

#![cfg(feature = "runtime-tests")]

use leo3::instance::LeanAny;
use leo3::prelude::*;

/// Extract a usize from a LeanAny bound that is assumed to hold a small nat.
fn nat_value(v: &LeanBound<'_, LeanAny>) -> usize {
    unsafe { leo3::ffi::inline::lean_unbox(v.as_ptr()) }
}

#[test]
fn test_none_creation_and_inspection() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::none(lean)?;

        assert!(LeanOption::isNone(&opt));
        assert!(!LeanOption::isSome(&opt));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_some_creation_and_inspection() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let value = LeanNat::from_usize(lean, 42)?;
        let opt = LeanOption::some(value.cast())?;

        assert!(!LeanOption::isNone(&opt));
        assert!(LeanOption::isSome(&opt));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_get_some_returns_value() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 42)?.cast())?;

        let retrieved = LeanOption::get(&opt);
        assert!(retrieved.is_some());
        assert_eq!(nat_value(&retrieved.expect("some")), 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_get_none_returns_none() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::none(lean)?;

        let retrieved = LeanOption::get(&opt);
        assert!(retrieved.is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_to_rust_option_some_and_none() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let some_opt = LeanOption::some(LeanNat::from_usize(lean, 100)?.cast())?;
        let none_opt = LeanOption::none(lean)?;

        let some_rust = LeanOption::toRustOption(&some_opt);
        assert!(some_rust.is_some());
        assert_eq!(nat_value(&some_rust.expect("some")), 100);

        let none_rust = LeanOption::toRustOption(&none_opt);
        assert!(none_rust.is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_getd_some_uses_stored_value() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 42)?.cast())?;
        let default = LeanNat::from_usize(lean, 0)?.cast();

        let val = LeanOption::getD(&opt, default);
        assert_eq!(nat_value(&val), 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_getd_none_uses_default() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::none(lean)?;
        let default = LeanNat::from_usize(lean, 7)?.cast();

        let val = LeanOption::getD(&opt, default);
        assert_eq!(nat_value(&val), 7);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_map_some_applies_function() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 21)?.cast())?;

        let mapped = LeanOption::map(opt, |v| {
            let sum = LeanNat::from_usize(lean, nat_value(&v) * 2)?;
            Ok(sum.cast())
        })?;

        assert!(LeanOption::isSome(&mapped));
        let got = LeanOption::get(&mapped).expect("some");
        assert_eq!(nat_value(&got), 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_map_none_returns_none() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::none(lean)?;

        // The function must not be called; returning an error here would
        // surface if map wrongly applied it to a none.
        let mapped = LeanOption::map(opt, |_v| Err(LeanError::conversion("map called on none")))?;

        assert!(LeanOption::isNone(&mapped));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_map_zero_boundary() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 0)?.cast())?;

        let mapped = LeanOption::map(opt, |v| {
            let sum = LeanNat::from_usize(lean, nat_value(&v) + 1)?;
            Ok(sum.cast())
        })?;

        let got = LeanOption::get(&mapped).expect("some");
        assert_eq!(nat_value(&got), 1);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bind_some_to_some() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 5)?.cast())?;

        let bound = LeanOption::bind(opt, |v| {
            let next = LeanNat::from_usize(lean, nat_value(&v) + 1)?;
            LeanOption::some(next.cast())
        })?;

        assert!(LeanOption::isSome(&bound));
        let got = LeanOption::get(&bound).expect("some");
        assert_eq!(nat_value(&got), 6);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bind_some_to_none() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 5)?.cast())?;

        let bound = LeanOption::bind(opt, |_v| LeanOption::none(lean))?;

        assert!(LeanOption::isNone(&bound));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bind_none_stays_none() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::none(lean)?;

        // The function must not be called for a none option.
        let bound = LeanOption::bind(opt, |_v| Err(LeanError::conversion("bind called on none")))?;

        assert!(LeanOption::isNone(&bound));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_filter_some_predicate_true() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 42)?.cast())?;

        let filtered = LeanOption::filter(opt, |v| nat_value(v) == 42)?;

        assert!(LeanOption::isSome(&filtered));
        let got = LeanOption::get(&filtered).expect("some");
        assert_eq!(nat_value(&got), 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_filter_some_predicate_false() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 42)?.cast())?;

        let filtered = LeanOption::filter(opt, |v| nat_value(v) != 42)?;

        assert!(LeanOption::isNone(&filtered));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_filter_none_returns_none() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::none(lean)?;

        let filtered = LeanOption::filter(opt, |_v| true)?;

        assert!(LeanOption::isNone(&filtered));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_to_list_some_singleton() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 42)?.cast())?;

        let list = LeanOption::toList(opt)?;

        assert!(!LeanList::isEmpty(&list));
        assert_eq!(LeanList::length(&list), 1);
        let head = LeanList::head(&list).expect("head of singleton");
        assert_eq!(nat_value(&head), 42);
        let tail = LeanList::tail(&list).expect("tail of singleton");
        assert!(LeanList::isEmpty(&tail));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_to_list_none_empty() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::none(lean)?;

        let list = LeanOption::toList(opt)?;

        assert!(LeanList::isEmpty(&list));
        assert_eq!(LeanList::length(&list), 0);
        assert!(LeanList::head(&list).is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_merge_both_some() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanOption::some(LeanNat::from_usize(lean, 3)?.cast())?;
        let b = LeanOption::some(LeanNat::from_usize(lean, 4)?.cast())?;

        let merged = LeanOption::merge(a, b, |x, y| {
            let sum = LeanNat::from_usize(lean, nat_value(&x) + nat_value(&y))?;
            Ok(sum.cast())
        })?;

        assert!(LeanOption::isSome(&merged));
        let got = LeanOption::get(&merged).expect("some");
        assert_eq!(nat_value(&got), 7);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_merge_first_none_returns_second() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanOption::none(lean)?;
        let b = LeanOption::some(LeanNat::from_usize(lean, 9)?.cast())?;

        // The merge function must not be called when one side is none.
        let merged = LeanOption::merge(a, b, |_x, _y| {
            Err(LeanError::conversion("merge called with a none"))
        })?;

        assert!(LeanOption::isSome(&merged));
        let got = LeanOption::get(&merged).expect("some");
        assert_eq!(nat_value(&got), 9);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_merge_second_none_returns_first() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanOption::some(LeanNat::from_usize(lean, 9)?.cast())?;
        let b = LeanOption::none(lean)?;

        let merged = LeanOption::merge(a, b, |_x, _y| {
            Err(LeanError::conversion("merge called with a none"))
        })?;

        assert!(LeanOption::isSome(&merged));
        let got = LeanOption::get(&merged).expect("some");
        assert_eq!(nat_value(&got), 9);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_merge_both_none() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanOption::none(lean)?;
        let b = LeanOption::none(lean)?;

        let merged = LeanOption::merge(a, b, |_x, _y| {
            Err(LeanError::conversion("merge called with a none"))
        })?;

        assert!(LeanOption::isNone(&merged));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_or_some_none() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanOption::some(LeanNat::from_usize(lean, 1)?.cast())?;
        let b = LeanOption::none(lean)?;

        let ored = LeanOption::or(a, b)?;

        assert!(LeanOption::isSome(&ored));
        let got = LeanOption::get(&ored).expect("some");
        assert_eq!(nat_value(&got), 1);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_or_none_some() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanOption::none(lean)?;
        let b = LeanOption::some(LeanNat::from_usize(lean, 2)?.cast())?;

        let ored = LeanOption::or(a, b)?;

        assert!(LeanOption::isSome(&ored));
        let got = LeanOption::get(&ored).expect("some");
        assert_eq!(nat_value(&got), 2);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_or_none_none() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanOption::none(lean)?;
        let b = LeanOption::none(lean)?;

        let ored = LeanOption::or(a, b)?;

        assert!(LeanOption::isNone(&ored));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_is_eq_some_equal_values() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 42)?.cast())?;
        let target = LeanNat::from_usize(lean, 42)?;
        let eq =
            |a: &LeanBound<'_, LeanAny>, b: &LeanBound<'_, LeanAny>| nat_value(a) == nat_value(b);

        assert!(LeanOption::isEqSome(&opt, &target.cast(), eq));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_is_eq_some_different_values() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 42)?.cast())?;
        let target = LeanNat::from_usize(lean, 43)?;
        let eq =
            |a: &LeanBound<'_, LeanAny>, b: &LeanBound<'_, LeanAny>| nat_value(a) == nat_value(b);

        assert!(!LeanOption::isEqSome(&opt, &target.cast(), eq));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_is_eq_some_none_option() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::none(lean)?;
        let target = LeanNat::from_usize(lean, 42)?;
        let eq =
            |a: &LeanBound<'_, LeanAny>, b: &LeanBound<'_, LeanAny>| nat_value(a) == nat_value(b);

        // isEqSome on none must be false without calling eq.
        assert!(!LeanOption::isEqSome(&opt, &target.cast(), eq));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_all_some_predicate_true() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 42)?.cast())?;

        assert!(LeanOption::all(&opt, |v| nat_value(v) == 42));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_all_some_predicate_false() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 42)?.cast())?;

        assert!(!LeanOption::all(&opt, |v| nat_value(v) > 100));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_all_none_is_vacuously_true() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::none(lean)?;

        // all on none is true regardless of the predicate.
        assert!(LeanOption::all(&opt, |_v| false));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_any_some_predicate_true() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 42)?.cast())?;

        assert!(LeanOption::any(&opt, |v| nat_value(v) == 42));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_any_some_predicate_false() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::some(LeanNat::from_usize(lean, 42)?.cast())?;

        assert!(!LeanOption::any(&opt, |v| nat_value(v) > 100));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_any_none_is_false() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let opt = LeanOption::none(lean)?;

        // any on none is false regardless of the predicate.
        assert!(!LeanOption::any(&opt, |_v| true));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_join_some_some_flattens() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let inner = LeanOption::some(LeanNat::from_usize(lean, 42)?.cast())?;
        let outer = LeanOption::some(inner.cast())?;

        let joined = LeanOption::join(outer)?;

        assert!(LeanOption::isSome(&joined));
        let got = LeanOption::get(&joined).expect("some");
        assert_eq!(nat_value(&got), 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_join_some_none_flattens_to_none() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let inner = LeanOption::none(lean)?;
        let outer = LeanOption::some(inner.cast())?;

        let joined = LeanOption::join(outer)?;

        assert!(LeanOption::isNone(&joined));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_join_none_stays_none() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let outer = LeanOption::none(lean)?;

        let joined = LeanOption::join(outer)?;

        assert!(LeanOption::isNone(&joined));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_large_value_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 2^62 fits in the small-nat representation (max small is 2^63-1).
        let big_small = 1usize << 62;
        let opt = LeanOption::some(LeanNat::from_usize(lean, big_small)?.cast())?;

        let got = LeanOption::get(&opt).expect("some");
        let nat: LeanBound<'_, LeanNat> = got.cast();
        assert_eq!(LeanNat::to_usize(&nat)?, big_small);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_big_nat_to_usize_error_path() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // usize::MAX exceeds the small-nat representation, so Lean builds a
        // big nat, and to_usize must report an error instead of truncating.
        let opt = LeanOption::some(LeanNat::from_usize(lean, usize::MAX)?.cast())?;

        let got = LeanOption::get(&opt).expect("some");
        let nat: LeanBound<'_, LeanNat> = got.cast();
        assert!(LeanNat::to_usize(&nat).is_err());

        Ok(())
    });

    assert!(result.is_ok());
}
