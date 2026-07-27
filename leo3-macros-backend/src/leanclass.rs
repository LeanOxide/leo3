//! Implementation of the `#[leanclass]` macro.
//!
//! This macro generates both Rust and Lean code for exposing Rust structs
//! as Lean external classes. In addition to the Rust FFI wrappers and
//! `ExternalClass` trait implementation, the macro produces string constants
//! containing the corresponding Lean source code (`opaque` type declarations
//! and `@[extern]`-annotated function signatures) that can be embedded into
//! `.lean` files.

use leo3_binding_ir::{
    analyze_lean_class_impl, analyze_lean_class_struct, class_binding_to_json,
    quote_runtime_class_metadata, quote_runtime_function_metadata, ClassImplBinding,
    ClassTypeBinding, FunctionBinding,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::Parse;

use crate::{get_leo3_crate, CommonOptions};

/// Options for the `#[leanclass]` attribute
pub struct LeanClassOptions {
    pub common: CommonOptions,
}

impl Parse for LeanClassOptions {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        Ok(Self {
            common: input.parse()?,
        })
    }
}

/// Information about a struct
struct StructInfo {
    name: syn::Ident,
    generics: syn::Generics,
    _vis: syn::Visibility,
}

/// Information about a method
struct MethodInfo {
    name: syn::Ident,
    receiver: MethodReceiver,
    params: Vec<(syn::Ident, syn::Type)>,
    return_type: syn::Type,
}

/// Type of method receiver
#[derive(Debug, Clone)]
enum MethodReceiver {
    None,   // Static method (no self)
    Ref,    // &self
    MutRef, // &mut self
    Owned,  // self
}

/// Build the #\[leanclass\] expansion for a struct
pub fn build_lean_class_struct(
    item: &mut syn::ItemStruct,
    options: LeanClassOptions,
) -> syn::Result<TokenStream> {
    let leo3_crate = get_leo3_crate(options.common.krate.as_ref());
    let class_binding = analyze_lean_class_struct(item)?;

    let struct_info = StructInfo {
        name: item.ident.clone(),
        generics: item.generics.clone(),
        _vis: item.vis.clone(),
    };

    // Check for generics
    if !struct_info.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &struct_info.generics,
            "#[leanclass] does not support generic types yet",
        ));
    }

    // Generate ExternalClass implementation
    let external_class_impl = generate_external_class_impl(&struct_info, &leo3_crate)?;

    // Generate Lean code metadata
    let lean_code_gen = generate_lean_code_metadata(&class_binding);

    // Keep original struct
    let original_struct = quote! { #item };

    Ok(quote! {
        #original_struct
        #external_class_impl
        #lean_code_gen
    })
}

/// Build the #\[leanclass\] expansion for an impl block
pub fn build_lean_class_impl(
    item: &mut syn::ItemImpl,
    options: LeanClassOptions,
) -> syn::Result<TokenStream> {
    let leo3_crate = get_leo3_crate(options.common.krate.as_ref());
    let impl_binding = analyze_lean_class_impl(item)?;

    // Extract the struct name from the impl
    let struct_name = match &*item.self_ty {
        syn::Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                segment.ident.clone()
            } else {
                return Err(syn::Error::new_spanned(
                    &item.self_ty,
                    "Could not extract struct name from impl",
                ));
            }
        }
        _ => {
            return Err(syn::Error::new_spanned(
                &item.self_ty,
                "#[leanclass] impl must be for a simple struct type",
            ));
        }
    };

    // Check for generics
    if !item.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &item.generics,
            "#[leanclass] does not support generic impl blocks yet",
        ));
    }

    // Extract methods
    let mut methods = Vec::new();
    for impl_item in &item.items {
        if let syn::ImplItem::Fn(method) = impl_item {
            let method_info = analyze_method(method, &struct_name)?;
            methods.push(method_info);
        }
    }

    // Generate FFI wrapper functions for each method
    let mut ffi_functions = Vec::new();
    for (method, binding) in methods.iter().zip(impl_binding.methods.iter()) {
        let ffi_fn = generate_method_ffi_wrapper(method, binding, &struct_name, &leo3_crate)?;
        ffi_functions.push(ffi_fn);
    }

    // Generate Lean code metadata
    let class_binding = ClassTypeBinding {
        rust_name: struct_name.to_string(),
        lean_name: struct_name.to_string(),
        opaque_decl: format!("opaque {} : Type", struct_name),
    };
    let lean_code_gen =
        generate_lean_code_metadata_for_methods(&class_binding, &impl_binding, &leo3_crate)?;

    // Strip the helper `#[getter]` / `#[setter]` attributes so the compiler
    // does not reject them as unknown attributes in the emitted impl block.
    strip_accessor_attrs(item);

    // Keep original impl
    let original_impl = quote! { #item };

    Ok(quote! {
        #original_impl
        #(#ffi_functions)*
        #lean_code_gen
    })
}

