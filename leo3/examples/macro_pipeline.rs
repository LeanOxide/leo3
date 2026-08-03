//! Example: end-to-end macro pipeline with `#[leanmodule]`, `#[leanfn]`, and `#[leanclass]`.

#![allow(unused_imports)]

use leo3::prelude::*;

#[derive(Clone, Debug, PartialEq)]
#[leanclass]
struct Counter {
    value: i32,
}

#[leanclass]
impl Counter {
    fn new(initial: i32) -> Self {
        Self { value: initial }
    }

    fn increment(&mut self, delta: i32) {
        self.value += delta;
    }

    fn increment_and_get(&mut self, delta: i32) -> i32 {
        self.value += delta;
        self.value
    }

    fn get(&self) -> i32 {
        self.value
    }
}

#[leanmodule(name = "CounterDemo")]
mod counter_demo {
    use leo3::prelude::*;

    #[allow(unused_imports)]
    #[leanfn(name = "counter_demo_add")]
    pub fn add(a: u64, b: u64) -> u64 {
        a + b
    }

    #[allow(unused_imports)]
    #[leanfn(name = "counter_demo_banner")]
    pub fn banner(name: String, count: i32) -> String {
        format!("{name} has {count} ticks")
    }
}

fn main() -> LeanResult<()> {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        println!("=== Macro Pipeline Example ===");
        println!("Lean class declaration:\n{}\n", COUNTER_LEAN_CLASS_DECL);
        println!("Lean method declarations:\n{}\n", COUNTER_LEAN_METHODS_DECL);

        let init_fn: unsafe extern "C" fn(u8, *mut std::ffi::c_void) -> *mut std::ffi::c_void =
            initialize_CounterDemo;
        println!(
            "Module init symbol is available at {:p}",
            init_fn as *const ()
        );

        let rust_sum = counter_demo::add(20, 22);
        println!("Rust call: add(20, 22) = {}", rust_sum);

        // Scalar-only exports use Lean's unboxed extern ABI: the generated
        // wrapper takes raw `u64` values and returns a raw `u64`.
        let sum = unsafe { counter_demo::counter_demo_add(20, 22) };
        println!("FFI call: add(20, 22) = {}", sum);

        // Class methods mix conventions: scalar parameters cross unboxed,
        // while the external object itself crosses as a boxed pointer.
        let counter_value = unsafe {
            let counter_ptr = __lean_ffi_Counter_new(5);
            let counter_ptr = __lean_ffi_Counter_increment(counter_ptr, 3);
            __lean_ffi_Counter_get(counter_ptr)
        };

        let pair_value = unsafe {
            let counter_ptr = __lean_ffi_Counter_new(10);
            let pair_ptr = __lean_ffi_Counter_increment_and_get(counter_ptr, 7);
            let pair = LeanBound::<LeanProd>::from_owned_ptr(lean, pair_ptr);
            let updated_counter_any = LeanProd::fst(&pair);
            let updated_counter: LeanBound<'_, leo3::external::LeanExternalType<Counter>> =
                updated_counter_any.cast();
            let observed = updated_counter.get_ref().value;
            let result_any = LeanProd::snd(&pair);
            let result: LeanBound<'_, LeanInt32> = result_any.cast();
            let returned = LeanInt32::to_i32(&result);
            assert_eq!(observed, returned);
            returned
        };
        println!(
            "FFI mut+value call: increment_and_get(10, 7) = {}",
            pair_value
        );

        let banner = counter_demo::banner("counter".to_string(), counter_value);
        println!("{}", banner);

        let ffi_banner = unsafe {
            let name = LeanString::mk(lean, "counter")?;
            // `String` stays boxed; the `i32` count crosses unboxed.
            let ptr = counter_demo::counter_demo_banner(name.into_ptr(), counter_value);
            let message = LeanBound::<LeanString>::from_owned_ptr(lean, ptr);
            LeanString::cstr(&message)?.to_owned()
        };
        println!("FFI banner: {}", ffi_banner);

        Ok(())
    })
}
