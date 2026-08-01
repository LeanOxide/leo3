# Leo3: Rust Bindings for Lean4

![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/AndPuQing/leo3/ci.yml?style=flat-square&logo=github)
![Crates.io Version](https://img.shields.io/crates/v/leo3?style=flat-square&logo=rust)
![Crates.io Downloads (recent)](https://img.shields.io/crates/dr/leo3?style=flat-square)
[![dependency status](https://deps.rs/repo/github/AndPuQing/leo3/status.svg?style=flat-square)](https://deps.rs/repo/github/AndPuQing/leo3)
![Crates.io License](https://img.shields.io/crates/l/leo3?style=flat-square) ![Crates.io Size](https://img.shields.io/crates/size/leo3?style=flat-square)

Leo3 provides safe, ergonomic Rust bindings to the [Lean4](https://github.com/leanprover/lean4) theorem prover, inspired by [PyO3](https://github.com/PyO3/pyo3)'s architecture.

## Quick Start

Requires Lean 4.25.2 (install via [elan](https://github.com/leanprover/elan)).

```toml
[dependencies]
leo3 = "0.3.0"
```

```rust,no_run
use leo3::prelude::*;

fn main() -> LeanResult<()> {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "Hello from Rust!")?;
        println!("{}", LeanString::cstr(&s)?);

        let n = LeanNat::from_usize(lean, 42)?;
        println!("{}", LeanNat::to_usize(&n)?);
        Ok(())
    })
}
```

`with_lean()` is the canonical safe entry point for caller code. It ensures the
shared runtime worker exists and attaches the current thread before handing out a
`Lean<'l>` token. `prepare_freethreaded_lean()` is still useful when you want to
pay initialization cost eagerly during startup.

## Cargo Features

Default features: **none**. The default build keeps the public surface intentionally
minimal: runtime initialization, smart pointers, core type wrappers/conversions,
closures, thunks, and synchronization helpers are always available.

| Feature | Enables |
|---------|---------|
| _default (none)_ | Core runtime/token APIs, conversions, closures, thunks, sync helpers, Lean type wrappers, container wrappers (`LeanHashMap`, `LeanHashSet`, `LeanRBMap`; requires Lean >= 4.22) |
| `macros` | `#[leanfn]`, `#[leanclass]`, `#[leanmodule]`, `#[derive(IntoLean, FromLean)]` |
| `meta` | `leo3::meta::*` metaprogramming APIs |
| `io` | `leo3::io::*` IO / filesystem / process / environment helpers |
| `task` | `leo3::task`, `leo3::task_combinators`, `leo3::promise` |
| `module-loading` | `leo3::module::*` dynamic shared-library loading |
| `tokio` | `leo3::tokio_bridge::*` (implies `task`) |
| `runtime-tests` | Runtime-dependent integration tests (for development/CI) |

Example dependency declarations:

```toml
# Minimal core surface (includes containers on Lean >= 4.22)
leo3 = "0.3.0"

# Opt into specific subsystems
leo3 = { version = "0.3.0", features = ["macros", "meta", "task"] }
```

## Lean Discovery

Leo3 build scripts resolve Lean in a single shared order:

1. `LEO3_NO_LEAN=1` disables Lean detection and linking entirely.
2. `LEO3_CONFIG_FILE=/path/to/leo3-build-config.txt` provides an explicit key/value config.
3. Host discovery tries `LEO3_CROSS_*`, then `LEAN_HOME` (with optional `LEAN_LIB_DIR` / `LEAN_INCLUDE_DIR`), then `lake`, then `elan`, then `PATH`.

`leo3-ffi` is the authoritative detector. It exports the resolved config through Cargo's `DEP_LEAN4_LEO3_CONFIG`, and `leo3` consumes that propagated config so both crates always agree on Lean version cfgs and linker settings.

Explicit overrides are authoritative: if `DEP_LEAN4_LEO3_CONFIG`, `LEO3_CONFIG_FILE`, `LEO3_CROSS_*`, `LEAN_HOME`, `LEAN_LIB_DIR`, or `LEAN_INCLUDE_DIR` is set but invalid, Leo3 reports that error instead of silently falling back to a lower-priority source.

## Capability Overview

### Core Type Conversions

Leo3's built-in `IntoLean` / `FromLean` support matrix is intentionally small and explicit:

| Rust shape | Lean shape | `IntoLean` | `FromLean` | Cost / ownership rule |
|------|------|---------|---------|---------|
| `u8`-`u64`, `usize`, `i8`-`i64`, `isize`, `f32`, `f64`, `bool`, `char`, `()` | scalar wrappers / `Unit` | yes | yes | value conversion; allocating the Lean object on Rust -> Lean |
| `String` | `String` | yes | yes | allocative copy |
| `&str` | `String` | yes | no | allocative copy into Lean only |
| `Vec<T>` | `Array` | yes | yes | allocates the container and converts elements one by one |
| `Option<T>` | `Option` | yes | yes | recursive container conversion |
| `Result<T, E>` | `Except E T` | yes | yes | recursive container conversion; Lean keeps the error type first |
| `(A, B)` | `Prod A B` | yes | yes | recursive pair conversion; only pairs are built in |
| `Vec<u8>` / `&[u8]` helpers | `ByteArray` | helper path | helper path | bulk memcpy fast path via `vec_u8_*`, `slice_u8_into_lean`, `to_lean!`, `from_lean!` |
| `T: ExternalClass` | Lean external object | yes | yes, if `T: Clone` | Rust -> Lean stores ownership in an external object; Lean -> Rust clones the inner value |

Rules behind that table:

- `FromLean` is not implemented for `&str`; round-trips use `String`.
- `Vec<u8>` does not use a specialized trait impl on stable Rust. The optimized `ByteArray` path is exposed through helper functions and the `to_lean!` / `from_lean!` macros.
- `FromLean` for `T: ExternalClass + Clone` is clone-based extraction, not borrowing. Borrow-first access goes through `LeanExternal<T>::borrow()`, `try_get_mut()`, and `try_take_inner()` (with `get_ref()` / `get_mut()` still available as lower-level APIs).
- User-defined types can extend the matrix with manual impls or `#[derive(IntoLean, FromLean)]`.

### Container Wrappers

`LeanHashMap`, `LeanHashSet`, and `LeanRBMap` are available on the default
feature set (requires Lean >= 4.22).

These wrappers use Lean's real runtime representation for an explicit, narrow
key matrix.

Current status:

- `LeanHashMap` uses Lean's real runtime representation for a narrow key
  matrix by pairing exported `Hashable` closures with real `BEq` closures
  derived from boxed `DecidableEq` functions.
- `LeanHashSet` uses the same real runtime path for the same narrow key
  matrix.
- `LeanRBMap` uses Lean's real runtime representation and reduced-arity
  container entry points for a narrow key matrix (`Nat`, `Int`, `String`,
  `Int8`–`Int64`, and `UInt8`–`UInt64`).
- runtime tests cover duplicate inserts, replacement semantics, string-key
  support, fixed-width signed and unsigned integer key support, and
  cross-family parity for the supported paths.

### Procedural Macros (`macros`)

Enable the `macros` feature on the `leo3` dependency, then prefer the root-path
form `#[leo3::leanfn]`, `#[leo3::leanclass]`, and `#[leo3::leanmodule]` so
examples work without extra macro imports.

`#[leanfn]` — Export Rust functions to Lean:

```rust,no_run
# #[cfg(feature = "macros")]
# mod leanfn_doctest {
use leo3::prelude::*;

#[leo3::leanfn]
fn add(a: u64, b: u64) -> u64 {
    a + b
}
# }
```

`#[leanfn]` wrappers accept a borrow-friendly subset beyond the base trait
matrix: parameters may use `&str`, `&String`, `&[T]`, `&[T; N]`, `&Vec<T>`,
and byte-oriented `&[u8]` / `&Vec<u8>` shapes. Those borrowed shapes are also
supported in the common wrapper containers `Option<T>`, `Result<T, E>`, tuples,
and `Option<Result<T, E>>` where the borrowed value is copied into wrapper-local
storage before the Rust function is called. Borrowed return values for the same
string/vector/slice family are converted back to owned Lean values.

`#[leanclass]` — Expose Rust structs as Lean external classes with auto-generated FFI wrappers and Lean source declarations:

```rust,no_run
# #[cfg(feature = "macros")]
# mod leanclass_doctest {
use leo3::prelude::*;

#[derive(Clone)]
#[leo3::leanclass]
struct Counter { value: i64 }

#[leo3::leanclass]
impl Counter {
    fn new() -> Self { Counter { value: 0 } }
    fn get(&self) -> i64 { self.value }
    fn increment(&mut self) { self.value += 1; }
}
# }
```

`#[leanclass]` receiver semantics are fixed:

| Rust receiver | Lean-visible signature rule | Runtime rule |
|------|------|------|
| no receiver | `A -> ... -> R` | parameters use `FromLean`, result uses `IntoLean` |
| `&self` | `Self -> A -> ... -> R` | shared borrow of the external object |
| `&mut self`, `-> ()` | `Self -> A -> ... -> Self` | copy-on-write mutation; returns the updated object |
| `&mut self`, `-> R` | `Self -> A -> ... -> Prod Self R` | copy-on-write mutation; preserves both updated object and value |
| `self` | `Self -> A -> ... -> R` | exclusive receivers move out; shared receivers clone first |

`&mut self` and `self` methods therefore require the class type to implement
`Clone`, because the shared-receiver fallback is clone-based.

Generated Lean declaration strings accept a narrower type grammar than "anything
that implements conversion traits". The supported declaration shapes are:

- scalars: integers, floats, `bool`, `char`, `String`, `()`
- `Self`, the current struct name, and other plain non-generic path types
- `Vec<T>`, `Option<T>`, `Result<T, E>`
- pairs `(A, B)`

The macro intentionally rejects these declaration shapes at compile time:

- references such as `&str` or `&T`
- tuples with arity other than `0` or `2`
- generic path types other than `Vec<T>`, `Option<T>`, and `Result<T, E>`
- generic `#[leanclass]` structs, generic `impl` blocks, generic methods, and non-identifier parameter patterns

Plain non-generic path types are emitted verbatim into the generated Lean type
string. That means the Rust side still needs the relevant conversion impls, and
the Lean side still needs a matching type name in scope.

`#[derive(IntoLean)]` / `#[derive(FromLean)]` — Automatic conversion derive macros.

`#[leanmodule]` now has an explicit module-registration story too: Leo3 treats
inline `#[leanfn]` items inside the annotated module as the module's implicit
export set, and generates `__leo3_module_metadata()` so downstream Rust code can
inspect the chosen Lean-visible names plus the shared structured binding schema
for each export. `#[leanfn]` emits the same schema through the generated
`__leo3_metadata_*()` accessors, and `#[leanclass]` now has matching
`__leo3_class_metadata_*()` accessors for class/method semantics.

For a single runnable example that combines module initialization, exported
functions, and an external class, run:

```bash
cargo run --example macro_pipeline --features macros
```

That example exercises a generated `initialize_*` module entry point, `#[leanfn]`
FFI wrappers, and the Lean declaration strings emitted for `#[leanclass]`.

### Meta-Programming (`meta`)

Full access to Lean's kernel and elaborator:

- `LeanEnvironment` — Create and manage declaration environments
- `LeanExpr` — Build expressions (binders, applications, literals, lambda, forall)
- `LeanDeclaration` — Define axioms, definitions, and theorems
- `MetaMContext` — Type inference, type checking, definitional equality, proof validation
- Tactic support

### IO, Tasks, and Dynamic Loading

- `io`: console, filesystem, environment variables, process, and time helpers
- `task`: `LeanTask` / `LeanPromise` plus combinators (`join`, `race`, `select`, `timeout`)
- `module-loading`: `LeanModule` for loading compiled Lean shared libraries
- `tokio`: async bridge for Lean tasks
- Core (always available): `LeanClosure` and `LeanThunk`

`LeanModule::load(...)` handles the host-side runtime attachment, opens Lean's
plugin importing window while loading and initializing the module, and exposes
arity-checked `get_function(...).callN(...)` helpers for exported `#[leanfn]`
symbols.

Downstream `cdylib` crates that expect to be loaded through
`leo3::module::LeanModule` should run `leo3-build-config` in their own
`build.rs`, so the final shared library carries Lean's runtime link search
path:

```toml
[build-dependencies]
leo3-build-config = "0.3.0"
```

```rust,ignore
fn main() {
    leo3_build_config::use_leo3_cfgs();
}
```

## Examples

| Example | Feature flags | Description |
|---------|--------------|-------------|
| `io_monad` | `io` | IO monad: console, file handles, sequencing |
| `proof_construction` | `meta` | Build and validate proof terms with MetaM |
| `macro_pipeline` | `macros` | `#[leanmodule]`, `#[leanfn]`, `#[leanclass]` end-to-end |
| `task_async` | `tokio` | Tasks, promises, combinators, and tokio bridge |
| `external_object` | _(none)_ | Wrap Rust structs as Lean external objects |
| `containers` | _(none)_ | HashMap, HashSet, RBMap wrappers |
| `module_loading` | `macros`, `module-loading` | Build and load a cdylib module at runtime |

Run any example with:

```bash
cargo run --example <name> --features "<flags>"
```

## Architecture

```text
leo3/
├── leo3/                   # Safe high-level abstractions
├── leo3-ffi/               # Raw FFI bindings to Lean4's C API
├── leo3-build-config/      # Build-time Lean4 detection and configuration
├── leo3-binding-ir/        # Shared semantic IR/analyzers for binding producers
├── leo3-macros/            # Procedural macro entry points
├── leo3-macros-backend/    # Procedural macro implementations
└── leo3-ffi-check/         # FFI layout validation (à la pyo3-ffi-check)
```

### Design

- `Lean<'l>` token proves runtime initialization at compile-time
- `LeanBound<'l, T>` (lifetime-bound), `LeanRef<T>` (unbound), `LeanUnbound<T>` (thread-safe) smart pointers with automatic reference counting
- `#[repr(transparent)]` zero-cost wrappers
- Copy-on-write semantics for `&mut self` FFI methods
- Worker thread architecture for all Lean runtime calls (avoids mimalloc heap issues)
- Version support: Lean 4.20.0–4.33+ with `#[cfg(lean_4_25)]` gates

### Comparison with PyO3

| PyO3 | Leo3 |
|------|------|
| `Python<'py>` | `Lean<'l>` |
| `Bound<'py, T>` | `LeanBound<'l, T>` |
| `Py<T>` | `LeanRef<T>` / `LeanUnbound<T>` |
| `FromPyObject` / `IntoPyObject` | `FromLean` / `IntoLean` |
| `#[pyfunction]` | `#[leanfn]` |
| `#[pymodule]` | `#[leanmodule]` |
| `#[pyclass]` | `#[leanclass]` |
| `#[derive(FromPyObject)]` | `#[derive(IntoLean, FromLean)]` |

## Development

New to Leo3? Start with the [getting-started tutorial](docs/getting-started.md).

See `TESTING.md` for the full tiered CI map, the
[current contract notes](docs/contracts.md), [architecture notes](docs/architecture.md),
the [PyO3 alignment notes](docs/pyo3-alignment.md), and the
[contributor guide](docs/contributing.md). Common local commands:

```bash
LEO3_NO_LEAN=1 cargo test --locked --workspace --exclude leo3 --lib
LEO3_NO_LEAN=1 cargo test --locked -p leo3 --no-default-features --test test_features
LEO3_NO_LEAN=1 cargo test --locked -p leo3 --features macros --test test_compile_error
cargo test --locked --all-features --workspace
```

## License

MIT OR Apache-2.0

## Acknowledgments

- [PyO3](https://github.com/PyO3/pyo3) — Architectural inspiration
- [Lean4](https://github.com/leanprover/lean4) — The theorem prover Leo3 binds to
