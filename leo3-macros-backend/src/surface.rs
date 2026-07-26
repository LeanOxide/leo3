//! Shared Leo3 semantic analysis for downstream consumers.
//!
//! This module centralizes the Rust-to-Lean surface rules used by Leo3 macro
//! consumers so tools like `lend` do not need to duplicate attribute parsing
//! or type-lowering policy.

use crate::leanclass::LeanClassOptions;
use crate::leanfn::LeanFunctionOptions;
use quote::ToTokens;
use std::collections::BTreeSet;
use syn::{Attribute, ImplItem, Item, ItemFn, ItemImpl, ItemStruct, Type};

/// A generated Lean declaration block sourced from Leo3 semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeanDeclBlock {
    /// Human-readable item name used in diagnostics.
    pub item_name: String,
    /// Optional doc comment text.
    pub doc: Option<String>,
    /// Lean declaration lines to emit verbatim.
    pub lines: Vec<String>,
    /// Custom Lean type names referenced by this declaration block.
    pub custom_types: BTreeSet<String>,
}

/// A discovered Leo3 item that could not be lowered safely.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedItem {
    /// Lean-facing binding or item name.
    pub item_name: String,
    /// Human-readable skip reason.
    pub reason: String,
}

/// Non-fatal semantic diagnostic discovered while scanning Leo3 items.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticDiagnostic {
    /// Associated item name.
    pub item_name: String,
    /// Human-readable diagnostic message.
    pub message: String,
}

/// Aggregated scan results for a Rust item tree.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ScanResult {
    /// Generated declaration blocks.
    pub declarations: Vec<LeanDeclBlock>,
    /// Leo3 items skipped because they could not be lowered.
    pub skipped: Vec<SkippedItem>,
    /// Non-fatal diagnostics.
    pub diagnostics: Vec<SemanticDiagnostic>,
}

/// Collect Lean-visible export names from a `#[leanmodule]` body.
pub fn collect_module_exports(items: &[Item]) -> syn::Result<Vec<String>> {
    let mut exports = Vec::new();

    for item in items {
        let Item::Fn(function) = item else {
            continue;
        };

        for attr in &function.attrs {
            if !is_leanfn_attr(attr) {
                continue;
            }

            let options = parse_leanfn_options(attr)?;
            let name = options
                .common
                .name
                .as_ref()
                .map(|value| value.value())
                .unwrap_or_else(|| function.sig.ident.to_string());
            exports.push(name);
        }
    }

    Ok(exports)
}

/// Scan a Rust item tree for Leo3-generated Lean declarations.
pub fn scan_items(items: &[Item]) -> ScanResult {
    let mut result = ScanResult::default();
    scan_items_into(items, &mut result);
    result
}

fn scan_items_into(items: &[Item], result: &mut ScanResult) {
    for item in items {
        match item {
            Item::Fn(function) => {
                if let Some(attr) = function.attrs.iter().find(|attr| is_leanfn_attr(attr)) {
                    match render_leanfn_block(function, attr) {
                        Ok(block) => result.declarations.push(block),
                        Err(reason) => result.skipped.push(SkippedItem {
                            item_name: function.sig.ident.to_string(),
                            reason,
                        }),
                    }
                }
            }
            Item::Struct(item_struct) => {
                if item_struct
                    .attrs
                    .iter()
                    .any(|attr| matches_attr_path(attr, "leanclass"))
                {
                    match render_leanclass_struct_block(item_struct) {
                        Ok(block) => result.declarations.push(block),
                        Err(message) => result.diagnostics.push(SemanticDiagnostic {
                            item_name: item_struct.ident.to_string(),
                            message,
                        }),
                    }
                }
            }
            Item::Impl(item_impl) => {
                if item_impl
                    .attrs
                    .iter()
                    .any(|attr| matches_attr_path(attr, "leanclass"))
                {
                    match render_leanclass_impl_block(item_impl) {
                        Ok(block) => result.declarations.push(block),
                        Err(message) => result.diagnostics.push(SemanticDiagnostic {
                            item_name: impl_item_name(item_impl),
                            message,
                        }),
                    }
                }
            }
            Item::Mod(item_mod) => {
                if let Some((_, nested)) = &item_mod.content {
                    scan_items_into(nested, result);
                }
            }
            _ => {}
        }
    }
}

