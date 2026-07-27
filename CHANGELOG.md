# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Declarative module registration metadata (schema v2): `exports = [...]`
  option for `#[leanmodule]`, dotted nested module paths (e.g. `Foo.Bar.baz`),
  and inner `mod` blocks with `#[leanfn]` discovered as nested submodules
- Container key matrix extended with fixed-width signed integers
  (`LeanInt8`/`LeanInt16`/`LeanInt32`/`LeanInt64`) across HashMap, HashSet,
  and RBMap families
- 4 new examples: task/async, external objects, containers, module loading
- `MetaMContext::from_parts` and `MetaMContext::into_parts` for FFI consumers
- Borrowed parameter support in `#[leanfn]`
- Shared Leo3 binding metadata model (`leo3-binding-ir`)

### Changed

- Containers stabilized: `experimental-containers` feature gate removed;
  `LeanHashMap`, `LeanHashSet`, and `LeanRBMap` are now part of the stable
  default surface on Lean >= 4.22
- `leo3-binding-ir` refactored: `lib.rs` split into `model`, `analysis`, and
  `quoting` modules
- `apply3`–`apply8` boilerplate replaced with macro generation
- Performance: pre-allocation in `flatten()`, closure `apply_once` variants,
  benchmarks enabled

### Fixed

- CI fmt and clippy errors

### Breaking Changes

- `experimental-containers` feature removed — containers are now always
  available by default. Users who explicitly enabled the feature can remove
  it from their `Cargo.toml`.
- Binding metadata schema bumped from v1 to v2 (`LeanSubmoduleMetadata`
  added). Consumers of the binding IR schema must update to handle the new
  structure.

## [0.2.2] - 2025-07-01

### Added

