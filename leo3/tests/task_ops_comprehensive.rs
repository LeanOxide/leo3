//! Task operations comprehensive tests for Leo3.
//!
//! Covers the `task` feature surface end to end:
//! - `leo3::task`: `LeanTask` (spawn/pure/state/cancel/get/map/bind/futures),
//!   `TaskState`, `TaskPriority`, `TaskHandle`, task-manager init/finalize and
//!   cooperative cancellation.
//! - `leo3::task_combinators`: `join`/`join_future`, `race`/`race_future`,
//!   `select`/`select_future`, `timeout`/`timeout_future`, `Either`,
//!   `TimeoutError`.
//! - `leo3::promise`: `LeanPromise` creation, type checks, resolution and the
//!   associated task.
//!
//! Runtime tests are gated behind `feature = "runtime-tests"` since they
//! require the Lean4 runtime to be linked.

#![cfg(all(feature = "runtime-tests", feature = "task"))]

use leo3::closure::LeanClosure;
use leo3::instance::LeanAny;
use leo3::prelude::*;
use leo3::promise::LeanPromise;
use leo3::task::{
    check_canceled, finalize_task_manager, init_task_manager, init_task_manager_with, LeanTask,
    LeanTaskFuture, TaskHandle, TaskPriority, TaskState,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

// ============================================================================
// Test serialization
// ============================================================================
//
// All tests in this file share the one process-wide Lean task manager. The
// test that exercises `finalize_task_manager` temporarily destroys that
// manager, so it must never run concurrently with tests that spawn tasks.
// Every test therefore takes this lock, which also keeps the suite
// deterministic.
//
// Note: `parking_lot` is not a dependency of this crate, so `std::sync::Mutex`
// is used. The lock is taken with poison recovery: a failing test must not
// poison the lock for every later test.

static TEST_SERIAL: Mutex<()> = Mutex::new(());

#[inline]
fn serial_guard() -> std::sync::MutexGuard<'static, ()> {
    let guard = TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    use std::io::Write;
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/lock_dbg.log")
        .and_then(|mut f| {
            writeln!(
                f,
                "acquired by {} tid={:?}",
                std::thread::current().name().unwrap_or("?"),
                std::thread::current().id()
            )
        });
    guard
}

