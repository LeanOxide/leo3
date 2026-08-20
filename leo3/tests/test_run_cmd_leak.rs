//! Regression test for W-351: `run_command` must not pin Lean objects on
//! the heap from one call to the next.
//!
//! This is a **standalone test binary** on purpose: the metric below
//! observes the base `Environment` object of one Lean session, and any
//! other test sharing the process would interleave its allocations on
//! the same worker and contaminate the measured delta.
//!
//! ## Metric: base-Environment refcount (primary)
//!
//! The W-351 leak pinned one per-call object graph per `run_command`
//! call: the temporary `ST.Ref` carrying the post-command state was
//! never dec'd, and the per-call initial `Command.State` (built from
//! the input environment) held two extra pins. Because every call in
//! this test starts from the same base environment (the imported env,
//! never fed back), each pinned per-call initial state adds one
//! reference to that same base `Environment` object. Reading its
//! header refcount (`lean_object::m_rc`) before and after N calls
//! therefore measures the pinned state count directly: +1 per leaked
//! call, flat when healthy. This is the same metric and command as the
//! 2000-call review probe (`axiom X_N : Nat`; base-env rc: pre-fix
//! 23 → 2023 on both toolchains, fixed 3 → 3).
//!
//! ## Why not mimalloc's `mi_stats_t`?
//!
//! The toolchain `libleanshared.so` embeds mimalloc v2.2.3 built as an
//! NDEBUG release, i.e. with `MI_STAT=1`:
//! - the per-size-class `malloc_bins` counters are maintained only when
//!   `MI_STAT > 1` (the increment/decrement sites are compiled out), so
//!   every bin field reads 0 — there is no per-class live count to sum;
//! - the remaining `current` fields mix per-thread net values (each
//!   thread's TLD is only visible after a merge) with the arena layer's
//!   direct global updates, and in this arena-based build they drift:
//!   observed +716 MiB of "live" `malloc_huge` bytes over 100 calls
//!   while `committed_total` and RSS stayed flat. They are not a
//!   reliable live-object count.
//!
//! The refcount probe is direct per-object evidence for the exact leak
//! this issue guards against.
//!
//! ## Metric: RSS (secondary backstop, Linux)
//!
//! Process RSS must not grow without bound either. The threshold is set
//! far above allocator/GC noise and far below any gross per-call
//! growth.
//!
//! ## Known residual (tracked in W-359, not asserted here)
//!
//! On 4.33 (and other 4.26+ frontends), per-command state is retained in
//! the Lean session: RSS grows ~26-86 KiB/call depending on command
//! kind (base-env rc flat), and per-call environment objects keep ~5
//! extra references after all calls finish. W-359 ruled out the stock
//! frontend as the cause — vanilla 4.33.0-rc1 in the identical loop is
//! clean at ~0.2-0.4 KiB/call, and 4.25.2 shows no env-level retention
//! even under the leo3 runtime — so the retention needs a 4.26+
//! frontend combined with the leo3 task-manager/worker context, and the
//! fix belongs on the leo3 side. See W-359 and its probes
//! (`test_w359_registry_probe.rs`, `test_w359_finalize_probe.rs`,
//! `tests/data/w359_vanilla.lean`).

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    not(target_os = "windows")
))]

use leo3::meta::*;
use leo3::prelude::*;

#[cfg(target_os = "linux")]
fn rss_bytes() -> u64 {
    let s = std::fs::read_to_string("/proc/self/statm").expect("statm");
    let resident_pages: u64 = s.split_whitespace().nth(1).unwrap().parse().unwrap();
    resident_pages * 4096
}

#[cfg(not(target_os = "linux"))]
fn rss_bytes() -> u64 {
    0
}

/// Read the base-Environment refcount and process RSS on the Lean
/// worker thread (the environment object is owned by the worker's
/// context; its header must be read from that thread).
fn snapshot(env: *mut leo3::ffi::object::lean_object) -> (i32, u64) {
    leo3::run_worker(|| unsafe {
        let rc = (*env).m_rc;
        (rc, rss_bytes())
    })
}

#[test]
fn test_run_cmd_no_object_leak_across_calls() {
    const WARMUP: u32 = 10;
    const ITERS: u32 = 100;
    // The pre-fix build pinned one base-environment reference per call
    // (+100 over 100 calls); a healthy run is flat. Allow a few units
    // of churn for benign transient refs.
    const MAX_RC_GROWTH: i32 = 5;
    // RSS backstop: an order of magnitude of headroom over the 4.33
    // per-command registry slope (~2 MiB per 100 calls) to absorb
    // allocator/GC noise, while still catching gross per-call growth
    // (the pre-fix build also grew ~1.9 MiB per 100 calls on 4.25.2,
    // which the primary refcount assertion catches on both toolchains).
    const MAX_RSS_GROWTH: u64 = 32 * 1024 * 1024;

    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let metam = MetaMContext::new(lean, env)?;
        let env_ptr = metam.env().as_ptr();
        let run = |cmd: &str| -> LeanResult<()> {
            let stx = leo3::meta::repl::parse_command(lean, metam.env(), cmd)?;
            leo3::meta::repl::run_command(lean, &metam, &stx).map(|_| ())
        };
        // Warm up: first calls pay one-time costs (module cache,
        // arenas, worker-thread lazy init) that must not count toward
        // the delta.
        for i in 0..WARMUP {
            run(&format!("axiom W{i} : Nat"))?;
        }
        let (rc_before, rss_before) = snapshot(env_ptr);
        for i in 0..ITERS {
            run(&format!("axiom X{i} : Nat"))?;
        }
        let (rc_after, rss_after) = snapshot(env_ptr);
        let rc_growth = rc_after - rc_before;
        let rss_growth = rss_after.saturating_sub(rss_before);
        eprintln!(
            "[leak-probe] {ITERS} run_command calls: base-env rc {rc_before} -> {rc_after} \
             (growth {rc_growth}); RSS {rss_before} -> {rss_after} (growth {rss_growth})"
        );
        assert!(
            rc_growth <= MAX_RC_GROWTH,
            "base Environment refcount grew by {rc_growth} over {ITERS} run_command calls \
             (a per-call ST.Ref / Command.State pin, W-351)"
        );
        assert!(
            rss_growth <= MAX_RSS_GROWTH,
            "process RSS grew by {rss_growth} bytes over {ITERS} run_command calls \
             (unbounded per-call growth)"
        );
        Ok(())
    });
    result.expect("run_command leak probe failed");
}
