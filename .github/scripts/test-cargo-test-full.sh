#!/usr/bin/env bash
# test-cargo-test-full.sh — regression scenarios for the flake-shield
# section mapping in cargo-test-full.sh (W-350 / W-389).
#
# Runs the real script against a fake `cargo` that emits captured cargo log
# lines. Each scenario asserts the script's exit code and whether the
# retry path was entered. Log bodies mirror real cargo section headers
# (verified with a scratch crate; cargo prints these exact lines):
#
#   --lib          Running unittests src/lib.rs (target/debug/deps/<stem>-<hash>)
#   --bin NAME     Running unittests src/main.rs (target/debug/deps/<name>-<hash>)
#                  Running unittests src/bin/NAME.rs (target/debug/deps/<name>-<hash>)
#   --example NAME Running unittests examples/NAME.rs (target/debug/examples/<name>-<hash>)
#   --bench NAME   Running benches/NAME.rs (target/debug/build/<pkg>/<hash>/out/<name>-<hash>)
#   --test NAME    Running tests/NAME.rs (target/debug/deps/<name>-<hash>)
#   --doc          Doc-tests <stem>
#
# Run: bash .github/scripts/test-cargo-test-full.sh
set -uo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
target_script="$script_dir/cargo-test-full.sh"
[ -f "$target_script" ] || { echo "missing $target_script"; exit 1; }

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT

failures=0

# --- fake cargo ------------------------------------------------------------
# A full run cats full.log and exits $FULL_EXIT. A run carrying a bare
# --test/--bin/--example/--bench/--lib/--doc token (the retry invocation)
# cats retry.log and exits $RETRY_EXIT. Token matching is word-exact, so the
# --test-threads argument cargo-test-full.sh appends never counts as a retry
# flag.
cat > "$workdir/fake-cargo" <<'FAKE'
#!/usr/bin/env bash
set -u
RETRY=""
for a in "$@"; do
  case "$a" in
    --test|--bin|--example|--bench|--lib|--doc) RETRY="$a" ;;
  esac
done
if [ -n "$RETRY" ]; then
  cat "$FAKE_LOGS/retry.log"
  exit "${RETRY_EXIT:-0}"
fi
cat "$FAKE_LOGS/full.log"
exit "${FULL_EXIT:-0}"
FAKE
chmod +x "$workdir/fake-cargo"

# Scenario data directories (created up front: the log bodies below are
# written with heredocs that cannot create directories).
for s in pass flake-test flake-test-still flake-deterministic multi-fail \
         compile-fail flake-lib-workspace flake-doc flake-example \
         flake-bin-main flake-bin-named flake-bench; do
  mkdir -p "$workdir/$s"
done

# scenario <name> <expect-exit: 0|nonzero> <expect-retry: yes|no>
# <full-exit> <retry-exit>
# Reads $workdir/<name>/full.log (required) and $workdir/<name>/retry.log
# (only used when the retry path is entered).
scenario() {
  local name=$1 expect_exit=$2 expect_retry=$3 full_exit=$4 retry_exit=$5
  local logs="$workdir/$name"
  if [ ! -f "$logs/full.log" ]; then
    echo "FAIL $name: missing full.log"
    failures=$((failures + 1))
    return
  fi
  FAKE_LOGS="$logs" FULL_EXIT="$full_exit" RETRY_EXIT="$retry_exit" \
    bash "$target_script" "$workdir/fake-cargo" test --locked --all-features --workspace \
    > "$logs/out.log" 2>&1
  local got=$?
  local ok=1
  if [ "$expect_exit" = 0 ]; then
    [ "$got" -eq 0 ] || ok=0
  else
    [ "$got" -ne 0 ] || ok=0
  fi
  local warned=no
  grep -q "retrying in isolation" "$logs/out.log" && warned=yes
  [ "$warned" = "$expect_retry" ] || ok=0
  if [ "$ok" -eq 1 ]; then
    echo "PASS $name (exit=$got retry=$warned)"
  else
    echo "FAIL $name: exit=$got (want $expect_exit) retry=$warned (want $expect_retry)"
    failures=$((failures + 1))
  fi
}

# --- scenarios --------------------------------------------------------------
# A killed target's section has NO "test result:" summary line; in every
# flake scenario a succeeding target's section (with summary) follows the
# killed one, so section slicing must stop at the next "Running "/"Doc-tests "
# line instead of swallowing the next target's summary.

# pass: everything green — no shield involvement.
cat > "$workdir/pass/full.log" <<'LOG'
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.42s
     Running unittests src/lib.rs (target/debug/deps/leo3-8993cf4275ab8e71)
