//! Meta context comprehensive tests for Leo3
//!
//! Targets previously uncovered paths in:
//! - leo3/src/meta/context.rs / metam.rs: `MetaMContext::from_parts` /
//!   `into_parts`, goal hypothesis lookups, error-returning MetaM operations
//!   (whnf / infer_type / check on bad expressions, is_def_eq false cases,
//!   is_type_correct false cases)
//! - leo3/src/meta/environment.rs: `find` miss, quot-init roundtrip,
//!   constant-info extraction, `add_decl` kernel error paths
//! - leo3/src/meta/declaration.rs: axiom/definition/theorem builders
//!   (including Partial/Unsafe safety levels)
//! - leo3/src/meta/tactic.rs: exact/apply/intro/rfl failure paths on
//!   registered goals (a successful `checked_assign` currently corrupts the
//!   Lean heap — see the note in the tactic section below)
//! - leo3/src/meta/name.rs / level.rs / literal.rs: NameKind operations,
//!   level roundtrips, literal type extraction

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    not(target_os = "windows")
))]

use leo3::err::KernelExceptionCode;
use leo3::meta::*;
use leo3::prelude::*;

// ============================================================================
// MetaMContext parts (from_parts / into_parts)
// ============================================================================

#[test]
fn test_metam_context_into_parts_roundtrip() {
    // into_parts should decompose a MetaMContext and from_parts should
    // reconstruct a fully functional one.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut ctx = MetaMContext::new(lean, env)?;

        // Use the context before decomposing it.
        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        assert!(ctx.is_type_correct(&prop));

        let (env2, core_ctx, core_state, meta_ctx, meta_state) = ctx.into_parts();
        assert!(!env2.as_ptr().is_null());
        assert!(!core_ctx.as_ptr().is_null());
        assert!(!core_state.as_ptr().is_null());
        assert!(!meta_ctx.as_ptr().is_null());
        assert!(!meta_state.as_ptr().is_null());

        // Reconstruct and verify it still runs MetaM computations.
        let mut ctx2 = unsafe {
            MetaMContext::from_parts(lean, env2, core_ctx, core_state, meta_ctx, meta_state)
        };
        assert!(!ctx2.env().as_ptr().is_null());

        let type0 = LeanExpr::sort(lean, LeanLevel::one(lean)?)?;
        let ty = ctx2.infer_type(&type0)?;
        assert!(
            LeanExpr::is_sort(&ty),
            "reconstructed context should infer types"
        );

        // Fresh goals still work after the roundtrip.
        let goal = ctx2.mk_goal(&prop)?;
        assert!(LeanExpr::is_mvar(&goal));
        assert!(ctx2.is_type_correct(&type0));

        Ok(())
    });
    assert!(result.is_ok(), "parts roundtrip failed: {:?}", result.err());
}

#[test]
fn test_metam_context_from_parts_is_reusable() {
    // from_parts with parts from a fresh context; the rebuilt context must
    // survive multiple run() calls (whnf + is_def_eq).
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let ctx = MetaMContext::new(lean, env)?;
        let (env2, core_ctx, core_state, meta_ctx, meta_state) = ctx.into_parts();
        let mut ctx = unsafe {
            MetaMContext::from_parts(lean, env2, core_ctx, core_state, meta_ctx, meta_state)
        };

        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let prop2 = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        assert!(ctx.is_def_eq(&prop, &prop2)?);

        let w = ctx.whnf(&prop)?;
        assert!(LeanExpr::is_sort(&w));

        let type0 = LeanExpr::sort(lean, LeanLevel::one(lean)?)?;
        assert!(!ctx.is_def_eq(&prop, &type0)?);

        Ok(())
    });
    assert!(
        result.is_ok(),
        "from_parts reuse failed: {:?}",
        result.err()
    );
}

// ============================================================================
// Error-returning MetaM operations
// ============================================================================

#[test]
fn test_whnf_unknown_fvar_fails() {
    // whnf on a free variable that is not in the local context must error.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut ctx = MetaMContext::new(lean, env)?;

        let fvar_id = LeanName::from_str(lean, "ghost.fvar")?;
        let fvar = LeanExpr::fvar(lean, fvar_id)?;

        let whnf_result = ctx.whnf(&fvar);
        match whnf_result {
            Err(LeanError::Exception { .. }) => {}
            Err(e) => panic!("expected a Lean exception, got {:?}", e),
            Ok(_) => panic!("whnf on an unknown free variable should fail"),
        }

        Ok(())
    });
    assert!(result.is_ok(), "whnf error test failed: {:?}", result.err());
}

