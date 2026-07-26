//! Example: Task/Async with Tokio bridge.
//!
//! Demonstrates LeanTask creation, combinators (join, select, race, timeout),
//! LeanPromise, and the tokio async bridge.
//!
//! Run with:
//! ```bash
//! cargo run --example task_async --features tokio
//! ```

use leo3::instance::LeanAny;
use leo3::prelude::*;
use leo3::promise::LeanPromise;
use leo3::task::LeanTask;
use leo3::task_combinators::{self, Either};
use leo3::types::LeanOption;
use std::time::Duration;

unsafe extern "C" fn compute_42(
    _world: *mut leo3::ffi::lean_object,
) -> *mut leo3::ffi::lean_object {
    std::thread::sleep(Duration::from_millis(100));
    leo3::ffi::inline::lean_box(42)
}

unsafe extern "C" fn compute_7(_world: *mut leo3::ffi::lean_object) -> *mut leo3::ffi::lean_object {
    std::thread::sleep(Duration::from_millis(50));
    leo3::ffi::inline::lean_box(7)
}

fn main() -> LeanResult<()> {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        println!("=== Task/Async Example ===\n");

        println!("1. Pure tasks and join:");
        let a = LeanTask::pure(LeanNat::from_usize(lean, 10)?);
        let b = LeanTask::pure(LeanNat::from_usize(lean, 20)?);
        let (ra, rb) = task_combinators::join(a, b);
        println!(
            "   join(10, 20) = ({}, {})",
            LeanNat::to_usize(&ra.cast())?,
            LeanNat::to_usize(&rb.cast())?
        );

        println!("\n2. Spawn and select:");
        let fast = LeanTask::pure(LeanNat::from_usize(lean, 99)?);
        let slow_closure = leo3::closure::LeanClosure::from_fn1(lean, compute_42)?;
        let slow: LeanTask<'_, LeanAny> = LeanTask::spawn(slow_closure);
        let result = task_combinators::select(fast, slow);
        match result {
            Either::Left(val) => println!(
                "   select winner (Left): {}",
                LeanNat::to_usize(&val.cast())?
            ),
            Either::Right(val) => println!(
                "   select winner (Right): {}",
                LeanNat::to_usize(&val.cast())?
            ),
        }

        println!("\n3. Race multiple tasks:");
        let t1: LeanTask<'_, LeanAny> = {
            let c = leo3::closure::LeanClosure::from_fn1(lean, compute_42)?;
            LeanTask::spawn(c)
        };
        let t2: LeanTask<'_, LeanAny> = {
            let c = leo3::closure::LeanClosure::from_fn1(lean, compute_7)?;
            LeanTask::spawn(c)
        };
        let t3: LeanTask<'_, LeanAny> = LeanTask::pure(LeanNat::from_usize(lean, 55)?).cast();
        let winner = task_combinators::race(vec![t1, t2, t3]);
        println!("   race winner: {}", LeanNat::to_usize(&winner.cast())?);

        println!("\n4. Timeout:");
        let task = LeanTask::pure(LeanNat::from_usize(lean, 77)?);
        match task_combinators::timeout(task, Duration::from_secs(5)) {
            Ok(val) => println!("   completed: {}", LeanNat::to_usize(&val.cast())?),
            Err(e) => println!("   timed out: {}", e),
        }

        println!("\n5. Promise:");
        let promise = LeanPromise::<LeanAny>::new(lean)?;
        let task = promise.task();
        let value = LeanNat::from_usize(lean, 123)?;
        promise.resolve(value.cast())?;
        let result = task.get_owned();
        let opt: LeanBound<'_, LeanOption> = result.cast();
        let inner = LeanOption::get(&opt).expect("expected Some");
        let nat: LeanBound<'_, LeanNat> = inner.cast();
        println!("   resolved promise value: {}", LeanNat::to_usize(&nat)?);

        println!("\n6. Tokio bridge (spawn_on_tokio):");
        let handle = {
            let c = leo3::closure::LeanClosure::from_fn1(lean, compute_42)?;
            LeanTask::spawn_on_tokio(c)
        };
        let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
        let unbound = rt.block_on(handle).expect("tokio join");
        let n = unsafe { leo3::ffi::inline::lean_unbox(unbound.as_ptr()) };
        println!("   spawn_on_tokio result: {}", n);

        println!("\n7. TaskHandle into_tokio_future:");
        let task_handle = {
            let c = leo3::closure::LeanClosure::from_fn1(lean, compute_7)?;
            LeanTask::<LeanAny>::spawn(c).into_handle()
        };
        let unbound = rt.block_on(task_handle.into_tokio_future());
        let n = unsafe { leo3::ffi::inline::lean_unbox(unbound.as_ptr()) };
        println!("   into_tokio_future result: {}", n);

        println!("\n=== All task/async operations completed successfully! ===");
        Ok(())
    })
}
