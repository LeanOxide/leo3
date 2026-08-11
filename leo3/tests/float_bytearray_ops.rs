//! Float and ByteArray tests for Leo3
//!
//! Covers `LeanFloat` (IEEE 754 binary64), `LeanFloat32` (binary32) and
//! `LeanByteArray`: creation/conversion, arithmetic, comparisons, rounding,
//! bit manipulation, integer conversions (verified against Rust saturating
//! `as` casts, which match Lean's runtime truncate-toward-zero + saturate
//! semantics), formatting, and the bulk `Vec<u8>`/`&[u8]` byte-array helpers.

#![cfg(feature = "runtime-tests")]

use leo3::conversion::{slice_u8_into_lean, vec_u8_from_lean, vec_u8_into_lean, FromLean};
use leo3::prelude::*;

/// Run a test body inside a fresh Lean runtime.
///
/// Prepares the freethreaded runtime and reports whether the body returned
/// `Ok`.  Panics after `prepare_freethreaded_lean()` abort the process (Lean
/// interposes the unwinder), so test bodies must only assert values that
/// provably hold and must propagate failures via `?`.
fn run_lean<F>(f: F) -> bool
where
    F: for<'l> FnOnce(Lean<'l>) -> LeanResult<()>,
{
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(f).is_ok()
}

// ============================================================================
// LeanFloat (64-bit)
// ============================================================================

#[test]
fn test_float_creation_roundtrip() {
    assert!(run_lean(|lean| {
        let values: [f64; 10] = [
            0.0,
            -0.0,
            1.5,
            -2.25,
            1.0e300,
            -1.0e300,
            1.0e-300,
            f64::MAX,
            f64::MIN_POSITIVE,
            -f64::MAX,
        ];
        for &v in &values {
            let f = LeanFloat::from_f64(lean, v)?;
            assert_eq!(LeanFloat::to_f64(&f), v);
        }
        // -0.0 keeps its sign bit through the box/unbox round trip.
        let nz = LeanFloat::from_f64(lean, -0.0)?;
        assert_eq!(LeanFloat::toBits(&nz), (-0.0f64).to_bits());
        Ok(())
    }));
}

#[test]
fn test_float_constants() {
    assert!(run_lean(|lean| {
        let zero = LeanFloat::zero(lean)?;
        assert_eq!(LeanFloat::to_f64(&zero), 0.0);
        assert!(LeanFloat::isFinite(&zero));
        assert!(!LeanFloat::isNaN(&zero));
        assert!(!LeanFloat::isInf(&zero));

        let one = LeanFloat::one(lean)?;
        assert_eq!(LeanFloat::to_f64(&one), 1.0);
        assert!(LeanFloat::isFinite(&one));

        let inf = LeanFloat::infinity(lean)?;
        assert!(LeanFloat::to_f64(&inf).is_infinite());
        assert!(LeanFloat::isInf(&inf));
        assert!(!LeanFloat::isFinite(&inf));
        assert!(!LeanFloat::isNaN(&inf));

        let ninf = LeanFloat::neg_infinity(lean)?;
        assert!(LeanFloat::isInf(&ninf));
        assert!(LeanFloat::to_f64(&ninf) < 0.0);

        let nan = LeanFloat::nan(lean)?;
        assert!(LeanFloat::to_f64(&nan).is_nan());
        assert!(LeanFloat::isNaN(&nan));
        assert!(!LeanFloat::isFinite(&nan));
        assert!(!LeanFloat::isInf(&nan));
        Ok(())
    }));
}

#[test]
fn test_float_bits() {
    assert!(run_lean(|lean| {
        // toBits matches Rust's IEEE 754 bit pattern.
        let values: [f64; 6] = [0.0, -0.0, 1.5, -2.25, f64::INFINITY, f64::NEG_INFINITY];
        for &v in &values {
            let f = LeanFloat::from_f64(lean, v)?;
            assert_eq!(LeanFloat::toBits(&f), v.to_bits());
        }
        // ofBits/toBits round trip for exact patterns.
        let patterns: [u64; 4] = [
            0x0000_0000_0000_0000,
            0x3FF8_0000_0000_0000,
            0x7FF0_0000_0000_0000,
            0xFFF0_0000_0000_0000,
        ];
        for &bits in &patterns {
            let f = LeanFloat::ofBits(lean, bits)?;
            assert_eq!(LeanFloat::toBits(&f), bits);
        }
        // NaN payloads are not canonicalized by ofBits.
        let nan = LeanFloat::ofBits(lean, 0x7FF8_0000_0000_0000)?;
        assert!(LeanFloat::isNaN(&nan));
        let qnan = LeanFloat::ofBits(lean, 0x7FF4_0000_0000_0000)?;
        assert!(LeanFloat::isNaN(&qnan));
        Ok(())
    }));
}

