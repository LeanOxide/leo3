//! FFI bindings for Lean4 IO operations
//!
//! This module provides low-level bindings to Lean4's IO primitives,
//! including file operations, handles, and environment variables.
//!
//! Based on the Lean4 C API for IO operations.

use crate::object::{b_lean_obj_arg, lean_obj_arg, lean_obj_res};

// ============================================================================
// IO Result Type Checking
// ============================================================================

// These functions are `static inline` in lean.h, not exported symbols.
// We reimplement them in Rust to match the C semantics exactly.

/// Check if an IO result represents an error.
///
/// IO results use constructor tags: tag 0 = `Except.ok`, tag 1 = `Except.error`.
///
/// # Safety
/// - `r` must be a valid IO result object
#[inline]
pub unsafe fn lean_io_result_is_error(r: b_lean_obj_arg) -> bool {
    crate::lean_obj_tag(r as lean_obj_arg) == 1
}

/// Check if an IO result represents success.
///
/// IO results use constructor tags: tag 0 = `Except.ok`, tag 1 = `Except.error`.
///
/// # Safety
/// - `r` must be a valid IO result object
#[inline]
pub unsafe fn lean_io_result_is_ok(r: b_lean_obj_arg) -> bool {
    crate::lean_obj_tag(r as lean_obj_arg) == 0
}

/// Get the value from a successful IO result (borrowed).
///
/// Returns field 0 of the `Except.ok` constructor. The returned pointer
/// is borrowed — caller must `lean_inc` if they want to own it.
///
/// # Safety
/// - `r` must be a valid IO result object with tag 0 (`Except.ok`)
/// - Calling this on an error result is undefined behavior
#[inline]
pub unsafe fn lean_io_result_get_value(r: b_lean_obj_arg) -> b_lean_obj_arg {
    crate::lean_ctor_get(r as lean_obj_arg, 0)
}

/// Get the error from a failed IO result (borrowed).
///
/// Returns field 0 of the `Except.error` constructor. The returned pointer
/// is borrowed — caller must `lean_inc` if they want to own it.
///
/// # Safety
/// - `r` must be a valid IO result object with tag 1 (`Except.error`)
/// - Calling this on an ok result is undefined behavior
#[inline]
pub unsafe fn lean_io_result_get_error(r: b_lean_obj_arg) -> b_lean_obj_arg {
    crate::lean_ctor_get(r as lean_obj_arg, 0)
}

/// Take the value from a successful IO result (consuming).
///
/// Extracts field 0 of the `Except.ok` constructor, increments its refcount,
/// and decrements the refcount of the IO result itself.
///
/// # Safety
/// - `r` must be a valid IO result object with tag 0 (`Except.ok`)
/// - `r` is consumed (caller must not use it after this call)
/// - Calling this on an error result is undefined behavior
#[inline]
pub unsafe fn lean_io_result_take_value(r: lean_obj_arg) -> lean_obj_res {
    debug_assert!(lean_io_result_is_ok(r));
    let v = crate::lean_ctor_get(r, 0);
    crate::lean_inc(v as lean_obj_arg);
    crate::lean_dec(r);
    v as lean_obj_res
}

/// Construct a successful IO result (`Except.ok a`).
///
/// Creates a 2-field constructor with tag 0. Field 0 is the value,
/// field 1 is the RealWorld token (`lean_box(0)`).
///
/// # Safety
/// - `a` must be a valid Lean object (consumed)
#[inline]
pub unsafe fn lean_io_result_mk_ok(a: lean_obj_arg) -> lean_obj_res {
    let r = crate::lean_alloc_ctor(0, 2, 0);
    crate::inline::lean_ctor_set(r, 0, a);
    crate::inline::lean_ctor_set(r, 1, crate::inline::lean_box(0));
    r
}

/// Construct a failed IO result (`Except.error e`).
///
/// Creates a 2-field constructor with tag 1. Field 0 is the error,
/// field 1 is the RealWorld token (`lean_box(0)`).
///
/// # Safety
/// - `e` must be a valid Lean object (consumed)
#[inline]
pub unsafe fn lean_io_result_mk_error(e: lean_obj_arg) -> lean_obj_res {
    let r = crate::lean_alloc_ctor(1, 2, 0);
    crate::inline::lean_ctor_set(r, 0, e);
    crate::inline::lean_ctor_set(r, 1, crate::inline::lean_box(0));
    r
}

// ============================================================================
// File System Operations
// ============================================================================

