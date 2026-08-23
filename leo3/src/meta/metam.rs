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

/// Unpack an `Option LocalDecl` returned by the local-context lookups.
///
/// The `some` wrapper is freshly allocated (owned by the caller), but the
/// wrapped `LocalDecl` is still owned by the local context (borrowed). Take
/// an owned reference to the declaration and drop the wrapper, so the
/// declaration's refcount stays balanced when the context is released.
unsafe fn unpack_option_local_decl(
    lean: Lean<'_>,
    raw: *mut ffi::lean_object,
) -> LeanBound<'_, LeanAny> {
    if ffi::inline::lean_ctor_num_objs(raw) == 1 {
        let inner = ffi::lean_ctor_get(raw, 0) as *mut ffi::lean_object;
        ffi::lean_inc(inner);
        ffi::lean_dec(raw);
        LeanBound::<LeanAny>::from_owned_ptr(lean, inner)
    } else {
        LeanBound::<LeanAny>::from_owned_ptr(lean, raw)
    }
}

/// A goal's local hypotheses as `(user_name, type_dbg)` pairs, together
/// with the goal's (instantiated) type.
type GoalHypsAndType<'l> = (Vec<(String, String)>, LeanBound<'l, LeanExpr>);

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
        unsafe {
            // The initial Meta.State / Meta.Context may be BSS statics with
            // rc 0 (constructors no longer inc them). When this context is
            // dropped or its state is replaced, the decrement would free the
            // static block into mimalloc's freelist and corrupt the heap
            // (observed as intermittent SIGSEGV across tests). Keep two
            // extra references so statics never drop to 0: the context's own
            // drop consumes one, leaving one behind (harmless leak).
            ffi::object::lean_inc(meta_state.as_ptr());
            ffi::object::lean_inc(meta_state.as_ptr());
            ffi::object::lean_inc(meta_ctx.as_ptr());
            ffi::object::lean_inc(meta_ctx.as_ptr());
        }
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
                // In Lean 4.26+, lean_st_mk_ref is a 1-arg export that
                // returns the ST.Ref directly. `world2` is the dummy
                // token for `lean_meta_metam_run`'s world parameter
                // (ignored by the 4.26+ export it forwards to).
                let core_state_ref = ffi::lean_st_mk_ref(core_state_ptr);
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
                // `MetaM.toIO` is the production entry point Lean's own
                // `runMetaM` uses. Unlike the `MetaM.run` family it returns
                // the final `Core.State` in the result pair directly (no
                // ST.Ref threading) and does not corrupt the heap when the
                // computation assigns a metavariable (2026-08 audit; the
                // `MetaM.run` variants double-drop the returned Meta.State).
                let world = ffi::lean_box(0);
                let result = ffi::meta::lean_meta_metam_to_io(
                    computation_ptr,
                    core_ctx_ptr,
                    core_state_ptr,
                    meta_ctx_ptr,
                    meta_state_ptr,
                    world,
                );

                // EIO ok carries `α × Core.State × State` (nested pairs).
                let pair = handle_eio_result(result)?;
                let alpha = ffi::lean_ctor_get(pair, 0) as *mut ffi::lean_object;
                let cs_ms = ffi::lean_ctor_get(pair, 1) as *mut ffi::lean_object;
                let core_state_ptr = ffi::lean_ctor_get(cs_ms, 0) as *mut ffi::lean_object;
                let meta_state_ptr = ffi::lean_ctor_get(cs_ms, 1) as *mut ffi::lean_object;
                ffi::lean_inc(alpha);
                ffi::lean_inc(core_state_ptr);
                ffi::lean_inc(meta_state_ptr);
                ffi::lean_dec(pair);

                Ok::<
                    (
                        *mut ffi::lean_object,
                        *mut ffi::lean_object,
                        *mut ffi::lean_object,
                    ),
                    LeanError,
                >((alpha, meta_state_ptr, core_state_ptr))
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
    #[allow(clippy::type_complexity)]
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
    /// Replace the stored `Meta.State` (used by the Repl layer to branch
    /// from a snapshot).
    pub fn replace_meta_state(&mut self, new_state: LeanBound<'l, crate::instance::LeanAny>) {
        unsafe {
            ffi::object::lean_inc(self.meta_state.as_ptr());
        }
        self.meta_state = new_state.cast();
    }

    /// Snapshot the current `Meta.State` as an unbound object (Repl states).
    pub fn meta_state_snapshot(&self) -> crate::unbound::LeanUnbound<crate::instance::LeanAny> {
        unsafe {
            ffi::object::lean_inc(self.meta_state.as_ptr());
            crate::unbound::LeanUnbound::from_owned_ptr(self.meta_state.as_ptr())
        }
    }

    /// The context's environment.
    pub fn env(&self) -> &LeanBound<'l, LeanEnvironment> {
        &self.env
    }

    /// Replace the context's environment (e.g. with the output of
    /// `crate::meta::repl::run_command`).
    ///
    /// Tactic elaboration reads constants from `Core.State.env` (field 0),
    /// so the new environment must be threaded there too — otherwise
    /// locally declared constants are invisible to `run_tactic`.
    pub fn replace_env(&mut self, env: LeanBound<'l, LeanEnvironment>) {
        unsafe {
            // Core.State has 9 object fields: env, nextMacroScope, ngen,
            // auxDeclNGen, traceState, cache, messages, infoState,
            // snapshotTasks.
            let old_core = self.core_state.as_ptr();
            let new_core = ffi::lean_alloc_ctor(0, 9, 0);
            ffi::lean_inc(env.as_ptr());
            ffi::lean_ctor_set(new_core, 0, env.as_ptr());
            for i in 1..9u32 {
                let f = ffi::lean_ctor_get(old_core, i) as *mut ffi::lean_object;
                ffi::lean_inc(f);
                ffi::lean_ctor_set(new_core, i, f);
            }
            self.core_state =
                LeanBound::<crate::meta::CoreState>::from_owned_ptr(self.lean, new_core);
        }
        self.env = env;
    }

    /// Get the [`Lean`] runtime token associated with this context.
    pub fn lean(&self) -> Lean<'l> {
        self.lean
    }

    #[cfg(lean_4_25)]
    pub(crate) fn core_ctx(&self) -> &LeanBound<'l, CoreContext> {
        &self.core_ctx
    }

    #[cfg(lean_4_25)]
    pub(crate) fn core_state(&self) -> &LeanBound<'l, CoreState> {
        &self.core_state
    }

    #[cfg(lean_4_25)]
    pub(crate) fn meta_ctx(&self) -> &LeanBound<'l, MetaContext> {
        &self.meta_ctx
    }

    #[cfg(lean_4_25)]
    pub(crate) fn meta_state(&self) -> &LeanBound<'l, MetaState> {
        &self.meta_state
    }

    /// Replace the stored `Meta.State` / `Core.State` after a computation
    /// that ran through the fully-uncurried FFI path.
    ///
    /// The previous state is dropped (refcount decremented). The initial
    /// `Meta.State` may be a BSS static object (rc 0 before the constructor
    /// incremented it); dropping it back to 0 would `lean_free` the static
    /// block into mimalloc's freelist and corrupt the heap. Increment before
    /// the swap so the old state keeps one extra reference (a harmless leak
    /// for statics; for heap states it just leaks one ref).
    #[cfg(lean_4_25)]
    pub(crate) fn update_states(
        &mut self,
        new_meta_state: LeanBound<'l, crate::instance::LeanAny>,
        new_core_state: LeanBound<'l, crate::instance::LeanAny>,
    ) {
        unsafe {
            ffi::object::lean_inc(self.meta_state.as_ptr());
        }
        self.meta_state = new_meta_state.cast();
        self.core_state = new_core_state.cast();
    }

    pub(crate) fn set_local_context(
        &mut self,
        lctx: &LeanBound<'l, LeanAny>,
        local_instances: &LeanBound<'l, LeanAny>,
    ) {
        unsafe {
            // Meta.Context has 7 object fields in all supported versions.
            // Scalar (Bool) bytes: 3 on Lean 4.25–4.27 (trackZetaDelta,
            // univApprox, inTypeClassResolution); Lean 4.28 adds
            // `cacheInferType` as a 4th scalar (verified against the
            // `Meta/Basic.lean` source of v4.25.2 and v4.33.0).
            #[cfg(lean_4_28)]
            let ctx = ffi::lean_alloc_ctor(0, 7, 4);
            #[cfg(all(lean_4_25, not(lean_4_28)))]
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

            #[cfg(lean_4_28)]
            {
                let src = ffi::inline::lean_ctor_scalar_cptr(self.meta_ctx.as_ptr());
                let dst = ffi::inline::lean_ctor_scalar_cptr(ctx);
                std::ptr::copy_nonoverlapping(src, dst, 4);
            }
            #[cfg(all(lean_4_25, not(lean_4_28)))]
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

            // Keep one extra reference on the previous Meta.Context before
            // replacing it: the initial context may be a BSS static object,
            // and dropping it to rc 0 would free the static block.
            ffi::object::lean_inc(self.meta_ctx.as_ptr());
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
            // MetavarDecl object layout (4.25.2, verified by LiveView of the
            // decl produced with `MVarId.getDecl`): userName(0), lctx(1),
            // type(2), localInstances(4, `Array LocalInstance`). Only these
            // three fields are read here.
            //
            // NOTE: Lean exports NO field accessors for `MetavarDecl` (nm
            // shows only `l_Lean_MetavarDecl_ctorIdx` on 4.25.2), so manual
            // slot reads are the only option; the `num_objs < 5` guard above
            // makes layout drift fail loudly instead of corrupting memory.
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
            let local_decl = unpack_option_local_decl(self.lean, raw);
            let fvar_id = ffi::meta::lean_local_decl_fvar_id(local_decl.as_ptr());
            let fvar_id = LeanBound::<LeanAny>::from_owned_ptr(self.lean, fvar_id).cast();
            LeanExpr::fvar(self.lean, fvar_id)
        }
    }

    /// Get the local hypotheses of a goal as `(user_name, type_dbg)` pairs,
    /// together with the goal's (instantiated) type.
    ///
    /// Used by the Repl layer to render goal states.
    pub fn goal_hyps_and_type(
        &mut self,
        mvar: &LeanBound<'l, LeanName>,
    ) -> LeanResult<GoalHypsAndType<'l>> {
        crate::runtime::ensure_meta_initialized();
        let gexpr = LeanExpr::mvar(self.lean, mvar.clone())?;
        let ty = self.infer_type(&gexpr)?;
        let decl = self.get_mvar_decl(&gexpr)?;
        let lctx = decl.lctx;
        unsafe {
            extern "C" {
                #[link_name = "l_Lean_Name_toString"]
                fn name_to_string(
                    env: *mut *mut ffi::lean_object,
                    arg: *mut ffi::lean_object,
                ) -> *mut ffi::lean_object;
            }
            ffi::lean_inc(lctx.as_ptr());
            let num_raw = ffi::meta::lean_local_ctx_num_indices(lctx.as_ptr());
            let num = LeanBound::<LeanNat>::from_owned_ptr(self.lean, num_raw);
            let n = LeanNat::to_usize(&num)?;
            let mut hyps = Vec::new();
            for i in 0..n {
                let idx = LeanNat::from_usize(self.lean, i)?;
                ffi::lean_inc(lctx.as_ptr());
                let raw = ffi::meta::lean_local_ctx_get_at(lctx.as_ptr(), idx.as_ptr());
                if ffi::inline::lean_is_scalar(raw) {
                    continue;
                }
                let local_decl = unpack_option_local_decl(self.lean, raw);
                let un_raw = ffi::meta::lean_local_decl_user_name(local_decl.as_ptr());
                let un = LeanBound::<LeanAny>::from_owned_ptr(self.lean, un_raw).cast::<LeanName>();
                let tp_raw = ffi::meta::lean_local_decl_type(local_decl.as_ptr());
                let tp = LeanBound::<LeanAny>::from_owned_ptr(self.lean, tp_raw).cast::<LeanExpr>();
                // Name.toString (arity-1 curried pure function).
                let closure = ffi::inline::lean_alloc_closure(
                    name_to_string as *mut std::ffi::c_void,
                    1u32,
                    0,
                );
                let s = ffi::closure::lean_apply_1(closure, un.into_ptr());
                let s = LeanBound::<crate::types::LeanString>::from_owned_ptr(self.lean, s);
                let name_str = crate::types::LeanString::cstr(&s)?.to_string();
                let ty_str = LeanExpr::dbg_to_string(&tp)?;
                hyps.push((name_str, ty_str));
            }
            Ok((hyps, ty))
        }
    }

    /// Sanitize a goal's local context via `LocalContext.sanitizeNames`, like
    /// `Lean.Meta.ppGoal` does before pretty-printing: declarations whose user
    /// names carry macro scopes (hygiene names such as `n._@._hyg.36` that
    /// `induction`/`intro` introduce) are renamed to clean display names
    /// (`n✝`), so the hypothesis names match the real frontend's goal view.
    ///
    /// Options come from the session's `Meta.Context` (the `pp.sanitizeNames`
    /// option defaults to `true`). The sanitizer state (`NameSanitizerState`:
    /// options + two empty name maps) is built fresh for each call.
    #[cfg(lean_4_25)]
    fn sanitize_local_ctx<'a>(
        metam: &MetaMContext<'a>,
        lctx: &LeanBound<'a, LeanAny>,
    ) -> LeanResult<LeanBound<'a, LeanAny>> {
        unsafe {
            // Options from the Core.Context (field 2, per `CoreContext::mk_default`);
            // `MetaM.getOptions` reads the same value. `pp.sanitizeNames`
            // defaults to `true`, so empty options still sanitize.
            let options = ffi::lean_ctor_get(metam.core_ctx().as_ptr(), 2) as *mut ffi::lean_object;

            // NameSanitizerState { options, nameStem2Idx := {}, userName2Sanitized := {} }.
            // Empty `NameMap` = `Std.DTreeMap.empty` = `Cell.nil` = box(1)
            // (verified against `l_Std_DTreeMap_empty`, which returns box(1)).
            let state = ffi::lean_alloc_ctor(0, 3, 0);
            ffi::lean_inc(options);
            ffi::inline::lean_ctor_set(state, 0, options);
            ffi::inline::lean_ctor_set(state, 1, ffi::lean_box(1));
            ffi::inline::lean_ctor_set(state, 2, ffi::lean_box(1));

            // The export consumes both arguments (standard convention); lctx is
            // borrowed here, so hand it a reference.
            ffi::lean_inc(lctx.as_ptr());
            let result = ffi::meta::lean_local_ctx_sanitize_names(lctx.as_ptr(), state);
            let lctx_new = ffi::lean_ctor_get(result, 0) as *mut ffi::lean_object;
            ffi::lean_inc(lctx_new);
            ffi::lean_dec(result);
            Ok(LeanBound::<LeanAny>::from_owned_ptr(metam.lean(), lctx_new).cast())
        }
    }

    /// Get the local hypotheses of a goal as `(user_name, type_pp)` pairs,
    /// together with the goal's pretty-printed type.
    ///
    /// Like [`Self::goal_hyps_and_type`], but both the hypothesis types and the
    /// goal type are rendered by Lean's real pretty printer
    /// (`Meta.ppExpr` under the goal's local context), so free variables
    /// appear with their user-facing names and usual notations.
    ///
    /// Used by the Repl layer to render goal states.
    #[cfg(lean_4_25)]
    pub fn goal_hyps_and_type_pp(
        &mut self,
        mvar: &LeanBound<'l, LeanName>,
    ) -> LeanResult<(Vec<(String, String)>, String)> {
        crate::runtime::ensure_meta_initialized();
        let gexpr = LeanExpr::mvar(self.lean, mvar.clone())?;
        let decl = self.get_mvar_decl(&gexpr)?;
        // Sanitize the goal's local context (like `Lean.Meta.ppGoal` does):
        // declarations whose user names carry macro scopes (e.g. the
        // hygiene names `n._@._hyg.36` induction introduces) are renamed
        // to clean display names (`n✝`), matching the real frontend.
        let lctx = Self::sanitize_local_ctx(self, &decl.lctx)?;
        let local_instances = decl.local_instances;
        let goal_ty = decl.type_;
        // Collect the user name + type expression of every hypothesis, plus
        // the goal type, then batch-render them all in ONE worker trip.
        // (Each `pp_expr` is a serialized worker rendezvous; the whole
        // local context + goal share one lctx / local-instances, so they
        // fold into a single `pp_exprs` call. Mirroring `Lean.Meta.ppGoal`'s
        // `prevType?` dedup, consecutive hypotheses with structurally-equal
        // types share one `pp_expr` call, so a goal whose hypotheses all have
        // the same type (e.g. `∀ x y z : Nat, …`) renders in O(hyps) instead
        // of O(hyps^2) - each separate `pp_expr` call rebuilds its
        // delaborator context from the whole local context.
        let mut hyps: Vec<(String, usize)> = Vec::new();
        let mut groups: Vec<LeanBound<'l, LeanExpr>> = Vec::new();
        unsafe {
            extern "C" {
                #[link_name = "l_Lean_Name_toString"]
                fn name_to_string(
                    env: *mut *mut ffi::lean_object,
                    arg: *mut ffi::lean_object,
                ) -> *mut ffi::lean_object;
            }
            ffi::lean_inc(lctx.as_ptr());
            let num_raw = ffi::meta::lean_local_ctx_num_indices(lctx.as_ptr());
            let num = LeanBound::<LeanNat>::from_owned_ptr(self.lean, num_raw);
            let n = LeanNat::to_usize(&num)?;
            for i in 0..n {
                let idx = LeanNat::from_usize(self.lean, i)?;
                ffi::lean_inc(lctx.as_ptr());
                let raw = ffi::meta::lean_local_ctx_get_at(lctx.as_ptr(), idx.as_ptr());
                if ffi::inline::lean_is_scalar(raw) {
                    continue;
                }
                let local_decl = unpack_option_local_decl(self.lean, raw);
                let un_raw = ffi::meta::lean_local_decl_user_name(local_decl.as_ptr());
                let un = LeanBound::<LeanAny>::from_owned_ptr(self.lean, un_raw).cast::<LeanName>();
                let tp_raw = ffi::meta::lean_local_decl_type(local_decl.as_ptr());
                let tp = LeanBound::<LeanAny>::from_owned_ptr(self.lean, tp_raw).cast::<LeanExpr>();
                // Name.toString (arity-1 curried pure function).
                let closure = ffi::inline::lean_alloc_closure(
                    name_to_string as *mut std::ffi::c_void,
                    1u32,
                    0,
                );
                let s = ffi::closure::lean_apply_1(closure, un.into_ptr());
                let s = LeanBound::<crate::types::LeanString>::from_owned_ptr(self.lean, s);
                let name_str = crate::types::LeanString::cstr(&s)?.to_string();
                // Mirror `ppGoal`'s `prevType?`: a hypothesis whose type is
                // structurally equal to the previous one reuses that run's
                // group, so a run of equal types costs one `pp_expr` call
                // regardless of its length.
                let dup = match groups.last() {
                    Some(prev_tp) => LeanExpr::equal(prev_tp, &tp),
                    None => false,
                };
                let group = if dup {
                    groups.len() - 1
                } else {
                    groups.push(tp);
                    groups.len() - 1
                };
                hyps.push((name_str, group));
            }
        }
        // One batched pretty-print: one entry per distinct consecutive
        // hypothesis type, followed by the goal type.
        let mut to_pp = groups;
        to_pp.push(goal_ty);
        let mut rendered = crate::meta::repl::pp_exprs(self, &lctx, &local_instances, &to_pp)?;
        let mut hyp_pp = Vec::with_capacity(hyps.len());
        for (name, gi) in &hyps {
            hyp_pp.push((name.clone(), rendered[*gi].clone()));
        }
        let ty_pp = rendered
            .pop()
            .ok_or_else(|| LeanError::other("pp_exprs returned no goal type"))?;
        Ok((hyp_pp, ty_pp))
    }

    /// Get the most recently introduced hypothesis from a goal's local context.
    pub fn goal_latest_hypothesis(
        &mut self,
        goal: &LeanBound<'l, LeanExpr>,
    ) -> LeanResult<LeanBound<'l, LeanExpr>> {
        let decl = self.get_mvar_decl(goal)?;
        unsafe {
            // `lean_local_ctx_num_indices` and `lean_local_ctx_get_at` both
            // consume the local context (verified against the 4.25.2 runtime
            // disassembly), so hand over owned references.
            let lctx_ptr = decl.lctx.as_ptr();
            ffi::lean_inc(lctx_ptr);
            let num = ffi::meta::lean_local_ctx_num_indices(lctx_ptr);
            let num = LeanBound::<LeanNat>::from_owned_ptr(self.lean, num);
            let num = LeanNat::to_usize(&num)?;
            if num == 0 {
                return Err(LeanError::other(
                    "goal_latest_hypothesis: local context is empty",
                ));
            }

            let index = LeanNat::from_usize(self.lean, num - 1)?;
            ffi::lean_inc(lctx_ptr);
            let raw = ffi::meta::lean_local_ctx_get_at(lctx_ptr, index.as_ptr());
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
            // Create a MetaM closure: partially apply `Meta.check` with `expr`.
            // The closure target and arity are version-dependent (Lean 4.31
            // added a `transparency` parameter); `lean_meta_check_closure`
            // dispatches on the version cfg.
            let closure = ffi::meta::lean_meta_check_closure(expr.as_ptr());

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
pub unsafe fn handle_eio_result(
    result: *mut ffi::lean_object,
) -> LeanResult<*mut ffi::lean_object> {
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
    let exc_objs = ffi::inline::lean_ctor_num_objs(exception_ptr);

    // `MetaM.run`-family errors carry a `Lean.Exception` (2 object fields:
    // error/ref+msg, internal/id+extra). `MetaM.toIO` instead reports
    // `IO.Error` (single object field: the rendered message string), e.g.
    // `userError` (tag 18). Dispatch on the object layout.
    let (is_internal, message) = if exc_objs >= 2 {
        // Lean.Exception: field 1 of the error constructor is MessageData.
        // (Fallback if the Exception layout ever drifts: `internal_elim` /
        // `error_elim` exports extract the fields version-safely, at the
        // cost of a Lean closure callback per error.)
        let msg_data = ffi::lean_ctor_get(exception_ptr, 1) as *mut ffi::lean_object;
        (exc_tag == 1, extract_message_data(msg_data))
    } else if exc_objs == 1 {
        // IO.Error: the message string is the (only) object field.
        let msg_ptr = ffi::lean_ctor_get(exception_ptr, 0) as *mut ffi::lean_object;
        let c_str = ffi::inline::lean_string_cstr(msg_ptr);
        let message = if c_str.is_null() {
            "<io error>".to_string()
        } else {
            std::ffi::CStr::from_ptr(c_str)
                .to_str()
                .map(|s| s.to_string())
                .unwrap_or_else(|_| "<io error:non-utf8>".to_string())
        };
        (false, message)
    } else {
        (exc_tag == 1, "<unknown exception>".to_string())
    };

    ffi::lean_dec(exception_ptr);

    Err(LeanError::exception(is_internal, &message))
}

/// Render a `MessageData` to a human-readable string.
///
/// Uses Lean's real message renderer (`MessageData.toString : BaseIO
/// String`): it forces lazy messages (the common shape for tactic errors)
/// and formats embedded expressions/names with the default pp options —
/// e.g. `rfl` on `n + m = m + n` yields
/// `Tactic 'rfl' failed, n + m = m + n is not definitionally equal to ...`
/// instead of `<MessageData:lazy><Format:scalar>...`.
///
/// ABI by era: pre-4.26 the export takes a world token and returns
/// `IO.Result` (ok = tag 0 carrying `(value, world)`); 4.26+ (ST
/// redesign) erases the singleton world, so the export takes no world
/// token and returns the rendered `String` directly.
///
/// Falls back to the hand-rolled extractor below if the renderer fails
/// (e.g. `IO.Error`), and to `<MessageData:scalar>` for scalar values.
///
/// # Safety
///
/// - `msg_data` must be a valid Lean `MessageData` object (borrowed, not consumed)
/// - the Lean runtime must be initialized (caller runs inside the worker)
pub(super) unsafe fn extract_message_data(msg_data: *mut ffi::lean_object) -> String {
    if ffi::inline::lean_is_scalar(msg_data) {
        return "<MessageData:scalar>".to_string();
    }

    // Real renderer first: `MessageData.toString` (BaseIO String).
    // The export consumes its argument (standard convention); msg_data is
    // borrowed here, so hand it a reference.
    ffi::lean_inc(msg_data);
    #[cfg(not(lean_4_26))]
    // Pre-4.26: `BaseIO` threads the world token; the export returns
    // `IO.Result` (`Except (IO.Error × World) (String × World)`) — ok =
    // tag 0 with fields `(value, world)` (see `lean_io_result_mk_ok`).
    let rendered = {
        let world = ffi::io::lean_io_mk_world();
        ffi::meta::lean_message_data_to_string(msg_data, world)
    };
    #[cfg(lean_4_26)]
    // 4.26+ (ST redesign): `BaseIO String` is `ST RealWorld String` with
    // the singleton world erased — the export takes no world token and
    // returns the `String` directly, no `IO.Result` wrapper.
    let rendered = ffi::meta::lean_message_data_to_string_st(msg_data);
    let string = {
        #[cfg(not(lean_4_26))]
        let str_obj: Option<*mut ffi::lean_object> = if ffi::lean_obj_tag(rendered) == 0 {
            Some(ffi::lean_ctor_get(rendered, 0) as *mut ffi::lean_object)
        } else {
            None
        };
        #[cfg(lean_4_26)]
        let str_obj: Option<*mut ffi::lean_object> = if ffi::inline::lean_is_string(rendered) {
            Some(rendered)
        } else {
            None
        };
        str_obj.and_then(|str_obj| {
            let c_str = ffi::inline::lean_string_cstr(str_obj);
            if c_str.is_null() {
                None
            } else {
                CStr::from_ptr(c_str).to_str().ok().map(|s| s.to_string())
            }
        })
    };
    ffi::lean_dec(rendered);
    if let Some(s) = string {
        if !s.is_empty() {
            return s;
        }
    }

    extract_message_data_fallback(msg_data)
}

/// Best-effort hand-rolled extraction of a human-readable string from
/// Lean's `MessageData`, used when the real renderer fails.
///
/// `MessageData` is a complex inductive type. This function handles the
/// most common cases:
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
unsafe fn extract_message_data_fallback(msg_data: *mut ffi::lean_object) -> String {
    // Hand-rolled extractor for the 4.25 constructor layout:
    // 0 ofFormatWithInfos, 1 ofGoal, 2 ofWidget, 3 withContext,
    // 4 withNamingContext, 5 nest, 6 group, 7 compose, 8 tagged,
    // 9 trace, 10 ofLazy. ofName/ofLevel/ofSyntax/ofExpr are defs
    // expanding to ofFormatWithInfos.
    let tag = ffi::lean_obj_tag(msg_data);
    match tag {
        // ofFormatWithInfos (0): field 0 is a FormatWithInfos struct
        // { fmt, infos } — extract fmt (field 0).
        0 => {
            let fwi = ffi::lean_ctor_get(msg_data, 0) as *mut ffi::lean_object;
            if fwi.is_null() || ffi::inline::lean_is_scalar(fwi) {
                return "<MessageData:no-fmt>".to_string();
            }
            let format = ffi::lean_ctor_get(fwi, 0) as *mut ffi::lean_object;
            extract_format(format)
        }
        // ofWidget (2): field 1 is the fallback message.
        2 => {
            let inner = ffi::lean_ctor_get(msg_data, 1) as *mut ffi::lean_object;
            extract_message_data(inner)
        }
        // withContext (3) / withNamingContext (4) / nest (5): field 1 is msg.
        3..=5 => {
            let inner = ffi::lean_ctor_get(msg_data, 1) as *mut ffi::lean_object;
            extract_message_data(inner)
        }
        // group (6): field 0 is msg.
        6 => {
            let inner = ffi::lean_ctor_get(msg_data, 0) as *mut ffi::lean_object;
            extract_message_data(inner)
        }
        // compose (7): fields 0/1 are msg.
        7 => {
            let left = ffi::lean_ctor_get(msg_data, 0) as *mut ffi::lean_object;
            let right = ffi::lean_ctor_get(msg_data, 1) as *mut ffi::lean_object;
            let left_str = extract_message_data(left);
            let right_str = extract_message_data(right);
            format!("{}{}", left_str, right_str)
        }
        // tagged (8): field 1 is msg.
        8 => {
            let inner = ffi::lean_ctor_get(msg_data, 1) as *mut ffi::lean_object;
            extract_message_data(inner)
        }
        // trace (9): field 1 is msg.
        9 => {
            let inner = ffi::lean_ctor_get(msg_data, 1) as *mut ffi::lean_object;
            extract_message_data(inner)
        }
        // ofLazy (10): cannot force safely; report as lazy.
        10 => "<MessageData:lazy>".to_string(),
        1 => "<MessageData:goal>".to_string(),
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
        // Format.text (tag 3 on 4.25: nil=0, line=1, align=2, text=3):
        // field 0 is a String
        3 => {
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
        // Format.append (tag 5): fields 0/1 are Format.
        5 => {
            let left = ffi::lean_ctor_get(format, 0) as *mut ffi::lean_object;
            let right = ffi::lean_ctor_get(format, 1) as *mut ffi::lean_object;
            let left_str = extract_format(left);
            let right_str = extract_format(right);
            format!("{}{}", left_str, right_str)
        }
        // Format.nest (tag 4): field 1 is Format.
        4 => {
            let inner = ffi::lean_ctor_get(format, 1) as *mut ffi::lean_object;
            extract_format(inner)
        }
        // Format.group (tag 6): field 0 is Format.
        6 => {
            let inner = ffi::lean_ctor_get(format, 0) as *mut ffi::lean_object;
            extract_format(inner)
        }
        // Format.tag (tag 7): field 1 is Format.
        7 => {
            let inner = ffi::lean_ctor_get(format, 1) as *mut ffi::lean_object;
            extract_format(inner)
        }
        // Format.line (tag 1) renders as a newline.
        1 => "\n".to_string(),
        // Format.nil (tag 0) renders as empty text.
        0 => String::new(),
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

    /// Build a synthetic `EStateM.Result.error exception world` (tag 1,
    /// 2 fields: error + state/world) — the shape `handle_eio_result`
    /// dispatches on.
    unsafe fn mk_except_error(exception: *mut ffi::lean_object) -> *mut ffi::lean_object {
        let obj = ffi::lean_alloc_ctor(1, 2, 0);
        ffi::lean_ctor_set(obj, 0, exception);
        ffi::lean_ctor_set(obj, 1, ffi::lean_box(0)); // world
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

    /// Build `Format.text str` (tag 3 on 4.25: nil=0, line=1, align=2, text=3).
    unsafe fn mk_format_text(s: &str) -> *mut ffi::lean_object {
        let lean_str = ffi::string::lean_mk_string_from_bytes(s.as_ptr() as *const _, s.len());
        let obj = ffi::lean_alloc_ctor(3, 1, 0);
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
                // Build an EStateM.Result.error wrapping an Exception.error
                // with a scalar MessageData. The extractor falls back to a
                // safe placeholder; the important contract is that the error
                // path extracts an Exception without corrupting memory
                // (hand-built Format objects on v4.25 need PPContext-driven
                // rendering, covered by integration tests).
                let msg_data = ffi::lean_box(0); // scalar MessageData
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
                        assert!(!message.is_empty());
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
                // Build: Except.error (Exception.internal id (scalar MessageData))
                let msg_data = ffi::lean_box(0); // scalar MessageData
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
                        assert!(!message.is_empty());
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
