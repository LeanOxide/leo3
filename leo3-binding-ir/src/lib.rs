#![cfg_attr(docsrs, feature(doc_cfg))]
//! Shared semantic IR and analyzers for Leo3 binding macros.

mod analysis;
mod model;
mod quoting;

pub use analysis::{
    analyze_lean_class_impl, analyze_lean_class_struct, analyze_lean_function,
    collect_module_exports, is_leanfn_attr,
};
pub use model::{
    BindingSemantics, ClassImplBinding, ClassTypeBinding, FunctionBinding, FunctionOptions,
    ModuleBinding, ParameterBinding, PassingStyle, ReceiverStyle, TypeBinding, TypeShape,
    BINDING_SCHEMA_VERSION,
};
pub use quoting::{
    quote_runtime_class_metadata, quote_runtime_function_metadata, quote_runtime_module_metadata,
};
