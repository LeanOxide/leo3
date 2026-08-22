//! Basic integration tests for meta-programming features

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    not(target_os = "windows")
))]

use leo3::meta::*;
use leo3::prelude::*;

#[test]
fn test_core_context_creation() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create a Core.Context with default values
        let ctx = CoreContext::mk_default(lean)?;

        // Should succeed and not be null
        assert!(!ctx.as_ptr().is_null());

        // Verify it's a constructor with tag 0 (Core.Context)
        unsafe {
            let tag = leo3_ffi::lean_obj_tag(ctx.as_ptr());
            assert_eq!(tag, 0, "Core.Context should have constructor tag 0");
        }

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Core.Context creation failed: {:?}",
        result.err()
    );
}

#[test]
fn test_environment_creation() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create an empty environment
        let env = LeanEnvironment::empty(lean, 0)?;

        // Should succeed
        assert!(!env.as_ptr().is_null());

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Environment creation failed: {:?}",
        result.err()
    );
}

#[test]
fn test_core_state_creation() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create an empty environment
        let env = LeanEnvironment::empty(lean, 0)?;

        // Create a Core.State with the environment
        let state = CoreState::mk_core_state(lean, &env)?;

        // Should succeed and not be null
        assert!(!state.as_ptr().is_null());

        // Verify it's a constructor with tag 0 (Core.State)
        unsafe {
            let tag = leo3_ffi::lean_obj_tag(state.as_ptr());
            assert_eq!(tag, 0, "Core.State should have constructor tag 0");

            // Verify it has the right number of fields
            // Field 0 should be the environment
            let env_field = leo3_ffi::lean_ctor_get(state.as_ptr(), 0);
            assert!(!env_field.is_null(), "Environment field should not be null");

            // Field 1: nextMacroScope. Matches the real frontend: a fresh
            // `Core.State` starts at `firstFrontendMacroScope + 1`. Both on
            // Lean >= 4.25 and on 4.20 `reservedMacroScope = 0`, so
            // `firstFrontendMacroScope = 1` and the initial value is 2.
            let macro_scope = leo3_ffi::lean_ctor_get(state.as_ptr(), 1);
            let macro_scope_val = leo3_ffi::lean_unbox(macro_scope);
            let expected_macro_scope = 2;
            assert_eq!(
                macro_scope_val, expected_macro_scope,
                "nextMacroScope should be {}",
                expected_macro_scope
            );

            // Field 2 should be NameGenerator (should be a constructor)
            let ngen = leo3_ffi::lean_ctor_get(state.as_ptr(), 2);
            assert!(!ngen.is_null(), "NameGenerator should not be null");
            let ngen_tag = leo3_ffi::lean_obj_tag(ngen as *mut _);
            assert_eq!(ngen_tag, 0, "NameGenerator should have constructor tag 0");

            // Last field should be snapshotTasks (empty Array SnapshotTask).
            // On Lean >= 4.25 there are 9 fields (idx 8), otherwise 8 fields
            // (idx 7).
            #[cfg(lean_4_25)]
            let snapshot_tasks_idx = 8;
            #[cfg(not(lean_4_25))]
            let snapshot_tasks_idx = 7;
            let snapshot_tasks = leo3_ffi::lean_ctor_get(state.as_ptr(), snapshot_tasks_idx);
            assert!(
                !snapshot_tasks.is_null(),
                "snapshotTasks should not be null"
            );
            assert_eq!(
                leo3_ffi::array::lean_array_size(snapshot_tasks),
                0,
                "snapshotTasks should be an empty array"
            );
        }

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Core.State creation failed: {:?}",
        result.err()
    );
}

#[test]
fn test_expression_bvar() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create a bound variable
        let bvar0 = LeanExpr::bvar(lean, 0)?;

        // Check it's a bvar
        assert!(LeanExpr::is_bvar(&bvar0));
        assert!(!LeanExpr::is_app(&bvar0));

        // Extract index
        let idx = LeanExpr::bvar_idx(&bvar0)?;
        assert_eq!(idx, 0);

        Ok(())
    });

    assert!(result.is_ok(), "BVar test failed: {:?}", result.err());
}

