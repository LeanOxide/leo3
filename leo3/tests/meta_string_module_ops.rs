//! LeanString, dynamic module loading, and Tokio bridge tests for Leo3
//!
//! Wave-3 coverage tests for three low-coverage public surfaces:
//!
//! - `leo3::types::LeanString` (leo3/src/types/string.rs): the full public
//!   surface — `mk`, `singleton`, zero-copy `as_str` / `cstr` views (embedded
//!   NUL and multi-byte payloads), `length`/`utf8ByteSize`/`isEmpty`,
//!   `push`/`pushn`/`append`, `eq`/`decEq`/`le`/`lt`,
//!   `startsWith`/`endsWith`/`isPrefixOf`/`contains`, `trim`/`trimLeft`/
//!   `trimRight`, `toUpper`/`toLower`, `capitalize`/`decapitalize`,
//!   `front`/`back`, `extract`, `get_char`, `next_pos`/`prev_pos`, `hash`,
//!   `Debug`, and `String`/`&str` conversion round-trips.
//! - `leo3::module` (leo3/src/module.rs): `LeanModule::load`, `get_function`
//!   (and the private `LeanFunction::lookup` it drives), `name()`/`arity()`,
//!   and `LeanFunction::call0`..`call8` success paths against the compiled
//!   fixture shared libraries.
//! - `leo3::tokio_bridge` (leo3/src/tokio_bridge.rs): `LeanTask::spawn_on_tokio`,
//!   `TaskHandle::into_tokio_future`, and `lean_block_in_place`.
//!
//! The string section needs only `runtime-tests`; the module section is gated
//! on `macros` + `module-loading`; the Tokio section is gated on `tokio`.

#![cfg(feature = "runtime-tests")]

use leo3::prelude::*;

// ============================================================================
// LeanString surface (leo3/src/types/string.rs)
// ============================================================================

/// `mk`, zero-copy `as_str`/`cstr` views, and the length family.
#[test]
fn test_string_mk_views_and_length() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "héllo 世界")?;

        // as_str is a zero-copy, length-aware view; cstr aliases it.
        assert_eq!(LeanString::as_str(&s)?, "héllo 世界");
        assert_eq!(LeanString::cstr(&s)?, "héllo 世界");
        assert_eq!(LeanString::as_str(&s)?, LeanString::cstr(&s)?);

        // length counts characters; utf8ByteSize counts payload bytes + NUL.
        assert_eq!(LeanString::length(&s), 8);
        assert_eq!(LeanString::utf8ByteSize(&s), 14);
        assert!(!LeanString::isEmpty(&s));

        // ASCII-only string.
        let ascii = LeanString::mk(lean, "abc")?;
        assert_eq!(LeanString::as_str(&ascii)?, "abc");
        assert_eq!(LeanString::length(&ascii), 3);
        assert_eq!(LeanString::utf8ByteSize(&ascii), 4);

        // Empty string.
        let empty = LeanString::mk(lean, "")?;
        assert_eq!(LeanString::as_str(&empty)?, "");
        assert_eq!(LeanString::length(&empty), 0);
        assert_eq!(LeanString::utf8ByteSize(&empty), 1);
        assert!(LeanString::isEmpty(&empty));

        Ok(())
    });

    assert!(result.is_ok());
}

/// Length-aware round trips: embedded NUL bytes and multi-byte payloads
/// survive construction and both extraction views unchanged.
#[test]
fn test_string_embedded_nul_and_multibyte_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "a\0b")?;
        assert_eq!(LeanString::as_str(&s)?, "a\0b");
        assert_eq!(LeanString::cstr(&s)?, "a\0b");
        assert_eq!(LeanString::length(&s), 3);
        assert_eq!(LeanString::utf8ByteSize(&s), 4);

        // A string that is entirely the NUL byte.
        let nul = LeanString::mk(lean, "\0")?;
        assert_eq!(LeanString::cstr(&nul)?, "\0");
        assert_eq!(LeanString::length(&nul), 1);

        // Mixed multi-byte + NUL payload.
        let mix = LeanString::mk(lean, "é\0世")?;
        assert_eq!(LeanString::as_str(&mix)?, "é\0世");
        assert_eq!(LeanString::length(&mix), 3);
        assert_eq!(LeanString::utf8ByteSize(&mix), 1 + 2 + 3 + 1); // payload + NUL

        Ok(())
    });

    assert!(result.is_ok());
}

