//! Conversion tests for Rust ↔ Lean type conversions
//!
//! These tests verify that data can be correctly converted between
//! Rust and Lean types.

#![cfg(feature = "runtime-tests")]

use leo3::instance::LeanAny;
use leo3::prelude::*;
use leo3::types::LeanExcept;

// =============================================================================
// Natural Number Conversions
// =============================================================================

#[test]
fn test_nat_from_usize() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Small numbers
        let n = LeanNat::from_usize(lean, 0)?;
        assert_eq!(LeanNat::to_usize(&n)?, 0);

        let n = LeanNat::from_usize(lean, 1)?;
        assert_eq!(LeanNat::to_usize(&n)?, 1);

        let n = LeanNat::from_usize(lean, 42)?;
        assert_eq!(LeanNat::to_usize(&n)?, 42);

        let n = LeanNat::from_usize(lean, 255)?;
        assert_eq!(LeanNat::to_usize(&n)?, 255);

        // Large numbers
        let n = LeanNat::from_usize(lean, 1_000_000)?;
        assert_eq!(LeanNat::to_usize(&n)?, 1_000_000);

        let n = LeanNat::from_usize(lean, usize::MAX / 2)?;
        assert_eq!(LeanNat::to_usize(&n)?, usize::MAX / 2);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_except_to_rust_result_rejects_scalar_values() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let invalid: LeanBound<LeanExcept> =
            unsafe { LeanBound::from_borrowed_ptr(lean, leo3::ffi::inline::lean_box(0)) };

        let err = match LeanExcept::toRustResult(&invalid) {
            Ok(_) => panic!("expected malformed Except conversion to fail"),
            Err(err) => err,
        };
        assert!(matches!(err, LeanError::Conversion(_)));
        assert!(err
            .to_string()
            .contains("Except value must be a constructor"));

        Ok(())
    });

    assert!(result.is_ok(), "test failed: {:?}", result.err());
}

#[test]
fn test_nat_to_usize() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Create various nats and convert back
        for value in [0, 1, 10, 100, 1000, 10000, 100000] {
            let n = LeanNat::from_usize(lean, value)?;
            assert_eq!(LeanNat::to_usize(&n)?, value);
        }

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_nat_is_small() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Small numbers should be "small"
        let n = LeanNat::from_usize(lean, 42)?;
        assert!(LeanNat::is_small(&n));

        let n = LeanNat::from_usize(lean, 1000)?;
        assert!(LeanNat::is_small(&n));

        // Note: Whether large numbers are "small" depends on Lean's implementation
        // We just verify the function works
        let n = LeanNat::from_usize(lean, 1_000_000)?;
        let _ = LeanNat::is_small(&n); // Just check it doesn't crash

        Ok(())
    });

    assert!(result.is_ok());
}

// =============================================================================
// String Conversions
// =============================================================================

#[test]
fn test_string_from_str() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Empty string
        let s = LeanString::mk(lean, "")?;
        assert_eq!(LeanString::cstr(&s)?, "");

        // Simple ASCII
        let s = LeanString::mk(lean, "Hello")?;
        assert_eq!(LeanString::cstr(&s)?, "Hello");

        // With punctuation
        let s = LeanString::mk(lean, "Hello, World!")?;
        assert_eq!(LeanString::cstr(&s)?, "Hello, World!");

        // With newlines
        let s = LeanString::mk(lean, "Line 1\nLine 2")?;
        assert_eq!(LeanString::cstr(&s)?, "Line 1\nLine 2");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_string_to_str() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let test_strings = [
            "",
            "a",
            "Hello",
            "Hello, World!",
            "1234567890",
            "Special: !@#$%^&*()",
        ];

        for &s in &test_strings {
            let lean_str = LeanString::mk(lean, s)?;
            assert_eq!(LeanString::cstr(&lean_str)?, s);
        }

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_string_unicode() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Various Unicode strings
        let s = LeanString::mk(lean, "こんにちは")?; // Japanese
        assert_eq!(LeanString::cstr(&s)?, "こんにちは");

        let s = LeanString::mk(lean, "你好")?; // Chinese
        assert_eq!(LeanString::cstr(&s)?, "你好");

        let s = LeanString::mk(lean, "Здравствуй")?; // Russian
        assert_eq!(LeanString::cstr(&s)?, "Здравствуй");

        let s = LeanString::mk(lean, "🦀🔥")?; // Emoji
        assert_eq!(LeanString::cstr(&s)?, "🦀🔥");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_string_lengths() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // ASCII - byte length == char length
        let s = LeanString::mk(lean, "Hello")?;
        assert_eq!(LeanString::length(&s), 5);
        assert_eq!(LeanString::utf8ByteSize(&s), 6);

        // Empty
        let s = LeanString::mk(lean, "")?;
        assert_eq!(LeanString::length(&s), 0);
        assert_eq!(LeanString::utf8ByteSize(&s), 1);

        // Note: For multibyte characters, byte_size > len
        // The exact values depend on Lean's UTF-8 handling

        Ok(())
    });

    assert!(result.is_ok());
}

