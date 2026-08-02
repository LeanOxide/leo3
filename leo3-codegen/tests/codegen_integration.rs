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
    if let Some(path) = option_env!("CARGO_BIN_EXE_leo3-codegen") {
        return PathBuf::from(path);
    }
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
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTC_WRAPPER")
        .env_remove("RUSTC_WORKSPACE_WRAPPER")
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
    // The class type is introduced through `NonemptyType` so the `opaque`
    // method declarations returning it can elaborate (see `class_opaque_decl`).
    assert!(class_content.contains("opaque FixtureCounter.ffi : NonemptyType"));
    assert!(class_content.contains("def FixtureCounter : Type := FixtureCounter.ffi.val"));
    assert!(
        class_content.contains("instance : Nonempty FixtureCounter := FixtureCounter.ffi.property")
    );
    assert!(class_content.contains("@[extern \"__lean_ffi_FixtureCounter_new\"] opaque FixtureCounter.new : Int32 → FixtureCounter"));
    assert!(class_content.contains("@[extern \"__lean_ffi_FixtureCounter_get\"] opaque FixtureCounter.get : FixtureCounter → Int32"));
    assert!(class_content.contains("@[extern \"__lean_ffi_FixtureCounter_increment\"] opaque FixtureCounter.increment : FixtureCounter → FixtureCounter"));

    let _ = std::fs::remove_dir_all(&output_dir);
}

/// Regression test for the macOS-only failure where the linker does not surface
/// the `#[no_mangle] #[used]` metadata symbols in a dylib's symbol table.
///
/// The macros also embed a self-describing framed copy of each metadata entry
/// into a dedicated link section. This test verifies that `leo3-codegen` can
/// recover the metadata purely from that section (no symbol table), which is the
/// fallback path macOS relies on. It runs on every platform, so a regression in
/// the framing/section layout is caught even though Linux itself uses symbols.
#[test]
fn metadata_is_recoverable_from_dedicated_section() {
    use object::{Object, ObjectSection};

    let dylib = build_fixture();
    let data = std::fs::read(&dylib).expect("should read built fixture");
    let obj = object::File::parse(data.as_slice()).expect("should parse object file");

    let mut entries: Vec<(String, String)> = Vec::new();
    for section in obj.sections() {
        let name = section.name().unwrap_or("");
        if !name.contains(leo3_binding_ir::METADATA_SECTION_MARKER) {
            continue;
        }
        if let Ok(section_data) = section.data() {
            entries.extend(leo3_binding_ir::parse_metadata_entries(section_data));
        }
    }

    let names: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        names.contains(&"__leo3_module_metadata_json_FixtureModule"),
        "expected module metadata entry in section, got: {names:?}"
    );
    assert!(
        names.contains(&"__leo3_class_metadata_json_FixtureCounter"),
        "expected class metadata entry in section, got: {names:?}"
    );

    // Each recovered JSON payload must deserialize into its IR type.
    for (name, json) in &entries {
        if name.starts_with("__leo3_module_metadata_json_") {
            let _: leo3_binding_ir::ModuleBinding =
                serde_json::from_str(json).expect("module metadata JSON should parse");
        } else if name.starts_with("__leo3_class_metadata_json_") {
            let _: leo3_binding_ir::ClassMetadata =
                serde_json::from_str(json).expect("class metadata JSON should parse");
        }
    }
}
