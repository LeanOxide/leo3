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
        #[link_name = "l_Lean_Elab_Tactic_tacticElabAttribute"]
        static tactic_attr: *mut ffi::lean_object;
    }
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
    register_builtin_tactic(lean, "Lean.Parser.Tactic.intro", "Lean.Elab.Tactic.evalIntro", eval_intro as *mut std::ffi::c_void, 10)?;
    register_builtin_tactic(lean, "Lean.Parser.Tactic.skip", "Lean.Elab.Tactic.evalSkip", eval_skip as *mut std::ffi::c_void, 10)?;
    register_builtin_tactic(lean, "Lean.Parser.Tactic.simp", "Lean.Elab.Tactic.evalSimp", eval_simp as *mut std::ffi::c_void, 11)?;
    register_builtin_tactic(lean, "Lean.Parser.Tactic.exact", "Lean.Elab.Tactic.evalExact", eval_exact as *mut std::ffi::c_void, 11)?;
    register_builtin_tactic(lean, "Lean.Parser.Tactic.induction", "Lean.Elab.Tactic.evalInduction", eval_induction as *mut std::ffi::c_void, 10)?;
    Ok(())
}

/// Mark the end of the initialization phase (mirrors the lean CLI calling
/// `lean_io_mark_end_initialization` after processing the input file).
/// Safe to call repeatedly.
pub fn finalize_initialization() {
    static FINALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !FINALIZED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        unsafe {
            ffi::lean_io_mark_end_initialization();
        }
    }
}
use crate::instance::LeanBound;
use crate::marker::Lean;
use crate::meta::environment::LeanEnvironment;
use crate::meta::expr::LeanExpr;
use crate::meta::metam::MetaMContext;
use crate::meta::name::LeanName;
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
            5 => ffi::closure::lean_apply_5(
                fn_obj,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
            ),
            6 => ffi::closure::lean_apply_6(
                fn_obj,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
                args[5],
            ),
            7 => ffi::closure::lean_apply_7(
                fn_obj,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
                args[5],
                args[6],
            ),
            8 => ffi::closure::lean_apply_8(
                fn_obj,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
                args[5],
                args[6],
                args[7],
            ),
            9 => {
                let partial = ffi::closure::lean_apply_8(
                    fn_obj,
                    args[0],
                    args[1],
                    args[2],
                    args[3],
                    args[4],
                    args[5],
                    args[6],
                    args[7],
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
            5 => ffi::closure::lean_apply_5(
                closure,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
            ),
            6 => ffi::closure::lean_apply_6(
                closure,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
                args[5],
            ),
            7 => ffi::closure::lean_apply_7(
                closure,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
                args[5],
                args[6],
            ),
            8 => ffi::closure::lean_apply_8(
                closure,
                args[0],
                args[1],
                args[2],
                args[3],
                args[4],
                args[5],
                args[6],
                args[7],
            ),
            9 => {
                let partial = ffi::closure::lean_apply_8(
                    closure,
                    args[0],
                    args[1],
                    args[2],
                    args[3],
                    args[4],
                    args[5],
                    args[6],
                    args[7],
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

/// A 1-argument boolean function reused as the (never-invoked) default for
/// `Term.Context.autoBoundImplicitForbidden`; with `autoBoundImplicit =
/// false` the field is dead, so any safe 1-arg function works.
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
    let cmp = ffi::inline::lean_alloc_closure(
        lean_name_quick_lt as *mut std::ffi::c_void,
        2u32,
        0,
    );
    apply_curried(lean_std_dtreemap_empty as *mut std::ffi::c_void, 1, &[cmp])
}

/// Construct a default `Lean.Elab.Term.Context` (all fields at their Lean
/// defaults), matching Lean 4.25.2's field layout.
///
/// # Safety
///
/// Layout must match the Lean version the runtime was built with.
pub unsafe fn default_term_context<'l>(
    lean: Lean<'l>,
) -> LeanResult<LeanBound<'l, crate::instance::LeanAny>> {
    crate::runtime::ensure_meta_initialized();
    unsafe {
        // Term.Context (Lean 4.25.2): 7 object fields followed by
        // 11 scalar (Bool) bytes, per Lean's object-first ctor layout.
        // Object fields (declaration order): declName?, macroStack,
        // autoBoundImplicits, autoBoundImplicitForbidden, sectionVars,
        // sectionFVars, tacSnap?.
        // Scalar bytes (declaration order): mayPostpone, errToSorry,
        // autoBoundImplicit, implicitLambda, heedElabAsElim,
        // isNoncomputableSection, ignoreTCFailures, inPattern,
        // saveRecAppSyntax, holesAsSyntheticOpaque, checkDeprecated.
        const NUM_OBJ_FIELDS: u32 = 7;
        let ctx = ffi::lean_alloc_ctor(0, NUM_OBJ_FIELDS, 11);
        let set_obj = |i: u32, v: *mut ffi::lean_object| {
            ffi::inline::lean_ctor_set(ctx, i, v)
        };
        // Scalar slots live *after* the object area; the byte offset counts
        // from the object area start: num_objs * ptr_size + scalar_index.
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
                std::ffi::CStr::from_ptr(c_str).to_string_lossy().into_owned()
            };
            ffi::lean_dec(result);
            Err(LeanError::other(format!("term parse error: {message}").as_str()))
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
                std::ffi::CStr::from_ptr(c_str).to_string_lossy().into_owned()
            };
            ffi::lean_dec(result);
            Err(LeanError::other(format!("tactic parse error: {message}").as_str()))
        }
    }
}

