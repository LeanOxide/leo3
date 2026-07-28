use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use leo3::conversion::FromLean;
use leo3::prelude::*;
use std::hint::black_box;

fn bench_rust_to_lean_string(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("rust_to_lean_string");

    for len in [8, 64, 256, 1024, 4096].iter() {
        let s: String = "a".repeat(*len);

        group.bench_with_input(BenchmarkId::new("mk", len), len, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let result = LeanString::mk(lean, black_box(&s))?;
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });

        group.bench_with_input(BenchmarkId::new("into_lean", len), len, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let owned = s.clone();
                    let result = leo3::conversion::IntoLean::into_lean(black_box(owned), lean)?;
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

fn bench_lean_to_rust_string(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("lean_to_rust_string");

    for len in [8, 64, 256, 1024, 4096].iter() {
        let s: String = "b".repeat(*len);

        let lean_str =
            leo3::with_lean(|lean| -> LeanResult<_> { Ok(LeanString::mk(lean, &s)?.unbind()) })
                .unwrap();

        group.bench_with_input(BenchmarkId::new("cstr_borrow", len), len, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let bound = lean_str.bind(lean);
                    let result = LeanString::cstr(&black_box(bound))?;
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });

        group.bench_with_input(BenchmarkId::new("from_lean_owned", len), len, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let bound = lean_str.bind(lean);
                    let result: String = FromLean::from_lean(&black_box(bound))?;
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

fn bench_string_roundtrip(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("string_roundtrip");

    for len in [8, 64, 256, 1024].iter() {
        let s: String = "c".repeat(*len);

        group.bench_with_input(BenchmarkId::new("mk_then_cstr", len), len, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let lean_s = LeanString::mk(lean, black_box(&s))?;
                    let result = LeanString::cstr(&lean_s)?;
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });

        group.bench_with_input(BenchmarkId::new("into_then_from", len), len, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let owned = s.clone();
                    let lean_s: LeanBound<LeanString> =
                        leo3::conversion::IntoLean::into_lean(black_box(owned), lean)?;
                    let result: String = FromLean::from_lean(&lean_s)?;
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });
    }

    group.finish();
}

fn bench_string_unicode(c: &mut Criterion) {
    leo3::prepare_freethreaded_lean();

    let mut group = c.benchmark_group("string_unicode");

    let mixed_long = "αβγδε".repeat(100);
    let cases: Vec<(&str, &str)> = vec![
        ("ascii_short", "hello"),
        ("cjk_medium", "你好世界こんにちは안녕하세요"),
        ("emoji_mix", "Hello 🌍 World 🦀 Rust ⚡"),
        ("mixed_long", &mixed_long),
    ];

    for (name, s) in &cases {
        group.bench_with_input(BenchmarkId::new("mk", name), s, |b, s| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let result = LeanString::mk(lean, black_box(s))?;
                    black_box(result);
                    Ok(())
                })
                .unwrap();
            });
        });

        let lean_str =
            leo3::with_lean(|lean| -> LeanResult<_> { Ok(LeanString::mk(lean, s)?.unbind()) })
                .unwrap();

        group.bench_with_input(BenchmarkId::new("cstr", name), s, |b, _| {
            b.iter(|| {
                leo3::with_lean(|lean| -> LeanResult<()> {
                    let bound = lean_str.bind(lean);
                    let result = LeanString::cstr(&black_box(bound))?;
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
    bench_rust_to_lean_string,
    bench_lean_to_rust_string,
    bench_string_roundtrip,
    bench_string_unicode,
);
criterion_main!(benches);
