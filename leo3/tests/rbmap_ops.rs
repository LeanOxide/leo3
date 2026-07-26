//! Runtime coverage for the real `LeanRBMap` wrapper.

#![cfg(all(feature = "runtime-tests", lean_4_22))]

use leo3::prelude::*;

fn rbmap_entries<'l>(list: &LeanBound<'l, LeanList>) -> LeanResult<Vec<(usize, String)>> {
    let mut out = Vec::new();
    let mut current = list.clone();

    while !LeanList::isEmpty(&current) {
        let head = LeanList::head(&current).expect("non-empty list should have head");
        let pair: LeanBound<'l, LeanProd> = head.cast();
        let key: LeanBound<'l, LeanNat> = LeanProd::fst(&pair).cast();
        let value: LeanBound<'l, LeanString> = LeanProd::snd(&pair).cast();
        out.push((
            LeanNat::to_usize(&key)?,
            LeanString::cstr(&value)?.to_owned(),
        ));
        current = LeanList::tail(&current).expect("non-empty list should have tail");
    }

    Ok(out)
}

#[test]
fn test_rbmap_empty_insert_find_contains_erase_and_list() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        let mut map = leo3::types::LeanRBMap::<LeanNat, LeanString>::empty(lean)?;

        assert!(map.is_empty());
        assert_eq!(map.size()?, 0);
        assert!(map.find(lean, &LeanNat::from_usize(lean, 1)?)?.is_none());

        map = map.insert(
            lean,
            LeanNat::from_usize(lean, 2)?,
            LeanString::mk(lean, "beta")?,
        )?;
        map = map.insert(
            lean,
            LeanNat::from_usize(lean, 1)?,
            LeanString::mk(lean, "alpha")?,
        )?;
        map = map.insert(
            lean,
            LeanNat::from_usize(lean, 3)?,
            LeanString::mk(lean, "gamma")?,
        )?;

        assert!(!map.is_empty());
        assert_eq!(map.size()?, 3);
        assert!(map.contains(lean, &LeanNat::from_usize(lean, 1)?)?);
        assert!(!map.contains(lean, &LeanNat::from_usize(lean, 9)?)?);

        let found = map
            .find(lean, &LeanNat::from_usize(lean, 2)?)?
            .expect("key 2 should exist");
        assert_eq!(LeanString::cstr(&found)?, "beta");

        assert_eq!(
            rbmap_entries(&map.to_list(lean)?)?,
            vec![
                (1, "alpha".to_string()),
                (2, "beta".to_string()),
                (3, "gamma".to_string())
            ]
        );

        let min = map.min_entry(lean)?.expect("min entry should exist");
        let min_key: LeanBound<'_, LeanNat> = LeanProd::fst(&min).cast();
        let min_value: LeanBound<'_, LeanString> = LeanProd::snd(&min).cast();
        assert_eq!(LeanNat::to_usize(&min_key)?, 1);
        assert_eq!(LeanString::cstr(&min_value)?, "alpha");

        let max = map.max_entry(lean)?.expect("max entry should exist");
        let max_key: LeanBound<'_, LeanNat> = LeanProd::fst(&max).cast();
        let max_value: LeanBound<'_, LeanString> = LeanProd::snd(&max).cast();
        assert_eq!(LeanNat::to_usize(&max_key)?, 3);
        assert_eq!(LeanString::cstr(&max_value)?, "gamma");

        map = map.erase(lean, &LeanNat::from_usize(lean, 2)?)?;
        assert_eq!(map.size()?, 2);
        assert!(map.find(lean, &LeanNat::from_usize(lean, 2)?)?.is_none());
        assert_eq!(
            rbmap_entries(&map.to_list(lean)?)?,
            vec![(1, "alpha".to_string()), (3, "gamma".to_string())]
        );

        Ok::<_, LeanError>(())
    })
    .unwrap();
}