#[test]
fn test_float_arithmetic() {
    assert!(run_lean(|lean| {
        let a = LeanFloat::from_f64(lean, 1.5)?;
        let b = LeanFloat::from_f64(lean, 2.25)?;
        assert_eq!(
            LeanFloat::to_f64(&LeanFloat::add(lean, &a, &b)?),
            1.5 + 2.25
        );
        assert_eq!(
            LeanFloat::to_f64(&LeanFloat::sub(lean, &b, &a)?),
            2.25 - 1.5
        );
        assert_eq!(
            LeanFloat::to_f64(&LeanFloat::mul(lean, &a, &b)?),
            1.5 * 2.25
        );
        assert_eq!(
            LeanFloat::to_f64(&LeanFloat::div(lean, &b, &a)?),
            2.25 / 1.5
        );

        // 0.1 + 0.2 exercises binary64 rounding the same way Rust does.
        let d1 = LeanFloat::from_f64(lean, 0.1)?;
        let d2 = LeanFloat::from_f64(lean, 0.2)?;
        assert_eq!(
            LeanFloat::to_f64(&LeanFloat::add(lean, &d1, &d2)?),
            0.1 + 0.2
        );

        // Negation (consumes the operand).
        let neg = LeanFloat::neg(lean, a)?;
        assert_eq!(LeanFloat::to_f64(&neg), -1.5);

        // 1 / 0 = +inf, 0 / 0 = NaN.
        let z = LeanFloat::zero(lean)?;
        let o = LeanFloat::one(lean)?;
        assert!(LeanFloat::to_f64(&LeanFloat::div(lean, &o, &z)?).is_infinite());
        assert!(LeanFloat::to_f64(&LeanFloat::div(lean, &z, &z)?).is_nan());

        // inf + -inf = NaN, inf * 0 = NaN.
        let inf = LeanFloat::infinity(lean)?;
        let ninf = LeanFloat::neg_infinity(lean)?;
        assert!(LeanFloat::to_f64(&LeanFloat::add(lean, &inf, &ninf)?).is_nan());
        assert!(LeanFloat::to_f64(&LeanFloat::mul(lean, &inf, &z)?).is_nan());

        // Overflow to infinity.
        let max = LeanFloat::from_f64(lean, f64::MAX)?;
        assert!(LeanFloat::to_f64(&LeanFloat::add(lean, &max, &max)?).is_infinite());

        // abs / sqrt / pow.
        let m = LeanFloat::from_f64(lean, -3.5)?;
        assert_eq!(LeanFloat::to_f64(&LeanFloat::abs(lean, m)?), 3.5);
        let sixteen = LeanFloat::from_f64(lean, 16.0)?;
        assert_eq!(LeanFloat::to_f64(&LeanFloat::sqrt(lean, sixteen)?), 4.0);
        let negone = LeanFloat::from_f64(lean, -1.0)?;
        assert!(LeanFloat::to_f64(&LeanFloat::sqrt(lean, negone)?).is_nan());
        let base = LeanFloat::from_f64(lean, 2.0)?;
        let exp10 = LeanFloat::from_f64(lean, 10.0)?;
        assert_eq!(
            LeanFloat::to_f64(&LeanFloat::pow(lean, &base, &exp10)?),
            1024.0
        );
        let zz = LeanFloat::zero(lean)?;
        assert_eq!(LeanFloat::to_f64(&LeanFloat::pow(lean, &zz, &zz)?), 1.0);
        let neg8 = LeanFloat::from_f64(lean, -8.0)?;
        let third = LeanFloat::from_f64(lean, 1.0 / 3.0)?;
        assert!(LeanFloat::to_f64(&LeanFloat::pow(lean, &neg8, &third)?).is_nan());

        // neg(-0.0) flips the sign bit to +0.0.
        let nz = LeanFloat::from_f64(lean, -0.0)?;
        let pz = LeanFloat::neg(lean, nz)?;
        assert_eq!(LeanFloat::toBits(&pz), 0.0f64.to_bits());
        Ok(())
    }));
}

#[test]
fn test_float_comparisons() {
    assert!(run_lean(|lean| {
        let a = LeanFloat::from_f64(lean, 1.5)?;
        let b = LeanFloat::from_f64(lean, 2.25)?;
        let c = LeanFloat::from_f64(lean, 1.5)?;
        assert!(LeanFloat::beq(&a, &c));
        assert!(!LeanFloat::beq(&a, &b));
        assert!(LeanFloat::lt(&a, &b));
        assert!(!LeanFloat::lt(&b, &a));
        assert!(LeanFloat::le(&a, &c));
        assert!(LeanFloat::le(&a, &b));
        assert!(!LeanFloat::le(&b, &a));

        // -0.0 compares equal to +0.0.
        let nz = LeanFloat::from_f64(lean, -0.0)?;
        let pz = LeanFloat::zero(lean)?;
        assert!(LeanFloat::beq(&nz, &pz));
        assert!(LeanFloat::le(&nz, &pz));
        assert!(!LeanFloat::lt(&nz, &pz));
        assert!(!LeanFloat::lt(&pz, &nz));

        // NaN is unequal to everything, including itself.
        let nan = LeanFloat::nan(lean)?;
        assert!(!LeanFloat::beq(&nan, &nan));
        assert!(!LeanFloat::lt(&nan, &nan));
        assert!(!LeanFloat::le(&nan, &nan));
        assert!(!LeanFloat::lt(&nan, &a));
        assert!(!LeanFloat::le(&a, &nan));

        // Infinities order correctly.
        let inf = LeanFloat::infinity(lean)?;
        let ninf = LeanFloat::neg_infinity(lean)?;
        let big = LeanFloat::from_f64(lean, 1.0e300)?;
        assert!(LeanFloat::lt(&big, &inf));
        assert!(LeanFloat::lt(&ninf, &big));
        assert!(LeanFloat::beq(&inf, &inf));
        assert!(LeanFloat::le(&inf, &inf));
        assert!(!LeanFloat::lt(&inf, &inf));

        // decLt/decLe agree with lt/le (same runtime primitives).
        assert_eq!(LeanFloat::decLt(&a, &b), LeanFloat::lt(&a, &b));
        assert_eq!(LeanFloat::decLe(&a, &b), LeanFloat::le(&a, &b));
        assert!(!LeanFloat::decLt(&a, &a));
        assert!(LeanFloat::decLe(&a, &a));
        assert!(!LeanFloat::decLt(&nan, &a));
        assert!(!LeanFloat::decLe(&nan, &a));
        Ok(())
    }));
}

#[test]
fn test_float_rounding() {
    assert!(run_lean(|lean| {
        let values: [f64; 9] = [3.7, 3.2, -3.7, -3.2, 2.5, -2.5, 0.0, -0.0, 1.0e300];
        for &v in &values {
            let f = LeanFloat::from_f64(lean, v)?;
            assert_eq!(LeanFloat::to_f64(&LeanFloat::floor(lean, f)?), v.floor());
            let f = LeanFloat::from_f64(lean, v)?;
            assert_eq!(LeanFloat::to_f64(&LeanFloat::ceil(lean, f)?), v.ceil());
            let f = LeanFloat::from_f64(lean, v)?;
            assert_eq!(LeanFloat::to_f64(&LeanFloat::round(lean, f)?), v.round());
        }
        // Sanity: explicit half-away-from-zero values.
        let h = LeanFloat::from_f64(lean, 2.5)?;
        assert_eq!(LeanFloat::to_f64(&LeanFloat::round(lean, h)?), 3.0);
        let h = LeanFloat::from_f64(lean, -2.5)?;
        assert_eq!(LeanFloat::to_f64(&LeanFloat::round(lean, h)?), -3.0);
        Ok(())
    }));
}

