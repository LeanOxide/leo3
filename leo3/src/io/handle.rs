//! File handle operations for Lean4.
//!
//! This module provides safe wrappers around Lean4's file handle primitives,
//! including opening, reading, writing, and closing files.

use crate::conversion::{FromLean, IntoLean};
use crate::err::LeanResult;
use crate::ffi;
use crate::instance::LeanBound;
use crate::io::LeanIO;
use crate::marker::Lean;

/// A Lean file handle.
///
/// This corresponds to Lean's `IO.FS.Handle` type.
/// Handles must be closed when done to avoid resource leaks.
#[repr(transparent)]
pub struct LeanHandle<'l> {
    inner: LeanBound<'l, LeanHandleAny>,
}

/// Marker type for file handles.
pub struct LeanHandleAny {
    _private: (),
}

/// File open mode.
///
/// Mirrors Lean's `IO.FS.Mode` (constructor order read / write / writeNew /
/// readWrite / append, matching the runtime's `lean_io_prim_handle_mk` mode
/// switch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileMode {
    /// Open for reading
    Read,
    /// Open for writing (truncate if exists)
    Write,
    /// Open for writing, failing if the file already exists
    WriteNew,
    /// Open for reading and writing
    ReadWrite,
    /// Open for appending
    Append,
}

impl FileMode {
    /// Convert to Lean's runtime mode value.
    fn to_lean_tag(self) -> u8 {
        match self {
            FileMode::Read => 0,
            FileMode::Write => 1,
            FileMode::WriteNew => 2,
            FileMode::ReadWrite => 3,
            FileMode::Append => 4,
        }
    }
}

impl<'l> LeanHandle<'l> {
    /// Create a handle from a raw Lean object.
    ///
    /// # Safety
    ///
    /// - `obj` must be a valid Lean handle object
    #[inline]
    pub unsafe fn from_raw(obj: LeanBound<'l, LeanHandleAny>) -> Self {
        LeanHandle { inner: obj }
    }

    /// Get the underlying Lean object.
    #[inline]
    pub fn as_inner(&self) -> &LeanBound<'l, LeanHandleAny> {
        &self.inner
    }

    /// Get the underlying Lean object pointer.
    #[inline]
    pub fn as_ptr(&self) -> *mut ffi::lean_object {
        self.inner.as_ptr()
    }
}

impl<'l> FromLean<'l> for LeanHandle<'l> {
    type Source = LeanHandleAny;

    fn from_lean(obj: &LeanBound<'l, Self::Source>) -> LeanResult<Self> {
        Ok(unsafe { LeanHandle::from_raw(obj.clone()) })
    }
}

/// Open a file with the specified mode.
///
/// # Arguments
///
/// * `path` - Path to the file
/// * `mode` - File open mode
/// * `binary` - Whether to open in binary mode (true) or text mode (false)
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::handle::{open, FileMode};
///
/// leo3::with_lean(|lean| {
///     let io = open(lean, "test.txt", FileMode::Read, false)?;
///     let handle = io.run()?;
///     // Use handle...
///     Ok(())
/// })
/// ```
/// Open a file with the specified mode.
///
/// # Arguments
///
/// * `path` - Path to the file
/// * `mode` - File open mode
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::handle::{open, FileMode};
///
/// leo3::with_lean(|lean| {
///     let io = open(lean, "test.txt", FileMode::Read)?;
///     let handle = io.run()?;
///     // Use handle...
///     Ok(())
/// })
/// ```
pub fn open<'l>(
    lean: Lean<'l>,
    path: &str,
    mode: FileMode,
) -> LeanResult<LeanIO<'l, LeanHandle<'l>>> {
    unsafe {
        // Convert path to Lean string (the closure slot owns this ref).
        let lean_path = path.into_lean(lean)?;

        // The runtime's `lean_io_prim_handle_mk` takes the mode as a raw
        // `uint8` scalar, which cannot live in a closure fixed slot; a small
        // wrapper unpacks the boxed mode at call time.
        let closure =
            ffi::inline::lean_alloc_closure(io_handle_mk_wrapper as *mut std::ffi::c_void, 3, 2);
        ffi::inline::lean_closure_set(closure, 0, lean_path.into_ptr());
        ffi::inline::lean_closure_set(closure, 1, ffi::lean_box(mode.to_lean_tag() as usize));

        Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, closure)))
    }
}