- Named feature gates for the leo3 crate surface (#116)
- Big integer conversions with round-trip validation
- `macro_pipeline` example and integration test
- `char` and `unit` type conversions
- Fixed-size array and slice support in `#[leanfn]` macro
- Module export metadata and borrow-first external APIs
- Real Lean runtime semantics for experimental containers
- Improved proof and tactic MetaM support
- String-key and cross-family parity container tests

### Changed

- Maturity spec phases 1-5 implemented: API honesty, runtime, errors
- Phase docs consolidated into `docs/contracts.md`
- CI layered into smoke, runtime, and heavy tiers (#123)
- Public error surface unified (#124)
- Runtime scheduling and task waiting paths unified (#122)
- Lean discovery resolution unified across platforms

### Fixed

- Windows container symbol resolution
- Large Nat and Int float conversions
- Lean 4.20 manual meta defaults
- Lean ABI boundary helpers alignment (#120)
- Float/string/external FFI helpers and guardrails (#121)
- `lean.exe` detection under `LEAN_HOME` on Windows
- Core docs examples now compile-checked (#125)

### Breaking Changes

- Feature surface split into named gates; users relying on implicit default
  features must now enable the relevant `leo3` features explicitly.
- Unsupported generics are now restricted at compile time.

## [0.2.1] - 2025-06-18

### Fixed

- Derive leo3 cfg flags from leo3-ffi to prevent publish mismatch

## [0.2.0] - 2025-06-17

### Added

- MetaM integration: `MetaMContext`, `MetaM::run()`, structured error
  extraction from EIO exceptions
- Type-level operations: `whnf`, `isDefEq`, `is_type_correct`, declaration
  builders
- Proof support: equality proof constructors (`mk_eq`, `mk_eq_refl`,
  `mk_eq_symm`, `mk_eq_trans`), proof utility helpers
- `LeanEnvironment::add_decl` with worker thread architecture
- Tactic integration with MetaM-based operations (#54)
- Structured error variants for improved error handling (#16)
- IO Monad support with `LeanIO::map()` and `LeanIO::bind()` (#13)
- Tuple conversion support between Rust tuples and `LeanProd`
- `LeanClosure` creation and application methods
- Move semantics for `#[leanclass]` exclusive objects (#62)
- COW semantics for `#[leanclass]` external objects (#61)
- Minimal Lean code generation for `#[leanclass]` (#60)
- Field offset validation in leo3-ffi-check (#81)
- docs.rs metadata and docsrs cfg for all crates
- CI: cargo-semver-checks, AddressSanitizer, feature powerset testing,
  MSRV check, beta clippy, cargo-careful

### Changed

- `leo3-build-config` overhauled to align with PyO3's pyo3-build-config
  design (#105)
- MSRV bumped to 1.88 (libloading 0.9 requirement)
- Raw `lean_alloc_ctor` sequences replaced with semantic constructor helpers
  (#83)
- `LeanModule::load` routed through worker thread to fix ASan crash (#104)
- Promise FFI updated for Lean 4.27+ API change
- Windows MetaM crash resolved via GetProcAddress BSS lookup

### Breaking Changes

- MSRV raised from 1.80 to 1.88.
- Error handling migrated to structured error variants; code matching on
  string errors must be updated.
- `leo3-build-config` API redesigned to mirror pyo3-build-config; build
  scripts using the old API need migration.

## [0.1.6] - 2025-04-20

### Added

- `LeanUnbound` for thread-safe Lean object management
- `LeanTask` and `LeanThunk` wrappers for parallel computation and lazy
  evaluation
- Lazy initialization for `Init.Prelude` and `Lean.Expr` modules
- Additional methods for `LeanNat`, `LeanOption`, and `LeanString`
- Comprehensive metaprogramming tests

## [0.1.5] - 2025-03-15

### Added

- `LeanHashMap`, `LeanHashSet`, and `LeanRBMap` types
- IO operations module with file and environment handling
- `LeanBitVec` and `LeanRange` type wrappers
- Lean type wrappers for `Empty`, `Fin`, `Sigma`, `Subtype`, and `Sum`
- FFI bindings for Lean's Name, Environment, and Expression APIs
- Meta-programming support for Lean levels and literals
- Windows linking support for MSVC and MinGW toolchains

## [0.1.4] - 2025-02-20

### Added

- `log2` functions for Lean unsigned integer types
- Arithmetic and bitwise operation tests for `LeanUInt8` and `LeanInt8`

### Changed

- Simplified shift and conversion functions across integer types

## [0.1.3] - 2025-02-10

### Added

- Lean array creation and manipulation functions
- Bitwise operations and shift functions for `LeanNat`
- Integer conversion functions for `LeanInt` types
- Conversions for `UInt8`, `UInt16`, `UInt32`, `UInt64`, and `USize`

### Changed

- Optimized `testBit` using native Lean functions
- Removed deprecated bitwise operations in favor of native implementations

## [0.1.2] - 2025-01-25

### Added

- `Float32` support and conversion functions
- Signed integer, float, `Option`, and `Result` type conversions
- Smart conversion macros
- `ArrayBuilder` for efficient array construction
- Benchmarks for `Vec<T>` ↔ `LeanArray` conversions
- Windows compatibility improvements (library path, linking)

### Changed

- `LeanArray` conversion optimized with pre-allocation and
  `push_unchecked`

## [0.1.1] - 2025-01-10

### Added

- Derive macros for `IntoLean` and `FromLean` with container, field, and
  variant attributes
- `#[leanfn]` macro for exposing Rust functions to Lean
- Lean type wrappers for integers, lists, options, products, and strings
- Comprehensive tests for high-priority types and operations
- CI, release, and security workflows
- Pre-commit configuration

[Unreleased]: https://github.com/AndPuQing/leo3/compare/v0.2.2...HEAD
[0.2.2]: https://github.com/AndPuQing/leo3/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/AndPuQing/leo3/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/AndPuQing/leo3/compare/v0.1.6...v0.2.0
[0.1.6]: https://github.com/AndPuQing/leo3/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/AndPuQing/leo3/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/AndPuQing/leo3/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/AndPuQing/leo3/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/AndPuQing/leo3/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/AndPuQing/leo3/releases/tag/v0.1.1
