//! W-417 stopgap: revive dangling `g_native_symbol_cache` keys across
//! *different* module sets, without pinning the lean base mapping in the
//! common (same-set) case.
//!
//! ## Why the cache keys dangle
//!
//! The Lean runtime's global `g_native_symbol_cache` (`src/ir/ir_interpreter.cpp`)
//! is keyed by `name` *content*, but holds a *borrowed* pointer to the key
//! object. For import-derived names that object lives in the environment's
//! compacted import region (an `mmap` of the `.olean`/`.ir` sidecar files).
//! `Environment.freeRegions` (bound here as `free_regions`) `munmap`s those
//! regions, so the cached key pointers dangle. A later import's symbol lookup
//! walks the rb-tree and dereferences them → SIGSEGV.
//!
//! The base address of each lean-region mapping is deterministic
//! (`hash(module_name) % 0x7f00_0000_0000`, 64 KiB aligned — `module.cpp`).
//! Re-`mmap`ing the same file at the same address restores the exact bytes the
//! key pointed at, so the key "revives" and compares equal to (and aliases) the
//! name the next import produces for the same module.
//!
//! ## Same set vs. cross set
//!
//! - **Same set** (import A, free A, import A again): the second import
//!   re-`mmap`s A's base *before* its lookups run, so it self-heals. Nothing to
//!   do — and we must NOT pin the base here, because holding it would force
//!   every subsequent same-set import onto the heap (the base is already
//!   mapped), which measurably inflates RSS.
//! - **Cross set** (import A, free A, import B with B ≠ A): B's import does
//!   *not* re-map A's base, so A's cached keys stay dangling and B's lookup
//!   crashes. This is the case we fix: before running a cross-set import we
//!   re-`mmap` the freed set's lean regions at their original addresses.
//!
//! The re-map is therefore **lazy**: it happens at import time, only for freed
//! sets that differ from the set being imported. The re-mapped regions are left
//! mapped (orphaned — no environment owns them), which is the stopgap cost:
//! each *distinct* freed set that is later crossed pins one resident copy of its
//! lean regions. The true fix is upstream (Option 1: content-owning cache keys);
//! see the issue thread.
//!
//! ## Platforms
//!
//! This stopgap is **Linux-only**: it reads `/proc/self/maps` and relies on
//! `mmap(MAP_FIXED_NOREPLACE)` (kernel ≥ 4.17). It is compiled out elsewhere
//! (see `mod.rs`), and leotower releases the environment with a plain
//! `lean_dec` instead of `free_regions` there — leaking the compacted regions
//! (the W-407 cost) but, crucially, not dangling the cache keys.
//!
//! ## Re-map safety
//!
//! The re-map must never clobber a live mapping. `remap_vma` verifies the file
//! identity recorded at snapshot time (inode + size), then uses
//! `MAP_FIXED_NOREPLACE`, which *fails* (instead of replacing) if the range is
//! already mapped. If the range is occupied by the identical mapping (same
//! inode + file offset) the re-map is treated as idempotent success; if it is
//! occupied by anything else, a failure is returned to the caller, which must
//! block the import rather than proceed with possibly-dangling keys.
//!
//! ## Recorded state
//!
//! On drop, leotower snapshots `/proc/self/maps` before and after
//! `free_regions`; the lean VMAs that disappear are the freed set's regions and
//! are recorded here (with the set's module key). `remap_cross_set_bases`
//! re-mmaps the regions of every recorded set whose key differs from the set
//! about to be imported.
//!
//! Set `LEO3_KEEPALIVE_DISABLE=1` to turn the whole stopgap off (snapshots
//! return empty, recording and re-mapping become no-ops) for A/B measurement.
//! The environment is still released on drop (the W-407 region fix is
//! independent), but cross-set imports after a free can then crash — which is
//! the point of the switch.

use std::collections::HashSet;
use std::fmt;
use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::os::unix::io::AsRawFd;
use std::sync::{LazyLock, Mutex};

use libc::{c_void, off_t, MAP_FAILED, MAP_FIXED_NOREPLACE, MAP_PRIVATE, PROT_READ};

