//! Comprehensive fixed-width integer wrapper tests for Leo3
//!
//! Exercises every macro-instantiated body in `types/uint.rs` and
//! `types/sint.rs`: LeanUInt8/16/32/64, LeanUSize, LeanInt8/16/32/64,
//! LeanISize. Covers mk / to_* round trips, wrapping arithmetic (add, sub,
//! mul, neg), div/mod (including div-by-zero returning 0 and mod-by-zero
//! returning the dividend), bitwise ops, shifts (shift amounts reduced mod
//! the type width, exactly as the FFI helpers implement), decEq/decLt/decLe/
//! le/lt, isValidChar/toChar (valid + error paths), toInt/ofInt, toNat/ofNat/
//! ofNatTruncate/ofNatLT (wrapping casts), toFloat/toFloat32, log2, the
//! cross-type `conversions:` methods, and MIN/MAX/SIZE constants. All
//! expected values mirror the Rust semantics in `leo3-ffi/src/inline/numeric.rs`.

#![cfg(feature = "runtime-tests")]

use leo3::prelude::*;

// ---------------------------------------------------------------------------
// LeanUInt8
// ---------------------------------------------------------------------------

#[test]
fn test_uint8_full_surface() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // mk + to_u8 round trips.
        let zero = LeanUInt8::mk(lean, 0)?;
        let one = LeanUInt8::mk(lean, 1)?;
        let max = LeanUInt8::mk(lean, 255)?;
        assert_eq!(LeanUInt8::to_u8(&zero), 0);
        assert_eq!(LeanUInt8::to_u8(&one), 1);
        assert_eq!(LeanUInt8::to_u8(&max), 255);

        // Wrapping add: 255 + 1 -> 0 (mod 256).
        let sum = LeanUInt8::add(lean, &max, &one)?;
        assert_eq!(LeanUInt8::to_u8(&sum), 0);

        // Wrapping sub (underflow wraps).
        let diff = LeanUInt8::sub(lean, &zero, &one)?;
        assert_eq!(LeanUInt8::to_u8(&diff), 255);

        // Wrapping mul: 255 * 255 = 65025 mod 256 = 1.
        let prod = LeanUInt8::mul(lean, &max, &max)?;
        assert_eq!(LeanUInt8::to_u8(&prod), 1);

        // div and mod.
        let a = LeanUInt8::mk(lean, 10)?;
        let b = LeanUInt8::mk(lean, 3)?;
        let q = LeanUInt8::div(lean, &a, &b)?;
        let r = LeanUInt8::mod_(lean, &a, &b)?;
        assert_eq!(LeanUInt8::to_u8(&q), 3);
        assert_eq!(LeanUInt8::to_u8(&r), 1);

        // div-by-zero -> 0, mod-by-zero -> dividend.
        let z = LeanUInt8::mk(lean, 0)?;
        let q = LeanUInt8::div(lean, &a, &z)?;
        let r = LeanUInt8::mod_(lean, &a, &z)?;
        assert_eq!(LeanUInt8::to_u8(&q), 0);
        assert_eq!(LeanUInt8::to_u8(&r), 10);

        // neg wraps: neg(1) = 255, neg(0) = 0.
        let neg = LeanUInt8::neg(lean, &one)?;
        assert_eq!(LeanUInt8::to_u8(&neg), 255);
        let neg = LeanUInt8::neg(lean, &zero)?;
        assert_eq!(LeanUInt8::to_u8(&neg), 0);

        // Bitwise ops: 0b1010_1010 (170) with 0b1100_1100 (204).
        let x = LeanUInt8::mk(lean, 170)?;
        let y = LeanUInt8::mk(lean, 204)?;
        let land = LeanUInt8::land(lean, &x, &y)?;
        let lor = LeanUInt8::lor(lean, &x, &y)?;
        let xor = LeanUInt8::xor(lean, &x, &y)?;
        assert_eq!(LeanUInt8::to_u8(&land), 136); // 0b1000_1000
        assert_eq!(LeanUInt8::to_u8(&lor), 238); // 0b1110_1110
        assert_eq!(LeanUInt8::to_u8(&xor), 102); // 0b0110_0110

        let comp = LeanUInt8::complement(lean, &zero)?;
        assert_eq!(LeanUInt8::to_u8(&comp), 255);
        let comp = LeanUInt8::complement(lean, &one)?;
        assert_eq!(LeanUInt8::to_u8(&comp), 254);

        // Shifts reduce the amount mod 8 (uint8 semantics).
        let sh = LeanUInt8::shiftLeft(lean, &one, &LeanUInt8::mk(lean, 3)?)?;
        assert_eq!(LeanUInt8::to_u8(&sh), 8);
        let sh = LeanUInt8::shiftLeft(lean, &one, &LeanUInt8::mk(lean, 9)?)?; // 9 % 8 = 1
        assert_eq!(LeanUInt8::to_u8(&sh), 2);
        let big = LeanUInt8::mk(lean, 0b1000_0001)?; // 129
        let sh = LeanUInt8::shiftLeft(lean, &big, &one)?;
        assert_eq!(LeanUInt8::to_u8(&sh), 2); // wrap out the top bit
        let sh = LeanUInt8::shiftRight(lean, &big, &one)?;
        assert_eq!(LeanUInt8::to_u8(&sh), 64);

        // Comparisons.
        assert!(LeanUInt8::decEq(&one, &one));
        assert!(!LeanUInt8::decEq(&one, &zero));
        assert!(LeanUInt8::decLt(&zero, &one));
        assert!(!LeanUInt8::decLt(&one, &zero));
        assert!(LeanUInt8::decLe(&one, &one));
        assert!(LeanUInt8::decLe(&zero, &one));
        assert!(LeanUInt8::lt(&zero, &one));
        assert!(LeanUInt8::le(&one, &one));
        assert!(!LeanUInt8::le(&one, &zero));

        // Char conversions: every u8 is a valid scalar.
        let c = LeanUInt8::mk(lean, 65)?;
        assert!(LeanUInt8::isValidChar(&c));
        let ch = LeanUInt8::toChar(&c, lean)?;
        assert_eq!(LeanChar::toNat(&ch), 'A' as u32);

        // Int round trips.
        let i = LeanUInt8::toInt(&max, lean)?;
        assert_eq!(LeanInt::to_i64(&i), Some(255));
        let i300 = LeanInt::from_i64(lean, 300)?;
        let v = LeanUInt8::ofInt(lean, &i300)?;
        assert_eq!(LeanUInt8::to_u8(&v), 44); // 300 as u8 truncates
        let ineg = LeanInt::from_i64(lean, -1)?;
        let v = LeanUInt8::ofInt(lean, &ineg)?;
        assert_eq!(LeanUInt8::to_u8(&v), 255); // -1 as u8 = 255

        // Nat round trips.
        let n = LeanUInt8::toNat(&max, lean)?;
        assert_eq!(LeanNat::to_usize(&n)?, 255);
        let big_nat = LeanNat::from_usize(lean, 300)?;
        let v = LeanUInt8::ofNat(lean, &big_nat)?;
        assert_eq!(LeanUInt8::to_u8(&v), 44);
        let v = LeanUInt8::ofNatTruncate(lean, &big_nat)?;
        assert_eq!(LeanUInt8::to_u8(&v), 44);
        let v = LeanUInt8::ofNatLT(lean, &big_nat)?;
        assert_eq!(LeanUInt8::to_u8(&v), 44);
        let n255 = LeanNat::from_usize(lean, 255)?;
        let v = LeanUInt8::ofNat(lean, &n255)?;
        assert_eq!(LeanUInt8::to_u8(&v), 255);

        // Float conversions.
        let f42 = LeanUInt8::mk(lean, 42)?;
        let f = LeanUInt8::toFloat(&f42, lean)?;
        assert_eq!(LeanFloat::to_f64(&f), 42.0);
        let f = LeanUInt8::toFloat32(&f42, lean)?;
        assert_eq!(LeanFloat32::to_f32(&f), 42.0f32);

        // Cross-type conversions.
        let v16 = LeanUInt8::toUInt16(&max, lean)?;
        let v32 = LeanUInt8::toUInt32(&max, lean)?;
        let v64 = LeanUInt8::toUInt64(&max, lean)?;
        let vus = LeanUInt8::toUSize(&max, lean)?;
        assert_eq!(LeanUInt16::to_u16(&v16), 255);
        assert_eq!(LeanUInt32::to_u32(&v32), 255);
        assert_eq!(LeanUInt64::to_u64(&v64), 255);
        assert_eq!(LeanUSize::to_usize(&vus), 255);

        // log2.
        let l0 = LeanUInt8::mk(lean, 0)?;
        let l1 = LeanUInt8::mk(lean, 1)?;
        let l2 = LeanUInt8::mk(lean, 2)?;
        let l255 = LeanUInt8::mk(lean, 255)?;
        assert_eq!(LeanUInt8::log2(&l0), 0);
        assert_eq!(LeanUInt8::log2(&l1), 0);
        assert_eq!(LeanUInt8::log2(&l2), 1);
        assert_eq!(LeanUInt8::log2(&l255), 7);

        // Debug formatting.
        assert_eq!(format!("{:?}", max), "LeanUInt8(255)");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_uint8_boundaries() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        assert_eq!(LeanUInt8::MIN, 0u8);
        assert_eq!(LeanUInt8::MAX, 255u8);
        assert_eq!(LeanUInt8::SIZE, 256u32);

        // 127 + 129 wraps to 0.
        let a = LeanUInt8::mk(lean, 127)?;
        let b = LeanUInt8::mk(lean, 129)?;
        let sum = LeanUInt8::add(lean, &a, &b)?;
        assert_eq!(LeanUInt8::to_u8(&sum), 0);

        // mul wrapping: 16 * 16 = 256 -> 0.
        let m16 = LeanUInt8::mk(lean, 16)?;
        let prod = LeanUInt8::mul(lean, &m16, &m16)?;
        assert_eq!(LeanUInt8::to_u8(&prod), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// LeanUInt16
// ---------------------------------------------------------------------------

#[test]
fn test_uint16_full_surface() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let zero = LeanUInt16::mk(lean, 0)?;
        let one = LeanUInt16::mk(lean, 1)?;
        let max = LeanUInt16::mk(lean, 65535)?;
        assert_eq!(LeanUInt16::to_u16(&max), 65535);

        let sum = LeanUInt16::add(lean, &max, &one)?;
        assert_eq!(LeanUInt16::to_u16(&sum), 0);
        let diff = LeanUInt16::sub(lean, &zero, &one)?;
        assert_eq!(LeanUInt16::to_u16(&diff), 65535);

        let prod = LeanUInt16::mul(
            lean,
            &LeanUInt16::mk(lean, 256)?,
            &LeanUInt16::mk(lean, 256)?,
        )?;
        assert_eq!(LeanUInt16::to_u16(&prod), 0); // 65536 mod 65536

        let a = LeanUInt16::mk(lean, 100)?;
        let b = LeanUInt16::mk(lean, 7)?;
        assert_eq!(LeanUInt16::to_u16(&LeanUInt16::div(lean, &a, &b)?), 14);
        assert_eq!(LeanUInt16::to_u16(&LeanUInt16::mod_(lean, &a, &b)?), 2);
        let z = LeanUInt16::mk(lean, 0)?;
        assert_eq!(LeanUInt16::to_u16(&LeanUInt16::div(lean, &a, &z)?), 0);
        assert_eq!(LeanUInt16::to_u16(&LeanUInt16::mod_(lean, &a, &z)?), 100);

        let neg = LeanUInt16::neg(lean, &one)?;
        assert_eq!(LeanUInt16::to_u16(&neg), 65535);

        // Bitwise: 0xFF00 (65280) with 0x0F0F (3855).
        let x = LeanUInt16::mk(lean, 0xFF00)?;
        let y = LeanUInt16::mk(lean, 0x0F0F)?;
        assert_eq!(LeanUInt16::to_u16(&LeanUInt16::land(lean, &x, &y)?), 0x0F00);
        assert_eq!(LeanUInt16::to_u16(&LeanUInt16::lor(lean, &x, &y)?), 0xFF0F);
        assert_eq!(LeanUInt16::to_u16(&LeanUInt16::xor(lean, &x, &y)?), 0xF00F);
        assert_eq!(
            LeanUInt16::to_u16(&LeanUInt16::complement(lean, &x)?),
            0x00FF
        );

        // Shifts reduce the amount mod 16 (uint16: a2 & 0xF).
        let sh = LeanUInt16::shiftLeft(lean, &one, &LeanUInt16::mk(lean, 3)?)?;
        assert_eq!(LeanUInt16::to_u16(&sh), 8);
        let sh = LeanUInt16::shiftLeft(lean, &one, &LeanUInt16::mk(lean, 16)?)?; // 16 & 0xF = 0
        assert_eq!(LeanUInt16::to_u16(&sh), 1);
        let sh = LeanUInt16::shiftRight(lean, &x, &LeanUInt16::mk(lean, 8)?)?;
        assert_eq!(LeanUInt16::to_u16(&sh), 0xFF);
        let sh = LeanUInt16::shiftRight(
            lean,
            &LeanUInt16::mk(lean, 0x8000)?,
            &LeanUInt16::mk(lean, 15)?,
        )?;
        assert_eq!(LeanUInt16::to_u16(&sh), 1);

        // Comparisons.
        assert!(LeanUInt16::decEq(&one, &one));
        assert!(!LeanUInt16::decEq(&zero, &one));
        assert!(LeanUInt16::decLt(&zero, &one));
        assert!(LeanUInt16::decLe(&one, &one));
        assert!(LeanUInt16::lt(&zero, &one));
        assert!(LeanUInt16::le(&one, &one));

        // Char conversion.
        let c65 = LeanUInt16::mk(lean, 65)?;
        assert!(LeanUInt16::isValidChar(&c65));
        let ch = LeanUInt16::toChar(&c65, lean)?;
        assert_eq!(LeanChar::toNat(&ch), 65);

        // Int/Nat round trips.
        let i = LeanUInt16::toInt(&max, lean)?;
        assert_eq!(LeanInt::to_i64(&i), Some(65535));
        let v = LeanUInt16::ofInt(lean, &LeanInt::from_i64(lean, -1)?)?;
        assert_eq!(LeanUInt16::to_u16(&v), 65535);
        let n = LeanUInt16::toNat(&max, lean)?;
        assert_eq!(LeanNat::to_usize(&n)?, 65535);
        let big = LeanNat::from_usize(lean, 70000)?;
        let v = LeanUInt16::ofNat(lean, &big)?;
        assert_eq!(LeanUInt16::to_u16(&v), 4464); // 70000 mod 65536
        let v = LeanUInt16::ofNatTruncate(lean, &big)?;
        assert_eq!(LeanUInt16::to_u16(&v), 4464);
        let v = LeanUInt16::ofNatLT(lean, &big)?;
        assert_eq!(LeanUInt16::to_u16(&v), 4464);

        // Floats.
        let f = LeanUInt16::toFloat(&LeanUInt16::mk(lean, 42)?, lean)?;
        assert_eq!(LeanFloat::to_f64(&f), 42.0);
        let f = LeanUInt16::toFloat32(&LeanUInt16::mk(lean, 42)?, lean)?;
        assert_eq!(LeanFloat32::to_f32(&f), 42.0f32);

        // Cross-type conversions.
        let v8 = LeanUInt16::toUInt8(&LeanUInt16::mk(lean, 0x0102)?, lean)?;
        assert_eq!(LeanUInt8::to_u8(&v8), 2);
        let v32 = LeanUInt16::toUInt32(&max, lean)?;
        assert_eq!(LeanUInt32::to_u32(&v32), 65535);
        let v64 = LeanUInt16::toUInt64(&max, lean)?;
        assert_eq!(LeanUInt64::to_u64(&v64), 65535);
        let vus = LeanUInt16::toUSize(&max, lean)?;
        assert_eq!(LeanUSize::to_usize(&vus), 65535);

        // log2.
        assert_eq!(LeanUInt16::log2(&zero), 0);
        assert_eq!(LeanUInt16::log2(&one), 0);
        assert_eq!(LeanUInt16::log2(&LeanUInt16::mk(lean, 2)?), 1);
        assert_eq!(LeanUInt16::log2(&max), 15);

        assert_eq!(LeanUInt16::MIN, 0u16);
        assert_eq!(LeanUInt16::MAX, 65535u16);
        assert_eq!(LeanUInt16::SIZE, 65536u32);

        Ok(())
    });

    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// LeanUInt32
