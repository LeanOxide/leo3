//! Miscellaneous library-surface tests for Leo3.
//!
//! Covers the top-level entry points and metadata types in `leo3/src/lib.rs`
//! (`with_lean` / `test_with_lean` closure shapes, idempotent
//! `prepare_freethreaded_lean`, the binding-schema constant, the structured
//! binding metadata structs, and the `__private` panic-boundary helpers),
//! residual `LeanByteArray` paths, `io::console` stdout/stderr writes,
//! `LeanPromise` round-trips, and the Tokio bridge.
//!
//! Sections are feature-gated individually so the file compiles (and the
//! ungated tests run) under `--features "runtime-tests"` as well as the full
//! `runtime-tests,tokio,task` combo:
//!
//! - ungated: entry points, metadata, `__private` helpers, `LeanByteArray`
//! - `feature = "task"`: `LeanPromise`
//! - `feature = "io"`: `io::console`
//! - `feature = "tokio"`: `tokio_bridge`

#![cfg(feature = "runtime-tests")]

use leo3::prelude::*;

// ============================================================================
// lib.rs: runtime entry points
// ============================================================================

#[test]
fn test_prepare_freethreaded_lean_is_idempotent() {
    // Calling prepare twice must be harmless (the worker is booted once).
    leo3::prepare_freethreaded_lean();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let n = LeanNat::from_usize(lean, 3)?;
        assert_eq!(LeanNat::to_usize(&n)?, 3);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_with_lean_plain_value_closure() {
    // with_lean supports closures returning a plain (non-Result) value.
    leo3::prepare_freethreaded_lean();
    let value: usize = leo3::with_lean(|_lean| 42);
    assert_eq!(value, 42);

    // The token handed to the closure is usable inside a plain-value closure.
    let doubled: usize = leo3::with_lean(|lean| {
        let n = LeanNat::from_usize(lean, 21);
        match n {
            Ok(n) => match LeanNat::to_usize(&n) {
                Ok(v) => v * 2,
                Err(_) => 0,
            },
            Err(_) => 0,
        }
    });
    assert_eq!(doubled, 42);
}

#[test]
fn test_with_lean_lean_result_closure() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<usize> = leo3::with_lean(|lean| {
        let n = LeanNat::from_usize(lean, 7)?;
        LeanNat::to_usize(&n)
    });
    assert_eq!(result.unwrap(), 7);

    let err_result: LeanResult<()> = leo3::with_lean(|_lean| Err(LeanError::other("boom")));
    assert!(err_result.is_err());
}

#[test]
fn test_test_with_lean_plain_value_closure() {
    leo3::prepare_freethreaded_lean();

    let value: u32 = leo3::test_with_lean(|_lean| 99);
    assert_eq!(value, 99);
}

#[test]
fn test_test_with_lean_lean_result_closure() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let s = LeanString::mk(lean, "Hello!")?;
        assert!(!LeanString::cstr(&s)?.is_empty());
        Ok(())
    });
    assert!(result.is_ok());
}

// ============================================================================
// lib.rs: binding schema version and metadata types
// ============================================================================

#[test]
fn test_binding_schema_version_constant() {
    assert_eq!(leo3::LEO3_BINDING_SCHEMA_VERSION, 3);
}

#[test]
fn test_metadata_enums_all_variants() {
    use leo3::{
        LeanBindingKind, LeanBindingReceiver, LeanBindingSemantics, LeanPassingStyle, LeanTypeShape,
    };

    let passings = [LeanPassingStyle::Owned, LeanPassingStyle::Borrowed];
    assert_eq!(passings[0], passings[0]);
    assert_ne!(passings[0], passings[1]);
    assert_eq!(format!("{:?}", LeanPassingStyle::Owned), "Owned");
    assert_eq!(format!("{:?}", LeanPassingStyle::Borrowed), "Borrowed");

    let receivers = [
        LeanBindingReceiver::None,
        LeanBindingReceiver::Ref,
        LeanBindingReceiver::MutRef,
        LeanBindingReceiver::Owned,
    ];
    for (i, r) in receivers.iter().enumerate() {
        assert_eq!(receivers[i], *r);
        for (j, other) in receivers.iter().enumerate() {
            assert_eq!(receivers[i] == *other, i == j);
        }
    }
    assert_eq!(format!("{:?}", LeanBindingReceiver::MutRef), "MutRef");

    let semantics = [
        LeanBindingSemantics::Value,
        LeanBindingSemantics::MutatesSelf,
        LeanBindingSemantics::MutatesSelfWithValue,
    ];
    assert_ne!(semantics[0], semantics[1]);
    assert_ne!(semantics[0], semantics[2]);
    assert_ne!(semantics[1], semantics[2]);
    assert_eq!(
        format!("{:?}", LeanBindingSemantics::MutatesSelfWithValue),
        "MutatesSelfWithValue"
    );

    let kinds = [
        LeanBindingKind::Method,
        LeanBindingKind::Getter,
        LeanBindingKind::Setter,
    ];
    assert_eq!(kinds[0], LeanBindingKind::Method);
    assert_ne!(kinds[0], kinds[1]);
    assert_ne!(kinds[1], kinds[2]);
    assert_eq!(format!("{:?}", LeanBindingKind::Setter), "Setter");

    let shapes = [
        LeanTypeShape::Unit,
        LeanTypeShape::Scalar,
        LeanTypeShape::String,
        LeanTypeShape::ByteArray,
        LeanTypeShape::Array,
        LeanTypeShape::Option,
        LeanTypeShape::Except,
        LeanTypeShape::Prod,
        LeanTypeShape::Named,
        LeanTypeShape::Unknown,
    ];
    for (i, s) in shapes.iter().enumerate() {
        assert_eq!(shapes[i], *s);
        for (j, other) in shapes.iter().enumerate() {
            assert_eq!(shapes[i] == *other, i == j);
        }
    }
    assert_eq!(format!("{:?}", LeanTypeShape::ByteArray), "ByteArray");
    assert_eq!(format!("{:?}", LeanTypeShape::Unknown), "Unknown");
}

