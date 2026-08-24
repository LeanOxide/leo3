#!/usr/bin/env bash
# cargo-test-full.sh — full-suite cargo test runner with flake shielding (W-350).
#
# Usage:
#   bash .github/scripts/cargo-test-full.sh <cargo test command...>
# e.g.:
#   bash .github/scripts/cargo-test-full.sh cargo test --locked --all-features --workspace
#
# Runs the given command with --no-fail-fast so one aborted test binary can
# never mask every later binary in the suite: without it, a SIGABRT in a
# single binary stops cargo from running the remaining test binaries, which
# hides real failures behind the crash (W-344 / W-350).
#
# The test threads are pinned to 8 by default (W-360; override with
# LEAN_TEST_THREADS) so libtest does not run as many threads as the host has
# cores, which can trigger an upstream Lean 4.26+ task-manager race that
# hangs a test binary.
#
# If the suite fails and the ONLY failing target died without printing a
# libtest summary (i.e. the process was killed by a signal instead of a test
# assertion failing), that target is retried in isolation up to
# $FLAKE_RETRY_LIMIT times. This covers the known Lean 4.33+ flake:
# libleanshared.so statically vendors libuv, which trips an assertion in
# uv__epoll_ctl_flush under load and aborts the test binary (the race is in
# the toolchain, not in leo3 — a clean pass in isolation is the
# reproducible signal). The abort can land on any binary that drives the
# runtime: it was first observed in test_eq_proofs (W-350), and after the
# 4.33.1 release it has also taken down other binaries (the 2026-08-23
# ubuntu/stable Full Matrix leg was red, and a rerun of the same commit
# passed — W-389), so the retry is not limited to one test name. A clean
# retry turns the run green with a warning annotation. Deterministic
# failures (failing tests, any other failing target, or a retry that fails
# again) still fail the run.
#
# The section-mapping anchors below are verified against real cargo output;
# the regression scenarios live in .github/scripts/test-cargo-test-full.sh —
# run it after editing this file.
set -uo pipefail

FLAKE_RETRY_LIMIT=2

# W-360: libtest's default --test-threads equals the core count (e.g. 80 on a
# big runner). Under that concurrency the Lean 4.26+ task manager can hit a
# worker-scaling race (upstream) in which a queued task starves behind a
# long-running task and the whole test binary hangs. Pin the test-thread
# count to keep the Lean task pool small; override with LEAN_TEST_THREADS=N.
TEST_THREADS="${LEAN_TEST_THREADS:-8}"
TEST_ARGS=()
if ! printf '%s\n' "$@" | grep -q -- "--test-threads"; then
  TEST_ARGS=(-- --test-threads "$TEST_THREADS")
fi

# Keep the original command verbatim: the retry path re-invokes it with the
# failed target's rerun hint appended.
CMD=("$@")

log_file=$(mktemp)
trap 'rm -f "$log_file"' EXIT

"${CMD[@]}" --no-fail-fast ${TEST_ARGS[@]+"${TEST_ARGS[@]}"} 2>&1 | tee "$log_file"
status=${PIPESTATUS[0]}
[ "$status" -eq 0 ] && exit 0

# Cargo prints one "to rerun pass" line per failed target, e.g.
#   error: test failed, to rerun pass '--test x'
#   error: test failed, to rerun pass `-p pkg --test x`   (workspace runs)
# (modern cargo quotes with single quotes, older ones with backticks).
# Retry only when exactly one target failed and it died without a libtest
# summary; any other co-failure means the run is red for a real reason.
failed_count=$(grep -c "to rerun pass" "$log_file")
[ "$failed_count" -eq 1 ] || exit "$status"
failed_hint=$(grep -oE "to rerun pass ['\`][^'\`]+['\`]" "$log_file" \
  | head -n1 | sed -E "s/^to rerun pass ['\`]//; s/['\`]$//")

# Split the hint into an optional `-p <pkg>` and the target selector
# (`--test NAME` | `--bin NAME` | `--example NAME` | `--bench NAME` |
# `--lib` | `--doc`).
pkg=""
target=""
set -- $failed_hint
while [ $# -gt 0 ]; do
  case "$1" in
    -p) pkg="${2-}"; shift ;;
    --test|--bin|--example|--bench) target="$1 ${2-}"; shift ;;
    --lib|--doc) target="$1" ;;
  esac
  shift