/// Analyze a method signature and extract relevant information
fn analyze_method(method: &syn::ImplItemFn, _struct_name: &syn::Ident) -> syn::Result<MethodInfo> {
    let method_name = method.sig.ident.clone();

    // Determine receiver type
    let mut receiver = MethodReceiver::None;
    let mut param_start_index = 0;

    if let Some(first_arg) = method.sig.inputs.first() {
        match first_arg {
            syn::FnArg::Receiver(recv) => {
                param_start_index = 1;
                receiver = if recv.mutability.is_some() {
                    MethodReceiver::MutRef
                } else if recv.reference.is_some() {
                    MethodReceiver::Ref
                } else {
                    MethodReceiver::Owned
                };
            }
            syn::FnArg::Typed(_) => {
                // No self, static method
                receiver = MethodReceiver::None;
            }
        }
    }

    // Extract parameters (excluding self)
    let mut params = Vec::new();
    for input in method.sig.inputs.iter().skip(param_start_index) {
        if let syn::FnArg::Typed(pat_type) = input {
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
    }

    // Extract return type
    let return_type = match &method.sig.output {
        syn::ReturnType::Default => syn::parse_quote! { () },
        syn::ReturnType::Type(_, ty) => (**ty).clone(),
    };

    // Check for generics
    if !method.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &method.sig.generics,
            "Generic methods are not supported yet",
        ));
    }

    Ok(MethodInfo {
        name: method_name,
        receiver,
        params,
        return_type,
    })
}

/// Generate the ExternalClass trait implementation
fn generate_external_class_impl(
    struct_info: &StructInfo,
    leo3_crate: &TokenStream,
) -> syn::Result<TokenStream> {
    let struct_name = &struct_info.name;
    let class_name_str = struct_name.to_string();

    Ok(quote! {
        impl #leo3_crate::external::ExternalClass for #struct_name {
            fn class_name() -> &'static str {
                #class_name_str
            }
        }
    })
}

/// Generate FFI wrapper for a method
fn generate_method_ffi_wrapper(
    method: &MethodInfo,
    binding: &FunctionBinding,
    struct_name: &syn::Ident,
    leo3_crate: &TokenStream,
) -> syn::Result<TokenStream> {
    let method_name = &method.name;
    let ffi_name = format_ident!("__lean_ffi_{}_{}", struct_name, method_name);
    let try_name = format_ident!("__leo3_try_{}_{}", struct_name, method_name);

    match &method.receiver {
        MethodReceiver::None => {
            // Static method
            generate_static_method_wrapper(
                method,
                binding,
                struct_name,
                &ffi_name,
                &try_name,
                leo3_crate,
            )
        }
        MethodReceiver::Ref => {
            // &self method
            generate_ref_method_wrapper(
                method,
                binding,
                struct_name,
                &ffi_name,
                &try_name,
                leo3_crate,
            )
        }
        MethodReceiver::MutRef => {
            // &mut self method - needs special handling for Lean's pure functional semantics
            generate_mut_ref_method_wrapper(
                method,
                binding,
                struct_name,
                &ffi_name,
                &try_name,
                leo3_crate,
            )
        }
        MethodReceiver::Owned => {
            // self method (consuming)
            generate_owned_method_wrapper(
                method,
                binding,
                struct_name,
                &ffi_name,
                &try_name,
                leo3_crate,
            )
        }
    }
}

fn generate_param_conversions_with_offset(
    params: &[(syn::Ident, syn::Type)],
    offset: usize,
    leo3_crate: &TokenStream,
) -> Vec<TokenStream> {
    params
        .iter()
        .enumerate()
        .map(|(i, (name, ty))| {
            let arg_name = format_ident!("arg{}", i + offset);
            quote! {
                let #name = {
                    let bound = #leo3_crate::LeanBound::<_>::from_owned_ptr(lean, #arg_name);
                    <#ty as #leo3_crate::conversion::FromLean>::from_lean(&bound)
                        .map_err(|e| #leo3_crate::LeanError::Conversion(format!(
                            "Failed to convert `{}` from Lean to Rust: {}",
                            stringify!(#name),
                            e
                        )))?
                };
            }
        })
        .collect()
}

