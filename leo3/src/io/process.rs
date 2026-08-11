//! Process control operations for Lean4.
//!
//! Safe wrappers around Lean4's process control primitives: exit codes and
//! process termination.
//!
//! `getExitCode` / `setExitCode` are backed by a process-local mirror of
//! Lean's runtime exit-code global (the C primitives
//! `lean_io_prim_get_exit_code` / `lean_io_prim_set_exit_code` are not
//! exported by every Lean release, notably 4.25.2). The mirror follows
//! Lean's semantics: the code stored by `set_exit_code` is returned by
//! `get_exit_code`. The host process's actual exit status is owned by the
//! Rust `main`, exactly as it is owned by the embedding application in any
//! Lean embedding.

use crate::err::LeanResult;
use crate::ffi;
use crate::instance::LeanBound;
use crate::io::{io_ok_value_world, LeanIO};
use crate::marker::Lean;
use std::sync::atomic::{AtomicU32, Ordering};

/// Process-local mirror of Lean's runtime exit-code global.
static EXIT_CODE: AtomicU32 = AtomicU32::new(0);

/// Get the current process exit code.
///
/// This corresponds to Lean's `IO.getExitCode` function.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::process;
///
/// leo3::with_lean(|lean| {
///     let io = process::get_exit_code(lean)?;
///     let code = io.run()?;
///     println!("Exit code: {}", code);
///     Ok(())
/// })
/// ```
pub fn get_exit_code<'l>(lean: Lean<'l>) -> LeanResult<LeanIO<'l, u32>> {
    unsafe {
        let closure =
            ffi::inline::lean_alloc_closure(io_get_exit_code_impl as *mut std::ffi::c_void, 1, 0);

        Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, closure)))
    }
}

extern "C" fn io_get_exit_code_impl(world: ffi::object::lean_obj_arg) -> ffi::object::lean_obj_res {
    unsafe {
        let code = EXIT_CODE.load(Ordering::SeqCst);
        // `FromLean for u32` expects the tagged scalar form (`LeanUInt32`).
        io_ok_value_world(ffi::inline::lean_box(code as usize), world)
    }
}

/// Set the process exit code.
///
/// This corresponds to Lean's `IO.setExitCode` function.
/// The exit code will be used when the process terminates.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::process;
///
/// leo3::with_lean(|lean| {
///     let io = process::set_exit_code(lean, 1)?;
///     io.run()?;
///     Ok(())
/// })
/// ```
pub fn set_exit_code<'l>(lean: Lean<'l>, code: u32) -> LeanResult<LeanIO<'l, ()>> {
    unsafe {
        let closure =
            ffi::inline::lean_alloc_closure(io_set_exit_code_impl as *mut std::ffi::c_void, 2, 1);
        ffi::inline::lean_closure_set(closure, 0, ffi::lean_box(code as usize));

        Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, closure)))
    }
}

extern "C" fn io_set_exit_code_impl(
    code: ffi::object::lean_obj_arg,
    world: ffi::object::lean_obj_arg,
) -> ffi::object::lean_obj_res {
    unsafe {
        EXIT_CODE.store(ffi::inline::lean_unbox(code) as u32, Ordering::SeqCst);
        io_ok_value_world(ffi::inline::lean_box(0), world)
    }
}

/// Exit the process immediately with the given exit code.
///
/// This corresponds to Lean's `IO.Process.exit` function.
/// This function does not return.
///
/// # Safety
///
/// This function terminates the process immediately without running destructors
/// or cleanup code. Use with caution.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::process;
///
/// leo3::with_lean(|lean| {
///     // This will terminate the process
///     process::exit(1);
/// })
/// ```
pub fn exit(code: u32) -> ! {
    unsafe { ffi::io::lean_io_exit(code as u8) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_io_types() {
        assert_eq!(
            std::mem::size_of::<LeanIO<u32>>(),
            std::mem::size_of::<*mut ()>()
        );
    }
}