// Shared metadata fixtures (const data, as macro-generated metadata is).
const UNIT_TY: leo3::LeanTypeMetadata = leo3::LeanTypeMetadata {
    rust: "()",
    lean: Some("Unit"),
    shape: leo3::LeanTypeShape::Unit,
};
const NAT_TY: leo3::LeanTypeMetadata = leo3::LeanTypeMetadata {
    rust: "u64",
    lean: Some("UInt64"),
    shape: leo3::LeanTypeShape::Scalar,
};
const STRING_TY: leo3::LeanTypeMetadata = leo3::LeanTypeMetadata {
    rust: "String",
    lean: Some("String"),
    shape: leo3::LeanTypeShape::String,
};
const BYTES_TY: leo3::LeanTypeMetadata = leo3::LeanTypeMetadata {
    rust: "Vec<u8>",
    lean: Some("ByteArray"),
    shape: leo3::LeanTypeShape::ByteArray,
};
const ARRAY_TY: leo3::LeanTypeMetadata = leo3::LeanTypeMetadata {
    rust: "Vec<u64>",
    lean: Some("Array UInt64"),
    shape: leo3::LeanTypeShape::Array,
};
const OPTION_TY: leo3::LeanTypeMetadata = leo3::LeanTypeMetadata {
    rust: "Option<u64>",
    lean: Some("Option UInt64"),
    shape: leo3::LeanTypeShape::Option,
};
const EXCEPT_TY: leo3::LeanTypeMetadata = leo3::LeanTypeMetadata {
    rust: "Result<u64, String>",
    lean: Some("Except String UInt64"),
    shape: leo3::LeanTypeShape::Except,
};
const PROD_TY: leo3::LeanTypeMetadata = leo3::LeanTypeMetadata {
    rust: "(u64, u64)",
    lean: Some("Prod UInt64 UInt64"),
    shape: leo3::LeanTypeShape::Prod,
};
const NAMED_TY: leo3::LeanTypeMetadata = leo3::LeanTypeMetadata {
    rust: "Counter",
    lean: Some("Counter"),
    shape: leo3::LeanTypeShape::Named,
};
const UNKNOWN_TY: leo3::LeanTypeMetadata = leo3::LeanTypeMetadata {
    rust: "Thunk<u64>",
    lean: None,
    shape: leo3::LeanTypeShape::Unknown,
};

static NO_PARAMS: &[leo3::LeanParameterMetadata] = &[];

static TWO_PARAMS: &[leo3::LeanParameterMetadata] = &[
    leo3::LeanParameterMetadata {
        name: "n",
        ty: NAT_TY,
        passing: leo3::LeanPassingStyle::Owned,
    },
    leo3::LeanParameterMetadata {
        name: "s",
        ty: STRING_TY,
        passing: leo3::LeanPassingStyle::Borrowed,
    },
];

/// Build a `LeanFunctionMetadata` with the given field combination.
#[allow(clippy::too_many_arguments)]
fn fn_meta(
    rust_name: &'static str,
    lean_name: &'static str,
    owner: Option<&'static str>,
    params: &'static [leo3::LeanParameterMetadata],
    return_type: leo3::LeanTypeMetadata,
    semantics: leo3::LeanBindingSemantics,
    kind: leo3::LeanBindingKind,
    lean_decl: Option<&'static str>,
) -> leo3::LeanFunctionMetadata {
    leo3::LeanFunctionMetadata {
        schema_version: leo3::LEO3_BINDING_SCHEMA_VERSION,
        rust_name,
        name: lean_name,
        owner,
        ffi_symbol: lean_name,
        receiver: match owner {
            Some(_) => leo3::LeanBindingReceiver::Ref,
            None => leo3::LeanBindingReceiver::None,
        },
        params,
        return_type,
        semantics,
        kind,
        lean_decl,
    }
}

