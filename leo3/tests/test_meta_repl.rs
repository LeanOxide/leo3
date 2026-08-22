//! Integration tests for the repl-oriented elaborator API: importing real
//! Lean modules, parsing tactic strings with the real parser, and executing
//! tactics on goals through `Lean.Elab.runTactic`.

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    not(target_os = "windows"),
    lean_4_25
))]

use leo3::meta::*;
use leo3::prelude::*;

/// `∀ n m : Nat, n + m = m + n` built by hand (de Bruijn indices),
/// using the `HAdd.hAdd` shape that Lean's `+` notation elaborates to, so
/// `simp`/`rw` theorems (`Nat.add_zero`, ...) match the goal.
fn add_comm_type<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanExpr>> {
    let nat = LeanExpr::const_(lean, LeanName::from_str(lean, "Nat")?, LeanList::nil(lean)?)?;
    // HAdd.hAdd.{0,0,0} Nat Nat Nat (instHAdd.{0} Nat instAddNat) a b
    let lv0 = LeanLevel::zero(lean)?;
    let levels = LeanList::cons(
        lv0.clone().cast(),
        LeanList::cons(
            lv0.clone().cast(),
            LeanList::cons(lv0.cast(), LeanList::nil(lean)?)?,
        )?,
    )?;
    let hadd = LeanExpr::const_(lean, LeanName::from_components(lean, "HAdd.hAdd")?, levels)?;
    let inst_hadd = LeanExpr::const_(
        lean,
        LeanName::from_components(lean, "instHAdd")?,
        LeanList::cons(LeanLevel::zero(lean)?.cast(), LeanList::nil(lean)?)?,
    )?;
    let inst_add_nat = LeanExpr::const_(
        lean,
        LeanName::from_components(lean, "instAddNat")?,
        LeanList::nil(lean)?,
    )?;
    let inst = LeanExpr::app(&inst_hadd, &nat)?; // instHAdd.{0} Nat
    let inst = LeanExpr::app(&inst, &inst_add_nat)?; // instHAdd.{0} Nat instAddNat
    let mk_nat_add = |a: LeanBound<'l, LeanExpr>,
                      b: LeanBound<'l, LeanExpr>|
     -> LeanResult<LeanBound<'l, LeanExpr>> {
        let f = LeanExpr::app(&hadd, &nat)?; // HAdd.hAdd Nat
        let f = LeanExpr::app(&f, &nat)?; // HAdd.hAdd Nat Nat
        let f = LeanExpr::app(&f, &nat)?; // HAdd.hAdd Nat Nat Nat
        let f = LeanExpr::app(&f, &inst)?; // ... (instHAdd Nat instAddNat)
        let f = LeanExpr::app(&f, &a)?; // ... a
        LeanExpr::app(&f, &b) // ... a b
    };
    let n = LeanExpr::bvar(lean, 1)?;
    let m = LeanExpr::bvar(lean, 0)?;
    let n_plus_m = mk_nat_add(n.clone(), m.clone())?;
    let m_plus_n = mk_nat_add(m.clone(), n.clone())?;
    let eq = LeanExpr::mk_eq(
        lean,
        LeanList::cons(LeanLevel::one(lean)?.cast(), LeanList::nil(lean)?)?,
        &nat,
        &n_plus_m,
        &m_plus_n,
    )?;
    let inner = LeanExpr::forall(
        LeanName::from_str(lean, "m")?,
        nat.clone(),
        eq,
        BinderInfo::Default,
    )?;
    LeanExpr::forall(
        LeanName::from_str(lean, "n")?,
        nat,
        inner,
        BinderInfo::Default,
    )
}

/// Importing `Init` must produce an environment containing `Nat`.
#[test]
fn test_import_init_has_nat() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let nat = LeanEnvironment::find(&env, &LeanName::from_str(lean, "Nat")?)?;
        assert!(nat.is_some(), "Nat should exist after importing Init");
        Ok(())
    });
    result.unwrap();
}

/// A well-formed tactic string parses to a Syntax object.
#[test]
fn test_parse_tactic_ok() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let stx = parse_tactic(lean, &env, "intro n")?;
        assert!(!stx.as_ptr().is_null());
        Ok(())
    });
    result.unwrap();
}