/// Unpacks the boxed `IO.FS.Mode` scalar and calls the runtime's handle
/// constructor. Called by Lean's closure machinery with the fixed slots as
/// leading arguments: `(path, boxed_mode, world)`.
unsafe extern "C" fn io_handle_mk_wrapper(
    path: ffi::object::lean_obj_arg,
    mode: ffi::object::lean_obj_arg,
    world: ffi::object::lean_obj_arg,
) -> ffi::object::lean_obj_res {
    let mode_u8 = ffi::inline::lean_unbox(mode) as u8;
    ffi::io::lean_io_prim_handle_mk(path, mode_u8, world)
}

/// Close a file handle.
///
/// In current Lean versions, there is no explicit close function.
/// File handles are automatically closed when they are dropped.
/// This function is a no-op that returns a successful IO action.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::handle::{open, close, FileMode};
///
/// leo3::with_lean(|lean| {
///     let handle = open(lean, "test.txt", FileMode::Read)?.run()?;
///     close(lean, handle)?.run()?;
///     Ok(())
/// })
/// ```
pub fn close<'l>(lean: Lean<'l>, handle: LeanHandle<'l>) -> LeanResult<LeanIO<'l, ()>> {
    // Drop the handle to release it
    drop(handle);
    // Return a pure IO action with unit
    // In Lean, Unit is represented as lean_box(0)
    unsafe {
        let unit_ptr = ffi::lean_box(0);
        // Create a closure that takes world and returns Except.ok (unit, world)
        let closure = ffi::inline::lean_alloc_closure(io_unit_impl as *mut std::ffi::c_void, 2, 1);
        ffi::inline::lean_closure_set(closure, 0, unit_ptr);
        Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, closure)))
    }
}

/// Implementation function for IO.pure Unit
extern "C" fn io_unit_impl(
    value: ffi::object::lean_obj_arg,
    world: ffi::object::lean_obj_arg,
) -> ffi::object::lean_obj_res {
    unsafe { crate::io::io_ok_value_world(value, world) }
}

/// Read bytes from a file handle.
///
/// # Arguments
///
/// * `handle` - The file handle to read from
/// * `size` - Number of bytes to read
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// Read up to `size` bytes from a file handle.
///
/// Returns a `LeanByteArray`; convert with `to_vec()` or
/// `leo3::conversion::vec_u8_from_lean`.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::handle::{open, read, FileMode};
///
/// leo3::with_lean(|lean| {
///     let handle = open(lean, "test.txt", FileMode::Read)?.run()?;
///     let io = read(lean, &handle, 1024)?;
///     let bytes = io.run()?.to_vec();
///     Ok(())
/// })
/// ```
pub fn read<'l>(
    lean: Lean<'l>,
    handle: &LeanHandle<'l>,
    size: usize,
) -> LeanResult<LeanIO<'l, LeanBound<'l, crate::types::LeanByteArray>>> {
    unsafe {
        // The runtime takes the size as a raw `usize` scalar, which cannot
        // live in a closure fixed slot; a small wrapper unpacks it at call
        // time. The handle slot owns its own reference (closure deallocation
        // releases fixed slots).
        ffi::lean_inc(handle.as_ptr());
        let closure =
            ffi::inline::lean_alloc_closure(io_handle_read_wrapper as *mut std::ffi::c_void, 3, 2);
        ffi::inline::lean_closure_set(closure, 0, handle.as_ptr());
        ffi::inline::lean_closure_set(closure, 1, ffi::lean_box(size));

        Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, closure)))
    }
}

/// Unpacks the boxed byte count and calls the runtime's handle read.
/// Called by Lean's closure machinery with the fixed slots as leading
/// arguments: `(handle, boxed_size, world)`.
unsafe extern "C" fn io_handle_read_wrapper(
    handle: ffi::object::lean_obj_arg,
    size: ffi::object::lean_obj_arg,
    world: ffi::object::lean_obj_arg,
) -> ffi::object::lean_obj_res {
    let nbytes = ffi::inline::lean_unbox(size);
    ffi::io::lean_io_prim_handle_read(handle, nbytes, world)
}

