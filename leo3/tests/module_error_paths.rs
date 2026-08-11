//! Error-path tests for dynamic Lean module loading (`leo3::module`).
//!
//! Covers `LeanModule::load` failures (missing file, non-library file, unknown
//! module name), `get_function` symbol lookup failures, `LeanFunction`
//! name/arity accessors, call-side arity validation, and a full success-path
//! round trip through the fixture so the module API is proven end to end.
//!
//! Note on wrong-typed arguments: `LeanFunction::callN` boxes every argument
//! and passes it through the raw extern ABI without runtime type checks
//! (matching Lean's own boxed calling convention). Passing an argument whose
//! Lean type does not match the compiled function's signature is undefined
//! behavior on Lean's side rather than a recoverable `Err`, so this file
//! exercises the type errors the API *does* surface deterministically
//! (unknown symbols, arity mismatches, load failures) instead.

#![cfg(all(
    feature = "macros",
    feature = "module-loading",
    feature = "runtime-tests"
))]

use leo3::module::LeanModule;
use leo3::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("leanmodule_runtime_fixture")
        .join("Cargo.toml")
}

fn unique_target_dir() -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after unix epoch")
        .as_millis();
    std::env::temp_dir().join(format!(
        "leo3-leanmodule-fixture-{}-{}",
        std::process::id(),
        millis
    ))
}

fn dylib_name() -> &'static str {
    #[cfg(target_os = "linux")]
    {
        "libleanmodule_runtime_fixture.so"
    }
    #[cfg(target_os = "macos")]
    {
        "libleanmodule_runtime_fixture.dylib"
    }
    #[cfg(target_os = "windows")]
    {
        "leanmodule_runtime_fixture.dll"
    }
}

fn address_sanitizer_enabled() -> bool {
    std::env::var("RUSTFLAGS")
        .ok()
        .is_some_and(|flags| flags.contains("sanitizer=address"))
}

fn build_fixture() -> PathBuf {
    let target_dir = unique_target_dir();
    let status = Command::new("cargo")
        .arg("build")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(fixture_manifest())
        .env("CARGO_TARGET_DIR", &target_dir)
        .status()
        .expect("fixture cargo build should start");

    assert!(status.success(), "fixture cargo build failed: {status}");

    target_dir.join("debug").join(dylib_name())
}

/// Build the fixture at most once per test process and share the artifact.
///
/// The generated `initialize_FixtureModule` only returns `IO.ok ()`, so loading
/// the same dylib from several tests in one process is side-effect free.
static FIXTURE_DYLIB: LazyLock<PathBuf> = LazyLock::new(build_fixture);

fn fixture_dylib() -> &'static PathBuf {
    &FIXTURE_DYLIB
}

fn load_fixture() -> LeanModule {
    let dylib = fixture_dylib();
    assert!(
        dylib.is_file(),
        "expected built fixture at {}",
        dylib.display()
    );
    LeanModule::load(dylib, "FixtureModule")
        .unwrap_or_else(|err| panic!("failed to load fixture {}: {err}", dylib.display()))
}

fn err_message(err: &LeanError) -> String {
    err.to_string()
}

#[test]
fn test_load_nonexistent_path_errors() {
    leo3::prepare_freethreaded_lean();

    let missing = unique_target_dir().join("does-not-exist.so");
    assert!(
        !missing.exists(),
        "precondition: {} must not exist",
        missing.display()
    );

    let err = match LeanModule::load(&missing, "FixtureModule") {
        Ok(_) => panic!("expected loading {} to fail", missing.display()),
        Err(err) => err,
    };
    let message = err_message(&err);
    assert!(
        message.contains("failed to load Lean module library"),
        "{message}"
    );
    assert!(message.contains("does-not-exist.so"), "{message}");
}

#[test]
fn test_load_non_library_file_errors() {
    leo3::prepare_freethreaded_lean();

    let non_library = unique_target_dir().join("not-a-library.txt");
    std::fs::create_dir_all(non_library.parent().expect("temp path has a parent")).unwrap();
    std::fs::write(&non_library, b"this is plain text, not a shared object").unwrap();

    let err = match LeanModule::load(&non_library, "FixtureModule") {
        Ok(_) => panic!("expected loading {} to fail", non_library.display()),
        Err(err) => err,
    };
    let message = err_message(&err);
    assert!(
        message.contains("failed to load Lean module library"),
        "{message}"
    );
}

#[test]
fn test_load_wrong_module_name_errors() {
    if address_sanitizer_enabled() {
        return;
    }
    leo3::prepare_freethreaded_lean();

    let dylib = fixture_dylib();
    let err = match LeanModule::load(dylib, "WrongModuleName") {
        Ok(_) => panic!("expected initialize_WrongModuleName lookup to fail"),
        Err(err) => err,
    };
    let message = err_message(&err);
    assert!(
        message.contains("failed to resolve Lean symbol `initialize_WrongModuleName`"),
        "{message}"
    );
}

