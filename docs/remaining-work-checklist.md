# Leo3 Remaining Work Checklist

Status snapshot as of 2026-08-01 (synced with the released 0.3.0 state).

This file tracks the maturity gaps that are still meaningfully open after the
current hardening pass. It is not a re-listing of every future enhancement
idea, and it should not regress into a stale copy of the older roadmap.

## What Changed Since the Old Checklist

The earlier four-item list no longer matches the codebase:

- `#[leanmodule]` now has structured parsing, crate-path-aware generation, and
  metadata-driven implicit export discovery through
  `__leo3_module_metadata()`.
- macro producers now share a real workspace semantic IR/analyzer crate
  (`leo3-binding-ir`) instead of ad hoc name-only metadata.
- external objects now have an explicit borrow-first wrapper API through
  `borrow()`, `try_get_mut()`, and `try_take_inner()`.
- architecture and contributor docs now exist and are linked from the main
  maintenance docs.

That means the old broad unresolved list is closed for the current conservative
policy. Remaining items below are future expansion, not blockers for the
current hardening pass.

The 0.3.0 release additionally landed two items that earlier snapshots of this
checklist still listed under future expansion: the `#[leanfn]` monomorphization
generics subset and the fixed-width signed/unsigned container keys. Both are
now tracked as closed below.

## Active Remaining Work

No active maturity gaps are tracked for the current hardening pass.

## Closed For The Current Policy

### Containers

Status:

- Stabilized and ungated on the default feature set.

Decision (2026-07-27):

- The `experimental-containers` feature gate was removed. `LeanHashMap`,
  `LeanHashSet`, and `LeanRBMap` are now part of the stable default surface.
- The surface still requires Lean >= 4.22 (the `lean_4_22` cfg), because the
  implementations rely on Lean's reduced-arity (`_redArg`) container ABI.

What landed:

- `LeanHashMap`, `LeanHashSet`, and `LeanRBMap` all have real runtime-backed
  implementations for a narrow key matrix.
- the supported key matrix is explicit: `LeanNat`, `LeanInt`, `LeanString`,
  `LeanInt8`–`LeanInt64`, and `LeanUInt8`–`LeanUInt64`.
- fixed-width signed wrappers (`LeanInt8`–`LeanInt64`) and unsigned wrappers
  (`LeanUInt8`–`LeanUInt64`) use Lean's unboxed scalar ABI representation,
  aligned with Lean's container typeclass instances, and are part of the
  supported matrix.
- runtime tests cover supported paths across `HashMap`, `HashSet`, and `RBMap`,
  including duplicate inserts, replacement semantics, string-key support,
  fixed-width signed and unsigned integer key support, and cross-family parity.

Code evidence:

- `leo3/src/types/containers/hashmap.rs`
- `leo3/src/types/containers/hashset.rs`
- `leo3/src/types/containers/rbmap.rs`
- `leo3/src/types/containers/README.md`
- `leo3/tests/hash_containers_ops.rs`
- `leo3/tests/hashset_nat_ops.rs`
- `leo3/tests/hashset_string_ops.rs`
- `leo3/tests/rbmap_ops.rs`
- `leo3/tests/rbmap_string_ops.rs`
- `leo3/tests/container_key_matrix_ops.rs`
- `leo3/tests/container_family_parity.rs`

Definition of done (met):

- The supported key matrix is documented, tested, and no longer likely to
  surprise downstream users.
- Runtime tests cover the supported paths against actual Lean semantics across
  all three container families.

Re-open this item only if Leo3 deliberately widens the key matrix or changes the
fixed-width integer wrapper representation.

### `#[leanfn]` monomorphization generics subset

Status:

- Landed in 0.3.0; closed for the current conservative policy.

What landed:

- generic `#[leanfn]` functions are supported through an explicit
  monomorphization subset using `concrete(Ty, name = "...")` annotations;
  each annotation generates a separate, fully monomorphized C ABI wrapper,
  metadata entry, and Lean-visible declaration
- the contract is documented in `docs/contracts.md` (the
  "`#[leanfn]` monomorphization subset" section) and the design rationale in
  `docs/rfc-generics.md`
- compile-fail coverage: `leo3/tests/ui/leanfn_generic_without_concrete.rs`,
  `leo3/tests/ui/leanfn_concrete_wrong_arity.rs`, and
  `leo3/tests/ui/leanfn_concrete_missing_name.rs`
- runtime coverage: `leo3/tests/test_leanfn_macro.rs` exercises concrete
  `u64` / `i64` instances end to end

Definition of done (met):

- the subset is documented, tested, and no longer tracked as open work.

Re-open this item only if Leo3 deliberately changes the monomorphization
contract. General (non-enumerated) generics remain intentionally infeasible —
see `docs/rfc-generics.md`.

### Real module-loading success-path coverage

Status:

- Closed for the current module-loading contract.

What landed:

- `#[leanmodule]` metadata tests cover the implicit export model.
- runtime tests cover the generated init symbol without reinitializing a
  downstream `cdylib`'s separate Rust `leo3` static state.