running 1 test
test core::t1 ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running tests/basic.rs (target/debug/deps/basic-0011223344556677)
running 1 test
test smoke ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests leo3

running 1 test
test src/lib.rs - module (line 1) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
LOG

# flake-test: sole --test target signal-killed, next test target succeeds.
cat > "$workdir/flake-test/full.log" <<'LOG'
     Running tests/test_gc.rs (target/debug/deps/test_gc-77889900aabbccdd)
running 12 tests
test gc::stress ... thread 'gc::stress' (12345) has exited with signal 6 (SIGABRT)

     Running tests/string_ops.rs (target/debug/deps/string_ops-1122334455667788)
running 3 tests
test s1 ... ok
test s2 ... ok
test s3 ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass '--test test_gc'
LOG
cat > "$workdir/flake-test/retry.log" <<'LOG'
     Running tests/test_gc.rs (target/debug/deps/test_gc-77889900aabbccdd)
running 12 tests
test gc::stress ... ok

test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.42s
LOG

# flake-test-still: same, but the retry is also signal-killed — stays red.
cat > "$workdir/flake-test-still/full.log" <<'LOG'
     Running tests/test_gc.rs (target/debug/deps/test_gc-77889900aabbccdd)
running 12 tests
test gc::stress ... thread 'gc::stress' (12345) has exited with signal 6 (SIGABRT)

     Running tests/string_ops.rs (target/debug/deps/string_ops-1122334455667788)
running 3 tests
test s1 ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass '--test test_gc'
LOG
cat > "$workdir/flake-test-still/retry.log" <<'LOG'
     Running tests/test_gc.rs (target/debug/deps/test_gc-77889900aabbccdd)
running 12 tests
test gc::stress ... thread 'gc::stress' (12345) has exited with signal 6 (SIGABRT)
LOG

# flake-deterministic: a real assertion failure (summary present) — no retry.
cat > "$workdir/flake-deterministic/full.log" <<'LOG'
     Running tests/test_gc.rs (target/debug/deps/test_gc-77889900aabbccdd)
running 2 tests
test gc::ok_case ... ok
test gc::panic_case ... FAILED

failures:

---- gc::panic_case stdout ----
thread 'gc::panic_case' panicked at assertion failed: false
note: panic did not occur as expected

failures:
    gc::panic_case

test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass '--test test_gc'
LOG

# multi-fail: two failing targets — never a single-target flake, no retry.
cat > "$workdir/multi-fail/full.log" <<'LOG'
     Running tests/test_gc.rs (target/debug/deps/test_gc-77889900aabbccdd)
running 1 test
test gc::x ... thread 'gc::x' (12345) has exited with signal 6 (SIGABRT)

     Running tests/string_ops.rs (target/debug/deps/string_ops-1122334455667788)
running 1 test
test s1 ... FAILED

test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass '--test test_gc'
error: test failed, to rerun pass '--test string_ops'
LOG

# compile-fail: no "to rerun pass" at all — no retry.
cat > "$workdir/compile-fail/full.log" <<'LOG'
error[E0308]: mismatched types
 --> crates/leo3/tests/bad.rs:2:5

error: could not compile `leo3` due to 1 previous error
LOG

# flake-lib-workspace: `-p pkg --lib` signal-killed; an earlier package's
# lib section (succeeded) must not be matched by the stem anchor, and the
# succeeding test section after the killed one must not leak in.
cat > "$workdir/flake-lib-workspace/full.log" <<'LOG'
     Running unittests src/lib.rs (target/debug/deps/leo3-8993cf4275ab8e71)
running 1 test
test a ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

     Running unittests src/lib.rs (target/debug/deps/leo3_ffi-d79dd2a4cd45fa1c)
running 2 tests
test ffi::load ... thread 'ffi::load' (999) has exited with signal 6 (SIGABRT)

     Running tests/basic.rs (target/debug/deps/basic-0011223344556677)
running 1 test
test smoke ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass `-p leo3-ffi --lib`
LOG
cat > "$workdir/flake-lib-workspace/retry.log" <<'LOG'
     Running unittests src/lib.rs (target/debug/deps/leo3_ffi-d79dd2a4cd45fa1c)
running 2 tests
test ffi::load ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
LOG

# flake-doc: Doc-tests section signal-killed, followed by a succeeding
# Doc-tests section (boundary must stop at the next "Doc-tests " line).
cat > "$workdir/flake-doc/full.log" <<'LOG'
   Doc-tests leo3_ffi

