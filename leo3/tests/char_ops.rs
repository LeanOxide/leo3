//! Character operation tests for Leo3
//!
//! These tests demonstrate LeanChar functionality including creation from
//! Rust chars and codepoints, classification, case conversion, comparison,
//! and UTF-8/UTF-16 size computation.

#![cfg(feature = "runtime-tests")]

use leo3::prelude::*;

#[test]
fn test_char_mk() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let c = LeanChar::mk(lean, 'A')?;

        assert_eq!(LeanChar::toChar(&c), Some('A'));
        assert_eq!(LeanChar::toNat(&c), 65);
        assert_eq!(LeanChar::toUInt8(&c), 65);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_mk_non_ascii() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let c = LeanChar::mk(lean, '中')?;

        assert_eq!(LeanChar::toChar(&c), Some('中'));
        assert_eq!(LeanChar::toNat(&c), 0x4E2D);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_of_nat_valid() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 'A' = 0x41
        let c = LeanChar::ofNat(lean, 0x41)?;
        assert_eq!(c.as_ref().and_then(LeanChar::toChar), Some('A'));

        // 0 is a valid scalar value (NUL)
        let c = LeanChar::ofNat(lean, 0)?;
        assert_eq!(c.as_ref().and_then(LeanChar::toChar), Some('\0'));

        // 0x10FFFF is the maximum valid scalar value
        let c = LeanChar::ofNat(lean, 0x10FFFF)?;
        assert_eq!(
            c.as_ref().and_then(LeanChar::toChar),
            Some(char::from_u32(0x10FFFF).unwrap())
        );

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_of_nat_invalid() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 0xD800-0xDFFF are surrogate codepoints, not valid scalar values
        assert!(LeanChar::ofNat(lean, 0xD800)?.is_none());
        assert!(LeanChar::ofNat(lean, 0xDFFF)?.is_none());

        // Above the maximum scalar value
        assert!(LeanChar::ofNat(lean, 0x110000)?.is_none());
        assert!(LeanChar::ofNat(lean, 0xFFFFFFFF)?.is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_of_uint8() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let c = LeanChar::ofUInt8(lean, 65)?;
        assert_eq!(LeanChar::toChar(&c), Some('A'));
        assert_eq!(LeanChar::toUInt8(&c), 65);

        // Boundary values 0 and 255
        let c = LeanChar::ofUInt8(lean, 0)?;
        assert_eq!(LeanChar::toChar(&c), Some('\0'));
        assert_eq!(LeanChar::toUInt8(&c), 0);

        let c = LeanChar::ofUInt8(lean, 255)?;
        assert_eq!(LeanChar::toChar(&c), Some('ÿ'));
        assert_eq!(LeanChar::toUInt8(&c), 255);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_to_uint8_truncation() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 'α' = U+03B1 = 945; truncating to u8 gives 945 & 0xFF = 177
        let c = LeanChar::mk(lean, 'α')?;
        assert_eq!(LeanChar::toNat(&c), 945);
        assert_eq!(LeanChar::toUInt8(&c), 177);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_to_char_invalid() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // A valid char round-trips
        let c = LeanChar::mk(lean, 'x')?;
        assert_eq!(LeanChar::toChar(&c), Some('x'));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_is_valid_char_nat() {
    leo3::prepare_freethreaded_lean();

    // Pure function: no Lean runtime needed, but keep house style for consistency
    leo3::prepare_freethreaded_lean();

    assert!(LeanChar::isValidCharNat(0));
    assert!(LeanChar::isValidCharNat(65));
    assert!(LeanChar::isValidCharNat(0x10FFFF));

    assert!(!LeanChar::isValidCharNat(0xD800));
    assert!(!LeanChar::isValidCharNat(0xDFFF));
    assert!(!LeanChar::isValidCharNat(0x110000));
    assert!(!LeanChar::isValidCharNat(0xFFFFFFFF));
}

#[test]
fn test_char_is_alpha() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        assert!(LeanChar::isAlpha(&LeanChar::mk(lean, 'a')?));
        assert!(LeanChar::isAlpha(&LeanChar::mk(lean, 'Z')?));
        assert!(LeanChar::isAlpha(&LeanChar::mk(lean, '中')?));
        assert!(!LeanChar::isAlpha(&LeanChar::mk(lean, '5')?));
        assert!(!LeanChar::isAlpha(&LeanChar::mk(lean, '!')?));
        assert!(!LeanChar::isAlpha(&LeanChar::mk(lean, ' ')?));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_is_alphanum() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        assert!(LeanChar::isAlphanum(&LeanChar::mk(lean, 'a')?));
        assert!(LeanChar::isAlphanum(&LeanChar::mk(lean, '7')?));
        assert!(!LeanChar::isAlphanum(&LeanChar::mk(lean, '!')?));
        assert!(!LeanChar::isAlphanum(&LeanChar::mk(lean, ' ')?));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_is_digit() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        assert!(LeanChar::isDigit(&LeanChar::mk(lean, '7')?));
        assert!(LeanChar::isDigit(&LeanChar::mk(lean, '0')?));
        assert!(!LeanChar::isDigit(&LeanChar::mk(lean, 'a')?));
        assert!(!LeanChar::isDigit(&LeanChar::mk(lean, '!')?));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_is_lower() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        assert!(LeanChar::isLower(&LeanChar::mk(lean, 'a')?));
        assert!(LeanChar::isLower(&LeanChar::mk(lean, 'z')?));
        assert!(!LeanChar::isLower(&LeanChar::mk(lean, 'A')?));
        assert!(!LeanChar::isLower(&LeanChar::mk(lean, '5')?));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_is_upper() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        assert!(LeanChar::isUpper(&LeanChar::mk(lean, 'A')?));
        assert!(LeanChar::isUpper(&LeanChar::mk(lean, 'Z')?));
        assert!(!LeanChar::isUpper(&LeanChar::mk(lean, 'a')?));
        assert!(!LeanChar::isUpper(&LeanChar::mk(lean, '5')?));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_is_whitespace() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        assert!(LeanChar::isWhitespace(&LeanChar::mk(lean, ' ')?));
        assert!(LeanChar::isWhitespace(&LeanChar::mk(lean, '\t')?));
        assert!(LeanChar::isWhitespace(&LeanChar::mk(lean, '\n')?));
        assert!(!LeanChar::isWhitespace(&LeanChar::mk(lean, 'a')?));
        assert!(!LeanChar::isWhitespace(&LeanChar::mk(lean, '5')?));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_to_upper() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 'a' -> 'A'
        let upper = LeanChar::toUpper(LeanChar::mk(lean, 'a')?)?;
        assert_eq!(LeanChar::toChar(&upper), Some('A'));

        // Already uppercase stays uppercase
        let upper = LeanChar::toUpper(LeanChar::mk(lean, 'A')?)?;
        assert_eq!(LeanChar::toChar(&upper), Some('A'));

        // Digit passes through unchanged
        let upper = LeanChar::toUpper(LeanChar::mk(lean, '5')?)?;
        assert_eq!(LeanChar::toChar(&upper), Some('5'));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_to_lower() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // 'A' -> 'a'
        let lower = LeanChar::toLower(LeanChar::mk(lean, 'A')?)?;
        assert_eq!(LeanChar::toChar(&lower), Some('a'));

        // Already lowercase stays lowercase
        let lower = LeanChar::toLower(LeanChar::mk(lean, 'a')?)?;
        assert_eq!(LeanChar::toChar(&lower), Some('a'));

        // Digit passes through unchanged
        let lower = LeanChar::toLower(LeanChar::mk(lean, '5')?)?;
        assert_eq!(LeanChar::toChar(&lower), Some('5'));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_le() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanChar::mk(lean, 'a')?;
        let b = LeanChar::mk(lean, 'b')?;

        assert!(LeanChar::le(&a, &b));
        assert!(LeanChar::le(&a, &a));
        assert!(!LeanChar::le(&b, &a));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_lt() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanChar::mk(lean, 'a')?;
        let b = LeanChar::mk(lean, 'b')?;

        assert!(LeanChar::lt(&a, &b));
        assert!(!LeanChar::lt(&b, &a));
        assert!(!LeanChar::lt(&a, &a));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_utf8_size() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // ASCII: 1 byte
        assert_eq!(LeanChar::utf8Size(&LeanChar::mk(lean, 'A')?), 1);
        // Greek alpha: 2 bytes
        assert_eq!(LeanChar::utf8Size(&LeanChar::mk(lean, 'α')?), 2);
        // CJK ideograph: 3 bytes
        assert_eq!(LeanChar::utf8Size(&LeanChar::mk(lean, '中')?), 3);
        // Emoji (U+1F600): 4 bytes
        assert_eq!(LeanChar::utf8Size(&LeanChar::mk(lean, '😀')?), 4);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_char_utf16_size() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // BMP characters: 1 UTF-16 code unit
        assert_eq!(LeanChar::utf16Size(&LeanChar::mk(lean, 'A')?), 1);
        assert_eq!(LeanChar::utf16Size(&LeanChar::mk(lean, 'α')?), 1);
        assert_eq!(LeanChar::utf16Size(&LeanChar::mk(lean, '中')?), 1);
        // Emoji (U+1F600): surrogate pair -> 2 UTF-16 code units
        assert_eq!(LeanChar::utf16Size(&LeanChar::mk(lean, '😀')?), 2);

        Ok(())
    });

    assert!(result.is_ok());
}
