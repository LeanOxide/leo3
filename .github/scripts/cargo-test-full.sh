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
# If the suite fails and the ONLY failing target is `test_eq_proofs`, and it
# died without printing a libtest summary (i.e. the process was killed by a
# signal instead of a test assertion failing), that binary is retried in
# isolation up to $FLAKE_RETRY_LIMIT times. This covers the known Lean 4.33
# flake: libleanshared.so statically vendors libuv, which trips an assertion
# in uv__epoll_ctl_flush under load and aborts the test binary (the race is
# in the toolchain, not in leo3 — a clean pass in isolation is the
# reproducible signal). A clean retry turns the run green with a warning
# annotation. Deterministic failures (failing tests, any other failing
# target, or a retry that fails again) still fail the run.
set -uo pipefail

FLAKY_TEST=test_eq_proofs
FLAKE_RETRY_LIMIT=2

log_file=$(mktemp)
trap 'rm -f "$log_file"' EXIT

"$@" --no-fail-fast 2>&1 | tee "$log_file"
status=${PIPESTATUS[0]}
[ "$status" -eq 0 ] && exit 0

# Cargo prints one "to rerun pass" line per failed target, e.g.
#   error: test failed, to rerun pass '--test x'
#   error: test failed, to rerun pass `-p pkg --test x`   (workspace runs)
# (modern cargo quotes with single quotes, older ones with backticks).
# Retry only when exactly one target failed and it is the flaky binary;
# any other co-failure means the run is red for a real reason.
failed_count=$(grep -c "to rerun pass" "$log_file")
[ "$failed_count" -eq 1 ] || exit "$status"
failed_hint=$(grep -oE "to rerun pass ['\`][^'\`]+['\`]" "$log_file" \
  | head -n1 | sed -E "s/^to rerun pass ['\`]//; s/['\`]$//")
case "$failed_hint" in
  *"--test $FLAKY_TEST") ;;
  *) exit "$status" ;;
esac

# A binary killed by a signal never reaches libtest's "test result:" summary
# line; a binary whose tests merely fail prints one. Only the former is a
# flake candidate. The section ends at the next "Running " line so summaries
# of later binaries (still executed under --no-fail-fast) cannot leak in.
section=$(awk -v t="tests/${FLAKY_TEST}.rs" '
  index($0, "Running " t) { f = 1; next }
  f && $0 ~ /^ *Running / { exit }
  f { print }
' "$log_file")
if printf '%s\n' "$section" | grep -q "test result:"; then
  exit "$status"
fi

echo "::warning::${FLAKY_TEST} aborted without a libtest summary (W-350: Lean 4.33 vendored-libuv flake candidate); retrying up to ${FLAKE_RETRY_LIMIT}x"
for _ in $(seq "$FLAKE_RETRY_LIMIT"); do
  if "$@" --test "$FLAKY_TEST"; then
    echo "::warning::${FLAKY_TEST} passed on retry; the original abort was the known W-350 vendored-libuv flake (uv__epoll_ctl_flush assertion in Lean 4.33 libleanshared.so)"
    exit 0
  fi
  echo "::warning::${FLAKY_TEST} retry failed; keeping the suite failure"
done
exit "$status"