fn generate_object_ffi_wrapper(
    ffi_name: &syn::Ident,
    try_name: &syn::Ident,
    ffi_params: &[TokenStream],
    wrapper_call_args: &[TokenStream],
    leo3_crate: &TokenStream,
) -> TokenStream {
    quote! {
        #[no_mangle]
        pub unsafe extern "C" fn #ffi_name(#(#ffi_params),*) -> #leo3_crate::ffi::object::lean_obj_res {
            #leo3_crate::__private::ffi_panic_boundary(|| #try_name(#(#wrapper_call_args),*))
        }
    }
}

/// Generate wrapper for static methods (no self)
fn generate_static_method_wrapper(
    method: &MethodInfo,
    binding: &FunctionBinding,
    struct_name: &syn::Ident,
    ffi_name: &syn::Ident,
    try_name: &syn::Ident,
    leo3_crate: &TokenStream,
) -> syn::Result<TokenStream> {
    let method_name = &method.name;
    let params = &method.params;
    let return_type = &method.return_type;

    // Generate FFI parameters
    let param_count = params.len();
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

    // Generate parameter conversions
    let param_conversions = generate_param_conversions_with_offset(params, 0, leo3_crate);

    // Generate method call
    let param_names: Vec<_> = params.iter().map(|(name, _)| name).collect();

    // Generate result conversion
    let result_conversion = if is_self_return(return_type, struct_name) {
        // Returns Self - wrap in LeanExternal
        quote! {
            {
                let external = #leo3_crate::external::LeanExternal::new(lean, result)
                    .map_err(|e| #leo3_crate::LeanError::Other(format!(
                        "Failed to create external object: {}",
                        e
                    )))?;
                Ok(external.into_ptr())
            }
        }
    } else if is_unit_type(return_type) {
        quote! {
            Ok(#leo3_crate::ffi::lean_mk_unit())
        }
    } else {
        quote! {
            {
                let lean_result = <#return_type as #leo3_crate::conversion::IntoLean>::into_lean(result, lean)
                    .map_err(|e| #leo3_crate::LeanError::Conversion(format!(
                        "Failed to convert Rust result to Lean: {}",
                        e
                    )))?;
                Ok(lean_result.into_ptr())
            }
        }
    };
    let ffi_wrapper = generate_object_ffi_wrapper(
        ffi_name,
        try_name,
        &ffi_params,
        &wrapper_call_args,
        leo3_crate,
    );
    let metadata = quote_runtime_function_metadata(binding, leo3_crate);

    Ok(quote! {
        #ffi_wrapper

        #[doc(hidden)]
        #[allow(non_snake_case)]
        pub(crate) unsafe fn #try_name(#(#ffi_params),*) -> #leo3_crate::LeanResult<#leo3_crate::ffi::object::lean_obj_res> {
            let lean = #leo3_crate::Lean::assume_initialized();

            #(#param_conversions)*

            let result = #struct_name::#method_name(#(#param_names),*);

            #result_conversion
        }

        #[allow(non_upper_case_globals)]
        const _: () = {
            #[link_section = "__DATA,__lean_metadata"]
            #[used]
            static __LEAN_METADATA: #leo3_crate::LeanFunctionMetadata =
                #metadata;
        };
    })
}