// ---------------------------------------------------------------------------

#[test]
fn test_uint32_full_surface() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let zero = LeanUInt32::mk(lean, 0)?;
        let one = LeanUInt32::mk(lean, 1)?;
        let max = LeanUInt32::mk(lean, 0xFFFF_FFFF)?;
        assert_eq!(LeanUInt32::to_u32(&max), 0xFFFF_FFFF);

        let sum = LeanUInt32::add(lean, &max, &one)?;
        assert_eq!(LeanUInt32::to_u32(&sum), 0);
        let diff = LeanUInt32::sub(lean, &zero, &one)?;
        assert_eq!(LeanUInt32::to_u32(&diff), 0xFFFF_FFFF);
        let prod = LeanUInt32::mul(
            lean,
            &LeanUInt32::mk(lean, 0x1_0000)?,
            &LeanUInt32::mk(lean, 0x1_0000)?,
        )?;
        assert_eq!(LeanUInt32::to_u32(&prod), 0); // 2^32 mod 2^32

        let a = LeanUInt32::mk(lean, 0xFFFF_FFFF)?;
        let b = LeanUInt32::mk(lean, 0x1_0000)?;
        assert_eq!(LeanUInt32::to_u32(&LeanUInt32::div(lean, &a, &b)?), 0xFFFF);
        assert_eq!(LeanUInt32::to_u32(&LeanUInt32::mod_(lean, &a, &b)?), 0xFFFF);
        let z = LeanUInt32::mk(lean, 0)?;
        assert_eq!(LeanUInt32::to_u32(&LeanUInt32::div(lean, &a, &z)?), 0);
        assert_eq!(
            LeanUInt32::to_u32(&LeanUInt32::mod_(lean, &a, &z)?),
            0xFFFF_FFFF
        );

        let neg = LeanUInt32::neg(lean, &one)?;
        assert_eq!(LeanUInt32::to_u32(&neg), 0xFFFF_FFFF);

        // Bitwise: 0xFF00FF00 with 0x0F0F0F0F.
        let x = LeanUInt32::mk(lean, 0xFF00_FF00)?;
        let y = LeanUInt32::mk(lean, 0x0F0F_0F0F)?;
        assert_eq!(
            LeanUInt32::to_u32(&LeanUInt32::land(lean, &x, &y)?),
            0x0F00_0F00
        );
        assert_eq!(
            LeanUInt32::to_u32(&LeanUInt32::lor(lean, &x, &y)?),
            0xFF0F_FF0F
        );
        assert_eq!(
            LeanUInt32::to_u32(&LeanUInt32::xor(lean, &x, &y)?),
            0xF00F_F00F
        );
        assert_eq!(
            LeanUInt32::to_u32(&LeanUInt32::complement(lean, &zero)?),
            0xFFFF_FFFF
        );

        // Shifts reduce the amount mod 32 (uint32: a2 & 31).
        let sh = LeanUInt32::shiftLeft(lean, &one, &LeanUInt32::mk(lean, 31)?)?;
        assert_eq!(LeanUInt32::to_u32(&sh), 0x8000_0000);
        let sh = LeanUInt32::shiftLeft(lean, &one, &LeanUInt32::mk(lean, 32)?)?; // 32 & 31 = 0
        assert_eq!(LeanUInt32::to_u32(&sh), 1);
        let sh = LeanUInt32::shiftRight(
            lean,
            &LeanUInt32::mk(lean, 0x8000_0000)?,
            &LeanUInt32::mk(lean, 31)?,
        )?;
        assert_eq!(LeanUInt32::to_u32(&sh), 1);

        // Comparisons.
        assert!(LeanUInt32::decEq(&one, &one));
        assert!(LeanUInt32::decLt(&zero, &one));
        assert!(LeanUInt32::decLe(&one, &one));
        assert!(LeanUInt32::lt(&zero, &one));
        assert!(LeanUInt32::le(&one, &one));

        // Char conversion: valid scalar, surrogate (invalid), > 0x10FFFF (invalid).
        let c65 = LeanUInt32::mk(lean, 65)?;
        assert!(LeanUInt32::isValidChar(&c65));
        let ch = LeanUInt32::toChar(&c65, lean)?;
        assert_eq!(LeanChar::toNat(&ch), 65);
        let surr = LeanUInt32::mk(lean, 0xD800)?;
        assert!(!LeanUInt32::isValidChar(&surr));
        assert!(LeanUInt32::toChar(&surr, lean).is_err());
        let oob = LeanUInt32::mk(lean, 0x110000)?;
        assert!(!LeanUInt32::isValidChar(&oob));
        assert!(LeanUInt32::toChar(&oob, lean).is_err());

        // Int/Nat round trips.
        let i = LeanUInt32::toInt(&LeanUInt32::mk(lean, 0x1234_5678)?, lean)?;
        assert_eq!(LeanInt::to_i64(&i), Some(0x1234_5678));
        let v = LeanUInt32::ofInt(lean, &LeanInt::from_i64(lean, 300)?)?;
        assert_eq!(LeanUInt32::to_u32(&v), 300); // 300 fits u32
        let v = LeanUInt32::ofInt(lean, &LeanInt::from_i64(lean, -1)?)?;
        assert_eq!(LeanUInt32::to_u32(&v), 0xFFFF_FFFF);
        let n = LeanUInt32::toNat(&max, lean)?;
        assert_eq!(LeanNat::to_usize(&n)?, 0xFFFF_FFFF);
        let big = LeanNat::from_usize(lean, 0x1_0000_0000)?; // 2^32
        let v = LeanUInt32::ofNat(lean, &big)?;
        assert_eq!(LeanUInt32::to_u32(&v), 0);
        let v = LeanUInt32::ofNatTruncate(lean, &big)?;
        assert_eq!(LeanUInt32::to_u32(&v), 0);
        let v = LeanUInt32::ofNatLT(lean, &big)?;
        assert_eq!(LeanUInt32::to_u32(&v), 0);

        // Floats.
        let f = LeanUInt32::toFloat(&LeanUInt32::mk(lean, 42)?, lean)?;
        assert_eq!(LeanFloat::to_f64(&f), 42.0);
        let f = LeanUInt32::toFloat32(&LeanUInt32::mk(lean, 42)?, lean)?;
        assert_eq!(LeanFloat32::to_f32(&f), 42.0f32);

        // Cross-type conversions.
        let v8 = LeanUInt32::toUInt8(&LeanUInt32::mk(lean, 0x1234_5678)?, lean)?;
        assert_eq!(LeanUInt8::to_u8(&v8), 0x78);
        let v16 = LeanUInt32::toUInt16(&LeanUInt32::mk(lean, 0x1234_5678)?, lean)?;
        assert_eq!(LeanUInt16::to_u16(&v16), 0x5678);
        let v64 = LeanUInt32::toUInt64(&max, lean)?;
        assert_eq!(LeanUInt64::to_u64(&v64), 0xFFFF_FFFF);
        let vus = LeanUInt32::toUSize(&max, lean)?;
        assert_eq!(LeanUSize::to_usize(&vus), 0xFFFF_FFFF);

        // log2.
        assert_eq!(LeanUInt32::log2(&zero), 0);
        assert_eq!(LeanUInt32::log2(&one), 0);
        assert_eq!(LeanUInt32::log2(&LeanUInt32::mk(lean, 2)?), 1);
        assert_eq!(LeanUInt32::log2(&max), 31);

        assert_eq!(LeanUInt32::MIN, 0u32);
        assert_eq!(LeanUInt32::MAX, u32::MAX);
        assert_eq!(LeanUInt32::SIZE, 4294967296u64);

        Ok(())
    });

    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// LeanUInt64
