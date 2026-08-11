//! Tests for `#[leanclass]` naming: `#[name = "..."]` on methods and
//! `#[leanclass(name = "...")]` on the class.
//!
//! PyO3-aligned (`#[pyo3(name = "...")]`): the Lean-visible names change
//! while the Rust identifiers and FFI symbols stay derived from the Rust
//! source.

#![cfg(all(feature = "macros", feature = "runtime-tests"))]

use leo3::external::LeanExternal;
use leo3::prelude::*;

#[derive(Clone)]
#[leanclass(name = "RenamedBox")]
struct Box {
    value: i64,
}

#[leanclass(name = "RenamedBox")]
impl Box {
    fn new(initial: i64) -> Self {
        Box { value: initial }
    }

    // Lean-visible name becomes `RenamedBox.double`, FFI symbol stays
    // `__lean_ffi_Box_double`.
    #[name = "double"]
    fn twice(&self) -> i64 {
        self.value * 2
    }

    #[getter(name = "contents")]
    fn get_contents(&self) -> i64 {
        self.value
    }
}

#[test]
fn test_method_rename_metadata() {
    leo3::prepare_freethreaded_lean();

    let meta = __leo3_class_metadata_Box();
    assert_eq!(meta.lean_name, "RenamedBox");
    assert_eq!(meta.rust_name, "Box");

    let double = meta
        .methods
        .iter()
        .find(|m| m.rust_name == "twice")
        .expect("twice method present");
    assert_eq!(double.name, "RenamedBox.double");
    assert_eq!(double.ffi_symbol, "__lean_ffi_Box_twice");

    // The getter's Lean name comes from `#[getter(name = "contents")]`.
    let contents = meta
        .methods
        .iter()
        .find(|m| m.rust_name == "get_contents")
        .expect("getter present");
    assert_eq!(contents.name, "RenamedBox.contents");
    assert!(matches!(contents.kind, leo3::LeanBindingKind::Getter));

    // Declarations use the renamed class and method names.
    assert!(BOX_LEAN_METHODS_DECL.contains("opaque RenamedBox.double : RenamedBox → Int64"));
    assert!(BOX_LEAN_METHODS_DECL.contains("opaque RenamedBox.contents : RenamedBox → Int64"));
    // The opaque class decl uses the renamed class.
    assert!(BOX_LEAN_CLASS_DECL.contains("opaque RenamedBox.ffi : NonemptyType"));
}

#[test]
fn test_method_rename_ffi() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let b = Box::new(21);
        let external = LeanExternal::new(lean, b)?;

        // FFI symbols stay Rust-identifier-derived.
        let doubled = unsafe { __lean_ffi_Box_twice(external.clone().into_ptr()) };
        assert_eq!(doubled, 42);
        let contents = unsafe { __lean_ffi_Box_get_contents(external.clone().into_ptr()) };
        assert_eq!(contents, 21);

        Ok(())
    });

    assert!(result.is_ok());
}
