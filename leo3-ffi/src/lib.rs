#![cfg_attr(docsrs, feature(doc_cfg))]
#![allow(non_camel_case_types, non_snake_case, clippy::missing_safety_doc)]
//! Raw FFI declarations for Lean4's C API.
//!
//! This crate provides low-level bindings to the Lean4 runtime and API, based on
//! the actual Lean4 C headers from `/usr/local/include/lean/lean.h`.
//!
//! It is meant for advanced users only - regular Leo3 users shouldn't
//! need to interact with this crate directly.
//!
//! # Safety
//!
//! The functions in this crate are unsafe and require careful handling:
//! - Pointer arguments must point to valid Lean objects of the correct type
//! - Null pointers are sometimes valid input (check function documentation)
//! - Reference counting must be managed carefully (inc_ref/dec_ref)
//! - Most functions require a properly initialized Lean runtime
//!
//! Consult the Lean4 C API documentation for detailed safety requirements.
//!
//! # Module Organization
//!
//! - `object` - Core object manipulation, reference counting, constructors
//! - `string` - String operations
//! - `array` - Array operations
//! - `nat` - Natural number operations
//! - `closure` - Closures, thunks, tasks, and promises
//! - `inline` - Rust re-implementations of Lean4's static inline functions
//! - `hashmap` - HashMap FFI bindings (Std.HashMap)
//! - `hashset` - HashSet FFI bindings (Std.HashSet)
//! - `rbmap` - RBMap FFI bindings (Lean.RBMap)
//! - `environment` - Environment and declaration FFI bindings
//! - `expr` - Expression FFI bindings (core term language)
//! - `io` - IO operations
//! - `meta` - MetaM monad FFI bindings (type inference, checking)

// Re-export libc types used across modules
pub use libc::{c_char, c_int, c_uint, c_void, size_t};

pub mod array;
pub mod closure;
pub mod environment;
pub mod expr;
pub mod float;
pub mod hashmap;
pub mod hashset;
pub mod int;
pub mod io;
pub mod meta;
pub mod name;
pub mod nat;
pub mod object;
pub mod rbmap;
pub mod string;

// Inline function implementations
pub mod inline;

// Semantic constructor helpers for standard Lean types
pub mod constructors;
pub use constructors::*;

// Re-export commonly used types from object module
pub use object::{
    b_lean_obj_arg,
    lean_box,
    // Constructor functions
    lean_ctor_get,
    lean_ctor_get_uint16,
    lean_ctor_get_uint32,
    lean_ctor_get_uint64,
    lean_ctor_get_uint8,
    lean_ctor_set,
    lean_ctor_set_uint16,
    lean_ctor_set_uint32,
    lean_ctor_set_uint64,
    lean_ctor_set_uint8,
    // Inline functions
    lean_is_scalar,
    lean_obj_arg,
    lean_obj_res,
    lean_obj_tag,
    lean_object,
    // External object class registration (extern C function)
    lean_register_external_class,
    lean_unbox,
    u_lean_obj_arg,
};

/// Lean object type tags (from lean.h)
pub const LEAN_MAX_CTOR_TAG: u8 = 243;
pub const LEAN_PROMISE: u8 = 244;
pub const LEAN_CLOSURE: u8 = 245;
pub const LEAN_ARRAY: u8 = 246;
pub const LEAN_STRUCT_ARRAY: u8 = 247;
pub const LEAN_SCALAR_ARRAY: u8 = 248;
pub const LEAN_STRING: u8 = 249;
pub const LEAN_MPZ: u8 = 250;
pub const LEAN_THUNK: u8 = 251;
pub const LEAN_TASK: u8 = 252;
pub const LEAN_REF: u8 = 253;
pub const LEAN_EXTERNAL: u8 = 254;
pub const LEAN_RESERVED: u8 = 255;

// Constructor and object size limits (from lean.h)
pub const LEAN_MAX_CTOR_FIELDS: c_uint = 256;
pub const LEAN_MAX_CTOR_SCALARS_SIZE: c_uint = 1024;
pub const LEAN_MAX_SMALL_OBJECT_SIZE: size_t = 4096;