extern "C" {
    /// Read entire file contents as a string
    ///
    /// # Safety
    /// - `path` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error String)
    pub fn lean_io_prim_fs_read_file(path: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// Write string to file (overwrites if exists)
    ///
    /// # Safety
    /// - `path` must be a valid Lean string object (consumed)
    /// - `content` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error Unit)
    pub fn lean_io_prim_fs_write_file(
        path: lean_obj_arg,
        content: lean_obj_arg,
        world: lean_obj_arg,
    ) -> lean_obj_res;

    /// Read file contents as byte array
    ///
    /// # Safety
    /// - `path` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error ByteArray)
    pub fn lean_io_prim_fs_read_bin_file(path: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// Write byte array to file
    ///
    /// # Safety
    /// - `path` must be a valid Lean string object (consumed)
    /// - `content` must be a valid Lean ByteArray object (consumed)
    /// - Returns IO (Except IO.Error Unit)
    pub fn lean_io_prim_fs_write_bin_file(
        path: lean_obj_arg,
        content: lean_obj_arg,
        world: lean_obj_arg,
    ) -> lean_obj_res;

    /// Check if file exists
    ///
    /// # Safety
    /// - `path` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error Bool)
    pub fn lean_io_prim_fs_file_exists(path: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// Check if directory exists
    ///
    /// # Safety
    /// - `path` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error Bool)
    pub fn lean_io_prim_fs_dir_exists(path: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// Remove file
    ///
    /// # Safety
    /// - `path` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error Unit)
    pub fn lean_io_prim_fs_remove_file(path: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// Remove directory
    ///
    /// # Safety
    /// - `path` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error Unit)
    pub fn lean_io_prim_fs_remove_dir(path: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// Create directory
    ///
    /// # Safety
    /// - `path` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error Unit)
    pub fn lean_io_prim_fs_create_dir(path: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// Rename/move file or directory
    ///
    /// # Safety
    /// - `old_path` must be a valid Lean string object (consumed)
    /// - `new_path` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error Unit)
    pub fn lean_io_prim_fs_rename(
        old_path: lean_obj_arg,
        new_path: lean_obj_arg,
        world: lean_obj_arg,
    ) -> lean_obj_res;
}

// ============================================================================
// File Metadata Operations
// ============================================================================

extern "C" {
    /// Get file size in bytes
    ///
    /// # Safety
    /// - `path` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error USize)
    pub fn lean_io_prim_fs_file_size(path: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// Get current working directory
    ///
    /// # Safety
    /// - Returns IO (Except IO.Error String)
    pub fn lean_io_prim_fs_get_cwd(world: lean_obj_arg) -> lean_obj_res;

    /// Set current working directory
    ///
    /// # Safety
    /// - `path` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error Unit)
    pub fn lean_io_prim_fs_set_cwd(path: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// File Handle Operations
// ============================================================================

extern "C" {
    /// Open file for reading
    ///
    /// # Safety
    /// - `path` must be a valid Lean string object (borrowed)
    /// - `mode` must be a valid `IO.FS.Mode` constructor (borrowed)
    /// - Returns IO (Except IO.Error FS.Handle)
    ///
    /// Note: modern Lean (4.25 through 4.32+) has no `binary` parameter on
    /// `Handle.mk` and takes the mode as a raw scalar; the C ABI is
    /// `(path, mode: uint8, world)`.
    pub fn lean_io_prim_handle_mk(
        path: lean_obj_arg,
        mode: u8,
        world: lean_obj_arg,
    ) -> lean_obj_res;

    /// Close file handle
    ///
    /// # Safety
    /// - `handle` must be a valid file handle object (consumed)
    /// - Returns IO (Except IO.Error Unit)
    ///
    /// Note: This function is only available in Lean 4.26+
    #[cfg(lean_4_26)]
    pub fn lean_io_prim_handle_close(handle: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// Read from file handle
    ///
    /// # Safety
    /// - `handle` must be a valid file handle object (borrowed)
    /// - `size` is the number of bytes to read
    /// - Returns IO (Except IO.Error ByteArray)
    pub fn lean_io_prim_handle_read(
        handle: b_lean_obj_arg,
        size: usize,
        world: lean_obj_arg,
    ) -> lean_obj_res;

    /// Read line from file handle
    ///
    /// # Safety
    /// - `handle` must be a valid file handle object (borrowed)
    /// - Returns IO (Except IO.Error String)
    pub fn lean_io_prim_handle_get_line(
        handle: b_lean_obj_arg,
        world: lean_obj_arg,
    ) -> lean_obj_res;

    /// Write to file handle
    ///
    /// # Safety
    /// - `handle` must be a valid file handle object (borrowed)
    /// - `content` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error Unit)
    pub fn lean_io_prim_handle_write(
        handle: b_lean_obj_arg,
        content: lean_obj_arg,
        world: lean_obj_arg,
    ) -> lean_obj_res;

    /// Flush file handle buffers
    ///
    /// # Safety
    /// - `handle` must be a valid file handle object (borrowed)
    /// - Returns IO (Except IO.Error Unit)
    pub fn lean_io_prim_handle_flush(handle: b_lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// Check if at end of file
    ///
    /// # Safety
    /// - `handle` must be a valid file handle object (borrowed)
    /// - Returns IO (Except IO.Error Bool)
    pub fn lean_io_prim_handle_is_eof(handle: b_lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// Standard Streams
// ============================================================================

extern "C" {
    /// Get stdin handle
    ///
    /// # Safety
    /// - Returns a borrowed handle to stdin
    ///
    /// Note: This function is only available in Lean 4.26+
    #[cfg(lean_4_26)]
    pub fn lean_io_prim_handle_get_stdin() -> lean_obj_res;

    /// Get stdout handle
    ///
    /// # Safety
    /// - Returns a borrowed handle to stdout
    ///
    /// Note: This function is only available in Lean 4.26+
    #[cfg(lean_4_26)]
    pub fn lean_io_prim_handle_get_stdout() -> lean_obj_res;

    /// Get stderr handle
    ///
    /// # Safety
    /// - Returns a borrowed handle to stderr
    ///
    /// Note: This function is only available in Lean 4.26+
    #[cfg(lean_4_26)]
    pub fn lean_io_prim_handle_get_stderr() -> lean_obj_res;

    /// Get stdin handle
    ///
    /// # Safety
    /// - `world` is the RealWorld token
    /// - Returns `EStateM.Result.ok (stream, world)` with a borrowed stream
    pub fn lean_get_stdin(world: lean_obj_arg) -> lean_obj_res;

    /// Get stdout handle
    ///
    /// # Safety
    /// - `world` is the RealWorld token
    /// - Returns `EStateM.Result.ok (stream, world)` with a borrowed stream
    pub fn lean_get_stdout(world: lean_obj_arg) -> lean_obj_res;

    /// Get stderr handle
    ///
    /// # Safety
    /// - `world` is the RealWorld token
    /// - Returns `EStateM.Result.ok (stream, world)` with a borrowed stream
    pub fn lean_get_stderr(world: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// Environment Variables
// ============================================================================

extern "C" {
    /// Get environment variable value
    ///
    /// # Safety
    /// - `name` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error (Option String))
    pub fn lean_io_prim_get_env(name: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// Set environment variable
    ///
    /// # Safety
    /// - `name` must be a valid Lean string object (consumed)
    /// - `value` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error Unit)
    pub fn lean_io_prim_set_env(
        name: lean_obj_arg,
        value: lean_obj_arg,
        world: lean_obj_arg,
    ) -> lean_obj_res;

    /// Unset environment variable
    ///
    /// # Safety
    /// - `name` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error Unit)
    pub fn lean_io_prim_unset_env(name: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// Process Operations
// ============================================================================

extern "C" {
    /// Exit process immediately
    ///
    /// # Safety
    /// - This function does not return
    /// - `code` is the exit code
    ///
    /// Uses Lean's `lean_io_exit` export (present across supported Lean
    /// releases; the older `lean_io_prim_exit` name is not exported by
    /// e.g. 4.25.2).
    pub fn lean_io_exit(code: u8) -> !;
}

// ============================================================================
// Console I/O
// ============================================================================

extern "C" {
    /// Print string to stdout
    ///
    /// # Safety
    /// - `s` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error Unit)
    pub fn lean_io_prim_put_str(s: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// Print string to stdout with newline
    ///
    /// # Safety
    /// - `s` must be a valid Lean string object (consumed)
    /// - Returns IO (Except IO.Error Unit)
    pub fn lean_io_prim_put_str_ln(s: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// Read line from stdin
    ///
    /// # Safety
    /// - Returns IO (Except IO.Error String)
    pub fn lean_io_prim_get_line(world: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// Time Operations
// ============================================================================

// Note: the historical `lean_io_prim_mono_nanos` /
// `lean_io_prim_get_unix_time_millis` exports are not present in every Lean
// release (notably 4.25.2), so `leo3::io::time` implements both operations
// host-side in pure Rust.

// ============================================================================
// IO Error Constructors
// ============================================================================

extern "C" {
    /// Create an "already exists" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_already_exists(errno: u32, details: lean_obj_arg) -> lean_obj_res;

    /// Create an "already exists" IO error with file path
    ///
    /// # Safety
    /// - `file` must be a valid Lean string object (consumed)
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_already_exists_file(
        file: lean_obj_arg,
        errno: u32,
        details: lean_obj_arg,
    ) -> lean_obj_res;

    /// Create an "end of file" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_eof(details: lean_obj_arg) -> lean_obj_res;

    /// Create a "hardware fault" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_hardware_fault(errno: u32, details: lean_obj_arg) -> lean_obj_res;

    /// Create an "illegal operation" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_illegal_operation(errno: u32, details: lean_obj_arg) -> lean_obj_res;

    /// Create an "inappropriate type" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_inappropriate_type(errno: u32, details: lean_obj_arg) -> lean_obj_res;

    /// Create an "inappropriate type" IO error with file path
    ///
    /// # Safety
    /// - `file` must be a valid Lean string object (consumed)
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_inappropriate_type_file(
        file: lean_obj_arg,
        errno: u32,
        details: lean_obj_arg,
    ) -> lean_obj_res;

    /// Create an "interrupted" IO error
    ///
    /// # Safety
    /// - `syscall` must be a valid Lean string object (consumed)
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_interrupted(
        syscall: lean_obj_arg,
        errno: u32,
        details: lean_obj_arg,
    ) -> lean_obj_res;

    /// Create an "invalid argument" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_invalid_argument(errno: u32, details: lean_obj_arg) -> lean_obj_res;

    /// Create an "invalid argument" IO error with file path
    ///
    /// # Safety
    /// - `file` must be a valid Lean string object (consumed)
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_invalid_argument_file(
        file: lean_obj_arg,
        errno: u32,
        details: lean_obj_arg,
    ) -> lean_obj_res;

    /// Create a "no file or directory" IO error
    ///
    /// # Safety
    /// - `file` must be a valid Lean string object (consumed)
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_no_file_or_directory(
        file: lean_obj_arg,
        errno: u32,
        details: lean_obj_arg,
    ) -> lean_obj_res;

    /// Create a "no such thing" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_no_such_thing(errno: u32, details: lean_obj_arg) -> lean_obj_res;

    /// Create a "no such thing" IO error with file path
    ///
    /// # Safety
    /// - `file` must be a valid Lean string object (consumed)
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_no_such_thing_file(
        file: lean_obj_arg,
        errno: u32,
        details: lean_obj_arg,
    ) -> lean_obj_res;

    /// Create an "other error" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_other_error(errno: u32, details: lean_obj_arg) -> lean_obj_res;

    /// Create a "permission denied" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_permission_denied(errno: u32, details: lean_obj_arg) -> lean_obj_res;

    /// Create a "permission denied" IO error with file path
    ///
    /// # Safety
    /// - `file` must be a valid Lean string object (consumed)
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_permission_denied_file(
        file: lean_obj_arg,
        errno: u32,
        details: lean_obj_arg,
    ) -> lean_obj_res;

    /// Create a "protocol error" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_protocol_error(errno: u32, details: lean_obj_arg) -> lean_obj_res;

    /// Create a "resource busy" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_resource_busy(errno: u32, details: lean_obj_arg) -> lean_obj_res;

    /// Create a "resource exhausted" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_resource_exhausted(errno: u32, details: lean_obj_arg) -> lean_obj_res;

    /// Create a "resource exhausted" IO error with file path
    ///
    /// # Safety
    /// - `file` must be a valid Lean string object (consumed)
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_resource_exhausted_file(
        file: lean_obj_arg,
        errno: u32,
        details: lean_obj_arg,
    ) -> lean_obj_res;

    /// Create a "resource vanished" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_resource_vanished(errno: u32, details: lean_obj_arg) -> lean_obj_res;

    /// Create a "time expired" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_time_expired(errno: u32, details: lean_obj_arg) -> lean_obj_res;

    /// Create an "unsatisfied constraints" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_unsatisfied_constraints(
        errno: u32,
        details: lean_obj_arg,
    ) -> lean_obj_res;

    /// Create an "unsupported operation" IO error
    ///
    /// # Safety
    /// - `details` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_error_unsupported_operation(
        errno: u32,
        details: lean_obj_arg,
    ) -> lean_obj_res;

    /// Create a user-defined IO error
    ///
    /// # Safety
    /// - `str` must be a valid Lean string object (consumed)
    pub fn lean_mk_io_user_error(str: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// RealWorld Token
// ============================================================================

/// Create a RealWorld token
///
/// In Lean4, IO operations take a RealWorld token to enforce ordering.
/// This function creates the initial RealWorld token.
///
/// # Safety
/// - Should only be called once per IO computation chain
#[inline]
pub unsafe fn lean_io_mk_world() -> lean_obj_res {
    // RealWorld is represented as a boxed 0
    crate::lean_box(0)
}
