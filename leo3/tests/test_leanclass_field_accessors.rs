//! Tests for `#[get]` / `#[set]` field accessors on `#[leanclass]` structs.
//!
//! Field accessors are PyO3-aligned (`#[pyo3(get, set)]`): `#[get]`
//! generates `fn field(&self) -> T` (clone-based, like external-object
//! extraction), `#[set]` generates `fn set_field(&mut self, value: T)`
//! (copy-on-write, like `&mut self -> ()` methods). Both get FFI wrappers,
//! Lean declarations, and metadata entries that merge with the impl block's
//! class metadata.

#![cfg(all(feature = "macros", feature = "runtime-tests"))]

use leo3::external::{LeanExternal, LeanExternalType};
use leo3::prelude::*;

#[derive(Clone, Debug, PartialEq)]
#[leanclass]
struct Player {
    #[get]
    name: String,
    #[get]
    #[set]
    score: i64,
    #[set]
    active: bool,
    #[get]
    ratio: f64,
}

#[leanclass]
impl Player {
    fn new(name: String, score: i64) -> Self {
        Player {
            name,
            score,
            active: true,
            ratio: 1.5,
        }
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }
}

#[test]
fn test_field_accessor_rust_methods() {
    let mut player = Player::new("Ada".to_string(), 42);
    assert_eq!(player.name(), "Ada");
    assert_eq!(player.score(), 42);
    assert!((player.ratio() - 1.5).abs() < 1e-9);

    player.set_score(100);
    assert_eq!(player.score(), 100);
    player.set_active(false);
    assert!(!player.active);
}

#[test]
fn test_field_accessor_ffi_wrappers() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let player = Player::new("Grace".to_string(), 7);
        let external = LeanExternal::new(lean, player)?;

        // Getter FFI: `Self -> String` (boxed). The wrappers take ownership
        // of their object reference, so pass a fresh owned ref per call.
        let name_ptr = unsafe { __lean_ffi_Player_name(external.clone().into_ptr()) };
        let name_bound: LeanBound<'_, LeanString> =
            unsafe { LeanBound::from_owned_ptr(lean, name_ptr) };
        assert_eq!(LeanString::cstr(&name_bound)?, "Grace");

        // Fixed-width scalars cross the FFI unboxed.
        let score_val = unsafe { __lean_ffi_Player_score(external.clone().into_ptr()) };
        assert_eq!(score_val, 7);

        // Setter FFI: `Self -> Int64 -> Self` (copy-on-write when shared).
        // Sharing the object (RC = 2) forces the COW path.
        let new_ptr = unsafe { __lean_ffi_Player_set_score(external.clone().into_ptr(), 99) };
        let new_bound: LeanBound<'_, LeanExternalType<Player>> =
            unsafe { LeanBound::from_owned_ptr(lean, new_ptr) };
        assert_eq!(new_bound.get_ref().score, 99);
        // The original object is unchanged (copy-on-write).
        assert_eq!(external.get_ref().score, 7);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_field_accessor_metadata() {
    leo3::prepare_freethreaded_lean();

    // Impl-block metadata and field-accessor metadata both exist.
    let class_meta = __leo3_class_metadata_Player();
    assert_eq!(class_meta.rust_name, "Player");
    let method_names: Vec<&str> = class_meta.methods.iter().map(|m| m.rust_name).collect();
    assert!(method_names.contains(&"new"));
    assert!(method_names.contains(&"get_name"));

    let fields_meta = __leo3_class_metadata_Player_fields();
    assert_eq!(fields_meta.rust_name, "Player");
    let getter_names: Vec<&str> = fields_meta
        .methods
        .iter()
        .filter(|m| matches!(m.kind, leo3::LeanBindingKind::Getter))
        .map(|m| m.rust_name)
        .collect();
    let setter_names: Vec<&str> = fields_meta
        .methods
        .iter()
        .filter(|m| matches!(m.kind, leo3::LeanBindingKind::Setter))
        .map(|m| m.rust_name)
        .collect();
    assert_eq!(getter_names, vec!["name", "score", "ratio"]);
    assert_eq!(setter_names, vec!["set_score", "set_active"]);

    // Lean declarations mention the accessors.
    assert!(PLAYER_LEAN_FIELDS_DECL.contains("opaque Player.name : Player → String"));
    assert!(PLAYER_LEAN_FIELDS_DECL.contains("opaque Player.set_score : Player → Int64 → Player"));
}

#[test]
fn test_field_accessor_roundtrip_through_lean() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Convert into a Lean external and back through FromLean.
        let player = Player::new("Lin".to_string(), 5);
        let external: LeanBound<'_, LeanExternalType<Player>> =
            LeanExternal::new(lean, player.clone())?;
        let recovered: Player = Player::from_lean(&external)?;
        assert_eq!(recovered, player);

        // The generated getter matches the manual getter.
        let external2 = LeanExternal::new(lean, player)?;
        let manual = unsafe { __lean_ffi_Player_get_name(external2.clone().into_ptr()) };
        let manual_bound: LeanBound<'_, LeanString> =
            unsafe { LeanBound::from_owned_ptr(lean, manual) };
        assert_eq!(LeanString::cstr(&manual_bound)?, "Lin");

        Ok(())
    });

    assert!(result.is_ok());
}
