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

use std::fmt;
use std::fs::File;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, LazyLock, Mutex};
use std::thread;

use libc::{c_void, off_t, MAP_FAILED, MAP_FIXED_NOREPLACE, MAP_PRIVATE, PROT_READ};

use super::environment::LeanEnvironment;
use crate::unbound::LeanUnbound;

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
    /// Whether the snapshot successfully read this region's size, i.e. whether
    /// its file identity is trackable. `false` when the snapshot's `stat`
    /// failed (permission / race / special fs): the recorded `size` is then 0
    /// by construction, and treating it as a valid "0-byte region" would let
    /// the re-map accept an arbitrary (even truncated or rewritten) file at
    /// the original address. Such a VMA is NOT safely trackable — the re-map
    /// refuses it and the drop must leak the env.
    pub size_known: bool,
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
    /// `/proc/self/maps` could not be read. Without it we cannot prove a base
    /// is free / an identical mapping is present, nor diff the VMAs a free
    /// unmapped — so the caller must fail closed (leak the env, or block the
    /// import) rather than assume the range is safe.
    MapsUnavailable,
    /// The keepalive stopgap has been quarantined: a prior destructive
    /// `free_regions` failed in a way that could not be proven fully recovered,
    /// so the process must not import again (the symbol-cache state may hold
    /// dangling keys). The re-map — and therefore the cross-set import — is
    /// blocked rather than risk a dangling name dereference.
    Poisoned,
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
            RemapError::MapsUnavailable => write!(
                f,
                "could not read /proc/self/maps; cannot prove the lean region state, failing closed"
            ),
            RemapError::Poisoned => write!(
                f,
                "keepalive stopgap is quarantined (a prior destructive free failed unrecoverably); blocking import"
            ),
        }
    }
}

static FREED_SETS: LazyLock<Mutex<Vec<FreedSet>>> = LazyLock::new(|| Mutex::new(Vec::new()));

/// Diagnostic / A-B switch: set `LEO3_KEEPALIVE_DISABLE=1` to turn the whole
/// stopgap off.
static DISABLED: LazyLock<bool> =
    LazyLock::new(|| std::env::var_os("LEO3_KEEPALIVE_DISABLE").is_some());

/// Process-wide quarantine latch. Once a *destructive* `free_regions` has
/// started and the process state can no longer be proven clean (the free
/// returned an error mid-sequence — `Environment.freeRegions` is a non-atomic
/// `forM CompactedRegion.free`, so a mid-sequence error leaves the first
/// regions already unmapped with their `g_native_symbol_cache` keys dangling —
/// or the post-free `/proc/self/maps` was unreadable), the lean region state
/// backing the symbol cache is untrustworthy. Rather than risk the next import
/// dereferencing a dangling name key, the process quarantines itself: no
/// further destructive free is ever performed (future envs leak) and no further
/// cross-set import is allowed (the re-map blocks). One-way latch; only tests
/// reset it.
static POISONED: AtomicBool = AtomicBool::new(false);

/// Latch the keepalive stopgap into quarantine (see [`POISONED`]). Idempotent.
pub fn poison_keepalive() {
    POISONED.store(true, Ordering::SeqCst);
}

/// Whether the keepalive stopgap has been quarantined (a prior destructive free
/// failed in a way that could not be proven fully recovered).
pub fn keepalive_poisoned() -> bool {
    POISONED.load(Ordering::SeqCst)
}

/// Test-only: clear the quarantine latch so one test's poison does not leak
/// into another test in the same process.
#[cfg(test)]
pub fn test_reset_poison() {
    POISONED.store(false, Ordering::SeqCst);
}

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

/// Process-wide, **reentrant** serialization for the environment lifecycle
/// (import / free / record / re-map / snapshot). See [`lifecycle_lock`].
struct ReentrantLifecycleLock {
    /// `(holder, hold-count)`. `holder` is the thread currently owning the
    /// lock; `hold-count` is how many times that thread has acquired it.
    state: Mutex<(Option<std::thread::ThreadId>, u32)>,
    cv: Condvar,
}

impl ReentrantLifecycleLock {
    const fn new() -> Self {
        Self {
            state: Mutex::new((None, 0)),
            cv: Condvar::new(),
        }
    }

    /// Acquire the lock, blocking until it is free. Re-acquiring from the same
    /// thread increments the hold-count instead of blocking (reentrancy), so a
    /// sequence that already holds the lock can call the instrumented entries
    /// ([`snapshot_lean_vmras`], [`record_freed_set`], [`remap_cross_set_bases`],
    /// and the import/free entries) without deadlocking.
    fn lock(&self) -> ReentrantLifecycleGuard<'_> {
        let me = thread::current().id();
        let mut state = self.state.lock().unwrap();
        if state.0 == Some(me) {
            state.1 += 1;
            return ReentrantLifecycleGuard {
                lk: self,
                _not_send: std::marker::PhantomData,
            };
        }
        while state.0.is_some() {
            state = self.cv.wait(state).unwrap();
        }
        state.0 = Some(me);
        state.1 = 1;
        ReentrantLifecycleGuard {
            lk: self,
            _not_send: std::marker::PhantomData,
        }
    }
}

