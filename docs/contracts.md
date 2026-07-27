# Leo3 Contracts

## Status

Current implementation record as of 2026-05-01.

This document replaces the older phase-by-phase maturity notes. It keeps the
current public/runtime/macro contract in one place, while
`docs/remaining-work-checklist.md` tracks the still-open gaps.

## Related Documents

- `README.md`
- `TESTING.md`
- `docs/architecture.md`
- `docs/contributing.md`
- `docs/pyo3-alignment.md`
- `docs/remaining-work-checklist.md`

## Stable Surface

### Public API honesty

Leo3's default public surface does not expose APIs whose behavior is knowingly
placeholder or semantically misleading.

Container wrappers (`LeanHashMap`, `LeanHashSet`, `LeanRBMap`) are now part of
the stable default surface on Lean >= 4.22. They use real Lean runtime
representations for an explicit, narrow key matrix. The `lean_4_22` cfg gate
remains to reflect the ABI requirement.

Surface contract tests:

- `leo3/tests/test_features.rs`

### Runtime and threading

Leo3 uses one coherent runtime model:

- `with_lean()` is the canonical safe caller entry point
- `prepare_freethreaded_lean()` eagerly starts the shared runtime worker, but
  does not attach the caller thread by itself
- `sync::ensure_lean_thread()` is the low-level attach-current-thread primitive
- module loading and generated module initialization align with the same runtime
  path

Current contract:

- the process-wide runtime worker is created lazily on first safe entry or
  eagerly by `prepare_freethreaded_lean()`
- a caller thread becomes Leo3-ready by calling `with_lean()` or
  `sync::ensure_lean_thread()`
- `module::LeanModule::load(...)` is responsible for making the host caller
  thread ready before invoking module init symbols
- generated `#[leanmodule]` init symbols follow Lean's plugin contract and do
  not start a separate Leo3 runtime from inside a downstream `cdylib`

### Error boundaries

Proc-macro-generated FFI wrappers share one explicit boundary policy:

- shared helpers live in `leo3::__private`
- `#[leanclass]` wrappers use `Result`-based try paths plus one outer panic
  boundary
- `#[lean_instance]` no longer uses silent empty-string fallbacks
- scalar-returning wrappers use an explicit abort-on-boundary-failure policy

This is the current contract for the macro families already in scope. It is not
yet a promise that every future wrapper shape has already been designed.

## Macro and Conversion Contract

### `#[leanmodule]`

`#[leanmodule]` supports structured forms:

- `#[leanmodule]`
- `#[leanmodule(MyModule)]`
- `#[leanmodule(name = "MyModule")]`
- `#[leanmodule(name = "Foo.Bar.baz")]` (dotted nested module path)
- `#[leanmodule(crate = my::leo3)]`
- `#[leanmodule(name = "MyModule", crate = my::leo3)]`
- `#[leanmodule(exports = ["fn_a", "fn_b"])]` (explicit export set)

Current module contract:

- generated module init code can target a re-exported `leo3` path
- inline `#[leanfn]` items inside the annotated module are treated as the
  module's implicit export set
- `exports = [...]` restricts the implicit export set to the listed names
  (matched by Lean-visible name or Rust name); unlisted `#[leanfn]` items
  remain callable from Rust but are excluded from module metadata
- inner `mod` blocks containing `#[leanfn]` items are discovered as nested
  submodule registrations and exposed through
  `__leo3_module_metadata().submodules` with dot-separated paths (e.g.
  `inner` or `inner.deep`)
- dotted module names (e.g. `Foo.Bar.baz`) are supported; the generated init
  function replaces dots with underscores (`initialize_Foo_Bar_baz`)
- the macro generates `__leo3_module_metadata()` so downstream Rust code can
  inspect the chosen Lean-visible export names and structured binding metadata

### Structured binding metadata

Leo3 macro producers now share one metadata schema:

- semantic analysis lives in the workspace crate `leo3-binding-ir`
- `#[leanfn]` generates `__leo3_metadata_*()` with structured parameter,
  receiver, return-type, FFI-symbol, and semantics metadata
- `#[leanmodule]` exports that same function metadata through
  `__leo3_module_metadata().exports`
- `#[leanclass]` keeps the declaration-string constants and also generates
  `__leo3_class_metadata_*()` with structured method semantics

Current schema rules:

- every generated metadata root carries `LEO3_BINDING_SCHEMA_VERSION`
- type metadata is honest about precision: `lean` is present when the producer
  can state the Lean-visible type exactly, and absent instead of guessing
- no consumer is expected to reconstruct `&mut self` / `Prod Self R` semantics
  from strings; that behavior is explicit in metadata
