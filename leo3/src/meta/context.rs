//! Context and state types for Lean's CoreM / MetaM monad stack.
//!
//! Lean's metaprogramming monads are layered as `MetaM = ReaderT Meta.Context
//! (StateRefT Meta.State CoreM)`, where `CoreM = ReaderT Core.Context
//! (StateRefT Core.State ...)`. Running a MetaM computation therefore requires
//! four objects:
//!
//! | Type | Kind | Purpose |
//! |------|------|---------|
//! | [`CoreContext`] | Read-only | Core settings: file name, recursion depth, heartbeats, namespace |
//! | [`CoreState`] | Mutable | Core state: environment, name generator, messages, trace |
//! | [`MetaContext`] | Read-only | Meta settings: local context, config, local instances |
//! | [`MetaState`] | Mutable | Meta state: metavariable context, caches, diagnostics |
//!
//! Each type provides an `mk_default` / `mk_*` constructor that fills in
//! sensible defaults so callers can get started without manually constructing
//! every field. These are used internally by [`MetaMContext::new()`].
//!
//! Based on:
//! - `/lean4/src/Lean/CoreM.lean`
//! - `/lean4/src/Lean/Meta/Basic.lean`
//! - Issue #26 - MetaM Integration Phase 1.2
//!
//! [`MetaMContext::new()`]: super::metam::MetaMContext::new

use crate::err::LeanResult;
use crate::instance::LeanBound;
use crate::marker::Lean;
use crate::meta::{LeanEnvironment, LeanExpr, LeanName};
use crate::types::{LeanList, LeanOption, LeanString};
use leo3_ffi as ffi;

#[inline(always)]
const fn ctor_scalar_offset(num_obj_fields: u32, scalar_offset: u32) -> u32 {
    num_obj_fields * std::mem::size_of::<*mut ffi::lean_object>() as u32 + scalar_offset
}

#[inline]
fn force_manual_meta_defaults() -> bool {
    std::env::var_os("LEO3_FORCE_MANUAL_META_DEFAULTS").is_some()
}

#[inline(always)]
unsafe fn empty_rbmap_like() -> *mut ffi::lean_object {
    // Lean's RBMap/RBTree empty values erase to the nullary `leaf` constructor.
    ffi::lean_box(0)
}

/// Core.Context - context for CoreM monad
///
/// Lean < 4.25: 13 object fields + 2 scalar bytes (constructor tag 0)
/// Lean >= 4.25: 14 object fields + 2 scalar bytes (added `quotContext: Name` at field 10)
///
/// Object fields (Bool fields are stored as scalars, not objects):
/// 0. `fileName: String`
/// 1. `fileMap: FileMap`
/// 2. `options: Options`
/// 3. `currRecDepth: Nat`
/// 4. `maxRecDepth: Nat`
/// 5. `ref: Syntax`
/// 6. `currNamespace: Name`
/// 7. `openDecls: List OpenDecl`
/// 8. `initHeartbeats: Nat`
/// 9. `maxHeartbeats: Nat`
/// 10. `quotContext: Name` (Lean >= 4.25 only)
/// 11. `currMacroScope: MacroScope` (field 10 in Lean < 4.25)
/// 12. `cancelTk?: Option IO.CancelToken` (field 11 in Lean < 4.25)
/// 13. `inheritedTraceOptions: Std.HashSet Name` (field 12 in Lean < 4.25)
///
/// Scalar fields:
/// offset 0: `diag: Bool` (1 byte)
/// offset 1: `suppressElabErrors: Bool` (1 byte)
#[repr(transparent)]
pub struct CoreContext {
    _private: (),
}

impl CoreContext {
    /// Create a Core.Context with default values
    ///
    /// This creates a minimal Core.Context suitable for running MetaM computations.
    /// All fields are set to sensible defaults:
    /// - fileName: `"<rust>"`
    /// - maxRecDepth: 1000
    /// - maxHeartbeats: 200000000
    /// - All other fields: empty/zero/false
    ///
    /// # Example
    ///
    /// ```ignore
    /// use leo3::prelude::*;
    /// use leo3::meta::*;
    ///
    /// leo3::with_lean(|lean| {
    ///     let ctx = CoreContext::mk_default(lean)?;
    ///     Ok(())
    /// })
    /// ```
    pub fn mk_default<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            // Core.Context: 13 object fields in Lean < 4.25, 14 in Lean >= 4.25
            // Bool fields (diag, suppressElabErrors) are stored as scalars.
            #[cfg(lean_4_25)]
            let num_obj_fields = 14;
            #[cfg(not(lean_4_25))]
            let num_obj_fields = 13;

            let ctx = ffi::lean_alloc_ctor(0, num_obj_fields, 2);

            // Object field 0: fileName (String) - use "<rust>"
            let filename = LeanString::mk(lean, "<rust>")?;
            ffi::lean_ctor_set(ctx, 0, filename.into_ptr());

            // Object field 1: fileMap (FileMap) - use Inhabited instance
            let filemap = Self::mk_empty_filemap(lean)?;
            ffi::lean_ctor_set(ctx, 1, filemap.into_ptr());

            // Object field 2: options (Options) - use empty
            let options = Self::mk_empty_options(lean)?;
            ffi::lean_ctor_set(ctx, 2, options.into_ptr());

            // Object field 3: currRecDepth (Nat) - use 0
            ffi::lean_ctor_set(ctx, 3, ffi::lean_box(0));

            // Object field 4: maxRecDepth (Nat) - use 1000
            ffi::lean_ctor_set(ctx, 4, ffi::lean_box(1000));

            // Object field 5: ref (Syntax) - use Syntax.missing
            let syntax_missing = Self::mk_syntax_missing(lean)?;
            ffi::lean_ctor_set(ctx, 5, syntax_missing.into_ptr());

            // Object field 6: currNamespace (Name) - use anonymous
            let anon_name = LeanName::anonymous(lean)?;
            ffi::lean_ctor_set(ctx, 6, anon_name.into_ptr());

            // Object field 7: openDecls (List OpenDecl) - use empty list
            let empty_list = LeanList::nil(lean)?;
            ffi::lean_ctor_set(ctx, 7, empty_list.into_ptr());

            // Object field 8: initHeartbeats (Nat) - use 0
            ffi::lean_ctor_set(ctx, 8, ffi::lean_box(0));