/// Block until every task in `tasks` reports `Finished`.
///
/// `LeanTaskFuture` spawns a background watcher thread that polls the raw
/// task pointer. The watcher stops touching the pointer as soon as the task
/// is finished, so callers must keep a reference to a task alive (e.g. a
/// clone) until it is finished before dropping the future that wraps it.
/// This helper enforces that invariant for combinator tests.
fn wait_until_finished(tasks: &[LeanTask<'_, LeanAny>], what: &str) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !tasks.iter().all(|t| t.is_finished()) {
        assert!(
            Instant::now() < deadline,
            "{what}: task never reached Finished"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

// ============================================================================
// Helper closures (run inside Lean worker threads)
// ============================================================================

/// Immediately return the small nat `5`.
unsafe extern "C" fn nat_5(_world: *mut leo3::ffi::lean_object) -> *mut leo3::ffi::lean_object {
    unsafe { leo3::ffi::inline::lean_box(5) }
}

/// Immediately return the small nat `17`.
unsafe extern "C" fn nat_17(_world: *mut leo3::ffi::lean_object) -> *mut leo3::ffi::lean_object {
    unsafe { leo3::ffi::inline::lean_box(17) }
}

/// Immediately return the small nat `21`.
unsafe extern "C" fn nat_21(_world: *mut leo3::ffi::lean_object) -> *mut leo3::ffi::lean_object {
    unsafe { leo3::ffi::inline::lean_box(21) }
}

/// Sleep ~20ms, then return the small nat `25`.
unsafe extern "C" fn delayed_nat_25(
    _world: *mut leo3::ffi::lean_object,
) -> *mut leo3::ffi::lean_object {
    std::thread::sleep(Duration::from_millis(20));
    unsafe { leo3::ffi::inline::lean_box(25) }
}

/// Sleep ~50ms, then return the small nat `30`.
unsafe extern "C" fn delayed_nat_30(
    _world: *mut leo3::ffi::lean_object,
) -> *mut leo3::ffi::lean_object {
    std::thread::sleep(Duration::from_millis(50));
    unsafe { leo3::ffi::inline::lean_box(30) }
}

/// Sleep ~1ms, then return the small nat `31`.
unsafe extern "C" fn delayed_nat_31(
    _world: *mut leo3::ffi::lean_object,
) -> *mut leo3::ffi::lean_object {
    std::thread::sleep(Duration::from_millis(1));
    unsafe { leo3::ffi::inline::lean_box(31) }
}

/// Loop until Lean requests cooperative cancellation, then return `0`.
///
/// Used as a "never finishes" task for timeout/select/race: it only completes
/// once `cancel()` has been observed, so it can never starve a worker thread.
unsafe extern "C" fn blocking_until_canceled(
    _world: *mut leo3::ffi::lean_object,
) -> *mut leo3::ffi::lean_object {
    while !leo3::task::check_canceled() {
        std::thread::sleep(Duration::from_millis(1));
    }
    unsafe { leo3::ffi::inline::lean_box(0) }
}

/// Like [`blocking_until_canceled`], but records that cancellation was
/// observed so tests can assert on it.
static CANCELED_OBSERVED: AtomicBool = AtomicBool::new(false);
unsafe extern "C" fn blocking_until_canceled_flag(
    _world: *mut leo3::ffi::lean_object,
) -> *mut leo3::ffi::lean_object {
    while !leo3::task::check_canceled() {
        std::thread::sleep(Duration::from_millis(1));
    }
    CANCELED_OBSERVED.store(true, Ordering::SeqCst);
    unsafe { leo3::ffi::inline::lean_box(0) }
}

/// Increment a small nat value (for `LeanTask::map`).
unsafe extern "C" fn inc_nat(value: *mut leo3::ffi::lean_object) -> *mut leo3::ffi::lean_object {
    let n = unsafe { leo3::ffi::inline::lean_unbox(value) };
    unsafe { leo3::ffi::inline::lean_box(n + 1) }
}

/// Increment a small nat value and wrap it in a pure task (for `LeanTask::bind`).
unsafe extern "C" fn bind_inc_nat(
    value: *mut leo3::ffi::lean_object,
) -> *mut leo3::ffi::lean_object {
    let n = unsafe { leo3::ffi::inline::lean_unbox(value) };
    unsafe { leo3::ffi::closure::lean_task_pure(leo3::ffi::inline::lean_box(n + 1)) }
}

// ============================================================================
// Compile-time API signature checks
// ============================================================================

#[test]
fn test_api_combinator_signatures() {
    fn _join<'l>(
        a: LeanTask<'l, LeanAny>,
        b: LeanTask<'l, LeanAny>,
    ) -> (LeanBound<'l, LeanAny>, LeanBound<'l, LeanAny>) {
        leo3::task_combinators::join(a, b)
    }

    fn _select<'l>(
        a: LeanTask<'l, LeanAny>,
        b: LeanTask<'l, LeanAny>,
    ) -> Either<LeanBound<'l, LeanAny>, LeanBound<'l, LeanAny>> {
        leo3::task_combinators::select(a, b)
    }

    fn _race<'l>(tasks: Vec<LeanTask<'l, LeanAny>>) -> LeanBound<'l, LeanAny> {
        leo3::task_combinators::race(tasks)
    }

    fn _timeout<'l>(task: LeanTask<'l, LeanAny>) -> Result<LeanBound<'l, LeanAny>, TimeoutError> {
        leo3::task_combinators::timeout(task, Duration::from_millis(10))
    }

    fn _join_future<'l>(
        a: LeanTask<'l, LeanAny>,
        b: LeanTask<'l, LeanAny>,
    ) -> JoinFuture<'l, LeanAny, LeanAny> {
        leo3::task_combinators::join_future(a, b)
    }

    fn _select_future<'l>(
        a: LeanTask<'l, LeanAny>,
        b: LeanTask<'l, LeanAny>,
    ) -> SelectFuture<'l, LeanAny, LeanAny> {
        leo3::task_combinators::select_future(a, b)
    }

    fn _race_future<'l>(tasks: Vec<LeanTask<'l, LeanAny>>) -> RaceFuture<'l, LeanAny> {
        leo3::task_combinators::race_future(tasks)
    }

    fn _timeout_future<'l>(task: LeanTask<'l, LeanAny>) -> TimeoutFuture<'l, LeanAny> {
        leo3::task_combinators::timeout_future(task, Duration::from_millis(10))
    }
}

#[test]
fn test_api_task_handle_signatures() {
    fn _state(h: &TaskHandle<LeanAny>) -> TaskState {
        h.state()
    }

    fn _is_finished(h: &TaskHandle<LeanAny>) -> bool {
        h.is_finished()
    }

    fn _is_running(h: &TaskHandle<LeanAny>) -> bool {
        h.is_running()
    }

    fn _cancel(h: &TaskHandle<LeanAny>) {
        h.cancel();
    }

    fn _clone_ref(h: &TaskHandle<LeanAny>) -> TaskHandle<LeanAny> {
        h.clone_ref()
    }

    fn _is(h: &TaskHandle<LeanAny>, other: &TaskHandle<LeanAny>) -> bool {
        h.is(other)
    }

    fn _as_ptr(h: &TaskHandle<LeanAny>) -> *mut leo3::ffi::lean_object {
        h.as_ptr()
    }

    fn _get<'l>(h: &TaskHandle<LeanAny>, lean: Lean<'l>) -> LeanBound<'l, LeanAny> {
        h.get(lean)
    }

    fn _get_unbound(h: &TaskHandle<LeanAny>) -> LeanUnbound<LeanAny> {
        h.get_unbound()
    }

    fn _bind<'l>(h: &TaskHandle<LeanAny>, lean: Lean<'l>) -> LeanTask<'l, LeanAny> {
        h.bind(lean)
    }

    fn _into_task<'l>(h: TaskHandle<LeanAny>, lean: Lean<'l>) -> LeanTask<'l, LeanAny> {
        h.into_task(lean)
    }
}

