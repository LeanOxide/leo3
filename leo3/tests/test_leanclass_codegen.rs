//! Tests for the Lean code generation from #[leanclass] macro.
//!
//! Verifies that the generated `*_LEAN_CLASS_DECL` and `*_LEAN_METHODS_DECL`
//! string constants contain the expected Lean declarations.

#![cfg(feature = "macros")]

use leo3::prelude::*;

#[derive(Clone)]
#[leanclass]
#[allow(dead_code)]
struct Widget {
    val: i32,
}

#[leanclass]
impl Widget {
    fn create(val: i32) -> Self {
        Widget { val }
    }

    fn value(&self) -> i32 {
        self.val
    }

    fn set_value(&mut self, val: i32) {
        self.val = val;
    }

    fn set_value_and_get(&mut self, val: i32) -> i32 {
        self.val = val;
        self.val
    }
}

#[derive(Clone)]
#[leanclass]
#[allow(dead_code)]
struct TypeShowcase;

#[leanclass]
impl TypeShowcase {
    fn shapes(
        &self,
        xs: Vec<u64>,
        flag: Option<bool>,
        pair: (u64, bool),
        value: Result<String, i32>,
    ) -> Result<Vec<u64>, (String, i32)> {
        let _ = (flag, pair, value);
        Ok(xs)
    }

    fn scalars(&self, a: usize, b: isize, c: f32, d: u8, e: i16, f: char) -> char {
        let _ = (a, b, c, d, e);
        f
    }
}

#[test]
fn test_lean_class_decl_constant() {
    assert_eq!(WIDGET_LEAN_CLASS_DECL, "opaque Widget : Type");
}

#[test]
fn test_lean_methods_decl_contains_all_methods() {
    let decl = WIDGET_LEAN_METHODS_DECL;
    // Should have one line per method
    let lines: Vec<&str> = decl.lines().collect();
    assert_eq!(
        lines.len(),
        4,
        "Expected 4 method declarations, got: {decl}"
    );
}

#[test]
fn test_lean_methods_decl_static_method() {
    let decl = WIDGET_LEAN_METHODS_DECL;
    // Static method: no self param, returns Self → mapped to Widget
    assert!(
        decl.contains(
            r#"@[extern "__lean_ffi_Widget_create"] opaque Widget.create : Int32 → Widget"#
        ),
        "Missing or incorrect static method declaration in:\n{decl}"
    );
}

#[test]
fn test_lean_methods_decl_ref_method() {
    let decl = WIDGET_LEAN_METHODS_DECL;
    // &self method: Widget → return type
    assert!(
        decl.contains(
            r#"@[extern "__lean_ffi_Widget_value"] opaque Widget.value : Widget → Int32"#
        ),
        "Missing or incorrect &self method declaration in:\n{decl}"
    );
}

#[test]
fn test_lean_methods_decl_mut_ref_method() {
    let decl = WIDGET_LEAN_METHODS_DECL;
    // &mut self with () return: Widget → params → Widget (returns modified self)
    assert!(
        decl.contains(r#"@[extern "__lean_ffi_Widget_set_value"] opaque Widget.set_value : Widget → Int32 → Widget"#),
        "Missing or incorrect &mut self method declaration in:\n{decl}"
    );
}

#[test]
fn test_lean_methods_decl_mut_ref_non_unit_method() {
    let decl = WIDGET_LEAN_METHODS_DECL;
    assert!(
        decl.contains(
            r#"@[extern "__lean_ffi_Widget_set_value_and_get"] opaque Widget.set_value_and_get : Widget → Int32 → Prod Widget Int32"#
        ),
        "Missing or incorrect &mut self non-unit declaration in:\n{decl}"
    );
}

#[test]
fn test_ffi_names_follow_pattern() {
    let decl = WIDGET_LEAN_METHODS_DECL;
    // All FFI names should follow __lean_ffi_{Struct}_{method}
    for line in decl.lines() {
        let ffi_start = line.find("\"__lean_ffi_").expect("FFI name not found");
        let ffi_end = line[ffi_start + 1..].find('"').unwrap() + ffi_start + 1;
        let ffi_name = &line[ffi_start + 1..ffi_end];
        assert!(
            ffi_name.starts_with("__lean_ffi_Widget_"),
            "FFI name {ffi_name} doesn't match expected pattern __lean_ffi_Widget_*"
        );
    }
}

#[test]
fn test_richer_type_mapping_for_common_supported_shapes() {
    let decl = TYPESHOWCASE_LEAN_METHODS_DECL;
    assert!(
        decl.contains(
            r#"@[extern "__lean_ffi_TypeShowcase_shapes"] opaque TypeShowcase.shapes : TypeShowcase → Array UInt64 → Option Bool → Prod UInt64 Bool → Except Int32 String → Except (Prod String Int32) (Array UInt64)"#
        ),
        "Missing or incorrect richer declaration mapping in:\n{decl}"
    );
    assert!(
        decl.contains(
            r#"@[extern "__lean_ffi_TypeShowcase_scalars"] opaque TypeShowcase.scalars : TypeShowcase → USize → ISize → Float32 → UInt8 → Int16 → Char → Char"#
        ),
        "Missing or incorrect scalar declaration mapping in:\n{decl}"
    );
}

#[derive(Clone)]
#[leanclass]
#[allow(dead_code)]
struct Property {
    width: u32,
}

#[leanclass]
impl Property {
    fn create(width: u32) -> Self {
        Property { width }
    }

    #[getter]
    fn width(&self) -> u32 {
        self.width
    }

    #[setter]
    fn set_width(&mut self, width: u32) {
        self.width = width;
    }
}

#[test]
fn test_lean_methods_decl_getter() {
    let decl = PROPERTY_LEAN_METHODS_DECL;
    assert!(
        decl.contains(
            r#"@[extern "__lean_ffi_Property_width"] opaque Property.width : Property → UInt32"#
        ),
        "Missing or incorrect getter declaration in:\n{decl}"
    );
}

#[test]
fn test_lean_methods_decl_setter() {
    let decl = PROPERTY_LEAN_METHODS_DECL;
    assert!(
        decl.contains(
            r#"@[extern "__lean_ffi_Property_set_width"] opaque Property.set_width : Property → UInt32 → Property"#
        ),
        "Missing or incorrect setter declaration in:\n{decl}"
    );
}
