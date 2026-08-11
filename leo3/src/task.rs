//! Safe wrapper for Lean tasks (parallel computation).
//!
//! This module provides [`LeanTask`], a safe wrapper around Lean's task
//! objects that support parallel computation with async/await integration.
//!
//! # Thread Safety
//!
//! For cross-thread task handling, use [`TaskHandle`] which is `Send + Sync`.
//! This allows spawning tasks in one thread and retrieving results in another.
//!
//! # Waiting Model
//!
//! Leo3 exposes Lean's native task primitives directly, but keeps the waiting
//! behavior layered and consistent:
//!
//! - blocking APIs such as [`LeanTask::get`] and [`TaskHandle::get_unbound`] use
//!   Lean's native blocking task wait
//! - polling-based coordination (`select`, `race`, timeout checks, and future
//!   watcher threads) reuse one shared exponential-backoff wait helper
//! - Tokio integration delegates blocking waits to Tokio's blocking pool rather
//!   than spawning ad hoc OS threads per bridge call
//!
//! # Example
//!
//! ```rust,no_run
//! use leo3::prelude::*;
//! use leo3::task::{finalize_task_manager, init_task_manager, LeanTask, TaskHandle};
//! use std::thread;
//!
//! fn main() -> LeanResult<()> {
//!     leo3::prepare_freethreaded_lean();
//!
//!     // Initialize task manager (call once at startup)
//!     init_task_manager();
//!
//!     let handle: TaskHandle<LeanNat> = leo3::with_lean(|lean| {
//!         let value = LeanNat::from_usize(lean, 42)?;
//!         Ok::<TaskHandle<LeanNat>, LeanError>(LeanTask::pure(value).into_handle())
//!     })?;
//!
//!     // Can be sent to another thread
//!     thread::spawn(move || -> LeanResult<()> {
//!         leo3::prepare_freethreaded_lean();
//!         leo3::with_lean(|lean| {
//!             let result = handle.get(lean);
//!             assert_eq!(LeanNat::to_usize(&result)?, 42);
//!             Ok(())
//!         })
//!     })
//!     .join()
//!     .unwrap()?;
//!
//!     // Clean up (call once at shutdown)
//!     finalize_task_manager();
//!     Ok(())
//! }
//! ```

use crate::closure::LeanClosure;
use crate::ffi;
use crate::instance::{LeanAny, LeanBorrowed, LeanBound};
use crate::marker::Lean;
use crate::unbound::LeanUnbound;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

// ============================================================================
// Task Manager Functions
// ============================================================================

/// Serializes the lazy task-manager initialization so concurrent first
/// spawns cannot race Lean's internal `g_task_manager` setup.
static TASK_MANAGER_GUARD: Mutex<()> = Mutex::new(());
static TASK_MANAGER_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Initialize the Lean task manager with the default number of workers.
///
/// This must be called before spawning any tasks. It is safe to call
/// multiple times, but only the first call has any effect.
pub fn init_task_manager() {
    let _guard = TASK_MANAGER_GUARD.lock().unwrap();
    unsafe {
        ffi::closure::lean_init_task_manager();
    }
    TASK_MANAGER_INITIALIZED.store(true, Ordering::Release);
}

/// Initialize the Lean task manager with a specific number of workers.
///
/// # Arguments
///
/// * `num_workers` - The number of worker threads to use.
pub fn init_task_manager_with(num_workers: u32) {
    let _guard = TASK_MANAGER_GUARD.lock().unwrap();
    unsafe {
        ffi::closure::lean_init_task_manager_using(num_workers);
    }
    TASK_MANAGER_INITIALIZED.store(true, Ordering::Release);
}

/// Ensure the task manager is initialized, exactly once, under the
/// initialization lock. `LeanTask::spawn` calls this automatically so
/// users do not need a manual `init_task_manager()` (which remains a
/// supported explicit form).
pub fn ensure_task_manager_initialized() {
    if !TASK_MANAGER_INITIALIZED.load(Ordering::Acquire) {
        let _guard = TASK_MANAGER_GUARD.lock().unwrap();
        if !TASK_MANAGER_INITIALIZED.load(Ordering::Acquire) {
            unsafe {
                ffi::closure::lean_init_task_manager();
            }
            TASK_MANAGER_INITIALIZED.store(true, Ordering::Release);
        }
    }
}

