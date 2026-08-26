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

use std::collections::{HashMap, HashSet};
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
/// The file identity is `inode` + `device` + `offset` + `path` (a replaced or
/// moved file changes its inode and/or device), and `size` is the on-disk file
/// size at snapshot time (to catch in-place truncation/modification that keeps
/// the inode). `addr` + `len` is the address range the re-map restores.
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
    /// Device (`major << 32 | minor`) of the backing file. Together with
    /// `inode` it uniquely identifies the file across filesystems.
    pub device: u64,
    /// On-disk size of the backing file at snapshot time (bytes). A re-mapped
    /// file must be at least this large, or the region tail would read stale
    /// / zero bytes (in-place truncation keeps the inode).
    pub size: u64,
    /// The `.olean`/`.ir` file backing the mapping (` (deleted)` stripped).
    pub path: String,
    /// True if the backing file has been unlinked (` (deleted)` in
    /// `/proc/self/maps`). Such a file cannot be re-opened, so the VMA cannot
    /// be re-mapped (it is NOT safely trackable).
    pub deleted: bool,
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
    /// Two recorded regions claim the same address range but back *different*
    /// files (a collision of the deterministic bases). The range cannot be
    /// re-mapped to revive both, so nothing is re-mapped and the import is
    /// blocked.
    IdentityConflict {
        /// The address range claimed by two different files.
        addr: usize,
    },
    /// The backing file was unlinked while mapped; it cannot be re-opened, so
    /// the region cannot be re-mapped.
    FileDeleted {
        /// The path that was unlinked.
        path: String,
    },
    /// The kernel does not support `MAP_FIXED_NOREPLACE` (< 4.17); the re-map
    /// cannot be done safely (the flag would be treated as `MAP_FIXED`,
    /// clobbering live mappings), so the import is blocked.
    KernelUnsupported,
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
            RemapError::IdentityConflict { addr } => write!(
                f,
                "address {addr:#x} is claimed by two different files; refusing to re-map (deterministic-base collision)"
            ),
            RemapError::FileDeleted { path } => {
                write!(f, "{path} was unlinked while mapped; cannot re-map it")
            }
            RemapError::KernelUnsupported => write!(
                f,
                "kernel does not support MAP_FIXED_NOREPLACE (needs >= 4.17); refusing to re-map to avoid clobbering live mappings"
            ),
        }
    }
}

static FREED_SETS: LazyLock<Mutex<Vec<FreedSet>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Diagnostic / A-B switch: set `LEO3_KEEPALIVE_DISABLE=1` to turn the whole
/// stopgap off.
static DISABLED: LazyLock<bool> =
    LazyLock::new(|| std::env::var_os("LEO3_KEEPALIVE_DISABLE").is_some());

/// True if the running kernel supports `mmap(MAP_FIXED_NOREPLACE)` (added in
/// Linux 4.17). Checked once; on older kernels the flag would be silently
/// treated as `MAP_FIXED` (clobbering a live mapping), so the re-map must be
/// refused rather than attempted.
static KERNEL_NO_REPLACE: LazyLock<bool> = LazyLock::new(kernel_supports_map_fixed_noreplace);

/// Read the kernel release string and check it is >= 4.17.
fn kernel_supports_map_fixed_noreplace() -> bool {
    let mut uts: libc::utsname = unsafe { std::mem::zeroed() };
    if unsafe { libc::uname(&mut uts) } != 0 {
        return false;
    }
    let Some(release) = cstr_field(&uts.release) else {
        return false;
    };
    let mut parts = release.split('.');
    let Some(major) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
        return false;
    };
    let Some(minor) = parts.next().and_then(|s| s.parse::<u32>().ok()) else {
        return false;
    };
    major > 4 || (major == 4 && minor >= 17)
}

/// Convert a NUL-terminated (or length-bounded) `c_char` array from `utsname`
/// to a `&str`, stopping at the first NUL.
fn cstr_field(arr: &[std::os::raw::c_char]) -> Option<&str> {
    let bytes: &[u8] = unsafe { std::slice::from_raw_parts(arr.as_ptr() as *const u8, arr.len()) };
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    std::str::from_utf8(&bytes[..len]).ok()
}