- class method metadata records the accessor `kind` (`Method`, `Getter`, or
  `Setter`) so consumers can recognize property-style accessors without
  re-parsing source attributes

Shared-library loading:

- downstream `cdylib` crates that want Lean runtime search paths in the final
  shared object should run `leo3-build-config::use_leo3_cfgs()` in their own
  `build.rs`
- `LeanModule::load(...)` temporarily enables Lean's importing mode while
  loading and initializing a module so option / environment-extension
  registration follows Lean's plugin-loading rules
- `LeanFunction::callN(...)` calls exported `#[leanfn]` C ABI wrappers directly
  by arity
- `leo3/tests/test_leanmodule_loading.rs` builds, loads, initializes, resolves,
  and calls a real downstream `cdylib` fixture through the public API

### `#[leanclass]` receiver matrix

`#[leanclass]` method receivers map to Lean-visible behavior as follows:

| Rust receiver | Lean-visible type rule | Runtime behavior |
| --- | --- | --- |
| no receiver | `A -> ... -> R` | static call, no external object parameter |
| `&self` | `Self -> A -> ... -> R` | shared borrow of the wrapped Rust value |
| `&mut self`, `-> ()` | `Self -> A -> ... -> Self` | copy-on-write mutation; updated object is returned |
| `&mut self`, `-> R` | `Self -> A -> ... -> Prod Self R` | copy-on-write mutation; updated object and return value are both preserved |
| `self` | `Self -> A -> ... -> R` | exclusive receiver moves out; shared receiver clones first |

Formal rules:

- any class exposing `&mut self` or `self` methods must implement `Clone`
- methods returning `Self` are supported through the normal `IntoLean`
  conversion path for external objects
- generated Lean declarations describe runtime behavior literally; the
  mutation-preserving `Prod Self R` form is part of the contract

### `#[leanclass]` property accessors

Methods inside a `#[leanclass]` impl block may be annotated with `#[getter]` or
`#[setter]` to mark them as property accessors. This is a metadata-level
distinction: the generated FFI wrappers and Lean declarations reuse the existing
`&self` / `&mut self` receiver machinery, and the accessor kind is recorded in
the structured binding metadata (`LeanBindingKind`) so downstream tooling can
recognize property-style access.

| Annotation | Required signature | Lean-visible type | Recorded kind |
| --- | --- | --- | --- |
| `#[getter]` | `fn name(&self) -> T` | `Self -> T` | `Getter` |
| `#[setter]` | `fn name(&mut self, value: T)` | `Self -> T -> Self` | `Setter` |

Validation rules (enforced at compile time):

- `#[getter]` and `#[setter]` are mutually exclusive on a single method
- `#[getter]` requires an `&self` receiver, no additional parameters, and a
  non-unit return type
- `#[setter]` requires an `&mut self` receiver, exactly one parameter, and a
  `()` return type (the copy-on-write updated object is returned to Lean)
- regular methods are recorded with kind `Method`

The compile-fail matrix for invalid accessor definitions lives in:

- `leo3/tests/ui/leanclass_getter_wrong_receiver.rs`
- `leo3/tests/ui/leanclass_getter_extra_params.rs`
- `leo3/tests/ui/leanclass_setter_wrong_receiver.rs`
- `leo3/tests/ui/leanclass_setter_wrong_params.rs`
- `leo3/tests/ui/leanclass_getter_and_setter.rs`

### Built-in conversion matrix

Leo3's built-in conversion rules are:

| Rust shape | Lean shape | `IntoLean` | `FromLean` | Cost / ownership |
| --- | --- | --- | --- | --- |
| scalar primitives plus `()` | scalar wrappers / `Unit` | yes | yes | value conversion; Rust -> Lean allocates a Lean object |
| `String` | `String` | yes | yes | allocative copy |
| `&str` | `String` | yes | no | allocative copy into Lean only |
| `Vec<T>` | `Array` | yes | yes | allocates the container and converts elements recursively |
| `Option<T>` | `Option` | yes | yes | recursive container conversion |
| `Result<T, E>` | `Except E T` | yes | yes | recursive container conversion |
| `(A, B)` | `Prod A B` | yes | yes | recursive pair conversion |
| `Vec<u8>` / `&[u8]` helper path | `ByteArray` | helper only | helper only | bulk memcpy path, exposed through helper fns and conversion macros |
| `T: ExternalClass` | Lean external object | yes | yes, if `T: Clone` | stores owned Rust value in Lean; extraction clones the Rust value |

Formal rules:

