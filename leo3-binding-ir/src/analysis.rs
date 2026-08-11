use quote::ToTokens;
use syn::parse::Parse;
use syn::{punctuated::Punctuated, Token};

use crate::model::*;

pub struct ConcreteAttr {
    pub types: Vec<syn::Type>,
    pub name: String,
}

struct ConcreteArgsParser {
    types: Vec<syn::Type>,
    name: Option<String>,
}

impl Parse for ConcreteArgsParser {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut types = Vec::new();
        let mut name = None;

        while !input.is_empty() {
            if input.peek(syn::Ident) && input.peek2(Token![=]) {
                let ident: syn::Ident = input.parse()?;
                if ident != "name" {
                    return Err(syn::Error::new(ident.span(), "expected `name`"));
                }
                let _: Token![=] = input.parse()?;
                let lit: syn::LitStr = input.parse()?;
                name = Some(lit.value());
            } else {
                let ty: syn::Type = input.parse()?;
                types.push(ty);
            }

            if !input.is_empty() {
                let _: Token![,] = input.parse()?;
            }
        }

        Ok(ConcreteArgsParser { types, name })
    }
}

fn parse_concrete_meta(list: &syn::MetaList) -> syn::Result<ConcreteAttr> {
    let args: ConcreteArgsParser = list.parse_args()?;
    let name = args
        .name
        .ok_or_else(|| syn::Error::new_spanned(list, "`concrete` requires `name = \"...\"`"))?;
    if args.types.is_empty() {
        return Err(syn::Error::new_spanned(
            list,
            "`concrete` requires at least one type argument",
        ));
    }
    Ok(ConcreteAttr {
        types: args.types,
        name,
    })
}

#[derive(Default)]
struct CommonAttrOptions {
    name: Option<String>,
    concretes: Vec<ConcreteAttr>,
}

impl Parse for CommonAttrOptions {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let metas: Punctuated<syn::Meta, Token![,]> =
            input.parse_terminated(syn::Meta::parse, Token![,])?;
        let mut options = Self::default();

        for meta in metas {
            match meta {
                syn::Meta::NameValue(nv) if nv.path.is_ident("name") => {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(value),
                        ..
                    }) = nv.value
                    {
                        options.name = Some(value.value());
                    }
                }
                syn::Meta::NameValue(nv) if nv.path.is_ident("crate") => {}
                syn::Meta::List(list) if list.path.is_ident("concrete") => {
                    options.concretes.push(parse_concrete_meta(&list)?);
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        other,
                        "Expected name-value attribute like `name = \"...\"`",
                    ))
                }
            }
        }

        Ok(options)
    }
}

pub fn is_leanfn_attr(attr: &syn::Attribute) -> bool {
    attr.path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "leanfn")
}

pub fn substitute_type(
    ty: &syn::Type,
    mapping: &std::collections::HashMap<String, syn::Type>,
) -> syn::Type {
    match ty {
        syn::Type::Path(type_path) if type_path.qself.is_none() => {
            if type_path.path.segments.len() == 1 {
                let segment = &type_path.path.segments[0];
                if matches!(segment.arguments, syn::PathArguments::None) {
                    if let Some(concrete) = mapping.get(&segment.ident.to_string()) {
                        return concrete.clone();
                    }
                }
            }
            let mut new_path = type_path.path.clone();
            for segment in new_path.segments.iter_mut() {
                if let syn::PathArguments::AngleBracketed(args) = &mut segment.arguments {
                    for arg in args.args.iter_mut() {
                        if let syn::GenericArgument::Type(inner) = arg {
                            *inner = substitute_type(inner, mapping);
                        }
                    }
                }
            }
            syn::Type::Path(syn::TypePath {
                qself: None,
                path: new_path,
            })
        }
        syn::Type::Reference(reference) => syn::Type::Reference(syn::TypeReference {
            and_token: reference.and_token,
            lifetime: reference.lifetime.clone(),
            mutability: reference.mutability,
            elem: Box::new(substitute_type(&reference.elem, mapping)),
        }),
        syn::Type::Tuple(tuple) => syn::Type::Tuple(syn::TypeTuple {
            paren_token: tuple.paren_token,
            elems: tuple
                .elems
                .iter()
                .map(|e| substitute_type(e, mapping))
                .collect(),
        }),
        syn::Type::Array(array) => syn::Type::Array(syn::TypeArray {
            bracket_token: array.bracket_token,
            elem: Box::new(substitute_type(&array.elem, mapping)),
            semi_token: array.semi_token,
            len: array.len.clone(),
        }),
        syn::Type::Slice(slice) => syn::Type::Slice(syn::TypeSlice {
            bracket_token: slice.bracket_token,
            elem: Box::new(substitute_type(&slice.elem, mapping)),
        }),
        syn::Type::Paren(paren) => syn::Type::Paren(syn::TypeParen {
            paren_token: paren.paren_token,
            elem: Box::new(substitute_type(&paren.elem, mapping)),
        }),
        syn::Type::Group(group) => syn::Type::Group(syn::TypeGroup {
            group_token: group.group_token,
            elem: Box::new(substitute_type(&group.elem, mapping)),
        }),
        _ => ty.clone(),
    }
}

fn generic_type_param_names(func: &syn::ItemFn) -> syn::Result<Vec<String>> {
    func.sig
        .generics
        .params
        .iter()
        .map(|p| match p {
            syn::GenericParam::Type(tp) => Ok(tp.ident.to_string()),
            syn::GenericParam::Lifetime(lt) => Err(syn::Error::new_spanned(
                lt,
                "lifetime parameters are not supported with `concrete`",
            )),
            syn::GenericParam::Const(cp) => Err(syn::Error::new_spanned(
                cp,
                "const parameters are not supported with `concrete`",
            )),
        })
        .collect()
}

pub fn analyze_concrete_instance(
    func: &syn::ItemFn,
    concrete: &ConcreteAttr,
) -> syn::Result<FunctionBinding> {
    let param_names = generic_type_param_names(func)?;

    if param_names.len() != concrete.types.len() {
        return Err(syn::Error::new_spanned(
            func,
            format!(
                "expected {} concrete type(s) for {} generic parameter(s), got {}",
                param_names.len(),
                param_names.len(),
                concrete.types.len()
            ),
        ));
    }

    let mapping: std::collections::HashMap<String, syn::Type> = param_names
        .into_iter()
        .zip(concrete.types.iter().cloned())
        .collect();

    let mut params = Vec::new();
    for input in &func.sig.inputs {
        match input {
            syn::FnArg::Typed(pat_type) => {
                let name = match &*pat_type.pat {
                    syn::Pat::Ident(ident) => ident.ident.to_string(),
                    _ => {
                        return Err(syn::Error::new_spanned(
                            pat_type,
                            "Only simple parameter patterns are supported",
                        ))
                    }
                };
                let ty = substitute_type(&pat_type.ty, &mapping);
                params.push(ParameterBinding {
                    name,
                    ty: analyze_leanfn_type(&ty)?,
                    passing: passing_style_for_leanfn(&ty),
                });
            }
            syn::FnArg::Receiver(_) => {
                return Err(syn::Error::new_spanned(
                    input,
                    "Methods with `self` are not supported. Use standalone functions.",
                ))
            }
        }
    }

    let return_type = match &func.sig.output {
        syn::ReturnType::Default => unit_binding(),
        syn::ReturnType::Type(_, ty) => analyze_leanfn_type(&substitute_type(ty, &mapping))?,
    };

    Ok(FunctionBinding {
        rust_name: func.sig.ident.to_string(),
        lean_name: concrete.name.clone(),
        owner: None,
        ffi_symbol: concrete.name.clone(),
        receiver: ReceiverStyle::None,
        params,
        return_type,
        semantics: BindingSemantics::Value,
        kind: BindingKind::Method,
        lean_decl: None,
    })
}

pub fn collect_module_exports(items: &[syn::Item]) -> syn::Result<Vec<FunctionBinding>> {
    let mut exports = Vec::new();

    for item in items {
        let syn::Item::Fn(function) = item else {
            continue;
        };

        for attr in &function.attrs {
            if !is_leanfn_attr(attr) {
                continue;
            }

            let options = attr.parse_args::<CommonAttrOptions>()?;

            if options.concretes.is_empty() {
                exports.push(analyze_lean_function(
                    function,
                    FunctionOptions {
                        lean_name: options.name,
                    },
                )?);
            } else {
                for concrete in &options.concretes {
                    exports.push(analyze_concrete_instance(function, concrete)?);
                }
            }
        }
    }

    Ok(exports)
}

