//! Integration tests for the `#[leanfn]` macro.

#![cfg(feature = "macros")]
#![allow(clippy::ptr_arg)]

use leo3::prelude::*;
use leo3::types::LeanExcept;
use std::sync::LazyLock;

static STATIC_NAME: LazyLock<String> = LazyLock::new(|| "static-name".to_string());
static STATIC_BYTES: LazyLock<Vec<u8>> = LazyLock::new(|| vec![2, 4, 6, 8]);
static STATIC_WORDS: LazyLock<Vec<u64>> = LazyLock::new(|| vec![5, 8, 13]);

// Test basic u64 function
#[leanfn]
fn double(x: u64) -> u64 {
    x * 2
}

// Test root-path macro usage without importing the attribute directly.
#[leo3::leanfn]
fn triple(x: u64) -> u64 {
    x * 3
}

// Test function with custom name
#[leanfn(name = "my_add")]
fn add(a: u64, b: u64) -> u64 {
    a + b
}

// Test function with string parameter
#[leanfn]
fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}

// Test function with bool
#[leanfn]
fn negate(b: bool) -> bool {
    !b
}

// Test function with mixed types
#[leanfn]
fn describe_number(n: u64, positive: bool) -> String {
    if positive {
        format!("positive {}", n)
    } else {
        format!("negative {}", n)
    }
}

// Test function with unit return
#[leanfn]
fn do_nothing(x: u64) {
    let _ = x;
}

// Test function with Vec<u64> parameter
#[leanfn]
fn sum_vec(numbers: Vec<u64>) -> u64 {
    numbers.iter().sum()
}

// Test function returning Vec<u64>
#[leanfn]
fn range_vec(n: u64) -> Vec<u64> {
    (0..n).collect()
}

// Test function with Vec<String>
#[leanfn]
fn join_strings(strings: Vec<String>) -> String {
    strings.join(", ")
}

// Test function that processes a Vec and returns a Vec
#[leanfn]
fn double_all(numbers: Vec<u64>) -> Vec<u64> {
    numbers.into_iter().map(|n| n * 2).collect()
}

// Test function with large array (for performance testing)
#[leanfn]
fn sum_large_vec(numbers: Vec<u64>) -> u64 {
    numbers.iter().sum()
}

// Test function with fixed-size array parameter
#[leanfn]
fn sum_array(numbers: [u64; 4]) -> u64 {
    numbers.into_iter().sum()
}

// Test function with borrowed non-byte slice parameter
#[leanfn]
fn sum_borrowed_slice(numbers: &[u64]) -> u64 {
    numbers.iter().sum()
}

#[leanfn]
fn sum_borrowed_array(numbers: &[u64; 3]) -> u64 {
    numbers.iter().sum()
}

#[leanfn]
fn borrowed_string_len(value: &String) -> u64 {
    value.len() as u64
}

#[leanfn]
fn borrowed_vec_u8_sum(values: &Vec<u8>) -> u64 {
    values.iter().map(|value| *value as u64).sum()
}

#[leanfn]
fn borrowed_vec_u64_sum(values: &Vec<u64>) -> u64 {
    values.iter().sum()
}

#[leanfn]
fn option_borrowed_alias_score(
    name: Option<&String>,
    bytes: Option<&Vec<u8>>,
    words: Option<&Vec<u64>>,
) -> u64 {
    name.map(|value| value.len() as u64).unwrap_or(0)
        + bytes
            .map(|values| values.iter().map(|value| *value as u64).sum::<u64>())
            .unwrap_or(0)
        + words.map(|values| values.iter().sum::<u64>()).unwrap_or(0)
}