/// Finalize the Lean task manager.
///
/// This should be called before program exit to clean up task resources.
pub fn finalize_task_manager() {
    unsafe {
        ffi::closure::lean_finalize_task_manager();
    }
    TASK_MANAGER_INITIALIZED.store(false, Ordering::Release);
}

/// Check if task cancellation has been requested.
///
/// This can be called from within a task to check for cooperative cancellation.
pub fn check_canceled() -> bool {
    unsafe { ffi::closure::lean_io_check_canceled_core() }
}

// ============================================================================
// Task State
// ============================================================================

/// The state of a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum TaskState {
    /// The task is waiting to be run (queued in the task manager).
    Waiting = 0,
    /// The task is actively running on a thread (or awaiting a promise resolution).
    Running = 1,
    /// The task has completed and its result is available.
    Finished = 2,
}

impl From<u8> for TaskState {
    fn from(value: u8) -> Self {
        match value {
            0 => TaskState::Waiting,
            1 => TaskState::Running,
            2 => TaskState::Finished,
            _ => TaskState::Finished, // Unknown states treated as finished
        }
    }
}

const TASK_WAIT_INITIAL_BACKOFF: Duration = Duration::from_millis(1);
const TASK_WAIT_MAX_BACKOFF: Duration = Duration::from_millis(50);

#[inline]
pub(crate) fn task_state_from_ptr(task_ptr: *mut ffi::lean_object) -> TaskState {
    unsafe { ffi::closure::lean_io_get_task_state_core(task_ptr).into() }
}

fn next_task_wait_backoff(backoff: Duration) -> Duration {
    backoff
        .checked_mul(2)
        .unwrap_or(TASK_WAIT_MAX_BACKOFF)
        .min(TASK_WAIT_MAX_BACKOFF)
}

pub(crate) fn wait_with_task_backoff<F, R>(mut step: F) -> R
where
    F: FnMut() -> Option<R>,
{
    let mut backoff = TASK_WAIT_INITIAL_BACKOFF;

    loop {
        if let Some(result) = step() {
            return result;
        }

        std::thread::sleep(backoff);
        backoff = next_task_wait_backoff(backoff);
    }
}

pub(crate) fn wait_for_task_completion(task_ptr: *mut ffi::lean_object) {
    wait_with_task_backoff(|| (task_state_from_ptr(task_ptr) == TaskState::Finished).then_some(()))
}

pub(crate) fn wait_for_task_completion_until(
    task_ptr: *mut ffi::lean_object,
    deadline: Instant,
) -> bool {
    wait_with_task_backoff(|| {
        if task_state_from_ptr(task_ptr) == TaskState::Finished {
            Some(true)
        } else if Instant::now() >= deadline {
            Some(false)
        } else {
            None
        }
    })
}

// ============================================================================
// Task Priority
// ============================================================================

/// Task priority level.
///
/// # Lean4 Reference
/// Corresponds to Lean's boxed `Task.Priority`/`Nat` ABI.
/// Leo3 keeps this as a small Rust wrapper and boxes it only at the
/// FFI boundary when calling Lean's inline task helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskPriority(pub u32);

impl TaskPriority {
    /// Default priority (0).
    ///
    /// # Lean4 Reference
    /// Corresponds to `Task.Priority.default` in Lean4.
    pub const DEFAULT: Self = Self(0);

    /// High priority (tasks with lower numbers run first).
    pub const HIGH: Self = Self(0);

    /// Low priority.
    pub const LOW: Self = Self(u32::MAX);

    /// Maximum priority within the thread pool.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Task.Priority.max` in Lean4.
    pub const MAX: Self = Self(8);

    /// Priority for dedicated threads.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Task.Priority.dedicated` in Lean4.
    ///
    /// Tasks spawned with this priority get their own dedicated thread
    /// and don't contend with other tasks for threads in the thread pool.
    pub const DEDICATED: Self = Self(9);

    #[inline]
    fn as_lean_obj(self) -> *mut ffi::lean_object {
        unsafe { ffi::lean_box(self.0 as usize) }
    }
}

impl Default for TaskPriority {
    fn default() -> Self {
        Self::DEFAULT
    }
}

// ============================================================================
// LeanTask
// ============================================================================

/// Marker type for Lean task objects.
pub struct LeanTaskType<T = LeanAny> {
    _marker: PhantomData<T>,
}