/// A single lean import-region virtual memory area, as seen in
/// `/proc/self/maps`.
///
/// `inode` + `offset` + `len` + `path` together identify the exact file
/// mapping, so the re-map can verify it is restoring the right bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeanVma {
    /// VMA start address (the deterministic lean base, header included).
    pub addr: usize,
    /// VMA length in bytes.
    pub len: usize,
    /// File offset the VMA is mapped at.
    pub offset: u64,
    /// Inode of the backing file, captured at snapshot time.
    pub inode: u64,
    /// The `.olean`/`.ir` file backing the mapping.
    pub path: String,
}

/// A module set that was freed, together with the lean regions that
/// `free_regions` unmapped for it.
#[derive(Debug)]
struct FreedSet {
    /// Normalized module key (sorted, NUL-joined) identifying the set.
    key: String,
    /// The lean VMAs that were freed (and now need re-mapping on a cross-set
    /// import).
    vmras: Vec<LeanVma>,
    /// Set once the regions have been re-mapped; the re-map is idempotent and
    /// the regions then stay mapped, so we never redo it.
    remapped: bool,
}

/// One failure to re-map a freed lean region. Carried back to the caller, which
/// must block the import rather than proceed with dangling cache keys.
#[derive(Debug)]
pub enum RemapError {
    /// The recorded file no longer exists or could not be opened.
    OpenFailed {
        /// The path that could not be opened.
        path: String,
        /// The OS error from `open`.
        source: io::Error,
    },
    /// The file at the recorded path no longer matches the recorded inode
    /// (deleted + recreated at the same path gets a new inode), or its
    /// metadata could not be read.
    IdentityMismatch {
        /// The path whose on-disk file no longer matches the snapshot.
        path: String,
        /// Human-readable reason (inode mismatch / stat error).
        detail: String,
    },
    /// The target address range is already occupied by a *different* mapping;
    /// the re-map was refused rather than clobbering it.
    RangeOccupied {
        /// The VMA start address that was already mapped.
        addr: usize,
    },
    /// The `mmap` syscall failed for a reason other than "already mapped".
    MmapFailed {
        /// The path that could not be mapped.
        path: String,
        /// The OS error from `mmap`.
        source: io::Error,
    },
}

impl fmt::Display for RemapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RemapError::OpenFailed { path, source } => write!(f, "cannot open {path}: {source}"),
            RemapError::IdentityMismatch { path, detail } => write!(f, "{path}: {detail}"),
            RemapError::RangeOccupied { addr } => write!(
                f,
                "address {addr:#x} is already mapped by a different region; refusing to overwrite it"
            ),
            RemapError::MmapFailed { path, source } => {
                write!(f, "mmap {path} at its recorded address failed: {source}")
            }
        }
    }
}

static FREED_SETS: LazyLock<Mutex<Vec<FreedSet>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Diagnostic / A-B switch: set `LEO3_KEEPALIVE_DISABLE=1` to turn the whole
/// stopgap off.
static DISABLED: LazyLock<bool> =
    LazyLock::new(|| std::env::var_os("LEO3_KEEPALIVE_DISABLE").is_some());

/// File suffixes of the lean import regions that `free_regions` unmaps.
///
/// Each imported module maps a main `.olean` plus a `.olean.server`, a
/// `.olean.private`, and an `.ir` sidecar; all four carry `name` objects that
/// can be cached as `g_native_symbol_cache` keys, so all four must be revived.
const LEAN_SUFFIXES: &[&str] = &[".olean", ".olean.server", ".olean.private", ".ir"];

fn is_lean_region(path: &str) -> bool {
    LEAN_SUFFIXES.iter().any(|s| path.ends_with(s))
}

/// Normalize a module list into a stable identity key.
fn key_of(modules: &[&str]) -> String {
    let mut v: Vec<&str> = modules.to_vec();
    v.sort_unstable();
    v.dedup();
    v.join("\0")
}

/// A parsed `/proc/self/maps` line.
#[derive(Debug)]
struct MapsVma {
    start: usize,
    end: usize,
    offset: u64,
    inode: u64,
    path: String,
}

