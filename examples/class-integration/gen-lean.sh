#!/usr/bin/env bash
# Regenerate the Lean extern declarations from the cdylib's embedded binding
# metadata. The output is committed so the Lake package builds without Rust;
# run this script whenever native/src/lib.rs changes, and commit the diff.
#
# Requirements: a Rust toolchain (cargo). Lean is NOT required here —
# leo3-codegen reads the metadata straight out of the binary.
set -euo pipefail
cd "$(dirname "$0")"

# Build the cdylib. LEO3_NO_LEAN=1 keeps the cdylib from linking
# libleanshared itself; the Lean executable provides those symbols at
# runtime, and linking them twice causes duplicate-symbol failures.
echo "==> cargo build (native cdylib)"
(cd native && LEO3_NO_LEAN=1 cargo build --release)

# Locate the platform-specific cdylib.
case "$(uname -s)" in
    Linux) lib=native/target/release/libclass_integration.so ;;
    Darwin) lib=native/target/release/libclass_integration.dylib ;;
    MINGW* | MSYS* | CYGWIN*) lib=native/target/release/class_integration.dll ;;
    *)
        echo "error: unsupported platform '$(uname -s)'" >&2
        exit 1
        ;;
esac

# Run leo3-codegen. By default it is built from the enclosing workspace, so
# the codegen version always matches the leo3 path dependency above. Set
# LEO3_CODEGEN=/path/to/leo3-codegen to use an installed binary instead
# (e.g. `cargo install leo3-codegen`).
#
# The output directory is the Lake library root: the module registered as
# `ClassIntegration.Native` generates `ClassIntegration/Native.lean` under
# it, exactly where `import ClassIntegration.Native` resolves, and the
# `Account` class generates `Account.lean`.
echo "==> leo3-codegen $lib -> lean/"
if [ -n "${LEO3_CODEGEN:-}" ]; then
    "$LEO3_CODEGEN" "$lib" -o lean
else
    cargo run --quiet --manifest-path ../../Cargo.toml -p leo3-codegen -- \
        "$lib" -o lean
fi

echo "==> done"
