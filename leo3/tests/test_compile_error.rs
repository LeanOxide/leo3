//! Compile-time error tests using trybuild
//!
//! These tests ensure that invalid code produces helpful error messages.
//! Inspired by PyO3's UI tests.

#[test]
fn ui_tests() {
    // `cargo careful` instruments the standard library and propagates that
    // via CARGO_ENCODED_RUSTFLAGS into trybuild's nested cargo build. The
    // instrumented std changes how rustc renders the closure return type in
    // the borrowck diagnostic (full type names vs `_` placeholders), which
    // would make the committed snapshot depend on the parent build's flags.
    // Strip the inherited flags so the trybuild sub-build always uses the
    // plain host toolchain, keeping the snapshot deterministic across
    // regular and `cargo careful` CI runs.
    std::env::remove_var("CARGO_ENCODED_RUSTFLAGS");
    std::env::remove_var("CARGO_ENCODED_RUSTDOCFLAGS");

    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