/// Parse one `/proc/self/maps` line: `start-end perm offset dev inode [pad]
/// path`. The inode and the path are separated by variable whitespace, so take
/// the first five fields and treat the trimmed remainder as the path.
fn parse_maps_line(line: &str) -> Option<MapsVma> {
    let mut parts = line.splitn(6, ' ');
    let (start_s, end_s) = parts.next()?.split_once('-')?;
    let start = usize::from_str_radix(start_s, 16).ok()?;
    let end = usize::from_str_radix(end_s, 16).ok()?;
    let _perm = parts.next()?;
    let offset = u64::from_str_radix(parts.next()?, 16).ok()?;
    let _dev = parts.next()?;
    let inode = parts.next()?.parse::<u64>().ok()?;
    let path = parts.next().unwrap_or("").trim_start().to_string();
    Some(MapsVma {
        start,
        end,
        offset,
        inode,
        path,
    })
}

fn read_maps() -> Option<String> {
    std::fs::read_to_string("/proc/self/maps").ok()
}

/// Snapshot the lean-backed VMAs currently mapped in this process.
///
/// Reads `/proc/self/maps` and returns one entry per mapping whose target path
/// is a lean import-region file (`.olean`, `.olean.server`, `.olean.private`,
/// or `.ir`). Used to bracket `free_regions` and diff out the VMAs it unmaps.
///
/// Returns empty when the stopgap is disabled (`LEO3_KEEPALIVE_DISABLE`) or
/// `/proc/self/maps` is unreadable; leotower treats an empty snapshot as "do
/// not free the regions" (see its `EnvRegions::drop`).
pub fn snapshot_lean_vmras() -> Vec<LeanVma> {
    if *DISABLED {
        return Vec::new();
    }
    let Some(maps) = read_maps() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for line in maps.lines() {
        let Some(m) = parse_maps_line(line) else {
            continue;
        };
        if m.path.is_empty() || !is_lean_region(&m.path) || m.end <= m.start {
            continue;
        }
        out.push(LeanVma {
            addr: m.start,
            len: m.end - m.start,
            offset: m.offset,
            inode: m.inode,
            path: m.path,
        });
    }
    out
}

/// Return the VMAs present in `before` but absent from `after` — i.e. the lean
/// regions that disappeared between the two snapshots (the ones
/// `free_regions` unmapped).
pub fn diff_freed_vmras(before: &[LeanVma], after: &[LeanVma]) -> Vec<LeanVma> {
    let after_set: HashSet<(usize, usize)> = after.iter().map(|v| (v.addr, v.len)).collect();
    before
        .iter()
        .filter(|v| !after_set.contains(&(v.addr, v.len)))
        .cloned()
        .collect()
}

/// Record a freed module set and the lean regions `free_regions` unmapped for
/// it. Called by leotower after `free_regions`, with the VMAs diffed out from
/// before/after `/proc/self/maps` snapshots. Sets with no freed lean regions
/// (e.g. an env whose data was heap-allocated) record nothing.
///
/// Sets are merged by key: re-recording the same module set de-dups by
/// `(addr, len)` instead of stacking duplicate records, and any newly-freed
/// address marks the set as needing a (re-)map again.
pub fn record_freed_set(modules: &[&str], freed_vmras: Vec<LeanVma>) {
    if *DISABLED || freed_vmras.is_empty() {
        return;
    }
    let key = key_of(modules);
    let mut guard = FREED_SETS.lock().unwrap();
    if let Some(set) = guard.iter_mut().find(|s| s.key == key) {
        let mut known: HashSet<(usize, usize)> =
            set.vmras.iter().map(|v| (v.addr, v.len)).collect();
        let mut added = 0;
        for vma in freed_vmras {
            if known.insert((vma.addr, vma.len)) {
                set.vmras.push(vma);
                added += 1;
            }
        }
        if added > 0 {
            set.remapped = false;
        }
    } else {
        guard.push(FreedSet {
            key,
            vmras: freed_vmras,
            remapped: false,
        });
    }
}