/// Read a line from a file handle.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::handle::{open, get_line, FileMode};
///
/// leo3::with_lean(|lean| {
///     let handle = open(lean, "test.txt", FileMode::Read)?.run()?;
///     let io = get_line(lean, &handle)?;
///     let line = io.run()?;
///     Ok(())
/// })
/// ```
pub fn get_line<'l>(lean: Lean<'l>, handle: &LeanHandle<'l>) -> LeanResult<LeanIO<'l, String>> {
    unsafe {
        // The closure slot owns its own reference (closure deallocation
        // releases fixed slots).
        ffi::lean_inc(handle.as_ptr());
        let closure = ffi::inline::lean_alloc_closure(
            ffi::io::lean_io_prim_handle_get_line as *mut std::ffi::c_void,
            2,
            1,
        );
        ffi::inline::lean_closure_set(closure, 0, handle.as_ptr());

        Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, closure)))
    }
}

/// Write a string to a file handle.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::handle::{open, write, FileMode};
///
/// leo3::with_lean(|lean| {
///     let handle = open(lean, "test.txt", FileMode::Write)?.run()?;
///     write(lean, &handle, "Hello, World!")?.run()?;
///     Ok(())
/// })
/// ```
pub fn write<'l>(
    lean: Lean<'l>,
    handle: &LeanHandle<'l>,
    content: &str,
) -> LeanResult<LeanIO<'l, ()>> {
    unsafe {
        // The runtime's handle write expects a ByteArray (Lean's
        // `Handle.write : Handle → ByteArray → IO Unit`), not a String.
        let lean_bytes = crate::types::LeanByteArray::from_bytes(lean, content.as_bytes())?;

        // The closure slots own their own references (closure deallocation
        // releases fixed slots).
        ffi::lean_inc(handle.as_ptr());
        let closure = ffi::inline::lean_alloc_closure(
            ffi::io::lean_io_prim_handle_write as *mut std::ffi::c_void,
            3,
            2,
        );
        ffi::inline::lean_closure_set(closure, 0, handle.as_ptr());
        ffi::inline::lean_closure_set(closure, 1, lean_bytes.into_ptr());

        Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, closure)))
    }
}

/// Flush a file handle's buffers.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::handle::{open, write, flush, FileMode};
///
/// leo3::with_lean(|lean| {
///     let handle = open(lean, "test.txt", FileMode::Write)?.run()?;
///     write(lean, &handle, "Hello")?.run()?;
///     flush(lean, &handle)?.run()?;
///     Ok(())
/// })
/// ```
pub fn flush<'l>(lean: Lean<'l>, handle: &LeanHandle<'l>) -> LeanResult<LeanIO<'l, ()>> {
    unsafe {
        ffi::lean_inc(handle.as_ptr());
        let closure = ffi::inline::lean_alloc_closure(
            ffi::io::lean_io_prim_handle_flush as *mut std::ffi::c_void,
            2,
            1,
        );
        ffi::inline::lean_closure_set(closure, 0, handle.as_ptr());

        Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, closure)))
    }
}

/// Check if a file handle is at end-of-file.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::handle::{open, is_eof, FileMode};
///
/// leo3::with_lean(|lean| {
///     let handle = open(lean, "test.txt", FileMode::Read)?.run()?;
///     let io = is_eof(lean, &handle)?;
///     let at_eof = io.run()?;
///     Ok(())
/// })
/// ```
pub fn is_eof<'l>(lean: Lean<'l>, handle: &LeanHandle<'l>) -> LeanResult<LeanIO<'l, bool>> {
    unsafe {
        ffi::lean_inc(handle.as_ptr());
        let closure = ffi::inline::lean_alloc_closure(
            ffi::io::lean_io_prim_handle_is_eof as *mut std::ffi::c_void,
            2,
            1,
        );
        ffi::inline::lean_closure_set(closure, 0, handle.as_ptr());

        Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, closure)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_size() {
        assert_eq!(
            std::mem::size_of::<LeanHandle>(),
            std::mem::size_of::<*mut ()>()
        );
    }

    #[test]
    fn test_file_mode() {
        assert_eq!(FileMode::Read.to_lean_tag(), 0);
        assert_eq!(FileMode::Write.to_lean_tag(), 1);
        assert_eq!(FileMode::WriteNew.to_lean_tag(), 2);
        assert_eq!(FileMode::ReadWrite.to_lean_tag(), 3);
        assert_eq!(FileMode::Append.to_lean_tag(), 4);
    }
}
