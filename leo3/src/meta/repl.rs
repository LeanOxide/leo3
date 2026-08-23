//! Repl-oriented elaborator API.
//!
//! Safe wrappers around Lean's real elaborator entry points: parsing tactic
//! strings, executing tactics on goals via `Lean.Elab.runTactic`, importing
//! compiled modules, and querying goal types. This is the substrate for
//! interactive proof-replay (LeanDojo-style) tooling built on Leo3.
//!
//! Verified against Lean 4.25.2. The elaborator symbols used here live in
//! `libleanshared.so` and are not part of Lean's public C API.

use crate::err::{LeanError, LeanResult};

/// Register a builtin tactic directly into the global `tacticElabAttribute`
/// table (`addBuiltin`), bypassing the module-initializer path which does
/// not run in embedded contexts. The table is snapshotted into every new
/// environment (`mkInitial`), so registering before `importModules` makes
/// the tactic available to all imported environments.
///
/// `arity` must be the full curried arity of the compiled tactic function
/// (verified against stage0 output: intro=10, skip=10, simp=11,
/// induction=10, exact=11).
unsafe fn register_builtin_tactic(
    lean: Lean<'_>,
    key: &str,
    decl_name: &str,
    fn_ptr: *mut std::ffi::c_void,
    arity: u32,
) -> LeanResult<()> {
    extern "C" {
        #[link_name = "l_Lean_KeyedDeclsAttribute_addBuiltin___redArg"]
        fn add_builtin(
            attr: *mut ffi::lean_object,
            key: *mut ffi::lean_object,
            decl_name: *mut ffi::lean_object,
            value: *mut ffi::lean_object,
            world: *mut ffi::lean_object,
        ) -> *mut ffi::lean_object;
    }
    // W-387: read the attribute via the reliable cross-platform accessor.
    // On Windows a raw `extern static` import of this DLL data symbol reads
    // null/stale, which would pass a null `attr` to `addBuiltin`. The accessor
    // routes Windows through `GetProcAddress` + deref.
    let tactic_attr = ffi::meta::get_tacticElabAttribute();
    let key_obj = LeanName::from_str(lean, key)?;
    let decl_obj = LeanName::from_str(lean, decl_name)?;
    let value = ffi::inline::lean_alloc_closure(fn_ptr, arity, 0);
    let world = ffi::io::lean_io_mk_world();
    // The callee consumes all object arguments; transfer ownership.
    let res = add_builtin(
        tactic_attr,
        key_obj.into_ptr(),
        decl_obj.into_ptr(),
        value,
        world,
    );
    if !ffi::io::lean_io_result_is_ok(res) {
        return Err(LeanError::other("addBuiltin failed"));
    }
    ffi::lean_dec(res);
    Ok(())
}

/// Register the core builtin tactics used by the Repl layer (once).
unsafe fn ensure_core_builtin_tactics(lean: Lean<'_>) -> LeanResult<()> {
    static REGISTERED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if REGISTERED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return Ok(());
    }
    extern "C" {
        #[link_name = "l_Lean_Elab_Tactic_evalIntro"]
        fn eval_intro(
            env: *mut *mut ffi::lean_object,
            arg: *mut ffi::lean_object,
        ) -> *mut ffi::lean_object;
        #[link_name = "l_Lean_Elab_Tactic_evalSkip"]
        fn eval_skip(
            env: *mut *mut ffi::lean_object,
            arg: *mut ffi::lean_object,
        ) -> *mut ffi::lean_object;
        #[link_name = "l_Lean_Elab_Tactic_evalSimp"]
        fn eval_simp(
            env: *mut *mut ffi::lean_object,
            arg: *mut ffi::lean_object,
        ) -> *mut ffi::lean_object;
        #[link_name = "l_Lean_Elab_Tactic_evalExact"]
        fn eval_exact(
            env: *mut *mut ffi::lean_object,
            arg: *mut ffi::lean_object,
        ) -> *mut ffi::lean_object;
        #[link_name = "l___private_Lean_Elab_Tactic_Induction_0__Lean_Elab_Tactic_evalInduction"]
        fn eval_induction(
            env: *mut *mut ffi::lean_object,
            arg: *mut ffi::lean_object,
        ) -> *mut ffi::lean_object;
    }
    register_builtin_tactic(
        lean,
        "Lean.Parser.Tactic.intro",
        "Lean.Elab.Tactic.evalIntro",
        eval_intro as *mut std::ffi::c_void,
        10,
    )?;
    register_builtin_tactic(
        lean,
        "Lean.Parser.Tactic.skip",
        "Lean.Elab.Tactic.evalSkip",
        eval_skip as *mut std::ffi::c_void,
        10,
    )?;
    register_builtin_tactic(
        lean,
        "Lean.Parser.Tactic.simp",
        "Lean.Elab.Tactic.evalSimp",
        eval_simp as *mut std::ffi::c_void,
        11,
    )?;
    register_builtin_tactic(
        lean,
        "Lean.Parser.Tactic.exact",
        "Lean.Elab.Tactic.evalExact",
        eval_exact as *mut std::ffi::c_void,
        11,
    )?;
    register_builtin_tactic(
        lean,
        "Lean.Parser.Tactic.induction",
        "Lean.Elab.Tactic.evalInduction",
        eval_induction as *mut std::ffi::c_void,
        10,
    )?;
    Ok(())
}

// `finalize_initialization` lives in `crate::runtime` (it is
// version-independent: `LeanEnvironment::empty` needs it on every supported
// Lean version, not only on the `lean_4_25`-gated repl layer); re-export it
// here so the historical `repl::finalize_initialization` path keeps working.
use crate::instance::{LeanAny, LeanBound};
use crate::marker::Lean;
use crate::meta::environment::LeanEnvironment;
use crate::meta::expr::LeanExpr;
use crate::meta::metam::MetaMContext;
use crate::meta::name::LeanName;
pub use crate::runtime::finalize_initialization;
use leo3_ffi as ffi;

/// Fully apply a curried Lean function (closed over `args`) and return its
/// result object.
///
/// The Lean C calling convention for curried functions is
/// `fn(env: *mut *mut lean_object, arg: *mut lean_object)`; we build a
/// closure over all but the last argument and apply the last one, which
/// triggers the function body.
///
/// # Safety
///
/// `fn_ptr` must be a compiled Lean function of the given arity and `args`
/// must supply exactly `arity` valid objects in order.
pub unsafe fn apply_curried(
    fn_ptr: *mut std::ffi::c_void,
    arity: usize,
    args: &[*mut ffi::lean_object],
) -> *mut ffi::lean_object {
    debug_assert_eq!(args.len(), arity, "curried arity mismatch");
    let fn_obj = fn_ptr as *mut ffi::lean_object;
    // `l_` export symbols come in two forms: raw code entries (e.g.
    // `lean_init_search_path`) and Lean function *objects* (closures, e.g.
    // `l_Lean_Parser_runParserCategory`). Detect closures by tag; code
    // entries must be wrapped in a `lean_alloc_closure` before applying.
    if ffi::inline::lean_is_closure(fn_obj) {
        match arity {
            1 => ffi::closure::lean_apply_1(fn_obj, args[0]),
            2 => ffi::closure::lean_apply_2(fn_obj, args[0], args[1]),
            3 => ffi::closure::lean_apply_3(fn_obj, args[0], args[1], args[2]),
            4 => ffi::closure::lean_apply_4(fn_obj, args[0], args[1], args[2], args[3]),
            5 => ffi::closure::lean_apply_5(fn_obj, args[0], args[1], args[2], args[3], args[4]),
            6 => ffi::closure::lean_apply_6(
                fn_obj, args[0], args[1], args[2], args[3], args[4], args[5],
            ),
            7 => ffi::closure::lean_apply_7(
                fn_obj, args[0], args[1], args[2], args[3], args[4], args[5], args[6],
            ),
            8 => ffi::closure::lean_apply_8(
                fn_obj, args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
            ),
            9 => {
                let partial = ffi::closure::lean_apply_8(
                    fn_obj, args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
                );
                ffi::closure::lean_apply_1(partial, args[8])
            }
            _ => unreachable!("apply_curried: unsupported arity {arity}"),
        }
    } else {
        // Code entry: wrap in a closure with the declared arity.
        let closure = ffi::inline::lean_alloc_closure(fn_ptr, arity as u32, 0);
        match arity {
            1 => ffi::closure::lean_apply_1(closure, args[0]),
            2 => ffi::closure::lean_apply_2(closure, args[0], args[1]),
            3 => ffi::closure::lean_apply_3(closure, args[0], args[1], args[2]),
            4 => ffi::closure::lean_apply_4(closure, args[0], args[1], args[2], args[3]),
            5 => ffi::closure::lean_apply_5(closure, args[0], args[1], args[2], args[3], args[4]),
            6 => ffi::closure::lean_apply_6(
                closure, args[0], args[1], args[2], args[3], args[4], args[5],
            ),
            7 => ffi::closure::lean_apply_7(
                closure, args[0], args[1], args[2], args[3], args[4], args[5], args[6],
            ),
            8 => ffi::closure::lean_apply_8(
                closure, args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
            ),
            9 => {
                let partial = ffi::closure::lean_apply_8(
                    closure, args[0], args[1], args[2], args[3], args[4], args[5], args[6], args[7],
                );
                ffi::closure::lean_apply_1(partial, args[8])
            }
            _ => unreachable!("apply_curried: unsupported arity {arity}"),
        }
    }
}

/// The empty persistent array (`Lean.PersistentArray.empty`), used as the
/// default `Term.Context.autoBoundImplicits`.
///
/// The BSS global `l_Lean_PersistentArray_empty` is the *object itself* (not
/// a pointer), so we call `Lean.PersistentArray.mkEmptyArray` instead,
/// which returns the runtime's static empty object.
unsafe fn persistent_array_empty() -> *mut ffi::lean_object {
    lean_persistent_array_mk_empty_array(ffi::lean_box(0))
}

// A 1-argument boolean function reused as the (never-invoked) default for
// `Term.Context.autoBoundImplicitForbidden`; with `autoBoundImplicit =
// false` the field is dead, so any safe 1-arg function works.
extern "C" {
    /// `Lean.Expr.isSort : Expr → Bool` (curried, arity 1)
    #[link_name = "l_Lean_Expr_isSort"]
    fn lean_expr_is_sort(
        env: *mut *mut ffi::lean_object,
        arg: *mut ffi::lean_object,
    ) -> *mut ffi::lean_object;

    /// `Lean.Name.quickLt : Name → Name → Ordering` (curried, arity 2)
    #[link_name = "l_Lean_Name_quickLt"]
    fn lean_name_quick_lt(
        env: *mut *mut ffi::lean_object,
        arg: *mut ffi::lean_object,
    ) -> *mut ffi::lean_object;

    /// `Std.DTreeMap.empty : (α → α → Ordering) → DTreeMap α β cmp`
    /// (curried, arity 1; the `NameMap` representation in Lean 4.25).
    #[link_name = "l_Std_DTreeMap_empty"]
    fn lean_std_dtreemap_empty(
        env: *mut *mut ffi::lean_object,
        arg: *mut ffi::lean_object,
    ) -> *mut ffi::lean_object;

    /// `Lean.PersistentArray.mkEmptyArray : α → PersistentArray α`
    /// (arg ignored; returns the runtime's static empty object).
    #[link_name = "l_Lean_PersistentArray_mkEmptyArray"]
    fn lean_persistent_array_mk_empty_array(x: *mut ffi::lean_object) -> *mut ffi::lean_object;
}