// ============================================================================
// Runtime Initialization
// ============================================================================

extern "C" {
    /// Initialize the Lean runtime module
    ///
    /// Must be called once before using any Lean API functions.
    pub fn lean_initialize_runtime_module();

    /// Finalize the Lean runtime module
    ///
    /// Should be called before program exit.
    pub fn lean_finalize_runtime_module();

    /// Initialize thread for Lean
    ///
    /// Must be called for each thread that will use Lean objects.
    pub fn lean_initialize_thread();

    /// Finalize thread for Lean
    ///
    /// Should be called before thread exit.
    pub fn lean_finalize_thread();

    /// Increment heartbeat counter (for interruption)
    ///
    /// Used by the Lean runtime to check for interrupts and resource limits.
    pub fn lean_inc_heartbeat();

    /// Initialize the Lean.Expr module
    ///
    /// Must be called before using exported functions from Lean.Expr like
    /// `lean_expr_mk_const`, `lean_lit_type`, etc.
    ///
    /// # Parameters
    /// - `builtin`: Set to 1 for builtin initialization
    /// - `w`: World token (pass null_mut for first call)
    ///
    /// # Returns
    /// Updated world token (can be ignored for initialization purposes)
    pub fn initialize_Lean_Expr(builtin: u8, w: *mut std::ffi::c_void) -> *mut std::ffi::c_void;

    /// Initialize the Init.Prelude module
    ///
    /// Must be called before using exported functions from Init.Prelude like
    /// `lean_name_mk_string`, `lean_name_mk_numeral`, etc.
    ///
    /// # Parameters
    /// - `builtin`: Set to 1 for builtin initialization
    /// - `w`: World token (pass null_mut for first call)
    ///
    /// # Returns
    /// Updated world token (can be ignored for initialization purposes)
    pub fn initialize_Init_Prelude(builtin: u8, w: *mut std::ffi::c_void) -> *mut std::ffi::c_void;

    /// Initialize the Lean.Environment module
    ///
    /// Must be called before using environment-related functions like
    /// `lean_mk_empty_environment`. This initializer recursively initializes
    /// all transitive dependencies (guarded by `_G_initialized` flags).
    ///
    /// # Parameters
    /// - `builtin`: Set to 1 for builtin initialization
    /// - `w`: World token (pass null_mut for first call)
    ///
    /// # Returns
    /// Updated world token (can be ignored for initialization purposes)
    pub fn initialize_Lean_Environment(
        builtin: u8,
        w: *mut std::ffi::c_void,
    ) -> *mut std::ffi::c_void;

    /// Initialize the Lean.Meta module
    ///
    /// Must be called before using Meta-level functions like `inferType` and
    /// `check`. This initializer recursively initializes all transitive
    /// dependencies (guarded by `_G_initialized` flags).
    ///
    /// # Parameters
    /// - `builtin`: Set to 1 for builtin initialization
    /// - `w`: World token (pass null_mut for first call)
    ///
    /// # Returns
    /// Updated world token (can be ignored for initialization purposes)
    pub fn initialize_Lean_Meta(builtin: u8, w: *mut std::ffi::c_void) -> *mut std::ffi::c_void;

    /// Initialize the kernel C++ module (type checker, declaration, environment globals)
    ///
    /// Must be called before using kernel type checking functions like `lean_add_decl`
    /// or `lean_elab_add_decl`. This is separate from the Lean-compiled module initializers.
    /// Requires `initialize_util_module()` to be called first.
    #[link_name = "_ZN4lean24initialize_kernel_moduleEv"]
    pub fn initialize_kernel_module();

    /// Install Lean's default expression print function.
    ///
    /// This is required for `lean_expr_dbg_to_string` to work.
    #[link_name = "_ZN4lean21init_default_print_fnEv"]
    pub fn init_default_print_fn();

    /// Initialize the C++ utility module (name, expr, etc.)
    ///
    /// Must be called before `initialize_kernel_module()`.
    #[link_name = "_ZN4lean22initialize_util_moduleEv"]
    pub fn initialize_util_module();

    /// Initialize the library C++ core module
    #[link_name = "_ZN4lean30initialize_library_core_moduleEv"]
    pub fn initialize_library_core_module();

    /// Initialize the library C++ module (full)
    #[link_name = "_ZN4lean25initialize_library_moduleEv"]
    pub fn initialize_library_module();

    /// Lean's private importing flag used by `Lean.initializing`.
    ///
    /// This is not part of Lean's public C API, but it is the runtime flag used
    /// by Lean's own `withImporting` wrapper when loading plugins whose
    /// initializers register options or environment extensions after
    /// `IO.initializing` has ended.
    ///
    /// # Do not reference this static directly
    ///
    /// Lean's Windows DLLs do not export this private symbol, so any direct
    /// reference makes `link.exe` fail with `LNK2019: unresolved external
    /// symbol` on every Windows build. Resolve it at runtime instead: `leo3`
    /// looks it up via `dlsym` / `GetProcAddress` and degrades to a no-op when
    /// it is unavailable (see `leo3::module`).
    #[link_name = "l___private_Lean_ImportingFlag_0__Lean_importingRef"]
    pub static mut lean_importing_ref: *mut lean_object;
}

