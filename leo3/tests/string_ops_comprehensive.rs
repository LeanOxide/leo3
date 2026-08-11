//! Comprehensive LeanString tests for Leo3
//!
//! Covers the public `LeanString` surface not already exercised by
//! `string_ops.rs`: mk (empty / unicode / embedded NUL), singleton, pushn,
//! append, eq/decEq/le/lt, startsWith/endsWith/isPrefixOf, contains,
//! trim/trimLeft/trimRight, toUpper/toLower, length/utf8ByteSize/isEmpty,
//! get_char (valid positions and error paths), next_pos/prev_pos, front/back,
//! capitalize/decapitalize, hash, and String/&str IntoLean/FromLean
//! round-trips (including embedded NUL and multi-byte payloads).

#![cfg(feature = "runtime-tests")]

use leo3::prelude::*;

#[test]
fn test_mk_cstr_as_str_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "Hello, Lean4!")?;

        assert_eq!(LeanString::cstr(&s)?, "Hello, Lean4!");
        assert_eq!(LeanString::as_str(&s)?, "Hello, Lean4!");

        // as_str and cstr are interchangeable views over the same payload.
        assert_eq!(LeanString::as_str(&s)?, LeanString::cstr(&s)?);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_mk_empty_string() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "")?;

        assert_eq!(LeanString::cstr(&s)?, "");
        assert_eq!(LeanString::length(&s), 0);
        assert_eq!(LeanString::utf8ByteSize(&s), 1); // size includes NUL terminator
        assert!(LeanString::isEmpty(&s));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_mk_unicode_string() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // "héllo 世界": h(1) é(2) l(1) l(1) o(1) ' '(1) 世(3) 界(3) = 13 payload bytes
        let s = LeanString::mk(lean, "héllo 世界")?;

        assert_eq!(LeanString::cstr(&s)?, "héllo 世界");
        assert_eq!(LeanString::length(&s), 8);
        assert_eq!(LeanString::utf8ByteSize(&s), 14); // payload + NUL
        assert!(!LeanString::isEmpty(&s));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_mk_embedded_nul() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Embedded NUL bytes are preserved by the length-aware constructor.
        let s = LeanString::mk(lean, "a\0b")?;

        assert_eq!(LeanString::cstr(&s)?, "a\0b");
        assert_eq!(LeanString::as_str(&s)?, "a\0b");
        assert_eq!(LeanString::length(&s), 3);
        assert_eq!(LeanString::utf8ByteSize(&s), 4);

        let s2 = LeanString::mk(lean, "\0")?;
        assert_eq!(LeanString::cstr(&s2)?, "\0");
        assert_eq!(LeanString::length(&s2), 1);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_singleton() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanString::singleton(lean, 'A')?;
        assert_eq!(LeanString::cstr(&a)?, "A");
        assert_eq!(LeanString::length(&a), 1);

        // Multi-byte character: 世 encodes as 3 UTF-8 bytes.
        let uni = LeanString::singleton(lean, '世')?;
        assert_eq!(LeanString::cstr(&uni)?, "世");
        assert_eq!(LeanString::length(&uni), 1);
        assert_eq!(LeanString::utf8ByteSize(&uni), 4); // 3 payload bytes + NUL

        // NUL character singleton.
        let nul = LeanString::singleton(lean, '\0')?;
        assert_eq!(LeanString::cstr(&nul)?, "\0");
        assert_eq!(LeanString::length(&nul), 1);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_push() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "Hello")?;
        let s = LeanString::push(s, '!')?;

        assert_eq!(LeanString::cstr(&s)?, "Hello!");
        assert_eq!(LeanString::length(&s), 6);

        // Pushing a multi-byte char.
        let s = LeanString::push(s, '界')?;
        assert_eq!(LeanString::cstr(&s)?, "Hello!界");
        assert_eq!(LeanString::length(&s), 7);

        // Pushing onto an empty string.
        let empty = LeanString::mk(lean, "")?;
        let empty = LeanString::push(empty, 'x')?;
        assert_eq!(LeanString::cstr(&empty)?, "x");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_pushn() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "Hello")?;
        let s = LeanString::pushn(s, 'x', 3)?;
        assert_eq!(LeanString::cstr(&s)?, "Helloxxx");

        // pushn with count 0 is identity.
        let s = LeanString::pushn(s, 'y', 0)?;
        assert_eq!(LeanString::cstr(&s)?, "Helloxxx");

        // Multi-byte repetition from empty.
        let empty = LeanString::mk(lean, "")?;
        let empty = LeanString::pushn(empty, '界', 2)?;
        assert_eq!(LeanString::cstr(&empty)?, "界界");
        assert_eq!(LeanString::length(&empty), 2);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_append() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s1 = LeanString::mk(lean, "Hello, ")?;
        let s2 = LeanString::mk(lean, "World!")?;
        let joined = LeanString::append(s1, &s2)?;

        assert_eq!(LeanString::cstr(&joined)?, "Hello, World!");

        // Appending empty string is identity.
        let base = LeanString::mk(lean, "abc")?;
        let empty = LeanString::mk(lean, "")?;
        let joined = LeanString::append(base, &empty)?;
        assert_eq!(LeanString::cstr(&joined)?, "abc");

        // Appending unicode payloads.
        let a = LeanString::mk(lean, "héllo ")?;
        let b = LeanString::mk(lean, "世界")?;
        let joined = LeanString::append(a, &b)?;
        assert_eq!(LeanString::cstr(&joined)?, "héllo 世界");
        assert_eq!(LeanString::length(&joined), 8); // 6 + 2 chars

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_eq_and_dec_eq() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanString::mk(lean, "hello")?;
        let b = LeanString::mk(lean, "hello")?;
        let c = LeanString::mk(lean, "Hello")?; // different case
        let d = LeanString::mk(lean, "hello!")?; // different length

        assert!(LeanString::eq(&a, &b));
        assert!(!LeanString::eq(&a, &c));
        assert!(!LeanString::eq(&a, &d));

        assert!(LeanString::decEq(&a, &b));
        assert!(!LeanString::decEq(&a, &c));

        // Equal empty strings.
        let e1 = LeanString::mk(lean, "")?;
        let e2 = LeanString::mk(lean, "")?;
        assert!(LeanString::eq(&e1, &e2));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_le_and_lt() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let apple = LeanString::mk(lean, "apple")?;
        let banana = LeanString::mk(lean, "banana")?;

        assert!(LeanString::lt(&apple, &banana));
        assert!(!LeanString::lt(&banana, &apple));
        assert!(!LeanString::lt(&apple, &apple));

        // le: a <= b is true for a < b and a == b.
        assert!(LeanString::le(&apple, &banana));
        assert!(LeanString::le(&apple, &apple));
        assert!(!LeanString::le(&banana, &apple));

        // Prefix ordering: "abc" < "abcd".
        let ab = LeanString::mk(lean, "abc")?;
        let abcd = LeanString::mk(lean, "abcd")?;
        assert!(LeanString::lt(&ab, &abcd));
        assert!(LeanString::le(&ab, &abcd));

        // Empty string sorts before everything non-empty.
        let empty = LeanString::mk(lean, "")?;
        assert!(LeanString::lt(&empty, &apple));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_starts_ends_prefix() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "Hello, World!")?;
        let prefix = LeanString::mk(lean, "Hello")?;
        let suffix = LeanString::mk(lean, "World!")?;
        let not_prefix = LeanString::mk(lean, "World")?;
        let not_suffix = LeanString::mk(lean, "Hello")?;

        assert!(LeanString::startsWith(&s, &prefix));
        assert!(!LeanString::startsWith(&s, &not_prefix));

        assert!(LeanString::endsWith(&s, &suffix));
        assert!(!LeanString::endsWith(&s, &not_suffix));

        // isPrefixOf(prefix, s) == startsWith(s, prefix).
        assert!(LeanString::isPrefixOf(&prefix, &s));
        assert!(!LeanString::isPrefixOf(&not_prefix, &s));

        // Empty prefix/suffix always matches.
        let empty = LeanString::mk(lean, "")?;
        assert!(LeanString::startsWith(&s, &empty));
        assert!(LeanString::endsWith(&s, &empty));
        assert!(LeanString::isPrefixOf(&empty, &s));

        // Unicode payloads.
        let u = LeanString::mk(lean, "世界你好")?;
        let u_prefix = LeanString::mk(lean, "世界")?;
        assert!(LeanString::startsWith(&u, &u_prefix));
        assert!(!LeanString::startsWith(&u, &suffix));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_contains() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "Hello, World!")?;

        assert!(LeanString::contains(&s, 'o'));
        assert!(LeanString::contains(&s, '!'));
        assert!(!LeanString::contains(&s, 'z'));
        assert!(!LeanString::contains(&s, 'x'));

        let u = LeanString::mk(lean, "café 世界")?;
        assert!(LeanString::contains(&u, 'é'));
        assert!(LeanString::contains(&u, '世'));
        assert!(!LeanString::contains(&u, 'x'));

        let empty = LeanString::mk(lean, "")?;
        assert!(!LeanString::contains(&empty, 'a'));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_trim() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "  \t hello \n\r ")?;
        let t = LeanString::trim(s)?;
        assert_eq!(LeanString::cstr(&t)?, "hello");

        // trimLeft only removes leading whitespace.
        let s = LeanString::mk(lean, " \t left \n")?;
        let t = LeanString::trimLeft(s)?;
        assert_eq!(LeanString::cstr(&t)?, "left \n");

        // trimRight only removes trailing whitespace.
        let s = LeanString::mk(lean, " \t right \n")?;
        let t = LeanString::trimRight(s)?;
        assert_eq!(LeanString::cstr(&t)?, " \t right");

        // Whitespace-only string trims to empty.
        let s = LeanString::mk(lean, "   \t  ")?;
        let t = LeanString::trim(s)?;
        assert_eq!(LeanString::cstr(&t)?, "");
        assert!(LeanString::isEmpty(&t));

        // Unicode whitespace (ideographic space U+3000).
        let s = LeanString::mk(lean, "\u{3000}center\u{3000}")?;
        let t = LeanString::trim(s)?;
        assert_eq!(LeanString::cstr(&t)?, "center");

        // No-op trim.
        let s = LeanString::mk(lean, "plain")?;
        let t = LeanString::trim(s)?;
        assert_eq!(LeanString::cstr(&t)?, "plain");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_to_upper_lower() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "Hello, World!")?;
        let upper = LeanString::toUpper(s)?;
        assert_eq!(LeanString::cstr(&upper)?, "HELLO, WORLD!");

        let s = LeanString::mk(lean, "Hello, World!")?;
        let lower = LeanString::toLower(s)?;
        assert_eq!(LeanString::cstr(&lower)?, "hello, world!");

        // Unicode-aware case conversion.
        let s = LeanString::mk(lean, "héllo wörld")?;
        let upper = LeanString::toUpper(s)?;
        assert_eq!(LeanString::cstr(&upper)?, "HÉLLO WÖRLD");

        let s = LeanString::mk(lean, "HÉLLO WÖRLD")?;
        let lower = LeanString::toLower(s)?;
        assert_eq!(LeanString::cstr(&lower)?, "héllo wörld");

        // Empty string stays empty.
        let s = LeanString::mk(lean, "")?;
        let upper = LeanString::toUpper(s)?;
        assert_eq!(LeanString::cstr(&upper)?, "");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_length_byte_size_is_empty() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // ASCII: length == byte size - 1.
        let s = LeanString::mk(lean, "hello")?;
        assert_eq!(LeanString::length(&s), 5);
        assert_eq!(LeanString::utf8ByteSize(&s), 6);

        // Multi-byte: length counts chars, utf8ByteSize counts bytes + NUL.
        let s = LeanString::mk(lean, "aé世")?;
        assert_eq!(LeanString::length(&s), 3);
        assert_eq!(LeanString::utf8ByteSize(&s), 1 + 2 + 3 + 1); // payload + NUL

        let empty = LeanString::mk(lean, "")?;
        assert_eq!(LeanString::length(&empty), 0);
        assert_eq!(LeanString::utf8ByteSize(&empty), 1);
        assert!(LeanString::isEmpty(&empty));
        assert!(!LeanString::isEmpty(&s));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_get_char() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // ASCII-only: byte positions are also character starts.
        let s = LeanString::mk(lean, "abc")?;
        assert_eq!(LeanString::get_char(&s, 0), 'a' as u32);
        assert_eq!(LeanString::get_char(&s, 1), 'b' as u32);
        assert_eq!(LeanString::get_char(&s, 2), 'c' as u32);

        // Out-of-bounds returns the default char 'A' (65).
        assert_eq!(LeanString::get_char(&s, 3), 'A' as u32);
        assert_eq!(LeanString::get_char(&s, 100), 'A' as u32);

        // Mixed ASCII + multi-byte: 界 occupies bytes 1..=3.
        let s = LeanString::mk(lean, "a界c")?;
        assert_eq!(LeanString::get_char(&s, 0), 'a' as u32);
        assert_eq!(LeanString::get_char(&s, 1), '界' as u32);

        // Mid-UTF8 position (inside 界) is invalid -> default char.
        assert_eq!(LeanString::get_char(&s, 3), 'A' as u32);
        // Last payload byte is a valid position ('c'); OOB starts at 5.
        assert_eq!(LeanString::get_char(&s, 4), 'c' as u32);
        assert_eq!(LeanString::get_char(&s, 5), 'A' as u32);
        assert_eq!(LeanString::get_char(&s, 10), 'A' as u32);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_next_prev_pos() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "a界c")?;

        // next_pos advances by the char's UTF-8 width.
        assert_eq!(LeanString::next_pos(&s, 0), 1);
        assert_eq!(LeanString::next_pos(&s, 1), 4); // skips 3-byte 界
        assert_eq!(LeanString::next_pos(&s, 4), 5);

        // Out-of-bounds next returns i + 1.
        assert_eq!(LeanString::next_pos(&s, 5), 6);

        // prev_pos walks back to the previous char start.
        assert_eq!(LeanString::prev_pos(&s, 0), 0);
        assert_eq!(LeanString::prev_pos(&s, 1), 0);
        assert_eq!(LeanString::prev_pos(&s, 4), 1);

        // i > payload size returns i - 1.
        assert_eq!(LeanString::prev_pos(&s, 5), 4);

        // ASCII sanity: prev of position 3 in "abc" is 2, next is 4.
        let s2 = LeanString::mk(lean, "abc")?;
        assert_eq!(LeanString::next_pos(&s2, 2), 3);
        assert_eq!(LeanString::prev_pos(&s2, 3), 2);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_front_back() {
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

        // Single character: front == back.
        let one = LeanString::singleton(lean, 'x')?;
        assert_eq!(LeanString::front(&one), Some('x'));
        assert_eq!(LeanString::back(&one), Some('x'));

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_capitalize_decapitalize() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "hello world")?;
        let cap = LeanString::capitalize(s)?;
        assert_eq!(LeanString::cstr(&cap)?, "Hello world");

        let s = LeanString::mk(lean, "HELLO WORLD")?;
        let decap = LeanString::decapitalize(s)?;
        assert_eq!(LeanString::cstr(&decap)?, "hELLO WORLD");

        // Unicode first char.
        let s = LeanString::mk(lean, "héllo")?;
        let cap = LeanString::capitalize(s)?;
        assert_eq!(LeanString::cstr(&cap)?, "Héllo");

        // Empty string stays empty.
        let s = LeanString::mk(lean, "")?;
        let cap = LeanString::capitalize(s)?;
        assert_eq!(LeanString::cstr(&cap)?, "");
        let s = LeanString::mk(lean, "")?;
        let decap = LeanString::decapitalize(s)?;
        assert_eq!(LeanString::cstr(&decap)?, "");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_hash() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let a = LeanString::mk(lean, "hello")?;
        let b = LeanString::mk(lean, "hello")?;
        let c = LeanString::mk(lean, "world")?;

        // Equal strings hash equally; hashing is deterministic.
        assert_eq!(LeanString::hash(&a), LeanString::hash(&b));
        assert_eq!(LeanString::hash(&a), LeanString::hash(&a));

        let empty = LeanString::mk(lean, "")?;
        assert_eq!(LeanString::hash(&empty), LeanString::hash(&empty));

        // Hashing different contents is well-defined (no crash, u64 range).
        let _ = LeanString::hash(&c);
        let _ = LeanString::hash(&empty);

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_string_into_lean() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Owned String -> LeanString.
        let s = String::from("owned string").into_lean(lean)?;
        assert_eq!(LeanString::cstr(&s)?, "owned string");

        // &str -> LeanString.
        let s = "borrowed str".into_lean(lean)?;
        assert_eq!(LeanString::cstr(&s)?, "borrowed str");

        // Empty round trip.
        let s = String::new().into_lean(lean)?;
        assert_eq!(LeanString::cstr(&s)?, "");

        // Embedded NUL survives into_lean.
        let s = String::from("a\0b").into_lean(lean)?;
        assert_eq!(LeanString::cstr(&s)?, "a\0b");
        assert_eq!(LeanString::length(&s), 3);

        // Multi-byte payload survives into_lean.
        let s = String::from("héllo 世界").into_lean(lean)?;
        assert_eq!(LeanString::cstr(&s)?, "héllo 世界");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_string_from_lean() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let ls = LeanString::mk(lean, "back to rust")?;

        let owned: String = String::from_lean(&ls)?;
        assert_eq!(owned, "back to rust");

        let borrowed: &str = <&str>::from_lean(&ls)?;
        assert_eq!(borrowed, "back to rust");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_string_conversion_roundtrip() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        // Round trip with multi-byte + embedded NUL mixed payload.
        let original = String::from("mix\0\u{1F600}世界");
        let ls = original.clone().into_lean(lean)?;
        let back: String = String::from_lean(&ls)?;
        assert_eq!(back, original);

        // Round trip through &'l str.
        let ls2 = "borrowed".into_lean(lean)?;
        let back2: &str = <&str>::from_lean(&ls2)?;
        assert_eq!(back2, "borrowed");

        Ok(())
    });

    assert!(result.is_ok());
}

#[test]
fn test_debug_format() {
    leo3::prepare_freethreaded_lean();

    let result: LeanResult<()> = leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "Hello")?;
        assert_eq!(format!("{:?}", s), "LeanString(\"Hello\")");

        let empty = LeanString::mk(lean, "")?;
        assert_eq!(format!("{:?}", empty), "LeanString(\"\")");

        Ok(())
    });

    assert!(result.is_ok());
}
