# Architecture

Leo3 is split into a small number of layers with intentionally narrow
responsibilities:

- `leo3-ffi`: raw C ABI bindings and inline runtime helpers
- `leo3-build-config`: Lean discovery and build-time config propagation
- `leo3-binding-ir`: shared semantic IR/analyzer crate for macro producers and downstream tooling
- `leo3-macros` + `leo3-macros-backend`: proc-macro entry points and expansion logic
- `leo3`: the safe user-facing runtime/token/conversion surface

## Runtime Model

Leo3 keeps Lean interaction in three lanes:

- one long-lived runtime worker thread performs one-time runtime bootstrap and
  serialized initialization-sensitive work
- caller threads use `with_lean()` or `ensure_lean_thread()` to attach safely
- Lean task APIs (`task`, `promise`, `tokio_bridge`) run on top of Lean's own
  task manager after the worker has initialized the runtime

This split exists because Lean's runtime and allocator assumptions are easier to
respect when bootstrap happens on a stable thread and callers only enter through
explicit attachment points.

## Ownership Model

The core pointer shapes are:

- `Lean<'l>`: proves the runtime is initialized for the current thread
- `LeanBound<'l, T>`: owned Lean object tied to that token
- `LeanBorrowed<'a, 'l, T>`: zero-cost borrowed view into an existing object
- `LeanRef<T>` / `LeanUnbound<T>`: ref-counted handles that can outlive a
  single token scope

Conversions are intentionally explicit. Leo3 keeps the built-in `IntoLean` /
`FromLean` matrix small, and uses wrapper-level APIs rather than broad implicit
trait magic when ownership rules would otherwise get fuzzy.

## Macro Boundary

The macros generate Rust shims, not a hidden second runtime:

- `leo3-binding-ir` owns the shared binding semantics model and AST analysis
- `#[leanfn]` builds FFI wrappers and structured function metadata from that model
- `#[leanclass]` builds external-object shims plus declaration and method metadata from that model
- `#[leanmodule]` builds the module init entry point and module export metadata from that model

The macros rely on public `leo3` APIs instead of reaching into crate-private
internals wherever practical. That keeps the runtime contract visible and makes
the generated behavior easier to audit in tests.

## Module Model

Leo3's current module story has three parts:

- initialization: `#[leanmodule]` generates `initialize_*`
- export discovery: inline `#[leanfn]` items become the module's implicit export
  set, exposed through `__leo3_module_metadata()` with the same per-binding
  schema used by standalone `#[leanfn]` accessors; `exports = [...]` restricts
  the set to an explicit allow-list
- nested submodules: inner `mod` blocks containing `#[leanfn]` items are
  discovered recursively and exposed through
  `__leo3_module_metadata().submodules` with dot-separated paths
- host loading: `LeanModule::load(...)` attaches the caller thread, temporarily
  opens Lean's importing window, calls the generated init symbol, and then
  resolves exported `#[leanfn]` C ABI wrappers through arity-checked
  `LeanFunction::callN(...)` helpers

The binding metadata schema is at version 2 (v1 → v2 added `submodules` to
`LeanModuleMetadata` and the new `LeanSubmoduleMetadata` type). Dotted module
names (e.g. `Foo.Bar.baz`) are supported for nested Lean module paths.

## Experimental Areas

The main intentionally narrow area is `experimental-containers`.

Those wrappers exist to reserve API shape and feature gating, not to claim that
Leo3 already has a stable container-semantic story. Any change in their actual
runtime behavior should be treated as stabilization work, not routine cleanup.
