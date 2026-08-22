//! Core implementation for leo3-build-config.
//!
//! This module is usable from both `lib.rs` (`mod impl_;`) and
//! `build.rs` (`#[path = "src/impl_.rs"] mod impl_;`) — the PyO3 pattern.
//! It accesses the errors module via `super::errors`.

// Some items are only used from build.rs or lib.rs, not both.
#![allow(dead_code)]

use super::errors::{self, bail, ensure, Context};
use std::cmp::Ordering;
use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::str::FromStr;

/// Maximum minor version for `cargo:rustc-check-cfg` generation.
pub const CFG_MAX_MINOR: u32 = 40;

// ---------------------------------------------------------------------------
// LeanAllocator
// ---------------------------------------------------------------------------

/// Build-time allocator backend configured by `lean/config.h`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeanAllocator {
    /// Lean's page-based small-object allocator is enabled.
    Small,
    /// Lean routes small-object allocation through mimalloc.
    Mimalloc,
    /// Lean falls back to the generic malloc-based small-object path.
    System,
}

impl fmt::Display for LeanAllocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            LeanAllocator::Small => "small",
            LeanAllocator::Mimalloc => "mimalloc",
            LeanAllocator::System => "system",
        };
        f.write_str(name)
    }
}

impl FromStr for LeanAllocator {
    type Err = errors::Error;

    fn from_str(s: &str) -> errors::Result<Self> {
        match s {
            "small" => Ok(LeanAllocator::Small),
            "mimalloc" => Ok(LeanAllocator::Mimalloc),
            "system" => Ok(LeanAllocator::System),
            _ => Err(errors::Error::new(format!("invalid allocator '{}'", s))),
        }
    }
}

// ---------------------------------------------------------------------------
// LeanVersion
// ---------------------------------------------------------------------------

/// Parsed Lean version (e.g. 4.25.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeanVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl fmt::Display for LeanVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

impl FromStr for LeanVersion {
    type Err = errors::Error;

    fn from_str(s: &str) -> errors::Result<Self> {
        let parts: Vec<&str> = s.split('.').collect();
        ensure!(
            parts.len() >= 3,
            "invalid version '{}': expected major.minor.patch",
            s
        );
        let major = parts[0]
            .parse::<u32>()
            .map_err(|_| errors::Error::new(format!("invalid major version '{}'", parts[0])))?;
        let minor = parts[1]
            .parse::<u32>()
            .map_err(|_| errors::Error::new(format!("invalid minor version '{}'", parts[1])))?;
        // Patch may have a pre-release suffix (e.g. "0-nightly-2026-02-13-rev2")
        let patch_str = parts[2].split('-').next().unwrap_or(parts[2]);
        let patch = patch_str
            .parse::<u32>()
            .map_err(|_| errors::Error::new(format!("invalid patch version '{}'", parts[2])))?;
        Ok(LeanVersion {
            major,
            minor,
            patch,
        })
    }
}

impl Ord for LeanVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        self.major
            .cmp(&other.major)
            .then(self.minor.cmp(&other.minor))
            .then(self.patch.cmp(&other.patch))
    }
}

impl PartialOrd for LeanVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

// ---------------------------------------------------------------------------
// LeanConfig
// ---------------------------------------------------------------------------

/// Configuration for a detected Lean4 installation.
#[derive(Debug, Clone)]
pub struct LeanConfig {
    pub lean_home: PathBuf,
    pub lean_lib_dir: PathBuf,
    pub lean_include_dir: PathBuf,
    pub version: LeanVersion,
    pub shared: bool,
    pub allocator: LeanAllocator,
    pub pointer_width: Option<u32>,
}

/// Where a [`LeanConfig`] was resolved from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeanConfigSource {
    /// Cargo metadata exported by an upstream `links = "lean4"` crate.
    CargoDepEnv,
    /// A user-supplied key=value config file.
    ConfigFile,
    /// Cross-compilation environment variables.
    CrossCompileEnv,
    /// `LEAN_HOME` (plus optional `LEAN_LIB_DIR` / `LEAN_INCLUDE_DIR`).
    LeanHomeEnv,
    /// `lake env printenv LEAN_HOME`.
    Lake,
    /// `elan which lean`.
    Elan,
    /// `which lean` / `where.exe lean`.
    Path,
}

impl LeanConfigSource {
    fn label(self) -> &'static str {
        match self {
            LeanConfigSource::CargoDepEnv => "DEP_LEAN4_LEO3_CONFIG",
            LeanConfigSource::ConfigFile => "LEO3_CONFIG_FILE",
            LeanConfigSource::CrossCompileEnv => "LEO3_CROSS_LIB_DIR + LEO3_CROSS_LEAN_VERSION",
            LeanConfigSource::LeanHomeEnv => "LEAN_HOME",
            LeanConfigSource::Lake => "lake env printenv LEAN_HOME",
            LeanConfigSource::Elan => "elan which lean",
            LeanConfigSource::Path => "PATH (lean)",
        }
    }
}

impl fmt::Display for LeanConfigSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// A [`LeanConfig`] paired with the source that produced it.
#[derive(Debug, Clone)]
pub struct ResolvedLeanConfig {
    pub source: LeanConfigSource,
    pub config: LeanConfig,
}

/// One failed configuration attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionAttempt {
    pub source: LeanConfigSource,
    pub error: String,
}

/// Rich error describing the resolution policy and every failed attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionError {
    pub summary: String,
    pub attempts: Vec<ResolutionAttempt>,
    pub hints: Vec<String>,
}

impl ResolutionError {
    fn single(
        summary: impl Into<String>,
        source: LeanConfigSource,
        error: impl Into<String>,
        hints: Vec<String>,
    ) -> Self {
        Self {
            summary: summary.into(),
            attempts: vec![ResolutionAttempt {
                source,
                error: error.into(),
            }],
            hints,
        }
    }

    fn host_detection(attempts: Vec<ResolutionAttempt>) -> Self {
        Self {
            summary: "failed to detect Lean4 via host discovery".to_string(),
            attempts,
            hints: vec![
                "Host discovery order: LEO3_CROSS_* -> LEAN_HOME -> lake -> elan -> PATH"
                    .to_string(),
                "Set LEO3_CONFIG_FILE=/path/to/leo3-build-config.txt to bypass host discovery"
                    .to_string(),
                "Set LEO3_NO_LEAN=1 for compile-only builds that should not link Lean4".to_string(),
            ],
        }
    }

    pub fn warning_lines(&self) -> Vec<String> {
        let mut lines = vec![format!(
            "Failed to resolve Lean4 configuration: {}",
            self.summary
        )];
        for attempt in &self.attempts {
            lines.push(format!("  {}: {}", attempt.source, attempt.error));
        }
        lines.extend(self.hints.iter().map(|hint| format!("  hint: {}", hint)));
        lines
    }
}

impl fmt::Display for ResolutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, line) in self.warning_lines().iter().enumerate() {
            if index > 0 {
                writeln!(f)?;
            }
            write!(f, "{}", line)?;
        }
        Ok(())
    }
}

impl std::error::Error for ResolutionError {}

// ---------------------------------------------------------------------------
// CrossCompileConfig
// ---------------------------------------------------------------------------

/// Cross-compilation configuration read from environment variables.
pub struct CrossCompileConfig {
    pub lib_dir: PathBuf,
    pub version: LeanVersion,
}

impl CrossCompileConfig {
    /// Read cross-compile config from `LEO3_CROSS_LIB_DIR` and `LEO3_CROSS_LEAN_VERSION`.
    pub fn from_env() -> errors::Result<Self> {
        let lib_dir = env::var("LEO3_CROSS_LIB_DIR")
            .map_err(|_| errors::Error::new("LEO3_CROSS_LIB_DIR not set"))?;
        let version_str = env::var("LEO3_CROSS_LEAN_VERSION")
            .map_err(|_| errors::Error::new("LEO3_CROSS_LEAN_VERSION not set"))?;
        let version = version_str
            .parse::<LeanVersion>()
            .context("parsing LEO3_CROSS_LEAN_VERSION")?;
        Ok(CrossCompileConfig {
            lib_dir: PathBuf::from(lib_dir),
            version,
        })
    }

    /// Convert into a full `LeanConfig`.
    pub fn into_lean_config(self) -> errors::Result<LeanConfig> {
        let lib_dir = &self.lib_dir;
        ensure!(
            lib_dir.exists(),
            "LEO3_CROSS_LIB_DIR does not exist: {}",
            lib_dir.display()
        );
        // For cross-compile, lean_home and include_dir are best-effort
        let lean_home = lib_dir
            .parent()
            .and_then(|p| p.parent())
            .unwrap_or(lib_dir)
            .to_path_buf();
        let lean_include_dir = lean_home.join("include");
        let shared = has_shared_libs(lib_dir);
        let allocator = detect_allocator_from_include_dir_best_effort(&lean_include_dir);
        Ok(LeanConfig {
            lean_home,
            lean_lib_dir: self.lib_dir,
            lean_include_dir,
            version: self.version,
            shared,
            allocator,
            pointer_width: None,
        })
    }
}

fn resolved(source: LeanConfigSource, config: LeanConfig) -> ResolvedLeanConfig {
    ResolvedLeanConfig { source, config }
}

