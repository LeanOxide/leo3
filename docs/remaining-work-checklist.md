# Leo3 Remaining Work Checklist

Status snapshot as of 2026-05-01.

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
  and `LeanInt8`–`LeanInt64`.
- fixed-width signed wrappers (`LeanInt8`–`LeanInt64`) use Lean's unboxed
  scalar ABI representation, aligned with Lean's container typeclass
  instances, and are part of the supported matrix.
- runtime tests cover supported paths across `HashMap`, `HashSet`, and `RBMap`,
  including duplicate inserts, replacement semantics, string-key support,
  fixed-width signed integer key support, and cross-family parity.

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

## Future Expansion, Not Current Blockers

- richer module-registration metadata beyond today's implicit inline
  `#[leanfn]` export set
- a broader external-object extraction contract, if Leo3 ever decides to widen
  beyond the current clone-based `FromLean` rule (see
  `docs/external-object-borrow-extraction.md`)
- `#[leanfn]` monomorphization generics subset (`concrete(Ty, name = "...")`
  annotation); general generics are not feasible — see `docs/rfc-generics.md`
- widening the container key matrix beyond the current narrow set
  (`LeanNat`, `LeanInt`, `LeanString`, `LeanInt8`–`LeanInt64`)

## Maintenance Rule

When one of the active items above moves, update these surfaces together:

- `README.md`
- `TESTING.md`
- `docs/contracts.md`
- runtime tests or UI tests that define the current contract
- this checklist