/// `singleton`, `push`, and `pushn`.
#[test]
fn test_string_singleton_push_pushn() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanString::singleton(lean, 'A')?;
        assert_eq!(LeanString::cstr(&a)?, "A");
        assert_eq!(LeanString::length(&a), 1);

        // Multi-byte singleton: 世 encodes as 3 UTF-8 bytes.
        let uni = LeanString::singleton(lean, '世')?;
        assert_eq!(LeanString::cstr(&uni)?, "世");
        assert_eq!(LeanString::utf8ByteSize(&uni), 4);

        let s = LeanString::mk(lean, "Hello")?;
        let s = LeanString::push(s, '!')?;
        let s = LeanString::push(s, '界')?;
        assert_eq!(LeanString::cstr(&s)?, "Hello!界");
        assert_eq!(LeanString::length(&s), 7);

        let s = LeanString::pushn(s, 'x', 3)?;
        assert_eq!(LeanString::cstr(&s)?, "Hello!界xxx");

        // pushn with count 0 is the identity.
        let s = LeanString::pushn(s, 'y', 0)?;
        assert_eq!(LeanString::cstr(&s)?, "Hello!界xxx");

        Ok(())
    });

    assert!(result.is_ok());
}

/// `append`, `eq`/`decEq`, and `le`/`lt`.
#[test]
fn test_string_append_and_ordering() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s1 = LeanString::mk(lean, "héllo ")?;
        let s2 = LeanString::mk(lean, "世界")?;
        let joined = LeanString::append(s1, &s2)?;
        assert_eq!(LeanString::cstr(&joined)?, "héllo 世界");
        assert_eq!(LeanString::length(&joined), 8);

        // Appending the empty string is the identity.
        let base = LeanString::mk(lean, "abc")?;
        let empty = LeanString::mk(lean, "")?;
        let joined = LeanString::append(base, &empty)?;
        assert_eq!(LeanString::cstr(&joined)?, "abc");

        let a = LeanString::mk(lean, "hello")?;
        let b = LeanString::mk(lean, "hello")?;
        let c = LeanString::mk(lean, "Hello")?;

        assert!(LeanString::eq(&a, &b));
        assert!(!LeanString::eq(&a, &c));
        assert!(LeanString::decEq(&a, &b));
        assert!(!LeanString::decEq(&a, &c));

        let apple = LeanString::mk(lean, "apple")?;
        let banana = LeanString::mk(lean, "banana")?;
        assert!(LeanString::lt(&apple, &banana));
        assert!(!LeanString::lt(&banana, &apple));
        assert!(!LeanString::lt(&apple, &apple));
        assert!(LeanString::le(&apple, &banana));
        assert!(LeanString::le(&apple, &apple));
        assert!(!LeanString::le(&banana, &apple));

        // The empty string sorts before every non-empty string.
        assert!(LeanString::lt(&empty, &apple));

        Ok(())
    });

    assert!(result.is_ok());
}