// ---------------------------------------------------------------------------

#[test]
fn test_uint64_full_surface() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let zero = LeanUInt64::mk(lean, 0)?;
        let one = LeanUInt64::mk(lean, 1)?;
        let max = LeanUInt64::mk(lean, u64::MAX)?;
        assert_eq!(LeanUInt64::to_u64(&max), u64::MAX);

        let sum = LeanUInt64::add(lean, &max, &one)?;
        assert_eq!(LeanUInt64::to_u64(&sum), 0);
        let diff = LeanUInt64::sub(lean, &zero, &one)?;
        assert_eq!(LeanUInt64::to_u64(&diff), u64::MAX);
        let prod = LeanUInt64::mul(
            lean,
            &LeanUInt64::mk(lean, 0x1_0000_0000)?,
            &LeanUInt64::mk(lean, 0x1_0000_0000)?,
        )?;
        assert_eq!(LeanUInt64::to_u64(&prod), 0); // 2^64 mod 2^64

        let a = LeanUInt64::mk(lean, u64::MAX)?;
        let b = LeanUInt64::mk(lean, 0x1_0000_0000)?;
        assert_eq!(
            LeanUInt64::to_u64(&LeanUInt64::div(lean, &a, &b)?),
            0xFFFF_FFFF
        );
        assert_eq!(
            LeanUInt64::to_u64(&LeanUInt64::mod_(lean, &a, &b)?),
            0xFFFF_FFFF
        );
        let z = LeanUInt64::mk(lean, 0)?;
        assert_eq!(LeanUInt64::to_u64(&LeanUInt64::div(lean, &a, &z)?), 0);
        assert_eq!(
            LeanUInt64::to_u64(&LeanUInt64::mod_(lean, &a, &z)?),
            u64::MAX
        );

        let neg = LeanUInt64::neg(lean, &one)?;
        assert_eq!(LeanUInt64::to_u64(&neg), u64::MAX);

        // Bitwise.
        let x = LeanUInt64::mk(lean, 0xFF00_0000_0000_00FF)?;
        let y = LeanUInt64::mk(lean, 0x0F0F)?;
        assert_eq!(LeanUInt64::to_u64(&LeanUInt64::land(lean, &x, &y)?), 0x000F);
        assert_eq!(
            LeanUInt64::to_u64(&LeanUInt64::lor(lean, &x, &y)?),
            0xFF00_0000_0000_0FFF
        );
        assert_eq!(
            LeanUInt64::to_u64(&LeanUInt64::xor(lean, &x, &y)?),
            0xFF00_0000_0000_0FF0
        );
        assert_eq!(
            LeanUInt64::to_u64(&LeanUInt64::complement(lean, &zero)?),
            u64::MAX
        );

        // Shifts reduce the amount mod 64 (uint64: a2 & 63).
        let sh = LeanUInt64::shiftLeft(lean, &one, &LeanUInt64::mk(lean, 63)?)?;
        assert_eq!(LeanUInt64::to_u64(&sh), 0x8000_0000_0000_0000);
        let sh = LeanUInt64::shiftLeft(lean, &one, &LeanUInt64::mk(lean, 64)?)?; // 64 & 63 = 0
        assert_eq!(LeanUInt64::to_u64(&sh), 1);
        let sh = LeanUInt64::shiftRight(
            lean,
            &LeanUInt64::mk(lean, 0x8000_0000_0000_0000)?,
            &LeanUInt64::mk(lean, 63)?,
        )?;
        assert_eq!(LeanUInt64::to_u64(&sh), 1);

        // Comparisons.
        assert!(LeanUInt64::decEq(&one, &one));
        assert!(!LeanUInt64::decEq(&zero, &one));
        assert!(LeanUInt64::decLt(&zero, &one));
        assert!(LeanUInt64::decLe(&one, &one));
        assert!(LeanUInt64::lt(&zero, &one));
        assert!(LeanUInt64::le(&one, &one));

        // Char conversions: > u32::MAX is out of range for a Unicode scalar.
        let c65 = LeanUInt64::mk(lean, 65)?;
        assert!(LeanUInt64::isValidChar(&c65));
        let ch = LeanUInt64::toChar(&c65, lean)?;
        assert_eq!(LeanChar::toNat(&ch), 65);
        let big = LeanUInt64::mk(lean, 0x1_0000_0000)?;
        assert!(!LeanUInt64::isValidChar(&big));
        assert!(LeanUInt64::toChar(&big, lean).is_err());

        // Int: u64::MAX does not fit i64 -> None; 42 fits.
        let i = LeanUInt64::toInt(&LeanUInt64::mk(lean, 42)?, lean)?;
        assert_eq!(LeanInt::to_i64(&i), Some(42));
        let i = LeanUInt64::toInt(&max, lean)?;
        assert_eq!(LeanInt::to_i64(&i), None);
        let v = LeanUInt64::ofInt(lean, &LeanInt::from_i64(lean, -1)?)?;
        assert_eq!(LeanUInt64::to_u64(&v), u64::MAX); // -1 as u64

        // Nat round trips (2^64 - 1 needs the big-nat path).
        let n = LeanUInt64::toNat(&max, lean)?;
        assert_eq!(LeanNat::toUInt64(&n)?, u64::MAX);
        let v = LeanUInt64::ofNat(lean, &n)?;
        assert_eq!(LeanUInt64::to_u64(&v), u64::MAX);
        let v = LeanUInt64::ofNatTruncate(lean, &n)?;
        assert_eq!(LeanUInt64::to_u64(&v), u64::MAX);
        let v = LeanUInt64::ofNatLT(lean, &n)?;
        assert_eq!(LeanUInt64::to_u64(&v), u64::MAX);

        // Floats.
        let f = LeanUInt64::toFloat(&max, lean)?;
        assert_eq!(LeanFloat::to_f64(&f), u64::MAX as f64);
        let f = LeanUInt64::toFloat32(&LeanUInt64::mk(lean, 42)?, lean)?;
        assert_eq!(LeanFloat32::to_f32(&f), 42.0f32);

        // Cross-type conversions.
        let v8 = LeanUInt64::toUInt8(&max, lean)?;
        let v16 = LeanUInt64::toUInt16(&max, lean)?;
        let v32 = LeanUInt64::toUInt32(&max, lean)?;
        let vus = LeanUInt64::toUSize(&max, lean)?;
        assert_eq!(LeanUInt8::to_u8(&v8), 255);
        assert_eq!(LeanUInt16::to_u16(&v16), 0xFFFF);
        assert_eq!(LeanUInt32::to_u32(&v32), 0xFFFF_FFFF);
        assert_eq!(LeanUSize::to_usize(&vus), usize::MAX);

        // log2.
        assert_eq!(LeanUInt64::log2(&zero), 0);
        assert_eq!(LeanUInt64::log2(&one), 0);
        assert_eq!(LeanUInt64::log2(&LeanUInt64::mk(lean, 2)?), 1);
        assert_eq!(LeanUInt64::log2(&max), 63);

        assert_eq!(LeanUInt64::MIN, 0u64);
        assert_eq!(LeanUInt64::MAX, u64::MAX);
        assert_eq!(LeanUInt64::SIZE, 18446744073709551616u128);

        Ok(())
    });

    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// LeanUSize