/// An empty `NameMap` (a `DTreeMap` in Lean 4.25): `DTreeMap.empty
/// Name.quickLt`. `DTreeMap` is a structure wrapping its tree, so the empty
/// value is *not* a scalar.
unsafe fn empty_name_map() -> *mut ffi::lean_object {
    let cmp = ffi::inline::lean_alloc_closure(lean_name_quick_lt as *mut std::ffi::c_void, 2u32, 0);
    apply_curried(lean_std_dtreemap_empty as *mut std::ffi::c_void, 1, &[cmp])
}

/// Construct a default `Lean.Elab.Term.Context` (all fields at their Lean
/// defaults), matching the target Lean version's field layout (verified
/// against the `TermElabM.lean` source of each version):
/// - Lean < 4.27 (4.25.x, 4.26.x): 7 object fields + 11 Bool scalars
///   (`autoBoundImplicits : PersistentArray` object, `autoBoundImplicit` scalar).
/// - Lean 4.27–4.28: 8 object fields + 10 Bool scalars (`autoBoundImplicits`
///   became `autoBoundImplicitContext : Option AutoBoundImplicitContext`,
///   `fixedTermElabs : Array FixedTermElabRef` was appended, and
///   `autoBoundImplicit : Bool` was removed; `checkDeprecated` was already
///   present since 4.25).
/// - Lean 4.29+: 8 object fields + 11 Bool scalars (the 4.27–4.28 layout
///   plus the `isMetaSection` scalar).
///
/// # Safety
///
/// Layout must match the Lean version the runtime was built with.
pub unsafe fn default_term_context<'l>(
    lean: Lean<'l>,
) -> LeanResult<LeanBound<'l, crate::instance::LeanAny>> {
    crate::runtime::ensure_meta_initialized();
    unsafe {
        #[cfg(lean_4_29)]
        {
            // Term.Context (Lean 4.29+): 8 object fields followed by
            // 11 scalar (Bool) bytes, per Lean's object-first ctor layout.
            // Object fields (declaration order): declName?, macroStack,
            // autoBoundImplicitContext, autoBoundImplicitForbidden,
            // sectionVars, sectionFVars, tacSnap?, fixedTermElabs.
            // Scalar bytes (declaration order): mayPostpone, errToSorry,
            // implicitLambda, heedElabAsElim, isNoncomputableSection,
            // isMetaSection, ignoreTCFailures, inPattern,
            // saveRecAppSyntax, holesAsSyntheticOpaque, checkDeprecated.
            const NUM_OBJ_FIELDS: u32 = 8;
            let ctx = ffi::lean_alloc_ctor(0, NUM_OBJ_FIELDS, 11);
            let set_obj = |i: u32, v: *mut ffi::lean_object| ffi::inline::lean_ctor_set(ctx, i, v);
            // Scalar slots live *after* the object area; the byte offset
            // counts from the object area start: num_objs * ptr_size +
            // scalar_index.
            let scalar_base = NUM_OBJ_FIELDS * std::mem::size_of::<*mut ffi::lean_object>() as u32;
            let set_bool = |scalar_idx: u32, v: u8| {
                ffi::inline::lean_ctor_set_uint8(ctx, scalar_base + scalar_idx, v)
            };

            // 0: declName? = none (first ctor = tag 0 = box(0))
            set_obj(0, ffi::inline::lean_box(0));
            // 1: macroStack = [] (List.nil = tag 0 = box(0))
            set_obj(1, ffi::inline::lean_box(0));
            // 2: autoBoundImplicitContext = none
            set_obj(2, ffi::inline::lean_box(0));
            // 3: autoBoundImplicitForbidden = (never-called) 1-arg bool fn
            set_obj(
                3,
                ffi::inline::lean_alloc_closure(
                    lean_expr_is_sort as *mut std::ffi::c_void,
                    1u32,
                    0,
                ),
            );
            // 4: sectionVars = {} (empty NameMap/DTreeMap)
            set_obj(4, empty_name_map());
            // 5: sectionFVars = {}
            set_obj(5, empty_name_map());
            // 6: tacSnap? = none
            set_obj(6, ffi::inline::lean_box(0));
            // 7: fixedTermElabs = #[] (Array ≡ PersistentArray; empty
            // singleton is shared)
            set_obj(7, persistent_array_empty());

            // Scalar bytes (all Bool, 1 = true, 0 = false).
            set_bool(0, 1); // mayPostpone
            set_bool(1, 1); // errToSorry
            set_bool(2, 1); // implicitLambda
            set_bool(3, 1); // heedElabAsElim
            set_bool(4, 0); // isNoncomputableSection
            set_bool(5, 0); // isMetaSection
            set_bool(6, 0); // ignoreTCFailures
            set_bool(7, 0); // inPattern
            set_bool(8, 1); // saveRecAppSyntax
            set_bool(9, 0); // holesAsSyntheticOpaque
            set_bool(10, 1); // checkDeprecated

            Ok(LeanBound::from_owned_ptr(lean, ctx))
        }
        #[cfg(all(lean_4_27, not(lean_4_29)))]
        {
            // Term.Context (Lean 4.27–4.28): 8 object fields followed by
            // 10 scalar (Bool) bytes. Same object layout as 4.29+, but the
            // `isMetaSection` scalar does not exist yet (`checkDeprecated`
            // has existed since 4.25). Object fields (declaration order):
            // declName?, macroStack, autoBoundImplicitContext,
            // autoBoundImplicitForbidden, sectionVars, sectionFVars,
            // tacSnap?, fixedTermElabs.
            const NUM_OBJ_FIELDS: u32 = 8;
            let ctx = ffi::lean_alloc_ctor(0, NUM_OBJ_FIELDS, 10);
            let set_obj = |i: u32, v: *mut ffi::lean_object| ffi::inline::lean_ctor_set(ctx, i, v);
            let scalar_base = NUM_OBJ_FIELDS * std::mem::size_of::<*mut ffi::lean_object>() as u32;
            let set_bool = |scalar_idx: u32, v: u8| {
                ffi::inline::lean_ctor_set_uint8(ctx, scalar_base + scalar_idx, v)
            };

            set_obj(0, ffi::inline::lean_box(0)); // declName? = none
            set_obj(1, ffi::inline::lean_box(0)); // macroStack = []
            set_obj(2, ffi::inline::lean_box(0)); // autoBoundImplicitContext = none
            set_obj(
                3,
                ffi::inline::lean_alloc_closure(
                    lean_expr_is_sort as *mut std::ffi::c_void,
                    1u32,
                    0,
                ),
            ); // autoBoundImplicitForbidden (never-called 1-arg bool fn)
            set_obj(4, empty_name_map()); // sectionVars = {}
            set_obj(5, empty_name_map()); // sectionFVars = {}
            set_obj(6, ffi::inline::lean_box(0)); // tacSnap? = none
            set_obj(7, persistent_array_empty()); // fixedTermElabs = #[]

            // Scalar bytes (declaration order, all Bool).
            set_bool(0, 1); // mayPostpone
            set_bool(1, 1); // errToSorry
            set_bool(2, 1); // implicitLambda
            set_bool(3, 1); // heedElabAsElim
            set_bool(4, 0); // isNoncomputableSection
            set_bool(5, 0); // ignoreTCFailures
            set_bool(6, 0); // inPattern
            set_bool(7, 1); // saveRecAppSyntax
            set_bool(8, 0); // holesAsSyntheticOpaque
            set_bool(9, 1); // checkDeprecated

            Ok(LeanBound::from_owned_ptr(lean, ctx))
        }
        #[cfg(not(lean_4_27))]
        {
            // Term.Context (Lean < 4.27): 7 object fields followed by
            // 11 scalar (Bool) bytes, per Lean's object-first ctor layout.
            // autoBoundImplicits, autoBoundImplicitForbidden, sectionVars,
            // sectionFVars, tacSnap?.
            // Scalar bytes (declaration order): mayPostpone, errToSorry,
            // autoBoundImplicit, implicitLambda, heedElabAsElim,
            // isNoncomputableSection, ignoreTCFailures, inPattern,
            // saveRecAppSyntax, holesAsSyntheticOpaque, checkDeprecated.
            const NUM_OBJ_FIELDS: u32 = 7;
            let ctx = ffi::lean_alloc_ctor(0, NUM_OBJ_FIELDS, 11);
            let set_obj = |i: u32, v: *mut ffi::lean_object| ffi::inline::lean_ctor_set(ctx, i, v);
            // Scalar slots live *after* the object area; the byte offset
            // counts from the object area start: num_objs * ptr_size +
            // scalar_index.
            let scalar_base = NUM_OBJ_FIELDS * std::mem::size_of::<*mut ffi::lean_object>() as u32;
            let set_bool = |scalar_idx: u32, v: u8| {
                ffi::inline::lean_ctor_set_uint8(ctx, scalar_base + scalar_idx, v)
            };

            // 0: declName? = none (first ctor = tag 0 = box(0))
            set_obj(0, ffi::inline::lean_box(0));
            // 1: macroStack = [] (List.nil = tag 0 = box(0))
            set_obj(1, ffi::inline::lean_box(0));
            // 2: autoBoundImplicits = PersistentArray.empty
            set_obj(2, persistent_array_empty());
            // 3: autoBoundImplicitForbidden = (never-called) 1-arg bool fn
            let forbidden = ffi::inline::lean_alloc_closure(
                lean_expr_is_sort as *mut std::ffi::c_void,
                1u32,
                0,
            );
            set_obj(3, forbidden);
            // 4: sectionVars = {} (empty NameMap/DTreeMap)
            set_obj(4, empty_name_map());
            // 5: sectionFVars = {}
            set_obj(5, empty_name_map());
            // 6: tacSnap? = none
            set_obj(6, ffi::inline::lean_box(0));

            // Scalar bytes (all Bool, 1 = true, 0 = false).
            set_bool(0, 1); // mayPostpone
            set_bool(1, 1); // errToSorry
            set_bool(2, 0); // autoBoundImplicit
            set_bool(3, 1); // implicitLambda
            set_bool(4, 1); // heedElabAsElim
            set_bool(5, 0); // isNoncomputableSection
            set_bool(6, 0); // ignoreTCFailures
            set_bool(7, 0); // inPattern
            set_bool(8, 1); // saveRecAppSyntax
            set_bool(9, 0); // holesAsSyntheticOpaque
            set_bool(10, 1); // checkDeprecated

            Ok(LeanBound::from_owned_ptr(lean, ctx))
        }
    }
}

/// The default `Lean.Elab.Term.State` (via the runtime's `Inhabited`
/// instance, avoiding a hardcoded layout).
///
/// # Safety
///
/// The Inhabited instance symbol must exist in the linked runtime.
pub unsafe fn default_term_state<'l>(
    lean: Lean<'l>,
) -> LeanResult<LeanBound<'l, crate::instance::LeanAny>> {
    crate::runtime::ensure_meta_initialized();
    unsafe {
        // The BSS symbol is the default `Term.State` object itself (7 object
        // fields on v4.25.2; `Inhabited` instance compiled to the bare value).
        // It is a *static* object with rc == 0: do NOT inc it, or the callee's
        // dec would drop the refcount to 0 and `lean_free` the static block
        // into mimalloc's freelist, corrupting the heap. rc stays 0, and
        // `lean_dec` is a no-op for rc == 0.
        let state = ffi::meta::repl::term_inst_inhabited_state();
        Ok(LeanBound::from_owned_ptr(lean, state))
    }
}