            // Object field 9: maxHeartbeats (Nat) - use 200000000
            ffi::lean_ctor_set(ctx, 9, ffi::lean_box(200000000));

            // Object field 10: quotContext (Name) - use anonymous (Lean >= 4.25 only)
            #[cfg(lean_4_25)]
            {
                let quot_ctx = LeanName::anonymous(lean)?;
                ffi::lean_ctor_set(ctx, 10, quot_ctx.into_ptr());
            }

            // Remaining fields shift by 1 in Lean >= 4.25
            #[cfg(lean_4_25)]
            let field_offset = 11;
            #[cfg(not(lean_4_25))]
            let field_offset = 10;

            // currMacroScope (MacroScope = Nat) - Lean's first frontend scope is 1.
            ffi::lean_ctor_set(ctx, field_offset, ffi::lean_box(1));

            // cancelTk? (Option IO.CancelToken) - use none
            let cancel_token = LeanOption::none(lean)?;
            ffi::lean_ctor_set(ctx, field_offset + 1, cancel_token.into_ptr());

            // inheritedTraceOptions (Std.HashSet Name) - use empty
            let empty_hashset = Self::mk_empty_hashset(lean)?;
            ffi::lean_ctor_set(ctx, field_offset + 2, empty_hashset.into_ptr());

            // Scalar offset 0: diag (Bool) - use false (0)
            ffi::inline::lean_ctor_set_uint8(ctx, ctor_scalar_offset(num_obj_fields, 0), 0);

            // Scalar offset 1: suppressElabErrors (Bool) - use false (0)
            ffi::inline::lean_ctor_set_uint8(ctx, ctor_scalar_offset(num_obj_fields, 1), 0);

            Ok(LeanBound::from_owned_ptr(lean, ctx))
        }
    }

    /// Get an empty `FileMap` from the Lean runtime's `Inhabited FileMap` instance.
    ///
    /// Uses the `l_Lean_instInhabitedFileMap` BSS global which is initialized
    /// during `initialize_Lean_Meta`. Falls back to manual construction if the
    /// symbol is not available (Windows).
    ///
    /// Requires: `ensure_meta_initialized()` must have been called.
    fn mk_empty_filemap<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            if !force_manual_meta_defaults() {
                let filemap = ffi::meta::get_instInhabitedFileMap();
                if !filemap.is_null() {
                    // BSS static: no inc (rc 0, dec no-op)
                    // ffi::lean_inc(filemap);
                    return Ok(LeanBound::from_owned_ptr(lean, filemap));
                }
            }
            // Lean 4.20 FileMap stores only `source` and `positions`.
            let fm = ffi::lean_alloc_ctor(0, 2, 0);
            // field 0: source (String) — empty string
            let empty_str = ffi::string::lean_mk_string_from_bytes(b"".as_ptr() as *const _, 0);
            ffi::lean_ctor_set(fm, 0, empty_str);
            // field 1: positions (Array Nat) — #[0]
            let arr = ffi::array::lean_mk_empty_array();
            let arr = ffi::array::lean_array_push(arr, ffi::lean_box(0));
            ffi::lean_ctor_set(fm, 1, arr);
            Ok(LeanBound::from_owned_ptr(lean, fm))
        }
    }

    /// Create empty Options
    ///
    /// Uses the `Inhabited Options` instance from the Lean runtime.
    /// Falls back to a manual construction if the symbol is not available
    /// (Windows) or `LEO3_FORCE_MANUAL_META_DEFAULTS` is set.
    ///
    /// Requires: `ensure_meta_initialized()` must have been called.
    fn mk_empty_options<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            if !force_manual_meta_defaults() {
                let options = ffi::meta::lean_inhabited_options();
                if !options.is_null() {
                    // BSS static: no inc (rc 0, dec no-op)
                    // ffi::lean_inc(options);
                    return Ok(LeanBound::from_owned_ptr(lean, options));
                }
            }
            // Manual fallback. Options erases to different shapes per version
            // (verified against `src/Lean/Data/Options.lean` and the compiled
            // symbols):
            // - Lean < 4.28: `Options := KVMap` (= `RBMap Name DataValue`),
            //   and `RBMap.empty` erases to the tagged scalar box(0)
            //   (`l_Lean_RBMap_empty` is `mov $1,%eax; ret`).
            // - Lean 4.28+: `structure Options { map : NameMap DataValue,
            //   hasTrace : Bool }` = ctor (0, 1, 1): field 0 = box(1) (the
            //   empty NameMap = `DTreeMap.empty`, whose single `root` field
            //   erases to the `.empty` leaf ctor — tag 1, confirmed against
            //   `l_Std_DTreeMap_empty` (`mov $3,%eax; ret`) and the BSS
            //   `l_Lean_Options_empty` object: { +0x8 = 0x3, hasTrace = 0 }),
            //   scalar byte = hasTrace false.
            #[cfg(lean_4_28)]
            {
                let opts = ffi::lean_alloc_ctor(0, 1, 1);
                // DTreeMap.empty erases to the tagged scalar box(1) (0x3).
                ffi::lean_ctor_set(opts, 0, ffi::lean_box(1));
                ffi::inline::lean_ctor_set_uint8(opts, ctor_scalar_offset(1, 0), 0);
                Ok(LeanBound::from_owned_ptr(lean, opts))
            }
            #[cfg(not(lean_4_28))]
            {
                Ok(LeanBound::from_owned_ptr(lean, ffi::lean_box(0)))
            }
        }
    }

    /// Get `Syntax.missing` from the Lean runtime's `Inhabited Syntax` instance.
    ///
    /// Uses the `l_Lean_instInhabitedSyntax` BSS global which is initialized
    /// during `initialize_Lean_Meta`. Falls back to `lean_box(0)` if the symbol
    /// is not available (Windows), since `Syntax.missing` is constructor tag 0
    /// with 0 fields (a scalar enum value).
    ///
    /// Requires: `ensure_meta_initialized()` must have been called.
    fn mk_syntax_missing<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            if !force_manual_meta_defaults() {
                let syntax = ffi::meta::get_instInhabitedSyntax();
                if !syntax.is_null() {
                    // BSS static: no inc (rc 0, dec no-op)
                    // ffi::lean_inc(syntax);
                    return Ok(LeanBound::from_owned_ptr(lean, syntax));
                }
            }
            // Syntax.missing is constructor tag 0 with 0 fields → lean_box(0)
            Ok(LeanBound::from_owned_ptr(lean, ffi::lean_box(0)))
        }
    }

    /// Create an empty `Std.HashSet Name` for `inheritedTraceOptions`.
    ///
    /// Calls `lean_hashset_empty` with the default capacity (8),
    /// which dispatches to the correct symbol for the Lean version.
    fn mk_empty_hashset<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        unsafe {
            let hashset = ffi::hashset::lean_hashset_empty(ffi::lean_box(8));
            Ok(LeanBound::from_owned_ptr(lean, hashset))
        }
    }
}

