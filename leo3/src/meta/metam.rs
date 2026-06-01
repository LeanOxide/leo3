//! High-level wrapper for running Lean's MetaM monad computations from Rust.
//!
//! Lean's `MetaM` monad is the primary interface for type-checking, unification,
//! and metavariable management in Lean 4. It sits atop the `CoreM` monad and
//! adds a local context, metavariable context, and type-inference caches.
//!
//! This module provides [`MetaMContext`], which bundles all four context/state
//! objects required by the MetaM monad stack:
//!
//! | Lean type | Rust wrapper | Role |
//! |-----------|-------------|------|
//! | `Core.Context` | [`CoreContext`] | Read-only core settings (recursion depth, heartbeats, etc.) |
//! | `Core.State` | [`CoreState`] | Mutable core state (environment, name generator, messages) |
//! | `Meta.Context` | [`MetaContext`] | Read-only meta settings (local context, config) |
//! | `Meta.State` | [`MetaState`] | Mutable meta state (metavariable context, caches) |
//!
//! # Usage
//!
//! ```ignore
//! use leo3::prelude::*;
//! use leo3::meta::*;
//!
//! leo3::with_lean(|lean| {
//!     let env = LeanEnvironment::empty(lean, 0)?;
//!     let mut ctx = MetaMContext::new(lean, env)?;
//!
//!     // Once Phase 2 FFI bindings are available:
//!     // let result = ctx.run(some_metam_computation)?;
//!     Ok(())
//! })?;
//! ```
//!
//! # Architecture
//!
//! `MetaMContext::run()` calls the Lean FFI function `lean_meta_metam_run`,
//! which executes a `MetaM α` computation and returns `EIO Exception α`
//! (i.e., `Except Exception α` in the IO monad). The result is decoded
//! internally, extracting the success value or converting the `Exception`
//! into a [`LeanError::Exception`].
//!
//! Based on `/lean4/src/Lean/Meta/Basic.lean` (Issue #30).

use crate::err::{LeanError, LeanResult};
use crate::instance::{LeanAny, LeanBound};
use crate::marker::Lean;
use crate::meta::context::{CoreContext, CoreState, MetaContext, MetaState};
use crate::meta::expr::LeanExpr;
use crate::meta::name::LeanName;
use crate::meta::LeanEnvironment;
use crate::types::{LeanNat, LeanOption};
use leo3_ffi as ffi;
use std::ffi::CStr;

/// Context for running MetaM computations.
///
/// `MetaMContext` bundles together all the context and state objects required
/// by Lean's MetaM monad: [`CoreContext`], [`CoreState`], [`MetaContext`], and
/// [`MetaState`]. It provides a [`run()`](Self::run) method to execute MetaM
/// computations and can be reused across multiple calls (context/state objects
/// are cloned before each FFI invocation).
///
/// # Example
///
/// ```ignore
/// use leo3::prelude::*;
/// use leo3::meta::*;
///
/// leo3::with_lean(|lean| {
///     let env = LeanEnvironment::empty(lean, 0)?;
///     let mut ctx = MetaMContext::new(lean, env)?;
///
///     // Access the environment
///     assert!(!ctx.env().as_ptr().is_null());
///
///     // The Lean runtime token is also available
///     let _lean = ctx.lean();
///     Ok(())
/// })?;
/// ```
pub struct MetaMContext<'l> {
    lean: Lean<'l>,
    env: LeanBound<'l, LeanEnvironment>,
    core_ctx: LeanBound<'l, CoreContext>,
    core_state: LeanBound<'l, CoreState>,
    meta_ctx: LeanBound<'l, MetaContext>,
    meta_state: LeanBound<'l, MetaState>,
}

#[derive(Clone)]
pub(crate) struct MVarDeclParts<'l> {
    pub lctx: LeanBound<'l, LeanAny>,
    pub type_: LeanBound<'l, LeanExpr>,
    pub local_instances: LeanBound<'l, LeanAny>,
}

impl<'l> MetaMContext<'l> {
    /// Create a new `MetaMContext` from an environment.
    ///
    /// Constructs all required context and state objects with default values:
    /// - `Core.Context`: default settings (`maxRecDepth=1000`, `maxHeartbeats=200_000_000`)
    /// - `Core.State`: initialized with the given environment
    /// - `Meta.Context`: default Meta configuration (empty local context)
    /// - `Meta.State`: empty metavariable context and caches
    ///
    /// # Errors
    ///
    /// Returns [`LeanError`] if any of the underlying context/state constructors
    /// fail (e.g., due to allocation failure).
    ///
    /// # Example
    ///
    /// ```ignore
    /// use leo3::prelude::*;
    /// use leo3::meta::*;
    ///
    /// leo3::with_lean(|lean| {
    ///     let env = LeanEnvironment::empty(lean, 0)?;
    ///     let ctx = MetaMContext::new(lean, env)?;
    ///     assert!(!ctx.env().as_ptr().is_null());
    ///     Ok(())
    /// })?;
    /// ```
    pub fn new(lean: Lean<'l>, env: LeanBound<'l, LeanEnvironment>) -> LeanResult<Self> {
        let core_ctx = CoreContext::mk_default(lean)?;
        let core_state = CoreState::mk_core_state(lean, &env)?;
        let meta_ctx = MetaContext::mk_default(lean)?;
        let meta_state = MetaState::mk_meta_state(lean)?;

        Ok(Self {
            lean,
            env,
            core_ctx,
            core_state,
            meta_ctx,
            meta_state,
        })
    }