/// Re-`mmap` the lean regions of every freed set whose module set differs from
/// `importing`, so that the next (cross-set) import's symbol lookups do not
/// dereference dangling cache keys.
///
/// Must be called **before** the import runs (the import's `finalizeImport` →
/// `evalConst` → symbol lookup is where the dangling keys are dereferenced).
/// Same-set imports are a no-op here: they self-heal by re-mapping their own
/// base before their lookups, and pinning the base would needlessly push them
/// onto the heap.
///
/// Returns an error (never panics) if any region could not be *safely* re-mapped;
/// the caller **must block the import** in that case, because proceeding would
/// risk the original dangling-key SIGSEGV. On failure the sets are left
/// retryable — state is only committed when *every* region succeeds.
pub fn remap_cross_set_bases(importing: &[&str]) -> Result<(), Vec<RemapError>> {
    if *DISABLED {
        return Ok(());
    }
    let import_key = key_of(importing);
    let mut guard = FREED_SETS.lock().unwrap();
    // Collect candidate sets (freed, not the importing set, not yet remapped)
    // and the de-duplicated VMAs to re-map, as (set, vma) indices so `guard`
    // can be mutated afterwards.
    let mut dedup: HashSet<(usize, usize)> = HashSet::new();
    let mut to_remap: Vec<(usize, usize)> = Vec::new();
    let mut candidates: Vec<usize> = Vec::new();
    for (i, set) in guard.iter().enumerate() {
        if set.remapped || set.key == import_key || set.vmras.is_empty() {
            continue;
        }
        for (j, vma) in set.vmras.iter().enumerate() {
            if dedup.insert((vma.addr, vma.len)) {
                to_remap.push((i, j));
            }
        }
        candidates.push(i);
    }
    if to_remap.is_empty() {
        return Ok(());
    }
    // Re-map every region; commit state only if all of them succeed.
    let mut failures: Vec<RemapError> = Vec::new();
    for (si, vi) in &to_remap {
        if let Err(e) = remap_vma(&guard[*si].vmras[*vi]) {
            failures.push(e);
        }
    }
    if failures.is_empty() {
        for si in candidates {
            guard[si].remapped = true;
            // The regions are now pinned for the life of the process; the
            // metadata is no longer needed.
            guard[si].vmras.clear();
        }
        Ok(())
    } else {
        Err(failures)
    }
}

/// Re-`mmap` a single lean VMA back at its original address, verifying the
/// file identity recorded at snapshot time and refusing to clobber a range
/// that is already occupied by a different mapping.
///
/// File identity is the **inode**: the lean regions are mapped
/// page/region-aligned, so a VMA's length (from `/proc/self/maps`) can
/// legitimately exceed the file size (the tail is zero-filled), and a size
/// check would misfire. A replaced file (deleted + recreated at the same
/// path) gets a new inode, so an inode match is the guarantee that we are
/// re-mapping the same file.
fn remap_vma(vma: &LeanVma) -> Result<(), RemapError> {
    let file = File::open(&vma.path).map_err(|source| RemapError::OpenFailed {
        path: vma.path.clone(),
        source,
    })?;
    let md = file.metadata().map_err(|e| RemapError::IdentityMismatch {
        path: vma.path.clone(),
        detail: format!("stat: {e}"),
    })?;
    let ino = md.ino();
    if ino != vma.inode {
        return Err(RemapError::IdentityMismatch {
            path: vma.path.clone(),
            detail: format!("inode mismatch: recorded {0}, on disk {ino}", vma.inode),
        });
    }
    // MAP_FIXED_NOREPLACE maps at `addr` only if the range is free; if it is
    // already mapped the call fails with EEXIST instead of silently replacing
    // the occupant.
    let p = unsafe {
        libc::mmap(
            vma.addr as *mut c_void,
            vma.len,
            PROT_READ,
            MAP_PRIVATE | MAP_FIXED_NOREPLACE,
            file.as_raw_fd(),
            vma.offset as off_t,
        )
    };
    if p == MAP_FAILED {
        let e = io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::EEXIST) {
            return if occupant_is_same(vma) {
                Ok(())
            } else {
                Err(RemapError::RangeOccupied { addr: vma.addr })
            };
        }
        return Err(RemapError::MmapFailed {
            path: vma.path.clone(),
            source: e,
        });
    }
    Ok(())
}