/// A copy of the default `Lean.Elab.Term.State` with a *fresh* empty
/// `MessageLog` installed in the `messages` slot (object field 6 on
/// v4.25.2).
///
/// `runTactic` writes tactic diagnostics (e.g. `exact 42` against a
/// proposition goal) into `Term.State.messages` as `error`-severity
/// messages and returns `Ok`; the caller must scan that log to surface
/// them, since the tactic itself does not throw. Feeding the shared static
/// default state would let those messages leak into the shared object, so
/// we clone the default and give it its own log (copies of every object
/// field except slot 6, which gets a fresh empty `MessageLog`).
///
/// # Safety
///
/// The default state is a static object (rc == 0): read its fields without
/// touching refcounts, and only materialize our own fresh log + increfs.
pub unsafe fn fresh_term_state_with_empty_messages<'l>(
    lean: Lean<'l>,
) -> LeanResult<LeanBound<'l, crate::instance::LeanAny>> {
    crate::runtime::ensure_meta_initialized();
    unsafe {
        let default = ffi::meta::repl::term_inst_inhabited_state();
        // v4.25: 7 object fields; slot 6 is `messages`.  Clone all object
        // slots, replacing slot 6 with a fresh empty MessageLog.
        let n_fields = 7;
        let new_state = ffi::lean_alloc_ctor(0, n_fields, 0);
        for i in 0..n_fields {
            if i == 6 {
                // Fresh empty MessageLog { reported := ∅, unreported := ∅,
                //   loggedKinds := ∅ } — the `PersistentArray.empty` +
                //   empty TreeSet layout, matching `mk_empty_message_log`.
                let pa = ffi::meta::get_PersistentArrayEmpty();
                if pa.is_null() {
                    return Err(LeanError::other(
                        "PersistentArray.empty unavailable for Term.State messages",
                    ));
                }
                let msg_log = ffi::lean_alloc_ctor(0, 3, 0);
                ffi::lean_ctor_set(msg_log, 0, pa);
                ffi::lean_ctor_set(msg_log, 1, pa);
                ffi::lean_ctor_set(msg_log, 2, ffi::lean_box(1));
                ffi::lean_ctor_set(new_state, i, msg_log);
            } else {
                let src = std::ptr::read::<u64>((default as *const u64).add(1 + i as usize))
                    as *mut ffi::lean_object;
                if !src.is_null() {
                    ffi::lean_inc(src);
                }
                ffi::lean_ctor_set(new_state, i, src);
            }
        }
        Ok(LeanBound::from_owned_ptr(lean, new_state))
    }
}

/// Parse a term string into a `Syntax` object using Lean's real parser
/// (`term` category).
pub fn parse_term<'l>(
    lean: Lean<'l>,
    env: &LeanBound<'l, LeanEnvironment>,
    input: &str,
) -> LeanResult<LeanBound<'l, LeanExpr>> {
    crate::runtime::ensure_meta_initialized();
    unsafe {
        let cat = LeanName::from_str(lean, "term")?;
        let input_obj = crate::types::LeanString::mk(lean, input)?;
        let file = crate::types::LeanString::mk(lean, "<stdin>")?;

        let result = apply_curried(
            ffi::meta::repl::lean_parser_run_parser_category as *mut std::ffi::c_void,
            4,
            &[
                // runParserCategory consumes its arguments; inc the borrowed env.
                {
                    ffi::lean_inc(env.as_ptr());
                    env.as_ptr()
                },
                cat.into_ptr(),
                input_obj.into_ptr(),
                file.into_ptr(),
            ],
        );

        // Except String Syntax — Lean 4.25 declares `error` before `ok`,
        // so error = tag 0, ok = tag 1 (unlike EStateM.Result where ok = 0).
        if ffi::lean_obj_tag(result) == 1 {
            let syntax = ffi::lean_ctor_get(result, 0) as *mut ffi::lean_object;
            ffi::lean_inc(syntax);
            ffi::lean_dec(result);
            Ok(LeanBound::from_owned_ptr(lean, syntax))
        } else {
            // error branch: field 0 is the String message.
            let err = ffi::lean_ctor_get(result, 0) as *mut ffi::lean_object;
            let c_str = ffi::inline::lean_string_cstr(err);
            let message = if c_str.is_null() {
                "<unprintable>".to_string()
            } else {
                std::ffi::CStr::from_ptr(c_str)
                    .to_string_lossy()
                    .into_owned()
            };
            ffi::lean_dec(result);
            Err(LeanError::other(
                format!("term parse error: {message}").as_str(),
            ))
        }
    }
}

/// Parse a tactic string into a `Syntax` object using Lean's real parser.
///
/// Calls `Lean.Parser.runParserCategory` (pure function) with the `tactic`
/// category.
pub fn parse_tactic<'l>(
    lean: Lean<'l>,
    env: &LeanBound<'l, LeanEnvironment>,
    input: &str,
) -> LeanResult<LeanBound<'l, LeanExpr>> {
    crate::runtime::ensure_meta_initialized();
    unsafe {
        let cat = LeanName::from_str(lean, "tactic")?;
        let input_obj = crate::types::LeanString::mk(lean, input)?;
        let file = crate::types::LeanString::mk(lean, "<stdin>")?;

        let result = apply_curried(
            ffi::meta::repl::lean_parser_run_parser_category as *mut std::ffi::c_void,
            4,
            &[
                // runParserCategory consumes its arguments; inc the borrowed env.
                {
                    ffi::lean_inc(env.as_ptr());
                    env.as_ptr()
                },
                cat.into_ptr(),
                input_obj.into_ptr(),
                file.into_ptr(),
            ],
        );

        // Except String Syntax — Lean 4.25 declares `error` before `ok`,
        // so error = tag 0, ok = tag 1 (unlike EStateM.Result where ok = 0).
        if ffi::lean_obj_tag(result) == 1 {
            let syntax = ffi::lean_ctor_get(result, 0) as *mut ffi::lean_object;
            ffi::lean_inc(syntax);
            ffi::lean_dec(result);
            Ok(LeanBound::from_owned_ptr(lean, syntax))
        } else {
            // error branch: field 0 is the String message.
            let err = ffi::lean_ctor_get(result, 0) as *mut ffi::lean_object;
            let c_str = ffi::inline::lean_string_cstr(err);
            let message = if c_str.is_null() {
                "<unprintable>".to_string()
            } else {
                std::ffi::CStr::from_ptr(c_str)
                    .to_string_lossy()
                    .into_owned()
            };
            ffi::lean_dec(result);
            Err(LeanError::other(
                format!("tactic parse error: {message}").as_str(),
            ))
        }
    }
}

/// Parse a command string using Lean's real parser (`command` category).
pub fn parse_command<'l>(
    lean: Lean<'l>,
    env: &LeanBound<'l, LeanEnvironment>,
    input: &str,
) -> LeanResult<LeanBound<'l, LeanExpr>> {
    crate::runtime::ensure_meta_initialized();
    unsafe {
        let cat = LeanName::from_str(lean, "command")?;
        let input_obj = crate::types::LeanString::mk(lean, input)?;
        let file = crate::types::LeanString::mk(lean, "<stdin>")?;

        let result = apply_curried(
            ffi::meta::repl::lean_parser_run_parser_category as *mut std::ffi::c_void,
            4,
            &[
                {
                    ffi::lean_inc(env.as_ptr());
                    env.as_ptr()
                },
                cat.into_ptr(),
                input_obj.into_ptr(),
                file.into_ptr(),
            ],
        );

        if ffi::lean_obj_tag(result) == 1 {
            let syntax = ffi::lean_ctor_get(result, 0) as *mut ffi::lean_object;
            ffi::lean_inc(syntax);
            ffi::lean_dec(result);
            Ok(LeanBound::from_owned_ptr(lean, syntax))
        } else {
            let err = ffi::lean_ctor_get(result, 0) as *mut ffi::lean_object;
            let c_str = ffi::inline::lean_string_cstr(err);
            let message = if c_str.is_null() {
                "<unprintable>".to_string()
            } else {
                std::ffi::CStr::from_ptr(c_str)
                    .to_string_lossy()
                    .into_owned()
            };
            ffi::lean_dec(result);
            Err(LeanError::other(
                format!("command parse error: {message}").as_str(),
            ))
        }
    }
}

/// Register a Lean sysroot directory in the search path so that
/// `import` can find modules under it.
pub fn init_search_path<'l>(lean: Lean<'l>, sysroot: &str) -> LeanResult<()> {
    crate::runtime::ensure_meta_initialized();
    unsafe {
        let path = crate::types::LeanString::mk(lean, sysroot)?;
        let world = ffi::io::lean_io_mk_world();
        let result = apply_curried(
            ffi::meta::repl::lean_init_search_path as *mut std::ffi::c_void,
            2,
            &[path.into_ptr(), world],
        );
        super::metam::handle_eio_result(result)?;
        Ok(())
    }
}

