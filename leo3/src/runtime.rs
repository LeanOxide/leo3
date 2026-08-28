//! Internal Lean runtime initialization and worker-thread coordination.
//!
//! This module is intentionally kept private. Public API exposure is controlled
//! from `lib.rs` via feature-gated modules, while the shared runtime bootstrap
//! remains available to the core crate implementation regardless of feature set.
//!
//! The runtime model has three layers:
//!
//! - a single long-lived worker thread owns one-time Lean runtime/module
//!   initialization and serialized environment/meta operations
//! - Lean's own task manager executes `LeanTask` bodies asynchronously once the
//!   runtime worker has initialized it
//! - caller-created threads may interact with MT-marked Lean objects after
//!   calling `crate::sync::ensure_lean_thread()`
//!
//! Waiting follows the same split: worker calls use blocking rendezvous
//! channels, while task-oriented polling paths share the task backoff helpers
//! in `crate::task`.

use leo3_ffi as ffi;
use std::sync::mpsc;
#[cfg(all(
    lean_4_25,
    target_os = "linux",
    any(feature = "meta", feature = "task")
))]
use std::sync::{Condvar, LazyLock};
use std::sync::{Mutex, Once};

static ENV_INIT: Once = Once::new();

/// Ensure `Init.Prelude` is initialized.
///
/// Delegates to the full official initialization sequence (which covers
/// `Init.Prelude`), so no module is ever initialized twice.
#[cfg(feature = "meta")]
#[inline]
pub(crate) fn ensure_prelude_initialized() {
    ensure_environment_initialized();
}

/// Ensure `Lean.Expr` is initialized.
#[cfg(feature = "meta")]
#[inline]
pub(crate) fn ensure_expr_initialized() {
    ensure_environment_initialized();
}

