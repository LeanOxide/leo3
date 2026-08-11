//! Console I/O operations for Lean4.
//!
//! Implemented over Lean's `IO.FS.Stream` objects (the std streams): a
//! `Stream` is a structure whose fields are the stream operations
//! (`flush`, `read`, `write`, `getLine`, `putStr`, `isTty`), so console
//! helpers apply the matching field closure to the captured argument.

use crate::conversion::IntoLean;
use crate::err::LeanResult;
use crate::ffi;
use crate::instance::{LeanAny, LeanBound};
use crate::io::LeanIO;
use crate::marker::Lean;

/// Fetch the current stdout stream object (a borrowed process-global).
unsafe fn stdout_stream<'l>(lean: Lean<'l>) -> LeanBound<'l, LeanAny> {
    #[cfg(not(lean_4_26))]
    let stream = {
        let world = ffi::io::lean_io_mk_world();
        let result = ffi::io::lean_get_stdout(world);
        ffi::object::lean_ctor_get(result, 0) as *mut ffi::lean_object
    };
    #[cfg(lean_4_26)]
    let stream = ffi::io::lean_get_stdout();
    LeanBound::from_borrowed_ptr(lean, stream)
}

/// Fetch the current stdin stream object (a borrowed process-global).
unsafe fn stdin_stream<'l>(lean: Lean<'l>) -> LeanBound<'l, LeanAny> {
    #[cfg(not(lean_4_26))]
    let stream = {
        let world = ffi::io::lean_io_mk_world();
        let result = ffi::io::lean_get_stdin(world);
        ffi::object::lean_ctor_get(result, 0) as *mut ffi::lean_object
    };
    #[cfg(lean_4_26)]
    let stream = ffi::io::lean_get_stdin();
    LeanBound::from_borrowed_ptr(lean, stream)
}

/// Apply `putStr : String -> IO Unit` from the stdout stream.
///
/// The stream's `putStr` field is a closure; the IO wrapper calls it with
/// the captured string and the world token.
extern "C" fn io_stream_put_str_impl(
    put_str_fn: ffi::object::lean_obj_arg,
    content: ffi::object::lean_obj_arg,
    world: ffi::object::lean_obj_arg,
) -> ffi::object::lean_obj_res {
    unsafe { ffi::closure::lean_apply_2(put_str_fn, content, world) }
}

/// Apply `getLine : IO String` from the stdin stream.
extern "C" fn io_stream_get_line_impl(
    get_line_fn: ffi::object::lean_obj_arg,
    world: ffi::object::lean_obj_arg,
) -> ffi::object::lean_obj_res {
    unsafe { ffi::closure::lean_apply_1(get_line_fn, world) }
}

/// Print a string to stdout without a newline.
///
/// This corresponds to Lean's `IO.print` function.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::console;
///
/// leo3::with_lean(|lean| {
///     let io = console::put_str(lean, "Hello, ")?;
///     io.run()?;
///     Ok(())
/// })
/// ```
pub fn put_str<'l>(lean: Lean<'l>, s: &str) -> LeanResult<LeanIO<'l, ()>> {
    unsafe {
        let stream = stdout_stream(lean);
        // `IO.FS.Stream.putStr` is field 4 of the stream structure.
        let put_str_fn = ffi::object::lean_ctor_get(stream.as_ptr(), 4) as *mut ffi::lean_object;
        let lean_s = s.into_lean(lean)?;

        // The closure slots own their own references.
        ffi::lean_inc(put_str_fn);
        let closure =
            ffi::inline::lean_alloc_closure(io_stream_put_str_impl as *mut std::ffi::c_void, 3, 2);
        ffi::inline::lean_closure_set(closure, 0, put_str_fn);
        ffi::inline::lean_closure_set(closure, 1, lean_s.into_ptr());

        Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, closure)))
    }
}

/// Print a string to stdout with a newline.
///
/// This corresponds to Lean's `IO.println` function.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::console;
///
/// leo3::with_lean(|lean| {
///     let io = console::put_str_ln(lean, "Hello, World!")?;
///     io.run()?;
///     Ok(())
/// })
/// ```
pub fn put_str_ln<'l>(lean: Lean<'l>, s: &str) -> LeanResult<LeanIO<'l, ()>> {
    let s_with_newline = format!("{}\n", s);
    put_str(lean, &s_with_newline)
}

/// Read a line from stdin.
///
/// This corresponds to Lean's `IO.getLine` function.
/// The returned string includes the trailing newline character.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::console;
///
/// leo3::with_lean(|lean| {
///     let io = console::get_line(lean)?;
///     let line = io.run()?;
///     println!("You entered: {}", line);
///     Ok(())
/// })
/// ```
pub fn get_line<'l>(lean: Lean<'l>) -> LeanResult<LeanIO<'l, String>> {
    unsafe {
        let stream = stdin_stream(lean);
        // `IO.FS.Stream.getLine` is field 3 of the stream structure.
        let get_line_fn = ffi::object::lean_ctor_get(stream.as_ptr(), 3) as *mut ffi::lean_object;

        ffi::lean_inc(get_line_fn);
        let closure =
            ffi::inline::lean_alloc_closure(io_stream_get_line_impl as *mut std::ffi::c_void, 2, 1);
        ffi::inline::lean_closure_set(closure, 0, get_line_fn);

        Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, closure)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_console_io_types() {
        // Ensure types are correctly sized
        assert_eq!(
            std::mem::size_of::<LeanIO<()>>(),
            std::mem::size_of::<*mut ()>()
        );
        assert_eq!(
            std::mem::size_of::<LeanIO<String>>(),
            std::mem::size_of::<*mut ()>()
        );
    }
}
