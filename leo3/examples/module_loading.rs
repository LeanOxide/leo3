//! Example: Dynamic Module Loading.
//!
//! Demonstrates building a cdylib fixture with #[leanmodule] and loading it
//! at runtime via LeanModule::load, then calling exported functions.
//!
//! Run with:
//! ```bash
//! cargo run --example module_loading --features "macros,module-loading"
//! ```

use leo3::module::LeanModule;
use leo3::prelude::*;
use std::path::{Path, PathBuf};
use std::process::Command;
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
        "leo3-module-example-{}-{}",
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

fn build_fixture() -> PathBuf {
    let target_dir = unique_target_dir();
    println!("   Building fixture cdylib...");
    let status = Command::new("cargo")
        .arg("build")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(fixture_manifest())
        .env("CARGO_TARGET_DIR", &target_dir)
        .status()
        .expect("fixture cargo build should start");
    assert!(status.success(), "fixture build failed");
    target_dir.join("debug").join(dylib_name())
}

fn main() -> LeanResult<()> {
    println!("=== Module Loading Example ===\n");

    println!("1. Build fixture shared library:");
    let dylib = build_fixture();
    println!("   Built: {}", dylib.display());

    println!("\n2. Load module dynamically:");
    leo3::prepare_freethreaded_lean();
    let module = LeanModule::load(&dylib, "FixtureModule")
        .unwrap_or_else(|e| panic!("failed to load module: {e}"));
    println!("   Loaded module: {}", module.name());

    println!("\n3. Call exported functions:");
    leo3::with_lean(|lean| {
        let add = module.get_function("fixture_add", 2)?;
        let sum: u64 = add.call2(lean, 20_u64, 22_u64)?;
        println!("   fixture_add(20, 22) = {}", sum);

        let banner = module.get_function("fixture_banner", 2)?;
        let msg: String = banner.call2(lean, String::from("leo3"), 3_i32)?;
        println!("   fixture_banner(\"leo3\", 3) = \"{}\"", msg);

        Ok::<_, LeanError>(())
    })?;

    println!("\n=== Module loading completed successfully! ===");
    Ok(())
}