#[test]
fn test_type_metadata_field_combinations() {
    // Every shape variant is readable and structurally distinct.
    let all = [
        UNIT_TY, NAT_TY, STRING_TY, BYTES_TY, ARRAY_TY, OPTION_TY, EXCEPT_TY, PROD_TY, NAMED_TY,
        UNKNOWN_TY,
    ];
    for (i, ty) in all.iter().enumerate() {
        for (j, other) in all.iter().enumerate() {
            assert_eq!(ty == other, i == j);
        }
    }

    // A copy is equal; fields read back exactly.
    let mut copy = STRING_TY;
    assert_eq!(copy, STRING_TY);
    assert_eq!(copy.rust, "String");
    assert_eq!(copy.lean, Some("String"));
    assert_eq!(copy.shape, leo3::LeanTypeShape::String);
    copy.rust = "char";
    assert_ne!(copy, STRING_TY);

    // Unknown shape carries no Lean-visible type.
    assert_eq!(UNKNOWN_TY.lean, None);

    // Debug rendering includes the field values.
    let dbg = format!("{:?}", NAT_TY);
    assert!(dbg.contains("UInt64") && dbg.contains("Scalar"));
}

#[test]
fn test_parameter_metadata_field_combinations() {
    use leo3::LeanParameterMetadata;

    let owned = LeanParameterMetadata {
        name: "n",
        ty: NAT_TY,
        passing: leo3::LeanPassingStyle::Owned,
    };
    let borrowed = LeanParameterMetadata {
        name: "n",
        ty: NAT_TY,
        passing: leo3::LeanPassingStyle::Borrowed,
    };
    let renamed = LeanParameterMetadata {
        name: "y",
        ty: NAT_TY,
        passing: leo3::LeanPassingStyle::Owned,
    };
    assert_eq!(owned, owned);
    assert_ne!(owned, borrowed);
    assert_ne!(owned, renamed);
    // TWO_PARAMS fixture matches `owned` exactly.
    assert_eq!(TWO_PARAMS[0], owned);
    assert_eq!(TWO_PARAMS[0].name, "n");
    assert_eq!(TWO_PARAMS[0].ty, NAT_TY);
    assert_eq!(TWO_PARAMS[0].passing, leo3::LeanPassingStyle::Owned);
    assert_eq!(TWO_PARAMS[1].name, "s");
    assert_eq!(TWO_PARAMS[1].ty.shape, leo3::LeanTypeShape::String);
    assert_eq!(TWO_PARAMS[1].ty.lean, Some("String"));
    assert_eq!(TWO_PARAMS[1].passing, leo3::LeanPassingStyle::Borrowed);
    assert_eq!(NO_PARAMS.len(), 0);
}

#[test]
fn test_function_metadata_field_combinations() {
    use leo3::{LeanBindingKind, LeanBindingReceiver, LeanBindingSemantics};

    // Free function, no params, no decl text.
    let free = fn_meta(
        "answer",
        "answer",
        None,
        NO_PARAMS,
        NAT_TY,
        LeanBindingSemantics::Value,
        LeanBindingKind::Method,
        None,
    );
    assert_eq!(free.schema_version, leo3::LEO3_BINDING_SCHEMA_VERSION);
    assert_eq!(free.rust_name, "answer");
    assert_eq!(free.name, "answer");
    assert_eq!(free.owner, None);
    assert_eq!(free.ffi_symbol, "answer");
    assert_eq!(free.receiver, LeanBindingReceiver::None);
    assert_eq!(free.params.len(), 0);
    assert_eq!(free.return_type, NAT_TY);
    assert_eq!(free.semantics, LeanBindingSemantics::Value);
    assert_eq!(free.kind, LeanBindingKind::Method);
    assert_eq!(free.lean_decl, None);

    // Method with owner, two params, decl text, mutating semantics.
    let method = fn_meta(
        "bump",
        "Counter.bump",
        Some("Counter"),
        TWO_PARAMS,
        PROD_TY,
        LeanBindingSemantics::MutatesSelfWithValue,
        LeanBindingKind::Method,
        Some("def Counter.bump ..."),
    );
    assert_eq!(method.owner, Some("Counter"));
    assert_eq!(method.receiver, LeanBindingReceiver::Ref);
    assert_eq!(method.params.len(), 2);
    assert_eq!(method.params[0], TWO_PARAMS[0]);
    assert_eq!(method.return_type, PROD_TY);
    assert_eq!(method.semantics, LeanBindingSemantics::MutatesSelfWithValue);
    assert_eq!(method.lean_decl, Some("def Counter.bump ..."));

    // Setter/getter accessor kinds and owned receiver.
    let getter = fn_meta(
        "size",
        "Counter.size",
        Some("Counter"),
        NO_PARAMS,
        NAT_TY,
        LeanBindingSemantics::Value,
        LeanBindingKind::Getter,
        None,
    );
    let setter = fn_meta(
        "set_size",
        "Counter.setSize",
        Some("Counter"),
        NO_PARAMS,
        UNIT_TY,
        LeanBindingSemantics::MutatesSelf,
        LeanBindingKind::Setter,
        None,
    );
    assert_eq!(getter.kind, LeanBindingKind::Getter);
    assert_eq!(setter.kind, LeanBindingKind::Setter);
    assert_eq!(setter.semantics, LeanBindingSemantics::MutatesSelf);

    // Equality follows field identity, not pointer identity.
    let same = fn_meta(
        "answer",
        "answer",
        None,
        NO_PARAMS,
        NAT_TY,
        LeanBindingSemantics::Value,
        LeanBindingKind::Method,
        None,
    );
    assert_eq!(free, same);
    assert_eq!(free, free.clone());
    assert_ne!(free, method);
    assert_ne!(method, setter);
    assert_ne!(getter, setter);

    // Debug rendering includes key fields.
    let dbg = format!("{:?}", method);
    assert!(dbg.contains("Counter.bump") && dbg.contains("MutatesSelfWithValue"));
}