#[test]
fn test_float_unsigned_conversions() {
    assert!(run_lean(|lean| {
        // Lean's runtime conversions truncate toward zero and saturate out of
        // range (NaN -> 0), which is exactly Rust's saturating `as` cast.
        let values: [f64; 16] = [
            0.0,
            -0.0,
            3.7,
            -3.7,
            255.9,
            256.0,
            -1.0,
            300.0,
            12345.7,
            65535.9,
            65536.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            1.0e300,
            -1.0e300,
        ];
        for &v in &values {
            let f = LeanFloat::from_f64(lean, v)?;
            assert_eq!(LeanFloat::toUInt8(&f), v as u8);
            assert_eq!(LeanFloat::toUInt16(&f), v as u16);
            assert_eq!(LeanFloat::toUInt32(&f), v as u32);
            assert_eq!(LeanFloat::toUInt64(&f), v as u64);
            assert_eq!(LeanFloat::toUSize(&f), v as usize);
        }
        // Large magnitudes at and beyond the representable boundaries.
        let large: [f64; 4] = [
            4_294_967_294.0,
            4_294_967_296.0,
            9_223_372_036_854_775_808.0,
            18_446_744_073_709_551_615.0,
        ];
        for &v in &large {
            let f = LeanFloat::from_f64(lean, v)?;
            assert_eq!(LeanFloat::toUInt32(&f), v as u32);
            assert_eq!(LeanFloat::toUInt64(&f), v as u64);
            assert_eq!(LeanFloat::toUSize(&f), v as usize);
        }
        Ok(())
    }));
}

#[test]
fn test_float_signed_conversions() {
    assert!(run_lean(|lean| {
        let values: [f64; 20] = [
            0.0,
            -0.0,
            3.7,
            -3.7,
            127.9,
            128.0,
            -128.9,
            -129.0,
            32767.9,
            32768.0,
            -32768.9,
            -32769.0,
            2_147_483_646.0,
            2_147_483_648.0,
            -2_147_483_648.9,
            -2_147_483_649.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            1.0e300,
        ];
        for &v in &values {
            let f = LeanFloat::from_f64(lean, v)?;
            assert_eq!(LeanFloat::toInt8(&f), v as i8);
            assert_eq!(LeanFloat::toInt16(&f), v as i16);
            assert_eq!(LeanFloat::toInt32(&f), v as i32);
            assert_eq!(LeanFloat::toInt64(&f), v as i64);
            assert_eq!(LeanFloat::toISize(&f), v as isize);
        }
        // 2^63 and -2^63-1 saturate at the i64 bounds.
        let p = LeanFloat::from_f64(lean, 9_223_372_036_854_775_808.0)?;
        assert_eq!(LeanFloat::toInt64(&p), i64::MAX);
        let n = LeanFloat::from_f64(lean, -9_223_372_036_854_775_809.0)?;
        assert_eq!(LeanFloat::toInt64(&n), i64::MIN);
        Ok(())
    }));
}

#[test]
fn test_float_ofnat_ofint() {
    assert!(run_lean(|lean| {
        let nat42 = LeanNat::from_usize(lean, 42)?;
        assert_eq!(LeanFloat::to_f64(&LeanFloat::ofNat(&nat42, lean)?), 42.0);
        let nat0 = LeanNat::from_usize(lean, 0)?;
        assert_eq!(LeanFloat::to_f64(&LeanFloat::ofNat(&nat0, lean)?), 0.0);
        // Bignum nat converts through LeanNat::toFloat.
        let nat_max = LeanNat::from_usize(lean, usize::MAX)?;
        let fmax = LeanFloat::ofNat(&nat_max, lean)?;
        assert_eq!(LeanFloat::to_f64(&fmax), usize::MAX as f64);
        assert_eq!(LeanFloat::toUInt64(&fmax), usize::MAX as u64);

        let int42 = LeanInt::from_i64(lean, 42)?;
        let fi = LeanFloat::ofInt(&int42, lean)?;
        assert_eq!(LeanFloat::to_f64(&fi), 42.0);
        assert_eq!(LeanFloat::toInt64(&fi), 42);
        let neg42 = LeanInt::from_i64(lean, -42)?;
        let fi = LeanFloat::ofInt(&neg42, lean)?;
        assert_eq!(LeanFloat::to_f64(&fi), -42.0);
        assert_eq!(LeanFloat::toInt64(&fi), -42);
        let zero = LeanInt::from_i64(lean, 0)?;
        assert_eq!(LeanFloat::to_f64(&LeanFloat::ofInt(&zero, lean)?), 0.0);
        let imax = LeanInt::from_i64(lean, i64::MAX)?;
        assert_eq!(
            LeanFloat::to_f64(&LeanFloat::ofInt(&imax, lean)?),
            i64::MAX as f64
        );
        let imin = LeanInt::from_i64(lean, i64::MIN)?;
        assert_eq!(
            LeanFloat::to_f64(&LeanFloat::ofInt(&imin, lean)?),
            i64::MIN as f64
        );
        Ok(())
    }));
}

#[test]
fn test_float_tostring() {
    assert!(run_lean(|lean| {
        let cases: [(f64, &str); 5] = [
            (1.5, "1.500000"),
            (0.0, "0.000000"),
            (-0.0, "-0.000000"),
            (-2.25, "-2.250000"),
            (std::f64::consts::PI, "3.141593"),
        ];
        for &(v, s) in &cases {
            let f = LeanFloat::from_f64(lean, v)?;
            let st = LeanFloat::toString(&f, lean)?;
            assert_eq!(LeanString::cstr(&st)?, s);
        }
        let nan = LeanFloat::nan(lean)?;
        assert_eq!(LeanString::cstr(&LeanFloat::toString(&nan, lean)?)?, "NaN");
        let inf = LeanFloat::infinity(lean)?;
        assert_eq!(LeanString::cstr(&LeanFloat::toString(&inf, lean)?)?, "inf");
        let ninf = LeanFloat::neg_infinity(lean)?;
        assert_eq!(
            LeanString::cstr(&LeanFloat::toString(&ninf, lean)?)?,
            "-inf"
        );
        Ok(())
    }));
}

