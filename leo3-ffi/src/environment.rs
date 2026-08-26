//! FFI bindings for Lean's Environment API
//!
//! This module provides raw C FFI bindings for working with Lean environments.
//! Environments are the core of Lean's module system, storing declarations, constants,
//! inductive types, and type class instances.
//!
//! Based on:
//! - `/lean4/src/kernel/environment.cpp`
//! - `/lean4/src/Lean/Environment.lean`
//! - `/lean4/src/kernel/declaration.cpp`

use super::*;

// ============================================================================
// Environment Creation and Management
// ============================================================================

// The `lean_mk_empty_environment` C export was removed in Lean 4.33
// (leanprover/lean4#14306). The Lean-compiled `mkEmptyEnvironment` function
// still exists as `l_Lean_mkEmptyEnvironment` with an identical ABI, so we
// dispatch on the version cfg.
#[cfg(not(lean_4_33))]
extern "C" {
    /// Create an empty environment with the specified trust level (Lean < 4.33)
    ///
    /// Trust level controls type checking:
    /// - 0: Full type checking (safest)
    /// - Higher values: Skip certain type checking operations (faster, less safe)
    ///
    /// Returns: `IO Environment` (requires IO execution to extract the environment)
    ///
    /// # Safety
    /// Must be called after lean runtime initialization
    #[link_name = "lean_mk_empty_environment"]
    fn lean_mk_empty_environment_old(trust_level: u32, world: lean_obj_arg) -> lean_obj_res;
}

#[cfg(lean_4_33)]
extern "C" {
    /// Create an empty environment with the specified trust level (Lean >= 4.33)
    ///
    /// Trust level controls type checking:
    /// - 0: Full type checking (safest)
    /// - Higher values: Skip certain type checking operations (faster, less safe)
    ///
    /// Returns: `IO Environment` (requires IO execution to extract the environment)
    ///
    /// # Safety
    /// Must be called after lean runtime initialization
    #[link_name = "l_Lean_mkEmptyEnvironment"]
    fn lean_mk_empty_environment_new(trust_level: u32, world: lean_obj_arg) -> lean_obj_res;
}

/// Create an empty environment with the specified trust level
///
/// Trust level controls type checking:
/// - 0: Full type checking (safest)
/// - Higher values: Skip certain type checking operations (faster, less safe)
///
/// Returns: `IO Environment` (requires IO execution to extract the environment)
///
/// Lean 4.33 removed the `lean_mk_empty_environment` export; the Lean-compiled
/// `l_Lean_mkEmptyEnvironment` symbol is used instead with an identical ABI.
///
/// # Safety
/// Must be called after lean runtime initialization
#[inline]
pub unsafe fn lean_mk_empty_environment(trust_level: u32, world: lean_obj_arg) -> lean_obj_res {
    #[cfg(not(lean_4_33))]
    {
        lean_mk_empty_environment_old(trust_level, world)
    }
    #[cfg(lean_4_33)]
    {
        lean_mk_empty_environment_new(trust_level, world)
    }
}

// ============================================================================
// Environment Queries
// ============================================================================

extern "C" {
    /// Find a constant by name in the environment
    ///
    /// # Parameters
    /// - `env`: Environment object (borrowed)
    /// - `name`: Name object (borrowed)
    ///
    /// # Returns
    /// `Option ConstantInfo` - Some if constant exists, None otherwise
    ///
    /// # Example (C++)
    /// ```cpp
    /// optional<constant_info> environment::find(name const & n) const;
    /// ```
    pub fn lean_environment_find(env: lean_obj_arg, name: lean_obj_arg) -> lean_obj_res;

    /// Check if the Quot type has been initialized in the environment
    ///
    /// # Returns
    /// 0 if not initialized, non-zero if initialized
    pub fn lean_environment_quot_init(env: lean_obj_arg) -> u8;
}

// ============================================================================
// Region Management
// ============================================================================

// `lean_environment_free_regions` is the direct C export of
// `Environment.freeRegions` (`src/Lean/Environment.lean`), the only release
// path for the C++ `compacted_region` buffers that hold a module import's
// olean payloads (`env.header.regions`).
//
// ABI by era: pre-4.26 the export threads the IO world token
// (`(env, world) -> EIO Unit`); 4.26+ (ST redesign) erases the singleton
// world, so the export takes only the environment (`(env) -> EIO Unit`).
// Verified by disassembly against the v4.20.0 / v4.25.2 binaries (world
// present, `rsi` saved and echoed into the result) and the v4.26.0 /
// v4.33.1 binaries (no `rsi` use at all).