/// A safe wrapper around a Lean task (parallel computation).
///
/// `LeanTask` provides:
/// - Task spawning with priority control
/// - Blocking and non-blocking result access
/// - Task state inspection
/// - Cancellation support
/// - Async/await integration via `Future`
///
/// # Memory Management
///
/// Like other Lean objects, tasks use reference counting. The `LeanTask`
/// wrapper automatically manages reference counts through [`LeanBound`].
pub type LeanTask<'l, T = LeanAny> = LeanBound<'l, LeanTaskType<T>>;

impl<'l, T> LeanTask<'l, T> {
    // ========================================================================
    // Type Checking
    // ========================================================================

    /// Check if a Lean object is a task.
    ///
    /// Returns `true` if the object is a task, `false` otherwise.
    #[inline]
    pub fn is_task(obj: &LeanBound<'l, LeanAny>) -> bool {
        unsafe {
            let ptr = obj.as_ptr();
            !ffi::inline::lean_is_scalar(ptr) && ffi::inline::lean_is_task(ptr)
        }
    }

    /// Try to convert a `LeanBound<LeanAny>` to a `LeanTask`.
    ///
    /// Returns `Some(task)` if the object is a task, `None` otherwise.
    #[inline]
    pub fn try_from_any(obj: LeanBound<'l, LeanAny>) -> Option<Self> {
        if Self::is_task(&obj) {
            Some(obj.cast())
        } else {
            None
        }
    }

    // ========================================================================
    // Creation
    // ========================================================================

    /// Spawn a new task from a closure.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Task.spawn` in Lean4.
    ///
    /// The closure will be executed asynchronously in a worker thread.
    pub fn spawn(closure: LeanClosure<'l>) -> Self {
        Self::spawn_with_priority(closure, TaskPriority::DEFAULT)
    }

    /// Spawn a new task with a specific priority.
    ///
    /// Tasks with lower priority numbers are executed first.
    /// Use `TaskPriority::DEDICATED` for a dedicated thread.
    pub fn spawn_with_priority(closure: LeanClosure<'l>, priority: TaskPriority) -> Self {
        let lean = closure.lean_token();
        // Lean's task manager is created lazily; initialize it exactly once
        // under a lock so concurrent first spawns cannot race the setup.
        ensure_task_manager_initialized();
        unsafe {
            let ptr = ffi::inline::lean_task_spawn(closure.into_ptr(), priority.as_lean_obj());
            LeanBound::from_owned_ptr(lean, ptr)
        }
    }

    /// Create a pure task that is already completed with a value.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Task.pure` in Lean4.
    ///
    /// This is useful for returning a task when you already have the result.
    pub fn pure(value: LeanBound<'l, T>) -> Self {
        let lean = value.lean_token();
        unsafe {
            let ptr = ffi::closure::lean_task_pure(value.into_ptr());
            LeanBound::from_owned_ptr(lean, ptr)
        }
    }

    // ========================================================================
    // Inspection
    // ========================================================================

    /// Get the current state of this task.
    ///
    /// # Lean4 Reference
    /// Corresponds to `IO.getTaskState` in Lean4.
    pub fn state(&self) -> TaskState {
        task_state_from_ptr(self.as_ptr())
    }

    /// Check if this task has finished and its result is available.
    ///
    /// # Lean4 Reference
    /// Corresponds to `IO.hasFinished` in Lean4.
    #[inline]
    pub fn is_finished(&self) -> bool {
        self.state() == TaskState::Finished
    }

    /// Alias for `is_finished` to match Lean4 naming.
    ///
    /// # Lean4 Reference
    /// Corresponds to `IO.hasFinished` in Lean4.
    #[inline]
    #[allow(non_snake_case)]
    pub fn hasFinished(&self) -> bool {
        self.is_finished()
    }

    /// Check if this task is still running or waiting to run.
    #[inline]
    pub fn is_running(&self) -> bool {
        !self.is_finished()
    }

    // ========================================================================
    // Result Access
    // ========================================================================

    /// Block until the task completes and get a borrowed reference to the result.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Task.get` in Lean4.
    ///
    /// This blocks the current thread until the task finishes.
    pub fn get(&self) -> LeanBorrowed<'_, 'l, T> {
        unsafe {
            let result = ffi::closure::lean_task_get(self.as_ptr());
            LeanBorrowed::from_ptr(result as *const _)
        }
    }