thread 'main' (888) has exited with signal 6 (SIGABRT)

   Doc-tests leo3

running 1 test
test src/lib.rs - module (line 1) ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass `-p leo3-ffi --doc`
LOG
cat > "$workdir/flake-doc/retry.log" <<'LOG'
   Doc-tests leo3_ffi

running 3 tests
test src/ffi.rs - ffi (line 10) ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
LOG

# flake-example: --example target signal-killed (anchor is "Running
# unittests examples/NAME.rs" — real cargo's header, NOT "Running
# examples/NAME.rs"), next target succeeds.
cat > "$workdir/flake-example/full.log" <<'LOG'
     Running unittests examples/macro_pipeline.rs (target/debug/examples/macro_pipeline-22ae3c469fd984b3)
running 2 tests
test pipe ... thread 'pipe' (777) has exited with signal 6 (SIGABRT)

     Running unittests src/lib.rs (target/debug/deps/leo3-8993cf4275ab8e71)
running 1 test
test a ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass '--example macro_pipeline'
LOG
cat > "$workdir/flake-example/retry.log" <<'LOG'
     Running unittests examples/macro_pipeline.rs (target/debug/examples/macro_pipeline-22ae3c469fd984b3)
running 2 tests
test pipe ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
LOG

# flake-bin-main: default bin (src/main.rs) signal-killed, next target
# succeeds — the stem anchor must hit "src/main.rs (target/debug/deps/NAME-".
cat > "$workdir/flake-bin-main/full.log" <<'LOG'
     Running unittests src/main.rs (target/debug/deps/demo-3169e5b5e09fb408)
running 1 test
test main_test ... thread 'main_test' (666) has exited with signal 6 (SIGABRT)

     Running unittests src/lib.rs (target/debug/deps/demo-5d464ed6742d1409)
running 1 test
test lib_test ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass '--bin demo'
LOG
cat > "$workdir/flake-bin-main/retry.log" <<'LOG'
     Running unittests src/main.rs (target/debug/deps/demo-3169e5b5e09fb408)
running 1 test
test main_test ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
LOG

# flake-bin-named: named bin (src/bin/NAME.rs) signal-killed, next target
# succeeds — second anchor variant.
cat > "$workdir/flake-bin-named/full.log" <<'LOG'
     Running unittests src/bin/subbin.rs (target/debug/deps/subbin-729d5a3fa5d60d62)
running 1 test
test sub_test ... thread 'sub_test' (555) has exited with signal 6 (SIGABRT)

     Running unittests src/main.rs (target/debug/deps/demo-3169e5b5e09fb408)
running 1 test
test main_test ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass '--bin subbin'
LOG
cat > "$workdir/flake-bin-named/retry.log" <<'LOG'
     Running unittests src/bin/subbin.rs (target/debug/deps/subbin-729d5a3fa5d60d62)
running 1 test
test sub_test ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
LOG

# flake-bench: bench target signal-killed (note the build/<pkg>/<hash>/out
# path in real cargo output — the anchor is the stable file-path part), next
# target succeeds.
cat > "$workdir/flake-bench/full.log" <<'LOG'
     Running benches/demobench.rs (target/debug/build/demo/16c000905443a2bd/out/demobench-16c000905443a2bd)
running 1 test
test bench_smoke ... thread 'bench_smoke' (444) has exited with signal 6 (SIGABRT)

     Running tests/basic.rs (target/debug/deps/basic-0011223344556677)
running 1 test
test smoke ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

error: test failed, to rerun pass '--bench demobench'
LOG
cat > "$workdir/flake-bench/retry.log" <<'LOG'
     Running benches/demobench.rs (target/debug/build/demo/16c000905443a2bd/out/demobench-16c000905443a2bd)
running 1 test
test bench_smoke ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
LOG

# --- run -------------------------------------------------------------------
scenario pass                 0       no    0    0
scenario flake-test           0       yes   101  0
scenario flake-test-still     nonzero yes   101  134
scenario flake-deterministic  nonzero no    101  0
scenario multi-fail           nonzero no    101  0
scenario compile-fail         nonzero no    101  0
scenario flake-lib-workspace  0       yes   101  0
scenario flake-doc            0       yes   101  0
scenario flake-example        0       yes   101  0
scenario flake-bin-main       0       yes   101  0
scenario flake-bin-named      0       yes   101  0
scenario flake-bench          0       yes   101  0

echo
if [ "$failures" -gt 0 ]; then
  echo "$failures scenario(s) FAILED"
  exit 1
fi
echo "all scenarios passed"