- `FromLean` for external objects is intentionally clone-based
- borrow-first access is a separate API surface:
  `LeanExternal<T>::borrow()`, `try_get_mut()`, and `try_take_inner()` for the
  safe/high-level path, plus `get_ref()`, `get_mut()`, and `take_inner()` for
  lower-level control
- a trait-level borrow extraction (`FromLeanBorrowed`) was evaluated and
  rejected: the wrapper-layer API already provides zero-copy access, and a
  generic trait would only apply to external objects (not to the rest of the
  conversion matrix). See `docs/external-object-borrow-extraction.md` for the
  full design record. This remains a long-term design constraint — the clone
  contract is the stable generic boundary
- `Vec<u8>` uses helper functions instead of trait specialization on stable
  Rust
- proc-macro-generated wrappers inherit this contract
- `#[leanfn]` wrappers add borrow-friendly generated storage for common
  function-boundary aliases: `&str`, `&String`, `&[T]`, `&[T; N]`, `&Vec<T>`,
  `&[u8]`, and `&Vec<u8>` parameters, including the supported direct
  `Option<T>`, `Result<T, E>`, tuple, and `Option<Result<T, E>>` wrapper shapes
  covered by the macro tests; these are wrapper conveniences, not blanket
  `FromLean` impls for borrowed Rust references
- `#[derive(IntoLean, FromLean)]` or manual impls can extend the Rust-side
  conversion set, but they do not automatically widen the `#[leanclass]`
  declaration grammar

### `#[leanfn]` monomorphization subset

Generic `#[leanfn]` functions are supported through an explicit monomorphization
subset using `concrete(Ty, name = "...")` annotations:

```rust
#[leanfn(concrete(u64, name = "add_u64"), concrete(i64, name = "add_i64"))]
fn add<T: Add<Output = T>>(a: T, b: T) -> T {
    a + b
}
```

Each `concrete` annotation generates a separate, fully monomorphized C ABI
wrapper, metadata entry, and Lean-visible declaration. The contract:

- the user must enumerate every concrete instantiation explicitly
- each instance must have a unique `name = "..."`
- the number of concrete types must match the number of generic type parameters
- the original generic function is preserved and called via turbofish
  (`add::<u64>(...)`) inside each generated wrapper
- lifetime and const generic parameters are rejected
- `#[leanclass]` generics remain unsupported (see compile-fail tests)

Compile-fail matrix for the monomorphization subset:

- `leo3/tests/ui/leanfn_generic_without_concrete.rs` — generic fn without `concrete`
- `leo3/tests/ui/leanfn_concrete_wrong_arity.rs` — wrong number of concrete types
- `leo3/tests/ui/leanfn_concrete_missing_name.rs` — missing `name = "..."`

### `#[leanclass]` declaration grammar

Generated Lean declarations support these Rust type shapes:

- scalar primitives, `bool`, `char`, `String`, and `()`
- `Self`, the current struct name, and other plain non-generic path types
- `Vec<T>`, `Option<T>`, and `Result<T, E>`
- pairs `(A, B)`

Generated Lean declarations intentionally reject these Rust type shapes:

- references such as `&str` or `&T`
- tuples with arity other than `0` or `2`
- generic path types other than `Vec<T>`, `Option<T>`, and `Result<T, E>`
- generic `#[leanclass]` structs
- generic `#[leanclass]` impl blocks
- generic methods inside `#[leanclass]` impls
- non-identifier parameter patterns

The compile-fail matrix for those unsupported shapes lives in:

- `leo3/tests/test_compile_error.rs`
- `leo3/tests/ui/`
- `TESTING.md`

## Container Surface

Container wrappers are now stable on the default feature set.

Current state:

- `LeanHashMap`, `LeanHashSet`, and `LeanRBMap` all use real Lean runtime
  representations and operations
- the supported key matrix is intentionally narrow and explicit:
  `LeanNat`, `LeanInt`, `LeanString`, `LeanInt8`, `LeanInt16`, `LeanInt32`,
  `LeanInt64`, `LeanUInt8`, `LeanUInt16`, `LeanUInt32`, and `LeanUInt64`
- fixed-width signed integers (`LeanInt8`–`LeanInt64`) and unsigned integers
  (`LeanUInt8`–`LeanUInt64`) use Lean's unboxed scalar ABI representation,
  aligned with Lean's container typeclass instances
- the surface requires Lean >= 4.22 (the `lean_4_22` cfg)
- runtime tests cover duplicate inserts, replacement semantics, string-key
  support, fixed-width signed and unsigned integer key support, and
  cross-family parity for the supported families

The contract is "real and stable for a narrow matrix". Widening the key matrix
is future expansion, tracked in `docs/remaining-work-checklist.md`.
