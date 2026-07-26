//! Tests for new type conversions: signed integers, floats, Option, Result

#![allow(clippy::approx_constant)]
#![cfg(feature = "runtime-tests")]

use leo3::conversion::{FromLean, IntoLean};
use leo3::err::LeanResult;

#[test]
fn test_i8_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        // Test positive values
        let val: i8 = 42;
        let lean_val = val.into_lean(lean)?;
        let result: i8 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        // Test negative values
        let val: i8 = -42;
        let lean_val = val.into_lean(lean)?;
        let result: i8 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        // Test min/max values
        let val: i8 = i8::MIN;
        let lean_val = val.into_lean(lean)?;
        let result: i8 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        let val: i8 = i8::MAX;
        let lean_val = val.into_lean(lean)?;
        let result: i8 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_i16_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        let val: i16 = -1000;
        let lean_val = val.into_lean(lean)?;
        let result: i16 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        let val: i16 = i16::MIN;
        let lean_val = val.into_lean(lean)?;
        let result: i16 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        let val: i16 = i16::MAX;
        let lean_val = val.into_lean(lean)?;
        let result: i16 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_i32_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        let val: i32 = -100000;
        let lean_val = val.into_lean(lean)?;
        let result: i32 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        let val: i32 = i32::MIN;
        let lean_val = val.into_lean(lean)?;
        let result: i32 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        let val: i32 = i32::MAX;
        let lean_val = val.into_lean(lean)?;
        let result: i32 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_i64_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        let val: i64 = -1000000000;
        let lean_val = val.into_lean(lean)?;
        let result: i64 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        // Test with small values that fit in scalar representation
        let val: i64 = -42;
        let lean_val = val.into_lean(lean)?;
        let result: i64 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_isize_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        let val: isize = -12345;
        let lean_val = val.into_lean(lean)?;
        let result: isize = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_char_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        let val: char = 'A';
        let lean_val = val.into_lean(lean)?;
        let result: char = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        let val: char = '中';
        let lean_val = val.into_lean(lean)?;
        let result: char = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        let val: char = '😀';
        let lean_val = val.into_lean(lean)?;
        let result: char = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_unit_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        let lean_val = ().into_lean(lean)?;
        let _: () = FromLean::from_lean(&lean_val)?;
        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_option_of_unit_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        let val: Option<()> = Some(());
        let lean_val = val.into_lean(lean)?;
        let result: Option<()> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, Some(()));

        let val: Option<()> = None;
        let lean_val = val.into_lean(lean)?;
        let result: Option<()> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, None);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_result_with_unit_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        let ok_val: Result<(), String> = Ok(());
        let lean_ok = ok_val.into_lean(lean)?;
        let ok_result: Result<(), String> = FromLean::from_lean(&lean_ok)?;
        assert_eq!(ok_result, Ok(()));

        let err_val: Result<(), String> = Err("boom".to_string());
        let lean_err = err_val.into_lean(lean)?;
        let err_result: Result<(), String> = FromLean::from_lean(&lean_err)?;
        assert_eq!(err_result, Err("boom".to_string()));

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_f64_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        // Test regular values
        let val: f64 = 3.141592653589793;
        let lean_val = val.into_lean(lean)?;
        let result: f64 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        // Test negative values
        let val: f64 = -2.718281828;
        let lean_val = val.into_lean(lean)?;
        let result: f64 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        // Test zero
        let val: f64 = 0.0;
        let lean_val = val.into_lean(lean)?;
        let result: f64 = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_option_some_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        // Test Some with u64
        let val: Option<u64> = Some(42);
        let lean_val = val.into_lean(lean)?;
        let result: Option<u64> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        // Test Some with i32
        let val: Option<i32> = Some(-100);
        let lean_val = val.into_lean(lean)?;
        let result: Option<i32> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        // Test Some with f64
        let val: Option<f64> = Some(42.0);
        let lean_val = val.into_lean(lean)?;
        let result: Option<f64> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_option_none_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        // Test None with u64
        let val: Option<u64> = None;
        let lean_val = val.into_lean(lean)?;
        let result: Option<u64> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        // Test None with String
        let val: Option<String> = None;
        let lean_val = val.into_lean(lean)?;
        let result: Option<String> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, None);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_nested_option_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        // Test nested Some(Some(value))
        let val: Option<Option<u64>> = Some(Some(42));
        let lean_val = val.into_lean(lean)?;
        let result: Option<Option<u64>> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        // Test nested Some(None)
        let val: Option<Option<u64>> = Some(None);
        let lean_val = val.into_lean(lean)?;
        let result: Option<Option<u64>> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        // Test nested None
        let val: Option<Option<u64>> = None;
        let lean_val = val.into_lean(lean)?;
        let result: Option<Option<u64>> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_result_ok_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        // Test Ok with u64
        let val: Result<u64, String> = Ok(42);
        let lean_val = val.into_lean(lean)?;
        let result: Result<u64, String> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, Ok(42));

        // Test Ok with i32
        let val: Result<i32, i32> = Ok(-100);
        let lean_val = val.into_lean(lean)?;
        let result: Result<i32, i32> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, Ok(-100));

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_result_err_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        // Test Err with String error
        let val: Result<u64, String> = Err("error message".to_string());
        let lean_val = val.into_lean(lean)?;
        let result: Result<u64, String> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, Err("error message".to_string()));

        // Test Err with i32 error
        let val: Result<String, i32> = Err(-1);
        let lean_val = val.into_lean(lean)?;
        let result: Result<String, i32> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, Err(-1));

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_nested_result_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        // Test nested Ok(Ok(value))
        let val: Result<Result<u64, String>, String> = Ok(Ok(42));
        let lean_val = val.into_lean(lean)?;
        let result: Result<Result<u64, String>, String> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, Ok(Ok(42)));

        // Test nested Ok(Err(error))
        let val: Result<Result<u64, String>, String> = Ok(Err("inner error".to_string()));
        let lean_val = val.into_lean(lean)?;
        let result: Result<Result<u64, String>, String> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, Ok(Err("inner error".to_string())));

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_option_of_result_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        // Test Some(Ok(value))
        let val: Option<Result<u64, String>> = Some(Ok(42));
        let lean_val = val.into_lean(lean)?;
        let result: Option<Result<u64, String>> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, Some(Ok(42)));

        // Test Some(Err(error))
        let val: Option<Result<u64, String>> = Some(Err("error".to_string()));
        let lean_val = val.into_lean(lean)?;
        let result: Option<Result<u64, String>> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, Some(Err("error".to_string())));

        // Test None
        let val: Option<Result<u64, String>> = None;
        let lean_val = val.into_lean(lean)?;
        let result: Option<Result<u64, String>> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, None);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_result_of_option_conversion() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        // Test Ok(Some(value))
        let val: Result<Option<u64>, String> = Ok(Some(42));
        let lean_val = val.into_lean(lean)?;
        let result: Result<Option<u64>, String> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, Ok(Some(42)));

        // Test Ok(None)
        let val: Result<Option<u64>, String> = Ok(None);
        let lean_val = val.into_lean(lean)?;
        let result: Result<Option<u64>, String> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, Ok(None));

        // Test Err
        let val: Result<Option<u64>, String> = Err("error".to_string());
        let lean_val = val.into_lean(lean)?;
        let result: Result<Option<u64>, String> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, Err("error".to_string()));

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_vec_of_signed_ints() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        let val: Vec<i32> = vec![-5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5];
        let lean_val = val.clone().into_lean(lean)?;
        let result: Vec<i32> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_vec_of_floats() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        let val: Vec<f64> = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let lean_val = val.clone().into_lean(lean)?;
        let result: Vec<f64> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        Ok(())
    })
    .expect("test failed");
}

#[test]
fn test_vec_of_options() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|lean| -> LeanResult<()> {
        let val: Vec<Option<u64>> = vec![Some(1), None, Some(3), None, Some(5)];
        let lean_val = val.clone().into_lean(lean)?;
        let result: Vec<Option<u64>> = FromLean::from_lean(&lean_val)?;
        assert_eq!(result, val);

        Ok(())
    })
    .expect("test failed");
}
