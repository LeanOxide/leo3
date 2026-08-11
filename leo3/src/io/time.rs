//! Time operations for Lean4.
//!
//! Pure-Rust implementations of Lean's `IO.monoNanos` and
//! `IO.currentTimeMillis`. The historical C primitives
//! (`lean_io_prim_mono_nanos`, `lean_io_prim_get_unix_time_millis`) are not
//! exported by every Lean release (notably 4.25.2), so the values are
//! computed host-side and packaged into the same
//! `Except.ok (value, world)` IO result shape the Lean primitives produce.

use crate::err::LeanResult;
use crate::ffi;
use crate::instance::LeanBound;
use crate::io::{box_u64, io_ok_value_world, LeanIO};
use crate::marker::Lean;
use std::sync::OnceLock;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Process-fixed monotonic origin so `mono_nanos` values stay small and the
/// clock is stable across calls.
static MONO_ORIGIN: OnceLock<Instant> = OnceLock::new();

/// Get monotonic time in nanoseconds.
///
/// This corresponds to Lean's `IO.monoNanos` function.
/// Returns a monotonically increasing timestamp suitable for measuring durations.
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::time;
///
/// leo3::with_lean(|lean| {
///     let io = time::mono_nanos(lean)?;
///     let nanos = io.run()?;
///     println!("Monotonic time: {} ns", nanos);
///     Ok(())
/// })
/// ```
pub fn mono_nanos<'l>(lean: Lean<'l>) -> LeanResult<LeanIO<'l, u64>> {
    unsafe {
        let closure =
            ffi::inline::lean_alloc_closure(io_mono_nanos_impl as *mut std::ffi::c_void, 1, 0);

        Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, closure)))
    }
}

extern "C" fn io_mono_nanos_impl(world: ffi::object::lean_obj_arg) -> ffi::object::lean_obj_res {
    unsafe {
        let origin = *MONO_ORIGIN.get_or_init(Instant::now);
        let nanos = origin.elapsed().as_nanos() as u64;
        io_ok_value_world(box_u64(nanos), world)
    }
}

/// Get Unix time in milliseconds since epoch.
///
/// This corresponds to Lean's `IO.currentTimeMillis` function.
/// Returns the current wall-clock time as milliseconds since Unix epoch (1970-01-01).
///
/// # Example
///
/// ```rust,ignore
/// use leo3::prelude::*;
/// use leo3::io::time;
///
/// leo3::with_lean(|lean| {
///     let io = time::unix_time_millis(lean)?;
///     let millis = io.run()?;
///     println!("Unix time: {} ms", millis);
///     Ok(())
/// })
/// ```
pub fn unix_time_millis<'l>(lean: Lean<'l>) -> LeanResult<LeanIO<'l, u64>> {
    unsafe {
        let closure = ffi::inline::lean_alloc_closure(
            io_unix_time_millis_impl as *mut std::ffi::c_void,
            1,
            0,
        );

        Ok(LeanIO::from_raw(LeanBound::from_owned_ptr(lean, closure)))
    }
}

extern "C" fn io_unix_time_millis_impl(
    world: ffi::object::lean_obj_arg,
) -> ffi::object::lean_obj_res {
    unsafe {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        io_ok_value_world(box_u64(millis), world)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_io_types() {
        assert_eq!(
            std::mem::size_of::<LeanIO<u64>>(),
            std::mem::size_of::<*mut ()>()
        );
    }
}