// =============================================================================
// Array Conversions
// =============================================================================

#[test]
fn test_array_empty() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = LeanArray::empty(lean)?;

        assert!(LeanArray::isEmpty(&arr));
        assert_eq!(LeanArray::size(&arr), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_with_nats() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let mut arr = LeanArray::empty(lean)?;

        // Add natural numbers
        for i in 0..10 {
            let n = LeanNat::from_usize(lean, i)?;
            arr = LeanArray::push(arr, n.cast())?;
        }

        assert_eq!(LeanArray::size(&arr), 10);

        // Get elements back (as LeanAny, can't directly convert to nat without unsafe)
        for i in 0..10 {
            let elem = LeanArray::get(&arr, i);
            assert!(elem.is_some(), "Element {} should exist", i);
        }

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_with_strings() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let mut arr = LeanArray::empty(lean)?;

        let strings = ["first", "second", "third", "fourth", "fifth"];

        for s in &strings {
            let lean_str = LeanString::mk(lean, s)?;
            arr = LeanArray::push(arr, lean_str.cast())?;
        }

        assert_eq!(LeanArray::size(&arr), strings.len());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_mixed_types() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let mut arr = LeanArray::empty(lean)?;

        // Add different types (all cast to LeanAny)
        let n = LeanNat::from_usize(lean, 42)?;
        arr = LeanArray::push(arr, n.cast())?;

        let s = LeanString::mk(lean, "Hello")?;
        arr = LeanArray::push(arr, s.cast())?;

        let inner_arr = LeanArray::empty(lean)?;
        arr = LeanArray::push(arr, inner_arr.cast())?;

        assert_eq!(LeanArray::size(&arr), 3);

        Ok(())
    });

    assert!(result.is_ok());
}

// =============================================================================
// Type Casting Tests
// =============================================================================

#[test]
fn test_cast_nat_to_any() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let n: LeanBound<LeanNat> = LeanNat::from_usize(lean, 42)?;
        let any: LeanBound<LeanAny> = n.cast();

        // The cast should preserve the object
        // (We can't easily convert back without unsafe, but it shouldn't crash)
        drop(any);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_cast_string_to_any() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s: LeanBound<LeanString> = LeanString::mk(lean, "test")?;
        let any: LeanBound<LeanAny> = s.cast();

        drop(any);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_cast_array_to_any() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr: LeanBound<LeanArray> = LeanArray::empty(lean)?;
        let any: LeanBound<LeanAny> = arr.cast();

        drop(any);

        Ok(())
    });

    assert!(result.is_ok());
}

// =============================================================================
// Unbind/Bind Conversion Tests
// =============================================================================

#[test]
fn test_nat_unbind_bind() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let n = LeanNat::from_usize(lean, 100)?;
        let n_ref = n.unbind();
        let n2 = n_ref.bind(lean);

        assert_eq!(LeanNat::to_usize(&n2)?, 100);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_string_unbind_bind() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "Test")?;
        let s_ref = s.unbind();
        let s2 = s_ref.bind(lean);

        assert_eq!(LeanString::cstr(&s2)?, "Test");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_unbind_bind() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let mut arr = LeanArray::empty(lean)?;
        let n = LeanNat::from_usize(lean, 1)?;
        arr = LeanArray::push(arr, n.cast())?;

        let arr_ref = arr.unbind();
        let arr2 = arr_ref.bind(lean);

        assert_eq!(LeanArray::size(&arr2), 1);

        Ok(())
    });

    assert!(result.is_ok());
}

// =============================================================================
// Edge Cases and Error Handling
// =============================================================================

