#![cfg_attr(docsrs, feature(doc_cfg))]
//! Build-time configuration for Leo3 (Rust-Lean4 bindings).
//!
//! This crate provides functionality to detect Lean4 installations and
//! configure build settings appropriately. It should be called from
//! build scripts of crates that depend on leo3.

pub mod errors;
pub mod impl_;

use errors::cargo_warn;
use std::env;
use std::sync::OnceLock;

pub use impl_::{
    LeanAllocator, LeanConfig, LeanConfigSource, LeanVersion, ResolutionAttempt, ResolutionError,
    ResolvedLeanConfig,
};

static LEAN_CONFIG: OnceLock<ResolvedLeanConfig> = OnceLock::new();

fn warn_resolution_error(error: &ResolutionError) {
    for line in error.warning_lines() {
        cargo_warn!("{}", line);
    }
}

fn emit_resolved_config(config: &LeanConfig) {
    impl_::emit_link_config(config);
    impl_::emit_version_cfgs(config);
    impl_::emit_allocator_cfgs(config);
    impl_::emit_runtime_envs(config);
    println!("cargo:rerun-if-changed={}", config.lean_home.display());
}

/// Main entry point: adds all Leo3 configuration to the current compilation.
///
/// ## Environment Variables
///
/// - `LEO3_NO_LEAN=1` - Skip Lean4 detection and linking (for compile-only tests)
/// - `LEAN_HOME` - Override Lean4 installation directory
/// - `LEAN_LIB_DIR` - Override Lean4 library directory
/// - `LEAN_INCLUDE_DIR` - Override Lean4 include directory
/// - `LEO3_CONFIG_FILE` - Path to a pre-built config file
/// - `LEO3_CROSS_LIB_DIR` - Cross-compile: Lean library directory
/// - `LEO3_CROSS_LEAN_VERSION` - Cross-compile: Lean version string
/// - `DEP_LEAN4_LEO3_CONFIG` - Upstream propagated config from `leo3-ffi`
///
/// Resolution order:
/// 1. `DEP_LEAN4_LEO3_CONFIG`
/// 2. `LEO3_CONFIG_FILE`
/// 3. Host discovery (`LEO3_CROSS_*` → `LEAN_HOME` → `lake` → `elan` → `PATH`)
///
/// Explicit higher-priority inputs are authoritative: if they are present but
/// invalid, Leo3 reports that configuration error instead of silently falling
/// back to a lower-priority discovery source.
pub fn use_leo3_cfgs() {
    impl_::print_expected_cfgs();
    impl_::print_rerun_if_env_changed();

    if env::var("LEO3_NO_LEAN").is_ok() {
        cargo_warn!("LEO3_NO_LEAN set: skipping Lean4 detection and linking");
        cargo_warn!("Tests requiring Lean4 runtime will not run");
        impl_::emit_no_lean_dynamic_lookup_link_arg();
        return;
    }

    match get_lean_config_with_source() {
        Ok(resolved) => {
            impl_::emit_resolved_config_rerun_if_changed(&resolved);
            emit_resolved_config(&resolved.config);
        }
        Err(error) => {
            warn_resolution_error(&error);
            cargo_warn!("Leo3 will not function without Lean4 installed");
            cargo_warn!("Set LEO3_NO_LEAN=1 to build without Lean4 (compile-only)");
        }
    }
}

/// Like [`use_leo3_cfgs`], but only accepts config propagated from `leo3-ffi`.
///
/// This keeps `leo3` aligned with the exact Lean cfg flags chosen by the
/// upstream `links = "lean4"` crate during `cargo publish` verification.
pub fn use_upstream_leo3_cfgs() {
    impl_::print_expected_cfgs();
    impl_::print_rerun_if_env_changed();

    if env::var("LEO3_NO_LEAN").is_ok() {
        cargo_warn!("LEO3_NO_LEAN set: skipping Lean4 configuration");
        impl_::emit_no_lean_dynamic_lookup_link_arg();
        return;
    }

    match impl_::resolve_dep_lean_config() {
        Ok(resolved) => {
            impl_::emit_resolved_config_rerun_if_changed(&resolved);
            emit_resolved_config(&resolved.config)
        }
        Err(error) => {
            warn_resolution_error(&error);
            cargo_warn!("leo3 reuses the Lean4 config chosen by leo3-ffi to avoid cfg mismatches");
        }
    }
}

