#![cfg_attr(docsrs, feature(doc_cfg))]
//! Implementation details for Leo3 procedural macros.
//!
//! This crate contains the actual implementation of the Leo3 proc macros.
//! It is not meant to be used directly - use the `leo3-macros` crate instead.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse::Parse, punctuated::Punctuated, Token};

pub mod conversion_plan;
pub mod derive;
pub mod lean_instance;
pub mod leanclass;
pub mod leanfn;
pub mod scalar_abi;
pub mod surface;

pub use lean_instance::{build_lean_instance, LeanInstanceOptions};
pub use leanclass::{build_lean_class_impl, build_lean_class_struct, LeanClassOptions};
pub use leanfn::{build_lean_function, LeanFunctionOptions};

/// Common options that can be applied to Leo3 items
#[derive(Default)]
pub struct CommonOptions {
    /// Custom name for the Lean4 item
    pub name: Option<syn::LitStr>,
    /// Path to the leo3 crate (for re-exports)
    pub krate: Option<syn::Path>,
}

impl Parse for CommonOptions {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut options = CommonOptions::default();

        let attrs: Punctuated<syn::Meta, Token![,]> =
            input.parse_terminated(syn::Meta::parse, Token![,])?;

        for attr in attrs {
            match attr {
                syn::Meta::NameValue(nv) => {
                    if nv.path.is_ident("name") {
                        if let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(s),
                            ..
                        }) = nv.value
                        {
                            options.name = Some(s);
                        }
                    } else if nv.path.is_ident("crate") {
                        if let syn::Expr::Path(path) = nv.value {
                            options.krate = Some(path.path);
                        }
                    }
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        attr,
                        "Expected name-value attribute like `name = \"...\"`",
                    ))
                }
            }
        }

        Ok(options)
    }
}

/// Get the path to the leo3 crate (handles re-exports)
pub fn get_leo3_crate(krate: Option<&syn::Path>) -> TokenStream {
    if let Some(krate) = krate {
        quote! { #krate }
    } else {
        quote! { ::leo3 }
    }
}