pub fn collect_submodule_exports(
    items: &[syn::Item],
    prefix: &str,
) -> syn::Result<Vec<SubmoduleBinding>> {
    let mut submodules = Vec::new();

    for item in items {
        let syn::Item::Mod(module) = item else {
            continue;
        };

        let Some((_, sub_items)) = &module.content else {
            continue;
        };

        let path = if prefix.is_empty() {
            module.ident.to_string()
        } else {
            format!("{}.{}", prefix, module.ident)
        };

        let exports = collect_module_exports(sub_items)?;
        if !exports.is_empty() {
            submodules.push(SubmoduleBinding {
                path: path.clone(),
                exports,
            });
        }

        submodules.extend(collect_submodule_exports(sub_items, &path)?);
    }

    Ok(submodules)
}

pub fn filter_exports(
    exports: Vec<FunctionBinding>,
    allowed: &[String],
) -> syn::Result<Vec<FunctionBinding>> {
    let mut result = Vec::new();

    for name in allowed {
        let found = exports
            .iter()
            .find(|e| e.lean_name == *name || e.rust_name == *name)
            .ok_or_else(|| {
                syn::Error::new(
                    proc_macro2::Span::call_site(),
                    format!("export `{}` not found in module", name),
                )
            })?;
        result.push(found.clone());
    }

    Ok(result)
}

pub fn analyze_lean_function(
    func: &syn::ItemFn,
    options: FunctionOptions,
) -> syn::Result<FunctionBinding> {
    let rust_name = func.sig.ident.to_string();
    let lean_name = options.lean_name.unwrap_or_else(|| rust_name.clone());

    if !func.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &func.sig.generics,
            "Generic functions are not supported yet",
        ));
    }

    let mut params = Vec::new();
    for input in &func.sig.inputs {
        match input {
            syn::FnArg::Typed(pat_type) => {
                let name = match &*pat_type.pat {
                    syn::Pat::Ident(ident) => ident.ident.to_string(),
                    _ => {
                        return Err(syn::Error::new_spanned(
                            pat_type,
                            "Only simple parameter patterns are supported",
                        ))
                    }
                };
                params.push(ParameterBinding {
                    name,
                    ty: analyze_leanfn_type(&pat_type.ty)?,
                    passing: passing_style_for_leanfn(&pat_type.ty),
                });
            }
            syn::FnArg::Receiver(_) => {
                return Err(syn::Error::new_spanned(
                    input,
                    "Methods with `self` are not supported. Use standalone functions.",
                ))
            }
        }
    }

    let return_type = match &func.sig.output {
        syn::ReturnType::Default => unit_binding(),
        syn::ReturnType::Type(_, ty) => analyze_leanfn_type(ty)?,
    };

    Ok(FunctionBinding {
        rust_name,
        lean_name: lean_name.clone(),
        owner: None,
        ffi_symbol: lean_name,
        receiver: ReceiverStyle::None,
        params,
        return_type,
        semantics: BindingSemantics::Value,
        kind: BindingKind::Method,
        lean_decl: None,
    })
}

/// Lean declarations introducing an external class type.
///
/// A bare `opaque Foo : Type` cannot be used: `opaque` declarations require
/// an `Inhabited` or `Nonempty` instance for their type, and every method
/// returning the class (constructors, `&mut self` updaters) would fail to
/// elaborate. The standard library solves the same problem for `IO.RealWorld`
/// by going through `NonemptyType` (a `Subtype` bundling a type with a
/// `Nonempty` proof, whose doc string names exactly this use case), so the
/// generated declarations do the same:
///
/// ```lean
/// opaque Foo.ffi : NonemptyType
/// def Foo : Type := Foo.ffi.val
/// instance : Nonempty Foo := Foo.ffi.property
/// ```
pub fn class_opaque_decl(lean_name: &str) -> String {
    format!(
        "opaque {lean_name}.ffi : NonemptyType\n\
         def {lean_name} : Type := {lean_name}.ffi.val\n\
         instance : Nonempty {lean_name} := {lean_name}.ffi.property"
    )
}

pub fn analyze_lean_class_struct(
    item: &syn::ItemStruct,
    lean_name_override: Option<&str>,
) -> syn::Result<ClassTypeBinding> {
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "#[leanclass] does not support generic types yet",
        ));
    }

    let rust_name = item.ident.to_string();
    let lean_name = lean_name_override.unwrap_or(&rust_name).to_string();
    Ok(ClassTypeBinding {
        rust_name,
        lean_name: lean_name.clone(),
        opaque_decl: class_opaque_decl(&lean_name),
    })
}

/// A `#[get]` / `#[set]` field accessor declared on a `#[leanclass]` struct.
#[derive(Clone, Debug)]
pub struct FieldAccessor {
    pub rust_name: String,
    pub getter: bool,
    pub setter: bool,
    pub ty: TypeBinding,
}

/// Analyze `#[get]` / `#[set]` attributes on a `#[leanclass]` struct's named
/// fields. Field types must be representable in the generated Lean
/// declaration grammar (see `analyze_leanclass_type`).
pub fn analyze_lean_class_field_accessors(
    item: &syn::ItemStruct,
) -> syn::Result<Vec<FieldAccessor>> {
    let class_name = item.ident.to_string();
    let fields = match &item.fields {
        syn::Fields::Named(named) => &named.named,
        syn::Fields::Unit => return Ok(Vec::new()),
        syn::Fields::Unnamed(unnamed) => {
            for field in &unnamed.unnamed {
                if field
                    .attrs
                    .iter()
                    .any(|a| a.path().is_ident("get") || a.path().is_ident("set"))
                {
                    return Err(syn::Error::new_spanned(
                        field,
                        "#[get] / #[set] field accessors require named struct fields",
                    ));
                }
            }
            return Ok(Vec::new());
        }
    };

    let mut accessors = Vec::new();
    for field in fields {
        let getter = field.attrs.iter().any(|a| a.path().is_ident("get"));
        let setter = field.attrs.iter().any(|a| a.path().is_ident("set"));
        if !getter && !setter {
            continue;
        }
        let rust_name = field
            .ident
            .as_ref()
            .ok_or_else(|| syn::Error::new_spanned(field, "#[get] / #[set] require a named field"))?
            .to_string();
        let ty = analyze_leanclass_type(&field.ty, &class_name)?;
        accessors.push(FieldAccessor {
            rust_name,
            getter,
            setter,
            ty,
        });
    }
    Ok(accessors)
}

/// Build `FunctionBinding`s for `#[get]` / `#[set]` field accessors, matching
/// the declaration and metadata shapes the impl-block analyzer produces:
///
/// - getter: `fn field(&self) -> T` — `Class -> T`, kind `Getter`
/// - setter: `fn set_field(&mut self, value: T)` — `Class -> T -> Class`,
///   kind `Setter` (copy-on-write, like `&mut self -> ()` methods)
pub fn field_accessor_bindings(
    class_name: &str,
    accessors: &[FieldAccessor],
) -> Vec<FunctionBinding> {
    let mut bindings = Vec::new();
    for acc in accessors {
        if acc.getter {
            let ffi_symbol = format!("__lean_ffi_{}_{}", class_name, acc.rust_name);
            let lean_name = format!("{}.{}", class_name, acc.rust_name);
            let lean_ty = acc.ty.lean.clone().unwrap_or_else(|| acc.ty.rust.clone());
            let lean_decl = format!(
                "@[extern \"{}\"] opaque {} : {} → {}",
                ffi_symbol, lean_name, class_name, lean_ty
            );
            bindings.push(FunctionBinding {
                rust_name: acc.rust_name.clone(),
                lean_name,
                owner: Some(class_name.to_string()),
                ffi_symbol,
                receiver: ReceiverStyle::Ref,
                params: Vec::new(),
                return_type: acc.ty.clone(),
                semantics: BindingSemantics::Value,
                kind: BindingKind::Getter,
                lean_decl: Some(lean_decl),
            });
        }
        if acc.setter {
            let setter_name = format!("set_{}", acc.rust_name);
            let ffi_symbol = format!("__lean_ffi_{}_{}", class_name, setter_name);
            let lean_name = format!("{}.{}", class_name, setter_name);
            let lean_ty = acc.ty.lean.clone().unwrap_or_else(|| acc.ty.rust.clone());
            let lean_decl = format!(
                "@[extern \"{}\"] opaque {} : {} → {} → {}",
                ffi_symbol, lean_name, class_name, lean_ty, class_name
            );
            bindings.push(FunctionBinding {
                rust_name: setter_name,
                lean_name,
                owner: Some(class_name.to_string()),
                ffi_symbol,
                receiver: ReceiverStyle::MutRef,
                params: vec![ParameterBinding {
                    name: "value".to_string(),
                    ty: acc.ty.clone(),
                    passing: PassingStyle::Owned,
                }],
                return_type: TypeBinding {
                    rust: class_name.to_string(),
                    lean: Some(class_name.to_string()),
                    shape: TypeShape::Named,
                },
                semantics: BindingSemantics::MutatesSelf,
                kind: BindingKind::Setter,
                lean_decl: Some(lean_decl),
            });
        }
    }
    bindings
}

