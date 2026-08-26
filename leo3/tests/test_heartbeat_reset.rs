//! W-407 Bug B regression: the worker thread's heartbeat baseline must be
//! reset at each CoreM command entry point.
//!
//! Lean's heartbeat counter (the small-allocation counter) is thread-local
//! and accumulates monotonically across every command run on the worker
//! thread. The monadic FFI entry points used by the `meta` module
//! (`Lean.Elab.runTactic`, `Lean.Meta.ppGoal`/`ppExpr`,
//! `Lean.Elab.Command.elabCommandTopLevel`, `MetaM.run'`) do not go through
//! `CoreM.toIO` — the only place that snapshots the baseline
//! (`initHeartbeats := (← IO.getNumHeartbeats)`) — so unless the entry
//! resets the counter, `Core.checkMaxHeartbeatsCore` measures the
//! *process-wide* allocation count against `maxHeartbeats`
//! (200000 × 1000) instead of a single command's. A trivial tactic then
//! deterministically "times out" once enough prior work has run: each
//! `import Modules` of `Lean` alone costs ~3.56M heartbeats, so a
//! LeanDojo-style loop of fresh `Repl()` sessions crossed the 200M limit
//! after ~56 imports (W-407 repro: `repro_w407.py` failed at i=27).
//!
//! This test pushes the worker's counter past the limit and then runs a
//! trivial tactic: without the entry reset this returns a `maxHeartbeats`
//! error, with the reset it succeeds.
//!
//! Concurrency note: the counter is one shared worker-thread global and
//! other tests' command entries can reset it at any time. That can only
//! *lower* the counter, which can never cause a spurious timeout, so this
//! test never false-fails. Run this test file in isolation
//! (`cargo test --test test_heartbeat_reset`) for a deterministic
//! demonstration that it fails without the reset.

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    not(target_os = "windows"),
    lean_4_25
))]

use leo3::meta::*;
use leo3::prelude::*;

/// `maxHeartbeats` (200000) × 1000 — the per-command small-allocation
/// budget that `checkMaxHeartbeatsCore` enforces.
const MAX_HEARTBEATS: u64 = 200_000_000;

/// `∀ n : Nat, Nat = Nat` — a trivial goal that `intro n` advances.
fn forall_nat_eq_nat<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
    let nat = LeanExpr::const_(lean, LeanName::from_str(lean, "Nat")?, LeanList::nil(lean)?)?;
    let body = LeanExpr::mk_eq(
        lean,
        LeanList::cons(LeanLevel::one(lean)?.cast(), LeanList::nil(lean)?)?,
        &nat,
        &nat,
        &nat,
    )?;
    LeanExpr::forall(
        LeanName::from_str(lean, "n")?,
        nat,
        body,
        BinderInfo::Default,
    )
}

/// Push the worker thread's heartbeat counter past `maxHeartbeats`,
/// simulating a process that has already run many commands (each
/// `import Lean` alone costs ~3.56M heartbeats).
fn push_heartbeats_past_limit() {
    leo3::run_worker(|| unsafe {
        let count = leo3::ffi::lean_box((MAX_HEARTBEATS + 50_000_000) as usize);
        #[cfg(not(lean_4_26))]
        {
            let world = leo3::ffi::io::lean_io_mk_world();
            let result = leo3::ffi::io::lean_io_set_heartbeats(count, world);
            // `set_heartbeats` decs `count`; `world` is ignored (the
            // result carries its own fresh token), so release it here.
            leo3::ffi::lean_dec(world);
            leo3::ffi::lean_dec(result);
        }
        #[cfg(lean_4_26)]
        // Lean >= 4.26 (ST redesign): no world token; the return is a raw
        // unit scalar (never dec'd), and the callee decs `count`.
        leo3::ffi::io::lean_io_set_heartbeats(count);
    });
}

/// A trivial tactic must not report a spurious `maxHeartbeats` timeout
/// after the worker's counter has already crossed the limit — the
/// `runTactic` entry resets the baseline, so the check measures only this
/// command's allocations (W-407 Bug B).
#[test]
fn test_run_tactic_resets_heartbeat_baseline() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let mut metam = MetaMContext::new(lean, env)?;
        let ty = forall_nat_eq_nat(lean)?;
        let goal = metam.mk_goal(&ty)?;
        let mvar = LeanExpr::mvar_id(&goal)?;

        // Simulate the accumulated process-wide allocation count of a
        // long-running REPL session.
        push_heartbeats_past_limit();

        // `intro n` must succeed: its `maxHeartbeats` check runs against
        // the reset counter, not the 250M+ process-wide count.
        let stx = parse_tactic(lean, metam.env(), "intro n")?;
        let outcome = run_tactic(&mut metam, &mvar, &stx, None).map_err(|e| {
            LeanError::other(&format!(
                "run_tactic after crossing the heartbeat limit: {e}"
            ))
        })?;
        assert_eq!(outcome.goals.len(), 1, "intro n leaves one goal");
        Ok(())
    });
    result.unwrap();
}
