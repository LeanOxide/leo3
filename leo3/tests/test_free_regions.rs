//! Regression test for W-407 Bug A: repeated `importModules` must not
//! permanently leak the C++ `compacted_region` buffers (one ~1.4 GB per
//! import of `Lean`) when the caller releases them via
//! `LeanEnvironment::free_regions`.
//!
//! This is a **standalone test binary** on purpose: the metric below
//! observes the whole process, and any other test sharing the process
//! would interleave its allocations on the same worker and contaminate
//! the measured delta.
//!
//! ## Metric: process RSS (Linux)
//!
//! Each `import_modules(["Lean"])` compacts the olean payload into
//! C++ `compacted_region` buffers attached to the environment header
//! (`env.header.regions`); only `Environment.freeRegions` releases them,
//! and the stock runtime only calls it from the one-shot `lean` CLI path.
//! Without the fix, the buffer outlives the environment object and RSS
//! grows ~1.4–1.6 GB per import cycle; with `free_regions`, the buffer is
//! released in the same cycle and RSS must stay flat across iterations
//! (the W-407 probe measured it flat at ~180 MB over 8 iterations).
//!
//! The threshold is set far below one region buffer (~1.4 GB) and far
//! above allocator noise, so it catches the unbounded per-import growth
//! while tolerating benign transient allocations.

#![cfg(all(
    feature = "meta",
    feature = "runtime-tests",
    not(target_os = "windows"),
    lean_4_25
))]

use leo3::meta::*;
use leo3::prelude::*;

#[cfg(target_os = "linux")]
fn rss_bytes() -> u64 {
    let s = std::fs::read_to_string("/proc/self/statm").expect("statm");
    let resident_pages: u64 = s.split_whitespace().nth(1).unwrap().parse().unwrap();
    resident_pages * 4096
}

#[cfg(not(target_os = "linux"))]
fn rss_bytes() -> u64 {
    0
}

#[test]
fn test_free_regions_keeps_rss_flat_across_imports() {
    const ITERS: u32 = 8;
    // One compacted_region buffer for a full `Lean` import is ~1.4 GB; a
    // leaking run grows by that much per iteration (11 GB over 8), while a
    // healthy run stays flat. 256 MiB of headroom absorbs allocator/GC
    // noise without masking a single leaked buffer.
    const MAX_RSS_GROWTH: u64 = 256 * 1024 * 1024;

    let result: LeanResult<()> = leo3::test_with_lean(|lean| {
        // Warm up: the first import pays one-time costs (module cache,
        // arenas, worker-thread lazy init) that must not count toward the
        // delta.
        {
            let env = import_modules(lean, &["Lean"], 0)?;
            env.free_regions()?;
        }
        let rss_before = rss_bytes();
        for _ in 0..ITERS {
            let env = import_modules(lean, &["Lean"], 0)?;
            env.free_regions()?;
        }
        let rss_after = rss_bytes();
        let growth = rss_after.saturating_sub(rss_before);
        eprintln!(
            "[free-regions-probe] {ITERS} import+free_regions cycles: RSS {rss_before} -> \
             {rss_after} (growth {growth})"
        );
        assert!(
            growth <= MAX_RSS_GROWTH,
            "process RSS grew by {growth} bytes over {ITERS} import+free_regions cycles \
             (compacted_region buffers not released, W-407 Bug A)"
        );
        Ok(())
    });
    result.expect("free_regions RSS probe failed");
}