    /// Block until the task completes and get an owned reference to the result.
    ///
    /// This consumes the task and returns an owned reference to the result.
    pub fn get_owned(self) -> LeanBound<'l, T> {
        let lean = self.lean_token();
        unsafe {
            let result = ffi::inline::lean_task_get_own(self.into_ptr());
            LeanBound::from_owned_ptr(lean, result)
        }
    }

    /// Get an owned reference to the result without consuming the task.
    ///
    /// This blocks until the task finishes and increments the reference count
    /// of the result.
    pub fn get_cloned(&self) -> LeanBound<'l, T> {
        self.get().to_owned()
    }

    // ========================================================================
    // Cancellation
    // ========================================================================

    /// Request cancellation of this task.
    ///
    /// This sets a cancellation flag. The task must cooperatively check for
    /// cancellation using [`check_canceled`].
    pub fn cancel(&self) {
        unsafe {
            ffi::closure::lean_io_cancel_core(self.as_ptr());
        }
    }

    // ========================================================================
    // Combinators
    // ========================================================================

    /// Map a function over the task result.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Task.map` in Lean4.
    ///
    /// Returns a new task that will apply `f` to the result of this task.
    pub fn map(self, f: LeanClosure<'l>) -> LeanTask<'l, LeanAny> {
        self.map_with_priority(f, TaskPriority::DEFAULT)
    }

    /// Map a function over the task result with a specific priority.
    pub fn map_with_priority(
        self,
        f: LeanClosure<'l>,
        priority: TaskPriority,
    ) -> LeanTask<'l, LeanAny> {
        let lean = self.lean_token();
        unsafe {
            let ptr = ffi::inline::lean_task_map(
                f.into_ptr(),
                self.into_ptr(),
                priority.as_lean_obj(),
                false,
            );
            LeanBound::from_owned_ptr(lean, ptr)
        }
    }

    /// Monadic bind (flat-map) over the task result.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Task.bind` in Lean4.
    ///
    /// Returns a new task that will apply `f` to the result of this task,
    /// where `f` returns another task.
    pub fn bind(self, f: LeanClosure<'l>) -> LeanTask<'l, LeanAny> {
        self.bind_with_priority(f, TaskPriority::DEFAULT, false)
    }

    /// Monadic bind with priority and sync flag.
    ///
    /// If `sync` is true, the continuation runs synchronously in the same thread.
    pub fn bind_with_priority(
        self,
        f: LeanClosure<'l>,
        priority: TaskPriority,
        sync: bool,
    ) -> LeanTask<'l, LeanAny> {
        let lean = self.lean_token();
        unsafe {
            let ptr = ffi::inline::lean_task_bind(
                self.into_ptr(),
                f.into_ptr(),
                priority.as_lean_obj(),
                sync,
            );
            LeanBound::from_owned_ptr(lean, ptr)
        }
    }

    // ========================================================================
    // Future Integration
    // ========================================================================

    /// Convert this task into a `Future` for async/await.
    ///
    /// # Waker Strategy
    ///
    /// Lean's task system doesn't support native waker callbacks. When the
    /// task isn't immediately ready on first poll, a lightweight background
    /// thread is spawned that monitors the task with exponential backoff
    /// and wakes the executor once the task completes.
    pub fn into_future(self) -> LeanTaskFuture<'l, T> {
        let task_ptr = self.as_ptr();
        // The background watcher thread polls the raw task pointer; give it
        // its own reference so the task object survives even if this future
        // is dropped while the watcher is still polling.
        unsafe {
            ffi::lean_inc(task_ptr);
        }
        LeanTaskFuture {
            task: Some(self),
            waker_state: Arc::new(WakerState {
                task_ptr,
                waker: Mutex::new(None),
                thread_spawned: AtomicBool::new(false),
            }),
        }
    }

    /// Convert this task into a thread-safe `TaskHandle`.
    ///
    /// The handle can be sent across threads and used to retrieve the
    /// task result from any thread.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use std::thread;
    ///
    /// let task = LeanTask::spawn(closure);
    /// let handle = task.into_handle();
    ///
    /// thread::spawn(move || {
    ///     leo3::with_lean(|lean| {
    ///         let result = handle.get(lean);
    ///     });
    /// });
    /// ```
    pub fn into_handle(self) -> TaskHandle<T> {
        TaskHandle {
            inner: self.unbind_mt(),
        }
    }

    /// Get a thread-safe handle without consuming this task.
    ///
    /// Creates a new `TaskHandle` by cloning the reference.
    pub fn to_handle(&self) -> TaskHandle<T> {
        TaskHandle {
            inner: self.clone().unbind_mt(),
        }
    }

    /// Get an unbound result for cross-thread use.
    ///
    /// Blocks until the task completes, then returns the result as
    /// a thread-safe `LeanUnbound`.
    pub fn get_unbound(&self) -> LeanUnbound<T> {
        self.get().to_owned().unbind_mt()
    }
}