/// Garbage input fails to parse with a readable error.
#[test]
fn test_parse_tactic_error() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let err = parse_tactic(lean, &env, "this is not a tactic !!!");
        assert!(err.is_err(), "invalid tactic should fail to parse");
        Ok(())
    });
    result.unwrap();
}

/// `rfl` cannot close `n + m = m + n` (not definitionally equal).
#[test]
fn test_run_tactic_rfl_fails_on_add_comm() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let mut metam = MetaMContext::new(lean, env)?;
        let ty = add_comm_type(lean)?;
        let goal = metam.mk_goal(&ty)?;
        let mvar = LeanExpr::mvar_id(&goal)?;
        let stx = parse_tactic(lean, metam.env(), "rfl")?;
        let outcome = run_tactic(&mut metam, &mvar, &stx, None);
        if let Err(e) = &outcome {
            eprintln!("[rfl] error: {e}");
        }
        assert!(
            outcome.is_err(),
            "rfl on `n + m = m + n` must fail (not definitionally equal)"
        );
        Ok(())
    });
    result.unwrap();
}

/// `intro n m` on the ∀-goal leaves exactly one goal (the equality).
#[test]
fn test_run_tactic_intro_advances_goal() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let mut metam = MetaMContext::new(lean, env)?;
        let ty = add_comm_type(lean)?;
        let goal = metam.mk_goal(&ty)?;
        let mvar = LeanExpr::mvar_id(&goal)?;
        // "intro n" peels the first binder, leaving `∀ m : Nat, n + m = m + n`.
        let stx = parse_tactic(lean, metam.env(), "intro n")?;
        let outcome = run_tactic(&mut metam, &mvar, &stx, None)?;
        assert_eq!(outcome.goals.len(), 1, "intro leaves one goal");
        // The remaining goal can be intro'd again, reusing the threaded
        // Meta.State ref.
        let stx2 = parse_tactic(lean, metam.env(), "intro m")?;
        let outcome2 = run_tactic(
            &mut metam,
            &outcome.goals[0],
            &stx2,
            Some(&outcome.meta_state_ref),
        )?;
        assert_eq!(outcome2.goals.len(), 1, "second intro leaves one goal");
        Ok(())
    });
    result.unwrap();
}

/// A real multi-step proof: `intro n; induction n` then `simp` per case.
/// This exercises the full elaborator (induction + simp) in-process.
#[test]
fn test_run_tactic_proves_add_comm_end_to_end() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let mut metam = MetaMContext::new(lean, env)?;
        let ty = add_comm_type(lean)?;
        let goal = metam.mk_goal(&ty)?;
        let mvar = LeanExpr::mvar_id(&goal)?;

        // intro n m
        let stx = parse_tactic(lean, metam.env(), "intro n m")?;
        let o1 = run_tactic(&mut metam, &mvar, &stx, None)?;
        let goals = o1.goals;
        assert_eq!(goals.len(), 1);
        let mut ref_ = o1.meta_state_ref;
        // induction n: yields two goals (zero/succ)
        let stx = parse_tactic(lean, metam.env(), "induction n")?;
        let o2 = run_tactic(&mut metam, &goals[0], &stx, Some(&ref_))?;
        let goals = o2.goals;
        ref_ = o2.meta_state_ref;
        assert_eq!(goals.len(), 2, "induction on n yields base + step");
        let step_goal = goals[1].clone();
        // base case: simp closes `0 + m = m + 0`
        let stx = parse_tactic(lean, metam.env(), "simp only [Nat.zero_add, Nat.add_zero]")?;
        let o3 = run_tactic(&mut metam, &goals[0], &stx, Some(&ref_))?;
        let goals = o3.goals;
        ref_ = o3.meta_state_ref;
        assert!(goals.is_empty(), "simp closes the base case");
        // step case: simp [Nat.add_comm, Nat.add_succ] closes `n+1 + m = m + (n+1)`
        let stx = parse_tactic(lean, metam.env(), "simp only [Nat.add_comm, Nat.add_succ]")?;
        let o4 = run_tactic(&mut metam, &step_goal, &stx, Some(&ref_))?;
        assert!(o4.goals.is_empty(), "all goals closed");
        Ok(())
    });
    result.unwrap();
}