#[test]
fn test_module_and_submodule_metadata() {
    use leo3::{LeanModuleMetadata, LeanSubmoduleMetadata};

    let export = fn_meta(
        "banner",
        "banner",
        None,
        NO_PARAMS,
        STRING_TY,
        leo3::LeanBindingSemantics::Value,
        leo3::LeanBindingKind::Method,
        None,
    );
    static EXPORTS: &[leo3::LeanFunctionMetadata] = &[leo3::LeanFunctionMetadata {
        schema_version: leo3::LEO3_BINDING_SCHEMA_VERSION,
        rust_name: "banner",
        name: "banner",
        owner: None,
        ffi_symbol: "banner",
        receiver: leo3::LeanBindingReceiver::None,
        params: &[],
        return_type: leo3::LeanTypeMetadata {
            rust: "String",
            lean: Some("String"),
            shape: leo3::LeanTypeShape::String,
        },
        semantics: leo3::LeanBindingSemantics::Value,
        kind: leo3::LeanBindingKind::Method,
        lean_decl: None,
    }];

    static SUBMODULES: &[LeanSubmoduleMetadata] = &[LeanSubmoduleMetadata {
        schema_version: leo3::LEO3_BINDING_SCHEMA_VERSION,
        path: "Demo.Inner",
        exports: &[leo3::LeanFunctionMetadata {
            schema_version: leo3::LEO3_BINDING_SCHEMA_VERSION,
            rust_name: "inner_fn",
            name: "inner_fn",
            owner: None,
            ffi_symbol: "inner_fn",
            receiver: leo3::LeanBindingReceiver::None,
            params: &[],
            return_type: leo3::LeanTypeMetadata {
                rust: "()",
                lean: Some("Unit"),
                shape: leo3::LeanTypeShape::Unit,
            },
            semantics: leo3::LeanBindingSemantics::Value,
            kind: leo3::LeanBindingKind::Method,
            lean_decl: None,
        }],
    }];

    let module = LeanModuleMetadata {
        schema_version: leo3::LEO3_BINDING_SCHEMA_VERSION,
        name: "Demo",
        exports: EXPORTS,
        submodules: SUBMODULES,
    };
    assert_eq!(module.schema_version, leo3::LEO3_BINDING_SCHEMA_VERSION);
    assert_eq!(module.name, "Demo");
    assert_eq!(module.exports.len(), 1);
    assert_eq!(module.exports[0], export);
    assert_eq!(module.exports[0].name, "banner");
    assert_eq!(module.submodules.len(), 1);
    assert_eq!(module.submodules[0].path, "Demo.Inner");
    assert_eq!(module.submodules[0].exports[0].rust_name, "inner_fn");
    assert_eq!(module.submodules[0].exports[0].return_type, UNIT_TY);

    // Empty module: no exports, no submodules.
    static NO_EXPORTS: &[leo3::LeanFunctionMetadata] = &[];
    static NO_SUBMODULES: &[LeanSubmoduleMetadata] = &[];
    let empty = LeanModuleMetadata {
        schema_version: leo3::LEO3_BINDING_SCHEMA_VERSION,
        name: "Empty",
        exports: NO_EXPORTS,
        submodules: NO_SUBMODULES,
    };
    assert_eq!(empty.exports.len(), 0);
    assert_eq!(empty.submodules.len(), 0);
    assert_ne!(module, empty);

    // Copy semantics: mutating a copy leaves the original intact.
    let mut copy = module;
    copy.name = "Renamed";
    assert_eq!(module.name, "Demo");
    assert_ne!(module, copy);

    let dbg = format!("{:?}", module);
    assert!(dbg.contains("Demo.Inner") && dbg.contains("Demo"));
}

