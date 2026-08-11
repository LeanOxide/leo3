//! Fin operation tests for Leo3
//!
//! These tests demonstrate LeanFin functionality including creation from Nat
//! (mod bound), direct construction, extraction, modular arithmetic, division,
//! bitwise logic, and shifts.

#![cfg(feature = "runtime-tests")]

use leo3::prelude::*;
use leo3::types::LeanFin;

#[test]
fn test_fin_ofnat_mod() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // ofNat takes the value mod bound.
        // bound 5: 7 % 5 = 2
        let bound = LeanNat::from_usize(lean, 5)?;
        let value = LeanNat::from_usize(lean, 7)?;
        let fin = LeanFin::ofNat(lean, value, bound)?;
        let nat = LeanFin::toNat(lean, &fin);
        assert_eq!(LeanNat::to_usize(&nat)?, 2);

        // value equal to the bound wraps to 0
        let bound = LeanNat::from_usize(lean, 5)?;
        let value = LeanNat::from_usize(lean, 5)?;
        let fin = LeanFin::ofNat(lean, value, bound)?;
        let nat = LeanFin::toNat(lean, &fin);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        // value 0 stays 0
        let bound = LeanNat::from_usize(lean, 5)?;
        let value = LeanNat::from_usize(lean, 0)?;
        let fin = LeanFin::ofNat(lean, value, bound)?;
        let nat = LeanFin::toNat(lean, &fin);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        // value well beyond the bound
        let bound = LeanNat::from_usize(lean, 5)?;
        let value = LeanNat::from_usize(lean, 12)?;
        let fin = LeanFin::ofNat(lean, value, bound)?;
        let nat = LeanFin::toNat(lean, &fin);
        assert_eq!(LeanNat::to_usize(&nat)?, 2); // 12 % 5

        // bound 16: 100 % 16 = 4
        let bound = LeanNat::from_usize(lean, 16)?;
        let value = LeanNat::from_usize(lean, 100)?;
        let fin = LeanFin::ofNat(lean, value, bound)?;
        let nat = LeanFin::toNat(lean, &fin);
        assert_eq!(LeanNat::to_usize(&nat)?, 4);

        // bound 1: everything collapses to 0
        let bound = LeanNat::from_usize(lean, 1)?;
        let value = LeanNat::from_usize(lean, 9)?;
        let fin = LeanFin::ofNat(lean, value, bound)?;
        let nat = LeanFin::toNat(lean, &fin);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_fin_mk_and_to_nat_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // mk constructs a Fin directly from a Nat (no bounds check)
        let value = LeanNat::from_usize(lean, 3)?;
        let fin = LeanFin::mk(lean, value)?;
        let nat = LeanFin::toNat(lean, &fin);
        assert_eq!(LeanNat::to_usize(&nat)?, 3);

        let value = LeanNat::from_usize(lean, 0)?;
        let fin = LeanFin::mk(lean, value)?;
        let nat = LeanFin::toNat(lean, &fin);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        let value = LeanNat::from_usize(lean, 6)?;
        let fin = LeanFin::mk(lean, value)?;
        let nat = LeanFin::toNat(lean, &fin);
        assert_eq!(LeanNat::to_usize(&nat)?, 6);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_fin_to_nat_respects_bound() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Every ofNat result is < bound
        for bound in 1..=16usize {
            for value in [0usize, 1, 2, 5, 13, 40, 1000] {
                let b = LeanNat::from_usize(lean, bound)?;
                let v = LeanNat::from_usize(lean, value)?;
                let fin = LeanFin::ofNat(lean, v, b)?;
                let nat = LeanFin::toNat(lean, &fin);
                let extracted = LeanNat::to_usize(&nat)?;
                assert!(
                    extracted < bound,
                    "value {extracted} out of range for bound {bound}"
                );
            }
        }

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_fin_add_mod() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // bound 5: 3 + 3 = 6, 6 % 5 = 1
        let bound = LeanNat::from_usize(lean, 5)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 3)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 3)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let r = LeanFin::add(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 1);

        // bound 5: 4 + 1 = 5 wraps to 0
        let bound = LeanNat::from_usize(lean, 5)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 1)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let r = LeanFin::add(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        // bound 7: 6 + 1 = 7 wraps to 0
        let bound = LeanNat::from_usize(lean, 7)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 6)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 1)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let r = LeanFin::add(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        // bound 16: 0 + 4 = 4
        let bound = LeanNat::from_usize(lean, 16)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 0)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::add(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 4);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_fin_sub_mod() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // bound 5: 1 - 3 = -2 mod 5 = 3
        let bound = LeanNat::from_usize(lean, 5)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 1)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 3)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let r = LeanFin::sub(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 3);

        // bound 5: 0 - 1 = 4
        let bound = LeanNat::from_usize(lean, 5)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 0)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 1)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let r = LeanFin::sub(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 4);

        // bound 5: 4 - 2 = 2 (no wrap)
        let bound = LeanNat::from_usize(lean, 5)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 2)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let r = LeanFin::sub(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 2);

        // bound 7: 3 - 5 = -2 mod 7 = 5
        let bound = LeanNat::from_usize(lean, 7)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 3)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 5)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let r = LeanFin::sub(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 5);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_fin_mul_mod() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // bound 5: 3 * 4 = 12, 12 % 5 = 2
        let bound = LeanNat::from_usize(lean, 5)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 3)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let r = LeanFin::mul(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 2);

        // bound 7: 4 * 4 = 16, 16 % 7 = 2
        let bound = LeanNat::from_usize(lean, 7)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let r = LeanFin::mul(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 2);

        // bound 16: 6 * 6 = 36, 36 % 16 = 4
        let bound = LeanNat::from_usize(lean, 16)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 6)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 6)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::mul(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 4);

        // x * 0 = 0
        let bound = LeanNat::from_usize(lean, 16)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 9)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 0)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::mul(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_fin_div() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 10 / 3 = 3 (truncating division)
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 10)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 3)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::div(lean, &a, &b)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 3);

        // 7 / 2 = 3
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 7)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 2)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::div(lean, &a, &b)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 3);

        // 5 / 5 = 1
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 5)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 5)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::div(lean, &a, &b)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 1);

        // 0 / 5 = 0
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 0)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 5)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::div(lean, &a, &b)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_fin_modulo() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 10 % 3 = 1
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 10)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 3)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::modulo(lean, &a, &b)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 1);

        // 7 % 5 = 2
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 7)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 5)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::modulo(lean, &a, &b)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 2);

        // 5 % 5 = 0
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 5)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 5)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::modulo(lean, &a, &b)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        // 4 % 16 = 4 (remainder less than divisor)
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 16)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::modulo(lean, &a, &b)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 4);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_fin_land() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // bound 16: 12 & 10 = 8
        let bound = LeanNat::from_usize(lean, 16)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 12)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 10)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::land(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 8);

        // bound 7: 6 & 3 = 2
        let bound = LeanNat::from_usize(lean, 7)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 6)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 3)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let r = LeanFin::land(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 2);

        // x & 0 = 0
        let bound = LeanNat::from_usize(lean, 16)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 15)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 0)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::land(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_fin_lor() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // bound 16: 12 | 10 = 14
        let bound = LeanNat::from_usize(lean, 16)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 12)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 10)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::lor(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 14);

        // bound 7: 6 | 1 = 7, 7 % 7 = 0 (wraps to 0)
        let bound = LeanNat::from_usize(lean, 7)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 6)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 1)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let r = LeanFin::lor(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        // x | 0 = x
        let bound = LeanNat::from_usize(lean, 16)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 9)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 0)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::lor(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 9);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_fin_xor() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // bound 16: 12 ^ 10 = 6
        let bound = LeanNat::from_usize(lean, 16)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 12)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 10)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::xor(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 6);

        // bound 7: 4 ^ 3 = 7, 7 % 7 = 0 (wraps to 0)
        let bound = LeanNat::from_usize(lean, 7)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 3)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let r = LeanFin::xor(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        // x ^ x = 0
        let bound = LeanNat::from_usize(lean, 16)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 5)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::xor(lean, &a, &a, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_fin_shift_left() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // bound 16: 3 << 2 = 12
        let bound = LeanNat::from_usize(lean, 16)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 3)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 2)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::shiftLeft(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 12);

        // bound 5: 1 << 4 = 16, 16 % 5 = 1 (wraps mod bound)
        let bound = LeanNat::from_usize(lean, 5)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 1)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 4)?,
            LeanNat::from_usize(lean, 5)?,
        )?;
        let r = LeanFin::shiftLeft(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 1);

        // bound 7: 1 << 3 = 8, 8 % 7 = 1
        let bound = LeanNat::from_usize(lean, 7)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 1)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 3)?,
            LeanNat::from_usize(lean, 7)?,
        )?;
        let r = LeanFin::shiftLeft(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 1);

        // shift by 0 is the identity
        let bound = LeanNat::from_usize(lean, 16)?;
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 5)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 0)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::shiftLeft(lean, &a, &b, bound)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 5);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_fin_shift_right() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 12 >> 2 = 3
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 12)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 2)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::shiftRight(lean, &a, &b)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 3);

        // 1 >> 1 = 0
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 1)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 1)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::shiftRight(lean, &a, &b)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 0);

        // shift by 0 is the identity
        let a = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 5)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let b = LeanFin::ofNat(
            lean,
            LeanNat::from_usize(lean, 0)?,
            LeanNat::from_usize(lean, 16)?,
        )?;
        let r = LeanFin::shiftRight(lean, &a, &b)?;
        let nat = LeanFin::toNat(lean, &r);
        assert_eq!(LeanNat::to_usize(&nat)?, 5);

        Ok(())
    });

    assert!(result.is_ok());
}
