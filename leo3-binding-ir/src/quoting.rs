use proc_macro2::TokenStream;
use quote::quote;

use crate::embed::{frame_metadata_entry, METADATA_SECTION_NAME, METADATA_SECTION_NAME_APPLE};
use crate::model::*;

/// Emit a `#[used]` static that embeds a framed metadata entry into the Leo3
/// metadata link section.
///
/// This is the cross-platform counterpart to the `#[no_mangle]` JSON symbols:
/// on Mach-O the linker does not surface those unreferenced data symbols in the
/// dylib symbol table, so `leo3-codegen` recovers the metadata by scanning the
/// dedicated section instead. The entry is self-describing (see the `embed`
/// module and [`crate::parse_metadata_entries`]) so the scanner can find it
/// regardless of padding/ordering. The section static is also a public,
/// unmangled symbol because the linker may otherwise garbage-collect the
/// custom section from a final `cdylib`, despite `#[used]` retaining it in the
/// intermediate object file.
///
/// * `static_ident` - unique identifier for the generated static.
/// * `symbol_name` - the full metadata symbol name embedded in the framing
///   (e.g. `__leo3_module_metadata_json_Foo`); its prefix tells consumers
///   whether this is module or class metadata.
/// * `json_str` - the serialized metadata JSON.
pub fn quote_metadata_section_static(
    static_ident: &proc_macro2::Ident,
    symbol_name: &str,
    json_str: &str,
) -> TokenStream {
    let framed = frame_metadata_entry(symbol_name, json_str);
    let framed_len = framed.len();
    let byte_literals = framed.iter().map(|&b| proc_macro2::Literal::u8_suffixed(b));

    quote! {
        #[doc(hidden)]
        // `#[used]` keeps this static in the object file, while the exported
        // symbol prevents the final cdylib linker's section GC from dropping
        // the custom section before leo3-codegen can scan it.
        #[no_mangle]
        #[used]
        #[cfg_attr(target_vendor = "apple", link_section = #METADATA_SECTION_NAME_APPLE)]
        #[cfg_attr(not(target_vendor = "apple"), link_section = #METADATA_SECTION_NAME)]
        pub static #static_ident: [u8; #framed_len] = [#(#byte_literals),*];
    }
}

pub fn quote_runtime_function_metadata(
    binding: &FunctionBinding,
    leo3_crate: &TokenStream,
) -> TokenStream {
    let rust_name = &binding.rust_name;
    let lean_name = &binding.lean_name;
    let owner = quote_opt_str(binding.owner.as_deref());
    let ffi_symbol = &binding.ffi_symbol;
    let receiver = quote_receiver(binding.receiver, leo3_crate);
    let params = binding
        .params
        .iter()
        .map(|param| quote_runtime_parameter_metadata(param, leo3_crate));
    let return_type = quote_runtime_type_metadata(&binding.return_type, leo3_crate);
    let semantics = quote_semantics(binding.semantics, leo3_crate);
    let kind = quote_binding_kind(binding.kind, leo3_crate);
    let lean_decl = quote_opt_str(binding.lean_decl.as_deref());

    quote! {
        #leo3_crate::LeanFunctionMetadata {
            schema_version: #leo3_crate::LEO3_BINDING_SCHEMA_VERSION,
            rust_name: #rust_name,
            name: #lean_name,
            owner: #owner,
            ffi_symbol: #ffi_symbol,
            receiver: #receiver,
            params: &[#(#params),*],
            return_type: #return_type,
            semantics: #semantics,
            kind: #kind,
            lean_decl: #lean_decl,
        }
    }
}

pub fn quote_runtime_class_metadata(
    class_binding: &ClassTypeBinding,
    impl_binding: &ClassImplBinding,
    leo3_crate: &TokenStream,
) -> TokenStream {
    let rust_name = &class_binding.rust_name;
    let lean_name = &class_binding.lean_name;
    let opaque_decl = &class_binding.opaque_decl;
    let methods_decl = &impl_binding.methods_decl;
    let methods = impl_binding
        .methods
        .iter()
        .map(|method| quote_runtime_function_metadata(method, leo3_crate));

    quote! {
        #leo3_crate::LeanClassMetadata {
            schema_version: #leo3_crate::LEO3_BINDING_SCHEMA_VERSION,
            rust_name: #rust_name,
            lean_name: #lean_name,
            opaque_decl: #opaque_decl,
            methods_decl: #methods_decl,
            methods: &[#(#methods),*],
        }
    }
}

