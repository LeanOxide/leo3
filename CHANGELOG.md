# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **`LeanEnvironment::free_regions`** (`meta` feature): releases the C++
  `compacted_region` buffers that `importModules` attaches to the
  environment header (`env.header.regions`, ~1.4 GB for a full `Lean`
  import) by calling Lean's `Environment.freeRegions` — the only release
  path for those buffers, which the stock runtime only invokes from the
  one-shot `lean` CLI path. The method consumes the environment (the
  underlying FFI is linear) and must be called on the last reference,
  after dropping everything derived from the import; repeated
  `importModules` sessions that release via `free_regions` now keep RSS
  flat instead of leaking ~1.4–1.6 GB per session (W-407 / W-413). The
  binding is version-gated on the 4.26 world-token erasure
  (`(env, world) -> EIO Unit` pre-4.26, `(env) -> EIO Unit` 4.26+) and is
  available on every supported toolchain (4.20–4.34)
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
- **Runtime layout assertion for Lean `Message`** (cross-toolchain,
  `runtime-tests` feature): a failing command is run through
  `run_command`, and the recorded `error`-severity `Message` is
  asserted directly from the runtime object header (5 object fields,
  aligned object size 56, `severity` byte 2 at object-relative offset
  41, `caption` / `data` fields readable) — so a Lean release that
  drifts the layout fails the test loudly instead of letting the FFI
  misread it silently (W-352)