/// Core.State - state for CoreM monad
///
/// Lean < 4.25: 8 fields (constructor tag 0)
/// Lean >= 4.25: 9 fields (added `auxDeclNGen: DeclNameGenerator` at field 3)
///
/// Fields:
/// 0. `env: Environment` - from parameter
/// 1. `nextMacroScope: MacroScope` - use default (firstFrontendMacroScope + 1)
/// 2. `ngen: NameGenerator` - create new
/// 3. `auxDeclNGen: DeclNameGenerator` - create new (Lean >= 4.25 only)
/// 4. `traceState: TraceState` - use empty (field 3 in Lean < 4.25)
/// 5. `cache: Cache` - use empty (field 4 in Lean < 4.25)
/// 6. `messages: MessageLog` - use empty (field 5 in Lean < 4.25)
/// 7. `infoState: Elab.InfoState` - use empty (field 6 in Lean < 4.25)
/// 8. `snapshotTasks: Array SnapshotTask` - use empty array (field 7 in Lean < 4.25)
#[repr(transparent)]
pub struct CoreState {
    _private: (),
}

impl CoreState {
    /// Create a Core.State with an Environment
    ///
    /// This creates a minimal Core.State suitable for running MetaM computations.
    /// All fields except the environment are set to sensible defaults:
    /// - env: from parameter
    /// - nextMacroScope: firstFrontendMacroScope + 1 (= 2)
    /// - ngen: new NameGenerator
    /// - All other fields: empty
    ///
    /// # Example
    ///
    /// ```ignore
    /// use leo3::prelude::*;
    /// use leo3::meta::*;
    ///
    /// leo3::with_lean(|lean| {
    ///     let env = LeanEnvironment::empty(lean, 0)?;
    ///     let state = CoreState::mk_core_state(lean, &env)?;
    ///     Ok(())
    /// })
    /// ```
    pub fn mk_core_state<'l>(
        lean: Lean<'l>,
        env: &LeanBound<'l, LeanEnvironment>,
    ) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            // Core.State: 8 fields in Lean < 4.25, 9 fields in Lean >= 4.25
            #[cfg(lean_4_25)]
            let num_fields = 9;
            #[cfg(not(lean_4_25))]
            let num_fields = 8;

            let state = ffi::lean_alloc_ctor(0, num_fields, 0);

            // Field 0: env (Environment) - from parameter
            let env_ptr = env.as_ptr();
            ffi::lean_inc(env_ptr);
            ffi::lean_ctor_set(state, 0, env_ptr);

            // Field 1: nextMacroScope (MacroScope) - firstFrontendMacroScope + 1
            // (reservedMacroScope = 0, firstFrontendMacroScope = 1 on 4.25)
            let next_macro_scope = ffi::lean_box(2);
            ffi::lean_ctor_set(state, 1, next_macro_scope);

            // Field 2: ngen (NameGenerator) - create new
            let ngen = Self::mk_name_generator(lean)?;
            ffi::lean_ctor_set(state, 2, ngen.into_ptr());

            // Field 3: auxDeclNGen (DeclNameGenerator) - create new (Lean >= 4.25 only)
            #[cfg(lean_4_25)]
            {
                let aux_decl_ngen = Self::mk_decl_name_generator(lean)?;
                ffi::lean_ctor_set(state, 3, aux_decl_ngen.into_ptr());
            }

            // Remaining fields shift by 1 in Lean >= 4.25
            #[cfg(lean_4_25)]
            let field_offset = 4;
            #[cfg(not(lean_4_25))]
            let field_offset = 3;

            // traceState (TraceState) - use empty
            let trace_state = Self::mk_empty_trace_state(lean)?;
            ffi::lean_ctor_set(state, field_offset, trace_state.into_ptr());

            // cache (Cache) - use empty
            let cache = Self::mk_empty_cache(lean)?;
            ffi::lean_ctor_set(state, field_offset + 1, cache.into_ptr());

            // messages (MessageLog) - use empty
            let messages = Self::mk_empty_message_log(lean)?;
            ffi::lean_ctor_set(state, field_offset + 2, messages.into_ptr());

            // infoState (Elab.InfoState) - use empty
            let info_state = Self::mk_empty_info_state(lean)?;
            ffi::lean_ctor_set(state, field_offset + 3, info_state.into_ptr());

            // snapshotTasks (Array SnapshotTask) - use empty array
            let empty_array = ffi::array::lean_mk_empty_array();
            ffi::lean_ctor_set(state, field_offset + 4, empty_array);

            Ok(LeanBound::from_owned_ptr(lean, state))
        }
    }

    /// Create a new NameGenerator
    ///
    /// Uses the `Inhabited NameGenerator` instance from the Lean runtime,
    /// which provides `{ namePrefix := `_uniq, idx := 1 }`.
    /// Falls back to manual construction if the symbol is not available (Windows).
    ///
    /// Requires: `ensure_meta_initialized()` must have been called.
    fn mk_name_generator<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            if !force_manual_meta_defaults() {
                let ngen = ffi::meta::get_instInhabitedNameGenerator();
                if !ngen.is_null() {
                    // BSS static: no inc (rc 0, dec no-op)
                    // ffi::lean_inc(ngen);
                    return Ok(LeanBound::from_owned_ptr(lean, ngen));
                }
            }
            // Manual construction: NameGenerator { namePrefix: `_uniq, idx: 1 }
            // Layout: ctor tag 0, 2 object fields, 0 scalar bytes
            let ng = ffi::lean_alloc_ctor(0, 2, 0);
            // field 0: namePrefix (Name) — `_uniq = Name.str Name.anonymous "_uniq"
            let anon = ffi::lean_box(0); // Name.anonymous
            let uniq_str = ffi::string::lean_mk_string_from_bytes(b"_uniq".as_ptr() as *const _, 5);
            let name_uniq = ffi::name::lean_name_mk_string(anon, uniq_str);
            ffi::lean_ctor_set(ng, 0, name_uniq);
            // field 1: idx (Nat) — 1
            ffi::lean_ctor_set(ng, 1, ffi::lean_box(1));
            Ok(LeanBound::from_owned_ptr(lean, ng))
        }
    }

    /// Create a new DeclNameGenerator (Lean >= 4.25)
    ///
    /// Uses the `Inhabited DeclNameGenerator` instance from the Lean runtime.
    /// Falls back to manual construction if the symbol is not available (Windows).
    ///
    /// Requires: `ensure_meta_initialized()` must have been called.
    #[cfg(lean_4_25)]
    fn mk_decl_name_generator<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            let decl_ngen = ffi::meta::get_instInhabitedDeclNameGenerator();
            if !decl_ngen.is_null() {
                // BSS static: no inc (rc 0, dec no-op)
                // ffi::lean_inc(decl_ngen);
                return Ok(LeanBound::from_owned_ptr(lean, decl_ngen));
            }
            // Manual construction: DeclNameGenerator { ngen: default, auxNGen: default }
            // Layout: ctor tag 0, 2 object fields, 0 scalar bytes
            let dng = ffi::lean_alloc_ctor(0, 2, 0);
            let ngen1 = Self::mk_name_generator(lean)?;
            let ngen2 = Self::mk_name_generator(lean)?;
            ffi::lean_ctor_set(dng, 0, ngen1.into_ptr());
            ffi::lean_ctor_set(dng, 1, ngen2.into_ptr());
            Ok(LeanBound::from_owned_ptr(lean, dng))
        }
    }

    /// Create an empty TraceState
    ///
    /// Uses the `Inhabited TraceState` instance from the Lean runtime.
    /// Falls back to manual construction if the symbol is not available (Windows).
    ///
    /// Requires: `ensure_meta_initialized()` must have been called.
    fn mk_empty_trace_state<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            if !force_manual_meta_defaults() {
                let trace_state = ffi::meta::get_instInhabitedTraceState();
                if !trace_state.is_null() {
                    // BSS static: no inc (rc 0, dec no-op)
                    // ffi::lean_inc(trace_state);
                    return Ok(LeanBound::from_owned_ptr(lean, trace_state));
                }
            }
            // Lean 4.20 TraceState = { tid : UInt64 := 0, traces := #[] }.
            let ts = ffi::lean_alloc_ctor(0, 1, 8);
            let pa_empty = ffi::meta::get_PersistentArrayEmpty();
            // BSS static: no inc (rc 0, dec no-op)
            // ffi::lean_inc(pa_empty);
            ffi::lean_ctor_set(ts, 0, pa_empty);
            ffi::lean_ctor_set_uint64(ts, ctor_scalar_offset(1, 0), 0);
            Ok(LeanBound::from_owned_ptr(lean, ts))
        }
    }

    /// Create an empty Cache
    ///
    /// Uses the `Inhabited Core.Cache` instance from the Lean runtime.
    /// Falls back to manual construction if the symbol is not available (Windows).
    ///
    /// Requires: `ensure_meta_initialized()` must have been called.
    fn mk_empty_cache<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            if !force_manual_meta_defaults() {
                let cache = ffi::meta::get_CoreInstInhabitedCache();
                if !cache.is_null() {
                    // BSS static: no inc (rc 0, dec no-op)
                    // ffi::lean_inc(cache);
                    return Ok(LeanBound::from_owned_ptr(lean, cache));
                }
            }
            // Lean 4.20 Core.Cache has two PersistentHashMap fields.
            let c = ffi::lean_alloc_ctor(0, 2, 0);
            let phm_empty = ffi::meta::get_PersistentHashMapEmpty();
            for i in 0..2u32 {
                // BSS static: no inc (rc 0, dec no-op)
                // ffi::lean_inc(phm_empty);
                ffi::lean_ctor_set(c, i, phm_empty);
            }
            Ok(LeanBound::from_owned_ptr(lean, c))
        }
    }

    /// Create an empty MessageLog
    ///
    /// Uses the `Inhabited MessageLog` instance from the Lean runtime.
    /// Falls back to manual construction if the symbol is not available (Windows).
    ///
    /// Requires: `ensure_meta_initialized()` must have been called.
    fn mk_empty_message_log<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            if !force_manual_meta_defaults() {
                let msg_log = ffi::meta::get_instInhabitedMessageLog();
                if !msg_log.is_null() {
                    // BSS static: no inc (rc 0, dec no-op)
                    // ffi::lean_inc(msg_log);
                    return Ok(LeanBound::from_owned_ptr(lean, msg_log));
                }
            }
            // Lean 4.20 MessageLog = { reported, unreported, loggedKinds }.
            let ml = ffi::lean_alloc_ctor(0, 3, 0);
            let pa_empty = ffi::meta::get_PersistentArrayEmpty();
            // BSS static: no inc (rc 0, dec no-op)
            // ffi::lean_inc(pa_empty);
            ffi::lean_ctor_set(ml, 0, pa_empty);
            // BSS static: no inc (rc 0, dec no-op)
            // ffi::lean_inc(pa_empty);
            ffi::lean_ctor_set(ml, 1, pa_empty);
            ffi::lean_ctor_set(ml, 2, empty_rbmap_like());
            Ok(LeanBound::from_owned_ptr(lean, ml))
        }
    }

    /// Create an empty Elab.InfoState
    ///
    /// Uses the `Inhabited Elab.InfoState` instance from the Lean runtime.
    /// Falls back to manual construction if the symbol is not available (Windows).
    ///
    /// Requires: `ensure_meta_initialized()` must have been called.
    fn mk_empty_info_state<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            if !force_manual_meta_defaults() {
                let info_state = ffi::meta::get_ElabInstInhabitedInfoState();
                if !info_state.is_null() {
                    // BSS static: no inc (rc 0, dec no-op)
                    // ffi::lean_inc(info_state);
                    return Ok(LeanBound::from_owned_ptr(lean, info_state));
                }
            }
            // Lean 4.20 InfoState = { enabled := true, assignment, lazyAssignment, trees }.
            let is = ffi::lean_alloc_ctor(0, 3, 1);
            let phm_empty = ffi::meta::get_PersistentHashMapEmpty();
            ffi::lean_inc(phm_empty);
            ffi::lean_ctor_set(is, 0, phm_empty);
            ffi::lean_inc(phm_empty);
            ffi::lean_ctor_set(is, 1, phm_empty);
            let pa_empty = ffi::meta::get_PersistentArrayEmpty();
            // BSS static: no inc (rc 0, dec no-op)
            // ffi::lean_inc(pa_empty);
            ffi::lean_ctor_set(is, 2, pa_empty);
            ffi::inline::lean_ctor_set_uint8(is, ctor_scalar_offset(3, 0), 1); // enabled = true
            Ok(LeanBound::from_owned_ptr(lean, is))
        }
    }
}