extern "C" {
    /// `Environment.freeRegions (env : Environment) : IO Unit` — frees the
    /// compacted regions of an imported environment.
    ///
    /// **Linear**: consumes `env`. The compiled body extracts
    /// `env.header.regions`, `dec`s `env`, and only then frees the regions —
    /// so the caller must hand over its last reference to the environment
    /// and must hold no other live references to objects derived from the
    /// same import (they may share the freed region storage).
    ///
    /// # Safety
    /// - `env` must be a valid `Environment` object (consumed)
    /// - `world` must be a valid world token; it is ignored by the callee
    ///   (not decremented — the result carries its own fresh token)
    /// - Must be called on the Lean worker thread
    /// - Returns an `EIO Unit` result; the caller owns it
    #[cfg(not(lean_4_26))]
    pub fn lean_environment_free_regions(env: lean_obj_arg, world: lean_obj_arg) -> lean_obj_res;

    /// `Environment.freeRegions (env : Environment) : IO Unit` (Lean
    /// >= 4.26, ST redesign): the world token was erased, so the export
    /// takes only the environment. Same linear semantics and preconditions
    /// as the pre-4.26 form.
    ///
    /// # Safety
    /// - `env` must be a valid `Environment` object (consumed)
    /// - Must be called on the Lean worker thread
    /// - Returns an `EIO Unit` result; the caller owns it
    #[cfg(lean_4_26)]
    pub fn lean_environment_free_regions(env: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// Environment Modifications (Immutable Updates)
// ============================================================================

// `lean_add_decl` and `lean_elab_add_decl` gained a `maxRecDepth` argument in
// Lean 4.33 (leanprover/lean4#13956); we dispatch on the version cfg.
#[cfg(not(lean_4_33))]
extern "C" {
    /// Add a declaration to the kernel environment with type checking (Lean < 4.33)
    ///
    /// **Note**: This operates on `Lean.Kernel.Environment`, not `Lean.Environment`.
    /// For the elaborator environment (created by `lean_mk_empty_environment`),
    /// use `lean_elab_add_decl` instead.
    ///
    /// # Parameters
    /// - `env`: Kernel.Environment object (consumed)
    /// - `max_heartbeat`: Maximum heartbeats for type checking (0 = unlimited)
    /// - `decl`: Declaration object (borrowed @&)
    /// - `cancel_token`: Optional IO.CancelToken for cancellation (borrowed @&)
    ///
    /// # Returns
    /// `Except Kernel.Exception Kernel.Environment`
    #[link_name = "lean_add_decl"]
    fn lean_add_decl_old(
        env: lean_obj_arg,
        max_heartbeat: usize,
        decl: lean_obj_arg,
        cancel_token: lean_obj_arg,
    ) -> lean_obj_res;

    /// Add a declaration to the elaborator environment with type checking (Lean < 4.33)
    ///
    /// This operates on `Lean.Environment` (created by `lean_mk_empty_environment`).
    ///
    /// # Parameters
    /// - `env`: Environment object (consumed)
    /// - `max_heartbeat`: Maximum heartbeats for type checking (0 = unlimited)
    /// - `decl`: Declaration object (borrowed @&)
    /// - `cancel_token`: Optional IO.CancelToken for cancellation (borrowed @&)
    ///
    /// # Returns
    /// `Except Kernel.Exception Environment`
    #[link_name = "lean_elab_add_decl"]
    fn lean_elab_add_decl_old(
        env: lean_obj_arg,
        max_heartbeat: usize,
        decl: lean_obj_arg,
        cancel_token: lean_obj_arg,
    ) -> lean_obj_res;
}

#[cfg(lean_4_33)]
extern "C" {
    /// Add a declaration to the kernel environment with type checking (Lean >= 4.33)
    ///
    /// **Note**: This operates on `Lean.Kernel.Environment`, not `Lean.Environment`.
    /// For the elaborator environment (created by `lean_mk_empty_environment`),
    /// use `lean_elab_add_decl` instead.
    ///
    /// # Parameters
    /// - `env`: Kernel.Environment object (consumed)
    /// - `max_heartbeat`: Maximum heartbeats for type checking (0 = unlimited)
    /// - `max_rec_depth`: Maximum kernel recursion depth (0 = unlimited)
    /// - `decl`: Declaration object (borrowed @&)
    /// - `cancel_token`: Optional IO.CancelToken for cancellation (borrowed @&)
    ///
    /// # Returns
    /// `Except Kernel.Exception Kernel.Environment`
    #[link_name = "lean_add_decl"]
    fn lean_add_decl_new(
        env: lean_obj_arg,
        max_heartbeat: usize,
        max_rec_depth: usize,
        decl: lean_obj_arg,
        cancel_token: lean_obj_arg,
    ) -> lean_obj_res;

    /// Add a declaration to the elaborator environment with type checking (Lean >= 4.33)
    ///
    /// This operates on `Lean.Environment` (created by `lean_mk_empty_environment`).
    ///
    /// # Parameters
    /// - `env`: Environment object (consumed)
    /// - `max_heartbeat`: Maximum heartbeats for type checking (0 = unlimited)
    /// - `max_rec_depth`: Maximum kernel recursion depth (0 = unlimited)
    /// - `decl`: Declaration object (borrowed @&)
    /// - `cancel_token`: Optional IO.CancelToken for cancellation (borrowed @&)
    ///
    /// # Returns
    /// `Except Kernel.Exception Environment`
    #[link_name = "lean_elab_add_decl"]
    fn lean_elab_add_decl_new(
        env: lean_obj_arg,
        max_heartbeat: usize,
        max_rec_depth: usize,
        decl: lean_obj_arg,
        cancel_token: lean_obj_arg,
    ) -> lean_obj_res;
}

/// Add a declaration to the kernel environment with type checking
///
/// **Note**: This operates on `Lean.Kernel.Environment`, not `Lean.Environment`.
/// For the elaborator environment (created by `lean_mk_empty_environment`),
/// use `lean_elab_add_decl` instead.
///
/// # Parameters
/// - `env`: Kernel.Environment object (consumed)
/// - `max_heartbeat`: Maximum heartbeats for type checking (0 = unlimited)
/// - `decl`: Declaration object (borrowed @&)
/// - `cancel_token`: Optional IO.CancelToken for cancellation (borrowed @&)
///
/// # Returns
/// `Except Kernel.Exception Kernel.Environment`
///
/// Lean 4.33 added a `maxRecDepth` argument; `0` (unlimited) preserves the
/// pre-4.33 behavior.
#[inline]
pub unsafe fn lean_add_decl(
    env: lean_obj_arg,
    max_heartbeat: usize,
    decl: lean_obj_arg,
    cancel_token: lean_obj_arg,
) -> lean_obj_res {
    #[cfg(not(lean_4_33))]
    {
        lean_add_decl_old(env, max_heartbeat, decl, cancel_token)
    }
    #[cfg(lean_4_33)]
    {
        lean_add_decl_new(env, max_heartbeat, 0, decl, cancel_token)
    }
}

/// Add a declaration to the elaborator environment with type checking
///
/// This operates on `Lean.Environment` (created by `lean_mk_empty_environment`).
///
/// # Parameters
/// - `env`: Environment object (consumed)
/// - `max_heartbeat`: Maximum heartbeats for type checking (0 = unlimited)
/// - `decl`: Declaration object (borrowed @&)
/// - `cancel_token`: Optional IO.CancelToken for cancellation (borrowed @&)
///
/// # Returns
/// `Except Kernel.Exception Environment`
///
/// Lean 4.33 added a `maxRecDepth` argument; `0` (unlimited) preserves the
/// pre-4.33 behavior.
#[inline]
pub unsafe fn lean_elab_add_decl(
    env: lean_obj_arg,
    max_heartbeat: usize,
    decl: lean_obj_arg,
    cancel_token: lean_obj_arg,
) -> lean_obj_res {
    #[cfg(not(lean_4_33))]
    {
        lean_elab_add_decl_old(env, max_heartbeat, decl, cancel_token)
    }
    #[cfg(lean_4_33)]
    {
        lean_elab_add_decl_new(env, max_heartbeat, 0, decl, cancel_token)
    }
}

extern "C" {
    /// Add a declaration to the kernel environment without type checking
    ///
    /// **Note**: This operates on `Lean.Kernel.Environment`, not `Lean.Environment`.
    /// For the elaborator environment, use `lean_elab_add_decl_without_checking` instead.
    ///
    /// # Parameters
    /// - `env`: Kernel.Environment object (consumed)
    /// - `decl`: Declaration object (borrowed @&)
    ///
    /// # Returns
    /// `Except Kernel.Exception Kernel.Environment`
    pub fn lean_add_decl_without_checking(env: lean_obj_arg, decl: lean_obj_arg) -> lean_obj_res;

    /// Add a declaration to the elaborator environment without type checking
    ///
    /// This operates on `Lean.Environment` (created by `lean_mk_empty_environment`).
    ///
    /// # Parameters
    /// - `env`: Environment object (consumed)
    /// - `decl`: Declaration object (borrowed @&)
    ///
    /// # Returns
    /// `Except Kernel.Exception Environment`
    pub fn lean_elab_add_decl_without_checking(
        env: lean_obj_arg,
        decl: lean_obj_arg,
    ) -> lean_obj_res;

    /// Mark the Quot type as initialized
    ///
    /// # Parameters
    /// - `env`: Environment object (consumed)
    ///
    /// # Returns
    /// New environment with Quot marked as initialized
    pub fn lean_environment_mark_quot_init(env: lean_obj_arg) -> lean_obj_res;

    /// Internal: Add a constant info directly to the environment
    ///
    /// This is a low-level function typically not used directly.
    /// Prefer `lean_add_decl` or `lean_add_decl_without_checking`.
    ///
    /// # Parameters
    /// - `env`: Environment object (consumed)
    /// - `cinfo`: ConstantInfo object (borrowed)
    ///
    /// # Returns
    /// New environment with the constant added
    pub fn lean_environment_add(env: lean_obj_arg, cinfo: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// Diagnostics
// ============================================================================

extern "C" {
    /// Check if kernel diagnostics are enabled
    ///
    /// # Parameters
    /// - `diag`: Diagnostics object (borrowed)
    ///
    /// # Returns
    /// 0 if disabled, non-zero if enabled
    pub fn lean_kernel_diag_is_enabled(diag: lean_obj_arg) -> u8;

    /// Record an unfold operation in diagnostics
    ///
    /// # Parameters
    /// - `diag`: Diagnostics object (consumed)
    /// - `decl_name`: Name of the declaration being unfolded (borrowed)
    ///
    /// # Returns
    /// New diagnostics object with the unfold recorded
    pub fn lean_kernel_record_unfold(diag: lean_obj_arg, decl_name: lean_obj_arg) -> lean_obj_res;

    /// Get the diagnostics from an environment
    ///
    /// # Parameters
    /// - `env`: Environment object (borrowed)
    ///
    /// # Returns
    /// Diagnostics object
    pub fn lean_kernel_get_diag(env: lean_obj_arg) -> lean_obj_res;

    /// Set the diagnostics for an environment
    ///
    /// # Parameters
    /// - `env`: Environment object (consumed)
    /// - `diag`: Diagnostics object (borrowed)
    ///
    /// # Returns
    /// New environment with updated diagnostics
    pub fn lean_kernel_set_diag(env: lean_obj_arg, diag: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// Environment Conversion (Kernel ↔ Elab)
// ============================================================================

extern "C" {
    /// Convert a kernel environment to an elaborator environment
    ///
    /// # Parameters
    /// - `env`: Kernel environment (borrowed)
    ///
    /// # Returns
    /// Elaborator environment
    pub fn lean_elab_environment_of_kernel_env(env: lean_obj_arg) -> lean_obj_res;

    /// Convert an elaborator environment to a kernel environment
    ///
    /// # Parameters
    /// - `env`: Elaborator environment (borrowed)
    ///
    /// # Returns
    /// Kernel environment
    pub fn lean_elab_environment_to_kernel_env(env: lean_obj_arg) -> lean_obj_res;
}

// ============================================================================
// Declaration Construction
// ============================================================================

extern "C" {
    // These constructors are emitted by the Lean compiler and are available as
    // `*_val` symbols (there is no `lean_mk_*` symbol in Lean's shipped libraries).
    fn lean_mk_axiom_val(
        name: lean_obj_arg,
        level_params: lean_obj_arg,
        type_: lean_obj_arg,
        is_unsafe: u8,
    ) -> lean_obj_res;

    fn lean_mk_definition_val(
        name: lean_obj_arg,
        level_params: lean_obj_arg,
        type_: lean_obj_arg,
        value: lean_obj_arg,
        hints: lean_obj_arg,
        safety: u8,
        all: lean_obj_arg,
    ) -> lean_obj_res;

    fn lean_mk_theorem_val(
        name: lean_obj_arg,
        level_params: lean_obj_arg,
        type_: lean_obj_arg,
        value: lean_obj_arg,
        all: lean_obj_arg,
    ) -> lean_obj_res;
}

/// Create an axiom declaration.
pub unsafe fn lean_mk_axiom(
    name: lean_obj_arg,
    level_params: lean_obj_arg,
    type_: lean_obj_arg,
    is_unsafe: u8,
) -> lean_obj_res {
    lean_mk_axiom_val(name, level_params, type_, is_unsafe)
}

/// Create a definition declaration.
pub unsafe fn lean_mk_definition(
    name: lean_obj_arg,
    level_params: lean_obj_arg,
    type_: lean_obj_arg,
    value: lean_obj_arg,
    hints: lean_obj_arg,
    safety: u8,
    all: lean_obj_arg,
) -> lean_obj_res {
    lean_mk_definition_val(name, level_params, type_, value, hints, safety, all)
}

/// Create a theorem declaration.
pub unsafe fn lean_mk_theorem(
    name: lean_obj_arg,
    level_params: lean_obj_arg,
    type_: lean_obj_arg,
    value: lean_obj_arg,
    all: lean_obj_arg,
) -> lean_obj_res {
    lean_mk_theorem_val(name, level_params, type_, value, all)
}

// ============================================================================
// ConstantInfo Accessors
// ============================================================================

extern "C" {
    // These are Lean-compiled helper functions (from `Lean/Declaration`), shipped in Lean's
    // libraries as `l_Lean_*` symbols.
    fn l_Lean_ConstantInfo_name(cinfo: lean_obj_arg) -> lean_obj_res;
    fn l_Lean_ConstantInfo_type(cinfo: lean_obj_arg) -> lean_obj_res;
    fn l_Lean_ConstantInfo_levelParams(cinfo: lean_obj_arg) -> lean_obj_res;
    fn l_Lean_ConstantInfo_hasValue(cinfo: lean_obj_arg) -> u8;
    fn l_Lean_ConstantInfo_value_x21(cinfo: lean_obj_arg) -> lean_obj_res;
    fn l_Lean_ConstantKind_ofConstantInfo(cinfo: lean_obj_arg) -> u8;
}

pub unsafe fn lean_constant_info_name(cinfo: lean_obj_arg) -> lean_obj_res {
    l_Lean_ConstantInfo_name(cinfo)
}

pub unsafe fn lean_constant_info_type(cinfo: lean_obj_arg) -> lean_obj_res {
    l_Lean_ConstantInfo_type(cinfo)
}

pub unsafe fn lean_constant_info_level_params(cinfo: lean_obj_arg) -> lean_obj_res {
    l_Lean_ConstantInfo_levelParams(cinfo)
}

pub unsafe fn lean_constant_info_kind(cinfo: lean_obj_arg) -> u8 {
    l_Lean_ConstantKind_ofConstantInfo(cinfo)
}

pub unsafe fn lean_constant_info_has_value(cinfo: lean_obj_arg) -> u8 {
    l_Lean_ConstantInfo_hasValue(cinfo)
}

pub unsafe fn lean_constant_info_value(cinfo: lean_obj_arg) -> lean_obj_res {
    l_Lean_ConstantInfo_value_x21(cinfo)
}

// ============================================================================
// Constants for Constant Kinds
// ============================================================================

/// Constant kind: Definition (defn)
pub const LEAN_CONSTANT_KIND_DEFINITION: u8 = 0;
/// Constant kind: Theorem (thm)
pub const LEAN_CONSTANT_KIND_THEOREM: u8 = 1;
/// Constant kind: Axiom
pub const LEAN_CONSTANT_KIND_AXIOM: u8 = 2;
/// Constant kind: Opaque definition
pub const LEAN_CONSTANT_KIND_OPAQUE: u8 = 3;
/// Constant kind: Quot type
pub const LEAN_CONSTANT_KIND_QUOT: u8 = 4;
/// Constant kind: Inductive type
pub const LEAN_CONSTANT_KIND_INDUCTIVE: u8 = 5;
/// Constant kind: Constructor
pub const LEAN_CONSTANT_KIND_CONSTRUCTOR: u8 = 6;
/// Constant kind: Recursor
pub const LEAN_CONSTANT_KIND_RECURSOR: u8 = 7;

// ============================================================================
// Safety Levels for Definitions
// ============================================================================

/// Definition safety: Unsafe (axiom-like)
pub const LEAN_DEF_SAFETY_UNSAFE: u8 = 0;
/// Definition safety: Safe (fully type-checked)
pub const LEAN_DEF_SAFETY_SAFE: u8 = 1;
/// Definition safety: Partial (may not terminate)
pub const LEAN_DEF_SAFETY_PARTIAL: u8 = 2;
