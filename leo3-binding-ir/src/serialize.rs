use crate::model::*;

pub fn module_binding_to_json(binding: &ModuleBinding) -> String {
    serde_json::to_string(binding).expect("module binding serialization should not fail")
}

pub fn class_binding_to_json(
    class_binding: &ClassTypeBinding,
    impl_binding: &ClassImplBinding,
) -> String {
    let metadata = ClassMetadata {
        schema_version: BINDING_SCHEMA_VERSION,
        rust_name: class_binding.rust_name.clone(),
        lean_name: class_binding.lean_name.clone(),
        opaque_decl: class_binding.opaque_decl.clone(),
        methods_decl: impl_binding.methods_decl.clone(),
        methods: impl_binding.methods.clone(),
    };

    serde_json::to_string(&metadata).expect("class binding serialization should not fail")
}