    /// Run a read-only MetaM computation.
    ///
    /// Executes the given MetaM computation using the stored context and state.
    /// The computation is dispatched to the worker thread to avoid cross-thread
    /// object access violations with mimalloc. Context and state objects are
    /// cloned before being passed to the FFI, so the `MetaMContext` can be
    /// reused for multiple `run()` calls.
    ///
    /// # Errors
    ///
    /// Returns [`LeanError::Exception`] if the MetaM computation raises a
    /// Lean exception. The error includes:
    /// - `is_internal`: whether the exception is an internal error
    /// - `message`: best-effort extraction of the human-readable message
    ///
    /// # EIO Result Handling
    ///
    /// `MetaM.run'` returns `EIO Exception α` via CoreM:
    /// - Tag 0 (`EStateM.Result.ok`) -- field 0 is the result value, field 1 is the world
    /// - Tag 1 (`EStateM.Result.error`) -- field 0 is the `Exception`, field 1 is the world
    pub fn run(
        &mut self,
        computation: LeanBound<'l, LeanAny>,
    ) -> LeanResult<LeanBound<'l, LeanAny>> {
        // Clone all context/state objects since FFI consumes them
        let meta_ctx = self.meta_ctx.clone();
        let meta_state = self.meta_state.clone();
        let core_ctx = self.core_ctx.clone();
        let core_state = self.core_state.clone();

        // Extract raw pointers for the worker closure
        let computation_ptr = computation.into_ptr();
        let meta_ctx_ptr = meta_ctx.into_ptr();
        let meta_state_ptr = meta_state.into_ptr();
        let core_ctx_ptr = core_ctx.into_ptr();
        let core_state_ptr = core_state.into_ptr();

        // Dispatch to the worker thread. MetaM operations must run on the
        // same thread where Lean was initialized to avoid cross-thread
        // object access violations (SIGSEGV from mimalloc thread-local heaps).
        let result = crate::runtime::with_worker(move || unsafe {
            // Wrap core_state in an ST.Ref as required by the CoreM monad stack.
            // lean_meta_metam_run creates the ST.Ref for Meta.State internally,
            // but expects Core.State to already be wrapped in an ST.Ref.

            // In Lean < 4.26, lean_st_mk_ref(value, world) returns
            // EStateM.Result.ok: field 0 = ST.Ref, field 1 = world.
            // In Lean >= 4.26, lean_st_mk_ref(value) returns the ST.Ref directly.
            #[cfg(not(lean_4_26))]
            let (core_state_ref, world2) = {
                let world = ffi::lean_box(0);
                let ref_result = ffi::lean_st_mk_ref(core_state_ptr, world);
                let core_state_ref = ffi::lean_ctor_get(ref_result, 0) as *mut ffi::lean_object;
                let world2 = ffi::lean_ctor_get(ref_result, 1) as *mut ffi::lean_object;
                ffi::lean_inc(core_state_ref);
                ffi::lean_inc(world2);
                ffi::lean_dec(ref_result);
                (core_state_ref, world2)
            };

            #[cfg(lean_4_26)]
            let (core_state_ref, world2) = {
                let core_state_ref = ffi::lean_st_mk_ref(core_state_ptr, ffi::lean_box(0));
                // In Lean 4.26+, lean_st_mk_ref returns the ST.Ref directly.
                // The second arg is ignored by the runtime but we pass it for ABI compat.
                let world2 = ffi::lean_box(0);
                (core_state_ref, world2)
            };

            let result = ffi::meta::lean_meta_metam_run(
                computation_ptr,
                meta_ctx_ptr,
                meta_state_ptr,
                core_ctx_ptr,
                core_state_ref,
                world2,
            );

            handle_eio_result(result)
        })?;