#[test]
fn test_expression_sort() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create Prop (level zero)
        let level_zero = LeanLevel::zero(lean)?;
        let prop = LeanExpr::sort(lean, level_zero)?;

        // Check it's a sort
        assert!(LeanExpr::is_sort(&prop));

        // Create Type (level one)
        let level_one = LeanLevel::one(lean)?;
        let type0 = LeanExpr::sort(lean, level_one)?;

        assert!(LeanExpr::is_sort(&type0));

        Ok(())
    });

    assert!(result.is_ok(), "Sort test failed: {:?}", result.err());
}

#[test]
fn test_expression_const() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let name = LeanName::from_str(lean, "Nat")?;
        let levels = LeanList::nil(lean)?;
        let expr = LeanExpr::const_(lean, name.clone(), levels)?;

        assert!(LeanExpr::is_const(&expr));
        assert!(!LeanExpr::is_app(&expr));
        assert!(!LeanExpr::is_bvar(&expr));
        assert!(!LeanExpr::is_sort(&expr));

        let extracted_name = LeanExpr::const_name(&expr)?;
        assert!(LeanName::eq(&extracted_name, &name));

        let extracted_levels = LeanExpr::const_levels(&expr)?;
        assert!(!extracted_levels.as_ptr().is_null());

        Ok(())
    });

    assert!(result.is_ok(), "Const test failed: {:?}", result.err());
}

#[test]
fn test_expression_const_with_levels() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let name = LeanName::from_str(lean, "List")?;
        let level = LeanLevel::zero(lean)?;
        let levels = LeanList::cons(level.cast(), LeanList::nil(lean)?)?;
        let expr = LeanExpr::const_(lean, name.clone(), levels)?;

        assert!(LeanExpr::is_const(&expr));

        let extracted_name = LeanExpr::const_name(&expr)?;
        assert!(LeanName::eq(&extracted_name, &name));

        let extracted_levels = LeanExpr::const_levels(&expr)?;
        assert!(!extracted_levels.as_ptr().is_null());

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Const with levels test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_name_anonymous() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let anon = LeanName::anonymous(lean)?;
        assert_eq!(LeanName::kind(&anon)?, NameKind::Anonymous);

        let anon2 = LeanName::anonymous(lean)?;
        assert!(LeanName::eq(&anon, &anon2));

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Name anonymous test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_name_from_str_and_kind() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let name = LeanName::from_str(lean, "Nat")?;
        assert_eq!(LeanName::kind(&name)?, NameKind::Str);

        let same = LeanName::from_str(lean, "Nat")?;
        assert!(LeanName::eq(&name, &same));

        let different = LeanName::from_str(lean, "Int")?;
        assert!(!LeanName::eq(&name, &different));

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Name from_str test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_name_append_num() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let base = LeanName::from_str(lean, "x")?;
        let num_name = LeanName::append_num(base, lean, 1)?;
        assert_eq!(LeanName::kind(&num_name)?, NameKind::Num);

        let base2 = LeanName::from_str(lean, "x")?;
        let num_name2 = LeanName::append_num(base2, lean, 2)?;
        assert!(!LeanName::eq(&num_name, &num_name2));

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Name append_num test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_name_from_components() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let dotted = LeanName::from_components(lean, "Nat.add")?;

        let nat = LeanName::from_str(lean, "Nat")?;
        let manual = LeanName::append_str(nat, lean, "add")?;

        assert!(LeanName::eq(&dotted, &manual));
        assert_eq!(LeanName::kind(&dotted)?, NameKind::Str);

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Name from_components test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_name_nested_hierarchy() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let deep = LeanName::from_components(lean, "Std.Data.List.head")?;

        let std = LeanName::from_str(lean, "Std")?;
        let data = LeanName::append_str(std, lean, "Data")?;
        let list = LeanName::append_str(data, lean, "List")?;
        let head = LeanName::append_str(list, lean, "head")?;

        assert!(LeanName::eq(&deep, &head));

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Name nested hierarchy test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_const_name_extraction_roundtrip() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let name = LeanName::from_components(lean, "Nat.add")?;
        let levels = LeanList::nil(lean)?;
        let expr = LeanExpr::const_(lean, name, levels)?;

        let extracted = LeanExpr::const_name(&expr)?;
        let expected = LeanName::from_components(lean, "Nat.add")?;
        assert!(LeanName::eq(&extracted, &expected));

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Const name roundtrip test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_const_in_application() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let fn_name = LeanName::from_str(lean, "f")?;
        let levels = LeanList::nil(lean)?;
        let f = LeanExpr::const_(lean, fn_name, levels)?;

        let arg = LeanExpr::bvar(lean, 0)?;
        let app = LeanExpr::app(&f, &arg)?;

        assert!(LeanExpr::is_app(&app));

        let fn_expr = LeanExpr::app_fn(&app)?;
        assert!(LeanExpr::is_const(&fn_expr));

        let fn_name_extracted = LeanExpr::const_name(&fn_expr)?;
        let expected = LeanName::from_str(lean, "f")?;
        assert!(LeanName::eq(&fn_name_extracted, &expected));

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Const in application test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_expression_app() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create f and a as bound variables
        let f = LeanExpr::bvar(lean, 0)?;
        let a = LeanExpr::bvar(lean, 1)?;

        // Create application (f a)
        let app = LeanExpr::app(&f, &a)?;

        // Check it's an application
        assert!(LeanExpr::is_app(&app));

        // Extract function and argument
        let fn_expr = LeanExpr::app_fn(&app)?;
        let arg_expr = LeanExpr::app_arg(&app)?;

        assert!(LeanExpr::is_bvar(&fn_expr));
        assert!(LeanExpr::is_bvar(&arg_expr));

        assert_eq!(LeanExpr::bvar_idx(&fn_expr)?, 0);
        assert_eq!(LeanExpr::bvar_idx(&arg_expr)?, 1);

        Ok(())
    });

    assert!(result.is_ok(), "App test failed: {:?}", result.err());
}