/// Meta.Context - context for MetaM monad
///
/// Lean structure with 7 object fields + 11 scalar bytes (constructor tag 0).
///
/// Object fields (Bool fields are stored as scalars, not objects):
/// 0. `config: Config`
/// 1. `zetaDeltaSet: FVarIdSet`
/// 2. `lctx: LocalContext`
/// 3. `localInstances: LocalInstances`
/// 4. `defEqCtx?: Option DefEqContext`
/// 5. `synthPendingDepth: Nat`
/// 6. `canUnfold?: Option (Config → ConstantInfo → CoreM Bool)`
///
/// Scalar fields:
/// offset 0: `configKey: UInt64` (8 bytes)
/// offset 8: `trackZetaDelta: Bool` (1 byte)
/// offset 9: `univApprox: Bool` (1 byte)
/// offset 10: `inTypeClassResolution: Bool` (1 byte)
#[repr(transparent)]
pub struct MetaContext {
    _private: (),
}

impl MetaContext {
    /// Create a Meta.Context with default values
    ///
    /// This creates a minimal Meta.Context suitable for running MetaM computations.
    /// All fields are set to sensible defaults:
    /// - config: default Meta.Config (all false/zero)
    /// - configKey: 0 (computed from config hash)
    /// - trackZetaDelta: false
    /// - zetaDeltaSet: empty FVarIdSet
    /// - lctx: empty LocalContext
    /// - localInstances: empty array
    ///
    /// # Example
    ///
    /// ```ignore
    /// use leo3::prelude::*;
    /// use leo3::meta::*;
    ///
    /// leo3::with_lean(|lean| {
    ///     let ctx = MetaContext::mk_default(lean)?;
    ///     Ok(())
    /// })
    /// ```
    pub fn mk_default<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, Self>> {
        // In Lean >= 4.25, Meta.Context was restructured (ConfigWithKey wrapper).
        // Use the runtime's Inhabited instance to avoid hardcoding the layout.
        // Falls back to manual construction if the symbol is not available (Windows).
        #[cfg(lean_4_25)]
        {
            crate::runtime::ensure_meta_initialized();
            unsafe {
                if !force_manual_meta_defaults() {
                    let ctx = ffi::meta::get_instInhabitedContext();
                    if !ctx.is_null() {
                        // BSS static: no inc (rc 0, dec no-op)
                        // ffi::lean_inc(ctx);
                        return Ok(LeanBound::from_owned_ptr(lean, ctx));
                    }
                }
            }
            // Fallback: manually construct Meta.Context for >= 4.25
            Self::mk_default_manual_v425_plus(lean)
        }
        // In Lean < 4.25, manually construct the struct (layout is stable).
        #[cfg(not(lean_4_25))]
        {
            Self::mk_default_manual(lean)
        }
    }

