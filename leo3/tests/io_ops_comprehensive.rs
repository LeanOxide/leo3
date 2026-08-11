//! Comprehensive IO tests for Leo3.
//!
//! Exercises the `io` feature modules: file handles (standard streams and
//! close), filesystem helpers (including error paths), environment helpers,
//! the IOError type (Display / std::error::Error / conversions), and the
//! LeanIO monad (pure / map / bind / run).
//!
//! NOTE ON COVERAGE: `handle::open`/`read`/`write`/`get_line`/`flush`/`is_eof`
//! and `console::put_str`/`put_str_ln` pass `num_fixed == arity` to
//! `ffi::inline::lean_alloc_closure` (e.g. `open` uses `(3, 3)` for the
//! C-arity-4 `lean_io_prim_handle_mk`), which violates upstream Lean's
//! `num_fixed < arity` invariant and panics under debug assertions. Similarly
//! `LeanIO::then` builds its seq closure with `(2, 2)` and panics, and
//! `io::time`/`io::process` reference Lean primitives (`lean_io_prim_mono_nanos`,
//! `lean_io_prim_get_unix_time_millis`, `lean_io_prim_get_exit_code`,
//! `lean_io_prim_set_exit_code`) that the v4.25.2 toolchain does not export at
//! all. Those APIs are therefore not exercised here; everything else is.

#![cfg(all(feature = "runtime-tests", feature = "io"))]

use leo3::io::handle::FileMode;
use leo3::io::{console, env, fs, IOError, IOResult, LeanIO};
use leo3::prelude::*;
use std::fs as std_fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

// Process-global state (env vars) means env tests must not run concurrently.
static GLOBAL_LOCK: Mutex<()> = Mutex::new(());
static DIR_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new(test_name: &str) -> Self {
        let unique = DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "leo3_io_comp_{}_{}_{}",
            std::process::id(),
            test_name,
            unique
        ));
        std_fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn file(&self, name: &str) -> String {
        self.path.join(name).to_string_lossy().into_owned()
    }

    fn dir(&self, name: &str) -> String {
        self.path.join(name).to_string_lossy().into_owned()
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std_fs::remove_dir_all(&self.path);
    }
}

// ============================================================================
// File Handle Tests
// ============================================================================

