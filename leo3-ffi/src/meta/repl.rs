//! FFI bindings for Lean's repl-oriented elaborator entry points.
//!
//! These bindings give access to the real Lean elaborator from Rust:
//! tactic-string parsing, tactic execution on a goal, module imports, and
//! goal pretty-printing. All symbols live in `libleanshared.so` as compiled
//! Lean functions (not part of the public C API).
//!
//! Verified against Lean 4.25.2.

use super::*;

// ============================================================================
// Tactic parsing (pure function)
// ============================================================================

extern "C" {
    /// `Lean.Parser.runParserCategory : Environment → Name → String → String → Except String Syntax`
    ///
    /// Pure curried function (arity 4): parse `input` in parser category
    /// `catName` (e.g. `tactic`) against `env`. Returns `Except.ok Syntax`
    /// or `Except.error String`.
    #[link_name = "l_Lean_Parser_runParserCategory"]
    pub fn lean_parser_run_parser_category(
        env: *mut *mut lean_object,
        arg: *mut lean_object,
    ) -> *mut lean_object;
}

// ============================================================================
// Static runtime objects (BSS globals; Windows-safe via lookup)
// ============================================================================

extern "C" {
    /// `Lean.initSearchPath : FilePath → IO Unit` (curried, arity 2 with
    /// the world token). Sets the global search path used by `importModules`
    /// to `sysroot/lib/lean` plus `LEAN_PATH`.
    #[link_name = "l_Lean_initSearchPath"]
    pub fn lean_init_search_path(
        env: *mut *mut lean_object,
        arg: *mut lean_object,
    ) -> *mut lean_object;
}

extern "C" {
    /// `Lean.Options.empty` — the empty options map.
    #[link_name = "l_Lean_Options_empty"]
    pub static mut lean_options_empty: *mut lean_object;

    /// `Lean.Elab.Term.instInhabitedState` — the `Inhabited Term.State`
    /// instance record; `default` is field 0.
    #[link_name = "l_Lean_Elab_Term_instInhabitedState"]
    pub static mut lean_elab_term_inst_inhabited_state: *mut lean_object;
}

bss_accessor!(/// The empty `Options` map (Windows-safe).
    pub fn options_empty() -> lean_options_empty);

bss_accessor!(/// The `Inhabited Term.State` record (Windows-safe).
    pub fn term_inst_inhabited_state() -> lean_elab_term_inst_inhabited_state);

/// The empty `PersistentArray` (Windows-safe; declared in `super`).
#[inline]
pub unsafe fn persistent_array_empty() -> *mut lean_object {
    #[cfg(not(target_os = "windows"))]
    {
        super::l_Lean_PersistentArray_empty()
    }
    #[cfg(target_os = "windows")]
    {
        super::win_bss::lookup_bss_global("l_Lean_PersistentArray_empty")
    }
}

// ============================================================================
// Module import (fully-uncurried)
// ============================================================================

extern "C" {
    /// `Lean.importModules` — direct mixed-ABI C function (verified against
    /// the 4.25.2 stage0 output):
    /// `(imports : List Name, opts : Options, trustLevel : UInt32,
    ///  plugins : PersistentArray, leakEnv : Bool, loadExts : Bool,
    ///  level : Name, arts : PersistentArray, world) → EIO Environment`.
    #[link_name = "l_Lean_importModules"]
    pub fn lean_import_modules_full(
        imports: lean_obj_arg,
        opts: lean_obj_arg,
        trust_level: u32,
        plugins: lean_obj_arg,
        leak_env: u8,
        load_exts: u8,
        level: u32,
        arts: lean_obj_arg,
        world: lean_obj_arg,
    ) -> lean_obj_res;
}


// ============================================================================
// Tactic execution (fully-uncurried monad form)
// ============================================================================