// ============================================================================
// Utility Functions
// ============================================================================

extern "C" {
    /// Mix two uint64 hashes
    ///
    /// Used for combining hash values
    pub fn lean_uint64_mix_hash(a1: u64, a2: u64) -> u64;

    /// Compare two Lean Name objects for equality
    ///
    /// # Safety
    /// - `n1` and `n2` must be valid name objects
    pub fn lean_name_eq(n1: b_lean_obj_arg, n2: b_lean_obj_arg) -> u8;

    /// Convert Lean Int to Lean Nat
    ///
    /// # Safety
    /// - `a` must be a valid int object (consumed)
    pub fn lean_big_int_to_nat(a: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// Debug Functions
// ============================================================================

extern "C" {
    /// Debug trace - print a string and call a function
    ///
    /// # Safety
    /// - `s` must be a valid string object (consumed)
    /// - `fn_` must be a valid closure (consumed)
    pub fn lean_dbg_trace(s: lean_obj_arg, fn_: lean_obj_arg) -> lean_obj_res;

    /// Debug sleep - sleep for specified milliseconds and call a function
    ///
    /// # Safety
    /// - `fn_` must be a valid closure (consumed)
    pub fn lean_dbg_sleep(ms: u32, fn_: lean_obj_arg) -> lean_obj_res;

    /// Debug trace if shared - trace object if it's shared
    ///
    /// # Safety
    /// - `s` must be a valid string object (consumed)
    /// - `a` must be a valid object (borrowed)
    pub fn lean_dbg_trace_if_shared(s: lean_obj_arg, a: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// ST Reference Functions (State Thread References)
// ============================================================================

extern "C" {
    /// Create a new ST reference
    ///
    /// # Safety
    /// - `v` is the initial value (consumed)
    /// - `w` is the IO world token (consumed)
    pub fn lean_st_mk_ref(v: lean_obj_arg, w: lean_obj_arg) -> lean_obj_res;

    /// Get the value from an ST reference
    ///
    /// # Safety
    /// - `r` must be a valid ST ref object
    /// - `w` is the IO world token (consumed)
    pub fn lean_st_ref_get(r: b_lean_obj_arg, w: lean_obj_arg) -> lean_obj_res;

    /// Set the value of an ST reference
    ///
    /// # Safety
    /// - `r` must be a valid ST ref object
    /// - `v` is the new value (consumed)
    /// - `w` is the IO world token (consumed)
    pub fn lean_st_ref_set(r: b_lean_obj_arg, v: lean_obj_arg, w: lean_obj_arg) -> lean_obj_res;

    /// Reset an ST reference (set to default/zero value)
    ///
    /// # Safety
    /// - `r` must be a valid ST ref object
    /// - `w` is the IO world token (consumed)
    pub fn lean_st_ref_reset(r: b_lean_obj_arg, w: lean_obj_arg) -> lean_obj_res;

    /// Swap the value of an ST reference, returning the old value
    ///
    /// # Safety
    /// - `r` must be a valid ST ref object
    /// - `v` is the new value (consumed)
    /// - `w` is the IO world token (consumed)
    pub fn lean_st_ref_swap(r: b_lean_obj_arg, v: lean_obj_arg, w: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// IO Error Handling
// ============================================================================

extern "C" {
    /// Decode a POSIX errno into a Lean IO error
    ///
    /// # Safety
    /// - `fname` must be a valid string object (borrowed)
    pub fn lean_decode_io_error(errnum: libc::c_int, fname: b_lean_obj_arg) -> lean_obj_res;

    /// Decode a libuv error code into a Lean IO error
    ///
    /// # Safety
    /// - `fname` must be a valid string object (borrowed)
    pub fn lean_decode_uv_error(errnum: libc::c_int, fname: b_lean_obj_arg) -> lean_obj_res;

    /// Show an IO error result to stderr
    ///
    /// # Safety
    /// - `r` must be a valid IO result object
    pub fn lean_io_result_show_error(r: b_lean_obj_arg);

    /// Mark end of initialization phase
    ///
    /// Called after program initialization is complete
    pub fn lean_io_mark_end_initialization();

    /// Create a user-defined IO error
    ///
    /// # Safety
    /// - `str` must be a valid string object (consumed)
    pub fn lean_mk_io_user_error(str: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// Object Type Checking Helpers (inline from lean.h)
// ============================================================================

pub use inline::{
    // External object inline functions
    lean_alloc_external,
    // Float inline functions
    lean_box_float,
    // ByteArray inline functions
    lean_byte_array_uget,
    lean_byte_array_uset,
    lean_get_external_class,
    lean_get_external_data,
    lean_is_array,
    lean_is_closure,
    lean_is_ctor,
    lean_is_external,
    lean_is_mpz,
    lean_is_promise,
    lean_is_ref,
    lean_is_sarray,
    lean_is_string,
    lean_is_task,
    lean_is_thunk,
    lean_sarray_cptr,
    lean_sarray_size,
    lean_to_array,
    lean_to_closure,
    lean_to_ctor,
    lean_to_external,
    lean_to_promise,
    lean_to_ref,
    lean_to_sarray,
    lean_to_string,
    lean_to_task,
    lean_to_thunk,
    lean_unbox_float,
};

// ============================================================================
// High-level inc/dec with scalar checking (from lean.h)
// ============================================================================

pub use inline::{lean_dec, lean_dec_ref, lean_inc, lean_inc_n, lean_inc_ref, lean_inc_ref_n};

// ============================================================================
// Array and ByteArray functions
// ============================================================================

pub use array::{lean_byte_array_push, lean_mk_empty_byte_array};

/// Allocate a constructor object (inline from lean.h)
///
/// Mirrors Lean's `lean_alloc_ctor` helper, including the constructor-memory
/// allocation path used to zero any trailing alignment padding.
///
/// # Safety
/// - `tag` must be <= LEAN_MAX_CTOR_TAG
/// - `num_objs` must be < LEAN_MAX_CTOR_FIELDS
/// - Must initialize all fields after allocation
#[inline]
pub unsafe fn lean_alloc_ctor(
    tag: c_uint,
    num_objs: c_uint,
    scalar_sz: c_uint,
) -> *mut lean_object {
    debug_assert!(tag <= LEAN_MAX_CTOR_TAG as c_uint);
    debug_assert!(num_objs < LEAN_MAX_CTOR_FIELDS);
    debug_assert!(scalar_sz < LEAN_MAX_CTOR_SCALARS_SIZE);

    let sz = std::mem::size_of::<inline::lean_ctor_object>()
        + (num_objs as usize) * std::mem::size_of::<*mut lean_object>()
        + (scalar_sz as usize);
    let obj = inline::lean_alloc_ctor_memory(sz as c_uint);
    inline::lean_set_st_header(obj, tag as u8, num_objs as u8);
    obj
}