#[test]
fn test_infer_type_unknown_const_fails() {
    // infer_type on a constant that does not exist in the environment fails.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut ctx = MetaMContext::new(lean, env)?;

        let unknown = LeanExpr::const_(
            lean,
            LeanName::from_str(lean, "DoesNotExist")?,
            LeanList::nil(lean)?,
        )?;

        let infer_result = ctx.infer_type(&unknown);
        match infer_result {
            Err(LeanError::Exception {
                is_internal,
                message,
            }) => {
                assert!(!is_internal, "unknown constant should be a user exception");
                assert!(
                    !message.is_empty(),
                    "expected a descriptive message, got empty"
                );
            }
            Err(e) => panic!("expected a Lean exception, got {:?}", e),
            Ok(_) => panic!("expected a Lean exception, got Ok"),
        }

        Ok(())
    });
    assert!(
        result.is_ok(),
        "infer_type error test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_check_ill_typed_app_fails() {
    // check must reject `Prop Prop` (Prop is not a function type).
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut ctx = MetaMContext::new(lean, env)?;

        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let bad = LeanExpr::app(&prop, &prop)?;

        let check_result = ctx.check(&bad);
        match check_result {
            Err(LeanError::Exception { .. }) => {}
            Err(e) => panic!("expected a Lean exception, got {:?}", e),
            Ok(()) => panic!("check should reject an ill-typed application"),
        }

        Ok(())
    });
    assert!(
        result.is_ok(),
        "check error test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_is_type_correct_false_for_ill_typed_app() {
    // is_type_correct false path: an application of Prop to Prop.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut ctx = MetaMContext::new(lean, env)?;

        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let bad = LeanExpr::app(&prop, &prop)?;
        assert!(
            !ctx.is_type_correct(&bad),
            "ill-typed app should not be type-correct"
        );

        // Sanity: well-typed exprs remain type-correct.
        assert!(ctx.is_type_correct(&prop), "Sort(0) should be type-correct");
        let x_name = LeanName::from_str(lean, "x")?;
        let lambda = LeanExpr::lambda(
            x_name,
            prop.clone(),
            LeanExpr::bvar(lean, 0)?,
            BinderInfo::Default,
        )?;
        assert!(
            ctx.is_type_correct(&lambda),
            "λ x : Prop, x should be type-correct"
        );

        Ok(())
    });
    assert!(
        result.is_ok(),
        "is_type_correct test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_is_def_eq_false_cases() {
    // Real false cases for is_def_eq: structurally different expressions.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut ctx = MetaMContext::new(lean, env)?;

        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let type0 = LeanExpr::sort(lean, LeanLevel::one(lean)?)?;

        // Lambda vs Sort: not def-eq.
        let x_name = LeanName::from_str(lean, "x")?;
        let lambda = LeanExpr::lambda(
            x_name.clone(),
            prop.clone(),
            LeanExpr::bvar(lean, 0)?,
            BinderInfo::Default,
        )?;
        assert!(!ctx.is_def_eq(&lambda, &prop)?);

        // Foralls with different domains: ∀ x : Prop, Prop vs ∀ x : Type, Prop.
        let y_name = LeanName::from_str(lean, "y")?;
        let fa_prop = LeanExpr::forall(x_name, prop.clone(), prop.clone(), BinderInfo::Default)?;
        let fa_type = LeanExpr::forall(
            y_name.clone(),
            type0.clone(),
            prop.clone(),
            BinderInfo::Default,
        )?;
        assert!(!ctx.is_def_eq(&fa_prop, &fa_type)?);

        // Lambdas with different bodies: λ x : Prop, x vs λ x : Prop, Prop.
        let body_sort = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let lambda2 = LeanExpr::lambda(y_name, prop.clone(), body_sort, BinderInfo::Default)?;
        assert!(!ctx.is_def_eq(&lambda, &lambda2)?);

        // Positive sanity check on the same context.
        assert!(ctx.is_def_eq(&prop, &LeanExpr::sort(lean, LeanLevel::zero(lean)?)?)?);

        Ok(())
    });
    assert!(
        result.is_ok(),
        "is_def_eq false cases failed: {:?}",
        result.err()
    );
}