/// Module-set keys that have a **file-backed** (re-mappable) copy.
///
/// A heap-backed environment (whose data lives in a `malloc` buffer, not file
/// VMAs — the Lean `module.cpp` fallback when any deterministic `mmap` fails)
/// is safe to free *only* when its set is in this registry: then its names
/// alias the file-backed copy's `g_native_symbol_cache` entries and were never
/// newly cached, so freeing the `malloc` buffer cannot dangle a cache key. A
/// set that was *first* imported heap-backed (its deterministic base collided
/// with another set's mapping) has no file-backed copy and its names WERE newly
/// cached — freeing it would dangle, so it is leaked instead.
static FILE_BACKED_SETS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

/// Register that the module set was imported file-backed (its data is in
/// re-mappable file VMAs). Called by leotower at import time, when the
/// import-time VMA diff is non-empty.
pub fn register_file_backed_set(modules: &[&str]) {
    if *DISABLED {
        return;
    }
    let key = key_of(modules);
    FILE_BACKED_SETS.lock().unwrap().insert(key);
}

/// True if the module set has a file-backed (re-mappable) copy. Called by
/// leotower at drop time to decide whether a heap-backed environment is safe
/// to free.
pub fn has_file_backed_copy(modules: &[&str]) -> bool {
    if *DISABLED {
        return false;
    }
    let key = key_of(modules);
    FILE_BACKED_SETS.lock().unwrap().contains(&key)
}

/// File suffixes of the lean import regions that `free_regions` unmaps.
///
/// Each imported module maps a main `.olean` plus a `.olean.server`, a
/// `.olean.private`, and an `.ir` sidecar; all four carry `name` objects that
/// can be cached as `g_native_symbol_cache` keys, so all four must be revived.
const LEAN_SUFFIXES: &[&str] = &[".olean", ".olean.server", ".olean.private", ".ir"];

fn is_lean_region(path: &str) -> bool {
    // Linux appends " (deleted)" to mappings whose backing file has been
    // unlinked; match the suffix on the path with that marker stripped.
    let path = path.strip_suffix(" (deleted)").unwrap_or(path);
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
    device: u64,
    inode: u64,
    path: String,
}

/// Parse a `/proc/self/maps` device field (`major:minor`, hex) into
/// `major << 32 | minor`.
fn parse_device(dev: &str) -> Option<u64> {
    let (maj_s, min_s) = dev.split_once(':')?;
    let maj = u64::from_str_radix(maj_s, 16).ok()?;
    let min = u64::from_str_radix(min_s, 16).ok()?;
    Some((maj << 32) | min)
}

/// Decode a `stat.st_dev` into the `major << 32 | minor` form used by
/// `parse_device`, so the on-disk device can be compared against the device
/// recorded from `/proc/self/maps`. The encoding is `major << 8 | minor` (the
/// classic kernel `makedev` layout, valid for minor < 256, which covers every
/// real device number).
fn st_dev_to_maps(dev: u64) -> u64 {
    let major = dev >> 8;
    let minor = dev & 0xff;
    (major << 32) | minor
}

