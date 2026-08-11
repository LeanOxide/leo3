//! Implementation of the `#[lean_instance]` macro.
//!
//! This macro generates FFI functions that implement Lean type classes for Rust types.
//!
//! Supported forms (one or more type classes per attribute):
//! - `#[lean_instance(BEq)]` requires `fn beq(&self, other: &Self) -> bool`
//! - `#[lean_instance(Hashable)]` requires `fn hash(&self) -> u64`
//! - `#[lean_instance(Repr)]` requires `fn repr(&self) -> String`
//! - `#[lean_instance(ToString)]` requires `fn to_string(&self) -> String`
//! - `#[lean_instance(Ord)]` requires `fn compare(&self, other: &Self) -> std::cmp::Ordering`
//!
//! Combined forms additionally derive container key traits:
//! - `#[lean_instance(Hashable, BEq)]` also implements
//!   `LeanHashKey`, so the external class can be used as a
//!   `LeanHashMap` / `LeanHashSet` key (PyO3-aligned: any hashable key).
//! - adding `Ord` (`#[lean_instance(Hashable, BEq, Ord)]`) also implements
//!   `LeanRBMapKey` for `LeanRBMap` keys.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse::Parse, ImplItem, ItemImpl, Token};

use crate::get_leo3_crate;

/// Options for the `#[lean_instance]` macro.
pub struct LeanInstanceOptions {
    /// The type classes to implement (BEq, Hashable, Repr, ToString, Ord).
    /// Multiple classes are allowed: `#[lean_instance(Hashable, BEq, Ord)]`.
    pub typeclasses: Vec<TypeClass>,
    /// Path to the leo3 crate (for re-exports).
    pub krate: Option<syn::Path>,
}

impl Parse for LeanInstanceOptions {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        let mut typeclasses = Vec::new();
        let mut krate = None;

        loop {
            let ident: syn::Ident = input.parse()?;
            if ident == "crate" {
                input.parse::<Token![=]>()?;
                krate = Some(input.parse()?);
            } else {
                let typeclass = TypeClass::from_ident(&ident).ok_or_else(|| {
                    syn::Error::new(
                        ident.span(),
                        format!(
                            "Unsupported type class '{}'. Supported: BEq, Hashable, Repr, ToString, Ord",
                            ident
                        ),
                    )
                })?;
                typeclasses.push(typeclass);
            }

            if input.is_empty() {
                break;
            }
            input.parse::<Token![,]>()?;
            if input.is_empty() {
                break;
            }
        }

        if typeclasses.is_empty() {
            return Err(syn::Error::new(
                input.span(),
                "Expected at least one type class, e.g. `#[lean_instance(Hashable)]`",
            ));
        }

        Ok(LeanInstanceOptions { typeclasses, krate })
    }
}

/// Supported type classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeClass {
    /// Boolean equality (BEq)
    BEq,
    /// Hashing (Hashable)
    Hashable,
    /// Representation (Repr)
    Repr,
    /// String conversion (ToString)
    ToString,
    /// Ordering comparison (Ord)
    Ord,
}

impl TypeClass {
    /// Parse from identifier.
    pub fn from_ident(ident: &syn::Ident) -> Option<Self> {
        match ident.to_string().as_str() {
            "BEq" => Some(TypeClass::BEq),
            "Hashable" => Some(TypeClass::Hashable),
            "Repr" => Some(TypeClass::Repr),
            "ToString" => Some(TypeClass::ToString),
            "Ord" => Some(TypeClass::Ord),
            _ => None,
        }
    }