fn load_config_file(path: &Path) -> errors::Result<LeanConfig> {
    let file = std::fs::File::open(path)
        .with_context(|| format!("failed to open config file '{}'", path.display()))?;
    LeanConfig::from_reader(std::io::BufReader::new(file))
        .with_context(|| format!("failed to parse config file '{}'", path.display()))
}

fn format_command_failure(output: &Output) -> String {
    let mut details = vec![format!("exit status {}", output.status)];

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        details.push(format!("stderr: {}", stderr));
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        details.push(format!("stdout: {}", stdout));
    }

    details.join(", ")
}

fn run_command(command: &mut Command, description: &str) -> errors::Result<Output> {
    let output = command
        .output()
        .with_context(|| format!("failed to run {}", description))?;
    ensure!(
        output.status.success(),
        "{} failed: {}",
        description,
        format_command_failure(&output)
    );
    Ok(output)
}

fn run_command_stdout(command: &mut Command, description: &str) -> errors::Result<String> {
    let output = run_command(command, description)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn detect_from_cross_compile() -> errors::Result<LeanConfig> {
    CrossCompileConfig::from_env().and_then(CrossCompileConfig::into_lean_config)
}

fn has_explicit_cross_compile_config() -> bool {
    env::var_os("LEO3_CROSS_LIB_DIR").is_some() || env::var_os("LEO3_CROSS_LEAN_VERSION").is_some()
}

pub fn resolve_host_lean_config() -> Result<ResolvedLeanConfig, ResolutionError> {
    if has_explicit_cross_compile_config() {
        return detect_from_cross_compile()
            .map(|config| resolved(LeanConfigSource::CrossCompileEnv, config))
            .map_err(|error| {
                ResolutionError::single(
                    "LEO3_CROSS_* was set but could not be resolved",
                    LeanConfigSource::CrossCompileEnv,
                    error.to_string(),
                    vec![
                        "Unset LEO3_CROSS_LIB_DIR and LEO3_CROSS_LEAN_VERSION to fall back to host discovery".to_string(),
                        "Set LEO3_CONFIG_FILE=/path/to/leo3-build-config.txt to bypass host discovery entirely".to_string(),
                    ],
                )
            });
    }

    if env::var_os("LEAN_HOME").is_some() {
        return detect_from_env()
            .map(|config| resolved(LeanConfigSource::LeanHomeEnv, config))
            .map_err(|error| {
                ResolutionError::single(
                    "LEAN_HOME was set but could not be resolved",
                    LeanConfigSource::LeanHomeEnv,
                    error.to_string(),
                    vec![
                        "Unset LEAN_HOME to fall back to lake / elan / PATH discovery".to_string(),
                        "If you use LEAN_LIB_DIR or LEAN_INCLUDE_DIR, verify those override paths exist".to_string(),
                    ],
                )
            });
    }

    let mut attempts = Vec::new();

    for (source, detect) in [
        (
            LeanConfigSource::Lake,
            detect_from_lake as fn() -> errors::Result<LeanConfig>,
        ),
        (
            LeanConfigSource::Elan,
            detect_from_elan as fn() -> errors::Result<LeanConfig>,
        ),
        (
            LeanConfigSource::Path,
            detect_from_path as fn() -> errors::Result<LeanConfig>,
        ),
    ] {
        let result = detect();
        match result {
            Ok(config) => return Ok(resolved(source, config)),
            Err(error) => attempts.push(ResolutionAttempt {
                source,
                error: error.to_string(),
            }),
        }
    }

    Err(ResolutionError::host_detection(attempts))
}

pub fn resolve_user_or_detect_config() -> Result<ResolvedLeanConfig, ResolutionError> {
    if let Ok(path) = env::var("LEO3_CONFIG_FILE") {
        return load_config_file(Path::new(&path))
            .map(|config| resolved(LeanConfigSource::ConfigFile, config))
            .map_err(|error| {
                ResolutionError::single(
                    "LEO3_CONFIG_FILE was set but could not be loaded",
                    LeanConfigSource::ConfigFile,
                    error.to_string(),
                    vec![
                        format!("Unset LEO3_CONFIG_FILE to fall back to host discovery (current value: {})", path),
                        "Set LEO3_NO_LEAN=1 for compile-only builds that should not link Lean4".to_string(),
                    ],
                )
            });
    }

    resolve_host_lean_config()
}

pub fn resolve_dep_lean_config() -> Result<ResolvedLeanConfig, ResolutionError> {
    let hex = env::var("DEP_LEAN4_LEO3_CONFIG").map_err(|_| {
        ResolutionError::single(
            "leo3 expects leo3-ffi to export DEP_LEAN4_LEO3_CONFIG",
            LeanConfigSource::CargoDepEnv,
            "not set",
            vec![
                "Build leo3 together with leo3-ffi so Cargo can propagate the shared Lean4 config".to_string(),
                "If leo3-ffi could not detect Lean4, check its warnings or set LEO3_CONFIG_FILE / LEAN_HOME before building".to_string(),
                "Set LEO3_NO_LEAN=1 for compile-only builds that should not link Lean4".to_string(),
            ],
        )
    })?;

    LeanConfig::from_cargo_dep_env(&hex)
        .map(|config| resolved(LeanConfigSource::CargoDepEnv, config))
        .map_err(|error| {
            ResolutionError::single(
                "DEP_LEAN4_LEO3_CONFIG was present but invalid",
                LeanConfigSource::CargoDepEnv,
                error.to_string(),
                vec![
                    "Rebuild leo3-ffi so it can re-export a fresh Lean4 config".to_string(),
                    "If you set LEO3_CONFIG_FILE, set it for the leo3-ffi build so the propagated config stays in sync".to_string(),
                ],
            )
        })
}

pub fn resolve_lean_config() -> Result<ResolvedLeanConfig, ResolutionError> {
    if let Ok(hex) = env::var("DEP_LEAN4_LEO3_CONFIG") {
        return LeanConfig::from_cargo_dep_env(&hex)
            .map(|config| resolved(LeanConfigSource::CargoDepEnv, config))
            .map_err(|error| {
                ResolutionError::single(
                    "DEP_LEAN4_LEO3_CONFIG was present but invalid",
                    LeanConfigSource::CargoDepEnv,
                    error.to_string(),
                    vec![
                        "Rebuild the upstream links = \"lean4\" crate so it can export a fresh config".to_string(),
                        "Unset DEP_LEAN4_LEO3_CONFIG only if you intentionally want local discovery to run instead".to_string(),
                    ],
                )
            });
    }

    resolve_user_or_detect_config()
}

pub fn emit_resolved_config_rerun_if_changed(resolved: &ResolvedLeanConfig) {
    if matches!(resolved.source, LeanConfigSource::ConfigFile) {
        if let Ok(path) = env::var("LEO3_CONFIG_FILE") {
            println!("cargo:rerun-if-changed={}", path);
        }
    }
}

// ---------------------------------------------------------------------------
// Detection functions
// ---------------------------------------------------------------------------

/// Main entry point: detect Lean4 configuration.
///
/// Tries cross-compile config first, then env vars, lake, elan, PATH.
pub fn detect_lean_config() -> errors::Result<LeanConfig> {
    resolve_host_lean_config()
        .map(|resolved| resolved.config)
        .map_err(|error| errors::Error::new(error.to_string()))
}

/// Try to detect Lean from the `LEAN_HOME` environment variable.
fn detect_from_env() -> errors::Result<LeanConfig> {
    let lean_home = env::var("LEAN_HOME").map_err(|_| errors::Error::new("LEAN_HOME not set"))?;
    validate_lean_installation(&PathBuf::from(lean_home))
}

/// Try to detect Lean from the `lake` command.
fn detect_from_lake() -> errors::Result<LeanConfig> {
    let mut version_cmd = Command::new("lake");
    version_cmd.arg("--version");
    run_command(&mut version_cmd, "lake --version")?;

    let mut env_cmd = Command::new("lake");
    env_cmd.arg("env").arg("printenv").arg("LEAN_HOME");
    let lean_home = run_command_stdout(&mut env_cmd, "lake env printenv LEAN_HOME")?;
    if lean_home.is_empty() {
        bail!("lake did not provide LEAN_HOME");
    }
    validate_lean_installation(&PathBuf::from(lean_home))
}

/// Try to detect Lean from the `elan` toolchain manager.
fn detect_from_elan() -> errors::Result<LeanConfig> {
    let mut elan_cmd = Command::new("elan");
    elan_cmd.arg("which").arg("lean");
    let lean_path = run_command_stdout(&mut elan_cmd, "elan which lean")?;
    let lean_home = resolve_lean_home(Path::new(&lean_path))?;
    validate_lean_installation(&lean_home)
}

/// Try to detect Lean from PATH.
///
/// Uses `where.exe` on Windows, `which` on Unix.
fn detect_from_path() -> errors::Result<LeanConfig> {
    let (cmd, arg) = if cfg!(target_os = "windows") {
        ("where.exe", "lean")
    } else {
        ("which", "lean")
    };
    let mut path_cmd = Command::new(cmd);
    path_cmd.arg(arg);
    let output = run_command(&mut path_cmd, &format!("{} {}", cmd, arg))?;
    // `where.exe` may return multiple lines; take the first.
    let lean_path = String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    let lean_home = resolve_lean_home(Path::new(&lean_path))?;
    validate_lean_installation(&lean_home)
}

/// Resolve the real Lean home directory from a lean binary path.
///
/// Uses `lean --print-prefix` which works correctly even when the binary
/// is an elan proxy (e.g. `~/.elan/bin/lean`). Falls back to the
/// parent-of-parent heuristic if `--print-prefix` fails.
fn resolve_lean_home(lean_bin: &Path) -> errors::Result<PathBuf> {
    if let Ok(output) = Command::new(lean_bin).arg("--print-prefix").output() {
        if output.status.success() {
            let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !prefix.is_empty() {
                return Ok(PathBuf::from(prefix));
            }
        }
    }
    // Fallback: assume <lean_home>/bin/lean layout
    lean_bin
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.to_path_buf())
        .context("could not determine LEAN_HOME from binary path")
}