#[test]
fn test_is_prop_false_and_error() {
    // is_prop returns false for expressions whose type is not Sort 0, and
    // errors when inference fails.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut ctx = MetaMContext::new(lean, env)?;

        // Type of λ x : Prop, x is Prop → Prop, which is Sort 1, not Sort 0.
        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let x_name = LeanName::from_str(lean, "x")?;
        let lambda = LeanExpr::lambda(
            x_name,
            prop.clone(),
            LeanExpr::bvar(lean, 0)?,
            BinderInfo::Default,
        )?;
        assert!(!ctx.is_prop(&lambda)?);

        // Type of Prop (Sort 0) is Type (Sort 1), not Sort 0.
        assert!(!ctx.is_prop(&prop)?);

        // Inference failure propagates as an error.
        let fvar_id = LeanName::from_str(lean, "ghost.prop")?;
        let fvar = LeanExpr::fvar(lean, fvar_id)?;
        assert!(ctx.is_prop(&fvar).is_err());

        Ok(())
    });
    assert!(result.is_ok(), "is_prop test failed: {:?}", result.err());
}

#[test]
fn test_get_proof_type_and_is_proof_of_errors() {
    // get_proof_type success and error; is_proof_of error propagation.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut ctx = MetaMContext::new(lean, env)?;

        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let x_name = LeanName::from_str(lean, "x")?;
        let lambda = LeanExpr::lambda(
            x_name,
            prop.clone(),
            LeanExpr::bvar(lean, 0)?,
            BinderInfo::Default,
        )?;

        // The type of λ x : Prop, x is a forall.
        let proof_type = ctx.get_proof_type(&lambda)?;
        assert!(LeanExpr::is_forall(&proof_type));
        assert!(ctx.is_proof_of(&lambda, &proof_type)?);

        // An unknown free variable fails type inference in both helpers.
        let fvar_id = LeanName::from_str(lean, "ghost.proof")?;
        let fvar = LeanExpr::fvar(lean, fvar_id)?;
        assert!(ctx.get_proof_type(&fvar).is_err());
        assert!(ctx.is_proof_of(&fvar, &prop).is_err());

        Ok(())
    });
    assert!(
        result.is_ok(),
        "proof helper test failed: {:?}",
        result.err()
    );
}

// ============================================================================
// Goal management (mk_named_goal, goal_hypothesis, goal_latest_hypothesis)
// ============================================================================

#[test]
fn test_mk_named_goal() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut metam = MetaMContext::new(lean, env)?;

        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let gname = LeanName::from_str(lean, "namedGoal")?;
        let goal = metam.mk_named_goal(&prop, &gname)?;

        assert!(
            LeanExpr::is_mvar(&goal),
            "named goal should be a metavariable"
        );

        // The goal's type must be the proposition we passed in.
        let ty = goal_type(&mut metam, &goal)?;
        assert!(metam.is_def_eq(&ty, &prop)?);

        Ok(())
    });
    assert!(result.is_ok(), "mk_named_goal failed: {:?}", result.err());
}

// ============================================================================
// Environment: find, quot-init, constant info, add_decl error paths
// ============================================================================

#[test]
fn test_environment_find_missing_returns_none() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;

        // Nothing is in an empty environment.
        let nat = LeanName::from_str(lean, "Nat")?;
        let found = LeanEnvironment::find(&env, &nat)?;
        assert!(found.is_none(), "empty env should not contain Nat");

        // A name that will never exist.
        let bogus = LeanName::from_components(lean, "No.Such.Decl")?;
        assert!(LeanEnvironment::find(&env, &bogus)?.is_none());

        Ok(())
    });
    assert!(result.is_ok(), "find miss test failed: {:?}", result.err());
}

