//! File handle operation tests (open/read/write/get_line/flush/is_eof).
//!
//! These paths were previously broken: the IO closures were allocated with
//! `num_fixed == arity`, tripping the runtime's closure invariant. They now
//! exercise the real `lean_io_prim_handle_*` primitives end to end.

#![cfg(all(feature = "runtime-tests", feature = "io"))]

use leo3::io::handle::{self, FileMode};
use leo3::prelude::*;
use std::path::PathBuf;

fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("leo3_handle_test_{}_{}", std::process::id(), name))
}

#[test]
fn test_handle_write_read_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let path = temp_path("roundtrip.txt");
        let path_str = path.to_str().unwrap();

        // Write a file through the handle API.
        let write_handle = handle::open(lean, path_str, FileMode::Write)?.run()?;
        handle::write(lean, &write_handle, "Hello, Handle!\n")?.run()?;
        handle::write(lean, &write_handle, "Second line")?.run()?;
        handle::flush(lean, &write_handle)?.run()?;
        drop(write_handle);

        // Read it back.
        let read_handle = handle::open(lean, path_str, FileMode::Read)?.run()?;
        // Partial read: not at EOF yet.
        let part = handle::read(lean, &read_handle, 5)?.run()?;
        assert_eq!(leo3::types::LeanByteArray::to_vec(&part), b"Hello");
        assert!(!handle::is_eof(lean, &read_handle)?.run()?);
        // Consume the rest, then EOF is reached.
        let bytes = handle::read(lean, &read_handle, 4096)?.run()?;
        assert_eq!(
            leo3::types::LeanByteArray::to_vec(&bytes),
            b", Handle!\nSecond line"
        );
        assert!(handle::is_eof(lean, &read_handle)?.run()?);
        drop(read_handle);

        // Cleanup.
        std::fs::remove_file(&path).ok();

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_handle_binary_read_with_size() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let path = temp_path("binary.bin");
        let path_str = path.to_str().unwrap();
        std::fs::write(&path, [0u8, 1, 2, 3, 4, 5, 6, 7]).unwrap();

        let read_handle = handle::open(lean, path_str, FileMode::Read)?.run()?;
        let bytes = handle::read(lean, &read_handle, 4)?.run()?;
        let bytes = leo3::types::LeanByteArray::to_vec(&bytes);
        assert_eq!(bytes, vec![0u8, 1, 2, 3]);
        let more = handle::read(lean, &read_handle, 100)?.run()?;
        let more = leo3::types::LeanByteArray::to_vec(&more);
        assert_eq!(more, vec![4u8, 5, 6, 7]);
        // Reading past EOF yields an empty buffer.
        let past = handle::read(lean, &read_handle, 100)?.run()?;
        let past = leo3::types::LeanByteArray::to_vec(&past);
        assert!(past.is_empty());

        std::fs::remove_file(&path).ok();
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_handle_append_mode_and_get_line() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let path = temp_path("lines.txt");
        let path_str = path.to_str().unwrap();
        std::fs::write(&path, "first\n").unwrap();

        // Append mode writes after the existing content.
        let append_handle = handle::open(lean, path_str, FileMode::Append)?.run()?;
        handle::write(lean, &append_handle, "second\n")?.run()?;
        handle::flush(lean, &append_handle)?.run()?;
        drop(append_handle);

        let read_handle = handle::open(lean, path_str, FileMode::Read)?.run()?;
        let line1 = handle::get_line(lean, &read_handle)?.run()?;
        assert_eq!(line1, "first\n");
        let line2 = handle::get_line(lean, &read_handle)?.run()?;
        assert_eq!(line2, "second\n");

        std::fs::remove_file(&path).ok();
        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_handle_open_missing_file_errors() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let path = temp_path("missing.txt");
        std::fs::remove_file(&path).ok();

        let io = handle::open(lean, path.to_str().unwrap(), FileMode::Read)?;
        let err = io.run();
        assert!(err.is_err());
        // The error surfaces as a LeanError carrying an IOError message.
        let msg = err.err().unwrap().to_string();
        assert!(
            msg.contains("No such file") || msg.contains("no such file"),
            "unexpected error message: {msg}"
        );

        Ok(())
    });

    assert!(result.is_ok());
}
