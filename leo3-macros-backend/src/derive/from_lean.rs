//! Code generation for `#[derive(FromLean)]`.

use crate::derive::common::*;
use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

/// Expand the `FromLean` derive macro.
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

/// Generate FromLean implementation for a struct.
fn expand_struct(type_info: &TypeInfo, struct_info: &StructInfo) -> TokenStream {
    let type_name = &type_info.name;
    let (impl_generics, type_generics) = add_trait_bounds(&type_info.generics, "FromLean");
    let (impl_generics, _ty_generics, where_clause) = impl_generics.split_for_impl();
    let (_impl_g, type_ty_generics, _where_c) = type_generics.split_for_impl();

    let leo3_crate = get_leo3_crate();

    // Check for transparent attribute
    if type_info.attrs.transparent {
        let field = &struct_info.fields[0];
        let field_ty = &field.ty;
        let construction = if let Some(name) = field.name.as_ref() {
            quote! { #type_name { #name: inner_value } }
        } else {
            quote! { #type_name(inner_value) }
        };

        return quote! {
            impl #impl_generics #leo3_crate::conversion::FromLean<'l> for #type_name #type_ty_generics #where_clause {
                type Source = #leo3_crate::instance::LeanAny;

                fn from_lean(obj: &#leo3_crate::instance::LeanBound<'l, Self::Source>) -> #leo3_crate::err::LeanResult<Self> {
                    // Cast the LeanAny to the inner type's Source type
                    let inner_bound = unsafe {
                        #leo3_crate::instance::LeanBound::from_borrowed_ptr(
                            obj.lean_token(),
                            obj.as_ptr()
                        )
                    };
                    let inner_value = <#field_ty as #leo3_crate::conversion::FromLean>::from_lean(&inner_bound)?;
                    Ok(#construction)
                }
            }
        };
    }

    // Non-transparent: extract from constructor
    let field_extractions = generate_field_extractions(&struct_info.fields);
    let struct_construction = generate_struct_construction(type_name, &struct_info.fields);

    quote! {
        impl #impl_generics #leo3_crate::conversion::FromLean<'l> for #type_name #type_ty_generics #where_clause {
            type Source = #leo3_crate::instance::LeanAny;

            fn from_lean(obj: &#leo3_crate::instance::LeanBound<'l, Self::Source>) -> #leo3_crate::err::LeanResult<Self> {
                unsafe {
                    let lean = obj.lean_token();

                    #field_extractions

                    Ok(#struct_construction)
                }
            }
        }
    }
}

/// Generate FromLean implementation for an enum.
fn expand_enum(type_info: &TypeInfo, enum_info: &EnumInfo) -> TokenStream {
    let type_name = &type_info.name;
    let (impl_generics, type_generics) = add_trait_bounds(&type_info.generics, "FromLean");
    let (impl_generics, _ty_generics, where_clause) = impl_generics.split_for_impl();
    let (_impl_g, type_ty_generics, _where_c) = type_generics.split_for_impl();

    let variant_arms = enum_info
        .variants
        .iter()
        .map(|variant| generate_variant_arm(type_name, variant));

    let type_name_str = type_name.to_string();
    let leo3_crate = get_leo3_crate();

    quote! {
        impl #impl_generics #leo3_crate::conversion::FromLean<'l> for #type_name #type_ty_generics #where_clause {
            type Source = #leo3_crate::instance::LeanAny;

            fn from_lean(obj: &#leo3_crate::instance::LeanBound<'l, Self::Source>) -> #leo3_crate::err::LeanResult<Self> {
                unsafe {
                    let lean = obj.lean_token();
                    let tag = #leo3_crate::ffi::lean_obj_tag(obj.as_ptr());

                    match tag {
                        #(#variant_arms)*
                        _ => Err(#leo3_crate::err::LeanError::conversion(&format!("Invalid tag {} for {}", tag, #type_name_str)))
                    }
                }
            }
        }
    }
}

/// Generate field extractions for a struct.
fn generate_field_extractions(fields: &[Field]) -> TokenStream {
    let leo3_crate = get_leo3_crate();

    // Filter out skipped fields and recalculate indices for Lean constructor access
    let active = active_fields(fields);

    let extractions = active.iter().map(|field| {
        let var_name = field_var_name(field);
        let field_index = field.index as u32; // Use recalculated index
        let field_ty = &field.ty;
        let error_prefix = field_error_prefix(field);

        // Check for custom extraction function
        if let Some(with_path) = &field.attrs.with {
            if field.attrs.default {
                // Custom extraction with default fallback
                quote! {
                    let #var_name = {
                        let field_ptr = #leo3_crate::ffi::lean_ctor_get(obj.as_ptr(), #field_index);
                        #leo3_crate::ffi::lean_inc(field_ptr as *mut _);
                        let field_bound = #leo3_crate::instance::LeanBound::from_owned_ptr(lean, field_ptr as *mut _);
                        #with_path(&field_bound).unwrap_or_else(|_| Default::default())
                    };
                }
            } else {
                // Custom extraction
                quote! {
                    let field_ptr = #leo3_crate::ffi::lean_ctor_get(obj.as_ptr(), #field_index);
                    #leo3_crate::ffi::lean_inc(field_ptr as *mut _);
                    let field_bound = #leo3_crate::instance::LeanBound::from_owned_ptr(lean, field_ptr as *mut _);
                    let #var_name = #with_path(&field_bound)
                        .map_err(|e| #leo3_crate::err::LeanError::conversion(&format!("{}{}", #error_prefix, e)))?;
                }
            }
        } else if field.attrs.default {
            // Standard extraction with default fallback
            quote! {
                let #var_name = {
                    let field_ptr = #leo3_crate::ffi::lean_ctor_get(obj.as_ptr(), #field_index);
                    #leo3_crate::ffi::lean_inc(field_ptr as *mut _);
                    let field_bound = #leo3_crate::instance::LeanBound::from_owned_ptr(lean, field_ptr as *mut _);
                    <#field_ty as #leo3_crate::conversion::FromLean>::from_lean(&field_bound)
                        .unwrap_or_else(|_| Default::default())
                };
            }
        } else {
            // Standard extraction
            quote! {
                let field_ptr = #leo3_crate::ffi::lean_ctor_get(obj.as_ptr(), #field_index);
                #leo3_crate::ffi::lean_inc(field_ptr as *mut _);
                let field_bound = #leo3_crate::instance::LeanBound::from_owned_ptr(lean, field_ptr as *mut _);
                let #var_name = <#field_ty as #leo3_crate::conversion::FromLean>::from_lean(&field_bound)
                    .map_err(|e| #leo3_crate::err::LeanError::conversion(&format!("{}{}", #error_prefix, e)))?;
            }
        }
    });

    quote! {
        #(#extractions)*
    }
}

/// Generate struct construction from extracted fields.
fn generate_struct_construction(type_name: &syn::Ident, fields: &[Field]) -> TokenStream {
    if fields.is_empty() {
        // Unit struct
        quote! { #type_name }
    } else if fields[0].name.is_some() {
        // Struct with named fields
        let field_inits = fields.iter().map(|field| {
            let name = field.name.as_ref().unwrap();
            if field.attrs.skip {
                // Use Default::default() for skipped fields
                quote! { #name: Default::default() }
            } else {
                let var_name = field_var_name(field);
                quote! { #name: #var_name }
            }
        });
        quote! { #type_name { #(#field_inits),* } }
    } else {
        // Tuple struct
        let field_vars = fields.iter().map(|field| {
            if field.attrs.skip {
                // Use Default::default() for skipped fields
                quote! { Default::default() }
            } else {
                let var_name = field_var_name(field);
                quote! { #var_name }
            }
        });
        quote! { #type_name ( #(#field_vars),* ) }
    }
}

/// Generate a match arm for an enum variant.
fn generate_variant_arm(type_name: &syn::Ident, variant: &Variant) -> TokenStream {
    let variant_name = &variant.name;
    let tag = variant.tag as u32;

    let _leo3_crate = get_leo3_crate();

    if variant.fields.is_empty() {
        // Unit variant
        quote! {
            #tag => Ok(#type_name::#variant_name),
        }
    } else {
        // Variant with fields
        let field_extractions = generate_variant_field_extractions(&variant.fields, &variant.name);
        let variant_construction = generate_variant_construction(type_name, variant);

        quote! {
            #tag => {
                #field_extractions
                Ok(#variant_construction)
            }
        }
    }
}

/// Generate field extractions for an enum variant.
fn generate_variant_field_extractions(fields: &[Field], variant_name: &syn::Ident) -> TokenStream {
    let leo3_crate = get_leo3_crate();

    let extractions = fields.iter().map(|field| {
        let var_name = field_var_name(field);
        let field_index = field.index as u32;
        let field_ty = &field.ty;
        let error_prefix = format!("Variant '{}' field {}: ", variant_name, field.index);

        quote! {
            let field_ptr = #leo3_crate::ffi::lean_ctor_get(obj.as_ptr(), #field_index);
            #leo3_crate::ffi::lean_inc(field_ptr as *mut _);
            let field_bound = #leo3_crate::instance::LeanBound::from_owned_ptr(lean, field_ptr as *mut _);
            let #var_name = <#field_ty as #leo3_crate::conversion::FromLean>::from_lean(&field_bound)
                .map_err(|e| #leo3_crate::err::LeanError::conversion(&format!("{}{}", #error_prefix, e)))?;
        }
    });

    quote! {
        #(#extractions)*
    }
}

/// Generate variant construction from extracted fields.
fn generate_variant_construction(type_name: &syn::Ident, variant: &Variant) -> TokenStream {
    let variant_name = &variant.name;

    if variant.fields.is_empty() {
        quote! { #type_name::#variant_name }
    } else if variant.fields[0].name.is_some() {
        // Struct variant
        let field_inits = variant.fields.iter().map(|field| {
            let name = field.name.as_ref().unwrap();
            let var_name = field_var_name(field);
            quote! { #name: #var_name }
        });
        quote! { #type_name::#variant_name { #(#field_inits),* } }
    } else {
        // Tuple variant
        let field_vars = variant.fields.iter().map(field_var_name);
        quote! { #type_name::#variant_name ( #(#field_vars),* ) }
    }
}