    /// Manually construct a Meta.Context for Lean >= 4.25.
    ///
    /// In Lean >= 4.25, field 0 is `ConfigWithKey` (wraps Config + key UInt64).
    /// Layout: ctor tag 0, 7 object fields + 3 scalar bytes on Lean 4.25–4.27,
    /// 4 scalar bytes on Lean 4.28+ (`cacheInferType`, default true; verified
    /// against the `Meta/Basic.lean` sources of v4.25.2, v4.27.0, v4.28.0,
    /// v4.30.0 and v4.33.0).
    ///
    /// Object fields:
    /// 0. configWithKey: ConfigWithKey (ctor 0, 1 obj field [Config], 8 scalar bytes [key: UInt64])
    /// 1. zetaDeltaSet: FVarIdSet (empty HashSet)
    /// 2. lctx: LocalContext (Inhabited instance — exported on Windows)
    /// 3. localInstances: LocalInstances (empty array)
    /// 4. defEqCtx?: Option DefEqContext (none)
    /// 5. synthPendingDepth: Nat (0)
    /// 6. canUnfold?: Option ... (none)
    ///
    /// Scalar fields (3 bytes on 4.25–4.27, 4 on 4.28+):
    /// offset 0: trackZetaDelta (Bool) = false
    /// offset 1: univApprox (Bool) = false
    /// offset 2: inTypeClassResolution (Bool) = false
    /// offset 3: cacheInferType (Bool) = true (Lean 4.28+ only)
    #[cfg(lean_4_25)]
    fn mk_default_manual_v425_plus<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            #[cfg(lean_4_28)]
            let ctx = ffi::lean_alloc_ctor(0, 7, 4);
            #[cfg(not(lean_4_28))]
            let ctx = ffi::lean_alloc_ctor(0, 7, 3);

