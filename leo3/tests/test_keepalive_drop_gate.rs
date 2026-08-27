//! Regression for the drop-time all-file-backed gate (W-417).
//!
//! `free_regions` releases *every* compacted region, including any **heap**
//! region (the `malloc` buffer the Lean `module.cpp` fallback uses when a
//! deterministic `mmap` base is already occupied). A heap region's
//! `g_native_symbol_cache` keys point into that buffer and cannot be re-mapped
//! once freed, so freeing a heap/mixed environment dangles them and the next
//! import's symbol lookup SIGSEGVs.
//!
//! The gate (`safe_to_free_regions`) frees an environment only when its live
//! region count equals the number of lean file VMAs its import added (each
//! file-backed region is one `mmap`; a heap region adds none). This test
//! exercises that predicate against real environments:
//!
//! - a first import of `Lean` is fully file-backed (count == VMAs added) and
//!   must be classified safe to free;
//! - a second import while the first is still alive falls back to the heap for
//!   every region (the first holds the deterministic bases), adds no file VMAs,
//!   and must be classified NOT safe to free.

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    target_os = "linux",
    lean_4_25
))]

use leo3::meta::*;
use leo3::prelude::*;

#[test]
fn drop_gate_frees_file_backed_and_leaks_heap_backed() {
    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Env A: first import of Lean, all regions file-backed.
        let before_a = snapshot_lean_vmras().expect("maps readable before A");
        let a = import_modules(lean, &["Lean"], 0)?.unbind_mt();
        let after_a = snapshot_lean_vmras().expect("maps readable after A");
        let added_a = diff_added_vmras(&before_a, &after_a).len() as u64;
        let count_a = environment_region_count(&a);
        assert!(
            safe_to_free_regions(count_a, added_a),
            "a fully file-backed environment must be safe to free (count={count_a:?}, added={added_a})",
        );

        // Env B: second import while A is still alive. A holds every
        // deterministic base, so B's regions fall back to the heap: same region
        // count, but zero new file VMAs. It must NOT be safe to free.
        let before_b = snapshot_lean_vmras().expect("maps readable before B");
        let b = import_modules(lean, &["Lean"], 0)?.unbind_mt();
        let after_b = snapshot_lean_vmras().expect("maps readable after B");
        let added_b = diff_added_vmras(&before_b, &after_b).len() as u64;
        let count_b = environment_region_count(&b);
        assert!(
            !safe_to_free_regions(count_b, added_b),
            "a heap-backed environment must NOT be safe to free (count={count_b:?}, added={added_b})",
        );

        // Both envs are still alive; their `LeanUnbound` drop does a plain
        // `lean_dec` (no `free_regions`), so no region is freed here and no
        // cache key dangles.
        drop(b);
        drop(a);
        Ok(())
    });
    result.expect("drop-gate regression failed");
}
