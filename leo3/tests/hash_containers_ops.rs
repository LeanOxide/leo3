//! Runtime coverage for real hash containers.

#![cfg(all(feature = "runtime-tests", lean_4_22))]

use leo3::prelude::*;

fn hashmap_entries<'l>(list: &LeanBound<'l, LeanList>) -> LeanResult<Vec<(usize, String)>> {
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
    out.sort_by_key(|(k, _)| *k);
    Ok(out)
}

#[test]
fn test_hashmap_nat_string_runtime_semantics() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        let mut map = leo3::types::LeanHashMap::<LeanNat, LeanString>::empty(lean)?;

        assert!(map.is_empty()?);
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

        assert!(!map.is_empty()?);
        assert_eq!(map.size()?, 3);
        assert!(map.contains(lean, &LeanNat::from_usize(lean, 1)?)?);
        assert!(!map.contains(lean, &LeanNat::from_usize(lean, 9)?)?);
        assert_eq!(
            LeanString::cstr(
                &map.find(lean, &LeanNat::from_usize(lean, 2)?)?
                    .expect("key 2 should exist")
            )?,
            "beta"
        );

        assert_eq!(
            hashmap_entries(&map.to_list(lean)?)?,
            vec![
                (1, "alpha".to_string()),
                (2, "beta".to_string()),
                (3, "gamma".to_string())
            ]
        );

        map = map.erase(lean, &LeanNat::from_usize(lean, 2)?)?;
        assert_eq!(map.size()?, 2);
        assert!(map.find(lean, &LeanNat::from_usize(lean, 2)?)?.is_none());
        assert_eq!(
            hashmap_entries(&map.to_list(lean)?)?,
            vec![(1, "alpha".to_string()), (3, "gamma".to_string())]
        );

        Ok::<_, LeanError>(())
    })
    .unwrap();
}
