//! Conversion matrix extensions: std collections, Cell, Cow, and long tuples.
//!
//! PyO3-aligned surface:
//! - `HashMap<K, V>` / `HashSet<K>` / `BTreeMap<K, V>` round-trip through
//!   Lean's real `LeanHashMap` / `LeanHashSet` / `LeanRBMap` for the
//!   supported key matrix (String, u8..u64, i8..i64).
//! - `Cell<T: Copy>` converts as `T`.
//! - `Cow<str>` / `Cow<[u8]>` convert like their owned counterparts.
//! - tuples up to arity 12 (PyO3's limit).

#![cfg(all(feature = "runtime-tests", lean_4_22))]

use leo3::conversion::FromLean;
use leo3::prelude::*;
use std::collections::{BTreeMap, HashMap, HashSet};

#[test]
fn test_hashmap_string_key_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let mut original = HashMap::new();
        original.insert("alpha".to_string(), 10u64);
        original.insert("beta".to_string(), 20u64);
        original.insert("gamma".to_string(), 30u64);

        let lean_map = original.clone().into_lean(lean)?;
        let recovered: HashMap<String, u64> = HashMap::from_lean(&lean_map)?;
        assert_eq!(recovered, original);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_hashmap_int_keys_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let mut original: HashMap<i64, String> = HashMap::new();
        original.insert(-5, "minus".to_string());
        original.insert(0, "zero".to_string());
        original.insert(7, "seven".to_string());

        let lean_map = original.clone().into_lean(lean)?;
        let recovered: HashMap<i64, String> = HashMap::from_lean(&lean_map)?;
        assert_eq!(recovered, original);

        let mut small: HashMap<u8, bool> = HashMap::new();
        small.insert(1, true);
        small.insert(255, false);
        let lean_small = small.clone().into_lean(lean)?;
        let recovered_small: HashMap<u8, bool> = HashMap::from_lean(&lean_small)?;
        assert_eq!(recovered_small, small);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_hashset_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let original: HashSet<String> = ["x".to_string(), "y".to_string(), "z".to_string()]
            .into_iter()
            .collect();
        let lean_set = original.clone().into_lean(lean)?;
        let recovered: HashSet<String> = HashSet::from_lean(&lean_set)?;
        assert_eq!(recovered, original);

        let ints: HashSet<i32> = [-1, 0, 1, 2].into_iter().collect();
        let lean_ints = ints.clone().into_lean(lean)?;
        let recovered_ints: HashSet<i32> = HashSet::from_lean(&lean_ints)?;
        assert_eq!(recovered_ints, ints);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_btreemap_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let mut original = BTreeMap::new();
        original.insert("a".to_string(), 1u64);
        original.insert("b".to_string(), 2u64);
        original.insert("c".to_string(), 3u64);

        let lean_map = original.clone().into_lean(lean)?;
        let recovered: BTreeMap<String, u64> = BTreeMap::from_lean(&lean_map)?;
        assert_eq!(recovered, original);

        let mut nums = BTreeMap::new();
        nums.insert(-2i16, "neg".to_string());
        nums.insert(3i16, "pos".to_string());
        let lean_nums = nums.clone().into_lean(lean)?;
        let recovered_nums: BTreeMap<i16, String> = BTreeMap::from_lean(&lean_nums)?;
        assert_eq!(recovered_nums, nums);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_cell_conversions() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let cell = std::cell::Cell::new(42u64);
        let lean_val = cell.into_lean(lean)?;
        let back: std::cell::Cell<u64> = std::cell::Cell::from_lean(&lean_val)?;
        assert_eq!(back.get(), 42);

        let bool_cell = std::cell::Cell::new(true);
        let lean_bool = bool_cell.into_lean(lean)?;
        let back_bool: std::cell::Cell<bool> = std::cell::Cell::from_lean(&lean_bool)?;
        assert!(back_bool.get());

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_cow_conversions() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let borrowed: std::borrow::Cow<'_, str> = std::borrow::Cow::Borrowed("cow says moo");
        let lean_str = borrowed.into_lean(lean)?;
        let back: std::borrow::Cow<'_, str> = std::borrow::Cow::from_lean(&lean_str)?;
        assert_eq!(back, "cow says moo");

        let owned: std::borrow::Cow<'_, str> = std::borrow::Cow::Owned("owned".to_string());
        let lean_str2 = owned.into_lean(lean)?;
        let back2: String = String::from_lean(&lean_str2)?;
        assert_eq!(back2, "owned");

        let bytes: std::borrow::Cow<'_, [u8]> = std::borrow::Cow::Borrowed(&[1, 2, 3]);
        let lean_ba = bytes.into_lean(lean)?;
        let back_bytes: std::borrow::Cow<'_, [u8]> = std::borrow::Cow::from_lean(&lean_ba)?;
        assert_eq!(back_bytes.as_ref(), &[1, 2, 3][..]);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_long_tuples_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let t7 = (1u64, 2u64, 3u64, 4u64, 5u64, 6u64, 7u64);
        let lean7 = t7.into_lean(lean)?;
        let back7: (u64, u64, u64, u64, u64, u64, u64) =
            <(u64, u64, u64, u64, u64, u64, u64) as FromLean>::from_lean(&lean7)?;
        assert_eq!(back7, t7);

        let t12 = (1u64, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12u64);
        let lean12 = t12.into_lean(lean)?;
        let back12: (u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64, u64) =
            FromLean::from_lean(&lean12)?;
        assert_eq!(back12, t12);

        // Mixed types through the recursion.
        let mixed = (1u64, "two".to_string(), 3.5f64, true);
        let lean_mixed = mixed.clone().into_lean(lean)?;
        let back_mixed: (u64, String, f64, bool) = FromLean::from_lean(&lean_mixed)?;
        assert_eq!(back_mixed, mixed);

        Ok(())
    });

    assert!(result.is_ok());
}
