//! Tests for the #[leanmodule] macro
//!
//! This macro generates module initialization functions for Lean4.

#![cfg(all(feature = "macros", feature = "runtime-tests"))]

use leo3::prelude::*;

// Define a test module with #[leanmodule]
#[leanmodule(name = "TestModule")]
mod test_module {
    #[allow(dead_code)]
    pub fn add(a: u64, b: u64) -> u64 {
        a + b
    }

    #[allow(dead_code)]
    pub fn multiply(a: u64, b: u64) -> u64 {
        a * b
    }
}

// Test module with default name (uses module identifier)
#[leanmodule]
mod another_module {
    #[allow(dead_code)]
    pub fn greet() -> &'static str {
        "Hello!"
    }
}

// Test module with bare identifier name
#[leanmodule(MathModule)]
mod math_module {
    #[allow(dead_code)]
    pub fn square(x: i32) -> i32 {
        x * x
    }
}

#[leanmodule(name = "CratePathModule", crate = leo3)]
mod crate_path_module {
    #[allow(dead_code)]
    pub fn negate(x: i32) -> i32 {
        -x
    }
}

#[test]
fn test_module_initialization_function_exists() {
    // The #[leanmodule] macro generates:
    // - initialize_TestModule
    // - initialize_another_module
    // - initialize_MathModule
    // - initialize_CratePathModule

    // If this compiles, the functions exist (they're #[no_mangle] extern "C")
}

#[test]
fn test_module_functions_accessible() {
    // Test that the module functions are still accessible
    assert_eq!(test_module::add(2, 3), 5);
    assert_eq!(test_module::multiply(4, 5), 20);
    assert_eq!(another_module::greet(), "Hello!");
    assert_eq!(math_module::square(7), 49);
    assert_eq!(crate_path_module::negate(9), -9);
}

#[test]
fn test_module_initialization_safety() {
    leo3::prepare_freethreaded_lean();

    // The initialization functions can be called from Lean
    // Here we just verify the runtime is ready
    leo3::with_lean(|_lean| {
        // Module initialization would happen on Lean side
        Ok::<_, LeanError>(())
    })
    .unwrap();
}

#[test]
fn test_module_initialization_does_not_reinitialize_runtime() {
    let handle = std::thread::spawn(|| {
        assert!(!leo3::sync::thread_is_lean_initialized());

        unsafe {
            let result = initialize_TestModule(0, std::ptr::null_mut());
            let result_ptr = result as *mut leo3::ffi::lean_object;
            leo3::ffi::lean_dec(result_ptr);
        }

        assert!(!leo3::sync::thread_is_lean_initialized());

        leo3::with_lean(|lean| {
            let n = LeanNat::from_usize(lean, 9)?;
            assert_eq!(LeanNat::to_usize(&n)?, 9);
            Ok::<_, LeanError>(())
        })
        .unwrap();
    });

    handle.join().unwrap();
}

// Test nested content in modules
#[leanmodule(name = "NestedModule")]
mod nested_module {
    pub mod inner {
        #[allow(dead_code)]
        pub fn inner_fn() -> i32 {
            42
        }
    }

    #[allow(dead_code)]
    pub struct ModuleStruct {
        pub value: i32,
    }

    impl ModuleStruct {
        #[allow(dead_code)]
        pub fn new(v: i32) -> Self {
            ModuleStruct { value: v }
        }
    }
}

#[test]
fn test_nested_module_content() {
    assert_eq!(nested_module::inner::inner_fn(), 42);

    let s = nested_module::ModuleStruct::new(100);
    assert_eq!(s.value, 100);
}

// Verify the generated initialization function signature
// The function should be: extern "C" fn(u8, *mut c_void) -> *mut c_void
#[test]
fn test_init_function_signature() {
    // The macro generates functions like:
    // #[no_mangle]
    // pub unsafe extern "C" fn initialize_TestModule(
    //     builtin: u8,
    //     _world: *mut std::ffi::c_void,
    // ) -> *mut std::ffi::c_void

    // This test verifies the types are correct by checking function pointer compatibility
    type InitFn = unsafe extern "C" fn(u8, *mut std::ffi::c_void) -> *mut std::ffi::c_void;

    // If this compiles, the function signature matches
    extern "C" {
        fn initialize_TestModule(
            builtin: u8,
            _world: *mut std::ffi::c_void,
        ) -> *mut std::ffi::c_void;
    }

    let _fn_ptr: InitFn = initialize_TestModule;
}

// Test that module can contain leanfn-annotated functions
// (In real usage, these would have #[leanfn] attribute)
#[leanmodule(name = "FunctionModule")]
#[allow(unused_imports)]
mod function_module {
    use leo3::prelude::leanfn;

    #[leanfn(name = "function_module_add")]
    pub fn exported_add(a: u64, b: u64) -> u64 {
        a + b
    }

    #[leanfn(name = "function_module_sub")]
    pub fn exported_sub(a: u64, b: u64) -> u64 {
        a.saturating_sub(b)
    }
}