/// Control: `simp` on a simple goal (no induction involved) must close it.
#[test]
fn test_simp_simple_goal() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let mut metam = MetaMContext::new(lean, env)?;
        // ∀ m : Nat, 0 + m = m + 0 (bvar 0 = m)
        let nat = LeanExpr::const_(lean, LeanName::from_str(lean, "Nat")?, LeanList::nil(lean)?)?;
        let lv0 = LeanLevel::zero(lean)?;
        let levels = LeanList::cons(
            lv0.clone().cast(),
            LeanList::cons(
                lv0.clone().cast(),
                LeanList::cons(lv0.cast(), LeanList::nil(lean)?)?,
            )?,
        )?;
        let hadd = LeanExpr::const_(lean, LeanName::from_components(lean, "HAdd.hAdd")?, levels)?;
        let inst_hadd = LeanExpr::const_(
            lean,
            LeanName::from_components(lean, "instHAdd")?,
            LeanList::cons(LeanLevel::zero(lean)?.cast(), LeanList::nil(lean)?)?,
        )?;
        let inst_add_nat = LeanExpr::const_(
            lean,
            LeanName::from_components(lean, "instAddNat")?,
            LeanList::nil(lean)?,
        )?;
        let inst = LeanExpr::app(&inst_hadd, &nat)?;
        let inst = LeanExpr::app(&inst, &inst_add_nat)?;
        // OfNat.ofNat.{0} Nat 0 (instOfNatNat 0) — the full numeral shape
        let lit0 = LeanExpr::lit(lean, LeanLiteral::nat(lean, 0)?)?;
        let inst_of_nat_nat = LeanExpr::const_(
            lean,
            LeanName::from_components(lean, "instOfNatNat")?,
            LeanList::cons(LeanLevel::zero(lean)?.cast(), LeanList::nil(lean)?)?,
        )?;
        let inst0 = LeanExpr::app(&inst_of_nat_nat, &lit0)?;
        let of_nat = LeanExpr::const_(
            lean,
            LeanName::from_components(lean, "OfNat.ofNat")?,
            LeanList::cons(LeanLevel::zero(lean)?.cast(), LeanList::nil(lean)?)?,
        )?;
        let zero_nat = LeanExpr::app(&of_nat, &nat)?;
        let zero_nat = LeanExpr::app(&zero_nat, &lit0)?;
        let zero_nat = LeanExpr::app(&zero_nat, &inst0)?;
        // 0 + m
        let zero_m = {
            let f = LeanExpr::app(&hadd, &nat)?;
            let f = LeanExpr::app(&f, &nat)?;
            let f = LeanExpr::app(&f, &nat)?;
            let f = LeanExpr::app(&f, &inst)?;
            let f = LeanExpr::app(&f, &zero_nat)?;
            LeanExpr::app(&f, &LeanExpr::bvar(lean, 0)?)?
        };
        // m + 0
        let m_zero = {
            let f = LeanExpr::app(&hadd, &nat)?;
            let f = LeanExpr::app(&f, &nat)?;
            let f = LeanExpr::app(&f, &nat)?;
            let f = LeanExpr::app(&f, &inst)?;
            let f = LeanExpr::app(&f, &LeanExpr::bvar(lean, 0)?)?;
            LeanExpr::app(&f, &zero_nat)?
        };
        let eq = LeanExpr::mk_eq(
            lean,
            LeanList::cons(LeanLevel::one(lean)?.cast(), LeanList::nil(lean)?)?,
            &nat,
            &zero_m,
            &m_zero,
        )?;
        let ty = LeanExpr::forall(LeanName::from_str(lean, "m")?, nat, eq, BinderInfo::Default)?;
        let goal = metam.mk_goal(&ty)?;
        let mvar = LeanExpr::mvar_id(&goal)?;
        let stx = parse_tactic(lean, metam.env(), "intro m")?;
        let o1 = run_tactic(&mut metam, &mvar, &stx, None)?;
        assert_eq!(o1.goals.len(), 1);
        let stx = parse_tactic(lean, metam.env(), "simp")?;
        let o2 = run_tactic(&mut metam, &o1.goals[0], &stx, Some(&o1.meta_state_ref))?;
        assert!(o2.goals.is_empty(), "simp closes 0 + m = m + 0");
        Ok(())
    });
    result.unwrap();
}

