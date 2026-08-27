//! High-level wrapper for Lean's Environment
//!
//! Environments are the core of Lean's module system, storing all declarations,
//! constants, inductive types, and type class instances. Environments are immutable -
//! all modifications return a new environment.

use crate::err::LeanError;
use crate::instance::LeanBound;
use crate::marker::Lean;
use crate::runtime::with_worker;
use crate::types::LeanList;
use crate::LeanResult;
use leo3_ffi as ffi;

use super::declaration::LeanDeclaration;
use super::name::LeanName;

/// Lean environment - immutable collection of declarations
///
/// Environments store:
/// - Constants (axioms, definitions, theorems)
/// - Inductive types and constructors
/// - Type class instances
/// - Attributes and metadata
///
/// All modifications create a new environment (immutable updates).
///
/// # Example
///
/// ```ignore
/// use leo3::prelude::*;
/// use leo3::meta::*;
///
/// leo3::with_lean(|lean| {
///     // Create empty environment
///     let env = LeanEnvironment::empty(lean, 0)?;
///
///     // Find a constant
///     let nat_name = LeanName::from_str(lean, "Nat")?;
///     if let Some(cinfo) = LeanEnvironment::find(&env, &nat_name)? {
///         println!("Found Nat with type: {:?}", LeanConstantInfo::type_(&cinfo)?);
///     }
///
///     Ok(())
/// })
/// ```
#[repr(transparent)]
pub struct LeanEnvironment {
    _private: (),
}