/// `startsWith`/`endsWith`/`isPrefixOf` and `contains`.
#[test]
fn test_string_prefix_suffix_contains() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "Hello, World!")?;
        let prefix = LeanString::mk(lean, "Hello")?;
        let suffix = LeanString::mk(lean, "World!")?;
        let other = LeanString::mk(lean, "World")?;

        assert!(LeanString::startsWith(&s, &prefix));
        assert!(!LeanString::startsWith(&s, &other));
        assert!(LeanString::endsWith(&s, &suffix));
        assert!(!LeanString::endsWith(&s, &other));
        assert!(LeanString::isPrefixOf(&prefix, &s));
        assert!(!LeanString::isPrefixOf(&other, &s));

        assert!(LeanString::contains(&s, 'o'));
        assert!(LeanString::contains(&s, '!'));
        assert!(!LeanString::contains(&s, 'z'));

        // Unicode payloads.
        let u = LeanString::mk(lean, "café 世界")?;
        assert!(LeanString::startsWith(&u, &LeanString::mk(lean, "café")?));
        assert!(LeanString::endsWith(&u, &LeanString::mk(lean, "世界")?));
        assert!(LeanString::contains(&u, '世'));
        assert!(!LeanString::contains(&u, 'x'));

        Ok(())
    });

    assert!(result.is_ok());
}

/// `trim`/`trimLeft`/`trimRight`, `toUpper`/`toLower`, and
/// `capitalize`/`decapitalize`.
#[test]
fn test_string_case_and_trim() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "  \t hello \n\r ")?;
        assert_eq!(LeanString::cstr(&LeanString::trim(s)?)?, "hello");

        let s = LeanString::mk(lean, " \t left \n")?;
        assert_eq!(LeanString::cstr(&LeanString::trimLeft(s)?)?, "left \n");

        let s = LeanString::mk(lean, " \t right \n")?;
        assert_eq!(LeanString::cstr(&LeanString::trimRight(s)?)?, " \t right");

        // Whitespace-only input trims to empty.
        let s = LeanString::mk(lean, "   \t  ")?;
        let t = LeanString::trim(s)?;
        assert_eq!(LeanString::cstr(&t)?, "");
        assert!(LeanString::isEmpty(&t));

        let s = LeanString::mk(lean, "héllo wörld")?;
        assert_eq!(LeanString::cstr(&LeanString::toUpper(s)?)?, "HÉLLO WÖRLD");

        let s = LeanString::mk(lean, "HÉLLO WÖRLD")?;
        assert_eq!(LeanString::cstr(&LeanString::toLower(s)?)?, "héllo wörld");

        let s = LeanString::mk(lean, "hello world")?;
        assert_eq!(
            LeanString::cstr(&LeanString::capitalize(s)?)?,
            "Hello world"
        );

        let s = LeanString::mk(lean, "HELLO")?;
        assert_eq!(LeanString::cstr(&LeanString::decapitalize(s)?)?, "hELLO");

        // Unicode first char.
        let s = LeanString::mk(lean, "héllo")?;
        assert_eq!(LeanString::cstr(&LeanString::capitalize(s)?)?, "Héllo");

        // Empty string stays empty through case/trim ops.
        let empty = LeanString::mk(lean, "")?;
        assert_eq!(LeanString::cstr(&LeanString::capitalize(empty)?)?, "");

        Ok(())
    });

    assert!(result.is_ok());
}