/// Discover the Lean system root: `LEAN_SYSROOT` env, else the Lean
/// library directory this crate was linked against (baked in at build
/// time by `leo3-build-config` as `LEO3_LEAN_LIB_DIR`), else `lean
/// --print-prefix` (one subprocess).
///
/// The linked-library directory wins over `lean --print-prefix` because
/// the binary's RPATH pins it to the `libleanshared.so` of the toolchain
/// it was built against, while `lean --print-prefix` follows elan and the
/// process cwd's `lean-toolchain` file — which can resolve to a different
/// Lean version (e.g. a repo-pinned toolchain under a `LEO3_CROSS_LIB_DIR`
/// cross build), making the runtime read `.olean` files it cannot parse
/// ("incompatible header").
fn discover_sysroot() -> LeanResult<String> {
    if let Ok(root) = std::env::var("LEAN_SYSROOT") {
        if !root.is_empty() {
            return Ok(root);
        }
    }
    if let Some(lib_dir) = option_env!("LEO3_LEAN_LIB_DIR") {
        if let Some(sysroot) = sysroot_from_lib_dir(lib_dir) {
            return Ok(sysroot);
        }
    }
    let out = std::process::Command::new("lean")
        .arg("--print-prefix")
        .output()
        .map_err(|e| LeanError::other(&format!("failed to run `lean --print-prefix`: {e}")))?;
    if !out.status.success() {
        return Err(LeanError::other(&format!(
            "`lean --print-prefix` exited with {}",
            out.status
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

/// The sysroot for a standard-layout `<sysroot>/lib/lean` library
/// directory, or `None` if the path is not shaped that way.
fn sysroot_from_lib_dir(lib_dir: &str) -> Option<String> {
    let lib = std::path::Path::new(lib_dir);
    if lib.file_name() != Some(std::ffi::OsStr::new("lean")) {
        return None;
    }
    let lib_parent = lib.parent()?;
    if lib_parent.file_name() != Some(std::ffi::OsStr::new("lib")) {
        return None;
    }
    Some(lib_parent.parent()?.to_string_lossy().into_owned())
}

/// Ensure Lean's search path reflects the current `LEAN_PATH` + sysroot,
/// so module imports resolve `.olean` files. Safe to call repeatedly.
///
/// The sysroot comes from `discover_sysroot` (`LEAN_SYSROOT`, then the
/// linked toolchain, then `lean --print-prefix`). If it points at a
/// different library directory than the one the binary was built
/// against, this fails with an explicit error: loading `.olean` files
/// from another Lean version aborts the process ("incompatible header").
///
/// The embedded runtime's `lean_init_search_path` (=
/// `initSearchPathInternal`) derives the system root from `IO.appDir.parent`
/// and prepends `LEAN_PATH` from the environment. We make that lookup
/// concrete: ensure `LEAN_PATH` (if unset) contains `sysroot/lib/lean`, so
/// the standard library (and, via an explicitly-set `LEAN_PATH`, lake-built
/// packages such as Mathlib) resolve regardless of the host executable's
/// directory.
///
/// Re-initialized on every call (not once per process) so a caller that
/// changes `LEAN_PATH` between sessions — e.g. to point at a lake-built
/// Mathlib — is honored on the next import.
pub fn ensure_search_path<'l>(lean: Lean<'l>) -> LeanResult<()> {
    let sysroot = discover_sysroot()?;
    let lean_lib = format!("{sysroot}/lib/lean");
    // The binary's RPATH pins it to the `libleanshared.so` of the
    // toolchain it was built against. If the resolved search path points
    // at a different library directory, the runtime would read `.olean`
    // files from another Lean version and abort on import ("incompatible
    // header") — fail fast with an explicit error instead.
    if let Some(linked) = option_env!("LEO3_LEAN_LIB_DIR") {
        if !same_lib_dir(&lean_lib, linked) {
            return Err(LeanError::other(&format!(
                "Lean module search path mismatch: this binary is linked against the \
                 Lean libraries in '{linked}', but the search path resolves to \
                 '{lean_lib}' (sysroot '{sysroot}'). Olean files from a different Lean \
                 version cannot be loaded (\"incompatible header\"). Point LEAN_SYSROOT \
                 at the linked toolchain, or rebuild against it."
            )));
        }
    }
    let lean_path = std::env::var("LEAN_PATH")
        .map(|p| format!("{p}:{lean_lib}"))
        .unwrap_or_else(|_| lean_lib.clone());
    // Ignore failure to set the env var (read-only environ in some
    // embedded hosts); `init_search_path` still tries `IO.appDir`.
    std::env::set_var("LEAN_PATH", lean_path);
    init_search_path(lean, &sysroot)?;
    Ok(())
}

/// True if two Lean library directories name the same path, comparing
/// after normalizing separators, trailing slashes, and (on Windows) case.
fn same_lib_dir(a: &str, b: &str) -> bool {
    let normalize = |s: &str| {
        let normalized = s.replace('\\', "/");
        let s = normalized.trim_end_matches('/');
        #[cfg(windows)]
        {
            s.to_ascii_lowercase()
        }
        #[cfg(not(windows))]
        {
            s.to_string()
        }
    };
    normalize(a) == normalize(b)
}

/// Import compiled Lean modules (e.g. `Init`) into a fresh environment.
///
/// Calls `Lean.importModules` (IO). The search path is initialized from
/// the linked toolchain's sysroot (overridable via `LEAN_SYSROOT`) plus
/// `LEAN_PATH` on first use.
pub fn import_modules<'l>(
    lean: Lean<'l>,
    names: &[&str],
    trust_level: u32,
) -> LeanResult<LeanBound<'l, LeanEnvironment>> {
    import_modules_with_exts(lean, names, trust_level, true)
}

/// Like [`import_modules`] but with explicit `loadExts` control. When
/// `load_exts` is false, environment-extension data (parser/syntax
/// registrations) is not restored from `.olean` files, but builtin tactic
/// registrations survive; when true the opposite can happen.
pub fn import_modules_with_exts<'l>(
    lean: Lean<'l>,
    names: &[&str],
    trust_level: u32,
    load_exts: bool,
) -> LeanResult<LeanBound<'l, LeanEnvironment>> {
    crate::runtime::ensure_meta_initialized();
    unsafe {
        ensure_search_path(lean)?;
        // `Lean.importModules` wraps each call in `withImporting`, whose
        // finally block resets `runInitializersRef` to false.  Re-enable it
        // immediately before each import on the worker thread; doing this
        // only once in process bootstrap means later Repl sessions load
        // constants but skip Mathlib's `[init]` registrations.
        // End the initialization phase BEFORE importing: environment
        // creation (`lean_mk_empty_environment`) requires
        // `IO.initializing == false`, and initializer execution during
        // import only depends on `enableInitializersExecution`.
        finalize_initialization();
        // Register core builtin tactics before creating the imported
        // environment: new environments snapshot the attribute table.
        ensure_core_builtin_tactics(lean)?;
        // Array Import: one Import = ctor(0, 1, 0) with field 0 = module Name.
        let imports = if names.is_empty() {
            ffi::array::lean_alloc_array(0, 0)
        } else {
            let arr = ffi::array::lean_alloc_array(names.len(), names.len());
            for (i, name) in names.iter().enumerate() {
                // Dot-separated module names (`basic.MyThm`, `Lean.Data.Name`)
                // must be built as hierarchical `Name` objects so `getRoot`
                // resolves the module directory (`basic/`, `Lean/Data/`).
                // `from_str` would produce a single flat component and the
                // olean lookup would fail with `unknown module prefix`.
                let nm = LeanName::from_components(lean, name)?;
                let imp = ffi::lean_alloc_ctor(0, 1, 0);
                ffi::inline::lean_ctor_set(imp, 0, nm.into_ptr());
                ffi::array::lean_array_set_core(arr, i, imp);
            }
            arr
        };

        let opts = ffi::meta::repl::options_empty();
        let plugins = ffi::array::lean_alloc_array(0, 0);
        // Empty NameMap (DTreeMap).
        let arts = empty_name_map();

        let result = crate::runtime::with_worker(move || {
            // Must run on the worker thread: importModules allocates Lean
            // objects (olean reading, environment construction), and Lean
            // objects must not cross mimalloc thread-local heaps.
            // Direct mixed-ABI call (not curried): scalar params are raw
            // u32/u8 values.
            let enable_world = ffi::io::lean_io_mk_world();
            let enable_res =
                ffi::lean_enable_initializer_execution(enable_world as *mut std::ffi::c_void);
            let enable_ok = ffi::io::lean_io_result_is_ok(enable_res);
            ffi::lean_dec(enable_res);
            if !enable_ok {
                return Err(LeanError::other(
                    "failed to enable initializer execution before import",
                ));
            }
            let world = ffi::io::lean_io_mk_world();
            let result = ffi::meta::repl::lean_import_modules_full(
                imports,
                opts,
                trust_level,
                plugins,
                0,                   // leakEnv
                u8::from(load_exts), // loadExts
                2,                   // OLeanLevel.private
                arts,
                world,
            );
            super::metam::handle_eio_result(result)
        })?;

        Ok(LeanBound::from_owned_ptr(lean, result))
    }
}

/// The outcome of applying a real tactic to a goal.
pub struct RunTacticOutcome<'l> {
    /// Remaining (unsolved) goals as metavariable IDs (`MVarId = Name`).
    pub goals: Vec<LeanBound<'l, LeanName>>,
    /// The resulting `Term.State` (carries pending synthetic metavariables
    /// etc. that the next tactic application must see).
    pub term_state: LeanBound<'l, crate::instance::LeanAny>,
    /// The `ST.Ref` through which the computation threaded `Meta.State`.
    /// The next `run_tactic` call must reuse this ref (it is mutated
    /// in place), not rebuild it from the (stale) bare state.
    pub meta_state_ref: LeanBound<'l, crate::instance::LeanAny>,
}