// ============================================================================
// Future Implementation
// ============================================================================

/// A `Future` adapter for `LeanTask`.
///
/// This allows using Lean tasks with Rust's async/await syntax.
///
/// # Waker Strategy
///
/// Because Lean's task system doesn't support native waker callbacks, this
/// future spawns a lightweight background thread on first poll when the task
/// isn't ready. The thread polls `lean_io_get_task_state_core` with
/// exponential backoff (1ms → 50ms cap) and calls `waker.wake()` once the
/// task completes. This avoids busy-polling the executor while still
/// providing timely notification.
pub struct LeanTaskFuture<'l, T = LeanAny> {
    task: Option<LeanTask<'l, T>>,
    waker_state: Arc<WakerState>,
}

/// Shared state between the future and its background watcher thread.
struct WakerState {
    /// The raw task pointer, used by the background thread to check state.
    /// Valid for the lifetime of the future (the future owns the task).
    task_ptr: *mut ffi::lean_object,
    /// The waker to notify when the task completes.
    waker: Mutex<Option<Waker>>,
    /// Whether the background watcher thread has been spawned.
    thread_spawned: AtomicBool,
}

// SAFETY: task_ptr points to an MT-marked Lean object whose refcount keeps it alive.
// lean_io_get_task_state_core is safe to call from any thread on a live task object.
unsafe impl Send for WakerState {}
unsafe impl Sync for WakerState {}

impl<'l, T> Future for LeanTaskFuture<'l, T> {
    type Output = LeanBound<'l, T>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let task = self.task.as_ref().expect("Future polled after completion");

        if task.is_finished() {
            let task = self.task.take().unwrap();
            return Poll::Ready(task.get_owned());
        }

        // Store/update the waker so the background thread can notify us
        {
            let mut waker_guard = self.waker_state.waker.lock().unwrap();
            match waker_guard.as_ref() {
                Some(existing) if existing.will_wake(cx.waker()) => {}
                _ => *waker_guard = Some(cx.waker().clone()),
            }
        }

        // Spawn the background watcher thread exactly once
        if !self
            .waker_state
            .thread_spawned
            .swap(true, Ordering::Relaxed)
        {
            let state = Arc::clone(&self.waker_state);
            std::thread::Builder::new()
                .name("lean-task-waker".into())
                .spawn(move || {
                    wait_for_task_completion(state.task_ptr);
                    // Release the watcher's own reference (taken in
                    // `into_future`); the future's own reference is managed
                    // by the `LeanTask` it owns.
                    unsafe {
                        ffi::lean_dec(state.task_ptr);
                    }
                    if let Some(waker) = state.waker.lock().unwrap().take() {
                        waker.wake();
                    }
                })
                .expect("failed to spawn lean-task-waker thread");
        }

        Poll::Pending
    }
}

// ============================================================================
// TaskHandle - Thread-Safe Task Reference
// ============================================================================

/// A thread-safe handle to a Lean task.
///
/// Unlike [`LeanTask`], `TaskHandle` is `Send + Sync` and can be passed
/// between threads. This is useful for:
///
/// - Spawning tasks in one thread and retrieving results in another
/// - Storing task references in concurrent data structures
/// - Building work-stealing or task-pool patterns
///
/// # Example
///
/// ```rust,ignore
/// use leo3::task::TaskHandle;
/// use std::thread;
///
/// // Spawn a task and get a handle
/// let handle: TaskHandle<LeanAny> = task.into_handle();
///
/// // Move to another thread
/// let join_handle = thread::spawn(move || {
///     // Initialize thread for Lean (if not already done)
///     leo3::prepare_freethreaded_lean();
///
///     leo3::with_lean(|lean| {
///         // Get the result in this thread
///         let result = handle.get(lean);
///         // use result...
///     });
/// });
///
/// join_handle.join().unwrap();
/// ```
pub struct TaskHandle<T = LeanAny> {
    inner: LeanUnbound<LeanTaskType<T>>,
}