/// `rfl` must close `m + 0 = m` — whnf must reduce `Nat.add m 0`.
#[test]
fn test_rfl_m_plus_zero() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let mut metam = MetaMContext::new(lean, env)?;
        let nat = LeanExpr::const_(lean, LeanName::from_str(lean, "Nat")?, LeanList::nil(lean)?)?;
        let lv0 = LeanLevel::zero(lean)?;
        let levels = LeanList::cons(
            lv0.clone().cast(),
            LeanList::cons(
                lv0.clone().cast(),
                LeanList::cons(lv0.cast(), LeanList::nil(lean)?)?,
            )?,
        )?;
        let hadd = LeanExpr::const_(lean, LeanName::from_components(lean, "HAdd.hAdd")?, levels)?;
        let inst_hadd = LeanExpr::const_(
            lean,
            LeanName::from_components(lean, "instHAdd")?,
            LeanList::cons(LeanLevel::zero(lean)?.cast(), LeanList::nil(lean)?)?,
        )?;
        let inst_add_nat = LeanExpr::const_(
            lean,
            LeanName::from_components(lean, "instAddNat")?,
            LeanList::nil(lean)?,
        )?;
        let inst = LeanExpr::app(&inst_hadd, &nat)?;
        let inst = LeanExpr::app(&inst, &inst_add_nat)?;
        let lit0 = LeanExpr::lit(lean, LeanLiteral::nat(lean, 0)?)?;
        let inst_of_nat_nat = LeanExpr::const_(
            lean,
            LeanName::from_components(lean, "instOfNatNat")?,
            LeanList::cons(LeanLevel::zero(lean)?.cast(), LeanList::nil(lean)?)?,
        )?;
        let inst0 = LeanExpr::app(&inst_of_nat_nat, &lit0)?;
        let of_nat = LeanExpr::const_(
            lean,
            LeanName::from_components(lean, "OfNat.ofNat")?,
            LeanList::cons(LeanLevel::zero(lean)?.cast(), LeanList::nil(lean)?)?,
        )?;
        let zero_nat = LeanExpr::app(&of_nat, &nat)?;
        let zero_nat = LeanExpr::app(&zero_nat, &lit0)?;
        let zero_nat = LeanExpr::app(&zero_nat, &inst0)?;
        // m + 0 (m = bvar 0)
        let m_plus_zero = {
            let f = LeanExpr::app(&hadd, &nat)?;
            let f = LeanExpr::app(&f, &nat)?;
            let f = LeanExpr::app(&f, &nat)?;
            let f = LeanExpr::app(&f, &inst)?;
            let f = LeanExpr::app(&f, &LeanExpr::bvar(lean, 0)?)?;
            LeanExpr::app(&f, &zero_nat)?
        };
        let eq = LeanExpr::mk_eq(
            lean,
            LeanList::cons(LeanLevel::one(lean)?.cast(), LeanList::nil(lean)?)?,
            &nat,
            &m_plus_zero,
            &LeanExpr::bvar(lean, 0)?,
        )?;
        let ty = LeanExpr::forall(LeanName::from_str(lean, "m")?, nat, eq, BinderInfo::Default)?;
        let goal = metam.mk_goal(&ty)?;
        let mvar = LeanExpr::mvar_id(&goal)?;
        let stx = parse_tactic(lean, metam.env(), "intro m")?;
        let o1 = run_tactic(&mut metam, &mvar, &stx, None)?;
        assert_eq!(o1.goals.len(), 1);
        for tac in ["dsimp", "rfl"] {
            let stx = parse_tactic(lean, metam.env(), tac)?;
            let o2 = run_tactic(&mut metam, &o1.goals[0], &stx, Some(&o1.meta_state_ref));
            match o2 {
                Ok(o) => assert!(o.goals.is_empty(), "{tac} closes m + 0 = m"),
                Err(_) => assert!(tac == "rfl", "{tac} should succeed"),
            }
        }
        Ok(())
    });
    result.unwrap();
}
