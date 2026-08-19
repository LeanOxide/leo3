# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- `io` now runs on **Lean 4.26 through 4.33** (and still 4.20/4.25): the
  runtime split that erased the `world` token from the IO primitives and
  from `EStateM.Result` (ctor `(0, 1)`), turned
  `lean_io_prim_handle_is_eof` into a raw `uint8_t`, and made
  `lean_get_stdin/stdout/stderr` parameterless is handled by `lean_4_26`
  version gates on the FFI declarations and the wrappers
- CI: the compat matrix now passes on Windows (the PE format forbids
  unresolved symbols in DLLs, so the dynamic-module test fixtures link the
  Lean runtime instead of building no-lean)
- `examples/class-integration/`: end-to-end Lean↔Rust template for the macro
  pipeline — a `cdylib` built with `#[leanclass]` (methods, `#[getter]` /
  `#[setter]`) plus `#[leanfn]` / `#[leanmodule]`, a reproducible
  `leo3-codegen` script, and a Lake package that consumes the generated
  declarations and verifies the results at runtime
- CI: an `examples` job that builds and runs both example projects end to end
  on Linux and macOS
- `leo3::__private::scalar_ffi_panic_boundary`: generic panic boundary for
  scalar-returning FFI entry points (used by generated code)
- `docs/codegen.md`: standalone `leo3-codegen` guide — installation from
  crates.io, the full cdylib → codegen → Lake project walkthrough (verified on
  Linux), cross-platform metadata extraction (ELF symbols / Mach-O section /
  PE exports), and the current scalar-ABI and class-elaboration limitations
  tracked in #159