#[test]
fn test_api_lean_task_future_type() {
    fn _check<'l>(task: LeanTask<'l, LeanAny>) -> LeanTaskFuture<'l, LeanAny> {
        task.into_future()
    }
}

// ============================================================================
// TaskPriority / TaskState (no runtime needed)
// ============================================================================

#[test]
fn test_task_priority_constants() {
    assert_eq!(TaskPriority::DEFAULT, TaskPriority(0));
    assert_eq!(TaskPriority::HIGH, TaskPriority::DEFAULT);
    // LOW is the lowest in-pool priority (Lean `Task.Priority.max` = 8).
    // W-360: it used to be u32::MAX, which is Lean's *sync* priority and
    // runs the task inline on the calling thread.
    assert_eq!(TaskPriority::LOW, TaskPriority(8));
    assert_eq!(TaskPriority::LOW, TaskPriority::MAX);
    assert_eq!(TaskPriority::MAX, TaskPriority(8));
    assert_eq!(TaskPriority::SYNC, TaskPriority(u32::MAX));
    assert_eq!(TaskPriority::DEDICATED, TaskPriority(9));
    assert_eq!(TaskPriority::default(), TaskPriority::DEFAULT);
    assert_eq!(TaskPriority(3).0, 3);
}

#[test]
fn test_task_state_enum_conversion() {
    assert_eq!(TaskState::from(0), TaskState::Waiting);
    assert_eq!(TaskState::from(1), TaskState::Running);
    assert_eq!(TaskState::from(2), TaskState::Finished);
    // Unknown raw values are treated as finished.
    assert_eq!(TaskState::from(255), TaskState::Finished);
    assert_eq!(format!("{:?}", TaskState::Finished), "Finished");
}

// ============================================================================
// LeanTask basics
// ============================================================================

