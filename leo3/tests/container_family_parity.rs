//! Cross-family runtime parity checks for containers.

#![cfg(all(feature = "runtime-tests", lean_4_22))]

use leo3::prelude::*;

fn map_entries<'l>(list: &LeanBound<'l, LeanList>) -> LeanResult<Vec<(String, String)>> {
    let mut out = Vec::new();
    let mut current = list.clone();
    while !LeanList::isEmpty(&current) {
        let head = LeanList::head(&current).expect("non-empty list should have head");
        let pair: LeanBound<'l, LeanProd> = head.cast();
        let key: LeanBound<'l, LeanString> = LeanProd::fst(&pair).cast();
        let value: LeanBound<'l, LeanString> = LeanProd::snd(&pair).cast();
        out.push((
            LeanString::cstr(&key)?.to_owned(),
            LeanString::cstr(&value)?.to_owned(),
        ));
        current = LeanList::tail(&current).expect("non-empty list should have tail");
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

fn set_entries<'l>(list: &LeanBound<'l, LeanList>) -> LeanResult<Vec<String>> {
    let mut out = Vec::new();
    let mut current = list.clone();
    while !LeanList::isEmpty(&current) {
        let head = LeanList::head(&current).expect("non-empty list should have head");
        let value: LeanBound<'l, LeanString> = head.cast();
        out.push(LeanString::cstr(&value)?.to_owned());
        current = LeanList::tail(&current).expect("non-empty list should have tail");
    }
    out.sort();
    Ok(out)
}

#[test]
fn test_string_key_container_families_share_final_semantics() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        let mut hash_map = leo3::types::LeanHashMap::<LeanString, LeanString>::empty(lean)?;
        let mut rb_map = leo3::types::LeanRBMap::<LeanString, LeanString>::empty(lean)?;
        let mut hash_set = leo3::types::LeanHashSet::<LeanString>::empty(lean)?;

        for (key, value) in [
            ("delta", "4"),
            ("alpha", "1"),
            ("beta", "2"),
            ("alpha", "1b"),
        ] {
            let key_for_map = LeanString::mk(lean, key)?;
            let value_for_map = LeanString::mk(lean, value)?;
            hash_map = hash_map.insert(lean, key_for_map, value_for_map)?;

            let key_for_rb = LeanString::mk(lean, key)?;
            let value_for_rb = LeanString::mk(lean, value)?;
            rb_map = rb_map.insert(lean, key_for_rb, value_for_rb)?;

            hash_set = hash_set.insert(lean, LeanString::mk(lean, key)?)?;
        }

        hash_map = hash_map.erase(lean, &LeanString::mk(lean, "delta")?)?;
        rb_map = rb_map.erase(lean, &LeanString::mk(lean, "delta")?)?;
        hash_set = hash_set.erase(lean, &LeanString::mk(lean, "delta")?)?;

        let expected_map = vec![
            ("alpha".to_string(), "1b".to_string()),
            ("beta".to_string(), "2".to_string()),
        ];
        let expected_keys = vec!["alpha".to_string(), "beta".to_string()];

        assert_eq!(map_entries(&hash_map.to_list(lean)?)?, expected_map);
        assert_eq!(map_entries(&rb_map.to_list(lean)?)?, expected_map);
        assert_eq!(set_entries(&hash_set.to_list(lean)?)?, expected_keys);

        Ok::<_, LeanError>(())
    })
    .unwrap();
}