pub fn analyze_lean_class_impl(
    item: &syn::ItemImpl,
    lean_class_name: Option<&str>,
) -> syn::Result<ClassImplBinding> {
    let class_name = class_name_from_self_ty(&item.self_ty)?;
    let lean_class_name = lean_class_name.unwrap_or(&class_name);

    if !item.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "#[leanclass] does not support generic impl blocks yet",
        ));
    }

    let mut methods = Vec::new();
    for impl_item in &item.items {
        if let syn::ImplItem::Fn(method) = impl_item {
            methods.push(analyze_lean_class_method(
                method,
                &class_name,
                lean_class_name,
            )?);
        }
    }

    let methods_decl = methods
        .iter()
        .filter_map(|method| method.lean_decl.as_deref())
        .collect::<Vec<_>>()
        .join("\n");

    Ok(ClassImplBinding {
        class_name,
        methods,
        methods_decl,
    })
}

fn analyze_lean_class_method(
    method: &syn::ImplItemFn,
    class_name: &str,
    lean_class_name: &str,
) -> syn::Result<FunctionBinding> {
    let method_name = method.sig.ident.to_string();

    if !method.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &method.sig.generics,
            "Generic methods are not supported yet",
        ));
    }

    let (receiver, param_start_index) = match method.sig.inputs.first() {
        Some(syn::FnArg::Receiver(recv)) => {
            if recv.mutability.is_some() {
                (ReceiverStyle::MutRef, 1usize)
            } else if recv.reference.is_some() {
                (ReceiverStyle::Ref, 1usize)
            } else {
                (ReceiverStyle::Owned, 1usize)
            }
        }
        Some(syn::FnArg::Typed(_)) | None => (ReceiverStyle::None, 0usize),
    };

    let mut params = Vec::new();
    for input in method.sig.inputs.iter().skip(param_start_index) {
        if let syn::FnArg::Typed(pat_type) = input {
            let name = match &*pat_type.pat {
                syn::Pat::Ident(ident) => ident.ident.to_string(),
                _ => {
                    return Err(syn::Error::new_spanned(
                        pat_type,
                        "Only simple parameter patterns are supported",
                    ))
                }
            };
            params.push(ParameterBinding {
                name,
                ty: analyze_leanclass_type(&pat_type.ty, class_name)?,
                passing: PassingStyle::Owned,
            });
        }
    }

    let rust_return = match &method.sig.output {
        syn::ReturnType::Default => syn::parse_quote! { () },
        syn::ReturnType::Type(_, ty) => (**ty).clone(),
    };

    let kind = detect_binding_kind(method, &receiver, &params, &rust_return)?;

    let base_return = analyze_leanclass_type(&rust_return, class_name)?;
    let return_type = match receiver {
        ReceiverStyle::MutRef if is_unit_type(&rust_return) => TypeBinding {
            rust: class_name.to_string(),
            lean: Some(class_name.to_string()),
            shape: TypeShape::Named,
        },
        ReceiverStyle::MutRef => TypeBinding {
            rust: format!("({}, {})", class_name, render_type(&rust_return)),
            lean: Some(format!(
                "Prod {} {}",
                class_name,
                lean_type_arg(base_return.lean.as_deref().unwrap_or(class_name))
            )),
            shape: TypeShape::Prod,
        },
        _ => base_return.clone(),
    };
    let semantics = match receiver {
        ReceiverStyle::MutRef if is_unit_type(&rust_return) => BindingSemantics::MutatesSelf,
        ReceiverStyle::MutRef => BindingSemantics::MutatesSelfWithValue,
        _ => BindingSemantics::Value,
    };

    // `#[name = "..."]` (or `#[getter(name = "...")]` / `#[setter(name = "...")]`)
    // overrides the Lean-visible method name; the FFI symbol keeps the Rust
    // identifier.
    let lean_method_name =
        attr_name_value(&method.attrs, "name").unwrap_or_else(|| method_name.clone());
    let lean_name = format!("{}.{}", lean_class_name, lean_method_name);
    let ffi_symbol = format!("__lean_ffi_{}_{}", class_name, method_name);
    let mut type_parts = Vec::new();
    match receiver {
        ReceiverStyle::Ref | ReceiverStyle::MutRef | ReceiverStyle::Owned => {
            type_parts.push(lean_class_name.to_string())
        }
        ReceiverStyle::None => {}
    }
    for param in &params {
        type_parts.push(
            param
                .ty
                .lean
                .clone()
                .unwrap_or_else(|| param.ty.rust.clone()),
        );
    }
    type_parts.push(
        return_type
            .lean
            .clone()
            .unwrap_or_else(|| return_type.rust.clone()),
    );
    let lean_decl = format!(
        "@[extern \"{}\"] opaque {} : {}",
        ffi_symbol,
        lean_name,
        type_parts.join(" → ")
    );

    Ok(FunctionBinding {
        rust_name: method_name,
        lean_name,
        owner: Some(class_name.to_string()),
        ffi_symbol,
        receiver,
        params,
        return_type,
        semantics,
        kind,
        lean_decl: Some(lean_decl),
    })
}

fn has_attr(method: &syn::ImplItemFn, name: &str) -> bool {
    method.attrs.iter().any(|attr| attr.path().is_ident(name))
}

/// Extract a `name = "..."` value from a helper attribute, either directly
/// (`#[name = "foo"]`) or nested inside another helper
/// (`#[getter(name = "foo")]`, `#[setter(name = "foo")]`).
fn attr_name_value(attrs: &[syn::Attribute], attr_name: &str) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident(attr_name) {
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
                {
                    return Some(s.value());
                }
            }
        }
        // Nested helper form: `#[getter(name = "foo")]` etc. The nested
        // name-value may be carried by any helper attribute.
        if let syn::Meta::List(meta_list) = &attr.meta {
            if let Ok(name_value) = meta_list.parse_args::<syn::MetaNameValue>() {
                if name_value.path.is_ident("name") {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = &name_value.value
                    {
                        return Some(s.value());
                    }
                }
            }
        }
    }
    None
}

fn detect_binding_kind(
    method: &syn::ImplItemFn,
    receiver: &ReceiverStyle,
    params: &[ParameterBinding],
    rust_return: &syn::Type,
) -> syn::Result<BindingKind> {
    let is_getter = has_attr(method, "getter");
    let is_setter = has_attr(method, "setter");

    if is_getter && is_setter {
        return Err(syn::Error::new_spanned(
            method,
            "#[getter] and #[setter] are mutually exclusive",
        ));
    }

    if is_getter {
        if *receiver != ReceiverStyle::Ref {
            return Err(syn::Error::new_spanned(
                method,
                "#[getter] methods must take &self",
            ));
        }
        if !params.is_empty() {
            return Err(syn::Error::new_spanned(
                method,
                "#[getter] methods must not take additional parameters",
            ));
        }
        if is_unit_type(rust_return) {
            return Err(syn::Error::new_spanned(
                method,
                "#[getter] methods must return a non-unit value",
            ));
        }
        return Ok(BindingKind::Getter);
    }

    if is_setter {
        if *receiver != ReceiverStyle::MutRef {
            return Err(syn::Error::new_spanned(
                method,
                "#[setter] methods must take &mut self",
            ));
        }
        if params.len() != 1 {
            return Err(syn::Error::new_spanned(
                method,
                "#[setter] methods must take exactly one parameter",
            ));
        }
        if !is_unit_type(rust_return) {
            return Err(syn::Error::new_spanned(
                method,
                "#[setter] methods must return `()` (the updated object is returned to Lean)",
            ));
        }
        return Ok(BindingKind::Setter);
    }

    Ok(BindingKind::Method)
}