#[test]
fn test_float_scaleb_frexp() {
    assert!(run_lean(|lean| {
        let one5 = LeanFloat::from_f64(lean, 1.5)?;
        let n3 = LeanInt::from_i64(lean, 3)?;
        assert_eq!(
            LeanFloat::to_f64(&LeanFloat::scaleB(&one5, &n3, lean)?),
            12.0
        );
        let nm2 = LeanInt::from_i64(lean, -2)?;
        assert_eq!(
            LeanFloat::to_f64(&LeanFloat::scaleB(&one5, &nm2, lean)?),
            0.375
        );
        let n0 = LeanInt::from_i64(lean, 0)?;
        assert_eq!(
            LeanFloat::to_f64(&LeanFloat::scaleB(&one5, &n0, lean)?),
            1.5
        );
        let n10 = LeanInt::from_i64(lean, 10)?;
        let one = LeanFloat::one(lean)?;
        assert_eq!(
            LeanFloat::to_f64(&LeanFloat::scaleB(&one, &n10, lean)?),
            1024.0
        );

        // frExp returns (mantissa, exponent) with value == mantissa * 2^exp
        // and mantissa in [0.5, 1.0).
        let f8 = LeanFloat::from_f64(lean, 8.0)?;
        let pair = LeanFloat::frExp(&f8, lean)?;
        let mant = LeanProd::fst(&pair).cast::<LeanFloat>();
        let exp = LeanNat::to_usize(&LeanProd::snd(&pair).cast::<LeanNat>())?;
        assert_eq!(LeanFloat::to_f64(&mant), 0.5);
        assert_eq!(exp, 4);

        let f1 = LeanFloat::from_f64(lean, 1.0)?;
        let pair = LeanFloat::frExp(&f1, lean)?;
        assert_eq!(
            LeanFloat::to_f64(&LeanProd::fst(&pair).cast::<LeanFloat>()),
            0.5
        );
        assert_eq!(
            LeanNat::to_usize(&LeanProd::snd(&pair).cast::<LeanNat>())?,
            1
        );

        // Property: m * 2^e == v for a spread of positive exponents.
        let values: [f64; 6] = [8.0, 1.0, 0.5, 2.0, 1000.0, 0.0];
        for &v in &values {
            let f = LeanFloat::from_f64(lean, v)?;
            let p = LeanFloat::frExp(&f, lean)?;
            let m = LeanFloat::to_f64(&LeanProd::fst(&p).cast::<LeanFloat>());
            let e = LeanNat::to_usize(&LeanProd::snd(&p).cast::<LeanNat>())?;
            assert_eq!(m * 2f64.powi(e as i32), v);
            if v > 0.0 {
                assert!((0.5..1.0).contains(&m));
            }
        }
        Ok(())
    }));
}

#[test]
fn test_float_float32_interconversion() {
    assert!(run_lean(|lean| {
        // Narrowing keeps representable values exact.
        let f = LeanFloat::from_f64(lean, 1.5)?;
        let f32 = LeanFloat::toFloat32(&f, lean)?;
        assert_eq!(LeanFloat32::to_f32(&f32), 1.5f32);
        let m = LeanFloat::from_f64(lean, -2.25)?;
        let m32 = LeanFloat::toFloat32(&m, lean)?;
        assert_eq!(LeanFloat32::to_f32(&m32), -2.25f32);

        // Widening keeps values exact.
        let g = LeanFloat32::from_f32(lean, 1.5)?;
        let g64 = LeanFloat32::toFloat(&g, lean)?;
        assert_eq!(LeanFloat::to_f64(&g64), 1.5f64);

        // Precision narrowing follows f64 -> f32.
        let d01 = LeanFloat::from_f64(lean, 0.1)?;
        assert_eq!(
            LeanFloat32::to_f32(&LeanFloat::toFloat32(&d01, lean)?),
            0.1f64 as f32
        );
        // Precision widening follows f32 -> f64.
        let f01 = LeanFloat32::from_f32(lean, 0.1f32)?;
        assert_eq!(
            LeanFloat::to_f64(&LeanFloat32::toFloat(&f01, lean)?),
            0.1f32 as f64
        );

        // Overflow to infinity, NaN stays NaN.
        let big = LeanFloat::from_f64(lean, 1.0e300)?;
        assert!(LeanFloat32::to_f32(&LeanFloat::toFloat32(&big, lean)?).is_infinite());
        let nan64 = LeanFloat::nan(lean)?;
        assert!(LeanFloat32::to_f32(&LeanFloat::toFloat32(&nan64, lean)?).is_nan());
        let nan32 = LeanFloat32::nan(lean)?;
        assert!(LeanFloat::to_f64(&LeanFloat32::toFloat(&nan32, lean)?).is_nan());
        let inf32 = LeanFloat32::infinity(lean)?;
        assert!(LeanFloat::to_f64(&LeanFloat32::toFloat(&inf32, lean)?).is_infinite());

        // Round trip f32 -> f64 -> f32 preserves the f32 value.
        let x = LeanFloat32::from_f32(lean, std::f32::consts::PI)?;
        let x64 = LeanFloat32::toFloat(&x, lean)?;
        let x32 = LeanFloat::toFloat32(&x64, lean)?;
        assert_eq!(LeanFloat32::to_f32(&x32), std::f32::consts::PI);
        Ok(())
    }));
}

#[test]
fn test_float_debug_format() {
    assert!(run_lean(|lean| {
        let f = LeanFloat::from_f64(lean, 1.5)?;
        assert_eq!(format!("{:?}", f), "LeanFloat(1.5)");
        let g = LeanFloat::from_f64(lean, -2.25)?;
        assert_eq!(format!("{:?}", g), "LeanFloat(-2.25)");
        let nan = LeanFloat::nan(lean)?;
        assert_eq!(format!("{:?}", nan), "LeanFloat(NaN)");
        let inf = LeanFloat::infinity(lean)?;
        assert_eq!(format!("{:?}", inf), "LeanFloat(inf)");
        let ninf = LeanFloat::neg_infinity(lean)?;
        assert_eq!(format!("{:?}", ninf), "LeanFloat(-inf)");
        Ok(())
    }));
}

// ============================================================================
// LeanFloat32 (32-bit)
// ============================================================================

