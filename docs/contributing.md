# Contributing

This guide is the maintenance companion to `TESTING.md`.

## Before Changing Behavior

When behavior changes, check all of these surfaces together:

- README
- crate docs / rustdoc examples
- `docs/contracts.md`
- tests or UI snapshots that define the current contract

If a change touches public behavior and only updates code, it is probably
incomplete.

## Where To Add Tests

- runtime/token/conversion behavior: `leo3/tests/*_ops.rs`, `basic.rs`,
  `test_conversion.rs`, `test_gc.rs`
- macro shape and integration: `test_leanfn_macro.rs`, `test_leanclass*.rs`,
  `test_leanmodule.rs`, `test_macro_pipeline.rs`
- compile-fail contracts: `test_compile_error.rs` and `leo3/tests/ui/`
- feature-gating / public surface honesty: `test_features.rs`,
  `test_surface_contract.rs`

Add the narrowest test that protects the behavior you changed. Prefer one
high-signal regression test over broad incidental coverage.

## Updating UI Snapshots

If a compile-fail diagnostic changes intentionally:

```bash
TRYBUILD=overwrite LEO3_NO_LEAN=1 cargo test --locked -p leo3 --features macros --test test_compile_error
```

Review the `leo3/tests/ui/*.stderr` diffs before committing them.

## Higher-Bar Areas

Treat these as higher-bar changes:

- container wrappers (`LeanHashMap`, `LeanHashSet`, `LeanRBMap`)
- macro-generated type/ABI surfaces
- runtime initialization and thread-attachment code

For those areas, code changes should usually come with:

- a contract doc update
- a focused regression test
- a note in `docs/remaining-work-checklist.md` if the change resolves or moves a
  tracked gap

## CHANGELOG Maintenance

Every PR that changes public behavior must update `CHANGELOG.md` in the same
PR. Follow these rules:

- Use [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) section
  headings: `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`.
- Add entries under an `[Unreleased]` heading at the top. The release PR
  renames `[Unreleased]` to the version being cut and adds a fresh empty
  `[Unreleased]` section.
- One bullet per user-visible change; reference the PR or issue number.
- Breaking changes get their own `### Breaking Changes` subsection with a
  short migration note.
- Do not log chores, CI-only changes, or internal refactors that have no
  user-visible effect.

## Documentation Update Path

Use this rule of thumb:

- README for user-facing feature summaries
- `CHANGELOG.md` for version-by-version change history
- `docs/contracts.md` for the current public/runtime/macro contract
- `docs/architecture.md` for internal model and layering
- `TESTING.md` for CI/local command truth
- `docs/remaining-work-checklist.md` for still-open maturity gaps
