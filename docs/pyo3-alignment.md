# PyO3 Alignment Notes

This document records Leo3's PyO3-inspired alignment points after auditing the
local comparison checkout at `../pyo3`.

Audit source:

- `../pyo3` revision: `8a00673`
- PyO3 crate version in that checkout: `0.28.3`
- Primary guide files checked:
  - `../pyo3/guide/src/types.md`
  - `../pyo3/guide/src/conversions/tables.md`
  - `../pyo3/guide/src/conversions/traits.md`
  - `../pyo3/guide/src/function.md`
  - `../pyo3/guide/src/module.md`
  - `../pyo3/guide/src/class.md`
  - `../pyo3/guide/src/features.md`
- Primary crate files checked:
  - `../pyo3/Cargo.toml`
  - `../pyo3/src/marker.rs`
  - `../pyo3/src/instance.rs`
  - `../pyo3/src/conversion.rs`
  - `../pyo3/src/pyclass.rs`

## Prompt-To-Artifact Checklist

| PyO3 area | PyO3 artifact checked | Leo3 artifact | Status |
| --- | --- | --- | --- |
| Runtime attachment token | `Python<'py>` in `types.md` / `marker.rs` | `Lean<'l>`, `with_lean()`, `prepare_freethreaded_lean()` | Aligned in shape. Leo3 attaches the current thread to the Lean runtime worker before issuing the token. |
| Lifetime-bound object handle | `Bound<'py, T>` in `types.md` / `instance.rs` | `LeanBound<'l, T>` | Aligned in ownership role. Leo3 uses Lean reference counting rather than Python reference counting. |
| Detached object handle | `Py<T>` in `types.md` / `instance.rs` | `LeanRef<T>` / `LeanUnbound<T>` | Aligned in role. Leo3 keeps the detached forms split around its Lean ownership/threading model. |
| Fallible API result | `PyResult<T>` / `PyErr` | `LeanResult<T>` / `LeanError` | Aligned in shape. Error categories are Lean/Leo3-specific. |
| Conversion traits | `FromPyObject`, `IntoPyObject` in conversions guide | `FromLean`, `IntoLean` | Aligned for the documented Lean conversion matrix. Built-ins are intentionally smaller than PyO3's Python ecosystem matrix. |
| Derived conversions | `#[derive(FromPyObject)]` in `conversions/traits.md` | `#[derive(IntoLean, FromLean)]` | Implemented for structs/enums/newtypes with Leo3 attributes such as `transparent`, `skip`, `default`, `with`, `rename`, and `tag`. |
| Function export macro | `#[pyfunction]` in `function.md` | `#[leanfn]` | Implemented with generated C ABI wrappers, metadata, custom Lean-facing names, borrowed input storage, borrowed returns, and runtime tests. |
| Module macro | `#[pymodule]` in `module.md` | `#[leanmodule]` | Implemented for generated Lean initialization symbols, inline `#[leanfn]` export discovery, declarative registration (schema v2: `exports = [...]`, dotted nested paths, inner `mod` submodules), metadata, and real dynamic loading through `LeanModule::load`. |
| Class/object macro | `#[pyclass]` / `#[pymethods]` in `class.md` | `#[leanclass]` / `#[lean_instance]` | Implemented for Leo3 external objects and method lowering. Receiver and mutation semantics are documented explicitly because Lean does not share Python's class model. |
| Container conversions | PyO3 list/dict/set tables | `Vec<T>`, `Option<T>`, `Result<T,E>`, pairs, plus Lean containers | Core conversion matrix is aligned in concept. `LeanHashMap`, `LeanHashSet`, and `LeanRBMap` are stable real runtime wrappers on the default surface (Lean >= 4.22) for `LeanNat`, `LeanInt`, `LeanString`, and `LeanInt8`–`LeanInt64` keys. |
| Feature gates | PyO3 `features.md` / `Cargo.toml` | Leo3 feature table in `README.md` and `leo3/Cargo.toml` | Aligned in principle: optional subsystems are explicit. Feature names differ because Python extension concerns such as `abi3` do not apply to Lean. |
| Build/runtime loading | PyO3 extension init in `module.md` | `leo3-build-config`, `#[leanmodule]`, `LeanModule::load` | Implemented and runtime-tested with the fixture under `leo3/tests/fixtures/leanmodule_runtime_fixture`. |
| Introspection metadata | PyO3 generated module/function/class metadata and optional inspect feature | `__leo3_metadata_*`, `__leo3_module_metadata`, `__leo3_class_metadata_*` | Implemented for Leo3 macro consumers. Leo3 does not currently generate Python-style type stubs. |

## Current Verified Surface

- `#[leanfn]` accepts owned conversions plus a borrow-friendly subset for
  strings, vectors, slices, arrays, and supported nested `Option` / `Result` /
  tuple shapes.
- `#[leanfn]` supports borrowed return values in the string/vector/slice family
  by converting them back into owned Lean values.
- `#[leanmodule]` has crate-path-aware generation, module metadata, generated
  init symbols, and a runtime-tested dynamic loading success path.
- External objects expose borrow-first helper APIs while keeping trait-level
  `FromLean` clone-based.
- Containers (`LeanHashMap`, `LeanHashSet`, `LeanRBMap`) are stable on the
  default surface (Lean >= 4.22) and use real Lean runtime representations for
  the documented key matrix: `LeanNat`, `LeanInt`, `LeanString`,
  `LeanInt8`–`LeanInt64`, and `LeanUInt8`–`LeanUInt64`.
- `#[leanmodule]` supports declarative module registration (schema v2):
  `exports = [...]` for explicit export selection, dotted nested module paths
  (e.g. `Foo.Bar.baz`), and inner `mod` blocks with `#[leanfn]` discovered as
  nested submodules via `LeanSubmoduleMetadata`.