impl LeanEnvironment {
    /// Create a new empty environment
    ///
    /// # Arguments
    ///
    /// * `lean` - Lean lifetime token
    /// * `trust_level` - Type checking trust level:
    ///   - `0` = Full type checking (safest, slowest)
    ///   - Higher values = Skip certain checks (faster, less safe)
    ///
    /// # Returns
    ///
    /// New empty environment
    ///
    /// # Example
    ///
    /// ```ignore
    /// let env = LeanEnvironment::empty(lean, 0)?;
    /// ```
    pub fn empty<'l>(lean: Lean<'l>, trust_level: u32) -> LeanResult<LeanBound<'l, Self>> {
        crate::runtime::ensure_meta_initialized();
        // `lean_mk_empty_environment` refuses to run while
        // `IO.initializing == true` ("environment objects cannot be created
        // during initialization"). `import_modules_with_exts` ends the
        // initialization phase before importing; do the same here (the call
        // is idempotent) so an empty environment can be created without a
        // prior import.
        crate::runtime::finalize_initialization();
        let env_ptr = with_worker(move || unsafe {
            let world = ffi::io::lean_io_mk_world();
            let io_result = ffi::environment::lean_mk_empty_environment(trust_level, world);

            if ffi::io::lean_io_result_is_error(io_result) {
                ffi::lean_dec(io_result);
                return Err(LeanError::null_pointer("lean_mk_empty_environment"));
            }

            Ok(ffi::io::lean_io_result_take_value(io_result))
        })?;

        unsafe { Ok(LeanBound::from_owned_ptr(lean, env_ptr)) }
    }

    /// Find a constant by name in the environment
    ///
    /// # Arguments
    ///
    /// * `env` - Environment to search (borrowed)
    /// * `name` - Name of the constant to find
    ///
    /// # Returns
    ///
    /// `Some(ConstantInfo)` if found, `None` if not found
    ///
    /// # Example
    ///
    /// ```ignore
    /// let name = LeanName::from_str(lean, "Nat.add")?;
    /// if let Some(cinfo) = LeanEnvironment::find(&env, &name)? {
    ///     println!("Found: {:?}", LeanConstantInfo::name(&cinfo)?);
    /// }
    /// ```
    pub fn find<'l>(
        env: &LeanBound<'l, Self>,
        name: &LeanBound<'l, LeanName>,
    ) -> LeanResult<Option<LeanBound<'l, LeanConstantInfo>>> {
        let lean = env.lean_token();
        let env_ptr = env.as_ptr();
        let name_ptr = name.as_ptr();

        let result_ptr = with_worker(move || unsafe {
            // lean_elab_environment_to_kernel_env consumes its argument,
            // so inc the borrowed env_ptr before the call.
            ffi::lean_inc(env_ptr);
            let kenv = ffi::environment::lean_elab_environment_to_kernel_env(env_ptr);

            // lean_environment_find consumes both kenv and name_ptr,
            // so inc the borrowed name_ptr. kenv is already owned (from above).
            ffi::lean_inc(name_ptr);
            ffi::environment::lean_environment_find(kenv, name_ptr)
        });

        unsafe {
            // Result is Option ConstantInfo
            // Option.none is scalar (lean_box(0)), Option.some is tag 1
            if ffi::inline::lean_is_scalar(result_ptr) {
                Ok(None)
            } else {
                let tag = ffi::lean_obj_tag(result_ptr);
                if tag == 0 {
                    // Option.none as ctor (shouldn't happen, but handle it)
                    ffi::lean_dec(result_ptr);
                    Ok(None)
                } else {
                    // Option.some — extract the value (field 0)
                    let cinfo_ptr = ffi::lean_ctor_get(result_ptr, 0) as *mut ffi::lean_object;
                    ffi::lean_inc(cinfo_ptr);
                    ffi::lean_dec(result_ptr);
                    Ok(Some(LeanBound::from_owned_ptr(lean, cinfo_ptr)))
                }
            }
        }
    }

    /// Check if the Quot type has been initialized in the environment
    ///
    /// # Arguments
    ///
    /// * `env` - Environment to check
    ///
    /// # Returns
    ///
    /// `true` if Quot is initialized, `false` otherwise
    pub fn is_quot_init<'l>(env: &LeanBound<'l, Self>) -> bool {
        let env_ptr = env.as_ptr();
        with_worker(move || unsafe {
            // lean_elab_environment_to_kernel_env consumes its argument,
            // so inc the borrowed env_ptr.
            ffi::lean_inc(env_ptr);
            let kenv = ffi::environment::lean_elab_environment_to_kernel_env(env_ptr);
            // lean_environment_quot_init consumes kenv (no @& annotation).
            ffi::environment::lean_environment_quot_init(kenv) != 0
        })
    }

    /// Mark the Quot type as initialized
    ///
    /// # Arguments
    ///
    /// * `env` - Environment to modify (consumed)
    ///
    /// # Returns
    ///
    /// New environment with Quot marked as initialized
    pub fn mark_quot_init<'l>(env: LeanBound<'l, Self>) -> LeanBound<'l, Self> {
        let lean = env.lean_token();
        let env_ptr = env.into_ptr();
        let result_ptr = with_worker(move || unsafe {
            ffi::environment::lean_environment_mark_quot_init(env_ptr)
        });
        unsafe { LeanBound::from_owned_ptr(lean, result_ptr) }
    }

    /// Add a declaration to the environment with kernel type checking.
    ///
    /// This performs full type checking on the declaration before adding it.
    /// Returns a new environment containing the declaration, or an error
    /// if the declaration is ill-typed.
    ///
    /// # Arguments
    ///
    /// * `env` - Environment to add the declaration to (borrowed)
    /// * `decl` - Declaration to add
    ///
    /// # Example
    ///
    /// ```ignore
    /// let decl = LeanDeclaration::axiom(lean, name, levels, type_, false)?;
    /// let new_env = LeanEnvironment::add_decl(&env, &decl)?;
    /// ```
    pub fn add_decl<'l>(
        env: &LeanBound<'l, Self>,
        decl: &LeanBound<'l, LeanDeclaration>,
    ) -> LeanResult<LeanBound<'l, Self>> {
        let lean = env.lean_token();
        let env_ptr = env.clone().into_ptr();
        // Although decl is @& (borrowed), the C++ error path may store
        // a reference to it in the KernelException. We increment the
        // refcount to ensure the declaration outlives the error result.
        let decl_ptr = decl.as_ptr();
        unsafe { ffi::lean_inc(decl_ptr) };

        let result_ptr = with_worker(move || unsafe {
            let cancel_token = ffi::inline::lean_box(0);
            let result = ffi::environment::lean_elab_add_decl(env_ptr, 0, decl_ptr, cancel_token);
            Self::handle_except_result_raw(result)
        })?;

        unsafe { Ok(LeanBound::from_owned_ptr(lean, result_ptr)) }
    }

    /// Add a declaration to the environment without type checking.
    ///
    /// **Warning**: This skips type checking entirely. Only use if you are certain
    /// the declaration is well-typed. Much faster than `add_decl` but unsafe
    /// in the Lean sense.
    ///
    /// # Arguments
    ///
    /// * `env` - Environment to add the declaration to (borrowed)
    /// * `decl` - Declaration to add
    ///
    /// # Example
    ///
    /// ```ignore
    /// let decl = LeanDeclaration::axiom(lean, name, levels, type_, false)?;
    /// let new_env = LeanEnvironment::add_decl_unchecked(&env, &decl)?;
    /// ```
    pub fn add_decl_unchecked<'l>(
        env: &LeanBound<'l, Self>,
        decl: &LeanBound<'l, LeanDeclaration>,
    ) -> LeanResult<LeanBound<'l, Self>> {
        // Pre-check: the unchecked FFI path SIGABRTs on duplicate names.
        // Extract the declaration's name and check if it already exists.
        let decl_name = LeanDeclaration::name(decl);
        if Self::find(env, &decl_name)?.is_some() {
            return Err(LeanError::kernel_exception(
                crate::err::KernelExceptionCode::AlreadyDeclared,
            ));
        }

        let lean = env.lean_token();
        let env_ptr = env.clone().into_ptr();
        // Match add_decl: increment refcount because the C++ error path may
        // store a reference to the declaration in the KernelException.
        let decl_ptr = decl.as_ptr();
        unsafe { ffi::lean_inc(decl_ptr) };

        let result_ptr = with_worker(move || unsafe {
            let result = ffi::environment::lean_elab_add_decl_without_checking(env_ptr, decl_ptr);
            Self::handle_except_result_raw(result)
        })?;

        unsafe { Ok(LeanBound::from_owned_ptr(lean, result_ptr)) }
    }

    /// Handle an `Except KernelException Environment` result from FFI.
    ///
    /// Tag 0 = `Except.error`, field 0 is the `KernelException`.
    /// Tag 1 = `Except.ok`, field 0 is the new environment.
    ///
    /// Returns a raw pointer on success (caller must wrap in `LeanBound`).
    /// This variant is `Send`-safe and used by the worker thread.
    unsafe fn handle_except_result_raw(
        result: *mut ffi::lean_object,
    ) -> LeanResult<*mut ffi::lean_object> {
        let tag = ffi::lean_obj_tag(result);
        if tag == 1 {
            // Except.ok — extract the new environment
            let new_env_ptr = ffi::lean_ctor_get(result, 0) as *mut ffi::lean_object;
            ffi::lean_inc(new_env_ptr);
            ffi::lean_dec(result);
            Ok(new_env_ptr)
        } else {
            // Except.error — extract KernelException tag for a descriptive message
            let exception_ptr = ffi::lean_ctor_get(result, 0) as *mut ffi::lean_object;
            let exc_tag = if ffi::inline::lean_is_scalar(exception_ptr) {
                (exception_ptr as usize) >> 1
            } else {
                ffi::lean_obj_tag(exception_ptr) as usize
            };
            ffi::lean_dec(result);

            let code = crate::err::KernelExceptionCode::from_tag(exc_tag);
            Err(LeanError::kernel_exception(code))
        }
    }
}