// ---------------------------------------------------------------------------

#[test]
fn test_usize_full_surface() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let zero = LeanUSize::mk(lean, 0)?;
        let one = LeanUSize::mk(lean, 1)?;
        let max = LeanUSize::mk(lean, usize::MAX)?;
        assert_eq!(LeanUSize::to_usize(&max), usize::MAX);

        let sum = LeanUSize::add(lean, &max, &one)?;
        assert_eq!(LeanUSize::to_usize(&sum), 0);
        let diff = LeanUSize::sub(lean, &zero, &one)?;
        assert_eq!(LeanUSize::to_usize(&diff), usize::MAX);
        let prod = LeanUSize::mul(
            lean,
            &LeanUSize::mk(lean, 0x1_0000_0000)?,
            &LeanUSize::mk(lean, 0x1_0000_0000)?,
        )?;
        assert_eq!(LeanUSize::to_usize(&prod), 0); // 2^64 mod 2^64 (64-bit host)

        let a = LeanUSize::mk(lean, usize::MAX)?;
        let b = LeanUSize::mk(lean, 0x1_0000_0000)?;
        assert_eq!(
            LeanUSize::to_usize(&LeanUSize::div(lean, &a, &b)?),
            0xFFFF_FFFF
        );
        assert_eq!(
            LeanUSize::to_usize(&LeanUSize::mod_(lean, &a, &b)?),
            0xFFFF_FFFF
        );
        let z = LeanUSize::mk(lean, 0)?;
        assert_eq!(LeanUSize::to_usize(&LeanUSize::div(lean, &a, &z)?), 0);
        assert_eq!(
            LeanUSize::to_usize(&LeanUSize::mod_(lean, &a, &z)?),
            usize::MAX
        );

        let neg = LeanUSize::neg(lean, &one)?;
        assert_eq!(LeanUSize::to_usize(&neg), usize::MAX);

        // Bitwise.
        assert_eq!(
            LeanUSize::to_usize(&LeanUSize::land(
                lean,
                &LeanUSize::mk(lean, 0b1111)?,
                &LeanUSize::mk(lean, 0b1100)?
            )?),
            0b1100
        );
        assert_eq!(
            LeanUSize::to_usize(&LeanUSize::lor(
                lean,
                &LeanUSize::mk(lean, 0b1010)?,
                &LeanUSize::mk(lean, 0b0101)?
            )?),
            0b1111
        );
        assert_eq!(
            LeanUSize::to_usize(&LeanUSize::xor(
                lean,
                &LeanUSize::mk(lean, 0b1010)?,
                &LeanUSize::mk(lean, 0b1111)?
            )?),
            0b0101
        );
        assert_eq!(
            LeanUSize::to_usize(&LeanUSize::complement(lean, &zero)?),
            usize::MAX
        );

        // Shifts reduce the amount mod 64 (usize: smod 64).
        let sh = LeanUSize::shiftLeft(lean, &one, &LeanUSize::mk(lean, 3)?)?;
        assert_eq!(LeanUSize::to_usize(&sh), 8);
        let sh = LeanUSize::shiftLeft(lean, &one, &LeanUSize::mk(lean, 63)?)?;
        assert_eq!(LeanUSize::to_usize(&sh), 0x8000_0000_0000_0000);
        let sh = LeanUSize::shiftLeft(lean, &one, &LeanUSize::mk(lean, 64)?)?; // 64 smod 64 = 0
        assert_eq!(LeanUSize::to_usize(&sh), 1);
        let sh = LeanUSize::shiftRight(
            lean,
            &LeanUSize::mk(lean, 0x8000_0000_0000_0000)?,
            &LeanUSize::mk(lean, 63)?,
        )?;
        assert_eq!(LeanUSize::to_usize(&sh), 1);

        // Comparisons.
        assert!(LeanUSize::decEq(&one, &one));
        assert!(LeanUSize::decLt(&zero, &one));
        assert!(LeanUSize::decLe(&one, &one));
        assert!(LeanUSize::lt(&zero, &one));
        assert!(LeanUSize::le(&one, &one));

        // Char conversions.
        let c65 = LeanUSize::mk(lean, 65)?;
        assert!(LeanUSize::isValidChar(&c65));
        let ch = LeanUSize::toChar(&c65, lean)?;
        assert_eq!(LeanChar::toNat(&ch), 65);
        let oob = LeanUSize::mk(lean, 0x110000)?;
        assert!(!LeanUSize::isValidChar(&oob));
        assert!(LeanUSize::toChar(&oob, lean).is_err());

        // Int/Nat round trips.
        let i = LeanUSize::toInt(&LeanUSize::mk(lean, 42)?, lean)?;
        assert_eq!(LeanInt::to_i64(&i), Some(42));
        let v = LeanUSize::ofInt(lean, &LeanInt::from_i64(lean, 300)?)?;
        assert_eq!(LeanUSize::to_usize(&v), 300);
        let n = LeanUSize::toNat(&max, lean)?;
        assert_eq!(LeanNat::toUInt64(&n)?, usize::MAX as u64);
        let v = LeanUSize::ofNat(lean, &n)?;
        assert_eq!(LeanUSize::to_usize(&v), usize::MAX);
        let v = LeanUSize::ofNatTruncate(lean, &n)?;
        assert_eq!(LeanUSize::to_usize(&v), usize::MAX);
        let v = LeanUSize::ofNatLT(lean, &n)?;
        assert_eq!(LeanUSize::to_usize(&v), usize::MAX);

        // Floats.
        let f = LeanUSize::toFloat(&max, lean)?;
        assert_eq!(LeanFloat::to_f64(&f), usize::MAX as f64);
        let f = LeanUSize::toFloat32(&LeanUSize::mk(lean, 42)?, lean)?;
        assert_eq!(LeanFloat32::to_f32(&f), 42.0f32);

        // Cross-type conversions (narrowing truncates).
        let v8 = LeanUSize::toUInt8(&LeanUSize::mk(lean, 300)?, lean)?;
        let v16 = LeanUSize::toUInt16(&LeanUSize::mk(lean, 300)?, lean)?;
        let v32 = LeanUSize::toUInt32(&LeanUSize::mk(lean, 300)?, lean)?;
        let v64 = LeanUSize::toUInt64(&LeanUSize::mk(lean, 300)?, lean)?;
        assert_eq!(LeanUInt8::to_u8(&v8), 44);
        assert_eq!(LeanUInt16::to_u16(&v16), 300);
        assert_eq!(LeanUInt32::to_u32(&v32), 300);
        assert_eq!(LeanUInt64::to_u64(&v64), 300);

        // log2.
        assert_eq!(LeanUSize::log2(&zero), 0);
        assert_eq!(LeanUSize::log2(&one), 0);
        assert_eq!(LeanUSize::log2(&LeanUSize::mk(lean, 1024)?), 10);
        assert_eq!(LeanUSize::log2(&max), 63);

        assert_eq!(LeanUSize::MIN, 0usize);
        assert_eq!(LeanUSize::MAX, usize::MAX);
        assert_eq!(LeanUSize::SIZE, 18446744073709551616u128);

        Ok(())
    });

    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// LeanInt8