            // field 0: ConfigWithKey — ctor 0, 1 obj field (Config), 8 scalar bytes (key: UInt64)
            let cwk = ffi::lean_alloc_ctor(0, 1, 8);
            // Config: 0 obj fields — 19 scalar bytes on Lean 4.25–4.33
            // (zetaHave present since 4.25), 20 on 4.34+ (lean4 #14323
            // appends `canUnfoldPredicateConfig : CanUnfoldPredicateConfig`
            // = .default, 1 byte).
            #[cfg(lean_4_34)]
            let config = ffi::lean_alloc_ctor(0, 0, 20);
            #[cfg(not(lean_4_34))]
            let config = ffi::lean_alloc_ctor(0, 0, 19);
            let base = ffi::inline::lean_ctor_scalar_cptr(config);
            *base.add(0) = 0; // foApprox = false
            *base.add(1) = 0; // ctxApprox = false
            *base.add(2) = 0; // quasiPatternApprox = false
            *base.add(3) = 0; // constApprox = false
            *base.add(4) = 0; // isDefEqStuckEx = false
            *base.add(5) = 1; // unificationHints = true
            *base.add(6) = 1; // proofIrrelevance = true
            *base.add(7) = 0; // assignSyntheticOpaque = false
            *base.add(8) = 1; // offsetCnstrs = true
            *base.add(9) = 1; // transparency = .default (tag 1)
            *base.add(10) = 0; // etaStruct = .all (tag 0)
            *base.add(11) = 1; // univApprox = true
            *base.add(12) = 1; // iota = true
            *base.add(13) = 1; // beta = true
            *base.add(14) = 2; // proj = .yesWithDelta (tag 2)
            *base.add(15) = 1; // zeta = true
            *base.add(16) = 1; // zetaDelta = true
            *base.add(17) = 1; // zetaUnused = true
            *base.add(18) = 1; // zetaHave = true
                               // byte 19 (Lean 4.34+): canUnfoldPredicateConfig = .default
            #[cfg(lean_4_34)]
            {
                *base.add(19) = 0;
            }
            ffi::lean_ctor_set(cwk, 0, config);
            ffi::lean_ctor_set_uint64(cwk, ctor_scalar_offset(1, 0), 0); // key = 0
            ffi::lean_ctor_set(ctx, 0, cwk);

            // field 1: zetaDeltaSet (FVarIdSet) — empty HashSet
            let fvar_set = ffi::hashset::lean_hashset_empty(ffi::lean_box(8));
            ffi::lean_ctor_set(ctx, 1, fvar_set);

            // field 2: lctx (LocalContext) — use Inhabited instance, fallback to manual
            let lctx = ffi::meta::get_instInhabitedLocalContext();
            if !lctx.is_null() {
                ffi::lean_inc(lctx);
                ffi::lean_ctor_set(ctx, 2, lctx);
            } else {
                // LocalContext: ctor 0, 2 obj fields {PersistentHashMap.empty, PersistentArray.empty}
                let lctx_manual = ffi::lean_alloc_ctor(0, 2, 0);
                let phm = ffi::meta::get_PersistentHashMapEmpty();
                ffi::lean_inc(phm);
                ffi::lean_ctor_set(lctx_manual, 0, phm);
                let pa = ffi::meta::get_PersistentArrayEmpty();
                ffi::lean_inc(pa);
                ffi::lean_ctor_set(lctx_manual, 1, pa);
                ffi::lean_ctor_set(ctx, 2, lctx_manual);
            }

            // field 3: localInstances (LocalInstances) — empty array
            let empty_array = ffi::array::lean_mk_empty_array();
            ffi::lean_ctor_set(ctx, 3, empty_array);

            // field 4: defEqCtx? (Option DefEqContext) — none
            let none = LeanOption::none(lean)?;
            ffi::lean_ctor_set(ctx, 4, none.into_ptr());

            // field 5: synthPendingDepth (Nat) — 0
            ffi::lean_ctor_set(ctx, 5, ffi::lean_box(0));

            // field 6: canUnfold? (Option ...) — none
            let none2 = LeanOption::none(lean)?;
            ffi::lean_ctor_set(ctx, 6, none2.into_ptr());

            // Scalar offset 0: trackZetaDelta (Bool) = false
            ffi::inline::lean_ctor_set_uint8(ctx, ctor_scalar_offset(7, 0), 0);
            // Scalar offset 1: univApprox (Bool) = false
            ffi::inline::lean_ctor_set_uint8(ctx, ctor_scalar_offset(7, 1), 0);
            // Scalar offset 2: inTypeClassResolution (Bool) = false
            ffi::inline::lean_ctor_set_uint8(ctx, ctor_scalar_offset(7, 2), 0);
            // Scalar offset 3 (Lean 4.28+): cacheInferType (Bool) = true
            #[cfg(lean_4_28)]
            ffi::inline::lean_ctor_set_uint8(ctx, ctor_scalar_offset(7, 3), 1);