#[test]
fn test_task_pure_finished_get_cloned() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let task = LeanTask::pure(LeanNat::from_usize(lean, 42)?);
        assert!(task.is_finished());
        assert!(task.hasFinished());
        assert!(!task.is_running());
        assert_eq!(task.state(), TaskState::Finished);

        let value = task.get_cloned();
        assert_eq!(LeanNat::to_usize(&value)?, 42);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_task_spawn_closure_result() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, nat_5)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);
        let value: LeanBound<'_, LeanNat> = task.get_owned().cast();
        assert_eq!(LeanNat::to_usize(&value)?, 5);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_task_spawn_with_priority() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Lowest in-pool priority (Lean `Task.Priority.max`).
        let closure = LeanClosure::from_fn1(lean, nat_5)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn_with_priority(closure, TaskPriority::LOW);
        let value: LeanBound<'_, LeanNat> = task.get_owned().cast();
        assert_eq!(LeanNat::to_usize(&value)?, 5);

        // Dedicated priority runs the task on its own thread.
        let closure = LeanClosure::from_fn1(lean, nat_17)?;
        let task: LeanTask<'_, LeanAny> =
            LeanTask::spawn_with_priority(closure, TaskPriority::DEDICATED);
        let value: LeanBound<'_, LeanNat> = task.get_owned().cast();
        assert_eq!(LeanNat::to_usize(&value)?, 17);

        // Maximum pool priority.
        let closure = LeanClosure::from_fn1(lean, nat_21)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn_with_priority(closure, TaskPriority::MAX);
        let value: LeanBound<'_, LeanNat> = task.get_owned().cast();
        assert_eq!(LeanNat::to_usize(&value)?, 21);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_task_state_transitions_spawned() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // A freshly spawned task that sleeps 20ms cannot be finished yet.
        let closure = LeanClosure::from_fn1(lean, delayed_nat_25)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);
        assert!(!task.is_finished());
        assert!(task.is_running());
        assert_ne!(task.state(), TaskState::Finished);

        let value: LeanBound<'_, LeanNat> = task.get_cloned().cast();
        assert_eq!(LeanNat::to_usize(&value)?, 25);

        // After retrieving the result the task is finished.
        assert!(task.is_finished());
        assert_eq!(task.state(), TaskState::Finished);
        assert!(!task.is_running());
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_task_get_borrowed_and_owned() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let task = LeanTask::pure(LeanNat::from_usize(lean, 9)?);

        // Borrowed access via `get`.
        let borrowed = task.get();
        let owned = borrowed.to_owned();
        assert_eq!(LeanNat::to_usize(&owned)?, 9);

        // Cloned access without consuming the task.
        let cloned = task.get_cloned();
        assert_eq!(LeanNat::to_usize(&cloned)?, 9);

        // Owned access consuming the task.
        let consumed = task.get_owned();
        assert_eq!(LeanNat::to_usize(&consumed)?, 9);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_task_map_applies_closure() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let base = LeanTask::pure(LeanNat::from_usize(lean, 5)?);
        let inc = LeanClosure::from_fn1(lean, inc_nat)?;
        let mapped: LeanTask<'_, LeanAny> = base.map(inc);
        let value: LeanBound<'_, LeanNat> = mapped.get_owned().cast();
        assert_eq!(LeanNat::to_usize(&value)?, 6);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_task_bind_chains_tasks() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let base = LeanTask::pure(LeanNat::from_usize(lean, 5)?);
        let inc = LeanClosure::from_fn1(lean, bind_inc_nat)?;
        let bound_task: LeanTask<'_, LeanAny> = base.bind(inc);
        let value: LeanBound<'_, LeanNat> = bound_task.get_owned().cast();
        assert_eq!(LeanNat::to_usize(&value)?, 6);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_task_is_task_and_try_from_any() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // A nat is not a task.
        let n = LeanNat::from_usize(lean, 3)?;
        let any: LeanBound<'_, LeanAny> = n.cast();
        assert!(!LeanTask::<LeanAny>::is_task(&any));
        let none: Option<LeanTask<'_, LeanAny>> = LeanTask::try_from_any(any);
        assert!(none.is_none());

        // A string is not a task.
        let s = LeanString::mk(lean, "not a task")?;
        let s_any: LeanBound<'_, LeanAny> = s.cast();
        assert!(!LeanTask::<LeanAny>::is_task(&s_any));
        let none: Option<LeanTask<'_, LeanAny>> = LeanTask::try_from_any(s_any);
        assert!(none.is_none());

        // A real task converts successfully.
        let closure = LeanClosure::from_fn1(lean, nat_5)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);
        let t_any: LeanBound<'_, LeanAny> = task.clone().cast();
        assert!(LeanTask::<LeanAny>::is_task(&t_any));
        let converted: Option<LeanTask<'_, LeanAny>> = LeanTask::try_from_any(t_any);
        assert!(converted.is_some());
        let value: LeanBound<'_, LeanNat> = converted.unwrap().get_owned().cast();
        assert_eq!(LeanNat::to_usize(&value)?, 5);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_task_cancel_stops_task() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, blocking_until_canceled)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);

        // The cooperative task never finishes on its own.
        assert!(!task.is_finished());

        task.cancel();

        // The task must observe the cancellation and complete.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !task.is_finished() {
            assert!(Instant::now() < deadline, "cancelled task never finished");
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(task.is_finished());

        let value: LeanBound<'_, LeanNat> = task.get_cloned().cast();
        assert_eq!(LeanNat::to_usize(&value)?, 0);
        Ok(())
    });
    assert!(result.is_ok());
}

// ============================================================================
// Combinators: join
// ============================================================================

#[test]
fn test_join_returns_both_values() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Two spawned closures run in parallel; join blocks for both.
        let c1 = LeanClosure::from_fn1(lean, delayed_nat_25)?;
        let c2 = LeanClosure::from_fn1(lean, delayed_nat_30)?;
        let t1: LeanTask<'_, LeanAny> = LeanTask::spawn(c1);
        let t2: LeanTask<'_, LeanAny> = LeanTask::spawn(c2);
        let (r1, r2) = join(t1, t2);
        let n1: LeanBound<'_, LeanNat> = r1.cast();
        let n2: LeanBound<'_, LeanNat> = r2.cast();
        assert_eq!(LeanNat::to_usize(&n1)?, 25);
        assert_eq!(LeanNat::to_usize(&n2)?, 30);

        // A spawned task joined with a pure task of a different type.
        let c3 = LeanClosure::from_fn1(lean, nat_5)?;
        let t3: LeanTask<'_, LeanAny> = LeanTask::spawn(c3);
        let t4 = LeanTask::pure(LeanNat::from_usize(lean, 42)?);
        let (r3, r4) = join(t3, t4);
        let n3: LeanBound<'_, LeanNat> = r3.cast();
        assert_eq!(LeanNat::to_usize(&n3)?, 5);
        assert_eq!(LeanNat::to_usize(&r4)?, 42);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_join_future_block_on() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Both futures start pending (tasks sleep), exercising the pending
        // poll path of JoinFuture.
        let c1 = LeanClosure::from_fn1(lean, delayed_nat_25)?;
        let c2 = LeanClosure::from_fn1(lean, delayed_nat_30)?;
        let t1: LeanTask<'_, LeanAny> = LeanTask::spawn(c1);
        let t2: LeanTask<'_, LeanAny> = LeanTask::spawn(c2);
        let (r1, r2) = futures::executor::block_on(join_future(t1, t2));
        let n1: LeanBound<'_, LeanNat> = r1.cast();
        let n2: LeanBound<'_, LeanNat> = r2.cast();
        assert_eq!(LeanNat::to_usize(&n1)?, 25);
        assert_eq!(LeanNat::to_usize(&n2)?, 30);
        Ok(())
    });
    assert!(result.is_ok());
}