// ---------------------------------------------------------------------------

#[test]
fn test_int8_full_surface() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let zero = LeanInt8::mk(lean, 0)?;
        let one = LeanInt8::mk(lean, 1)?;
        let neg_one = LeanInt8::mk(lean, -1)?;
        let min = LeanInt8::mk(lean, i8::MIN)?;
        let max = LeanInt8::mk(lean, i8::MAX)?;
        assert_eq!(LeanInt8::to_i8(&min), -128);
        assert_eq!(LeanInt8::to_i8(&max), 127);

        // Wrapping add: 127 + 1 = -128.
        let sum = LeanInt8::add(lean, &max, &one)?;
        assert_eq!(LeanInt8::to_i8(&sum), -128);
        let sum = LeanInt8::add(lean, &LeanInt8::mk(lean, -5)?, &LeanInt8::mk(lean, -3)?)?;
        assert_eq!(LeanInt8::to_i8(&sum), -8);

        // Wrapping sub: -128 - 1 = 127.
        let diff = LeanInt8::sub(lean, &min, &one)?;
        assert_eq!(LeanInt8::to_i8(&diff), 127);
        let diff = LeanInt8::sub(lean, &LeanInt8::mk(lean, 5)?, &LeanInt8::mk(lean, 10)?)?;
        assert_eq!(LeanInt8::to_i8(&diff), -5);

        // Wrapping mul: -128 * -1 = -128 (wrap); 10 * 10 = 100.
        let prod = LeanInt8::mul(lean, &min, &neg_one)?;
        assert_eq!(LeanInt8::to_i8(&prod), -128);
        let prod = LeanInt8::mul(lean, &LeanInt8::mk(lean, 10)?, &LeanInt8::mk(lean, 10)?)?;
        assert_eq!(LeanInt8::to_i8(&prod), 100);

        // Truncating div/mod (Rust semantics); div-by-zero -> 0, mod-by-zero -> lhs.
        let a = LeanInt8::mk(lean, -7)?;
        let b = LeanInt8::mk(lean, 2)?;
        assert_eq!(LeanInt8::to_i8(&LeanInt8::div(lean, &a, &b)?), -3);
        assert_eq!(LeanInt8::to_i8(&LeanInt8::mod_(lean, &a, &b)?), -1);
        let pos = LeanInt8::mk(lean, 7)?;
        let neg2 = LeanInt8::mk(lean, -2)?;
        assert_eq!(LeanInt8::to_i8(&LeanInt8::div(lean, &pos, &neg2)?), -3);
        assert_eq!(LeanInt8::to_i8(&LeanInt8::mod_(lean, &pos, &neg2)?), 1);
        let z = LeanInt8::mk(lean, 0)?;
        assert_eq!(LeanInt8::to_i8(&LeanInt8::div(lean, &pos, &z)?), 0);
        assert_eq!(LeanInt8::to_i8(&LeanInt8::mod_(lean, &pos, &z)?), 7);

        // neg / abs.
        let n = LeanInt8::neg(lean, &LeanInt8::mk(lean, -5)?)?;
        assert_eq!(LeanInt8::to_i8(&n), 5);
        let n = LeanInt8::neg(lean, &min)?;
        assert_eq!(LeanInt8::to_i8(&n), -128); // wrapping neg of MIN
        let ab = LeanInt8::abs(lean, &LeanInt8::mk(lean, -5)?)?;
        assert_eq!(LeanInt8::to_i8(&ab), 5);
        let ab = LeanInt8::abs(lean, &min)?;
        assert_eq!(LeanInt8::to_i8(&ab), -128); // abs(MIN) = MIN
        let ab = LeanInt8::abs(lean, &LeanInt8::mk(lean, 5)?)?;
        assert_eq!(LeanInt8::to_i8(&ab), 5);

        // Bitwise on the two's-complement representation.
        let land = LeanInt8::land(lean, &neg_one, &LeanInt8::mk(lean, 0x0F)?)?;
        assert_eq!(LeanInt8::to_i8(&land), 15); // 0xFF & 0x0F
        let lor = LeanInt8::lor(lean, &neg_one, &zero)?;
        assert_eq!(LeanInt8::to_i8(&lor), -1);
        let xor = LeanInt8::xor(lean, &neg_one, &zero)?;
        assert_eq!(LeanInt8::to_i8(&xor), -1);
        let comp = LeanInt8::complement(lean, &zero)?;
        assert_eq!(LeanInt8::to_i8(&comp), -1);
        let comp = LeanInt8::complement(lean, &neg_one)?;
        assert_eq!(LeanInt8::to_i8(&comp), 0);

        // Shifts reduce the amount via smod 8 and shift arithmetically.
        let sh = LeanInt8::shiftLeft(lean, &neg_one, &one)?;
        assert_eq!(LeanInt8::to_i8(&sh), -2);
        let sh = LeanInt8::shiftLeft(lean, &one, &LeanInt8::mk(lean, 3)?)?;
        assert_eq!(LeanInt8::to_i8(&sh), 8);
        let sh = LeanInt8::shiftRight(lean, &LeanInt8::mk(lean, -8)?, &LeanInt8::mk(lean, 1)?)?;
        assert_eq!(LeanInt8::to_i8(&sh), -4); // arithmetic shift
        let sh = LeanInt8::shiftRight(lean, &neg_one, &LeanInt8::mk(lean, 9)?)?; // smod 8 = 1
        assert_eq!(LeanInt8::to_i8(&sh), -1);

        // Comparisons (signed).
        assert!(LeanInt8::decEq(&neg_one, &neg_one));
        assert!(!LeanInt8::decEq(&neg_one, &one));
        assert!(LeanInt8::decLt(&neg_one, &one));
        assert!(!LeanInt8::decLt(&one, &neg_one));
        assert!(LeanInt8::decLe(&one, &one));
        assert!(LeanInt8::le(&neg_one, &one));
        assert!(LeanInt8::lt(&neg_one, &one));

        // Char: valid ASCII scalar; negative -> error.
        let c65 = LeanInt8::mk(lean, 65)?;
        assert!(LeanInt8::isValidChar(&c65));
        let ch = LeanInt8::toChar(&c65, lean)?;
        assert_eq!(LeanChar::toNat(&ch), 65);
        assert!(!LeanInt8::isValidChar(&neg_one));
        assert!(LeanInt8::toChar(&neg_one, lean).is_err());

        // Int round trips.
        let i = LeanInt8::toInt(&LeanInt8::mk(lean, -5)?, lean)?;
        assert_eq!(LeanInt::to_i64(&i), Some(-5));
        let v = LeanInt8::ofInt(lean, &LeanInt::from_i64(lean, 300)?)?;
        assert_eq!(LeanInt8::to_i8(&v), 44); // 300 as i8
        let v = LeanInt8::ofInt(lean, &LeanInt::from_i64(lean, 200)?)?;
        assert_eq!(LeanInt8::to_i8(&v), -56); // 200 as i8 wraps
        let v = LeanInt8::ofIntTruncate(lean, &LeanInt::from_i64(lean, 300)?)?;
        assert_eq!(LeanInt8::to_i8(&v), 44);

        // Nat: negative -> error; positive round trips; ofNat wraps.
        assert!(LeanInt8::toNat(&neg_one, lean).is_err());
        let n = LeanInt8::toNat(&LeanInt8::mk(lean, 42)?, lean)?;
        assert_eq!(LeanNat::to_usize(&n)?, 42);
        let n255 = LeanNat::from_usize(lean, 255)?;
        let v = LeanInt8::ofNat(lean, &n255)?;
        assert_eq!(LeanInt8::to_i8(&v), -1); // 255 as i8
        let v = LeanInt8::ofNatTruncate(lean, &n255)?;
        assert_eq!(LeanInt8::to_i8(&v), -1);
        let v = LeanInt8::ofIntTruncate(lean, &LeanInt::from_i64(lean, 255)?)?;
        assert_eq!(LeanInt8::to_i8(&v), -1);

        // Floats.
        let f = LeanInt8::toFloat(&LeanInt8::mk(lean, -5)?, lean)?;
        assert_eq!(LeanFloat::to_f64(&f), -5.0);
        let f = LeanInt8::toFloat32(&LeanInt8::mk(lean, -5)?, lean)?;
        assert_eq!(LeanFloat32::to_f32(&f), -5.0f32);

        // Cross-type conversions.
        let v16 = LeanInt8::toInt16(&neg_one, lean)?;
        let v32 = LeanInt8::toInt32(&neg_one, lean)?;
        let v64 = LeanInt8::toInt64(&neg_one, lean)?;
        let vis = LeanInt8::toISize(&neg_one, lean)?;
        assert_eq!(LeanInt16::to_i16(&v16), -1);
        assert_eq!(LeanInt32::to_i32(&v32), -1);
        assert_eq!(LeanInt64::to_i64(&v64), -1);
        assert_eq!(LeanISize::to_isize(&vis), -1);

        assert_eq!(LeanInt8::MIN, i8::MIN);
        assert_eq!(LeanInt8::MAX, i8::MAX);
        assert_eq!(LeanInt8::SIZE, 256u32);
        assert_eq!(format!("{:?}", neg_one), "LeanInt8(-1)");

        Ok(())
    });

    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// LeanInt16