            Ok(LeanBound::from_owned_ptr(lean, ctx))
        }
    }

    /// Manually construct a Meta.Context (for Lean < 4.25 where the layout is known).
    ///
    /// Meta.Context: 7 object fields + 11 scalar bytes (constructor tag 0).
    #[cfg(not(lean_4_25))]
    fn mk_default_manual<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            // Meta.Context: 7 object fields + 11 scalar bytes (constructor tag 0)
            // Bool fields (trackZetaDelta, univApprox, inTypeClassResolution) are scalars.
            let ctx = ffi::lean_alloc_ctor(0, 7, 11);

            // Object field 0: config (Meta.Config) - use default config
            let config = Self::mk_default_config(lean)?;
            ffi::lean_ctor_set(ctx, 0, config.into_ptr());

            // Object field 1: zetaDeltaSet (FVarIdSet) - use empty
            let empty_fvar_set = Self::mk_empty_fvar_id_set(lean)?;
            ffi::lean_ctor_set(ctx, 1, empty_fvar_set.into_ptr());

            // Object field 2: lctx (LocalContext) - use empty
            let empty_lctx = Self::mk_empty_local_context(lean)?;
            ffi::lean_ctor_set(ctx, 2, empty_lctx.into_ptr());

            // Object field 3: localInstances (LocalInstances) - use empty array
            let empty_array = ffi::array::lean_mk_empty_array();
            ffi::lean_ctor_set(ctx, 3, empty_array);

            // Object field 4: defEqCtx? (Option DefEqContext) - use none
            let none = LeanOption::none(lean)?;
            ffi::lean_ctor_set(ctx, 4, none.into_ptr());

            // Object field 5: synthPendingDepth (Nat) - use 0
            ffi::lean_ctor_set(ctx, 5, ffi::lean_box(0));

            // Object field 6: canUnfold? (Option ...) - use none
            let none2 = LeanOption::none(lean)?;
            ffi::lean_ctor_set(ctx, 6, none2.into_ptr());

            // Scalar offset 0-7: configKey (UInt64) - use 0
            ffi::lean_ctor_set_uint64(ctx, ctor_scalar_offset(7, 0), 0);

            // Scalar offset 8: trackZetaDelta (Bool) - use false (0)
            ffi::inline::lean_ctor_set_uint8(ctx, ctor_scalar_offset(7, 8), 0);

            // Scalar offset 9: univApprox (Bool) - use false (0)
            ffi::inline::lean_ctor_set_uint8(ctx, ctor_scalar_offset(7, 9), 0);

            // Scalar offset 10: inTypeClassResolution (Bool) - use false (0)
            ffi::inline::lean_ctor_set_uint8(ctx, ctor_scalar_offset(7, 10), 0);

            Ok(LeanBound::from_owned_ptr(lean, ctx))
        }
    }

    /// Create a default Meta.Config with correct reduction settings.
    ///
    /// Manually constructs a `Meta.Config` with the documented default values
    /// from Lean's `Meta.Config` structure definition. The `Inhabited` instance
    /// (`deriving Inhabited`) uses `Inhabited Bool = false` for all booleans,
    /// which disables reduction. We need the actual structure defaults instead.
    ///
    /// Layout (Lean 4.20.0): 0 object fields, 18 scalar bytes.
    /// ```text
    /// byte 0:  foApprox (Bool) = false
    /// byte 1:  ctxApprox (Bool) = false
    /// byte 2:  quasiPatternApprox (Bool) = false
    /// byte 3:  constApprox (Bool) = false
    /// byte 4:  isDefEqStuckEx (Bool) = false
    /// byte 5:  unificationHints (Bool) = true
    /// byte 6:  proofIrrelevance (Bool) = true
    /// byte 7:  assignSyntheticOpaque (Bool) = false
    /// byte 8:  offsetCnstrs (Bool) = true
    /// byte 9:  transparency (TransparencyMode) = .default (tag 1)
    /// byte 10: etaStruct (EtaStructMode) = .all (tag 0)
    /// byte 11: univApprox (Bool) = true
    /// byte 12: iota (Bool) = true
    /// byte 13: beta (Bool) = true
    /// byte 14: proj (ProjReductionKind) = .yesWithDelta (tag 2)
    /// byte 15: zeta (Bool) = true
    /// byte 16: zetaDelta (Bool) = true
    /// byte 17: zetaUnused (Bool) = true
    /// ```
    #[cfg(not(lean_4_25))]
    fn mk_default_config<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        unsafe {
            let config = ffi::lean_alloc_ctor(0, 0, 18);
            let base = ffi::inline::lean_ctor_scalar_cptr(config);
            *base.add(0) = 0; // foApprox = false
            *base.add(1) = 0; // ctxApprox = false
            *base.add(2) = 0; // quasiPatternApprox = false
            *base.add(3) = 0; // constApprox = false
            *base.add(4) = 0; // isDefEqStuckEx = false
            *base.add(5) = 1; // unificationHints = true
            *base.add(6) = 1; // proofIrrelevance = true
            *base.add(7) = 0; // assignSyntheticOpaque = false
            *base.add(8) = 1; // offsetCnstrs = true
            *base.add(9) = 1; // transparency = TransparencyMode.default (tag 1)
            *base.add(10) = 0; // etaStruct = EtaStructMode.all (tag 0)
            *base.add(11) = 1; // univApprox = true
            *base.add(12) = 1; // iota = true
            *base.add(13) = 1; // beta = true
            *base.add(14) = 2; // proj = ProjReductionKind.yesWithDelta (tag 2)
            *base.add(15) = 1; // zeta = true
            *base.add(16) = 1; // zetaDelta = true
            *base.add(17) = 1; // zetaUnused = true
            Ok(LeanBound::from_owned_ptr(lean, config))
        }
    }

    /// Create an empty FVarIdSet (`Std.HashSet FVarId`).
    ///
    /// In Lean 4.20 this is an `RBTree`, so the empty value erases to `leaf`.
    #[cfg(not(lean_4_25))]
    fn mk_empty_fvar_id_set<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        unsafe {
            let fvar_set = empty_rbmap_like();
            Ok(LeanBound::from_owned_ptr(lean, fvar_set))
        }
    }

    /// Create an empty LocalContext
    ///
    /// Uses the `Inhabited LocalContext` instance from the Lean runtime.
    /// Falls back to manual construction if the symbol is not available (Windows).
    ///
    /// Requires: `ensure_meta_initialized()` must have been called.
    #[cfg(not(lean_4_25))]
    fn mk_empty_local_context<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            if !force_manual_meta_defaults() {
                let lctx = ffi::meta::get_instInhabitedLocalContext();
                if !lctx.is_null() {
                    ffi::lean_inc(lctx);
                    return Ok(LeanBound::from_owned_ptr(lean, lctx));
                }
            }
            let obj = Self::mk_empty_local_context_manual();
            Ok(LeanBound::from_owned_ptr(lean, obj))
        }
    }

    /// Manually construct an empty LocalContext.
    #[cfg(not(lean_4_25))]
    unsafe fn mk_empty_local_context_manual() -> *mut ffi::lean_object {
        ffi::meta::lean_mk_empty_local_ctx(ffi::lean_box(0))
    }
}

/// Meta.State - state for MetaM monad
///
/// This structure has 5 fields (constructor tag 0):
/// 0. `mctx: MetavarContext` - use empty
/// 1. `cache: Cache` - use empty
/// 2. `zetaDeltaFVarIds: FVarIdSet` - use empty
/// 3. `postponed: PersistentArray PostponedEntry` - use empty
/// 4. `diag: Diagnostics` - use empty
#[repr(transparent)]
pub struct MetaState {
    _private: (),
}

impl MetaState {
    /// Create a Meta.State with empty state
    ///
    /// This creates a minimal Meta.State suitable for running MetaM computations.
    /// All fields are set to empty/default values:
    /// - mctx: empty MetavarContext
    /// - cache: empty Cache
    /// - zetaDeltaFVarIds: empty FVarIdSet
    /// - postponed: empty PersistentArray
    /// - diag: empty Diagnostics
    ///
    /// Uses the runtime's Inhabited instance when available, falling back to
    /// manual construction when BSS symbols are not exported (Windows).
    ///
    /// # Example
    ///
    /// ```ignore
    /// use leo3::prelude::*;
    /// use leo3::meta::*;
    ///
    /// leo3::with_lean(|lean| {
    ///     let meta_state = MetaState::mk_meta_state(lean)?;
    ///     Ok(())
    /// })
    /// ```
    pub fn mk_meta_state<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, Self>> {
        crate::runtime::ensure_meta_initialized();
        unsafe {
            if !force_manual_meta_defaults() {
                let state = ffi::meta::get_instInhabitedState();
                if !state.is_null() {
                    // BSS static: no inc (rc 0, dec no-op)
                    // ffi::lean_inc(state);
                    return Ok(LeanBound::from_owned_ptr(lean, state));
                }
            }
        }
        // Fallback: manually construct Meta.State
        Self::mk_meta_state_manual(lean)
    }

