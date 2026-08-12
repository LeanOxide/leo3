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
use std::sync::{Mutex, Once};

static ENV_INIT: Once = Once::new();

/// Ensure `Init.Prelude` is initialized.
///
/// Delegates to the full official initialization sequence (which covers
/// `Init.Prelude`), so no module is ever initialized twice.
#[inline]
pub(crate) fn ensure_prelude_initialized() {
    ensure_environment_initialized();
}

/// Ensure `Lean.Expr` is initialized.
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
        {
            extern "C" {
                #[link_name = "l_Lean_Elab_Tactic_tacticElabAttribute"]
                static tactic_attr: *mut ffi::lean_object;
            }
            if tactic_attr.is_null() || ffi::inline::lean_is_scalar(tactic_attr) {
                panic!("tacticElabAttribute not initialized after initialize_Lean");
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
        extern "C" {
            #[link_name = "l_Lean_Elab_Tactic_evalIntro___regBuiltin_Lean_Elab_Tactic_evalIntro__1"]
            fn reg_builtin_intro(w: *mut ffi::lean_object) -> *mut ffi::lean_object;
        }
        let w = ffi::io::lean_io_mk_world();
        let reg_res = reg_builtin_intro(w);
        assert!(ffi::io::lean_io_result_is_ok(reg_res), "regBuiltin intro failed");
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
pub fn run_worker<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    with_worker(f)
}