/// Ensure `Lean.Environment` and its transitive dependencies are initialized.
#[inline]
pub(crate) fn ensure_environment_initialized() {
    ENV_INIT.call_once(|| unsafe {
        // Official `lean_initialize` sequence (see lean4 src/initialize/init.cpp):
        // the whole Init/Std/Lean module trees (including Options, Parser,
        // Elab, Meta — required for real elaborator use), then the C++
        // kernel/library modules. Replaces the previous partial manual list.
        ffi::initialize_util_module();
        let w = ffi::io::lean_io_mk_world();
        let r = ffi::initialize_Init(1, w as *mut std::ffi::c_void);
        debug_assert!(ffi::io::lean_io_result_is_ok(r));
        let w = ffi::io::lean_io_mk_world();
        let r = ffi::initialize_Std(1, w as *mut std::ffi::c_void);
        debug_assert!(ffi::io::lean_io_result_is_ok(r));
        let w = ffi::io::lean_io_mk_world();
        let r = ffi::initialize_Lean(1, w as *mut std::ffi::c_void);
        debug_assert!(ffi::io::lean_io_result_is_ok(r));
        ffi::initialize_kernel_module();
        ffi::init_default_print_fn();
        ffi::initialize_library_core_module();
        ffi::initialize_library_module();
        ffi::initialize_constructions_module();
        #[cfg(lean_4_25)]
        {
            // W-387: read via the reliable cross-platform accessor. On
            // Windows a raw `extern static` import of this DLL data symbol
            // reads null/stale (Rust extern-static imports are unreliable
            // for Windows DLL data symbols), which made this canary panic
            // even though `initialize_Lean` had set the DLL's global. The
            // accessor routes Windows through `GetProcAddress` + deref.
            let tactic_attr = ffi::meta::get_tacticElabAttribute();
            if tactic_attr.is_null() || ffi::inline::lean_is_scalar(tactic_attr) {
                panic!(
                    "tacticElabAttribute not initialized after initialize_Lean \
                     (read {tactic_attr:#p}; null => symbol not exported or \
                     init chain did not run, scalar => unexpected encoding)"
                );
            }
        }
        // Ensure builtin tactic registrations exist (the Lean module
        // initializer chain does not always reach BuiltinTactic).
        extern "C" {
            #[link_name = "initialize_Lean_Compiler_InitAttr"]
            fn init_compiler_init_attr(
                builtin: u8,
                w: *mut std::ffi::c_void,
            ) -> *mut ffi::lean_object;
        }
        // `Lean.regularInitAttr` (the `[init]` attribute extension) gates
        // `runInitAttrs` inside `finalizePersistentExtensions` — without it,
        // builtin registrations (`@[builtin_tactic]` etc.) never run on
        // import. The `initialize_Lean` dependency chain does not reach
        // `Lean.Compiler.InitAttr`, so register it explicitly.
        let w = ffi::io::lean_io_mk_world();
        let ria_res = init_compiler_init_attr(1, w as *mut std::ffi::c_void);
        assert!(ffi::io::lean_io_result_is_ok(ria_res), "init_compiler_init_attr failed");
        // Directly invoke the intro registration function (bypasses the
        // initializer's `_G_initialized` guard).
        //
        // Version notes: the symbol does not exist in Lean 4.20, so the
        // call is gated on 4.25 (where the repl layer, which needs the
        // `intro` tactic registered, also lives). The mangled name of the
        // `regBuiltin` closure changed in Lean 4.31: declarations defined
        // in a file sub-module of `Lean.Elab.Tactic` now carry the
        // `___private_Lean_Elab_Tactic_BuiltinTactic_0__` file-attribution
        // prefix (verified against v4.31.0 / v4.32.2 / v4.33.0; the old
        // name is what v4.25.2 exports).
        #[cfg(lean_4_25)]
        {
            #[cfg(not(lean_4_31))]
            extern "C" {
                #[link_name = "l_Lean_Elab_Tactic_evalIntro___regBuiltin_Lean_Elab_Tactic_evalIntro__1"]
                fn reg_builtin_intro(w: *mut ffi::lean_object) -> *mut ffi::lean_object;
            }
            #[cfg(lean_4_31)]
            extern "C" {
                #[link_name = "l___private_Lean_Elab_Tactic_BuiltinTactic_0__Lean_Elab_Tactic_evalIntro___regBuiltin_Lean_Elab_Tactic_evalIntro__1"]
                fn reg_builtin_intro(w: *mut ffi::lean_object) -> *mut ffi::lean_object;
            }
            let w = ffi::io::lean_io_mk_world();
            let reg_res = reg_builtin_intro(w);
            assert!(ffi::io::lean_io_result_is_ok(reg_res), "regBuiltin intro failed");
        }
        // Allow `importModules (loadExts := true)` to run module
        // initializers while loading `.olean` files (lean CLI order:
        // init_search_path → enable_initializer_execution → mark_end).
        //
        // `mark_end_initialization` is intentionally deferred: module
        // initializers for imported `.olean` files only run while
        // `IO.initializing` is still true, and Lean's own CLI imports the
        // user's file before marking the end. Leo3 calls
        // `finalize_initialization()` after the first module import.
        let w = ffi::io::lean_io_mk_world();
        let enable_res = ffi::lean_enable_initializer_execution(w as *mut std::ffi::c_void);
        debug_assert!(ffi::io::lean_io_result_is_ok(enable_res));
    });
}

/// Ensure `Lean.Meta` is initialized.
#[cfg(feature = "meta")]
#[inline]
pub(crate) fn ensure_meta_initialized() {
    ensure_environment_initialized();
}

/// Mark the end of the initialization phase (mirrors the lean CLI calling
/// `lean_io_mark_end_initialization` after processing the input file).
/// Safe to call repeatedly.
///
/// Version-independent: `lean_io_mark_end_initialization` is part of Lean's
/// public C API on every supported version, so this lives here (not in the
/// `lean_4_25`-gated `repl` module) — `LeanEnvironment::empty` needs it on
/// all versions.
#[cfg(feature = "meta")]
pub fn finalize_initialization() {
    static FINALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !FINALIZED.swap(true, std::sync::atomic::Ordering::SeqCst) {
        unsafe {
            ffi::lean_io_mark_end_initialization();
        }
    }
}