/// Generate wrapper for &self methods
fn generate_ref_method_wrapper(
    method: &MethodInfo,
    binding: &FunctionBinding,
    struct_name: &syn::Ident,
    ffi_name: &syn::Ident,
    try_name: &syn::Ident,
    leo3_crate: &TokenStream,
) -> syn::Result<TokenStream> {
    let method_name = &method.name;
    let params = &method.params;
    let return_type = &method.return_type;

    // First parameter is the external object
    let param_count = params.len() + 1;
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

    // Convert self parameter
    let self_conversion = quote! {
        let self_obj = #leo3_crate::LeanBound::<#leo3_crate::external::LeanExternalType<#struct_name>>::from_owned_ptr(lean, arg0);
        let self_ref = self_obj.get_ref();
    };

    // Convert other parameters
    let param_conversions = generate_param_conversions_with_offset(params, 1, leo3_crate);

    // Generate method call
    let param_names: Vec<_> = params.iter().map(|(name, _)| name).collect();

    // Generate result conversion
    let result_conversion = if is_unit_type(return_type) {
        quote! {
            Ok(unsafe { #leo3_crate::ffi::lean_mk_unit() })
        }
    } else {
        quote! {
            {
                let lean_result = <#return_type as #leo3_crate::conversion::IntoLean>::into_lean(result, lean)
                    .map_err(|e| #leo3_crate::LeanError::Conversion(format!(
                        "Failed to convert Rust result to Lean: {}",
                        e
                    )))?;
                Ok(lean_result.into_ptr())
            }
        }
    };
    let ffi_wrapper = generate_object_ffi_wrapper(
        ffi_name,
        try_name,
        &ffi_params,
        &wrapper_call_args,
        leo3_crate,
    );
    let metadata = quote_runtime_function_metadata(binding, leo3_crate);

    Ok(quote! {
        #ffi_wrapper

        #[doc(hidden)]
        #[allow(non_snake_case)]
        pub(crate) unsafe fn #try_name(#(#ffi_params),*) -> #leo3_crate::LeanResult<#leo3_crate::ffi::object::lean_obj_res> {
            let lean = #leo3_crate::Lean::assume_initialized();

            #self_conversion
            #(#param_conversions)*

            let result = self_ref.#method_name(#(#param_names),*);

            #result_conversion
        }

        #[allow(non_upper_case_globals)]
        const _: () = {
            #[link_section = "__DATA,__lean_metadata"]
            #[used]
            static __LEAN_METADATA: #leo3_crate::LeanFunctionMetadata =
                #metadata;
        };
    })
}

/// Generate wrapper for &mut self methods
/// In Lean (pure functional), this must return a new instance
fn generate_mut_ref_method_wrapper(
    method: &MethodInfo,
    binding: &FunctionBinding,
    struct_name: &syn::Ident,
    ffi_name: &syn::Ident,
    try_name: &syn::Ident,
    leo3_crate: &TokenStream,
) -> syn::Result<TokenStream> {
    let method_name = &method.name;
    let params = &method.params;
    let return_type = &method.return_type;

    // First parameter is the external object
    let param_count = params.len() + 1;
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

    // Convert self parameter with COW (copy-on-write) semantics.
    // If the object is shared (refcount > 1), clone the inner value into a
    // fresh external object so that mutation never affects other references.
    let self_conversion = quote! {
        let mut self_obj = if #leo3_crate::ffi::object::lean_is_exclusive(arg0) {
            // Exclusively owned — safe to mutate in place.
            #leo3_crate::LeanBound::<#leo3_crate::external::LeanExternalType<#struct_name>>::from_owned_ptr(lean, arg0)
        } else {
            // Shared — clone the inner value, release our reference to the
            // original, and wrap the clone in a new external object.
            let borrowed = #leo3_crate::LeanBound::<#leo3_crate::external::LeanExternalType<#struct_name>>::from_owned_ptr(lean, arg0);
            let cloned: #struct_name = borrowed.get_ref().clone();
            drop(borrowed);
            #leo3_crate::external::LeanExternal::new(lean, cloned)
                .map_err(|e| #leo3_crate::LeanError::Other(format!(
                    "Failed to allocate COW copy of external object: {}",
                    e
                )))?
        };
        let self_mut = self_obj.get_mut();
    };

    // Convert other parameters
    let param_conversions = generate_param_conversions_with_offset(params, 1, leo3_crate);

    // Generate method call
    let param_names: Vec<_> = params.iter().map(|(name, _)| name).collect();

    // For &mut self methods, we return the modified object. When the Rust
    // method also returns a value, we preserve both pieces of information as a
    // Lean `Prod self result` so mutation never gets discarded.
    let result_conversion = if is_unit_type(return_type) {
        // Method returns (), so return the modified self
        quote! {
            Ok(self_obj.into_ptr())
        }
    } else {
        quote! {
            {
                let any_self: #leo3_crate::LeanBound<'_, #leo3_crate::instance::LeanAny> = self_obj.cast();
                let lean_result = <#return_type as #leo3_crate::conversion::IntoLean>::into_lean(result, lean)
                    .map_err(|e| #leo3_crate::LeanError::Conversion(format!(
                        "Failed to convert Rust result to Lean: {}",
                        e
                    )))?;
                let any_result: #leo3_crate::LeanBound<'_, #leo3_crate::instance::LeanAny> =
                    lean_result.cast();
                let pair = #leo3_crate::types::LeanProd::mk(any_self, any_result)?;
                Ok(pair.into_ptr())
            }
        }
    };
    let ffi_wrapper = generate_object_ffi_wrapper(
        ffi_name,
        try_name,
        &ffi_params,
        &wrapper_call_args,
        leo3_crate,
    );
    let metadata = quote_runtime_function_metadata(binding, leo3_crate);

    Ok(quote! {
        #ffi_wrapper

        #[doc(hidden)]
        #[allow(non_snake_case)]
        pub(crate) unsafe fn #try_name(#(#ffi_params),*) -> #leo3_crate::LeanResult<#leo3_crate::ffi::object::lean_obj_res> {
            let lean = #leo3_crate::Lean::assume_initialized();

            #self_conversion
            #(#param_conversions)*

            let result = self_mut.#method_name(#(#param_names),*);

            #result_conversion
        }

        #[allow(non_upper_case_globals)]
        const _: () = {
            #[link_section = "__DATA,__lean_metadata"]
            #[used]
            static __LEAN_METADATA: #leo3_crate::LeanFunctionMetadata =
                #metadata;
        };
    })
}