/// Execute a real Lean tactic string on a goal metavariable.
///
/// Calls `Lean.Elab.runTactic` with default `Term.Context`/`Term.State`,
/// runs it through the persistent MetaM backend (which updates the context's
/// stored `Meta.State`/`Core.State`), and returns the remaining goals plus
/// the produced `Term.State`.
///
/// # Errors
///
/// Returns the Lean exception (tactic failure, unknown tactic, ...) with a
/// best-effort rendered message.
pub fn run_tactic<'l>(
    metam: &mut MetaMContext<'l>,
    mvar: &LeanBound<'l, LeanName>,
    stx: &LeanBound<'l, LeanExpr>,
    persistent_meta_ref: Option<&LeanBound<'l, crate::instance::LeanAny>>,
) -> LeanResult<RunTacticOutcome<'l>> {
    crate::runtime::ensure_meta_initialized();

    let term_ctx = unsafe { default_term_context(metam.lean())? };
    // Run the tactic against a Term.State whose `messages` slot is a fresh
    // empty log: `runTactic` returns `Ok` even when it recorded an
    // error-severity diagnostic (e.g. `exact 42` on a proposition goal),
    // and we surface those below rather than silently reporting success.
    let term_state = unsafe { fresh_term_state_with_empty_messages(metam.lean())? };
    let mvar_ptr = mvar.as_ptr();
    let stx_ptr = stx.as_ptr();
    unsafe {
        ffi::lean_inc(mvar_ptr);
        ffi::lean_inc(stx_ptr);
    }
    let term_ctx_ptr = term_ctx.into_ptr();
    let term_state_ptr = term_state.into_ptr();

    let meta_ctx = metam.meta_ctx().clone();
    let core_ctx = metam.core_ctx().clone();
    let core_state = metam.core_state().clone();
    let meta_ctx_ptr = meta_ctx.into_ptr();
    let core_ctx_ptr = core_ctx.into_ptr();
    let core_state_ptr = core_state.into_ptr();
    let metam_meta_state_ptr = metam.meta_state().clone().into_ptr();

    let (alpha, new_meta_state, new_core_state, new_term_state, kept_meta_ref) =
        crate::runtime::with_worker(move || unsafe {
            // Meta.State ref: reuse the caller's persistent ref if given,
            // else wrap the context's current bare state.
            let meta_state_ref = if let Some(r) = &persistent_meta_ref {
                ffi::lean_inc(r.as_ptr());
                r.as_ptr()
            } else {
                #[cfg(not(lean_4_26))]
                {
                    let world_in = ffi::lean_box(0);
                    let ref_result = ffi::lean_st_mk_ref(metam_meta_state_ptr, world_in);
                    let r = ffi::lean_ctor_get(ref_result, 0) as *mut ffi::lean_object;
                    ffi::lean_inc(r);
                    ffi::lean_dec(ref_result);
                    r
                }
                #[cfg(lean_4_26)]
                {
                    ffi::lean_st_mk_ref(metam_meta_state_ptr)
                }
            };
            #[cfg(not(lean_4_26))]
            let (core_state_ref, world) = {
                let world_in = ffi::lean_box(0);
                let ref_result = ffi::lean_st_mk_ref(core_state_ptr, world_in);
                let r = ffi::lean_ctor_get(ref_result, 0) as *mut ffi::lean_object;
                let w = ffi::lean_ctor_get(ref_result, 1) as *mut ffi::lean_object;
                ffi::lean_inc(r);
                ffi::lean_inc(w);
                ffi::lean_dec(ref_result);
                (r, w)
            };
            #[cfg(lean_4_26)]
            let (core_state_ref, world) =
                { (ffi::lean_st_mk_ref(core_state_ptr), ffi::lean_box(0)) };
            // Keep our own references so the refs survive the callee's dec.
            ffi::lean_inc(meta_state_ref);
            ffi::lean_inc(core_state_ref);

            let result = ffi::meta::repl::lean_elab_run_tactic_full(
                mvar_ptr,
                stx_ptr,
                term_ctx_ptr,
                term_state_ptr,
                meta_ctx_ptr,
                meta_state_ref,
                core_ctx_ptr,
                core_state_ref,
                world,
            );
            // The computation threaded Meta.State and Core.State through
            // their refs in place; read both back for the next step.
            let read_ref = |r: *mut ffi::lean_object| -> *mut ffi::lean_object {
                #[cfg(lean_4_26)]
                {
                    // Lean >= 4.26 (ST redesign): the C ABI dropped the
                    // world token, so `lean_st_ref_get` is a 1-arg export
                    // returning the value directly. Decoding the return as
                    // the old `(value, world)` pair would read `field 0` of
                    // the state itself (e.g. Core.State's `env`) and
                    // corrupt the session.
                    ffi::lean_st_ref_get(r)
                }
                #[cfg(not(lean_4_26))]
                {
                    let res = ffi::lean_st_ref_get(r, world);
                    let v = ffi::lean_ctor_get(res, 0) as *mut ffi::lean_object;
                    ffi::lean_inc(v);
                    ffi::lean_dec(res);
                    v
                }
            };
            let updated_meta = read_ref(meta_state_ref);
            let updated_core = read_ref(core_state_ref);

            // TermElabM.run returns Result.ok((α', Term.State), world):
            // pair = (α', term_state) with α' = (goals, X).
            let pair = super::metam::handle_eio_result(result)?;
            let alpha = ffi::lean_ctor_get(pair, 0) as *mut ffi::lean_object;
            let term_state_ptr = ffi::lean_ctor_get(pair, 1) as *mut ffi::lean_object;
            ffi::lean_inc(alpha);
            ffi::lean_inc(term_state_ptr);
            ffi::lean_dec(pair);

            Ok::<
                (
                    *mut ffi::lean_object,
                    *mut ffi::lean_object,
                    *mut ffi::lean_object,
                    *mut ffi::lean_object,
                    *mut ffi::lean_object,
                ),
                LeanError,
            >((
                alpha,
                updated_meta,
                updated_core,
                term_state_ptr,
                meta_state_ref,
            ))
        })?;

    unsafe {
        // Surface error-severity diagnostics the tactic recorded but did
        // not throw (e.g. `exact 42` against a proposition goal): scan the
        // `Core.State.messages` log the computation threaded through the
        // Core.State ref (via `CoreM.logMessage` → `messages.add`), and
        // reject on the first `error` message, like `run_command` does for
        // commands. Scanned BEFORE `update_states` consumes `new_core_state`;
        // the scan borrows the pointer without taking ownership.
        if let Some(msg) = {
            let core_state = LeanBound::<crate::instance::LeanAny>::from_borrowed_ptr(
                metam.lean(),
                new_core_state,
            );
            scan_core_state_error(&core_state)
        } {
            return Err(LeanError::other(&format!("tactic error: {msg}")));
        }

        // Core.State came back in the result; Meta.State lives in the kept
        // ref (threaded in place) — store that ref so the next step reuses it.
        metam.update_states(
            LeanBound::<crate::instance::LeanAny>::from_owned_ptr(metam.lean(), new_meta_state),
            LeanBound::<crate::instance::LeanAny>::from_owned_ptr(metam.lean(), new_core_state),
        );

        // α' is `List MVarId` (runTactic : MetaM (List MVarId × Term.State)):
        // nil = box(0), cons = tag 1 (head, tail).
        let mut goals = Vec::new();
        let mut cur = alpha;
        while !ffi::inline::lean_is_scalar(cur) && ffi::lean_obj_tag(cur) == 1 {
            let head = ffi::lean_ctor_get(cur, 0) as *mut ffi::lean_object;
            let tail = ffi::lean_ctor_get(cur, 1) as *mut ffi::lean_object;
            ffi::lean_inc(head);
            goals.push(LeanBound::from_owned_ptr(metam.lean(), head));
            ffi::lean_inc(tail);
            cur = tail;
        }

        // The Meta.State ref is the persistent state carrier: the next call
        // must reuse THIS ref object (its content was threaded in place).
        let term_state = LeanBound::from_owned_ptr(metam.lean(), new_term_state);
        let meta_ref: LeanBound<crate::instance::LeanAny> =
            LeanBound::from_owned_ptr(metam.lean(), kept_meta_ref);

        Ok(RunTacticOutcome {
            goals,
            term_state,
            meta_state_ref: meta_ref,
        })
    }
}

/// Scan the message log of a `Lean.Core.State` (object field 6 on v4.25)
/// for the first `error`-severity entry and return its rendered text (with
/// the usual `<file>:<line>:<col>: error: ...` position prefix). Returns
/// `None` when the log has no error.
///
/// Mirrors the message scan in `run_command`: `MessageLog.toList` (pure) +
/// `Message.serialize` + `SerialMessage.toString` render each message, and
/// we look for the `: error` severity marker after the position prefix.
///
/// # Safety
///
/// `core_state` must be a real `Lean.Core.State` object; the log is read
/// without mutation.
unsafe fn scan_core_state_error<'l>(
    core_state: &LeanBound<'l, crate::instance::LeanAny>,
) -> Option<String> {
    unsafe {
        let state = core_state.as_ptr();
        // v4.25 Core.State: 9 object fields, `messages` is object field 6
        // (see `CoreState::mk_core_state`, `field_offset + 2`).
        let msg_log =
            std::ptr::read::<u64>((state as *const u64).add(1 + 6)) as *mut ffi::lean_object;
        ffi::lean_inc(msg_log);
        extern "C" {
            #[link_name = "l_Lean_MessageLog_toList"]
            fn lean_msglog_tolist(a: *mut ffi::lean_object) -> *mut ffi::lean_object;
            #[link_name = "l_Lean_Message_serialize"]
            fn lean_message_serialize(
                a: *mut ffi::lean_object,
                w: *mut ffi::lean_object,
            ) -> *mut ffi::lean_object;
            #[link_name = "l_Lean_SerialMessage_toString"]
            fn lean_serial_to_string(
                a: *mut ffi::lean_object,
                b: *mut ffi::lean_object,
            ) -> *mut ffi::lean_object;
        }
        let c = ffi::inline::lean_alloc_closure(lean_msglog_tolist as *mut std::ffi::c_void, 1, 0);
        let lst = ffi::closure::lean_apply_1(c, msg_log);
        let mut err_msg: Option<String> = None;
        let mut cur = lst;
        while !ffi::inline::lean_is_scalar(cur) && err_msg.is_none() {
            let m = ffi::lean_ctor_get(cur, 0) as *mut ffi::lean_object;
            ffi::lean_inc(m);
            let w = ffi::io::lean_io_mk_world();
            let c2 = ffi::inline::lean_alloc_closure(
                lean_message_serialize as *mut std::ffi::c_void,
                2,
                0,
            );
            let r = ffi::closure::lean_apply_2(c2, m, w);
            if !ffi::inline::lean_is_scalar(r) && ffi::lean_obj_tag(r) == 0 {
                let sm = ffi::lean_ctor_get(r, 0) as *mut ffi::lean_object;
                ffi::lean_inc(sm);
                ffi::lean_dec(r);
                let c3 = ffi::inline::lean_alloc_closure(
                    lean_serial_to_string as *mut std::ffi::c_void,
                    2,
                    0,
                );
                let s2 = ffi::closure::lean_apply_2(c3, sm, ffi::lean_box(0));
                let cs = ffi::inline::lean_string_cstr(s2);
                if !cs.is_null() {
                    let txt = std::ffi::CStr::from_ptr(cs).to_string_lossy().into_owned();
                    // Severity markers: `: error: ...` or `: error(name): ...`
                    // after the position prefix.
                    if txt.contains(": error") {
                        err_msg = Some(txt.trim().to_string());
                    }
                }
                ffi::lean_dec(s2);
            }
            cur = ffi::lean_ctor_get(cur, 1) as *mut ffi::lean_object;
        }
        ffi::lean_dec(lst);
        err_msg
    }
}
///
/// Runs `Lean.Meta.ppGoal` (delaborator + pretty printer) in the session's
/// `MetaM` context and renders the resulting `Format` with
/// `Format.pretty`. Unlike `LeanExpr::dbg_to_string` — which prints free
/// variables by their internal id — the output uses the local context's
/// user-facing names and the usual notations, e.g.
/// `n m : Nat ⊢ n + m = m + n`.
///
/// # Errors
///
/// Returns the Lean exception if pretty-printing fails (e.g. the goal
/// metavariable is not in the context).
pub fn pp_goal<'l>(
    metam: &mut MetaMContext<'l>,
    mvar: &LeanBound<'l, LeanName>,
) -> LeanResult<String> {
    crate::runtime::ensure_meta_initialized();

    let meta_ctx = metam.meta_ctx().clone();
    let core_ctx = metam.core_ctx().clone();
    let core_state = metam.core_state().clone();
    let meta_state = metam.meta_state().clone();
    let mvar_ptr = mvar.as_ptr();
    unsafe {
        ffi::lean_inc(mvar_ptr);
    }
    let meta_ctx_ptr = meta_ctx.into_ptr();
    let core_ctx_ptr = core_ctx.into_ptr();
    let core_state_ptr = core_state.into_ptr();
    let meta_state_ptr = meta_state.into_ptr();

    let rendered = crate::runtime::with_worker(move || unsafe {
        // State refs, following the run_tactic pattern: wrap the session's
        // current Meta.State / Core.State in fresh ST refs plus a world token.
        #[cfg(not(lean_4_26))]
        let meta_state_ref = {
            let world_in = ffi::lean_box(0);
            let ref_result = ffi::lean_st_mk_ref(meta_state_ptr, world_in);
            let r = ffi::lean_ctor_get(ref_result, 0) as *mut ffi::lean_object;
            ffi::lean_inc(r);
            ffi::lean_dec(ref_result);
            r
        };
        #[cfg(lean_4_26)]
        let meta_state_ref = ffi::lean_st_mk_ref(meta_state_ptr);
        #[cfg(not(lean_4_26))]
        let (core_state_ref, world) = {
            let world_in = ffi::lean_box(0);
            let ref_result = ffi::lean_st_mk_ref(core_state_ptr, world_in);
            let r = ffi::lean_ctor_get(ref_result, 0) as *mut ffi::lean_object;
            let w = ffi::lean_ctor_get(ref_result, 1) as *mut ffi::lean_object;
            ffi::lean_inc(r);
            ffi::lean_inc(w);
            ffi::lean_dec(ref_result);
            (r, w)
        };
        #[cfg(lean_4_26)]
        let (core_state_ref, world) = (ffi::lean_st_mk_ref(core_state_ptr), ffi::lean_box(0));
        // Keep our own references so the refs survive the callee's dec; we
        // release them after the call (ppGoal is read-only).
        ffi::lean_inc(meta_state_ref);
        ffi::lean_inc(core_state_ref);

        let result = ffi::meta::repl::lean_meta_pp_goal(
            mvar_ptr,
            meta_ctx_ptr,
            meta_state_ref,
            core_ctx_ptr,
            core_state_ref,
            world,
        );
        let fmt = super::metam::handle_eio_result(result)?;
        // Render the Format with the default width (120), no indent/column.
        let width = ffi::inline::lean_usize_to_nat(120);
        let indent = ffi::inline::lean_usize_to_nat(0);
        let column = ffi::inline::lean_usize_to_nat(0);
        let s = ffi::meta::repl::lean_format_pretty(fmt, width, indent, column);

        // ppGoal does not mutate the state refs; drop our extra references.
        ffi::lean_dec(meta_state_ref);
        ffi::lean_dec(core_state_ref);
        Ok::<*mut ffi::lean_object, LeanError>(s)
    })?;

    unsafe {
        let s = LeanBound::<crate::types::LeanString>::from_owned_ptr(metam.lean(), rendered);
        Ok(crate::types::LeanString::cstr(&s)?.to_string())
    }
}

