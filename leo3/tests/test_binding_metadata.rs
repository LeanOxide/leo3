//! Structured binding metadata contract tests.

#![cfg(feature = "macros")]

use leo3::prelude::*;

#[derive(Clone)]
#[leanclass]
struct SchemaCounter {
    value: i32,
}

#[leanclass]
impl SchemaCounter {
    fn new(value: i32) -> Self {
        Self { value }
    }

    fn bump(&mut self, delta: i32) -> i32 {
        self.value += delta;
        self.value
    }
}

#[derive(Clone)]
#[leanclass]
struct SchemaWidget {
    size: u32,
}

#[leanclass]
impl SchemaWidget {
    #[getter]
    fn size(&self) -> u32 {
        self.size
    }

    #[setter]
    fn set_size(&mut self, size: u32) {
        self.size = size;
    }
}

#[leanmodule(name = "SchemaModule")]
mod schema_module {
    use leo3::prelude::*;

    #[leanfn(name = "schema_banner")]
    pub fn banner(name: String, count: u64) -> String {
        let _ = count;
        name
    }
}

#[test]
fn function_and_module_metadata_are_structured() {
    let function = schema_module::__leo3_metadata_banner();
    assert_eq!(function.schema_version, leo3::LEO3_BINDING_SCHEMA_VERSION);
    assert_eq!(function.rust_name, "banner");
    assert_eq!(function.name, "schema_banner");
    assert_eq!(function.ffi_symbol, "schema_banner");
    assert_eq!(function.params[0].ty.lean, Some("String"));
    assert_eq!(function.params[1].ty.lean, Some("UInt64"));
    assert_eq!(function.return_type.lean, Some("String"));
    assert_eq!(function.receiver, leo3::LeanBindingReceiver::None);
    assert_eq!(function.semantics, leo3::LeanBindingSemantics::Value);

    let module = schema_module::__leo3_module_metadata();
    assert_eq!(module.schema_version, leo3::LEO3_BINDING_SCHEMA_VERSION);
    assert_eq!(module.name, "SchemaModule");
    assert_eq!(module.exports.len(), 1);
    assert_eq!(module.exports[0].name, "schema_banner");
}

#[test]
fn leanclass_metadata_captures_receiver_semantics() {
    assert_eq!(SCHEMACOUNTER_LEAN_CLASS_DECL, "opaque SchemaCounter : Type");
    assert!(SCHEMACOUNTER_LEAN_METHODS_DECL.contains("SchemaCounter.bump"));

    let class = __leo3_class_metadata_SchemaCounter();
    assert_eq!(class.schema_version, leo3::LEO3_BINDING_SCHEMA_VERSION);
    assert_eq!(class.methods.len(), 2);
    assert_eq!(class.methods[0].receiver, leo3::LeanBindingReceiver::None);
    assert_eq!(class.methods[1].receiver, leo3::LeanBindingReceiver::MutRef);
    assert_eq!(
        class.methods[1].semantics,
        leo3::LeanBindingSemantics::MutatesSelfWithValue
    );
    assert_eq!(
        class.methods[1].return_type.lean,
        Some("Prod SchemaCounter Int32")
    );
    assert!(class.methods[1]
        .lean_decl
        .expect("method declaration")
        .contains("__lean_ffi_SchemaCounter_bump"));
}

#[test]
fn leanclass_metadata_captures_accessor_kind() {
    let class = __leo3_class_metadata_SchemaWidget();
    assert_eq!(class.schema_version, leo3::LEO3_BINDING_SCHEMA_VERSION);
    assert_eq!(class.methods.len(), 2);

    let getter = &class.methods[0];
    assert_eq!(getter.rust_name, "size");
    assert_eq!(getter.kind, leo3::LeanBindingKind::Getter);
    assert_eq!(getter.receiver, leo3::LeanBindingReceiver::Ref);
    assert_eq!(getter.return_type.lean, Some("UInt32"));

    let setter = &class.methods[1];
    assert_eq!(setter.rust_name, "set_size");
    assert_eq!(setter.kind, leo3::LeanBindingKind::Setter);
    assert_eq!(setter.receiver, leo3::LeanBindingReceiver::MutRef);
    assert_eq!(setter.semantics, leo3::LeanBindingSemantics::MutatesSelf);
    assert_eq!(setter.return_type.lean, Some("SchemaWidget"));
}

#[test]
fn leanclass_metadata_regular_methods_are_kind_method() {
    let class = __leo3_class_metadata_SchemaCounter();
    for method in class.methods {
        assert_eq!(method.kind, leo3::LeanBindingKind::Method);
    }
}