#[leanfn]
fn result_borrowed_alias_score(
    name: Result<&String, &String>,
    bytes: Result<&Vec<u8>, &String>,
    words: Result<&Vec<u64>, &String>,
) -> u64 {
    let name_score = match name {
        Ok(value) => value.len() as u64,
        Err(err) => err.len() as u64,
    };
    let bytes_score = match bytes {
        Ok(values) => values.iter().map(|value| *value as u64).sum(),
        Err(err) => err.len() as u64,
    };
    let words_score = match words {
        Ok(values) => values.iter().sum(),
        Err(err) => err.len() as u64,
    };
    name_score + bytes_score + words_score
}

#[leanfn]
fn tuple_borrowed_alias_score(value: (&String, &Vec<u8>, &Vec<u64>)) -> u64 {
    value.0.len() as u64
        + value.1.iter().map(|byte| *byte as u64).sum::<u64>()
        + value.2.iter().sum::<u64>()
}

#[leanfn]
fn option_result_borrowed_alias_score(value: Option<Result<&Vec<u64>, &String>>) -> u64 {
    match value {
        Some(Ok(values)) => values.iter().sum(),
        Some(Err(err)) => err.len() as u64,
        None => 0,
    }
}

#[leanfn]
fn option_borrowed_slice_score(values: Option<&[u64]>) -> u64 {
    values.map(|values| values.iter().sum()).unwrap_or(0)
}

#[leanfn]
fn result_borrowed_slice_score(values: Result<&[u64], &[u64]>) -> u64 {
    match values {
        Ok(values) => values.iter().sum(),
        Err(values) => values.iter().sum::<u64>() * 10,
    }
}

#[leanfn]
fn tuple_borrowed_slice_score(value: (&[u64], &[u64; 3])) -> u64 {
    value.0.iter().sum::<u64>() + value.1.iter().sum::<u64>()
}

#[leanfn]
fn static_word_slice() -> &'static [u64] {
    &[1, 2, 3]
}

#[leanfn]
fn static_word_array() -> &'static [u64; 3] {
    &[4, 5, 6]
}

#[leanfn]
fn static_string_ref() -> &'static String {
    &STATIC_NAME
}

#[leanfn]
fn static_vec_u8_ref() -> &'static Vec<u8> {
    &STATIC_BYTES
}

#[leanfn]
fn static_vec_u64_ref() -> &'static Vec<u64> {
    &STATIC_WORDS
}

// Test function returning fixed-size arrays
#[leanfn]
fn flip_bool_array(values: [bool; 2]) -> [bool; 2] {
    [!values[0], !values[1]]
}

// Test function with a conversion path that can fail gracefully.
#[leanfn]
fn unwrap_result_or_zero(value: Result<u64, String>) -> u64 {
    value.unwrap_or(0)
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_double() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        // Create Lean UInt64
        let input = LeanUInt64::mk(lean, 21)?;

        // Call via FFI (simulating Lean calling us)
        // Access FFI function via internal name
        unsafe {
            let result_ptr = __leo3_leanfn_double::__ffi_double(input.into_ptr());
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 42);
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_with_name() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        let a = LeanUInt64::mk(lean, 10)?;
        let b = LeanUInt64::mk(lean, 32)?;

        unsafe {
            // Function is exported as "my_add" (different from rust name "add")
            // So it's available at top level
            let result_ptr = my_add(a.into_ptr(), b.into_ptr());
            let result = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 42);
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_string() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        let name = LeanString::mk(lean, "World")?;

        unsafe {
            let result_ptr = __leo3_leanfn_greet::__ffi_greet(name.into_ptr());
            let result: LeanBound<LeanString> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanString::cstr(&result)?, "Hello, World!");
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_bool() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        let input_true = LeanBool::mk(lean, true)?;
        let input_false = LeanBool::mk(lean, false)?;

        unsafe {
            let result_ptr = __leo3_leanfn_negate::__ffi_negate(input_true.into_ptr());
            let result: LeanBound<LeanBool> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert!(!LeanBool::toBool(&result));

            let result_ptr = __leo3_leanfn_negate::__ffi_negate(input_false.into_ptr());
            let result: LeanBound<LeanBool> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert!(LeanBool::toBool(&result));
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_mixed_types() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        let n = LeanUInt64::mk(lean, 42)?;
        let positive = LeanBool::mk(lean, true)?;

        unsafe {
            let result_ptr = __leo3_leanfn_describe_number::__ffi_describe_number(
                n.into_ptr(),
                positive.into_ptr(),
            );
            let result: LeanBound<LeanString> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanString::cstr(&result)?, "positive 42");
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_unit_return() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        let input = LeanUInt64::mk(lean, 42)?;

        unsafe {
            let result_ptr = __leo3_leanfn_do_nothing::__ffi_do_nothing(input.into_ptr());
            // Unit return should be a constructor with tag 0 and 0 fields
            // Using LeanUInt64 as a placeholder type since we just check the pointer
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            // Just verify it doesn't crash
            assert!(!result.as_ptr().is_null());
        }

        Ok(())
    })
    .unwrap();
}