    /// Get the required method name.
    pub fn method_name(&self) -> &'static str {
        match self {
            TypeClass::BEq => "beq",
            TypeClass::Hashable => "hash",
            TypeClass::Repr => "repr",
            TypeClass::ToString => "to_string",
            TypeClass::Ord => "compare",
        }
    }

    /// Get the FFI function prefix.
    pub fn ffi_prefix(&self) -> &'static str {
        match self {
            TypeClass::BEq => "__lean_beq",
            TypeClass::Hashable => "__lean_hash",
            TypeClass::Repr => "__lean_repr",
            TypeClass::ToString => "__lean_to_string",
            TypeClass::Ord => "__lean_compare",
        }
    }
}

/// Build the `#[lean_instance]` macro output.
pub fn build_lean_instance(
    item: &mut ItemImpl,
    options: LeanInstanceOptions,
) -> syn::Result<TokenStream> {
    let struct_name = extract_struct_name(item)?;

    // Generate the FFI function for each requested type class.
    let leo3_crate = get_leo3_crate(options.krate.as_ref());
    let mut generated = Vec::new();
    for typeclass in &options.typeclasses {
        let method = find_method(item, typeclass.method_name())?;
        let ffi_fn = generate_ffi_function(&leo3_crate, &struct_name, *typeclass, method)?;
        generated.push(ffi_fn);
    }

    // When both Hashable and BEq are implemented, the type can be used as a
    // LeanHashMap / LeanHashSet key: generate the `ExternalHashKey` bridge
    // implementation (the `LeanHashKey` blanket impl lives in leo3, where the
    // orphan rule permits it).
    let hash_key_impl = if options.typeclasses.contains(&TypeClass::Hashable)
        && options.typeclasses.contains(&TypeClass::BEq)
    {
        let eq_fn = format_ident!("__lean_beq_{}", struct_name);
        let hash_fn = format_ident!("__lean_hash_{}", struct_name);
        Some(quote! {
            /// Auto-generated `ExternalHashKey` implementation: this type can
            /// be used as a `LeanHashMap` / `LeanHashSet` key.
            /// (The container wrappers are only available on Lean >= 4.22.)
            #[cfg(lean_4_22)]
            impl #leo3_crate::types::containers::hashmap::ExternalHashKey for #struct_name {
                fn decidable_eq_fn() -> *mut std::ffi::c_void {
                    #eq_fn as *mut std::ffi::c_void
                }

                fn hash_fn() -> *mut std::ffi::c_void {
                    #hash_fn as *mut std::ffi::c_void
                }
            }
        })
    } else {
        None
    };

    // With Ord the type can be used as a LeanRBMap key: generate the
    // `ExternalOrdKey` bridge implementation.
    let rb_key_impl = if options.typeclasses.contains(&TypeClass::Ord) {
        let compare_fn = format_ident!("__lean_compare_{}", struct_name);
        Some(quote! {
            /// Auto-generated `ExternalOrdKey` implementation: this type can
            /// be used as a `LeanRBMap` key.
            /// (The container wrappers are only available on Lean >= 4.22.)
            #[cfg(lean_4_22)]
            impl #leo3_crate::types::containers::rbmap::ExternalOrdKey for #struct_name {
                fn compare_fn() -> *mut std::ffi::c_void {
                    #compare_fn as *mut std::ffi::c_void
                }
            }
        })
    } else {
        None
    };

    // Return original impl + generated FFI functions (+ key impls)
    Ok(quote! {
        #item

        #(#generated)*

        #hash_key_impl

        #rb_key_impl
    })
}

/// Extract the struct name from an impl block.
fn extract_struct_name(item: &ItemImpl) -> syn::Result<syn::Ident> {
    match &*item.self_ty {
        syn::Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                Ok(segment.ident.clone())
            } else {
                Err(syn::Error::new_spanned(
                    &item.self_ty,
                    "Cannot determine struct name from impl block",
                ))
            }
        }
        _ => Err(syn::Error::new_spanned(
            &item.self_ty,
            "Expected a path type in impl block",
        )),
    }
}