pub fn quote_runtime_module_metadata(
    binding: &ModuleBinding,
    leo3_crate: &TokenStream,
) -> TokenStream {
    let name = &binding.name;
    let exports = binding
        .exports
        .iter()
        .map(|export| quote_runtime_function_metadata(export, leo3_crate));
    let submodules = binding
        .submodules
        .iter()
        .map(|sub| quote_runtime_submodule_metadata(sub, leo3_crate));

    quote! {
        #leo3_crate::LeanModuleMetadata {
            schema_version: #leo3_crate::LEO3_BINDING_SCHEMA_VERSION,
            name: #name,
            exports: &[#(#exports),*],
            submodules: &[#(#submodules),*],
        }
    }
}

pub fn quote_runtime_submodule_metadata(
    binding: &SubmoduleBinding,
    leo3_crate: &TokenStream,
) -> TokenStream {
    let path = &binding.path;
    let exports = binding
        .exports
        .iter()
        .map(|export| quote_runtime_function_metadata(export, leo3_crate));

    quote! {
        #leo3_crate::LeanSubmoduleMetadata {
            schema_version: #leo3_crate::LEO3_BINDING_SCHEMA_VERSION,
            path: #path,
            exports: &[#(#exports),*],
        }
    }
}

fn quote_runtime_parameter_metadata(
    param: &ParameterBinding,
    leo3_crate: &TokenStream,
) -> TokenStream {
    let name = &param.name;
    let ty = quote_runtime_type_metadata(&param.ty, leo3_crate);
    let passing = match param.passing {
        PassingStyle::Owned => quote! { #leo3_crate::LeanPassingStyle::Owned },
        PassingStyle::Borrowed => quote! { #leo3_crate::LeanPassingStyle::Borrowed },
    };

    quote! {
        #leo3_crate::LeanParameterMetadata {
            name: #name,
            ty: #ty,
            passing: #passing,
        }
    }
}

fn quote_runtime_type_metadata(binding: &TypeBinding, leo3_crate: &TokenStream) -> TokenStream {
    let rust = &binding.rust;
    let lean = quote_opt_str(binding.lean.as_deref());
    let shape = match binding.shape {
        TypeShape::Unit => quote! { #leo3_crate::LeanTypeShape::Unit },
        TypeShape::Scalar => quote! { #leo3_crate::LeanTypeShape::Scalar },
        TypeShape::String => quote! { #leo3_crate::LeanTypeShape::String },
        TypeShape::ByteArray => quote! { #leo3_crate::LeanTypeShape::ByteArray },
        TypeShape::Array => quote! { #leo3_crate::LeanTypeShape::Array },
        TypeShape::Option => quote! { #leo3_crate::LeanTypeShape::Option },
        TypeShape::Except => quote! { #leo3_crate::LeanTypeShape::Except },
        TypeShape::Prod => quote! { #leo3_crate::LeanTypeShape::Prod },
        TypeShape::Named => quote! { #leo3_crate::LeanTypeShape::Named },
        TypeShape::Unknown => quote! { #leo3_crate::LeanTypeShape::Unknown },
    };

    quote! {
        #leo3_crate::LeanTypeMetadata {
            rust: #rust,
            lean: #lean,
            shape: #shape,
        }
    }
}

fn quote_receiver(receiver: ReceiverStyle, leo3_crate: &TokenStream) -> TokenStream {
    match receiver {
        ReceiverStyle::None => quote! { #leo3_crate::LeanBindingReceiver::None },
        ReceiverStyle::Ref => quote! { #leo3_crate::LeanBindingReceiver::Ref },
        ReceiverStyle::MutRef => quote! { #leo3_crate::LeanBindingReceiver::MutRef },
        ReceiverStyle::Owned => quote! { #leo3_crate::LeanBindingReceiver::Owned },
    }
}

fn quote_semantics(semantics: BindingSemantics, leo3_crate: &TokenStream) -> TokenStream {
    match semantics {
        BindingSemantics::Value => quote! { #leo3_crate::LeanBindingSemantics::Value },
        BindingSemantics::MutatesSelf => {
            quote! { #leo3_crate::LeanBindingSemantics::MutatesSelf }
        }
        BindingSemantics::MutatesSelfWithValue => {
            quote! { #leo3_crate::LeanBindingSemantics::MutatesSelfWithValue }
        }
    }
}

fn quote_binding_kind(kind: BindingKind, leo3_crate: &TokenStream) -> TokenStream {
    match kind {
        BindingKind::Method => quote! { #leo3_crate::LeanBindingKind::Method },
        BindingKind::Getter => quote! { #leo3_crate::LeanBindingKind::Getter },
        BindingKind::Setter => quote! { #leo3_crate::LeanBindingKind::Setter },
    }
}

fn quote_opt_str(value: Option<&str>) -> TokenStream {
    match value {
        Some(value) => quote! { Some(#value) },
        None => quote! { None },
    }
}
