#[cfg(not(feature = "macros"))]
fn main() {}

#[cfg(feature = "macros")]
use criterion::{criterion_group, criterion_main, Criterion};
#[cfg(feature = "macros")]
use leo3::prelude::*;
#[cfg(feature = "macros")]
use std::hint::black_box;

#[cfg(feature = "macros")]
#[leanfn]
fn bench_add_u64(a: u64, b: u64) -> u64 {
    a + b
}

#[cfg(feature = "macros")]
#[leanfn]
fn bench_identity_u64(x: u64) -> u64 {
    x
}

#[cfg(feature = "macros")]
#[leanfn]
fn bench_string_len(s: String) -> u64 {
    s.len() as u64
}

#[cfg(feature = "macros")]
fn manual_ffi_add(lean: Lean, a: u64, b: u64) -> LeanResult<u64> {
    let la = LeanUInt64::mk(lean, a)?;
    let lb = LeanUInt64::mk(lean, b)?;
    let result = LeanUInt64::add(lean, &la, &lb)?;
    Ok(LeanUInt64::to_u64(&result))
}

#[cfg(feature = "macros")]
fn bench_macro_vs_direct_u64(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("macro_vs_direct_u64");

    group.bench_function("direct_rust_call", |b| {
        b.iter(|| {
            let result = black_box(21u64) + black_box(21u64);
            black_box(result);
        });
    });

    group.bench_function("macro_ffi_wrapper", |b| {
        b.iter(|| {
            leo3::with_lean(|lean| -> LeanResult<()> {
                let a = LeanUInt64::mk(lean, black_box(21))?;
                let b_val = LeanUInt64::mk(lean, black_box(21))?;
                unsafe {
                    let result_ptr = __leo3_leanfn_bench_add_u64::__ffi_bench_add_u64(
                        a.into_ptr(),
                        b_val.into_ptr(),
                    );
                    let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
                    black_box(LeanUInt64::to_u64(&result));
                }
                Ok(())
            })
            .unwrap();
        });
    });

    group.bench_function("manual_lean_api", |b| {
        b.iter(|| {
            leo3::with_lean(|lean| -> LeanResult<()> {
                let result = manual_ffi_add(lean, black_box(21), black_box(21))?;
                black_box(result);
                Ok(())
            })
            .unwrap();
        });
    });

    group.finish();
}

#[cfg(feature = "macros")]
fn bench_macro_identity_overhead(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("macro_identity_overhead");

    group.bench_function("identity_macro_ffi", |b| {
        b.iter(|| {
            leo3::with_lean(|lean| -> LeanResult<()> {
                let x = LeanUInt64::mk(lean, black_box(42))?;
                unsafe {
                    let result_ptr =
                        __leo3_leanfn_bench_identity_u64::__ffi_bench_identity_u64(x.into_ptr());
                    let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
                    black_box(LeanUInt64::to_u64(&result));
                }
                Ok(())
            })
            .unwrap();
        });
    });

    group.bench_function("identity_manual_roundtrip", |b| {
        b.iter(|| {
            leo3::with_lean(|lean| -> LeanResult<()> {
                let x = LeanUInt64::mk(lean, black_box(42))?;
                let val = LeanUInt64::to_u64(&x);
                let y = LeanUInt64::mk(lean, black_box(val))?;
                black_box(LeanUInt64::to_u64(&y));
                Ok(())
            })
            .unwrap();
        });
    });

    group.finish();
}

#[cfg(feature = "macros")]
fn bench_macro_string_overhead(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("macro_string_overhead");

    group.bench_function("string_len_macro_ffi", |b| {
        b.iter(|| {
            leo3::with_lean(|lean| -> LeanResult<()> {
                let s = LeanString::mk(lean, black_box("hello world"))?;
                unsafe {
                    let result_ptr =
                        __leo3_leanfn_bench_string_len::__ffi_bench_string_len(s.into_ptr());
                    let result: LeanBound<LeanUInt64> = LeanBound::from_owned_ptr(lean, result_ptr);
                    black_box(LeanUInt64::to_u64(&result));
                }
                Ok(())
            })
            .unwrap();
        });
    });

    group.bench_function("string_len_manual", |b| {
        b.iter(|| {
            leo3::with_lean(|lean| -> LeanResult<()> {
                let s = LeanString::mk(lean, black_box("hello world"))?;
                let rust_s = LeanString::cstr(&s)?;
                let len = rust_s.len() as u64;
                let result = LeanUInt64::mk(lean, black_box(len))?;
                black_box(LeanUInt64::to_u64(&result));
                Ok(())
            })
            .unwrap();
        });
    });

    group.finish();
}

#[cfg(feature = "macros")]
criterion_group!(
    benches,
    bench_macro_vs_direct_u64,
    bench_macro_identity_overhead,
    bench_macro_string_overhead,
);
#[cfg(feature = "macros")]
criterion_main!(benches);
