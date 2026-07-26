//! Runtime coverage for Lean `HashSet` with `Nat` keys.

#![cfg(all(feature = "runtime-tests", lean_4_22))]

use leo3::prelude::*;

fn hashset_entries<'l>(list: &LeanBound<'l, LeanList>) -> LeanResult<Vec<usize>> {
    let mut out = Vec::new();
    let mut current = list.clone();
    while !LeanList::isEmpty(&current) {
        let head = LeanList::head(&current).expect("non-empty list should have head");
        let value: LeanBound<'l, LeanNat> = head.cast();
        out.push(LeanNat::to_usize(&value)?);
        current = LeanList::tail(&current).expect("non-empty list should have tail");
    }
    out.sort();
    Ok(out)
}

#[test]
fn test_hashset_nat_runtime_semantics() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        let mut set = leo3::types::LeanHashSet::<LeanNat>::empty(lean)?;

        assert!(set.is_empty()?);
        assert_eq!(set.size()?, 0);
        assert!(!set.contains(lean, &LeanNat::from_usize(lean, 1)?)?);

        set = set.insert(lean, LeanNat::from_usize(lean, 2)?)?;
        set = set.insert(lean, LeanNat::from_usize(lean, 1)?)?;
        set = set.insert(lean, LeanNat::from_usize(lean, 3)?)?;

        assert!(!set.is_empty()?);
        assert_eq!(set.size()?, 3);
        assert!(set.contains(lean, &LeanNat::from_usize(lean, 1)?)?);
        assert!(!set.contains(lean, &LeanNat::from_usize(lean, 9)?)?);
        assert_eq!(
            LeanNat::to_usize(
                &set.get(lean, &LeanNat::from_usize(lean, 2)?)?
                    .expect("2 should exist")
            )?,
            2
        );

        assert_eq!(hashset_entries(&set.to_list(lean)?)?, vec![1, 2, 3]);

        set = set.erase(lean, &LeanNat::from_usize(lean, 2)?)?;
        assert_eq!(set.size()?, 2);
        assert!(set.get(lean, &LeanNat::from_usize(lean, 2)?)?.is_none());
        assert_eq!(hashset_entries(&set.to_list(lean)?)?, vec![1, 3]);

        Ok::<_, LeanError>(())
    })
    .unwrap();
}