extern "C" {
    /// `Lean.Elab.runTactic` — the fully-uncurried monad form (9 args):
    /// `(mvarId, stx, termCtx, termState, metaCtx, metaStateRef, coreCtx,
    ///  coreStateRef, world) → EStateM.Result`. Verified against the v4.25.2
    /// stage0 C output.
    #[link_name = "l_Lean_Elab_runTactic"]
    pub fn lean_elab_run_tactic_full(
        mvar_id: lean_obj_arg,
        stx: lean_obj_arg,
        term_ctx: lean_obj_arg,
        term_state: lean_obj_arg,
        meta_ctx: lean_obj_arg,
        meta_state_ref: lean_obj_arg,
        core_ctx: lean_obj_arg,
        core_state_ref: lean_obj_arg,
        world: lean_obj_arg,
    ) -> lean_obj_res;
}

// ============================================================================
// Command elaboration (5-arg direct call)
// ============================================================================

extern "C" {
    /// `Lean.Elab.Command.elabCommandTopLevel` — direct mixed-ABI call.
    /// Arity 4: `(stx, ctx, ref, world)`.
    #[link_name = "l_Lean_Elab_Command_elabCommandTopLevel"]
    pub fn lean_elab_command_top_level_4(
        stx: lean_obj_arg,
        cmd_ctx: lean_obj_arg,
        cmd_state_ref: lean_obj_arg,
        world: lean_obj_arg,
    ) -> lean_obj_res;
}

// ============================================================================
// Goal pretty-printing (fully-uncurried monad form)
// ============================================================================

extern "C" {
    /// `Lean.Meta.ppGoal` — the fully-uncurried monad form (6 args):
    /// `(mvarId, metaCtx, metaStateRef, coreCtx, coreStateRef, world) →
    /// EStateM.Result Format`. Runs the real delaborator + pretty printer,
    /// producing a VSCode-style goal view (hypotheses + `⊢` + type).
    #[link_name = "l_Lean_Meta_ppGoal"]
    pub fn lean_meta_pp_goal(
        mvar_id: lean_obj_arg,
        meta_ctx: lean_obj_arg,
        meta_state_ref: lean_obj_arg,
        core_ctx: lean_obj_arg,
        core_state_ref: lean_obj_arg,
        world: lean_obj_arg,
    ) -> lean_obj_res;
}

#[cfg(not(lean_4_31))]
extern "C" {
    /// `Format.pretty : Format → Nat → Nat → Nat → String` — render a
    /// `Format` to a string with the given width / indent / starting
    /// column.
    ///
    /// Version note: in Lean ≤ 4.30 the function lives in the root
    /// `Format` namespace and is exported under the C name
    /// `lean_format_pretty` (`@[export]`). From 4.31 it moved to
    /// `Std.Format` without the export attribute, so its mangled name is
    /// `l_Std_Format_pretty` (verified against v4.20.0 / v4.25.2 for the
    /// old name and v4.31.0 / v4.32.2 / v4.33.0 for the new). Same
    /// 4-arg signature in both.
    #[link_name = "lean_format_pretty"]
    pub fn lean_format_pretty(
        fmt: lean_obj_arg,
        width: lean_obj_arg,
        indent: lean_obj_arg,
        column: lean_obj_arg,
    ) -> lean_obj_res;
}

#[cfg(lean_4_31)]
extern "C" {
    /// `Std.Format.pretty : Format → Nat → Nat → Nat → String` (Lean
    /// ≥ 4.31; same 4-arg signature, new mangled name).
    #[link_name = "l_Std_Format_pretty"]
    pub fn lean_format_pretty(
        fmt: lean_obj_arg,
        width: lean_obj_arg,
        indent: lean_obj_arg,
        column: lean_obj_arg,
    ) -> lean_obj_res;
}

extern "C" {
    /// `Lean.Meta.ppExpr` — the fully-uncurried monad form (6 args):
    /// `(e, metaCtx, metaStateRef, coreCtx, coreStateRef, world) →
    /// EStateM.Result Format`. Pretty-prints an expression with Lean's real
    /// delaborator + pretty printer, using the *current* local context.
    #[link_name = "l_Lean_Meta_ppExpr"]
    pub fn lean_meta_pp_expr(
        e: lean_obj_arg,
        meta_ctx: lean_obj_arg,
        meta_state_ref: lean_obj_arg,
        core_ctx: lean_obj_arg,
        core_state_ref: lean_obj_arg,
        world: lean_obj_arg,
    ) -> lean_obj_res;
}