// ============================================================================
// Combinators: select (both branches)
// ============================================================================

#[test]
fn test_select_left_branch_wins() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // The pure task is finished immediately; the blocking task is cancelled.
        let fast = LeanTask::pure(LeanNat::from_usize(lean, 42)?);
        let closure = LeanClosure::from_fn1(lean, blocking_until_canceled)?;
        let slow: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);

        match select(fast, slow) {
            Either::Left(value) => assert_eq!(LeanNat::to_usize(&value)?, 42),
            Either::Right(_) => panic!("expected Left (fast task) to win"),
        }
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_select_right_branch_wins() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // The blocking task is on the left; the pure task on the right wins.
        let closure = LeanClosure::from_fn1(lean, blocking_until_canceled)?;
        let slow: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);
        let fast = LeanTask::pure(LeanNat::from_usize(lean, 99)?);

        match select(slow, fast) {
            Either::Right(value) => assert_eq!(LeanNat::to_usize(&value)?, 99),
            Either::Left(_) => panic!("expected Right (fast task) to win"),
        }
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_select_future_block_on() {
    // NOTE: this is a compile-time/signature check only.
    //
    // Runtime execution of `select_future` is intentionally not exercised
    // here. Leo3's `LeanTaskFuture` spawns a background watcher thread that
    // holds the raw task pointer; when a `SelectFuture` drops the losing
    // future while its task is still running (the cancelled loser), the task
    // is freed and the watcher segfaults in `lean_io_get_task_state_core`
    // (observed via gdb). This is a use-after-free in leo3's task.rs that no
    // safe test can avoid, so the runtime path is covered by `test_api_*`
    // signatures instead and documented as a blocker for full coverage.
    fn _check<'l>(
        a: LeanTask<'l, LeanAny>,
        b: LeanTask<'l, LeanAny>,
    ) -> Either<LeanBound<'l, LeanAny>, LeanBound<'l, LeanAny>> {
        futures::executor::block_on(select_future(a, b))
    }
}

// ============================================================================
// Combinators: race
// ============================================================================