/// True iff `a` and `b` are the same VMA: same address range AND same file
/// identity (offset + inode + device + path). Matching on identity as well as
/// address is what lets the diff / dedup distinguish "the same mapping is
/// still there" from "that address was reused by a different file".
fn vma_eq(a: &LeanVma, b: &LeanVma) -> bool {
    a.addr == b.addr
        && a.len == b.len
        && a.offset == b.offset
        && a.inode == b.inode
        && a.device == b.device
        && a.path == b.path
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
    let device = parse_device(parts.next()?)?;
    let inode = parts.next()?.parse::<u64>().ok()?;
    let path = parts.next().unwrap_or("").trim_start().to_string();
    Some(MapsVma {
        start,
        end,
        offset,
        device,
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
        // A file that has been unlinked shows up as "<path> (deleted)". It
        // cannot be re-opened, so it cannot be re-mapped: record it (so the
        // drop logic knows the region is NOT safely trackable) but flag it.
        let deleted = m.path.ends_with(" (deleted)");
        let path = m
            .path
            .strip_suffix(" (deleted)")
            .unwrap_or(&m.path)
            .to_string();
        // File size at snapshot time, for the in-place-truncation check on
        // re-map. A deleted / unstat-able file has size 0 (and `deleted`).
        let size = if deleted {
            0
        } else {
            std::fs::metadata(&path).map(|md| md.len()).unwrap_or(0)
        };
        out.push(LeanVma {
            addr: m.start,
            len: m.end - m.start,
            offset: m.offset,
            inode: m.inode,
            device: m.device,
            size,
            path,
            deleted,
        });
    }
    out
}

/// Return the VMAs present in `before` but absent from `after` — i.e. the lean
/// regions that disappeared between the two snapshots (the ones
/// `free_regions` unmapped).
///
/// A VMA counts as "still present" only if `after` holds a VMA with the same
/// address range *and* the same file identity. If that address was freed and
/// then re-used by a different file, the original mapping is reported as freed
/// (it is gone) even though the range is mapped again.
pub fn diff_freed_vmras(before: &[LeanVma], after: &[LeanVma]) -> Vec<LeanVma> {
    before
        .iter()
        .filter(|v| !after.iter().any(|a| vma_eq(v, a)))
        .cloned()
        .collect()
}

/// Return the VMAs present in `after` but absent from `before` — i.e. the lean
/// regions that *appeared* between the two snapshots (the ones an import
/// `mmap`'d). The mirror image of [`diff_freed_vmras`]: use this to detect
/// whether an import was **file-backed** (it added file VMAs) as opposed to
/// **heap-backed** (it added none, because the deterministic base was already
/// occupied and Lean fell back to a `malloc` buffer).
///
/// A VMA counts as "new" only if `before` holds no VMA with the same address
/// range *and* the same file identity.
pub fn diff_added_vmras(before: &[LeanVma], after: &[LeanVma]) -> Vec<LeanVma> {
    after
        .iter()
        .filter(|v| !before.iter().any(|b| vma_eq(v, b)))
        .cloned()
        .collect()
}

/// Record a freed module set and the lean regions `free_regions` unmapped for
/// it. Called by leotower after `free_regions`, with the VMAs diffed out from
/// before/after `/proc/self/maps` snapshots. Sets with no freed lean regions
/// (e.g. an env whose data was heap-allocated) record nothing.
///
/// Sets are merged by key: re-recording the same module set de-dups by full
/// identity (address range + file identity) instead of stacking duplicate
/// records, and any newly-freed region marks the set as needing a (re-)map
/// again. Two records at the same address but with *different* file identity
/// are both kept — `remap_cross_set_bases` detects that as a conflict and
/// refuses to re-map (it cannot revive two files at one address).
pub fn record_freed_set(modules: &[&str], freed_vmras: Vec<LeanVma>) {
    if *DISABLED || freed_vmras.is_empty() {
        return;
    }
    let key = key_of(modules);
    let mut guard = FREED_SETS.lock().unwrap();
    if let Some(set) = guard.iter_mut().find(|s| s.key == key) {
        let mut added = 0;
        for vma in freed_vmras {
            if !set.vmras.iter().any(|e| vma_eq(e, &vma)) {
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
    // MAP_FIXED_NOREPLACE needs kernel >= 4.17; on older kernels the flag is
    // silently treated as MAP_FIXED, which would CLOBBER a live mapping. Refuse
    // to re-map (block the import) rather than risk it.
    if !*KERNEL_NO_REPLACE {
        return Err(vec![RemapError::KernelUnsupported]);
    }
    let import_key = key_of(importing);
    let mut guard = FREED_SETS.lock().unwrap();
    // Collect candidate sets (freed, not the importing set, not yet remapped)
    // and the VMAs to re-map, as (set, vma) indices so `guard` can be mutated
    // afterwards. De-dup on full identity: a true duplicate (same address +
    // identity) is skipped, but two records at the same address with *different*
    // file identity are a conflict the re-map cannot resolve — error out before
    // touching any mapping (the import is then blocked).
    let mut seen: HashMap<(usize, usize), &LeanVma> = HashMap::new();
    let mut to_remap: Vec<(usize, usize)> = Vec::new();
    let mut candidates: Vec<usize> = Vec::new();
    for (i, set) in guard.iter().enumerate() {
        if set.remapped || set.key == import_key || set.vmras.is_empty() {
            continue;
        }
        candidates.push(i);
        for (j, vma) in set.vmras.iter().enumerate() {
            match seen.get(&(vma.addr, vma.len)) {
                Some(prev) if vma_eq(prev, vma) => {
                    // Duplicate (same address + identity) — already scheduled.
                }
                Some(_) => {
                    return Err(vec![RemapError::IdentityConflict { addr: vma.addr }]);
                }
                None => {
                    seen.insert((vma.addr, vma.len), vma);
                    to_remap.push((i, j));
                }
            }
        }
    }
    // `seen` holds borrows into `guard`; end them before mutating.
    drop(seen);
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
/// File identity is `inode` + `device` (a replaced/moved file changes its
/// inode and/or device), plus an on-disk *size* that must not have shrunk
/// (in-place truncation keeps the inode but leaves the region tail pointing at
/// stale / zero bytes). The lean regions are mapped page/region-aligned, so a
/// VMA's length (from `/proc/self/maps`) can legitimately exceed the file size
/// (the tail is zero-filled) — the size check therefore compares against the
/// recorded *file* size, not the VMA length.
fn remap_vma(vma: &LeanVma) -> Result<(), RemapError> {
    // A deleted (unlinked) file cannot be re-opened, so it cannot be re-mapped.
    // Refuse rather than leave the cached key dangling.
    if vma.deleted {
        return Err(RemapError::FileDeleted {
            path: vma.path.clone(),
        });
    }
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
    let dev = st_dev_to_maps(md.dev());
    if dev != vma.device {
        return Err(RemapError::IdentityMismatch {
            path: vma.path.clone(),
            detail: format!(
                "device mismatch: recorded {:#x}, on disk {dev:#x}",
                vma.device
            ),
        });
    }
    let size = md.len();
    if size < vma.size {
        return Err(RemapError::IdentityMismatch {
            path: vma.path.clone(),
            detail: format!("file size shrank: recorded {0}, on disk {size}", vma.size),
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
    // Verify the kernel actually honored MAP_FIXED_NOREPLACE: the returned
    // address must equal the requested one. Pre-4.17 kernels can ignore the
    // flag and map elsewhere — unmap the stray mapping and refuse.
    let p_addr = p as usize;
    if p_addr != vma.addr {
        unsafe { libc::munmap(p, vma.len) };
        return Err(RemapError::MmapFailed {
            path: vma.path.clone(),
            source: io::Error::other(format!(
                "mmap returned {p_addr:#x}, not the recorded {:#x}; the kernel did not honor MAP_FIXED_NOREPLACE",
                vma.addr
            )),
        });
    }
    Ok(())
}

/// If `vma`'s address range is currently mapped, is the *entire* range
/// `[addr, addr+len)` covered by the identical file mapping (same inode +
/// device + path, each byte at the matching file offset)? This proves the
/// re-map is idempotent ("we already re-mapped it") rather than a range
/// reused by something else (a failure we must not clobber).
///
/// The range is walked end to end — a single overlapping VMA is not enough,
/// since a partial overlap would leave the rest of the range clobbered. All
/// arithmetic is checked (an overflow or underflow returns `false` rather than
/// wrapping).
fn occupant_is_same(vma: &LeanVma) -> bool {
    let Some(maps) = read_maps() else {
        return false;
    };
    let Some(end) = vma.addr.checked_add(vma.len) else {
        return false;
    };
    if vma.len == 0 {
        return true; // empty range is trivially covered
    }
    // Pre-parse the maps once (each walk step re-scans for the VMA at `pos`).
    let mut parsed: Vec<MapsVma> = Vec::new();
    for line in maps.lines() {
        if let Some(m) = parse_maps_line(line) {
            parsed.push(m);
        }
    }
    let mut pos = vma.addr;
    while pos < end {
        let Some(m) = parsed.iter().find(|m| m.start <= pos && pos < m.end) else {
            return false; // gap: this address is not mapped
        };
        // This VMA must be the same file at the matching offset.
        if m.inode != vma.inode || m.device != vma.device || m.path != vma.path {
            return false;
        }
        let off_at_pos = match m.offset.checked_add((pos - m.start) as u64) {
            Some(o) => o,
            None => return false,
        };
        let expected_offset = match vma.offset.checked_add((pos - vma.addr) as u64) {
            Some(o) => o,
            None => return false,
        };
        if off_at_pos != expected_offset {
            return false;
        }
        // Advance to the end of this VMA (or `end`, whichever is first). Both
        // are > pos (m.end > pos by the match, end > pos by the loop), so this
        // always makes progress.
        pos = std::cmp::min(m.end, end);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a `LeanVma` with the identity fields the unit tests do not
    /// exercise (`device = 0`, `size = 0`, `deleted = false`).
    fn mk_vma(addr: usize, len: usize, offset: u64, inode: u64, path: &str) -> LeanVma {
        LeanVma {
            addr,
            len,
            offset,
            inode,
            device: 0,
            size: 0,
            path: path.to_string(),
            deleted: false,
        }
    }

    #[test]
    fn key_of_sorts_and_dedups() {
        assert_eq!(key_of(&["B", "A", "B"]), "A\0B");
        assert_eq!(key_of(&["A"]), "A");
    }

    #[test]
    fn diff_freed_vmras_returns_disappeared() {
        let a = mk_vma(0x1000, 0x100, 0, 1, "/a.olean");
        let b = mk_vma(0x2000, 0x100, 0, 2, "/b.olean");
        let before = vec![a.clone(), b.clone()];
        let after = vec![b.clone()];
        let freed = diff_freed_vmras(&before, &after);
        assert_eq!(freed, vec![a]);
    }

    #[test]
    fn diff_freed_vmras_detects_replaced_file() {
        // The VMA at 0x1000 is present in `before` (inode 1) but in `after`
        // the same address maps a DIFFERENT file (inode 99). The original
        // mapping is gone, so it must be reported as freed.
        let a = mk_vma(0x1000, 0x100, 0, 1, "/a.olean");
        let a_replaced = mk_vma(0x1000, 0x100, 0, 99, "/replaced.olean");
        let before = vec![a.clone()];
        let after = vec![a_replaced];
        let freed = diff_freed_vmras(&before, &after);
        assert_eq!(freed, vec![a], "a replaced mapping must be reported freed");
    }

    #[test]
    fn record_ignores_empty() {
        let before = FREED_SETS.lock().unwrap().len();
        record_freed_set(&["A-empty-check"], vec![]);
        let after = FREED_SETS.lock().unwrap().len();
        assert_eq!(before, after, "recording an empty set must be a no-op");
    }

    #[test]
    fn record_merges_by_key_and_dedups() {
        let a = mk_vma(0x1000, 0x100, 0, 1, "/a.olean");
        let b = mk_vma(0x2000, 0x100, 0, 2, "/b.olean");
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
            "same-key sets must merge and dedup by full identity"
        );
        assert!(!set.remapped);
    }

    #[test]
    fn remap_fails_on_missing_file() {
        let vma = mk_vma(0x7f00_0000_1000, 0x1000, 0, 1, "/no/such/file.olean");
        let err = remap_vma(&vma).unwrap_err();
        assert!(matches!(err, RemapError::OpenFailed { .. }), "got {err:?}");
    }

    #[test]
    fn remap_fails_on_inode_mismatch() {
        let path = std::env::temp_dir().join(format!("leo3_ka_ino_{}.olean", std::process::id()));
        std::fs::write(&path, [0u8; 4096]).unwrap();
        let ino = std::fs::metadata(&path).unwrap().ino();
        // Correct path but a WRONG recorded inode: the file was replaced.
        let vma = mk_vma(
            0x7f00_0001_0000,
            0x1000,
            0,
            ino + 1,
            &path.to_string_lossy(),
        );
        let err = remap_vma(&vma).unwrap_err();
        assert!(
            matches!(err, RemapError::IdentityMismatch { .. }),
            "got {err:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn remap_fails_on_device_mismatch() {
        let path = std::env::temp_dir().join(format!("leo3_ka_dev_{}.olean", std::process::id()));
        std::fs::write(&path, [0u8; 4096]).unwrap();
        let md = std::fs::metadata(&path).unwrap();
        // Correct inode, but a WRONG recorded device.
        let vma = LeanVma {
            addr: 0x7f00_0002_0000,
            len: 0x1000,
            offset: 0,
            inode: md.ino(),
            device: st_dev_to_maps(md.dev()) + 1,
            size: md.len(),
            path: path.to_string_lossy().into_owned(),
            deleted: false,
        };
        let err = remap_vma(&vma).unwrap_err();
        assert!(
            matches!(err, RemapError::IdentityMismatch { .. }),
            "got {err:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn remap_fails_on_size_shrink() {
        let path = std::env::temp_dir().join(format!("leo3_ka_size_{}.olean", std::process::id()));
        std::fs::write(&path, [0u8; 4096]).unwrap();
        let md = std::fs::metadata(&path).unwrap();
        // Correct inode + device, but the recorded size is LARGER than the
        // on-disk file: an in-place truncation (same inode, shorter file).
        let vma = LeanVma {
            addr: 0x7f00_0003_0000,
            len: 0x1000,
            offset: 0,
            inode: md.ino(),
            device: st_dev_to_maps(md.dev()),
            size: md.len() + 1024,
            path: path.to_string_lossy().into_owned(),
            deleted: false,
        };
        let err = remap_vma(&vma).unwrap_err();
        assert!(
            matches!(err, RemapError::IdentityMismatch { .. }),
            "got {err:?}"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn remap_fails_on_deleted_file() {
        let vma = LeanVma {
            addr: 0x7f00_0004_0000,
            len: 0x1000,
            offset: 0,
            inode: 1,
            device: 0,
            size: 0,
            path: "/no/matter.olean".into(),
            deleted: true,
        };
        let err = remap_vma(&vma).unwrap_err();
        assert!(matches!(err, RemapError::FileDeleted { .. }), "got {err:?}");
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
        let md = std::fs::metadata(&path).unwrap();
        // Correct inode + device + size + offset, but the address is occupied
        // by the anonymous page above.
        let vma = LeanVma {
            addr,
            len: 0x1000,
            offset: 0,
            inode: md.ino(),
            device: st_dev_to_maps(md.dev()),
            size: md.len(),
            path: path.to_string_lossy().into_owned(),
            deleted: false,
        };
        let err = remap_vma(&vma).unwrap_err();
        assert!(
            matches!(err, RemapError::RangeOccupied { .. }),
            "got {err:?}"
        );
        unsafe { libc::munmap(p, 0x1000) };
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn conflicting_identities_kept_and_block_remap() {
        // Two VMAs at the same address but different file identity (a
        // deterministic-base collision): both must be kept (not deduped), and
        // the conflict must block the re-map (the import is then refused).
        let a = mk_vma(0x7f00_0005_0000, 0x1000, 0, 1, "/a.olean");
        let b = mk_vma(0x7f00_0005_0000, 0x1000, 0, 2, "/b.olean");
        record_freed_set(&["S-conflict-test"], vec![a.clone(), b.clone()]);
        let guard = FREED_SETS.lock().unwrap();
        let set = guard
            .iter()
            .find(|s| s.key == key_of(&["S-conflict-test"]))
            .unwrap();
        assert_eq!(
            set.vmras.len(),
            2,
            "conflicting identities must both be kept"
        );
        drop(guard);
        let err = remap_cross_set_bases(&["importing-set"]).unwrap_err();
        assert!(
            err.iter()
                .any(|e| matches!(e, RemapError::IdentityConflict { .. })),
            "got {err:?}"
        );
    }
}