/// Generate wrapper for self methods (consuming)
fn generate_owned_method_wrapper(
    method: &MethodInfo,
    binding: &FunctionBinding,
    struct_name: &syn::Ident,
    ffi_name: &syn::Ident,
    try_name: &syn::Ident,
    leo3_crate: &TokenStream,
) -> syn::Result<TokenStream> {
    let method_name = &method.name;
    let params = &method.params;
    let return_type = &method.return_type;

    // First parameter is the external object
    let param_count = params.len() + 1;
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

    // Convert self parameter (consuming) with move semantics.
    // If exclusively owned (RC == 1), move the value out without cloning.
    // If shared (RC > 1), clone the inner value instead.
    let self_conversion = quote! {
        let self_owned: #struct_name = if #leo3_crate::ffi::object::lean_is_exclusive(arg0) {
            // Exclusively owned — move the value out (no clone).
            let mut self_obj = #leo3_crate::LeanBound::<#leo3_crate::external::LeanExternalType<#struct_name>>::from_owned_ptr(lean, arg0);
            self_obj.take_inner()
        } else {
            // Shared — clone the inner value.
            let self_obj = #leo3_crate::LeanBound::<#leo3_crate::external::LeanExternalType<#struct_name>>::from_owned_ptr(lean, arg0);
            self_obj.get_ref().clone()
        };
    };

    // Convert other parameters
    let param_conversions = generate_param_conversions_with_offset(params, 1, leo3_crate);

    // Generate method call
    let param_names: Vec<_> = params.iter().map(|(name, _)| name).collect();

    // Generate result conversion
    let result_conversion = if is_unit_type(return_type) {
        quote! {
            Ok(unsafe { #leo3_crate::ffi::lean_mk_unit() })
        }
    } else {
        quote! {
            {
                let lean_result = <#return_type as #leo3_crate::conversion::IntoLean>::into_lean(result, lean)
                    .map_err(|e| #leo3_crate::LeanError::Conversion(format!(
                        "Failed to convert Rust result to Lean: {}",
                        e
                    )))?;
                Ok(lean_result.into_ptr())
            }
        }
    };
    let ffi_wrapper = generate_object_ffi_wrapper(
        ffi_name,
        try_name,
        &ffi_params,
        &wrapper_call_args,
        leo3_crate,
    );
    let metadata = quote_runtime_function_metadata(binding, leo3_crate);

    Ok(quote! {
        #ffi_wrapper

        #[doc(hidden)]
        #[allow(non_snake_case)]
        pub(crate) unsafe fn #try_name(#(#ffi_params),*) -> #leo3_crate::LeanResult<#leo3_crate::ffi::object::lean_obj_res> {
            let lean = #leo3_crate::Lean::assume_initialized();

            #self_conversion
            #(#param_conversions)*

            let result = self_owned.#method_name(#(#param_names),*);

            #result_conversion
        }

        #[allow(non_upper_case_globals)]
        const _: () = {
            #[link_section = "__DATA,__lean_metadata"]
            #[used]
            static __LEAN_METADATA: #leo3_crate::LeanFunctionMetadata =
                #metadata;
        };
    })
}