#[test]
fn test_class_metadata() {
    use leo3::LeanClassMetadata;

    static METHODS: &[leo3::LeanFunctionMetadata] = &[leo3::LeanFunctionMetadata {
        schema_version: leo3::LEO3_BINDING_SCHEMA_VERSION,
        rust_name: "bump",
        name: "Counter.bump",
        owner: Some("Counter"),
        ffi_symbol: "Counter.bump",
        receiver: leo3::LeanBindingReceiver::MutRef,
        params: &[],
        return_type: leo3::LeanTypeMetadata {
            rust: "u32",
            lean: Some("UInt32"),
            shape: leo3::LeanTypeShape::Scalar,
        },
        semantics: leo3::LeanBindingSemantics::MutatesSelf,
        kind: leo3::LeanBindingKind::Method,
        lean_decl: Some("def Counter.bump (self : Counter) : UInt32 := ..."),
    }];

    let class = LeanClassMetadata {
        schema_version: leo3::LEO3_BINDING_SCHEMA_VERSION,
        rust_name: "Counter",
        lean_name: "Counter",
        opaque_decl: "opaque Counter.ffi : NonemptyType",
        methods_decl: "def Counter.bump : ...",
        methods: METHODS,
    };
    assert_eq!(class.schema_version, leo3::LEO3_BINDING_SCHEMA_VERSION);
    assert_eq!(class.rust_name, "Counter");
    assert_eq!(class.lean_name, "Counter");
    assert!(class.opaque_decl.starts_with("opaque"));
    assert!(class.methods_decl.contains("Counter.bump"));
    assert_eq!(class.methods.len(), 1);
    assert_eq!(class.methods[0].receiver, leo3::LeanBindingReceiver::MutRef);
    assert_eq!(
        class.methods[0].semantics,
        leo3::LeanBindingSemantics::MutatesSelf
    );
    assert_eq!(class.methods[0].return_type.lean, Some("UInt32"));

    let same = class;
    assert_eq!(class, same);
    let mut changed = class;
    changed.lean_name = "Renamed";
    assert_ne!(class, changed);
    assert_eq!(class.lean_name, "Counter");

    let dbg = format!("{:?}", class);
    assert!(dbg.contains("Counter") && dbg.contains("opaque"));
}

// ============================================================================
// lib.rs: __private boundary helpers
// ============================================================================

/// A tiny object-returning FFI entry point, shaped like macro-generated code.
unsafe extern "C" fn ffi_panic_boundary_ok(
    _world: *mut leo3::ffi::lean_object,
) -> *mut leo3::ffi::lean_object {
    leo3::__private::ffi_panic_boundary(|| Ok(leo3::ffi::inline::lean_box(12)))
}

/// A scalar-returning FFI entry point using the u64 boundary helper.
unsafe extern "C" fn ffi_scalar_u64_add(a: u64, b: u64) -> u64 {
    leo3::__private::scalar_u64_ffi_panic_boundary("ffi_scalar_u64_add", || Ok(a + b))
}

/// A scalar-returning FFI entry point using the generic scalar boundary helper.
unsafe extern "C" fn ffi_scalar_u32_neg(a: u32) -> u32 {
    leo3::__private::scalar_ffi_panic_boundary("ffi_scalar_u32_neg", || Ok(a.wrapping_neg()))
}

#[test]
fn test_panic_payload_message_variants() {
    use leo3::__private::panic_payload_message;

    // &'static str payload.
    let static_payload: &'static str = "static boom";
    let boxed: Box<dyn std::any::Any + Send> = Box::new(static_payload);
    assert_eq!(
        panic_payload_message(boxed.as_ref()),
        "Rust panic in FFI: static boom"
    );

    // String payload.
    let string_payload: Box<dyn std::any::Any + Send> = Box::new(String::from("string boom"));
    assert_eq!(
        panic_payload_message(string_payload.as_ref()),
        "Rust panic in FFI: string boom"
    );

    // Any other payload type.
    let other_payload: Box<dyn std::any::Any + Send> = Box::new(42u64);
    assert_eq!(
        panic_payload_message(other_payload.as_ref()),
        "Rust panic in FFI"
    );
}

#[test]
fn test_ffi_panic_boundary_ok_path() {
    leo3::prepare_freethreaded_lean();

    // Successful body: the boundary returns the produced object untouched.
    let result: LeanResult<()> = leo3::with_lean(|_lean| {
        let ptr = unsafe { ffi_panic_boundary_ok(std::ptr::null_mut()) };
        assert_eq!(unsafe { leo3::ffi::inline::lean_unbox(ptr) }, 12);
        unsafe { leo3::ffi::lean_dec(ptr) };
        Ok(())
    });
    assert!(result.is_ok());
}

