//! W-417 stopgap: revive dangling `g_native_symbol_cache` keys across
//! *different* module sets, without pinning the olean base mapping in the
//! common (same-set) case.
//!
//! ## Why the cache keys dangle
//!
//! The Lean runtime's global `g_native_symbol_cache` (`src/ir/ir_interpreter.cpp`)
//! is keyed by `name` *content*, but holds a *borrowed* pointer to the key
//! object. For import-derived names that object lives in the environment's
//! compacted import region (an `mmap` of the `.olean`). `Environment.freeRegions`
//! (bound here as `free_regions`) `munmap`s that region, so the cached key
//! pointers dangle. A later import's symbol lookup walks the rb-tree and
//! dereferences them → SIGSEGV.
//!
//! The base address of each olean mapping is deterministic
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
//!   re-`mmap` the freed set's olean regions at their original addresses.
//!
//! The re-map is therefore **lazy**: it happens at import time, only for freed
//! sets that differ from the set being imported. The re-mapped regions are left
//! mapped (orphaned — no environment owns them), which is the stopgap cost:
//! each *distinct* freed set that is later crossed pins one resident copy of its
//! olean regions. The true fix is upstream (Option 1: content-owning cache keys);
//! see the issue thread.
//!
//! ## Recorded state
//!
//! On drop, leotower snapshots `/proc/self/maps` before and after
//! `free_regions`; the olean VMAs that disappear are the freed set's regions and
//! are recorded here (with the set's module key). `remap_cross_set_bases`
//! re-mmaps the regions of every recorded set whose key differs from the set
//! about to be imported.

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

/// A single olean-backed virtual memory area, as seen in `/proc/self/maps`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OleanVma {
    /// VMA start address (the deterministic olean base, header included).
    pub addr: usize,
    /// VMA length in bytes.
    pub len: usize,
    /// File offset the VMA is mapped at.
    pub offset: u64,
    /// The `.olean` file backing the mapping.
    pub path: String,
}

/// A module set that was freed, together with the olean regions that
/// `free_regions` unmapped for it.
#[derive(Debug)]
struct FreedSet {
    /// Normalized module key (sorted, NUL-joined) identifying the set.
    key: String,
    /// The olean VMAs that were freed (and now need re-mapping on a cross-set
    /// import).
    vmras: Vec<OleanVma>,
    /// Set once the regions have been re-mmapped; the re-map is idempotent and
    /// the regions then stay mapped, so we never redo it.
    remapped: bool,
}

static FREED_SETS: LazyLock<Mutex<Vec<FreedSet>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Normalize a module list into a stable identity key.
fn key_of(modules: &[&str]) -> String {
    let mut v: Vec<&str> = modules.to_vec();
    v.sort_unstable();
    v.dedup();
    v.join("\0")
}

/// Snapshot the olean-backed VMAs currently mapped in this process.
///
/// Reads `/proc/self/maps` and returns one entry per `r*` mapping whose target
/// path ends in `.olean`. Used to bracket `free_regions` and diff out the VMAs
/// it unmaps.
pub fn snapshot_olean_vmras() -> Vec<OleanVma> {
    let mut out = Vec::new();
    let Ok(content) = std::fs::read_to_string("/proc/self/maps") else {
        return out;
    };
    for line in content.lines() {
        // /proc/self/maps: `start-end perm offset dev inode [padding] path`.
        // The inode and the path are separated by variable whitespace, so
        // take the first five fields and treat the trimmed remainder as the
        // path.
        let mut parts = line.splitn(6, ' ');
        let Some(range) = parts.next() else { continue };
        let Some(perm) = parts.next() else { continue };
        let Some(offset) = parts.next() else { continue };
        let _ = parts.next(); // dev
        let _ = parts.next(); // inode
        let Some(path_raw) = parts.next() else { continue };
        let path = path_raw.trim_start();
        if path.is_empty() || !perm.starts_with('r') || !path.ends_with(".olean") {
            continue;
        }
        let Some((start, end)) = range.split_once('-') else { continue };
        let Ok(start) = usize::from_str_radix(start, 16) else { continue };
        let Ok(end) = usize::from_str_radix(end, 16) else { continue };
        let Ok(offset) = u64::from_str_radix(offset, 16) else { continue };
        if end <= start {
            continue;
        }
        out.push(OleanVma { addr: start, len: end - start, offset, path: path.to_string() });
    }
    out
}

