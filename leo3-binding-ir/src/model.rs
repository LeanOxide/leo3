use serde::{Deserialize, Serialize};

pub const BINDING_SCHEMA_VERSION: u32 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PassingStyle {
    Owned,
    Borrowed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReceiverStyle {
    None,
    Ref,
    MutRef,
    Owned,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingSemantics {
    Value,
    MutatesSelf,
    MutatesSelfWithValue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindingKind {
    Method,
    Getter,
    Setter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypeShape {
    Unit,
    Scalar,
    String,
    ByteArray,
    Array,
    Option,
    Except,
    Prod,
    Named,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TypeBinding {
    pub rust: String,
    pub lean: Option<String>,
    pub shape: TypeShape,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParameterBinding {
    pub name: String,
    pub ty: TypeBinding,
    pub passing: PassingStyle,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FunctionBinding {
    pub rust_name: String,
    pub lean_name: String,
    pub owner: Option<String>,
    pub ffi_symbol: String,
    pub receiver: ReceiverStyle,
    pub params: Vec<ParameterBinding>,
    pub return_type: TypeBinding,
    pub semantics: BindingSemantics,
    pub kind: BindingKind,
    pub lean_decl: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassTypeBinding {
    pub rust_name: String,
    pub lean_name: String,
    pub opaque_decl: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassImplBinding {
    pub class_name: String,
    pub methods: Vec<FunctionBinding>,
    pub methods_decl: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubmoduleBinding {
    pub path: String,
    pub exports: Vec<FunctionBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleBinding {
    pub name: String,
    pub exports: Vec<FunctionBinding>,
    pub submodules: Vec<SubmoduleBinding>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClassMetadata {
    pub schema_version: u32,
    pub rust_name: String,
    pub lean_name: String,
    pub opaque_decl: String,
    pub methods_decl: String,
    pub methods: Vec<FunctionBinding>,
}

#[derive(Default)]
pub struct FunctionOptions {
    pub lean_name: Option<String>,
}
