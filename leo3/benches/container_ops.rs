#[cfg(not(all(feature = "runtime-tests", lean_4_22)))]
fn main() {}

#[cfg(all(feature = "runtime-tests", lean_4_22))]
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
#[cfg(all(feature = "runtime-tests", lean_4_22))]
use leo3::prelude::*;
#[cfg(all(feature = "runtime-tests", lean_4_22))]
use leo3::types::{LeanHashMap, LeanHashSet, LeanRBMap};
#[cfg(all(feature = "runtime-tests", lean_4_22))]
use std::hint::black_box;

#[cfg(all(feature = "runtime-tests", lean_4_22))]
fn bench_hashmap_insert(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("hashmap_insert");

    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("nat_key", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let mut map = LeanHashMap::<LeanNat, LeanNat>::empty(lean)?;
                    for i in 0..size {
                        let key = LeanNat::from_usize(lean, i)?;
                        let val = LeanNat::from_usize(lean, i * 2)?;
                        map = map.insert(lean, key, val)?;
                    }
                    black_box(map);
                    Ok(())
                })
                .unwrap();
            });
        });

        group.bench_with_input(BenchmarkId::new("string_key", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let mut map = LeanHashMap::<LeanString, LeanNat>::empty(lean)?;
                    for i in 0..size {
                        let key = LeanString::mk(lean, &format!("key_{}", i))?;
                        let val = LeanNat::from_usize(lean, i)?;
                        map = map.insert(lean, key, val)?;
                    }
                    black_box(map);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

#[cfg(all(feature = "runtime-tests", lean_4_22))]
fn bench_hashmap_lookup(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("hashmap_lookup");

    for size in [10, 100, 1000].iter() {
        let map = leo3::with_lean(|lean| -> LeanResult<_> {
            let mut map = LeanHashMap::<LeanNat, LeanNat>::empty(lean)?;
            for i in 0..*size {
                let key = LeanNat::from_usize(lean, i)?;
                let val = LeanNat::from_usize(lean, i * 2)?;
                map = map.insert(lean, key, val)?;
            }
            Ok(map.unbind())
        })
        .unwrap();

        group.bench_with_input(BenchmarkId::new("find_hit", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let bound = map.bind(lean);
                    let key = LeanNat::from_usize(lean, black_box(size / 2))?;
                    let result = bound.find(lean, &key)?;
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });

        group.bench_with_input(BenchmarkId::new("find_miss", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let bound = map.bind(lean);
                    let key = LeanNat::from_usize(lean, black_box(size + 1))?;
                    let result = bound.find(lean, &key)?;
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });

        group.bench_with_input(BenchmarkId::new("contains", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let bound = map.bind(lean);
                    let key = LeanNat::from_usize(lean, black_box(size / 2))?;
                    let result = bound.contains(lean, &key)?;
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

#[cfg(all(feature = "runtime-tests", lean_4_22))]
fn bench_hashmap_remove(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("hashmap_remove");

    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("erase", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let mut map = LeanHashMap::<LeanNat, LeanNat>::empty(lean)?;
                    for i in 0..size {
                        let key = LeanNat::from_usize(lean, i)?;
                        let val = LeanNat::from_usize(lean, i)?;
                        map = map.insert(lean, key, val)?;
                    }
                    for i in 0..size {
                        let key = LeanNat::from_usize(lean, i)?;
                        map = map.erase(lean, &key)?;
                    }
                    black_box(map);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

#[cfg(all(feature = "runtime-tests", lean_4_22))]
fn bench_hashset_ops(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("hashset_ops");

    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("insert", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let mut set = LeanHashSet::<LeanNat>::empty(lean)?;
                    for i in 0..size {
                        let elem = LeanNat::from_usize(lean, i)?;
                        set = set.insert(lean, elem)?;
                    }
                    black_box(set);
                    Ok(())
                })
                .unwrap();
            });
        });

        group.bench_with_input(BenchmarkId::new("contains", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let mut set = LeanHashSet::<LeanNat>::empty(lean)?;
                    for i in 0..size {
                        let elem = LeanNat::from_usize(lean, i)?;
                        set = set.insert(lean, elem)?;
                    }
                    for i in (0..size).step_by(size / 10 + 1) {
                        let elem = LeanNat::from_usize(lean, i)?;
                        black_box(set.contains(lean, &elem)?);
                    }
                    Ok(())
                })
                .unwrap();
            });
        });

        group.bench_with_input(BenchmarkId::new("erase", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let mut set = LeanHashSet::<LeanNat>::empty(lean)?;
                    for i in 0..size {
                        let elem = LeanNat::from_usize(lean, i)?;
                        set = set.insert(lean, elem)?;
                    }
                    for i in 0..size {
                        let elem = LeanNat::from_usize(lean, i)?;
                        set = set.erase(lean, &elem)?;
                    }
                    black_box(set);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

#[cfg(all(feature = "runtime-tests", lean_4_22))]
fn bench_rbmap_insert(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("rbmap_insert");

    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("nat_key", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let mut map = LeanRBMap::<LeanNat, LeanNat>::empty(lean)?;
                    for i in 0..size {
                        let key = LeanNat::from_usize(lean, i)?;
                        let val = LeanNat::from_usize(lean, i * 2)?;
                        map = map.insert(lean, key, val)?;
                    }
                    black_box(map);
                    Ok(())
                })
                .unwrap();
            });
        });

        group.bench_with_input(BenchmarkId::new("string_key", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let mut map = LeanRBMap::<LeanString, LeanNat>::empty(lean)?;
                    for i in 0..size {
                        let key = LeanString::mk(lean, &format!("key_{}", i))?;
                        let val = LeanNat::from_usize(lean, i)?;
                        map = map.insert(lean, key, val)?;
                    }
                    black_box(map);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

#[cfg(all(feature = "runtime-tests", lean_4_22))]
fn bench_rbmap_lookup(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("rbmap_lookup");

    for size in [10, 100, 1000].iter() {
        let map = leo3::with_lean(|lean| -> LeanResult<_> {
            let mut map = LeanRBMap::<LeanNat, LeanNat>::empty(lean)?;
            for i in 0..*size {
                let key = LeanNat::from_usize(lean, i)?;
                let val = LeanNat::from_usize(lean, i * 2)?;
                map = map.insert(lean, key, val)?;
            }
            Ok(map.unbind())
        })
        .unwrap();

        group.bench_with_input(BenchmarkId::new("find_hit", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let bound = map.bind(lean);
                    let key = LeanNat::from_usize(lean, black_box(size / 2))?;
                    let result = bound.find(lean, &key)?;
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });

        group.bench_with_input(BenchmarkId::new("find_miss", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let bound = map.bind(lean);
                    let key = LeanNat::from_usize(lean, black_box(size + 1))?;
                    let result = bound.find(lean, &key)?;
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

#[cfg(all(feature = "runtime-tests", lean_4_22))]
fn bench_rbmap_remove(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("rbmap_remove");

    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("erase", size), size, |b, &size| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let mut map = LeanRBMap::<LeanNat, LeanNat>::empty(lean)?;
                    for i in 0..size {
                        let key = LeanNat::from_usize(lean, i)?;
                        let val = LeanNat::from_usize(lean, i)?;
                        map = map.insert(lean, key, val)?;
                    }
                    for i in 0..size {
                        let key = LeanNat::from_usize(lean, i)?;
                        map = map.erase(lean, &key)?;
                    }
                    black_box(map);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

#[cfg(all(feature = "runtime-tests", lean_4_22))]
criterion_group!(
    benches,
    bench_hashmap_insert,
    bench_hashmap_lookup,
    bench_hashmap_remove,
    bench_hashset_ops,
    bench_rbmap_insert,
    bench_rbmap_lookup,
    bench_rbmap_remove,
);
#[cfg(all(feature = "runtime-tests", lean_4_22))]
criterion_main!(benches);
