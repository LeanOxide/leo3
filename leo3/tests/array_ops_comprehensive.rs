//! Comprehensive LeanArray tests for Leo3
//!
//! Exercises every public method on `LeanArray` (leo3/src/types/array.rs):
//! empty, emptyWithCapacity, with_capacity, size, capacity, isEmpty, push,
//! get, set, pop, swap, replicate, getD, back, mk, push_unchecked, singleton,
//! range, extract, toList, reverse, take, drop, append, contains, all, any,
//! find, findIdx, filter, map, foldl, foldr, countP, flatten, zip,
//! isPrefixOf, plus the Debug impl — including error/edge paths (empty
//! arrays, out-of-bounds indexes, wrong-tag accessors).

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

/// Build a `LeanArray` of `LeanNat`s from Rust values.
fn build_nat_array<'l>(lean: Lean<'l>, values: &[usize]) -> LeanResult<LeanBound<'l, LeanArray>> {
    let mut arr = LeanArray::empty(lean)?;
    for v in values {
        let elem = LeanNat::from_usize(lean, *v)?;
        arr = LeanArray::push(arr, elem.cast())?;
    }
    Ok(arr)
}

#[test]
fn test_array_empty() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = LeanArray::empty(lean)?;

        assert_eq!(LeanArray::size(&arr), 0);
        assert!(LeanArray::isEmpty(&arr));
        assert!(LeanArray::back(&arr).is_none());
        assert!(LeanArray::get(&arr, 0).is_none());
        assert!(LeanArray::capacity(&arr) >= LeanArray::size(&arr));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_empty_with_capacity() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = LeanArray::emptyWithCapacity(lean, 100)?;

        assert_eq!(LeanArray::size(&arr), 0);
        assert!(LeanArray::isEmpty(&arr));
        // lean_mk_empty_array_with_capacity allocates the exact capacity
        assert_eq!(LeanArray::capacity(&arr), 100);

        // Capacity 0 is valid
        let zero = LeanArray::emptyWithCapacity(lean, 0)?;
        assert_eq!(LeanArray::size(&zero), 0);
        assert_eq!(LeanArray::capacity(&zero), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_with_capacity() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = LeanArray::with_capacity(lean, 100)?;

        assert_eq!(LeanArray::size(&arr), 0);
        assert!(LeanArray::isEmpty(&arr));
        // Capacity is grown by doubling, so it must be at least the request
        assert!(LeanArray::capacity(&arr) >= 100);

        // The pre-allocated array is usable for pushes
        let elem = LeanNat::from_usize(lean, 7)?;
        let arr = LeanArray::push(arr, elem.cast())?;
        assert_eq!(LeanArray::size(&arr), 1);
        assert_eq!(any_nat(&LeanArray::get(&arr, 0).expect("0")), 7);

        // Zero capacity
        let zero = LeanArray::with_capacity(lean, 0)?;
        assert_eq!(LeanArray::size(&zero), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_push_values() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let mut arr = LeanArray::empty(lean)?;
        assert_eq!(LeanArray::size(&arr), 0);

        for v in [10, 20, 30] {
            let elem = LeanNat::from_usize(lean, v)?;
            arr = LeanArray::push(arr, elem.cast())?;
        }

        assert_eq!(LeanArray::size(&arr), 3);
        assert!(!LeanArray::isEmpty(&arr));
        assert_eq!(any_nat(&LeanArray::get(&arr, 0).expect("0")), 10);
        assert_eq!(any_nat(&LeanArray::get(&arr, 1).expect("1")), 20);
        assert_eq!(any_nat(&LeanArray::get(&arr, 2).expect("2")), 30);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_push_growth() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Pushing many elements forces reallocations; capacity should grow
        let mut arr = LeanArray::emptyWithCapacity(lean, 2)?;
        for i in 0..64 {
            let elem = LeanNat::from_usize(lean, i)?;
            arr = LeanArray::push(arr, elem.cast())?;
        }

        assert_eq!(LeanArray::size(&arr), 64);
        assert!(LeanArray::capacity(&arr) >= 64);
        assert_eq!(any_nat(&LeanArray::get(&arr, 0).expect("0")), 0);
        assert_eq!(any_nat(&LeanArray::get(&arr, 32).expect("32")), 32);
        assert_eq!(any_nat(&LeanArray::get(&arr, 63).expect("63")), 63);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_get() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3])?;

        assert_eq!(any_nat(&LeanArray::get(&arr, 0).expect("0")), 1);
        assert_eq!(any_nat(&LeanArray::get(&arr, 1).expect("1")), 2);
        assert_eq!(any_nat(&LeanArray::get(&arr, 2).expect("2")), 3);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_get_out_of_bounds() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2])?;

        // index == size
        assert!(LeanArray::get(&arr, 2).is_none());
        // index > size
        assert!(LeanArray::get(&arr, 5).is_none());
        // huge index
        assert!(LeanArray::get(&arr, usize::MAX).is_none());

        let empty = LeanArray::empty(lean)?;
        assert!(LeanArray::get(&empty, 0).is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_set() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3])?;

        let new_val = LeanNat::from_usize(lean, 100)?;
        let arr = LeanArray::set(arr, 1, new_val.cast())?;
        assert_eq!(LeanArray::size(&arr), 3);
        assert_eq!(any_nat(&LeanArray::get(&arr, 0).expect("0")), 1);
        assert_eq!(any_nat(&LeanArray::get(&arr, 1).expect("1")), 100);
        assert_eq!(any_nat(&LeanArray::get(&arr, 2).expect("2")), 3);

        // Setting the first and last element
        let first = LeanNat::from_usize(lean, 7)?;
        let arr = LeanArray::set(arr, 0, first.cast())?;
        let last = LeanNat::from_usize(lean, 9)?;
        let arr = LeanArray::set(arr, 2, last.cast())?;
        assert_eq!(any_nat(&LeanArray::get(&arr, 0).expect("0")), 7);
        assert_eq!(any_nat(&LeanArray::get(&arr, 2).expect("2")), 9);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_set_out_of_bounds() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2])?;
        let val = LeanNat::from_usize(lean, 9)?;

        let res: LeanResult<LeanBound<LeanArray>> = LeanArray::set(arr, 2, val.cast());
        assert!(res.is_err());

        // Empty array: any set is out of bounds
        let empty = LeanArray::empty(lean)?;
        let val = LeanNat::from_usize(lean, 9)?;
        let res: LeanResult<LeanBound<LeanArray>> = LeanArray::set(empty, 0, val.cast());
        assert!(res.is_err());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_pop() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3])?;

        let arr = LeanArray::pop(arr)?;
        assert_eq!(LeanArray::size(&arr), 2);
        assert_eq!(any_nat(&LeanArray::get(&arr, 0).expect("0")), 1);
        assert_eq!(any_nat(&LeanArray::get(&arr, 1).expect("1")), 2);

        let arr = LeanArray::pop(arr)?;
        assert_eq!(LeanArray::size(&arr), 1);
        assert_eq!(any_nat(&LeanArray::get(&arr, 0).expect("0")), 1);

        let arr = LeanArray::pop(arr)?;
        assert_eq!(LeanArray::size(&arr), 0);
        assert!(LeanArray::isEmpty(&arr));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_pop_empty_error() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let empty = LeanArray::empty(lean)?;
        let res: LeanResult<LeanBound<LeanArray>> = LeanArray::pop(empty);
        assert!(res.is_err());

        // Pop down to empty, then pop again
        let arr = build_nat_array(lean, &[1])?;
        let arr = LeanArray::pop(arr)?;
        assert!(LeanArray::isEmpty(&arr));
        let res: LeanResult<LeanBound<LeanArray>> = LeanArray::pop(arr);
        assert!(res.is_err());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_swap() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3])?;
        let arr = LeanArray::swap(arr, 0, 2)?;

        assert_eq!(LeanArray::size(&arr), 3);
        assert_eq!(any_nat(&LeanArray::get(&arr, 0).expect("0")), 3);
        assert_eq!(any_nat(&LeanArray::get(&arr, 1).expect("1")), 2);
        assert_eq!(any_nat(&LeanArray::get(&arr, 2).expect("2")), 1);

        // Swapping an index with itself is a no-op
        let arr = LeanArray::swap(arr, 1, 1)?;
        assert_eq!(any_nat(&LeanArray::get(&arr, 1).expect("1")), 2);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_swap_out_of_bounds() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2])?;

        let res: LeanResult<LeanBound<LeanArray>> = LeanArray::swap(arr, 2, 0);
        assert!(res.is_err());

        let arr = build_nat_array(lean, &[1, 2])?;
        let res: LeanResult<LeanBound<LeanArray>> = LeanArray::swap(arr, 0, 5);
        assert!(res.is_err());

        // Empty array: both indexes out of bounds
        let empty = LeanArray::empty(lean)?;
        let res: LeanResult<LeanBound<LeanArray>> = LeanArray::swap(empty, 0, 0);
        assert!(res.is_err());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_replicate() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let val = LeanNat::from_usize(lean, 42)?;
        let arr = LeanArray::replicate(5, val.cast())?;

        assert_eq!(LeanArray::size(&arr), 5);
        for i in 0..5 {
            assert_eq!(any_nat(&LeanArray::get(&arr, i).expect("elem")), 42);
        }

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_replicate_zero() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let val = LeanNat::from_usize(lean, 42)?;
        let arr = LeanArray::replicate(0, val.cast())?;
        assert_eq!(LeanArray::size(&arr), 0);
        assert!(LeanArray::isEmpty(&arr));
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_get_default_in_bounds() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[5, 6])?;
        let default = LeanNat::from_usize(lean, 0)?;
        let elem = LeanArray::getD(&arr, 1, default.cast())?;
        assert_eq!(any_nat(&elem), 6);

        // First element
        let default = LeanNat::from_usize(lean, 0)?;
        let elem = LeanArray::getD(&arr, 0, default.cast())?;
        assert_eq!(any_nat(&elem), 5);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_get_default_out_of_bounds() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[5, 6])?;
        let default = LeanNat::from_usize(lean, 99)?;
        let elem = LeanArray::getD(&arr, 10, default.cast())?;
        assert_eq!(any_nat(&elem), 99);

        // Default of zero on an out-of-bounds index
        let zero = LeanNat::from_usize(lean, 0)?;
        let elem = LeanArray::getD(&arr, usize::MAX, zero.cast())?;
        assert_eq!(any_nat(&elem), 0);

        // Empty array: always the default
        let empty = LeanArray::empty(lean)?;
        let default = LeanNat::from_usize(lean, 7)?;
        let elem = LeanArray::getD(&empty, 0, default.cast())?;
        assert_eq!(any_nat(&elem), 7);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_back() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3])?;
        let back = LeanArray::back(&arr);
        assert!(back.is_some());
        assert_eq!(any_nat(&back.unwrap()), 3);

        // Singleton: back == get(0)
        let single = build_nat_array(lean, &[9])?;
        assert_eq!(any_nat(&LeanArray::back(&single).expect("back")), 9);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_back_empty() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let empty = LeanArray::empty(lean)?;
        assert!(LeanArray::back(&empty).is_none());
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_mk_from_list() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Build a list [1, 2, 3]
        let mut list = LeanList::nil(lean)?;
        for v in [1, 2, 3].iter().rev() {
            let elem = LeanNat::from_usize(lean, *v)?;
            list = LeanList::cons(elem.cast(), list)?;
        }

        // SAFETY: `list` is a valid Lean list
        let arr = unsafe { LeanArray::mk(list.cast())? };
        assert_eq!(LeanArray::size(&arr), 3);
        assert_eq!(any_nat(&LeanArray::get(&arr, 0).expect("0")), 1);
        assert_eq!(any_nat(&LeanArray::get(&arr, 1).expect("1")), 2);
        assert_eq!(any_nat(&LeanArray::get(&arr, 2).expect("2")), 3);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_mk_empty_list() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let list = LeanList::nil(lean)?;
        // SAFETY: `list` is a valid (empty) Lean list
        let arr = unsafe { LeanArray::mk(list.cast())? };
        assert_eq!(LeanArray::size(&arr), 0);
        assert!(LeanArray::isEmpty(&arr));
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_push_unchecked() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Pre-allocate exact capacity 4, then push without capacity checks
        let mut arr = LeanArray::emptyWithCapacity(lean, 4)?;
        for v in [11, 22, 33] {
            let elem = LeanNat::from_usize(lean, v)?;
            // SAFETY: size (0,1,2) < capacity (4) at every call
            arr = unsafe { LeanArray::push_unchecked(arr, elem.cast())? };
        }

        assert_eq!(LeanArray::size(&arr), 3);
        assert_eq!(any_nat(&LeanArray::get(&arr, 0).expect("0")), 11);
        assert_eq!(any_nat(&LeanArray::get(&arr, 1).expect("1")), 22);
        assert_eq!(any_nat(&LeanArray::get(&arr, 2).expect("2")), 33);

        // Fill to exactly capacity
        let elem = LeanNat::from_usize(lean, 44)?;
        // SAFETY: size (3) < capacity (4)
        let arr = unsafe { LeanArray::push_unchecked(arr, elem.cast())? };
        assert_eq!(LeanArray::size(&arr), 4);
        assert_eq!(any_nat(&LeanArray::get(&arr, 3).expect("3")), 44);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_singleton() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let elem = LeanNat::from_usize(lean, 42)?;
        let arr = LeanArray::singleton(elem.cast())?;

        assert_eq!(LeanArray::size(&arr), 1);
        assert_eq!(any_nat(&LeanArray::get(&arr, 0).expect("0")), 42);
        assert_eq!(any_nat(&LeanArray::back(&arr).expect("back")), 42);

        // Singleton of zero
        let zero = LeanNat::from_usize(lean, 0)?;
        let arr0 = LeanArray::singleton(zero.cast())?;
        assert_eq!(any_nat(&LeanArray::get(&arr0, 0).expect("0")), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_range() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = LeanArray::range(lean, 5)?;
        assert_eq!(LeanArray::size(&arr), 5);
        for i in 0..5 {
            assert_eq!(any_nat(&LeanArray::get(&arr, i).expect("elem")), i);
        }

        // Empty range
        let empty = LeanArray::range(lean, 0)?;
        assert_eq!(LeanArray::size(&empty), 0);

        // Boundary: single-element range
        let single = LeanArray::range(lean, 1)?;
        assert_eq!(any_nat(&LeanArray::get(&single, 0).expect("0")), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_extract() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[0, 1, 2, 3, 4])?;

        // Middle slice
        let mid = LeanArray::extract(&arr, 1, 4)?;
        assert_eq!(LeanArray::size(&mid), 3);
        assert_eq!(any_nat(&LeanArray::get(&mid, 0).expect("0")), 1);
        assert_eq!(any_nat(&LeanArray::get(&mid, 2).expect("2")), 3);

        // Whole array
        let whole = LeanArray::extract(&arr, 0, 5)?;
        assert_eq!(LeanArray::size(&whole), 5);

        // Empty range (start == end)
        let empty = LeanArray::extract(&arr, 2, 2)?;
        assert_eq!(LeanArray::size(&empty), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_extract_bounds() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[0, 1, 2, 3, 4])?;

        // end beyond size is clamped
        let clamped = LeanArray::extract(&arr, 3, 100)?;
        assert_eq!(LeanArray::size(&clamped), 2);
        assert_eq!(any_nat(&LeanArray::get(&clamped, 0).expect("0")), 3);
        assert_eq!(any_nat(&LeanArray::get(&clamped, 1).expect("1")), 4);

        // start beyond size yields empty
        let past = LeanArray::extract(&arr, 10, 20)?;
        assert_eq!(LeanArray::size(&past), 0);

        // start > end yields empty
        let inverted = LeanArray::extract(&arr, 4, 1)?;
        assert_eq!(LeanArray::size(&inverted), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_to_list() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3])?;
        let list = LeanArray::toList(arr)?;

        assert_eq!(LeanList::length(&list), 3);
        assert_eq!(any_nat(&LeanList::get(&list, 0).expect("0")), 1);
        assert_eq!(any_nat(&LeanList::get(&list, 1).expect("1")), 2);
        assert_eq!(any_nat(&LeanList::get(&list, 2).expect("2")), 3);

        // Empty array -> empty list
        let empty = LeanArray::empty(lean)?;
        let list = LeanArray::toList(empty)?;
        assert!(LeanList::isEmpty(&list));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_reverse() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3])?;
        let rev = LeanArray::reverse(arr)?;

        assert_eq!(LeanArray::size(&rev), 3);
        assert_eq!(any_nat(&LeanArray::get(&rev, 0).expect("0")), 3);
        assert_eq!(any_nat(&LeanArray::get(&rev, 1).expect("1")), 2);
        assert_eq!(any_nat(&LeanArray::get(&rev, 2).expect("2")), 1);

        // Singleton and empty are identity
        let single = build_nat_array(lean, &[9])?;
        let rev_single = LeanArray::reverse(single)?;
        assert_eq!(LeanArray::size(&rev_single), 1);
        assert_eq!(any_nat(&LeanArray::get(&rev_single, 0).expect("0")), 9);

        let empty = LeanArray::empty(lean)?;
        let rev_empty = LeanArray::reverse(empty)?;
        assert_eq!(LeanArray::size(&rev_empty), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_take() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3, 4, 5])?;

        let taken2 = LeanArray::take(&arr, 2)?;
        assert_eq!(LeanArray::size(&taken2), 2);
        assert_eq!(any_nat(&LeanArray::get(&taken2, 0).expect("0")), 1);
        assert_eq!(any_nat(&LeanArray::get(&taken2, 1).expect("1")), 2);

        // take 0 -> empty
        let taken0 = LeanArray::take(&arr, 0)?;
        assert_eq!(LeanArray::size(&taken0), 0);

        // take more than size -> whole array
        let taken9 = LeanArray::take(&arr, 9)?;
        assert_eq!(LeanArray::size(&taken9), 5);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_drop() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3, 4, 5])?;

        let dropped2 = LeanArray::drop(&arr, 2)?;
        assert_eq!(LeanArray::size(&dropped2), 3);
        assert_eq!(any_nat(&LeanArray::get(&dropped2, 0).expect("0")), 3);
        assert_eq!(any_nat(&LeanArray::get(&dropped2, 2).expect("2")), 5);

        // drop 0 -> whole array
        let dropped0 = LeanArray::drop(&arr, 0)?;
        assert_eq!(LeanArray::size(&dropped0), 5);

        // drop more than size -> empty
        let dropped9 = LeanArray::drop(&arr, 9)?;
        assert_eq!(LeanArray::size(&dropped9), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_append() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let xs = build_nat_array(lean, &[1, 2])?;
        let ys = build_nat_array(lean, &[3, 4])?;
        let zs = LeanArray::append(xs, &ys)?;

        assert_eq!(LeanArray::size(&zs), 4);
        assert_eq!(any_nat(&LeanArray::get(&zs, 0).expect("0")), 1);
        assert_eq!(any_nat(&LeanArray::get(&zs, 1).expect("1")), 2);
        assert_eq!(any_nat(&LeanArray::get(&zs, 2).expect("2")), 3);
        assert_eq!(any_nat(&LeanArray::get(&zs, 3).expect("3")), 4);

        // Appending an empty array returns xs unchanged
        let empty = LeanArray::empty(lean)?;
        let xs = build_nat_array(lean, &[5, 6])?;
        let kept = LeanArray::append(xs, &empty)?;
        assert_eq!(LeanArray::size(&kept), 2);
        assert_eq!(any_nat(&LeanArray::get(&kept, 0).expect("0")), 5);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_contains() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3, 2])?;

        let target = LeanNat::from_usize(lean, 2)?;
        let target_any: LeanBound<'_, LeanAny> = target.cast();
        assert!(LeanArray::contains(&arr, &target_any, nat_eq));

        let missing = LeanNat::from_usize(lean, 99)?;
        let missing_any: LeanBound<'_, LeanAny> = missing.cast();
        assert!(!LeanArray::contains(&arr, &missing_any, nat_eq));

        // Zero is a real value here
        let zero_arr = build_nat_array(lean, &[0, 1])?;
        let zero = LeanNat::from_usize(lean, 0)?;
        let zero_any: LeanBound<'_, LeanAny> = zero.cast();
        assert!(LeanArray::contains(&zero_arr, &zero_any, nat_eq));

        // Empty array contains nothing
        let empty = LeanArray::empty(lean)?;
        let one = LeanNat::from_usize(lean, 1)?;
        let one_any: LeanBound<'_, LeanAny> = one.cast();
        assert!(!LeanArray::contains(&empty, &one_any, nat_eq));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_all() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let evens = build_nat_array(lean, &[2, 4, 6])?;
        assert!(LeanArray::all(&evens, |e| any_nat(e).is_multiple_of(2)));
        assert!(!LeanArray::all(&evens, |e| any_nat(e) > 3));

        // Vacuous truth on empty array
        let empty = LeanArray::empty(lean)?;
        assert!(LeanArray::all(&empty, |_| false));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_any() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 3, 4])?;
        assert!(LeanArray::any(&arr, |e| any_nat(e).is_multiple_of(2)));
        assert!(!LeanArray::any(&arr, |e| any_nat(e) > 100));

        // Empty array: any is false
        let empty = LeanArray::empty(lean)?;
        assert!(!LeanArray::any(&empty, |_| true));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_find() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 4, 3, 8])?;

        let found = LeanArray::find(&arr, |e| any_nat(e).is_multiple_of(2));
        assert!(found.is_some());
        assert_eq!(any_nat(&found.unwrap()), 4);

        assert!(LeanArray::find(&arr, |e| any_nat(e) > 100).is_none());

        // Empty array
        let empty = LeanArray::empty(lean)?;
        assert!(LeanArray::find(&empty, |_| true).is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_find_idx() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 4, 3, 8])?;

        assert_eq!(
            LeanArray::findIdx(&arr, |e| any_nat(e).is_multiple_of(2)),
            Some(1)
        );
        assert_eq!(LeanArray::findIdx(&arr, |e| any_nat(e) == 8), Some(3));
        assert_eq!(LeanArray::findIdx(&arr, |e| any_nat(e) == 1), Some(0));
        assert_eq!(LeanArray::findIdx(&arr, |e| any_nat(e) > 100), None);

        // Empty array
        let empty = LeanArray::empty(lean)?;
        assert_eq!(LeanArray::findIdx(&empty, |_| true), None);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_filter() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3, 4, 5, 6])?;
        let filtered = LeanArray::filter(arr, |e| any_nat(e).is_multiple_of(2))?;

        assert_eq!(LeanArray::size(&filtered), 3);
        assert_eq!(any_nat(&LeanArray::get(&filtered, 0).expect("0")), 2);
        assert_eq!(any_nat(&LeanArray::get(&filtered, 1).expect("1")), 4);
        assert_eq!(any_nat(&LeanArray::get(&filtered, 2).expect("2")), 6);

        // Keep everything / drop everything
        let arr = build_nat_array(lean, &[1, 2])?;
        let kept = LeanArray::filter(arr, |_| true)?;
        assert_eq!(LeanArray::size(&kept), 2);

        let arr = build_nat_array(lean, &[1, 2])?;
        let dropped = LeanArray::filter(arr, |_| false)?;
        assert_eq!(LeanArray::size(&dropped), 0);

        // Empty array
        let empty = LeanArray::empty(lean)?;
        let filtered_empty = LeanArray::filter(empty, |_| true)?;
        assert_eq!(LeanArray::size(&filtered_empty), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_map() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3])?;
        let mapped = LeanArray::map(arr, |elem| {
            let v = any_nat(&elem);
            Ok(LeanNat::from_usize(lean, v * 2)?.cast())
        })?;

        assert_eq!(LeanArray::size(&mapped), 3);
        assert_eq!(any_nat(&LeanArray::get(&mapped, 0).expect("0")), 2);
        assert_eq!(any_nat(&LeanArray::get(&mapped, 1).expect("1")), 4);
        assert_eq!(any_nat(&LeanArray::get(&mapped, 2).expect("2")), 6);

        // Map over empty array
        let empty = LeanArray::empty(lean)?;
        let mapped_empty = LeanArray::map(empty, |elem| {
            let v = any_nat(&elem);
            Ok(LeanNat::from_usize(lean, v + 1)?.cast())
        })?;
        assert_eq!(LeanArray::size(&mapped_empty), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_foldl() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3, 4])?;
        let sum = LeanArray::foldl(&arr, 0usize, |acc, e| acc + any_nat(&e));
        assert_eq!(sum, 10);

        // Order-observable: digits become 123
        let arr = build_nat_array(lean, &[1, 2, 3])?;
        let num = LeanArray::foldl(&arr, 0usize, |acc, e| acc * 10 + any_nat(&e));
        assert_eq!(num, 123);

        // Empty array returns init
        let empty = LeanArray::empty(lean)?;
        assert_eq!(LeanArray::foldl(&empty, 42usize, |acc, _| acc + 1), 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_foldr() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Order-observable: foldr [1,2,3] with acc*10+v processes right-to-left
        // (3 -> 32 -> 321), the reverse of foldl which gives 123.
        let arr = build_nat_array(lean, &[1, 2, 3])?;
        let num = LeanArray::foldr(&arr, 0usize, |e, acc| acc * 10 + any_nat(&e));
        assert_eq!(num, 321);

        // Sum works the same regardless of direction
        let arr = build_nat_array(lean, &[1, 2, 3, 4])?;
        let sum = LeanArray::foldr(&arr, 0usize, |e, acc| acc + any_nat(&e));
        assert_eq!(sum, 10);

        // Empty array returns init
        let empty = LeanArray::empty(lean)?;
        assert_eq!(LeanArray::foldr(&empty, 42usize, |_, acc| acc + 1), 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_count_pred() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3, 4, 5, 6])?;

        assert_eq!(LeanArray::countP(&arr, |e| any_nat(e).is_multiple_of(2)), 3);
        assert_eq!(LeanArray::countP(&arr, |_| true), 6);
        assert_eq!(LeanArray::countP(&arr, |e| any_nat(e) > 100), 0);

        // Empty array counts zero
        let empty = LeanArray::empty(lean)?;
        assert_eq!(LeanArray::countP(&empty, |_| true), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_flatten() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Outer array of two inner arrays: [[1, 2], [3, 4]]
        let inner1 = build_nat_array(lean, &[1, 2])?;
        let inner2 = build_nat_array(lean, &[3, 4])?;

        let mut outer = LeanArray::empty(lean)?;
        outer = LeanArray::push(outer, inner1.cast())?;
        outer = LeanArray::push(outer, inner2.cast())?;

        // SAFETY: all elements are arrays
        let flat = unsafe { LeanArray::flatten(outer)? };
        assert_eq!(LeanArray::size(&flat), 4);
        assert_eq!(any_nat(&LeanArray::get(&flat, 0).expect("0")), 1);
        assert_eq!(any_nat(&LeanArray::get(&flat, 1).expect("1")), 2);
        assert_eq!(any_nat(&LeanArray::get(&flat, 2).expect("2")), 3);
        assert_eq!(any_nat(&LeanArray::get(&flat, 3).expect("3")), 4);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_flatten_empty() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Flattening an empty outer array
        let outer = LeanArray::empty(lean)?;
        // SAFETY: vacuously all elements are arrays
        let flat = unsafe { LeanArray::flatten(outer)? };
        assert_eq!(LeanArray::size(&flat), 0);

        // Flattening an outer array of empty arrays
        let inner = LeanArray::empty(lean)?;
        let mut outer = LeanArray::empty(lean)?;
        outer = LeanArray::push(outer, inner.cast())?;
        // SAFETY: all elements are arrays
        let flat = unsafe { LeanArray::flatten(outer)? };
        assert_eq!(LeanArray::size(&flat), 0);

        // Mixed empty and non-empty inner arrays
        let inner1 = build_nat_array(lean, &[5])?;
        let inner2 = LeanArray::empty(lean)?;
        let mut outer = LeanArray::empty(lean)?;
        outer = LeanArray::push(outer, inner1.cast())?;
        outer = LeanArray::push(outer, inner2.cast())?;
        // SAFETY: all elements are arrays
        let flat = unsafe { LeanArray::flatten(outer)? };
        assert_eq!(LeanArray::size(&flat), 1);
        assert_eq!(any_nat(&LeanArray::get(&flat, 0).expect("0")), 5);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_zip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let xs = build_nat_array(lean, &[1, 2, 3])?;
        let ys = build_nat_array(lean, &[10, 20, 30])?;
        let zipped = LeanArray::zip(&xs, &ys)?;

        // Result length is the minimum of the two lengths
        assert_eq!(LeanArray::size(&zipped), 3);

        // Each element is a pair; verify both fields
        for (i, (x, y)) in [(1, 10), (2, 20), (3, 30)].iter().enumerate() {
            let pair = LeanArray::get(&zipped, i).expect("pair");
            let fst = pair_field(lean, &pair, 0);
            let snd = pair_field(lean, &pair, 1);
            assert_eq!(any_nat(&fst), *x);
            assert_eq!(any_nat(&snd), *y);
        }

        Ok(())
    });

    assert!(result.is_ok());
}