done

# Map the selector to the literal text of the log line that starts that
# target's section. Headers verified against real cargo output (see
# .github/scripts/test-cargo-test-full.sh):
#
#   --test NAME     Running tests/NAME.rs
#   --bin NAME      Running unittests src/main.rs (target/debug/deps/NAME-
#                   or Running unittests src/bin/NAME.rs (target/debug/deps/NAME-
#                   (in the deps path, NAME is cargo's artifact name: target
#                   name with dashes normalized to underscores; the source
#                   file path keeps the original target name)
#   --example NAME  Running unittests examples/NAME.rs
#   --bench NAME    Running benches/NAME.rs
#   --lib           Running unittests src/lib.rs (target/debug/deps/<stem>-
#   --doc           Doc-tests <stem>
#
# The section ends at the next "Running " or "Doc-tests " line (so summaries
# of later targets — still executed under --no-fail-fast, including
# doc-tests, which print after every integration test binary — cannot leak
# in). A binary killed by a signal never reaches libtest's "test result:"
# summary line, so the section of a flake-aborted target contains no summary.
start_text2=""
case "$target" in
  "--test "*)
    name=${target#--test }
    start_text="Running tests/${name}.rs"
    ;;
  "--bin "*)
    name=${target#--bin }
    stem=${name//-/_}
    start_text="Running unittests src/main.rs (target/debug/deps/${stem}-"
    start_text2="Running unittests src/bin/${name}.rs (target/debug/deps/${stem}-"
    ;;
  "--example "*)
    name=${target#--example }
    start_text="Running unittests examples/${name}.rs"
    ;;
  "--bench "*)
    name=${target#--bench }
    start_text="Running benches/${name}.rs"
    ;;
  --lib)
    if [ -n "$pkg" ]; then
      start_text="Running unittests src/lib.rs (target/debug/deps/${pkg//-/_}-"
    else
      start_text="Running unittests src/lib.rs"
    fi
    ;;
  --doc)
    if [ -n "$pkg" ]; then
      start_text="Doc-tests ${pkg//-/_}"
    else
      start_text="Doc-tests "
    fi
    ;;
  *)
    # Unknown target form: keep the suite failure rather than guess.
    exit "$status"
    ;;
esac

# Multiple packages can legitimately carry the same target name, and cargo's
# section header does not name the owning package, so when the anchor matches
# more than one section there is no way to tell which one belongs to the
# killed target. Keep the failure rather than check the wrong section
# (see the multi-same-name scenario in test-cargo-test-full.sh).
anchor_count=$(grep -cF -- "$start_text" "$log_file")
if [ -n "$start_text2" ]; then
  anchor_count=$((anchor_count + $(grep -cF -- "$start_text2" "$log_file")))
fi
[ "$anchor_count" -le 1 ] || exit "$status"

section=$(awk -v s="$start_text" -v s2="$start_text2" '
  !f && (index($0, s) || (s2 != "" && index($0, s2))) { f = 1; next }
  f && ($0 ~ /^ *Running / || $0 ~ /^ *Doc-tests /) { exit }
  f { print }
' "$log_file")
if [ -z "$section" ] || printf '%s\n' "$section" | grep -q "test result:"; then
  exit "$status"
fi

# Flake candidate: the sole failing target died by signal. Retry it in
# isolation; a clean retry means the abort was the toolchain race.
echo "::warning::target '${failed_hint}' died without a libtest summary (W-350 family: Lean vendored-libuv abort under load, e.g. uv__epoll_ctl_flush in Lean 4.33+ libleanshared.so); retrying in isolation up to ${FLAKE_RETRY_LIMIT}x"
for _ in $(seq "$FLAKE_RETRY_LIMIT"); do
  # $failed_hint is word-split on purpose (it is cargo's own rerun
  # arguments); its tokens contain no glob metacharacters.
  if "${CMD[@]}" $failed_hint ${TEST_ARGS[@]+"${TEST_ARGS[@]}"}; then
    echo "::warning::target '${failed_hint}' passed on retry; the original abort was a flake, not a leo3 regression"
    exit 0
  fi
  echo "::warning::target '${failed_hint}' retry failed; keeping the suite failure"
done
exit "$status"