/// Guard for [`ReentrantLifecycleLock`]. Dropping it decrements the hold-count
/// and releases the lock (waking a waiter) only on the last release.
///
/// The guard is deliberately `!Send`: the lock records the *holding thread's*
/// `ThreadId`, so the guard must be dropped on the thread that acquired it.
/// Moving it to another thread would leave the lock's holder as the original
/// thread, and a re-acquire from the new thread would wait for the original
/// thread forever (a deadlock if the original thread waits on the new one).
/// `PhantomData<*const ()>` is the standard "not movable across threads" marker.
pub struct ReentrantLifecycleGuard<'a> {
    lk: &'a ReentrantLifecycleLock,
    _not_send: std::marker::PhantomData<*const ()>,
}

impl Drop for ReentrantLifecycleGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.lk.state.lock().unwrap();
        state.1 -= 1;
        if state.1 == 0 {
            state.0 = None;
            self.lk.cv.notify_all();
        }
    }
}

static LIFECYCLE_LOCK: LazyLock<ReentrantLifecycleLock> =
    LazyLock::new(ReentrantLifecycleLock::new);

/// Acquire the process-wide lifecycle lock.
///
/// Every operation that touches the shared lean VMA state that backs
/// `g_native_symbol_cache` is serialized by this (reentrant) lock: the import
/// and `free_regions` entries, plus [`snapshot_lean_vmras`],
/// [`record_freed_set`], and [`remap_cross_set_bases`], each acquire it at
/// entry. leotower additionally holds it across the *whole* free/record
/// sequence (`EnvRegions::drop`: snapshot → `free_regions` → snapshot → record)
/// and the whole import sequence (`Repl::new`: re-map → import), so those
/// multi-step sequences are atomic against a concurrent direct leo3 caller.
/// The lock is reentrant, so the inner instrumented entries do not deadlock
/// when called from within such a held sequence. The worker thread never takes
/// this lock, so it can never be held across a `with_worker` dispatch in a way
/// that would block the worker.
pub fn lifecycle_lock() -> ReentrantLifecycleGuard<'static> {
    LIFECYCLE_LOCK.lock()
}

/// Decide whether an imported environment is safe to `free_regions`.
///
/// `free_regions` releases *every* compacted region, including any **heap**
/// region (the `malloc` buffer the Lean `module.cpp` fallback uses when a
/// deterministic `mmap` base is already occupied). A heap region's
/// `g_native_symbol_cache` keys point into that buffer and cannot be re-mapped
/// after it is freed, so freeing a heap/mixed environment dangles them and the
/// next import's symbol lookup SIGSEGVs.
///
/// An environment is all-file-backed — and therefore safe to free — exactly
/// when its region count equals the number of lean file VMAs the import added:
/// each file-backed region is one `mmap` (one VMA line), a heap region adds
/// none. Any uncertainty (the count could not be read, or it does not match)
/// returns `false`, so the caller leaks the environment instead.
pub fn safe_to_free_regions(region_count: Option<u64>, file_vmas_added: u64) -> bool {
    matches!(region_count, Some(n) if n >= 1 && n == file_vmas_added)
}

/// Read the environment's compacted-region count from the live environment
/// object, guarding every step. This is the number of compacted import
/// regions the environment holds — one per file-backed *part* of an imported
/// module (a `.olean` region plus a companion `.ir` / `.server` / `.private`
/// region when the module has such code). For a fully file-backed import this
/// count equals the number of lean file VMAs the import adds (each region is
/// exactly one `mmap` / one VMA line); a heap-fallback region is still counted
/// here but adds no VMA, which is what makes the count gate in
/// [`safe_to_free_regions`] discriminate file- from heap-backed envs.
///
/// The field path is the compiled layout of the elaboration `Environment` in
/// the pinned Lean toolchain (verified against the disassembly of
/// `lean_environment_free_regions` and a runtime probe): `env.field0.field0.
/// field5.field2` is that region array; its element count (word 1 of the array
/// object, word 0 being the refcount header) is the region count. A full
/// `Lean` import reads 6828 here (4 × 1707 modules), matching the 6828 lean
/// file VMAs it adds. Any step that fails a sanity check returns `None`, which
/// [`safe_to_free_regions`] treats as "cannot prove all regions are
/// file-backed" (leak) — a safe failure mode if the layout ever changes.
///
/// # Safety
///
/// Typed (a `LeanUnbound<LeanEnvironment>`), but **not safe**: `LeanUnbound`
/// `T` is only a Rust-level tag and `LeanUnbound::cast::<U>` is a *safe*
/// generic cast, so `cast` can forge a `LeanUnbound<LeanEnvironment>` out of a
/// valid `LeanUnbound<LeanString>` / `LeanUnbound<LeanAny>` without any
/// `unsafe`. The `Environment` object this walks is therefore *not* guaranteed
/// by the type system. The caller must uphold: `env` points at a genuine, live
/// elaboration `Environment` (not a `cast` from some other Lean object) that
/// the caller owns and keeps alive for the duration of the call. The function
/// only *reads* through it (never mutates or frees); the raw field walk lives
/// in the private `count_regions_from` and is sound precisely for a real
/// in-memory `Environment`.
pub unsafe fn environment_region_count(env: &LeanUnbound<LeanEnvironment>) -> Option<u64> {
    count_regions_from(env.as_ptr() as *const c_void)
}