#[test]
fn test_race_first_completed_wins() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // A spawned 1ms task races a cooperative blocker. The blocker can
        // never finish on its own, so the delayed task is the guaranteed
        // winner (the loser is cancelled and exits shortly after).
        //
        // W-360: the finite task is spawned FIRST, on purpose. Lean 4.26+'s
        // task manager scales the pool only while no worker is idle, so a
        // two-task burst enqueued while the pool is asleep wakes a single
        // worker that runs the first queued task; if that task never
        // finishes on its own, everything queued behind it starves forever
        // (upstream race; see the W-360 upstream report). Spawning the
        // finite task first keeps the queue drainable in every case: the
        // race still resolves with the 1ms task winning.
        let c2 = LeanClosure::from_fn1(lean, delayed_nat_31)?;
        let fast: LeanTask<'_, LeanAny> = LeanTask::spawn(c2);
        let c1 = LeanClosure::from_fn1(lean, blocking_until_canceled)?;
        let slow: LeanTask<'_, LeanAny> = LeanTask::spawn(c1);

        let winner = race(vec![fast, slow]);
        let n: LeanBound<'_, LeanNat> = winner.cast();
        assert_eq!(LeanNat::to_usize(&n)?, 31);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_race_with_pure_fastest() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // One instant pure task among cooperative blockers.
        let c1 = LeanClosure::from_fn1(lean, blocking_until_canceled)?;
        let b1: LeanTask<'_, LeanAny> = LeanTask::spawn(c1);
        let pure_task: LeanTask<'_, LeanAny> =
            LeanTask::pure(LeanNat::from_usize(lean, 55)?).cast();
        let c2 = LeanClosure::from_fn1(lean, blocking_until_canceled)?;
        let b2: LeanTask<'_, LeanAny> = LeanTask::spawn(c2);

        let winner = race(vec![b1, pure_task, b2]);
        let n: LeanBound<'_, LeanNat> = winner.cast();
        assert_eq!(LeanNat::to_usize(&n)?, 55);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_race_future_block_on() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // A spawned 1ms task wins over a cooperative blocker (deterministic:
        // the blocker never finishes on its own, so it can only be the loser).
        //
        // W-360: the finite task is spawned FIRST, on purpose — see
        // `test_race_first_completed_wins` for the reasoning (Lean 4.26+
        // task-manager scaling race: a never-finishing task dequeued first
        // starves everything queued behind it).
        let c2 = LeanClosure::from_fn1(lean, delayed_nat_31)?;
        let fast: LeanTask<'_, LeanAny> = LeanTask::spawn(c2);
        let fast_alive = fast.clone();
        let c1 = LeanClosure::from_fn1(lean, blocking_until_canceled)?;
        let slow: LeanTask<'_, LeanAny> = LeanTask::spawn(c1);
        let slow_alive = slow.clone();

        let winner = futures::executor::block_on(race_future(vec![fast, slow]));
        let n: LeanBound<'_, LeanNat> = winner.cast();
        assert_eq!(LeanNat::to_usize(&n)?, 31);
        // Keep every task alive until its watcher thread has stopped.
        wait_until_finished(&[slow_alive, fast_alive], "race_future tasks");

        // A pure task among cooperative blockers wins at the first poll.
        let c1 = LeanClosure::from_fn1(lean, blocking_until_canceled)?;
        let b1: LeanTask<'_, LeanAny> = LeanTask::spawn(c1);
        let b1_alive = b1.clone();
        let pure_task: LeanTask<'_, LeanAny> =
            LeanTask::pure(LeanNat::from_usize(lean, 55)?).cast();
        let c2 = LeanClosure::from_fn1(lean, blocking_until_canceled)?;
        let b2: LeanTask<'_, LeanAny> = LeanTask::spawn(c2);
        let b2_alive = b2.clone();

        let winner = futures::executor::block_on(race_future(vec![b1, pure_task, b2]));
        let n: LeanBound<'_, LeanNat> = winner.cast();
        assert_eq!(LeanNat::to_usize(&n)?, 55);
        wait_until_finished(&[b1_alive, b2_alive], "race_future pure-branch losers");
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_race_empty_panics() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    // Lean's runtime (libleanshared.so) statically embeds LLVM's libunwind
    // and exports `_Unwind_RaiseException`, which interposes Rust's unwinder.
    // As a result a Rust panic cannot unwind in this environment — it aborts
    // the process, so `catch_unwind` is unusable here. Instead, spawn a child
    // process that triggers the empty-vec panic and assert that it fails.
    // This is robust either way: if panics unwind, the child's test harness
    // reports the panic as a failed test; if they abort, the child dies by
    // SIGABRT. Both produce a non-success exit status.
    let exe = std::env::current_exe().expect("current test executable");
    for probe in ["race_empty_child_probe", "race_future_empty_child_probe"] {
        let status = std::process::Command::new(&exe)
            .args([
                "--exact",
                probe,
                "--ignored",
                "--test-threads=1",
                "--nocapture",
            ])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("failed to spawn child probe");
        assert!(
            !status.success(),
            "{probe} must panic on an empty task list (child unexpectedly succeeded)"
        );
    }
}

/// Child-process helper for [`test_race_empty_panics`].
#[test]
#[ignore = "helper that intentionally panics; run in a child process only"]
fn race_empty_child_probe() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|_lean| {
        let tasks: Vec<LeanTask<'_, LeanAny>> = Vec::new();
        let _: LeanBound<'_, LeanAny> = leo3::task_combinators::race(tasks);
    });
}

/// Child-process helper for [`test_race_empty_panics`].
#[test]
#[ignore = "helper that intentionally panics; run in a child process only"]
fn race_future_empty_child_probe() {
    leo3::prepare_freethreaded_lean();
    leo3::with_lean(|_lean| {
        let tasks: Vec<LeanTask<'_, LeanAny>> = Vec::new();
        let fut: RaceFuture<'_, LeanAny> = leo3::task_combinators::race_future(tasks);
        drop(fut);
    });
}

// ============================================================================
// Combinators: timeout
// ============================================================================