/// Pretty-print an expression with Lean's real pretty printer.
///
/// Runs `Meta.ppExpr` with a `Meta.Context` whose local context and local
/// instances are the given goal's, so free variables resolve to their
/// user-facing names and notations are applied (e.g. `n + m` rather than
/// `HAdd.hAdd.{0, 0, 0} Nat Nat Nat (instHAdd.{0} Nat instAddNat) 65 68`).
///
/// We build the context by copying the session's current `Meta.Context`
/// (same config/options) and patching the `lctx` and `localInstances`
/// slots, then call the fully-uncurried `lean_meta_pp_expr` export
/// directly. `Lean.Meta.withLCtx` cannot be used here: its `___redArg`
/// export expects its action argument as a closure in an undocumented
/// calling convention (it dereferences `[arg2+0x10]` unconditionally, so a
/// nil `LocalInstances` — a scalar `box(0)` — segfaults inside the callee).
///
/// `Meta.Context` layout (verified on 4.25.2, both against the `Inhabited`
/// static and against `Lean.Meta.withLCtx'`'s compiled code, which patches
/// `[ctx+0x18]`): 7 object slots + 3 trailing bool bytes, 0x48 bytes total:
/// keyedConfig(0), trackZetaDelta(1, scalar), lctx(2), localInstances(3),
/// defEqCtx?(4, scalar `none`), synthPendingDepth(5, scalar),
/// canUnfold?(6, scalar `none`), bytes 0x40-0x42 (univApprox,
/// inTypeClassResolution).
pub fn pp_expr<'l>(
    metam: &MetaMContext<'l>,
    lctx: &LeanBound<'l, crate::instance::LeanAny>,
    local_instances: &LeanBound<'l, crate::instance::LeanAny>,
    e: &LeanBound<'l, LeanExpr>,
) -> LeanResult<String> {
    // Delegate to the batched implementation: a single-expression batch is
    // one worker trip, so this preserves the single-expression contract
    // without duplicating the refcounting.
    pp_exprs(metam, lctx, local_instances, std::slice::from_ref(e))?
        .pop()
        .ok_or_else(|| LeanError::other("pp_exprs returned no result for a single expression"))
}

/// Batch variant of [`pp_expr`]: pretty-print several expressions in a
/// **single** worker trip.
///
/// Each expression is rendered under the same local context / local
/// instances (the goal's), so the per-iteration work is one patched
/// `Meta.Context` plus one `lean_meta_pp_expr` call. The state refs and
/// world token are created **once** and shared across the loop: the
/// callee consumes the references we hand it each iteration (we
/// pre-increment before every call), while the underlying
/// `Meta.State` / `Core.State` are read-only and survive. This collapses
/// the `(h+1)` serialized worker rendezvous of the per-hyp `pp_expr`
/// loop in `goal_hyps_and_type_pp` into one.
///
/// Returns the rendered strings in input order. An empty slice returns an
/// empty vec (no worker trip).
pub fn pp_exprs<'l>(
    metam: &MetaMContext<'l>,
    lctx: &LeanBound<'l, crate::instance::LeanAny>,
    local_instances: &LeanBound<'l, crate::instance::LeanAny>,
    exprs: &[LeanBound<'l, LeanExpr>],
) -> LeanResult<Vec<String>> {
    if exprs.is_empty() {
        return Ok(Vec::new());
    }
    crate::runtime::ensure_meta_initialized();

    let meta_ctx = metam.meta_ctx().clone();
    let core_ctx = metam.core_ctx().clone();
    let core_state = metam.core_state().clone();
    let meta_state = metam.meta_state().clone();
    let lctx_ptr = lctx.as_ptr();
    let insts_ptr = local_instances.as_ptr();
    // One owned reference per expression (the callee consumes `e`).
    let e_ptrs: Vec<*mut ffi::lean_object> = exprs.iter().map(|e| e.as_ptr()).collect();
    unsafe {
        // One owned reference per expression (the callee consumes `e`).
        // lctx / localInstances are inc'd per-iteration inside the loop,
        // because each patched `new_ctx` takes its own reference (slots 2/3)
        // and the callee decs them when it consumes the context.
        for p in &e_ptrs {
            ffi::lean_inc(*p);
        }
    }
    let meta_ctx_ptr = meta_ctx.into_ptr();
    let core_ctx_ptr = core_ctx.into_ptr();
    let core_state_ptr = core_state.into_ptr();
    let meta_state_ptr = meta_state.into_ptr();

    let rendered: Vec<*mut ffi::lean_object> = crate::runtime::with_worker(move || unsafe {
        // State refs + world, created once and shared across the loop.
        // The callee decs the references it is handed; we keep one base
        // reference each (the `mk_ref` extraction) and drop it at the end.
        #[cfg(not(lean_4_26))]
        let meta_state_ref = {
            let world_in = ffi::lean_box(0);
            let ref_result = ffi::lean_st_mk_ref(meta_state_ptr, world_in);
            let r = ffi::lean_ctor_get(ref_result, 0) as *mut ffi::lean_object;
            ffi::lean_inc(r);
            ffi::lean_dec(ref_result);
            r
        };
        #[cfg(lean_4_26)]
        let meta_state_ref = ffi::lean_st_mk_ref(meta_state_ptr);
        #[cfg(not(lean_4_26))]
        let (core_state_ref, world) = {
            let world_in = ffi::lean_box(0);
            let ref_result = ffi::lean_st_mk_ref(core_state_ptr, world_in);
            let r = ffi::lean_ctor_get(ref_result, 0) as *mut ffi::lean_object;
            let w = ffi::lean_ctor_get(ref_result, 1) as *mut ffi::lean_object;
            ffi::lean_inc(r);
            ffi::lean_inc(w);
            ffi::lean_dec(ref_result);
            (r, w)
        };
        #[cfg(lean_4_26)]
        let (_core_state_ref, _world) = (ffi::lean_st_mk_ref(core_state_ptr), ffi::lean_box(0));
        let mut out: Vec<*mut ffi::lean_object> = Vec::with_capacity(e_ptrs.len());
        for e_ptr in e_ptrs {
            // Pre-increment the shared refs; the callee consumes these.
            ffi::lean_inc(meta_state_ref);
            #[cfg(not(lean_4_26))]
            ffi::lean_inc(core_state_ref);
            // Each patched context below owns a fresh reference to lctx /
            // localInstances (slots 2/3); the callee decs them.
            ffi::lean_inc(lctx_ptr);
            ffi::lean_inc(insts_ptr);

            // Patched Meta.Context: 7 object slots + 8 trailing scalar
            // bytes (see `pp_expr`); slots 2/3 take the goal's lctx /
            // localInstances, all other object slots are refcounted.
            let new_ctx = ffi::lean_alloc_ctor(0, 7, 8);
            for i in 0..7u32 {
                let src = *(meta_ctx_ptr as *const u64).add(1 + i as usize);
                let v = match i {
                    2 => lctx_ptr,
                    3 => insts_ptr,
                    _ => {
                        if src & 1 == 0 {
                            ffi::lean_inc(src as *mut ffi::lean_object);
                        }
                        src as *mut ffi::lean_object
                    }
                };
                ffi::inline::lean_ctor_set(new_ctx, i, v);
            }
            *(new_ctx as *mut u64).add(8) = *(meta_ctx_ptr as *const u64).add(8);

            let result = ffi::meta::repl::lean_meta_pp_expr(
                e_ptr,
                new_ctx,
                meta_state_ref,
                core_ctx_ptr,
                #[cfg(not(lean_4_26))]
                core_state_ref,
                #[cfg(lean_4_26)]
                _core_state_ref,
                #[cfg(not(lean_4_26))]
                world,
                #[cfg(lean_4_26)]
                _world,
            );
            let fmt = super::metam::handle_eio_result(result)?;
            let width = ffi::inline::lean_usize_to_nat(120);
            let indent = ffi::inline::lean_usize_to_nat(0);
            let column = ffi::inline::lean_usize_to_nat(0);
            let s = ffi::meta::repl::lean_format_pretty(fmt, width, indent, column);
            out.push(s);
        }
        // Drop the base references kept alive across the loop.
        ffi::lean_dec(meta_state_ref);
        #[cfg(not(lean_4_26))]
        ffi::lean_dec(core_state_ref);
        Ok::<Vec<*mut ffi::lean_object>, LeanError>(out)
    })?;

    let mut strings = Vec::with_capacity(rendered.len());
    unsafe {
        for p in rendered {
            let s = LeanBound::<crate::types::LeanString>::from_owned_ptr(metam.lean(), p);
            strings.push(crate::types::LeanString::cstr(&s)?.to_string());
        }
    }
    Ok(strings)
}

// ============================================================================
// Command execution (run_cmd)
// ============================================================================

/// Result of [`run_command_core`]: the updated `Environment` plus, when
/// the command recorded an `error`-severity message in the command log,
/// that message and the text the production path renders from it —
/// `Some` carries both together. The `env` pointer and the `Message`
/// pointer (when present) carry owned refcounts that the caller must
/// release.
struct CommandOutcome {
    env: *mut ffi::lean_object,
    error_message: Option<(*mut ffi::lean_object, String)>,
}