- `leo3/tests/test_leanmodule_loading.rs` builds a real downstream `cdylib`
  fixture and loads it through `leo3::module::LeanModule::load(...)`.
- the fixture uses its own `build.rs` plus `leo3-build-config`, so the
  final shared library carries Lean's runtime search path like a real
  downstream artifact should.
- `LeanModule::load(...)` temporarily opens Lean's importing window around
  dynamic loading and module initialization, matching Lean's plugin-loading
  requirement for option / environment-extension registration after
  `IO.initializing` has ended.
- `LeanFunction::callN(...)` calls exported `#[leanfn]` C ABI wrappers directly,
  so loaded-module calls exercise the actual exported symbols rather than a
  closure-returning proxy assumption.
- the success-path test resolves `fixture_add` and `fixture_banner`, calls both,
  and verifies converted Rust results.

Code evidence:

- `leo3/src/module.rs`
- `leo3/tests/test_leanmodule.rs`
- `leo3/tests/test_leanmodule_loading.rs`
- `leo3/tests/fixtures/leanmodule_runtime_fixture/src/lib.rs`

Definition of done:

- the test fixture is built, loaded, initialized, queried for an exported
  function, and called successfully through the public module-loading API.
- failures in that flow localize whether the issue is fixture generation,
  dynamic loading, symbol naming, init return shape, or call conversion.

### External objects

Status:

- Closed for the current conservative contract.

What landed:

- borrow-first helper APIs
- runtime coverage for the non-cloning wrapper path
- docs that make the ownership split explicit

What is intentionally not treated as open work right now:

- trait-level `FromLean` remains clone-based
- borrow-first extraction remains a wrapper-level API, not a trait-level one

Design record: `docs/external-object-borrow-extraction.md` evaluates a
trait-level `FromLeanBorrowed` and concludes the wrapper-layer API is
sufficient.

Re-open this item only if Leo3 deliberately changes the extraction contract.

### Architecture and contributor docs

Status:

- Closed for this phase.

What landed:

- `docs/architecture.md`
- `docs/contributing.md`
- cross-links from `README.md` and `TESTING.md`

These docs can always deepen later, but their absence is no longer a maturity
gap.

## Closed Since 0.3.0

### User-defined container keys (wider key matrix)

Status:

- Landed: external classes can now be used as `LeanHashMap` / `LeanHashSet` /
  `LeanRBMap` keys.

What landed:

- the combined `#[lean_instance(Hashable, BEq)]` and
  `#[lean_instance(Hashable, BEq, Ord)]` forms generate
  `ExternalHashKey` / `ExternalOrdKey` bridge implementations for the class
  (plus the existing per-class FFI functions)
- leo3 implements `LeanHashKey` / `LeanRBMapKey` for
  `LeanExternalType<K>` through blanket impls that build the runtime
  `Hashable` / `Ord` instance objects; the instance layout (erased
  one-field structure = the hash/compare closure itself, and the boxed
  `UInt64` hash result as `ctor(0, 0, 1)` with the value in scalar slot 0)
  was verified against the runtime's own `l_instHashableNat` object
- runtime tests cover HashMap insert/find/erase, HashSet insert/contains/
  erase, and RBMap insert/find/erase with user-defined key classes,
  including replacement semantics and cross-object equality

Code evidence:

- `leo3-macros-backend/src/lean_instance.rs`
- `leo3/src/types/containers/hashmap.rs` (`ExternalHashKey`),
  `leo3/src/types/containers/rbmap.rs` (`ExternalOrdKey`)
- `leo3/tests/container_user_keys.rs`

Remaining matrix limits (documented, intentional): `Float` / `Float32` keys
(Lean has no `Hashable Float` instance) and non-external user keys.

### `#[leanclass]` field accessors and naming

Status:

- Landed: `#[get]` / `#[set]` on named fields; `#[name = "..."]` on methods
  and `#[getter(name)]` / `#[setter(name)]`; `#[leanclass(name = "...")]`
  on the struct and impl block.

What landed:

- getters (`fn field(&self) -> T`, clone-based) and setters
  (`fn set_field(&mut self, value: T)`, copy-on-write) with FFI wrappers,
  Lean declarations, metadata, and UI rejections for unsupported field
  types and tuple-struct accessors
- `leo3-codegen` merges the field-accessor metadata with the impl-block
  metadata into one `.lean` file per class

Code evidence:

- `leo3-binding-ir/src/analysis.rs` (`FieldAccessor`,
  `analyze_lean_class_field_accessors`, `field_accessor_bindings`)
- `leo3-macros-backend/src/leanclass.rs`
- `leo3-codegen/src/main.rs` (per-class metadata merge)
- `leo3/tests/test_leanclass_field_accessors.rs`,
  `leo3/tests/test_leanclass_rename.rs`,
  `leo3/tests/ui/leanclass_field_accessor_*.rs`

### std collection conversions and conversion matrix extensions

Status:

- Landed: `HashMap` / `HashSet` / `BTreeMap` ↔ `LeanHashMap` /
  `LeanHashSet` / `LeanRBMap` for the supported key matrix; `Cell<T: Copy>`;
  `Cow<str>` / `Cow<[u8]>`; tuples up to arity 12.

Code evidence:

- `leo3/src/conversion.rs` (`std_collection_conversions`,
  tuple arity 7–12, `Cell` / `Cow` impls)
- `leo3/tests/conversion_matrix_ext.rs`

### IO module correctness against the modern Lean ABI

Status:

- Landed: the `io` module was rewritten against the 4.25+ runtime ABI and
  is now covered by runtime tests (previously the handle primitives were
  called with a stale C signature and never worked on any modern Lean
  release).
- **Lean 4.26–4.33 support landed** (2026-08): Lean 4.26 erased the `world`
  token from every IO primitive and from `EStateM.Result` (the ok
  constructor carries one field instead of two), turned
  `lean_io_prim_handle_is_eof` into a raw `uint8_t`, and made
  `lean_get_stdin/stdout/stderr` parameterless with the stream returned
  directly. The FFI declarations and wrappers are version-gated on
  `lean_4_26`, so 4.20/4.25 (world-based ABI) and 4.26+ both work; verified
  locally against 4.25.2 and 4.32.2 and by the compat matrix
  (ubuntu/macos/windows × v4.20.0/stable/beta/nightly).
- Windows: dynamic-module test fixtures now link the Lean runtime instead
  of building no-lean (the PE format forbids unresolved symbols in DLLs,
  where ELF/Mach-O resolve them lazily), fixing the LNK2019 link failures
  in the `meta` fixture tests.

What landed:

- `handle::open` (5-mode `FileMode` mirroring `IO.FS.Mode`, no `binary`
  parameter), `write` (ByteArray), `read` (returns
  `LeanIO<LeanBound<LeanByteArray>>`), `get_line`, `flush`, `is_eof`
- `LeanIO::run` and the `pure`/`then` combinators use the real
  `EStateM.Result` layout; IO closures carry correct arity and owned fixed
  slots
- `IOError` maps the 4.25+ 19-constructor `IO.Error` table
- `io::time` / `io::process` are pure-Rust implementations (the historical
  C primitives are not exported by Lean 4.25.2); `process::exit` binds
  `lean_io_exit`
- `LeanString::mk` / `as_str` are length-aware (embedded NULs round-trip,
  single-copy construction)

Code evidence:

- `leo3/src/io/{mod,handle,error,time,process}.rs`,
  `leo3-ffi/src/io.rs`
- `leo3/tests/io_handle_ops.rs`, `leo3/tests/io_ops_comprehensive.rs`

## Known Latent Issues (documented, not yet fixed)

These were surfaced by the 2026-08 coverage pass. They are latent (triggered
only by specific `meta` API sequences) and require precise ABI verification
against Lean's runtime, so they are tracked here rather than fixed blindly:

- `l_Lean_ConstantInfo_*` accessors consume their argument per Lean's ABI,
  but `leo3/src/meta/environment.rs` passes borrowed pointers; repeated
  access is a use-after-free (tests pass `cinfo.clone()` per accessor to
  work around it)
- `lean_local_ctx_find_from_user_name` / `lean_local_decl_fvar_id` /
  `lean_local_ctx_num_indices` are declared as borrowed in
  `leo3/src/meta/context.rs` but the bound symbols consume their argument,
  causing heap corruption; the corresponding
  `goal_hypothesis` / `goal_latest_hypothesis` / `assumption`-success tests
  are omitted until the declarations are fixed
- `lean_finalize_task_manager` on Lean 4.25.2 has a runtime join race that
  occasionally hangs the process (timing-dependent; also triggered by
  llvm-cov instrumentation). `LeanTask::spawn` no longer depends on Lean's
  lazy manager setup (it initializes once under a lock), but the
  finalize/re-init cycle remains a Lean runtime boundary behavior and its
  test is `#[ignore]`d
- a successful `MetaMContext::checked_assign` (e.g. `exact` / `apply`
  success) corrupts the Lean heap and crashes the next
  `LeanEnvironment::empty`; only the failure paths are tested

## Future Expansion, Not Current Blockers

- richer module-registration metadata beyond today's implicit inline
  `#[leanfn]` export set
- a broader external-object extraction contract, if Leo3 ever decides to widen
  beyond the current clone-based `FromLean` rule (see
  `docs/external-object-borrow-extraction.md`)
- general `#[leanfn]` generics beyond the landed monomorphization subset
  (`concrete(Ty, name = "...")`); intentionally not feasible — see
  `docs/rfc-generics.md`
- widening the container key matrix beyond the current set (external-class
  keys landed; `Float` / `Float32` keys remain infeasible because Lean has
  no `Hashable Float` instance)

## Maintenance Rule

When one of the active items above moves, update these surfaces together:

- `README.md`
- `TESTING.md`
- `docs/contracts.md`
- runtime tests or UI tests that define the current contract
- this checklist