- `leo3::task::TaskPriority::SYNC`: explicit inline-execution priority
  (Lean's `Task.Priority.sync`, `u32::MAX`) — the task runs on the calling
  thread instead of being queued to the task pool
- CI: a **v4.26.0** leg in `compat-runtime-matrix` (ubuntu/windows; macOS
  excluded, as with the flaky v4.20.0 leg) — same spec as the v4.20.0 leg
  (`cargo-test-full.sh cargo test --locked --all-features --workspace`).
  The 4.26 boundary is the densest version-gating seam in the codebase
  (world-token removal, Promise/Task C API de-IO-wrapping, the
  `Format.pretty` symbol rename, ST ref C API changes), and both W-375 and
  W-376 were only caught when a real v4.26.0 toolchain crashed — the
  pinned leg gives that boundary direct regression coverage in CI

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
- **`run_command` refactored around a shared elaboration core
  (`run_command_core`)**: the error-message scan is extracted into
  `first_error_message`, the updated `Environment`'s reference is taken
  once in the core (a single `lean_inc` when the `Command.State` field is
  read) and released by each caller on both the success and error paths
  (net refcount behavior unchanged), and the `runtime-tests`-only test
  hook `test_first_error_message` reuses the exact production pipeline
  instead of duplicating it (W-352)

### Fixed
- **spurious `maxHeartbeats` timeouts on long-lived workers (W-407 / W-412)**:
  Lean's heartbeat counter (the small-allocation counter) is thread-local and
  accumulates monotonically across every command on the worker thread. Only
  `CoreM.toIO` snapshots the baseline (`initHeartbeats := (← IO.getNumHeartbeats)`),
  but the monadic FFI entry points used by the `meta` module —
  `Lean.Elab.runTactic`, `Lean.Meta.ppGoal` / `ppExpr`,
  `Lean.Elab.Command.elabCommandTopLevel`, and `MetaM.run'` — do not go through
  `CoreM.toIO`, so `Core.checkMaxHeartbeatsCore` measured the *process-wide*
  allocation count against `maxHeartbeats` (200000 × 1000). A trivial tactic
  then deterministically "timed out" once enough prior work had run — each
  `import Modules` of `Lean` alone costs ~3.56M heartbeats, so a LeanDojo-style
  loop of fresh `Repl()` sessions crossed the 200M limit after ~56 imports.
  Each of those entry points now resets the worker thread's counter via
  `lean_io_set_heartbeats` (new FFI binding, alongside
  `lean_io_get_num_heartbeats`, version-gated for the 4.26 ST redesign that
  erased the world token from the IO primitives) before dispatching the
  command, so the
  per-command budget measures a single command's allocations again
  (`run_persistent` needs no reset — `MetaM.toIO` already snapshots the
  baseline). `tests/test_heartbeat_reset.rs` pins the regression: the worker
  counter is pushed past the limit, and a trivial tactic must still succeed.
- **CI: the vendored-libuv flake shield now covers every test target (W-389)**:
  the Lean 4.33 `libleanshared.so` libuv abort (W-350) is load-dependent and
  can take down any binary that drives the runtime, not just
  `test_eq_proofs` — after the 4.33.1 release the `Compat / Full Matrix`
  ubuntu/stable leg went red on exactly that signature and a rerun of the
  same commit passed. `.github/scripts/cargo-test-full.sh` now retries in
  isolation **any** sole failing target that died without printing a
  libtest summary (signal kill, not assertion failure), using cargo's own
  `to rerun pass` hint across all target forms — `--test`, `--bin`,
  `--example`, `--bench`, `--lib`, `--doc` (incl. workspace `-p pkg`); the
  section anchors were corrected against real cargo output (examples print
  `Running unittests examples/NAME.rs`, bins print
  `Running unittests src/main.rs` or `src/bin/NAME.rs` with the artifact
  stem dash-normalized — package/target `foo-bar` builds
  `deps/foo_bar-...` while the rerun hint keeps `--bin foo-bar` — benches
  print `Running benches/NAME.rs`); when the same target name exists in
  multiple workspace packages the anchor is ambiguous (cargo section
  headers do not name the owning package) and the run conservatively
  stays red; a clean retry turns the run green with a warning annotation,
  while deterministic failures (failing tests, any co-failing target, or a
  retry that fails again) still fail the run.
  `.github/scripts/test-cargo-test-full.sh` pins the section-mapping logic
  with 15 fake-cargo scenarios (run by the new `Smoke / Scripts` CI job).
- **`smoke-docs` job red since 2026-08-13 (broken rustdoc intra-doc link)**:
  `a25e55c` added a `[crate::meta::repl::run_command]` link to
  `MetaMContext::replace_env`'s doc, but `mod repl` is `#[cfg(lean_4_25)]`-gated,
  so under `LEO3_NO_LEAN=1` (the smoke-docs configuration) the target does not
  exist and `cargo doc -D warnings` fails — main has been red on the job
  since. The reference is now plain code text. Two same-class links that CI
  never exercised (they only fail the with-Lean doc build) are fixed too:
  `goal_hyps_and_type_pp`'s sibling-method link is `Self::`-qualified, and
  `ensure_search_path`'s doc no longer links the private `discover_sysroot`
- **v4.26.0 Promise/Task SIGSEGV/SIGABRT (W-376)**: the same 4.26 runtime
  rewrite that dropped the `RealWorld` token from the IO primitives
  (see W-375) also unwrapped the promise C API — `lean_io_promise_new`
  is now argument-free and returns the raw promise object, and
  `lean_io_promise_resolve` no longer takes a `world` token and returns
  the raw unit scalar (verified against the v4.25.2 vs v4.26.0 sources).
  The version gate that selected the raw-promise path was `lean_4_27`,
  so on v4.26.0 the `lean_4_26` build took the `IO`-wrapped path,
  misread the raw promise as a failed `IO` result, and
  `Promise::resolve` handed the unit scalar back as if it were an
  `IO` result — the resulting bad reference corrupted the worker thread
  and aborted the test process. The FFI declarations and the
  `LeanPromise::new` / `LeanPromise::resolve` code paths are now gated on
  `lean_4_26`; the `test_w359_registry_probe` promise canary uses the
  same gate
- **Windows LNK2019 `l_Lean_Elab_Tactic_tacticElabAttribute` (W-356)**: the
  import libraries bundled with official Windows dists (`<stem>.dll.a` under
  `lib/lean/`) can lag the DLL export tables shipped in `bin/` — in Lean
  4.33.0 the symbol is exported by `libleanshared_1.dll`, but the bundled
  chain does not provide it, so the MSVC link fails. The build script now
  parses each Lean DLL's PE export directory (including forwarded exports)
  and regenerates import libraries with the rustc-bundled `rust-lld`,
  cached under `~/.cache/leo3` keyed by DLL path+size+mtime; the bundled
  import libs remain the fallback when regeneration is not possible
- **Windows runtime init panic `tacticElabAttribute not initialized after
  initialize_Lean` (W-387)**: the next layer of the W-356 root-cause chain.
  Linking now passes (regenerated import libs), and `initialize_Lean` returns
  OK, yet the runtime canary at `leo3/src/runtime.rs` still saw
  `l_Lean_Elab_Tactic_tacticElabAttribute` as null/scalar on `windows-latest`
  for every Lean version (v4.26.0, stable, nightly) — while the identical
  init sequence is green on ubuntu. The canary (and
  `repl::register_builtin_tactic`) read the BSS global through a raw Rust
  `extern static` import. Rust `extern static` imports are unreliable for
  Windows DLL **data** symbols (the established fact documented in
  `leo3-ffi`, why every other BSS global in the crate is read through
  `win_bss::lookup_bss_global` = `GetProcAddress` + deref): the raw import
  reads null/stale even though `initialize_Lean` correctly set the DLL's
  global (same Lean C init code that works on Linux). Both sites now read
  the global via the new cross-platform `ffi::meta::get_tacticElabAttribute`
  accessor (extern static on non-Windows, `GetProcAddress` + deref on
  Windows), and the canary's panic message now reports the raw value read so
  a future recurrence self-diagnoses (null ⇒ symbol not exported or init
  chain did not run; scalar ⇒ unexpected encoding). Layer determination:
  not a stale import lib (link resolves + DLL loads), not a toolchain
  mismatch (all versions fail identically) — it is the data-symbol import
  form. The existing `windows-latest` × {stable, v4.26.0} legs of
  `compat-runtime-matrix` regression-protect the fix.
- **v4.20.0 compat reds (W-356)**: the `runTactic` / `ppGoal` repl bridge
  uses the Lean 4.25+ elaborator ABI, so it is now gated on `lean_4_25`
  (module, FFI re-exports, and the dependent tests). The `nextMacroScope`
  test expectation for Lean < 4.25 was wrong — the initial scope is 2 on
  every supported version (verified in the v4.20 and v4.25.2 sources), so
  the test no longer special-cases it
- **Nightly `lean_st_ref_set` removal (W-356)**: Lean 4.35 renamed
  `lean_st_ref_set` to `lean_st_ref_put` (the IO world token had already
  been dropped from the ST ref C API in 4.26 — see W-375 below); the
  declaration and the importing-flag writer are now version-gated on
  `lean_4_35`
- **v4.26.x `Format.pretty` link break (W-375)**: the same 4.26 commit that
  dropped the world token also dropped `@[export lean_format_pretty]`,
  so `lean_format_pretty` is gone from the exports of every release
  from v4.26.0 on (verified against the binaries of v4.25.2 vs
  v4.26.0 / v4.27.0 / v4.30.0 / v4.33.0 / v4.34.0-rc1). The
  `lean_format_pretty` / `l_Std_Format_pretty` gate is now
  `lean_4_26` instead of `lean_4_31` — v4.26–4.30 linked a symbol that
  does not exist, so the v4.26.x leg (and anything on 4.27–4.30) failed
  to build the `pp_goal` test binary
- **ST ref FFI world-token boundary (W-375)**: the C runtime dropped the
  IO world token from the entire `st_ref` family (`lean_st_mk_ref`,
  `lean_st_ref_get`, `lean_st_ref_set`, `lean_st_ref_swap`, and the
  take/reset export) in **v4.26.0**, not in 4.35 (verified against the
  `lean.h` headers and exported symbols of v4.20.0, v4.25.2, v4.26.0,
  v4.33.0, v4.34.0-rc1, and 4.35.0-nightly). The Rust declarations
  (2-arg `mk_ref` / `get`, 3-arg `set` / `swap`, 2-arg reset) disagreed
  with the real C signatures on 4.26–4.34 and the call sites
  compensated with dummy world boxes. The declarations and call sites
  are now gated on the real boundary (`lean_4_26`; `lean_st_ref_put`
  from `lean_4_35`). The reset declaration now also binds the real
  exported symbol `lean_st_ref_take` — the ≤ 4.34 header declares it as
  `lean_st_ref_reset`, but the runtime has exported `take` since the
  2020 ST primitive rename and never exported the header name
- **Mid-suite test crashes on cross toolchains (W-357)**: several
  version-gated layout bugs in the hand-built elaborator contexts:
  - `Lean.Elab.Term.Context` was built with the 4.25/4.26 layout
    (7 object fields + 11 Bool scalars) on every Lean < 4.31. The layout
    changed in **4.27** (`autoBoundImplicits : PersistentArray` became
    `autoBoundImplicitContext : Option AutoBoundImplicitContext` and
    `fixedTermElabs : Array FixedTermElabRef` was appended → 8 objects),
    and **4.29** added the `isMetaSection` scalar (`checkDeprecated`,
    present since 4.25, was already there): 8 + 10 scalars on 4.27–4.28,
    8 + 11 on 4.29+ (verified against the `TermElabM.lean` source of
    every release from v4.25.2 through the 4.34-nightly commit
    23393b95, and cross-checked against the stage0-generated C:
    `lean_alloc_ctor(0, 8, 10)` on 4.28/4.29). On 4.27–4.30 the 11
    scalar bytes were written over the `fixedTermElabs` object pointer,
    so the first incref of that field SIGSEGV'd every `run_tactic` call
    (`test_meta_repl` on 4.28.0 / 4.30.0). `default_term_context` now
    has one branch per layout: < 4.27 (7 + 11), 4.27–4.28 (8 + 10),
    4.29+ (8 + 11).
  - `Lean.Elab.Command.State` gained a 12th object field in **4.34**
    (`prevLinterStates : Option (Task (Array LinterState))`, default
    `none`); `mk_command_state` allocated 11 fields, so the linter
    bookkeeping in `elabCommandTopLevel` wrote the field out of bounds,
    corrupting the adjacent mimalloc block and SIGSEGV'ing the next
    unrelated allocation (`test_run_cmd` on 4.34-nightly). The field is
    now initialized on `lean_4_34`+ (verified against `Command.lean`
    v4.25.2…v4.33.0 + 4.34-nightly: 11 fields on 4.25–4.33, 12 on 4.34).
  - The manual `Meta.Context` fallback (Windows / missing-BSS path)
    gated the `cacheInferType` scalar on 4.31, but Lean added it in
    **4.28** (7 objects + 3 scalars on 4.25–4.27, + 4 on 4.28+), and its
    hand-built `Meta.Config` was missing the `zetaHave` byte (present
    since 4.25: 19 scalar bytes, not 18). Both are now version-correct
    (`Meta/Basic.lean` v4.25.2…v4.33.0).
  - `MetaMContext::set_local_context` (the Linux hot path behind
    `with_local_context`, used by every tactic-side
    `infer_type` / `whnf` / `is_def_eq` / `checked_assign`) had the same
    `cacheInferType` mis-gate: it built its scoped `Meta.Context` with
    3 scalar bytes on Lean 4.28–4.30, leaving the 4th byte (read by
    `Meta.inferType` on every call) uninitialized. Gated on 4.28 like
    the fallback above.
  - The manual `Meta.Config` now allocates **20** scalar bytes on Lean
    4.34+ (`canUnfoldPredicateConfig : CanUnfoldPredicateConfig`, 1
    byte, appended by lean4 #14323 / commit `47cd72d8fb20` in the
    4.34-nightly tree; `Context.canUnfold?` → `customCanUnfoldPredicate?`
    is a rename with the same position, so only the byte count changes).
    4.25–4.33 stay at 19 (`Meta/Basic.lean` v4.25.2…v4.33.0 + 23393b95).
  - The manual/default fallbacks that `LEO3_FORCE_MANUAL_META_DEFAULTS`
    (and the Windows missing-BSS path) rely on were never executed on
    Linux, and two of them were wrong. Now exercised and fixed:
    - `Core.Context.options`: the forced path returned `box(0)` (KVMap
      guess). On Lean 4.28+ `Options` is `structure { map : NameMap
      DataValue, hasTrace : Bool }` (`Data/Options.lean`) — ctor (0,1,1)
      with field 0 = `DTreeMap.empty` (which erases to the tagged scalar
      `box(1)`, confirmed via `l_Std_DTreeMap_empty` → `mov $3,%eax;ret`
      and the BSS `l_Lean_Options_empty` object bytes); ≤ 4.27 it stays
      `box(0)` (`Options := KVMap`, `RBMap.empty` erases).
    - `MetavarContext`: the manual 9-field shape (3 boxed Nats + 6
      PersistentHashMaps) is right for ≤ 4.30, but 4.31 split out the
      LMVarId state (`lmvarCounter`, `lDecls`; `MetavarContext.lean` of
      v4.31.0…v4.33.0 + 23393b95) → 4 Nats + 6 maps. The manual
      construction is now 10 object fields on `lean_4_31`+; the old
      shape crashed `MetavarContext.addExprMVarDecl` (bogus big-Nat).
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
  and the `lean_st_mk_ref` world-argument dummy box (ignored by both the
  2-arg pre-4.26 export and the 1-arg 4.26+ export) was allocated but
  never released. The temporary ref is now released after each call (on
  all error paths too), the redundant pins are gone, and the dummy box is
  released right after the `mk_ref` call; repeated `run_command` calls
  keep Lean object counts flat (regression test:
  `test_run_cmd_no_object_leak_across_calls`, standalone binary asserting
  that the base-Environment refcount — and RSS, as a backstop — stay flat
  across 100 `run_command` calls, on 4.25.2 and 4.33)
- **CI: the full cross-toolchain suite can no longer be masked by a single
  aborted test binary (W-350)**: the Lean 4.33 `libleanshared.so` statically
  vendors libuv, which intermittently trips an assertion in
  `uv__epoll_ctl_flush` under load and SIGABRTs the `test_eq_proofs` binary
  (a toolchain race, not a leo3 bug). Every full-workspace test job
  (`Compat / Full Matrix`, `Heavy / Careful`, `Heavy / AddressSanitizer`)
  now runs through `.github/scripts/cargo-test-full.sh`, which adds
  `--no-fail-fast` (one abort no longer stops every later binary from
  running, as in W-344) and retries `test_eq_proofs` in isolation when it
  is the sole failure and died without a libtest summary — a clean retry
  turns the run green with a warning annotation, while deterministic
  failures still fail the run. A new `.gitattributes` pins `*.sh` to LF so
  the Windows matrix legs (runner git sets `core.autocrlf=true`) check the
  runner script out byte-identical to the committed LF version.
- `leo3::task::TaskPriority::LOW` now maps to Lean's `Task.Priority.max`
  (8, the lowest in-pool priority). The previous value `u32::MAX` is
  Lean's sync priority, which makes `enqueue_core` run the task inline on
  the calling thread — `LOW` silently bypassed the task pool (W-360)

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
