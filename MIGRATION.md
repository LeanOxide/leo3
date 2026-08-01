# Migration Guide: 0.2.x → 0.3.0

## Breaking Changes

### 1. `experimental-containers` feature removed

Containers (`LeanHashMap`, `LeanHashSet`, `LeanRBMap`) are now part of the
stable default surface on Lean >= 4.22. The `experimental-containers` feature
gate no longer exists.

**Before (0.2.x):**

```toml
[dependencies]
leo3 = { version = "0.2", features = ["experimental-containers"] }
```

**After (0.3.0):**

```toml
[dependencies]
leo3 = "0.3"
```

Simply remove `experimental-containers` from your feature list. Containers are
available by default with no feature gate required.

### 2. Binding metadata schema v1 → v2

The binding IR metadata schema has been bumped from v1 to v2.
`LeanSubmoduleMetadata` was added to support declarative module registration
with `exports = [...]` and dotted nested module paths.

If you consume the binding IR schema directly (e.g. via `leo3-binding-ir`),
update your deserialization to handle the new `LeanSubmoduleMetadata`
structure.

## New Features (no action required)

- **Monomorphization generics**: `#[leanfn]` now supports a concrete
  monomorphization subset via `concrete(Ty, name = "...")` annotations.
- **Property accessors**: `#[leanclass]` supports `#[getter]` / `#[setter]`
  attributes to generate Lean accessor functions.
- **Extended key matrix**: Containers now support `UInt8`–`UInt64` keys in
  addition to the existing signed integer keys.
- **`leo3-codegen` CLI**: Reads embedded JSON metadata from cdylib binaries
  and generates Lean 4 `extern` declaration files.
- **Declarative module registration**: `#[leanmodule]` supports
  `exports = [...]`, dotted nested paths (e.g. `Foo.Bar.baz`), and inner
  `mod` blocks with `#[leanfn]`.

## MSRV

The minimum supported Rust version remains **1.88**. No change from 0.2.x.
