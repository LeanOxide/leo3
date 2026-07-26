use proc_macro2::{Span, TokenStream};
use quote::quote;
use syn::Type;

#[derive(Clone)]
pub(crate) enum StoragePlan {
    Borrowed(Type),
    OptionBorrowed(Type),
    Result {
        ok: ResultArmPlan,
        err: ResultArmPlan,
    },
    OptionResult {
        ok: ResultArmPlan,
        err: ResultArmPlan,
    },
    Tuple {
        ty: Type,
    },
}

#[derive(Clone)]
pub(crate) enum ResultArmPlan {
    Borrowed(Type),
    Owned(Type),
}

#[derive(Clone)]
pub(crate) enum ReturnPlan {
    BorrowedString,
    BorrowedU8Slice,
    BorrowedVecU8,
    BorrowedVec,
    BorrowedSliceLike,
    VecU8,
    Option(Box<ReturnPlan>),
    Result {
        ok: Box<ReturnPlan>,
        err: Box<ReturnPlan>,
    },
    Tuple(Vec<ReturnPlan>),
    Generic(Type),
}

pub(crate) fn build_storage_plan(ty: &Type) -> Option<StoragePlan> {
    if build_tuple_plan(ty).is_some() {
        return Some(StoragePlan::Tuple { ty: ty.clone() });
    }

    if let Some((ok_ty, err_ty)) = result_parts(ty) {
        let ok = build_result_arm(ok_ty)?;
        let err = build_result_arm(err_ty)?;
        return Some(StoragePlan::Result { ok, err });
    }

    if let Some(inner) = option_inner(ty) {
        if let Some((ok_ty, err_ty)) = result_parts(inner) {
            let ok = build_result_arm(ok_ty)?;
            let err = build_result_arm(err_ty)?;
            return Some(StoragePlan::OptionResult { ok, err });
        }

        if supports_borrowed_storage(inner) {
            return Some(StoragePlan::OptionBorrowed(inner.clone()));
        }
    }

    if supports_borrowed_storage(ty) {
        return Some(StoragePlan::Borrowed(ty.clone()));
    }

    None
}

pub(crate) fn build_return_plan(ty: &Type) -> ReturnPlan {
    if is_borrowed_string(ty) {
        return ReturnPlan::BorrowedString;
    }

    if is_borrowed_u8_slice(ty) {
        return ReturnPlan::BorrowedU8Slice;
    }

    if is_borrowed_vec_u8(ty) {
        return ReturnPlan::BorrowedVecU8;
    }

    if borrowed_vec_inner(ty).is_some() {
        return ReturnPlan::BorrowedVec;
    }

    if borrowed_non_u8_slice_inner(ty).is_some() || borrowed_fixed_array_inner(ty).is_some() {
        return ReturnPlan::BorrowedSliceLike;
    }

    if is_vec_u8(ty) {
        return ReturnPlan::VecU8;
    }

    if let Some(inner) = option_inner(ty) {
        return ReturnPlan::Option(Box::new(build_return_plan(inner)));
    }

    if let Some((ok, err)) = result_parts(ty) {
        return ReturnPlan::Result {
            ok: Box::new(build_return_plan(ok)),
            err: Box::new(build_return_plan(err)),
        };
    }

    if let Some(items) = tuple_items(ty) {
        return ReturnPlan::Tuple(items.iter().map(build_return_plan).collect());
    }

    ReturnPlan::Generic(ty.clone())
}

fn build_tuple_plan(ty: &Type) -> Option<Vec<StoragePlan>> {
    let items = tuple_items(ty)?;
    items
        .iter()
        .map(|item| {
            if build_tuple_plan(item).is_some() {
                Some(StoragePlan::Tuple { ty: item.clone() })
            } else if supports_borrowed_storage(item) {
                Some(StoragePlan::Borrowed(item.clone()))
            } else {
                None
            }
        })
        .collect()
}

fn build_result_arm(ty: &Type) -> Option<ResultArmPlan> {
    if supports_borrowed_storage(ty) {
        Some(ResultArmPlan::Borrowed(ty.clone()))
    } else if !type_contains_reference(ty) {
        Some(ResultArmPlan::Owned(ty.clone()))
    } else {
        None
    }
}

fn supports_borrowed_storage(ty: &Type) -> bool {
    is_borrowed_string(ty)
        || is_borrowed_vec_u8(ty)
        || borrowed_vec_inner(ty).is_some()
        || borrowed_non_u8_slice_inner(ty).is_some()
        || borrowed_fixed_array_inner(ty).is_some()
}

