//! Implementation of the `#[leanfn]` macro.

use leo3_binding_ir::{
    analyze_lean_function, quote_runtime_function_metadata, FunctionBinding, FunctionOptions,
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

/// Options for the `#[leanfn]` attribute
pub struct LeanFunctionOptions {
    pub common: CommonOptions,
}

impl Parse for LeanFunctionOptions {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            common: input.parse()?,
        })
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

/// Analyze a function signature and extract relevant information
fn analyze_function(
    func: &syn::ItemFn,
    options: &LeanFunctionOptions,
) -> syn::Result<FunctionInfo> {
    let rust_name = func.sig.ident.clone();
    let lean_name = options
        .common
        .name
        .as_ref()
        .map(|s| s.value())
        .unwrap_or_else(|| rust_name.to_string());

    // Extract parameters
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
                params.push((name, (*pat_type.ty).clone()));
            }
            syn::FnArg::Receiver(_) => {
                return Err(syn::Error::new_spanned(
                    input,
                    "Methods with `self` are not supported. Use standalone functions.",
                ))
            }
        }
    }

    // Extract return type
    let return_type = match &func.sig.output {
        syn::ReturnType::Default => syn::parse_quote! { () },
        syn::ReturnType::Type(_, ty) => (**ty).clone(),
    };

    // Check for generics
    if !func.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &func.sig.generics,
            "Generic functions are not supported yet",
        ));
    }

    Ok(FunctionInfo {
        rust_name,
        lean_name,
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
fn generate_conversion_wrapper(info: &FunctionInfo, leo3_crate: &TokenStream) -> TokenStream {
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

    // Check if return type is unit
    let call_and_return = if matches!(info.return_type, syn::Type::Tuple(ref t) if t.elems.is_empty())
    {
        quote! {
            #rust_name(#(#call_args),*);
            #result_conversion
        }
    } else {
        quote! {
            let result = #rust_name(#(#call_args),*);
            #result_conversion
        }
    };

    quote! {
        pub(crate) unsafe fn __ffi_try_wrapper(
            #(#ffi_params),*
        ) -> #leo3_crate::err::LeanResult<#leo3_crate::ffi::object::lean_obj_res> {
            // Get Lean token (proves runtime is initialized)
            let lean = #leo3_crate::Lean::assume_initialized();

            // Convert arguments from Lean to Rust
            #(#param_conversions)*

            // Call the original function and convert result
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
    let binding = analyze_lean_function(
        func,
        FunctionOptions {
            lean_name: options.common.name.as_ref().map(|value| value.value()),
        },
    )?;
    let info = analyze_function(func, &options)?;
    let leo3_crate = get_leo3_crate(options.common.krate.as_ref());

    let rust_name = &info.rust_name;
    let lean_name_ident = format_ident!("{}", &info.lean_name);
    let wrapper_module = format_ident!("__leo3_leanfn_{}", rust_name);
    let metadata_name = format_ident!("__leo3_metadata_{}", rust_name);

    // Generate wrapper components
    let ffi_wrapper = generate_ffi_wrapper(&info, &leo3_crate);
    let conversion_wrapper = generate_conversion_wrapper(&info, &leo3_crate);
    let metadata = generate_metadata(&binding, &leo3_crate);

    // Only re-export FFI function if the lean name is different from rust name
    let internal_ffi_name = format_ident!("__ffi_{}", &info.lean_name);
    let ffi_reexport = if *rust_name != info.lean_name {
        quote! {
            // Re-export FFI function with its Lean name (for FFI calls)
            #[allow(non_snake_case, unused_imports)]
            pub use #wrapper_module::#internal_ffi_name as #lean_name_ident;
        }
    } else {
        quote! {}
    };

    // Remove ALL attributes from the function to ensure clean output
    func.attrs.clear();

    // Keep the original function as-is (without any attributes)
    Ok(quote! {
        // Original function stays at top level - unchanged
        #func

        // Hidden module to hold FFI wrappers and metadata (PyO3 pattern)
        #[allow(non_snake_case, unused_imports)]
        mod #wrapper_module {
            use super::*;

            // FFI entry point with panic boundary
            #ffi_wrapper

            // Conversion logic (testable separately)
            #conversion_wrapper

            // Metadata
            #metadata
        }

        // Re-export FFI function if name is different
        #ffi_reexport

        // Re-export metadata with unique name
        #[doc(hidden)]
        #[allow(non_snake_case)]
        pub use #wrapper_module::__leo3_metadata as #metadata_name;
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
}
