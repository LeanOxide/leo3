//! Temporary probe to localize the SIGSEGV in the repl FFI chain.
#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    not(target_os = "windows"),
    lean_4_25
))]

use leo3::meta::*;
use leo3::prelude::*;

#[test]
fn probe_search_path() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        eprintln!("[probe] in with_lean");
        let sysroot = "/home/ljm/.lemma/toolchains/v4.25.2-linux";
        match init_search_path(lean, sysroot) {
            Ok(()) => eprintln!("[probe] init_search_path OK"),
            Err(e) => eprintln!("[probe] init_search_path ERR: {e}"),
        }
        let env = match import_modules(lean, &["Init"], 0) {
            Ok(env) => {
                eprintln!("[probe] import_modules OK");
                env
            }
            Err(e) => {
                eprintln!("[probe] import_modules ERR: {e}");
                return Ok(());
            }
        };
        match parse_tactic(lean, &env, "rfl") {
            Ok(_) => eprintln!("[probe] parse_tactic OK"),
            Err(e) => eprintln!("[probe] parse_tactic ERR: {e}"),
        }
        Ok(())
    });
    match result {
        Ok(()) => eprintln!("[probe] outer OK"),
        Err(e) => eprintln!("[probe] outer ERR: {e}"),
    }
}

#[test]
fn probe_import_variants() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        let sysroot = "/home/ljm/.lemma/toolchains/v4.25.2-linux";
        init_search_path(lean, sysroot)?;
        eprintln!("[probe2] search path set");
        // Variant A: empty imports
        match import_modules(lean, &[], 0) {
            Ok(_) => eprintln!("[probe2] empty imports OK"),
            Err(e) => eprintln!("[probe2] empty imports ERR: {e}"),
        }
        // Variant B: bogus module name — distinguishes Import-array
        // construction from Init data loading.
        match import_modules(lean, &["BogusModule"], 0) {
            Ok(_) => eprintln!("[probe2] Bogus OK"),
            Err(e) => eprintln!("[probe2] Bogus ERR: {e}"),
        }
        // Variant C: real Init
        match import_modules(lean, &["Init"], 0) {
            Ok(_) => eprintln!("[probe2] Init OK"),
            Err(e) => eprintln!("[probe2] Init ERR: {e}"),
        }
        Ok(())
    });
    match result {
        Ok(()) => eprintln!("[probe2] outer OK"),
        Err(e) => eprintln!("[probe2] outer ERR: {e}"),
    }
}