        unsafe { Ok(LeanBound::from_owned_ptr(self.lean, result)) }
    }

    pub(crate) fn run_persistent(
        &mut self,
        computation: LeanBound<'l, LeanAny>,
    ) -> LeanResult<LeanBound<'l, LeanAny>> {
        let meta_ctx = self.meta_ctx.clone();
        let meta_state = self.meta_state.clone();
        let core_ctx = self.core_ctx.clone();
        let core_state = self.core_state.clone();

        let computation_ptr = computation.into_ptr();
        let meta_ctx_ptr = meta_ctx.into_ptr();
        let meta_state_ptr = meta_state.into_ptr();
        let core_ctx_ptr = core_ctx.into_ptr();
        let core_state_ptr = core_state.into_ptr();

        let (result, new_meta_state, new_core_state) =
            crate::runtime::with_worker(move || unsafe {
                #[cfg(not(lean_4_26))]
                let (core_state_ref, world2) = {
                    let world = ffi::lean_box(0);
                    let ref_result = ffi::lean_st_mk_ref(core_state_ptr, world);
                    let core_state_ref = ffi::lean_ctor_get(ref_result, 0) as *mut ffi::lean_object;
                    let world2 = ffi::lean_ctor_get(ref_result, 1) as *mut ffi::lean_object;
                    ffi::lean_inc(core_state_ref);
                    ffi::lean_inc(world2);
                    ffi::lean_dec(ref_result);
                    (core_state_ref, world2)
                };

                #[cfg(lean_4_26)]
                let (core_state_ref, world2) = {
                    let core_state_ref = ffi::lean_st_mk_ref(core_state_ptr, ffi::lean_box(0));
                    let world2 = ffi::lean_box(0);
                    (core_state_ref, world2)
                };

                let result = ffi::meta::lean_meta_metam_run_state(
                    computation_ptr,
                    meta_ctx_ptr,
                    meta_state_ptr,
                    core_ctx_ptr,
                    core_state_ref,
                    world2,
                );

                let pair = handle_eio_result(result)?;
                let value_ptr = ffi::lean_ctor_get(pair, 0) as *mut ffi::lean_object;
                let meta_state_ptr = ffi::lean_ctor_get(pair, 1) as *mut ffi::lean_object;
                ffi::lean_inc(value_ptr);
                ffi::lean_inc(meta_state_ptr);
                ffi::lean_dec(pair);

                #[cfg(not(lean_4_26))]
                let core_state_ptr = {
                    let get_result = ffi::lean_st_ref_get(core_state_ref, ffi::lean_box(0));
                    let value = ffi::lean_ctor_get(get_result, 0) as *mut ffi::lean_object;
                    ffi::lean_inc(value);
                    ffi::lean_dec(get_result);
                    value
                };

                #[cfg(lean_4_26)]
                let core_state_ptr = {
                    let value = ffi::lean_st_ref_get(core_state_ref, ffi::lean_box(0));
                    ffi::lean_inc(value);
                    value
                };

                ffi::lean_dec(core_state_ref);

                Ok::<
                    (
                        *mut ffi::lean_object,
                        *mut ffi::lean_object,
                        *mut ffi::lean_object,
                    ),
                    LeanError,
                >((value_ptr, meta_state_ptr, core_state_ptr))
            })?;

        unsafe {
            self.meta_state =
                LeanBound::<LeanAny>::from_owned_ptr(self.lean, new_meta_state).cast();
            self.core_state =
                LeanBound::<LeanAny>::from_owned_ptr(self.lean, new_core_state).cast();
            Ok(LeanBound::from_owned_ptr(self.lean, result))
        }
    }

    /// Reconstruct a `MetaMContext` from pre-built parts.
    ///
    /// This is intended for FFI consumers (e.g., Python bindings) that store
    /// context/state as unbound objects and need to rebind them to a fresh
    /// lifetime on each call.
    ///
    /// # Safety
    ///
    /// The caller must ensure that all parts were originally produced by
    /// `MetaMContext::new` (or equivalent) and that the types are correct
    /// (i.e., `env` is actually a `LeanEnvironment`, etc.).
    pub unsafe fn from_parts(
        lean: Lean<'l>,
        env: LeanBound<'l, LeanEnvironment>,
        core_ctx: LeanBound<'l, CoreContext>,
        core_state: LeanBound<'l, CoreState>,
        meta_ctx: LeanBound<'l, MetaContext>,
        meta_state: LeanBound<'l, MetaState>,
    ) -> Self {
        Self {
            lean,
            env,
            core_ctx,
            core_state,
            meta_ctx,
            meta_state,
        }
    }

    /// Decompose this `MetaMContext` into its constituent parts.
    ///
    /// This is the inverse of [`from_parts`](Self::from_parts). It consumes
    /// the context and returns the individual bound objects, which can then
    /// be unbound for storage across lifetime boundaries.
    ///
    /// Returns `(env, core_ctx, core_state, meta_ctx, meta_state)`.
    pub fn into_parts(
        self,
    ) -> (
        LeanBound<'l, LeanEnvironment>,
        LeanBound<'l, CoreContext>,
        LeanBound<'l, CoreState>,
        LeanBound<'l, MetaContext>,
        LeanBound<'l, MetaState>,
    ) {
        (
            self.env,
            self.core_ctx,
            self.core_state,
            self.meta_ctx,
            self.meta_state,
        )
    }

    /// Get a reference to the [`LeanEnvironment`] used by this context.
    pub fn env(&self) -> &LeanBound<'l, LeanEnvironment> {
        &self.env
    }

    /// Get the [`Lean`] runtime token associated with this context.
    pub fn lean(&self) -> Lean<'l> {
        self.lean
    }

    pub(crate) fn set_local_context(
        &mut self,
        lctx: &LeanBound<'l, LeanAny>,
        local_instances: &LeanBound<'l, LeanAny>,
    ) {
        unsafe {
            #[cfg(lean_4_25)]
            let ctx = ffi::lean_alloc_ctor(0, 7, 3);
            #[cfg(not(lean_4_25))]
            let ctx = ffi::lean_alloc_ctor(0, 7, 11);

            for i in [0u32, 1, 4, 5, 6] {
                let field = ffi::lean_ctor_get(self.meta_ctx.as_ptr(), i) as *mut ffi::lean_object;
                ffi::lean_inc(field);
                ffi::lean_ctor_set(ctx, i, field);
            }
            ffi::lean_ctor_set(ctx, 2, lctx.clone().into_ptr());
            ffi::lean_ctor_set(ctx, 3, local_instances.clone().into_ptr());

            #[cfg(lean_4_25)]
            {
                let src = ffi::inline::lean_ctor_scalar_cptr(self.meta_ctx.as_ptr());
                let dst = ffi::inline::lean_ctor_scalar_cptr(ctx);
                std::ptr::copy_nonoverlapping(src, dst, 3);
            }
            #[cfg(not(lean_4_25))]
            {
                let src = ffi::inline::lean_ctor_scalar_cptr(self.meta_ctx.as_ptr());
                let dst = ffi::inline::lean_ctor_scalar_cptr(ctx);
                std::ptr::copy_nonoverlapping(src, dst, 11);
            }

            self.meta_ctx = LeanBound::<LeanAny>::from_owned_ptr(self.lean, ctx).cast();
        }
    }

    pub(crate) fn with_local_context<R>(
        &mut self,
        lctx: &LeanBound<'l, LeanAny>,
        local_instances: &LeanBound<'l, LeanAny>,
        f: impl FnOnce(&mut Self) -> LeanResult<R>,
    ) -> LeanResult<R> {
        let old_ctx = self.meta_ctx.clone();
        self.set_local_context(lctx, local_instances);
        let result = f(self);
        self.meta_ctx = old_ctx;
        result
    }

    /// Create a fresh metavariable goal in the current local context.
    pub fn mk_goal(
        &mut self,
        type_: &LeanBound<'l, LeanExpr>,
    ) -> LeanResult<LeanBound<'l, LeanExpr>> {
        let user_name = LeanName::anonymous(self.lean)?;
        self.mk_fresh_expr_mvar(type_, &user_name)
    }

    /// Create a fresh metavariable goal with a user-visible name.
    pub fn mk_named_goal(
        &mut self,
        type_: &LeanBound<'l, LeanExpr>,
        user_name: &LeanBound<'l, LeanName>,
    ) -> LeanResult<LeanBound<'l, LeanExpr>> {
        self.mk_fresh_expr_mvar(type_, user_name)
    }

    pub(crate) fn mk_fresh_expr_mvar(
        &mut self,
        type_: &LeanBound<'l, LeanExpr>,
        user_name: &LeanBound<'l, LeanName>,
    ) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            let type_opt = LeanOption::some(type_.clone().cast())?;
            let kind = ffi::lean_box(0); // MetavarKind.natural

            ffi::lean_inc(type_opt.as_ptr());
            ffi::lean_inc(kind);
            ffi::lean_inc(user_name.as_ptr());
            let closure = ffi::inline::lean_alloc_closure(
                ffi::meta::l_Lean_Meta_mkFreshExprMVar as *mut std::ffi::c_void,
                8,
                3,
            );
            ffi::inline::lean_closure_set(closure, 0, type_opt.into_ptr());
            ffi::inline::lean_closure_set(closure, 1, kind);
            ffi::inline::lean_closure_set(closure, 2, user_name.as_ptr());

            let computation = LeanBound::from_owned_ptr(self.lean, closure);
            let result = self.run_persistent(computation)?;
            Ok(result.cast())
        }
    }

    pub(crate) fn mk_fresh_expr_mvar_at(
        &mut self,
        lctx: &LeanBound<'l, LeanAny>,
        local_instances: &LeanBound<'l, LeanAny>,
        type_: &LeanBound<'l, LeanExpr>,
        user_name: &LeanBound<'l, LeanName>,
    ) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            let kind = ffi::lean_box(0); // MetavarKind.natural
            let num_scope_args = LeanNat::from_usize(self.lean, 0)?;

            ffi::lean_inc(lctx.as_ptr());
            ffi::lean_inc(local_instances.as_ptr());
            ffi::lean_inc(type_.as_ptr());
            ffi::lean_inc(kind);
            ffi::lean_inc(user_name.as_ptr());
            ffi::lean_inc(num_scope_args.as_ptr());
            let closure = ffi::inline::lean_alloc_closure(
                ffi::meta::l_Lean_Meta_mkFreshExprMVarAt as *mut std::ffi::c_void,
                11,
                6,
            );
            ffi::inline::lean_closure_set(closure, 0, lctx.as_ptr());
            ffi::inline::lean_closure_set(closure, 1, local_instances.as_ptr());
            ffi::inline::lean_closure_set(closure, 2, type_.as_ptr());
            ffi::inline::lean_closure_set(closure, 3, kind);
            ffi::inline::lean_closure_set(closure, 4, user_name.as_ptr());
            ffi::inline::lean_closure_set(closure, 5, num_scope_args.into_ptr());

            let computation = LeanBound::from_owned_ptr(self.lean, closure);
            let result = self.run_persistent(computation)?;
            Ok(result.cast())
        }
    }

    pub(crate) fn get_mvar_decl(
        &mut self,
        mvar: &LeanBound<'l, LeanExpr>,
    ) -> LeanResult<MVarDeclParts<'l>> {
        crate::runtime::ensure_meta_initialized();
        let mvar_id = LeanExpr::mvar_id(mvar)?;
        unsafe {
            ffi::lean_inc(mvar_id.as_ptr());
            let closure = ffi::inline::lean_alloc_closure(
                ffi::meta::l_Lean_MVarId_getDecl as *mut std::ffi::c_void,
                6,
                1,
            );
            ffi::inline::lean_closure_set(closure, 0, mvar_id.into_ptr());

            let computation = LeanBound::from_owned_ptr(self.lean, closure);
            let decl = self.run(computation)?;

            let decl_ptr = decl.as_ptr();
            let num_objs = ffi::inline::lean_ctor_num_objs(decl_ptr);
            if num_objs < 5 {
                return Err(LeanError::other(&format!(
                    "unexpected MetavarDecl layout: expected >= 5 object fields, got {num_objs}"
                )));
            }
            let lctx =
                LeanBound::<LeanAny>::from_borrowed_ptr(self.lean, ffi::lean_ctor_get(decl_ptr, 1))
                    .cast();
            let type_ =
                LeanBound::<LeanAny>::from_borrowed_ptr(self.lean, ffi::lean_ctor_get(decl_ptr, 2))
                    .cast();
            let local_instances =
                LeanBound::<LeanAny>::from_borrowed_ptr(self.lean, ffi::lean_ctor_get(decl_ptr, 4))
                    .cast();

            Ok(MVarDeclParts {
                lctx,
                type_,
                local_instances,
            })
        }
    }

    /// Look up a local hypothesis by its user-facing name in a goal's local context.
    pub fn goal_hypothesis(
        &mut self,
        goal: &LeanBound<'l, LeanExpr>,
        user_name: &LeanBound<'l, LeanName>,
    ) -> LeanResult<LeanBound<'l, LeanExpr>> {
        let decl = self.get_mvar_decl(goal)?;
        unsafe {
            let raw = ffi::meta::lean_local_ctx_find_from_user_name(
                decl.lctx.as_ptr(),
                user_name.as_ptr(),
            );
            if ffi::inline::lean_is_scalar(raw) {
                return Err(LeanError::other(
                    "goal_hypothesis: no local declaration with that user name",
                ));
            }
            let raw = LeanBound::<LeanAny>::from_owned_ptr(self.lean, raw);
            let local_decl = if ffi::inline::lean_ctor_num_objs(raw.as_ptr()) == 1 {
                LeanBound::<LeanAny>::from_borrowed_ptr(
                    self.lean,
                    ffi::lean_ctor_get(raw.as_ptr(), 0),
                )
            } else {
                raw
            };
            let fvar_id = ffi::meta::lean_local_decl_fvar_id(local_decl.as_ptr());
            let fvar_id = LeanBound::<LeanAny>::from_owned_ptr(self.lean, fvar_id).cast();
            LeanExpr::fvar(self.lean, fvar_id)
        }
    }

    /// Get the most recently introduced hypothesis from a goal's local context.
    pub fn goal_latest_hypothesis(
        &mut self,
        goal: &LeanBound<'l, LeanExpr>,
    ) -> LeanResult<LeanBound<'l, LeanExpr>> {
        let decl = self.get_mvar_decl(goal)?;
        unsafe {
            let num = ffi::meta::lean_local_ctx_num_indices(decl.lctx.as_ptr());
            let num = LeanBound::<LeanNat>::from_owned_ptr(self.lean, num);
            let num = LeanNat::to_usize(&num)?;
            if num == 0 {
                return Err(LeanError::other(
                    "goal_latest_hypothesis: local context is empty",
                ));
            }

            let index = LeanNat::from_usize(self.lean, num - 1)?;
            let raw = ffi::meta::lean_local_ctx_get_at(decl.lctx.as_ptr(), index.into_ptr());
            if ffi::inline::lean_is_scalar(raw) {
                return Err(LeanError::other(
                    "goal_latest_hypothesis: missing declaration at last local-context index",
                ));
            }

            let raw = LeanBound::<LeanAny>::from_owned_ptr(self.lean, raw);
            let local_decl = if ffi::inline::lean_ctor_num_objs(raw.as_ptr()) == 1 {
                LeanBound::<LeanAny>::from_borrowed_ptr(
                    self.lean,
                    ffi::lean_ctor_get(raw.as_ptr(), 0),
                )
            } else {
                raw
            };
            let fvar_id = ffi::meta::lean_local_decl_fvar_id(local_decl.as_ptr());
            let fvar_id = LeanBound::<LeanAny>::from_owned_ptr(self.lean, fvar_id).cast();
            LeanExpr::fvar(self.lean, fvar_id)
        }
    }

    /// Infer the type of a Lean expression.
    ///
    /// Uses Lean's `Meta.inferType` to compute the type of the given expression
    /// within this MetaM context. The expression must be well-typed in the
    /// current environment.
    ///
    /// # Errors
    ///
    /// Returns [`LeanError::Exception`] if the expression is ill-typed or
    /// references unknown constants.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use leo3::prelude::*;
    /// use leo3::meta::*;
    ///
    /// leo3::with_lean(|lean| {
    ///     let env = LeanEnvironment::empty(lean, 0)?;
    ///     let mut ctx = MetaMContext::new(lean, env)?;
    ///
    ///     // Sort(0) is Prop, its type should be Sort(1) (Type)
    ///     let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
    ///     let prop_type = ctx.infer_type(&prop)?;
    ///     assert!(LeanExpr::is_sort(&prop_type));
    ///     Ok(())
    /// })?;
    /// ```
    pub fn infer_type(
        &mut self,
        expr: &LeanBound<'l, LeanExpr>,
    ) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            // Create a MetaM closure: partially apply lean_infer_type with expr.
            // lean_infer_type has arity 6 (@[extern 6]): (expr, meta_ctx, meta_state_ref,
            // core_ctx, core_state_ref, world). We fix 1 arg (expr), leaving 5 for run().
            ffi::lean_inc(expr.as_ptr());
            let closure = ffi::inline::lean_alloc_closure(
                ffi::meta::lean_infer_type as *mut std::ffi::c_void,
                6,
                1,
            );
            ffi::inline::lean_closure_set(closure, 0, expr.as_ptr());

            let computation = LeanBound::from_owned_ptr(self.lean, closure);
            let result = self.run(computation)?;
            Ok(result.cast::<LeanExpr>())
        }
    }

    /// Reduce a Lean expression to weak head normal form.
    ///
    /// Uses Lean's `whnf` to reduce the given expression. This is useful for
    /// normalizing expressions before comparison or pattern matching.
    ///
    /// # Errors
    ///
    /// Returns [`LeanError::Exception`] if the reduction fails.
    pub fn whnf(&mut self, expr: &LeanBound<'l, LeanExpr>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            // lean_whnf has arity 6 (same as lean_infer_type):
            // (expr, meta_ctx, meta_state_ref, core_ctx, core_state_ref, world)
            ffi::lean_inc(expr.as_ptr());
            let closure = ffi::inline::lean_alloc_closure(
                ffi::meta::lean_whnf as *mut std::ffi::c_void,
                6,
                1,
            );
            ffi::inline::lean_closure_set(closure, 0, expr.as_ptr());

            let computation = LeanBound::from_owned_ptr(self.lean, closure);
            let result = self.run(computation)?;
            Ok(result.cast::<LeanExpr>())
        }
    }

    /// Check if two expressions are definitionally equal.
    ///
    /// Uses Lean's `isDefEq` to determine whether two expressions are equal
    /// up to computation rules (beta, delta, eta, iota reduction).
    ///
    /// # Errors
    ///
    /// Returns [`LeanError::Exception`] if the comparison fails.
    pub fn is_def_eq(
        &mut self,
        a: &LeanBound<'l, LeanExpr>,
        b: &LeanBound<'l, LeanExpr>,
    ) -> LeanResult<bool> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            // lean_is_expr_def_eq has arity 7 (@[extern 7]):
            // (expr_a, expr_b, meta_ctx, meta_state_ref, core_ctx, core_state_ref, world)
            // We fix 2 args (both exprs), leaving 5 for run().
            ffi::lean_inc(a.as_ptr());
            ffi::lean_inc(b.as_ptr());
            let closure = ffi::inline::lean_alloc_closure(
                ffi::meta::lean_is_expr_def_eq as *mut std::ffi::c_void,
                7,
                2,
            );
            ffi::inline::lean_closure_set(closure, 0, a.as_ptr());
            ffi::inline::lean_closure_set(closure, 1, b.as_ptr());

            let computation = LeanBound::from_owned_ptr(self.lean, closure);
            let result = self.run(computation)?;

            // Result is a Lean Bool: lean_box(0) = false, lean_box(1) = true
            let bool_val = ffi::lean_unbox(result.as_ptr());
            Ok(bool_val != 0)
        }
    }

    /// Type-check a Lean expression.
    ///
    /// Uses Lean's `Meta.check` to verify that the given expression is well-typed
    /// in the current environment. Returns `Ok(())` if the expression is valid.
    ///
    /// # Errors
    ///
    /// Returns [`LeanError::Exception`] if the expression is ill-typed.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use leo3::prelude::*;
    /// use leo3::meta::*;
    ///
    /// leo3::with_lean(|lean| {
    ///     let env = LeanEnvironment::empty(lean, 0)?;
    ///     let mut ctx = MetaMContext::new(lean, env)?;
    ///
    ///     let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
    ///     ctx.check(&prop)?; // Should succeed
    ///     Ok(())
    /// })?;
    /// ```
    pub fn check(&mut self, expr: &LeanBound<'l, LeanExpr>) -> LeanResult<()> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            // Create a MetaM closure: partially apply l_Lean_Meta_check with expr.
            // check : Expr → MetaM Unit compiles to arity 6:
            // (expr, meta_ctx, meta_state_ref, core_ctx, core_state_ref, world)
            ffi::lean_inc(expr.as_ptr());
            let closure = ffi::inline::lean_alloc_closure(
                ffi::meta::l_Lean_Meta_check as *mut std::ffi::c_void,
                6,
                1,
            );
            ffi::inline::lean_closure_set(closure, 0, expr.as_ptr());

            let computation = LeanBound::from_owned_ptr(self.lean, closure);
            let _result = self.run(computation)?;
            Ok(())
        }
    }

    /// Check if an expression is type-correct.
    ///
    /// This is a convenience wrapper around [`check()`](Self::check) that returns
    /// a boolean instead of propagating the error. Returns `true` if the expression
    /// is well-typed, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use leo3::prelude::*;
    /// use leo3::meta::*;
    ///
    /// leo3::with_lean(|lean| {
    ///     let env = LeanEnvironment::empty(lean, 0)?;
    ///     let mut ctx = MetaMContext::new(lean, env)?;
    ///
    ///     let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
    ///     assert!(ctx.is_type_correct(&prop));
    ///     Ok(())
    /// })?;
    /// ```
    pub fn is_type_correct(&mut self, expr: &LeanBound<'l, LeanExpr>) -> bool {
        self.check(expr).is_ok()
    }

    /// Get the type of a proof term (i.e., the proposition it proves).
    ///
    /// This is semantically equivalent to [`infer_type`](Self::infer_type), but
    /// named to clarify intent in proof-oriented contexts: the type of a proof
    /// term is the proposition it proves.
    ///
    /// # Errors
    ///
    /// Returns [`LeanError::Exception`] if the expression is ill-typed or
    /// references unknown constants.
    pub fn get_proof_type(
        &mut self,
        proof: &LeanBound<'l, LeanExpr>,
    ) -> LeanResult<LeanBound<'l, LeanExpr>> {
        self.infer_type(proof)
    }

    /// Check if a proof term proves a given proposition.
    ///
    /// Infers the type of `proof` and checks whether it is definitionally equal
    /// to `prop`. Returns `true` if the proof proves the proposition.
    ///
    /// # Errors
    ///
    /// Returns [`LeanError::Exception`] if type inference or definitional
    /// equality checking fails.
    pub fn is_proof_of(
        &mut self,
        proof: &LeanBound<'l, LeanExpr>,
        prop: &LeanBound<'l, LeanExpr>,
    ) -> LeanResult<bool> {
        let inferred = self.infer_type(proof)?;
        self.is_def_eq(&inferred, prop)
    }

    /// Check if an expression is a proposition (its type is `Prop`, i.e., `Sort 0`).
    ///
    /// Infers the type of `expr`, reduces it to weak head normal form, and checks
    /// whether the result is `Sort 0`.
    ///
    /// # Errors
    ///
    /// Returns [`LeanError::Exception`] if type inference or WHNF reduction fails.
    pub fn is_prop(&mut self, expr: &LeanBound<'l, LeanExpr>) -> LeanResult<bool> {
        let ty = self.infer_type(expr)?;
        let ty_whnf = self.whnf(&ty)?;
        if !LeanExpr::is_sort(&ty_whnf) {
            return Ok(false);
        }
        let level = LeanExpr::sort_level(&ty_whnf)?;
        // Sort 0 is Prop. Level.zero is a scalar: lean_box(0) == 0x1
        unsafe {
            let ptr = level.as_ptr();
            Ok(ffi::inline::lean_is_scalar(ptr) && ffi::lean_unbox(ptr) == 0)
        }
    }
}