/// Resolve a `LeanConfig` with the following priority:
///
/// 1. `DEP_LEAN4_LEO3_CONFIG` env var (hex-encoded, set by `leo3-ffi` via `links = "lean4"`)
/// 2. `LEO3_CONFIG_FILE` env var (path to a key=value config file)
/// 3. Host detection (`LEO3_CROSS_*` → `LEAN_HOME` → lake → elan → PATH)
///
/// Explicit higher-priority inputs are authoritative.
fn get() -> Result<ResolvedLeanConfig, ResolutionError> {
    impl_::resolve_lean_config()
}

/// Detect Lean4 configuration (cached) with a typed error.
pub fn resolve_lean_config() -> Result<LeanConfig, ResolutionError> {
    get_lean_config_with_source().map(|resolved| resolved.config)
}

/// Detect Lean4 configuration (cached).
pub fn get_lean_config() -> Result<LeanConfig, ResolutionError> {
    resolve_lean_config()
}

/// Detect Lean4 configuration (cached) and stringify any resolution error.
///
/// Deprecated: prefer [`get_lean_config`] or [`get_lean_config_with_source`]
/// for structured [`ResolutionError`] diagnostics.
#[deprecated(note = "use get_lean_config() or get_lean_config_with_source() for typed errors")]
pub fn get_lean_config_string() -> Result<LeanConfig, String> {
    get_lean_config().map_err(|error| error.to_string())
}

/// Detect Lean4 configuration (cached), returning the resolution source.
pub fn get_lean_config_with_source() -> Result<ResolvedLeanConfig, ResolutionError> {
    if let Some(config) = LEAN_CONFIG.get() {
        return Ok(config.clone());
    }

    let config = get()?;
    let _ = LEAN_CONFIG.set(config.clone());
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::impl_::tests as impl_tests;

    #[test]
    fn get_lean_config_has_typed_error() {
        let _typed: fn() -> Result<LeanConfig, ResolutionError> = get_lean_config;
    }

    #[test]
    fn test_version_parsing() {
        // This test requires Lean to be installed
        // In CI, this might need to be conditional
    }

    #[test]
    fn test_warn_resolution_error_prints_lines() {
        let error = ResolutionError {
            summary: "boom".to_string(),
            attempts: vec![ResolutionAttempt {
                source: LeanConfigSource::Lake,
                error: "lake not found".to_string(),
            }],
            hints: vec!["hint one".to_string()],
        };
        warn_resolution_error(&error);
    }

    /// Exercises every branch of `use_leo3_cfgs` and the cached config
    /// accessors in one deterministic sequence:
    /// the `LEAN_CONFIG` `OnceLock` is only ever populated by this test, and
    /// only after the failure path has run, so the cache cannot change the
    /// outcome of the earlier phases.
    #[test]
    #[allow(deprecated)]
    fn test_config_resolution_full_flow() {
        impl_tests::with_temp_env(|| {
            // 1. Resolution failure: explicit config file that cannot be loaded.
            std::env::set_var("LEO3_CONFIG_FILE", "/definitely/not/here.txt");
            let err = get().unwrap_err();
            assert!(err
                .summary
                .contains("LEO3_CONFIG_FILE was set but could not be loaded"));

            // 2. LEO3_NO_LEAN short-circuit.
            std::env::set_var("LEO3_NO_LEAN", "1");
            use_leo3_cfgs();
            std::env::remove_var("LEO3_NO_LEAN");

            // 3. Successful resolution via config file, then caching.
            let config = impl_tests::sample_config("4.25.2");
            let path = impl_tests::write_temp_config(&config, "lib-config");
            std::env::set_var("LEO3_CONFIG_FILE", &path);
            use_leo3_cfgs();

            let resolved = get_lean_config_with_source().unwrap();
            assert_eq!(resolved.source, LeanConfigSource::ConfigFile);
            assert_eq!(resolved.config.version, config.version);

            // Cached: even after the env var is removed, the same value returns.
            std::env::remove_var("LEO3_CONFIG_FILE");
            let cached = get_lean_config_with_source().unwrap();
            assert_eq!(cached.config.version, config.version);

            assert!(resolve_lean_config().is_ok());
            assert!(get_lean_config().is_ok());
            assert!(get_lean_config_string().is_ok());

            std::fs::remove_file(&path).unwrap();
        });
    }

    #[test]
    fn test_use_upstream_leo3_cfgs() {
        impl_tests::with_temp_env(|| {
            // DEP not set → error path.
            use_upstream_leo3_cfgs();

            // DEP set → success path.
            let config = impl_tests::sample_config("4.25.2");
            std::env::set_var("DEP_LEAN4_LEO3_CONFIG", config.to_cargo_dep_env());
            use_upstream_leo3_cfgs();

            // LEO3_NO_LEAN → short-circuit path.
            std::env::set_var("LEO3_NO_LEAN", "1");
            use_upstream_leo3_cfgs();
        });
    }
}