## Intentional Differences

- PyO3 models Python's object system and import/package machinery. Leo3 models
  Lean runtime objects, Lean module initialization, and Lean plugin loading.
- Leo3 does not implement PyO3's Python-only function signature controls such as
  keyword-only arguments, default argument syntax, `text_signature`, warnings, or
  `from_py_with`; those concepts do not have a direct Lean-call ABI equivalent.
- Leo3 does not mirror PyO3's broad optional conversions for Python ecosystem
  types such as `Path`, datetime, decimal, IP addresses, `uuid`, `hashbrown`, or
  `indexmap`. Leo3's built-in matrix is the Lean semantic core, and users can
  extend it with manual impls or derives.
- Leo3 does not expose Python-style properties, class attributes, subclassing,
  or magic-method slots. `#[leanclass]` instead exposes Rust values as Lean
  external objects with explicit receiver rules.
- Fixed-width signed integer wrappers (`LeanInt8`–`LeanInt64`) are now aligned
  with Lean's unboxed scalar ABI and are supported as container keys across the
  HashMap, HashSet, and RBMap families.
- Declarative module registration is implemented at the metadata level (schema
  v2): `#[leanmodule]` supports `exports = [...]` selection, dotted nested
  module paths, and inner `mod` blocks discovered as nested submodules. Leo3
  still does not mirror PyO3's runtime `PyModule::add_submodule` import-graph
  wiring, since Lean module initialization follows the generated init-symbol and
  plugin-loading model rather than Python's import system.

## Verification Boundary

This pass was checked against the local PyO3 checkout and then verified by
Leo3's own contract tests and runtime tests, including the all-features
workspace gate. The alignment target is PyO3's architecture and ergonomic shape,
not feature-for-feature cloning of Python-specific behavior.

## Gap Analysis (2026-08, audited against PyO3 0.28.3)

A fresh feature-by-feature audit of the PyO3 0.28.3 source (marker, instance,
conversion, err, types, pyclass, macros, build config) against Leo3's public
surface produced the following non-intentional gaps. All listed gaps are now
implemented, tested, and documented:

| PyO3 feature | Leo3 counterpart | Status |
| --- | --- | --- |
| any hashable object as dict/set key | user-defined `LeanHashMap` / `LeanHashSet` / `LeanRBMap` keys via `#[lean_instance(Hashable, BEq, Ord)]` (generates `ExternalHashKey` / `ExternalOrdKey` bridge impls; the Lean `Hashable` / `Ord` instance objects are built to match Lean's runtime layout) | implemented + runtime tests (`container_user_keys.rs`) |
| `#[pyo3(get, set)]` field accessors | `#[get]` / `#[set]` on `#[leanclass]` struct fields (getter is clone-based, setter is copy-on-write) | implemented + runtime/UI tests + `leo3-codegen` merge |
| `#[pyo3(name = "...")]` on methods / `#[pyclass(name)]` | `#[name = "..."]` on methods, `#[getter(name)]` / `#[setter(name)]`, `#[leanclass(name = "...")]` on struct and impl | implemented + runtime/metadata tests |
| `HashMap` / `HashSet` / `BTreeMap` conversions | `std::collections::{HashMap, HashSet, BTreeMap}` ↔ `LeanHashMap` / `LeanHashSet` / `LeanRBMap` for the supported key matrix (`String`, `u8`–`u64`, `i8`–`i64`) | implemented + runtime tests (`conversion_matrix_ext.rs`) |
| `Cell<T: Copy>` conversions | `Cell<T: Copy>` ↔ Lean (as `T`) | implemented + tests |
| `Cow<str>` / `Cow<[u8]>` conversions | `Cow<'_, str>` / `Cow<'_, [u8]>` ↔ `LeanString` / `LeanByteArray` | implemented + tests |
| tuples up to arity 12 | tuples up to arity 12 (was 6) | implemented + tests |
| macros available via prelude | `lean_instance` added to `leo3::prelude` | implemented |
| working IO handle surface | `io::handle` (`open`/`read`/`write`/`get_line`/`flush`/`is_eof`), `io::time`, `io::process`, `LeanIO` monad, and `IOError` all rewritten against the modern Lean ABI (4.25+) and covered by runtime tests — the previous implementation referenced stale C signatures that never worked on any modern Lean release | fixed + runtime tests |

### Items evaluated and intentionally not pursued

- `Python::allow_threads` / `Ungil` (detach): Lean's runtime has no global
  interpreter lock to release; caller threads attach through
  `with_lean` / `sync::ensure_lean_thread` instead. Not applicable by design.
- `intern!` string interning: Python's attribute-lookup hot path has no Lean
  analogue at this layer; Lean names live in the `meta` module.
- `Bound::cast_exact` family: Leo3's external-object `try_cast` already checks
  the exact registered class; an unchecked exact cast would be a footgun.
- `PyErr`-style typed exception constructors: `LeanError` variants plus
  `KernelExceptionCode` already cover the Lean error space.
- `#[pyfunction]` signature controls (keyword-only, defaults, `from_py_with`):
  documented Intentional Difference — Lean's extern ABI has no equivalent.
- `#[pymodule]` runtime `add_submodule` / import-graph wiring: documented
  Intentional Difference — Lean uses generated init symbols and plugin loading.
- Python-ecosystem conversions (`Path`, datetime, decimal, IP, uuid,
  hashbrown, indexmap): documented Intentional Difference — the built-in
  matrix is the Lean semantic core; users extend via manual impls or derives.
- Python-style properties / class attributes / subclassing / magic-method
  slots: documented Intentional Difference — `#[leanclass]` models Lean
  external objects with explicit receiver rules.
