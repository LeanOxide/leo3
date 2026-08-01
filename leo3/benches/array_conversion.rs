//! Benchmarks for Vec<T> ↔ LeanArray conversions
//!
//! This benchmarks the performance improvements from:
//! 1. Pre-allocation and unchecked push operations
//! 2. Bulk Vec<u8> ↔ LeanByteArray conversions
//! 3. ArrayBuilder pattern
//! 4. Primitive type fast paths

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use leo3::conversion::{vec_u8_into_lean, ArrayBuilder};
use leo3::instance::LeanAny;
use leo3::prelude::*;
use std::hint::black_box;

/// Naive implementation (for comparison) - uses regular push without pre-allocation
fn vec_to_array_naive<'l>(lean: Lean<'l>, vec: Vec<u64>) -> LeanResult<LeanBound<'l, LeanArray>> {
    let mut arr = LeanArray::empty(lean)?;

    for item in vec {
        let lean_item = LeanUInt64::mk(lean, item)?;
        let any_item: LeanBound<'l, LeanAny> = lean_item.cast();
        arr = LeanArray::push(arr, any_item)?;
    }

    Ok(arr)
}

/// Optimized implementation - uses with_capacity + push_unchecked
fn vec_to_array_optimized<'l>(
    lean: Lean<'l>,
    vec: Vec<u64>,
) -> LeanResult<LeanBound<'l, LeanArray>> {
    let len = vec.len();

    if len == 0 {
        return LeanArray::empty(lean);
    }

    let mut arr = LeanArray::with_capacity(lean, len)?;

    for item in vec {
        let lean_item = LeanUInt64::mk(lean, item)?;
        let any_item: LeanBound<'l, LeanAny> = lean_item.cast();
        arr = unsafe { LeanArray::push_unchecked(arr, any_item)? };
    }

    Ok(arr)
}

/// Benchmark converting Vec → LeanArray for different sizes
fn bench_vec_to_array(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("vec_to_array");

    for size in [10, 100, 1000, 10000].iter() {
        let vec: Vec<u64> = (0..*size).collect();

        group.bench_with_input(BenchmarkId::new("naive", size), size, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let v = vec.clone();
                    let arr = vec_to_array_naive(lean, black_box(v)).unwrap();
                    black_box(arr);
                    Ok(())
                })
                .unwrap();
            });
        });

        group.bench_with_input(BenchmarkId::new("optimized", size), size, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let v = vec.clone();
                    let arr = vec_to_array_optimized(lean, black_box(v)).unwrap();
                    black_box(arr);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

/// Benchmark converting LeanArray → Vec for different sizes
fn bench_array_to_vec(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("array_to_vec");

    for size in [10, 100, 1000, 10000].iter() {
        // Pre-create the array
        let arr = leo3::with_lean(|lean| -> LeanResult<_> {
            let mut arr = LeanArray::empty(lean)?;
            for i in 0..*size {
                let n = LeanUInt64::mk(lean, i as u64)?;
                arr = LeanArray::push(arr, n.cast())?;
            }
            Ok(arr.unbind())
        })
        .unwrap();

        group.bench_with_input(BenchmarkId::new("with_prealloc", size), size, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let bound_arr = arr.bind(lean);
                    let vec: Vec<u64> =
                        leo3::conversion::FromLean::from_lean(&black_box(bound_arr)).unwrap();
                    black_box(vec);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

/// Benchmark round-trip conversion Vec → LeanArray → Vec
fn bench_roundtrip(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("roundtrip");

    for size in [10, 100, 1000].iter() {
        let vec: Vec<u64> = (0..*size).collect();

        group.bench_with_input(BenchmarkId::new("naive", size), size, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let v = vec.clone();
                    let arr = vec_to_array_naive(lean, black_box(v)).unwrap();
                    let result: Vec<u64> = leo3::conversion::FromLean::from_lean(&arr).unwrap();
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });

        group.bench_with_input(BenchmarkId::new("optimized", size), size, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let v = vec.clone();
                    let arr = vec_to_array_optimized(lean, black_box(v)).unwrap();
                    let result: Vec<u64> = leo3::conversion::FromLean::from_lean(&arr).unwrap();
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_vec_to_array,
    bench_array_to_vec,
    bench_roundtrip,
    bench_vec_u8_bulk,
    bench_array_builder,
);
criterion_main!(benches);

// =============================================================================
// NEW BENCHMARKS: Vec<u8> bulk conversion
// =============================================================================

/// Benchmark Vec<u8> conversion using bulk memcpy vs element-by-element
fn bench_vec_u8_bulk(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("vec_u8_conversion");

    for size in [100, 1000, 10000, 100000].iter() {
        let vec: Vec<u8> = (0..=255).cycle().take(*size).collect();

        // Element-by-element conversion (using generic Vec<T> trait)
        group.bench_with_input(
            BenchmarkId::new("element_by_element", size),
            size,
            |b, _| {
                b.iter(|| {
                    leo3::with_lean(|lean| -> LeanResult<()> {
                        let v = vec.clone();
                        // Convert to Vec<LeanNat> array (slow path)
                        let mut arr = LeanArray::empty(lean)?;
                        for byte in black_box(v) {
                            let n = LeanNat::from_usize(lean, byte as usize)?;
                            arr = LeanArray::push(arr, n.cast())?;
                        }
                        black_box(arr);
                        Ok(())
                    })
                    .unwrap();
                });
            },
        );

        // Bulk memcpy conversion (optimized)
        group.bench_with_input(BenchmarkId::new("bulk_memcpy", size), size, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let v = vec.clone();
                    let ba = vec_u8_into_lean(black_box(v), lean)?;
                    black_box(ba);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

// =============================================================================
// NEW BENCHMARKS: ArrayBuilder pattern
// =============================================================================

/// Benchmark ArrayBuilder vs manual array construction
fn bench_array_builder(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("array_builder");

    for size in [100, 1000, 10000].iter() {
        // Manual construction with optimized impl
        group.bench_with_input(BenchmarkId::new("manual_optimized", size), size, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let mut arr = LeanArray::with_capacity(lean, *size)?;
                    for i in 0..*size {
                        let n = LeanNat::from_usize(lean, i)?;
                        arr = unsafe { LeanArray::push_unchecked(arr, n.cast())? };
                    }
                    black_box(arr);
                    Ok(())
                })
                .unwrap();
            });
        });

        // Using ArrayBuilder
        group.bench_with_input(BenchmarkId::new("array_builder", size), size, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let mut builder = ArrayBuilder::with_capacity(lean, *size)?;
                    for i in 0..*size {
                        let n = LeanNat::from_usize(lean, i)?;
                        builder.push(n.cast())?;
                    }
                    let arr = builder.finish();
                    black_box(arr);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}