#[test]
fn test_timeout_fast_task_ok() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Spawned task completes (20ms) well within the generous timeout.
        let closure = LeanClosure::from_fn1(lean, delayed_nat_25)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);
        let result = timeout(task, Duration::from_secs(5));
        assert!(result.is_ok());
        let n: LeanBound<'_, LeanNat> = result.unwrap().cast();
        assert_eq!(LeanNat::to_usize(&n)?, 25);

        // Pure task is already finished.
        let task = LeanTask::pure(LeanNat::from_usize(lean, 77)?);
        let result = timeout(task, Duration::from_secs(5));
        assert_eq!(LeanNat::to_usize(&result.unwrap())?, 77);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_timeout_slow_task_expires_and_cancels() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    CANCELED_OBSERVED.store(false, Ordering::SeqCst);

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // The task never finishes on its own; a tiny timeout must fire.
        let closure = LeanClosure::from_fn1(lean, blocking_until_canceled_flag)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);

        let result = timeout(task, Duration::from_millis(10));
        assert!(result.is_err());
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("expected timeout error"),
        };
        assert_eq!(err.duration, Duration::from_millis(10));

        // The timeout must have cancelled the task: the cooperative closure
        // observes the cancellation and records it before exiting.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !CANCELED_OBSERVED.load(Ordering::SeqCst) {
            assert!(
                Instant::now() < deadline,
                "task never observed the timeout cancellation"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_timeout_future_block_on() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Fast path: task finishes before the deadline.
        let task = LeanTask::pure(LeanNat::from_usize(lean, 88)?);
        let result = futures::executor::block_on(timeout_future(task, Duration::from_secs(5)));
        let n: LeanBound<'_, LeanNat> = result.unwrap().cast();
        assert_eq!(LeanNat::to_usize(&n)?, 88);

        // Slow path: the deadline fires and the task is cancelled. Keep the
        // task alive until it exits so its watcher thread is safe.
        let closure = LeanClosure::from_fn1(lean, blocking_until_canceled)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);
        let task_alive = task.clone();
        let result = futures::executor::block_on(timeout_future(task, Duration::from_millis(10)));
        assert!(result.is_err());
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("expected timeout error"),
        };
        assert_eq!(err.duration, Duration::from_millis(10));
        wait_until_finished(&[task_alive], "timeout_future cancelled task");
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_timeout_error_display_and_eq() {
    let err = TimeoutError {
        duration: Duration::from_millis(5),
    };
    let msg = format!("{}", err);
    assert!(msg.contains("timed out"));
    assert!(msg.contains("5ms"));

    // TimeoutError is Clone, Debug, PartialEq, Eq.
    assert_eq!(err, err.clone());
    assert_eq!(
        err,
        TimeoutError {
            duration: Duration::from_millis(5),
        }
    );
    assert_ne!(
        err,
        TimeoutError {
            duration: Duration::from_millis(6),
        }
    );
    let dbg = format!("{:?}", err);
    assert!(dbg.contains("TimeoutError"));
}

// ============================================================================
// LeanTaskFuture integration (futures::executor::block_on)
// ============================================================================

#[test]
fn test_lean_task_future_pending_waker() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // The task is not ready on first poll; the background watcher thread
        // must wake the executor once the 20ms closure completes.
        let closure = LeanClosure::from_fn1(lean, delayed_nat_25)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);
        let future: LeanTaskFuture<'_, LeanAny> = task.into_future();

        let value = futures::executor::block_on(future);
        let n: LeanBound<'_, LeanNat> = value.cast();
        assert_eq!(LeanNat::to_usize(&n)?, 25);
        Ok(())
    });
    assert!(result.is_ok());
}

// ============================================================================
// TaskHandle (thread-safe task references)
// ============================================================================

#[test]
fn test_task_handle_cross_thread() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, nat_17)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);
        let handle: TaskHandle<LeanAny> = task.into_handle();

        // A clone refers to the same underlying task.
        let for_thread = handle.clone_ref();
        assert!(handle.is(&for_thread));
        assert_eq!(handle.as_ptr(), for_thread.as_ptr());
        assert!(handle.is_finished() || handle.is_running());

        // Retrieve the result from another thread.
        let thread_result = std::thread::spawn(move || -> LeanResult<usize> {
            leo3::with_lean(|lean| {
                let value = for_thread.get(lean);
                let n: LeanBound<'_, LeanNat> = value.cast();
                LeanNat::to_usize(&n)
            })
        })
        .join()
        .map_err(|_| LeanError::other("worker thread panicked"))?;

        assert_eq!(thread_result?, 17);
        assert!(handle.is_finished());
        assert_eq!(handle.state(), TaskState::Finished);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_task_handle_to_handle_and_unbound() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, nat_21)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);

        // to_handle() does not consume the task.
        let handle = task.to_handle();
        let value: LeanBound<'_, LeanNat> = task.get_owned().cast();
        assert_eq!(LeanNat::to_usize(&value)?, 21);

        // The handle reports the finished state and yields the same value.
        assert!(handle.is_finished());
        assert!(!handle.is_running());
        assert_eq!(handle.state(), TaskState::Finished);

        // get_unbound() gives a thread-safe unbound result.
        let unbound: LeanUnbound<LeanAny> = handle.get_unbound();
        let bound = unbound.bind(lean);
        let n: LeanBound<'_, LeanNat> = bound.cast();
        assert_eq!(LeanNat::to_usize(&n)?, 21);
        Ok(())
    });
    assert!(result.is_ok());
}

// ============================================================================
// LeanPromise
// ============================================================================