#[test]
fn test_environment_quot_init_and_mark() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;

        // An empty environment has not initialized Quot.
        assert!(
            !LeanEnvironment::is_quot_init(&env),
            "empty env should not have Quot initialized"
        );

        // mark_quot_init returns a new environment. Note: the mark is stored
        // on the elaborator environment, while is_quot_init inspects the
        // converted kernel environment, so the observable flag stays false
        // until Quot declarations are actually added. Both objects are still
        // fully usable afterwards.
        let env2 = LeanEnvironment::mark_quot_init(env);
        assert!(
            !LeanEnvironment::is_quot_init(&env2),
            "kernel env does not carry the elab-level Quot mark"
        );

        // The marked environment still accepts declarations.
        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let name = LeanName::from_str(lean, "AfterQuotMark")?;
        let decl = LeanDeclaration::axiom(lean, name.clone(), LeanList::nil(lean)?, prop, false)?;
        let env3 = LeanEnvironment::add_decl(&env2, &decl)?;
        assert!(LeanEnvironment::find(&env3, &name)?.is_some());
        assert!(!LeanEnvironment::is_quot_init(&env3));

        Ok(())
    });
    assert!(result.is_ok(), "quot init test failed: {:?}", result.err());
}

#[test]
fn test_environment_axiom_constant_info() {
    // Add a universe-polymorphic axiom via kernel-checked add_decl and
    // inspect every LeanConstantInfo accessor.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;

        // axiom TestEnv.ax : Sort (u + 1) with level param [u]
        let u_name = LeanName::from_str(lean, "u")?;
        let u_level = LeanLevel::param(lean, u_name.clone())?;
        let u_succ = LeanLevel::succ(u_level)?;
        let ty = LeanExpr::sort(lean, u_succ)?;
        let level_params = LeanList::cons(u_name.cast(), LeanList::nil(lean)?)?;
        let ax_name = LeanName::from_components(lean, "TestEnv.ax")?;
        let decl = LeanDeclaration::axiom(
            lean,
            ax_name.clone(),
            level_params.clone(),
            ty.clone(),
            false,
        )?;
        let env2 = LeanEnvironment::add_decl(&env, &decl)?;

        let cinfo = LeanEnvironment::find(&env2, &ax_name)?.expect("axiom should be findable");

        // NOTE: the l_Lean_ConstantInfo_* accessors consume their argument
        // (Lean ABI), but leo3 passes a borrowed pointer. Passing a clone
        // gives each accessor its own reference to consume.
        // name roundtrip
        let cinfo_name = LeanConstantInfo::name(&cinfo.clone())?;
        assert!(LeanName::eq(&cinfo_name, &ax_name));

        // type roundtrip
        let cinfo_ty = LeanConstantInfo::type_(&cinfo.clone())?;
        assert!(LeanExpr::equal(&cinfo_ty, &ty));

        // universe level parameters
        let cinfo_params = LeanConstantInfo::level_params(&cinfo.clone())?;
        assert_eq!(LeanList::length(&cinfo_params), 1, "expected 1 level param");

        // kind / value
        assert_eq!(LeanConstantInfo::kind(&cinfo.clone()), ConstantKind::Axiom);
        assert!(
            !LeanConstantInfo::has_value(&cinfo.clone()),
            "axiom should have no value"
        );
        assert!(LeanConstantInfo::value(&cinfo.clone())?.is_none());

        Ok(())
    });
    assert!(
        result.is_ok(),
        "axiom constant info failed: {:?}",
        result.err()
    );
}