/// Execute a parsed command via `Lean.Elab.Command.elabCommand` (5-arg
/// direct: `(stx, cmdCtx, cmdStateRef, cmdState, world)`), returning the
/// updated `Environment` from the final `Command.State` and the first
/// `error`-severity message recorded in the command log, if any.
///
/// # Safety
///
/// `stx` must be a valid command syntax object. The returned
/// `Environment` and `Message` (if any) carry owned refcounts.
unsafe fn run_command_core<'l>(
    lean: Lean<'l>,
    metam: &MetaMContext<'l>,
    stx: &LeanBound<'l, LeanExpr>,
) -> LeanResult<CommandOutcome> {
    let cmd_ctx = mk_command_context(lean, stx.as_ptr())?;
    let env = metam.env().clone();

    let stx_ptr = {
        ffi::lean_inc(stx.as_ptr());
        stx.as_ptr()
    };
    crate::runtime::run_worker(move || -> LeanResult<CommandOutcome> {
        // Must run on the worker thread: elabCommand allocates Lean
        // objects (olean reading, environment construction), and Lean
        // objects must not cross minimal-move local heaps.
        let cmd_state_owned = mk_command_state(&env)?;
        #[cfg(not(lean_4_26))]
        let (cmd_state_ref, world) = {
            // 4.25.2 `lean_st_mk_ref` is the 2-arg export: it consumes
            // `init` (the ref keeps a reference) and never reads the
            // world argument (disassembly-verified), returning
            // `(ref, 1)` — the world token is the immediate unit
            // constant, so every world inc/dec below is a no-op. The
            // dummy box only fills the ignored slot; the callee holds
            // no reference to it, so release it right after the call
            // (same pattern as the 4.26+ dummy box below).
            let world_in = ffi::lean_box(0);
            let ref_result = ffi::lean_st_mk_ref(cmd_state_owned, world_in);
            let cmd_state_ref = ffi::lean_ctor_get(ref_result, 0) as *mut ffi::lean_object;
            let world = ffi::lean_ctor_get(ref_result, 1) as *mut ffi::lean_object;
            ffi::lean_inc(cmd_state_ref);
            ffi::lean_inc(world);
            ffi::lean_dec(ref_result);
            ffi::lean_dec(world_in);
            (cmd_state_ref, world)
        };
        #[cfg(lean_4_26)]
        let (cmd_state_ref, world) = {
            // Lean >= 4.26: `lean_st_mk_ref` is a 1-arg C export
            // returning the ST.Ref directly, and `lean_st_ref_get`
            // is 1-arg with no world token. The only world slot left
            // is the Lean-level one at the tail of
            // `elabCommandTopLevel`, so the dummy box exists solely to
            // fill that slot and is released once after the read-back.
            let world = ffi::lean_box(0);
            (ffi::lean_st_mk_ref(cmd_state_owned), world)
        };
        // The callee also consumes the ref argument (lean_obj_arg):
        // keep our own reference so the ref survives the call (same
        // pattern as run_tactic's "Keep our own references"); it is
        // released right after the state read-back below.
        ffi::lean_inc(cmd_state_ref);

        // `elabCommandTopLevel` is the real frontend entry.
        // Pre-4.32 arity 4: (stx, ctx, ref, world); from 4.32 the
        // signature gained a leading `cmds : Array Syntax` linter
        // bookkeeping parameter (default `#[]`), shifting the ABI to
        // (stx, cmds, ctx, ref, world).
        #[cfg(lean_4_32)]
        let result = ffi::meta::repl::lean_elab_command_top_level_5(
            stx_ptr,
            ffi::array::lean_mk_empty_array(),
            cmd_ctx.into_ptr(),
            cmd_state_ref,
            world,
        );
        #[cfg(not(lean_4_32))]
        let result = ffi::meta::repl::lean_elab_command_top_level_4(
            stx_ptr,
            cmd_ctx.into_ptr(),
            cmd_state_ref,
            world,
        );
        // On the (rare) exception path the read-back below never
        // runs, so release the per-call ref and world token here.
        let pair = match crate::meta::metam::handle_eio_result(result) {
            Ok(pair) => pair,
            Err(e) => {
                ffi::lean_dec(cmd_state_ref);
                ffi::lean_dec(world);
                return Err(e);
            }
        };
        ffi::lean_dec(pair);

        // The elaboration pipeline threads the final Command.State
        // through the ref in place; read it back.
        #[cfg(lean_4_26)]
        // Lean >= 4.26: `lean_st_ref_get` returns the value directly.
        let state = ffi::lean_st_ref_get(cmd_state_ref);
        #[cfg(not(lean_4_26))]
        let state = {
            let final_ref = ffi::lean_st_ref_get(cmd_state_ref, world);
            let state = ffi::lean_ctor_get(final_ref, 0) as *mut ffi::lean_object;
            ffi::lean_inc(state);
            ffi::lean_dec(final_ref);
            state
        };
        // The state is read back from the ref: release the per-call
        // ST ref (the pipeline's final Command.State is now owned by
        // `state`) and the world token (pre-4.26: the real token;
        // 4.26+: the dummy that fills the `elabCommandTopLevel`
        // tail slot).
        ffi::lean_dec(cmd_state_ref);
        ffi::lean_dec(world);
        // Command.State field 0 = Environment.
        let env_out = std::ptr::read::<u64>((state as *const u64).add(1)) as *mut ffi::lean_object;
        ffi::lean_inc(env_out);

        // Error reporting: `withLogging` swallows elaboration
        // failures into the command message log (no exception
        // propagates), so scan the log for the first `error`-severity
        // message, render its text, and keep the raw `Message` object
        // for callers that inspect the runtime layout.
        //
        // `Message = BaseMessage MessageData` has a flat layout
        // stable across 4.25.2..4.33: object fields first in
        // declaration order (`fileName`, `pos`, `endPos`,
        // `caption`, `data`), then the scalar bytes
        // (`keepFullRange`, `severity`, `isSilent`).
        // `MessageSeverity`: information=0, warning=1, error=2, so
        // `severity` is the second scalar byte. The scalar offset
        // passed to `lean_ctor_get_uint8` is relative to the object
        // field array (just past the 8-byte header), so after 5
        // object fields (`5 * 8 = 40`) severity sits at
        // `40 + 1 = 41`. The error text is read from the raw
        // message's own `data` field (object field 4, a
        // `MessageData`) and rendered via the version-robust
        // `MessageData.toString`. Reading the raw message (rather
        // than a serialized `SerialMessage`) avoids the
        // serialize/`l_Lean_SerialMessage_toString` shape
        // disagreement in 4.33.0-rc1.
        let error_message = first_error_message(state).map(|m| {
            let caption = message_field_string(m, 3);
            let data_md = ffi::lean_ctor_get(m, 4) as *mut ffi::lean_object;
            ffi::lean_inc(data_md);
            let data = super::metam::extract_message_data(data_md);
            ffi::lean_dec(data_md);
            let text = if caption.is_empty() {
                if data.is_empty() {
                    "command failed".to_string()
                } else {
                    data
                }
            } else {
                format!("{caption}: {data}")
            };
            (m, text)
        });
        ffi::lean_dec(state);
        Ok(CommandOutcome {
            env: env_out,
            error_message,
        })
    })
}

/// Execute a parsed command via `Lean.Elab.Command.elabCommand`,
/// returning the updated `Environment` from the final
/// `Command.State`.
///
/// `withLogging` swallows elaboration failures into the command message
/// log (no exception propagates), so the first `error`-severity message
/// is surfaced as a [`LeanError`] instead.
pub fn run_command<'l>(
    lean: Lean<'l>,
    metam: &MetaMContext<'l>,
    stx: &LeanBound<'l, LeanExpr>,
) -> LeanResult<LeanBound<'l, LeanEnvironment>> {
    unsafe {
        let outcome = run_command_core(lean, metam, stx)?;
        if let Some((m, msg)) = outcome.error_message {
            ffi::lean_dec(m);
            let msg = format!("run_cmd: command failed: {msg}");
            ffi::lean_dec(outcome.env);
            return Err(LeanError::other(&msg));
        }

        // Note: commands that never touch the environment (`#check`,
        // `set_option` with no options change, ...) legitimately
        // return the same environment object — do not reject them;
        // elaboration failures are surfaced via the message check.
        Ok(LeanBound::from_owned_ptr(lean, outcome.env))
    }
}

/// Test-only (enabled by the `runtime-tests` feature): parse and run a
/// command, and return the first `error`-severity `Message` recorded in
/// the command log — the raw object plus the text the production path
/// renders from it — so tests can assert the runtime `Message` layout
/// directly. Returns `Ok(None)` when the command records no error.
#[cfg(feature = "runtime-tests")]
pub fn test_first_error_message<'l>(
    lean: Lean<'l>,
    metam: &MetaMContext<'l>,
    cmd: &str,
) -> LeanResult<Option<(LeanBound<'l, LeanAny>, String)>> {
    let stx = parse_command(lean, metam.env(), cmd)?;
    unsafe {
        let outcome = run_command_core(lean, metam, &stx)?;
        ffi::lean_dec(outcome.env);
        match outcome.error_message {
            Some((obj, text)) => Ok(Some((LeanBound::from_owned_ptr(lean, obj), text))),
            None => Ok(None),
        }
    }
}

/// Scan the message log of a `Command.State` (object field 1) for the
/// first `error`-severity message and return it with an owned
/// refcount, or `None` when the log holds no error.
///
/// # Safety
///
/// `state` must be a valid `Command.State` object; the severity read
/// relies on the `Message` layout documented in `run_command_core`.
unsafe fn first_error_message(state: *const ffi::lean_object) -> Option<*mut ffi::lean_object> {
    extern "C" {
        #[link_name = "l_Lean_MessageLog_toList"]
        fn lean_msglog_tolist(a: *mut ffi::lean_object) -> *mut ffi::lean_object;
    }
    let msg_log = std::ptr::read::<u64>((state as *const u64).add(2)) as *mut ffi::lean_object;
    ffi::lean_inc(msg_log);
    let c = ffi::inline::lean_alloc_closure(lean_msglog_tolist as *mut std::ffi::c_void, 1, 0);
    let lst = ffi::closure::lean_apply_1(c, msg_log);
    let mut found: Option<*mut ffi::lean_object> = None;
    let mut cur = lst;
    while !ffi::inline::lean_is_scalar(cur) && found.is_none() {
        let m = ffi::lean_ctor_get(cur, 0) as *mut ffi::lean_object;
        let severity = if !ffi::inline::lean_is_scalar(m) {
            // Second scalar byte (see `Message` layout note in
            // `run_command_core`).
            ffi::inline::lean_ctor_get_uint8(m as ffi::b_lean_obj_arg, 41)
        } else {
            0
        };
        if severity == 2 {
            ffi::lean_inc(m);
            found = Some(m);
        }
        cur = ffi::lean_ctor_get(cur, 1) as *mut ffi::lean_object;
    }
    ffi::lean_dec(lst);
    found
}

/// Read a Lean `String` stored in object field `f` of `o`, returning an
/// empty string when the field is a scalar.
///
/// # Safety
///
/// `o` must be a valid constructor object with more than `f` object
/// fields.
unsafe fn message_field_string(o: *mut ffi::lean_object, f: u32) -> String {
    let s = ffi::lean_ctor_get(o, f) as *mut ffi::lean_object;
    if ffi::inline::lean_is_scalar(s) {
        return String::new();
    }
    let cs = ffi::inline::lean_string_cstr(s as ffi::b_lean_obj_arg);
    if cs.is_null() {
        String::new()
    } else {
        std::ffi::CStr::from_ptr(cs).to_string_lossy().into_owned()
    }
}