// ---------------------------------------------------------------------------

#[test]
fn test_int16_full_surface() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let zero = LeanInt16::mk(lean, 0)?;
        let one = LeanInt16::mk(lean, 1)?;
        let min = LeanInt16::mk(lean, i16::MIN)?;
        let max = LeanInt16::mk(lean, i16::MAX)?;
        assert_eq!(LeanInt16::to_i16(&min), -32768);
        assert_eq!(LeanInt16::to_i16(&max), 32767);

        let sum = LeanInt16::add(lean, &max, &one)?;
        assert_eq!(LeanInt16::to_i16(&sum), -32768);
        let diff = LeanInt16::sub(lean, &min, &one)?;
        assert_eq!(LeanInt16::to_i16(&diff), 32767);
        let prod = LeanInt16::mul(lean, &LeanInt16::mk(lean, 30000)?, &LeanInt16::mk(lean, 2)?)?;
        assert_eq!(LeanInt16::to_i16(&prod), -5536); // 60000 wraps to 60000 - 65536

        let a = LeanInt16::mk(lean, -7)?;
        let b = LeanInt16::mk(lean, 2)?;
        assert_eq!(LeanInt16::to_i16(&LeanInt16::div(lean, &a, &b)?), -3);
        assert_eq!(LeanInt16::to_i16(&LeanInt16::mod_(lean, &a, &b)?), -1);
        let z = LeanInt16::mk(lean, 0)?;
        assert_eq!(LeanInt16::to_i16(&LeanInt16::div(lean, &a, &z)?), 0);
        assert_eq!(LeanInt16::to_i16(&LeanInt16::mod_(lean, &a, &z)?), -7);

        let neg = LeanInt16::neg(lean, &min)?;
        assert_eq!(LeanInt16::to_i16(&neg), -32768);
        let ab = LeanInt16::abs(lean, &min)?;
        assert_eq!(LeanInt16::to_i16(&ab), -32768);
        let ab = LeanInt16::abs(lean, &LeanInt16::mk(lean, -42)?)?;
        assert_eq!(LeanInt16::to_i16(&ab), 42);

        let x = LeanInt16::mk(lean, -1)?;
        let y = LeanInt16::mk(lean, 0x0FFF)?;
        assert_eq!(LeanInt16::to_i16(&LeanInt16::land(lean, &x, &y)?), 0x0FFF);
        assert_eq!(LeanInt16::to_i16(&LeanInt16::lor(lean, &x, &zero)?), -1);
        assert_eq!(LeanInt16::to_i16(&LeanInt16::xor(lean, &x, &zero)?), -1);
        assert_eq!(LeanInt16::to_i16(&LeanInt16::complement(lean, &zero)?), -1);

        // Shifts use smod 16.
        let sh = LeanInt16::shiftLeft(lean, &one, &LeanInt16::mk(lean, 15)?)?;
        assert_eq!(LeanInt16::to_i16(&sh), -32768);
        let sh = LeanInt16::shiftRight(lean, &LeanInt16::mk(lean, -16)?, &LeanInt16::mk(lean, 3)?)?;
        assert_eq!(LeanInt16::to_i16(&sh), -2);

        assert!(LeanInt16::decEq(&x, &x));
        assert!(LeanInt16::decLt(&x, &one));
        assert!(LeanInt16::decLe(&one, &one));
        assert!(LeanInt16::lt(&x, &one));
        assert!(LeanInt16::le(&one, &one));

        let c65 = LeanInt16::mk(lean, 65)?;
        assert!(LeanInt16::isValidChar(&c65));
        let ch = LeanInt16::toChar(&c65, lean)?;
        assert_eq!(LeanChar::toNat(&ch), 65);
        assert!(!LeanInt16::isValidChar(&x));
        assert!(LeanInt16::toChar(&x, lean).is_err());

        let i = LeanInt16::toInt(&LeanInt16::mk(lean, -5)?, lean)?;
        assert_eq!(LeanInt::to_i64(&i), Some(-5));
        let v = LeanInt16::ofInt(lean, &LeanInt::from_i64(lean, 70000)?)?;
        assert_eq!(LeanInt16::to_i16(&v), 4464); // 70000 as i16 = 4464
        let n = LeanInt16::toNat(&LeanInt16::mk(lean, 42)?, lean)?;
        assert_eq!(LeanNat::to_usize(&n)?, 42);
        assert!(LeanInt16::toNat(&x, lean).is_err());
        let big = LeanNat::from_usize(lean, 70000)?;
        let v = LeanInt16::ofNat(lean, &big)?;
        assert_eq!(LeanInt16::to_i16(&v), 4464);
        let v = LeanInt16::ofNatTruncate(lean, &big)?;
        assert_eq!(LeanInt16::to_i16(&v), 4464);

        let f = LeanInt16::toFloat(&LeanInt16::mk(lean, -5)?, lean)?;
        assert_eq!(LeanFloat::to_f64(&f), -5.0);
        let f = LeanInt16::toFloat32(&LeanInt16::mk(lean, -5)?, lean)?;
        assert_eq!(LeanFloat32::to_f32(&f), -5.0f32);

        // Cross-type conversions (narrowing truncates).
        let v8 = LeanInt16::toInt8(&LeanInt16::mk(lean, 0x0102)?, lean)?;
        let v32 = LeanInt16::toInt32(&x, lean)?;
        let v64 = LeanInt16::toInt64(&x, lean)?;
        let vis = LeanInt16::toISize(&x, lean)?;
        assert_eq!(LeanInt8::to_i8(&v8), 2);
        assert_eq!(LeanInt32::to_i32(&v32), -1);
        assert_eq!(LeanInt64::to_i64(&v64), -1);
        assert_eq!(LeanISize::to_isize(&vis), -1);

        assert_eq!(LeanInt16::MIN, i16::MIN);
        assert_eq!(LeanInt16::MAX, i16::MAX);
        assert_eq!(LeanInt16::SIZE, 65536u32);

        Ok(())
    });

    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// LeanInt32
// ---------------------------------------------------------------------------