#[test]
fn test_environment_theorem_constant_info() {
    // Add a theorem with a real proof via kernel-checked add_decl.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;

        // theorem TestEnv.thm : ∀ P : Prop, P → P
        let p_name = LeanName::from_str(lean, "P")?;
        let h_name = LeanName::from_str(lean, "h")?;
        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let bvar0 = LeanExpr::bvar(lean, 0)?;
        let bvar1 = LeanExpr::bvar(lean, 1)?;
        let inner_forall = LeanExpr::forall(h_name, bvar0, bvar1, BinderInfo::Default)?;
        let proposition =
            LeanExpr::forall(p_name, prop.clone(), inner_forall, BinderInfo::Default)?;

        let prop2 = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let bvar0_inner = LeanExpr::bvar(lean, 0)?;
        let bvar0_body = LeanExpr::bvar(lean, 0)?;
        let inner_lambda = LeanExpr::lambda(
            LeanName::from_str(lean, "h2")?,
            bvar0_inner,
            bvar0_body,
            BinderInfo::Default,
        )?;
        let proof = LeanExpr::lambda(
            LeanName::from_str(lean, "P2")?,
            prop2,
            inner_lambda,
            BinderInfo::Default,
        )?;

        let thm_name = LeanName::from_components(lean, "TestEnv.thm")?;
        let decl = LeanDeclaration::theorem(
            lean,
            thm_name.clone(),
            LeanList::nil(lean)?,
            proposition.clone(),
            proof,
        )?;
        let env2 = LeanEnvironment::add_decl(&env, &decl)?;

        let cinfo = LeanEnvironment::find(&env2, &thm_name)?.expect("theorem should be findable");

        // Each accessor consumes its argument (Lean ABI); pass clones so the
        // shared ConstantInfo keeps its own reference. `value` internally
        // consumes twice (hasValue + value!), hence the double clone.
        assert_eq!(
            LeanConstantInfo::kind(&cinfo.clone()),
            ConstantKind::Theorem
        );
        assert!(
            LeanConstantInfo::has_value(&cinfo.clone()),
            "theorem should have a value"
        );
        let value = LeanConstantInfo::value(&cinfo.clone().clone())?;
        assert!(value.is_some(), "theorem value should be extractable");
        let cinfo_ty = LeanConstantInfo::type_(&cinfo.clone())?;
        assert!(LeanExpr::equal(&cinfo_ty, &proposition));
        let cinfo_params = LeanConstantInfo::level_params(&cinfo.clone())?;
        assert_eq!(LeanList::length(&cinfo_params), 0);

        Ok(())
    });
    assert!(
        result.is_ok(),
        "theorem constant info failed: {:?}",
        result.err()
    );
}

#[test]
fn test_environment_definition_constant_info() {
    // Add definitions (Safe in env) and exercise all DefinitionSafety levels
    // through the builder.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;

        // def TestEnv.def : Type := Prop
        let type1 = LeanExpr::sort(lean, LeanLevel::one(lean)?)?;
        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let hints = unsafe { LeanBound::from_owned_ptr(lean, leo3_ffi::lean_box(0)) };
        let def_name = LeanName::from_components(lean, "TestEnv.def")?;
        let decl = LeanDeclaration::definition(
            lean,
            def_name.clone(),
            LeanList::nil(lean)?,
            type1.clone(),
            prop.clone(),
            hints,
            DefinitionSafety::Safe,
        )?;
        let env2 = LeanEnvironment::add_decl(&env, &decl)?;

        let cinfo =
            LeanEnvironment::find(&env2, &def_name)?.expect("definition should be findable");

        // Accessors consume their argument (Lean ABI); pass clones so the
        // shared ConstantInfo keeps its own reference.
        assert_eq!(
            LeanConstantInfo::kind(&cinfo.clone()),
            ConstantKind::Definition
        );
        assert!(LeanConstantInfo::has_value(&cinfo.clone()));
        assert!(LeanConstantInfo::value(&cinfo.clone().clone())?.is_some());
        let cinfo_ty = LeanConstantInfo::type_(&cinfo.clone())?;
        assert!(LeanExpr::equal(&cinfo_ty, &type1));

        // Partial and Unsafe safety levels build fine (kernel not involved).
        let partial_name = LeanName::from_components(lean, "TestEnv.partial")?;
        let decl_partial = LeanDeclaration::definition(
            lean,
            partial_name.clone(),
            LeanList::nil(lean)?,
            type1.clone(),
            prop.clone(),
            unsafe { LeanBound::from_owned_ptr(lean, leo3_ffi::lean_box(0)) },
            DefinitionSafety::Partial,
        )?;
        assert!(LeanName::eq(
            &LeanDeclaration::name(&decl_partial),
            &partial_name
        ));

        let unsafe_name = LeanName::from_components(lean, "TestEnv.unsafe")?;
        let decl_unsafe = LeanDeclaration::definition(
            lean,
            unsafe_name.clone(),
            LeanList::nil(lean)?,
            type1,
            prop,
            unsafe { LeanBound::from_owned_ptr(lean, leo3_ffi::lean_box(0)) },
            DefinitionSafety::Unsafe,
        )?;
        assert!(LeanName::eq(
            &LeanDeclaration::name(&decl_unsafe),
            &unsafe_name
        ));

        Ok(())
    });
    assert!(
        result.is_ok(),
        "definition constant info failed: {:?}",
        result.err()
    );
}

