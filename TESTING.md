# Leo3 Testing Guide

Leo3 CI is split into small, named tiers so failures are easier to localize and contributors can run the same commands locally.

For the higher-level maintenance workflow, pair this guide with
`docs/contracts.md`, `docs/architecture.md`, `docs/pyo3-alignment.md`, and
`docs/contributing.md`.

## CI Tiers

| Tier | Purpose | Typical jobs | Default trigger |
| --- | --- | --- | --- |
| Smoke | Fast formatting / compile / feature-surface regressions | `rustfmt`, `clippy`, `msrv`, `no-lean`, minimal + optional feature surface, docs | Every PR and push |
| Runtime | Focused Lean-backed integration coverage | core runtime, async/tokio, macro runtime, FFI layout check | Every PR and push |
| API | PR-only compatibility guard | semver checks | Pull requests |
| Compat / Heavy | Broad matrix and expensive diagnostics | feature powerset, full OS/Lean matrix, beta clippy, careful, ASan, coverage | Pushes to `main` / `develop`, daily schedule, or PRs labeled `CI-build-full` |

The scheduled compatibility sweep runs daily at **03:17 UTC**.

## Required vs Optional Paths

- **Required on PRs:** Smoke + Runtime + API tiers.
- **Required on pushes to `main` / `develop`:** Smoke + Runtime + Compat / Heavy tiers.
- **Opt into the full PR matrix:** add the `CI-build-full` label.
- **Disable matrix fail-fast on a PR:** add the `CI-no-fail-fast` label.
- **Allow PR cache writes:** add the `CI-save-pr-cache` label.

## Local Commands

### Smoke

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features --workspace -- -D warnings
LEO3_NO_LEAN=1 cargo test --locked --workspace --exclude leo3 --lib
LEO3_NO_LEAN=1 cargo check --locked --workspace --tests --all-features
RUSTC_WRAPPER= LEO3_NO_LEAN=1 cargo test --locked -p leo3 --doc --no-default-features
RUSTC_WRAPPER= LEO3_NO_LEAN=1 cargo test --locked -p leo3 --doc --features "macros,task,tokio"
RUSTC_WRAPPER= LEO3_NO_LEAN=1 cargo test --locked -p leo3-macros --doc
LEO3_NO_LEAN=1 cargo test --locked -p leo3 --no-default-features --test test_features
LEO3_NO_LEAN=1 cargo test --locked -p leo3 --no-default-features --features "macros,meta,io,module-loading,tokio" --test test_features
LEO3_NO_LEAN=1 cargo test --locked -p leo3 --features macros --test test_compile_error
LEO3_NO_LEAN=1 cargo test --locked -p leo3 --features macros --test test_binding_metadata
LEO3_NO_LEAN=1 RUSTDOCFLAGS="-D warnings --cfg docsrs" cargo +nightly doc --locked --workspace --no-deps --all-features
```

### Runtime

```bash
cargo test --locked -p leo3 --features runtime-tests \
  --test basic \
  --test nat_ops \
  --test string_ops \
  --test array_ops \
  --test test_conversion \
  --test test_gc

cargo test --locked -p leo3 --features runtime-tests \
  --test rbmap_ops

cargo test --locked -p leo3 --features runtime-tests \
  --test hash_containers_ops

cargo test --locked -p leo3 --features "tokio,runtime-tests" \
  --test test_task_async \
  --test test_tokio_bridge

cargo test --locked -p leo3 --features "macros,runtime-tests" \
  --test test_conversion_macros \
  --test test_derive_macros \
  --test test_leanclass \
  --test test_leanclass_codegen \
  --test test_leanclass_minimal \
  --test test_leanfn_macro \
  --test test_lean_instance \
  --test test_leanmodule \
  --test test_leanmodule_loading \
  --test test_macro_pipeline \
  --example macro_pipeline