#[test]
fn test_get_function_unknown_symbol_errors() {
    if address_sanitizer_enabled() {
        return;
    }
    leo3::prepare_freethreaded_lean();

    let module = load_fixture();
    let err = match module.get_function("no_such_export", 1) {
        Ok(_) => panic!("expected unknown symbol lookup to fail"),
        Err(err) => err,
    };
    let message = err_message(&err);
    assert!(
        message.contains("failed to resolve Lean symbol `no_such_export`"),
        "{message}"
    );
}

#[test]
fn test_function_name_and_arity() {
    if address_sanitizer_enabled() {
        return;
    }
    leo3::prepare_freethreaded_lean();

    let module = load_fixture();
    leo3::with_lean(|_lean| {
        let add = module
            .get_function("fixture_add", 2)
            .expect("fixture_add should be exported");
        assert_eq!(add.name(), "fixture_add");
        assert_eq!(add.arity(), 2);

        let banner = module
            .get_function("fixture_banner", 2)
            .expect("fixture_banner should be exported");
        assert_eq!(banner.name(), "fixture_banner");
        assert_eq!(banner.arity(), 2);

        Ok::<_, LeanError>(())
    })
    .unwrap();
}

#[test]
fn test_call_wrong_arity_errors() {
    if address_sanitizer_enabled() {
        return;
    }
    leo3::prepare_freethreaded_lean();

    let module = load_fixture();
    leo3::with_lean(|lean| {
        let add = module
            .get_function("fixture_add", 2)
            .expect("fixture_add should be exported");

        // Too few arguments: arity is validated before any conversion or call.
        let result: LeanResult<u64> = add.call1(lean, 20_u64);
        let err = match result {
            Ok(_) => panic!("call1 with an arity-2 function should fail"),
            Err(err) => err,
        };
        let message = err_message(&err);
        assert!(message.contains("fixture_add"), "{message}");
        assert!(
            message.contains("expects 2 argument(s), but 1 provided"),
            "{message}"
        );

        // Too many arguments.
        let result: LeanResult<u64> = add.call3(lean, 20_u64, 22_u64, 24_u64);
        let err = match result {
            Ok(_) => panic!("call3 with an arity-2 function should fail"),
            Err(err) => err,
        };
        let message = err_message(&err);
        assert!(message.contains("fixture_add"), "{message}");
        assert!(
            message.contains("expects 2 argument(s), but 3 provided"),
            "{message}"
        );

        Ok::<_, LeanError>(())
    })
    .unwrap();
}

#[test]
fn test_wrong_arity_metadata_from_get_function() {
    if address_sanitizer_enabled() {
        return;
    }
    leo3::prepare_freethreaded_lean();

    let module = load_fixture();
    leo3::with_lean(|lean| {
        // The arity supplied at lookup time is validated at the call boundary,
        // so declaring the wrong arity surfaces as an arity mismatch.
        let add = module
            .get_function("fixture_add", 3)
            .expect("fixture_add should be exported");
        assert_eq!(add.arity(), 3);

        let result: LeanResult<u64> = add.call2(lean, 20_u64, 22_u64);
        let err = match result {
            Ok(_) => panic!("call2 against arity-3 metadata should fail"),
            Err(err) => err,
        };
        let message = err_message(&err);
        assert!(message.contains("fixture_add"), "{message}");
        assert!(
            message.contains("expects 3 argument(s), but 2 provided"),
            "{message}"
        );

        Ok::<_, LeanError>(())
    })
    .unwrap();
}

#[test]
fn test_module_success_path() {
    if address_sanitizer_enabled() {
        return;
    }
    leo3::prepare_freethreaded_lean();

    let dylib = fixture_dylib();
    let module = LeanModule::load(dylib, "FixtureModule")
        .unwrap_or_else(|err| panic!("failed to load fixture {}: {err}", dylib.display()));

    assert_eq!(module.name(), "FixtureModule");

    leo3::with_lean(|lean| {
        let add = module
            .get_function("fixture_add", 2)
            .expect("fixture_add should be exported");
        assert_eq!(add.name(), "fixture_add");
        assert_eq!(add.arity(), 2);
        let sum: u64 = add
            .call2(lean, 20_u64, 22_u64)
            .expect("fixture_add should execute successfully");
        assert_eq!(sum, 42);

        let banner = module
            .get_function("fixture_banner", 2)
            .expect("fixture_banner should be exported");
        assert_eq!(banner.name(), "fixture_banner");
        assert_eq!(banner.arity(), 2);
        let message: String = banner
            .call2(lean, String::from("orbiter"), 7_i32)
            .expect("fixture_banner should execute successfully");
        assert_eq!(message, "orbiter has 7 ticks");

        Ok::<_, LeanError>(())
    })
    .unwrap();
}
