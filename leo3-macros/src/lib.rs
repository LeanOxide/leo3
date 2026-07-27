#![cfg_attr(docsrs, feature(doc_cfg))]
//! Procedural macros for Leo3 (Rust-Lean4 bindings).
//!
//! This crate provides the proc macro attributes for Leo3. The actual implementation
//! is in `leo3-macros-backend`.

use leo3_binding_ir::{
    collect_module_exports, collect_submodule_exports, filter_exports, module_binding_to_json,
    quote_runtime_module_metadata, ModuleBinding,
};
use leo3_macros_backend::{build_lean_function, LeanFunctionOptions};
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{parse::Parse, parse_macro_input, punctuated::Punctuated, Token};

/// A proc macro used to expose Rust functions to Lean4.
///
/// # Example
///
/// ```rust,no_run
/// mod doctest {
///     use leo3_macros::leanfn;
///
///     #[leanfn]
///     fn add(a: u64, b: u64) -> u64 {
///         a + b
///     }
/// }
/// ```
///
/// Functions annotated with `#[leanfn]` can also be annotated with the following options:
///
/// | Annotation | Description |
/// | :- | :- |
/// | `#[leo3(name = "...")]` | Defines the name of the function in Lean4. |
/// | `#[leo3(crate = "leo3")]` | Defines the path to Leo3 to use in generated code. |
///
/// # Name Override
///
/// By default, the Lean4 function name will match the Rust function name.
/// Use `#[leo3(name = "my_name")]` to override it.
#[proc_macro_attribute]
pub fn leanfn(attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut ast = parse_macro_input!(input as syn::ItemFn);
    let options = parse_macro_input!(attr as LeanFunctionOptions);

    let expanded = build_lean_function(&mut ast, options).unwrap_or_compile_error();

    expanded.into()
}

/// Derive macro for automatic `IntoLean` trait implementation.
///
/// Automatically generates an implementation of the `IntoLean` trait for converting
/// Rust types into Lean constructors.
///
/// # Example
///
/// ```rust,no_run
/// use leo3_macros::IntoLean;
///
/// #[derive(IntoLean)]
/// struct Point {
///     x: u64,
///     y: u64,
/// }
/// ```
///
/// This will generate an `IntoLean` implementation that converts the struct into
/// a Lean constructor with tag 0 and two fields.
///
/// # Supported Types
///
/// - Structs with named fields
/// - Structs with unnamed fields (tuple structs)
/// - Enums with unit variants
/// - Enums with tuple/struct variants
/// - Generic types (with appropriate trait bounds)
///
/// # Requirements
///
/// All field types must implement `IntoLean<'l>`.
///
/// # Attributes
///
/// The derive supports the following attributes:
/// - `#[lean(transparent)]` - For newtype wrappers, skips the constructor layer
/// - `#[lean(skip)]` - Excludes a field from conversion
/// - `#[lean(with = "path")]` - Uses a custom conversion function
/// - `#[lean(rename = "name")]` - Custom field name for error messages
/// - `#[lean(tag = n)]` - Explicit constructor tag for enum variants
#[proc_macro_derive(IntoLean, attributes(lean))]
pub fn derive_into_lean(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    leo3_macros_backend::derive::expand_into_lean(ast)
        .unwrap_or_compile_error()
        .into()
}

/// Derive macro for automatic `FromLean` trait implementation.
///
/// Automatically generates an implementation of the `FromLean` trait for extracting
/// Rust types from Lean constructors.
///
/// # Example
///
/// ```rust,no_run
/// use leo3_macros::FromLean;
///
/// #[derive(FromLean)]
/// struct Point {
///     x: u64,
///     y: u64,
/// }
/// ```
///
/// This will generate a `FromLean` implementation that extracts the struct from
/// a Lean constructor with tag 0 and two fields.
///
/// # Supported Types
///
/// - Structs with named fields
/// - Structs with unnamed fields (tuple structs)
/// - Enums with unit variants
/// - Enums with tuple/struct variants
/// - Generic types (with appropriate trait bounds)
///
/// # Requirements
///
/// All field types must implement `FromLean<'l>`.
///
/// # Attributes
///
/// The derive supports the following attributes:
/// - `#[lean(transparent)]` - For newtype wrappers, extracts directly from inner type
/// - `#[lean(skip)]` - Excludes a field from extraction, uses Default::default()
/// - `#[lean(default)]` - Uses Default::default() if extraction fails
/// - `#[lean(with = "path")]` - Uses a custom extraction function
/// - `#[lean(rename = "name")]` - Custom field name for error messages
/// - `#[lean(tag = n)]` - Explicit constructor tag for enum variants
#[proc_macro_derive(FromLean, attributes(lean))]
pub fn derive_from_lean(input: TokenStream) -> TokenStream {
    let ast = parse_macro_input!(input as syn::DeriveInput);
    leo3_macros_backend::derive::expand_from_lean(ast)
        .unwrap_or_compile_error()
        .into()
}