fn render_leanfn_block(function: &ItemFn, attr: &Attribute) -> Result<LeanDeclBlock, String> {
    let options = parse_leanfn_options(attr)
        .map_err(|err| format!("ignoring malformed #[leanfn(...)] attribute: {err}"))?;

    if !function.sig.generics.params.is_empty() {
        return Err(
            "skipping generic binding; generic exported functions are not supported".into(),
        );
    }

    if function.sig.variadic.is_some() {
        return Err("skipping variadic binding; variadic functions are not supported".into());
    }

    let lean_name = options
        .common
        .name
        .as_ref()
        .map(|value| value.value())
        .unwrap_or_else(|| function.sig.ident.to_string());

    let mut custom_types = BTreeSet::new();
    let mut params = Vec::new();
    for input in &function.sig.inputs {
        match input {
            syn::FnArg::Typed(pat_type) => {
                let rendered =
                    render_param_type(&pat_type.ty, None, &mut custom_types).map_err(|err| {
                        format!("unsupported leo3 type in `{lean_name}` parameter: {err}")
                    })?;
                params.push(rendered);
            }
            syn::FnArg::Receiver(_) => {
                return Err(
                    "skipping binding with self receiver; only free functions are supported".into(),
                );
            }
        }
    }

    let return_type = match &function.sig.output {
        syn::ReturnType::Default => "Unit".to_string(),
        syn::ReturnType::Type(_, ty) => render_value_type(ty, None, &mut custom_types)
            .map_err(|err| format!("unsupported leo3 return type in `{lean_name}`: {err}"))?,
    };

    let mut signature_parts = params;
    signature_parts.push(return_type);

    Ok(LeanDeclBlock {
        item_name: lean_name.clone(),
        doc: extract_doc_comments(&function.attrs),
        lines: vec![
            format!("@[extern \"{lean_name}\"]"),
            format!("opaque {lean_name} : {}", signature_parts.join(" → ")),
        ],
        custom_types,
    })
}

fn render_leanclass_struct_block(item_struct: &ItemStruct) -> Result<LeanDeclBlock, String> {
    let attr = item_struct
        .attrs
        .iter()
        .find(|attr| matches_attr_path(attr, "leanclass"))
        .ok_or_else(|| "missing #[leanclass] attribute".to_string())?;
    let _ = parse_leanclass_options(attr)
        .map_err(|err| format!("ignoring malformed #[leanclass(...)] attribute: {err}"))?;

    if !item_struct.generics.params.is_empty() {
        return Err("#[leanclass] does not support generic types yet".into());
    }

    let struct_name = item_struct.ident.to_string();
    Ok(LeanDeclBlock {
        item_name: struct_name.clone(),
        doc: extract_doc_comments(&item_struct.attrs),
        lines: vec![format!("opaque {struct_name} : Type")],
        custom_types: BTreeSet::new(),
    })
}