/// Find a method in an impl block.
fn find_method<'a>(item: &'a ItemImpl, name: &str) -> syn::Result<&'a syn::ImplItemFn> {
    for impl_item in &item.items {
        if let ImplItem::Fn(method) = impl_item {
            if method.sig.ident == name {
                return Ok(method);
            }
        }
    }

    Err(syn::Error::new_spanned(
        item,
        format!(
            "Method '{}' not found in impl block. This method is required for the type class.",
            name
        ),
    ))
}

/// Generate the FFI function for a type class.
fn generate_ffi_function(
    leo3_crate: &TokenStream,
    struct_name: &syn::Ident,
    typeclass: TypeClass,
    _method: &syn::ImplItemFn,
) -> syn::Result<TokenStream> {
    let ffi_name = format_ident!("{}_{}", typeclass.ffi_prefix(), struct_name);
    let try_name = format_ident!("__leo3_try_{}", ffi_name);
    let method_ident = format_ident!("{}", typeclass.method_name());

    match typeclass {
        TypeClass::BEq => Ok(quote! {
            /// Auto-generated BEq instance implementation.
            #[no_mangle]
            pub unsafe extern "C" fn #ffi_name(
                a: *mut #leo3_crate::ffi::lean_object,
                b: *mut #leo3_crate::ffi::lean_object,
            ) -> *mut #leo3_crate::ffi::lean_object {
                #leo3_crate::__private::ffi_panic_boundary(|| #try_name(a, b))
            }

            #[doc(hidden)]
            #[allow(non_snake_case)]
            pub(crate) unsafe fn #try_name(
                a: *mut #leo3_crate::ffi::lean_object,
                b: *mut #leo3_crate::ffi::lean_object,
            ) -> #leo3_crate::LeanResult<*mut #leo3_crate::ffi::lean_object> {
                let lean = #leo3_crate::Lean::assume_initialized();

                // Extern entry points receive borrowed arguments (the caller
                // keeps its reference); balance the LeanBound drops below
                // with an inc so the net refcount change is zero.
                #leo3_crate::ffi::lean_inc(a);
                #leo3_crate::ffi::lean_inc(b);

                let a_bound: #leo3_crate::LeanBound<'_, #leo3_crate::external::LeanExternalType<#struct_name>> =
                    #leo3_crate::LeanBound::from_owned_ptr(lean, a);
                let b_bound: #leo3_crate::LeanBound<'_, #leo3_crate::external::LeanExternalType<#struct_name>> =
                    #leo3_crate::LeanBound::from_owned_ptr(lean, b);

                let result = a_bound.get_ref().#method_ident(b_bound.get_ref());
                Ok(#leo3_crate::ffi::inline::lean_box(result as usize))
            }
        }),

        TypeClass::Hashable => Ok(quote! {
            /// Auto-generated Hashable instance implementation.
            ///
            /// Lean's runtime applies `hash` through `lean_apply_1`, which
            /// always yields a boxed object, so the extern returns the boxed
            /// `UInt64` (matching the runtime representation of Lean's own
            /// `l_instHashable*` closures).
            #[no_mangle]
            pub unsafe extern "C" fn #ffi_name(
                obj: *mut #leo3_crate::ffi::lean_object,
            ) -> *mut #leo3_crate::ffi::lean_object {
                #leo3_crate::__private::ffi_panic_boundary(|| #try_name(obj))
            }

            #[doc(hidden)]
            #[allow(non_snake_case)]
            pub(crate) unsafe fn #try_name(
                obj: *mut #leo3_crate::ffi::lean_object,
            ) -> #leo3_crate::LeanResult<*mut #leo3_crate::ffi::lean_object> {
                let lean = #leo3_crate::Lean::assume_initialized();

                // Extern entry points receive borrowed arguments (the caller
                // keeps its reference); balance the LeanBound drop below
                // with an inc so the net refcount change is zero.
                #leo3_crate::ffi::lean_inc(obj);

                let obj_bound: #leo3_crate::LeanBound<'_, #leo3_crate::external::LeanExternalType<#struct_name>> =
                    #leo3_crate::LeanBound::from_owned_ptr(lean, obj);

                let result = obj_bound.get_ref().#method_ident();
                // Box the u64 the way Lean's own hash closures do: a ctor
                // with tag 0, no object fields, and the value in scalar
                // slot 0 (dereferenceable at offset 8, as DHashMap expects).
                let boxed = #leo3_crate::ffi::lean_alloc_ctor(0, 0, 1);
                #leo3_crate::ffi::object::lean_ctor_set_uint64(boxed, 0, result);
                Ok(boxed)
            }
        }),

        TypeClass::Repr => Ok(quote! {
            /// Auto-generated Repr instance implementation.
            #[no_mangle]
            pub unsafe extern "C" fn #ffi_name(
                obj: *mut #leo3_crate::ffi::lean_object,
            ) -> *mut #leo3_crate::ffi::lean_object {
                #leo3_crate::__private::ffi_panic_boundary(|| #try_name(obj))
            }

            #[doc(hidden)]
            #[allow(non_snake_case)]
            pub(crate) unsafe fn #try_name(
                obj: *mut #leo3_crate::ffi::lean_object,
            ) -> #leo3_crate::LeanResult<*mut #leo3_crate::ffi::lean_object> {
                let lean = #leo3_crate::Lean::assume_initialized();

                // Extern entry points receive borrowed arguments; balance the
                // LeanBound drop below with an inc (net refcount change zero).
                #leo3_crate::ffi::lean_inc(obj);

                let obj_bound: #leo3_crate::LeanBound<'_, #leo3_crate::external::LeanExternalType<#struct_name>> =
                    #leo3_crate::LeanBound::from_owned_ptr(lean, obj);

                let repr_str: String = obj_bound.get_ref().#method_ident();
                let s = #leo3_crate::types::LeanString::mk(lean, &repr_str)
                    .map_err(|e| #leo3_crate::LeanError::Conversion(format!(
                        "Failed to convert Rust repr result to Lean: {}",
                        e
                    )))?;
                Ok(s.into_ptr())
            }
        }),

        TypeClass::ToString => Ok(quote! {
            /// Auto-generated ToString instance implementation.
            #[no_mangle]
            pub unsafe extern "C" fn #ffi_name(
                obj: *mut #leo3_crate::ffi::lean_object,
            ) -> *mut #leo3_crate::ffi::lean_object {
                #leo3_crate::__private::ffi_panic_boundary(|| #try_name(obj))
            }

            #[doc(hidden)]
            #[allow(non_snake_case)]
            pub(crate) unsafe fn #try_name(
                obj: *mut #leo3_crate::ffi::lean_object,
            ) -> #leo3_crate::LeanResult<*mut #leo3_crate::ffi::lean_object> {
                let lean = #leo3_crate::Lean::assume_initialized();

                // Extern entry points receive borrowed arguments; balance the
                // LeanBound drop below with an inc (net refcount change zero).
                #leo3_crate::ffi::lean_inc(obj);

                let obj_bound: #leo3_crate::LeanBound<'_, #leo3_crate::external::LeanExternalType<#struct_name>> =
                    #leo3_crate::LeanBound::from_owned_ptr(lean, obj);

                let s: String = obj_bound.get_ref().#method_ident();
                let lean_str = #leo3_crate::types::LeanString::mk(lean, &s)
                    .map_err(|e| #leo3_crate::LeanError::Conversion(format!(
                        "Failed to convert Rust string result to Lean: {}",
                        e
                    )))?;
                Ok(lean_str.into_ptr())
            }
        }),

        TypeClass::Ord => Ok(quote! {
            /// Auto-generated Ord instance implementation.
            #[no_mangle]
            pub unsafe extern "C" fn #ffi_name(
                a: *mut #leo3_crate::ffi::lean_object,
                b: *mut #leo3_crate::ffi::lean_object,
            ) -> *mut #leo3_crate::ffi::lean_object {
                #leo3_crate::__private::ffi_panic_boundary(|| #try_name(a, b))
            }

            #[doc(hidden)]
            #[allow(non_snake_case)]
            pub(crate) unsafe fn #try_name(
                a: *mut #leo3_crate::ffi::lean_object,
                b: *mut #leo3_crate::ffi::lean_object,
            ) -> #leo3_crate::LeanResult<*mut #leo3_crate::ffi::lean_object> {
                let lean = #leo3_crate::Lean::assume_initialized();

                // Extern entry points receive borrowed arguments (the caller
                // keeps its reference); balance the LeanBound drops below
                // with an inc so the net refcount change is zero.
                #leo3_crate::ffi::lean_inc(a);
                #leo3_crate::ffi::lean_inc(b);

                let a_bound: #leo3_crate::LeanBound<'_, #leo3_crate::external::LeanExternalType<#struct_name>> =
                    #leo3_crate::LeanBound::from_owned_ptr(lean, a);
                let b_bound: #leo3_crate::LeanBound<'_, #leo3_crate::external::LeanExternalType<#struct_name>> =
                    #leo3_crate::LeanBound::from_owned_ptr(lean, b);

                let ordering = a_bound.get_ref().#method_ident(b_bound.get_ref());
                let ord_val = match ordering {
                    std::cmp::Ordering::Less => 0usize,
                    std::cmp::Ordering::Equal => 1usize,
                    std::cmp::Ordering::Greater => 2usize,
                };

                Ok(#leo3_crate::ffi::inline::lean_box(ord_val))
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_lean_instance_wrappers_use_boundary_helpers() {
        let mut item: syn::ItemImpl = syn::parse_quote! {
            impl Demo {
                fn repr(&self) -> String {
                    "demo".to_string()
                }
            }
        };

        let repr_tokens = build_lean_instance(
            &mut item,
            LeanInstanceOptions {
                typeclasses: vec![TypeClass::Repr],
                krate: None,
            },
        )
        .expect("lean_instance repr expansion should succeed");
        let rendered_repr = repr_tokens.to_string();
        assert!(rendered_repr.contains("__private :: ffi_panic_boundary"));
        assert!(rendered_repr.contains("Failed to convert Rust repr result to Lean"));
        assert!(!rendered_repr.contains("empty string"));
        assert!(!rendered_repr.contains(".expect("));
        assert!(!rendered_repr.contains(". expect ("));

        let mut hash_item: syn::ItemImpl = syn::parse_quote! {
            impl Demo {
                fn hash(&self) -> u64 {
                    7
                }
            }
        };

        let hash_tokens = build_lean_instance(
            &mut hash_item,
            LeanInstanceOptions {
                typeclasses: vec![TypeClass::Hashable],
                krate: None,
            },
        )
        .expect("lean_instance hash expansion should succeed");
        let rendered_hash = hash_tokens.to_string();
        assert!(rendered_hash.contains("ffi_panic_boundary"));
        assert!(rendered_hash.contains("lean_ctor_set_uint64"));
        assert!(!rendered_hash.contains(".expect("));
        assert!(!rendered_hash.contains(". expect ("));

        // A combined Hashable + BEq + Ord expansion must derive the container
        // key traits from the generated FFI functions.
        let mut key_item: syn::ItemImpl = syn::parse_quote! {
            impl Demo {
                fn hash(&self) -> u64 {
                    7
                }
                fn beq(&self, other: &Self) -> bool {
                    other == self
                }
                fn compare(&self, other: &Self) -> std::cmp::Ordering {
                    self.cmp(other)
                }
            }
        };
        let key_tokens = build_lean_instance(
            &mut key_item,
            LeanInstanceOptions {
                typeclasses: vec![TypeClass::Hashable, TypeClass::BEq, TypeClass::Ord],
                krate: None,
            },
        )
        .expect("lean_instance key expansion should succeed");
        let rendered_key = key_tokens.to_string();
        assert!(rendered_key.contains("ExternalHashKey"));
        assert!(rendered_key.contains("ExternalOrdKey"));
        assert!(rendered_key.contains("__lean_hash_Demo"));
        assert!(rendered_key.contains("__lean_beq_Demo"));
        assert!(rendered_key.contains("__lean_compare_Demo"));
    }

    #[test]
    fn beq_wrapper_generation() {
        let mut item: syn::ItemImpl = syn::parse_quote! {
            impl Demo {
                fn beq(&self, other: &Self) -> bool {
                    self == other
                }
            }
        };
        let tokens = build_lean_instance(
            &mut item,
            LeanInstanceOptions {
                typeclasses: vec![TypeClass::BEq],
                krate: None,
            },
        )
        .expect("beq expansion should succeed");
        let rendered = tokens.to_string();
        assert!(rendered.contains("__lean_beq_Demo"));
        assert!(rendered.contains("__leo3_try___lean_beq_Demo"));
        assert!(rendered.contains("no_mangle"));
        assert!(rendered.contains("lean_inc"));
        assert!(rendered.contains("lean_box (result as usize)"));
        // A lone BEq must not derive container key traits.
        assert!(!rendered.contains("ExternalHashKey"));
        assert!(!rendered.contains("ExternalOrdKey"));
    }

    #[test]
    fn tostring_and_ord_wrappers() {
        let mut item: syn::ItemImpl = syn::parse_quote! {
            impl Demo {
                fn to_string(&self) -> String {
                    "demo".to_string()
                }
            }
        };
        let tokens = build_lean_instance(
            &mut item,
            LeanInstanceOptions {
                typeclasses: vec![TypeClass::ToString],
                krate: None,
            },
        )
        .expect("tostring expansion should succeed");
        let rendered = tokens.to_string();
        assert!(rendered.contains("__lean_to_string_Demo"));
        assert!(rendered.contains("LeanString :: mk"));
        assert!(rendered.contains("Failed to convert Rust string result to Lean"));

        let mut item: syn::ItemImpl = syn::parse_quote! {
            impl Demo {
                fn compare(&self, other: &Self) -> std::cmp::Ordering {
                    self.cmp(other)
                }
            }
        };
        let tokens = build_lean_instance(
            &mut item,
            LeanInstanceOptions {
                typeclasses: vec![TypeClass::Ord],
                krate: None,
            },
        )
        .expect("ord expansion should succeed");
        let rendered = tokens.to_string();
        assert!(rendered.contains("__lean_compare_Demo"));
        assert!(rendered.contains("Ordering :: Less"));
        assert!(rendered.contains("Ordering :: Equal"));
        assert!(rendered.contains("Ordering :: Greater"));
        // Ord alone derives the RBMap key trait but not the hash key trait.
        assert!(rendered.contains("ExternalOrdKey"));
        assert!(!rendered.contains("ExternalHashKey"));
    }

    #[test]
    fn hashable_beq_pair_derives_hash_key_only() {
        let mut item: syn::ItemImpl = syn::parse_quote! {
            impl Demo {
                fn hash(&self) -> u64 { 1 }
                fn beq(&self, other: &Self) -> bool { self == other }
            }
        };
        let tokens = build_lean_instance(
            &mut item,
            LeanInstanceOptions {
                typeclasses: vec![TypeClass::Hashable, TypeClass::BEq],
                krate: None,
            },
        )
        .expect("hash+beq expansion should succeed");
        let rendered = tokens.to_string();
        assert!(rendered.contains("ExternalHashKey"));
        assert!(rendered.contains("decidable_eq_fn"));
        assert!(rendered.contains("hash_fn"));
        assert!(!rendered.contains("ExternalOrdKey"));
    }

    #[test]
    fn missing_required_method_errors() {
        let mut item: syn::ItemImpl = syn::parse_quote! {
            impl Demo {
                fn unrelated(&self) {}
            }
        };
        let err = build_lean_instance(
            &mut item,
            LeanInstanceOptions {
                typeclasses: vec![TypeClass::BEq],
                krate: None,
            },
        )
        .expect_err("missing beq must fail");
        assert!(err
            .to_string()
            .contains("Method 'beq' not found in impl block"));
    }

    #[test]
    fn typeclass_metadata_mappings() {
        let ident = |name: &str| syn::Ident::new(name, proc_macro2::Span::call_site());
        assert_eq!(TypeClass::from_ident(&ident("BEq")), Some(TypeClass::BEq));
        assert_eq!(
            TypeClass::from_ident(&ident("Hashable")),
            Some(TypeClass::Hashable)
        );
        assert_eq!(TypeClass::from_ident(&ident("Repr")), Some(TypeClass::Repr));
        assert_eq!(
            TypeClass::from_ident(&ident("ToString")),
            Some(TypeClass::ToString)
        );
        assert_eq!(TypeClass::from_ident(&ident("Ord")), Some(TypeClass::Ord));
        assert_eq!(TypeClass::from_ident(&ident("Foo")), None);
        assert_eq!(TypeClass::BEq.method_name(), "beq");
        assert_eq!(TypeClass::Hashable.method_name(), "hash");
        assert_eq!(TypeClass::Repr.method_name(), "repr");
        assert_eq!(TypeClass::ToString.method_name(), "to_string");
        assert_eq!(TypeClass::Ord.method_name(), "compare");
        assert_eq!(TypeClass::BEq.ffi_prefix(), "__lean_beq");
        assert_eq!(TypeClass::Hashable.ffi_prefix(), "__lean_hash");
        assert_eq!(TypeClass::Repr.ffi_prefix(), "__lean_repr");
        assert_eq!(TypeClass::ToString.ffi_prefix(), "__lean_to_string");
        assert_eq!(TypeClass::Ord.ffi_prefix(), "__lean_compare");
    }

    #[test]
    fn options_parse_accumulates_typeclasses() {
        let opts: LeanInstanceOptions = syn::parse_str("BEq, Hashable").expect("parse options");
        assert_eq!(opts.typeclasses, vec![TypeClass::BEq, TypeClass::Hashable]);
        assert!(opts.krate.is_none());

        let opts: LeanInstanceOptions = syn::parse_str("BEq,").expect("parse trailing comma");
        assert_eq!(opts.typeclasses, vec![TypeClass::BEq]);
    }

    #[test]
    fn unsupported_typeclass_and_empty_options_error() {
        let err = syn::parse_str::<LeanInstanceOptions>("Foo")
            .err()
            .expect("unknown class must fail");
        assert!(err.to_string().contains("Unsupported type class 'Foo'"));
        assert!(err
            .to_string()
            .contains("Supported: BEq, Hashable, Repr, ToString, Ord"));

        assert!(syn::parse_str::<LeanInstanceOptions>("").is_err());
    }

    #[test]
    fn krate_override_replaces_leo3_path() {
        let mut item: syn::ItemImpl = syn::parse_quote! {
            impl Demo {
                fn hash(&self) -> u64 { 1 }
            }
        };
        let tokens = build_lean_instance(
            &mut item,
            LeanInstanceOptions {
                typeclasses: vec![TypeClass::Hashable],
                krate: Some(syn::parse_quote!(my_leo3)),
            },
        )
        .expect("hash expansion should succeed");
        let rendered = tokens.to_string();
        assert!(rendered.contains("my_leo3 :: ffi :: lean_alloc_ctor"));
        assert!(rendered.contains("my_leo3 :: __private :: ffi_panic_boundary"));
        assert!(!rendered.contains(":: leo3"));
    }
}