/// Handle an EIO result from a MetaM computation.
///
/// The EIO result is `Except Exception T`:
/// - Tag 0 = `Except.ok` → field 0 is the success value
/// - Tag 1 = `Except.error` → field 0 is the `Exception` object
///
/// On success, returns the owned value pointer. On error, extracts the
/// `Exception` and returns a `LeanError::Exception`.
///
/// # Safety
///
/// - `result` must be a valid Lean `Except Exception T` object (consumed)
unsafe fn handle_eio_result(result: *mut ffi::lean_object) -> LeanResult<*mut ffi::lean_object> {
    let tag = ffi::lean_obj_tag(result);
    if tag == 0 {
        // Except.ok - extract value
        let value_ptr = ffi::lean_ctor_get(result, 0) as *mut ffi::lean_object;
        ffi::lean_inc(value_ptr);
        ffi::lean_dec(result);
        return Ok(value_ptr);
    }

    // Except.error - extract Exception
    let exception_ptr = ffi::lean_ctor_get(result, 0) as *mut ffi::lean_object;
    ffi::lean_inc(exception_ptr);
    ffi::lean_dec(result);

    let exc_tag = ffi::lean_obj_tag(exception_ptr);
    let is_internal = exc_tag == 1;

    // Field 1 of both Exception constructors is MessageData
    let msg_data = ffi::lean_ctor_get(exception_ptr, 1) as *mut ffi::lean_object;
    let message = extract_message_data(msg_data);

    ffi::lean_dec(exception_ptr);

    Err(LeanError::exception(is_internal, &message))
}

