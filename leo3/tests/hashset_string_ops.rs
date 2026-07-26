//! Runtime coverage for Lean `HashSet` with `String` keys.

#![cfg(all(feature = "runtime-tests", lean_4_22, not(target_os = "macos")))]

use leo3::prelude::*;

fn string_hashset_entries<'l>(list: &LeanBound<'l, LeanList>) -> LeanResult<Vec<String>> {
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
fn test_hashset_string_keys_deduplicate_duplicate_inserts() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        let mut set = leo3::types::LeanHashSet::<LeanString>::empty(lean)?;

        set = set.insert(lean, LeanString::mk(lean, "alpha")?)?;
        set = set.insert(lean, LeanString::mk(lean, "beta")?)?;
        set = set.insert(lean, LeanString::mk(lean, "alpha")?)?;

        assert_eq!(set.size()?, 2, "duplicate inserts should be idempotent");
        assert!(set.contains(lean, &LeanString::mk(lean, "alpha")?)?);
        assert_eq!(
            string_hashset_entries(&set.to_list(lean)?)?,
            vec!["alpha".to_string(), "beta".to_string()]
        );

        Ok::<_, LeanError>(())
    })
    .unwrap();
}