/// `front`/`back`, `extract`, `get_char`, and `next_pos`/`prev_pos`.
#[test]
fn test_string_char_position_ops() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "abc")?;
        assert_eq!(LeanString::front(&s), Some('a'));
        assert_eq!(LeanString::back(&s), Some('c'));

        let u = LeanString::mk(lean, "世界")?;
        assert_eq!(LeanString::front(&u), Some('世'));
        assert_eq!(LeanString::back(&u), Some('界'));

        let empty = LeanString::mk(lean, "")?;
        assert_eq!(LeanString::front(&empty), None);
        assert_eq!(LeanString::back(&empty), None);

        // extract works on byte positions.
        let s = LeanString::mk(lean, "Hello, World!")?;
        assert_eq!(
            LeanString::cstr(&LeanString::extract(lean, &s, 0, 5)?)?,
            "Hello"
        );
        assert_eq!(
            LeanString::cstr(&LeanString::extract(lean, &s, 7, 12)?)?,
            "World"
        );

        // get_char at valid byte positions (ASCII).
        assert_eq!(LeanString::get_char(&s, 0), 'H' as u32);
        assert_eq!(LeanString::get_char(&s, 1), 'e' as u32);

        // Mixed ASCII + multi-byte: 界 occupies bytes 1..=3.
        let s = LeanString::mk(lean, "a界c")?;
        assert_eq!(LeanString::get_char(&s, 0), 'a' as u32);
        assert_eq!(LeanString::get_char(&s, 1), '界' as u32);
        assert_eq!(LeanString::get_char(&s, 4), 'c' as u32);

        // next_pos advances by the char's UTF-8 width.
        assert_eq!(LeanString::next_pos(&s, 0), 1);
        assert_eq!(LeanString::next_pos(&s, 1), 4); // skips the 3-byte 界
        assert_eq!(LeanString::next_pos(&s, 4), 5);

        // prev_pos walks back to the previous char start.
        assert_eq!(LeanString::prev_pos(&s, 4), 1);
        assert_eq!(LeanString::prev_pos(&s, 1), 0);

        Ok(())
    });

    assert!(result.is_ok());
}

/// `hash` is deterministic and equal for equal strings.
#[test]
fn test_string_hash() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanString::mk(lean, "hello")?;
        let b = LeanString::mk(lean, "hello")?;
        let c = LeanString::mk(lean, "world")?;

        assert_eq!(LeanString::hash(&a), LeanString::hash(&b));
        assert_eq!(LeanString::hash(&a), LeanString::hash(&a));

        let empty = LeanString::mk(lean, "")?;
        assert_eq!(LeanString::hash(&empty), LeanString::hash(&empty));

        // Hashing runs to completion on other payloads.
        let _ = LeanString::hash(&c);
        let _ = LeanString::hash(&u64::MAX.to_string().as_str().into_lean(lean)?);

        Ok(())
    });

    assert!(result.is_ok());
}

/// `Debug` formatting and `String`/`&str` conversion round-trips.
#[test]
fn test_string_debug_and_conversions() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "Hello")?;
        assert_eq!(format!("{:?}", s), "LeanString(\"Hello\")");

        let empty = LeanString::mk(lean, "")?;
        assert_eq!(format!("{:?}", empty), "LeanString(\"\")");

        // Owned String -> LeanString.
        let s = String::from("owned").into_lean(lean)?;
        assert_eq!(LeanString::cstr(&s)?, "owned");

        // &str -> LeanString.
        let s = "borrowed".into_lean(lean)?;
        assert_eq!(LeanString::cstr(&s)?, "borrowed");

        // Round trip with a mixed multi-byte + embedded NUL payload.
        let original = String::from("mix\0\u{1F600}世界");
        let ls = original.clone().into_lean(lean)?;
        let back: String = String::from_lean(&ls)?;
        assert_eq!(back, original);

        let back: &str = <&str>::from_lean(&ls)?;
        assert_eq!(back, original);

        Ok(())
    });

    assert!(result.is_ok());
}

// ============================================================================
// Dynamic module loading (leo3/src/module.rs)
// ============================================================================

#[cfg(all(feature = "macros", feature = "module-loading"))]
mod module_loading_ops {
    use leo3::module::LeanModule;
    use leo3::LeanError;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use std::sync::LazyLock;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture_manifest(fixture: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(fixture)
            .join("Cargo.toml")
    }

    fn unique_target_dir() -> PathBuf {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be after unix epoch")
            .as_millis();
        std::env::temp_dir().join(format!(
            "leo3-meta-module-fixtures-{}-{}",
            std::process::id(),
            millis
        ))
    }