/// Best-effort extraction of a human-readable string from Lean's `MessageData`.
///
/// `MessageData` is a complex inductive type. This function handles the most
/// common cases:
/// - `ofFormat` (tag 0): checks for `Format.text` (tag 0) containing a string
/// - `ofExpr` (tag 4): uses `lean_expr_dbg_to_string` for a debug representation
/// - `withContext` (tag 6): recurses into the inner MessageData
/// - `tagged` (tag 8): recurses into the inner MessageData
/// - `nest` (tag 9): recurses into the inner MessageData
/// - `compose` (tag 10): recursively extracts from both children and concatenates
/// - `group` (tag 11): recurses into the inner MessageData
///
/// For other constructors, returns a descriptive fallback like `"<level>"`.
///
/// # Safety
///
/// - `msg_data` must be a valid Lean `MessageData` object (borrowed, not consumed)
unsafe fn extract_message_data(msg_data: *mut ffi::lean_object) -> String {
    if ffi::inline::lean_is_scalar(msg_data) {
        return "<MessageData:scalar>".to_string();
    }

    let tag = ffi::lean_obj_tag(msg_data);
    match tag {
        // MessageData.ofFormat (tag 0): field 0 is a Format
        0 => {
            let format = ffi::lean_ctor_get(msg_data, 0) as *mut ffi::lean_object;
            extract_format(format)
        }
        // MessageData.ofExpr (tag 4): field 0 is an Expr
        4 => {
            let expr = ffi::lean_ctor_get(msg_data, 0) as *mut ffi::lean_object;
            if expr.is_null() || ffi::inline::lean_is_scalar(expr) {
                return "<expr>".to_string();
            }
            ffi::lean_inc(expr);
            let dbg_str = ffi::expr::lean_expr_dbg_to_string(expr);
            let c_str = ffi::inline::lean_string_cstr(dbg_str);
            let result = match CStr::from_ptr(c_str).to_str() {
                Ok(s) => s.to_string(),
                Err(_) => "<expr:non-utf8>".to_string(),
            };
            ffi::lean_dec(dbg_str);
            result
        }
        // MessageData.withContext (tag 6): field 0 is context, field 1 is MessageData
        6 => {
            let inner = ffi::lean_ctor_get(msg_data, 1) as *mut ffi::lean_object;
            extract_message_data(inner)
        }
        // MessageData.tagged (tag 8): field 0 is Name, field 1 is MessageData
        8 => {
            let inner = ffi::lean_ctor_get(msg_data, 1) as *mut ffi::lean_object;
            extract_message_data(inner)
        }
        // MessageData.nest (tag 9): field 0 is Nat (indent), field 1 is MessageData
        9 => {
            let inner = ffi::lean_ctor_get(msg_data, 1) as *mut ffi::lean_object;
            extract_message_data(inner)
        }
        // MessageData.compose (tag 10): field 0 and field 1 are MessageData
        10 => {
            let left = ffi::lean_ctor_get(msg_data, 0) as *mut ffi::lean_object;
            let right = ffi::lean_ctor_get(msg_data, 1) as *mut ffi::lean_object;
            let left_str = extract_message_data(left);
            let right_str = extract_message_data(right);
            format!("{}{}", left_str, right_str)
        }
        // MessageData.group (tag 11): field 0 is MessageData
        11 => {
            let inner = ffi::lean_ctor_get(msg_data, 0) as *mut ffi::lean_object;
            extract_message_data(inner)
        }
        1 => "<level>".to_string(),
        2 => "<name>".to_string(),
        3 => "<syntax>".to_string(),
        5 => "<goal>".to_string(),
        7 => "<MessageData:withNamingContext>".to_string(),
        12 => "<MessageData:node>".to_string(),
        13 => "<MessageData:trace>".to_string(),
        14 => "<MessageData:lazy>".to_string(),
        _ => format!("<MessageData:unknown(tag={})>", tag),
    }
}

