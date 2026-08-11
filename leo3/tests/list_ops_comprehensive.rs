//! Comprehensive LeanList tests for Leo3
//!
//! Exercises every public method on `LeanList` (leo3/src/types/list.rs):
//! nil, cons, singleton, isEmpty, head, tail, length, append, reverse,
//! headOpt, tailOpt, headD, get, getD, getLast, contains, all, any, find,
//! filter, map, foldl, take, drop, countP, plus the Debug impl — including
//! error/edge paths (empty lists, out-of-bounds indexes, missing elements).

#![cfg(feature = "runtime-tests")]

use leo3::instance::LeanAny;
use leo3::prelude::*;

/// Extract the `usize` value from a `LeanAny` bound that wraps a `LeanNat`.
fn any_nat(any: &LeanBound<'_, LeanAny>) -> usize {
    let nat: LeanBound<'_, LeanNat> = any.clone().cast();
    LeanNat::to_usize(&nat).expect("expected a small LeanNat element")
}

/// Equality helper comparing two `LeanAny`-wrapped nats.
fn nat_eq(a: &LeanBound<'_, LeanAny>, b: &LeanBound<'_, LeanAny>) -> bool {
    any_nat(a) == any_nat(b)
}

/// Build a `LeanList` of `LeanNat`s from Rust values.
fn build_nat_list<'l>(lean: Lean<'l>, values: &[usize]) -> LeanResult<LeanBound<'l, LeanList>> {
    let mut list = LeanList::nil(lean)?;
    for v in values.iter().rev() {
        let elem = LeanNat::from_usize(lean, *v)?;
        list = LeanList::cons(elem.cast(), list)?;
    }
    Ok(list)
}

