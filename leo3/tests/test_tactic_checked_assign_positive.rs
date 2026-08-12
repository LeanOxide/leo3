//! Integration test: the checked-assignment (exact) SUCCESS path.
//!
//! Previously only failure paths were tested (`exact` on an unregistered
//! metavariable). The success path used to corrupt the Lean heap via
//! `MetaM.run` (run_persistent); it now routes through `MetaM.run'` and
//! must complete a full goal/assign round-trip plus fresh environment and
//! metavariable creation afterwards without crashing.

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    not(target_os = "windows")
))]

use leo3::meta::*;
use leo3::prelude::*;

/// Goal type: `∀ (P : Prop), P → P` (a Pi type whose body is the
/// non-dependent arrow `P → P` — de Bruijn 1 for `P` inside the inner
/// binder).
fn forall_pp<'l>(
    lean: Lean<'l>,
    prop: &LeanBound<'l, LeanExpr>,
) -> LeanResult<LeanBound<'l, LeanExpr>> {
    let p_name = LeanName::from_str(lean, "P")?;
    let h_name = LeanName::from_str(lean, "h")?;
    let bvar0 = LeanExpr::bvar(lean, 0)?;
    let bvar1_body = LeanExpr::bvar(lean, 1)?;
    let inner = LeanExpr::forall(h_name, bvar0, bvar1_body, BinderInfo::Default)?;
    LeanExpr::forall(p_name, prop.clone(), inner, BinderInfo::Default)
}

/// Proof term: `fun (P : Prop) (h : P) => h` (body is de Bruijn 0 = `h`).
fn proof_term<'l>(
    lean: Lean<'l>,
    prop: &LeanBound<'l, LeanExpr>,
) -> LeanResult<LeanBound<'l, LeanExpr>> {
    let p_name = LeanName::from_str(lean, "P")?;
    let h_name = LeanName::from_str(lean, "h")?;
    let bvar0 = LeanExpr::bvar(lean, 0)?;
    let bvar0_body = LeanExpr::bvar(lean, 0)?;
    let inner = LeanExpr::lambda(h_name, bvar0, bvar0_body, BinderInfo::Default)?;
    LeanExpr::lambda(p_name, prop.clone(), inner, BinderInfo::Default)
}

/// Regression test for the checked-assignment success path.
///
/// `MetaM.run` (the previous run_persistent backend) corrupts the Lean heap
/// when a computation assigns a metavariable; the backend now uses
/// `MetaM.toIO`, which returns the final `Core.State`/`Meta.State` directly
/// and is the entry point Lean's own `runMetaM` uses.
#[test]
fn test_exact_success_roundtrip_does_not_corrupt_heap() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut metam = MetaMContext::new(lean, env)?;

        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let proof_ty = forall_pp(lean, &prop)?;
        let proof = proof_term(lean, &prop)?;

        // Type-check the proof against the goal type first.
        assert!(metam.is_def_eq(&proof_ty, &proof_ty)?);
        let inferred = metam.infer_type(&proof)?;
        assert!(metam.is_def_eq(&inferred, &proof_ty)?);

        // Register the goal and close it with exact (checked assignment).
        let goal = metam.mk_goal(&proof_ty)?;
        let state = TacticState::new(vec![goal]);
        let result = exact(&mut metam, state, &proof);
        assert!(
            matches!(result, TacticResult::Success(_)),
            "exact should succeed on a matching proof term"
        );

        // The heap must stay intact: fresh environments and metavariables
        // still work after the assignment.
        let env3 = LeanEnvironment::empty(lean, 0)?;
        let _ = env3;
        let goal2 = metam.mk_goal(&prop)?;
        let ty2 = goal_type(&mut metam, &goal2)?;
        let _ = ty2;
        Ok(())
    });
    assert!(result.is_ok(), "test failed: {:?}", result.err());
}

#[test]
fn test_exact_failure_path_still_rejects_wrong_type() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut metam = MetaMContext::new(lean, env)?;

        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let proof = proof_term(lean, &prop)?;

        // A goal of a *different* type must be rejected before assignment.
        let goal = metam.mk_goal(&prop)?;
        let state = TacticState::new(vec![goal]);
        let result = exact(&mut metam, state, &proof);
        assert!(matches!(result, TacticResult::Failure(_)));
        Ok(())
    });
    assert!(result.is_ok(), "test failed: {:?}", result.err());
}
