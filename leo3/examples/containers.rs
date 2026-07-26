//! Example: Experimental Container Wrappers.
//!
//! Demonstrates LeanHashMap, LeanHashSet, and LeanRBMap with
//! insert, find, contains, erase, and iteration.
//!
//! Run with:
//! ```bash
//! cargo run --example containers --features experimental-containers
//! ```

#[cfg(not(lean_4_22))]
fn main() {
    println!("This example requires Lean >= 4.22 (lean_4_22 cfg not set).");
}

#[cfg(lean_4_22)]
use leo3::prelude::*;
#[cfg(lean_4_22)]
use leo3::types::{LeanHashMap, LeanHashSet, LeanRBMap};

#[cfg(lean_4_22)]
fn main() -> LeanResult<()> {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        println!("=== Containers Example ===\n");

        println!("1. LeanHashMap (Nat -> String):");
        let mut map = LeanHashMap::<LeanNat, LeanString>::empty(lean)?;
        map = map.insert(
            lean,
            LeanNat::from_usize(lean, 1)?,
            LeanString::mk(lean, "one")?,
        )?;
        map = map.insert(
            lean,
            LeanNat::from_usize(lean, 2)?,
            LeanString::mk(lean, "two")?,
        )?;
        map = map.insert(
            lean,
            LeanNat::from_usize(lean, 3)?,
            LeanString::mk(lean, "three")?,
        )?;
        println!("   size: {}", map.size()?);

        let key2 = LeanNat::from_usize(lean, 2)?;
        let found = map.find(lean, &key2)?;
        if let Some(val) = found {
            println!("   find(2) = {}", LeanString::cstr(&val)?);
        }
        println!(
            "   contains(3): {}",
            map.contains(lean, &LeanNat::from_usize(lean, 3)?)?
        );

        map = map.erase(lean, &key2)?;
        println!("   after erase(2), size: {}", map.size()?);
        println!("   contains(2): {}", map.contains(lean, &key2)?);

        println!("\n2. LeanHashMap (String -> Nat):");
        let mut smap = LeanHashMap::<LeanString, LeanNat>::empty(lean)?;
        smap = smap.insert(
            lean,
            LeanString::mk(lean, "answer")?,
            LeanNat::from_usize(lean, 42)?,
        )?;
        smap = smap.insert(
            lean,
            LeanString::mk(lean, "zero")?,
            LeanNat::from_usize(lean, 0)?,
        )?;
        let answer_key = LeanString::mk(lean, "answer")?;
        if let Some(val) = smap.find(lean, &answer_key)? {
            println!("   find(\"answer\") = {}", LeanNat::to_usize(&val)?);
        }

        println!("\n3. LeanHashSet (Nat):");
        let mut set = LeanHashSet::<LeanNat>::empty(lean)?;
        set = set.insert(lean, LeanNat::from_usize(lean, 10)?)?;
        set = set.insert(lean, LeanNat::from_usize(lean, 20)?)?;
        set = set.insert(lean, LeanNat::from_usize(lean, 10)?)?;
        println!("   size (after duplicate insert): {}", set.size()?);
        println!(
            "   contains(10): {}",
            set.contains(lean, &LeanNat::from_usize(lean, 10)?)?
        );
        println!(
            "   contains(30): {}",
            set.contains(lean, &LeanNat::from_usize(lean, 30)?)?
        );

        set = set.erase(lean, &LeanNat::from_usize(lean, 10)?)?;
        println!("   after erase(10), size: {}", set.size()?);

        println!("\n4. LeanRBMap (Nat -> String):");
        let mut rbmap = LeanRBMap::<LeanNat, LeanString>::empty(lean)?;
        rbmap = rbmap.insert(
            lean,
            LeanNat::from_usize(lean, 5)?,
            LeanString::mk(lean, "five")?,
        )?;
        rbmap = rbmap.insert(
            lean,
            LeanNat::from_usize(lean, 1)?,
            LeanString::mk(lean, "one")?,
        )?;
        rbmap = rbmap.insert(
            lean,
            LeanNat::from_usize(lean, 9)?,
            LeanString::mk(lean, "nine")?,
        )?;
        println!("   size: {}", rbmap.size()?);

        if let Some(val) = rbmap.find(lean, &LeanNat::from_usize(lean, 5)?)? {
            println!("   find(5) = {}", LeanString::cstr(&val)?);
        }

        if let Some(min) = rbmap.min_entry(lean)? {
            let key: LeanBound<'_, LeanNat> = LeanProd::fst(&min).cast();
            let val: LeanBound<'_, LeanString> = LeanProd::snd(&min).cast();
            println!(
                "   min_entry: {} -> {}",
                LeanNat::to_usize(&key)?,
                LeanString::cstr(&val)?
            );
        }
        if let Some(max) = rbmap.max_entry(lean)? {
            let key: LeanBound<'_, LeanNat> = LeanProd::fst(&max).cast();
            let val: LeanBound<'_, LeanString> = LeanProd::snd(&max).cast();
            println!(
                "   max_entry: {} -> {}",
                LeanNat::to_usize(&key)?,
                LeanString::cstr(&val)?
            );
        }

        println!("\n5. LeanRBMap (String -> Nat):");
        let mut srbmap = LeanRBMap::<LeanString, LeanNat>::empty(lean)?;
        srbmap = srbmap.insert(
            lean,
            LeanString::mk(lean, "alpha")?,
            LeanNat::from_usize(lean, 1)?,
        )?;
        srbmap = srbmap.insert(
            lean,
            LeanString::mk(lean, "beta")?,
            LeanNat::from_usize(lean, 2)?,
        )?;
        println!("   size: {}", srbmap.size()?);
        println!(
            "   contains(\"alpha\"): {}",
            srbmap.contains(lean, &LeanString::mk(lean, "alpha")?)?
        );

        println!("\n=== All container operations completed successfully! ===");
        Ok(())
    })
}
