//! Implementation of the `#[leanfn]` macro.

use leo3_binding_ir::{
    analyze_concrete_instance, analyze_lean_function, quote_runtime_function_metadata,
    substitute_type, ConcreteAttr, FunctionBinding, FunctionOptions,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::parse::Parse;

use crate::conversion_plan::{
    borrowed_non_u8_slice_inner, borrowed_vec_inner, is_borrowed_fixed_array, is_borrowed_str,
    is_borrowed_string, is_borrowed_u8_slice, is_borrowed_vec_u8, is_vec_u8, option_inner,
    render_return_plan, result_parts,
};
use crate::{get_leo3_crate, CommonOptions};

pub struct ConcreteInstance {
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
            if input.peek(syn::Ident) && input.peek2(syn::Token![=]) {
                let ident: syn::Ident = input.parse()?;
                if ident != "name" {
                    return Err(syn::Error::new(ident.span(), "expected `name`"));
                }
                let _: syn::Token![=] = input.parse()?;
                let lit: syn::LitStr = input.parse()?;
                name = Some(lit.value());
            } else {
                let ty: syn::Type = input.parse()?;
                types.push(ty);
            }

            if !input.is_empty() {
                let _: syn::Token![,] = input.parse()?;
            }
        }

        Ok(ConcreteArgsParser { types, name })
    }
}

fn parse_concrete_meta(list: &syn::MetaList) -> syn::Result<ConcreteInstance> {
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
    Ok(ConcreteInstance {
        types: args.types,
        name,
    })
}

/// Options for the `#[leanfn]` attribute
pub struct LeanFunctionOptions {
    pub common: CommonOptions,
    pub concretes: Vec<ConcreteInstance>,
}

impl Parse for LeanFunctionOptions {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        use syn::punctuated::Punctuated;

        let attrs: Punctuated<syn::Meta, syn::Token![,]> =
            input.parse_terminated(syn::Meta::parse, syn::Token![,])?;

        let mut common = CommonOptions::default();
        let mut concretes = Vec::new();

        for attr in attrs {
            match attr {
                syn::Meta::NameValue(nv) => {
                    if nv.path.is_ident("name") {
                        if let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(s),
                            ..
                        }) = nv.value
                        {
                            common.name = Some(s);
                        }
                    } else if nv.path.is_ident("crate") {
                        if let syn::Expr::Path(path) = nv.value {
                            common.krate = Some(path.path);
                        }
                    }
                }
                syn::Meta::List(list) if list.path.is_ident("concrete") => {
                    concretes.push(parse_concrete_meta(&list)?);
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        other,
                        "Expected name-value attribute like `name = \"...\"`",
                    ))
                }
            }
        }

        Ok(Self { common, concretes })
    }
}

/// Information extracted from a function signature
struct FunctionInfo {
    rust_name: syn::Ident,
    lean_name: String,
    params: Vec<(syn::Ident, syn::Type)>,
    return_type: syn::Type,
    #[allow(unused)]
    vis: syn::Visibility,
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

/// Analyze a function signature and extract relevant information
fn analyze_function(
    func: &syn::ItemFn,
    lean_name: &str,
    type_mapping: Option<&std::collections::HashMap<String, syn::Type>>,
) -> syn::Result<FunctionInfo> {
    let rust_name = func.sig.ident.clone();

    let mut params = Vec::new();
    for input in &func.sig.inputs {
        match input {
            syn::FnArg::Typed(pat_type) => {
                let name = match &*pat_type.pat {
                    syn::Pat::Ident(ident) => ident.ident.clone(),
                    _ => {
                        return Err(syn::Error::new_spanned(
                            pat_type,
                            "Only simple parameter patterns are supported",
                        ))
                    }
                };
                let ty = match type_mapping {
                    Some(mapping) => substitute_type(&pat_type.ty, mapping),
                    None => (*pat_type.ty).clone(),
                };
                params.push((name, ty));
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
        syn::ReturnType::Default => syn::parse_quote! { () },
        syn::ReturnType::Type(_, ty) => match type_mapping {
            Some(mapping) => substitute_type(ty, mapping),
            None => (**ty).clone(),
        },
    };

    if type_mapping.is_none() && !func.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &func.sig.generics,
            "Generic functions are not supported yet",
        ));
    }

    Ok(FunctionInfo {
        rust_name,
        lean_name: lean_name.to_string(),
        params,
        return_type,
        vis: func.vis.clone(),
    })
}