#[test]
fn test_list_nil() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = LeanList::nil(lean)?;

        assert!(LeanList::isEmpty(&list));
        assert_eq!(LeanList::length(&list), 0);
        // head/tail/get/getLast of an empty list are all None
        assert!(LeanList::head(&list).is_none());
        assert!(LeanList::tail(&list).is_none());
        assert!(LeanList::get(&list, 0).is_none());
        assert!(LeanList::getLast(&list).is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_cons_values() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Build [1, 2, 3] via cons
        let list = build_nat_list(lean, &[1, 2, 3])?;

        assert!(!LeanList::isEmpty(&list));
        assert_eq!(LeanList::length(&list), 3);

        // Verify every element value in order
        assert_eq!(any_nat(&LeanList::get(&list, 0).expect("elem 0")), 1);
        assert_eq!(any_nat(&LeanList::get(&list, 1).expect("elem 1")), 2);
        assert_eq!(any_nat(&LeanList::get(&list, 2).expect("elem 2")), 3);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_head_value() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[7, 8, 9])?;

        let head = LeanList::head(&list);
        assert!(head.is_some());
        assert_eq!(any_nat(&head.unwrap()), 7);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_head_empty() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = LeanList::nil(lean)?;
        assert!(LeanList::head(&list).is_none());
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_tail_value() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 2, 3])?;

        let tail = LeanList::tail(&list).expect("tail of non-empty list");
        assert_eq!(LeanList::length(&tail), 2);
        assert_eq!(any_nat(&LeanList::head(&tail).expect("head of tail")), 2);

        // Tail of the tail is a singleton
        let tail2 = LeanList::tail(&tail).expect("tail of [2, 3]");
        assert_eq!(LeanList::length(&tail2), 1);
        assert_eq!(any_nat(&LeanList::get(&tail2, 0).expect("elem")), 3);

        // Tail of singleton is empty
        let tail3 = LeanList::tail(&tail2).expect("tail of [3]");
        assert!(LeanList::isEmpty(&tail3));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_tail_empty() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = LeanList::nil(lean)?;
        assert!(LeanList::tail(&list).is_none());
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_length_multiple() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        assert_eq!(LeanList::length(&LeanList::nil(lean)?), 0);
        assert_eq!(LeanList::length(&build_nat_list(lean, &[1])?), 1);
        assert_eq!(
            LeanList::length(&build_nat_list(lean, &[1, 2, 3, 4, 5])?),
            5
        );
        assert_eq!(LeanList::length(&build_nat_list(lean, &[0; 10])?), 10);
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_singleton() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let elem = LeanNat::from_usize(lean, 42)?;
        let list = LeanList::singleton(elem.cast())?;

        assert_eq!(LeanList::length(&list), 1);
        assert_eq!(any_nat(&LeanList::head(&list).expect("head")), 42);

        // Singleton of zero
        let zero = LeanNat::from_usize(lean, 0)?;
        let list0 = LeanList::singleton(zero.cast())?;
        assert_eq!(LeanList::length(&list0), 1);
        assert_eq!(any_nat(&LeanList::head(&list0).expect("head")), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_append() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let xs = build_nat_list(lean, &[1, 2])?;
        let ys = build_nat_list(lean, &[3, 4])?;
        let zs = LeanList::append(xs, ys)?;

        assert_eq!(LeanList::length(&zs), 4);
        assert_eq!(any_nat(&LeanList::get(&zs, 0).expect("0")), 1);
        assert_eq!(any_nat(&LeanList::get(&zs, 1).expect("1")), 2);
        assert_eq!(any_nat(&LeanList::get(&zs, 2).expect("2")), 3);
        assert_eq!(any_nat(&LeanList::get(&zs, 3).expect("3")), 4);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_append_empty() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let empty = LeanList::nil(lean)?;
        let xs = build_nat_list(lean, &[5, 6])?;

        // Empty ++ non-empty
        let a = LeanList::append(empty, xs)?;
        assert_eq!(LeanList::length(&a), 2);
        assert_eq!(any_nat(&LeanList::get(&a, 0).expect("0")), 5);

        // Non-empty ++ empty
        let empty2 = LeanList::nil(lean)?;
        let xs2 = build_nat_list(lean, &[5, 6])?;
        let b = LeanList::append(xs2, empty2)?;
        assert_eq!(LeanList::length(&b), 2);
        assert_eq!(any_nat(&LeanList::get(&b, 1).expect("1")), 6);

        // Empty ++ Empty
        let e1 = LeanList::nil(lean)?;
        let e2 = LeanList::nil(lean)?;
        let c = LeanList::append(e1, e2)?;
        assert!(LeanList::isEmpty(&c));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_reverse() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 2, 3])?;
        let rev = LeanList::reverse(list)?;

        assert_eq!(LeanList::length(&rev), 3);
        assert_eq!(any_nat(&LeanList::get(&rev, 0).expect("0")), 3);
        assert_eq!(any_nat(&LeanList::get(&rev, 1).expect("1")), 2);
        assert_eq!(any_nat(&LeanList::get(&rev, 2).expect("2")), 1);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_reverse_empty_and_singleton() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let empty = LeanList::reverse(LeanList::nil(lean)?)?;
        assert!(LeanList::isEmpty(&empty));

        let elem = LeanNat::from_usize(lean, 9)?;
        let single = LeanList::reverse(LeanList::singleton(elem.cast())?)?;
        assert_eq!(LeanList::length(&single), 1);
        assert_eq!(any_nat(&LeanList::head(&single).expect("head")), 9);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_head_opt_some() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[10, 20])?;
        let opt = LeanList::headOpt(&list)?;

        assert!(LeanOption::isSome(&opt));
        let head = LeanOption::get(&opt).expect("some head");
        assert_eq!(any_nat(&head), 10);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_head_opt_none() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = LeanList::nil(lean)?;
        let opt = LeanList::headOpt(&list)?;

        assert!(LeanOption::isNone(&opt));
        assert!(LeanOption::get(&opt).is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_tail_opt() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 2, 3])?;
        let opt = LeanList::tailOpt(&list)?;

        assert!(LeanOption::isSome(&opt));
        let tail_any = LeanOption::get(&opt).expect("some tail");
        let tail: LeanBound<'_, LeanList> = tail_any.cast();
        assert_eq!(LeanList::length(&tail), 2);
        assert_eq!(any_nat(&LeanList::get(&tail, 0).expect("0")), 2);

        // Empty list: tailOpt yields none
        let empty = LeanList::nil(lean)?;
        let opt_empty = LeanList::tailOpt(&empty)?;
        assert!(LeanOption::isNone(&opt_empty));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_head_default_some() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[5, 6])?;
        let default = LeanNat::from_usize(lean, 999)?;
        let head = LeanList::headD(&list, default.cast());

        // Non-empty list: head wins over the (consumed) default
        assert_eq!(any_nat(&head), 5);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_head_default_none() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = LeanList::nil(lean)?;
        let default = LeanNat::from_usize(lean, 999)?;
        let head = LeanList::headD(&list, default.cast());

        // Empty list: default is returned
        assert_eq!(any_nat(&head), 999);

        // Default of zero on an empty list
        let zero = LeanNat::from_usize(lean, 0)?;
        let head0 = LeanList::headD(&list, zero.cast());
        assert_eq!(any_nat(&head0), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_get() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[100, 200, 300])?;

        assert_eq!(any_nat(&LeanList::get(&list, 0).expect("0")), 100);
        assert_eq!(any_nat(&LeanList::get(&list, 1).expect("1")), 200);
        assert_eq!(any_nat(&LeanList::get(&list, 2).expect("2")), 300);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_get_out_of_bounds() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 2])?;

        // index == length
        assert!(LeanList::get(&list, 2).is_none());
        // index > length
        assert!(LeanList::get(&list, 5).is_none());
        // huge index
        assert!(LeanList::get(&list, usize::MAX).is_none());
        // empty list, any index
        let empty = LeanList::nil(lean)?;
        assert!(LeanList::get(&empty, 0).is_none());
        assert!(LeanList::get(&empty, usize::MAX).is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_get_default() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[7, 8])?;

        // In bounds: element returned, default consumed
        let def = LeanNat::from_usize(lean, 0)?;
        assert_eq!(any_nat(&LeanList::getD(&list, 1, def.cast())), 8);

        // Out of bounds: default returned
        let def = LeanNat::from_usize(lean, 42)?;
        assert_eq!(any_nat(&LeanList::getD(&list, 9, def.cast())), 42);

        // Empty list with default
        let empty = LeanList::nil(lean)?;
        let def = LeanNat::from_usize(lean, 1)?;
        assert_eq!(any_nat(&LeanList::getD(&empty, 0, def.cast())), 1);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_get_last() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 2, 3, 4])?;
        let last = LeanList::getLast(&list);
        assert!(last.is_some());
        assert_eq!(any_nat(&last.unwrap()), 4);

        // Singleton: last == head
        let single = build_nat_list(lean, &[9])?;
        assert_eq!(any_nat(&LeanList::getLast(&single).expect("last")), 9);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_get_last_empty() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = LeanList::nil(lean)?;
        assert!(LeanList::getLast(&list).is_none());
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_contains() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 2, 3, 2])?;

        // Present
        let target = LeanNat::from_usize(lean, 2)?;
        let target_any: LeanBound<'_, LeanAny> = target.cast();
        assert!(LeanList::contains(&list, &target_any, nat_eq));

        // Absent
        let missing = LeanNat::from_usize(lean, 99)?;
        let missing_any: LeanBound<'_, LeanAny> = missing.cast();
        assert!(!LeanList::contains(&list, &missing_any, nat_eq));

        // Zero is present when in the list
        let zero_list = build_nat_list(lean, &[0, 1])?;
        let zero = LeanNat::from_usize(lean, 0)?;
        let zero_any: LeanBound<'_, LeanAny> = zero.cast();
        assert!(LeanList::contains(&zero_list, &zero_any, nat_eq));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_contains_empty() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = LeanList::nil(lean)?;
        let target = LeanNat::from_usize(lean, 1)?;
        let target_any: LeanBound<'_, LeanAny> = target.cast();
        assert!(!LeanList::contains(&list, &target_any, nat_eq));
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_all() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let evens = build_nat_list(lean, &[2, 4, 6])?;
        assert!(LeanList::all(&evens, |e| any_nat(e).is_multiple_of(2)));
        assert!(!LeanList::all(&evens, |e| any_nat(e) > 3));

        // Vacuous truth on empty list
        let empty = LeanList::nil(lean)?;
        assert!(LeanList::all(&empty, |_| false));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_any() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 3, 4])?;
        assert!(LeanList::any(&list, |e| any_nat(e).is_multiple_of(2)));
        assert!(!LeanList::any(&list, |e| any_nat(e) > 100));

        // Empty list: any is false
        let empty = LeanList::nil(lean)?;
        assert!(!LeanList::any(&empty, |_| true));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_find() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 4, 3, 8])?;

        // First even element
        let found = LeanList::find(&list, |e| any_nat(e).is_multiple_of(2));
        assert!(found.is_some());
        assert_eq!(any_nat(&found.unwrap()), 4);

        // Not found
        assert!(LeanList::find(&list, |e| any_nat(e) > 100).is_none());

        // Empty list
        let empty = LeanList::nil(lean)?;
        assert!(LeanList::find(&empty, |_| true).is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_filter() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 2, 3, 4, 5, 6])?;
        let filtered = LeanList::filter(list, |e| any_nat(e).is_multiple_of(2))?;

        assert_eq!(LeanList::length(&filtered), 3);
        assert_eq!(any_nat(&LeanList::get(&filtered, 0).expect("0")), 2);
        assert_eq!(any_nat(&LeanList::get(&filtered, 1).expect("1")), 4);
        assert_eq!(any_nat(&LeanList::get(&filtered, 2).expect("2")), 6);

        // Filter that keeps everything
        let list = build_nat_list(lean, &[1, 2])?;
        let kept = LeanList::filter(list, |_| true)?;
        assert_eq!(LeanList::length(&kept), 2);

        // Filter that drops everything
        let list = build_nat_list(lean, &[1, 2])?;
        let dropped = LeanList::filter(list, |_| false)?;
        assert!(LeanList::isEmpty(&dropped));

        // Filter on empty list
        let empty = LeanList::nil(lean)?;
        let filtered_empty = LeanList::filter(empty, |_| true)?;
        assert!(LeanList::isEmpty(&filtered_empty));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_map() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 2, 3])?;
        let mapped = LeanList::map(list, |elem| {
            let v = any_nat(&elem);
            Ok(LeanNat::from_usize(lean, v * 2)?.cast())
        })?;

        assert_eq!(LeanList::length(&mapped), 3);
        assert_eq!(any_nat(&LeanList::get(&mapped, 0).expect("0")), 2);
        assert_eq!(any_nat(&LeanList::get(&mapped, 1).expect("1")), 4);
        assert_eq!(any_nat(&LeanList::get(&mapped, 2).expect("2")), 6);

        // Map over empty list
        let empty = LeanList::nil(lean)?;
        let mapped_empty = LeanList::map(empty, |elem| {
            let v = any_nat(&elem);
            Ok(LeanNat::from_usize(lean, v + 1)?.cast())
        })?;
        assert!(LeanList::isEmpty(&mapped_empty));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_foldl() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 2, 3, 4])?;
        let sum = LeanList::foldl(&list, 0usize, |acc, e| acc + any_nat(&e));
        assert_eq!(sum, 10);

        // Order-observable fold: digits
        let list = build_nat_list(lean, &[1, 2, 3])?;
        let num = LeanList::foldl(&list, 0usize, |acc, e| acc * 10 + any_nat(&e));
        assert_eq!(num, 123);

        // Fold over empty list returns init
        let empty = LeanList::nil(lean)?;
        assert_eq!(LeanList::foldl(&empty, 42usize, |acc, _| acc + 1), 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_take() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 2, 3, 4, 5])?;

        // take 0 -> empty
        let taken0 = LeanList::take(list, 0)?;
        assert!(LeanList::isEmpty(&taken0));

        // take 2 -> [1, 2]
        let list = build_nat_list(lean, &[1, 2, 3, 4, 5])?;
        let taken2 = LeanList::take(list, 2)?;
        assert_eq!(LeanList::length(&taken2), 2);
        assert_eq!(any_nat(&LeanList::get(&taken2, 0).expect("0")), 1);
        assert_eq!(any_nat(&LeanList::get(&taken2, 1).expect("1")), 2);

        // take more than length -> whole list
        let list = build_nat_list(lean, &[1, 2])?;
        let taken9 = LeanList::take(list, 9)?;
        assert_eq!(LeanList::length(&taken9), 2);

        // take on empty list
        let empty = LeanList::nil(lean)?;
        let taken_empty = LeanList::take(empty, 3)?;
        assert!(LeanList::isEmpty(&taken_empty));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_drop() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 2, 3, 4, 5])?;

        // drop 0 -> whole list
        let dropped0 = LeanList::drop(list, 0)?;
        assert_eq!(LeanList::length(&dropped0), 5);

        // drop 2 -> [3, 4, 5]
        let list = build_nat_list(lean, &[1, 2, 3, 4, 5])?;
        let dropped2 = LeanList::drop(list, 2)?;
        assert_eq!(LeanList::length(&dropped2), 3);
        assert_eq!(any_nat(&LeanList::get(&dropped2, 0).expect("0")), 3);
        assert_eq!(any_nat(&LeanList::get(&dropped2, 2).expect("2")), 5);

        // drop more than length -> empty
        let list = build_nat_list(lean, &[1, 2])?;
        let dropped9 = LeanList::drop(list, 9)?;
        assert!(LeanList::isEmpty(&dropped9));

        // drop exactly length -> empty
        let list = build_nat_list(lean, &[1, 2])?;
        let dropped2exact = LeanList::drop(list, 2)?;
        assert!(LeanList::isEmpty(&dropped2exact));

        // drop on empty list
        let empty = LeanList::nil(lean)?;
        let dropped_empty = LeanList::drop(empty, 1)?;
        assert!(LeanList::isEmpty(&dropped_empty));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_count_pred() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 2, 3, 4, 5, 6])?;

        assert_eq!(LeanList::countP(&list, |e| any_nat(e).is_multiple_of(2)), 3);
        assert_eq!(LeanList::countP(&list, |_| true), 6);
        assert_eq!(LeanList::countP(&list, |e| any_nat(e) > 100), 0);

        // Empty list counts zero
        let empty = LeanList::nil(lean)?;
        assert_eq!(LeanList::countP(&empty, |_| true), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_boundary_values() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 0, 1, 2^32 (still a small nat), and usize::MAX (big nat)
        let list = build_nat_list(lean, &[0, 1, 1 << 32, usize::MAX])?;
        assert_eq!(LeanList::length(&list), 4);

        assert_eq!(any_nat(&LeanList::get(&list, 0).expect("0")), 0);
        assert_eq!(any_nat(&LeanList::get(&list, 1).expect("1")), 1);
        assert_eq!(any_nat(&LeanList::get(&list, 2).expect("2")), 1usize << 32);

        // usize::MAX round-trips as a big nat: to_usize reports it is too large
        let big: LeanBound<'_, LeanNat> = LeanList::get(&list, 3).expect("3").cast();
        assert!(!LeanNat::is_small(&big));
        assert!(LeanNat::to_usize(&big).is_err());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_debug_format() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = build_nat_list(lean, &[1, 2, 3])?;
        let dbg = format!("{:?}", list);
        assert!(dbg.contains("LeanList"));
        assert!(dbg.contains("length: 3"));

        let empty = LeanList::nil(lean)?;
        let dbg_empty = format!("{:?}", empty);
        assert!(dbg_empty.contains("length: 0"));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_list_pipeline() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Build [1..=6], filter evens, map double, reverse, take 2
        let list = build_nat_list(lean, &[1, 2, 3, 4, 5, 6])?;
        let filtered = LeanList::filter(list, |e| any_nat(e).is_multiple_of(2))?; // [2, 4, 6]
        let mapped = LeanList::map(filtered, |elem| {
            let v = any_nat(&elem);
            Ok(LeanNat::from_usize(lean, v * 2)?.cast())
        })?; // [4, 8, 12]
        let reversed = LeanList::reverse(mapped)?; // [12, 8, 4]
        let taken = LeanList::take(reversed, 2)?; // [12, 8]

        assert_eq!(LeanList::length(&taken), 2);
        assert_eq!(any_nat(&LeanList::get(&taken, 0).expect("0")), 12);
        assert_eq!(any_nat(&LeanList::get(&taken, 1).expect("1")), 8);

        // Append the remainder back and fold
        let appended = LeanList::append(taken, build_nat_list(lean, &[1, 1])?)?; // [12, 8, 1, 1]
        let sum = LeanList::foldl(&appended, 0usize, |acc, e| acc + any_nat(&e));
        assert_eq!(sum, 22);

        Ok(())
    });

    assert!(result.is_ok());
}
