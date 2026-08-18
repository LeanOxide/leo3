//! TEMPORARY debug test: bisect the 4.33 crash in test_rfl_m_plus_zero.
//! Delete after diagnosis.

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    not(target_os = "windows")
))]

use leo3::meta::*;
use leo3::prelude::*;

#[test]
fn debug_rfl_steps() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        eprintln!("[step] before import");
        let env = import_modules(lean, &["Lean"], 0)?;
        eprintln!("[step] import ok");
        let mut metam = MetaMContext::new(lean, env)?;
        eprintln!("[step] MetaMContext::new ok");

        let nat = LeanExpr::const_(lean, LeanName::from_str(lean, "Nat")?, LeanList::nil(lean)?)?;
        let lv0 = LeanLevel::zero(lean)?;
        let levels =
            LeanList::cons(lv0.clone().cast(), LeanList::cons(lv0.clone().cast(), LeanList::cons(lv0.cast(), LeanList::nil(lean)?)?)?)?;
        let hadd = LeanExpr::const_(lean, LeanName::from_components(lean, "HAdd.hAdd")?, levels)?;
        let inst_hadd = LeanExpr::const_(lean, LeanName::from_components(lean, "instHAdd")?, LeanList::cons(LeanLevel::zero(lean)?.cast(), LeanList::nil(lean)?)?)?;
        let inst_add_nat = LeanExpr::const_(lean, LeanName::from_components(lean, "instAddNat")?, LeanList::nil(lean)?)?;
        let inst = LeanExpr::app(&inst_hadd, &nat)?;
        let inst = LeanExpr::app(&inst, &inst_add_nat)?;
        let lit0 = LeanExpr::lit(lean, LeanLiteral::nat(lean, 0)?)?;
        let inst_of_nat_nat = LeanExpr::const_(lean, LeanName::from_components(lean, "instOfNatNat")?, LeanList::cons(LeanLevel::zero(lean)?.cast(), LeanList::nil(lean)?)?)?;
        let inst0 = LeanExpr::app(&inst_of_nat_nat, &lit0)?;
        let of_nat = LeanExpr::const_(lean, LeanName::from_components(lean, "OfNat.ofNat")?, LeanList::cons(LeanLevel::zero(lean)?.cast(), LeanList::nil(lean)?)?)?;
        let zero_nat = LeanExpr::app(&of_nat, &nat)?;
        let zero_nat = LeanExpr::app(&zero_nat, &lit0)?;
        let zero_nat = LeanExpr::app(&zero_nat, &inst0)?;
        let m_plus_zero = {
            let f = LeanExpr::app(&hadd, &nat)?;
            let f = LeanExpr::app(&f, &nat)?;
            let f = LeanExpr::app(&f, &nat)?;
            let f = LeanExpr::app(&f, &inst)?;
            let f = LeanExpr::app(&f, &LeanExpr::bvar(lean, 0)?)?;
            LeanExpr::app(&f, &zero_nat)?
        };
        let eq = LeanExpr::mk_eq(lean, LeanList::cons(LeanLevel::one(lean)?.cast(), LeanList::nil(lean)?)?, &nat, &m_plus_zero, &LeanExpr::bvar(lean, 0)?)?;
        let ty = LeanExpr::forall(LeanName::from_str(lean, "m")?, nat, eq, BinderInfo::Default)?;
        eprintln!("[step] exprs built");

        let goal = metam.mk_goal(&ty)?;
        eprintln!("[step] mk_goal ok");
        let mvar = LeanExpr::mvar_id(&goal)?;
        let stx = parse_tactic(lean, metam.env(), "intro m")?;
        eprintln!("[step] parse ok");
        let o1 = run_tactic(&mut metam, &mvar, &stx, None)?;
        eprintln!("[step] intro ok, goals: {}", o1.goals.len());
        Ok(())
    });
    eprintln!("[step] result err: {:?}", result.as_ref().err().map(|e| e.to_string()));
    result.unwrap();
}