/// Wrapper to force `Send` on types that cross the worker-thread channel.
///
/// SAFETY: The calling thread blocks until the worker finishes, so the
/// wrapped value's captures are guaranteed to outlive the worker's use of them.
#[cfg(any(feature = "meta", feature = "task"))]
struct SendBox<T>(T);
#[cfg(any(feature = "meta", feature = "task"))]
unsafe impl<T> Send for SendBox<T> {}

/// Global worker thread state.
#[allow(clippy::type_complexity)]
static WORKER: Mutex<Option<mpsc::SyncSender<Box<dyn FnOnce() + Send>>>> = Mutex::new(None);

/// `ThreadId` of the long-lived Lean worker, captured when it starts. Used to
/// detect (and refuse) a call that would dispatch to the worker *from the
/// worker itself*: a self-`with_worker` dispatch deadlocks, because the worker
/// blocks in the rendezvous channel waiting for the task it cannot run.
/// `meta`-only: the sole readers are the `meta` import/free entries, so this is
/// gated to `meta` and is never dead code in a `meta`-off workspace build.
#[cfg(feature = "meta")]
static WORKER_THREAD_ID: Mutex<Option<std::thread::ThreadId>> = Mutex::new(None);

/// True iff the current thread is the long-lived Lean worker thread. Compiled
/// only with `meta` (its sole callers live there) to avoid dead-code warnings
/// in `meta`-off workspace builds.
#[cfg(feature = "meta")]
pub(crate) fn on_worker_thread() -> bool {
    *WORKER_THREAD_ID.lock().unwrap() == Some(std::thread::current().id())
}

/// Ensure the long-lived Lean worker thread is spawned and fully initialized.
///
/// This worker is the canonical serialized path for runtime bootstrap and for
/// operations that must not hop across short-lived threads.
pub(crate) fn ensure_worker_initialized() {
    static WORKER_INIT: Once = Once::new();

    WORKER_INIT.call_once(|| {
        let (tx, rx) = mpsc::sync_channel::<Box<dyn FnOnce() + Send>>(0);
        let (init_tx, init_rx) = mpsc::sync_channel::<()>(0);

        std::thread::Builder::new()
            .name("leo3-runtime-worker".into())
            // Full module-tree initialization (`initialize_Init`/`initialize_Std`/
            // `initialize_Lean`) recurses deeply; the default 2 MiB stack
            // overflows. 64 MiB matches typical host-process expectations.
            .stack_size(64 * 1024 * 1024)
            .spawn(move || {
                #[cfg(feature = "meta")]
                {
                    *WORKER_THREAD_ID.lock().unwrap() = Some(std::thread::current().id());
                }
                unsafe {
                    ffi::lean_initialize_runtime_module();
                    ffi::lean_initialize_thread();
                    ffi::closure::lean_init_task_manager();
                }
                crate::sync::mark_current_thread_initialized();
                ensure_environment_initialized();

                let _ = init_tx.send(());

                for task in rx {
                    task();
                }

                loop {
                    std::thread::park();
                }
            })
            .expect("failed to spawn leo3-runtime-worker thread");

        init_rx.recv().expect("worker thread initialization failed");

        let mut guard = WORKER.lock().unwrap();
        *guard = Some(tx);
    });
}

/// Process-wide, **reentrant** serialization for the environment lifecycle
/// (import / free / record / re-map / snapshot / every worker-dispatched
/// evaluation). Acquired by [`with_worker`], so every op that runs the Lean
/// evaluator on the worker (tactics, commands, elaboration, constant lookup,
/// pretty-printing) is serialized against a concurrent `free_regions` (a
/// `Drop` on another thread) — see the W-417 keepalive stopgap. Moved from
/// `meta::keepalive` to `runtime` so the worker-dispatch primitive itself can
/// hold it; `meta::keepalive` re-exports it for compatibility.
#[cfg(all(
    lean_4_25,
    target_os = "linux",
    any(feature = "meta", feature = "task")
))]
struct ReentrantLifecycleLock {
    /// `(holder, hold-count)`. `holder` is the thread currently owning the
    /// lock; `hold-count` is how many times that thread has acquired it.
    state: Mutex<(Option<std::thread::ThreadId>, u32)>,
    cv: Condvar,
}