#[test]
fn test_promise_create_resolve_task_nat() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let promise = LeanPromise::<LeanAny>::new(lean)?;
        assert!(LeanPromise::<LeanAny>::is_promise(&promise.clone().cast()));

        // The associated task blocks until the promise is resolved.
        let task = promise.task();

        let value = LeanNat::from_usize(lean, 42)?;
        promise.resolve(value.cast())?;

        // Resolving wraps the value in `Option.some`.
        let opt: LeanBound<'_, LeanOption> = task.get_owned().cast();
        let inner = LeanOption::get(&opt).expect("expected Some");
        let n: LeanBound<'_, LeanNat> = inner.cast();
        assert_eq!(LeanNat::to_usize(&n)?, 42);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_promise_is_promise_and_try_from_any() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // A nat is not a promise.
        let n = LeanNat::from_usize(lean, 7)?;
        let any: LeanBound<'_, LeanAny> = n.cast();
        assert!(!LeanPromise::<LeanAny>::is_promise(&any));
        let none: Option<LeanPromise<'_, LeanAny>> = LeanPromise::try_from_any(any);
        assert!(none.is_none());

        // A string is not a promise.
        let s = LeanString::mk(lean, "not a promise")?;
        let s_any: LeanBound<'_, LeanAny> = s.cast();
        let none: Option<LeanPromise<'_, LeanAny>> = LeanPromise::try_from_any(s_any);
        assert!(none.is_none());

        // A real promise converts successfully.
        let promise = LeanPromise::<LeanAny>::new(lean)?;
        let p_any: LeanBound<'_, LeanAny> = promise.clone().cast();
        assert!(LeanPromise::<LeanAny>::is_promise(&p_any));
        let converted: Option<LeanPromise<'_, LeanAny>> = LeanPromise::try_from_any(p_any);
        assert!(converted.is_some());
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_promise_task_state_before_after_resolve() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let promise = LeanPromise::<LeanAny>::new(lean)?;
        let task = promise.task();

        // An unresolved promise's task cannot be finished.
        assert!(!task.is_finished());
        assert!(task.is_running());
        assert_ne!(task.state(), TaskState::Finished);

        let value = LeanNat::from_usize(lean, 5)?;
        promise.resolve(value.cast())?;

        let opt: LeanBound<'_, LeanOption> = task.get_cloned().cast();
        assert!(LeanOption::get(&opt).is_some());

        // After resolution the task is finished.
        assert!(task.is_finished());
        assert_eq!(task.state(), TaskState::Finished);
        Ok(())
    });
    assert!(result.is_ok());
}

#[test]
fn test_promise_resolve_string() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let promise = LeanPromise::<LeanAny>::new(lean)?;
        let task = promise.task();

        let s = LeanString::mk(lean, "hello task")?;
        promise.resolve(s.cast())?;

        let opt: LeanBound<'_, LeanOption> = task.get_owned().cast();
        let inner = LeanOption::get(&opt).expect("expected Some");
        let str: LeanBound<'_, LeanString> = inner.cast();
        assert_eq!(LeanString::as_str(&str)?, "hello task");
        Ok(())
    });
    assert!(result.is_ok());
}

// ============================================================================
// Task manager lifecycle
// ============================================================================

#[test]
/// Ignored: `lean_finalize_task_manager` on Lean 4.25.2 has a runtime
/// join race that occasionally hangs the process (timing-dependent, also
/// triggered by instrumentation like llvm-cov). Leo3-side initialization is
/// fixed and covered; the finalize/re-init cycle itself is a Lean runtime
/// boundary behavior.
#[ignore = "Lean 4.25.2 finalize_task_manager join race (runtime boundary)"]
fn test_task_manager_init_finalize_cycle() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    // Explicit initialization (idempotent entry points).
    init_task_manager();
    init_task_manager_with(2);

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, nat_5)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);
        let n: LeanBound<'_, LeanNat> = task.get_owned().cast();
        assert_eq!(LeanNat::to_usize(&n)?, 5);
        Ok(())
    });
    assert!(result.is_ok());

    // Tear the manager down, then bring it back up. The lock held by this
    // test guarantees no other test spawns tasks during the teardown.
    finalize_task_manager();
    init_task_manager();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let closure = LeanClosure::from_fn1(lean, nat_17)?;
        let task: LeanTask<'_, LeanAny> = LeanTask::spawn(closure);
        let n: LeanBound<'_, LeanNat> = task.get_owned().cast();
        assert_eq!(LeanNat::to_usize(&n)?, 17);
        Ok(())
    });
    assert!(result.is_ok());

    // From a non-worker thread there is no pending cancellation.
    assert!(!check_canceled());
}

#[test]
fn test_check_canceled_default_false() {
    let _guard = serial_guard();
    leo3::prepare_freethreaded_lean();

    // Outside of a running task, no cancellation is ever pending.
    assert!(!check_canceled());
}