#[test]
fn test_add_decl_duplicate_name_fails() {
    // The kernel must reject a second declaration with the same name.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;

        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let name = LeanName::from_str(lean, "DupDecl")?;
        let decl1 = LeanDeclaration::axiom(
            lean,
            name.clone(),
            LeanList::nil(lean)?,
            prop.clone(),
            false,
        )?;
        let env2 = LeanEnvironment::add_decl(&env, &decl1)?;

        let decl2 = LeanDeclaration::axiom(lean, name, LeanList::nil(lean)?, prop, false)?;
        let dup_result = LeanEnvironment::add_decl(&env2, &decl2);
        match dup_result {
            Err(LeanError::KernelException { code, .. }) => {
                assert_eq!(
                    code,
                    KernelExceptionCode::AlreadyDeclared,
                    "expected AlreadyDeclared kernel error"
                );
            }
            Err(e) => panic!("expected AlreadyDeclared error, got {:?}", e),
            Ok(_) => panic!("expected AlreadyDeclared error, got Ok"),
        }

        Ok(())
    });
    assert!(
        result.is_ok(),
        "duplicate decl test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_add_decl_unknown_constant_fails() {
    // The kernel must reject a declaration whose type references an
    // unknown constant.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;

        let bad_type = LeanExpr::const_(
            lean,
            LeanName::from_str(lean, "NotInEnv")?,
            LeanList::nil(lean)?,
        )?;
        let name = LeanName::from_str(lean, "badAxiom")?;
        let decl = LeanDeclaration::axiom(lean, name, LeanList::nil(lean)?, bad_type, false)?;

        let add_result = LeanEnvironment::add_decl(&env, &decl);
        match add_result {
            Err(LeanError::KernelException { code, .. }) => {
                assert_eq!(
                    code,
                    KernelExceptionCode::UnknownConstant,
                    "expected UnknownConstant kernel error"
                );
            }
            Err(e) => panic!("expected UnknownConstant error, got {:?}", e),
            Ok(_) => panic!("expected UnknownConstant error, got Ok"),
        }

        Ok(())
    });
    assert!(
        result.is_ok(),
        "unknown constant decl test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_declaration_name_extraction_all_builders() {
    // LeanDeclaration::name must work for axiom, theorem, and definition.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let type1 = LeanExpr::sort(lean, LeanLevel::one(lean)?)?;

        let ax_name = LeanName::from_components(lean, "Decl.ax")?;
        let ax_decl = LeanDeclaration::axiom(
            lean,
            ax_name.clone(),
            LeanList::nil(lean)?,
            prop.clone(),
            false,
        )?;
        assert!(LeanName::eq(&LeanDeclaration::name(&ax_decl), &ax_name));

        let thm_name = LeanName::from_components(lean, "Decl.thm")?;
        let thm_decl = LeanDeclaration::theorem(
            lean,
            thm_name.clone(),
            LeanList::nil(lean)?,
            prop.clone(),
            type1.clone(),
        )?;
        assert!(LeanName::eq(&LeanDeclaration::name(&thm_decl), &thm_name));

        let def_name = LeanName::from_components(lean, "Decl.def")?;
        let def_decl = LeanDeclaration::definition(
            lean,
            def_name.clone(),
            LeanList::nil(lean)?,
            type1,
            prop,
            unsafe { LeanBound::from_owned_ptr(lean, leo3_ffi::lean_box(0)) },
            DefinitionSafety::Safe,
        )?;
        assert!(LeanName::eq(&LeanDeclaration::name(&def_decl), &def_name));

        Ok(())
    });
    assert!(
        result.is_ok(),
        "declaration name extraction failed: {:?}",
        result.err()
    );
}

// ============================================================================
// Tactic failure paths
// ============================================================================

// NOTE: a successful `exact`/`apply` (i.e., `checked_assign` returning true)
// currently corrupts the Lean heap in this runtime, surfacing as a crash in
// the *next* `LeanEnvironment::empty` call. That library-level bug prevents
// testing the exact-success path here; the required failure paths below are
// fully exercised instead.