/// Set Lean's global search path to `sysroot/lib/lean` plus `LEAN_PATH`.
///
/// In embedded scenarios the runtime's builtin-initialized search path is
/// empty (its default uses the *process executable* directory), so callers
/// must initialize it before importing modules. Idempotent: re-invoking
/// replaces the path with the same value.
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

/// Discover the Lean system root: `LEAN_SYSROOT` env, else `lean
/// --print-prefix` (one subprocess, cached).
fn discover_sysroot() -> LeanResult<String> {
    if let Ok(root) = std::env::var("LEAN_SYSROOT") {
        if !root.is_empty() {
            return Ok(root);
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

static SEARCH_PATH_INITIALIZED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Ensure Lean's search path is initialized (once per process), so module
/// imports resolve `.olean` files. Safe to call repeatedly.
pub fn ensure_search_path<'l>(lean: Lean<'l>) -> LeanResult<()> {
    if !SEARCH_PATH_INITIALIZED.load(std::sync::atomic::Ordering::Relaxed) {
        let sysroot = discover_sysroot()?;
        init_search_path(lean, &sysroot)?;
        SEARCH_PATH_INITIALIZED.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(())
}

/// Import compiled Lean modules (e.g. `Init`) into a fresh environment.
///
/// Calls `Lean.importModules` (IO). The search path is initialized from
/// `LEAN_SYSROOT` (or `lean --print-prefix`) plus `LEAN_PATH` on first use.
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
        // End the initialization phase BEFORE importing: environment
        // creation (`lean_mk_empty_environment`) requires
        // `IO.initializing == false`, and initializer execution during
        // import only depends on `enableInitializersExecution`.
        finalize_initialization();
        // Register core builtin tactics before creating the imported
        // environment: new environments snapshot the attribute table.
        ensure_core_builtin_tactics(lean)?;
        {
            extern "C" {
                #[link_name = "l___private_Lean_Environment_0__Lean_EnvExtension_envExtensionsRef"]
                static env_exts_ref: *mut ffi::lean_object;
                #[link_name = "l_Lean_persistentEnvExtensionsRef"]
                static pers_exts_ref: *mut ffi::lean_object;
            }
            let e = ffi::inline::lean_to_ref(env_exts_ref);
            let p = ffi::inline::lean_to_ref(pers_exts_ref);
            let _ = (e, p);
        }
        // Array Import: one Import = ctor(0, 1, 0) with field 0 = module Name.
        let imports = if names.is_empty() {
            ffi::array::lean_alloc_array(0, 0)
        } else {
            let arr = ffi::array::lean_alloc_array(names.len(), names.len());
            for (i, name) in names.iter().enumerate() {
                let nm = LeanName::from_str(lean, name)?;
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
        let world = ffi::io::lean_io_mk_world();

        let result = crate::runtime::with_worker(move || unsafe {
            // Must run on the worker thread: importModules allocates Lean
            // objects (olean reading, environment construction), and Lean
            // objects must not cross mimalloc thread-local heaps.
            // Direct mixed-ABI call (not curried): scalar params are raw
            // u32/u8 values.
            let result = ffi::meta::repl::lean_import_modules_full(
                imports,
                opts,
                trust_level,
                plugins,
                0, // leakEnv
                u8::from(load_exts), // loadExts
                2, // OLeanLevel.private
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
    let term_state = unsafe { default_term_state(metam.lean())? };
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
                    ffi::lean_st_mk_ref(metam_meta_state_ptr, ffi::lean_box(0))
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
            let (core_state_ref, world) = {
                (
                    ffi::lean_st_mk_ref(core_state_ptr, ffi::lean_box(0)),
                    ffi::lean_box(0),
                )
            };
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
                let res = ffi::lean_st_ref_get(r, world);
                let v = ffi::lean_ctor_get(res, 0) as *mut ffi::lean_object;
                ffi::lean_inc(v);
                ffi::lean_dec(res);
                v
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
            >((alpha, updated_meta, updated_core, term_state_ptr, meta_state_ref))
        })?;

    unsafe {
        // Core.State came back in the result; Meta.State lives in the kept
        // ref (threaded in place) — store that ref so the next step reuses it.
        metam.update_states(
            LeanBound::<crate::instance::LeanAny>::from_owned_ptr(
                metam.lean(),
                new_meta_state,
            ),
            LeanBound::<crate::instance::LeanAny>::from_owned_ptr(
                metam.lean(),
                new_core_state,
            ),
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