#[test]
fn test_float32_creation_roundtrip() {
    assert!(run_lean(|lean| {
        let values: [f32; 9] = [
            0.0,
            -0.0,
            1.5,
            -2.25,
            std::f32::consts::PI,
            f32::MAX,
            -f32::MAX,
            f32::MIN_POSITIVE,
            1.0e-45,
        ];
        for &v in &values {
            let f = LeanFloat32::from_f32(lean, v)?;
            assert_eq!(LeanFloat32::to_f32(&f), v);
        }
        let nz = LeanFloat32::from_f32(lean, -0.0)?;
        assert_eq!(LeanFloat32::toBits(&nz), (-0.0f32).to_bits());
        Ok(())
    }));
}

#[test]
fn test_float32_constants() {
    assert!(run_lean(|lean| {
        let zero = LeanFloat32::zero(lean)?;
        assert_eq!(LeanFloat32::to_f32(&zero), 0.0);
        assert!(LeanFloat32::isFinite(&zero));
        assert!(!LeanFloat32::isNaN(&zero));
        assert!(!LeanFloat32::isInf(&zero));

        let one = LeanFloat32::one(lean)?;
        assert_eq!(LeanFloat32::to_f32(&one), 1.0);

        let inf = LeanFloat32::infinity(lean)?;
        assert!(LeanFloat32::isInf(&inf));
        assert!(!LeanFloat32::isFinite(&inf));
        assert!(!LeanFloat32::isNaN(&inf));

        let ninf = LeanFloat32::neg_infinity(lean)?;
        assert!(LeanFloat32::isInf(&ninf));
        assert!(LeanFloat32::to_f32(&ninf) < 0.0);

        let nan = LeanFloat32::nan(lean)?;
        assert!(LeanFloat32::isNaN(&nan));
        assert!(!LeanFloat32::isFinite(&nan));
        assert!(!LeanFloat32::isInf(&nan));
        Ok(())
    }));
}

#[test]
fn test_float32_bits() {
    assert!(run_lean(|lean| {
        let patterns: [u32; 5] = [
            0x0000_0000,
            0x3FC0_0000,
            0xC020_0000,
            0x7F80_0000,
            0xFF80_0000,
        ];
        for &bits in &patterns {
            let f = LeanFloat32::ofBits(lean, bits)?;
            assert_eq!(LeanFloat32::toBits(&f), bits);
        }
        let nan = LeanFloat32::ofBits(lean, 0x7FC0_0000)?;
        assert!(LeanFloat32::isNaN(&nan));

        let values: [f32; 6] = [0.0, -0.0, 1.5, -2.25, f32::INFINITY, f32::NEG_INFINITY];
        for &v in &values {
            let f = LeanFloat32::from_f32(lean, v)?;
            assert_eq!(LeanFloat32::toBits(&f), v.to_bits());
        }
        Ok(())
    }));
}

#[test]
fn test_float32_arithmetic() {
    assert!(run_lean(|lean| {
        let a = LeanFloat32::from_f32(lean, 1.5)?;
        let b = LeanFloat32::from_f32(lean, 2.25)?;
        assert_eq!(
            LeanFloat32::to_f32(&LeanFloat32::add(lean, &a, &b)?),
            1.5f32 + 2.25f32
        );
        assert_eq!(
            LeanFloat32::to_f32(&LeanFloat32::sub(lean, &b, &a)?),
            2.25f32 - 1.5f32
        );
        assert_eq!(
            LeanFloat32::to_f32(&LeanFloat32::mul(lean, &a, &b)?),
            1.5f32 * 2.25f32
        );
        assert_eq!(
            LeanFloat32::to_f32(&LeanFloat32::div(lean, &b, &a)?),
            2.25f32 / 1.5f32
        );

        // 0.1 + 0.2 in binary32.
        let d1 = LeanFloat32::from_f32(lean, 0.1f32)?;
        let d2 = LeanFloat32::from_f32(lean, 0.2f32)?;
        assert_eq!(
            LeanFloat32::to_f32(&LeanFloat32::add(lean, &d1, &d2)?),
            0.1f32 + 0.2f32
        );

        let neg = LeanFloat32::neg(lean, a)?;
        assert_eq!(LeanFloat32::to_f32(&neg), -1.5f32);

        let z = LeanFloat32::zero(lean)?;
        let o = LeanFloat32::one(lean)?;
        assert!(LeanFloat32::to_f32(&LeanFloat32::div(lean, &o, &z)?).is_infinite());
        assert!(LeanFloat32::to_f32(&LeanFloat32::div(lean, &z, &z)?).is_nan());

        let inf = LeanFloat32::infinity(lean)?;
        assert!(LeanFloat32::to_f32(&LeanFloat32::mul(lean, &inf, &z)?).is_nan());

        let max = LeanFloat32::from_f32(lean, f32::MAX)?;
        assert!(LeanFloat32::to_f32(&LeanFloat32::add(lean, &max, &max)?).is_infinite());

        let m = LeanFloat32::from_f32(lean, -3.5)?;
        assert_eq!(LeanFloat32::to_f32(&LeanFloat32::abs(lean, m)?), 3.5f32);
        let sixteen = LeanFloat32::from_f32(lean, 16.0)?;
        assert_eq!(
            LeanFloat32::to_f32(&LeanFloat32::sqrt(lean, sixteen)?),
            4.0f32
        );
        let negone = LeanFloat32::from_f32(lean, -1.0)?;
        assert!(LeanFloat32::to_f32(&LeanFloat32::sqrt(lean, negone)?).is_nan());
        let base = LeanFloat32::from_f32(lean, 2.0)?;
        let exp10 = LeanFloat32::from_f32(lean, 10.0)?;
        assert_eq!(
            LeanFloat32::to_f32(&LeanFloat32::pow(lean, &base, &exp10)?),
            1024.0f32
        );
        Ok(())
    }));
}

