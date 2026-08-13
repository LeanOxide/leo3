//! Repro for the goal pretty-printing path: `pp_expr`, `pp_goal`, and
//! `goal_hyps_and_type_pp` must render user-facing names without crashing.

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    not(target_os = "windows")
))]

use leo3::meta::*;
use leo3::prelude::*;

fn add_comm_type<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
    let nat = LeanExpr::const_(lean, LeanName::from_str(lean, "Nat")?, LeanList::nil(lean)?)?;
    let lv0 = LeanLevel::zero(lean)?;
    let levels =
        LeanList::cons(lv0.clone().cast(), LeanList::cons(lv0.clone().cast(), LeanList::cons(lv0.cast(), LeanList::nil(lean)?)?)?)?;
    let hadd = LeanExpr::const_(lean, LeanName::from_components(lean, "HAdd.hAdd")?, levels)?;
    let inst_hadd = LeanExpr::const_(lean, LeanName::from_components(lean, "instHAdd")?, LeanList::cons(LeanLevel::zero(lean)?.cast(), LeanList::nil(lean)?)?)?;
    let inst_add_nat = LeanExpr::const_(lean, LeanName::from_components(lean, "instAddNat")?, LeanList::nil(lean)?)?;
    let inst = LeanExpr::app(&inst_hadd, &nat)?;
    let inst = LeanExpr::app(&inst, &inst_add_nat)?;
    let mk_nat_add = |a: LeanBound<'l, LeanExpr>, b: LeanBound<'l, LeanExpr>| -> LeanResult<LeanBound<'l, LeanExpr>> {
        let f = LeanExpr::app(&hadd, &nat)?;
        let f = LeanExpr::app(&f, &nat)?;
        let f = LeanExpr::app(&f, &nat)?;
        let f = LeanExpr::app(&f, &inst)?;
        let f = LeanExpr::app(&f, &a)?;
        LeanExpr::app(&f, &b)
    };
    let n = LeanExpr::bvar(lean, 1)?;
    let m = LeanExpr::bvar(lean, 0)?;
    let n_plus_m = mk_nat_add(n.clone(), m.clone())?;
    let m_plus_n = mk_nat_add(m.clone(), n.clone())?;
    let eq = LeanExpr::mk_eq(lean, LeanList::cons(LeanLevel::one(lean)?.cast(), LeanList::nil(lean)?)?, &nat, &n_plus_m, &m_plus_n)?;
    let inner = LeanExpr::forall(LeanName::from_str(lean, "m")?, nat.clone(), eq, BinderInfo::Default)?;
    LeanExpr::forall(LeanName::from_str(lean, "n")?, nat, inner, BinderInfo::Default)
}

/// `goal_hyps_and_type_pp` after `intro n m` must render hypothesis names
/// and the goal type with the pretty printer.
#[test]
fn test_goal_hyps_and_type_pp_after_intro() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let mut metam = MetaMContext::new(lean, env)?;
        let ty = add_comm_type(lean)?;
        let goal = metam.mk_goal(&ty)?;
        let mvar = LeanExpr::mvar_id(&goal)?;
        let stx = parse_tactic(lean, metam.env(), "intro n m")?;
        let outcome = run_tactic(&mut metam, &mvar, &stx, None)?;
        let g = &outcome.goals[0];
        let (hyps, ty_pp) = metam.goal_hyps_and_type_pp(g)?;
        eprintln!("HYP0: {:?}", hyps.get(0));
        eprintln!("HYP1: {:?}", hyps.get(1));
        eprintln!("TY-PP: {ty_pp}");
        assert_eq!(hyps.len(), 2, "expected n and m hypotheses");
        assert!(hyps[0].0 == "n" || hyps[1].0 == "n", "user-facing hyp names: {hyps:?}");
        assert!(ty_pp.contains("n + m"), "expected notation, got: {ty_pp}");
        Ok(())
    });
    result.unwrap();
}

/// `pp_goal` (the `Lean.Meta.ppGoal` path) after `intro n m`.
#[test]
fn test_pp_goal_after_intro() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let mut metam = MetaMContext::new(lean, env)?;
        let ty = add_comm_type(lean)?;
        let goal = metam.mk_goal(&ty)?;
        let mvar = LeanExpr::mvar_id(&goal)?;
        let stx = parse_tactic(lean, metam.env(), "intro n m")?;
        let outcome = run_tactic(&mut metam, &mvar, &stx, None)?;
        let g = &outcome.goals[0];
        let pp = leo3::meta::repl::pp_goal(&mut metam, g)?;
        eprintln!("PP-GOAL:\n{pp}");
        assert!(pp.contains("n"), "expected goal view, got: {pp}");
        Ok(())
    });
    result.unwrap();
}

/// Two consecutive failing tactics must render readable messages and leave
/// the session usable (regression for MessageData.toString rendering).
#[test]
fn test_error_message_rendering_twice() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let mut metam = MetaMContext::new(lean, env)?;
        let ty = add_comm_type(lean)?;
        let goal = metam.mk_goal(&ty)?;
        let mvar = LeanExpr::mvar_id(&goal)?;
        let stx = parse_tactic(lean, metam.env(), "intro n m")?;
        let outcome = run_tactic(&mut metam, &mvar, &stx, None)?;
        let g = &outcome.goals[0];

        let stx_rfl = parse_tactic(lean, metam.env(), "rfl")?;
        let e1 = run_tactic(&mut metam, g, &stx_rfl, Some(&outcome.meta_state_ref))
            .err()
            .expect("rfl must fail");
        let m1 = format!("{e1}");
        eprintln!("ERR1: {m1}");
        assert!(
            !m1.contains("MessageData") && !m1.contains("<Format"),
            "raw MessageData leaked into error: {m1}"
        );
        assert!(m1.contains("definitionally equal"), "expected readable rfl error, got: {m1}");

        let stx_exact = parse_tactic(lean, metam.env(), "exact Nat.zero")?;
        let e2 = run_tactic(&mut metam, g, &stx_exact, Some(&outcome.meta_state_ref))
            .err()
            .expect("exact must fail");
        let m2 = format!("{e2}");
        eprintln!("ERR2: {m2}");
        assert!(!m2.contains("MessageData") && !m2.contains("<Format"), "raw MessageData leaked: {m2}");

        // Session still usable after two rendered failures.
        let stx2 = parse_tactic(lean, metam.env(), "induction n")?;
        let o2 = run_tactic(&mut metam, g, &stx2, Some(&outcome.meta_state_ref))?;
        assert_eq!(o2.goals.len(), 2);
        Ok(())
    });
    result.unwrap();
}