/// Generate parameter conversion code
fn generate_param_conversions(
    params: &[(syn::Ident, syn::Type)],
    leo3_crate: &TokenStream,
) -> Vec<TokenStream> {
    let mut counter = 0usize;
    params
        .iter()
        .enumerate()
        .map(|(i, (name, ty))| {
            let arg_name = format_ident!("arg{}", i);
            let source_ty = lean_source_type(ty, leo3_crate);
            if let Some(plan) = crate::conversion_plan::build_storage_plan(ty) {
                return crate::conversion_plan::render_storage_plan_binding(
                    name,
                    &arg_name,
                    &source_ty,
                    &plan,
                    leo3_crate,
                    &mut counter,
                    lean_source_type,
                    generate_from_lean_expr,
                );
            }
            let from_expr = generate_from_lean_expr(ty, quote! { bound }, leo3_crate, &mut counter);
            quote! {
                let #name = {
                    let bound: #leo3_crate::LeanBound<'_, #source_ty> =
                        #leo3_crate::LeanBound::from_owned_ptr(lean, #arg_name);
                    #from_expr
                        .map_err(|e| #leo3_crate::LeanError::conversion(&format!(
                            "Failed to convert `{}` from Lean to Rust: {}",
                            stringify!(#name),
                            e
                        )))?
                };
            }
        })
        .collect()
}

/// Generate result conversion code
fn generate_result_conversion(return_type: &syn::Type, leo3_crate: &TokenStream) -> TokenStream {
    // Check if return type is unit ()
    if matches!(return_type, syn::Type::Tuple(t) if t.elems.is_empty()) {
        // For unit return type, return a unit Lean object (constructor 0 with 0 fields)
        quote! {
            Ok(unsafe {
                #leo3_crate::ffi::lean_alloc_ctor(0, 0, 0)
            })
        }
    } else {
        let mut counter = 0usize;
        let into_expr =
            generate_into_lean_expr(return_type, quote! { result }, leo3_crate, &mut counter);
        quote! {
            {
                let lean_result = #into_expr
                    .map_err(|e| #leo3_crate::LeanError::conversion(&format!(
                        "Failed to convert Rust result to Lean: {}",
                        e
                    )))?;
                Ok(lean_result.into_ptr())
            }
        }
    }
}