    fn dylib_name(crate_name: &str) -> String {
        #[cfg(target_os = "linux")]
        {
            format!("lib{crate_name}.so")
        }
        #[cfg(target_os = "macos")]
        {
            format!("lib{crate_name}.dylib")
        }
        #[cfg(target_os = "windows")]
        {
            format!("{crate_name}.dll")
        }
    }

    fn address_sanitizer_enabled() -> bool {
        std::env::var("RUSTFLAGS")
            .ok()
            .is_some_and(|flags| flags.contains("sanitizer=address"))
    }

    fn build_fixture(target_dir: &Path, fixture: &str) -> PathBuf {
        let output = Command::new("cargo")
            .arg("build")
            .arg("--quiet")
            .arg("--manifest-path")
            .arg(fixture_manifest(fixture))
            .env("CARGO_TARGET_DIR", target_dir)
            .output()
            .expect("fixture cargo build should start");

        assert!(
            output.status.success(),
            "fixture cargo build failed: {}
stderr:
{}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );

        target_dir.join("debug").join(dylib_name(fixture))
    }

    struct FixturePaths {
        runtime: PathBuf,
        multi: PathBuf,
    }

    /// Build both fixtures at most once per test process and share the
    /// artifacts. The generated `initialize_*` functions only return
    /// `IO.ok ()`, so loading the same dylib from several tests in one
    /// process is side-effect free.
    static FIXTURES: LazyLock<FixturePaths> = LazyLock::new(|| {
        let target_dir = unique_target_dir();
        let runtime = build_fixture(&target_dir, "leanmodule_runtime_fixture");
        let multi = build_fixture(&target_dir, "leanmodule_multi_arity_fixture");
        FixturePaths { runtime, multi }
    });