/// If `vma`'s address range is currently mapped, is the occupant the identical
/// file mapping (same inode, same file offset at `vma.addr`)? Distinguishes
/// "we already re-mapped it" (idempotent success) from "the address was reused
/// by something else" (a failure we must not clobber).
fn occupant_is_same(vma: &LeanVma) -> bool {
    let Some(maps) = read_maps() else {
        return false;
    };
    let end = vma.addr + vma.len;
    for line in maps.lines() {
        let Some(m) = parse_maps_line(line) else {
            continue;
        };
        if m.start < end && m.end > vma.addr {
            let off_at_addr = m.offset + (vma.addr - m.start) as u64;
            return off_at_addr == vma.offset && m.inode == vma.inode && m.path == vma.path;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_of_sorts_and_dedups() {
        assert_eq!(key_of(&["B", "A", "B"]), "A\0B");
        assert_eq!(key_of(&["A"]), "A");
    }

    #[test]
    fn diff_freed_vmras_returns_disappeared() {
        let a = LeanVma {
            addr: 0x1000,
            len: 0x100,
            offset: 0,
            inode: 1,
            path: "/a.olean".into(),
        };
        let b = LeanVma {
            addr: 0x2000,
            len: 0x100,
            offset: 0,
            inode: 2,
            path: "/b.olean".into(),
        };
        let before = vec![a.clone(), b.clone()];
        let after = vec![b.clone()];
        let freed = diff_freed_vmras(&before, &after);
        assert_eq!(freed, vec![a]);
    }

    #[test]
    fn record_ignores_empty() {
        // Relative (not absolute) so it is safe under parallel test threads.
        let before = FREED_SETS.lock().unwrap().len();
        record_freed_set(&["A-empty-check"], vec![]);
        let after = FREED_SETS.lock().unwrap().len();
        assert_eq!(before, after, "recording an empty set must be a no-op");
    }

    #[test]
    fn record_merges_by_key_and_dedups() {
        let a = LeanVma {
            addr: 0x1000,
            len: 0x100,
            offset: 0,
            inode: 1,
            path: "/a.olean".into(),
        };
        let b = LeanVma {
            addr: 0x2000,
            len: 0x100,
            offset: 0,
            inode: 2,
            path: "/b.olean".into(),
        };
        record_freed_set(&["M-merge"], vec![a.clone()]);
        record_freed_set(&["M-merge"], vec![a.clone(), b.clone()]); // a dup, b new
        let guard = FREED_SETS.lock().unwrap();
        let set = guard
            .iter()
            .find(|s| s.key == key_of(&["M-merge"]))
            .unwrap();
        assert_eq!(
            set.vmras.len(),
            2,
            "same-key sets must merge and dedup by (addr, len)"
        );
        assert!(!set.remapped);
    }

    #[test]
    fn remap_fails_on_missing_file() {
        let vma = LeanVma {
            addr: 0x7f00_0000_1000,
            len: 0x1000,
            offset: 0,
            inode: 1,
            path: "/no/such/file.olean".into(),
        };
        let err = remap_vma(&vma).unwrap_err();
        assert!(matches!(err, RemapError::OpenFailed { .. }), "got {err:?}");
    }

    #[test]
    fn remap_fails_on_inode_mismatch() {
        let path = std::env::temp_dir().join(format!("leo3_ka_ino_{}.olean", std::process::id()));
        std::fs::write(&path, [0u8; 4096]).unwrap();
        let ino = std::fs::metadata(&path).unwrap().ino();
        // Correct path/size but a WRONG recorded inode: the file was replaced.
        let vma = LeanVma {
            addr: 0x7f00_0001_0000,
            len: 0x1000,
            offset: 0,
            inode: ino + 1,
            path: path.to_string_lossy().into_owned(),
        };
        let err = remap_vma(&vma).unwrap_err();
        assert!(
            matches!(err, RemapError::IdentityMismatch { .. }),
            "got {err:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn remap_refuses_to_clobber_occupied_range() {
        // Map an anonymous page; then try to "remap" a lean file over it. The
        // range is occupied by a different (anonymous) mapping, so the re-map
        // must be refused (RangeOccupied), not silently overwritten.
        let p = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                0x1000,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        assert!(p != MAP_FAILED, "anonymous mmap failed");
        let addr = p as usize;

        let path = std::env::temp_dir().join(format!("leo3_ka_occ_{}.olean", std::process::id()));
        std::fs::write(&path, [0u8; 4096]).unwrap();
        let ino = std::fs::metadata(&path).unwrap().ino();
        // Correct inode + size + offset, but the address is occupied by the
        // anonymous page above.
        let vma = LeanVma {
            addr,
            len: 0x1000,
            offset: 0,
            inode: ino,
            path: path.to_string_lossy().into_owned(),
        };
        let err = remap_vma(&vma).unwrap_err();
        assert!(
            matches!(err, RemapError::RangeOccupied { .. }),
            "got {err:?}"
        );

        unsafe { libc::munmap(p, 0x1000) };
        let _ = std::fs::remove_file(&path);
    }
}