#[cfg(all(
    lean_4_25,
    target_os = "linux",
    any(feature = "meta", feature = "task")
))]
impl ReentrantLifecycleLock {
    const fn new() -> Self {
        Self {
            state: Mutex::new((None, 0)),
            cv: Condvar::new(),
        }
    }

    /// Acquire the lock, blocking until it is free. Re-acquiring from the same
    /// thread increments the hold-count instead of blocking (reentrancy), so a
    /// sequence that already holds the lock (import/free, or a multi-step
    /// leotower drop) can call another instrumented entry without deadlocking.
    fn lock(&self) -> ReentrantLifecycleGuard<'_> {
        let me = std::thread::current().id();
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
/// and releases (waking a waiter) only on the last release.
///
/// The guard is deliberately `!Send`: the lock records the *holding thread's*
/// `ThreadId`, so the guard must be dropped on the thread that acquired it.
/// `PhantomData<*const ()>` is the standard "not movable across threads" marker.
#[cfg(all(
    lean_4_25,
    target_os = "linux",
    any(feature = "meta", feature = "task")
))]
pub struct ReentrantLifecycleGuard<'a> {
    lk: &'a ReentrantLifecycleLock,
    _not_send: std::marker::PhantomData<*const ()>,
}

#[cfg(all(
    lean_4_25,
    target_os = "linux",
    any(feature = "meta", feature = "task")
))]
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

#[cfg(all(
    lean_4_25,
    target_os = "linux",
    any(feature = "meta", feature = "task")
))]
static LIFECYCLE_LOCK: LazyLock<ReentrantLifecycleLock> =
    LazyLock::new(ReentrantLifecycleLock::new);

/// Acquire the process-wide lifecycle lock (reentrant). See
/// [`ReentrantLifecycleLock`] for what it serializes.
#[cfg(all(
    lean_4_25,
    target_os = "linux",
    any(feature = "meta", feature = "task")
))]
pub fn lifecycle_lock() -> ReentrantLifecycleGuard<'static> {
    LIFECYCLE_LOCK.lock()
}

/// Dispatch a closure to the long-lived worker thread and block until it completes.
///
/// # Safety
///
/// The closure `f` and its return value cross a thread boundary via channels.
/// Callers must ensure that any captured pointers remain valid and that
/// reference counts are properly managed before and after the call.
#[cfg(any(feature = "meta", feature = "task"))]
pub(crate) fn with_worker<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    ensure_worker_initialized();
    // W-417: serialize every worker-dispatched evaluation against a concurrent
    // `free_regions` (a `Drop` on another thread). Without this, a Repl could
    // run the evaluator (`evalConst` → native symbol lookup) while another
    // Repl's drop frees the compacted regions, dangling the global
    // `g_native_symbol_cache` keys. Reentrant, so an entry that already holds
    // the lock (import/free) re-enters without deadlocking. The guard is held
    // on this caller thread across the rendezvous and dropped here; the worker
    // itself never takes the lock, so this cannot self-deadlock.
    #[cfg(all(lean_4_25, target_os = "linux"))]
    let _lifecycle = lifecycle_lock();

    let sender = {
        let guard = WORKER.lock().unwrap();
        guard.as_ref().unwrap().clone()
    };

    let (done_tx, done_rx) = mpsc::sync_channel::<SendBox<R>>(0);
    let task = move || {
        let result = f();
        let _ = done_tx.send(SendBox(result));
    };

    let task: Box<dyn FnOnce() + Send> = unsafe {
        std::mem::transmute::<Box<dyn FnOnce() + '_>, Box<dyn FnOnce() + Send>>(Box::new(task))
    };

    sender.send(task).expect("runtime worker thread died");
    done_rx.recv().expect("runtime worker thread died").0
}