/// Serializes the two boundary tests that toggle the process-global
/// `g_panic_messages` flag, so one test's `set_panic_messages(true)` restore
/// cannot re-enable diagnostics while the other test is mid-call.
static PANIC_MESSAGE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn test_ffi_panic_boundary_err_path() {
    leo3::prepare_freethreaded_lean();

    // Body returning Err: the boundary converts it into a Lean panic object.
    // Suppress the runtime's PANIC message + backtrace diagnostic on stderr
    // (it would otherwise spam the test log); restore the default afterwards.
    let _guard = PANIC_MESSAGE_LOCK.lock().unwrap();
    let result: LeanResult<()> = leo3::with_lean(|_lean| {
        unsafe { leo3::ffi::object::lean_set_panic_messages(false) };
        let ptr = unsafe {
            leo3::__private::ffi_panic_boundary(|| Err(LeanError::other("boundary failure")))
        };
        unsafe { leo3::ffi::object::lean_set_panic_messages(true) };
        assert!(!ptr.is_null());
        unsafe { leo3::ffi::lean_dec(ptr) };
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_lean_panic_message_builds_object() {
    leo3::prepare_freethreaded_lean();

    let _guard = PANIC_MESSAGE_LOCK.lock().unwrap();
    let result: LeanResult<()> = leo3::with_lean(|lean| {
        unsafe { leo3::ffi::object::lean_set_panic_messages(false) };
        let ptr = unsafe { leo3::__private::lean_panic_message(lean, "direct panic message") };
        unsafe { leo3::ffi::object::lean_set_panic_messages(true) };
        assert!(!ptr.is_null());
        unsafe { leo3::ffi::lean_dec(ptr) };
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_scalar_ffi_panic_boundaries_ok_path() {
    leo3::prepare_freethreaded_lean();

    // Both scalar boundary helpers pass successful bodies straight through.
    let sum = unsafe { ffi_scalar_u64_add(30, 12) };
    assert_eq!(sum, 42);

    let neg = unsafe { ffi_scalar_u32_neg(7) };
    assert_eq!(neg, u32::MAX - 6);

    // Generic helper used directly with a u64 body, as generated wrappers do.
    let direct: u64 = leo3::__private::scalar_ffi_panic_boundary("direct", || Ok(5u64 + 5));
    assert_eq!(direct, 10);
}

// ============================================================================
// LeanByteArray residual paths
// ============================================================================

#[test]
fn test_bytearray_residual_paths() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Large buffer through from_bytes: size, capacity, and contents agree.
        let large: Vec<u8> = (0..4096).map(|i| (i % 251) as u8).collect();
        let ba = LeanByteArray::from_bytes(lean, &large)?;
        assert_eq!(LeanByteArray::size(&ba), 4096);
        assert!(!LeanByteArray::isEmpty(&ba));
        assert!(LeanByteArray::capacity(&ba) >= 4096);
        assert_eq!(LeanByteArray::to_vec(&ba), large);
        assert_eq!(format!("{:?}", ba), "LeanByteArray(size: 4096)");

        // Push-driven growth past a small initial capacity.
        let mut g = LeanByteArray::with_capacity(lean, 1)?;
        for b in 0u16..=300 {
            g = LeanByteArray::push(g, (b % 256) as u8)?;
        }
        assert_eq!(LeanByteArray::size(&g), 301);
        assert!(!LeanByteArray::isEmpty(&g));
        assert!(LeanByteArray::capacity(&g) >= 301);
        assert_eq!(LeanByteArray::get(&g, 300), Some((300 % 256) as u8));
        assert_eq!(LeanByteArray::get(&g, 301), None);

        // Checked set out-of-bounds returns the array unchanged; get is None.
        let mut arr = LeanByteArray::from_bytes(lean, &[1, 2, 3])?;
        arr = LeanByteArray::set(arr, 3, 9)?; // OOB: unchanged
        assert_eq!(LeanByteArray::size(&arr), 3);
        assert_eq!(LeanByteArray::get(&arr, 3), None);
        arr = LeanByteArray::set(arr, 1, 8)?; // in-bounds: replaced
        assert_eq!(LeanByteArray::get(&arr, 1), Some(8));

        // Raw uget/uset paths.
        unsafe {
            assert_eq!(LeanByteArray::uget(&arr, 0), 1);
            arr = LeanByteArray::uset(arr, 0, 7)?;
        }
        assert_eq!(LeanByteArray::get(&arr, 0), Some(7));
        assert_eq!(LeanByteArray::size(&arr), 3);
        assert_eq!(format!("{:?}", arr), "LeanByteArray(size: 3)");

        // isEmpty / capacity after operations on a freshly pushed array.
        let e = LeanByteArray::empty(lean)?;
        assert!(LeanByteArray::isEmpty(&e));
        assert_eq!(LeanByteArray::capacity(&e), 0);
        let one = LeanByteArray::push(e, 5)?;
        assert!(!LeanByteArray::isEmpty(&one));
        assert_eq!(LeanByteArray::size(&one), 1);
        assert_eq!(LeanByteArray::get(&one, 0), Some(5));
        assert_eq!(format!("{:?}", one), "LeanByteArray(size: 1)");
        Ok(())
    });
    assert!(result.is_ok());
}

// ============================================================================
// LeanPromise round-trips
// ============================================================================

#[cfg(feature = "task")]
mod promise_ops {
    use leo3::instance::LeanAny;
    use leo3::prelude::*;
    use leo3::promise::LeanPromise;
    use leo3::types::LeanOption;

    /// Read the Option-some payload out of a resolved promise task.
    fn take_some<'l>(opt: LeanBound<'l, LeanOption>) -> LeanResult<LeanBound<'l, LeanAny>> {
        match LeanOption::get(&opt) {
            Some(inner) => Ok(inner),
            None => Err(LeanError::other("expected Option.some from promise task")),
        }
    }

    #[test]
    fn test_promise_is_promise_and_try_from_any_on_real_promise() {
        leo3::prepare_freethreaded_lean();

        let result: LeanResult<()> = leo3::with_lean(|lean| {
            let promise = LeanPromise::<LeanAny>::new(lean)?;

            // A real promise reports is_promise == true.
            assert!(LeanPromise::<LeanAny>::is_promise(&promise.clone().cast()));

            // try_from_any on a real promise yields Some.
            let converted: LeanPromise<'_, LeanAny> =
                match LeanPromise::try_from_any(promise.clone().cast()) {
                    Some(p) => p,
                    None => return Err(LeanError::other("try_from_any rejected a promise")),
                };

            // The converted reference drives the same underlying task.
            let task = converted.task();
            let value = LeanNat::from_usize(lean, 5)?;
            converted.resolve(value.cast())?;

            let opt: LeanBound<'_, LeanOption> = task.get_owned().cast();
            let inner = take_some(opt)?;
            let n: LeanBound<'_, LeanNat> = inner.cast();
            assert_eq!(LeanNat::to_usize(&n)?, 5);
            Ok(())
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_promise_resolve_twice_first_value_wins() {
        leo3::prepare_freethreaded_lean();

        let result: LeanResult<()> = leo3::with_lean(|lean| {
            let promise = LeanPromise::<LeanAny>::new(lean)?;
            let task = promise.task();

            // A second reference to the same underlying promise.
            let second = promise.clone();

            let first_value = LeanNat::from_usize(lean, 42)?;
            promise.resolve(first_value.cast())?;

            // The runtime treats a second resolve as a silent no-op: the first
            // value wins and the second value is dropped.
            let second_value = LeanNat::from_usize(lean, 99)?;
            second.resolve(second_value.cast())?;

            let opt: LeanBound<'_, LeanOption> = task.get_owned().cast();
            let inner = take_some(opt)?;
            let n: LeanBound<'_, LeanNat> = inner.cast();
            assert_eq!(LeanNat::to_usize(&n)?, 42);
            Ok(())
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_promise_string_and_nat_values() {
        leo3::prepare_freethreaded_lean();

        let result: LeanResult<()> = leo3::with_lean(|lean| {
            // String value round-trip.
            let sp = LeanPromise::<LeanAny>::new(lean)?;
            let stask = sp.task();
            let s = LeanString::mk(lean, "promise string")?;
            sp.resolve(s.cast())?;
            let opt: LeanBound<'_, LeanOption> = stask.get_owned().cast();
            let inner = take_some(opt)?;
            let str: LeanBound<'_, LeanString> = inner.cast();
            assert_eq!(LeanString::cstr(&str)?, "promise string");

            // Nat value round-trip with a typed promise.
            let np = LeanPromise::<LeanNat>::new(lean)?;
            let ntasks = [np.task(), np.task()];
            let n = LeanNat::from_usize(lean, 1234)?;
            np.resolve(n)?;
            for task in &ntasks {
                let opt: LeanBound<'_, LeanOption> = task.get_cloned().cast();
                let inner = take_some(opt)?;
                let nat: LeanBound<'_, LeanNat> = inner.cast();
                assert_eq!(LeanNat::to_usize(&nat)?, 1234);
            }
            Ok(())
        });
        assert!(result.is_ok());
    }
}

// ============================================================================
// io::console output (stdout / stdin)
// ============================================================================

#[cfg(feature = "io")]
mod console_io {
    use leo3::io::console;
    use leo3::prelude::*;

    #[test]
    fn test_console_put_str_empty_and_long() {
        leo3::prepare_freethreaded_lean();

        let result: LeanResult<()> = leo3::with_lean(|lean| {
            // Empty strings exercise the closure capture path with no payload.
            console::put_str(lean, "")?.run()?;
            console::put_str_ln(lean, "")?.run()?;

            // Long strings exercise the length-aware string conversion.
            let long = "x".repeat(8192);
            console::put_str(lean, &long)?.run()?;
            let long_line = format!("{}!", "y".repeat(4096));
            console::put_str_ln(lean, &long_line)?.run()?;

            // Multi-byte UTF-8 content.
            console::put_str(lean, "héllo — wörld ✓")?.run()?;
            console::put_str_ln(lean, "ünïcode line")?.run()?;
            Ok(())
        });
        assert!(result.is_ok());
    }

    #[test]
    fn test_console_get_line_from_piped_stdin() {
        // get_line reads the process stdin, which the test harness does not
        // provide, so drive it through a child process with a piped stdin.
        let exe = std::env::current_exe().expect("current test executable");
        let mut child = std::process::Command::new(&exe)
            .args([
                "--exact",
                "console_io::get_line_child_probe",
                "--ignored",
                "--test-threads=1",
            ])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to spawn get_line child probe");

        {
            use std::io::Write;
            let _ = child
                .stdin
                .as_mut()
                .expect("child stdin pipe")
                .write_all(b"probe line\n");
        }

        let status = child.wait().expect("failed to wait for child probe");
        assert!(
            status.success(),
            "get_line child probe failed to read the piped line"
        );
    }

    /// Child-process helper for [`test_console_get_line_from_piped_stdin`].
    #[test]
    #[ignore = "child probe; driven by test_console_get_line_from_piped_stdin"]
    fn get_line_child_probe() {
        leo3::prepare_freethreaded_lean();

        let result: LeanResult<()> = leo3::with_lean(|lean| {
            let io = console::get_line(lean)?;
            let line = io.run()?;
            // The returned string includes the trailing newline.
            assert_eq!(line, "probe line\n");
            Ok(())
        });
        assert!(result.is_ok());
    }
}

// ============================================================================
// tokio_bridge
// ============================================================================

#[cfg(all(feature = "runtime-tests", feature = "tokio"))]
mod tokio_bridge_ops {
    use leo3::instance::LeanAny;
    use leo3::task::LeanTask;
    use leo3::tokio_bridge::lean_block_in_place;

    unsafe extern "C" fn make_nat_7(
        _world: *mut leo3::ffi::lean_object,
    ) -> *mut leo3::ffi::lean_object {
        leo3::ffi::inline::lean_box(7)
    }

    unsafe extern "C" fn slow_nat_30(
        _world: *mut leo3::ffi::lean_object,
    ) -> *mut leo3::ffi::lean_object {
        std::thread::sleep(std::time::Duration::from_millis(30));
        leo3::ffi::inline::lean_box(30)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_spawn_on_tokio_returns_value() {
        leo3::prepare_freethreaded_lean();

        let join = leo3::with_lean(|lean| {
            let closure = leo3::closure::LeanClosure::from_fn1(lean, make_nat_7).unwrap();
            LeanTask::<LeanAny>::spawn_on_tokio(closure)
        });

        let unbound = join.await.unwrap();
        let n = unsafe { leo3::ffi::inline::lean_unbox(unbound.as_ptr()) };
        assert_eq!(n, 7);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_task_handle_into_tokio_future_awaited() {
        leo3::prepare_freethreaded_lean();

        let handle = leo3::with_lean(|lean| {
            let closure = leo3::closure::LeanClosure::from_fn1(lean, slow_nat_30).unwrap();
            LeanTask::<LeanAny>::spawn(closure).into_handle()
        });

        let unbound = handle.into_tokio_future().await;
        let n = unsafe { leo3::ffi::inline::lean_unbox(unbound.as_ptr()) };
        assert_eq!(n, 30);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_lean_block_in_place_plain_closure() {
        leo3::prepare_freethreaded_lean();

        // block_in_place is only legal on a multi-threaded runtime; the test
        // flavor above provides it. Direct call and prelude re-export agree.
        let value = lean_block_in_place(|| 2 + 2);
        assert_eq!(value, 4);
        let again = leo3::prelude::lean_block_in_place(|| 3 * 3);
        assert_eq!(again, 9);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_lean_block_in_place_lean_wait() {
        leo3::prepare_freethreaded_lean();

        let handle = leo3::with_lean(|lean| {
            let closure = leo3::closure::LeanClosure::from_fn1(lean, slow_nat_30).unwrap();
            LeanTask::<LeanAny>::spawn(closure).into_handle()
        });

        // A real Lean native wait (TaskHandle::get_unbound) executed through
        // the blocking-pool bridge on the multi-thread runtime.
        let unbound = lean_block_in_place(|| handle.get_unbound());
        let n = unsafe { leo3::ffi::inline::lean_unbox(unbound.as_ptr()) };
        assert_eq!(n, 30);
    }
}