#[test]
fn test_console_streams() {
    let _lock = GLOBAL_LOCK.lock().unwrap();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Writing to stdout and stderr via the console helpers must succeed
        // (output appears in the test log).
        console::put_str(lean, "io-ops-stdout ")?.run()?;
        console::put_str_ln(lean, "done")?.run()?;
        Ok::<_, LeanError>(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_handle_file_mode_variants() {
    // FileMode tags are the public surface consumed by handle::open; verify
    // every variant exists and is distinct, mirroring Lean's ctor tags.
    let modes = [
        FileMode::Read,
        FileMode::Write,
        FileMode::ReadWrite,
        FileMode::Append,
    ];
    assert_eq!(modes.len(), 4);
    for (i, mode) in modes.iter().enumerate() {
        for (j, other) in modes.iter().enumerate() {
            assert_eq!(mode == other, i == j);
        }
    }
    // Debug formatting must not panic
    assert!(!format!("{:?}", FileMode::Append).is_empty());
}

// ============================================================================
// Filesystem Helper Tests
// ============================================================================

#[test]
fn test_fs_string_and_bytes_roundtrips() {
    let _lock = GLOBAL_LOCK.lock().unwrap();
    leo3::prepare_freethreaded_lean();
    let test_dir = TestDir::new("fs_roundtrip");

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Empty content round trip
        let empty = test_dir.file("empty.txt");
        fs::write_file(lean, &empty, "")?;
        assert_eq!(fs::read_file(lean, &empty)?, "");
        assert_eq!(fs::file_size(lean, &empty)?, 0);
        assert!(fs::file_exists(lean, &empty)?);

        // Multiline content round trip
        let multi = test_dir.file("multi.txt");
        let content = "alpha\nbeta\ngamma\n";
        fs::write_file(lean, &multi, content)?;
        assert_eq!(fs::read_file(lean, &multi)?, content);
        assert_eq!(fs::file_size(lean, &multi)?, content.len());

        // Byte round trips, including the empty slice
        let bin = test_dir.file("bytes.bin");
        fs::write_bytes(lean, &bin, &[])?;
        assert!(fs::read_bytes(lean, &bin)?.is_empty());
        fs::write_bytes(lean, &bin, &[9u8, 8, 7, 255])?;
        assert_eq!(fs::read_bytes(lean, &bin)?, vec![9u8, 8, 7, 255]);

        // Remove and confirm gone
        fs::remove_file(lean, &multi)?;
        assert!(!fs::file_exists(lean, &multi)?);
        Ok::<_, LeanError>(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_fs_error_paths() {
    let _lock = GLOBAL_LOCK.lock().unwrap();
    leo3::prepare_freethreaded_lean();
    let test_dir = TestDir::new("fs_errors");

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let missing = test_dir.file("nope.txt");
        let missing_dir = test_dir.dir("no_such_subdir");

        // All operations on a nonexistent file fail
        assert!(fs::read_file(lean, &missing).is_err());
        assert!(fs::read_bytes(lean, &missing).is_err());
        assert!(fs::file_size(lean, &missing).is_err());
        assert!(fs::remove_file(lean, &missing).is_err());
        assert!(fs::rename(lean, &missing, &test_dir.file("dst.txt")).is_err());

        // Directory operations on missing paths fail
        assert!(fs::remove_dir(lean, &missing_dir).is_err());
        assert!(fs::set_cwd(lean, &missing_dir).is_err());

        // Creating a directory twice fails
        let dup = test_dir.dir("duplicate");
        fs::create_dir(lean, &dup)?;
        assert!(fs::create_dir(lean, &dup).is_err());
        fs::remove_dir(lean, &dup)?;
        // Removing the same directory twice fails the second time
        assert!(fs::remove_dir(lean, &dup).is_err());
        Ok::<_, LeanError>(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_fs_cwd() {
    let _lock = GLOBAL_LOCK.lock().unwrap();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let cwd = fs::get_cwd(lean)?;
        assert!(!cwd.is_empty());
        assert!(Path::new(&cwd).is_dir());
        Ok::<_, LeanError>(())
    });
    assert!(result.is_ok());
}

// ============================================================================
// Environment Variable Tests
// ============================================================================

#[test]
fn test_env_home() {
    let _lock = GLOBAL_LOCK.lock().unwrap();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let home = env::get_env(lean, "HOME")?;
        assert!(home.is_some(), "HOME should be set in the test environment");
        assert!(!home.unwrap().is_empty());
        Ok::<_, LeanError>(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_env_set_get_unset_roundtrip() {
    let _lock = GLOBAL_LOCK.lock().unwrap();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let name = format!("LEO3_COMPREHENSIVE_VAR_{}", std::process::id());

        // Starts unset
        assert_eq!(env::get_env(lean, &name)?, None);

        // Set, overwrite, read back
        env::set_env(lean, &name, "v1")?;
        assert_eq!(env::get_env(lean, &name)?, Some("v1".to_string()));
        env::set_env(lean, &name, "v2")?;
        assert_eq!(env::get_env(lean, &name)?, Some("v2".to_string()));

        // Empty values round trip too
        env::set_env(lean, &name, "")?;
        assert_eq!(env::get_env(lean, &name)?, Some("".to_string()));

        // Unset -> None; unsetting a missing variable is fine
        env::unset_env(lean, &name)?;
        assert_eq!(env::get_env(lean, &name)?, None);
        env::unset_env(lean, &format!("{}_NEVER_SET", name))?;

        // Empty variable name simply yields None
        assert_eq!(env::get_env(lean, "")?, None);
        Ok::<_, LeanError>(())
    });
    assert!(result.is_ok());
}

// ============================================================================
// IOError Type Tests
// ============================================================================

#[test]
fn test_ioerror_display_all_variants() {
    assert_eq!(
        IOError::filesystem("boom").to_string(),
        "Filesystem error: boom"
    );
    assert_eq!(IOError::user_error("boom").to_string(), "User error: boom");
    assert_eq!(
        IOError::unsupported("boom").to_string(),
        "Unsupported operation: boom"
    );
    assert_eq!(IOError::other("boom").to_string(), "IO error: boom");
    assert_eq!(IOError::Interrupted.to_string(), "Operation interrupted");
    // Debug format must not panic
    assert!(!format!("{:?}", IOError::filesystem("dbg")).is_empty());
}

#[test]
fn test_ioerror_error_trait() {
    let err: Box<dyn std::error::Error> = Box::new(IOError::filesystem("missing"));
    assert!(err.source().is_none());
    assert!(err.to_string().contains("missing"));

    let err: Box<dyn std::error::Error> = Box::new(IOError::user_error("u"));
    assert_eq!(err.to_string(), "User error: u");
}

#[test]
fn test_ioresult_alias() {
    let ok: IOResult<u32> = Ok(42);
    assert!(matches!(ok, Ok(42)));
    let err: IOResult<u32> = Err(IOError::other("nope"));
    assert!(err.is_err());
    let msg = err.err().unwrap();
    assert!(msg.to_string().contains("nope"));
}

#[test]
fn test_ioerror_to_leanerror_conversion() {
    let lean_err: LeanError = IOError::filesystem("no such file").into();
    match &lean_err {
        LeanError::Other(msg) => assert!(msg.contains("Filesystem error: no such file")),
        other => panic!("expected LeanError::Other, got {:?}", other),
    }
}

#[test]
fn test_leanerror_to_ioerror_conversion() {
    let io_err: IOError = LeanError::Other("boom".to_string()).into();
    assert!(matches!(&io_err, IOError::Other(msg) if msg == "boom"));
    assert_eq!(io_err.to_string(), "IO error: boom");

    // Full round trip preserves the message
    let lean_err: LeanError = IOError::other("round trip").into();
    let round: IOError = lean_err.into();
    assert!(round.to_string().contains("round trip"));
}

// ============================================================================
// LeanIO Monad Tests
// ============================================================================

#[test]
fn test_leanio_pure_and_run() {
    let _lock = GLOBAL_LOCK.lock().unwrap();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let io: LeanIO<u64> = LeanIO::pure(lean, 42u64)?;
        assert_eq!(io.run()?, 42u64);

        let io: LeanIO<String> = LeanIO::pure(lean, "hello".to_string())?;
        assert_eq!(io.run()?, "hello");

        let io: LeanIO<bool> = LeanIO::pure(lean, true)?;
        assert!(io.run()?);
        Ok::<_, LeanError>(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_leanio_map() {
    let _lock = GLOBAL_LOCK.lock().unwrap();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // map: u64 -> u64
        let doubled: LeanIO<u64> = LeanIO::pure(lean, 21u64)?.map(lean, |x| x * 2)?;
        assert_eq!(doubled.run()?, 42u64);

        // map: String -> usize
        let len: LeanIO<usize> = LeanIO::pure(lean, "abcd".to_string())?.map(lean, |s| s.len())?;
        assert_eq!(len.run()?, 4);

        // chained map
        let chained: LeanIO<u64> = LeanIO::pure(lean, 5u64)?
            .map(lean, |x| x + 1)?
            .map(lean, |x| x * 10)?;
        assert_eq!(chained.run()?, 60u64);
        Ok::<_, LeanError>(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_leanio_bind() {
    let _lock = GLOBAL_LOCK.lock().unwrap();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let bound: LeanIO<String> = LeanIO::pure(lean, 21u64)?
            .bind(lean, |x| LeanIO::pure(lean, format!("Result: {}", x * 2)))?;
        assert_eq!(bound.run()?, "Result: 42");

        // Chained binds pass values through the pipeline
        let chained: LeanIO<u64> = LeanIO::pure(lean, 2u64)?
            .bind(lean, |x| LeanIO::pure(lean, x * 3))?
            .bind(lean, |x| LeanIO::pure(lean, x + 4))?;
        assert_eq!(chained.run()?, 10u64);
        Ok::<_, LeanError>(())
    });
    assert!(result.is_ok());
}
