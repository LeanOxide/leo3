//! W-417 stopgap regression (cross-set): `free_regions` on one module set
//! unmaps its compacted lean regions (all four suffixes), dangling the global
//! native-symbol-cache keys that point into them. A *cross-set* import — one
//! whose module set differs from the freed set — does not re-map the freed set's
//! base, so its symbol lookups would dereference the dangling keys and crash
//! unless `remap_cross_set_bases` re-maps the freed regions first.
//!
//! Unlike `test_elab_free_regions` (same-set `Lean → free → Lean`, where the
//! re-map is a no-op and the import self-heals by re-mapping its own base),
//! this test frees set A and then asks the keepalive to re-map for a
//! *different* set, which is the case where the keepalive actually does work.
//! It verifies:
//!   1. all four lean suffixes (`.olean`, `.olean.server`, `.olean.private`,
//!      `.ir`) were among the freed regions;
//!   2. a same-set re-map is a no-op (the base must not be pinned);
//!   3. a cross-set re-map restores every freed region.

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    target_os = "linux",
    lean_4_25
))]

use leo3::meta::*;
use leo3::prelude::*;

/// The lean import-region file suffixes that `free_regions` unmaps.
const SUFFIXES: [&str; 4] = [".olean", ".olean.server", ".olean.private", ".ir"];

/// Count of `freed` regions that are mapped again in `remapped` (by addr+len).
fn remapped_count(freed: &[LeanVma], remapped: &[LeanVma]) -> usize {
    freed
        .iter()
        .filter(|v| remapped.iter().any(|r| r.addr == v.addr && r.len == v.len))
        .count()
}

#[test]
fn cross_set_remap_restores_freed_regions() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Import set A and capture its lean regions.
        let env = import_modules(lean, &["Lean"], 0)?;
        let before = snapshot_lean_vmras();
        // Free set A's regions (consumes `env`).
        unsafe { env.free_regions() }?;
        let after = snapshot_lean_vmras();
        let freed = diff_freed_vmras(&before, &after);
        assert!(
            !freed.is_empty(),
            "free_regions unmapped no lean regions (test harness broken?)"
        );
        // All four suffixes must be among the freed regions.
        for suffix in SUFFIXES {
            assert!(
                freed.iter().any(|v| v.path.ends_with(suffix)),
                "no freed {suffix} region — is the suffix coverage incomplete?"
            );
        }
        // Record the freed set for cross-set revival.
        record_freed_set(&["Lean"], freed.clone());

        // A same-set re-map must be a NO-OP: the freed set's base must not be
        // pinned (pinning would push same-set imports onto the heap and
        // inflate RSS).
        remap_cross_set_bases(&["Lean"]).expect("same-set re-map errored");
        let same = snapshot_lean_vmras();
        assert_eq!(
            remapped_count(&freed, &same),
            0,
            "same-set re-map must be a no-op, but re-mapped freed regions"
        );

        // A cross-set re-map must restore every freed region (all four
        // suffixes) so the next import's lookups do not dangle.
        remap_cross_set_bases(&["Other.Set"]).expect("cross-set re-map errored");
        let remapped = snapshot_lean_vmras();
        let restored = remapped_count(&freed, &remapped);
        assert_eq!(
            restored,
            freed.len(),
            "cross-set re-map restored only {restored}/{total} freed lean regions",
            total = freed.len()
        );
        Ok(())
    });
    result.expect("cross-set re-map test failed");
}
