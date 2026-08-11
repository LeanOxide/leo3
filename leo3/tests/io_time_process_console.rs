//! Tests for `io::time`, `io::process`, and `io::console`.
//!
//! The time and process helpers are pure-Rust implementations (the
//! historical Lean C primitives are not exported by Lean 4.25.2); console
//! routes through the fixed handle layer.

#![cfg(all(feature = "runtime-tests", feature = "io", not(target_os = "windows")))]

use leo3::io::{console, process, time};
use leo3::prelude::*;

#[test]
fn test_mono_nanos_monotonic() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = time::mono_nanos(lean)?.run()?;
        let b = time::mono_nanos(lean)?.run()?;
        let c = time::mono_nanos(lean)?.run()?;
        // Monotonic: non-decreasing across calls.
        assert!(a <= b);
        assert!(b <= c);
        // Non-trivial magnitude (more than a millisecond into the process).
        assert!(c > 0);
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_unix_time_millis_plausible() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let millis = time::unix_time_millis(lean)?.run()?;
        // 2020-01-01 .. 2100-01-01 in epoch millis.
        assert!(millis > 1_577_836_800_000, "implausibly small: {millis}");
        assert!(millis < 4_102_444_800_000, "implausibly large: {millis}");
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_exit_code_mirror() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Default is 0.
        assert_eq!(process::get_exit_code(lean)?.run()?, 0);

        process::set_exit_code(lean, 7)?.run()?;
        assert_eq!(process::get_exit_code(lean)?.run()?, 7);

        process::set_exit_code(lean, 0)?.run()?;
        assert_eq!(process::get_exit_code(lean)?.run()?, 0);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_console_put_str() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Writing to stdout must succeed (output appears in the test log).
        console::put_str(lean, "leo3-console-test ")?.run()?;
        console::put_str_ln(lean, "line")?.run()?;
        Ok(())
    });

    assert!(result.is_ok());
}
