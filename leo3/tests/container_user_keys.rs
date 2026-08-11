//! User-defined container keys via `#[lean_instance(Hashable, BEq, Ord)]`
//!
//! PyO3 lets any hashable Python object be a dict/set key. The Leo3 analogue:
//! an external class that implements `Hashable` + `BEq` (and optionally `Ord`)
//! through `#[lean_instance(...)]` automatically derives `LeanHashKey` /
//! `LeanRBMapKey`, so it can be used as a `LeanHashMap`, `LeanHashSet`, or
//! `LeanRBMap` key.

#![cfg(all(feature = "macros", feature = "runtime-tests", lean_4_22))]

use leo3::external::{LeanExternal, LeanExternalType};
use leo3::prelude::*;

#[derive(Clone, Debug, PartialEq)]
#[leanclass]
struct Point {
    x: i64,
    y: i64,
}

#[leo3_macros::lean_instance(Hashable, BEq, Ord)]
impl Point {
    fn hash(&self) -> u64 {
        (self.x as u64) ^ (self.y as u64).wrapping_shl(32)
    }

    fn beq(&self, other: &Self) -> bool {
        self == other
    }

    fn compare(&self, other: &Self) -> std::cmp::Ordering {
        (self.x, self.y).cmp(&(other.x, other.y))
    }
}

fn mk_point<'l>(
    lean: Lean<'l>,
    x: i64,
    y: i64,
) -> LeanResult<LeanBound<'l, LeanExternalType<Point>>> {
    LeanExternal::new(lean, Point { x, y })
}

#[test]
fn test_user_key_hashmap_insert_find_erase() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let mut map: LeanHashMap<'_, LeanExternalType<Point>, LeanNat> = LeanHashMap::empty(lean)?;
        assert!(map.is_empty()?);

        let k1 = mk_point(lean, 1, 2)?;
        let v1 = LeanNat::from_usize(lean, 10)?;
        map = map.insert(lean, k1.clone(), v1)?;
        assert_eq!(map.size()?, 1);

        let k2 = mk_point(lean, 3, 4)?;
        let v2 = LeanNat::from_usize(lean, 20)?;
        map = map.insert(lean, k2.clone(), v2)?;
        assert_eq!(map.size()?, 2);

        // Lookup with an equal (but distinct) key object must hit.
        let k1_lookup = mk_point(lean, 1, 2)?;
        assert!(map.contains(lean, &k1_lookup)?);
        let found = map.find(lean, &k1_lookup)?;
        assert!(found.is_some());
        assert_eq!(LeanNat::to_usize(&found.unwrap())?, 10);

        // Lookup with a different key must miss.
        let k3 = mk_point(lean, 5, 6)?;
        assert!(!map.contains(lean, &k3)?);
        assert!(map.find(lean, &k3)?.is_none());

        // Replacement semantics: inserting an equal key replaces the value.
        let v1b = LeanNat::from_usize(lean, 100)?;
        map = map.insert(lean, k1_lookup, v1b)?;
        assert_eq!(map.size()?, 2);
        let found = map.find(lean, &k1)?;
        assert_eq!(LeanNat::to_usize(&found.unwrap())?, 100);

        // Erase.
        map = map.erase(lean, &k2)?;
        assert_eq!(map.size()?, 1);
        assert!(!map.contains(lean, &k2)?);
        assert!(map.contains(lean, &k1)?);

        // to_list / from_list round-trip through the public API.
        let list = map.to_list(lean)?;
        assert_eq!(LeanList::length(&list), 1);
        let rebuilt = LeanHashMap::<LeanExternalType<Point>, LeanNat>::from_list(lean, list)?;
        assert_eq!(rebuilt.size()?, 1);
        assert_eq!(LeanNat::to_usize(&rebuilt.find(lean, &k1)?.unwrap())?, 100);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_user_key_hashset_ops() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let mut set: LeanHashSet<'_, LeanExternalType<Point>> = LeanHashSet::empty(lean)?;

        let p1 = mk_point(lean, 1, 1)?;
        set = set.insert(lean, p1.clone())?;
        assert_eq!(set.size()?, 1);

        // Equal key: no duplicate.
        let p1b = mk_point(lean, 1, 1)?;
        set = set.insert(lean, p1b)?;
        assert_eq!(set.size()?, 1);

        let p2 = mk_point(lean, 2, 2)?;
        set = set.insert(lean, p2.clone())?;
        assert_eq!(set.size()?, 2);

        assert!(set.contains(lean, &p1)?);
        assert!(set.contains(lean, &p2)?);
        assert!(!set.contains(lean, &mk_point(lean, 9, 9)?)?);

        set = set.erase(lean, &p1)?;
        assert_eq!(set.size()?, 1);
        assert!(!set.contains(lean, &p1)?);
        assert!(set.contains(lean, &p2)?);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_user_key_rbmap_ops() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let mut map: LeanRBMap<'_, LeanExternalType<Point>, LeanNat> = LeanRBMap::empty(lean)?;

        let k1 = mk_point(lean, 1, 2)?;
        let v1 = LeanNat::from_usize(lean, 10)?;
        map = map.insert(lean, k1.clone(), v1)?;

        let k2 = mk_point(lean, 3, 4)?;
        let v2 = LeanNat::from_usize(lean, 20)?;
        map = map.insert(lean, k2.clone(), v2)?;
        assert_eq!(map.size()?, 2);

        let found = map.find(lean, &mk_point(lean, 1, 2)?)?;
        assert!(found.is_some());
        assert_eq!(LeanNat::to_usize(&found.unwrap())?, 10);
        assert!(map.find(lean, &mk_point(lean, 9, 9)?)?.is_none());

        map = map.erase(lean, &k1)?;
        assert_eq!(map.size()?, 1);
        assert!(map.find(lean, &k2)?.is_some());

        Ok(())
    });

    assert!(result.is_ok());
}
