//! W-417 stopgap regression: `free_regions` on an elaborated environment,
//! followed by a fresh `import_modules`, must not crash. Elaboration adds
//! entries to the Lean runtime's global native-symbol cache; `free_regions`
//! unmaps the environment's compacted regions, which would dangle those cache
//! keys. The next import's symbol lookup must not dereference them.

#![cfg(all(feature = "meta", feature = "runtime-tests", lean_4_25))]

use leo3::meta::*;
use leo3::prelude::*;

#[test]
fn free_regions_after_elab_keeps_next_import_crash_free() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        for _ in 0..6u32 {
            let env = import_modules(lean, &["Lean"], 0)?;
            let mut metam = MetaMContext::new(lean, env)?;
            let ty = LeanExpr::const_(
                lean,
                LeanName::from_str(lean, "True")?,
                LeanList::nil(lean)?,
            )?;
            let goal = metam.mk_goal(&ty)?;
            let mvar = LeanExpr::mvar_id(&goal)?;
            let stx = parse_tactic(lean, metam.env(), "exact True.intro")?;
            let outcome = run_tactic(&mut metam, &mvar, &stx, None)?;
            assert!(outcome.goals.is_empty(), "goal must close");
            let (elab_env, core_ctx, core_state, meta_ctx, meta_state) = metam.into_parts();
            drop(core_ctx);
            drop(core_state);
            drop(meta_ctx);
            drop(meta_state);
            // Safety: `elab_env` is the last live reference to the imported
            // environment; all derived objects are dropped above.
            unsafe { elab_env.free_regions(&["Lean"]) }?;
        }
        // The next import must not crash.
        let _env = import_modules(lean, &["Lean"], 0)?;
        Ok(())
    });
    result.expect("free_regions after elab: next import crashed");
}
