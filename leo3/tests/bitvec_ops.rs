//! BitVec operation tests for Leo3
//!
//! These tests demonstrate LeanBitVec functionality including creation from
//! Nat (mod 2^width), extraction, zero/allOnes constructors, bitwise logic,
//! modular arithmetic, and shifts.

#![cfg(feature = "runtime-tests")]

use leo3::prelude::*;
use leo3::types::LeanBitVec;

#[test]
fn test_bitvec_ofnat_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // ofNat takes the value mod 2^width.
        // width=4: 19 % 16 = 3
        let width = LeanNat::from_usize(lean, 4)?;
        let value = LeanNat::from_usize(lean, 19)?;
        let bv = LeanBitVec::ofNat(lean, width, value)?;
        let nat = LeanBitVec::toNat(lean, &bv);
        assert_eq!(LeanNat::to_usize(&nat)?, 3);

        // value exactly at the boundary wraps to 0
        let width = LeanNat::from_usize(lean, 4)?;
        let value = LeanNat::from_usize(lean, 16)?;
        let bv = LeanBitVec::ofNat(lean, width, value)?;
        let nat = LeanBitVec::toNat(lean, &bv);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        // value well beyond the width
        let width = LeanNat::from_usize(lean, 4)?;
        let value = LeanNat::from_usize(lean, 100)?;
        let bv = LeanBitVec::ofNat(lean, width, value)?;
        let nat = LeanBitVec::toNat(lean, &bv);
        assert_eq!(LeanNat::to_usize(&nat)?, 4); // 100 % 16

        // larger width: 300 % 256 = 44
        let width = LeanNat::from_usize(lean, 8)?;
        let value = LeanNat::from_usize(lean, 300)?;
        let bv = LeanBitVec::ofNat(lean, width, value)?;
        let nat = LeanBitVec::toNat(lean, &bv);
        assert_eq!(LeanNat::to_usize(&nat)?, 44);

        // width=1: only 0 and 1 are representable
        let width = LeanNat::from_usize(lean, 1)?;
        let value = LeanNat::from_usize(lean, 5)?;
        let bv = LeanBitVec::ofNat(lean, width, value)?;
        let nat = LeanBitVec::toNat(lean, &bv);
        assert_eq!(LeanNat::to_usize(&nat)?, 1); // 5 % 2

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_ofnat_width_zero() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // width=0 => 2^0 = 1, everything collapses to 0
        let width = LeanNat::from_usize(lean, 0)?;
        let value = LeanNat::from_usize(lean, 0)?;
        let bv = LeanBitVec::ofNat(lean, width, value)?;
        let nat = LeanBitVec::toNat(lean, &bv);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        let width = LeanNat::from_usize(lean, 0)?;
        let value = LeanNat::from_usize(lean, 12345)?;
        let bv = LeanBitVec::ofNat(lean, width, value)?;
        let nat = LeanBitVec::toNat(lean, &bv);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_to_nat_respects_width_bound() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Every representable value is < 2^width
        for width in 0..=12usize {
            for value in [0usize, 1, 2, 13, 129, 1000, 65535] {
                let w = LeanNat::from_usize(lean, width)?;
                let v = LeanNat::from_usize(lean, value)?;
                let bv = LeanBitVec::ofNat(lean, w, v)?;
                let nat = LeanBitVec::toNat(lean, &bv);
                let extracted = LeanNat::to_usize(&nat)?;
                let bound = 1usize << width.min(63);
                assert!(
                    extracted < bound,
                    "value {extracted} out of range for width {width}"
                );
            }
        }

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_zero() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let width = LeanNat::from_usize(lean, 8)?;
        let bv = LeanBitVec::zero(lean, width)?;
        let nat = LeanBitVec::toNat(lean, &bv);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        let width = LeanNat::from_usize(lean, 1)?;
        let bv = LeanBitVec::zero(lean, width)?;
        let nat = LeanBitVec::toNat(lean, &bv);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_all_ones() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let width = LeanNat::from_usize(lean, 4)?;
        let bv = LeanBitVec::allOnes(lean, width)?;
        let nat = LeanBitVec::toNat(lean, &bv);
        assert_eq!(LeanNat::to_usize(&nat)?, 15); // 0b1111

        let width = LeanNat::from_usize(lean, 1)?;
        let bv = LeanBitVec::allOnes(lean, width)?;
        let nat = LeanBitVec::toNat(lean, &bv);
        assert_eq!(LeanNat::to_usize(&nat)?, 1);

        let width = LeanNat::from_usize(lean, 8)?;
        let bv = LeanBitVec::allOnes(lean, width)?;
        let nat = LeanBitVec::toNat(lean, &bv);
        assert_eq!(LeanNat::to_usize(&nat)?, 255);

        // width 0: 2^0 - 1 = 0
        let width = LeanNat::from_usize(lean, 0)?;
        let bv = LeanBitVec::allOnes(lean, width)?;
        let nat = LeanBitVec::toNat(lean, &bv);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_and() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 0b1010 & 0b1100 = 0b1000
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 0b1010)?,
        )?;
        let b = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 0b1100)?,
        )?;
        let r = LeanBitVec::and(lean, &a, &b, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0b1000);

        // AND with zero clears everything
        let width = LeanNat::from_usize(lean, 8)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 8)?,
            LeanNat::from_usize(lean, 0xFF)?,
        )?;
        let b = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 8)?,
            LeanNat::from_usize(lean, 0)?,
        )?;
        let r = LeanBitVec::and(lean, &a, &b, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_or() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 0b1010 | 0b1100 = 0b1110
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 0b1010)?,
        )?;
        let b = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 0b1100)?,
        )?;
        let r = LeanBitVec::or(lean, &a, &b, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0b1110);

        // OR with allOnes saturates to allOnes (result masked to width)
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 0)?,
        )?;
        let ones = LeanBitVec::allOnes(lean, LeanNat::from_usize(lean, 4)?)?;
        let r = LeanBitVec::or(lean, &a, &ones, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 15);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_xor() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 0b1010 ^ 0b1100 = 0b0110
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 0b1010)?,
        )?;
        let b = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 0b1100)?,
        )?;
        let r = LeanBitVec::xor(lean, &a, &b, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0b0110);

        // x ^ x = 0
        let width = LeanNat::from_usize(lean, 8)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 8)?,
            LeanNat::from_usize(lean, 0xA5)?,
        )?;
        let r = LeanBitVec::xor(lean, &a, &a, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_not() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // not(0b1010, width 4) = 0b0101
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 0b1010)?,
        )?;
        let r = LeanBitVec::not(lean, &a, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0b0101);

        // not(0) = allOnes
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 0)?,
        )?;
        let r = LeanBitVec::not(lean, &a, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 15);

        // not(allOnes) = 0 (involution)
        let width = LeanNat::from_usize(lean, 8)?;
        let a = LeanBitVec::allOnes(lean, LeanNat::from_usize(lean, 8)?)?;
        let r = LeanBitVec::not(lean, &a, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_add_wrap() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 10 + 12 = 22, 22 % 16 = 6 (wrap-around)
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 10)?,
        )?;
        let b = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 12)?,
        )?;
        let r = LeanBitVec::add(lean, &a, &b, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 6);

        // max + 1 wraps to 0
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 15)?,
        )?;
        let b = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 1)?,
        )?;
        let r = LeanBitVec::add(lean, &a, &b, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        // 0 + 0 = 0
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::zero(lean, LeanNat::from_usize(lean, 4)?)?;
        let b = LeanBitVec::zero(lean, LeanNat::from_usize(lean, 4)?)?;
        let r = LeanBitVec::add(lean, &a, &b, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_sub_underflow() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 3 - 5 = -2 mod 16 = 14 (underflow wrap)
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 3)?,
        )?;
        let b = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let r = LeanBitVec::sub(lean, &a, &b, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 14);

        // 0 - 1 = 15
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::zero(lean, LeanNat::from_usize(lean, 4)?)?;
        let b = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 1)?,
        )?;
        let r = LeanBitVec::sub(lean, &a, &b, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 15);

        // 10 - 3 = 7 (no wrap)
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 10)?,
        )?;
        let b = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 3)?,
        )?;
        let r = LeanBitVec::sub(lean, &a, &b, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 7);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_mul_wrap() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 6 * 6 = 36, 36 % 16 = 4
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 6)?,
        )?;
        let b = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 6)?,
        )?;
        let r = LeanBitVec::mul(lean, &a, &b, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 4);

        // 15 * 15 = 225, 225 % 16 = 1
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 15)?,
        )?;
        let b = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 15)?,
        )?;
        let r = LeanBitVec::mul(lean, &a, &b, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 1);

        // x * 0 = 0
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let b = LeanBitVec::zero(lean, LeanNat::from_usize(lean, 4)?)?;
        let r = LeanBitVec::mul(lean, &a, &b, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_shift_left() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 0b0011 << 2 = 0b1100 (3 << 2 = 12)
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 3)?,
        )?;
        let n = LeanNat::from_usize(lean, 2)?;
        let r = LeanBitVec::shiftLeft(lean, &a, &n, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 12);

        // shift by 0 is the identity
        let width = LeanNat::from_usize(lean, 8)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 8)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let n = LeanNat::from_usize(lean, 0)?;
        let r = LeanBitVec::shiftLeft(lean, &a, &n, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 5);

        // shift beyond the width wraps to 0 (1 << 4 = 16, 16 % 16 = 0)
        let width = LeanNat::from_usize(lean, 4)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 1)?,
        )?;
        let n = LeanNat::from_usize(lean, 4)?;
        let r = LeanBitVec::shiftLeft(lean, &a, &n, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        // full-width shift in 8 bits: 1 << 7 = 128
        let width = LeanNat::from_usize(lean, 8)?;
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 8)?,
            LeanNat::from_usize(lean, 1)?,
        )?;
        let n = LeanNat::from_usize(lean, 7)?;
        let r = LeanBitVec::shiftLeft(lean, &a, &n, width)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 128);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_shift_right() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 0b1100 >> 2 = 0b0011 (12 >> 2 = 3)
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 12)?,
        )?;
        let n = LeanNat::from_usize(lean, 2)?;
        let r = LeanBitVec::shiftRight(lean, &a, &n)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 3);

        // shift by 0 is the identity
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 8)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let n = LeanNat::from_usize(lean, 0)?;
        let r = LeanBitVec::shiftRight(lean, &a, &n)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 5);

        // shift past all set bits yields 0
        let a = LeanBitVec::ofNat(
            lean,
            LeanNat::from_usize(lean, 8)?,
            LeanNat::from_usize(lean, 1)?,
        )?;
        let n = LeanNat::from_usize(lean, 1)?;
        let r = LeanBitVec::shiftRight(lean, &a, &n)?;
        let nat = LeanBitVec::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_bitvec_to_nat_overflow_error() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 2^64 does not fit in a usize; to_usize must report an error
        let two = LeanNat::from_usize(lean, 2)?;
        let exp = LeanNat::from_usize(lean, 64)?;
        let big = LeanNat::pow(two, exp)?;
        assert!(LeanNat::to_usize(&big).is_err());

        // allOnes of width 64 is 2^64 - 1, also beyond usize
        let width = LeanNat::from_usize(lean, 64)?;
        let ones = LeanBitVec::allOnes(lean, width)?;
        let nat = LeanBitVec::toNat(lean, &ones);
        assert!(LeanNat::to_usize(&nat).is_err());

        // but the BitVec itself is still valid: 2^63 - 1 fits
        let width = LeanNat::from_usize(lean, 63)?;
        let ones = LeanBitVec::allOnes(lean, width)?;
        let nat = LeanBitVec::toNat(lean, &ones);
        assert_eq!(LeanNat::to_usize(&nat)?, (1usize << 63) - 1);

        Ok(())
    });

    assert!(result.is_ok());
}