/// Read the region count from a raw environment pointer. Kept private: public
/// callers route through [`environment_region_count`], which guarantees the
/// pointer came from a `LeanUnbound<LeanEnvironment>` rather than an
/// arbitrary caller-supplied `*const c_void`.
///
/// # Safety
///
/// `env_ptr` must point at a live, valid elaboration `Environment` that the
/// caller owns and keeps alive for the call; the function only reads through
/// it. Passing an aligned-but-unmapped or otherwise invalid pointer is
/// undefined behavior (a fault).
unsafe fn count_regions_from(env_ptr: *const c_void) -> Option<u64> {
    let e = env_ptr as *const u64;
    if !sane_ptr(e as u64) {
        return None;
    }
    let read = |base: *const u64, i: usize| unsafe { *base.add(i) };
    let f0 = read(e, 1); // env.field0
    if !sane_ptr(f0) {
        return None;
    }
    let f00 = read(f0 as *const u64, 1); // env.field0.field0
    if !sane_ptr(f00) {
        return None;
    }
    let a = read(f00 as *const u64, 6); // env[0][0][5]
    if !sane_ptr(a) {
        return None;
    }
    let b = read(a as *const u64, 3); // env[0][0][5][2] (the regions array)
    if !sane_ptr(b) {
        return None;
    }
    let n = read(b as *const u64, 1); // array element count
    (1..200_000).contains(&n).then_some(n)
}

