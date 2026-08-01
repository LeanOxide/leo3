#![cfg_attr(docsrs, feature(doc_cfg))]
//! Shared semantic IR and analyzers for Leo3 binding macros.

mod analysis;
mod embed;
mod model;
mod quoting;
mod serialize;

pub use analysis::{
    analyze_concrete_instance, analyze_lean_class_impl, analyze_lean_class_struct,
    analyze_lean_function, collect_module_exports, collect_submodule_exports, filter_exports,
    is_leanfn_attr, substitute_type, ConcreteAttr,
};
pub use embed::{
    frame_metadata_entry, parse_metadata_entries, METADATA_ENTRY_MAGIC, METADATA_SECTION_MARKER,
    METADATA_SECTION_NAME, METADATA_SECTION_NAME_APPLE,
};
pub use model::{
    BindingKind, BindingSemantics, ClassImplBinding, ClassMetadata, ClassTypeBinding,
    FunctionBinding, FunctionOptions, ModuleBinding, ParameterBinding, PassingStyle, ReceiverStyle,
    SubmoduleBinding, TypeBinding, TypeShape, BINDING_SCHEMA_VERSION,
};
pub use quoting::quote_metadata_section_static;
pub use quoting::{
    quote_runtime_class_metadata, quote_runtime_function_metadata, quote_runtime_module_metadata,
    quote_runtime_submodule_metadata,
};
pub use serialize::{class_binding_to_json, module_binding_to_json};