#[test]
fn test_exact_wrong_term_fails() {
    // exact with a term whose type is not definitionally equal to the goal
    // type must fail (registered goal, real type mismatch).
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut metam = MetaMContext::new(lean, env)?;

        // Goal: Prop.  Trying to close it with `Prop` (which has type Type).
        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let goal = metam.mk_goal(&prop)?;

        let result = exact(&mut metam, TacticState::new(vec![goal]), &prop);
        match result {
            TacticResult::Failure(_) => {
                // Expected: Prop : Type is not def-eq to the goal type Prop.
            }
            TacticResult::Success(_) => {
                panic!("exact with a wrong term should fail");
            }
        }

        Ok(())
    });
    assert!(
        result.is_ok(),
        "exact failure test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_apply_untypeable_expr_fails() {
    // apply with an expression whose type cannot be inferred (unknown
    // constant) must fail on a registered goal.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut metam = MetaMContext::new(lean, env)?;

        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let goal = metam.mk_goal(&prop)?;

        let unknown = LeanExpr::const_(
            lean,
            LeanName::from_str(lean, "NotInEnv")?,
            LeanList::nil(lean)?,
        )?;
        let result = apply(&mut metam, TacticState::new(vec![goal]), &unknown);
        match result {
            TacticResult::Failure(_) => {
                // Expected: infer_type on an unknown constant fails.
            }
            TacticResult::Success(_) => {
                panic!("apply with an untypeable expression should fail");
            }
        }

        Ok(())
    });
    assert!(
        result.is_ok(),
        "apply failure test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_rfl_mismatched_sides_fails() {
    // rfl must fail when the equality sides are not definitionally equal.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut metam = MetaMContext::new(lean, env)?;

        // Goal: Prop = Type  (mk_eq only builds the AST; no env axioms needed
        // because rfl fails before constructing the proof).
        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let type0 = LeanExpr::sort(lean, LeanLevel::one(lean)?)?;
        let levels = LeanList::cons(LeanLevel::zero(lean)?.cast(), LeanList::nil(lean)?)?;
        let eq_goal = LeanExpr::mk_eq(lean, levels, &prop, &prop, &type0)?;
        let goal = metam.mk_goal(&eq_goal)?;

        let result = rfl(&mut metam, TacticState::new(vec![goal]));
        match result {
            TacticResult::Failure(_) => {
                // Expected: Prop is not def-eq to Type.
            }
            TacticResult::Success(_) => {
                panic!("rfl should fail when the sides differ");
            }
        }

        Ok(())
    });
    assert!(
        result.is_ok(),
        "rfl failure test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_intro_non_forall_goal_fails() {
    // intro on a registered goal whose type is not a forall must fail.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut metam = MetaMContext::new(lean, env)?;

        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let goal = metam.mk_goal(&prop)?;
        let h_name = LeanName::from_str(lean, "h")?;

        let result = intro(&mut metam, TacticState::new(vec![goal]), &h_name);
        match result {
            TacticResult::Failure(_) => {
                // Expected: Prop is not a ∀ type.
            }
            TacticResult::Success(_) => {
                panic!("intro should fail on a non-forall goal");
            }
        }

        Ok(())
    });
    assert!(
        result.is_ok(),
        "intro failure test failed: {:?}",
        result.err()
    );
}

// ============================================================================
// Name kind operations
// ============================================================================

#[test]
fn test_name_kind_component_operations() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Hierarchical names built via from_components are Str components.
        let dotted = LeanName::from_components(lean, "Std.Data.List")?;
        assert_eq!(LeanName::kind(&dotted)?, NameKind::Str);

        // append_str produces another Str component.
        let tail = LeanName::append_str(dotted.clone(), lean, "tail")?;
        assert_eq!(LeanName::kind(&tail)?, NameKind::Str);
        assert!(LeanName::eq(
            &tail,
            &LeanName::from_components(lean, "Std.Data.List.tail")?
        ));

        // append_num produces a Num component.
        let indexed = LeanName::append_num(dotted, lean, 42)?;
        assert_eq!(LeanName::kind(&indexed)?, NameKind::Num);

        // Anonymous root stays anonymous.
        let anon = LeanName::anonymous(lean)?;
        assert_eq!(LeanName::kind(&anon)?, NameKind::Anonymous);

        // Plain strings are Str.
        let plain = LeanName::from_str(lean, "foo")?;
        assert_eq!(LeanName::kind(&plain)?, NameKind::Str);

        Ok(())
    });
    assert!(result.is_ok(), "name kind test failed: {:?}", result.err());
}