fn render_leanclass_impl_block(item_impl: &ItemImpl) -> Result<LeanDeclBlock, String> {
    let attr = item_impl
        .attrs
        .iter()
        .find(|attr| matches_attr_path(attr, "leanclass"))
        .ok_or_else(|| "missing #[leanclass] attribute".to_string())?;
    let _ = parse_leanclass_options(attr)
        .map_err(|err| format!("ignoring malformed #[leanclass(...)] attribute: {err}"))?;

    if !item_impl.generics.params.is_empty() {
        return Err("#[leanclass] does not support generic impl blocks yet".into());
    }

    let struct_name = impl_item_name(item_impl);
    let mut custom_types = BTreeSet::new();
    let mut lines = Vec::new();

    for item in &item_impl.items {
        let ImplItem::Fn(method) = item else {
            continue;
        };

        if !method.sig.generics.params.is_empty() {
            return Err(format!(
                "unsupported #[leanclass] method `{}`; generic methods are not supported yet",
                method.sig.ident
            ));
        }

        let mut parts = Vec::new();
        let mut iter = method.sig.inputs.iter();
        let mut receiver_kind = ReceiverKind::None;

        if let Some(first) = iter.next() {
            match first {
                syn::FnArg::Receiver(recv) => {
                    receiver_kind = if recv.mutability.is_some() {
                        ReceiverKind::MutRef
                    } else if recv.reference.is_some() {
                        ReceiverKind::Ref
                    } else {
                        ReceiverKind::Owned
                    };
                    parts.push(struct_name.clone());
                }
                syn::FnArg::Typed(pat_type) => {
                    parts.push(render_param_type(
                        &pat_type.ty,
                        Some(struct_name.as_str()),
                        &mut custom_types,
                    )?);
                }
            }
        }

        for input in iter {
            match input {
                syn::FnArg::Typed(pat_type) => parts.push(render_param_type(
                    &pat_type.ty,
                    Some(struct_name.as_str()),
                    &mut custom_types,
                )?),
                syn::FnArg::Receiver(_) => {
                    return Err(format!(
                        "unsupported #[leanclass] method `{}`; only a leading receiver is supported",
                        method.sig.ident
                    ));
                }
            }
        }

        let return_ty = match &method.sig.output {
            syn::ReturnType::Default => "Unit".to_string(),
            syn::ReturnType::Type(_, ty) => {
                render_value_type(ty, Some(struct_name.as_str()), &mut custom_types)?
            }
        };

        let return_ty = match receiver_kind {
            ReceiverKind::MutRef if return_ty == "Unit" => struct_name.clone(),
            ReceiverKind::MutRef => {
                format!("Prod {} {}", struct_name, lean_type_arg(&return_ty))
            }
            _ => return_ty,
        };
        parts.push(return_ty);

        let ffi_name = format!("__lean_ffi_{}_{}", struct_name, method.sig.ident);
        let lean_name = format!("{}.{}", struct_name, method.sig.ident);
        lines.push(format!(
            "@[extern \"{ffi_name}\"] opaque {lean_name} : {}",
            parts.join(" → ")
        ));
    }

    Ok(LeanDeclBlock {
        item_name: struct_name,
        doc: None,
        lines,
        custom_types,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ReceiverKind {
    None,
    Ref,
    MutRef,
    Owned,
}

fn render_param_type(
    ty: &Type,
    self_type: Option<&str>,
    custom_types: &mut BTreeSet<String>,
) -> Result<String, String> {
    match ty {
        Type::Reference(reference) => {
            if reference.mutability.is_some() {
                return Err(
                    "unsupported leo3 type `&mut ...`; mutable borrowed inputs are not supported yet"
                        .into(),
                );
            }

            match &*reference.elem {
                Type::Path(path) if path_is_str(path) => Ok("String".into()),
                Type::Path(path) if path_is_string(path) => Ok("String".into()),
                Type::Slice(slice) if is_u8_type(&slice.elem) => Ok("ByteArray".into()),
                Type::Slice(slice) => {
                    let inner = render_value_type(&slice.elem, self_type, custom_types)?;
                    Ok(format!("Array {}", lean_type_arg(&inner)))
                }
                Type::Array(array) => {
                    let inner = render_value_type(&array.elem, self_type, custom_types)?;
                    Ok(format!("Array {}", lean_type_arg(&inner)))
                }
                Type::Path(path) if path_simple_name(path).as_deref() == Some("Vec") => {
                    let last = path.path.segments.last().ok_or_else(|| {
                        format!("unsupported leo3 borrowed type `{}`", render_rust_type(ty))
                    })?;
                    let args = extract_type_args(last, 1)?;
                    if is_u8_type(args[0]) {
                        Ok("ByteArray".into())
                    } else {
                        let inner = render_value_type(args[0], self_type, custom_types)?;
                        Ok(format!("Array {}", lean_type_arg(&inner)))
                    }
                }
                _ => Err(format!(
                    "unsupported leo3 borrowed type `{}`; currently supported borrowed shapes are `&str`, `&String`, borrowed Vec containers like `&Vec<T>`, borrowed slices/arrays like `&[T]`/`&[T; N]`, and borrowed bytes `&[u8]`",
                    render_rust_type(ty)
                )),
            }
        }
        _ => render_value_type(ty, self_type, custom_types),
    }
}

fn render_value_type(
    ty: &Type,
    self_type: Option<&str>,
    custom_types: &mut BTreeSet<String>,
) -> Result<String, String> {
    match ty {
        Type::Paren(paren) => render_value_type(&paren.elem, self_type, custom_types),
        Type::Group(group) => render_value_type(&group.elem, self_type, custom_types),
        Type::Tuple(tuple) if tuple.elems.is_empty() => Ok("Unit".into()),
        Type::Tuple(tuple) => render_tuple_type(tuple.elems.iter(), self_type, custom_types),
        Type::Path(path) => render_path_type(path, self_type, custom_types),
        Type::Reference(_) => render_param_type(ty, self_type, custom_types),
        Type::Array(array) => {
            let inner = render_value_type(&array.elem, self_type, custom_types)?;
            Ok(format!("Array {}", lean_type_arg(&inner)))
        }
        Type::Ptr(_) => Err(format!(
            "unsupported leo3 type `{}`; raw pointers are out of scope for leo3 mode",
            render_rust_type(ty)
        )),
        Type::Slice(_) => Err(format!(
            "unsupported leo3 type `{}`; slice values are only supported as borrowed `&[u8]`, not as standalone slice types",
            render_rust_type(ty)
        )),
        _ => Err(format!("unsupported leo3 type `{}`", render_rust_type(ty))),
    }
}

fn render_tuple_type<'a>(
    items: impl Iterator<Item = &'a Type>,
    self_type: Option<&str>,
    custom_types: &mut BTreeSet<String>,
) -> Result<String, String> {
    let items = items.collect::<Vec<_>>();
    if items.len() < 2 {
        return Err("tuple Lean declarations require at least two elements".to_string());
    }

    let left = render_value_type(items[0], self_type, custom_types)?;
    let right = if items.len() == 2 {
        render_value_type(items[1], self_type, custom_types)?
    } else {
        render_tuple_type(items[1..].iter().copied(), self_type, custom_types)?
    };

    Ok(format!(
        "Prod {} {}",
        lean_type_arg(&left),
        lean_type_arg(&right)
    ))
}

fn render_path_type(
    path: &syn::TypePath,
    self_type: Option<&str>,
    custom_types: &mut BTreeSet<String>,
) -> Result<String, String> {
    let Some(last) = path.path.segments.last() else {
        return Err(format!(
            "unsupported leo3 type `{}`",
            render_rust_type(&Type::Path(path.clone()))
        ));
    };
    let ident = last.ident.to_string();

    match ident.as_str() {
        "bool" => Ok("Bool".into()),
        "u8" => Ok("UInt8".into()),
        "u16" => Ok("UInt16".into()),
        "u32" => Ok("UInt32".into()),
        "u64" => Ok("UInt64".into()),
        "usize" => Ok("USize".into()),
        "i8" => Ok("Int8".into()),
        "i16" => Ok("Int16".into()),
        "i32" => Ok("Int32".into()),
        "i64" => Ok("Int64".into()),
        "isize" => Ok("ISize".into()),
        "f32" => Ok("Float32".into()),
        "f64" => Ok("Float".into()),
        "char" => Ok("Char".into()),
        "String" if path_is_string(path) => Ok("String".into()),
        "str" if path_is_str(path) => Err("unsupported leo3 type `str`; use owned `String`".into()),
        "Self" => self_type
            .map(str::to_string)
            .ok_or_else(|| "unsupported leo3 type `Self` outside `#[leanclass]` methods".into()),
        "Vec" => {
            let args = extract_type_args(last, 1)?;
            if is_u8_type(args[0]) {
                Ok("ByteArray".into())
            } else {
                let inner = render_value_type(args[0], self_type, custom_types)?;
                Ok(format!("Array {}", lean_type_arg(&inner)))
            }
        }
        "Option" => {
            let args = extract_type_args(last, 1)?;
            let inner = render_value_type(args[0], self_type, custom_types)?;
            Ok(format!("Option {}", lean_type_arg(&inner)))
        }
        "Result" => {
            let args = extract_type_args(last, 2)?;
            let ok = render_value_type(args[0], self_type, custom_types)?;
            let err = render_value_type(args[1], self_type, custom_types)?;
            Ok(format!(
                "Except {} {}",
                lean_type_arg(&err),
                lean_type_arg(&ok)
            ))
        }
        _ => {
            if !matches!(last.arguments, syn::PathArguments::None) {
                return Err(format!(
                    "unsupported leo3 generic type `{}`; only Vec<T>, Option<T>, and Result<T, E> are supported",
                    render_rust_type(&Type::Path(path.clone()))
                ));
            }

            let simple = ident;
            custom_types.insert(simple.clone());
            Ok(simple)
        }
    }
}

fn extract_type_args(segment: &syn::PathSegment, expected: usize) -> Result<Vec<&Type>, String> {
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return Err(format!(
            "{} requires exactly {} type argument(s)",
            segment.ident, expected
        ));
    };

    let tys = args
        .args
        .iter()
        .filter_map(|arg| match arg {
            syn::GenericArgument::Type(ty) => Some(ty),
            _ => None,
        })
        .collect::<Vec<_>>();

    if tys.len() != expected {
        return Err(format!(
            "{} requires exactly {} type argument(s)",
            segment.ident, expected
        ));
    }

    Ok(tys)
}

fn is_leanfn_attr(attr: &Attribute) -> bool {
    matches_attr_path(attr, "leanfn")
}

fn parse_leanfn_options(attr: &Attribute) -> syn::Result<LeanFunctionOptions> {
    match &attr.meta {
        syn::Meta::Path(_) => Ok(LeanFunctionOptions {
            common: crate::CommonOptions::default(),
        }),
        _ => attr.parse_args::<LeanFunctionOptions>(),
    }
}

fn parse_leanclass_options(attr: &Attribute) -> syn::Result<LeanClassOptions> {
    match &attr.meta {
        syn::Meta::Path(_) => Ok(LeanClassOptions {
            common: crate::CommonOptions::default(),
        }),
        _ => attr.parse_args::<LeanClassOptions>(),
    }
}

fn matches_attr_path(attr: &Attribute, ident: &str) -> bool {
    let segments = attr.path().segments.iter().collect::<Vec<_>>();
    match segments.as_slice() {
        [single] => single.ident == ident,
        [prefix, terminal] => prefix.ident == "leo3" && terminal.ident == ident,
        _ => false,
    }
}

fn path_simple_name(path: &syn::TypePath) -> Option<String> {
    path.path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn path_is_string(path: &syn::TypePath) -> bool {
    let segments = path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect::<Vec<_>>();

    match segments.as_slice() {
        [single] => single == "String",
        [std, string, name] => {
            (std == "std" || std == "alloc") && string == "string" && name == "String"
        }
        _ => false,
    }
}

fn path_is_str(path: &syn::TypePath) -> bool {
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "str")
}

fn is_u8_type(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Path(path) if path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "u8")
    )
}