- **User-defined container keys** (PyO3-aligned "any hashable key"): the
  combined `#[lean_instance(Hashable, BEq)]` /
  `#[lean_instance(Hashable, BEq, Ord)]` forms generate
  `ExternalHashKey` / `ExternalOrdKey` bridge implementations, so external
  classes can be used as `LeanHashMap` / `LeanHashSet` / `LeanRBMap` keys
  with real runtime semantics (the Lean `Hashable` / `Ord` instance objects
  are built by hand and match Lean's own instance layout, verified against
  the runtime's `l_instHashableNat` representation)
- `#[leanclass]` **field accessors**: `#[get]` / `#[set]` on named struct
  fields generate `fn field(&self) -> T` (clone-based) and
  `fn set_field(&mut self, value: T)` (copy-on-write) methods with FFI
  wrappers, Lean declarations, and metadata; `leo3-codegen` merges the
  field-accessor metadata with the impl-block metadata into one file per
  class
- `#[leanclass]` **naming**: `#[name = "..."]` on methods,
  `#[getter(name = "...")]` / `#[setter(name = "...")]` on accessors, and
  `#[leanclass(name = "...")]` on the struct and impl block override the
  Lean-visible names while FFI symbols stay Rust-identifier-derived
- **std collection conversions** (PyO3-aligned): `HashMap<K, V>`,
  `HashSet<K>`, and `BTreeMap<K, V>` round-trip through the real
  `LeanHashMap` / `LeanHashSet` / `LeanRBMap` for the supported key matrix
  (`String`, `u8`–`u64`, `i8`–`i64`)
- `Cell<T: Copy>` conversions (convert as `T`, both directions) and
  `Cow<'_, str>` / `Cow<'_, [u8]>` conversions, matching the plain
  `String` / byte-array paths
- Tuples now convert up to arity **12** (PyO3's limit; previously 6)
- `leo3::prelude` now also re-exports `lean_instance`
- `LeanByteArray` gained an identity `FromLean` implementation, and
  `LeanString::as_str` provides a length-aware zero-copy view

### Changed
- **`io` module rewritten against the modern Lean handle ABI** (verified
  against Lean 4.25.2's runtime and `IO.lean`):
  - `handle::open` no longer takes a `binary` parameter (modern Lean's
    `Handle.mk` has none; the C ABI is `(path, mode: uint8, world)`), and
    `FileMode` now mirrors `IO.FS.Mode` exactly: `Read`, `Write`,
    `WriteNew`, `ReadWrite`, `Append` (runtime tags 0–4)
  - `handle::write` sends a `ByteArray` (modern `Handle.write`'s argument)
    instead of a `String`; `handle::read` returns
    `LeanIO<LeanBound<LeanByteArray>>` (convert with `to_vec()` /
    `vec_u8_from_lean`)
  - IO closures are built with correct arity/fixed-slot counts and hold
    owned references in their fixed slots (closure deallocation releases
    them); scalar parameters (`mode`, read size) go through small wrappers
  - `LeanIO::run` and the `pure` / `then` combinators build and consume the
    real `EStateM.Result` layout (`ctor(0, 2, 0)` = `(value, world)`), the
    shape `lean_io_result_mk_ok` produces — previously the hand-built
    closures used a pair-wrapped layout that real Lean primitives do not
  - `IOError::from_lean_io_error` maps the 4.25+ `IO.Error` constructor
    table (19 constructors); the message comes from the last object field
  - `io::time` (`mono_nanos`, `unix_time_millis`) and `io::process`
    (`get_exit_code`, `set_exit_code`) are implemented in pure Rust — the
    historical `lean_io_prim_mono_nanos` / `lean_io_prim_get_unix_time_millis`
    / `lean_io_prim_get_exit_code` / `lean_io_prim_set_exit_code` symbols are
    not exported by Lean 4.25.2; `process::exit` now binds Lean's exported
    `lean_io_exit`
  - `io::console` is implemented over Lean's real `IO.FS.Stream` objects
    (applying the stream structure's `putStr` / `getLine` closures), and the
    `handle::stdin` / `handle::stdout` / `handle::stderr` functions were
    removed — they returned stream objects masquerading as file handles,
    which corrupted the `FILE*` read when passed to the handle primitives
- **`LeanString::mk` is a single length-aware copy** via
  `lean_mk_string_from_bytes` — no intermediate `CString` allocation — and
  embedded NUL bytes now round-trip (Lean strings are byte arrays);
  `cstr()` is length-aware (no C-string truncation)
- `leo3-codegen` merges multiple class-metadata entries by class name
  (impl block + field accessors) into a single `.lean` file per class
- **Module search path now anchors to the linked toolchain**: the runtime
  exports `LEO3_LEAN_SYSROOT` / `LEO3_LEAN_LIB_DIR` (the Lean library
  directory `leo3-build-config` bakes in at build time), so `discover_sysroot`
  resolves against the same toolchain the binary is linked against rather than
  elan / the project `lean-toolchain`. When an explicit `LEAN_SYSROOT`
  disagrees, `ensure_search_path` reports an actionable error instead of
  reading the wrong toolchain's `.olean` files and crashing.
- **`run_command` failure messages render via `MessageData.toString`**: the
  caption/data are read from the raw `Message` (stable 5-object-field layout)
  and rendered as `"{caption}: {data}"` (no position prefix), replacing the
  previous fixed-offset read of the serialized `SerialMessage` (which asserted
  on 4.25.2 and misrendered on 4.33.0-rc1).

### Fixed
- **Task manager initialization race**: `LeanTask::spawn` now initializes
  Lean's task manager exactly once under a lock (previously the manager was
  created lazily by Lean's runtime, and concurrent first spawns could queue
  tasks behind an unstarted worker pool, hanging every task wait).
  `finalize_task_manager` resets the initialization state so a
  finalize/re-init cycle works
- `LeanTaskFuture` watcher threads hold their own reference to the task
  object, fixing a use-after-free when a future is dropped while its
  background watcher thread is still polling the raw task pointer
- `#[leanfn]` codegen produced unannotated `Ok(...)` expressions when decoding
  `&[u8]` and `Vec<u8>` parameters (and `Option` / `Result` wrappers around
  them), which failed type inference (E0282) at the use site; the generated
  decodes now carry explicit error types
- `io` handle operations (`open` / `read` / `write` / `get_line` / `flush` /
  `is_eof`) previously panicked or produced garbage against every modern
  Lean release: IO closures were allocated with `num_fixed == arity`,
  `Handle.mk` was called with a stale 4-argument ABI (a `binary` parameter
  that does not exist), writes sent `String` where the runtime expects
  `ByteArray`, and the IO result layout was misread (see the `io` rewrite
  above). All handle paths now work end to end against Lean 4.25.2 with
  runtime tests
- `handle::is_eof` no longer references the Lean-level `IO.FS.Handle.isEof`
  API (absent in 4.25.2); it binds the exported
  `lean_io_prim_handle_is_eof` primitive directly
- **meta command/goal paths (`run_command` / `run_tactic` / `pp_goal`) now run
  on 4.26–4.33 cross toolchains**: the message-severity byte was read one slot
  early and the error text was extracted from the serialized `SerialMessage`
  at fixed field offsets (assert on 4.25.2, misrender on 4.33.0-rc1); reading
  the raw message fixes both. Also fixes the 4.33 cross test suite aborting
  (SIGABRT) in `test_run_cmd` and killing every subsequent test binary (W-344).
- `run_command` no longer leaks Lean objects on every call: the per-call
  `ST.Ref` that carried the post-command state was inc'd once and read back
  but never dec'd (pinning a full final `Command.State` — Environment /
  InfoState / MessageLog — on the heap for the lifetime of the session),
  the initial state held two extra `lean_inc` pins that nothing referenced,
  and on 4.26+ the `lean_st_mk_ref` world-argument box was allocated but
  never released. The temporary ref is now released after each call (on all
  error paths too) and the redundant pins are gone; repeated `run_command`
  calls keep Lean object counts flat (regression test: 100-iteration
  object-count stability, 4.25.2 and 4.33)

### Changed

- **Breaking (generated-code ABI):** `#[leanfn]` and `#[leanclass]` wrappers
  now follow Lean's extern calling convention — fixed-width scalar parameters
  and results (`u8`–`u64`, `i8`–`i64`, `usize`, `isize`, `f32`, `f64`, `bool`
  as `u8`, `char` as `u32`) cross the FFI boundary unboxed, while `String`,
  containers and class objects stay boxed `lean_object*` values. Previously
  every parameter and result crossed boxed, which did not match the
  declarations `leo3-codegen` emits: calling a generated `@[extern]`
  declaration from Lean segfaulted. Rust code calling the generated wrappers
  directly must pass/expect raw scalar values instead of boxed objects.
  Every `#[leanfn]` export additionally gets an all-boxed `{name}_boxed`
  companion entry point (mirroring Lean's own `_boxed` convention); dynamic
  loading via `LeanFunction::callN` prefers the companion, so module loading
  keeps its object-based `IntoLean` / `FromLean` calling ergonomics
- Scalar-returning wrappers report boundary failures by aborting with a
  diagnostic (scalar extern signatures cannot carry a Lean panic object);
  object-returning wrappers keep the existing `lean_panic_fn` behavior
- `*_LEAN_CLASS_DECL` constants and `leo3-codegen` class output now introduce
  the class type through `NonemptyType` (`opaque Foo.ffi : NonemptyType` /
  `def Foo : Type := Foo.ffi.val` / `instance : Nonempty Foo`), the same
  pattern the Lean standard library uses for `IO.RealWorld`. The previous
  bare `opaque Foo : Type` did not elaborate: `opaque` declarations require
  an `Inhabited`/`Nonempty` instance, so every generated constructor or
  updater declaration failed to type-check.
- `release.yml`: the publish step now skips crate versions that are already
  published on crates.io, so a partially failed release can be retried (via
  job re-run or `workflow_dispatch`) without bumping the version or
  overwriting the tag

### Fixed

- macOS link failure for `LEO3_NO_LEAN=1` cdylib builds (the lake-integration
  workflow): Apple's linker rejects the Lean runtime symbols such cdylibs
  intentionally leave undefined for load-time resolution (ELF linkers accept
  them). `leo3-build-config` now passes `-Wl,-undefined,dynamic_lookup` for
  Apple-target links in no-lean mode
- Heap corruption when a `LEO3_NO_LEAN=1` cdylib allocates Lean objects
  (external class instances, `Prod` results from `&mut self` methods that
  also return a value, inline-allocated containers): without a detected Lean
  toolchain the inline small-object allocator fell back to `libc::malloc`
  with a size prefix (a port of lean.h's system-allocator branch), but
  official Lean toolchains are built with `LEAN_MIMALLOC`, so the first
  host-side deallocation of such an object freed a malloc pointer through
  mimalloc and segfaulted. The fallback now allocates through the runtime's
  exported `lean_alloc_object` entry point, which dispatches to the host's
  real allocator; linkers surface the symbol into the host executable
  because the cdylib references it
- `leo3-codegen`: dotted module names (`#[leanmodule(name = "A.B")]`) now
  generate nested file paths (`A/B.lean`) matching Lean's import resolution.
  Previously the output file name was derived from the metadata symbol, whose
  dots are sanitized to underscores, producing `A_B.lean` that Lean could not
  import as `A.B`

## [0.3.1] - 2026-08-03

### Added

- Lake integration template: working `examples/lake-integration/` project with a
  Rust `cdylib` (scalar + `String` `extern "C"` functions), a Lake package with
  matching `@[extern]` declarations, and a step-by-step guide in
  `docs/getting-started.md` (#145)

### Changed

- Applied nightly rustfmt formatting fixes across the workspace
- Docs synced with the released 0.3.0 state: the `#[leanfn]` monomorphization
  generics subset and fixed-width container keys are no longer listed as
  future work in `docs/remaining-work-checklist.md`; README / TESTING /
  container docs no longer claim `Float` / `Float32` container key support
  that has not landed

### Fixed

- Windows linking: `leo3::module::set_importing_flag` referenced Lean's private
  `l___private_Lean_ImportingFlag_0__Lean_importingRef` symbol directly, but
  Lean's Windows DLLs do not export private `l_` symbols, so `link.exe` failed
  with `LNK2019` on every `compat-runtime-matrix` Windows leg (latent since the
  symbol was introduced, masked by fail-fast). The flag is now resolved at
  runtime via `dlsym` (Unix) / `GetProcAddress` (Windows) and degrades to a
  no-op when unavailable; Unix behavior is unchanged (W-138)
- `leo3-codegen` on macOS and Windows: the Mach-O linker does not surface the
  unreferenced `#[no_mangle] #[used]` metadata symbols in a dylib's symbol
  table, and PE DLLs only carry them in the export table (the COFF symbol
  table is stripped), so codegen failed to find any metadata. The macros now
  also embed each metadata entry (framed with a magic marker and explicit
  lengths) into a dedicated `leo3meta` link section (`__DATA,__leo3meta` on
  Apple targets; the name is kept <= 8 bytes because MSVC `link.exe`
  truncates longer PE section names), and `leo3-codegen` scans that section
  plus the PE export table as cross-platform fallbacks, merging the results
  with any symbols it finds. Linux behavior is unchanged (symbols still
  used); fixes the `compat-runtime-matrix` macOS/Windows failures (W-138)
- Lean 4.33+ compat: `lean_mk_empty_environment` export was removed; bind the
  Lean-compiled `l_Lean_mkEmptyEnvironment` symbol instead (leanprover/lean4#14306)
- Lean 4.33+ compat: `lean_add_decl`/`lean_elab_add_decl` gained a `maxRecDepth`
  argument; pass `0` (unlimited) to preserve prior behavior (leanprover/lean4#13956)
- Lean 4.31+ compat: `Meta.check` gained a `transparency : TransparencyMode`
  parameter, changing the compiled `l_Lean_Meta_check` arity from 6 to 7 with an
  unboxed scalar argument. `MetaMContext::check` now builds its closure via
  `lean_meta_check_closure`, which targets the compiler-generated
  `l_Lean_Meta_check___boxed` wrapper and fixes the default
  `TransparencyMode.all` on `lean_4_31`; the old arity-shifted closure corrupted
  the `Core.Context` reader and segfaulted in `checkTraceOption`
- `leo3-codegen` integration test strips CI instrumentation flags from the nested
  fixture build and resolves the binary via `CARGO_BIN_EXE_leo3-codegen` so it
  works under ASan / llvm-cov and redirected target dirs
- `array_conversion` benchmarks build `LeanArray` with `LeanUInt64` so
  `Vec<u64>` round-trips no longer read garbage from tagged Nat scalars
- trybuild UI snapshot test strips inherited `CARGO_ENCODED_RUSTFLAGS` so the
  `invalid_lifetime.stderr` snapshot stays deterministic under `cargo careful`

## [0.3.0] - 2026-07-27

### Added

- `leo3-codegen` CLI tool: reads embedded JSON metadata from cdylib binaries
  and generates Lean 4 `extern` declaration files for `#[leanmodule]` and
  `#[leanclass]` exports
- `#[leanmodule]` and `#[leanclass]` now embed JSON metadata as `#[no_mangle]`
  static symbols (`__leo3_module_metadata_json_*`, `__leo3_class_metadata_json_*`)
  in the compiled cdylib for consumption by external tooling
- Declarative module registration metadata (schema v2): `exports = [...]`
  option for `#[leanmodule]`, dotted nested module paths (e.g. `Foo.Bar.baz`),
  and inner `mod` blocks with `#[leanfn]` discovered as nested submodules
- `#[leanfn]` monomorphization generics subset via `concrete(Ty, name = "...")`
  annotation
- `#[leanclass]` property accessor support: `#[getter]` / `#[setter]`
  attributes generate Lean accessor functions
- Container key matrix extended with fixed-width signed integers
  (`LeanInt8`/`LeanInt16`/`LeanInt32`/`LeanInt64`) across HashMap, HashSet,
  and RBMap families
- Container key matrix extended with fixed-width unsigned integers
  (`LeanUInt8`/`LeanUInt16`/`LeanUInt32`/`LeanUInt64`) across HashMap, HashSet,
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

[Unreleased]: https://github.com/AndPuQing/leo3/compare/v0.3.1...HEAD
[0.3.1]: https://github.com/AndPuQing/leo3/compare/v0.2.2...v0.3.1
[0.3.0]: https://github.com/AndPuQing/leo3/compare/v0.2.2...v0.3.0
[0.2.2]: https://github.com/AndPuQing/leo3/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/AndPuQing/leo3/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/AndPuQing/leo3/compare/v0.1.6...v0.2.0
[0.1.6]: https://github.com/AndPuQing/leo3/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/AndPuQing/leo3/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/AndPuQing/leo3/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/AndPuQing/leo3/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/AndPuQing/leo3/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/AndPuQing/leo3/releases/tag/v0.1.1
