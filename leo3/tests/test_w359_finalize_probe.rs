//! W-359 standalone probe: does `lean_finalize_task_manager()` release the
//! per-call env retention?
//!
//! Runs `run_command("axiom ...")` N times from the same base environment
//! (keeping every per-call env alive), re-reads the per-call env
//! refcounts, then finalizes the Lean task manager mid-process and
//! re-reads them again. If the task manager's global task registry were
//! the retainer, finalization would drop the refcounts back to the
//! observer-only value.
//!
//! This probe MUST live in its own test binary (standalone-binary
//! convention, see the header of `test_run_cmd_leak.rs`): finalization is
//! a process-global, one-way operation. Sharing a process with the other
//! W-359 probes would race finalization against in-flight tasks (UB) and
//! contaminate every process-level measurement (RSS/refcounts) in that
//! binary — the suite runs with `--test-threads=8`.
//!
//! Diagnostic, not a regression assertion. Run with:
//!   cargo test --features "meta runtime-tests" --test test_w359_finalize_probe -- --nocapture
//! (one test in one binary: no in-process concurrency)

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

type Env<'l> = LeanBound<'l, LeanEnvironment>;

/// Layout guard (duplicated from `test_w359_registry_probe.rs`; test
/// binaries are separate crates): the probe re-reads 4.26+
/// `Environment` refcounts, which are only meaningful on the 4.26+
/// frontend layout (8 object fields, struct tag 0; e.g. v4.20.0 has no
/// `checked` field). Prints a reason and skips otherwise.
fn frontend_env_layout_ok(e: *const leo3::ffi::object::lean_object, probe: &str) -> bool {
    leo3::run_worker(|| unsafe {
        if (*e).m_other == 8 && (*e).m_tag == 0 {
            true
        } else {
            eprintln!(
                "[w359] {probe}: skip — frontend Environment has \
                 m_other={} m_tag={}; assumed 8/0 (4.26+ layout)",
                (*e).m_other,
                (*e).m_tag
            );
            false
        }
    })
}

#[test]
fn probe_task_manager_finalize() {
    const ITERS: u32 = 50;

    leo3::test_with_lean(|lean: Lean<'_>| -> LeanResult<()> {
        let env = import_modules(lean, &["Lean"], 0)?;
        let metam = MetaMContext::new(lean, env)?;
        let base_ptr = metam.env().as_ptr();
        if !frontend_env_layout_ok(base_ptr, "probe_task_manager_finalize") {
            return Ok(());
        }
        let run = |cmd: &str| -> LeanResult<Env<'_>> {
            let stx = leo3::meta::repl::parse_command(lean, metam.env(), cmd)?;
            leo3::meta::repl::run_command(lean, &metam, &stx)
        };

        // Session warm-up.
        for i in 0..5 {
            run(&format!("axiom W{i} : Nat"))?;
        }

        let mut envs: Vec<Env<'_>> = Vec::with_capacity(ITERS as usize);
        for i in 0..ITERS {
            envs.push(run(&format!("axiom F{i} : Nat"))?);
        }

        let rc_of = |envs: &Vec<Env<'_>>| -> (Option<i32>, Option<i32>) {
            leo3::run_worker(|| unsafe {
                let rcs: Vec<i32> = envs.iter().map(|e| (*e.as_ptr()).m_rc).collect();
                (rcs.iter().min().copied(), rcs.iter().max().copied())
            })
        };
        let rss = || leo3::run_worker(rss_bytes);

        let (amin, amax) = rc_of(&envs);
        let base_rc = leo3::run_worker(|| unsafe { (*base_ptr).m_rc });
        let rss0 = rss();
        eprintln!(
            "[w359-finalize] before finalize: {ITERS} axioms, per-call env rc \
             min={amin:?} max={amax:?}; base rc={base_rc}; RSS={rss0}"
        );

        // One-way, process-global: must be the last Lean operation in
        // this binary.
        eprintln!("[w359-finalize] finalizing task manager ...");
        leo3::run_worker(|| unsafe {
            leo3::ffi::closure::lean_finalize_task_manager();
        });
        let (amin2, amax2) = rc_of(&envs);
        let rss1 = rss();
        eprintln!(
            "[w359-finalize] after finalize: per-call env rc min={amin2:?} \
             max={amax2:?} (back to 1 = the task manager was the retainer); \
             RSS={rss1} (delta {} bytes)",
            rss1 as i64 - rss0 as i64
        );
        Ok(())
    })
    .expect("probe failed");
}