/// Return the VMAs present in `before` but absent from `after` — i.e. the
/// olean regions that disappeared between the two snapshots (the ones
/// `free_regions` unmapped).
pub fn diff_freed_vmras(before: &[OleanVma], after: &[OleanVma]) -> Vec<OleanVma> {
    let after_set: HashSet<(usize, usize)> = after.iter().map(|v| (v.addr, v.len)).collect();
    before
        .iter()
        .filter(|v| !after_set.contains(&(v.addr, v.len)))
        .cloned()
        .collect()
}

/// Record a freed module set and the olean regions `free_regions` unmapped for
/// it. Called by leotower after `free_regions`, with the VMAs diffed out from
/// before/after `/proc/self/maps` snapshots. Sets with no freed olean regions
/// (e.g. an env whose data was heap-allocated) record nothing.
pub fn record_freed_set(modules: &[&str], freed_vmras: Vec<OleanVma>) {
    if freed_vmras.is_empty() {
        return;
    }
    let key = key_of(modules);
    FREED_SETS
        .lock()
        .unwrap()
        .push(FreedSet { key, vmras: freed_vmras, remapped: false });
}

/// Re-`mmap` the olean regions of every freed set whose module set differs from
/// `importing`, so that the next (cross-set) import's symbol lookups do not
/// dereference dangling cache keys.
///
/// Must be called **before** the import runs (the import's `finalizeImport` →
/// `evalConst` → symbol lookup is where the dangling keys are dereferenced).
/// Same-set imports are a no-op here: they self-heal by re-mapping their own
/// base before their lookups, and pinning the base would needlessly push them
/// onto the heap.
pub fn remap_cross_set_bases(importing: &[&str]) {
    let import_key = key_of(importing);
    let mut dedup: HashSet<(usize, usize)> = HashSet::new();
    let mut to_remap: Vec<OleanVma> = Vec::new();
    {
        let mut guard = FREED_SETS.lock().unwrap();
        for set in guard.iter_mut() {
            if set.remapped || set.key == import_key {
                continue;
            }
            for vma in set.vmras.iter() {
                if dedup.insert((vma.addr, vma.len)) {
                    to_remap.push(vma.clone());
                }
            }
            set.remapped = true;
        }
    }
    for vma in &to_remap {
        let _ = remap_vma(vma);
    }
}

/// `mmap` a single olean VMA back at its original address. The region was
/// `munmap`ed by `free_regions`, so the address is free and `MAP_FIXED` is safe.
fn remap_vma(vma: &OleanVma) -> bool {
    let Ok(path) = std::ffi::CString::new(vma.path.as_bytes()) else {
        return false;
    };
    unsafe {
        let fd = libc::open(path.as_ptr(), libc::O_RDONLY);
        if fd < 0 {
            return false;
        }
        let addr = libc::mmap(
            vma.addr as *mut libc::c_void,
            vma.len,
            libc::PROT_READ,
            libc::MAP_PRIVATE | libc::MAP_FIXED,
            fd,
            vma.offset as libc::off_t,
        );
        libc::close(fd);
        if addr == libc::MAP_FAILED {
            eprintln!(
                "leo3 keepalive: failed to remap olean region at {:#x} (len {:#x}, {}): {}",
                vma.addr,
                vma.len,
                vma.path,
                std::io::Error::last_os_error()
            );
            return false;
        }
        true
    }
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
        let a = OleanVma { addr: 0x1000, len: 0x100, offset: 0, path: "/a.olean".into() };
        let b = OleanVma { addr: 0x2000, len: 0x100, offset: 0, path: "/b.olean".into() };
        let before = vec![a.clone(), b.clone()];
        let after = vec![b.clone()];
        let freed = diff_freed_vmras(&before, &after);
        assert_eq!(freed, vec![a]);
    }

    #[test]
    fn record_ignores_empty() {
        record_freed_set(&["A"], vec![]);
        assert!(FREED_SETS.lock().unwrap().is_empty());
    }
}