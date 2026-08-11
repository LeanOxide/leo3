//! Code generation for `#[derive(IntoLean)]`.

use crate::derive::common::*;
use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

/// Expand the `IntoLean` derive macro.
pub fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let type_info = analyze_type(&input)?;

    let impl_tokens = match &type_info.data {
        TypeData::Struct(struct_info) => expand_struct(&type_info, struct_info),
        TypeData::Enum(enum_info) => expand_enum(&type_info, enum_info),
    };

    Ok(quote! {
        #[allow(clippy::redundant_field_names)]
        #impl_tokens
    })
}

/// Generate IntoLean implementation for a struct.
fn expand_struct(type_info: &TypeInfo, struct_info: &StructInfo) -> TokenStream {
    let type_name = &type_info.name;
    let (impl_generics, type_generics) = add_trait_bounds(&type_info.generics, "IntoLean");
    let (impl_generics, _ty_generics, where_clause) = impl_generics.split_for_impl();
    let (_impl_g, type_ty_generics, _where_c) = type_generics.split_for_impl();

    let leo3_crate = get_leo3_crate();

    // Check for transparent attribute
    if type_info.attrs.transparent {
        let field = &struct_info.fields[0];
        let field_name = field_name(field);
        let field_ty = &field.ty;

        return quote! {
            impl #impl_generics #leo3_crate::conversion::IntoLean<'l> for #type_name #type_ty_generics #where_clause {
                type Target = #leo3_crate::instance::LeanAny;

                fn into_lean(self, lean: #leo3_crate::marker::Lean<'l>) -> #leo3_crate::err::LeanResult<#leo3_crate::instance::LeanBound<'l, Self::Target>> {
                    let inner = <#field_ty as #leo3_crate::conversion::IntoLean>::into_lean(self.#field_name, lean)?;
                    Ok(unsafe { #leo3_crate::instance::LeanBound::from_owned_ptr(lean, inner.into_ptr()) })
                }
            }
        };
    }

    // Non-transparent: use constructor
    // Use active_fields to exclude skipped fields
    let active = active_fields(&struct_info.fields);
    let num_fields = active.len() as u32;
    let field_conversions = generate_field_conversions(&struct_info.fields);

    quote! {
        impl #impl_generics #leo3_crate::conversion::IntoLean<'l> for #type_name #type_ty_generics #where_clause {
            type Target = #leo3_crate::instance::LeanAny;

            fn into_lean(self, lean: #leo3_crate::marker::Lean<'l>) -> #leo3_crate::err::LeanResult<#leo3_crate::instance::LeanBound<'l, Self::Target>> {
                unsafe {
                    let ptr = #leo3_crate::ffi::lean_alloc_ctor(0, #num_fields, 0);

                    #field_conversions

                    Ok(#leo3_crate::instance::LeanBound::from_owned_ptr(lean, ptr))
                }
            }
        }
    }
}

/// Generate IntoLean implementation for an enum.
fn expand_enum(type_info: &TypeInfo, enum_info: &EnumInfo) -> TokenStream {
    let type_name = &type_info.name;
    let (impl_generics, type_generics) = add_trait_bounds(&type_info.generics, "IntoLean");
    let (impl_generics, _ty_generics, where_clause) = impl_generics.split_for_impl();
    let (_impl_g, type_ty_generics, _where_c) = type_generics.split_for_impl();

    let variant_arms = enum_info
        .variants
        .iter()
        .map(|variant| generate_variant_arm(type_name, variant));

    let leo3_crate = get_leo3_crate();

    quote! {
        impl #impl_generics #leo3_crate::conversion::IntoLean<'l> for #type_name #type_ty_generics #where_clause {
            type Target = #leo3_crate::instance::LeanAny;

            fn into_lean(self, lean: #leo3_crate::marker::Lean<'l>) -> #leo3_crate::err::LeanResult<#leo3_crate::instance::LeanBound<'l, Self::Target>> {
                unsafe {
                    match self {
                        #(#variant_arms)*
                    }
                }
            }
        }
    }
}

