//! Feature flag testing.
//!
//! These tests are intentionally compile-only.
//!
//! They verify that the crate's minimal default surface remains usable while
//! optional subsystems become available behind named features, without calling
//! into the Lean runtime. That keeps the smoke suite valid under
//! `LEO3_NO_LEAN=1`.

use leo3::prelude::*;

fn assert_into_lean<T: for<'l> IntoLean<'l>>() {}
fn assert_from_lean<T: for<'l> FromLean<'l>>() {}
fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn test_core_surface_always_available() {
    fn _bound_nat<'l>(_: LeanBound<'l, LeanNat>) {}
    fn _ref_string(_: LeanRef<LeanString>) {}
    fn _borrowed_array<'a, 'l>(_: LeanBorrowed<'a, 'l, LeanArray>) {}
    fn _result(_: LeanResult<()>) {}

    assert_into_lean::<usize>();
    assert_from_lean::<usize>();
    assert_into_lean::<()>();
    assert_from_lean::<()>();
    assert_into_lean::<String>();
    assert_from_lean::<String>();
    assert_send_sync::<LeanUnbound<LeanNat>>();
}

#[cfg(feature = "macros")]
mod macros_feature {
    use super::*;

    #[derive(Debug, PartialEq, IntoLean, FromLean)]
    struct FeaturePair(u64, u64);

    #[test]
    fn test_macros_feature_surface() {
        assert_into_lean::<FeaturePair>();
        assert_from_lean::<FeaturePair>();
    }
}

#[cfg(feature = "meta")]
#[test]
fn test_meta_feature_surface() {
    fn _env<'l>(_: LeanBound<'l, leo3::meta::LeanEnvironment>) {}
    fn _expr<'l>(_: LeanBound<'l, leo3::meta::LeanExpr>) {}
    fn _ctx<'l>(_: leo3::meta::MetaMContext<'l>) {}
}

#[cfg(feature = "io")]
#[test]
fn test_io_feature_surface() {
    fn _io<'l>(_: leo3::io::LeanIO<'l, String>) {}
    fn _io_result(_: leo3::io::IOResult<String>) {}
    fn _io_error(_: leo3::io::IOError) {}
}

#[cfg(feature = "task")]
#[test]
fn test_task_feature_surface() {
    fn _task<'l>(_: leo3::task::LeanTask<'l, LeanNat>) {}
    fn _handle(_: leo3::task::TaskHandle<leo3::instance::LeanAny>) {}
}

#[cfg(feature = "task")]
#[test]
fn test_promise_feature_surface() {
    fn _promise<'l>(_: leo3::promise::LeanPromise<'l, leo3::instance::LeanAny>) {}
}

#[cfg(lean_4_22)]
#[test]
fn test_containers_surface() {
    fn _hash_map<'l>(_: leo3::types::LeanHashMap<'l, LeanNat, LeanString>) {}
    fn _hash_set<'l>(_: leo3::types::LeanHashSet<'l, LeanNat>) {}
    fn _rb_map<'l>(_: leo3::types::LeanRBMap<'l, LeanNat, LeanString>) {}
}

#[cfg(feature = "module-loading")]
#[test]
fn test_module_loading_feature_surface() {
    fn _module(_: leo3::module::LeanModule) {}
    fn _function(_: leo3::module::LeanFunction<'_>) {}
}

#[cfg(feature = "tokio")]
#[test]
fn test_tokio_feature_surface() {
    let _block_in_place = leo3::tokio_bridge::lean_block_in_place::<fn() -> usize, usize>;
}