    /// Manually construct a Meta.State.
    ///
    /// Meta.State: 5 object fields, 0 scalar bytes (constructor tag 0)
    /// 0. mctx: MetavarContext
    /// 1. cache: Meta.Cache
    /// 2. zetaDeltaFVarIds: FVarIdSet (empty HashSet)
    /// 3. postponed: PersistentArray PostponedEntry (PersistentArray.empty)
    /// 4. diag: Meta.Diagnostics
    fn mk_meta_state_manual<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            let state = ffi::lean_alloc_ctor(0, 5, 0);

            // field 0: mctx (MetavarContext)
            let mctx = Self::mk_empty_metavar_context()?;
            ffi::lean_ctor_set(state, 0, mctx);

            // field 1: cache (Meta.Cache)
            let meta_cache = Self::mk_empty_meta_cache()?;
            ffi::lean_ctor_set(state, 1, meta_cache);

            // field 2: zetaDeltaFVarIds (FVarIdSet) — empty RBTree
            let fvar_set = empty_rbmap_like();
            ffi::lean_ctor_set(state, 2, fvar_set);

            // field 3: postponed (PersistentArray PostponedEntry) — empty
            let pa_empty = ffi::meta::get_PersistentArrayEmpty();
            // BSS static: no inc (rc 0, dec no-op)
            // ffi::lean_inc(pa_empty);
            ffi::lean_ctor_set(state, 3, pa_empty);

            // field 4: diag (Meta.Diagnostics)
            let diag = Self::mk_empty_diagnostics()?;
            ffi::lean_ctor_set(state, 4, diag);

            Ok(LeanBound::from_owned_ptr(lean, state))
        }
    }

    /// Create an empty MetavarContext.
    ///
    /// MetavarContext stores the Nat counters followed by the PersistentHashMaps.
    /// Lean <= 4.30: 9 object fields — 3 × Nat (depth, levelAssignDepth,
    /// mvarCounter) + 6 maps. Lean 4.31+: 10 object fields — the LMVarId
    ///    state was split out (`lmvarCounter`, `lDecls`; see `MetavarContext.lean`
    ///    of v4.31.0–v4.33.0 and 23393b95), so 4 × Nat + 6 maps.
    unsafe fn mk_empty_metavar_context() -> LeanResult<*mut ffi::lean_object> {
        if !force_manual_meta_defaults() {
            let mctx_bss = ffi::meta::get_instInhabitedMetavarContext();
            if !mctx_bss.is_null() {
                // BSS static: no inc (rc 0, dec no-op)
                // ffi::lean_inc(mctx_bss);
                return Ok(mctx_bss);
            }
        }
        #[cfg(lean_4_31)]
        let mctx = ffi::lean_alloc_ctor(0, 10, 0);
        #[cfg(not(lean_4_31))]
        let mctx = ffi::lean_alloc_ctor(0, 9, 0);
        #[cfg(lean_4_31)]
        {
            ffi::lean_ctor_set(mctx, 0, ffi::lean_box(0)); // depth
            ffi::lean_ctor_set(mctx, 1, ffi::lean_box(0)); // levelAssignDepth
            ffi::lean_ctor_set(mctx, 2, ffi::lean_box(0)); // lmvarCounter
            ffi::lean_ctor_set(mctx, 3, ffi::lean_box(0)); // mvarCounter
            let phm_empty = ffi::meta::get_PersistentHashMapEmpty();
            for i in 4..10u32 {
                ffi::lean_inc(phm_empty);
                ffi::lean_ctor_set(mctx, i, phm_empty);
            }
        }
        #[cfg(not(lean_4_31))]
        {
            ffi::lean_ctor_set(mctx, 0, ffi::lean_box(0)); // depth
            ffi::lean_ctor_set(mctx, 1, ffi::lean_box(0)); // levelAssignDepth
            ffi::lean_ctor_set(mctx, 2, ffi::lean_box(0)); // mvarCounter
            let phm_empty = ffi::meta::get_PersistentHashMapEmpty();
            for i in 3..9u32 {
                ffi::lean_inc(phm_empty);
                ffi::lean_ctor_set(mctx, i, phm_empty);
            }
        }
        Ok(mctx)
    }

    /// Create an empty Meta.Cache.
    ///
    /// Lean 4.20 Meta.Cache has six PersistentHashMap fields.
    unsafe fn mk_empty_meta_cache() -> LeanResult<*mut ffi::lean_object> {
        if !force_manual_meta_defaults() {
            let cache_bss = ffi::meta::get_instInhabitedCache();
            if !cache_bss.is_null() {
                // BSS static: no inc (rc 0, dec no-op)
                // ffi::lean_inc(cache_bss);
                return Ok(cache_bss);
            }
        }
        let cache = ffi::lean_alloc_ctor(0, 6, 0);
        let phm_empty = ffi::meta::get_PersistentHashMapEmpty();
        for i in 0..6u32 {
            ffi::lean_inc(phm_empty);
            ffi::lean_ctor_set(cache, i, phm_empty);
        }
        Ok(cache)
    }

    /// Create an empty Meta.Diagnostics.
    ///
    /// Lean 4.20 Meta.Diagnostics has four PersistentHashMap fields.
    unsafe fn mk_empty_diagnostics() -> LeanResult<*mut ffi::lean_object> {
        if !force_manual_meta_defaults() {
            let diag_bss = ffi::meta::get_instInhabitedDiagnostics();
            if !diag_bss.is_null() {
                // BSS static: no inc (rc 0, dec no-op)
                // ffi::lean_inc(diag_bss);
                return Ok(diag_bss);
            }
        }
        let diag = ffi::lean_alloc_ctor(0, 4, 0);
        let phm_empty = ffi::meta::get_PersistentHashMapEmpty();
        for i in 0..4u32 {
            ffi::lean_inc(phm_empty);
            ffi::lean_ctor_set(diag, i, phm_empty);
        }
        Ok(diag)
    }
}
