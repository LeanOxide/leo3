use std::path::{Path, PathBuf};
use std::process::Command;

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("leo3")
        .join("tests")
        .join("fixtures")
        .join("leanmodule_runtime_fixture")
        .join("Cargo.toml")
}

fn codegen_bin() -> PathBuf {
    let mut path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("target")
        .join("debug")
        .join("leo3-codegen");
    if cfg!(windows) {
        path.set_extension("exe");
    }
    path
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
    let target_dir =
        std::env::temp_dir().join(format!("leo3-codegen-fixture-{}", std::process::id()));

    let status = Command::new("cargo")
        .arg("build")
        .arg("--quiet")
        .arg("--manifest-path")
        .arg(fixture_manifest())
        .env("CARGO_TARGET_DIR", &target_dir)
        .status()
        .expect("fixture cargo build should start");

    assert!(status.success(), "fixture cargo build failed");

    target_dir.join("debug").join(dylib_name())
}

#[test]
fn codegen_generates_module_and_class_lean_files() {
    let dylib = build_fixture();
    assert!(
        dylib.is_file(),
        "expected built fixture at {}",
        dylib.display()
    );

    let output_dir =
        std::env::temp_dir().join(format!("leo3-codegen-output-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&output_dir);

    let output = Command::new(codegen_bin())
        .arg(&dylib)
        .arg("-o")
        .arg(&output_dir)
        .output()
        .expect("leo3-codegen should execute");

    assert!(
        output.status.success(),
        "leo3-codegen failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let module_file = output_dir.join("FixtureModule.lean");
    assert!(
        module_file.is_file(),
        "expected generated module file at {}",
        module_file.display()
    );
    let module_content = std::fs::read_to_string(&module_file).unwrap();
    assert!(module_content.contains("-- Module: FixtureModule"));
    assert!(module_content
        .contains("@[extern \"fixture_add\"] opaque fixture_add : UInt64 → UInt64 → UInt64"));
    assert!(module_content
        .contains("@[extern \"fixture_banner\"] opaque fixture_banner : String → Int32 → String"));

    let class_file = output_dir.join("FixtureCounter.lean");
    assert!(
        class_file.is_file(),
        "expected generated class file at {}",
        class_file.display()
    );
    let class_content = std::fs::read_to_string(&class_file).unwrap();
    assert!(class_content.contains("-- Class: FixtureCounter"));
    assert!(class_content.contains("opaque FixtureCounter : Type"));
    assert!(class_content.contains("@[extern \"__lean_ffi_FixtureCounter_new\"] opaque FixtureCounter.new : Int32 → FixtureCounter"));
    assert!(class_content.contains("@[extern \"__lean_ffi_FixtureCounter_get\"] opaque FixtureCounter.get : FixtureCounter → Int32"));
    assert!(class_content.contains("@[extern \"__lean_ffi_FixtureCounter_increment\"] opaque FixtureCounter.increment : FixtureCounter → FixtureCounter"));

    let _ = std::fs::remove_dir_all(&output_dir);
}