fn analyze_leanfn_type(ty: &syn::Type) -> syn::Result<TypeBinding> {
    match ty {
        syn::Type::Paren(paren) => analyze_leanfn_type(&paren.elem),
        syn::Type::Group(group) => analyze_leanfn_type(&group.elem),
        syn::Type::Reference(reference) if reference.mutability.is_none() => {
            match reference.elem.as_ref() {
                syn::Type::Path(type_path) if path_is_simple_ident(type_path, "str") => {
                    Ok(TypeBinding {
                        rust: render_type(ty),
                        lean: Some("String".to_string()),
                        shape: TypeShape::String,
                    })
                }
                syn::Type::Slice(slice) if is_u8_type(&slice.elem) => Ok(TypeBinding {
                    rust: render_type(ty),
                    lean: Some("ByteArray".to_string()),
                    shape: TypeShape::ByteArray,
                }),
                syn::Type::Slice(slice) => Ok(TypeBinding {
                    rust: render_type(ty),
                    lean: Some(format!(
                        "Array {}",
                        lean_type_arg(
                            analyze_leanfn_type(&slice.elem)?
                                .lean
                                .as_deref()
                                .unwrap_or("?")
                        )
                    )),
                    shape: TypeShape::Array,
                }),
                syn::Type::Array(array) => Ok(TypeBinding {
                    rust: render_type(ty),
                    lean: Some(format!(
                        "Array {}",
                        lean_type_arg(
                            analyze_leanfn_type(&array.elem)?
                                .lean
                                .as_deref()
                                .unwrap_or("?")
                        )
                    )),
                    shape: TypeShape::Array,
                }),
                _ => Ok(TypeBinding {
                    rust: render_type(ty),
                    lean: None,
                    shape: TypeShape::Unknown,
                }),
            }
        }
        syn::Type::Array(array) => Ok(TypeBinding {
            rust: render_type(ty),
            lean: Some(format!(
                "Array {}",
                lean_type_arg(
                    analyze_leanfn_type(&array.elem)?
                        .lean
                        .as_deref()
                        .unwrap_or("?")
                )
            )),
            shape: TypeShape::Array,
        }),
        syn::Type::Tuple(tuple) if tuple.elems.is_empty() => Ok(unit_binding()),
        syn::Type::Tuple(tuple) if tuple.elems.len() >= 2 => {
            let items = tuple
                .elems
                .iter()
                .map(analyze_leanfn_type)
                .collect::<syn::Result<Vec<_>>>()?;
            let lean = nested_prod_type(&items);
            Ok(TypeBinding {
                rust: render_type(ty),
                lean: Some(lean),
                shape: TypeShape::Prod,
            })
        }
        syn::Type::Path(type_path) => analyze_path_type(type_path, None),
        _ => Ok(TypeBinding {
            rust: render_type(ty),
            lean: None,
            shape: TypeShape::Unknown,
        }),
    }
}

fn analyze_leanclass_type(ty: &syn::Type, class_name: &str) -> syn::Result<TypeBinding> {
    match ty {
        syn::Type::Paren(paren) => analyze_leanclass_type(&paren.elem, class_name),
        syn::Type::Group(group) => analyze_leanclass_type(&group.elem, class_name),
        syn::Type::Tuple(tuple) if tuple.elems.is_empty() => Ok(unit_binding()),
        syn::Type::Tuple(tuple) if tuple.elems.len() == 2 => {
            let left = analyze_leanclass_type(&tuple.elems[0], class_name)?;
            let right = analyze_leanclass_type(&tuple.elems[1], class_name)?;
            let left_lean = left
                .lean
                .ok_or_else(|| syn::Error::new_spanned(&tuple.elems[0], "unsupported Rust type in generated Lean declaration"))?;
            let right_lean = right
                .lean
                .ok_or_else(|| syn::Error::new_spanned(&tuple.elems[1], "unsupported Rust type in generated Lean declaration"))?;
            Ok(TypeBinding {
                rust: render_type(ty),
                lean: Some(format!("Prod {} {}", left_lean, lean_type_arg(&right_lean))),
                shape: TypeShape::Prod,
            })
        }
        syn::Type::Tuple(_) => Err(syn::Error::new_spanned(
            ty,
            "tuple Lean declarations currently support only unit `()` or pairs `(A, B)`",
        )),
        syn::Type::Reference(_) => Err(syn::Error::new_spanned(
            ty,
            "reference types are not supported in generated Lean declarations; use owned types instead",
        )),
        syn::Type::Path(type_path) => analyze_path_type(type_path, Some(class_name)),
        other => Err(syn::Error::new_spanned(
            other,
            "unsupported Rust type in generated Lean declaration",
        )),
    }
}

fn analyze_path_type(
    type_path: &syn::TypePath,
    class_name: Option<&str>,
) -> syn::Result<TypeBinding> {
    let Some(segment) = type_path.path.segments.last() else {
        return Err(syn::Error::new_spanned(
            type_path,
            "cannot determine Lean type for an empty path",
        ));
    };

    let ident = segment.ident.to_string();
    let binding = match ident.as_str() {
        "i8" | "i16" | "i32" | "i64" | "isize" | "u8" | "u16" | "u32" | "u64" | "usize"
        | "f32" | "f64" | "bool" | "char" => TypeBinding {
            rust: render_type(type_path),
            lean: Some(match ident.as_str() {
                "i8" => "Int8",
                "i16" => "Int16",
                "i32" => "Int32",
                "i64" => "Int64",
                "isize" => "ISize",
                "u8" => "UInt8",
                "u16" => "UInt16",
                "u32" => "UInt32",
                "u64" => "UInt64",
                "usize" => "USize",
                "f32" => "Float32",
                "f64" => "Float",
                "bool" => "Bool",
                "char" => "Char",
                _ => unreachable!(),
            }
            .to_string()),
            shape: TypeShape::Scalar,
        },
        "String" => TypeBinding {
            rust: render_type(type_path),
            lean: Some("String".to_string()),
            shape: TypeShape::String,
        },
        "Vec" => {
            let elem = expect_single_type_arg(segment, "Vec")?;
            let elem_ty = analyze_inner_type(elem, class_name)?;
            TypeBinding {
                rust: render_type(type_path),
                lean: elem_ty
                    .lean
                    .map(|elem| format!("Array {}", lean_type_arg(&elem))),
                shape: TypeShape::Array,
            }
        }
        "Option" => {
            let elem = expect_single_type_arg(segment, "Option")?;
            let elem_ty = analyze_inner_type(elem, class_name)?;
            TypeBinding {
                rust: render_type(type_path),
                lean: elem_ty
                    .lean
                    .map(|elem| format!("Option {}", lean_type_arg(&elem))),
                shape: TypeShape::Option,
            }
        }
        "Result" => {
            let (ok_ty, err_ty) = expect_two_type_args(segment, "Result")?;
            let ok_ty = analyze_inner_type(ok_ty, class_name)?;
            let err_ty = analyze_inner_type(err_ty, class_name)?;
            TypeBinding {
                rust: render_type(type_path),
                lean: match (ok_ty.lean, err_ty.lean) {
                    (Some(ok_ty), Some(err_ty)) => Some(format!(
                        "Except {} {}",
                        lean_type_arg(&err_ty),
                        lean_type_arg(&ok_ty)
                    )),
                    _ => None,
                },
                shape: TypeShape::Except,
            }
        }
        "Self" => TypeBinding {
            rust: render_type(type_path),
            lean: class_name.map(str::to_string),
            shape: TypeShape::Named,
        },
        other if class_name.is_some_and(|name| name == other) => TypeBinding {
            rust: render_type(type_path),
            lean: Some(other.to_string()),
            shape: TypeShape::Named,
        },
        _ => match &segment.arguments {
            syn::PathArguments::None => TypeBinding {
                rust: render_type(type_path),
                lean: Some(ident),
                shape: TypeShape::Named,
            },
            syn::PathArguments::AngleBracketed(_) if class_name.is_some() => {
                return Err(syn::Error::new_spanned(
                    segment,
                    format!(
                        "generic type `{}` is not supported in generated Lean declarations; only Vec<T>, Option<T>, Result<T, E>, and pairs `(A, B)` are currently supported",
                        segment.ident
                    ),
                ))
            }
            syn::PathArguments::AngleBracketed(_) => TypeBinding {
                rust: render_type(type_path),
                lean: None,
                shape: TypeShape::Unknown,
            },
            syn::PathArguments::Parenthesized(_) if class_name.is_some() => {
                return Err(syn::Error::new_spanned(
                    segment,
                    "function-trait-style path arguments are not supported in generated Lean declarations",
                ))
            }
            syn::PathArguments::Parenthesized(_) => TypeBinding {
                rust: render_type(type_path),
                lean: None,
                shape: TypeShape::Unknown,
            },
        },
    };

    Ok(binding)
}

