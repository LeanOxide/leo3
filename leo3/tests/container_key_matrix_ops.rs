//! Runtime coverage for the experimental container key matrix.

#![cfg(all(
    feature = "experimental-containers",
    feature = "runtime-tests",
    lean_4_22
))]

use leo3::prelude::*;

macro_rules! exercise_key_family {
    ($lean:ident, $key_ty:ty, $make:expr, $one:expr, $two:expr, $missing:expr) => {{
        let mut hash_map = leo3::types::LeanHashMap::<$key_ty, LeanString>::empty($lean)?;
        assert!(hash_map.is_empty()?);
        assert_eq!(hash_map.size()?, 0);

        hash_map = hash_map.insert($lean, ($make)($lean, $one)?, LeanString::mk($lean, "one")?)?;
        hash_map = hash_map.insert($lean, ($make)($lean, $two)?, LeanString::mk($lean, "two")?)?;
        hash_map = hash_map.insert($lean, ($make)($lean, $one)?, LeanString::mk($lean, "uno")?)?;

        assert_eq!(hash_map.size()?, 2);
        assert!(hash_map.contains($lean, &($make)($lean, $one)?)?);
        assert!(!hash_map.contains($lean, &($make)($lean, $missing)?)?);
        let found = hash_map
            .find($lean, &($make)($lean, $one)?)?
            .expect("replaced key should exist");
        assert_eq!(LeanString::cstr(&found)?, "uno");
        hash_map = hash_map.erase($lean, &($make)($lean, $two)?)?;
        assert_eq!(hash_map.size()?, 1);
        assert!(hash_map.find($lean, &($make)($lean, $two)?)?.is_none());

        let mut hash_set = leo3::types::LeanHashSet::<$key_ty>::empty($lean)?;
        assert!(hash_set.is_empty()?);
        hash_set = hash_set.insert($lean, ($make)($lean, $one)?)?;
        hash_set = hash_set.insert($lean, ($make)($lean, $two)?)?;
        hash_set = hash_set.insert($lean, ($make)($lean, $one)?)?;

        assert_eq!(hash_set.size()?, 2);
        assert!(hash_set.contains($lean, &($make)($lean, $one)?)?);
        assert!(!hash_set.contains($lean, &($make)($lean, $missing)?)?);
        assert!(hash_set.get($lean, &($make)($lean, $one)?)?.is_some());
        hash_set = hash_set.erase($lean, &($make)($lean, $two)?)?;
        assert_eq!(hash_set.size()?, 1);
        assert!(hash_set.get($lean, &($make)($lean, $two)?)?.is_none());

        let mut rb_map = leo3::types::LeanRBMap::<$key_ty, LeanString>::empty($lean)?;
        assert!(rb_map.is_empty());
        rb_map = rb_map.insert($lean, ($make)($lean, $one)?, LeanString::mk($lean, "one")?)?;
        rb_map = rb_map.insert($lean, ($make)($lean, $two)?, LeanString::mk($lean, "two")?)?;
        rb_map = rb_map.insert($lean, ($make)($lean, $one)?, LeanString::mk($lean, "uno")?)?;

        assert_eq!(rb_map.size()?, 2);
        assert!(rb_map.contains($lean, &($make)($lean, $one)?)?);
        assert!(!rb_map.contains($lean, &($make)($lean, $missing)?)?);
        let found = rb_map
            .find($lean, &($make)($lean, $one)?)?
            .expect("replaced key should exist");
        assert_eq!(LeanString::cstr(&found)?, "uno");
        rb_map = rb_map.erase($lean, &($make)($lean, $two)?)?;
        assert_eq!(rb_map.size()?, 1);
        assert!(rb_map.find($lean, &($make)($lean, $two)?)?.is_none());
    }};
}

#[test]
fn test_supported_non_nat_string_key_matrix_runtime_semantics() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        exercise_key_family!(lean, LeanInt, LeanInt::from_i64, -1, 2, 99);

        Ok::<_, LeanError>(())
    })
    .unwrap();
}

#[test]
fn test_fixed_width_signed_int_key_matrix_runtime_semantics() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        exercise_key_family!(lean, LeanInt8, LeanInt8::mk, -1i8, 2i8, 99i8);
        exercise_key_family!(lean, LeanInt16, LeanInt16::mk, -1i16, 2i16, 99i16);
        exercise_key_family!(lean, LeanInt32, LeanInt32::mk, -1i32, 2i32, 99i32);
        exercise_key_family!(lean, LeanInt64, LeanInt64::mk, -1i64, 2i64, 99i64);

        Ok::<_, LeanError>(())
    })
    .unwrap();
}
