//! Runtime coverage for Lean `RBMap` with `String` keys.

#![cfg(all(feature = "runtime-tests", lean_4_22))]

use leo3::prelude::*;

fn string_rbmap_entries<'l>(list: &LeanBound<'l, LeanList>) -> LeanResult<Vec<(String, String)>> {
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

    Ok(out)
}

#[test]
fn test_rbmap_string_keys_replace_existing_values_in_order() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        let mut map = leo3::types::LeanRBMap::<LeanString, LeanString>::empty(lean)?;

        map = map.insert(
            lean,
            LeanString::mk(lean, "gamma")?,
            LeanString::mk(lean, "third")?,
        )?;
        map = map.insert(
            lean,
            LeanString::mk(lean, "alpha")?,
            LeanString::mk(lean, "first")?,
        )?;
        map = map.insert(
            lean,
            LeanString::mk(lean, "gamma")?,
            LeanString::mk(lean, "updated")?,
        )?;

        assert_eq!(map.size()?, 2, "replacement should not grow the map");
        assert_eq!(
            string_rbmap_entries(&map.to_list(lean)?)?,
            vec![
                ("alpha".to_string(), "first".to_string()),
                ("gamma".to_string(), "updated".to_string()),
            ]
        );

        Ok::<_, LeanError>(())
    })
    .unwrap();
}