fn lean_binary_in_home(lean_home: &Path) -> errors::Result<PathBuf> {
    let bin_dir = lean_home.join("bin");
    let lean = bin_dir.join("lean");
    if lean.exists() {
        return Ok(lean);
    }

    let lean_exe = bin_dir.join("lean.exe");
    if lean_exe.exists() {
        return Ok(lean_exe);
    }

    bail!(
        "lean binary not found: {} or {}",
        lean.display(),
        lean_exe.display()
    );
}

/// Validate a Lean installation directory and build a `LeanConfig`.
///
/// Respects `LEAN_LIB_DIR` and `LEAN_INCLUDE_DIR` env var overrides.
fn validate_lean_installation(lean_home: &Path) -> errors::Result<LeanConfig> {
    ensure!(
        lean_home.exists(),
        "Lean home does not exist: {}",
        lean_home.display()
    );

    let lean_include_dir = match env::var("LEAN_INCLUDE_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => lean_home.join("include"),
    };
    let lean_lib_dir = match env::var("LEAN_LIB_DIR") {
        Ok(dir) => PathBuf::from(dir),
        Err(_) => lean_home.join("lib").join("lean"),
    };

    ensure!(
        lean_include_dir.exists(),
        "include directory not found: {}",
        lean_include_dir.display()
    );

    ensure!(
        lean_lib_dir.exists(),
        "library directory not found: {}",
        lean_lib_dir.display()
    );

    let lean_bin = lean_binary_in_home(lean_home)?;
    let version = get_lean_version(&lean_bin)?;
    let shared = has_shared_libs(&lean_lib_dir);
    let allocator = detect_allocator_from_include_dir(&lean_include_dir)?;

    Ok(LeanConfig {
        lean_home: lean_home.to_path_buf(),
        lean_lib_dir,
        lean_include_dir,
        version,
        shared,
        allocator,
        pointer_width: None,
    })
}

fn detect_allocator_from_include_dir(include_dir: &Path) -> errors::Result<LeanAllocator> {
    let config_h = include_dir.join("lean").join("config.h");
    let config = std::fs::read_to_string(&config_h)
        .context(format!("failed to read {}", config_h.display()))?;
    Ok(parse_allocator_from_config_h(&config))
}

fn detect_allocator_from_include_dir_best_effort(include_dir: &Path) -> LeanAllocator {
    detect_allocator_from_include_dir(include_dir).unwrap_or(LeanAllocator::System)
}

fn parse_allocator_from_config_h(config: &str) -> LeanAllocator {
    let mut small = false;
    let mut mimalloc = false;

    for line in config.lines() {
        let line = line.trim_start();
        if line.starts_with("#define LEAN_SMALL_ALLOCATOR") {
            small = true;
        }
        if line.starts_with("#define LEAN_MIMALLOC") {
            mimalloc = true;
        }
    }

    if small {
        LeanAllocator::Small
    } else if mimalloc {
        LeanAllocator::Mimalloc
    } else {
        LeanAllocator::System
    }
}

/// Run `lean --version` and parse the output into a `LeanVersion`.
fn get_lean_version(lean_bin: &Path) -> errors::Result<LeanVersion> {
    let mut version_cmd = Command::new(lean_bin);
    version_cmd.arg("--version");
    let output = run_command(
        &mut version_cmd,
        &format!("{} --version", lean_bin.display()),
    )?;
    parse_lean_version_output(&String::from_utf8_lossy(&output.stdout))
}

/// Parse the version out of `lean --version` output.
///
/// The output looks like `Lean (version 4.25.2, arm64-apple-darwin, commit
/// abc, release)`; the version token is the first whitespace-delimited word
/// that starts with `4` and contains a `.`.
fn parse_lean_version_output(version_output: &str) -> errors::Result<LeanVersion> {
    let version_str = version_output
        .split_whitespace()
        .find_map(|word| {
            let trimmed = word.trim_end_matches(',');
            if trimmed.starts_with('4') && trimmed.contains('.') {
                Some(trimmed.to_string())
            } else {
                None
            }
        })
        .context("could not parse Lean version from output")?;
    version_str.parse::<LeanVersion>()
}

/// Check whether the lib directory contains shared libraries.
fn has_shared_libs(lib_dir: &Path) -> bool {
    lib_dir.join("libleanshared.so").exists()
        || lib_dir.join("libleanshared.dylib").exists()
        || lib_dir.join("libleanshared.dll.a").exists()
        || lib_dir.join("leanshared.dll").exists()
}

// ---------------------------------------------------------------------------
// Serialization — key=value text format
// ---------------------------------------------------------------------------

impl LeanConfig {
    /// Serialize to a key=value text format.
    pub fn to_writer(&self, writer: &mut impl std::io::Write) -> std::io::Result<()> {
        writeln!(writer, "lean_home={}", self.lean_home.display())?;
        writeln!(writer, "lean_lib_dir={}", self.lean_lib_dir.display())?;
        writeln!(
            writer,
            "lean_include_dir={}",
            self.lean_include_dir.display()
        )?;
        writeln!(writer, "version={}", self.version)?;
        writeln!(writer, "shared={}", self.shared)?;
        writeln!(writer, "allocator={}", self.allocator)?;
        match self.pointer_width {
            Some(w) => writeln!(writer, "pointer_width={}", w)?,
            None => writeln!(writer, "pointer_width=")?,
        }
        Ok(())
    }

    /// Deserialize from a key=value text format.
    pub fn from_reader(reader: impl std::io::BufRead) -> errors::Result<Self> {
        let mut lean_home = None;
        let mut lean_lib_dir = None;
        let mut lean_include_dir = None;
        let mut version = None;
        let mut shared = None;
        let mut allocator = None;
        let mut pointer_width: Option<Option<u32>> = None;

        for line in reader.lines() {
            let line = line.map_err(|e| errors::Error::with_source("reading config", e))?;
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .context(format!("invalid config line: {}", line))?;
            match key {
                "lean_home" => lean_home = Some(PathBuf::from(value)),
                "lean_lib_dir" => lean_lib_dir = Some(PathBuf::from(value)),
                "lean_include_dir" => lean_include_dir = Some(PathBuf::from(value)),
                "version" => version = Some(value.parse::<LeanVersion>()?),
                "shared" => shared = Some(value == "true"),
                "allocator" => allocator = Some(value.parse::<LeanAllocator>()?),
                "pointer_width" => {
                    pointer_width = Some(if value.is_empty() {
                        None
                    } else {
                        Some(value.parse::<u32>().map_err(|_| {
                            errors::Error::new(format!("invalid pointer_width '{}'", value))
                        })?)
                    });
                }
                _ => {} // ignore unknown keys for forward compat
            }
        }

        let lean_include_dir = lean_include_dir.context("missing lean_include_dir")?;
        let allocator = allocator
            .unwrap_or_else(|| detect_allocator_from_include_dir_best_effort(&lean_include_dir));

        Ok(LeanConfig {
            lean_home: lean_home.context("missing lean_home")?,
            lean_lib_dir: lean_lib_dir.context("missing lean_lib_dir")?,
            lean_include_dir,
            version: version.context("missing version")?,
            shared: shared.context("missing shared")?,
            allocator,
            pointer_width: pointer_width.context("missing pointer_width")?,
        })
    }
}

// ---------------------------------------------------------------------------
// Hex serialization for Cargo DEP_* transport
// ---------------------------------------------------------------------------

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0xf) as usize] as char);
    }
    out
}