    fn fixtures() -> &'static FixturePaths {
        &FIXTURES
    }

    /// `LeanModule::load` + `name()`, plus `get_function` metadata
    /// (`LeanFunction::name()` / `arity()`) for both fixtures.
    #[test]
    fn test_module_load_and_function_metadata() {
        if address_sanitizer_enabled() {
            return;
        }
        leo3::prepare_freethreaded_lean();

        let module = LeanModule::load(&fixtures().multi, "MultiArityModule")
            .unwrap_or_else(|err| panic!("failed to load fixture: {err}"));
        assert_eq!(module.name(), "MultiArityModule");

        let f = module
            .get_function("ma_two", 2)
            .expect("ma_two should be exported");
        assert_eq!(f.name(), "ma_two");
        assert_eq!(f.arity(), 2);

        let runtime_module = LeanModule::load(&fixtures().runtime, "FixtureModule")
            .unwrap_or_else(|err| panic!("failed to load fixture: {err}"));
        assert_eq!(runtime_module.name(), "FixtureModule");
        let add = runtime_module
            .get_function("fixture_add", 2)
            .expect("fixture_add should be exported");
        assert_eq!(add.name(), "fixture_add");
        assert_eq!(add.arity(), 2);
    }

    /// `LeanFunction::call0`..`call8` success paths.
    ///
    /// Each fixture export `ma_<k>` takes `k` `u64` arguments and returns
    /// `10 * k + sum(args)`; calling with `1..=k` yields a value the test
    /// recomputes independently.
    #[test]
    fn test_lean_function_call0_through_call8() {
        if address_sanitizer_enabled() {
            return;
        }
        leo3::prepare_freethreaded_lean();

        let module = LeanModule::load(&fixtures().multi, "MultiArityModule")
            .unwrap_or_else(|err| panic!("failed to load fixture: {err}"));

        leo3::with_lean(|lean| {
            let f0 = module.get_function("ma_zero", 0)?;
            let r0: u64 = f0.call0(lean)?;
            assert_eq!(r0, 0);

            let f1 = module.get_function("ma_one", 1)?;
            let r1: u64 = f1.call1(lean, 1_u64)?;
            assert_eq!(r1, 11);

            let f2 = module.get_function("ma_two", 2)?;
            let r2: u64 = f2.call2(lean, 1_u64, 2_u64)?;
            assert_eq!(r2, 10 * 2 + 1 + 2);

            let f3 = module.get_function("ma_three", 3)?;
            let r3: u64 = f3.call3(lean, 1_u64, 2_u64, 3_u64)?;
            assert_eq!(r3, 10 * 3 + 1 + 2 + 3);

            let f4 = module.get_function("ma_four", 4)?;
            let r4: u64 = f4.call4(lean, 1_u64, 2_u64, 3_u64, 4_u64)?;
            assert_eq!(r4, 10 * 4 + 1 + 2 + 3 + 4);

            let f5 = module.get_function("ma_five", 5)?;
            let r5: u64 = f5.call5(lean, 1_u64, 2_u64, 3_u64, 4_u64, 5_u64)?;
            assert_eq!(r5, 10 * 5 + 1 + 2 + 3 + 4 + 5);

            let f6 = module.get_function("ma_six", 6)?;
            let r6: u64 = f6.call6(lean, 1_u64, 2_u64, 3_u64, 4_u64, 5_u64, 6_u64)?;
            assert_eq!(r6, 10 * 6 + 1 + 2 + 3 + 4 + 5 + 6);

            let f7 = module.get_function("ma_seven", 7)?;
            let r7: u64 = f7.call7(lean, 1_u64, 2_u64, 3_u64, 4_u64, 5_u64, 6_u64, 7_u64)?;
            assert_eq!(r7, 10 * 7 + 1 + 2 + 3 + 4 + 5 + 6 + 7);

            let f8 = module.get_function("ma_eight", 8)?;
            let r8: u64 = f8.call8(lean, 1_u64, 2_u64, 3_u64, 4_u64, 5_u64, 6_u64, 7_u64, 8_u64)?;
            assert_eq!(r8, 10 * 8 + 1 + 2 + 3 + 4 + 5 + 6 + 7 + 8);

            Ok::<_, LeanError>(())
        })
        .unwrap();
    }

    /// Boxed `String`/scalar round trips through `call2` on both fixtures.
    #[test]
    fn test_lean_function_string_roundtrip() {
        if address_sanitizer_enabled() {
            return;
        }
        leo3::prepare_freethreaded_lean();

        let module = LeanModule::load(&fixtures().multi, "MultiArityModule")
            .unwrap_or_else(|err| panic!("failed to load fixture: {err}"));
        let runtime_module = LeanModule::load(&fixtures().runtime, "FixtureModule")
            .unwrap_or_else(|err| panic!("failed to load fixture: {err}"));

        leo3::with_lean(|lean| {
            let greet = module.get_function("ma_greet", 2)?;
            let msg: String = greet.call2(lean, String::from("probe"), 3_u64)?;
            assert_eq!(msg, "probe x3");

            let banner = runtime_module.get_function("fixture_banner", 2)?;
            let message: String = banner.call2(lean, String::from("orbiter"), 7_i32)?;
            assert_eq!(message, "orbiter has 7 ticks");

            Ok::<_, LeanError>(())
        })
        .unwrap();
    }

    /// The canonical fixture_add call from test_leanmodule_loading.rs.
    #[test]
    fn test_fixture_add_via_call2() {
        if address_sanitizer_enabled() {
            return;
        }
        leo3::prepare_freethreaded_lean();

        let module = LeanModule::load(&fixtures().runtime, "FixtureModule")
            .unwrap_or_else(|err| panic!("failed to load fixture: {err}"));

        leo3::with_lean(|lean| {
            let add = module
                .get_function("fixture_add", 2)
                .expect("fixture_add should be exported");
            let sum: u64 = add.call2(lean, 20_u64, 22_u64)?;
            assert_eq!(sum, 42);

            Ok::<_, LeanError>(())
        })
        .unwrap();
    }

    /// `check_arity` rejects a call with the wrong number of arguments.
    #[test]
    fn test_lean_function_arity_mismatch_is_error() {
        if address_sanitizer_enabled() {
            return;
        }
        leo3::prepare_freethreaded_lean();

        let module = LeanModule::load(&fixtures().multi, "MultiArityModule")
            .unwrap_or_else(|err| panic!("failed to load fixture: {err}"));

        leo3::with_lean(|lean| {
            // ma_two has arity 2; calling it through call1 must fail fast
            // with an arity-mismatch error before any FFI call.
            let f = module.get_function("ma_two", 2)?;
            let err = f.call1::<u64, u64>(lean, 1_u64).unwrap_err();
            let message = err.to_string();
            assert!(
                message.contains("expects 2 argument(s)"),
                "unexpected error: {message}"
            );

            Ok::<_, LeanError>(())
        })
        .unwrap();
    }
}