fn impl_item_name(item_impl: &ItemImpl) -> String {
    match &*item_impl.self_ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_else(|| "impl".to_string()),
        _ => "impl".to_string(),
    }
}

fn extract_doc_comments(attrs: &[Attribute]) -> Option<String> {
    let docs = attrs
        .iter()
        .filter(|attr| attr.path().is_ident("doc"))
        .filter_map(|attr| match &attr.meta {
            syn::Meta::NameValue(name_value) => match &name_value.value {
                syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(value),
                    ..
                }) => Some(value.value().trim().to_string()),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n"))
    }
}

fn render_rust_type(ty: &Type) -> String {
    ty.to_token_stream().to_string().replace(" ,", ",")
}

fn lean_type_arg(ty: &str) -> String {
    if ty.contains(' ') {
        format!("({ty})")
    } else {
        ty.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_items(source: &str) -> Vec<Item> {
        syn::parse_file(source).expect("source should parse").items
    }

    #[test]
    fn scan_items_accepts_borrowed_owned_container_aliases() {
        let scan = scan_items(&parse_items(
            r#"
#[leanfn]
fn borrowed_string_len(value: &String) -> usize {
    value.len()
}

#[leanfn]
fn borrowed_vec_u8_len(values: &Vec<u8>) -> usize {
    values.len()
}

#[leanfn]
fn borrowed_vec_u64_len(values: &Vec<u64>) -> usize {
    values.len()
}
"#,
        ));

        assert!(scan.skipped.is_empty());
        assert!(scan.diagnostics.is_empty());
        assert!(scan.declarations.iter().any(|block| {
            block
                .lines
                .iter()
                .any(|line| line == "opaque borrowed_string_len : String → USize")
        }));
        assert!(scan.declarations.iter().any(|block| {
            block
                .lines
                .iter()
                .any(|line| line == "opaque borrowed_vec_u8_len : ByteArray → USize")
        }));
        assert!(scan.declarations.iter().any(|block| {
            block
                .lines
                .iter()
                .any(|line| line == "opaque borrowed_vec_u64_len : Array UInt64 → USize")
        }));
    }

    #[test]
    fn scan_items_accepts_option_of_borrowed_owned_container_aliases() {
        let scan = scan_items(&parse_items(
            r#"
#[leanfn]
fn maybe_name(value: Option<&String>) -> Option<String> {
    value.cloned()
}

#[leanfn]
fn maybe_bytes(value: Option<&Vec<u8>>) -> u64 {
    value.map(|bytes| bytes.len() as u64).unwrap_or(0)
}

#[leanfn]
fn maybe_words(value: Option<&Vec<u64>>) -> u64 {
    value.map(|words| words.iter().sum()).unwrap_or(0)
}
"#,
        ));

        assert!(scan.skipped.is_empty());
        assert!(scan.diagnostics.is_empty());
        assert!(scan.declarations.iter().any(|block| {
            block
                .lines
                .iter()
                .any(|line| line == "opaque maybe_name : Option String → Option String")
        }));
        assert!(scan.declarations.iter().any(|block| {
            block
                .lines
                .iter()
                .any(|line| line == "opaque maybe_bytes : Option ByteArray → UInt64")
        }));
        assert!(scan.declarations.iter().any(|block| {
            block
                .lines
                .iter()
                .any(|line| line == "opaque maybe_words : Option (Array UInt64) → UInt64")
        }));
    }

    #[test]
    fn scan_items_accepts_result_of_borrowed_owned_container_aliases() {
        let scan = scan_items(&parse_items(
            r#"
#[leanfn]
fn result_name(value: Result<&String, &String>) -> u64 {
    match value {
        Ok(name) => name.len() as u64,
        Err(err) => err.len() as u64,
    }
}

#[leanfn]
fn result_bytes(value: Result<&Vec<u8>, &String>) -> u64 {
    match value {
        Ok(bytes) => bytes.len() as u64,
        Err(err) => err.len() as u64,
    }
}

#[leanfn]
fn result_words(value: Result<&Vec<u64>, &String>) -> u64 {
    match value {
        Ok(words) => words.iter().sum(),
        Err(err) => err.len() as u64,
    }
}
"#,
        ));

        assert!(scan.skipped.is_empty());
        assert!(scan.diagnostics.is_empty());
        assert!(scan.declarations.iter().any(|block| {
            block
                .lines
                .iter()
                .any(|line| line == "opaque result_name : Except String String → UInt64")
        }));
        assert!(scan.declarations.iter().any(|block| {
            block
                .lines
                .iter()
                .any(|line| line == "opaque result_bytes : Except String ByteArray → UInt64")
        }));
        assert!(scan.declarations.iter().any(|block| {
            block
                .lines
                .iter()
                .any(|line| line == "opaque result_words : Except String (Array UInt64) → UInt64")
        }));
    }

    #[test]
    fn scan_items_accepts_tuple_of_borrowed_owned_container_aliases() {
        let scan = scan_items(&parse_items(
            r#"
#[leanfn]
fn tuple_aliases(value: (&String, &Vec<u8>, &Vec<u64>)) -> u64 {
    value.0.len() as u64 + value.1.len() as u64 + value.2.iter().sum::<u64>()
}
"#,
        ));

        assert!(scan.skipped.is_empty());
        assert!(scan.diagnostics.is_empty());
        assert!(scan.declarations.iter().any(|block| {
            block.lines.iter().any(|line| {
                line
                    == "opaque tuple_aliases : Prod String (Prod ByteArray (Array UInt64)) → UInt64"
            })
        }));
    }

    #[test]
    fn scan_items_accepts_nested_tuple_of_borrowed_owned_container_aliases() {
        let scan = scan_items(&parse_items(
            r#"
#[leanfn]
fn nested_tuple_aliases(value: ((&String, &Vec<u8>), &Vec<u64>)) -> u64 {
    value.0.0.len() as u64 + value.0.1.len() as u64 + value.1.iter().sum::<u64>()
}
"#,
        ));

        assert!(scan.skipped.is_empty());
        assert!(scan.diagnostics.is_empty());
        assert!(scan.declarations.iter().any(|block| {
            block.lines.iter().any(|line| {
                line
                    == "opaque nested_tuple_aliases : Prod (Prod String ByteArray) (Array UInt64) → UInt64"
            })
        }));
    }

    #[test]
    fn scan_items_accepts_option_of_result_borrowed_aliases() {
        let scan = scan_items(&parse_items(
            r#"
#[leanfn]
fn nested_names(value: Option<Result<&String, &String>>) -> u64 {
    match value {
        Some(Ok(name)) => name.len() as u64,
        Some(Err(err)) => err.len() as u64,
        None => 0,
    }
}

#[leanfn]
fn nested_bytes(value: Option<Result<&Vec<u8>, &String>>) -> u64 {
    match value {
        Some(Ok(bytes)) => bytes.len() as u64,
        Some(Err(err)) => err.len() as u64,
        None => 0,
    }
}

#[leanfn]
fn nested_words(value: Option<Result<Vec<u64>, &String>>) -> u64 {
    match value {
        Some(Ok(words)) => words.iter().sum(),
        Some(Err(err)) => err.len() as u64,
        None => 0,
    }
}
"#,
        ));

        assert!(scan.skipped.is_empty());
        assert!(scan.diagnostics.is_empty());
        assert!(scan.declarations.iter().any(|block| {
            block
                .lines
                .iter()
                .any(|line| line == "opaque nested_names : Option (Except String String) → UInt64")
        }));
        assert!(scan.declarations.iter().any(|block| {
            block.lines.iter().any(|line| {
                line == "opaque nested_bytes : Option (Except String ByteArray) → UInt64"
            })
        }));
        assert!(scan.declarations.iter().any(|block| {
            block.lines.iter().any(|line| {
                line == "opaque nested_words : Option (Except String (Array UInt64)) → UInt64"
            })
        }));
    }
}