/// A proc macro used to expose Rust structs as Lean4 classes.
///
/// # Example
///
/// ```rust,no_run
/// mod doctest {
///     use leo3_macros::leanclass;
///
///     #[derive(Clone)]
///     #[leanclass]
///     struct Counter {
///         value: i32,
///     }
///
///     #[leanclass]
///     impl Counter {
///         fn new() -> Self {
///             Counter { value: 0 }
///         }
///
///         fn increment(&mut self) {
///             self.value += 1;
///         }
///
///         fn get(&self) -> i32 {
///             self.value
///         }
///     }
/// }
/// ```
///
/// This macro generates:
/// - An `ExternalClass` implementation for the struct
/// - FFI wrappers for each method
/// - Metadata for Lean code generation
#[proc_macro_attribute]
pub fn leanclass(attr: TokenStream, input: TokenStream) -> TokenStream {
    use leo3_macros_backend::LeanClassOptions;

    let options = parse_macro_input!(attr as LeanClassOptions);

    // Try to parse as struct first, then as impl
    if let Ok(mut item_struct) = syn::parse::<syn::ItemStruct>(input.clone()) {
        let expanded = leo3_macros_backend::build_lean_class_struct(&mut item_struct, options)
            .unwrap_or_compile_error();
        return expanded.into();
    }

    if let Ok(mut item_impl) = syn::parse::<syn::ItemImpl>(input.clone()) {
        let expanded = leo3_macros_backend::build_lean_class_impl(&mut item_impl, options)
            .unwrap_or_compile_error();
        return expanded.into();
    }

    // If neither struct nor impl, return error
    quote!(
        compile_error!("#[leanclass] can only be applied to structs or impl blocks");
    )
    .into()
}