/// A plausible heap-object pointer: non-null, 8-aligned, in the user VA range.
fn sane_ptr(v: u64) -> bool {
    0x1000 < v && v < (1 << 48) && v.is_multiple_of(8)
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
/// identity (offset + inode + device + on-disk size + path + deleted flag).
/// Matching on identity as well as address is what lets the diff / dedup
/// distinguish "the same mapping is still there" from "that address was reused
/// by a different file"; `size` catches in-place truncation and `deleted`
/// an unlinked backing file that keeps the inode.
fn vma_eq(a: &LeanVma, b: &LeanVma) -> bool {
    a.addr == b.addr
        && a.len == b.len
        && a.offset == b.offset
        && a.inode == b.inode
        && a.device == b.device
        && a.size == b.size
        && a.path == b.path
        && a.deleted == b.deleted
        && a.size_known == b.size_known
}

/// True iff the two VMAs' address ranges overlap as **half-open** intervals
/// `[addr, addr+len)`. Uses checked arithmetic (an overflowing end is treated
/// as no overlap rather than wrapping). This is the correct test for
/// deterministic-base collisions: two regions collide whenever their ranges
/// intersect at *all*, not only when start and length happen to match exactly
/// (a pending set occupying `[0x2000,0x3000)` still blocks an importing set
/// whose region is `[0x1000,0x3000)`).
fn ranges_overlap(a: &LeanVma, b: &LeanVma) -> bool {
    match (a.addr.checked_add(a.len), b.addr.checked_add(b.len)) {
        (Some(a_end), Some(b_end)) => a.addr < b_end && b.addr < a_end,
        _ => false,
    }
}

/// If two VMAs' ranges overlap as half-open intervals but back *different*
/// files, return the conflict address (the lower start); otherwise `None`.
/// Identical regions (same full identity) are the same mapping and are not a
/// conflict.
fn overlapping_conflict(a: &LeanVma, b: &LeanVma) -> Option<usize> {
    if ranges_overlap(a, b) && !vma_eq(a, b) {
        Some(a.addr.min(b.addr))
    } else {
        None
    }
}

/// Find a deterministic-base collision across ALL recorded VMAs: the first
/// pair of VMAs (whether from the same freed set or different ones) whose
/// ranges overlap as half-open intervals but back different files. Pure (no
/// global state) so it is unit-testable in isolation. Returns the conflict
/// address, or `None` if the ranges are clean.
fn find_collision(all: &[&LeanVma]) -> Option<usize> {
    for i in 0..all.len() {
        for j in (i + 1)..all.len() {
            if let Some(addr) = overlapping_conflict(all[i], all[j]) {
                return Some(addr);
            }
        }
    }
    None
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
/// Returns `Err(RemapError::MapsUnavailable)` when `/proc/self/maps` cannot be
/// read, so the caller can fail closed (leak / block) instead of treating an
/// unreadable maps as "no lean VMAs" (which would free with nothing recorded).
/// When the stopgap is disabled (`LEO3_KEEPALIVE_DISABLE`) it returns `Ok` of an
/// empty list: the stopgap is deliberately off, so there is nothing to track.
pub fn snapshot_lean_vmras() -> Result<Vec<LeanVma>, RemapError> {
    // Serialize the read against the free / re-map sequences that also read
    // `/proc/self/maps` (a snapshot interleaved with another sequence's free
    // would mix their VMAs). Reentrant, so a held sequence's snapshots do not
    // deadlock.
    let _lifecycle = lifecycle_lock();
    if *DISABLED {
        return Ok(Vec::new());
    }
    snapshot_from(read_maps())
}

/// Parse a `/proc/self/maps` snapshot into lean VMAs. `None` means the read
/// failed (return `MapsUnavailable`); `Some(..)` is the maps content.
fn snapshot_from(maps: Option<String>) -> Result<Vec<LeanVma>, RemapError> {
    let maps = maps.ok_or(RemapError::MapsUnavailable)?;
    Ok(parse_maps_content(&maps))
}

/// Pure filter of a maps snapshot down to lean import-region VMAs (testable
/// without touching `/proc/self/maps`).
fn parse_maps_content(maps: &str) -> Vec<LeanVma> {
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
        // re-map. `size_known` distinguishes a *legitimate* 0-byte region from
        // a *failed* `stat` (both record `size == 0`): only the former is
        // trackable. A failed `stat` (permission / race / special fs) marks
        // the region untrackable so the re-map refuses it and the drop leaks.
        let (size, size_known) = if deleted {
            (0, false)
        } else {
            match std::fs::metadata(&path) {
                Ok(md) => (md.len(), true),
                Err(_) => (0, false),
            }
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
            size_known,
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
    // Serialize the metadata write against concurrent sequences (reentrant, so
    // a drop holding the lock across its whole sequence does not deadlock).
    let _lifecycle = lifecycle_lock();
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
    // Serialize the whole re-map (collision check → importing-set validation →
    // re-mmap) against concurrent free/import sequences and against other
    // callers of this entry (reentrant, so a sequence already holding the lock
    // does not deadlock).
    let _lifecycle = lifecycle_lock();
    if *DISABLED {
        return Ok(());
    }
    // Quarantined: a prior destructive free failed unrecoverably, so the symbol
    // cache may hold dangling keys. Block the import (and its re-map) rather
    // than risk a dangling name dereference on the next import.
    if keepalive_poisoned() {
        return Err(vec![RemapError::Poisoned]);
    }
    let import_key = key_of(importing);
    let mut guard = FREED_SETS.lock().unwrap();

    // Collision detection across *all* freed sets — pending **and** retained —
    // using checked half-open interval intersection, not an exact `(addr, len)`
    // key. Two recorded regions from *different* sets whose ranges intersect
    // but back *different* files are a deterministic-base collision: they
    // cannot both be revived, and if the collision involves the importing set
    // the import's self-heal re-mmap would hit a partially-occupied range and
    // fall back to the heap, dangling its keys. So any such overlap blocks the
    // import up front. Identical regions (same full identity) are the same
    // mapping, not a conflict.
    // Build the flat list of every recorded VMA (across all freed sets,
    // including retained) and look for a deterministic-base collision via the
    // pure `find_collision` helper. This catches conflicts both *within* a
    // single set (two different files on the same base) and *across* sets.
    let all_vmras: Vec<&LeanVma> = guard.iter().flat_map(|s| s.vmras.iter()).collect();
    if let Some(addr) = find_collision(&all_vmras) {
        return Err(vec![RemapError::IdentityConflict { addr }]);
    }

    // Schedule the actual re-mmap: only *pending* (not-yet-remapped)
    // non-importing sets. The importing set re-maps its own bases during the
    // import, and already-remapped sets stay mapped. Deleted files are
    // *included* (not skipped) so `remap_vma` → `check_identity` reports
    // `FileDeleted` and blocks the import rather than silently leaving a
    // dangling key from an un-remappable region.
    let mut to_remap: Vec<(usize, usize)> = Vec::new();
    let mut candidates: Vec<usize> = Vec::new();
    for (i, set) in guard.iter().enumerate() {
        if set.vmras.is_empty() || set.remapped || set.key == import_key {
            continue;
        }
        for (j, _vma) in set.vmras.iter().enumerate() {
            to_remap.push((i, j));
        }
        candidates.push(i);
    }

    // Validate the importing set's *own* pending regions. The importing set is
    // deliberately skipped by the re-mmap loop above (it re-maps its own bases
    // during the import), so none of the file-identity / range-occupation
    // checks would otherwise run for it. Its import relies on re-mapping at
    // exactly these bases to self-heal its dangling keys, which only works if
    // each base is currently free (or still mapped by the identical file); a
    // different occupant would force a heap fallback and dangle the keys.
    if let Some(i) = guard.iter().position(|s| s.key == import_key) {
        let set = &guard[i];
        if !set.remapped {
            for vma in &set.vmras {
                check_identity(vma).map_err(|e| vec![e])?;
                base_is_reusable(vma).map_err(|e| vec![e])?;
            }
        }
    }

    // No cross-set regions need re-mapping. The stopgap then has nothing to do
    // and — crucially — does not require `MAP_FIXED_NOREPLACE`, so an old
    // kernel is not a problem here (the importing-set validation above reads
    // `/proc/self/maps`, which needs no special kernel support).
    if to_remap.is_empty() {
        return Ok(());
    }

    // MAP_FIXED_NOREPLACE needs kernel >= 4.17; on older kernels the flag is
    // silently treated as MAP_FIXED, which would CLOBBER a live mapping. Refuse
    // to re-map (block the import) rather than risk it.
    if !*KERNEL_NO_REPLACE {
        return Err(vec![RemapError::KernelUnsupported]);
    }

    // Re-map every region; commit state only if all of them succeed.
    let mut failures: Vec<RemapError> = Vec::new();
    for (si, vi) in &to_remap {
        if let Err(e) = remap_vma(&guard[*si].vmras[*vi]) {
            failures.push(e);
        }
    }
    if failures.is_empty() {
        for si in &candidates {
            guard[*si].remapped = true;
            // NOTE: `vmras` is intentionally *not* cleared. The re-mapped
            // regions stay mapped for the life of the process, but their
            // address + identity are still needed by the collision detection
            // above: a later import of a *different* set whose deterministic
            // base collides with a re-mapped region must be refused, not
            // silently fall back to the heap and dangle its keys.
        }
        Ok(())
    } else {
        Err(failures)
    }
}

/// Verify the recorded file still exists with the same identity (inode,
/// device) and has not shrunk since the snapshot. Shared by the re-mmap path
/// ([`remap_vma`]) and the importing-set self-heal validation
/// ([`base_is_reusable`]).
fn check_identity(vma: &LeanVma) -> Result<(), RemapError> {
    // A deleted (unlinked) file cannot be re-opened, so it cannot be re-mapped
    // or re-read; refuse rather than leave the cached key dangling.
    if vma.deleted {
        return Err(RemapError::FileDeleted {
            path: vma.path.clone(),
        });
    }
    // A region whose snapshot `stat` failed is not trackable: its recorded
    // `size` is 0 by construction, and treating that as "any file satisfies"
    // would let the re-map accept an arbitrary (even truncated/rewritten) file
    // at the original address. Refuse it (the drop must leak the env instead).
    if !vma.size_known {
        return Err(RemapError::IdentityMismatch {
            path: vma.path.clone(),
            detail:
                "region size was not trackable at snapshot time (stat failed); refusing to re-map"
                    .into(),
        });
    }
    // Identity here is (device, inode, "size did not shrink"). That catches
    // recreation (new inode), deletion (deleted flag) and in-place
    // truncation (smaller size). It does NOT catch a same-inode, same-size
    // in-place *rewrite* — the bytes at the revived address could differ from
    // the bytes that produced the dangling `g_native_symbol_cache` keys. That
    // residual is closed by the deployment contract: the `.olean`/`.ir` files
    // are compiler build artifacts that must not be rewritten in place for the
    // lifetime of the process. See the module docs / CHANGELOG.
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
    Ok(())
}

/// The importing set re-maps its own freed bases during the import (Lean's
/// deterministic `mmap`). That self-heal only revives the dangling
/// `g_native_symbol_cache` keys if, at the moment the import runs, every byte
/// of `[addr, addr+len)` is either *unmapped* (the import maps it fresh) or
/// already mapped by the *identical* file at the matching offset (the region
/// was already revived, so the keys are valid). Any other occupant would make
/// the import's single `mmap` of the range fall back to the heap, leaving the
/// keys dangling — so refuse the import.
fn base_is_reusable(vma: &LeanVma) -> Result<(), RemapError> {
    let Some(maps) = read_maps() else {
        // Unreadable maps: we cannot prove the range is free or mapped by the
        // identical file, so we cannot prove the import's self-heal will revive
        // the keys. Fail closed — block the import rather than let it fall back
        // to the heap and dangle the keys.
        return Err(RemapError::MapsUnavailable);
    };
    let end = match vma.addr.checked_add(vma.len) {
        Some(e) => e,
        None => return Err(RemapError::RangeOccupied { addr: vma.addr }),
    };
    if vma.len == 0 {
        return Ok(());
    }
    let parsed: Vec<MapsVma> = maps.lines().filter_map(parse_maps_line).collect();
    let mut pos = vma.addr;
    while pos < end {
        match parsed.iter().find(|m| m.start <= pos && pos < m.end) {
            None => {
                // `pos` is unmapped (free) — the import will map it. Advance to
                // the start of the next VMA (or `end`) to skip the gap.
                let next = parsed
                    .iter()
                    .filter(|m| m.start > pos)
                    .map(|m| m.start)
                    .min();
                pos = std::cmp::min(next.unwrap_or(end), end);
            }
            Some(m) => {
                // Occupied: it must be the identical file at the matching offset.
                if m.inode != vma.inode || m.device != vma.device || m.path != vma.path {
                    return Err(RemapError::RangeOccupied { addr: vma.addr });
                }
                let off_at_pos = match m.offset.checked_add((pos - m.start) as u64) {
                    Some(o) => o,
                    None => return Err(RemapError::RangeOccupied { addr: vma.addr }),
                };
                let expected_offset = match vma.offset.checked_add((pos - vma.addr) as u64) {
                    Some(o) => o,
                    None => return Err(RemapError::RangeOccupied { addr: vma.addr }),
                };
                if off_at_pos != expected_offset {
                    return Err(RemapError::RangeOccupied { addr: vma.addr });
                }
                pos = std::cmp::min(m.end, end);
            }
        }
    }
    Ok(())
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
    // Verify the file identity (exists, inode, device, size not shrunk) before
    // touching any mapping; see `check_identity`.
    check_identity(vma)?;
    let file = File::open(&vma.path).map_err(|source| RemapError::OpenFailed {
        path: vma.path.clone(),
        source,
    })?;
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
            size_known: true,
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
        // Recording an empty set must not create an entry for that key. Check
        // the specific key rather than the registry's total length, so the
        // assertion is independent of concurrent tests adding their own sets.
        let key = key_of(&["A-empty-check"]);
        record_freed_set(&["A-empty-check"], vec![]);
        let guard = FREED_SETS.lock().unwrap();
        assert!(
            !guard.iter().any(|s| s.key == key),
            "recording an empty set must not create a set entry"
        );
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
            size_known: true,
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
            size_known: true,
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
            size_known: false,
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
            size_known: true,
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

    // ---- Item 1+2 regression: the per-env all-file-backed gate ----

    #[test]
    fn safe_to_free_requires_count_to_match_added_vmras() {
        // All regions file-backed: region count == file VMAs added ⇒ free.
        assert!(safe_to_free_regions(Some(6828), 6828));
        assert!(safe_to_free_regions(Some(1), 1));
        // Mixed: some regions heap-backed (fewer file VMAs than regions) ⇒ leak.
        assert!(!safe_to_free_regions(Some(6828), 0));
        assert!(!safe_to_free_regions(Some(6828), 6825));
        // Over-count (should not happen, but be conservative) ⇒ leak.
        assert!(!safe_to_free_regions(Some(6828), 6829));
        // Unreadable region count ⇒ cannot prove ⇒ leak.
        assert!(!safe_to_free_regions(None, 6828));
        // Empty env (no regions) is not freeable ⇒ leak.
        assert!(!safe_to_free_regions(Some(0), 0));
    }

    #[test]
    fn vma_eq_compares_size_and_deleted() {
        let a = mk_vma(0x1000, 0x100, 0, 1, "/a.olean");
        let mut b = a.clone();
        assert!(vma_eq(&a, &b), "identical VMAs must match");
        b.size = a.size + 1;
        assert!(!vma_eq(&a, &b), "size change must break equality");
        let mut c = mk_vma(0x1000, 0x100, 0, 1, "/a.olean");
        c.deleted = true;
        assert!(!vma_eq(&a, &c), "deleted flag must break equality");
    }

    // ---- Item 3 regression: the importing set's base must not be clobbered ----

    #[test]
    fn pending_set_cannot_occupy_importing_base() {
        // A (the importing set) and B (a pending, not-yet-remapped set) both
        // have a recorded VMA at the same address but back *different* files
        // (a deterministic-base collision). Importing A must be refused (the
        // pending B would otherwise be re-mapped onto A's base and break A's
        // self-heal), not silently scheduled.
        let a = mk_vma(0x7f00_0006_0000, 0x1000, 0, 1, "/a.olean");
        let b = mk_vma(0x7f00_0006_0000, 0x1000, 0, 2, "/b.olean");
        record_freed_set(&["S-imp-occ-A"], vec![a.clone()]);
        record_freed_set(&["S-imp-occ-B"], vec![b.clone()]);
        // Importing a *third* set must still surface the collision: B's pending
        // VMA occupies A's recorded base.
        let err = remap_cross_set_bases(&["S-imp-occ-C"]).unwrap_err();
        assert!(
            err.iter()
                .any(|e| matches!(e, RemapError::IdentityConflict { .. })),
            "a pending set occupying another freed base must block the re-map, got {err:?}"
        );
    }

    #[test]
    fn base_is_reusable_when_range_free_or_identical() {
        // Allocate a page we own, then test the three occupancy cases against
        // it without ever clobbering an address we do not control.
        let path = std::env::temp_dir().join(format!("leo3_kareuse_{}.olean", std::process::id()));
        std::fs::write(&path, [0u8; 4096]).unwrap();
        let md = std::fs::metadata(&path).unwrap();
        let fd = File::open(&path).unwrap();
        let base = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                4096,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        assert!(base != MAP_FAILED, "anonymous mmap failed");
        let want = base as usize;
        let dev = st_dev_to_maps(md.dev());
        let identical = LeanVma {
            addr: want,
            len: 4096,
            offset: 0,
            inode: md.ino(),
            device: dev,
            size: md.len(),
            path: path.to_string_lossy().into_owned(),
            deleted: false,
            size_known: true,
        };
        // 1. Occupied by an anonymous page (different file) => refused.
        assert!(
            matches!(
                base_is_reusable(&identical).unwrap_err(),
                RemapError::RangeOccupied { .. }
            ),
            "a base occupied by a different (anonymous) mapping must be refused"
        );
        // 2. Map the real file over the page we own => identical => reusable.
        let p = unsafe {
            libc::mmap(
                base,
                4096,
                libc::PROT_READ,
                libc::MAP_PRIVATE | libc::MAP_FIXED,
                fd.as_raw_fd(),
                0,
            )
        };
        assert_eq!(p as usize, want, "MAP_FIXED over our own page failed");
        assert!(
            base_is_reusable(&identical).is_ok(),
            "an identically-occupied base must be reusable"
        );
        // 3. Same range claimed by a different file (wrong inode) => refused.
        let different = LeanVma {
            inode: md.ino() + 1,
            path: format!("{}/other.olean", std::env::temp_dir().display()),
            ..identical.clone()
        };
        assert!(
            matches!(
                base_is_reusable(&different).unwrap_err(),
                RemapError::RangeOccupied { .. }
            ),
            "a base claimed by a different file must be refused"
        );
        unsafe { libc::munmap(p, 4096) };
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn ranges_overlap_is_half_open_interval() {
        // Touching at the boundary is NOT an overlap (half-open).
        let a = mk_vma(0x1000, 0x1000, 0, 1, "/a.olean"); // [0x1000, 0x2000)
        let b = mk_vma(0x2000, 0x1000, 0, 2, "/b.olean"); // [0x2000, 0x3000)
        assert!(!ranges_overlap(&a, &b), "adjacent ranges must not overlap");
        // Partial overlap: a=[0x1000,0x3000), b=[0x2000,0x3000).
        let a = mk_vma(0x1000, 0x2000, 0, 1, "/a.olean");
        let b = mk_vma(0x2000, 0x1000, 0, 2, "/b.olean");
        assert!(ranges_overlap(&a, &b), "partial overlap must be detected");
        // Containment: a=[0x1000,0x4000), b=[0x2000,0x3000).
        let a = mk_vma(0x1000, 0x3000, 0, 1, "/a.olean");
        let b = mk_vma(0x2000, 0x1000, 0, 2, "/b.olean");
        assert!(ranges_overlap(&a, &b), "containment must be an overlap");
        // Identical ranges overlap.
        let c = mk_vma(0x1000, 0x1000, 0, 1, "/a.olean");
        let d = mk_vma(0x1000, 0x1000, 0, 1, "/a.olean");
        assert!(ranges_overlap(&c, &d));
    }

    #[test]
    fn find_collision_detects_partial_overlap_but_not_identical() {
        // Two VMAs with partially overlapping ranges and DIFFERENT files ->
        // conflict (detected even though no (addr,len) pair is exact):
        // a=[0xaa_0000,0xca_0000), b=[0xab_0000,0xbb_0000).
        let a = mk_vma(0x7f00_00aa_0000, 0x200_0000, 0, 1, "/a.olean");
        let b = mk_vma(0x7f00_00ab_0000, 0x100_0000, 0, 2, "/b.olean");
        assert!(
            find_collision(&[&a, &b]).is_some(),
            "partially-overlapping ranges from different sets must be a conflict"
        );
        // The SAME region recorded twice (identical identity) is not a
        // conflict — it is the same mapping and dedup-able.
        let x = mk_vma(0x7f00_00aa_0000, 0x200_0000, 0, 1, "/a.olean");
        let y = mk_vma(0x7f00_00aa_0000, 0x200_0000, 0, 1, "/a.olean");
        assert!(
            find_collision(&[&x, &y]).is_none(),
            "identical regions are the same mapping, not a conflict"
        );
        // Disjoint ranges (with a gap) -> no conflict:
        // p=[0xaa_0000,0xab_0000), q=[0xac_0000,0xad_0000) (0x1_0000 = 0x10000).
        let p = mk_vma(0x7f00_00aa_0000, 0x1_0000, 0, 1, "/a.olean");
        let q = mk_vma(0x7f00_00ac_0000, 0x1_0000, 0, 2, "/b.olean");
        assert!(
            find_collision(&[&p, &q]).is_none(),
            "disjoint ranges must not be a conflict"
        );
    }

    #[test]
    fn snapshot_from_unreadable_maps_fails_closed() {
        assert!(
            matches!(snapshot_from(None), Err(RemapError::MapsUnavailable)),
            "an unreadable /proc/self/maps must be reported, not treated as empty"
        );
    }

    #[test]
    fn parse_maps_content_keeps_only_lean_suffixes() {
        let maps = "\
00400000-00401000 r--p 00000000 00:00 0
7f0000000000-7f0000001000 r--p 00000000 08:01 1234 /opt/lean/lib/Lean.olean
7f0000002000-7f0000003000 r--p 00000000 08:01 1235 /opt/lean/lib/Lean.ir
7f0000004000-7f0000005000 r--p 00000000 08:01 1236 /opt/lean/lib/other.so
";
        let vm = parse_maps_content(maps);
        assert_eq!(
            vm.len(),
            2,
            "only the .olean and .ir mappings should be kept"
        );
        assert!(vm
            .iter()
            .all(|v| { v.path.ends_with(".olean") || v.path.ends_with(".ir") }));
        assert!(
            vm.iter().all(|v| !v.deleted),
            "none of these are marked deleted"
        );
    }

    #[test]
    fn parse_marks_unstatable_region_untrackable() {
        // A maps line for a lean path whose `stat` fails must be recorded as
        // NOT trackable (`size_known = false`), not as a 0-byte region that any
        // file would satisfy.
        let maps = "7f0000006000-7f0000007000 r--p 00000000 08:01 9999 /no/such/dir/mod.olean\n";
        let vm = parse_maps_content(maps);
        let v = vm.iter().find(|v| v.path.ends_with("mod.olean")).unwrap();
        assert!(
            !v.size_known,
            "a region whose snapshot stat failed must be marked untrackable"
        );
        assert_eq!(v.size, 0);
    }

    #[test]
    fn remap_refuses_untrackable_region() {
        // An untrackable region must be refused by the identity check rather
        // than accepted (a `size == 0` from a failed stat is not a valid
        // 0-byte identity that any file satisfies).
        let vma = LeanVma {
            addr: 0x7f00_0009_0000,
            len: 0x1000,
            offset: 0,
            inode: 1,
            device: 0,
            size: 0,
            path: "/no/such/dir/mod.olean".into(),
            deleted: false,
            size_known: false,
        };
        let err = check_identity(&vma).unwrap_err();
        assert!(
            matches!(err, RemapError::IdentityMismatch { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn poison_quarantine_blocks_remap() {
        // A quarantined keepalive must block the cross-set re-map (and thus the
        // import) rather than risk dereferencing a dangling cache key.
        struct ResetOnDrop;
        impl Drop for ResetOnDrop {
            fn drop(&mut self) {
                test_reset_poison();
            }
        }
        let _reset = ResetOnDrop;
        test_reset_poison(); // start clean
        poison_keepalive();
        assert!(keepalive_poisoned());
        let err = remap_cross_set_bases(&["S-poison-test"]).unwrap_err();
        assert!(
            err.iter().any(|e| matches!(e, RemapError::Poisoned)),
            "quarantined re-map must return Poisoned, got {err:?}"
        );
        test_reset_poison();
        assert!(
            !keepalive_poisoned(),
            "quarantine must be resettable in tests"
        );
    }
}
