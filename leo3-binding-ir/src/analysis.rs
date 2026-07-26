use quote::ToTokens;
use syn::parse::Parse;
use syn::{punctuated::Punctuated, Token};

use crate::model::*;

#[derive(Default)]
struct CommonAttrOptions {
    name: Option<String>,
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
            exports.push(analyze_lean_function(
                function,
                FunctionOptions {
                    lean_name: options.name,
                },
            )?);
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
        lean_decl: None,
    })
}

pub fn analyze_lean_class_struct(item: &syn::ItemStruct) -> syn::Result<ClassTypeBinding> {
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "#[leanclass] does not support generic types yet",
        ));
    }

    let rust_name = item.ident.to_string();
    Ok(ClassTypeBinding {
        rust_name: rust_name.clone(),
        lean_name: rust_name.clone(),
        opaque_decl: format!("opaque {} : Type", rust_name),
    })
}

pub fn analyze_lean_class_impl(item: &syn::ItemImpl) -> syn::Result<ClassImplBinding> {
    let class_name = class_name_from_self_ty(&item.self_ty)?;

    if !item.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "#[leanclass] does not support generic impl blocks yet",
        ));
    }

    let mut methods = Vec::new();
    for impl_item in &item.items {
        if let syn::ImplItem::Fn(method) = impl_item {
            methods.push(analyze_lean_class_method(method, &class_name)?);
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

    let lean_name = format!("{}.{}", class_name, method_name);
    let ffi_symbol = format!("__lean_ffi_{}_{}", class_name, method_name);
    let mut type_parts = Vec::new();
    match receiver {
        ReceiverStyle::Ref | ReceiverStyle::MutRef | ReceiverStyle::Owned => {
            type_parts.push(class_name.to_string())
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
        lean_decl: Some(lean_decl),
    })
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