impl<'l> LeanBound<'l, LeanEnvironment> {
    /// Free the compacted regions backing this environment's imported
    /// modules, releasing the C++ `compacted_region` buffers that hold the
    /// olean payloads.
    ///
    /// W-407 Bug A: `importModules` compacts each module's olean payload
    /// into C++ `compacted_region` buffers attached to the environment
    /// header (`env.header.regions`) — ~1.4 GB for a full `Lean` import.
    /// Nothing else ever releases those buffers: `lean_dec` on the
    /// environment only drops reference counts, and the native runtime has
    /// no mark-sweep GC. Lean's `Environment.freeRegions` is the only
    /// release path, and the stock runtime only invokes it from the
    /// one-shot `lean` CLI path (`withImportModules`) — so embedded callers
    /// that call `importModules` repeatedly (e.g. one environment per
    /// session) permanently leak one ~1.4–1.6 GB buffer per import. This
    /// method is the Rust-side entry to that release.
    ///
    /// **Consumes `self`.** `freeRegions` is linear: the callee `dec`s the
    /// environment before freeing the regions. After this call, the
    /// environment and **every Lean object derived from its import**
    /// (expressions, names, tactic contexts, `MetaMContext`s built on it,
    /// ...) are dangling and must not be used. If another environment was
    /// derived from this one (e.g. via `run_cmd` or `add_decl`), drop it
    /// and everything derived from it **before** calling this method.
    ///
    /// # Safety
    ///
    /// The caller must guarantee that `self` is the **last live reference**
    /// to this environment: no other Rust value may hold the environment
    /// or any object allocated during its import. `LeanBound` is `Clone`,
    /// and every clone (and every object derived from the import) keeps
    /// the same Lean reference count alive — releasing the regions while
    /// any of them still lives, or using/dropping one afterwards, is a
    /// use-after-free or heap corruption. This restates `freeRegions`'
    /// documented precondition ("no live references to imported objects
    /// may exist at the time of invocation"), which a consuming method on
    /// a `Clone` type cannot enforce from safe Rust.
    ///
    /// # Errors
    ///
    /// Returns the Lean exception if the region-free IO step fails
    /// (normally unreachable).
    pub unsafe fn free_regions(self) -> LeanResult<()> {
        let env_ptr = self.into_ptr();
        // W-417: serialize this free against the keepalive import/snapshot/
        // record/re-map sequences (reentrant, so a drop holding the lock across
        // its whole sequence does not deadlock). Held on this (caller) thread
        // across the `with_worker` free below.
        #[cfg(all(lean_4_25, target_os = "linux"))]
        let _lifecycle = super::keepalive::lifecycle_lock();
        with_worker(move || unsafe {
            #[cfg(not(lean_4_26))]
            {
                let world = ffi::io::lean_io_mk_world();
                let result = ffi::environment::lean_environment_free_regions(env_ptr, world);
                // The callee ignores `world` (the result carries its own
                // fresh token), so release it here.
                ffi::lean_dec(world);
                // `EIO Unit`: tag 0 = ok(unit) — the payload is a scalar
                // unit, never dec'd; tag 1 = error (rendered by
                // `handle_eio_result`).
                super::metam::handle_eio_result(result).map(|_| ())
            }
            #[cfg(lean_4_26)]
            {
                // Lean >= 4.26 (ST redesign): the world token is erased
                // from the export; same `EIO Unit` result handling.
                let result = ffi::environment::lean_environment_free_regions(env_ptr);
                super::metam::handle_eio_result(result).map(|_| ())
            }
        })
    }
}