macro_rules! int_key_parity {
    ($lean:ident, $key_ty:ty, $mk:expr) => {{
        let mut hash_map = leo3::types::LeanHashMap::<$key_ty, LeanString>::empty($lean)?;
        let mut rb_map = leo3::types::LeanRBMap::<$key_ty, LeanString>::empty($lean)?;
        let mut hash_set = leo3::types::LeanHashSet::<$key_ty>::empty($lean)?;

        let keys: [i64; 4] = [10, -5, 3, -5];
        for &k in &keys {
            let v = format!("v{}", k);
            hash_map = hash_map.insert($lean, ($mk)($lean, k as _)?, LeanString::mk($lean, &v)?)?;
            rb_map = rb_map.insert($lean, ($mk)($lean, k as _)?, LeanString::mk($lean, &v)?)?;
            hash_set = hash_set.insert($lean, ($mk)($lean, k as _)?)?;
        }

        assert_eq!(hash_map.size()?, 3);
        assert_eq!(rb_map.size()?, 3);
        assert_eq!(hash_set.size()?, 3);

        hash_map = hash_map.erase($lean, &($mk)($lean, 10 as _)?)?;
        rb_map = rb_map.erase($lean, &($mk)($lean, 10 as _)?)?;
        hash_set = hash_set.erase($lean, &($mk)($lean, 10 as _)?)?;

        assert_eq!(hash_map.size()?, 2);
        assert_eq!(rb_map.size()?, 2);
        assert_eq!(hash_set.size()?, 2);

        assert!(hash_map.contains($lean, &($mk)($lean, -5 as _)?)?);
        assert!(rb_map.contains($lean, &($mk)($lean, -5 as _)?)?);
        assert!(hash_set.contains($lean, &($mk)($lean, -5 as _)?)?);
        assert!(!hash_map.contains($lean, &($mk)($lean, 10 as _)?)?);
        assert!(!rb_map.contains($lean, &($mk)($lean, 10 as _)?)?);
        assert!(!hash_set.contains($lean, &($mk)($lean, 10 as _)?)?);
    }};
}

#[test]
fn test_fixed_width_signed_int_key_container_families_share_final_semantics() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        int_key_parity!(lean, LeanInt8, LeanInt8::mk);
        int_key_parity!(lean, LeanInt16, LeanInt16::mk);
        int_key_parity!(lean, LeanInt32, LeanInt32::mk);
        int_key_parity!(lean, LeanInt64, LeanInt64::mk);

        Ok::<_, LeanError>(())
    })
    .unwrap();
}

macro_rules! uint_key_parity {
    ($lean:ident, $key_ty:ty, $mk:expr) => {{
        let mut hash_map = leo3::types::LeanHashMap::<$key_ty, LeanString>::empty($lean)?;
        let mut rb_map = leo3::types::LeanRBMap::<$key_ty, LeanString>::empty($lean)?;
        let mut hash_set = leo3::types::LeanHashSet::<$key_ty>::empty($lean)?;

        let keys: [u64; 4] = [10, 5, 3, 5];
        for &k in &keys {
            let v = format!("v{}", k);
            hash_map = hash_map.insert($lean, ($mk)($lean, k as _)?, LeanString::mk($lean, &v)?)?;
            rb_map = rb_map.insert($lean, ($mk)($lean, k as _)?, LeanString::mk($lean, &v)?)?;
            hash_set = hash_set.insert($lean, ($mk)($lean, k as _)?)?;
        }

        assert_eq!(hash_map.size()?, 3);
        assert_eq!(rb_map.size()?, 3);
        assert_eq!(hash_set.size()?, 3);

        hash_map = hash_map.erase($lean, &($mk)($lean, 10 as _)?)?;
        rb_map = rb_map.erase($lean, &($mk)($lean, 10 as _)?)?;
        hash_set = hash_set.erase($lean, &($mk)($lean, 10 as _)?)?;

        assert_eq!(hash_map.size()?, 2);
        assert_eq!(rb_map.size()?, 2);
        assert_eq!(hash_set.size()?, 2);

        assert!(hash_map.contains($lean, &($mk)($lean, 5 as _)?)?);
        assert!(rb_map.contains($lean, &($mk)($lean, 5 as _)?)?);
        assert!(hash_set.contains($lean, &($mk)($lean, 5 as _)?)?);
        assert!(!hash_map.contains($lean, &($mk)($lean, 10 as _)?)?);
        assert!(!rb_map.contains($lean, &($mk)($lean, 10 as _)?)?);
        assert!(!hash_set.contains($lean, &($mk)($lean, 10 as _)?)?);
    }};
}

#[test]
fn test_fixed_width_unsigned_int_key_container_families_share_final_semantics() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        uint_key_parity!(lean, LeanUInt8, LeanUInt8::mk);
        uint_key_parity!(lean, LeanUInt16, LeanUInt16::mk);
        uint_key_parity!(lean, LeanUInt32, LeanUInt32::mk);
        uint_key_parity!(lean, LeanUInt64, LeanUInt64::mk);

        Ok::<_, LeanError>(())
    })
    .unwrap();
}