// ============================================================================
// Tokio bridge (leo3/src/tokio_bridge.rs)
// ============================================================================

#[cfg(feature = "tokio")]
mod tokio_bridge_ops {
    use leo3::closure::LeanClosure;
    use leo3::instance::LeanAny;
    use leo3::task::LeanTask;
    use leo3::tokio_bridge::lean_block_in_place;

    unsafe extern "C" fn make_nat_10(
        _world: *mut leo3::ffi::lean_object,
    ) -> *mut leo3::ffi::lean_object {
        leo3::ffi::inline::lean_box(10)
    }

    unsafe extern "C" fn slow_nat_50(
        _world: *mut leo3::ffi::lean_object,
    ) -> *mut leo3::ffi::lean_object {
        std::thread::sleep(std::time::Duration::from_millis(50));
        leo3::ffi::inline::lean_box(50)
    }

    /// `LeanTask::spawn_on_tokio`: spawn a Lean task, await the returned
    /// `tokio::task::JoinHandle`, and read the unbound result.
    #[tokio::test]
    async fn test_spawn_on_tokio() {
        leo3::prepare_freethreaded_lean();

        let result = leo3::with_lean(|lean| {
            let closure = LeanClosure::from_fn1(lean, make_nat_10).unwrap();
            LeanTask::spawn_on_tokio(closure)
        });

        let unbound = result.await.unwrap();
        let n = unsafe { leo3::ffi::inline::lean_unbox(unbound.as_ptr()) };
        assert_eq!(n, 10);
    }

    /// `TaskHandle::into_tokio_future`: convert a handle into a Tokio future
    /// and await the Lean task's completion.
    #[tokio::test]
    async fn test_task_handle_into_tokio_future() {
        leo3::prepare_freethreaded_lean();

        let handle = leo3::with_lean(|lean| {
            let closure = LeanClosure::from_fn1(lean, slow_nat_50).unwrap();
            LeanTask::<LeanAny>::spawn(closure).into_handle()
        });

        let unbound = handle.into_tokio_future().await;
        let n = unsafe { leo3::ffi::inline::lean_unbox(unbound.as_ptr()) };
        assert_eq!(n, 50);
    }

    /// `lean_block_in_place` runs a synchronous closure inside a Tokio
    /// multi-thread runtime without blocking its worker threads.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_lean_block_in_place() {
        leo3::prepare_freethreaded_lean();

        // Plain computation through the wrapper.
        let value = lean_block_in_place(|| 2 + 2);
        assert_eq!(value, 4);

        // The intended use: a blocking Lean wait (TaskHandle::get_unbound)
        // delegated to Tokio's blocking pool.
        let handle = leo3::with_lean(|lean| {
            let closure = LeanClosure::from_fn1(lean, slow_nat_50).unwrap();
            LeanTask::<LeanAny>::spawn(closure).into_handle()
        });
        let unbound = lean_block_in_place(move || handle.get_unbound());
        let n = unsafe { leo3::ffi::inline::lean_unbox(unbound.as_ptr()) };
        assert_eq!(n, 50);
    }
}