/// Run a closure on the single long-lived Lean worker thread (the
/// canonical serialized path for FFI calls).
#[cfg(any(feature = "meta", feature = "task"))]
pub fn run_worker<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    with_worker(f)
}

/// Reset the worker thread's Lean heartbeat counter to zero.
///
/// W-407 Bug B: the heartbeat counter (the small-allocation counter) is
/// thread-local and accumulates monotonically across every command run on
/// the worker thread. The monadic FFI entry points used by the `meta`
/// module (`Lean.Elab.runTactic`, `Lean.Meta.ppGoal`/`ppExpr`,
/// `Lean.Elab.Command.elabCommandTopLevel`, `MetaM.run'`) do not go through
/// `CoreM.toIO` — the only place that snapshots the baseline
/// (`initHeartbeats := (← IO.getNumHeartbeats)`, `src/Lean/CoreM.lean`) — so
/// `Core.checkMaxHeartbeatsCore` measures the *process-wide* allocation
/// count against `maxHeartbeats` (200000 × 1000) instead of a single
/// command's. A trivial `runTactic` then deterministically times out once
/// enough prior work has run (each `import Modules` of `Lean` alone costs
/// ~3.56M heartbeats).
///
/// Call this as the first statement of each such command entry, on the
/// worker thread, to restore the intended "200M small allocations per
/// command" semantics. The worker thread serializes all Lean work, so the
/// reset is race-free.
///
/// # Safety
/// - Must be called on the Lean worker thread: the counter is
///   thread-local, and the call must stay inside the serialized FFI path
///   that precedes the command it guards.
#[cfg(feature = "meta")]
pub(crate) unsafe fn reset_heartbeats() {
    let zero = crate::ffi::lean_box(0);
    #[cfg(not(lean_4_26))]
    {
        let world = crate::ffi::io::lean_io_mk_world();
        let result = crate::ffi::io::lean_io_set_heartbeats(zero, world);
        // The callee decs `zero`. `world` is ignored (the result carries
        // its own fresh token), so release it ourselves.
        crate::ffi::lean_dec(world);
        crate::ffi::lean_dec(result);
    }
    #[cfg(lean_4_26)]
    {
        // Lean >= 4.26 (ST redesign): the world token is gone and the
        // return is a raw unit scalar (never dec'd); the callee decs
        // `zero`.
        crate::ffi::io::lean_io_set_heartbeats(zero);
    }
}

#[cfg(all(test, feature = "meta", lean_4_25, target_os = "linux"))]
mod lifecycle_tests {
    use super::*;
    use std::time::Duration;

    /// W-417 item 7: every worker-dispatched evaluation holds the lifecycle
    /// lock, so a concurrent `free_regions`/`Drop` (which also takes the lock)
    /// is serialized against it. A `with_worker` dispatched from another thread
    /// must therefore BLOCK while this thread holds the lock — proving the eval
    /// runs *inside* the lock (not merely before it), so a concurrent drop
    /// cannot free the compacted regions mid-`evalConst`.
    #[test]
    fn with_worker_serializes_against_lifecycle_lock() {
        crate::test_with_lean(|_lean| -> std::result::Result<(), crate::LeanError> {
            let _held = lifecycle_lock();
            let (tx, rx) = mpsc::channel::<bool>();
            let h = std::thread::spawn(move || {
                // Trivial no-op task (no Lean FFI): the only thing that can
                // delay it is the lifecycle lock the main thread holds.
                let ok = with_worker(|| true);
                let _ = tx.send(ok);
            });
            assert!(
                rx.recv_timeout(Duration::from_secs(3)).is_err(),
                "with_worker completed while another thread held the lifecycle lock — \n                 eval is not serialized against a concurrent free_regions"
            );
            drop(_held);
            h.join().expect("worker-dispatch thread panicked");
            assert!(
                rx.recv().unwrap(),
                "the eval must complete once the lock is released"
            );
            Ok(())
        })
        .expect("with_worker / lifecycle-lock serialization test failed");
    }
}