#[test]
fn test_expression_lambda() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create λ x : Prop, x
        let x_name = LeanName::from_str(lean, "x")?;
        let prop_level = LeanLevel::zero(lean)?;
        let prop_sort = LeanExpr::sort(lean, prop_level)?;
        let x_body = LeanExpr::bvar(lean, 0)?;

        let lambda = LeanExpr::lambda(x_name.clone(), prop_sort, x_body, BinderInfo::Default)?;

        // Check it's a lambda
        assert!(LeanExpr::is_lambda(&lambda));

        // Extract parts and verify name
        let extracted_name = LeanExpr::lambda_name(&lambda)?;
        assert!(LeanName::eq(&extracted_name, &x_name));

        let binder_type = LeanExpr::lambda_type(&lambda)?;
        assert!(LeanExpr::is_sort(&binder_type));

        let body = LeanExpr::lambda_body(&lambda)?;
        assert!(LeanExpr::is_bvar(&body));

        let info = LeanExpr::lambda_info(&lambda)?;
        assert_eq!(info, BinderInfo::Default);

        Ok(())
    });

    assert!(result.is_ok(), "Lambda test failed: {:?}", result.err());
}

#[test]
fn test_expression_forall() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create ∀ (n : Prop), Prop
        let n_name = LeanName::from_str(lean, "n")?;
        let prop_level = LeanLevel::zero(lean)?;
        let prop_sort = LeanExpr::sort(lean, prop_level.clone())?;
        let prop_body = LeanExpr::sort(lean, prop_level)?;

        let forall = LeanExpr::forall(n_name, prop_sort, prop_body, BinderInfo::Default)?;

        // Check it's a forall
        assert!(LeanExpr::is_forall(&forall));

        // Extract domain
        let domain = LeanExpr::forall_domain(&forall)?;
        assert!(LeanExpr::is_sort(&domain));

        Ok(())
    });

    assert!(result.is_ok(), "Forall test failed: {:?}", result.err());
}