fn lean_source_type(ty: &syn::Type, leo3_crate: &TokenStream) -> TokenStream {
    if is_borrowed_str(ty) || is_borrowed_string(ty) {
        quote! { #leo3_crate::types::LeanString }
    } else if is_borrowed_u8_slice(ty) || is_vec_u8(ty) || is_borrowed_vec_u8(ty) {
        quote! { #leo3_crate::types::LeanByteArray }
    } else if is_borrowed_fixed_array(ty)
        || borrowed_non_u8_slice_inner(ty).is_some()
        || borrowed_vec_inner(ty).is_some()
    {
        quote! { #leo3_crate::types::LeanArray }
    } else if option_inner(ty).is_some() {
        quote! { #leo3_crate::types::LeanOption }
    } else if result_parts(ty).is_some() {
        quote! { #leo3_crate::types::LeanExcept }
    } else if tuple_items(ty).is_some() {
        quote! { #leo3_crate::types::LeanProd }
    } else {
        quote! { <#ty as #leo3_crate::conversion::FromLean>::Source }
    }
}

fn generate_from_lean_expr(
    ty: &syn::Type,
    obj_expr: TokenStream,
    leo3_crate: &TokenStream,
    counter: &mut usize,
) -> TokenStream {
    if is_borrowed_str(ty) {
        return quote! { #leo3_crate::types::LeanString::cstr(&#obj_expr) };
    }

    if is_borrowed_string(ty) {
        return quote! {
            <String as #leo3_crate::conversion::FromLean>::from_lean(&#obj_expr)
        };
    }

    if is_borrowed_u8_slice(ty) {
        return quote! { Ok(unsafe { #leo3_crate::types::LeanByteArray::as_slice(&#obj_expr) }) };
    }

    if is_vec_u8(ty) {
        return quote! { Ok(#leo3_crate::conversion::vec_u8_from_lean(&#obj_expr)) };
    }

    if let Some(inner) = option_inner(ty) {
        let any_ident = fresh_ident("any_value", counter);
        let typed_ident = fresh_ident("typed_value", counter);
        let value_ident = fresh_ident("rust_value", counter);
        let inner_source = lean_source_type(inner, leo3_crate);
        let inner_expr =
            generate_from_lean_expr(inner, quote! { #typed_ident }, leo3_crate, counter);
        return quote! {
            match #leo3_crate::types::LeanOption::get(&#obj_expr) {
                None => Ok(None),
                Some(#any_ident) => {
                    let #typed_ident: #leo3_crate::LeanBound<'_, #inner_source> = #any_ident.cast();
                    let #value_ident = #inner_expr?;
                    Ok(Some(#value_ident))
                }
            }
        };
    }

    if let Some((ok_ty, err_ty)) = result_parts(ty) {
        let ok_any = fresh_ident("ok_any", counter);
        let err_any = fresh_ident("err_any", counter);
        let ok_typed = fresh_ident("ok_typed", counter);
        let err_typed = fresh_ident("err_typed", counter);
        let ok_value = fresh_ident("ok_value", counter);
        let err_value = fresh_ident("err_value", counter);
        let ok_source = lean_source_type(ok_ty, leo3_crate);
        let err_source = lean_source_type(err_ty, leo3_crate);
        let ok_expr = generate_from_lean_expr(ok_ty, quote! { #ok_typed }, leo3_crate, counter);
        let err_expr = generate_from_lean_expr(err_ty, quote! { #err_typed }, leo3_crate, counter);
        return quote! {
            ::std::result::Result::<#ty, #leo3_crate::LeanError>::Ok(
                match #leo3_crate::types::LeanExcept::toRustResult(&#obj_expr)? {
                Err(#err_any) => {
                    let #err_typed: #leo3_crate::LeanBound<'_, #err_source> = #err_any.cast();
                    let #err_value = #err_expr?;
                    Err(#err_value)
                }
                Ok(#ok_any) => {
                    let #ok_typed: #leo3_crate::LeanBound<'_, #ok_source> = #ok_any.cast();
                    let #ok_value = #ok_expr?;
                    Ok(#ok_value)
                }
            })
        };
    }

    if let Some(items) = tuple_items(ty) {
        let head_typed = fresh_ident("head_typed", counter);
        let tail_typed = fresh_ident("tail_typed", counter);
        let head_value = fresh_ident("head_value", counter);
        let tail_value = fresh_ident("tail_value", counter);
        let head_source = lean_source_type(&items[0], leo3_crate);
        let head_expr =
            generate_from_lean_expr(&items[0], quote! { #head_typed }, leo3_crate, counter);
        if items.len() == 2 {
            let tail_source = lean_source_type(&items[1], leo3_crate);
            let tail_expr =
                generate_from_lean_expr(&items[1], quote! { #tail_typed }, leo3_crate, counter);
            return quote! {
                {
                    let #head_typed: #leo3_crate::LeanBound<'_, #head_source> =
                        #leo3_crate::types::LeanProd::fst(&#obj_expr).cast();
                    let #tail_typed: #leo3_crate::LeanBound<'_, #tail_source> =
                        #leo3_crate::types::LeanProd::snd(&#obj_expr).cast();
                    let #head_value = #head_expr?;
                    let #tail_value = #tail_expr?;
                    Ok((#head_value, #tail_value))
                }
            };
        }

        let tail_ty = tuple_tail_type(&items);
        let tail_source = lean_source_type(&tail_ty, leo3_crate);
        let tail_expr =
            generate_from_lean_expr(&tail_ty, quote! { #tail_typed }, leo3_crate, counter);
        let tuple_unpack = tuple_unpack_tokens(&tail_value, items.len() - 1);
        return quote! {
            {
                let #head_typed: #leo3_crate::LeanBound<'_, #head_source> =
                    #leo3_crate::types::LeanProd::fst(&#obj_expr).cast();
                let #tail_typed: #leo3_crate::LeanBound<'_, #tail_source> =
                    #leo3_crate::types::LeanProd::snd(&#obj_expr).cast();
                let #head_value = #head_expr?;
                let #tail_value = #tail_expr?;
                Ok((#head_value, #tuple_unpack))
            }
        };
    }

    quote! { <#ty as #leo3_crate::conversion::FromLean>::from_lean(&#obj_expr) }
}

fn generate_into_lean_expr(
    ty: &syn::Type,
    value_expr: TokenStream,
    leo3_crate: &TokenStream,
    counter: &mut usize,
) -> TokenStream {
    render_return_plan(ty, value_expr, leo3_crate, counter)
}

fn tuple_items(ty: &syn::Type) -> Option<Vec<syn::Type>> {
    match ty {
        syn::Type::Tuple(tuple) if tuple.elems.len() >= 2 => {
            Some(tuple.elems.iter().cloned().collect())
        }
        _ => None,
    }
}

fn tuple_tail_type(items: &[syn::Type]) -> syn::Type {
    let tail = &items[1..];
    syn::parse_quote! { (#(#tail),*) }
}

fn tuple_unpack_tokens(value_ident: &syn::Ident, count: usize) -> TokenStream {
    let indexes = (0..count).map(syn::Index::from).collect::<Vec<_>>();
    quote! { #(#value_ident.#indexes),* }
}

fn fresh_ident(prefix: &str, counter: &mut usize) -> syn::Ident {
    let ident = syn::Ident::new(&format!("{prefix}_{counter}"), Span::call_site());
    *counter += 1;
    ident
}

/// Generate the FFI wrapper function with panic boundary
fn generate_ffi_wrapper(info: &FunctionInfo, leo3_crate: &TokenStream) -> TokenStream {
    let lean_name = &info.lean_name;
    // Internal name to avoid conflicts with imported names
    let internal_ffi_name = format_ident!("__ffi_{}", lean_name);

    let param_count = info.params.len();
    let ffi_params: Vec<_> = (0..param_count)
        .map(|i| {
            let arg_name = format_ident!("arg{}", i);
            quote! { #arg_name: #leo3_crate::ffi::object::lean_obj_arg }
        })
        .collect();

    let wrapper_call_args: Vec<_> = (0..param_count)
        .map(|i| {
            let arg_name = format_ident!("arg{}", i);
            quote! { #arg_name }
        })
        .collect();

    quote! {
        #[no_mangle]
        #[export_name = #lean_name]
        pub unsafe extern "C" fn #internal_ffi_name(
            #(#ffi_params),*
        ) -> #leo3_crate::ffi::object::lean_obj_res {
            // Panic safety boundary - catch any panics and convert to Lean panic
            match ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                __ffi_try_wrapper(#(#wrapper_call_args),*)
            })) {
                Ok(Ok(ptr)) => ptr,
                Ok(Err(err)) => {
                    let lean = #leo3_crate::Lean::assume_initialized();
                    #leo3_crate::__private::lean_panic_message(lean, &err.to_string())
                }
                Err(payload) => {
                    let lean = #leo3_crate::Lean::assume_initialized();
                    let message = #leo3_crate::__private::panic_payload_message(payload.as_ref());
                    #leo3_crate::__private::lean_panic_message(lean, &message)
                }
            }
        }
    }
}

/// Generate the conversion wrapper (separate for testing)
fn generate_conversion_wrapper(
    info: &FunctionInfo,
    leo3_crate: &TokenStream,
    turbofish: Option<&TokenStream>,
) -> TokenStream {
    let rust_name = &info.rust_name;
    let param_count = info.params.len();

    let ffi_params: Vec<_> = (0..param_count)
        .map(|i| {
            let arg_name = format_ident!("arg{}", i);
            quote! { #arg_name: #leo3_crate::ffi::object::lean_obj_arg }
        })
        .collect();

    let param_conversions = generate_param_conversions(&info.params, leo3_crate);
    let result_conversion = generate_result_conversion(&info.return_type, leo3_crate);

    let call_args: Vec<_> = info
        .params
        .iter()
        .map(|(name, _)| quote! { #name })
        .collect();

    let call_expr = match turbofish {
        Some(tf) => quote! { #rust_name::#tf(#(#call_args),*) },
        None => quote! { #rust_name(#(#call_args),*) },
    };

    let call_and_return = if matches!(info.return_type, syn::Type::Tuple(ref t) if t.elems.is_empty())
    {
        quote! {
            #call_expr;
            #result_conversion
        }
    } else {
        quote! {
            let result = #call_expr;
            #result_conversion
        }
    };

    quote! {
        pub(crate) unsafe fn __ffi_try_wrapper(
            #(#ffi_params),*
        ) -> #leo3_crate::err::LeanResult<#leo3_crate::ffi::object::lean_obj_res> {
            let lean = #leo3_crate::Lean::assume_initialized();

            #(#param_conversions)*

            #call_and_return
        }
    }
}

/// Generate metadata
fn generate_metadata(binding: &FunctionBinding, leo3_crate: &TokenStream) -> TokenStream {
    let lean_name = &binding.lean_name;
    let metadata = quote_runtime_function_metadata(binding, leo3_crate);

    quote! {
        pub const LEAN_NAME: &str = #lean_name;

        #[doc(hidden)]
        pub fn __leo3_metadata() -> #leo3_crate::LeanFunctionMetadata {
            #metadata
        }
    }
}

/// Build the implementation for a `#[leanfn]` annotated function.
///
/// This generates:
/// - A wrapper function that handles Lean FFI calling conventions
/// - Type conversions between Rust and Lean types
/// - Error handling
pub fn build_lean_function(
    func: &mut syn::ItemFn,
    options: LeanFunctionOptions,
) -> syn::Result<TokenStream> {
    let leo3_crate = get_leo3_crate(options.common.krate.as_ref());

    if options.concretes.is_empty() {
        build_lean_function_simple(func, &options, &leo3_crate)
    } else {
        build_lean_function_concrete(func, &options, &leo3_crate)
    }
}

fn build_lean_function_simple(
    func: &mut syn::ItemFn,
    options: &LeanFunctionOptions,
    leo3_crate: &TokenStream,
) -> syn::Result<TokenStream> {
    let binding = analyze_lean_function(
        func,
        FunctionOptions {
            lean_name: options.common.name.as_ref().map(|value| value.value()),
        },
    )?;
    let lean_name = options
        .common
        .name
        .as_ref()
        .map(|s| s.value())
        .unwrap_or_else(|| func.sig.ident.to_string());
    let info = analyze_function(func, &lean_name, None)?;

    let rust_name = &info.rust_name;
    let lean_name_ident = format_ident!("{}", &info.lean_name);
    let wrapper_module = format_ident!("__leo3_leanfn_{}", rust_name);
    let metadata_name = format_ident!("__leo3_metadata_{}", rust_name);

    let ffi_wrapper = generate_ffi_wrapper(&info, leo3_crate);
    let conversion_wrapper = generate_conversion_wrapper(&info, leo3_crate, None);
    let metadata = generate_metadata(&binding, leo3_crate);

    let internal_ffi_name = format_ident!("__ffi_{}", &info.lean_name);
    let ffi_reexport = if *rust_name != info.lean_name {
        quote! {
            #[allow(non_snake_case, unused_imports)]
            pub use #wrapper_module::#internal_ffi_name as #lean_name_ident;
        }
    } else {
        quote! {}
    };

    func.attrs.clear();

    Ok(quote! {
        #func

        #[allow(non_snake_case, unused_imports)]
        mod #wrapper_module {
            use super::*;

            #ffi_wrapper

            #conversion_wrapper

            #metadata
        }

        #ffi_reexport

        #[doc(hidden)]
        #[allow(non_snake_case)]
        pub use #wrapper_module::__leo3_metadata as #metadata_name;
    })
}

fn build_lean_function_concrete(
    func: &mut syn::ItemFn,
    options: &LeanFunctionOptions,
    leo3_crate: &TokenStream,
) -> syn::Result<TokenStream> {
    let param_names = generic_type_param_names(func)?;

    let mut seen_names = std::collections::HashSet::new();
    for concrete in &options.concretes {
        if !seen_names.insert(&concrete.name) {
            return Err(syn::Error::new_spanned(
                func,
                format!("duplicate concrete name `{}`", concrete.name),
            ));
        }
    }

    let rust_name = func.sig.ident.clone();
    let mut modules = Vec::new();
    let mut reexports = Vec::new();

    for concrete in &options.concretes {
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
            .iter()
            .cloned()
            .zip(concrete.types.iter().cloned())
            .collect();

        let concrete_attr = ConcreteAttr {
            types: concrete.types.clone(),
            name: concrete.name.clone(),
        };
        let binding = analyze_concrete_instance(func, &concrete_attr)?;
        let info = analyze_function(func, &concrete.name, Some(&mapping))?;

        let wrapper_module = format_ident!("__leo3_leanfn_{}_{}", rust_name, concrete.name);
        let metadata_name = format_ident!("__leo3_metadata_{}", concrete.name);

        let turbofish_types = &concrete.types;
        let turbofish = quote! { <#(#turbofish_types),*> };

        let ffi_wrapper = generate_ffi_wrapper(&info, leo3_crate);
        let conversion_wrapper = generate_conversion_wrapper(&info, leo3_crate, Some(&turbofish));
        let metadata = generate_metadata(&binding, leo3_crate);

        let internal_ffi_name = format_ident!("__ffi_{}", &concrete.name);
        let lean_name_ident = format_ident!("{}", &concrete.name);

        modules.push(quote! {
            #[allow(non_snake_case, unused_imports)]
            mod #wrapper_module {
                use super::*;

                #ffi_wrapper

                #conversion_wrapper

                #metadata
            }
        });

        reexports.push(quote! {
            #[allow(non_snake_case, unused_imports)]
            pub use #wrapper_module::#internal_ffi_name as #lean_name_ident;

            #[doc(hidden)]
            #[allow(non_snake_case)]
            pub use #wrapper_module::__leo3_metadata as #metadata_name;
        });
    }

    func.attrs.clear();

    Ok(quote! {
        #func

        #(#modules)*

        #(#reexports)*
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommonOptions;

    #[test]
    fn generated_wrapper_avoids_expect_for_boundary_failures() {
        let mut func: syn::ItemFn = syn::parse_quote! {
            fn demo(x: u64) -> u64 {
                x
            }
        };

        let tokens = build_lean_function(
            &mut func,
            LeanFunctionOptions {
                common: CommonOptions::default(),
                concretes: Vec::new(),
            },
        )
        .expect("leanfn expansion should succeed");

        let rendered = tokens.to_string();
        assert!(rendered.contains("__private :: lean_panic_message"));
        assert!(rendered.contains("__private :: panic_payload_message"));
        assert!(rendered.contains("Failed to convert"));
        assert!(rendered.contains("Failed to convert Rust result to Lean"));
        assert!(!rendered.contains(".expect("));
        assert!(!rendered.contains(". expect ("));
    }

    #[test]
    fn generated_wrapper_supports_borrowed_string_and_vec_aliases() {
        let mut func: syn::ItemFn = syn::parse_quote! {
            fn demo(name: &String, bytes: &Vec<u8>, values: &Vec<u64>) -> (&String, &Vec<u8>, &Vec<u64>) {
                (name, bytes, values)
            }
        };

        let tokens = build_lean_function(
            &mut func,
            LeanFunctionOptions {
                common: CommonOptions::default(),
                concretes: Vec::new(),
            },
        )
        .expect("leanfn expansion should succeed");

        let rendered = tokens.to_string();
        assert!(rendered.contains("__name_string_storage"));
        assert!(rendered.contains("__bytes_vec_storage"));
        assert!(rendered.contains("__values_vec_storage"));
        assert!(rendered.contains("vec_u8_from_lean"));
        assert!(rendered.contains("slice_u8_into_lean"));
        assert!(rendered.contains("slice_into_lean"));
        assert!(rendered.contains("LeanString :: mk"));
    }

    #[test]
    fn generated_wrapper_supports_option_of_borrowed_owned_container_aliases() {
        let mut func: syn::ItemFn = syn::parse_quote! {
            fn demo(
                name: Option<&String>,
                bytes: Option<&Vec<u8>>,
                values: Option<&Vec<u64>>
            ) -> u64 {
                name.map(|s| s.len() as u64).unwrap_or(0)
                    + bytes.map(|v| v.len() as u64).unwrap_or(0)
                    + values.map(|v| v.iter().sum::<u64>()).unwrap_or(0)
            }
        };

        let tokens = build_lean_function(
            &mut func,
            LeanFunctionOptions {
                common: CommonOptions::default(),
                concretes: Vec::new(),
            },
        )
        .expect("leanfn expansion should succeed");

        let rendered = tokens.to_string();
        assert!(rendered.contains("__name_option_storage"));
        assert!(rendered.contains("__bytes_option_storage"));
        assert!(rendered.contains("__values_option_storage"));
        assert!(rendered.contains("LeanOption :: get"));
        assert!(rendered.contains("vec_u8_from_lean"));
        assert!(rendered.contains("as_ref"));
    }

    #[test]
    fn generated_wrapper_supports_result_of_borrowed_owned_container_aliases() {
        let mut func: syn::ItemFn = syn::parse_quote! {
            fn demo(
                value: Result<&String, &String>,
                bytes: Result<&Vec<u8>, &String>,
                words: Result<&Vec<u64>, &String>
            ) -> u64 {
                (match value {
                    Ok(name) => name.len() as u64,
                    Err(err) => err.len() as u64,
                }) + (match bytes {
                    Ok(data) => data.len() as u64,
                    Err(err) => err.len() as u64,
                }) + (match words {
                    Ok(items) => items.iter().sum::<u64>(),
                    Err(err) => err.len() as u64,
                })
            }
        };

        let tokens = build_lean_function(
            &mut func,
            LeanFunctionOptions {
                common: CommonOptions::default(),
                concretes: Vec::new(),
            },
        )
        .expect("leanfn expansion should succeed");

        let rendered = tokens.to_string();
        assert!(rendered.contains("__value_result_ok_storage"));
        assert!(rendered.contains("__value_result_err_storage"));
        assert!(rendered.contains("__bytes_result_ok_storage"));
        assert!(rendered.contains("__bytes_result_err_storage"));
        assert!(rendered.contains("__words_result_ok_storage"));
        assert!(rendered.contains("__words_result_err_storage"));
        assert!(rendered.contains("LeanExcept :: toRustResult"));
        assert!(rendered.contains("typed_ok"));
        assert!(rendered.contains("typed_err"));
    }

    #[test]
    fn generated_wrapper_supports_tuple_of_borrowed_owned_container_aliases() {
        let mut func: syn::ItemFn = syn::parse_quote! {
            fn demo(value: (&String, &Vec<u8>, &Vec<u64>)) -> u64 {
                value.0.len() as u64 + value.1.len() as u64 + value.2.iter().sum::<u64>()
            }
        };

        let tokens = build_lean_function(
            &mut func,
            LeanFunctionOptions {
                common: CommonOptions::default(),
                concretes: Vec::new(),
            },
        )
        .expect("leanfn expansion should succeed");

        let rendered = tokens.to_string();
        assert!(rendered.contains("__value_tuple_storage"));
        assert!(rendered.contains("LeanProd :: fst"));
        assert!(rendered.contains("LeanProd :: snd"));
        assert!(rendered.contains("& __value_tuple_storage"));
    }

    #[test]
    fn generated_wrapper_supports_nested_tuple_of_borrowed_owned_container_aliases() {
        let mut func: syn::ItemFn = syn::parse_quote! {
            fn demo(value: ((&String, &Vec<u8>), &Vec<u64>)) -> u64 {
                value.0.0.len() as u64 + value.0.1.len() as u64 + value.1.iter().sum::<u64>()
            }
        };

        let tokens = build_lean_function(
            &mut func,
            LeanFunctionOptions {
                common: CommonOptions::default(),
                concretes: Vec::new(),
            },
        )
        .expect("leanfn expansion should succeed");

        let rendered = tokens.to_string();
        assert!(rendered.contains("__value_tuple_storage"));
        assert!(rendered.contains("LeanProd :: fst"));
        assert!(rendered.contains("LeanProd :: snd"));
        assert!(rendered.contains("& __value_tuple_storage . 0 . 0"));
    }

    #[test]
    fn generated_wrapper_supports_mixed_result_borrowed_aliases() {
        let mut func: syn::ItemFn = syn::parse_quote! {
            fn demo(left: Result<&String, String>, right: Result<Vec<u64>, &String>) -> u64 {
                (match left {
                    Ok(name) => name.len() as u64,
                    Err(err) => err.len() as u64,
                }) + (match right {
                    Ok(values) => values.iter().sum::<u64>(),
                    Err(err) => err.len() as u64,
                })
            }
        };

        let tokens = build_lean_function(
            &mut func,
            LeanFunctionOptions {
                common: CommonOptions::default(),
                concretes: Vec::new(),
            },
        )
        .expect("leanfn expansion should succeed");

        let rendered = tokens.to_string();
        assert!(rendered.contains("__left_result_ok_storage"));
        assert!(rendered.contains("__left_result_err_storage"));
        assert!(rendered.contains("__right_result_ok_storage"));
        assert!(rendered.contains("__right_result_err_storage"));
        assert!(rendered.contains("take"));
        assert!(rendered.contains("as_ref"));
    }

    #[test]
    fn generated_wrapper_supports_option_of_result_borrowed_aliases() {
        let mut func: syn::ItemFn = syn::parse_quote! {
            fn demo(
                names: Option<Result<&String, &String>>,
                bytes: Option<Result<&Vec<u8>, &String>>,
                words: Option<Result<Vec<u64>, &String>>
            ) -> u64 {
                (match names {
                    Some(Ok(name)) => name.len() as u64,
                    Some(Err(err)) => err.len() as u64,
                    None => 0,
                }) + (match bytes {
                    Some(Ok(data)) => data.len() as u64,
                    Some(Err(err)) => err.len() as u64,
                    None => 0,
                }) + (match words {
                    Some(Ok(items)) => items.iter().sum::<u64>(),
                    Some(Err(err)) => err.len() as u64,
                    None => 0,
                })
            }
        };

        let tokens = build_lean_function(
            &mut func,
            LeanFunctionOptions {
                common: CommonOptions::default(),
                concretes: Vec::new(),
            },
        )
        .expect("leanfn expansion should succeed");

        let rendered = tokens.to_string();
        assert!(rendered.contains("__names_option_result_ok_storage"));
        assert!(rendered.contains("__names_option_result_err_storage"));
        assert!(rendered.contains("__bytes_option_result_ok_storage"));
        assert!(rendered.contains("__bytes_option_result_err_storage"));
        assert!(rendered.contains("__words_option_result_ok_storage"));
        assert!(rendered.contains("__words_option_result_err_storage"));
        assert!(rendered.contains("LeanOption :: get"));
        assert!(rendered.contains("LeanExcept :: toRustResult"));
    }

    #[test]
    fn concrete_generates_separate_wrappers_per_instance() {
        let mut func: syn::ItemFn = syn::parse_quote! {
            fn add<T: std::ops::Add<Output = T>>(a: T, b: T) -> T {
                a + b
            }
        };

        let tokens = build_lean_function(
            &mut func,
            LeanFunctionOptions {
                common: CommonOptions::default(),
                concretes: vec![
                    ConcreteInstance {
                        types: vec![syn::parse_quote! { u64 }],
                        name: "add_u64".to_string(),
                    },
                    ConcreteInstance {
                        types: vec![syn::parse_quote! { i64 }],
                        name: "add_i64".to_string(),
                    },
                ],
            },
        )
        .expect("concrete leanfn expansion should succeed");

        let rendered = tokens.to_string();
        assert!(rendered.contains("__ffi_add_u64"));
        assert!(rendered.contains("__ffi_add_i64"));
        assert!(rendered.contains("add_u64"));
        assert!(rendered.contains("add_i64"));
        assert!(rendered.contains("__leo3_leanfn_add_add_u64"));
        assert!(rendered.contains("__leo3_leanfn_add_add_i64"));
        assert!(rendered.contains("__leo3_metadata_add_u64"));
        assert!(rendered.contains("__leo3_metadata_add_i64"));
        assert!(rendered.contains("< u64 >"));
        assert!(rendered.contains("< i64 >"));
    }

    #[test]
    fn concrete_supports_two_generic_params() {
        let mut func: syn::ItemFn = syn::parse_quote! {
            fn convert<A, B>(input: A) -> B {
                todo!()
            }
        };

        let tokens = build_lean_function(
            &mut func,
            LeanFunctionOptions {
                common: CommonOptions::default(),
                concretes: vec![ConcreteInstance {
                    types: vec![syn::parse_quote! { u64 }, syn::parse_quote! { String }],
                    name: "u64_to_string".to_string(),
                }],
            },
        )
        .expect("two-param concrete expansion should succeed");

        let rendered = tokens.to_string();
        assert!(rendered.contains("__ffi_u64_to_string"));
        assert!(rendered.contains("< u64 , String >"));
    }

    #[test]
    fn concrete_rejects_wrong_arity() {
        let mut func: syn::ItemFn = syn::parse_quote! {
            fn add<T: std::ops::Add<Output = T>>(a: T, b: T) -> T {
                a + b
            }
        };

        let result = build_lean_function(
            &mut func,
            LeanFunctionOptions {
                common: CommonOptions::default(),
                concretes: vec![ConcreteInstance {
                    types: vec![syn::parse_quote! { u64 }, syn::parse_quote! { i64 }],
                    name: "add_bad".to_string(),
                }],
            },
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("expected 1 concrete type(s)"));
    }

    #[test]
    fn concrete_rejects_duplicate_names() {
        let mut func: syn::ItemFn = syn::parse_quote! {
            fn add<T: std::ops::Add<Output = T>>(a: T, b: T) -> T {
                a + b
            }
        };

        let result = build_lean_function(
            &mut func,
            LeanFunctionOptions {
                common: CommonOptions::default(),
                concretes: vec![
                    ConcreteInstance {
                        types: vec![syn::parse_quote! { u64 }],
                        name: "add_dup".to_string(),
                    },
                    ConcreteInstance {
                        types: vec![syn::parse_quote! { i64 }],
                        name: "add_dup".to_string(),
                    },
                ],
            },
        );

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("duplicate concrete name"));
    }

    #[test]
    fn concrete_substitutes_types_in_params_and_return() {
        let mut func: syn::ItemFn = syn::parse_quote! {
            fn identity<T>(x: T) -> T {
                x
            }
        };

        let tokens = build_lean_function(
            &mut func,
            LeanFunctionOptions {
                common: CommonOptions::default(),
                concretes: vec![ConcreteInstance {
                    types: vec![syn::parse_quote! { u64 }],
                    name: "identity_u64".to_string(),
                }],
            },
        )
        .expect("identity concrete expansion should succeed");

        let rendered = tokens.to_string();
        assert!(rendered.contains("FromLean"));
        assert!(rendered.contains("identity :: < u64 >"));
    }
}
