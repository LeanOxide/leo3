//! Range operation tests for Leo3
//!
//! These tests demonstrate LeanRange functionality including creation,
//! accessor round-trips, and size calculation for a variety of ranges.

#![cfg(feature = "runtime-tests")]

use leo3::prelude::*;
use leo3::types::LeanRange;

#[test]
fn test_range_new_step_one() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let start = LeanNat::from_usize(lean, 2)?;
        let stop = LeanNat::from_usize(lean, 10)?;
        let step = LeanNat::from_usize(lean, 1)?;
        let range = LeanRange::new(start, stop, step)?;

        assert_eq!(LeanNat::to_usize(&LeanRange::start(&range))?, 2);
        assert_eq!(LeanNat::to_usize(&LeanRange::stop(&range))?, 10);
        assert_eq!(LeanNat::to_usize(&LeanRange::step(&range))?, 1);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_range_new_step_greater_than_one() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let start = LeanNat::from_usize(lean, 0)?;
        let stop = LeanNat::from_usize(lean, 10)?;
        let step = LeanNat::from_usize(lean, 2)?;
        let range = LeanRange::new(start, stop, step)?;

        assert_eq!(LeanNat::to_usize(&LeanRange::start(&range))?, 0);
        assert_eq!(LeanNat::to_usize(&LeanRange::stop(&range))?, 10);
        assert_eq!(LeanNat::to_usize(&LeanRange::step(&range))?, 2);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_range_new_nonzero_start() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let start = LeanNat::from_usize(lean, 5)?;
        let stop = LeanNat::from_usize(lean, 20)?;
        let step = LeanNat::from_usize(lean, 3)?;
        let range = LeanRange::new(start, stop, step)?;

        assert_eq!(LeanNat::to_usize(&LeanRange::start(&range))?, 5);
        assert_eq!(LeanNat::to_usize(&LeanRange::stop(&range))?, 20);
        assert_eq!(LeanNat::to_usize(&LeanRange::step(&range))?, 3);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_range_mk() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let stop = LeanNat::from_usize(lean, 10)?;
        let range = LeanRange::mk(stop)?;

        // [:10] starts at 0 with step 1
        assert_eq!(LeanNat::to_usize(&LeanRange::start(&range))?, 0);
        assert_eq!(LeanNat::to_usize(&LeanRange::stop(&range))?, 10);
        assert_eq!(LeanNat::to_usize(&LeanRange::step(&range))?, 1);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_range_mk_zero_stop() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let stop = LeanNat::from_usize(lean, 0)?;
        let range = LeanRange::mk(stop)?;

        assert_eq!(LeanNat::to_usize(&LeanRange::start(&range))?, 0);
        assert_eq!(LeanNat::to_usize(&LeanRange::stop(&range))?, 0);
        assert_eq!(LeanNat::to_usize(&LeanRange::step(&range))?, 1);
        assert_eq!(LeanNat::to_usize(&LeanRange::size(&range)?)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_range_size_step_two() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // [0:10:2] = 0, 2, 4, 6, 8 -> size 5
        let start = LeanNat::from_usize(lean, 0)?;
        let stop = LeanNat::from_usize(lean, 10)?;
        let step = LeanNat::from_usize(lean, 2)?;
        let range = LeanRange::new(start, stop, step)?;

        assert_eq!(LeanNat::to_usize(&LeanRange::size(&range)?)?, 5);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_range_size_mk() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // [:10] = 0..9 -> size 10
        let stop = LeanNat::from_usize(lean, 10)?;
        let range = LeanRange::mk(stop)?;

        assert_eq!(LeanNat::to_usize(&LeanRange::size(&range)?)?, 10);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_range_size_empty() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // [0:0] is empty -> size 0
        let start = LeanNat::from_usize(lean, 0)?;
        let stop = LeanNat::from_usize(lean, 0)?;
        let step = LeanNat::from_usize(lean, 1)?;
        let range = LeanRange::new(start, stop, step)?;

        assert_eq!(LeanNat::to_usize(&LeanRange::size(&range)?)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_range_size_step_three() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // [0:10:3] = 0, 3, 6, 9 -> size 4 (non-aligned stop rounds up)
        let start = LeanNat::from_usize(lean, 0)?;
        let stop = LeanNat::from_usize(lean, 10)?;
        let step = LeanNat::from_usize(lean, 3)?;
        let range = LeanRange::new(start, stop, step)?;

        assert_eq!(LeanNat::to_usize(&LeanRange::size(&range)?)?, 4);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_range_size_nonzero_start() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // [2:10:2] = 2, 4, 6, 8 -> size 4
        let start = LeanNat::from_usize(lean, 2)?;
        let stop = LeanNat::from_usize(lean, 10)?;
        let step = LeanNat::from_usize(lean, 2)?;
        let range = LeanRange::new(start, stop, step)?;

        assert_eq!(LeanNat::to_usize(&LeanRange::size(&range)?)?, 4);

        // [5:10] = 5..9 -> size 5
        let start = LeanNat::from_usize(lean, 5)?;
        let stop = LeanNat::from_usize(lean, 10)?;
        let step = LeanNat::from_usize(lean, 1)?;
        let range = LeanRange::new(start, stop, step)?;

        assert_eq!(LeanNat::to_usize(&LeanRange::size(&range)?)?, 5);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_range_size_start_equals_stop() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // [5:5] is empty -> size 0
        let start = LeanNat::from_usize(lean, 5)?;
        let stop = LeanNat::from_usize(lean, 5)?;
        let step = LeanNat::from_usize(lean, 1)?;
        let range = LeanRange::new(start, stop, step)?;

        assert_eq!(LeanNat::to_usize(&LeanRange::size(&range)?)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_range_size_start_greater_than_stop() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // [10:5] has start > stop; Nat subtraction saturates, size is 0
        let start = LeanNat::from_usize(lean, 10)?;
        let stop = LeanNat::from_usize(lean, 5)?;
        let step = LeanNat::from_usize(lean, 1)?;
        let range = LeanRange::new(start, stop, step)?;

        assert_eq!(LeanNat::to_usize(&LeanRange::size(&range)?)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_range_size_single_element() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // [0:1] = {0} -> size 1
        let start = LeanNat::from_usize(lean, 0)?;
        let stop = LeanNat::from_usize(lean, 1)?;
        let step = LeanNat::from_usize(lean, 1)?;
        let range = LeanRange::new(start, stop, step)?;

        assert_eq!(LeanNat::to_usize(&LeanRange::size(&range)?)?, 1);

        Ok(())
    });

    assert!(result.is_ok());
}