/// A proc macro used to create Lean4 modules.
///
/// # Example
///
/// ```rust,no_run
/// mod doctest {
///     use leo3_macros::{leanfn, leanmodule};
///
///     #[leanmodule(name = "MyRustLib")]
///     mod my_module {
///         #[leo3_macros::leanfn()]
///         pub fn add(a: u64, b: u64) -> u64 {
///             a + b
///         }
///     }
/// }
/// ```
///
/// This generates a module initialization function `initialize_MyRustLib` that
/// can be called from Lean4 to initialize the module. The generated entry point
/// follows Lean's plugin contract: the host runtime is responsible for Lean
/// initialization before the module initializer is invoked.
///
/// Supported options:
///
/// - `#[leanmodule]` uses the Rust module identifier
/// - `#[leanmodule(MyName)]` uses a bare identifier as the Lean module name
/// - `#[leanmodule(name = "MyName")]` uses an explicit string name
/// - `#[leanmodule(name = "Foo.Bar")]` uses a dotted nested module path
/// - `#[leanmodule(crate = my::leo3)]` changes the crate path used in generated code
/// - `#[leanmodule(exports = ["fn_a", "fn_b"])]` restricts the implicit export set
#[proc_macro_attribute]
pub fn leanmodule(attr: TokenStream, input: TokenStream) -> TokenStream {
    let mut item_mod = parse_macro_input!(input as syn::ItemMod);
    let options = parse_macro_input!(attr as LeanModuleOptions);

    let module_name = options.name.unwrap_or_else(|| item_mod.ident.to_string());
    let leo3_crate = options
        .krate
        .map(|path| quote! { #path })
        .unwrap_or_else(|| quote! { ::leo3 });

    let init_fn_name = syn::Ident::new(
        &format!("initialize_{}", module_name.replace('.', "_")),
        proc_macro2::Span::call_site(),
    );

    let module_binding = match item_mod.content.as_ref() {
        Some((_, items)) => {
            let mut exports = match collect_module_exports(items) {
                Ok(exports) => exports,
                Err(error) => return error.into_compile_error().into(),
            };

            if let Some(ref allowed) = options.exports {
                exports = match filter_exports(exports, allowed) {
                    Ok(filtered) => filtered,
                    Err(error) => return error.into_compile_error().into(),
                };
            }

            let submodules = match collect_submodule_exports(items, "") {
                Ok(submodules) => submodules,
                Err(error) => return error.into_compile_error().into(),
            };

            ModuleBinding {
                name: module_name.clone(),
                exports,
                submodules,
            }
        }
        None => ModuleBinding {
            name: module_name.clone(),
            exports: Vec::new(),
            submodules: Vec::new(),
        },
    };

    if let Some((_, items)) = &mut item_mod.content {
        let metadata = quote_runtime_module_metadata(&module_binding, &leo3_crate);

        let metadata_item: syn::Item = syn::parse_quote! {
            #[doc(hidden)]
            pub fn __leo3_module_metadata() -> #leo3_crate::LeanModuleMetadata {
                #metadata
            }
        };
        items.push(metadata_item);
    }

    let json_str = module_binding_to_json(&module_binding);
    let json_symbol_name = syn::Ident::new(
        &format!("__leo3_module_metadata_json_{}", module_name.replace('.', "_")),
        proc_macro2::Span::call_site(),
    );
    let json_bytes = json_str.as_bytes();
    let json_len = json_bytes.len() + 1;
    let byte_literals: Vec<proc_macro2::Literal> = json_bytes
        .iter()
        .map(|&b| proc_macro2::Literal::u8_suffixed(b))
        .collect();

    let expanded = quote! {
        #item_mod

        /// Module initialization function for Lean4.
        ///
        /// This function is called by Lean when loading the module.
        #[no_mangle]
        pub unsafe extern "C" fn #init_fn_name(
            _builtin: u8,
            _world: *mut ::std::ffi::c_void,
        ) -> *mut ::std::ffi::c_void {
            // Return IO.ok ()
            let unit = #leo3_crate::ffi::lean_mk_unit();
            let io_ok = #leo3_crate::ffi::io::lean_io_result_mk_ok(unit);
            io_ok as *mut ::std::ffi::c_void
        }

        #[doc(hidden)]
        #[no_mangle]
        #[used]
        pub static #json_symbol_name: [u8; #json_len] = [#(#byte_literals),*, 0u8];
    };

    expanded.into()
}

/// A proc macro to implement Lean type classes for Rust types.
///
/// # Supported Type Classes
///
/// - `BEq` - Boolean equality (requires `fn beq(&self, other: &Self) -> bool`)
/// - `Hashable` - Hashing (requires `fn hash(&self) -> u64`)
/// - `Repr` - Representation (requires `fn repr(&self) -> String`)
/// - `ToString` - String conversion (requires `fn to_string(&self) -> String`)
/// - `Ord` - Ordering (requires `fn compare(&self, other: &Self) -> Ordering`)
///
/// # Example
///
/// ```rust,no_run
/// mod doctest {
///     use leo3_macros::{lean_instance, leanclass};
///
///     #[derive(Clone)]
///     #[leanclass]
///     struct Point { x: i32, y: i32 }
///
///     #[lean_instance(BEq)]
///     impl Point {
///         fn beq(&self, other: &Self) -> bool {
///             self.x == other.x && self.y == other.y
///         }
///     }
///
///     #[lean_instance(Hashable)]
///     impl Point {
///         fn hash(&self) -> u64 {
///             (self.x as u64) ^ (self.y as u64).wrapping_shl(32)
///         }
///     }
/// }
/// ```
///
/// This generates FFI functions that can be used as type class instances in Lean4.
#[proc_macro_attribute]
pub fn lean_instance(attr: TokenStream, input: TokenStream) -> TokenStream {
    use leo3_macros_backend::LeanInstanceOptions;

    let mut item_impl = parse_macro_input!(input as syn::ItemImpl);
    let options = parse_macro_input!(attr as LeanInstanceOptions);

    let expanded =
        leo3_macros_backend::build_lean_instance(&mut item_impl, options).unwrap_or_compile_error();

    expanded.into()
}

trait UnwrapOrCompileError {
    fn unwrap_or_compile_error(self) -> TokenStream2;
}

impl UnwrapOrCompileError for syn::Result<TokenStream2> {
    fn unwrap_or_compile_error(self) -> TokenStream2 {
        self.unwrap_or_else(|e| e.into_compile_error())
    }
}