// SAFETY: TaskHandle wraps LeanUnbound which is already Send + Sync
// and guarantees MT mode for atomic reference counting
unsafe impl<T: Send> Send for TaskHandle<T> {}
unsafe impl<T: Sync> Sync for TaskHandle<T> {}

impl<T> TaskHandle<T> {
    /// Create a new `TaskHandle` from an unbound task.
    ///
    /// The task must already be marked for MT use (via `unbind_mt()`).
    pub fn from_unbound(task: LeanUnbound<LeanTaskType<T>>) -> Self {
        Self { inner: task }
    }

    /// Bind this handle to a `Lean<'l>` lifetime and get a `LeanTask`.
    ///
    /// This creates a bound reference for use within a Lean scope.
    pub fn bind<'l>(&self, lean: Lean<'l>) -> LeanTask<'l, T> {
        self.inner.bind(lean)
    }

    /// Consume this handle and bind it to a `Lean<'l>` lifetime.
    pub fn into_task<'l>(self, lean: Lean<'l>) -> LeanTask<'l, T> {
        self.inner.into_bound(lean)
    }

    /// Get the current state of this task.
    ///
    /// This can be called without a `Lean` token since it only reads
    /// atomic state from the task object.
    pub fn state(&self) -> TaskState {
        task_state_from_ptr(self.inner.as_ptr())
    }

    /// Check if this task has finished.
    #[inline]
    pub fn is_finished(&self) -> bool {
        self.state() == TaskState::Finished
    }

    /// Check if this task is still running or waiting to run.
    #[inline]
    pub fn is_running(&self) -> bool {
        !self.is_finished()
    }

    /// Request cancellation of this task.
    ///
    /// This sets a cancellation flag. The task must cooperatively check
    /// for cancellation using [`check_canceled`].
    pub fn cancel(&self) {
        unsafe {
            ffi::closure::lean_io_cancel_core(self.inner.as_ptr());
        }
    }

    /// Block until the task completes and get the result.
    ///
    /// Returns a `LeanBound` tied to the provided `Lean` lifetime.
    pub fn get<'l>(&self, lean: Lean<'l>) -> LeanBound<'l, T> {
        let task = self.bind(lean);
        task.get_cloned()
    }

    /// Block until the task completes and get an unbound result.
    ///
    /// Returns a `LeanUnbound` that can be used across threads.
    pub fn get_unbound(&self) -> LeanUnbound<T> {
        unsafe {
            // We need to get the result without a Lean token
            // This is safe because:
            // 1. The task is MT-marked
            // 2. lean_task_get blocks until completion
            // 3. We immediately mark the result as MT
            let result = ffi::closure::lean_task_get(self.inner.as_ptr()) as *mut ffi::lean_object;
            ffi::object::lean_inc(result);
            LeanUnbound::from_owned_ptr(result)
        }
    }

    /// Clone this handle (increments reference count).
    #[inline]
    pub fn clone_ref(&self) -> Self {
        Self {
            inner: self.inner.clone_ref(),
        }
    }

    /// Check if this and another handle refer to the same task.
    #[inline]
    pub fn is<U>(&self, other: &TaskHandle<U>) -> bool {
        self.inner.is(&other.inner)
    }

    /// Get the raw pointer to the underlying task object.
    #[inline]
    pub fn as_ptr(&self) -> *mut ffi::lean_object {
        self.inner.as_ptr()
    }
}

impl<T> Clone for TaskHandle<T> {
    fn clone(&self) -> Self {
        self.clone_ref()
    }
}

impl<T> std::fmt::Debug for TaskHandle<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TaskHandle")
            .field("state", &self.state())
            .field("ptr", &self.inner.as_ptr())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_task_type_size() {
        // LeanTask should be pointer-sized (same as LeanBound)
        assert_eq!(
            std::mem::size_of::<LeanTask<LeanAny>>(),
            std::mem::size_of::<*mut ()>()
        );
    }

    #[test]
    fn test_task_state_conversion() {
        assert_eq!(TaskState::from(0), TaskState::Waiting);
        assert_eq!(TaskState::from(1), TaskState::Running);
        assert_eq!(TaskState::from(2), TaskState::Finished);
        assert_eq!(TaskState::from(255), TaskState::Finished);
    }
}