fn analyze_inner_type(ty: &syn::Type, class_name: Option<&str>) -> syn::Result<TypeBinding> {
    match class_name {
        Some(class_name) => analyze_leanclass_type(ty, class_name),
        None => analyze_leanfn_type(ty),
    }
}

fn class_name_from_self_ty(self_ty: &syn::Type) -> syn::Result<String> {
    match self_ty {
        syn::Type::Path(type_path) => type_path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .ok_or_else(|| {
                syn::Error::new_spanned(self_ty, "Could not extract struct name from impl")
            }),
        _ => Err(syn::Error::new_spanned(
            self_ty,
            "#[leanclass] impl must be for a simple struct type",
        )),
    }
}

fn nested_prod_type(items: &[TypeBinding]) -> String {
    if items.len() == 2 {
        let left = items[0].lean.as_deref().unwrap_or("?");
        let right = items[1].lean.as_deref().unwrap_or("?");
        return format!("Prod {} {}", left, lean_type_arg(right));
    }

    let head = items[0].lean.as_deref().unwrap_or("?");
    let tail = nested_prod_type(&items[1..]);
    format!("Prod {} {}", head, lean_type_arg(&tail))
}

fn unit_binding() -> TypeBinding {
    TypeBinding {
        rust: "()".to_string(),
        lean: Some("Unit".to_string()),
        shape: TypeShape::Unit,
    }
}

fn passing_style_for_leanfn(ty: &syn::Type) -> PassingStyle {
    match ty {
        syn::Type::Reference(reference) if reference.mutability.is_none() => PassingStyle::Borrowed,
        _ => PassingStyle::Owned,
    }
}

fn render_type<T: ToTokens>(value: &T) -> String {
    value.to_token_stream().to_string()
}

fn lean_type_arg(ty: &str) -> String {
    if ty.contains(' ') {
        format!("({ty})")
    } else {
        ty.to_string()
    }
}

fn expect_single_type_arg<'a>(
    segment: &'a syn::PathSegment,
    type_name: &str,
) -> syn::Result<&'a syn::Type> {
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return Err(syn::Error::new_spanned(
            segment,
            format!("{type_name} requires one type argument in generated Lean declarations"),
        ));
    };

    let mut tys = args.args.iter().filter_map(|arg| match arg {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    let first = tys.next().ok_or_else(|| {
        syn::Error::new_spanned(
            segment,
            format!("{type_name} requires one type argument in generated Lean declarations"),
        )
    })?;
    if tys.next().is_some() {
        return Err(syn::Error::new_spanned(
            segment,
            format!(
                "{type_name} requires exactly one type argument in generated Lean declarations"
            ),
        ));
    }
    Ok(first)
}

fn expect_two_type_args<'a>(
    segment: &'a syn::PathSegment,
    type_name: &str,
) -> syn::Result<(&'a syn::Type, &'a syn::Type)> {
    let syn::PathArguments::AngleBracketed(args) = &segment.arguments else {
        return Err(syn::Error::new_spanned(
            segment,
            format!("{type_name} requires two type arguments in generated Lean declarations"),
        ));
    };

    let mut tys = args.args.iter().filter_map(|arg| match arg {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    let first = tys.next().ok_or_else(|| {
        syn::Error::new_spanned(
            segment,
            format!("{type_name} requires two type arguments in generated Lean declarations"),
        )
    })?;
    let second = tys.next().ok_or_else(|| {
        syn::Error::new_spanned(
            segment,
            format!("{type_name} requires two type arguments in generated Lean declarations"),
        )
    })?;
    if tys.next().is_some() {
        return Err(syn::Error::new_spanned(
            segment,
            format!(
                "{type_name} requires exactly two type arguments in generated Lean declarations"
            ),
        ));
    }
    Ok((first, second))
}

fn is_unit_type(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Tuple(tuple) if tuple.elems.is_empty())
}

fn is_u8_type(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Path(type_path) if path_is_simple_ident(type_path, "u8"))
}