/// Read a `Lean.Name` as its dotted string form (str/num segments),
/// mirroring `Name.toString` for the common hierarchical names used as
/// syntax-node kinds. Returns `None` for malformed names.
/// Parse a Lean source file into its top-level commands, returning the
/// command syntaxes in order.
///
/// Uses Lean's real `command`-category parser (`runParserCategory`, the
/// same entry the repl uses for single commands) line by line, accumulating
/// lines until the accumulated buffer parses as exactly one command — this
/// handles multi-line commands such as long `theorem` bodies. `import`
/// commands are skipped (module loading is the caller's responsibility via
/// [`import_modules`]). A file whose final accumulated lines fail to parse
/// yields a `LeanError` with the parser message. Commands must be
/// separated by newlines (one command per line is the Lean convention; a
/// single line holding several commands is rejected with an explicit
/// error). Feed the results to [`run_command`] to elaborate a file.
pub fn parse_file_commands<'l>(
    lean: Lean<'l>,
    env: &LeanBound<'l, LeanEnvironment>,
    input: &str,
    file_name: &str,
) -> LeanResult<Vec<LeanBound<'l, LeanExpr>>> {
    crate::runtime::ensure_meta_initialized();
    let _ = file_name;
    let mut out: Vec<LeanBound<'l, LeanExpr>> = Vec::new();
    let mut buf = String::new();
    let mut line_no = 0usize;
    // `lines()` splits on '\n' without the terminator; reassemble with
    // '\n' so the parser sees faithful positions.
    for line in input.split('\n') {
        line_no += 1;
        if line_no == 1 && buf.is_empty() && line.trim().is_empty() {
            continue; // skip leading blank lines
        }
        buf.push_str(line);
        buf.push('\n');
        // Parse the accumulated candidate with trailing whitespace
        // stripped: Lean's `runParserCategory` requires the whole input to
        // be consumed, and a trailing newline after a complete command is
        // reported as "expected end of input".
        let candidate = buf.trim_end();
        if candidate.is_empty() {
            buf.clear();
            continue;
        }
        // `import` lives at module level, not in the `command` category —
        // skip import lines entirely (module loading is the caller's job).
        if candidate.starts_with("import ") || candidate == "import" {
            buf.clear();
            continue;
        }
        match parse_command(lean, env, candidate) {
            Ok(stx) => {
                // One complete command: check it is not an import, keep it.
                out.push(stx);
                buf.clear();
            }
            Err(e) => {
                let msg = format!("{e}");
                if msg.contains("unexpected end of input") {
                    // Incomplete command (multi-line body): keep accumulating.
                    continue;
                }
                if msg.contains("expected end of input") {
                    // The candidate is complete but holds trailing content
                    // that is not whitespace: several commands on one line.
                    let msg2 = format!(
                        "parse_file_commands: multiple commands on one line (line {line_no}) \
                         are not supported: {msg}"
                    );
                    return Err(LeanError::other(&msg2));
                }
                // Any other parse failure: could still be a multi-line
                // command, keep accumulating; the trailing buffer reports
                // the error at EOF if it never completes.
            }
        }
    }
    if !buf.trim().is_empty() {
        // Trailing lines never formed a complete command.
        let msg2 = format!(
            "parse_file_commands: incomplete or invalid command at end of file (line {line_no}): {buf:?}"
        );
        return Err(LeanError::other(&msg2));
    }
    Ok(out)
}

/// Build a default `Lean.Elab.Command.Context` (v4.25 layout: 10 object
/// fields + 1 Bool scalar: suppressElabErrors). `stx` becomes `ctx.ref`
/// (matching the real frontend).
unsafe fn mk_command_context<'l>(
    lean: Lean<'l>,
    stx: *mut ffi::lean_object,
) -> LeanResult<LeanBound<'l, LeanAny>> {
    let ctx = ffi::lean_alloc_ctor(0, 10, 1);
    // 0: fileName
    let file_name = crate::types::LeanString::mk(lean, "<stdin>")?;
    ffi::lean_ctor_set(ctx, 0, file_name.into_ptr());
    // 1: fileMap (BSS static, no inc — dec is a no-op)
    let file_map = ffi::meta::get_instInhabitedFileMap();
    if file_map.is_null() {
        return Err(LeanError::other("instInhabitedFileMap unavailable"));
    }
    ffi::lean_ctor_set(ctx, 1, file_map);
    // 2: currRecDepth = 0
    ffi::lean_ctor_set(ctx, 2, ffi::lean_box(0));
    // 3: cmdPos = 0
    ffi::lean_ctor_set(ctx, 3, ffi::lean_box(0));
    // 4: macroStack = []
    ffi::lean_ctor_set(ctx, 4, ffi::lean_box(0));
    // 5: quotContext? = none
    ffi::lean_ctor_set(ctx, 5, ffi::lean_box(0));
    // 6: currMacroScope = firstFrontendMacroScope = 1
    ffi::lean_ctor_set(ctx, 6, ffi::lean_box(1));
    // 7: ref = the command's syntax (like the real frontend does)
    ffi::lean_inc(stx);
    ffi::lean_ctor_set(ctx, 7, stx);
    // 8: snap? = none
    ffi::lean_ctor_set(ctx, 8, ffi::lean_box(0));
    // 9: cancelTk? = none
    ffi::lean_ctor_set(ctx, 9, ffi::lean_box(0));
    // scalar 0: suppressElabErrors = false
    ffi::inline::lean_ctor_set_uint8(ctx, 10 * 8, 0);
    Ok(LeanBound::from_owned_ptr(lean, ctx))
}

/// Build a `Lean.Elab.Command.State` from an environment.
///
/// Layout: 11 object fields on Lean 4.25–4.33
/// (env, messages, scopes, usedQuotCtxts, nextMacroScope, maxRecDepth,
/// ngen, auxDeclNGen, infoState, traceState, snapshotTasks), plus a
/// 12th object field `prevLinterStates : Option (Task (Array
/// LinterState))` (default `none`) appended in Lean 4.34 — verified
/// against the `Command.lean` sources of v4.25.2, v4.28.0, v4.30.0,
/// v4.31.0, v4.32.0, v4.33.0 and the 4.34-nightly commit 23393b95.
unsafe fn mk_command_state<'l>(
    env: &LeanBound<'l, LeanEnvironment>,
) -> LeanResult<*mut ffi::lean_object> {
    // Command.State: 11 object fields on Lean 4.25–4.33, 12 on 4.34+
    // (the 4.34 field `prevLinterStates` is appended last).
    #[cfg(lean_4_34)]
    let state = ffi::lean_alloc_ctor(0, 12, 0);
    #[cfg(not(lean_4_34))]
    let state = ffi::lean_alloc_ctor(0, 11, 0);
    // 0: env
    let env_ptr = env.as_ptr();
    ffi::lean_inc(env_ptr);
    ffi::lean_ctor_set(state, 0, env_ptr);
    // 1: messages = MessageLog { reported := ∅, unreported := ∅,
    //    loggedKinds := ∅ } — 3 fields (loggedKinds : NameSet = empty
    //    TreeSet, the scalar box(1) = 3).
    let pa = ffi::meta::get_PersistentArrayEmpty();
    if pa.is_null() {
        return Err(LeanError::other("PersistentArray.empty unavailable"));
    }
    let msg_log = ffi::lean_alloc_ctor(0, 3, 0);
    ffi::lean_ctor_set(msg_log, 0, pa);
    ffi::lean_ctor_set(msg_log, 1, pa);
    ffi::lean_ctor_set(msg_log, 2, ffi::lean_box(1));
    ffi::lean_ctor_set(state, 1, msg_log);
    // 2: scopes = [ base Scope ]. Use the toolchain's own
    // `Inhabited Scope.default` (`{ header := "" }`) instead of
    // hand-building the struct: the `Scope` layout changed across
    // versions (4.31+ adds `varUIds`/`includedVars`/`omittedVars`/
    // Bool flags; 4.28+ `opts` is the `Options` struct, not `KVMap`),
    // and this is exactly the object the frontend's
    // `elabCommandTopLevel` uses as the `List.head` default for
    // `state.scopes`, so the layout is always the one the linked
    // runtime expects.
    let scope = ffi::meta::get_instInhabitedScope();
    if scope.is_null() {
        return Err(LeanError::other("instInhabitedScope default unavailable"));
    }
    let scopes = ffi::lean_alloc_ctor(1, 2, 0); // List.cons
    ffi::lean_ctor_set(scopes, 0, scope);
    ffi::lean_ctor_set(scopes, 1, ffi::lean_box(0)); // List.nil
    ffi::lean_ctor_set(state, 2, scopes);
    // 3: usedQuotCtxts = ∅ — `NameSet` is `Std.TreeSet Name`; the empty
    // tree is the BSS static `l_Lean_NameSet_empty` (NOT
    // NameHashSet.empty, which is a HashSet). rc 0 so dec is a no-op.
    let hs = ffi::meta::get_NameSetEmpty();
    if hs.is_null() {
        return Err(LeanError::other("NameSet.empty unavailable"));
    }
    ffi::lean_ctor_set(state, 3, hs);
    // 4: nextMacroScope = firstFrontendMacroScope + 1 = 2
    ffi::lean_ctor_set(state, 4, ffi::lean_box(2));
    // 5: maxRecDepth = 1000
    ffi::lean_ctor_set(state, 5, ffi::lean_box(1000));
    // 6: ngen (BSS static NameGenerator)
    let ngen = ffi::meta::get_instInhabitedNameGenerator();
    if ngen.is_null() {
        return Err(LeanError::other("instInhabitedNameGenerator unavailable"));
    }
    ffi::lean_ctor_set(state, 6, ngen);
    // 7: auxDeclNGen (BSS static DeclNameGenerator)
    let aux = ffi::meta::get_instInhabitedDeclNameGenerator();
    if aux.is_null() {
        return Err(LeanError::other(
            "instInhabitedDeclNameGenerator unavailable",
        ));
    }
    ffi::lean_ctor_set(state, 7, aux);
    // 8: infoState = InfoState { enabled := true, assignment := ∅,
    //    lazyAssignment := ∅, trees := ∅ } — 4 fields: 3 objects + 1 Bool
    //    scalar (enabled).
    let phm = ffi::meta::get_PersistentHashMapEmpty();
    if phm.is_null() {
        return Err(LeanError::other("PersistentHashMap.empty unavailable"));
    }
    let info_state = ffi::lean_alloc_ctor(0, 3, 1);
    ffi::lean_ctor_set(info_state, 0, phm);
    ffi::lean_ctor_set(info_state, 1, phm);
    ffi::lean_ctor_set(info_state, 2, pa);
    ffi::inline::lean_ctor_set_uint8(info_state, 3 * 8, 1); // enabled := true
    ffi::lean_ctor_set(state, 8, info_state);
    // 9: traceState = TraceState { tid := 0, traces := ∅ } — 1 object +
    //    1 UInt64 scalar (tid, 8 bytes). NB: scalar_sz is BYTES: allocating
    //    1 and writing 8 overwrites the neighboring heap block.
    let trace_state = ffi::lean_alloc_ctor(0, 1, 8);
    ffi::lean_ctor_set(trace_state, 0, pa);
    ffi::lean_ctor_set_uint64(trace_state, 8, 0);
    ffi::lean_ctor_set(state, 9, trace_state);
    // 10: snapshotTasks = #[] (empty Array)
    let empty_arr = ffi::array::lean_mk_empty_array();
    ffi::lean_ctor_set(state, 10, empty_arr);
    // 11: prevLinterStates (Option ...) = none (Lean 4.34+ only)
    #[cfg(lean_4_34)]
    ffi::lean_ctor_set(state, 11, ffi::lean_box(0));
    Ok(state)
}