cargo test --manifest-path leo3-ffi-check/Cargo.toml
```

### Compat / Heavy

```bash
LEO3_NO_LEAN=1 cargo hack check --feature-powerset --exclude-features runtime-tests --workspace --tests
cargo test --locked --all-features --workspace
cargo careful test --locked --all-features --workspace
RUSTFLAGS='-Zsanitizer=address' cargo test --locked -Zbuild-std --target x86_64-unknown-linux-gnu --all-features --workspace
cargo llvm-cov --no-report nextest
cargo llvm-cov --no-report --doc
cargo llvm-cov report --doctests --lcov --output-path lcov.info
```

## UI Snapshot Updates

`macro-ui` runs the `trybuild` compile-fail suite in `leo3/tests/ui` explicitly.
When diagnostics intentionally change, refresh the snapshots with:

```bash
TRYBUILD=overwrite LEO3_NO_LEAN=1 cargo test --locked -p leo3 --features macros --test test_compile_error
```

Review the updated `leo3/tests/ui/*.stderr` files before committing them.

## Compile-Fail Matrix

The UI suite is the contract for intentionally unsupported surfaces. Current
named cases are:

| Rule | UI test |
| --- | --- |
| Lean object escapes `Lean<'l>` lifetime | `leo3/tests/ui/invalid_lifetime.rs` |
| missing `Lean<'l>` token is rejected by types | `leo3/tests/ui/missing_lean_token.rs` |
| wrapper/type mismatch is rejected by types | `leo3/tests/ui/type_mismatch.rs` |
| generic `#[leanclass]` struct is rejected | `leo3/tests/ui/leanclass_generic_struct.rs` |
| generic `#[leanclass]` impl is rejected | `leo3/tests/ui/leanclass_generic_impl.rs` |
| generic `#[leanclass]` method is rejected | `leo3/tests/ui/leanclass_generic_method.rs` |
| non-identifier parameter pattern is rejected | `leo3/tests/ui/leanclass_unsupported_pattern.rs` |
| reference type in generated Lean declaration is rejected | `leo3/tests/ui/leanclass_unsupported_ref.rs` |
| tuple arity other than pair is rejected | `leo3/tests/ui/leanclass_unsupported_tuple_arity.rs` |
| unsupported generic path type is rejected | `leo3/tests/ui/leanclass_unsupported_generic_type.rs` |

## Lean Discovery and No-Lean Mode

Build scripts use the same precedence rules in CI and locally:

1. `LEO3_NO_LEAN=1` short-circuits Lean detection and linking.
2. `DEP_LEAN4_LEO3_CONFIG` wins if Cargo provided it.
3. `LEO3_CONFIG_FILE` provides an explicit config file.
4. Otherwise host discovery tries `LEO3_CROSS_*`, then `LEAN_HOME`, then `lake`, then `elan`, then `PATH`.

Use `LEO3_NO_LEAN=1` whenever you want a compile-only path that should not depend on a Lean installation.

## Documentation Examples

- `leo3/src/lib.rs` includes `README.md` under `#[cfg(doctest)]`, so `cargo test --doc -p leo3 ...` validates the public quick-start examples too.
- Run the two `leo3` doctest commands in Smoke to cover both the minimal runtime surface and the `macros` / `task` / `tokio` paths.
- Run `cargo test --doc -p leo3-macros` to compile-check the proc-macro examples against a real downstream crate context.
- Leave examples as `ignore` only when they require values or Lean-side setup that a standalone doctest cannot construct cleanly (for example: opaque runtime-created handles, downstream Lean modules, or long API tours).

## Test Coverage Map

- `leo3/tests/test_features.rs`: feature-surface smoke tests.
- `leo3/tests/test_compile_error.rs` + `leo3/tests/ui/`: explicit `trybuild` UI coverage for the compile-fail matrix above.
- `leo3/tests/test_leanfn_macro.rs`: runtime FFI coverage for `#[leanfn]`,
  including borrowed string/vector/slice aliases and their supported
  `Option`/`Result`/tuple wrapper forms.
- `leo3` doctests: runtime initialization, README quick start, string/nat conversion, and task/tokio docs.
- `leo3-macros` doctests: compile-check macro usage snippets such as `#[leanfn]`, `#[leanclass]`, and derives.
- `leo3/tests/basic.rs`, `nat_ops.rs`, `string_ops.rs`, `array_ops.rs`, `test_conversion.rs`, `test_gc.rs`: core runtime path.
- `leo3/tests/hash_containers_ops.rs`, `leo3/tests/hashset_nat_ops.rs`, `leo3/tests/hashset_string_ops.rs`: real Lean `HashMap` / `HashSet` runtime path, including string-key and duplicate-insert coverage.
- `leo3/tests/rbmap_ops.rs`, `leo3/tests/rbmap_string_ops.rs`: real Lean `RBMap` runtime path, including string-key replacement coverage.
- `leo3/tests/container_key_matrix_ops.rs`: runtime coverage for the non-string
  supported key matrix beyond `Nat`, currently `Int`, `Int8`–`Int64`,
  `UInt8`–`UInt64`, `Float`, and `Float32`, across `HashMap`, `HashSet`, and
  `RBMap` (floats are hash-container keys only).
- `leo3/tests/container_family_parity.rs`: cross-family parity checks for the supported string-key and integer-key matrix.
- `leo3/tests/test_task_async.rs`, `leo3/tests/test_tokio_bridge.rs`: async/task/tokio runtime path.
- `leo3/tests/test_lean*.rs`, `test_derive_macros.rs`, `test_conversion_macros.rs`: macro integration path.
- `leo3/tests/test_binding_metadata.rs`: no-Lean structured metadata contract for `#[leanfn]`, `#[leanmodule]`, and `#[leanclass]`.
- `leo3/tests/test_macro_pipeline.rs` + `leo3/examples/macro_pipeline.rs`: end-to-end macro golden path covering `#[leanmodule]`, `#[leanfn]`, and `#[leanclass]`.
- `leo3/tests/test_leanmodule_loading.rs`: builds a real `cdylib` fixture with
  its own `build.rs` + `leo3-build-config` wiring, then loads, initializes,
  resolves symbols from, and calls that downstream-style artifact.
- `leo3-ffi-check/`: bindgen-backed FFI layout validation.

## Troubleshooting

**Lean runtime not found**
- Run a smoke command with `LEO3_NO_LEAN=1` first to confirm the Rust side still builds.
- If you expect Lean to be present, inspect the `cargo:warning=` lines from `leo3-build-config`.

**`trybuild` failures**
- If the new error is expected, regenerate snapshots with `TRYBUILD=overwrite`.
- If the new error is unexpected, fix the macro expansion or test input instead.

**Heavy jobs are too slow for a PR**
- Rely on the default PR tiers first.
- Add `CI-build-full` only when you need the full matrix before merge.