#[test]
fn test_function_module() {
    assert_eq!(function_module::exported_add(10, 5), 15);
    assert_eq!(function_module::exported_sub(10, 5), 5);
    assert_eq!(function_module::exported_sub(5, 10), 0); // saturating
}

#[test]
fn test_module_metadata_tracks_leanfn_exports() {
    let metadata = function_module::__leo3_module_metadata();
    let export_names: Vec<_> = metadata.exports.iter().map(|item| item.name).collect();

    assert_eq!(metadata.schema_version, leo3::LEO3_BINDING_SCHEMA_VERSION);
    assert_eq!(metadata.name, "FunctionModule");
    assert_eq!(
        export_names,
        vec!["function_module_add", "function_module_sub"]
    );
    assert_eq!(metadata.exports[0].rust_name, "exported_add");
    assert_eq!(metadata.exports[0].ffi_symbol, "function_module_add");
    assert_eq!(metadata.exports[0].params.len(), 2);
    assert_eq!(metadata.exports[0].params[0].ty.lean, Some("UInt64"));
    assert_eq!(metadata.exports[0].return_type.lean, Some("UInt64"));
    assert_eq!(
        metadata.exports[0].receiver,
        leo3::LeanBindingReceiver::None
    );
}

#[leanmodule(name = "SubmoduleHost")]
#[allow(unused_imports)]
mod submodule_host {
    use leo3::prelude::leanfn;

    #[leanfn(name = "host_top")]
    #[allow(dead_code)]
    pub fn top_fn(x: u64) -> u64 {
        x + 1
    }

    pub mod inner {
        use leo3::prelude::leanfn;

        #[leanfn(name = "inner_fn")]
        #[allow(dead_code)]
        pub fn inner_export(a: u64) -> u64 {
            a * 2
        }
    }

    pub mod deep {
        pub mod nested {
            use leo3::prelude::leanfn;

            #[leanfn(name = "deep_fn")]
            #[allow(dead_code)]
            pub fn deep_export(a: u64) -> u64 {
                a * 3
            }
        }
    }
}

#[test]
fn test_submodule_discovery() {
    let metadata = submodule_host::__leo3_module_metadata();
    assert_eq!(metadata.name, "SubmoduleHost");
    assert_eq!(metadata.exports.len(), 1);
    assert_eq!(metadata.exports[0].name, "host_top");

    assert_eq!(metadata.submodules.len(), 2);

    let inner = metadata
        .submodules
        .iter()
        .find(|s| s.path == "inner")
        .expect("inner submodule");
    assert_eq!(inner.exports.len(), 1);
    assert_eq!(inner.exports[0].name, "inner_fn");
    assert_eq!(inner.exports[0].rust_name, "inner_export");

    let deep = metadata
        .submodules
        .iter()
        .find(|s| s.path == "deep.nested")
        .expect("deep.nested submodule");
    assert_eq!(deep.exports.len(), 1);
    assert_eq!(deep.exports[0].name, "deep_fn");
    assert_eq!(deep.exports[0].rust_name, "deep_export");
}

#[leanmodule(name = "ExplicitExports", exports = ["sel_add"])]
#[allow(unused_imports)]
mod explicit_exports_module {
    use leo3::prelude::leanfn;

    #[leanfn(name = "sel_add")]
    #[allow(dead_code)]
    pub fn selected_add(a: u64, b: u64) -> u64 {
        a + b
    }

    #[leanfn(name = "sel_mul")]
    #[allow(dead_code)]
    pub fn unselected_mul(a: u64, b: u64) -> u64 {
        a * b
    }
}

#[test]
fn test_explicit_exports_filter() {
    let metadata = explicit_exports_module::__leo3_module_metadata();
    assert_eq!(metadata.name, "ExplicitExports");
    assert_eq!(metadata.exports.len(), 1);
    assert_eq!(metadata.exports[0].name, "sel_add");
    assert_eq!(metadata.exports[0].rust_name, "selected_add");

    assert_eq!(explicit_exports_module::selected_add(2, 3), 5);
    assert_eq!(explicit_exports_module::unselected_mul(2, 3), 6);
}

#[leanmodule(name = "Foo.Bar.baz")]
#[allow(unused_imports)]
mod dotted_module {
    use leo3::prelude::leanfn;

    #[leanfn(name = "dotted_fn")]
    #[allow(dead_code)]
    pub fn dotted_export(x: u64) -> u64 {
        x
    }
}

#[test]
fn test_dotted_module_name() {
    let metadata = dotted_module::__leo3_module_metadata();
    assert_eq!(metadata.name, "Foo.Bar.baz");
    assert_eq!(metadata.exports.len(), 1);
    assert_eq!(metadata.exports[0].name, "dotted_fn");
}

#[test]
fn test_schema_version_is_v2() {
    assert_eq!(leo3::LEO3_BINDING_SCHEMA_VERSION, 2);

    let metadata = function_module::__leo3_module_metadata();
    assert_eq!(metadata.schema_version, 2);
}