/// Generate a string constant containing the Lean `opaque` type declaration for the struct.
///
/// For a struct named `Foo`, this produces:
/// ```ignore
/// pub const FOO_LEAN_CLASS_DECL: &str = "opaque Foo : Type";
/// ```
fn generate_lean_code_metadata(class_binding: &ClassTypeBinding) -> TokenStream {
    let const_name = format_ident!("{}_LEAN_CLASS_DECL", class_binding.rust_name.to_uppercase());
    let lean_code = &class_binding.opaque_decl;

    quote! {
        pub const #const_name: &str = #lean_code;
    }
}

/// Generate a string constant containing Lean `@[extern]` declarations for each method.
///
/// For a struct `Foo` with a method `bar(&self, x: u32) -> u32`, this produces:
/// ```ignore
/// pub const FOO_LEAN_METHODS_DECL: &str = r#"@[extern "__lean_ffi_Foo_bar"] opaque Foo.bar : Foo → UInt32 → UInt32"#;
/// ```
///
/// Receiver types are mapped as follows:
/// - `&self` / `&mut self` / `self` → the struct type as the first parameter
/// - `&mut self` with `()` return → return type becomes the struct
/// - `&mut self` with non-unit return → return type becomes `Prod Struct Return`
fn generate_lean_code_metadata_for_methods(
    class_binding: &ClassTypeBinding,
    impl_binding: &ClassImplBinding,
    leo3_crate: &TokenStream,
) -> syn::Result<TokenStream> {
    let const_name = format_ident!(
        "{}_LEAN_METHODS_DECL",
        class_binding.rust_name.to_uppercase()
    );
    let lean_code = &impl_binding.methods_decl;
    let class_metadata = quote_runtime_class_metadata(class_binding, impl_binding, leo3_crate);
    let class_metadata_fn = format_ident!("__leo3_class_metadata_{}", class_binding.rust_name);
    let json_symbol_name = format_ident!(
        "__leo3_class_metadata_json_{}",
        class_binding.rust_name
    );
    let json_str = class_binding_to_json(class_binding, impl_binding);
    let json_bytes = json_str.as_bytes();
    let json_len = json_bytes.len() + 1;
    let byte_literals: Vec<proc_macro2::Literal> = json_bytes
        .iter()
        .map(|&b| proc_macro2::Literal::u8_suffixed(b))
        .collect();

    Ok(quote! {
        pub const #const_name: &str = #lean_code;

        #[doc(hidden)]
        pub fn #class_metadata_fn() -> #leo3_crate::LeanClassMetadata {
            #class_metadata
        }

        #[doc(hidden)]
        #[no_mangle]
        #[used]
        pub static #json_symbol_name: [u8; #json_len] = [#(#byte_literals),*, 0u8];
    })
}

/// Check if a type is unit ()
fn is_unit_type(ty: &syn::Type) -> bool {
    matches!(ty, syn::Type::Tuple(t) if t.elems.is_empty())
}

/// Remove the helper `#[getter]` / `#[setter]` attributes from methods in an
/// impl block so they are not emitted into the expanded output (where the
/// compiler would reject them as unknown attributes).
fn strip_accessor_attrs(item: &mut syn::ItemImpl) {
    for impl_item in &mut item.items {
        if let syn::ImplItem::Fn(method) = impl_item {
            method
                .attrs
                .retain(|attr| !attr.path().is_ident("getter") && !attr.path().is_ident("setter"));
        }
    }
}

/// Check if return type is Self
fn is_self_return(ty: &syn::Type, struct_name: &syn::Ident) -> bool {
    match ty {
        syn::Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                segment.ident == "Self" || segment.ident == *struct_name
            } else {
                false
            }
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CommonOptions;

    #[test]
    fn generated_leanclass_wrapper_uses_boundary_helpers() {
        let mut item: syn::ItemImpl = syn::parse_quote! {
            impl Demo {
                fn bump(&mut self, amount: i32) -> i32 {
                    self.value += amount;
                    self.value
                }
            }
        };

        let tokens = build_lean_class_impl(
            &mut item,
            LeanClassOptions {
                common: CommonOptions::default(),
            },
        )
        .expect("leanclass expansion should succeed");

        let rendered = tokens.to_string();
        assert!(rendered.contains("__private :: ffi_panic_boundary"));
        assert!(rendered.contains("Failed to convert"));
        assert!(rendered.contains("amount"));
        assert!(rendered.contains("Failed to convert Rust result to Lean"));
        assert!(!rendered.contains(".expect("));
        assert!(!rendered.contains(". expect ("));
    }
}