#[test]
fn test_float32_comparisons() {
    assert!(run_lean(|lean| {
        let a = LeanFloat32::from_f32(lean, 1.5)?;
        let b = LeanFloat32::from_f32(lean, 2.25)?;
        let c = LeanFloat32::from_f32(lean, 1.5)?;
        assert!(LeanFloat32::beq(&a, &c));
        assert!(!LeanFloat32::beq(&a, &b));
        assert!(LeanFloat32::lt(&a, &b));
        assert!(!LeanFloat32::lt(&b, &a));
        assert!(LeanFloat32::le(&a, &c));
        assert!(!LeanFloat32::le(&b, &a));

        let nz = LeanFloat32::from_f32(lean, -0.0)?;
        let pz = LeanFloat32::zero(lean)?;
        assert!(LeanFloat32::beq(&nz, &pz));
        assert!(!LeanFloat32::lt(&nz, &pz));

        let nan = LeanFloat32::nan(lean)?;
        assert!(!LeanFloat32::beq(&nan, &nan));
        assert!(!LeanFloat32::lt(&nan, &nan));
        assert!(!LeanFloat32::le(&nan, &nan));
        assert!(!LeanFloat32::le(&a, &nan));

        let inf = LeanFloat32::infinity(lean)?;
        let ninf = LeanFloat32::neg_infinity(lean)?;
        let big = LeanFloat32::from_f32(lean, 1.0e30)?;
        assert!(LeanFloat32::lt(&big, &inf));
        assert!(LeanFloat32::lt(&ninf, &big));
        assert!(LeanFloat32::beq(&inf, &inf));

        assert_eq!(LeanFloat32::decLt(&a, &b), LeanFloat32::lt(&a, &b));
        assert_eq!(LeanFloat32::decLe(&a, &b), LeanFloat32::le(&a, &b));
        assert!(!LeanFloat32::decLt(&nan, &a));
        assert!(!LeanFloat32::decLe(&nan, &a));
        Ok(())
    }));
}

#[test]
fn test_float32_rounding() {
    assert!(run_lean(|lean| {
        let values: [f32; 8] = [3.7, 3.2, -3.7, -3.2, 2.5, -2.5, 0.0, 1.0e30];
        for &v in &values {
            let f = LeanFloat32::from_f32(lean, v)?;
            assert_eq!(
                LeanFloat32::to_f32(&LeanFloat32::floor(lean, f)?),
                v.floor()
            );
            let f = LeanFloat32::from_f32(lean, v)?;
            assert_eq!(LeanFloat32::to_f32(&LeanFloat32::ceil(lean, f)?), v.ceil());
            let f = LeanFloat32::from_f32(lean, v)?;
            assert_eq!(
                LeanFloat32::to_f32(&LeanFloat32::round(lean, f)?),
                v.round()
            );
        }
        Ok(())
    }));
}

#[test]
fn test_float32_unsigned_conversions() {
    assert!(run_lean(|lean| {
        let values: [f32; 15] = [
            0.0,
            -0.0,
            3.7,
            -3.7,
            255.9,
            256.0,
            -1.0,
            300.0,
            12345.7,
            65535.9,
            65536.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            1.0e38,
        ];
        for &v in &values {
            let f = LeanFloat32::from_f32(lean, v)?;
            assert_eq!(LeanFloat32::toUInt8(&f), v as u8);
            assert_eq!(LeanFloat32::toUInt16(&f), v as u16);
            assert_eq!(LeanFloat32::toUInt32(&f), v as u32);
            assert_eq!(LeanFloat32::toUInt64(&f), v as u64);
            assert_eq!(LeanFloat32::toUSize(&f), v as usize);
        }
        let large: [f32; 5] = [
            4_294_967_294.0,
            4_294_967_296.0,
            1.0e19,
            1.0e20,
            3.4028235e38,
        ];
        for &v in &large {
            let f = LeanFloat32::from_f32(lean, v)?;
            assert_eq!(LeanFloat32::toUInt32(&f), v as u32);
            assert_eq!(LeanFloat32::toUInt64(&f), v as u64);
            assert_eq!(LeanFloat32::toUSize(&f), v as usize);
        }
        Ok(())
    }));
}

#[test]
fn test_float32_signed_conversions() {
    assert!(run_lean(|lean| {
        let values: [f32; 18] = [
            0.0,
            -0.0,
            3.7,
            -3.7,
            127.9,
            128.0,
            -128.9,
            -129.0,
            32767.9,
            32768.0,
            -32768.9,
            -32769.0,
            2_147_483_646.0,
            2_147_483_648.0,
            -2_147_483_648.9,
            -2_147_483_649.0,
            f32::NAN,
            f32::INFINITY,
        ];
        for &v in &values {
            let f = LeanFloat32::from_f32(lean, v)?;
            assert_eq!(LeanFloat32::toInt8(&f), v as i8);
            assert_eq!(LeanFloat32::toInt16(&f), v as i16);
            assert_eq!(LeanFloat32::toInt32(&f), v as i32);
            assert_eq!(LeanFloat32::toInt64(&f), v as i64);
            assert_eq!(LeanFloat32::toISize(&f), v as isize);
        }
        // 2^63 and -2^63 (exactly representable in binary32) saturate.
        let p = LeanFloat32::from_f32(lean, 9.223372e18)?;
        assert_eq!(LeanFloat32::toInt64(&p), i64::MAX);
        let n = LeanFloat32::from_f32(lean, -9.223372e18)?;
        assert_eq!(LeanFloat32::toInt64(&n), i64::MIN);
        Ok(())
    }));
}