/// Extract object field `idx` from a Lean constructor (e.g. a Prod pair),
/// taking an owned reference to the field value.
fn pair_field<'l>(
    lean: Lean<'l>,
    pair: &LeanBound<'l, LeanAny>,
    idx: u32,
) -> LeanBound<'l, LeanAny> {
    unsafe {
        let field = leo3::ffi::lean_ctor_get(pair.as_ptr(), idx) as *mut leo3::ffi::lean_object;
        leo3::ffi::lean_inc(field);
        LeanBound::from_owned_ptr(lean, field)
    }
}

#[test]
fn test_array_zip_mismatched_lengths() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // xs longer than ys
        let xs = build_nat_array(lean, &[1, 2, 3])?;
        let ys = build_nat_array(lean, &[10, 20])?;
        let zipped = LeanArray::zip(&xs, &ys)?;
        assert_eq!(LeanArray::size(&zipped), 2);

        let pair = LeanArray::get(&zipped, 0).expect("pair");
        assert_eq!(any_nat(&pair_field(lean, &pair, 0)), 1);
        assert_eq!(any_nat(&pair_field(lean, &pair, 1)), 10);

        // ys longer than xs
        let xs = build_nat_array(lean, &[1])?;
        let ys = build_nat_array(lean, &[10, 20])?;
        let zipped = LeanArray::zip(&xs, &ys)?;
        assert_eq!(LeanArray::size(&zipped), 1);

        // Both empty
        let empty = LeanArray::empty(lean)?;
        let zipped = LeanArray::zip(&empty, &empty)?;
        assert_eq!(LeanArray::size(&zipped), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_is_prefix_of() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let xs = build_nat_array(lean, &[1, 2])?;
        let ys = build_nat_array(lean, &[1, 2, 3])?;

        assert!(LeanArray::isPrefixOf(&xs, &ys, nat_eq));
        assert!(!LeanArray::isPrefixOf(&ys, &xs, nat_eq)); // longer xs -> false

        // Mismatch at a position
        let bad = build_nat_array(lean, &[1, 9])?;
        assert!(!LeanArray::isPrefixOf(&bad, &ys, nat_eq));

        // Empty prefix is a prefix of everything
        let empty = LeanArray::empty(lean)?;
        assert!(LeanArray::isPrefixOf(&empty, &ys, nat_eq));
        assert!(LeanArray::isPrefixOf(&empty, &empty, nat_eq));

        // Equal arrays are prefixes of each other
        let same = build_nat_array(lean, &[1, 2, 3])?;
        assert!(LeanArray::isPrefixOf(&ys, &same, nat_eq));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_debug_format() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = build_nat_array(lean, &[1, 2, 3])?;
        let dbg = format!("{:?}", arr);
        assert!(dbg.contains("LeanArray"));
        assert!(dbg.contains("size: 3"));

        let empty = LeanArray::empty(lean)?;
        let dbg_empty = format!("{:?}", empty);
        assert!(dbg_empty.contains("size: 0"));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_boundary_values() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 0, 1, 2^32 (still small), and usize::MAX (big nat)
        let arr = build_nat_array(lean, &[0, 1, 1 << 32, usize::MAX])?;
        assert_eq!(LeanArray::size(&arr), 4);

        assert_eq!(any_nat(&LeanArray::get(&arr, 0).expect("0")), 0);
        assert_eq!(any_nat(&LeanArray::get(&arr, 1).expect("1")), 1);
        assert_eq!(any_nat(&LeanArray::get(&arr, 2).expect("2")), 1usize << 32);

        // usize::MAX round-trips as a big nat: to_usize reports overflow
        let big: LeanBound<'_, LeanNat> = LeanArray::get(&arr, 3).expect("3").cast();
        assert!(!LeanNat::is_small(&big));
        assert!(LeanNat::to_usize(&big).is_err());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_pipeline() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // range 0..6 -> filter evens -> map double -> reverse -> take 2
        let arr = LeanArray::range(lean, 6)?;
        let filtered = LeanArray::filter(arr, |e| any_nat(e).is_multiple_of(2))?; // [0, 2, 4]
        let mapped = LeanArray::map(filtered, |elem| {
            let v = any_nat(&elem);
            Ok(LeanNat::from_usize(lean, v * 2)?.cast())
        })?; // [0, 4, 8]
        let reversed = LeanArray::reverse(mapped)?; // [8, 4, 0]
        let taken = LeanArray::take(&reversed, 2)?; // [8, 4]

        assert_eq!(LeanArray::size(&taken), 2);
        assert_eq!(any_nat(&LeanArray::get(&taken, 0).expect("0")), 8);
        assert_eq!(any_nat(&LeanArray::get(&taken, 1).expect("1")), 4);

        // Convert to list, fold, and verify round-trip sizes
        let list = LeanArray::toList(taken)?;
        assert_eq!(LeanList::length(&list), 2);
        let sum = LeanList::foldl(&list, 0usize, |acc, e| acc + any_nat(&e));
        assert_eq!(sum, 12);

        Ok(())
    });

    assert!(result.is_ok());
}