#[derive(Default)]
struct LeanModuleOptions {
    name: Option<String>,
    krate: Option<syn::Path>,
    exports: Option<Vec<String>>,
}

impl Parse for LeanModuleOptions {
    fn parse(input: syn::parse::ParseStream) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Self::default());
        }

        let metas: Punctuated<syn::Meta, Token![,]> =
            input.parse_terminated(syn::Meta::parse, Token![,])?;
        let mut options = LeanModuleOptions::default();

        for meta in metas {
            match meta {
                syn::Meta::Path(path) => {
                    if options.name.is_some() {
                        return Err(syn::Error::new_spanned(
                            path,
                            "module name was already specified",
                        ));
                    }
                    if path.segments.len() != 1 {
                        return Err(syn::Error::new_spanned(
                            path,
                            "bare #[leanmodule(...)] names must be a single identifier",
                        ));
                    }
                    options.name = path.get_ident().map(|ident| ident.to_string());
                }
                syn::Meta::NameValue(nv) if nv.path.is_ident("name") => {
                    let syn::Expr::Lit(syn::ExprLit {
                        lit: syn::Lit::Str(s),
                        ..
                    }) = nv.value
                    else {
                        return Err(syn::Error::new_spanned(
                            nv,
                            "`name` must be a string literal",
                        ));
                    };
                    options.name = Some(s.value());
                }
                syn::Meta::NameValue(nv) if nv.path.is_ident("crate") => {
                    let syn::Expr::Path(path) = nv.value else {
                        return Err(syn::Error::new_spanned(
                            nv,
                            "`crate` must be a Rust path",
                        ));
                    };
                    options.krate = Some(path.path);
                }
                syn::Meta::NameValue(nv) if nv.path.is_ident("exports") => {
                    let syn::Expr::Array(array) = nv.value else {
                        return Err(syn::Error::new_spanned(
                            nv,
                            "`exports` must be an array of string literals, e.g. exports = [\"foo\", \"bar\"]",
                        ));
                    };
                    let mut names = Vec::new();
                    for elem in &array.elems {
                        let syn::Expr::Lit(syn::ExprLit {
                            lit: syn::Lit::Str(s),
                            ..
                        }) = elem
                        else {
                            return Err(syn::Error::new_spanned(
                                elem,
                                "each `exports` entry must be a string literal",
                            ));
                        };
                        names.push(s.value());
                    }
                    options.exports = Some(names);
                }
                other => {
                    return Err(syn::Error::new_spanned(
                        other,
                        "expected #[leanmodule], #[leanmodule(Name)], #[leanmodule(name = \"...\")], #[leanmodule(crate = path)], or #[leanmodule(exports = [...])]",
                    ))
                }
            }
        }

        Ok(options)
    }
}

#[cfg(test)]
mod tests {
    use super::LeanModuleOptions;
    use quote::ToTokens;

    #[test]
    fn parse_empty_module_options() {
        let options: LeanModuleOptions = syn::parse_quote! {};
        assert!(options.name.is_none());
        assert!(options.krate.is_none());
        assert!(options.exports.is_none());
    }

    #[test]
    fn parse_bare_module_name() {
        let options: LeanModuleOptions = syn::parse_quote! { MyModule };
        assert_eq!(options.name.as_deref(), Some("MyModule"));
        assert!(options.krate.is_none());
        assert!(options.exports.is_none());
    }

    #[test]
    fn parse_named_module_options() {
        let options: LeanModuleOptions = syn::parse_quote! { name = "MyModule", crate = my::leo3 };
        assert_eq!(options.name.as_deref(), Some("MyModule"));
        assert_eq!(
            options
                .krate
                .as_ref()
                .unwrap()
                .to_token_stream()
                .to_string(),
            "my :: leo3"
        );
    }

    #[test]
    fn parse_exports_option() {
        let options: LeanModuleOptions =
            syn::parse_quote! { name = "MyModule", exports = ["foo", "bar"] };
        assert_eq!(options.name.as_deref(), Some("MyModule"));
        assert_eq!(
            options.exports.as_deref(),
            Some(&["foo".to_string(), "bar".to_string()][..])
        );
    }

    #[test]
    fn parse_dotted_module_name() {
        let options: LeanModuleOptions = syn::parse_quote! { name = "Foo.Bar.baz" };
        assert_eq!(options.name.as_deref(), Some("Foo.Bar.baz"));
    }
}
