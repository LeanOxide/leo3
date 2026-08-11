//! Sum, Sigma, and Subtype operation tests for Leo3
//!
//! These tests demonstrate LeanSum (disjoint union), LeanSigma (dependent
//! pair), and LeanSubtype (value + erased proof) creation, inspection, and
//! value round-trips.

#![cfg(feature = "runtime-tests")]

use leo3::prelude::*;
use leo3::types::{LeanSigma, LeanSubtype, LeanSum};

// ---------------------------------------------------------------------------
// LeanSum
// ---------------------------------------------------------------------------

#[test]
fn test_sum_inl() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let value = LeanNat::from_usize(lean, 42)?;
        let sum = LeanSum::inl(value.cast())?;

        assert!(LeanSum::isLeft(&sum));
        assert!(!LeanSum::isRight(&sum));

        // getLeft returns Some with the round-tripped value
        let left = LeanSum::getLeft(&sum).expect("inl sum should have a left value");
        let nat = left.cast::<LeanNat>();
        assert_eq!(LeanNat::to_usize(&nat)?, 42);

        // getRight returns None for an inl sum
        assert!(LeanSum::getRight(&sum).is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_sum_inr() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let value = LeanString::mk(lean, "hello")?;
        let sum = LeanSum::inr(value.cast())?;

        assert!(!LeanSum::isLeft(&sum));
        assert!(LeanSum::isRight(&sum));

        // getRight returns Some with the round-tripped value
        let right = LeanSum::getRight(&sum).expect("inr sum should have a right value");
        let string = right.cast::<LeanString>();
        assert_eq!(LeanString::cstr(&string)?, "hello");

        // getLeft returns None for an inr sum
        assert!(LeanSum::getLeft(&sum).is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_sum_swap_inl_to_inr() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let value = LeanNat::from_usize(lean, 42)?;
        let sum = LeanSum::inl(value.cast())?;

        let swapped = LeanSum::swap(sum)?;
        assert!(LeanSum::isRight(&swapped));
        assert!(!LeanSum::isLeft(&swapped));

        // Value is preserved across the swap
        let right = LeanSum::getRight(&swapped).expect("swapped inl should be inr");
        let nat = right.cast::<LeanNat>();
        assert_eq!(LeanNat::to_usize(&nat)?, 42);

        // Swap back: inr -> inl
        let swapped_back = LeanSum::swap(swapped)?;
        assert!(LeanSum::isLeft(&swapped_back));
        let left = LeanSum::getLeft(&swapped_back).expect("double-swapped should be inl");
        let nat = left.cast::<LeanNat>();
        assert_eq!(LeanNat::to_usize(&nat)?, 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_sum_swap_inr_to_inl() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let value = LeanString::mk(lean, "world")?;
        let sum = LeanSum::inr(value.cast())?;

        let swapped = LeanSum::swap(sum)?;
        assert!(LeanSum::isLeft(&swapped));
        assert!(!LeanSum::isRight(&swapped));

        // Value is preserved across the swap
        let left = LeanSum::getLeft(&swapped).expect("swapped inr should be inl");
        let string = left.cast::<LeanString>();
        assert_eq!(LeanString::cstr(&string)?, "world");

        // Swap back: inl -> inr
        let swapped_back = LeanSum::swap(swapped)?;
        assert!(LeanSum::isRight(&swapped_back));
        let right = LeanSum::getRight(&swapped_back).expect("double-swapped should be inr");
        let string = right.cast::<LeanString>();
        assert_eq!(LeanString::cstr(&string)?, "world");

        Ok(())
    });

    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// LeanSigma
// ---------------------------------------------------------------------------

#[test]
fn test_sigma_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let first = LeanNat::from_usize(lean, 42)?;
        let second = LeanString::mk(lean, "hello")?;
        let sigma = LeanSigma::mk(first.cast(), second.cast())?;

        let fst_val = LeanSigma::fst(&sigma).cast::<LeanNat>();
        assert_eq!(LeanNat::to_usize(&fst_val)?, 42);

        let snd_val = LeanSigma::snd(&sigma).cast::<LeanString>();
        assert_eq!(LeanString::cstr(&snd_val)?, "hello");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_sigma_mixed_types() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let first = LeanString::mk(lean, "count")?;
        let second = LeanNat::from_usize(lean, 7)?;
        let sigma = LeanSigma::mk(first.cast(), second.cast())?;

        let fst_val = LeanSigma::fst(&sigma).cast::<LeanString>();
        assert_eq!(LeanString::cstr(&fst_val)?, "count");

        let snd_val = LeanSigma::snd(&sigma).cast::<LeanNat>();
        assert_eq!(LeanNat::to_usize(&snd_val)?, 7);

        Ok(())
    });

    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// LeanSubtype
// ---------------------------------------------------------------------------

#[test]
fn test_subtype_nat() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // In Lean: { n : Nat // n > 0 } with value 42
        let value = LeanNat::from_usize(lean, 42)?;
        let subtype = LeanSubtype::mk(value.cast())?;

        let extracted = LeanSubtype::val(&subtype).cast::<LeanNat>();
        assert_eq!(LeanNat::to_usize(&extracted)?, 42);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_subtype_string() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // In Lean: { s : String // s ≠ "" }
        let value = LeanString::mk(lean, "nonempty")?;
        let subtype = LeanSubtype::mk(value.cast())?;

        let extracted = LeanSubtype::val(&subtype).cast::<LeanString>();
        assert_eq!(LeanString::cstr(&extracted)?, "nonempty");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_subtype_zero_boundary() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Boundary value 0 round-trips through a subtype
        let value = LeanNat::from_usize(lean, 0)?;
        let subtype = LeanSubtype::mk(value.cast())?;

        let extracted = LeanSubtype::val(&subtype).cast::<LeanNat>();
        assert_eq!(LeanNat::to_usize(&extracted)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}