#[test]
fn test_expression_arrow() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create Prop → Prop
        let prop_level = LeanLevel::zero(lean)?;
        let prop1 = LeanExpr::sort(lean, prop_level.clone())?;
        let prop2 = LeanExpr::sort(lean, prop_level)?;

        let arrow = LeanExpr::arrow(prop1, prop2)?;

        // Arrow is a forall
        assert!(LeanExpr::is_forall(&arrow));

        // And it is a non-dependent arrow
        assert!(LeanExpr::is_arrow(&arrow));

        Ok(())
    });

    assert!(result.is_ok(), "Arrow test failed: {:?}", result.err());
}

#[test]
fn test_expression_literal() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create literal 42
        let lit = LeanLiteral::nat(lean, 42)?;
        let expr = LeanExpr::lit(lean, lit)?;

        // Check it's a literal
        assert!(LeanExpr::is_lit(&expr));

        Ok(())
    });

    assert!(result.is_ok(), "Literal test failed: {:?}", result.err());
}

#[test]
fn test_expression_dbg_string() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create a bound variable
        let bvar = LeanExpr::bvar(lean, 0)?;
        let dbg = LeanExpr::dbg_to_string(&bvar)?;
        assert_eq!(dbg, "#0");

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Debug string test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_expression_alpha_eqv() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create λ x, x
        let x_name = LeanName::from_str(lean, "x")?;
        let prop_level = LeanLevel::zero(lean)?;
        let prop_sort = LeanExpr::sort(lean, prop_level.clone())?;
        let x_body = LeanExpr::bvar(lean, 0)?;

        let lambda_x = LeanExpr::lambda(
            x_name,
            prop_sort.clone(),
            x_body.clone(),
            BinderInfo::Default,
        )?;

        // Create λ y, y
        let y_name = LeanName::from_str(lean, "y")?;
        let lambda_y = LeanExpr::lambda(y_name, prop_sort, x_body, BinderInfo::Default)?;

        // They should be alpha equivalent
        assert!(LeanExpr::alpha_eqv(&lambda_x, &lambda_y));

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Alpha equivalence test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_expression_mk_app() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create a function f
        let f = LeanExpr::bvar(lean, 0)?;

        // Create arguments a, b, c
        let a = LeanExpr::bvar(lean, 1)?;
        let b = LeanExpr::bvar(lean, 2)?;
        let c = LeanExpr::bvar(lean, 3)?;

        // Create f a b c
        let app = LeanExpr::mk_app(&f, &[&a, &b, &c])?;

        // Should be an application
        assert!(LeanExpr::is_app(&app));

        // The outermost application should have c as argument
        let arg = LeanExpr::app_arg(&app)?;
        assert_eq!(LeanExpr::bvar_idx(&arg)?, 3);

        Ok(())
    });

    assert!(result.is_ok(), "mk_app test failed: {:?}", result.err());
}

