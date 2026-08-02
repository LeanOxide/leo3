//! Runtime tests for #[leanclass] property accessors (#[getter] / #[setter]).

#![cfg(all(feature = "macros", feature = "runtime-tests"))]

use leo3::external::LeanExternal;
use leo3::prelude::*;

#[derive(Clone, Debug, PartialEq)]
#[leanclass]
struct Config {
    level: i32,
}

#[leanclass]
impl Config {
    fn new(level: i32) -> Self {
        Config { level }
    }

    #[getter]
    fn level(&self) -> i32 {
        self.level
    }

    #[setter]
    fn set_level(&mut self, level: i32) {
        self.level = level;
    }
}

unsafe fn read_config_level(ptr: *mut leo3::ffi::lean_object) -> i32 {
    let data_ptr = leo3::ffi::lean_get_external_data(ptr);
    (*(data_ptr as *const Config)).level
}

#[test]
fn test_getter_returns_field_value() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        let config = Config { level: 7 };
        let external = LeanExternal::new(lean, config).unwrap();

        // The getter's `i32` result crosses the boundary unboxed.
        let value = unsafe { __lean_ffi_Config_level(external.into_ptr()) };
        assert_eq!(value, 7);

        Ok::<_, LeanError>(())
    })
    .unwrap();
}

#[test]
fn test_setter_exclusive_mutates_in_place() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        let config = Config { level: 1 };
        let external = LeanExternal::new(lean, config).unwrap();
        let ptr = external.into_ptr();

        assert!(unsafe { leo3::ffi::object::lean_is_exclusive(ptr) });

        // The setter's `i32` argument crosses the boundary unboxed.
        let result_ptr = unsafe { __lean_ffi_Config_set_level(ptr, 42) };

        assert_eq!(
            ptr, result_ptr,
            "exclusive setter must return the same object (in-place)"
        );
        assert_eq!(unsafe { read_config_level(result_ptr) }, 42);

        unsafe { leo3::ffi::object::lean_dec_ref(result_ptr) };

        Ok::<_, LeanError>(())
    })
    .unwrap();
}

#[test]
fn test_setter_shared_uses_copy_on_write() {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        let config = Config { level: 5 };
        let external = LeanExternal::new(lean, config).unwrap();
        let ptr = external.into_ptr();

        unsafe { leo3::ffi::object::lean_inc_ref(ptr) };

        let new_ptr = unsafe { __lean_ffi_Config_set_level(ptr, 99) };

        assert_ne!(
            ptr, new_ptr,
            "shared setter must return a new object (copy-on-write)"
        );
        assert_eq!(unsafe { read_config_level(ptr) }, 5);
        assert_eq!(unsafe { read_config_level(new_ptr) }, 99);

        unsafe {
            leo3::ffi::object::lean_dec_ref(ptr);
            leo3::ffi::object::lean_dec_ref(new_ptr);
        }

        Ok::<_, LeanError>(())
    })
    .unwrap();
}