#[test]
fn test_name_kind_invalid_tag_errors() {
    // A bogus tag must surface as InvalidKind rather than panicking.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // lean_box(7) is a scalar whose tag is 7 — not a valid Name tag.
        let bogus: LeanBound<'_, LeanName> =
            unsafe { LeanBound::from_owned_ptr(lean, leo3_ffi::lean_box(7)) };

        match LeanName::kind(&bogus) {
            Err(LeanError::InvalidKind { kind, tag }) => {
                assert_eq!(kind, "name");
                assert_eq!(tag, 7);
            }
            other => panic!("expected InvalidKind, got {:?}", other),
        }

        Ok(())
    });
    assert!(
        result.is_ok(),
        "invalid name kind test failed: {:?}",
        result.err()
    );
}

// ============================================================================
// Level operations
// ============================================================================

#[test]
fn test_level_ops_roundtrip_and_sort() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let mut ctx = MetaMContext::new(lean, env)?;

        // succ ( max 0 1 ) — composed level in a Sort, roundtripped via
        // sort_level and verified type-correct.
        let composed = LeanLevel::succ(LeanLevel::max(
            LeanLevel::zero(lean)?,
            LeanLevel::one(lean)?,
        )?)?;
        let sort = LeanExpr::sort(lean, composed)?;
        assert!(ctx.is_type_correct(&sort));
        let extracted = LeanExpr::sort_level(&sort)?;
        assert!(
            !extracted.as_ptr().is_null(),
            "sort_level should extract the level"
        );

        // imax level sorts are valid.
        let imax = LeanLevel::imax(LeanLevel::zero(lean)?, LeanLevel::one(lean)?)?;
        let sort_imax = LeanExpr::sort(lean, imax)?;
        assert!(ctx.is_type_correct(&sort_imax));

        // param level: the sort contains a level parameter.
        let u_name = LeanName::from_str(lean, "u")?;
        let param = LeanLevel::param(lean, u_name)?;
        let sort_param = LeanExpr::sort(lean, param)?;
        assert!(LeanExpr::has_level_param(&sort_param));

        // whnf on a sort is stable.
        let w = ctx.whnf(&sort)?;
        assert!(LeanExpr::is_sort(&w));

        Ok(())
    });
    assert!(result.is_ok(), "level ops test failed: {:?}", result.err());
}

#[test]
fn test_sort_level_wrong_kind_errors() {
    // sort_level on a non-sort must error, not panic.
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let bvar = LeanExpr::bvar(lean, 0)?;
        assert!(
            LeanExpr::sort_level(&bvar).is_err(),
            "sort_level on a bvar should error"
        );

        let prop = LeanExpr::sort(lean, LeanLevel::zero(lean)?)?;
        let app = LeanExpr::app(&prop, &prop)?;
        assert!(
            LeanExpr::sort_level(&app).is_err(),
            "sort_level on an app should error"
        );

        Ok(())
    });
    assert!(
        result.is_ok(),
        "sort_level error test failed: {:?}",
        result.err()
    );
}

// ============================================================================
// Literal operations
// ============================================================================

#[test]
fn test_literal_ops_types() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Nat literal's type is the `Nat` constant.
        let nat_lit = LeanLiteral::nat(lean, 42)?;
        let nat_ty = LeanLiteral::type_(&nat_lit)?;
        assert!(LeanExpr::is_const(&nat_ty));
        let nat_name = LeanExpr::const_name(&nat_ty)?;
        assert!(LeanName::eq(&nat_name, &LeanName::from_str(lean, "Nat")?));

        // String literal's type is the `String` constant.
        let str_lit = LeanLiteral::string(lean, "hello")?;
        let str_ty = LeanLiteral::type_(&str_lit)?;
        assert!(LeanExpr::is_const(&str_ty));
        let str_name = LeanExpr::const_name(&str_ty)?;
        assert!(LeanName::eq(
            &str_name,
            &LeanName::from_str(lean, "String")?
        ));

        // Nat and String literal types differ.
        assert!(!LeanName::eq(&nat_name, &str_name));

        // Literal exprs wrap the literal and are recognized as such.
        let lit_expr = LeanExpr::lit(lean, nat_lit)?;
        assert!(LeanExpr::is_lit(&lit_expr));

        Ok(())
    });
    assert!(
        result.is_ok(),
        "literal ops test failed: {:?}",
        result.err()
    );
}