/// Best-effort extraction of a string from Lean's `Format` type.
///
/// ```lean
/// inductive Format where
///   | text : String → Format       -- tag 0
///   | append : Format → Format → Format  -- tag 1
///   | ...
/// ```
///
/// # Safety
///
/// - `format` must be a valid Lean `Format` object (borrowed, not consumed)
unsafe fn extract_format(format: *mut ffi::lean_object) -> String {
    if ffi::inline::lean_is_scalar(format) {
        return "<Format:scalar>".to_string();
    }

    let tag = ffi::lean_obj_tag(format);
    match tag {
        // Format.text (tag 0): field 0 is a String
        0 => {
            let str_obj = ffi::lean_ctor_get(format, 0) as *mut ffi::lean_object;
            if str_obj.is_null() || ffi::inline::lean_is_scalar(str_obj) {
                return "<Format:invalid-string>".to_string();
            }
            let c_str = ffi::inline::lean_string_cstr(str_obj);
            match CStr::from_ptr(c_str).to_str() {
                Ok(s) => s.to_string(),
                Err(_) => "<Format:non-utf8>".to_string(),
            }
        }
        // Format.append (tag 1): field 0 and field 1 are Format
        1 => {
            let left = ffi::lean_ctor_get(format, 0) as *mut ffi::lean_object;
            let right = ffi::lean_ctor_get(format, 1) as *mut ffi::lean_object;
            let left_str = extract_format(left);
            let right_str = extract_format(right);
            format!("{}{}", left_str, right_str)
        }
        _ => format!("<Format:tag={}>", tag),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a synthetic `Except.ok value` object (tag 0, 1 field).
    unsafe fn mk_except_ok(value: *mut ffi::lean_object) -> *mut ffi::lean_object {
        let obj = ffi::lean_alloc_ctor(0, 1, 0);
        ffi::lean_ctor_set(obj, 0, value);
        obj
    }

    /// Build a synthetic `Except.error exception` object (tag 1, 1 field).
    unsafe fn mk_except_error(exception: *mut ffi::lean_object) -> *mut ffi::lean_object {
        let obj = ffi::lean_alloc_ctor(1, 1, 0);
        ffi::lean_ctor_set(obj, 0, exception);
        obj
    }

    /// Build a synthetic `Exception.error ref msg_data` (tag 0, 2 fields).
    unsafe fn mk_exception_error(
        ref_obj: *mut ffi::lean_object,
        msg_data: *mut ffi::lean_object,
    ) -> *mut ffi::lean_object {
        let obj = ffi::lean_alloc_ctor(0, 2, 0);
        ffi::lean_ctor_set(obj, 0, ref_obj);
        ffi::lean_ctor_set(obj, 1, msg_data);
        obj
    }

    /// Build a synthetic `Exception.internal id msg_data` (tag 1, 2 fields).
    unsafe fn mk_exception_internal(
        id: *mut ffi::lean_object,
        msg_data: *mut ffi::lean_object,
    ) -> *mut ffi::lean_object {
        let obj = ffi::lean_alloc_ctor(1, 2, 0);
        ffi::lean_ctor_set(obj, 0, id);
        ffi::lean_ctor_set(obj, 1, msg_data);
        obj
    }

    /// Build `MessageData.ofFormat fmt` (tag 0, 1 field).
    unsafe fn mk_msg_data_of_format(fmt: *mut ffi::lean_object) -> *mut ffi::lean_object {
        let obj = ffi::lean_alloc_ctor(0, 1, 0);
        ffi::lean_ctor_set(obj, 0, fmt);
        obj
    }

    /// Build `Format.text str` (tag 0, 1 field).
    unsafe fn mk_format_text(s: &str) -> *mut ffi::lean_object {
        let lean_str = ffi::string::lean_mk_string_from_bytes(s.as_ptr() as *const _, s.len());
        let obj = ffi::lean_alloc_ctor(0, 1, 0);
        ffi::lean_ctor_set(obj, 0, lean_str);
        obj
    }

    #[test]
    fn test_handle_eio_result_ok() {
        let result: LeanResult<()> = crate::test_with_lean(|_lean| {
            unsafe {
                // Create a dummy value (boxed 42)
                let value = ffi::lean_box(42);
                let except_ok = mk_except_ok(value);

                let result = handle_eio_result(except_ok);
                assert!(result.is_ok());

                let ptr = result.unwrap();
                // The value should be the boxed 42
                let unboxed = ffi::lean_unbox(ptr);
                assert_eq!(unboxed, 42);
                // ptr is a scalar (boxed), no need to dec
            }
            Ok(())
        });
        assert!(result.is_ok(), "test failed: {:?}", result.err());
    }

    #[test]
    fn test_handle_eio_result_error_with_text_message() {
        let result: LeanResult<()> = crate::test_with_lean(|_lean| {
            unsafe {
                // Build: Except.error (Exception.error ref (MessageData.ofFormat (Format.text "test error")))
                let format_text = mk_format_text("test error");
                let msg_data = mk_msg_data_of_format(format_text);
                let ref_obj = ffi::lean_box(0); // dummy Ref (scalar)
                let exception = mk_exception_error(ref_obj, msg_data);
                let except_err = mk_except_error(exception);

                let result = handle_eio_result(except_err);
                assert!(result.is_err());

                let err = result.unwrap_err();
                match &err {
                    LeanError::Exception {
                        is_internal,
                        message,
                    } => {
                        assert!(!is_internal);
                        assert_eq!(message, "test error");
                    }
                    other => panic!("Expected Exception, got: {:?}", other),
                }
            }
            Ok(())
        });
        assert!(result.is_ok(), "test failed: {:?}", result.err());
    }

    #[test]
    fn test_handle_eio_result_internal_exception() {
        let result: LeanResult<()> = crate::test_with_lean(|_lean| {
            unsafe {
                // Build: Except.error (Exception.internal id (MessageData.ofFormat (Format.text "internal fail")))
                let format_text = mk_format_text("internal fail");
                let msg_data = mk_msg_data_of_format(format_text);
                let id_obj = ffi::lean_box(0); // dummy InternalExceptionId (scalar)
                let exception = mk_exception_internal(id_obj, msg_data);
                let except_err = mk_except_error(exception);

                let result = handle_eio_result(except_err);
                assert!(result.is_err());

                let err = result.unwrap_err();
                match &err {
                    LeanError::Exception {
                        is_internal,
                        message,
                    } => {
                        assert!(is_internal);
                        assert_eq!(message, "internal fail");
                    }
                    other => panic!("Expected Exception, got: {:?}", other),
                }
            }
            Ok(())
        });
        assert!(result.is_ok(), "test failed: {:?}", result.err());
    }

    #[test]
    fn test_extract_message_data_scalar() {
        let result: LeanResult<()> = crate::test_with_lean(|_lean| {
            unsafe {
                // A scalar (tagged pointer) should return the fallback
                let scalar = ffi::lean_box(0);
                let msg = extract_message_data(scalar);
                assert_eq!(msg, "<MessageData:scalar>");
            }
            Ok(())
        });
        assert!(result.is_ok(), "test failed: {:?}", result.err());
    }

    #[test]
    fn test_extract_format_text() {
        let result: LeanResult<()> = crate::test_with_lean(|_lean| {
            unsafe {
                let format_text = mk_format_text("hello world");
                let msg = extract_format(format_text);
                assert_eq!(msg, "hello world");
                ffi::lean_dec(format_text);
            }
            Ok(())
        });
        assert!(result.is_ok(), "test failed: {:?}", result.err());
    }
}