#[test]
fn test_level_operations() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create levels
        let _zero = LeanLevel::zero(lean)?;
        let one = LeanLevel::one(lean)?;
        let two = LeanLevel::succ(one.clone())?;

        // Create max
        let _max_level = LeanLevel::max(one.clone(), two.clone())?;

        // Create imax
        let _imax_level = LeanLevel::imax(one, two)?;

        // Create param
        let u_name = LeanName::from_str(lean, "u")?;
        let _param_level = LeanLevel::param(lean, u_name)?;

        // All operations should succeed
        Ok(())
    });

    assert!(
        result.is_ok(),
        "Level operations test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_meta_context_creation() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create a Meta.Context with default values
        let ctx = MetaContext::mk_default(lean)?;

        // Should succeed and not be null
        assert!(!ctx.as_ptr().is_null());

        // Verify it's a constructor with tag 0 (Meta.Context)
        unsafe {
            let tag = leo3_ffi::lean_obj_tag(ctx.as_ptr());
            assert_eq!(tag, 0, "Meta.Context should have constructor tag 0");

            // Verify it has 5 object fields and 8 scalar bytes (for UInt64)
            // Field 0 should be config (not null)
            let config = leo3_ffi::lean_ctor_get(ctx.as_ptr(), 0);
            assert!(!config.is_null(), "Config field should not be null");

            // Note: configKey scalar field is version-dependent and may be
            // set by the Lean runtime, so we don't assert its value here.

            // Field 1 should be zetaDeltaSet (FVarIdSet = Std.HashSet, not null)
            let zeta_delta_set = leo3_ffi::lean_ctor_get(ctx.as_ptr(), 1);
            assert!(!zeta_delta_set.is_null(), "zetaDeltaSet should not be null");

            // Field 2 should be lctx (empty LocalContext)
            let lctx = leo3_ffi::lean_ctor_get(ctx.as_ptr(), 2);
            assert!(!lctx.is_null(), "lctx should not be null");

            // Field 3 should be localInstances (empty array)
            let local_instances = leo3_ffi::lean_ctor_get(ctx.as_ptr(), 3);
            assert!(
                !local_instances.is_null(),
                "localInstances should not be null"
            );
        }

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Meta.Context creation failed: {:?}",
        result.err()
    );
}

#[test]
fn test_meta_state_creation() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create a Meta.State with empty state
        let state = MetaState::mk_meta_state(lean)?;

        // Should succeed and not be null
        assert!(!state.as_ptr().is_null());

        // Verify it's a constructor with tag 0 (Meta.State)
        unsafe {
            let tag = leo3_ffi::lean_obj_tag(state.as_ptr());
            assert_eq!(tag, 0, "Meta.State should have constructor tag 0");

            // Verify it has 5 object fields
            // Field 0 should be mctx (MetavarContext, not null)
            let mctx = leo3_ffi::lean_ctor_get(state.as_ptr(), 0);
            assert!(!mctx.is_null(), "mctx field should not be null");

            // Field 1 should be cache (Cache, not null)
            let cache = leo3_ffi::lean_ctor_get(state.as_ptr(), 1);
            assert!(!cache.is_null(), "cache field should not be null");

            // Field 2 should be zetaDeltaFVarIds (FVarIdSet, not null)
            let zeta_fvars = leo3_ffi::lean_ctor_get(state.as_ptr(), 2);
            assert!(!zeta_fvars.is_null(), "zetaDeltaFVarIds should not be null");

            // Field 3 should be postponed (PersistentArray, not null)
            let postponed = leo3_ffi::lean_ctor_get(state.as_ptr(), 3);
            assert!(!postponed.is_null(), "postponed should not be null");

            // Field 4 should be diag (Diagnostics, not null)
            let diag = leo3_ffi::lean_ctor_get(state.as_ptr(), 4);
            assert!(!diag.is_null(), "diag should not be null");
        }

        Ok(())
    });

    assert!(
        result.is_ok(),
        "Meta.State creation failed: {:?}",
        result.err()
    );
}

#[test]
fn test_metam_context_creation() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Create an empty environment
        let env = LeanEnvironment::empty(lean, 0)?;

        // Create a MetaMContext
        let ctx = MetaMContext::new(lean, env)?;

        // Verify the environment is accessible
        assert!(!ctx.env().as_ptr().is_null());

        Ok(())
    });

    assert!(
        result.is_ok(),
        "MetaMContext creation failed: {:?}",
        result.err()
    );
}