#[test]
fn test_string_with_null_byte_roundtrips() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Lean strings are byte arrays: embedded null bytes are preserved
        // through mk -> cstr and through the IntoLean/FromLean round-trip.
        let s = LeanString::mk(lean, "Hello\0World")?;
        assert_eq!(LeanString::cstr(&s)?, "Hello\0World");

        let roundtrip: String = String::from_lean(&s)?;
        assert_eq!(roundtrip, "Hello\0World");
        assert_eq!(roundtrip.len(), 11);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_out_of_bounds_get() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = LeanArray::empty(lean)?;

        // Getting from empty array should return None
        assert!(LeanArray::get(&arr, 0).is_none());
        assert!(LeanArray::get(&arr, 100).is_none());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_out_of_bounds_set_panics() {
    leo3::prepare_freethreaded_lean();

    let result = leo3::with_lean(|lean| {
        let arr = LeanArray::empty(lean)?;
        let n = LeanNat::from_usize(lean, 1)?;

        // Setting out of bounds should return an error
        let result = LeanArray::set(arr, 0, n.cast());
        assert!(result.is_err(), "Expected error for out of bounds set");

        Ok::<(), LeanError>(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_pop_empty_panics() {
    leo3::prepare_freethreaded_lean();

    let result = leo3::with_lean(|lean| {
        let arr = LeanArray::empty(lean)?;

        // Popping from empty array should return an error
        let result = LeanArray::pop(arr);
        assert!(result.is_err(), "Expected error for pop from empty array");

        Ok::<(), LeanError>(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_vec_u8_bulk_conversion() {
    use leo3::conversion::{vec_u8_from_lean, vec_u8_into_lean};

    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Test empty vec
        let empty_vec: Vec<u8> = vec![];
        let ba = vec_u8_into_lean(empty_vec, lean)?;
        assert_eq!(LeanByteArray::size(&ba), 0);
        let result_vec = vec_u8_from_lean(&ba);
        assert_eq!(result_vec, vec![]);

        // Test small vec
        let small_vec = vec![1, 2, 3, 4, 5];
        let ba = vec_u8_into_lean(small_vec.clone(), lean)?;
        assert_eq!(LeanByteArray::size(&ba), 5);
        let result_vec = vec_u8_from_lean(&ba);
        assert_eq!(result_vec, small_vec);

        // Test large vec
        let large_vec: Vec<u8> = (0..=255).cycle().take(10000).collect();
        let ba = vec_u8_into_lean(large_vec.clone(), lean)?;
        assert_eq!(LeanByteArray::size(&ba), 10000);
        let result_vec = vec_u8_from_lean(&ba);
        assert_eq!(result_vec, large_vec);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_slice_u8_bulk_conversion() {
    use leo3::conversion::{slice_u8_into_lean, vec_u8_from_lean};

    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Test byte string literal
        let bytes = b"Hello, World!";
        let ba = slice_u8_into_lean(bytes, lean)?;
        assert_eq!(LeanByteArray::size(&ba), 13);
        let result_vec = vec_u8_from_lean(&ba);
        assert_eq!(result_vec.as_slice(), bytes);

        // Test slice from vec
        let vec = [10, 20, 30, 40, 50];
        let ba = slice_u8_into_lean(&vec[1..4], lean)?;
        assert_eq!(LeanByteArray::size(&ba), 3);
        let result_vec = vec_u8_from_lean(&ba);
        assert_eq!(result_vec, vec![20, 30, 40]);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_builder_basic() {
    use leo3::conversion::ArrayBuilder;

    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let mut builder = ArrayBuilder::with_capacity(lean, 10)?;
        assert_eq!(builder.len(), 0);
        assert!(builder.is_empty());

        for i in 0..10 {
            let n = LeanNat::from_usize(lean, i)?;
            builder.push(n.cast())?;
        }

        assert_eq!(builder.len(), 10);
        assert!(!builder.is_empty());

        let arr = builder.finish();
        assert_eq!(LeanArray::size(&arr), 10);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_array_builder_empty() {
    use leo3::conversion::ArrayBuilder;

    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let builder = ArrayBuilder::with_capacity(lean, 5)?;
        let arr = builder.finish();
        assert_eq!(LeanArray::size(&arr), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_iter_into_lean() {
    use leo3::conversion::iter_into_lean;

    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Test with range iterator
        let arr = iter_into_lean(0..10_usize, lean)?;
        assert_eq!(LeanArray::size(&arr), 10);

        // Test with mapped iterator
        let arr = iter_into_lean((0..5_usize).map(|x| x * 2), lean)?;
        assert_eq!(LeanArray::size(&arr), 5);

        // Verify elements (spot check)
        for i in 0..5 {
            let elem = LeanArray::get(&arr, i).unwrap();
            let uint: LeanBound<LeanUSize> = elem.cast();
            let value = LeanUSize::to_usize(&uint);
            assert_eq!(value, i * 2);
        }

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_slice_into_lean() {
    use leo3::conversion::slice_into_lean;

    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let data = [1_u64, 2, 3, 4, 5];
        let arr = slice_into_lean(&data, lean)?;
        assert_eq!(LeanArray::size(&arr), 5);

        // Verify elements
        for (i, &expected) in data.iter().enumerate() {
            let elem = LeanArray::get(&arr, i).unwrap();
            let uint: LeanBound<LeanUInt64> = elem.cast();
            let value = LeanUInt64::to_u64(&uint);
            assert_eq!(value, expected);
        }

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_iter_into_lean_empty() {
    use leo3::conversion::iter_into_lean;

    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let arr = iter_into_lean(0..0_usize, lean)?;
        assert_eq!(LeanArray::size(&arr), 0);
        assert!(LeanArray::isEmpty(&arr));

        Ok(())
    });

    assert!(result.is_ok());
}