fn hex_decode(hex: &str) -> errors::Result<Vec<u8>> {
    ensure!(hex.len().is_multiple_of(2), "hex string has odd length");
    let mut out = Vec::with_capacity(hex.len() / 2);
    let bytes = hex.as_bytes();
    for i in (0..bytes.len()).step_by(2) {
        let hi = hex_nibble(bytes[i])?;
        let lo = hex_nibble(bytes[i + 1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn hex_nibble(b: u8) -> errors::Result<u8> {
    match b {
        b'0'..=b'9' => Ok(b - b'0'),
        b'a'..=b'f' => Ok(b - b'a' + 10),
        b'A'..=b'F' => Ok(b - b'A' + 10),
        _ => Err(errors::Error::new(format!(
            "invalid hex char '{}'",
            b as char
        ))),
    }
}

impl LeanConfig {
    /// Hex-encode the key=value representation for Cargo `DEP_*` env transport.
    pub fn to_cargo_dep_env(&self) -> String {
        let mut buf = Vec::new();
        // to_writer only fails on I/O; Vec<u8> never fails.
        self.to_writer(&mut buf).expect("write to Vec failed");
        hex_encode(&buf)
    }

    /// Decode a hex-encoded config previously produced by `to_cargo_dep_env`.
    pub fn from_cargo_dep_env(hex: &str) -> errors::Result<Self> {
        let bytes = hex_decode(hex)?;
        let cursor = std::io::Cursor::new(bytes);
        Self::from_reader(std::io::BufReader::new(cursor))
    }
}

// ---------------------------------------------------------------------------
// Build script helpers
// ---------------------------------------------------------------------------

/// Print `cargo:rustc-check-cfg` for all supported `lean_4_N` flags.
pub fn print_expected_cfgs() {
    for minor in 0..=CFG_MAX_MINOR {
        println!("cargo:rustc-check-cfg=cfg(lean_4_{})", minor);
    }
    println!("cargo:rustc-check-cfg=cfg(lean_small_allocator)");
    println!("cargo:rustc-check-cfg=cfg(lean_mimalloc)");
}

/// Print `cargo:rerun-if-env-changed` for every env var we inspect.
pub fn print_rerun_if_env_changed() {
    for var in &[
        "DEP_LEAN4_LEO3_CONFIG",
        "LEO3_NO_LEAN",
        "LEAN_HOME",
        "ELAN_HOME",
        "LEAN_LIB_DIR",
        "LEAN_INCLUDE_DIR",
        "LEO3_CONFIG_FILE",
        "LEO3_CROSS_LIB_DIR",
        "LEO3_CROSS_LEAN_VERSION",
        "PATH",
    ] {
        println!("cargo:rerun-if-env-changed={}", var);
    }
}

/// Emit `cargo:rustc-link-*` directives for linking against Lean.
/// Allow unresolved Lean runtime symbols in `LEO3_NO_LEAN=1` builds on Apple
/// targets.
///
/// Lake-integration cdylibs intentionally leave Lean symbols
/// (`lean_mk_string`, `lean_dec_ref_cold`, ...) undefined: the host Lean
/// executable provides them at load time. ELF linkers accept unresolved dylib
/// symbols by default, but Apple's linker rejects them at link time unless
/// `-undefined dynamic_lookup` is passed, so every macOS cdylib build for the
/// lake-integration workflow failed to link.
///
/// Build scripts cannot see the crate type being built (`CARGO_CRATE_TYPE`
/// is not set for them), so the flag is emitted for every no-lean Apple link.
/// For non-dynamic targets it is harmless unless symbols are genuinely
/// missing, in which case the diagnostic moves from link time to load time.
pub fn emit_no_lean_dynamic_lookup_link_arg() {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "macos" {
        println!("cargo:rustc-link-arg=-Wl,-undefined,dynamic_lookup");
    }
}

/// Fallback link artifact for a Lean DLL when the import libraries cannot
/// be regenerated from the live export tables (W-356): prefer the
/// dist-bundled import library (`<stem>.dll.a` under `lib/lean/`); when
/// that is missing, the DLL itself (MSVC `link.exe` generates an import
/// library for it, `lld-link` does not).
fn windows_lean_link_artifact(config: &LeanConfig, stem: &str) -> Option<String> {
    let import_lib = format!("{stem}.dll.a");
    if config.lean_lib_dir.join(&import_lib).exists() {
        return Some(import_lib);
    }
    let dll_name = format!("{stem}.dll");
    let bin_dir = config.lean_home.join("bin");
    for dir in [bin_dir, config.lean_lib_dir.clone()] {
        if dir.join(&dll_name).exists() {
            return Some(dll_name);
        }
    }
    None
}

pub fn emit_link_config(config: &LeanConfig) {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();

    println!(
        "cargo:rustc-link-search=native={}",
        config.lean_lib_dir.display()
    );

    if target_os == "windows" {
        // Official dists (and elan toolchains) keep the DLLs in `bin/`.
        let bin_dir = config.lean_home.join("bin");
        println!("cargo:rustc-link-search=native={}", bin_dir.display());
    }

    let has_init_shared = if target_os == "windows" {
        config.lean_lib_dir.join("libInit_shared.dll.a").exists()
    } else {
        config.lean_lib_dir.join("libInit_shared.so").exists()
            || config.lean_lib_dir.join("libInit_shared.dylib").exists()
    };

    let has_leanshared_2 = if target_os == "windows" {
        config.lean_lib_dir.join("libleanshared_2.dll.a").exists()
    } else {
        config.lean_lib_dir.join("libleanshared_2.so").exists()
            || config.lean_lib_dir.join("libleanshared_2.dylib").exists()
    };

    let has_leanshared_1 = if target_os == "windows" {
        config.lean_lib_dir.join("libleanshared_1.dll.a").exists()
    } else {
        config.lean_lib_dir.join("libleanshared_1.so").exists()
            || config.lean_lib_dir.join("libleanshared_1.dylib").exists()
    };

    if target_os == "windows" {
        // Preferred: regenerate the import libraries from the Lean DLLs'
        // live export tables, because the dist-bundled import libs can lag
        // the DLLs (W-356: Lean 4.33.0's bundled `libleanshared.dll.a`
        // chain misses `l_Lean_Elab_Tactic_tacticElabAttribute` → LNK2019).
        match crate::windows_lean_imports::regenerate(config) {
            Some(generated) => {
                println!(
                    "cargo:rustc-link-search=native={}",
                    generated.search_dir.display()
                );
                for (_, lib_name) in &generated.libs {
                    // Verbatim the generated import library's exact file
                    // name: build-script link libs only reach the final
                    // link through rlib metadata, and only verbatim names
                    // are resolved to a file path there (plain names are
                    // passed to the linker as `-l`, which rust-lld-based
                    // cross linking ignores). The name matches a file only
                    // in `search_dir`, so resolution is unambiguous.
                    println!("cargo:rustc-link-lib=dylib:+verbatim={lib_name}");
                }
            }
            None => {
                // Fallback: the dist-bundled import libraries. Optional
                // split-DLL companions are linked only when shipped.
                for stem in ["libInit_shared", "libleanshared_2", "libleanshared_1"] {
                    if let Some(artifact) = windows_lean_link_artifact(config, stem) {
                        println!("cargo:rustc-link-lib=dylib:+verbatim={artifact}");
                    }
                }
                // `libleanshared` is mandatory: emit the import lib
                // unconditionally when neither the DLL nor its import lib
                // is found, so the link fails with the usual "file not
                // found" diagnostic instead of silently skipping it.
                match windows_lean_link_artifact(config, "libleanshared") {
                    Some(artifact) => {
                        println!("cargo:rustc-link-lib=dylib:+verbatim={artifact}");
                    }
                    None => println!("cargo:rustc-link-lib=dylib:+verbatim=libleanshared.dll.a"),
                }
            }
        }
    } else {
        if has_init_shared {
            println!("cargo:rustc-link-lib=dylib=Init_shared");
        }
        if has_leanshared_2 {
            println!("cargo:rustc-link-lib=dylib=leanshared_2");
        }
        if has_leanshared_1 {
            println!("cargo:rustc-link-lib=dylib=leanshared_1");
        }
        println!("cargo:rustc-link-lib=dylib=leanshared");
    }

    if target_os == "windows" {
        println!("cargo:rustc-link-lib=dylib=Ws2_32");
        println!("cargo:rustc-link-lib=dylib=Bcrypt");
        println!("cargo:rustc-link-lib=dylib=Userenv");
    }

    if target_os != "windows" {
        println!(
            "cargo:rustc-link-arg=-Wl,-rpath,{}",
            config.lean_lib_dir.display()
        );
    }

    println!("cargo:include={}", config.lean_include_dir.display());
}

/// Emit the Lean library directory the current crate is being linked
/// against, baked into the compiled crate so the runtime can anchor its
/// module search path to the *same* toolchain.
///
/// The link step pins `libleanshared.so` via RPATH, but Lean's module
/// search path is resolved at runtime — by default through elan, which
/// follows the process cwd's `lean-toolchain` file. Under a
/// `LEO3_CROSS_LIB_DIR` cross build those disagree (the runtime reads
/// `.olean` files from a different Lean version and aborts with
/// "incompatible header"), so the runtime prefers this build-time value
/// over re-resolving via `lean --print-prefix`.
pub fn emit_runtime_envs(config: &LeanConfig) {
    println!(
        "cargo:rustc-env=LEO3_LEAN_LIB_DIR={}",
        config.lean_lib_dir.display()
    );
}

/// The `lean_4_N` minors to emit for a version (`0..=minor`), or `None` for
/// versions that are not Lean 4 (no cfgs are emitted).
fn lean_4_cfg_minors(version: &LeanVersion) -> Option<Vec<u32>> {
    if version.major != 4 {
        return None;
    }
    Some((0..=version.minor).collect())
}

/// Emit `cargo:rustc-cfg=lean_4_N` for the detected version and all earlier minors.
pub fn emit_version_cfgs(config: &LeanConfig) {
    let Some(minors) = lean_4_cfg_minors(&config.version) else {
        return;
    };
    for v in minors {
        println!("cargo:rustc-cfg=lean_4_{}", v);
    }
}

/// Emit allocator cfgs derived from `lean/config.h`.
pub fn emit_allocator_cfgs(config: &LeanConfig) {
    match config.allocator {
        LeanAllocator::Small => println!("cargo:rustc-cfg=lean_small_allocator"),
        LeanAllocator::Mimalloc => println!("cargo:rustc-cfg=lean_mimalloc"),
        LeanAllocator::System => {}
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::{LazyLock, Mutex};

    /// Process-wide lock serializing all env-var manipulation in this crate's
    /// tests (shared with the `lib.rs` test module, which also mutates the
    /// environment).
    pub(crate) fn env_lock() -> &'static Mutex<()> {
        static ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
        &ENV_LOCK
    }

    /// Run `f` with a clean environment: every env var the crate reads is
    /// removed first and restored afterwards.
    pub(crate) fn with_temp_env<T>(f: impl FnOnce() -> T) -> T {
        let _guard = env_lock().lock().unwrap();

        let keys = [
            "DEP_LEAN4_LEO3_CONFIG",
            "LEO3_CONFIG_FILE",
            "LEO3_CROSS_LIB_DIR",
            "LEO3_CROSS_LEAN_VERSION",
            "LEAN_HOME",
            "LEAN_LIB_DIR",
            "LEAN_INCLUDE_DIR",
            "LEO3_NO_LEAN",
            "CARGO_CFG_TARGET_OS",
            "PATH",
        ];

        let saved: Vec<_> = keys
            .iter()
            .map(|key| ((*key).to_string(), std::env::var(key).ok()))
            .collect();

        for key in keys {
            std::env::remove_var(key);
        }

        let result = f();

        for (key, value) in saved {
            if let Some(value) = value {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }

        result
    }

    pub(crate) fn write_temp_config(config: &LeanConfig, stem: &str) -> PathBuf {
        let unique = format!(
            "{}-{}-{}",
            stem,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(format!("{}.txt", unique));
        let mut file = std::fs::File::create(&path).unwrap();
        config.to_writer(&mut file).unwrap();
        path
    }

    pub(crate) fn sample_config(version: &str) -> LeanConfig {
        LeanConfig {
            lean_home: PathBuf::from("/opt/lean"),
            lean_lib_dir: PathBuf::from("/opt/lean/lib/lean"),
            lean_include_dir: PathBuf::from("/opt/lean/include"),
            version: version.parse().unwrap(),
            shared: true,
            allocator: LeanAllocator::Mimalloc,
            pointer_width: Some(64),
        }
    }

    /// Create a unique (nonexistent) temp directory path for a test.
    fn temp_unique_dir(stem: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "leo3-{}-{}-{}",
            stem,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn test_lean_version_parse() {
        let v: LeanVersion = "4.25.2".parse().unwrap();
        assert_eq!(v.major, 4);
        assert_eq!(v.minor, 25);
        assert_eq!(v.patch, 2);
        assert_eq!(v.to_string(), "4.25.2");

        // Pre-release suffix (nightly builds)
        let v: LeanVersion = "4.29.0-nightly-2026-02-13-rev2".parse().unwrap();
        assert_eq!(v.major, 4);
        assert_eq!(v.minor, 29);
        assert_eq!(v.patch, 0);

        assert!("abc".parse::<LeanVersion>().is_err());
        assert!("4".parse::<LeanVersion>().is_err());
        assert!("4.25".parse::<LeanVersion>().is_err());
    }

    #[test]
    fn test_lean_version_ord() {
        let v1: LeanVersion = "4.20.0".parse().unwrap();
        let v2: LeanVersion = "4.25.2".parse().unwrap();
        let v3: LeanVersion = "4.25.3".parse().unwrap();
        assert!(v1 < v2);
        assert!(v2 < v3);
        assert_eq!(v2, "4.25.2".parse().unwrap());
    }

    #[test]
    fn test_lean_config_serialization_roundtrip() {
        let config = LeanConfig {
            lean_home: PathBuf::from("/opt/lean"),
            lean_lib_dir: PathBuf::from("/opt/lean/lib/lean"),
            lean_include_dir: PathBuf::from("/opt/lean/include"),
            version: "4.25.2".parse().unwrap(),
            shared: true,
            allocator: LeanAllocator::Mimalloc,
            pointer_width: Some(64),
        };
        let mut buf = Vec::new();
        config.to_writer(&mut buf).unwrap();
        let restored =
            LeanConfig::from_reader(std::io::BufReader::new(std::io::Cursor::new(buf))).unwrap();
        assert_eq!(restored.lean_home, config.lean_home);
        assert_eq!(restored.lean_lib_dir, config.lean_lib_dir);
        assert_eq!(restored.lean_include_dir, config.lean_include_dir);
        assert_eq!(restored.version, config.version);
        assert_eq!(restored.shared, config.shared);
        assert_eq!(restored.pointer_width, config.pointer_width);
    }

    #[test]
    fn test_cargo_dep_env_roundtrip() {
        let config = LeanConfig {
            lean_home: PathBuf::from("/home/user/.elan/toolchains/leanprover-lean4-v4.25.2"),
            lean_lib_dir: PathBuf::from(
                "/home/user/.elan/toolchains/leanprover-lean4-v4.25.2/lib/lean",
            ),
            lean_include_dir: PathBuf::from(
                "/home/user/.elan/toolchains/leanprover-lean4-v4.25.2/include",
            ),
            version: "4.25.2".parse().unwrap(),
            shared: false,
            allocator: LeanAllocator::System,
            pointer_width: None,
        };
        let hex = config.to_cargo_dep_env();
        let restored = LeanConfig::from_cargo_dep_env(&hex).unwrap();
        assert_eq!(restored.lean_home, config.lean_home);
        assert_eq!(restored.version, config.version);
        assert_eq!(restored.shared, config.shared);
        assert_eq!(restored.allocator, config.allocator);
        assert_eq!(restored.pointer_width, config.pointer_width);
    }

    #[test]
    fn test_parse_allocator_from_config_h() {
        assert_eq!(
            parse_allocator_from_config_h("#define LEAN_MIMALLOC\n"),
            LeanAllocator::Mimalloc
        );
        assert_eq!(
            parse_allocator_from_config_h("#define LEAN_SMALL_ALLOCATOR\n"),
            LeanAllocator::Small
        );
        assert_eq!(
            parse_allocator_from_config_h("/* none */\n"),
            LeanAllocator::System
        );
    }

    #[test]
    fn test_print_expected_cfgs_count() {
        // Verify the loop would generate CFG_MAX_MINOR + 1 lines (0..=40 = 41).
        let count = (0..=CFG_MAX_MINOR).count();
        assert_eq!(count, 41);
    }

    #[test]
    fn test_hex_roundtrip() {
        let data = b"hello world\nversion=4.25.2\n";
        let encoded = hex_encode(data);
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_hex_decode_invalid() {
        assert!(hex_decode("0").is_err()); // odd length
        assert!(hex_decode("zz").is_err()); // invalid chars
    }

    #[test]
    fn test_resolution_error_warning_lines() {
        let error = ResolutionError::host_detection(vec![ResolutionAttempt {
            source: LeanConfigSource::LeanHomeEnv,
            error: "LEAN_HOME not set".to_string(),
        }]);

        let lines = error.warning_lines();
        assert!(lines[0].contains("Failed to resolve Lean4 configuration"));
        assert!(lines
            .iter()
            .any(|line| line.contains("LEAN_HOME: LEAN_HOME not set")));
        assert!(lines
            .iter()
            .any(|line| line.contains("Host discovery order")));
    }

    #[test]
    fn test_resolve_lean_config_prefers_dep_env() {
        with_temp_env(|| {
            let dep_config = sample_config("4.25.2");
            let file_config = sample_config("4.30.0");
            let file_path = write_temp_config(&file_config, "leo3-config-file");

            std::env::set_var("DEP_LEAN4_LEO3_CONFIG", dep_config.to_cargo_dep_env());
            std::env::set_var("LEO3_CONFIG_FILE", &file_path);

            let resolved = resolve_lean_config().unwrap();
            assert_eq!(resolved.source, LeanConfigSource::CargoDepEnv);
            assert_eq!(resolved.config.version, dep_config.version);

            std::fs::remove_file(file_path).unwrap();
        });
    }

    #[test]
    fn test_resolve_user_or_detect_config_prefers_config_file() {
        with_temp_env(|| {
            let file_config = sample_config("4.28.1");
            let file_path = write_temp_config(&file_config, "leo3-config-file-only");

            std::env::set_var("LEO3_CONFIG_FILE", &file_path);

            let resolved = resolve_user_or_detect_config().unwrap();
            assert_eq!(resolved.source, LeanConfigSource::ConfigFile);
            assert_eq!(resolved.config.version, file_config.version);

            std::fs::remove_file(file_path).unwrap();
        });
    }

    #[test]
    fn test_resolve_host_lean_config_stops_on_invalid_lean_home() {
        with_temp_env(|| {
            std::env::set_var("LEAN_HOME", "/definitely/not/here");

            let err = resolve_host_lean_config().unwrap_err();
            assert_eq!(err.attempts.len(), 1);
            assert_eq!(err.attempts[0].source, LeanConfigSource::LeanHomeEnv);
            assert!(err.attempts[0].error.contains("Lean home does not exist"));
        });
    }

    #[test]
    fn test_resolve_host_lean_config_stops_on_partial_cross_compile_env() {
        with_temp_env(|| {
            std::env::set_var("LEO3_CROSS_LIB_DIR", "/definitely/not/here");

            let err = resolve_host_lean_config().unwrap_err();
            assert_eq!(err.attempts.len(), 1);
            assert_eq!(err.attempts[0].source, LeanConfigSource::CrossCompileEnv);
            assert!(err.attempts[0]
                .error
                .contains("LEO3_CROSS_LEAN_VERSION not set"));
        });
    }

    #[test]
    fn test_validate_lean_installation_requires_existing_lib_dir() {
        with_temp_env(|| {
            let lean_home = std::env::temp_dir().join(format!(
                "leo3-invalid-lib-dir-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(lean_home.join("include")).unwrap();

            std::env::set_var("LEAN_HOME", &lean_home);
            std::env::set_var("LEAN_LIB_DIR", lean_home.join("missing-lib"));

            let err = resolve_host_lean_config().unwrap_err();
            assert_eq!(err.attempts.len(), 1);
            assert_eq!(err.attempts[0].source, LeanConfigSource::LeanHomeEnv);
            assert!(err.attempts[0]
                .error
                .contains("library directory not found"));

            std::fs::remove_dir_all(lean_home).unwrap();
        });
    }

    #[test]
    fn test_lean_binary_in_home_accepts_windows_exe_name() {
        let lean_home = std::env::temp_dir().join(format!(
            "leo3-lean-exe-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bin_dir = lean_home.join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::write(bin_dir.join("lean.exe"), b"").unwrap();

        let lean_bin = lean_binary_in_home(&lean_home).unwrap();
        assert_eq!(lean_bin, bin_dir.join("lean.exe"));

        std::fs::remove_dir_all(lean_home).unwrap();
    }

    #[test]
    fn test_lean_binary_in_home_finds_plain_lean() {
        let lean_home = std::env::temp_dir().join(format!(
            "leo3-lean-bin-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let bin_dir = lean_home.join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::write(bin_dir.join("lean"), b"").unwrap();

        let lean_bin = lean_binary_in_home(&lean_home).unwrap();
        assert_eq!(lean_bin, bin_dir.join("lean"));

        std::fs::remove_dir_all(lean_home).unwrap();
    }

    #[test]
    fn test_resolve_user_or_detect_config_falls_through_to_host_detection() {
        with_temp_env(|| {
            let empty_path = temp_unique_dir("empty-path-2");
            std::fs::create_dir_all(&empty_path).unwrap();
            std::env::set_var("PATH", &empty_path);

            // No LEO3_CONFIG_FILE: host discovery runs and fails on every step.
            let err = resolve_user_or_detect_config().unwrap_err();
            assert!(err.summary.contains("host discovery"));
            assert_eq!(err.attempts.len(), 3);

            std::fs::remove_dir_all(&empty_path).unwrap();
        });
    }

    #[test]
    fn test_lean_allocator_display_and_parse() {
        assert_eq!(LeanAllocator::Small.to_string(), "small");
        assert_eq!(LeanAllocator::Mimalloc.to_string(), "mimalloc");
        assert_eq!(LeanAllocator::System.to_string(), "system");
        assert_eq!(
            "small".parse::<LeanAllocator>().unwrap(),
            LeanAllocator::Small
        );
        assert_eq!(
            "mimalloc".parse::<LeanAllocator>().unwrap(),
            LeanAllocator::Mimalloc
        );
        assert_eq!(
            "system".parse::<LeanAllocator>().unwrap(),
            LeanAllocator::System
        );
        let err = "bogus".parse::<LeanAllocator>().unwrap_err();
        assert!(err.to_string().contains("invalid allocator 'bogus'"));
    }

    #[test]
    fn test_lean_config_source_labels() {
        assert_eq!(
            LeanConfigSource::CargoDepEnv.label(),
            "DEP_LEAN4_LEO3_CONFIG"
        );
        assert_eq!(LeanConfigSource::ConfigFile.label(), "LEO3_CONFIG_FILE");
        assert_eq!(
            LeanConfigSource::CrossCompileEnv.label(),
            "LEO3_CROSS_LIB_DIR + LEO3_CROSS_LEAN_VERSION"
        );
        assert_eq!(LeanConfigSource::LeanHomeEnv.label(), "LEAN_HOME");
        assert_eq!(
            LeanConfigSource::Lake.label(),
            "lake env printenv LEAN_HOME"
        );
        assert_eq!(LeanConfigSource::Elan.label(), "elan which lean");
        assert_eq!(LeanConfigSource::Path.label(), "PATH (lean)");
        // Display delegates to label().
        assert_eq!(
            LeanConfigSource::Lake.to_string(),
            "lake env printenv LEAN_HOME"
        );
        assert_eq!(LeanConfigSource::Path.to_string(), "PATH (lean)");
    }

    #[test]
    fn test_resolution_error_display_multiline() {
        let error = ResolutionError::host_detection(vec![
            ResolutionAttempt {
                source: LeanConfigSource::Lake,
                error: "lake not found".to_string(),
            },
            ResolutionAttempt {
                source: LeanConfigSource::Path,
                error: "which failed".to_string(),
            },
        ]);
        let text = error.to_string();
        assert!(text.starts_with("Failed to resolve Lean4 configuration"));
        assert!(text.contains("lake env printenv LEAN_HOME: lake not found"));
        assert!(text.contains("PATH (lean): which failed"));
        assert!(text.lines().count() >= 6);
    }

    #[test]
    fn test_cross_compile_from_env_missing() {
        with_temp_env(|| {
            let err = match CrossCompileConfig::from_env() {
                Ok(_) => panic!("expected error with no env vars set"),
                Err(err) => err,
            };
            assert_eq!(err.to_string(), "LEO3_CROSS_LIB_DIR not set");

            std::env::set_var("LEO3_CROSS_LIB_DIR", "/tmp/foo");
            let err = match CrossCompileConfig::from_env() {
                Ok(_) => panic!("expected error with LEO3_CROSS_LEAN_VERSION unset"),
                Err(err) => err,
            };
            assert_eq!(err.to_string(), "LEO3_CROSS_LEAN_VERSION not set");
        });
    }

    #[test]
    fn test_cross_compile_from_env_invalid_version() {
        with_temp_env(|| {
            std::env::set_var("LEO3_CROSS_LIB_DIR", "/tmp/foo");
            std::env::set_var("LEO3_CROSS_LEAN_VERSION", "not-a-version");
            let err = match CrossCompileConfig::from_env() {
                Ok(_) => panic!("expected parse error"),
                Err(err) => err,
            };
            assert!(err.to_string().contains("parsing LEO3_CROSS_LEAN_VERSION"));
        });
    }

    #[test]
    fn test_cross_compile_from_env_ok() {
        with_temp_env(|| {
            std::env::set_var("LEO3_CROSS_LIB_DIR", "/tmp/foo");
            std::env::set_var("LEO3_CROSS_LEAN_VERSION", "4.25.2");
            let config = CrossCompileConfig::from_env().unwrap();
            assert_eq!(config.lib_dir, PathBuf::from("/tmp/foo"));
            assert_eq!(config.version, "4.25.2".parse::<LeanVersion>().unwrap());
        });
    }

    #[test]
    fn test_cross_compile_into_lean_config() {
        let root = temp_unique_dir("cross-into");
        let lib_dir = root.join("a").join("b");
        std::fs::create_dir_all(&lib_dir).unwrap();

        let config = CrossCompileConfig {
            lib_dir: lib_dir.clone(),
            version: "4.25.2".parse().unwrap(),
        }
        .into_lean_config()
        .unwrap();
        // lean_home falls back to the grandparent of the lib dir.
        assert_eq!(config.lean_home, root);
        assert_eq!(config.lean_include_dir, root.join("include"));
        assert_eq!(config.lean_lib_dir, lib_dir);
        assert_eq!(config.version, "4.25.2".parse().unwrap());
        assert!(!config.shared);
        assert_eq!(config.allocator, LeanAllocator::System);
        assert_eq!(config.pointer_width, None);

        let missing = root.join("missing");
        let err = CrossCompileConfig {
            lib_dir: missing,
            version: "4.25.2".parse().unwrap(),
        }
        .into_lean_config()
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("LEO3_CROSS_LIB_DIR does not exist"));

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn test_resolve_host_lean_config_cross_compile_ok() {
        with_temp_env(|| {
            let root = temp_unique_dir("cross-ok");
            let lib_dir = root.join("lib").join("lean");
            std::fs::create_dir_all(&lib_dir).unwrap();
            std::env::set_var("LEO3_CROSS_LIB_DIR", &lib_dir);
            std::env::set_var("LEO3_CROSS_LEAN_VERSION", "4.25.2");

            let resolved = resolve_host_lean_config().unwrap();
            assert_eq!(resolved.source, LeanConfigSource::CrossCompileEnv);
            assert_eq!(resolved.config.version, "4.25.2".parse().unwrap());

            std::fs::remove_dir_all(&root).unwrap();
        });
    }

    #[test]
    fn test_resolve_host_lean_config_all_detectors_fail() {
        with_temp_env(|| {
            // Scrub PATH so lake / elan / which cannot be found: every
            // detection step fails and the attempts are collected.
            let empty_path = temp_unique_dir("empty-path");
            std::fs::create_dir_all(&empty_path).unwrap();
            std::env::set_var("PATH", &empty_path);

            let err = resolve_host_lean_config().unwrap_err();
            assert_eq!(err.attempts.len(), 3);
            assert_eq!(err.attempts[0].source, LeanConfigSource::Lake);
            assert_eq!(err.attempts[1].source, LeanConfigSource::Elan);
            assert_eq!(err.attempts[2].source, LeanConfigSource::Path);
            assert!(err.summary.contains("host discovery"));

            std::fs::remove_dir_all(&empty_path).unwrap();
        });
    }

    #[test]
    fn test_resolve_host_lean_config_missing_include_dir() {
        with_temp_env(|| {
            let lean_home = temp_unique_dir("lean-home-no-include");
            std::fs::create_dir_all(&lean_home).unwrap();

            std::env::set_var("LEAN_HOME", &lean_home);
            let err = resolve_host_lean_config().unwrap_err();
            assert_eq!(err.attempts[0].source, LeanConfigSource::LeanHomeEnv);
            assert!(err.attempts[0]
                .error
                .contains("include directory not found"));

            // LEAN_INCLUDE_DIR override pointing at an existing directory
            // passes the include check and fails on the library check instead.
            std::env::set_var("LEAN_INCLUDE_DIR", &lean_home);
            let err = resolve_host_lean_config().unwrap_err();
            assert!(err.attempts[0]
                .error
                .contains("library directory not found"));

            std::fs::remove_dir_all(&lean_home).unwrap();
        });
    }

    #[test]
    fn test_lean_binary_missing_error() {
        with_temp_env(|| {
            let lean_home = temp_unique_dir("lean-home-no-bin");
            std::fs::create_dir_all(lean_home.join("include")).unwrap();
            std::fs::create_dir_all(lean_home.join("lib").join("lean")).unwrap();

            std::env::set_var("LEAN_HOME", &lean_home);
            let err = resolve_host_lean_config().unwrap_err();
            assert!(err.attempts[0].error.contains("lean binary not found"));

            std::fs::remove_dir_all(&lean_home).unwrap();
        });
    }

    #[test]
    fn test_resolve_lean_home_fallback_heuristic() {
        // No executable at the path: `--print-prefix` fails and the
        // parent-of-parent heuristic kicks in.
        let home = resolve_lean_home(Path::new(
            "/nonexistent/toolchains/leanprover-lean4-v4.25.2/bin/lean",
        ))
        .unwrap();
        assert_eq!(
            home,
            PathBuf::from("/nonexistent/toolchains/leanprover-lean4-v4.25.2")
        );

        // A bare file name has no parent-of-parent: hard error. Scrub PATH
        // first so a `lean` on PATH cannot resolve and defeat the error path.
        with_temp_env(|| {
            let empty_path = temp_unique_dir("empty-path");
            std::fs::create_dir_all(&empty_path).unwrap();
            std::env::set_var("PATH", &empty_path);

            let err = resolve_lean_home(Path::new("lean")).unwrap_err();
            assert!(err
                .to_string()
                .contains("could not determine LEAN_HOME from binary path"));
        });
    }

    #[test]
    fn test_parse_lean_version_output() {
        assert_eq!(
            parse_lean_version_output(
                "Lean (version 4.25.2, x86_64-unknown-linux-gnu, commit abc, release)"
            )
            .unwrap(),
            "4.25.2".parse().unwrap()
        );
        // Trailing comma on the bare version token.
        assert_eq!(
            parse_lean_version_output("Lean (version 4.12.0, ...)").unwrap(),
            "4.12.0".parse().unwrap()
        );
        // Pre-release suffix survives.
        assert_eq!(
            parse_lean_version_output("Lean (version 4.29.0-nightly-2026-02-13-rev2, ...)")
                .unwrap(),
            "4.29.0".parse().unwrap()
        );
        // Plain version string also parses.
        assert_eq!(
            parse_lean_version_output("4.25.2\n").unwrap(),
            "4.25.2".parse().unwrap()
        );
        // Nothing looking like a Lean 4 version.
        assert!(parse_lean_version_output("no version here").is_err());
        assert!(parse_lean_version_output("Lean (version 5.0.0, ...)").is_err());
    }

    #[test]
    fn test_format_command_failure() {
        use std::process::Output;

        let output = Output {
            status: std::process::ExitStatus::default(),
            stdout: b"some stdout".to_vec(),
            stderr: b"some stderr".to_vec(),
        };
        let text = format_command_failure(&output);
        assert!(text.contains("some stderr"));
        assert!(text.contains("some stdout"));

        let empty = Output {
            status: std::process::ExitStatus::default(),
            stdout: Vec::new(),
            stderr: Vec::new(),
        };
        let text = format_command_failure(&empty);
        assert!(text.contains("exit status"));
        assert!(!text.contains("stderr:"));
    }

    #[test]
    fn test_run_command_spawn_error() {
        let mut cmd = Command::new("leo3-test-command-that-does-not-exist-xyz");
        let err = run_command(&mut cmd, "definitely missing").unwrap_err();
        assert!(err.to_string().contains("failed to run definitely missing"));
    }

    #[test]
    fn test_load_config_file() {
        let config = sample_config("4.25.2");
        let path = write_temp_config(&config, "load-config");
        let loaded = load_config_file(&path).unwrap();
        assert_eq!(loaded.version, config.version);

        let bad = temp_unique_dir("load-config-bad");
        std::fs::create_dir_all(&bad).unwrap();
        let bad_path = bad.join("config.txt");
        std::fs::write(&bad_path, "this is not key=value\n").unwrap();
        let err = load_config_file(&bad_path).unwrap_err();
        assert!(err.to_string().contains("failed to parse config file"));

        let err = load_config_file(&bad.join("missing.txt")).unwrap_err();
        assert!(err.to_string().contains("failed to open config file"));

        std::fs::remove_file(&path).unwrap();
        std::fs::remove_dir_all(&bad).unwrap();
    }

    #[test]
    fn test_from_reader_missing_keys() {
        let base = "\
lean_home=/opt/lean
lean_lib_dir=/opt/lean/lib/lean
lean_include_dir=/opt/lean/include
version=4.25.2
shared=true
allocator=mimalloc
pointer_width=64
";
        // Removing any of these keys is a hard error naming the key.
        for key in [
            "lean_home",
            "lean_lib_dir",
            "lean_include_dir",
            "version",
            "shared",
            "pointer_width",
        ] {
            let content = base
                .lines()
                .filter(|l| !l.starts_with(key))
                .collect::<Vec<_>>()
                .join("\n");
            let err =
                LeanConfig::from_reader(std::io::BufReader::new(content.as_bytes())).unwrap_err();
            assert!(
                err.to_string().contains(&format!("missing {}", key)),
                "expected 'missing {}' in: {}",
                key,
                err
            );
        }
    }

    #[test]
    fn test_from_reader_invalid_content() {
        // Line without '='
        let err = LeanConfig::from_reader(std::io::BufReader::new("garbage line\n".as_bytes()))
            .unwrap_err();
        assert!(err.to_string().contains("invalid config line"));

        // Invalid version value
        let err = LeanConfig::from_reader(std::io::BufReader::new("version=abc\n".as_bytes()))
            .unwrap_err();
        assert!(err.to_string().contains("invalid version"));

        // Invalid allocator value
        let err = LeanConfig::from_reader(std::io::BufReader::new("allocator=nope\n".as_bytes()))
            .unwrap_err();
        assert!(err.to_string().contains("invalid allocator"));

        // Invalid pointer_width value
        let err =
            LeanConfig::from_reader(std::io::BufReader::new("pointer_width=wide\n".as_bytes()))
                .unwrap_err();
        assert!(err.to_string().contains("invalid pointer_width"));
    }

    #[test]
    fn test_from_reader_unknown_keys_and_blank_lines() {
        let content = "\
lean_home=/opt/lean

lean_lib_dir=/opt/lean/lib/lean

unknown_key=whatever
lean_include_dir=/opt/lean/include
version=4.25.2
shared=true
allocator=mimalloc
pointer_width=
";
        let config = LeanConfig::from_reader(std::io::BufReader::new(content.as_bytes())).unwrap();
        assert_eq!(config.pointer_width, None);
        assert_eq!(config.version, "4.25.2".parse().unwrap());
    }

    #[test]
    fn test_from_reader_allocator_fallback_from_config_h() {
        let root = temp_unique_dir("alloc-fallback");
        let include = root.join("include").join("lean");
        std::fs::create_dir_all(&include).unwrap();
        std::fs::write(include.join("config.h"), "#define LEAN_SMALL_ALLOCATOR\n").unwrap();

        let content = format!(
            "lean_home={0}\nlean_lib_dir={0}/lib/lean\nlean_include_dir={0}/include\nversion=4.25.2\nshared=true\npointer_width=64\n",
            root.display()
        );
        let config = LeanConfig::from_reader(std::io::BufReader::new(content.as_bytes())).unwrap();
        assert_eq!(config.allocator, LeanAllocator::Small);

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn test_has_shared_libs() {
        let root = temp_unique_dir("shared-libs");
        std::fs::create_dir_all(&root).unwrap();
        assert!(!has_shared_libs(&root));

        for name in [
            "libleanshared.so",
            "libleanshared.dylib",
            "libleanshared.dll.a",
            "leanshared.dll",
        ] {
            let path = root.join(name);
            std::fs::write(&path, b"").unwrap();
            assert!(has_shared_libs(&root), "expected {name} to count");
            std::fs::remove_file(&path).unwrap();
        }

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn test_detect_allocator_from_include_dir() {
        let root = temp_unique_dir("alloc-detect");
        let include = root.join("include").join("lean");
        std::fs::create_dir_all(&include).unwrap();
        let config_h = include.join("config.h");

        std::fs::write(&config_h, "#define LEAN_MIMALLOC\n").unwrap();
        assert_eq!(
            detect_allocator_from_include_dir(&root.join("include")).unwrap(),
            LeanAllocator::Mimalloc
        );

        std::fs::write(
            &config_h,
            "#define LEAN_SMALL_ALLOCATOR\n#define LEAN_MIMALLOC\n",
        )
        .unwrap();
        assert_eq!(
            detect_allocator_from_include_dir(&root.join("include")).unwrap(),
            LeanAllocator::Small
        );

        // Missing config.h → hard error; best-effort wrapper falls back to System.
        std::fs::remove_file(&config_h).unwrap();
        assert!(detect_allocator_from_include_dir(&root.join("include")).is_err());
        assert_eq!(
            detect_allocator_from_include_dir_best_effort(&root.join("include")),
            LeanAllocator::System
        );

        std::fs::write(&config_h, "#define LEAN_MIMALLOC\n").unwrap();
        assert_eq!(
            detect_allocator_from_include_dir_best_effort(&root.join("include")),
            LeanAllocator::Mimalloc
        );

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn test_lean_4_cfg_minors() {
        assert_eq!(
            lean_4_cfg_minors(&"4.25.2".parse().unwrap()),
            Some((0..=25).collect::<Vec<_>>())
        );
        assert_eq!(lean_4_cfg_minors(&"4.0.0".parse().unwrap()), Some(vec![0]));
        assert_eq!(lean_4_cfg_minors(&"3.18.0".parse().unwrap()), None);
        assert_eq!(lean_4_cfg_minors(&"5.0.0".parse().unwrap()), None);
    }

    #[test]
    fn test_emit_cfg_helpers() {
        // Non-Lean-4 versions emit nothing (early return).
        emit_version_cfgs(&sample_config("3.18.0"));
        // Lean 4 emits one cfg per earlier minor.
        emit_version_cfgs(&sample_config("4.25.2"));

        emit_allocator_cfgs(&LeanConfig {
            allocator: LeanAllocator::Small,
            ..sample_config("4.25.2")
        });
        emit_allocator_cfgs(&LeanConfig {
            allocator: LeanAllocator::Mimalloc,
            ..sample_config("4.25.2")
        });
        emit_allocator_cfgs(&LeanConfig {
            allocator: LeanAllocator::System,
            ..sample_config("4.25.2")
        });

        print_expected_cfgs();
        print_rerun_if_env_changed();
    }

    #[test]
    fn test_emit_no_lean_dynamic_lookup_link_arg() {
        with_temp_env(|| {
            std::env::set_var("CARGO_CFG_TARGET_OS", "macos");
            emit_no_lean_dynamic_lookup_link_arg();
            std::env::set_var("CARGO_CFG_TARGET_OS", "linux");
            emit_no_lean_dynamic_lookup_link_arg();
        });
    }

    fn config_with_lib_dir(lib_dir: &Path, version: &str) -> LeanConfig {
        LeanConfig {
            lean_home: lib_dir
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_default(),
            lean_lib_dir: lib_dir.to_path_buf(),
            lean_include_dir: lib_dir
                .parent()
                .map(|p| p.join("include"))
                .unwrap_or_default(),
            version: version.parse().unwrap(),
            shared: true,
            allocator: LeanAllocator::System,
            pointer_width: Some(64),
        }
    }

    #[test]
    fn test_emit_link_config_unix() {
        with_temp_env(|| {
            std::env::remove_var("CARGO_CFG_TARGET_OS");
            let root = temp_unique_dir("link-unix");
            let lib_dir = root.join("lib");
            std::fs::create_dir_all(&lib_dir).unwrap();
            for name in [
                "libInit_shared.so",
                "libleanshared_2.so",
                "libleanshared_1.so",
            ] {
                std::fs::write(lib_dir.join(name), b"").unwrap();
            }
            emit_link_config(&config_with_lib_dir(&lib_dir, "4.25.2"));
            std::fs::remove_dir_all(&root).unwrap();
        });
    }

    #[test]
    fn test_emit_link_config_windows() {
        with_temp_env(|| {
            std::env::set_var("CARGO_CFG_TARGET_OS", "windows");
            let root = temp_unique_dir("link-win");
            let lib_dir = root.join("lib");
            std::fs::create_dir_all(&lib_dir).unwrap();
            for name in [
                "libInit_shared.dll.a",
                "libleanshared_2.dll.a",
                "libleanshared_1.dll.a",
            ] {
                std::fs::write(lib_dir.join(name), b"").unwrap();
            }
            std::fs::create_dir_all(root.join("bin")).unwrap();
            emit_link_config(&config_with_lib_dir(&lib_dir, "4.25.2"));
            std::fs::remove_dir_all(&root).unwrap();
        });
    }

    #[test]
    fn test_resolve_user_or_detect_config_config_file_error() {
        with_temp_env(|| {
            std::env::set_var("LEO3_CONFIG_FILE", "/definitely/not/a/config.txt");
            let err = resolve_user_or_detect_config().unwrap_err();
            assert_eq!(err.attempts.len(), 1);
            assert_eq!(err.attempts[0].source, LeanConfigSource::ConfigFile);
            assert!(err.attempts[0].error.contains("failed to open config file"));
        });
    }

    #[test]
    fn test_resolve_dep_lean_config_paths() {
        with_temp_env(|| {
            // Not set → error with CargoDepEnv source.
            let err = resolve_dep_lean_config().unwrap_err();
            assert_eq!(err.attempts[0].source, LeanConfigSource::CargoDepEnv);
            assert!(err.attempts[0].error.contains("not set"));

            // Invalid hex → "present but invalid".
            std::env::set_var("DEP_LEAN4_LEO3_CONFIG", "zz-not-hex");
            let err = resolve_dep_lean_config().unwrap_err();
            assert!(err.summary.contains("present but invalid"));

            // Valid config → Ok.
            let config = sample_config("4.25.2");
            std::env::set_var("DEP_LEAN4_LEO3_CONFIG", config.to_cargo_dep_env());
            let resolved = resolve_dep_lean_config().unwrap();
            assert_eq!(resolved.source, LeanConfigSource::CargoDepEnv);
            assert_eq!(resolved.config.version, config.version);
        });
    }

    #[test]
    fn test_resolve_lean_config_invalid_dep_env() {
        with_temp_env(|| {
            std::env::set_var("DEP_LEAN4_LEO3_CONFIG", "zz");
            let err = resolve_lean_config().unwrap_err();
            assert!(err.summary.contains("present but invalid"));
        });
    }

    #[test]
    fn test_emit_resolved_config_rerun_if_changed() {
        with_temp_env(|| {
            let config_file = temp_unique_dir("rerun").join("config.txt");
            let resolved = ResolvedLeanConfig {
                source: LeanConfigSource::ConfigFile,
                config: sample_config("4.25.2"),
            };
            std::env::set_var("LEO3_CONFIG_FILE", &config_file);
            emit_resolved_config_rerun_if_changed(&resolved);

            let dep = ResolvedLeanConfig {
                source: LeanConfigSource::CargoDepEnv,
                config: sample_config("4.25.2"),
            };
            emit_resolved_config_rerun_if_changed(&dep);
        });
    }

    #[test]
    fn test_detect_lean_config() {
        with_temp_env(|| {
            // Ok path via cross-compile env.
            let root = temp_unique_dir("detect-ok");
            let lib_dir = root.join("lib").join("lean");
            std::fs::create_dir_all(&lib_dir).unwrap();
            std::env::set_var("LEO3_CROSS_LIB_DIR", &lib_dir);
            std::env::set_var("LEO3_CROSS_LEAN_VERSION", "4.25.2");
            let config = detect_lean_config().unwrap();
            assert_eq!(config.version, "4.25.2".parse().unwrap());
            std::fs::remove_dir_all(&root).unwrap();

            // Error path maps to a plain errors::Error.
            std::env::remove_var("LEO3_CROSS_LIB_DIR");
            std::env::remove_var("LEO3_CROSS_LEAN_VERSION");
            std::env::set_var("LEAN_HOME", "/definitely/not/here");
            let err = detect_lean_config().unwrap_err();
            assert!(err.to_string().contains("Lean home does not exist"));
        });
    }
}