/// Information about a constant in the environment
///
/// Constants include axioms, definitions, theorems, inductive types, constructors, etc.
#[repr(transparent)]
pub struct LeanConstantInfo {
    _private: (),
}

impl LeanConstantInfo {
    /// Get the name of the constant
    ///
    /// # Example
    ///
    /// ```ignore
    /// let name = LeanConstantInfo::name(&cinfo)?;
    /// println!("Constant name: {:?}", name);
    /// ```
    pub fn name<'l>(cinfo: &LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, LeanName>> {
        unsafe {
            let lean = cinfo.lean_token();
            // The bound `l_Lean_ConstantInfo_*` symbols consume their
            // `ConstantInfo` argument (verified against the 4.25.2 runtime
            // disassembly), so hand over an owned reference.
            ffi::lean_inc(cinfo.as_ptr());
            let ptr = ffi::environment::lean_constant_info_name(cinfo.as_ptr());
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Get the type of the constant
    ///
    /// # Example
    ///
    /// ```ignore
    /// let type_expr = LeanConstantInfo::type_(&cinfo)?;
    /// ```
    pub fn type_<'l>(
        cinfo: &LeanBound<'l, Self>,
    ) -> LeanResult<LeanBound<'l, super::expr::LeanExpr>> {
        unsafe {
            let lean = cinfo.lean_token();
            // The bound `l_Lean_ConstantInfo_*` symbols consume their
            // `ConstantInfo` argument (verified against the 4.25.2 runtime
            // disassembly), so hand over an owned reference.
            ffi::lean_inc(cinfo.as_ptr());
            let ptr = ffi::environment::lean_constant_info_type(cinfo.as_ptr());
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Get the universe level parameters of the constant
    ///
    /// # Example
    ///
    /// ```ignore
    /// let level_params = LeanConstantInfo::level_params(&cinfo)?;
    /// ```
    pub fn level_params<'l>(cinfo: &LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, LeanList>> {
        unsafe {
            let lean = cinfo.lean_token();
            // The bound `l_Lean_ConstantInfo_*` symbols consume their
            // `ConstantInfo` argument (verified against the 4.25.2 runtime
            // disassembly), so hand over an owned reference.
            ffi::lean_inc(cinfo.as_ptr());
            let ptr = ffi::environment::lean_constant_info_level_params(cinfo.as_ptr());
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Get the kind of the constant
    ///
    /// # Example
    ///
    /// ```ignore
    /// match LeanConstantInfo::kind(&cinfo) {
    ///     ConstantKind::Axiom => println!("This is an axiom"),
    ///     ConstantKind::Definition => println!("This is a definition"),
    ///     ConstantKind::Theorem => println!("This is a theorem"),
    ///     _ => println!("Other kind"),
    /// }
    /// ```
    pub fn kind<'l>(cinfo: &LeanBound<'l, Self>) -> ConstantKind {
        unsafe {
            let kind_u8 = ffi::environment::lean_constant_info_kind(cinfo.as_ptr());
            ConstantKind::from_u8(kind_u8)
        }
    }

    /// Check if the constant has a value (is a definition or theorem)
    ///
    /// # Returns
    ///
    /// `true` if the constant has a value, `false` for axioms
    pub fn has_value<'l>(cinfo: &LeanBound<'l, Self>) -> bool {
        unsafe { ffi::environment::lean_constant_info_has_value(cinfo.as_ptr()) != 0 }
    }