#[test]
fn test_float32_tostring_scaleb_frexp() {
    assert!(run_lean(|lean| {
        let cases: [(f32, &str); 5] = [
            (1.5, "1.500000"),
            (0.0, "0.000000"),
            (-0.0, "-0.000000"),
            (-2.25, "-2.250000"),
            (std::f32::consts::PI, "3.141593"),
        ];
        for &(v, s) in &cases {
            let f = LeanFloat32::from_f32(lean, v)?;
            let st = LeanFloat32::toString(&f, lean)?;
            assert_eq!(LeanString::cstr(&st)?, s);
        }
        let nan = LeanFloat32::nan(lean)?;
        assert_eq!(
            LeanString::cstr(&LeanFloat32::toString(&nan, lean)?)?,
            "NaN"
        );
        let inf = LeanFloat32::infinity(lean)?;
        assert_eq!(
            LeanString::cstr(&LeanFloat32::toString(&inf, lean)?)?,
            "inf"
        );
        let ninf = LeanFloat32::neg_infinity(lean)?;
        assert_eq!(
            LeanString::cstr(&LeanFloat32::toString(&ninf, lean)?)?,
            "-inf"
        );

        let one5 = LeanFloat32::from_f32(lean, 1.5)?;
        let n3 = LeanInt::from_i64(lean, 3)?;
        assert_eq!(
            LeanFloat32::to_f32(&LeanFloat32::scaleB(&one5, &n3, lean)?),
            12.0
        );
        let nm2 = LeanInt::from_i64(lean, -2)?;
        assert_eq!(
            LeanFloat32::to_f32(&LeanFloat32::scaleB(&one5, &nm2, lean)?),
            0.375
        );

        // Note: the Float32 frexp mantissa is boxed as a Float32 (4 bytes),
        // so it must be read back through LeanFloat32.
        let f8 = LeanFloat32::from_f32(lean, 8.0)?;
        let pair = LeanFloat32::frExp(&f8, lean)?;
        assert_eq!(
            LeanFloat32::to_f32(&LeanProd::fst(&pair).cast::<LeanFloat32>()),
            0.5
        );
        assert_eq!(
            LeanNat::to_usize(&LeanProd::snd(&pair).cast::<LeanNat>())?,
            4
        );

        let f1 = LeanFloat32::from_f32(lean, 1.0)?;
        let pair = LeanFloat32::frExp(&f1, lean)?;
        assert_eq!(
            LeanFloat32::to_f32(&LeanProd::fst(&pair).cast::<LeanFloat32>()),
            0.5
        );
        assert_eq!(
            LeanNat::to_usize(&LeanProd::snd(&pair).cast::<LeanNat>())?,
            1
        );
        Ok(())
    }));
}

#[test]
fn test_float32_debug_format() {
    assert!(run_lean(|lean| {
        let f = LeanFloat32::from_f32(lean, 1.5)?;
        assert_eq!(format!("{:?}", f), "LeanFloat32(1.5)");
        let g = LeanFloat32::from_f32(lean, -2.25)?;
        assert_eq!(format!("{:?}", g), "LeanFloat32(-2.25)");
        let nan = LeanFloat32::nan(lean)?;
        assert_eq!(format!("{:?}", nan), "LeanFloat32(NaN)");
        let inf = LeanFloat32::infinity(lean)?;
        assert_eq!(format!("{:?}", inf), "LeanFloat32(inf)");
        let ninf = LeanFloat32::neg_infinity(lean)?;
        assert_eq!(format!("{:?}", ninf), "LeanFloat32(-inf)");
        Ok(())
    }));
}

// ============================================================================
// LeanByteArray
// ============================================================================

#[test]
fn test_bytearray_empty_and_capacity() {
    assert!(run_lean(|lean| {
        let ba = LeanByteArray::empty(lean)?;
        assert_eq!(LeanByteArray::size(&ba), 0);
        assert!(LeanByteArray::isEmpty(&ba));
        assert_eq!(LeanByteArray::capacity(&ba), 0);
        assert_eq!(LeanByteArray::to_vec(&ba), Vec::<u8>::new());

        let cap16 = LeanByteArray::with_capacity(lean, 16)?;
        assert_eq!(LeanByteArray::size(&cap16), 0);
        assert!(LeanByteArray::isEmpty(&cap16));
        assert_eq!(LeanByteArray::capacity(&cap16), 16);

        let cap0 = LeanByteArray::with_capacity(lean, 0)?;
        assert_eq!(LeanByteArray::capacity(&cap0), 0);

        let cap256 = LeanByteArray::with_capacity(lean, 256)?;
        assert_eq!(LeanByteArray::capacity(&cap256), 256);
        assert_eq!(LeanByteArray::size(&cap256), 0);
        assert!(LeanByteArray::isEmpty(&cap256));
        Ok(())
    }));
}

#[test]
fn test_bytearray_from_bytes_to_vec() {
    assert!(run_lean(|lean| {
        let ba = LeanByteArray::from_bytes(lean, b"Hello, World!")?;
        assert_eq!(LeanByteArray::size(&ba), 13);
        assert!(!LeanByteArray::isEmpty(&ba));
        assert_eq!(LeanByteArray::to_vec(&ba), b"Hello, World!");

        let empty = LeanByteArray::from_bytes(lean, b"")?;
        assert_eq!(LeanByteArray::size(&empty), 0);
        assert!(LeanByteArray::isEmpty(&empty));
        assert_eq!(LeanByteArray::to_vec(&empty), Vec::<u8>::new());

        let bytes: [u8; 6] = [0, 255, 128, 1, 0, 7];
        let ba = LeanByteArray::from_bytes(lean, &bytes)?;
        assert_eq!(LeanByteArray::size(&ba), 6);
        assert_eq!(LeanByteArray::to_vec(&ba), bytes);

        // Large buffer: all 256 byte values, repeated.
        let large: Vec<u8> = (0u8..=255).cycle().take(100_000).collect();
        let ba = LeanByteArray::from_bytes(lean, &large)?;
        assert_eq!(LeanByteArray::size(&ba), 100_000);
        assert_eq!(LeanByteArray::to_vec(&ba), large);
        assert!(LeanByteArray::capacity(&ba) >= 100_000);
        Ok(())
    }));
}

#[test]
fn test_bytearray_push_get() {
    assert!(run_lean(|lean| {
        let mut ba = LeanByteArray::empty(lean)?;
        ba = LeanByteArray::push(ba, 1)?;
        ba = LeanByteArray::push(ba, 2)?;
        ba = LeanByteArray::push(ba, 3)?;
        assert_eq!(LeanByteArray::size(&ba), 3);
        assert_eq!(LeanByteArray::get(&ba, 0), Some(1));
        assert_eq!(LeanByteArray::get(&ba, 1), Some(2));
        assert_eq!(LeanByteArray::get(&ba, 2), Some(3));
        assert_eq!(LeanByteArray::get(&ba, 3), None);
        assert_eq!(LeanByteArray::get(&ba, 100), None);
        assert!(!LeanByteArray::isEmpty(&ba));

        // Push enough bytes to force capacity growth past the initial 4.
        let mut big = LeanByteArray::with_capacity(lean, 4)?;
        for i in 0u16..=255 {
            big = LeanByteArray::push(big, i as u8)?;
        }
        assert_eq!(LeanByteArray::size(&big), 256);
        assert_eq!(LeanByteArray::get(&big, 0), Some(0));
        assert_eq!(LeanByteArray::get(&big, 127), Some(127));
        assert_eq!(LeanByteArray::get(&big, 255), Some(255));
        assert_eq!(LeanByteArray::get(&big, 256), None);
        let expected: Vec<u8> = (0u8..=255).collect();
        assert_eq!(LeanByteArray::to_vec(&big), expected);
        assert!(LeanByteArray::capacity(&big) >= 256);
        Ok(())
    }));
}

