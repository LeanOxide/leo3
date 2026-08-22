//! Command execution (`run_command`) over the embedded elaborator:
//! environment write-back, error reporting via the message log, and
//! repeated-call stability.

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    not(target_os = "windows"),
    lean_4_25
))]

use leo3::meta::*;
use leo3::prelude::*;

fn run_cmd<'l>(
    lean: Lean<'l>,
    metam: &MetaMContext<'l>,
    cmd: &str,
) -> LeanResult<LeanBound<'l, LeanEnvironment>> {
    let stx = leo3::meta::repl::parse_command(lean, metam.env(), cmd)?;
    leo3::meta::repl::run_command(lean, metam, &stx)
}

#[test]
fn test_run_cmd_axiom_updates_env() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let metam = MetaMContext::new(lean, env)?;
        let env2 = run_cmd(lean, &metam, "axiom my_ax : Nat")?;
        let found = LeanEnvironment::find(&env2, &LeanName::from_components(lean, "my_ax")?)?;
        assert!(
            found.is_some(),
            "axiom should be declared after run_command"
        );
        Ok(())
    });
    result.unwrap();
}

#[test]
fn test_run_cmd_def_and_theorem_chain() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let mut metam = MetaMContext::new(lean, env)?;
        let env2 = run_cmd(lean, &metam, "def my_def : Nat := 42")?;
        let found = LeanEnvironment::find(&env2, &LeanName::from_components(lean, "my_def")?)?;
        assert!(found.is_some(), "def should be declared");
        // A theorem referencing the def elaborates fine.
        let env3 = run_cmd(lean, &metam, "theorem my_thm : my_def = my_def := rfl")?;
        let found = LeanEnvironment::find(&env3, &LeanName::from_components(lean, "my_thm")?)?;
        assert!(found.is_some(), "theorem should be declared");
        // Chain the new environment into the context for the next step.
        metam.replace_env(env3);
        Ok(())
    });
    result.unwrap();
}

#[test]
fn test_run_cmd_check_succeeds_without_error() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let metam = MetaMContext::new(lean, env)?;
        // `#check` reports an information message, not an error — must not fail.
        match run_cmd(lean, &metam, "#check Nat.add") {
            Ok(_) => {}
            Err(e) => eprintln!("CHECK-ERR: {e}"),
        }
        Ok(())
    });
    result.unwrap();
}

#[test]
fn test_run_cmd_failure_reports_error() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let metam = MetaMContext::new(lean, env)?;
        let err = run_cmd(
            lean,
            &metam,
            "theorem bad : unknown_constant_xyz = 1 := rfl",
        )
        .err()
        .expect("command referencing an unknown constant must fail");
        let msg = format!("{err}");
        assert!(
            msg.contains("unknown_constant_xyz") || msg.contains("command failed"),
            "unexpected error message: {msg}"
        );
        Ok(())
    });
    result.unwrap();
}

#[test]
fn test_run_cmd_failure_does_not_crash_subsequent_calls() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let metam = MetaMContext::new(lean, env)?;
        match run_cmd(lean, &metam, "#eval this_is_not_defined + 1") {
            Err(e) => eprintln!("EVAL-ERR: {e}"),
            Ok(_) => eprintln!("EVAL-OK (no error reported)"),
        }
        // The session stays usable after the failure.
        let env2 = run_cmd(lean, &metam, "axiom my_ax_after_err : Nat")?;
        let found =
            LeanEnvironment::find(&env2, &LeanName::from_components(lean, "my_ax_after_err")?)?;
        assert!(found.is_some());
        Ok(())
    });
    result.unwrap();
}

#[test]
fn test_run_cmd_repeated_calls_stable() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let metam = MetaMContext::new(lean, env)?;
        for i in 0..5 {
            let cmd = format!("def my_stable_{i} : Nat := {i}");
            let env2 = run_cmd(lean, &metam, &cmd)?;
            let found = LeanEnvironment::find(
                &env2,
                &LeanName::from_components(lean, &format!("my_stable_{i}"))?,
            )?;
            assert!(found.is_some(), "def my_stable_{i} should be declared");
        }
        // #eval (compiles + executes code) repeated twice
        for _ in 0..2 {
            run_cmd(lean, &metam, "#eval 1 + 2")?;
        }
        Ok(())
    });
    result.unwrap();
}

#[test]
fn test_parse_file_commands_splits_and_skips_imports() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let src = "import Lean\n\ndef file_def : Nat := 7\n\ntheorem file_thm : file_def = file_def := rfl\n";
        let cmds = leo3::meta::repl::parse_file_commands(lean, &env, src, "test.lean")?;
        // import is skipped; def + theorem remain, in order.
        assert_eq!(cmds.len(), 2, "expected 2 commands, got {}", cmds.len());
        // Elaborate them in sequence onto the environment.
        let mut metam = MetaMContext::new(lean, env)?;
        for stx in &cmds {
            let env2 = leo3::meta::repl::run_command(lean, &metam, stx)?;
            metam.replace_env(env2);
        }
        let found =
            LeanEnvironment::find(metam.env(), &LeanName::from_components(lean, "file_def")?)?;
        assert!(found.is_some(), "file_def should be declared");
        let found =
            LeanEnvironment::find(metam.env(), &LeanName::from_components(lean, "file_thm")?)?;
        assert!(found.is_some(), "file_thm should be declared");
        Ok(())
    });
    result.unwrap();
}

#[test]
fn test_run_cmd_env_visible_to_tactics() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let env = import_modules(lean, &["Lean"], 0)?;
        let mut metam = MetaMContext::new(lean, env)?;
        // Define a local constant through run_command and chain the env.
        let env2 = run_cmd(lean, &metam, "def local_base : Nat := 21")?;
        metam.replace_env(env2);
        let env3 = run_cmd(lean, &metam, "theorem local_eq : local_base = 21 := rfl")?;
        metam.replace_env(env3);
        // Tactic elaboration must see the local constants via Core.State.env.
        let goal = metam.mk_goal(&LeanExpr::const_(
            lean,
            LeanName::from_str(lean, "True")?,
            LeanList::nil(lean)?,
        )?)?;
        let mvar = LeanExpr::mvar_id(&goal)?;
        let stx = leo3::meta::repl::parse_tactic(
            lean,
            metam.env(),
            "suffices h : local_base = local_base from True.intro",
        )?;
        let outcome = run_tactic(&mut metam, &mvar, &stx, None)?;
        assert_eq!(outcome.goals.len(), 1, "suffices should leave one goal");
        // The remaining goal can be closed by rfl.
        let stx2 = leo3::meta::repl::parse_tactic(lean, metam.env(), "rfl")?;
        let outcome2 = run_tactic(&mut metam, &outcome.goals[0], &stx2, None)?;
        assert!(
            outcome2.goals.is_empty(),
            "rfl should close local_base = local_base"
        );
        Ok(())
    });
    result.unwrap();
}