#[test]
fn test_int32_full_surface() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let zero = LeanInt32::mk(lean, 0)?;
        let one = LeanInt32::mk(lean, 1)?;
        let min = LeanInt32::mk(lean, i32::MIN)?;
        let max = LeanInt32::mk(lean, i32::MAX)?;

        let sum = LeanInt32::add(lean, &max, &one)?;
        assert_eq!(LeanInt32::to_i32(&sum), i32::MIN);
        let diff = LeanInt32::sub(lean, &min, &one)?;
        assert_eq!(LeanInt32::to_i32(&diff), i32::MAX);
        let prod = LeanInt32::mul(
            lean,
            &LeanInt32::mk(lean, 0x1_0000)?,
            &LeanInt32::mk(lean, 0x1_0000)?,
        )?;
        assert_eq!(LeanInt32::to_i32(&prod), 0); // 2^32 mod 2^32

        let a = LeanInt32::mk(lean, -7)?;
        let b = LeanInt32::mk(lean, 2)?;
        assert_eq!(LeanInt32::to_i32(&LeanInt32::div(lean, &a, &b)?), -3);
        assert_eq!(LeanInt32::to_i32(&LeanInt32::mod_(lean, &a, &b)?), -1);
        let z = LeanInt32::mk(lean, 0)?;
        assert_eq!(LeanInt32::to_i32(&LeanInt32::div(lean, &a, &z)?), 0);
        assert_eq!(LeanInt32::to_i32(&LeanInt32::mod_(lean, &a, &z)?), -7);

        let neg = LeanInt32::neg(lean, &min)?;
        assert_eq!(LeanInt32::to_i32(&neg), i32::MIN);
        let ab = LeanInt32::abs(lean, &min)?;
        assert_eq!(LeanInt32::to_i32(&ab), i32::MIN);
        let ab = LeanInt32::abs(lean, &LeanInt32::mk(lean, -42)?)?;
        assert_eq!(LeanInt32::to_i32(&ab), 42);

        let x = LeanInt32::mk(lean, -1)?;
        let y = LeanInt32::mk(lean, 0x0FFF_FFFF)?;
        assert_eq!(
            LeanInt32::to_i32(&LeanInt32::land(lean, &x, &y)?),
            0x0FFF_FFFF
        );
        assert_eq!(LeanInt32::to_i32(&LeanInt32::lor(lean, &x, &zero)?), -1);
        assert_eq!(LeanInt32::to_i32(&LeanInt32::xor(lean, &x, &zero)?), -1);
        assert_eq!(LeanInt32::to_i32(&LeanInt32::complement(lean, &zero)?), -1);

        // Shifts use smod 32.
        let sh = LeanInt32::shiftLeft(lean, &one, &LeanInt32::mk(lean, 31)?)?;
        assert_eq!(LeanInt32::to_i32(&sh), i32::MIN);
        let sh = LeanInt32::shiftRight(lean, &LeanInt32::mk(lean, -8)?, &LeanInt32::mk(lean, 2)?)?;
        assert_eq!(LeanInt32::to_i32(&sh), -2);

        assert!(LeanInt32::decEq(&x, &x));
        assert!(LeanInt32::decLt(&x, &one));
        assert!(LeanInt32::decLe(&one, &one));
        assert!(LeanInt32::lt(&x, &one));
        assert!(LeanInt32::le(&one, &one));

        // Char: valid scalar, negative error, surrogate error.
        let c65 = LeanInt32::mk(lean, 65)?;
        assert!(LeanInt32::isValidChar(&c65));
        let ch = LeanInt32::toChar(&c65, lean)?;
        assert_eq!(LeanChar::toNat(&ch), 65);
        assert!(!LeanInt32::isValidChar(&x));
        assert!(LeanInt32::toChar(&x, lean).is_err());
        let surr = LeanInt32::mk(lean, 0xD800)?;
        assert!(!LeanInt32::isValidChar(&surr));
        assert!(LeanInt32::toChar(&surr, lean).is_err());
        let oob = LeanInt32::mk(lean, 0x110000)?;
        assert!(!LeanInt32::isValidChar(&oob));
        assert!(LeanInt32::toChar(&oob, lean).is_err());

        let i = LeanInt32::toInt(&LeanInt32::mk(lean, -5)?, lean)?;
        assert_eq!(LeanInt::to_i64(&i), Some(-5));
        let v = LeanInt32::ofInt(lean, &LeanInt::from_i64(lean, -1)?)?;
        assert_eq!(LeanInt32::to_i32(&v), -1);
        let n = LeanInt32::toNat(&LeanInt32::mk(lean, 42)?, lean)?;
        assert_eq!(LeanNat::to_usize(&n)?, 42);
        assert!(LeanInt32::toNat(&x, lean).is_err());
        let big = LeanNat::from_usize(lean, 0xFFFF_FFFF)?;
        let v = LeanInt32::ofNat(lean, &big)?;
        assert_eq!(LeanInt32::to_i32(&v), -1); // 0xFFFFFFFF as i32
        let v = LeanInt32::ofNatTruncate(lean, &big)?;
        assert_eq!(LeanInt32::to_i32(&v), -1);
        let v = LeanInt32::ofIntTruncate(lean, &LeanInt::from_i64(lean, 0xFFFF_FFFF)?)?;
        assert_eq!(LeanInt32::to_i32(&v), -1);

        let f = LeanInt32::toFloat(&LeanInt32::mk(lean, -5)?, lean)?;
        assert_eq!(LeanFloat::to_f64(&f), -5.0);
        let f = LeanInt32::toFloat32(&LeanInt32::mk(lean, -5)?, lean)?;
        assert_eq!(LeanFloat32::to_f32(&f), -5.0f32);

        // Cross-type conversions.
        let v8 = LeanInt32::toInt8(&LeanInt32::mk(lean, 0x1234_5678)?, lean)?;
        let v16 = LeanInt32::toInt16(&LeanInt32::mk(lean, 0x1234_5678)?, lean)?;
        let v64 = LeanInt32::toInt64(&x, lean)?;
        let vis = LeanInt32::toISize(&x, lean)?;
        assert_eq!(LeanInt8::to_i8(&v8), 0x78);
        assert_eq!(LeanInt16::to_i16(&v16), 0x5678);
        assert_eq!(LeanInt64::to_i64(&v64), -1);
        assert_eq!(LeanISize::to_isize(&vis), -1);

        assert_eq!(LeanInt32::MIN, i32::MIN);
        assert_eq!(LeanInt32::MAX, i32::MAX);
        assert_eq!(LeanInt32::SIZE, 4294967296u64);

        Ok(())
    });

    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// LeanInt64
// ---------------------------------------------------------------------------

#[test]
fn test_int64_full_surface() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let zero = LeanInt64::mk(lean, 0)?;
        let one = LeanInt64::mk(lean, 1)?;
        let min = LeanInt64::mk(lean, i64::MIN)?;
        let max = LeanInt64::mk(lean, i64::MAX)?;
        assert_eq!(LeanInt64::to_i64(&min), i64::MIN);
        assert_eq!(LeanInt64::to_i64(&max), i64::MAX);

        let sum = LeanInt64::add(lean, &max, &one)?;
        assert_eq!(LeanInt64::to_i64(&sum), i64::MIN);
        let diff = LeanInt64::sub(lean, &min, &one)?;
        assert_eq!(LeanInt64::to_i64(&diff), i64::MAX);
        let prod = LeanInt64::mul(
            lean,
            &LeanInt64::mk(lean, 0x1_0000_0000)?,
            &LeanInt64::mk(lean, 0x1_0000_0000)?,
        )?;
        assert_eq!(LeanInt64::to_i64(&prod), 0); // 2^64 mod 2^64

        let a = LeanInt64::mk(lean, -7)?;
        let b = LeanInt64::mk(lean, 2)?;
        assert_eq!(LeanInt64::to_i64(&LeanInt64::div(lean, &a, &b)?), -3);
        assert_eq!(LeanInt64::to_i64(&LeanInt64::mod_(lean, &a, &b)?), -1);
        let z = LeanInt64::mk(lean, 0)?;
        assert_eq!(LeanInt64::to_i64(&LeanInt64::div(lean, &a, &z)?), 0);
        assert_eq!(LeanInt64::to_i64(&LeanInt64::mod_(lean, &a, &z)?), -7);

        let neg = LeanInt64::neg(lean, &min)?;
        assert_eq!(LeanInt64::to_i64(&neg), i64::MIN);
        let ab = LeanInt64::abs(lean, &min)?;
        assert_eq!(LeanInt64::to_i64(&ab), i64::MIN);
        let ab = LeanInt64::abs(lean, &LeanInt64::mk(lean, -42)?)?;
        assert_eq!(LeanInt64::to_i64(&ab), 42);

        let x = LeanInt64::mk(lean, -1)?;
        let y = LeanInt64::mk(lean, 0x0FFF)?;
        assert_eq!(LeanInt64::to_i64(&LeanInt64::land(lean, &x, &y)?), 0x0FFF);
        assert_eq!(LeanInt64::to_i64(&LeanInt64::lor(lean, &x, &zero)?), -1);
        assert_eq!(LeanInt64::to_i64(&LeanInt64::xor(lean, &x, &zero)?), -1);
        assert_eq!(LeanInt64::to_i64(&LeanInt64::complement(lean, &zero)?), -1);

        // Shifts use smod 64.
        let sh = LeanInt64::shiftLeft(lean, &one, &LeanInt64::mk(lean, 63)?)?;
        assert_eq!(LeanInt64::to_i64(&sh), i64::MIN);
        let sh = LeanInt64::shiftRight(lean, &LeanInt64::mk(lean, -8)?, &LeanInt64::mk(lean, 2)?)?;
        assert_eq!(LeanInt64::to_i64(&sh), -2);

        assert!(LeanInt64::decEq(&x, &x));
        assert!(!LeanInt64::decEq(&zero, &one));
        assert!(LeanInt64::decLt(&x, &one));
        assert!(LeanInt64::decLe(&one, &one));
        assert!(LeanInt64::lt(&x, &one));
        assert!(LeanInt64::le(&one, &one));

        // Char: valid scalar; negative error; surrogate error.
        let c65 = LeanInt64::mk(lean, 65)?;
        assert!(LeanInt64::isValidChar(&c65));
        let ch = LeanInt64::toChar(&c65, lean)?;
        assert_eq!(LeanChar::toNat(&ch), 65);
        assert!(!LeanInt64::isValidChar(&x));
        assert!(LeanInt64::toChar(&x, lean).is_err());
        let surr = LeanInt64::mk(lean, 0xD800)?;
        assert!(!LeanInt64::isValidChar(&surr));
        assert!(LeanInt64::toChar(&surr, lean).is_err());

        // Int round trips (i64 is exactly representable).
        let i = LeanInt64::toInt(&LeanInt64::mk(lean, -5)?, lean)?;
        assert_eq!(LeanInt::to_i64(&i), Some(-5));
        let i = LeanInt64::toInt(&min, lean)?;
        assert_eq!(LeanInt::to_i64(&i), Some(i64::MIN));
        let v = LeanInt64::ofInt(lean, &LeanInt::from_i64(lean, -1)?)?;
        assert_eq!(LeanInt64::to_i64(&v), -1);

        // Nat: negative error; big ofNat wraps to -1 for 2^64-1.
        assert!(LeanInt64::toNat(&x, lean).is_err());
        let n = LeanInt64::toNat(&LeanInt64::mk(lean, 42)?, lean)?;
        assert_eq!(LeanNat::to_usize(&n)?, 42);
        let big_nat = LeanNat::from_usize(lean, usize::MAX)?;
        let v = LeanInt64::ofNat(lean, &big_nat)?;
        assert_eq!(LeanInt64::to_i64(&v), -1);
        let v = LeanInt64::ofNatTruncate(lean, &big_nat)?;
        assert_eq!(LeanInt64::to_i64(&v), -1);

        let f = LeanInt64::toFloat(&LeanInt64::mk(lean, -5)?, lean)?;
        assert_eq!(LeanFloat::to_f64(&f), -5.0);
        let f = LeanInt64::toFloat32(&LeanInt64::mk(lean, -5)?, lean)?;
        assert_eq!(LeanFloat32::to_f32(&f), -5.0f32);

        // Cross-type conversions.
        let v8 = LeanInt64::toInt8(&x, lean)?;
        let v16 = LeanInt64::toInt16(&x, lean)?;
        let v32 = LeanInt64::toInt32(&x, lean)?;
        let vis = LeanInt64::toISize(&x, lean)?;
        assert_eq!(LeanInt8::to_i8(&v8), -1);
        assert_eq!(LeanInt16::to_i16(&v16), -1);
        assert_eq!(LeanInt32::to_i32(&v32), -1);
        assert_eq!(LeanISize::to_isize(&vis), -1);

        assert_eq!(LeanInt64::MIN, i64::MIN);
        assert_eq!(LeanInt64::MAX, i64::MAX);
        assert_eq!(LeanInt64::SIZE, 18446744073709551616u128);

        Ok(())
    });

    assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// LeanISize
