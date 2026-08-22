//! Build script for leo3-build-config.
//!
//! When the `resolve-config` feature is enabled, this script materializes the
//! same `LEO3_CONFIG_FILE` / host-detection result that the library API uses.

#[cfg(feature = "resolve-config")]
#[path = "src/errors.rs"]
mod errors;
#[cfg(feature = "resolve-config")]
#[path = "src/impl_.rs"]
mod impl_;
#[cfg(feature = "resolve-config")]
#[path = "src/windows_lean_imports.rs"]
mod windows_lean_imports;

fn main() {
    #[cfg(feature = "resolve-config")]
    resolve_config();

    #[cfg(not(feature = "resolve-config"))]
    {
        // Nothing to do — downstream crates read config via DEP_LEAN4_LEO3_CONFIG
    }
}

#[cfg(feature = "resolve-config")]
fn resolve_config() {
    use std::env;
    use std::path::PathBuf;

    impl_::print_rerun_if_env_changed();

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR not set"));
    let config_path = out_dir.join("leo3-build-config.txt");

    // LEO3_NO_LEAN: write a sentinel empty file
    if env::var("LEO3_NO_LEAN").is_ok() {
        std::fs::write(&config_path, "").expect("failed to write sentinel config");
        return;
    }

    match impl_::resolve_user_or_detect_config() {
        Ok(resolved) => {
            impl_::emit_resolved_config_rerun_if_changed(&resolved);
            let mut file =
                std::fs::File::create(&config_path).expect("failed to create config file");
            resolved
                .config
                .to_writer(&mut file)
                .expect("failed to write config");
        }
        Err(error) => {
            for line in error.warning_lines() {
                errors::cargo_warn!("{}", line);
            }
            // Write empty sentinel so downstream doesn't fail on missing file
            std::fs::write(&config_path, "").expect("failed to write sentinel config");
        }
    }
}