fn path_is_simple_ident(type_path: &syn::TypePath, ident: &str) -> bool {
    type_path.qself.is_none()
        && type_path.path.segments.len() == 1
        && type_path
            .path
            .segments
            .first()
            .is_some_and(|segment| segment.ident == ident)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn method_at(item: &syn::ItemImpl, idx: usize) -> &syn::ImplItemFn {
        match &item.items[idx] {
            syn::ImplItem::Fn(method) => method,
            _ => panic!("expected impl item {idx} to be a method"),
        }
    }

    // ---- analyze_lean_class_struct / class_opaque_decl ----

    #[test]
    fn class_struct_defaults_to_rust_name() {
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct Foo {
                pub x: u32,
            }
        };
        let binding = analyze_lean_class_struct(&item, None).expect("struct analysis");
        assert_eq!(binding.rust_name, "Foo");
        assert_eq!(binding.lean_name, "Foo");
        assert_eq!(
            binding.opaque_decl,
            "opaque Foo.ffi : NonemptyType\n\
             def Foo : Type := Foo.ffi.val\n\
             instance : Nonempty Foo := Foo.ffi.property"
        );
    }

    #[test]
    fn class_struct_respects_lean_name_override() {
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct Foo {
                pub x: u32,
            }
        };
        let binding = analyze_lean_class_struct(&item, Some("Renamed")).expect("struct analysis");
        assert_eq!(binding.rust_name, "Foo");
        assert_eq!(binding.lean_name, "Renamed");
        assert!(binding
            .opaque_decl
            .starts_with("opaque Renamed.ffi : NonemptyType"));
        assert!(binding
            .opaque_decl
            .contains("def Renamed : Type := Renamed.ffi.val"));
        assert!(binding
            .opaque_decl
            .contains("instance : Nonempty Renamed := Renamed.ffi.property"));
    }

    #[test]
    fn class_struct_rejects_generics() {
        let item: syn::ItemStruct = syn::parse_quote! {
            pub struct Foo<T> {
                pub x: T,
            }
        };
        let err = analyze_lean_class_struct(&item, None).expect_err("generic struct must fail");
        assert!(err
            .to_string()
            .contains("#[leanclass] does not support generic types yet"));
    }

    #[test]
    fn class_opaque_decl_emits_nonempty_type_triple() {
        assert_eq!(
            class_opaque_decl("Foo"),
            "opaque Foo.ffi : NonemptyType\n\
             def Foo : Type := Foo.ffi.val\n\
             instance : Nonempty Foo := Foo.ffi.property"
        );
    }

    // ---- analyze_lean_class_field_accessors ----

    #[test]
    fn field_accessors_get_set_both_and_none() {
        let item: syn::ItemStruct = syn::parse_quote! {
            struct Foo {
                #[get]
                x: u32,
                #[set]
                y: u32,
                #[get]
                #[set]
                z: u32,
                plain: u32,
            }
        };
        let accessors = analyze_lean_class_field_accessors(&item).expect("accessors");
        assert_eq!(accessors.len(), 3);
        assert_eq!((accessors[0].getter, accessors[0].setter), (true, false));
        assert_eq!((accessors[1].getter, accessors[1].setter), (false, true));
        assert_eq!((accessors[2].getter, accessors[2].setter), (true, true));
        assert_eq!(accessors[0].rust_name, "x");
        assert_eq!(accessors[0].ty.lean.as_deref(), Some("UInt32"));

        // No accessor attributes -> empty.
        let item: syn::ItemStruct = syn::parse_quote! { struct Bar { a: u32 } };
        assert!(analyze_lean_class_field_accessors(&item)
            .expect("accessors")
            .is_empty());

        // Unit struct -> empty.
        let item: syn::ItemStruct = syn::parse_quote! { struct Baz; };
        assert!(analyze_lean_class_field_accessors(&item)
            .expect("accessors")
            .is_empty());
    }

    #[test]
    fn field_accessors_reject_unsupported_types() {
        let item: syn::ItemStruct = syn::parse_quote! {
            struct Foo {
                #[get]
                x: &'static str,
            }
        };
        let err = analyze_lean_class_field_accessors(&item).expect_err("reference field must fail");
        assert!(err
            .to_string()
            .contains("reference types are not supported"));

        let item: syn::ItemStruct = syn::parse_quote! {
            struct Foo {
                #[get]
                x: std::collections::HashMap<u8, u8>,
            }
        };
        let err = analyze_lean_class_field_accessors(&item).expect_err("generic field must fail");
        assert!(err
            .to_string()
            .contains("generic type `HashMap` is not supported"));
    }

    #[test]
    fn field_accessors_require_named_fields() {
        let item: syn::ItemStruct = syn::parse_quote! { struct Foo(#[get] u32); };
        let err =
            analyze_lean_class_field_accessors(&item).expect_err("tuple field accessor must fail");
        assert!(err
            .to_string()
            .contains("#[get] / #[set] field accessors require named struct fields"));
    }

    // ---- field_accessor_bindings ----

    #[test]
    fn field_accessor_bindings_build_decls() {
        let item: syn::ItemStruct = syn::parse_quote! {
            struct Foo {
                #[get]
                x: u32,
                #[set]
                y: u32,
            }
        };
        let accessors = analyze_lean_class_field_accessors(&item).expect("accessors");
        let bindings = field_accessor_bindings("Foo", &accessors);
        assert_eq!(bindings.len(), 2);

        let getter = &bindings[0];
        assert_eq!(getter.rust_name, "x");
        assert_eq!(getter.lean_name, "Foo.x");
        assert_eq!(getter.owner.as_deref(), Some("Foo"));
        assert_eq!(getter.ffi_symbol, "__lean_ffi_Foo_x");
        assert_eq!(getter.receiver, ReceiverStyle::Ref);
        assert_eq!(getter.kind, BindingKind::Getter);
        assert_eq!(getter.semantics, BindingSemantics::Value);
        assert!(getter.params.is_empty());
        assert_eq!(
            getter.lean_decl.as_deref(),
            Some("@[extern \"__lean_ffi_Foo_x\"] opaque Foo.x : Foo → UInt32")
        );

        let setter = &bindings[1];
        assert_eq!(setter.rust_name, "set_y");
        assert_eq!(setter.lean_name, "Foo.set_y");
        assert_eq!(setter.ffi_symbol, "__lean_ffi_Foo_set_y");
        assert_eq!(setter.receiver, ReceiverStyle::MutRef);
        assert_eq!(setter.kind, BindingKind::Setter);
        assert_eq!(setter.semantics, BindingSemantics::MutatesSelf);
        assert_eq!(setter.params.len(), 1);
        assert_eq!(setter.params[0].name, "value");
        assert_eq!(setter.params[0].passing, PassingStyle::Owned);
        assert_eq!(setter.return_type.lean.as_deref(), Some("Foo"));
        assert_eq!(
            setter.lean_decl.as_deref(),
            Some("@[extern \"__lean_ffi_Foo_set_y\"] opaque Foo.set_y : Foo → UInt32 → Foo")
        );
    }

    // ---- analyze_lean_class_impl ----

    #[test]
    fn class_impl_analyzes_receivers() {
        let item: syn::ItemImpl = syn::parse_quote! {
            impl Foo {
                fn stat(x: u32) -> u32 { x }
                fn get(&self) -> u32 { self.x }
                fn bump(&mut self, v: u32) { self.x = v; }
                fn take(self) -> u32 { self.x }
            }
        };
        let binding = analyze_lean_class_impl(&item, None).expect("impl analysis");
        assert_eq!(binding.class_name, "Foo");
        assert_eq!(binding.methods.len(), 4);
        let receivers: Vec<ReceiverStyle> = binding.methods.iter().map(|m| m.receiver).collect();
        assert_eq!(
            receivers,
            vec![
                ReceiverStyle::None,
                ReceiverStyle::Ref,
                ReceiverStyle::MutRef,
                ReceiverStyle::Owned
            ]
        );

        let stat = &binding.methods[0];
        assert_eq!(stat.lean_name, "Foo.stat");
        assert_eq!(stat.owner.as_deref(), Some("Foo"));
        assert_eq!(stat.ffi_symbol, "__lean_ffi_Foo_stat");
        assert!(stat
            .lean_decl
            .as_deref()
            .unwrap()
            .contains("opaque Foo.stat : UInt32 → UInt32"));

        let bump = &binding.methods[2];
        assert_eq!(bump.semantics, BindingSemantics::MutatesSelf);
        assert_eq!(bump.return_type.lean.as_deref(), Some("Foo"));
        assert!(bump
            .lean_decl
            .as_deref()
            .unwrap()
            .contains("opaque Foo.bump : Foo → UInt32 → Foo"));

        let take = &binding.methods[3];
        assert_eq!(take.semantics, BindingSemantics::Value);
        assert!(take
            .lean_decl
            .as_deref()
            .unwrap()
            .contains("opaque Foo.take : Foo → UInt32"));

        assert!(binding
            .methods_decl
            .contains("opaque Foo.get : Foo → UInt32"));
    }

    #[test]
    fn class_impl_lean_name_override() {
        let item: syn::ItemImpl = syn::parse_quote! {
            impl Foo {
                fn get(&self) -> u32 { self.x }
            }
        };
        let binding = analyze_lean_class_impl(&item, Some("Renamed")).expect("impl analysis");
        assert_eq!(binding.class_name, "Foo");
        assert_eq!(binding.methods[0].lean_name, "Renamed.get");
        assert!(binding.methods[0]
            .lean_decl
            .as_deref()
            .unwrap()
            .contains("opaque Renamed.get : Renamed → UInt32"));
        assert!(binding.methods_decl.contains("opaque Renamed.get"));
    }

    #[test]
    fn class_impl_rejects_generics_and_non_path_self() {
        let item: syn::ItemImpl = syn::parse_quote! { impl<T> Foo<T> { fn m(&self) {} } };
        let err = analyze_lean_class_impl(&item, None).expect_err("generic impl must fail");
        assert!(err
            .to_string()
            .contains("#[leanclass] does not support generic impl blocks yet"));

        let item: syn::ItemImpl = syn::parse_quote! { impl (Foo,) { fn m(&self) {} } };
        let err = analyze_lean_class_impl(&item, None).expect_err("tuple self type must fail");
        assert!(err
            .to_string()
            .contains("#[leanclass] impl must be for a simple struct type"));
    }

    // ---- analyze_lean_class_method ----

    #[test]
    fn class_method_name_attr_and_kinds() {
        let item: syn::ItemImpl = syn::parse_quote! {
            impl Foo {
                #[name = "renamed"]
                fn m(&self) -> u32 { 1 }

                #[getter(name = "g")]
                fn g(&self) -> u32 { 1 }

                #[setter(name = "s")]
                fn s(&mut self, v: u32) { }
            }
        };
        let m =
            analyze_lean_class_method(method_at(&item, 0), "Foo", "Foo").expect("method analysis");
        assert_eq!(m.rust_name, "m");
        assert_eq!(m.lean_name, "Foo.renamed");
        assert_eq!(m.ffi_symbol, "__lean_ffi_Foo_m");
        assert_eq!(m.kind, BindingKind::Method);
        assert!(m
            .lean_decl
            .as_deref()
            .unwrap()
            .contains("opaque Foo.renamed : Foo → UInt32"));

        let g =
            analyze_lean_class_method(method_at(&item, 1), "Foo", "Foo").expect("getter analysis");
        assert_eq!(g.lean_name, "Foo.g");
        assert_eq!(g.kind, BindingKind::Getter);
        assert_eq!(g.receiver, ReceiverStyle::Ref);

        let s =
            analyze_lean_class_method(method_at(&item, 2), "Foo", "Foo").expect("setter analysis");
        assert_eq!(s.lean_name, "Foo.s");
        assert_eq!(s.kind, BindingKind::Setter);
        assert_eq!(s.receiver, ReceiverStyle::MutRef);
        assert_eq!(s.semantics, BindingSemantics::MutatesSelf);
        assert_eq!(s.return_type.lean.as_deref(), Some("Foo"));
    }

    #[test]
    fn class_method_kind_errors() {
        let cases: &[(&str, &str)] = &[
            (
                "#[getter]\n#[setter]\nfn m(&self) -> u32 { 1 }",
                "#[getter] and #[setter] are mutually exclusive",
            ),
            (
                "#[getter]\nfn m(&mut self) -> u32 { 1 }",
                "#[getter] methods must take &self",
            ),
            (
                "#[getter]\nfn m(&self, x: u32) -> u32 { 1 }",
                "#[getter] methods must not take additional parameters",
            ),
            (
                "#[getter]\nfn m(&self) { }",
                "#[getter] methods must return a non-unit value",
            ),
            (
                "#[setter]\nfn m(&self, v: u32) { }",
                "#[setter] methods must take &mut self",
            ),
            (
                "#[setter]\nfn m(&mut self, a: u32, b: u32) { }",
                "#[setter] methods must take exactly one parameter",
            ),
            (
                "#[setter]\nfn m(&mut self, v: u32) -> u32 { 1 }",
                "#[setter] methods must return `()` (the updated object is returned to Lean)",
            ),
        ];
        for (method_src, expected) in cases {
            let item: syn::ItemImpl =
                syn::parse_str(&format!("impl Foo {{ {method_src} }}")).expect("parse impl");
            let err = analyze_lean_class_method(method_at(&item, 0), "Foo", "Foo")
                .expect_err("kind validation must fail");
            assert!(
                err.to_string().contains(expected),
                "expected {expected:?} in error {err}"
            );
        }
    }

    #[test]
    fn class_method_rejects_generics() {
        let item: syn::ItemImpl = syn::parse_quote! { impl Foo { fn m<T>(&self) {} } };
        let err = analyze_lean_class_method(method_at(&item, 0), "Foo", "Foo")
            .expect_err("generic method must fail");
        assert!(err
            .to_string()
            .contains("Generic methods are not supported yet"));
    }

    // ---- analyze_lean_function ----

    #[test]
    fn leanfn_borrowed_params_and_name_override() {
        let func: syn::ItemFn = syn::parse_quote! {
            fn greet(name: &str, bytes: &[u8], nums: &[u32], fixed: &[u8; 4]) -> u64 { 0 }
        };
        let binding = analyze_lean_function(
            &func,
            FunctionOptions {
                lean_name: Some("greetLean".to_string()),
            },
        )
        .expect("leanfn analysis");
        assert_eq!(binding.rust_name, "greet");
        assert_eq!(binding.lean_name, "greetLean");
        assert_eq!(binding.ffi_symbol, "greetLean");
        assert_eq!(binding.receiver, ReceiverStyle::None);
        assert_eq!(binding.kind, BindingKind::Method);
        assert_eq!(binding.semantics, BindingSemantics::Value);
        assert_eq!(binding.params.len(), 4);
        assert_eq!(binding.params[0].name, "name");
        assert_eq!(binding.params[0].passing, PassingStyle::Borrowed);
        assert_eq!(binding.params[0].ty.lean.as_deref(), Some("String"));
        assert_eq!(binding.params[0].ty.shape, TypeShape::String);
        assert_eq!(binding.params[1].ty.lean.as_deref(), Some("ByteArray"));
        assert_eq!(binding.params[1].ty.shape, TypeShape::ByteArray);
        assert_eq!(binding.params[2].ty.lean.as_deref(), Some("Array UInt32"));
        assert_eq!(binding.params[2].ty.shape, TypeShape::Array);
        assert_eq!(binding.params[3].ty.lean.as_deref(), Some("Array UInt8"));
        assert_eq!(binding.return_type.lean.as_deref(), Some("UInt64"));
        assert_eq!(binding.return_type.shape, TypeShape::Scalar);
    }

    #[test]
    fn leanfn_default_name_and_unit_return() {
        let func: syn::ItemFn = syn::parse_quote! { fn noop() {} };
        let binding =
            analyze_lean_function(&func, FunctionOptions::default()).expect("leanfn analysis");
        assert_eq!(binding.rust_name, "noop");
        assert_eq!(binding.lean_name, "noop");
        assert!(binding.params.is_empty());
        assert_eq!(binding.return_type.rust, "()");
        assert_eq!(binding.return_type.lean.as_deref(), Some("Unit"));
        assert_eq!(binding.return_type.shape, TypeShape::Unit);
    }

    #[test]
    fn leanfn_rejects_generics_self_and_complex_patterns() {
        let func: syn::ItemFn = syn::parse_quote! { fn generic<T>(x: T) {} };
        let err = analyze_lean_function(&func, FunctionOptions::default())
            .expect_err("generics must fail");
        assert!(err
            .to_string()
            .contains("Generic functions are not supported yet"));

        let func: syn::ItemFn = syn::parse_quote! { fn method(&self) {} };
        let err =
            analyze_lean_function(&func, FunctionOptions::default()).expect_err("self must fail");
        assert!(err
            .to_string()
            .contains("Methods with `self` are not supported."));

        let func: syn::ItemFn = syn::parse_quote! { fn pat((a, b): (u32, u32)) {} };
        let err = analyze_lean_function(&func, FunctionOptions::default())
            .expect_err("complex pattern must fail");
        assert!(err
            .to_string()
            .contains("Only simple parameter patterns are supported"));
    }

    // ---- analyze_concrete_instance ----

    #[test]
    fn concrete_instance_monomorphizes_generic_function() {
        let func: syn::ItemFn = syn::parse_quote! {
            fn wrap<T>(value: T, extra: u8) -> T { value }
        };
        let concrete = ConcreteAttr {
            types: vec![syn::parse_quote!(u32)],
            name: "wrapU32".to_string(),
        };
        let binding = analyze_concrete_instance(&func, &concrete).expect("concrete instance");
        assert_eq!(binding.rust_name, "wrap");
        assert_eq!(binding.lean_name, "wrapU32");
        assert_eq!(binding.ffi_symbol, "wrapU32");
        assert_eq!(binding.receiver, ReceiverStyle::None);
        assert_eq!(binding.semantics, BindingSemantics::Value);
        assert_eq!(binding.params.len(), 2);
        assert_eq!(binding.params[0].name, "value");
        assert_eq!(binding.params[0].ty.lean.as_deref(), Some("UInt32"));
        assert_eq!(binding.params[0].passing, PassingStyle::Owned);
        assert_eq!(binding.params[1].ty.lean.as_deref(), Some("UInt8"));
        assert_eq!(binding.return_type.lean.as_deref(), Some("UInt32"));
    }

    #[test]
    fn concrete_instance_error_cases() {
        let func: syn::ItemFn = syn::parse_quote! { fn one<T>(x: T) {} };
        let concrete = ConcreteAttr {
            types: vec![syn::parse_quote!(u32), syn::parse_quote!(u64)],
            name: "n".to_string(),
        };
        let err = analyze_concrete_instance(&func, &concrete).expect_err("wrong arity must fail");
        assert!(err
            .to_string()
            .contains("expected 1 concrete type(s) for 1 generic parameter(s), got 2"));

        let func: syn::ItemFn = syn::parse_quote! { fn lt<'a>(x: &'a str) {} };
        let concrete = ConcreteAttr {
            types: vec![syn::parse_quote!(u32)],
            name: "n".to_string(),
        };
        let err = analyze_concrete_instance(&func, &concrete).expect_err("lifetime must fail");
        assert!(err
            .to_string()
            .contains("lifetime parameters are not supported with `concrete`"));

        let func: syn::ItemFn = syn::parse_quote! { fn cn<const N: usize>(x: [u8; N]) {} };
        let concrete = ConcreteAttr {
            types: vec![syn::parse_quote!(u32)],
            name: "n".to_string(),
        };
        let err = analyze_concrete_instance(&func, &concrete).expect_err("const param must fail");
        assert!(err
            .to_string()
            .contains("const parameters are not supported with `concrete`"));

        let func: syn::ItemFn = syn::parse_quote! { fn selfy<T>(&self, x: T) {} };
        let concrete = ConcreteAttr {
            types: vec![syn::parse_quote!(u32)],
            name: "n".to_string(),
        };
        let err = analyze_concrete_instance(&func, &concrete).expect_err("self must fail");
        assert!(err
            .to_string()
            .contains("Methods with `self` are not supported."));

        let func: syn::ItemFn = syn::parse_quote! { fn pat<T>((a, b): (T, T)) {} };
        let concrete = ConcreteAttr {
            types: vec![syn::parse_quote!(u32)],
            name: "n".to_string(),
        };
        let err =
            analyze_concrete_instance(&func, &concrete).expect_err("complex pattern must fail");
        assert!(err
            .to_string()
            .contains("Only simple parameter patterns are supported"));
    }

    // ---- collect_module_exports ----

    #[test]
    fn collect_module_exports_plain_named_crate_and_concrete() {
        let file: syn::File = syn::parse_quote! {
            #[leanfn()]
            fn alpha() {}

            #[leanfn(name = "beta_lean")]
            fn beta() {}

            #[leanfn(concrete(u32, name = "fooU32"), crate = "leo3")]
            fn foo<T>(x: T) -> T { x }

            fn plain() {}

            struct NotAFunction;
        };
        let exports = collect_module_exports(&file.items).expect("module exports");
        assert_eq!(exports.len(), 3);
        assert_eq!(exports[0].rust_name, "alpha");
        assert_eq!(exports[0].lean_name, "alpha");
        assert_eq!(exports[0].ffi_symbol, "alpha");
        assert_eq!(exports[1].rust_name, "beta");
        assert_eq!(exports[1].lean_name, "beta_lean");
        assert_eq!(exports[1].ffi_symbol, "beta_lean");
        assert_eq!(exports[2].rust_name, "foo");
        assert_eq!(exports[2].lean_name, "fooU32");
        assert_eq!(exports[2].ffi_symbol, "fooU32");
        assert_eq!(exports[2].params[0].ty.lean.as_deref(), Some("UInt32"));
        assert_eq!(exports[2].return_type.lean.as_deref(), Some("UInt32"));
    }

    #[test]
    fn leanfn_option_parse_errors() {
        let file: syn::File = syn::parse_quote! {
            #[leanfn(unknown = "x")]
            fn foo() {}
        };
        let err = collect_module_exports(&file.items).expect_err("unknown meta must fail");
        assert!(err
            .to_string()
            .contains("Expected name-value attribute like `name = \"...\"`"));

        let file: syn::File = syn::parse_quote! {
            #[leanfn(concrete(u32))]
            fn foo<T>(x: T) -> T { x }
        };
        let err = collect_module_exports(&file.items).expect_err("missing concrete name must fail");
        assert!(err
            .to_string()
            .contains("`concrete` requires `name = \"...\"`"));

        let file: syn::File = syn::parse_quote! {
            #[leanfn(concrete(name = "x"))]
            fn foo<T>(x: T) -> T { x }
        };
        let err = collect_module_exports(&file.items).expect_err("empty concrete types must fail");
        assert!(err
            .to_string()
            .contains("`concrete` requires at least one type argument"));

        let file: syn::File = syn::parse_quote! {
            #[leanfn(concrete(other = "x"))]
            fn foo<T>(x: T) -> T { x }
        };
        let err = collect_module_exports(&file.items).expect_err("unknown concrete key must fail");
        assert!(err.to_string().contains("expected `name`"));
    }

    // ---- collect_submodule_exports ----

    #[test]
    fn collect_submodule_exports_nested_and_prefixed() {
        let file: syn::File = syn::parse_quote! {
            mod outer {
                #[leanfn(name = "a_lean")]
                fn a() {}

                mod inner {
                    #[leanfn()]
                    fn b() {}

                    mod deepest {
                        #[leanfn(name = "z_lean")]
                        fn z() {}
                    }
                }
            }
        };
        let subs = collect_submodule_exports(&file.items, "").expect("submodule exports");
        let paths: Vec<&str> = subs.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(paths, vec!["outer", "outer.inner", "outer.inner.deepest"]);
        assert_eq!(subs[0].exports.len(), 1);
        assert_eq!(subs[0].exports[0].lean_name, "a_lean");
        assert_eq!(subs[1].exports[0].rust_name, "b");
        assert_eq!(subs[2].exports[0].lean_name, "z_lean");

        let subs = collect_submodule_exports(&file.items, "base").expect("prefixed exports");
        let paths: Vec<&str> = subs.iter().map(|s| s.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["base.outer", "base.outer.inner", "base.outer.inner.deepest"]
        );
    }

    #[test]
    fn collect_submodule_exports_skips_empty_and_out_of_line_mods() {
        let file: syn::File = syn::parse_quote! {
            mod empty {}
            mod external;
        };
        let subs = collect_submodule_exports(&file.items, "").expect("submodule exports");
        assert!(subs.is_empty());
    }

    // ---- filter_exports ----

    #[test]
    fn filter_exports_restricts_and_orders() {
        let file: syn::File = syn::parse_quote! {
            #[leanfn()]
            fn alpha() {}

            #[leanfn(name = "beta_lean")]
            fn beta() {}
        };
        let exports = collect_module_exports(&file.items).expect("module exports");
        let filtered = filter_exports(
            exports.clone(),
            &["beta_lean".to_string(), "alpha".to_string()],
        )
        .expect("filter exports");
        assert_eq!(filtered.len(), 2);
        // Order follows the `allowed` list, and both Lean and Rust names match.
        assert_eq!(filtered[0].rust_name, "beta");
        assert_eq!(filtered[1].rust_name, "alpha");
    }

    #[test]
    fn filter_exports_missing_name_errors() {
        let file: syn::File = syn::parse_quote! {
            #[leanfn()]
            fn alpha() {}
        };
        let exports = collect_module_exports(&file.items).expect("module exports");
        let err = filter_exports(exports, &["missing".to_string()])
            .expect_err("unknown export must fail");
        assert!(err
            .to_string()
            .contains("export `missing` not found in module"));
    }

    // ---- is_leanfn_attr ----

    #[test]
    fn is_leanfn_attr_matches_last_path_segment() {
        let cases: &[(&str, bool)] = &[
            ("#[leanfn]", true),
            ("#[leanfn(name = \"x\")]", true),
            ("#[leo3::leanfn]", true),
            ("#[leanfn::nested]", false),
            ("#[leanfnx]", false),
            ("#[cfg(test)]", false),
        ];
        for (src, expected) in cases {
            let item: syn::ItemFn =
                syn::parse_str(&format!("{src}\nfn f() {{}}")).expect("parse fn");
            let attr = &item.attrs[0];
            assert_eq!(is_leanfn_attr(attr), *expected, "for {src}");
        }
    }

    // ---- substitute_type ----

    #[test]
    fn substitute_type_replaces_generic_params() {
        let mut mapping = std::collections::HashMap::new();
        mapping.insert("T".to_string(), syn::parse_quote!(u32));

        let check = |input: proc_macro2::TokenStream, expected: &str| {
            let ty: syn::Type = syn::parse2(input).expect("parse type");
            assert_eq!(render_type(&substitute_type(&ty, &mapping)), expected);
        };

        check(syn::parse_quote!(T), "u32");
        check(syn::parse_quote!(Vec<T>), "Vec < u32 >");
        check(syn::parse_quote!(&T), "& u32");
        check(syn::parse_quote!((T, u32)), "(u32 , u32)");
        check(syn::parse_quote!([T; 4]), "[u32 ; 4]");
        check(syn::parse_quote!(&[T]), "& [u32]");
        check(syn::parse_quote!((T)), "(u32)");
        check(
            syn::parse_quote!(std::collections::HashMap<T, u8>),
            "std :: collections :: HashMap < u32 , u8 >",
        );
    }

    #[test]
    fn substitute_type_leaves_unmapped_types_untouched() {
        let mapping = std::collections::HashMap::new();
        let check = |input: proc_macro2::TokenStream, expected: &str| {
            let ty: syn::Type = syn::parse2(input).expect("parse type");
            assert_eq!(render_type(&substitute_type(&ty, &mapping)), expected);
        };

        check(syn::parse_quote!(String), "String");
        check(syn::parse_quote!(*const T), "* const T");
        check(syn::parse_quote!(&mut str), "& mut str");
        check(syn::parse_quote!(Vec<u8>), "Vec < u8 >");
        check(syn::parse_quote!(u64), "u64");
    }
}