// ---------------------------------------------------------------------------

#[test]
fn test_isize_full_surface() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let zero = LeanISize::mk(lean, 0)?;
        let one = LeanISize::mk(lean, 1)?;
        let min = LeanISize::mk(lean, isize::MIN)?;
        let max = LeanISize::mk(lean, isize::MAX)?;
        assert_eq!(LeanISize::to_isize(&min), isize::MIN);
        assert_eq!(LeanISize::to_isize(&max), isize::MAX);

        let sum = LeanISize::add(lean, &max, &one)?;
        assert_eq!(LeanISize::to_isize(&sum), isize::MIN);
        let diff = LeanISize::sub(lean, &min, &one)?;
        assert_eq!(LeanISize::to_isize(&diff), isize::MAX);
        let prod = LeanISize::mul(
            lean,
            &LeanISize::mk(lean, 0x1_0000_0000)?,
            &LeanISize::mk(lean, 0x1_0000_0000)?,
        )?;
        assert_eq!(LeanISize::to_isize(&prod), 0); // 2^64 mod 2^64 (64-bit host)

        let a = LeanISize::mk(lean, -7)?;
        let b = LeanISize::mk(lean, 2)?;
        assert_eq!(LeanISize::to_isize(&LeanISize::div(lean, &a, &b)?), -3);
        assert_eq!(LeanISize::to_isize(&LeanISize::mod_(lean, &a, &b)?), -1);
        let z = LeanISize::mk(lean, 0)?;
        assert_eq!(LeanISize::to_isize(&LeanISize::div(lean, &a, &z)?), 0);
        assert_eq!(LeanISize::to_isize(&LeanISize::mod_(lean, &a, &z)?), -7);

        let neg = LeanISize::neg(lean, &min)?;
        assert_eq!(LeanISize::to_isize(&neg), isize::MIN);
        let ab = LeanISize::abs(lean, &min)?;
        assert_eq!(LeanISize::to_isize(&ab), isize::MIN);
        let ab = LeanISize::abs(lean, &LeanISize::mk(lean, -42)?)?;
        assert_eq!(LeanISize::to_isize(&ab), 42);

        let x = LeanISize::mk(lean, -1)?;
        let y = LeanISize::mk(lean, 0x0FFF)?;
        assert_eq!(LeanISize::to_isize(&LeanISize::land(lean, &x, &y)?), 0x0FFF);
        assert_eq!(LeanISize::to_isize(&LeanISize::lor(lean, &x, &zero)?), -1);
        assert_eq!(LeanISize::to_isize(&LeanISize::xor(lean, &x, &zero)?), -1);
        assert_eq!(
            LeanISize::to_isize(&LeanISize::complement(lean, &zero)?),
            -1
        );

        // Shifts: arithmetic on isize; small amounts stay exact.
        let sh = LeanISize::shiftLeft(lean, &one, &LeanISize::mk(lean, 3)?)?;
        assert_eq!(LeanISize::to_isize(&sh), 8);
        let sh = LeanISize::shiftRight(lean, &LeanISize::mk(lean, -8)?, &LeanISize::mk(lean, 2)?)?;
        assert_eq!(LeanISize::to_isize(&sh), -2);

        assert!(LeanISize::decEq(&x, &x));
        assert!(!LeanISize::decEq(&zero, &one));
        assert!(LeanISize::decLt(&x, &one));
        assert!(LeanISize::decLe(&one, &one));
        assert!(LeanISize::lt(&x, &one));
        assert!(LeanISize::le(&one, &one));

        // Char: valid scalar; negative error; surrogate error.
        let c65 = LeanISize::mk(lean, 65)?;
        assert!(LeanISize::isValidChar(&c65));
        let ch = LeanISize::toChar(&c65, lean)?;
        assert_eq!(LeanChar::toNat(&ch), 65);
        assert!(!LeanISize::isValidChar(&x));
        assert!(LeanISize::toChar(&x, lean).is_err());
        let surr = LeanISize::mk(lean, 0xD800)?;
        assert!(!LeanISize::isValidChar(&surr));
        assert!(LeanISize::toChar(&surr, lean).is_err());

        // Int/Nat round trips.
        let i = LeanISize::toInt(&LeanISize::mk(lean, -5)?, lean)?;
        assert_eq!(LeanInt::to_i64(&i), Some(-5));
        let v = LeanISize::ofInt(lean, &LeanInt::from_i64(lean, 300)?)?;
        assert_eq!(LeanISize::to_isize(&v), 300);
        let v = LeanISize::ofIntTruncate(lean, &LeanInt::from_i64(lean, 300)?)?;
        assert_eq!(LeanISize::to_isize(&v), 300);
        assert!(LeanISize::toNat(&x, lean).is_err());
        let n = LeanISize::toNat(&LeanISize::mk(lean, 42)?, lean)?;
        assert_eq!(LeanNat::to_usize(&n)?, 42);
        let big_nat = LeanNat::from_usize(lean, usize::MAX)?;
        let v = LeanISize::ofNat(lean, &big_nat)?;
        assert_eq!(LeanISize::to_isize(&v), -1); // usize::MAX as isize
        let v = LeanISize::ofNatTruncate(lean, &big_nat)?;
        assert_eq!(LeanISize::to_isize(&v), -1);

        let f = LeanISize::toFloat(&LeanISize::mk(lean, -5)?, lean)?;
        assert_eq!(LeanFloat::to_f64(&f), -5.0);
        let f = LeanISize::toFloat32(&LeanISize::mk(lean, -5)?, lean)?;
        assert_eq!(LeanFloat32::to_f32(&f), -5.0f32);

        // Cross-type conversions.
        let v8 = LeanISize::toInt8(&LeanISize::mk(lean, 300)?, lean)?;
        let v16 = LeanISize::toInt16(&LeanISize::mk(lean, 300)?, lean)?;
        let v32 = LeanISize::toInt32(&x, lean)?;
        let v64 = LeanISize::toInt64(&x, lean)?;
        assert_eq!(LeanInt8::to_i8(&v8), 44);
        assert_eq!(LeanInt16::to_i16(&v16), 300);
        assert_eq!(LeanInt32::to_i32(&v32), -1);
        assert_eq!(LeanInt64::to_i64(&v64), -1);

        assert_eq!(LeanISize::MIN, isize::MIN);
        assert_eq!(LeanISize::MAX, isize::MAX);
        assert_eq!(LeanISize::SIZE, 18446744073709551616u128);

        Ok(())
    });

    assert!(result.is_ok());
}