pub(crate) fn supports_result_borrowed_alias_param(ty: &Type) -> bool {
    borrowed_storage_owned_type(ty).is_some()
}

pub(crate) fn borrowed_storage_owned_type(ty: &Type) -> Option<TokenStream> {
    if is_borrowed_string(ty) {
        return Some(quote! { String });
    }

    if is_borrowed_u8_slice(ty) {
        return Some(quote! { Vec<u8> });
    }

    if is_borrowed_vec_u8(ty) {
        return Some(quote! { Vec<u8> });
    }

    if let Some(inner) = borrowed_vec_inner(ty) {
        return Some(quote! { Vec<#inner> });
    }

    if let Some(inner) = borrowed_non_u8_slice_inner(ty) {
        return Some(quote! { Vec<#inner> });
    }

    if let Some(inner) = borrowed_fixed_array_inner(ty) {
        return Some(quote! { #inner });
    }

    None
}

pub(crate) fn result_storage_type(ty: &Type) -> Option<TokenStream> {
    if let Some(storage) = borrowed_storage_owned_type(ty) {
        Some(storage)
    } else if !type_contains_reference(ty) {
        Some(quote! { #ty })
    } else {
        None
    }
}

pub(crate) fn tuple_borrowed_alias_owned_type(ty: &Type) -> Option<TokenStream> {
    let items = tuple_items(ty)?;
    let item_types = items
        .iter()
        .map(|item| {
            if tuple_items(item).is_some() {
                tuple_borrowed_alias_owned_type(item)
            } else {
                borrowed_storage_owned_type(item)
            }
        })
        .collect::<Option<Vec<_>>>()?;
    Some(quote! { (#(#item_types),*) })
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn render_storage_plan_binding(
    name: &syn::Ident,
    arg_name: &syn::Ident,
    source_ty: &TokenStream,
    plan: &StoragePlan,
    leo3_crate: &TokenStream,
    counter: &mut usize,
    lean_source_type: fn(&Type, &TokenStream) -> TokenStream,
    generate_from_lean_expr: fn(&Type, TokenStream, &TokenStream, &mut usize) -> TokenStream,
) -> TokenStream {
    match plan {
        StoragePlan::Borrowed(ty) => {
            render_borrowed_storage_binding(name, arg_name, source_ty, ty, leo3_crate)
        }
        StoragePlan::OptionBorrowed(ty) => {
            render_option_borrowed_storage_binding(name, arg_name, source_ty, ty, leo3_crate)
        }
        StoragePlan::Result { ok, err } => render_result_storage_binding(
            name,
            arg_name,
            source_ty,
            ok,
            err,
            leo3_crate,
            counter,
            false,
            lean_source_type,
            generate_from_lean_expr,
        ),
        StoragePlan::OptionResult { ok, err } => render_result_storage_binding(
            name,
            arg_name,
            source_ty,
            ok,
            err,
            leo3_crate,
            counter,
            true,
            lean_source_type,
            generate_from_lean_expr,
        ),
        StoragePlan::Tuple { ty } => {
            let storage_name = quote::format_ident!("__{}_tuple_storage", name);
            let storage_ty = tuple_borrowed_alias_owned_type(ty).unwrap();
            let decode = generate_tuple_borrowed_storage_decode(
                ty,
                quote! { bound },
                leo3_crate,
                counter,
                lean_source_type,
                generate_from_lean_expr,
            );
            let view = generate_tuple_borrowed_alias_view(ty, quote! { #storage_name }).unwrap();
            quote! {
                let #storage_name: #storage_ty = {
                    let bound: #leo3_crate::LeanBound<'_, #source_ty> =
                        #leo3_crate::LeanBound::from_owned_ptr(lean, #arg_name);
                    #decode
                        .map_err(|e| #leo3_crate::LeanError::conversion(&format!(
                            "Failed to convert `{}` from Lean to Rust: {}",
                            stringify!(#name),
                            e
                        )))?
                };
                let #name = #view;
            }
        }
    }
}

pub(crate) fn render_return_plan(
    ty: &Type,
    value_expr: TokenStream,
    leo3_crate: &TokenStream,
    counter: &mut usize,
) -> TokenStream {
    render_built_return_plan(&build_return_plan(ty), value_expr, leo3_crate, counter)
}

fn render_borrowed_storage_binding(
    name: &syn::Ident,
    arg_name: &syn::Ident,
    source_ty: &TokenStream,
    ty: &Type,
    leo3_crate: &TokenStream,
) -> TokenStream {
    if is_borrowed_string(ty) {
        let storage_name = quote::format_ident!("__{}_string_storage", name);
        return quote! {
            let #storage_name = {
                let bound: #leo3_crate::LeanBound<'_, #source_ty> =
                    #leo3_crate::LeanBound::from_owned_ptr(lean, #arg_name);
                <String as #leo3_crate::conversion::FromLean>::from_lean(&bound)
                    .map_err(|e| #leo3_crate::LeanError::conversion(&format!(
                        "Failed to convert `{}` from Lean to Rust: {}",
                        stringify!(#name),
                        e
                    )))?
            };
            let #name = &#storage_name;
        };
    }

    if is_borrowed_vec_u8(ty) {
        let storage_name = quote::format_ident!("__{}_vec_storage", name);
        return quote! {
            let #storage_name = {
                let bound: #leo3_crate::LeanBound<'_, #source_ty> =
                    #leo3_crate::LeanBound::from_owned_ptr(lean, #arg_name);
                #leo3_crate::conversion::vec_u8_from_lean(&bound)
            };
            let #name = &#storage_name;
        };
    }

    if let Some(inner) = borrowed_vec_inner(ty) {
        let storage_name = quote::format_ident!("__{}_vec_storage", name);
        return quote! {
            let #storage_name = {
                let bound: #leo3_crate::LeanBound<'_, #source_ty> =
                    #leo3_crate::LeanBound::from_owned_ptr(lean, #arg_name);
                <Vec<#inner> as #leo3_crate::conversion::FromLean>::from_lean(&bound)
                    .map_err(|e| #leo3_crate::LeanError::conversion(&format!(
                        "Failed to convert `{}` from Lean to Rust: {}",
                        stringify!(#name),
                        e
                    )))?
            };
            let #name = &#storage_name;
        };
    }

    if let Some(inner) = borrowed_fixed_array_inner(ty) {
        let storage_name = quote::format_ident!("__{}_array_storage", name);
        return quote! {
            let #storage_name = {
                let bound: #leo3_crate::LeanBound<'_, #source_ty> =
                    #leo3_crate::LeanBound::from_owned_ptr(lean, #arg_name);
                <#inner as #leo3_crate::conversion::FromLean>::from_lean(&bound)
                    .map_err(|e| #leo3_crate::LeanError::conversion(&format!(
                        "Failed to convert `{}` from Lean to Rust: {}",
                        stringify!(#name),
                        e
                    )))?
            };
            let #name = &#storage_name;
        };
    }

    if let Some(inner) = borrowed_non_u8_slice_inner(ty) {
        let storage_name = quote::format_ident!("__{}_slice_storage", name);
        return quote! {
            let #storage_name = {
                let bound: #leo3_crate::LeanBound<'_, #source_ty> =
                    #leo3_crate::LeanBound::from_owned_ptr(lean, #arg_name);
                <Vec<#inner> as #leo3_crate::conversion::FromLean>::from_lean(&bound)
                    .map_err(|e| #leo3_crate::LeanError::conversion(&format!(
                        "Failed to convert `{}` from Lean to Rust: {}",
                        stringify!(#name),
                        e
                    )))?
            };
            let #name = #storage_name.as_slice();
        };
    }

    unreachable!("unsupported borrowed storage plan")
}

fn render_built_return_plan(
    plan: &ReturnPlan,
    value_expr: TokenStream,
    leo3_crate: &TokenStream,
    counter: &mut usize,
) -> TokenStream {
    match plan {
        ReturnPlan::BorrowedString => {
            quote! { #leo3_crate::types::LeanString::mk(lean, #value_expr.as_str()) }
        }
        ReturnPlan::BorrowedU8Slice => {
            quote! { #leo3_crate::conversion::slice_u8_into_lean(#value_expr, lean) }
        }
        ReturnPlan::BorrowedVecU8 => {
            quote! { #leo3_crate::conversion::slice_u8_into_lean(#value_expr.as_slice(), lean) }
        }
        ReturnPlan::BorrowedVec => {
            quote! { #leo3_crate::conversion::slice_into_lean(#value_expr.as_slice(), lean) }
        }
        ReturnPlan::BorrowedSliceLike => {
            quote! { #leo3_crate::conversion::slice_into_lean(#value_expr, lean) }
        }
        ReturnPlan::VecU8 => {
            quote! { #leo3_crate::conversion::vec_u8_into_lean(#value_expr, lean) }
        }
        ReturnPlan::Option(inner) => {
            let some_ident = fresh_ident("some_value", counter);
            let inner_expr =
                render_built_return_plan(inner, quote! { #some_ident }, leo3_crate, counter);
            quote! {
                match #value_expr {
                    None => #leo3_crate::types::LeanOption::none(lean),
                    Some(#some_ident) => {
                        let lean_value = #inner_expr?;
                        let any_value: #leo3_crate::instance::LeanBound<'_, #leo3_crate::instance::LeanAny> =
                            lean_value.cast();
                        #leo3_crate::types::LeanOption::some(any_value)
                    }
                }
            }
        }
        ReturnPlan::Result { ok, err } => {
            let ok_ident = fresh_ident("ok_value", counter);
            let err_ident = fresh_ident("err_value", counter);
            let ok_expr = render_built_return_plan(ok, quote! { #ok_ident }, leo3_crate, counter);
            let err_expr =
                render_built_return_plan(err, quote! { #err_ident }, leo3_crate, counter);
            quote! {
                match #value_expr {
                    Err(#err_ident) => {
                        let lean_error = #err_expr?;
                        let any_error: #leo3_crate::instance::LeanBound<'_, #leo3_crate::instance::LeanAny> =
                            lean_error.cast();
                        #leo3_crate::types::LeanExcept::error(any_error)
                    }
                    Ok(#ok_ident) => {
                        let lean_value = #ok_expr?;
                        let any_value: #leo3_crate::instance::LeanBound<'_, #leo3_crate::instance::LeanAny> =
                            lean_value.cast();
                        #leo3_crate::types::LeanExcept::ok(any_value)
                    }
                }
            }
        }
        ReturnPlan::Tuple(items) => {
            let value_ident = fresh_ident("tuple_value", counter);
            let head_ident = fresh_ident("tuple_head", counter);
            let tail_ident = fresh_ident("tuple_tail", counter);
            let head_expr =
                render_built_return_plan(&items[0], quote! { #head_ident }, leo3_crate, counter);
            let tail_value = tuple_tail_value_tokens(&value_ident, items.len() - 1);
            if items.len() == 2 {
                let tail_expr = render_built_return_plan(
                    &items[1],
                    quote! { #tail_ident },
                    leo3_crate,
                    counter,
                );
                quote! {
                    {
                        let #value_ident = #value_expr;
                        let (#head_ident, #tail_ident) = #value_ident;
                        let lean_head = #head_expr?;
                        let lean_tail = #tail_expr?;
                        #leo3_crate::types::LeanProd::mk(lean_head.cast(), lean_tail.cast())
                    }
                }
            } else {
                let tail_plan = ReturnPlan::Tuple(items[1..].to_vec());
                let tail_expr = render_built_return_plan(
                    &tail_plan,
                    quote! { #tail_ident },
                    leo3_crate,
                    counter,
                );
                quote! {
                    {
                        let #value_ident = #value_expr;
                        let (#head_ident, #tail_ident) = (#value_ident.0, #tail_value);
                        let lean_head = #head_expr?;
                        let lean_tail = #tail_expr?;
                        #leo3_crate::types::LeanProd::mk(lean_head.cast(), lean_tail.cast())
                    }
                }
            }
        }
        ReturnPlan::Generic(ty) => quote! { #leo3_crate::to_lean!(#value_expr, lean, #ty) },
    }
}

fn render_option_borrowed_storage_binding(
    name: &syn::Ident,
    arg_name: &syn::Ident,
    source_ty: &TokenStream,
    ty: &Type,
    leo3_crate: &TokenStream,
) -> TokenStream {
    let storage_name = quote::format_ident!("__{}_option_storage", name);
    let storage_ty = borrowed_storage_owned_type(ty).unwrap();
    let inner_source_ty = borrowed_storage_source_type(ty, leo3_crate).unwrap();
    let decode =
        generate_borrowed_storage_owned_decode(ty, quote! { typed_value }, leo3_crate).unwrap();
    let view = generate_borrowed_storage_view(ty, quote! { value }).unwrap();

    quote! {
        let #storage_name: Option<#storage_ty> = {
            let bound: #leo3_crate::LeanBound<'_, #source_ty> =
                #leo3_crate::LeanBound::from_owned_ptr(lean, #arg_name);
            match #leo3_crate::types::LeanOption::get(&bound) {
                None => Ok(None),
                Some(any_value) => {
                    let typed_value: #leo3_crate::LeanBound<'_, #inner_source_ty> =
                        any_value.cast();
                    #decode.map(Some)
                }
            }
            .map_err(|e| #leo3_crate::LeanError::conversion(&format!(
                "Failed to convert `{}` from Lean to Rust: {}",
                stringify!(#name),
                e
            )))?
        };
        let #name = #storage_name.as_ref().map(|value| #view);
    }
}

#[allow(clippy::too_many_arguments)]
fn render_result_storage_binding(
    name: &syn::Ident,
    arg_name: &syn::Ident,
    source_ty: &TokenStream,
    ok: &ResultArmPlan,
    err: &ResultArmPlan,
    leo3_crate: &TokenStream,
    counter: &mut usize,
    inside_option: bool,
    lean_source_type: fn(&Type, &TokenStream) -> TokenStream,
    generate_from_lean_expr: fn(&Type, TokenStream, &TokenStream, &mut usize) -> TokenStream,
) -> TokenStream {
    let ok_storage_name = if inside_option {
        quote::format_ident!("__{}_option_result_ok_storage", name)
    } else {
        quote::format_ident!("__{}_result_ok_storage", name)
    };
    let err_storage_name = if inside_option {
        quote::format_ident!("__{}_option_result_err_storage", name)
    } else {
        quote::format_ident!("__{}_result_err_storage", name)
    };
    let ok_ty = result_arm_ty(ok);
    let err_ty = result_arm_ty(err);
    let ok_source_ty = lean_source_type(ok_ty, leo3_crate);
    let err_source_ty = lean_source_type(err_ty, leo3_crate);
    let ok_storage_ty = result_storage_type(ok_ty).unwrap();
    let err_storage_ty = result_storage_type(err_ty).unwrap();
    let ok_decode = generate_result_storage_decode(
        ok_ty,
        quote! { typed_ok },
        leo3_crate,
        counter,
        generate_from_lean_expr,
    );
    let err_decode = generate_result_storage_decode(
        err_ty,
        quote! { typed_err },
        leo3_crate,
        counter,
        generate_from_lean_expr,
    );
    let ok_view = generate_result_output_view(ok_ty, quote! { #ok_storage_name }).unwrap();
    let err_view = generate_result_output_view(err_ty, quote! { #err_storage_name }).unwrap();

    if inside_option {
        let inner_source_ty = quote! { #leo3_crate::types::LeanExcept };
        return quote! {
            let mut #ok_storage_name: Option<#ok_storage_ty> = None;
            let mut #err_storage_name: Option<#err_storage_ty> = None;
            let #name = {
                let bound: #leo3_crate::LeanBound<'_, #source_ty> =
                    #leo3_crate::LeanBound::from_owned_ptr(lean, #arg_name);
                match #leo3_crate::types::LeanOption::get(&bound) {
                    None => None,
                    Some(any_value) => {
                        let typed_result: #leo3_crate::LeanBound<'_, #inner_source_ty> =
                            any_value.cast();
                        match #leo3_crate::types::LeanExcept::toRustResult(&typed_result)? {
                            Ok(any_ok) => {
                                let typed_ok: #leo3_crate::LeanBound<'_, #ok_source_ty> =
                                    any_ok.cast();
                                let ok_value = #ok_decode
                                    .map_err(|e| #leo3_crate::LeanError::conversion(&format!(
                                        "Failed to convert `{}` from Lean to Rust: {}",
                                        stringify!(#name),
                                        e
                                    )))?;
                                #ok_storage_name = Some(ok_value);
                                Some(Ok(#ok_view))
                            }
                            Err(any_err) => {
                                let typed_err: #leo3_crate::LeanBound<'_, #err_source_ty> =
                                    any_err.cast();
                                let err_value = #err_decode
                                    .map_err(|e| #leo3_crate::LeanError::conversion(&format!(
                                        "Failed to convert `{}` from Lean to Rust: {}",
                                        stringify!(#name),
                                        e
                                    )))?;
                                #err_storage_name = Some(err_value);
                                Some(Err(#err_view))
                            }
                        }
                    }
                }
            };
        };
    }

    quote! {
        let mut #ok_storage_name: Option<#ok_storage_ty> = None;
        let mut #err_storage_name: Option<#err_storage_ty> = None;
        let #name = {
            let bound: #leo3_crate::LeanBound<'_, #source_ty> =
                #leo3_crate::LeanBound::from_owned_ptr(lean, #arg_name);
            match #leo3_crate::types::LeanExcept::toRustResult(&bound)? {
                Ok(any_ok) => {
                    let typed_ok: #leo3_crate::LeanBound<'_, #ok_source_ty> =
                        any_ok.cast();
                    let ok_value = #ok_decode
                        .map_err(|e| #leo3_crate::LeanError::conversion(&format!(
                            "Failed to convert `{}` from Lean to Rust: {}",
                            stringify!(#name),
                            e
                        )))?;
                    #ok_storage_name = Some(ok_value);
                    Ok(#ok_view)
                }
                Err(any_err) => {
                    let typed_err: #leo3_crate::LeanBound<'_, #err_source_ty> =
                        any_err.cast();
                    let err_value = #err_decode
                        .map_err(|e| #leo3_crate::LeanError::conversion(&format!(
                            "Failed to convert `{}` from Lean to Rust: {}",
                            stringify!(#name),
                            e
                        )))?;
                    #err_storage_name = Some(err_value);
                    Err(#err_view)
                }
            }
        };
    }
}

fn result_arm_ty(plan: &ResultArmPlan) -> &Type {
    match plan {
        ResultArmPlan::Borrowed(ty) | ResultArmPlan::Owned(ty) => ty,
    }
}

fn generate_result_storage_decode(
    ty: &Type,
    obj_expr: TokenStream,
    leo3_crate: &TokenStream,
    counter: &mut usize,
    generate_from_lean_expr: fn(&Type, TokenStream, &TokenStream, &mut usize) -> TokenStream,
) -> TokenStream {
    if supports_result_borrowed_alias_param(ty) {
        generate_borrowed_storage_owned_decode(ty, obj_expr, leo3_crate).unwrap()
    } else {
        generate_from_lean_expr(ty, obj_expr, leo3_crate, counter)
    }
}

#[allow(clippy::only_used_in_recursion)]
fn generate_tuple_borrowed_storage_decode(
    ty: &Type,
    obj_expr: TokenStream,
    leo3_crate: &TokenStream,
    counter: &mut usize,
    lean_source_type: fn(&Type, &TokenStream) -> TokenStream,
    generate_from_lean_expr: fn(&Type, TokenStream, &TokenStream, &mut usize) -> TokenStream,
) -> TokenStream {
    let items = tuple_items(ty).expect("tuple decode requires tuple type");
    let storage_ty = tuple_borrowed_alias_owned_type(ty).unwrap();
    let head_typed = fresh_ident("tuple_head_typed", counter);
    let tail_typed = fresh_ident("tuple_tail_typed", counter);
    let head_value = fresh_ident("tuple_head_value", counter);
    let tail_value = fresh_ident("tuple_tail_value", counter);
    let head_source = lean_source_type(&items[0], leo3_crate);
    let head_expr = if tuple_items(&items[0]).is_some() {
        generate_tuple_borrowed_storage_decode(
            &items[0],
            quote! { #head_typed },
            leo3_crate,
            counter,
            lean_source_type,
            generate_from_lean_expr,
        )
    } else {
        generate_borrowed_storage_owned_decode(&items[0], quote! { #head_typed }, leo3_crate)
            .unwrap()
    };

    if items.len() == 2 {
        let tail_source = lean_source_type(&items[1], leo3_crate);
        let tail_expr = if tuple_items(&items[1]).is_some() {
            generate_tuple_borrowed_storage_decode(
                &items[1],
                quote! { #tail_typed },
                leo3_crate,
                counter,
                lean_source_type,
                generate_from_lean_expr,
            )
        } else {
            generate_borrowed_storage_owned_decode(&items[1], quote! { #tail_typed }, leo3_crate)
                .unwrap()
        };
        return quote! {
            {
                let #head_typed: #leo3_crate::LeanBound<'_, #head_source> =
                    #leo3_crate::types::LeanProd::fst(&#obj_expr).cast();
                let #tail_typed: #leo3_crate::LeanBound<'_, #tail_source> =
                    #leo3_crate::types::LeanProd::snd(&#obj_expr).cast();
                let #head_value = #head_expr?;
                let #tail_value = #tail_expr?;
                ::std::result::Result::<#storage_ty, #leo3_crate::LeanError>::Ok((
                    #head_value,
                    #tail_value
                ))
            }
        };
    }

    let tail_ty = tuple_tail_type(&items);
    let tail_source = lean_source_type(&tail_ty, leo3_crate);
    let tail_expr = generate_tuple_borrowed_storage_decode(
        &tail_ty,
        quote! { #tail_typed },
        leo3_crate,
        counter,
        lean_source_type,
        generate_from_lean_expr,
    );
    let tuple_unpack = tuple_unpack_tokens(&tail_value, items.len() - 1);
    quote! {
        {
            let #head_typed: #leo3_crate::LeanBound<'_, #head_source> =
                #leo3_crate::types::LeanProd::fst(&#obj_expr).cast();
            let #tail_typed: #leo3_crate::LeanBound<'_, #tail_source> =
                #leo3_crate::types::LeanProd::snd(&#obj_expr).cast();
            let #head_value = #head_expr?;
            let #tail_value = #tail_expr?;
            ::std::result::Result::<#storage_ty, #leo3_crate::LeanError>::Ok((
                #head_value,
                #tuple_unpack
            ))
        }
    }
}

fn borrowed_storage_source_type(ty: &Type, leo3_crate: &TokenStream) -> Option<TokenStream> {
    if is_borrowed_string(ty) {
        Some(quote! { #leo3_crate::types::LeanString })
    } else if is_borrowed_u8_slice(ty) || is_borrowed_vec_u8(ty) {
        Some(quote! { #leo3_crate::types::LeanByteArray })
    } else if borrowed_vec_inner(ty).is_some()
        || borrowed_non_u8_slice_inner(ty).is_some()
        || borrowed_fixed_array_inner(ty).is_some()
    {
        Some(quote! { #leo3_crate::types::LeanArray })
    } else {
        None
    }
}

fn generate_borrowed_storage_owned_decode(
    ty: &Type,
    obj_expr: TokenStream,
    leo3_crate: &TokenStream,
) -> Option<TokenStream> {
    if is_borrowed_string(ty) {
        return Some(
            quote! { <String as #leo3_crate::conversion::FromLean>::from_lean(&#obj_expr) },
        );
    }

    if is_borrowed_u8_slice(ty) {
        return Some(quote! {
            ::std::result::Result::<Vec<u8>, #leo3_crate::LeanError>::Ok(
                #leo3_crate::conversion::vec_u8_from_lean(&#obj_expr)
            )
        });
    }

    if is_borrowed_vec_u8(ty) {
        return Some(quote! {
            ::std::result::Result::<Vec<u8>, #leo3_crate::LeanError>::Ok(
                #leo3_crate::conversion::vec_u8_from_lean(&#obj_expr)
            )
        });
    }

    if let Some(inner) = borrowed_vec_inner(ty) {
        return Some(
            quote! { <Vec<#inner> as #leo3_crate::conversion::FromLean>::from_lean(&#obj_expr) },
        );
    }

    if let Some(inner) = borrowed_non_u8_slice_inner(ty) {
        return Some(
            quote! { <Vec<#inner> as #leo3_crate::conversion::FromLean>::from_lean(&#obj_expr) },
        );
    }

    if let Some(inner) = borrowed_fixed_array_inner(ty) {
        return Some(
            quote! { <#inner as #leo3_crate::conversion::FromLean>::from_lean(&#obj_expr) },
        );
    }

    None
}

fn generate_borrowed_storage_view(ty: &Type, ref_expr: TokenStream) -> Option<TokenStream> {
    if is_borrowed_string(ty) || is_borrowed_vec_u8(ty) || borrowed_vec_inner(ty).is_some() {
        return Some(quote! { #ref_expr });
    }

    if is_borrowed_u8_slice(ty) || borrowed_non_u8_slice_inner(ty).is_some() {
        return Some(quote! { (#ref_expr).as_slice() });
    }

    if borrowed_fixed_array_inner(ty).is_some() {
        return Some(quote! { #ref_expr });
    }

    None
}

fn generate_result_output_view(ty: &Type, storage_expr: TokenStream) -> Option<TokenStream> {
    if supports_result_borrowed_alias_param(ty) {
        return generate_borrowed_storage_view(ty, quote! { #storage_expr.as_ref().unwrap() });
    }

    if !type_contains_reference(ty) {
        return Some(quote! { #storage_expr.take().unwrap() });
    }

    None
}

fn generate_tuple_borrowed_alias_view(ty: &Type, storage_expr: TokenStream) -> Option<TokenStream> {
    let items = tuple_items(ty)?;
    let fields = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let index = syn::Index::from(index);
            if tuple_items(item).is_some() {
                generate_tuple_borrowed_alias_view(item, quote! { #storage_expr.#index })
            } else {
                generate_borrowed_storage_view(item, quote! { &#storage_expr.#index })
            }
        })
        .collect::<Option<Vec<_>>>()?;
    Some(quote! { (#(#fields),*) })
}

fn tuple_tail_type(items: &[Type]) -> Type {
    let tail = &items[1..];
    syn::parse_quote! { (#(#tail),*) }
}

fn tuple_unpack_tokens(value_ident: &syn::Ident, count: usize) -> TokenStream {
    let indexes = (0..count).map(syn::Index::from).collect::<Vec<_>>();
    quote! { #(#value_ident.#indexes),* }
}

fn tuple_tail_value_tokens(value_ident: &syn::Ident, count: usize) -> TokenStream {
    let indexes = (1..=count).map(syn::Index::from).collect::<Vec<_>>();
    quote! { (#(#value_ident.#indexes),*) }
}

fn fresh_ident(prefix: &str, counter: &mut usize) -> syn::Ident {
    let ident = syn::Ident::new(&format!("{prefix}_{counter}"), Span::call_site());
    *counter += 1;
    ident
}

pub(crate) fn tuple_items(ty: &Type) -> Option<Vec<Type>> {
    match ty {
        Type::Tuple(tuple) if tuple.elems.len() >= 2 => Some(tuple.elems.iter().cloned().collect()),
        _ => None,
    }
}

pub(crate) fn option_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let last = type_path.path.segments.last()?;
    if last.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    match args.args.first()? {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    }
}

pub(crate) fn result_parts(ty: &Type) -> Option<(&Type, &Type)> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let last = type_path.path.segments.last()?;
    if last.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    let mut type_args = args.args.iter().filter_map(|arg| match arg {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    Some((type_args.next()?, type_args.next()?))
}

fn vec_inner(ty: &Type) -> Option<&Type> {
    let Type::Path(type_path) = ty else {
        return None;
    };
    let last = type_path.path.segments.last()?;
    if last.ident != "Vec" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    match args.args.first()? {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    }
}

pub(crate) fn is_vec_u8(ty: &Type) -> bool {
    matches!(vec_inner(ty), Some(inner) if is_u8_type(inner))
}

pub(crate) fn is_borrowed_u8_slice(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) if reference.mutability.is_none() => {
            matches!(&*reference.elem, Type::Slice(slice) if is_u8_type(&slice.elem))
        }
        _ => false,
    }
}

pub(crate) fn borrowed_vec_inner(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Reference(reference) if reference.mutability.is_none() => {
            vec_inner(reference.elem.as_ref()).filter(|inner| !is_u8_type(inner))
        }
        _ => None,
    }
}

pub(crate) fn is_borrowed_vec_u8(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) if reference.mutability.is_none() => {
            is_vec_u8(reference.elem.as_ref())
        }
        _ => false,
    }
}

pub(crate) fn borrowed_non_u8_slice_inner(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Reference(reference) if reference.mutability.is_none() => match &*reference.elem {
            Type::Slice(slice) if !is_u8_type(&slice.elem) => Some(&slice.elem),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn borrowed_fixed_array_inner(ty: &Type) -> Option<&Type> {
    match ty {
        Type::Reference(reference) if reference.mutability.is_none() => match &*reference.elem {
            Type::Array(_) => Some(reference.elem.as_ref()),
            _ => None,
        },
        _ => None,
    }
}

pub(crate) fn is_borrowed_fixed_array(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) if reference.mutability.is_none() => {
            matches!(&*reference.elem, Type::Array(_))
        }
        _ => false,
    }
}

pub(crate) fn is_borrowed_string(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) if reference.mutability.is_none() => {
            matches!(&*reference.elem, Type::Path(type_path) if path_is_string(type_path))
        }
        _ => false,
    }
}

pub(crate) fn is_borrowed_str(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) if reference.mutability.is_none() => {
            matches!(&*reference.elem, Type::Path(type_path) if path_is_simple_ident(type_path, "str"))
        }
        _ => false,
    }
}

pub(crate) fn is_u8_type(ty: &Type) -> bool {
    matches!(ty, Type::Path(type_path) if path_is_simple_ident(type_path, "u8"))
}

fn path_is_string(type_path: &syn::TypePath) -> bool {
    let segments = type_path
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

fn path_is_simple_ident(type_path: &syn::TypePath, ident: &str) -> bool {
    type_path.qself.is_none()
        && type_path.path.segments.len() == 1
        && type_path
            .path
            .segments
            .first()
            .map(|segment| segment.ident == ident)
            .unwrap_or(false)
}

pub(crate) fn type_contains_reference(ty: &Type) -> bool {
    match ty {
        Type::Reference(_) => true,
        Type::Paren(paren) => type_contains_reference(&paren.elem),
        Type::Group(group) => type_contains_reference(&group.elem),
        Type::Array(array) => type_contains_reference(&array.elem),
        Type::Slice(slice) => type_contains_reference(&slice.elem),
        Type::Tuple(tuple) => tuple.elems.iter().any(type_contains_reference),
        Type::Path(type_path) => type_path.path.segments.iter().any(|segment| {
            match &segment.arguments {
                syn::PathArguments::AngleBracketed(args) => args.args.iter().any(|arg| {
                    matches!(arg, syn::GenericArgument::Type(inner) if type_contains_reference(inner))
                }),
                _ => false,
            }
        }),
        _ => false,
    }
}