#[test]
fn test_add_axiom_to_env() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;
        let name = LeanName::from_str(lean, "myAxiom")?;
        let level_params = LeanList::nil(lean)?;
        let prop_level = LeanLevel::zero(lean)?;
        let prop = LeanExpr::sort(lean, prop_level)?;

        let decl = LeanDeclaration::axiom(lean, name.clone(), level_params, prop, false)?;

        let new_env = LeanEnvironment::add_decl_unchecked(&env, &decl)?;

        let found = LeanEnvironment::find(&new_env, &name)?;
        assert!(found.is_some(), "Axiom should be found in the environment");

        Ok(())
    });

    assert!(
        result.is_ok(),
        "add_axiom_to_env failed: {:?}",
        result.err()
    );
}

#[test]
fn test_add_invalid_decl_fails() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;

        // theorem bad : Prop := Prop (ill-typed: Prop has type Type, not Prop)
        let name = LeanName::from_str(lean, "bad")?;
        let level_params = LeanList::nil(lean)?;
        let prop_level = LeanLevel::zero(lean)?;
        let prop = LeanExpr::sort(lean, prop_level.clone())?;
        let proof = LeanExpr::sort(lean, prop_level)?;

        let decl = LeanDeclaration::theorem(lean, name, level_params, prop, proof)?;

        let result = LeanEnvironment::add_decl(&env, &decl);
        assert!(result.is_err(), "Adding ill-typed theorem should fail");

        Ok(())
    });

    assert!(
        result.is_ok(),
        "add_invalid_decl_fails test failed: {:?}",
        result.err()
    );
}

#[test]
fn test_add_theorem_to_env() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = LeanEnvironment::empty(lean, 0)?;

        // theorem id_prop : ∀ (P : Prop), P → P := fun P h => h
        // This is a valid proposition (type lives in Prop).
        let name = LeanName::from_str(lean, "id_prop")?;
        let level_params = LeanList::nil(lean)?;

        let prop_level = LeanLevel::zero(lean)?;
        let prop = LeanExpr::sort(lean, prop_level)?;

        // Type: ∀ (P : Prop), P → P
        // = forall P : Sort 0, forall _ : #0, #1
        // In de Bruijn: forall (P : Sort 0), forall (_ : bvar 0), bvar 1
        let p_name = LeanName::from_str(lean, "P")?;
        let h_name = LeanName::from_str(lean, "h")?;

        let bvar0 = LeanExpr::bvar(lean, 0)?;
        let bvar1 = LeanExpr::bvar(lean, 1)?;

        // Inner: ∀ (h : P), P  where P is bvar 0 from outer binder
        // = forall (h : bvar 0), bvar 1
        let inner_forall = LeanExpr::forall(
            h_name.clone(),
            bvar0.clone(),
            bvar1.clone(),
            BinderInfo::Default,
        )?;

        // Outer: ∀ (P : Prop), (inner)
        let type_ = LeanExpr::forall(p_name.clone(), prop, inner_forall, BinderInfo::Default)?;

        // Value: fun (P : Prop) (h : P) => h
        // = lambda P : Sort 0, lambda h : bvar 0, bvar 0
        let prop_level2 = LeanLevel::zero(lean)?;
        let prop2 = LeanExpr::sort(lean, prop_level2)?;
        let bvar0_2 = LeanExpr::bvar(lean, 0)?;
        let bvar0_3 = LeanExpr::bvar(lean, 0)?;

        let inner_lambda = LeanExpr::lambda(h_name, bvar0_2, bvar0_3, BinderInfo::Default)?;
        let value = LeanExpr::lambda(p_name, prop2, inner_lambda, BinderInfo::Default)?;

        let decl = LeanDeclaration::theorem(lean, name.clone(), level_params, type_, value)?;

        let new_env = LeanEnvironment::add_decl(&env, &decl)?;

        let found = LeanEnvironment::find(&new_env, &name)?;
        assert!(
            found.is_some(),
            "Theorem should be found in the environment"
        );

        let cinfo = found.unwrap();
        assert_eq!(LeanConstantInfo::kind(&cinfo), ConstantKind::Theorem);
        assert!(LeanConstantInfo::has_value(&cinfo));

        Ok(())
    });

    assert!(
        result.is_ok(),
        "add_theorem_to_env failed: {:?}",
        result.err()
    );
}
