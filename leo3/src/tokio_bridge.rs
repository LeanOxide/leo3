//! Optional Tokio integration for Lean tasks.
//!
//! This module provides bridges between Lean's task system and Tokio,
//! allowing Lean tasks to be awaited in Tokio runtimes without busy-polling.
//! Blocking Lean waits are delegated to Tokio's blocking pool so the bridge
//! shares Tokio's scheduler instead of creating dedicated helper threads.
//!
//! Enable with the `tokio` feature flag.
//!
//! # Example
//!
//! ```rust,no_run
//! use leo3::instance::LeanAny;
//! use leo3::task::TaskHandle;
//! use leo3::tokio_bridge::lean_block_in_place;
//! use leo3::unbound::LeanUnbound;
//!
//! async fn bridge(handle: TaskHandle<LeanAny>) {
//!     let result: LeanUnbound<LeanAny> = handle.into_tokio_future().await;
//!     let _ = result;
//!
//!     let value = lean_block_in_place(|| 2 + 2);
//!     assert_eq!(value, 4);
//! }
//! ```

use crate::closure::LeanClosure;
use crate::instance::LeanAny;
use crate::task::{LeanTask, TaskHandle};
use crate::unbound::LeanUnbound;

impl<'l> LeanTask<'l, LeanAny> {
    /// Spawn a Lean task and return a `tokio::task::JoinHandle` that resolves
    /// when the Lean task completes.
    ///
    /// Internally, Tokio's blocking pool waits for the Lean task to finish, so
    /// the async scheduler is never blocked.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use leo3::closure::LeanClosure;
    /// use leo3::instance::LeanAny;
    /// use leo3::task::LeanTask;
    /// use leo3::unbound::LeanUnbound;
    ///
    /// async fn spawn_on_tokio<'l>(closure: LeanClosure<'l>) {
    ///     let join_handle = LeanTask::spawn_on_tokio(closure);
    ///     let result: LeanUnbound<LeanAny> = join_handle.await.unwrap();
    ///     let _ = result;
    /// }
    /// ```
    pub fn spawn_on_tokio(
        closure: LeanClosure<'l>,
    ) -> ::tokio::task::JoinHandle<LeanUnbound<LeanAny>> {
        let handle = LeanTask::spawn(closure).into_handle();

        ::tokio::task::spawn_blocking(move || handle.get_unbound())
    }
}

impl<T: Send + 'static> TaskHandle<T> {
    /// Convert this handle into a future compatible with Tokio runtimes.
    ///
    /// Tokio's blocking pool waits for the Lean task to complete, avoiding
    /// busy-polling and avoiding blocking the async executor.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use leo3::instance::LeanAny;
    /// use leo3::task::TaskHandle;
    /// use leo3::unbound::LeanUnbound;
    ///
    /// async fn into_tokio_future(handle: TaskHandle<LeanAny>) {
    ///     let result: LeanUnbound<LeanAny> = handle.into_tokio_future().await;
    ///     let _ = result;
    /// }
    /// ```
    pub async fn into_tokio_future(self) -> LeanUnbound<T> {
        ::tokio::task::spawn_blocking(move || self.get_unbound())
            .await
            .expect("lean-tokio-bridge blocking task panicked")
    }
}

/// Run a synchronous closure within a Tokio context without blocking the
/// runtime's worker threads.
///
/// This is a thin wrapper around [`tokio::task::block_in_place`] for running
/// synchronous Lean FFI operations (e.g., `TaskHandle::get`, environment
/// queries) from within an async Tokio task.
///
/// # Panics
///
/// Panics if called from outside a Tokio multi-thread runtime context
/// (same as `tokio::task::block_in_place`).
///
/// # Example
///
/// ```rust,no_run
/// use leo3::tokio_bridge::lean_block_in_place;
///
/// let result = lean_block_in_place(|| 2 + 2);
/// assert_eq!(result, 4);
/// ```
pub fn lean_block_in_place<F, R>(f: F) -> R
where
    F: FnOnce() -> R,
{
    ::tokio::task::block_in_place(f)
}