    /// Get the value of the constant (only valid if `has_value` returns true)
    ///
    /// # Returns
    ///
    /// `Some(expr)` if the constant has a value (definition/theorem),
    /// `None` if it doesn't (axiom)
    ///
    /// # Example
    ///
    /// ```ignore
    /// if let Some(value) = LeanConstantInfo::value(&cinfo)? {
    ///     println!("Definition value: {:?}", value);
    /// }
    /// ```
    pub fn value<'l>(
        cinfo: &LeanBound<'l, Self>,
    ) -> LeanResult<Option<LeanBound<'l, super::expr::LeanExpr>>> {
        if !Self::has_value(cinfo) {
            return Ok(None);
        }

        unsafe {
            let lean = cinfo.lean_token();
            // The bound `l_Lean_ConstantInfo_*` symbols consume their
            // `ConstantInfo` argument (verified against the 4.25.2 runtime
            // disassembly), so hand over an owned reference.
            ffi::lean_inc(cinfo.as_ptr());
            let ptr = ffi::environment::lean_constant_info_value(cinfo.as_ptr());
            Ok(Some(LeanBound::from_owned_ptr(lean, ptr)))
        }
    }
}

/// The kind of a constant in the environment
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConstantKind {
    /// Axiom: assumed without proof
    Axiom,
    /// Definition: constant with computational content
    Definition,
    /// Theorem: proved proposition (never reduced)
    Theorem,
    /// Opaque definition: definition that won't be unfolded
    Opaque,
    /// Quot: quotient type
    Quot,
    /// Inductive type declaration
    Inductive,
    /// Constructor of an inductive type
    Constructor,
    /// Recursor for an inductive type
    Recursor,
}

impl ConstantKind {
    /// Convert from FFI u8 representation
    pub(crate) fn from_u8(val: u8) -> Self {
        match val {
            ffi::environment::LEAN_CONSTANT_KIND_AXIOM => Self::Axiom,
            ffi::environment::LEAN_CONSTANT_KIND_DEFINITION => Self::Definition,
            ffi::environment::LEAN_CONSTANT_KIND_THEOREM => Self::Theorem,
            ffi::environment::LEAN_CONSTANT_KIND_OPAQUE => Self::Opaque,
            ffi::environment::LEAN_CONSTANT_KIND_QUOT => Self::Quot,
            ffi::environment::LEAN_CONSTANT_KIND_INDUCTIVE => Self::Inductive,
            ffi::environment::LEAN_CONSTANT_KIND_CONSTRUCTOR => Self::Constructor,
            ffi::environment::LEAN_CONSTANT_KIND_RECURSOR => Self::Recursor,
            _ => Self::Axiom, // Fallback for unknown values
        }
    }

    /// Convert to FFI u8 representation
    #[allow(dead_code)]
    pub(crate) fn to_u8(self) -> u8 {
        match self {
            Self::Axiom => ffi::environment::LEAN_CONSTANT_KIND_AXIOM,
            Self::Definition => ffi::environment::LEAN_CONSTANT_KIND_DEFINITION,
            Self::Theorem => ffi::environment::LEAN_CONSTANT_KIND_THEOREM,
            Self::Opaque => ffi::environment::LEAN_CONSTANT_KIND_OPAQUE,
            Self::Quot => ffi::environment::LEAN_CONSTANT_KIND_QUOT,
            Self::Inductive => ffi::environment::LEAN_CONSTANT_KIND_INDUCTIVE,
            Self::Constructor => ffi::environment::LEAN_CONSTANT_KIND_CONSTRUCTOR,
            Self::Recursor => ffi::environment::LEAN_CONSTANT_KIND_RECURSOR,
        }
    }
}

impl std::fmt::Display for ConstantKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Axiom => write!(f, "axiom"),
            Self::Definition => write!(f, "def"),
            Self::Theorem => write!(f, "theorem"),
            Self::Opaque => write!(f, "opaque"),
            Self::Quot => write!(f, "quot"),
            Self::Inductive => write!(f, "inductive"),
            Self::Constructor => write!(f, "constructor"),
            Self::Recursor => write!(f, "recursor"),
        }
    }
}