/// Generate field conversions for a struct.
fn generate_field_conversions(fields: &[Field]) -> TokenStream {
    let leo3_crate = get_leo3_crate();

    // Filter out skipped fields and use recalculated indices
    let active = active_fields(fields);

    let conversions = active.iter().map(|field| {
        let field_name = field_name(field);
        let field_index = field.index as u32;
        let field_ty = &field.ty;
        let error_prefix = field_error_prefix(field);

        // Check for custom conversion function
        if let Some(with_path) = &field.attrs.with {
            quote! {
                let field_value = #with_path(self.#field_name, lean)
                    .map_err(|e| #leo3_crate::err::LeanError::conversion(&format!("{}{}", #error_prefix, e)))?;
                #leo3_crate::ffi::lean_ctor_set(ptr, #field_index, field_value.into_ptr());
            }
        } else {
            // Standard conversion
            quote! {
                let field_value = <#field_ty as #leo3_crate::conversion::IntoLean>::into_lean(self.#field_name, lean)
                    .map_err(|e| #leo3_crate::err::LeanError::conversion(&format!("{}{}", #error_prefix, e)))?;
                #leo3_crate::ffi::lean_ctor_set(ptr, #field_index, field_value.into_ptr());
            }
        }
    });

    quote! {
        #(#conversions)*
    }
}

/// Generate a match arm for an enum variant.
fn generate_variant_arm(type_name: &syn::Ident, variant: &Variant) -> TokenStream {
    let variant_name = &variant.name;
    let tag = variant.tag as u32;
    let num_fields = variant.fields.len() as u32;

    let leo3_crate = get_leo3_crate();

    if variant.fields.is_empty() {
        // Unit variant
        quote! {
            #type_name::#variant_name => {
                let ptr = #leo3_crate::ffi::lean_alloc_ctor(#tag, 0, 0);
                Ok(#leo3_crate::instance::LeanBound::from_owned_ptr(lean, ptr))
            }
        }
    } else {
        // Variant with fields
        let pattern = generate_variant_pattern(&variant.fields);
        let field_conversions = generate_variant_field_conversions(&variant.fields, &variant.name);

        quote! {
            #type_name::#variant_name #pattern => {
                let ptr = #leo3_crate::ffi::lean_alloc_ctor(#tag, #num_fields, 0);
                #field_conversions
                Ok(#leo3_crate::instance::LeanBound::from_owned_ptr(lean, ptr))
            }
        }
    }
}

/// Generate a pattern for matching an enum variant.
fn generate_variant_pattern(fields: &[Field]) -> TokenStream {
    if fields.is_empty() {
        quote! {}
    } else if fields[0].name.is_some() {
        // Struct variant - use shorthand field patterns
        let field_names = fields.iter().map(|field| field.name.as_ref().unwrap());
        quote! { { #(#field_names),* } }
    } else {
        // Tuple variant
        let field_vars = fields.iter().map(field_var_name);
        quote! { ( #(#field_vars),* ) }
    }
}

/// Generate field conversions for an enum variant.
fn generate_variant_field_conversions(fields: &[Field], variant_name: &syn::Ident) -> TokenStream {
    let leo3_crate = get_leo3_crate();

    let conversions = fields.iter().map(|field| {
        let var_name = field_var_name(field);
        let field_index = field.index as u32;
        let field_ty = &field.ty;
        let error_prefix = format!("Variant '{}' field {}: ", variant_name, field.index);

        quote! {
            let field_value = <#field_ty as #leo3_crate::conversion::IntoLean>::into_lean(#var_name, lean)
                .map_err(|e| #leo3_crate::err::LeanError::conversion(&format!("{}{}", #error_prefix, e)))?;
            #leo3_crate::ffi::lean_ctor_set(ptr, #field_index, field_value.into_ptr());
        }
    });

    quote! {
        #(#conversions)*
    }
}