// Test that we can call the function from Rust-side too
#[test]
fn test_rust_side_call() {
    // The original function should still be callable from Rust
    assert_eq!(double(21), 42);
    assert_eq!(add(10, 32), 42);
    assert_eq!(greet("Rust".to_string()), "Hello, Rust!");
    assert!(!negate(true));
    assert_eq!(describe_number(42, true), "positive 42");

    // Test Vec functions
    assert_eq!(sum_vec(vec![1, 2, 3, 4]), 10);
    assert_eq!(range_vec(5), vec![0, 1, 2, 3, 4]);
    assert_eq!(join_strings(vec!["a".to_string(), "b".to_string()]), "a, b");
    assert_eq!(double_all(vec![1, 2, 3]), vec![2, 4, 6]);
    assert_eq!(sum_large_vec((0..1000).collect()), 499500);
}

fn lean_u64_array<'l>(
    lean: Lean<'l>,
    values: &[u64],
) -> Result<LeanBound<'l, LeanArray>, Box<dyn std::error::Error>> {
    let mut arr = LeanArray::empty(lean)?;
    for value in values {
        let n = LeanUInt64::mk(lean, *value)?;
        arr = LeanArray::push(arr, n.cast())?;
    }
    Ok(arr)
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_vec_sum() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        use leo3::types::LeanArray;

        // Create a LeanArray with [10, 20, 30]
        let mut arr = LeanArray::empty(lean)?;
        let n1 = LeanUInt64::mk(lean, 10)?;
        let n2 = LeanUInt64::mk(lean, 20)?;
        let n3 = LeanUInt64::mk(lean, 30)?;

        arr = LeanArray::push(arr, n1.cast())?;
        arr = LeanArray::push(arr, n2.cast())?;
        arr = LeanArray::push(arr, n3.cast())?;

        unsafe {
            let result_ptr = __leo3_leanfn_sum_vec::__ffi_sum_vec(arr.into_ptr());
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 60);
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_vec_return() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        use leo3::types::LeanArray;

        let n = LeanUInt64::mk(lean, 5)?;

        unsafe {
            let result_ptr = __leo3_leanfn_range_vec::__ffi_range_vec(n.into_ptr());
            let result: LeanBound<LeanArray> = LeanBound::from_owned_ptr(lean, result_ptr);

            // Verify array has 5 elements: [0, 1, 2, 3, 4]
            assert_eq!(LeanArray::size(&result), 5);

            for i in 0..5 {
                let elem = LeanArray::get(&result, i).unwrap();
                let uint: LeanBound<LeanUInt64> = elem.cast();
                assert_eq!(LeanUInt64::to_u64(&uint), i as u64);
            }
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_vec_strings() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        use leo3::types::LeanArray;

        // Create array of strings ["hello", "world"]
        let mut arr = LeanArray::empty(lean)?;
        let s1 = LeanString::mk(lean, "hello")?;
        let s2 = LeanString::mk(lean, "world")?;

        arr = LeanArray::push(arr, s1.cast())?;
        arr = LeanArray::push(arr, s2.cast())?;

        unsafe {
            let result_ptr = __leo3_leanfn_join_strings::__ffi_join_strings(arr.into_ptr());
            let result: LeanBound<LeanString> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanString::cstr(&result)?, "hello, world");
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_vec_transform() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        use leo3::types::LeanArray;

        // Create array [1, 2, 3]
        let mut arr = LeanArray::empty(lean)?;
        for i in 1..=3 {
            let n = LeanUInt64::mk(lean, i)?;
            arr = LeanArray::push(arr, n.cast())?;
        }

        unsafe {
            let result_ptr = __leo3_leanfn_double_all::__ffi_double_all(arr.into_ptr());
            let result: LeanBound<LeanArray> = LeanBound::from_owned_ptr(lean, result_ptr);

            // Should return [2, 4, 6]
            assert_eq!(LeanArray::size(&result), 3);

            let expected = [2, 4, 6];
            for (i, &expected_val) in expected.iter().enumerate() {
                let elem = LeanArray::get(&result, i).unwrap();
                let uint: LeanBound<LeanUInt64> = elem.cast();
                assert_eq!(LeanUInt64::to_u64(&uint), expected_val);
            }
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_large_vec() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        use leo3::types::LeanArray;

        // Create a large array with 1000 elements
        let size = 1000;
        let mut arr = LeanArray::empty(lean)?;

        for i in 0..size {
            let n = LeanUInt64::mk(lean, i)?;
            arr = LeanArray::push(arr, n.cast())?;
        }

        unsafe {
            let result_ptr = __leo3_leanfn_sum_large_vec::__ffi_sum_large_vec(arr.into_ptr());
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);

            // Sum of 0..1000 is 999*1000/2 = 499500
            let expected: u64 = (0..size).sum();
            assert_eq!(LeanUInt64::to_u64(&result), expected);
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_fixed_array_sum() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        use leo3::types::LeanArray;

        let mut arr = LeanArray::empty(lean)?;
        for value in [10_u64, 20, 30, 40] {
            let n = LeanUInt64::mk(lean, value)?;
            arr = LeanArray::push(arr, n.cast())?;
        }

        unsafe {
            let result_ptr = __leo3_leanfn_sum_array::__ffi_sum_array(arr.into_ptr());
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 100);
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_borrowed_slice_sum() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        use leo3::types::LeanArray;

        let mut arr = LeanArray::empty(lean)?;
        for value in [10_u64, 20, 30] {
            let n = LeanUInt64::mk(lean, value)?;
            arr = LeanArray::push(arr, n.cast())?;
        }

        unsafe {
            let result_ptr =
                __leo3_leanfn_sum_borrowed_slice::__ffi_sum_borrowed_slice(arr.into_ptr());
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 60);
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_borrowed_array_sum() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        use leo3::types::LeanArray;

        let mut arr = LeanArray::empty(lean)?;
        for value in [7_u64, 8, 9] {
            let n = LeanUInt64::mk(lean, value)?;
            arr = LeanArray::push(arr, n.cast())?;
        }

        unsafe {
            let result_ptr =
                __leo3_leanfn_sum_borrowed_array::__ffi_sum_borrowed_array(arr.into_ptr());
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 24);
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_borrowed_owned_alias_params() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        let name = LeanString::mk(lean, "borrowed")?;
        let bytes = LeanByteArray::from_bytes(lean, &[1, 2, 3, 4])?;
        let words = lean_u64_array(lean, &[10, 20, 30])?;

        unsafe {
            let result_ptr =
                __leo3_leanfn_borrowed_string_len::__ffi_borrowed_string_len(name.into_ptr());
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 8);

            let result_ptr =
                __leo3_leanfn_borrowed_vec_u8_sum::__ffi_borrowed_vec_u8_sum(bytes.into_ptr());
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 10);

            let result_ptr =
                __leo3_leanfn_borrowed_vec_u64_sum::__ffi_borrowed_vec_u64_sum(words.into_ptr());
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 60);
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_borrowed_owned_alias_nested_params() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        let name = LeanString::mk(lean, "nested")?;
        let err = LeanString::mk(lean, "err")?;
        let bytes = LeanByteArray::from_bytes(lean, &[2, 3, 5])?;
        let words = lean_u64_array(lean, &[7, 11, 13])?;

        let opt_name = LeanOption::some(name.clone().cast())?;
        let opt_bytes = LeanOption::some(bytes.clone().cast())?;
        let opt_words = LeanOption::some(words.clone().cast())?;

        let result_name = LeanExcept::ok(name.clone().cast())?;
        let result_bytes = LeanExcept::ok(bytes.clone().cast())?;
        let result_words = LeanExcept::ok(words.clone().cast())?;

        let pair_tail = LeanProd::mk(bytes.clone().cast(), words.clone().cast())?;
        let tuple = LeanProd::mk(name.clone().cast(), pair_tail.cast())?;

        let nested_ok = LeanExcept::ok(words.clone().cast())?;
        let nested_some = LeanOption::some(nested_ok.cast())?;
        let nested_err = LeanExcept::error(err.clone().cast())?;
        let nested_err_some = LeanOption::some(nested_err.cast())?;
        let nested_none = LeanOption::none(lean)?;
        let slice_values = lean_u64_array(lean, &[3, 4, 5])?;
        let slice_error_values = lean_u64_array(lean, &[1, 2])?;
        let opt_slice_values = LeanOption::some(slice_values.clone().cast())?;
        let result_slice_values = LeanExcept::ok(slice_values.clone().cast())?;
        let result_slice_error_values = LeanExcept::error(slice_error_values.cast())?;
        let fixed_array_values = lean_u64_array(lean, &[6, 7, 8])?;
        let tuple_slices = LeanProd::mk(slice_values.clone().cast(), fixed_array_values.cast())?;

        unsafe {
            let result_ptr =
                __leo3_leanfn_option_borrowed_alias_score::__ffi_option_borrowed_alias_score(
                    opt_name.into_ptr(),
                    opt_bytes.into_ptr(),
                    opt_words.into_ptr(),
                );
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 6 + 10 + 31);

            let result_ptr =
                __leo3_leanfn_result_borrowed_alias_score::__ffi_result_borrowed_alias_score(
                    result_name.into_ptr(),
                    result_bytes.into_ptr(),
                    result_words.into_ptr(),
                );
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 6 + 10 + 31);

            let result_ptr =
                __leo3_leanfn_tuple_borrowed_alias_score::__ffi_tuple_borrowed_alias_score(
                    tuple.into_ptr(),
                );
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 6 + 10 + 31);

            let result_ptr =
                __leo3_leanfn_option_result_borrowed_alias_score::__ffi_option_result_borrowed_alias_score(
                    nested_some.into_ptr(),
                );
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 31);

            let result_ptr =
                __leo3_leanfn_option_result_borrowed_alias_score::__ffi_option_result_borrowed_alias_score(
                    nested_err_some.into_ptr(),
                );
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 3);

            let result_ptr =
                __leo3_leanfn_option_result_borrowed_alias_score::__ffi_option_result_borrowed_alias_score(
                    nested_none.into_ptr(),
                );
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 0);

            let result_ptr =
                __leo3_leanfn_option_borrowed_slice_score::__ffi_option_borrowed_slice_score(
                    opt_slice_values.into_ptr(),
                );
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 12);

            let result_ptr =
                __leo3_leanfn_result_borrowed_slice_score::__ffi_result_borrowed_slice_score(
                    result_slice_values.into_ptr(),
                );
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 12);

            let result_ptr =
                __leo3_leanfn_result_borrowed_slice_score::__ffi_result_borrowed_slice_score(
                    result_slice_error_values.into_ptr(),
                );
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 30);

            let result_ptr =
                __leo3_leanfn_tuple_borrowed_slice_score::__ffi_tuple_borrowed_slice_score(
                    tuple_slices.into_ptr(),
                );
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 33);
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_borrowed_owned_alias_returns() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        unsafe {
            let result_ptr = __leo3_leanfn_static_string_ref::__ffi_static_string_ref();
            let result: LeanBound<LeanString> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanString::cstr(&result)?, "static-name");

            let result_ptr = __leo3_leanfn_static_vec_u8_ref::__ffi_static_vec_u8_ref();
            let result: LeanBound<LeanByteArray> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanByteArray::to_vec(&result), vec![2, 4, 6, 8]);

            let result_ptr = __leo3_leanfn_static_vec_u64_ref::__ffi_static_vec_u64_ref();
            let result: LeanBound<LeanArray> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanArray::size(&result), 3);
            for (index, expected) in [5_u64, 8, 13].into_iter().enumerate() {
                let item: LeanBound<LeanUInt64> = LeanArray::get(&result, index).unwrap().cast();
                assert_eq!(LeanUInt64::to_u64(&item), expected);
            }
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_fixed_array_return() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        use leo3::types::LeanArray;

        let mut arr = LeanArray::empty(lean)?;
        let a = LeanBool::mk(lean, true)?;
        let b = LeanBool::mk(lean, false)?;
        arr = LeanArray::push(arr, a.cast())?;
        arr = LeanArray::push(arr, b.cast())?;

        unsafe {
            let result_ptr = __leo3_leanfn_flip_bool_array::__ffi_flip_bool_array(arr.into_ptr());
            let result: LeanBound<LeanArray> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanArray::size(&result), 2);

            let first: LeanBound<LeanBool> = LeanArray::get(&result, 0).unwrap().cast();
            let second: LeanBound<LeanBool> = LeanArray::get(&result, 1).unwrap().cast();
            assert!(!LeanBool::toBool(&first));
            assert!(LeanBool::toBool(&second));
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_try_wrapper_reports_conversion_errors() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        let invalid = unsafe { leo3::ffi::inline::lean_box(0) };

        let err =
            unsafe { __leo3_leanfn_unwrap_result_or_zero::__ffi_try_wrapper(invalid).unwrap_err() };

        match err {
            LeanError::Conversion(message) => {
                assert!(message.contains("Except value must be a constructor"));
            }
            other => panic!("expected conversion error, got: {:?}", other),
        }

        let _ = lean;
        Ok(())
    })
    .unwrap();
}

// Generic function exposed to Lean through the monomorphization subset.
#[leanfn(
    concrete(u64, name = "mono_add_u64"),
    concrete(i64, name = "mono_add_i64")
)]
fn mono_add<T: std::ops::Add<Output = T>>(a: T, b: T) -> T {
    a + b
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_concrete_u64_instance() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        let a = LeanUInt64::mk(lean, 10)?;
        let b = LeanUInt64::mk(lean, 32)?;

        unsafe {
            let result_ptr = mono_add_u64(a.into_ptr(), b.into_ptr());
            let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanUInt64::to_u64(&result), 42);
        }

        Ok(())
    })
    .unwrap();
}

#[test]
#[cfg_attr(not(feature = "runtime-tests"), ignore = "Requires Lean4 runtime")]
fn test_leanfn_concrete_i64_instance() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| -> Result<(), Box<dyn std::error::Error>> {
        let a = LeanInt64::mk(lean, -10)?;
        let b = LeanInt64::mk(lean, 52)?;

        unsafe {
            let result_ptr = mono_add_i64(a.into_ptr(), b.into_ptr());
            let result: LeanBound<LeanInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
            assert_eq!(LeanInt64::to_i64(&result), 42);
        }

        Ok(())
    })
    .unwrap();
}