#[test]
fn test_bytearray_set() {
    assert!(run_lean(|lean| {
        let mut ba = LeanByteArray::from_bytes(lean, &[1, 2, 3, 4, 5])?;
        ba = LeanByteArray::set(ba, 2, 99)?;
        assert_eq!(LeanByteArray::size(&ba), 5);
        assert_eq!(LeanByteArray::get(&ba, 2), Some(99));
        assert_eq!(LeanByteArray::get(&ba, 1), Some(2));
        assert_eq!(LeanByteArray::get(&ba, 3), Some(4));

        // Out-of-bounds set returns the array unchanged.
        let unchanged = LeanByteArray::set(ba, 5, 42)?;
        assert_eq!(LeanByteArray::size(&unchanged), 5);
        assert_eq!(LeanByteArray::get(&unchanged, 5), None);
        assert_eq!(LeanByteArray::get(&unchanged, 2), Some(99));

        // Setting on an empty array is a no-op.
        let small = LeanByteArray::empty(lean)?;
        let same = LeanByteArray::set(small, 0, 7)?;
        assert_eq!(LeanByteArray::size(&same), 0);
        assert_eq!(LeanByteArray::get(&same, 0), None);

        // Unsafe in-bounds uset.
        let mut raw = LeanByteArray::from_bytes(lean, &[10, 20, 30])?;
        unsafe {
            raw = LeanByteArray::uset(raw, 1, 77)?;
        }
        assert_eq!(LeanByteArray::size(&raw), 3);
        assert_eq!(LeanByteArray::get(&raw, 1), Some(77));
        assert_eq!(LeanByteArray::get(&raw, 0), Some(10));
        Ok(())
    }));
}

#[test]
fn test_bytearray_uget_as_slice() {
    assert!(run_lean(|lean| {
        let ba = LeanByteArray::from_bytes(lean, b"slice")?;
        unsafe {
            assert_eq!(LeanByteArray::uget(&ba, 0), b's');
            assert_eq!(LeanByteArray::uget(&ba, 4), b'e');
            assert_eq!(LeanByteArray::as_slice(&ba), b"slice");
        }
        assert_eq!(LeanByteArray::to_vec(&ba), b"slice");

        let empty = LeanByteArray::empty(lean)?;
        unsafe {
            assert_eq!(LeanByteArray::as_slice(&empty), &[]);
        }
        Ok(())
    }));
}

#[test]
fn test_bytearray_fromlean_identity() {
    assert!(run_lean(|lean| {
        let ba = LeanByteArray::from_bytes(lean, &[5, 6, 7, 8])?;
        let clone: LeanBound<LeanByteArray> = FromLean::from_lean(&ba)?;
        assert_eq!(LeanByteArray::size(&clone), 4);
        assert_eq!(LeanByteArray::to_vec(&clone), vec![5, 6, 7, 8]);
        assert_eq!(LeanByteArray::to_vec(&clone), LeanByteArray::to_vec(&ba));

        let e = LeanByteArray::empty(lean)?;
        let ec: LeanBound<LeanByteArray> = FromLean::from_lean(&e)?;
        assert!(LeanByteArray::isEmpty(&ec));
        assert_eq!(LeanByteArray::size(&ec), 0);
        Ok(())
    }));
}

#[test]
fn test_bytearray_vec_u8_helpers() {
    assert!(run_lean(|lean| {
        // Empty buffer.
        let ba = vec_u8_into_lean(Vec::<u8>::new(), lean)?;
        assert_eq!(LeanByteArray::size(&ba), 0);
        assert!(LeanByteArray::isEmpty(&ba));
        assert_eq!(vec_u8_from_lean(&ba), Vec::<u8>::new());

        // Small buffer.
        let small = vec![10u8, 20, 30, 40, 50];
        let ba = vec_u8_into_lean(small.clone(), lean)?;
        assert_eq!(LeanByteArray::size(&ba), 5);
        assert_eq!(LeanByteArray::to_vec(&ba), small);
        assert_eq!(vec_u8_from_lean(&ba), small);

        // Large buffer (bulk memcpy path).
        let large: Vec<u8> = (0u8..=255).cycle().take(10_000).collect();
        let ba = vec_u8_into_lean(large.clone(), lean)?;
        assert_eq!(LeanByteArray::size(&ba), 10_000);
        assert_eq!(vec_u8_from_lean(&ba), large);

        // Slice helper.
        let sl = slice_u8_into_lean(b"Hello, World!", lean)?;
        assert_eq!(LeanByteArray::size(&sl), 13);
        assert_eq!(vec_u8_from_lean(&sl), b"Hello, World!");
        let sl_empty = slice_u8_into_lean(&[], lean)?;
        assert_eq!(LeanByteArray::size(&sl_empty), 0);
        assert_eq!(vec_u8_from_lean(&sl_empty), Vec::<u8>::new());
        let sl_small = slice_u8_into_lean(&[1, 2, 3], lean)?;
        assert_eq!(vec_u8_from_lean(&sl_small), vec![1, 2, 3]);

        // Cross-check: from_bytes and vec_u8_into_lean agree.
        let a = LeanByteArray::from_bytes(lean, &[9, 8, 7])?;
        let b = vec_u8_into_lean(vec![9, 8, 7], lean)?;
        assert_eq!(LeanByteArray::to_vec(&a), LeanByteArray::to_vec(&b));
        assert_eq!(vec_u8_from_lean(&a), vec![9, 8, 7]);
        Ok(())
    }));
}

#[test]
fn test_bytearray_debug_format() {
    assert!(run_lean(|lean| {
        let ba = LeanByteArray::from_bytes(lean, &[1, 2, 3])?;
        assert_eq!(format!("{:?}", ba), "LeanByteArray(size: 3)");
        let e = LeanByteArray::empty(lean)?;
        assert_eq!(format!("{:?}", e), "LeanByteArray(size: 0)");
        Ok(())
    }));
}
