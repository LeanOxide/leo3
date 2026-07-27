use std::ffi::CStr;
use std::os::raw::c_char;

use leo3::ffi::inline::{lean_dec, lean_string_cstr};
use leo3::ffi::object::{lean_obj_arg, lean_obj_res};
use leo3::ffi::string::lean_mk_string;

#[no_mangle]
pub extern "C" fn native_add(a: u64, b: u64) -> u64 {
    a + b
}

#[no_mangle]
pub extern "C" fn native_mul(a: u64, b: u64) -> u64 {
    a * b
}

#[no_mangle]
pub unsafe extern "C" fn native_greet(name: lean_obj_arg) -> lean_obj_res {
    let cstr = lean_string_cstr(name);
    let rust_str = CStr::from_ptr(cstr).to_string_lossy().into_owned();
    lean_dec(name);
    let greeting = format!("Hello, {rust_str}! (from Rust)");
    let c_greeting = std::ffi::CString::new(greeting).unwrap();
    lean_mk_string(c_greeting.as_ptr() as *const c_char)
}

#[no_mangle]
pub extern "C" fn native_accumulator_new(initial: i64) -> i64 {
    initial
}

#[no_mangle]
pub extern "C" fn native_accumulator_add(state: i64, value: i64) -> i64 {
    state + value
}

#[no_mangle]
pub extern "C" fn native_accumulator_get(state: i64) -> i64 {
    state
}
