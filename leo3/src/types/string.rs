//! Lean string type wrapper.

use crate::err::{LeanError, LeanResult};
use crate::ffi;
use crate::instance::LeanBound;
use crate::marker::Lean;

/// A Lean string object.
///
/// Provides safe wrappers around Lean's UTF-8 string type.
pub struct LeanString {
    _private: (),
}

impl LeanString {
    /// Create a new Lean string from a Rust string.
    ///
    /// # Lean4 Reference
    /// Corresponds to string literal construction in Lean4.
    /// Uses C API `lean_mk_string_from_bytes` (length-aware single copy;
    /// embedded NUL bytes are preserved, matching Lean's byte-array strings).
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use leo3::prelude::*;
    ///
    /// fn main() -> LeanResult<()> {
    ///     leo3::prepare_freethreaded_lean();
    ///
    ///     leo3::with_lean(|lean| {
    ///         let s = LeanString::mk(lean, "Hello, Lean!")?;
    ///         println!("{}", LeanString::cstr(&s)?);
    ///         Ok(())
    ///     })
    /// }
    /// ```
    pub fn mk<'l>(lean: Lean<'l>, s: &str) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            // Length-aware copy: no intermediate CString allocation, no strlen
            // on extraction, and embedded NUL bytes round-trip unchanged.
            let ptr =
                ffi::string::lean_mk_string_from_bytes(s.as_ptr() as *const libc::c_char, s.len());
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Create a singleton string from a character.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.singleton` in Lean4.
    pub fn singleton<'l>(lean: Lean<'l>, c: char) -> LeanResult<LeanBound<'l, Self>> {
        let mut buf = [0u8; 4];
        let s = c.encode_utf8(&mut buf);
        Self::mk(lean, s)
    }

    /// Get the string as a C-style string pointer (null-terminated).
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.data` in Lean4 (conceptually).
    /// Uses C API `lean_string_cstr`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use leo3::prelude::*;
    ///
    /// fn main() -> LeanResult<()> {
    ///     leo3::prepare_freethreaded_lean();
    ///
    ///     leo3::with_lean(|lean| {
    ///         let s = LeanString::mk(lean, "Hello")?;
    ///         assert_eq!(LeanString::cstr(&s)?, "Hello");
    ///         Ok(())
    ///     })
    /// }
    /// ```
    /// Zero-copy UTF-8 view of the string's byte payload.
    ///
    /// The view covers the full payload (length-aware, so embedded NUL bytes
    /// are preserved) and borrows from the Lean object.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.data` in Lean4 (conceptually).
    pub fn as_str<'l>(obj: &LeanBound<'l, Self>) -> LeanResult<&'l str> {
        unsafe {
            let ptr = ffi::inline::lean_string_cstr(obj.as_ptr());
            // `lean_string_size` counts the trailing NUL terminator.
            let len = ffi::inline::lean_string_size(obj.as_ptr()) - 1;
            let bytes = std::slice::from_raw_parts(ptr as *const u8, len);
            std::str::from_utf8(bytes)
                .map_err(|e| LeanError::conversion(&format!("Invalid UTF-8: {}", e)))
        }
    }

    /// Get the string as a C-style string pointer (null-terminated).
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.data` in Lean4 (conceptually).
    /// Uses C API `lean_string_cstr`.
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use leo3::prelude::*;
    ///
    /// fn main() -> LeanResult<()> {
    ///     leo3::prepare_freethreaded_lean();
    ///
    ///     leo3::with_lean(|lean| {
    ///         let s = LeanString::mk(lean, "Hello")?;
    ///         assert_eq!(LeanString::cstr(&s)?, "Hello");
    ///         Ok(())
    ///     })
    /// }
    /// ```
    pub fn cstr<'l>(obj: &LeanBound<'l, Self>) -> LeanResult<&'l str> {
        Self::as_str(obj)
    }

    /// Get the length of the string (number of characters).
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.length` in Lean4.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let s = LeanString::mk(lean, "Hello")?;
    /// assert_eq!(LeanString::length(&s), 5);
    /// ```
    pub fn length<'l>(obj: &LeanBound<'l, Self>) -> usize {
        unsafe { ffi::inline::lean_string_len(obj.as_ptr()) }
    }

    /// Get the UTF-8 byte size of the string.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.utf8ByteSize` in Lean4.
    #[allow(non_snake_case)]
    pub fn utf8ByteSize<'l>(obj: &LeanBound<'l, Self>) -> usize {
        unsafe { ffi::inline::lean_string_size(obj.as_ptr()) }
    }

    /// Check if the string is empty.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.isEmpty` in Lean4.
    #[allow(non_snake_case)]
    pub fn isEmpty<'l>(obj: &LeanBound<'l, Self>) -> bool {
        Self::length(obj) == 0
    }

    /// Push a character onto the end of the string.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.push` in Lean4.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let s = LeanString::mk(lean, "Hello")?;
    /// let s = LeanString::push(s, '!')?;
    /// assert_eq!(LeanString::cstr(&s)?, "Hello!");
    /// ```
    pub fn push<'l>(s: LeanBound<'l, Self>, c: char) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            let lean = s.lean_token();
            let ptr = ffi::string::lean_string_push(s.into_ptr(), c as u32);
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Push a character n times onto the end of the string.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.pushn` in Lean4.
    pub fn pushn<'l>(s: LeanBound<'l, Self>, c: char, n: usize) -> LeanResult<LeanBound<'l, Self>> {
        let mut result = s;
        for _ in 0..n {
            result = Self::push(result, c)?;
        }
        Ok(result)
    }

    /// Append another string to this string.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.append` in Lean4.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let s1 = LeanString::mk(lean, "Hello, ")?;
    /// let s2 = LeanString::mk(lean, "World!")?;
    /// let result = LeanString::append(s1, &s2)?;
    /// assert_eq!(LeanString::cstr(&result)?, "Hello, World!");
    /// ```
    pub fn append<'l>(
        s1: LeanBound<'l, Self>,
        s2: &LeanBound<'l, Self>,
    ) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            let lean = s1.lean_token();
            let ptr = ffi::string::lean_string_append(s1.into_ptr(), s2.as_ptr());
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Compare two strings for equality.
    ///
    /// # Lean4 Reference
    /// Corresponds to decidable equality on strings in Lean4.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let s1 = LeanString::mk(lean, "hello")?;
    /// let s2 = LeanString::mk(lean, "hello")?;
    /// assert!(LeanString::eq(&s1, &s2));
    /// ```
    pub fn eq<'l>(s1: &LeanBound<'l, Self>, s2: &LeanBound<'l, Self>) -> bool {
        unsafe { ffi::string::lean_string_eq(s1.as_ptr(), s2.as_ptr()) }
    }

    /// Decidable equality.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.decEq` in Lean4.
    #[allow(non_snake_case)]
    pub fn decEq<'l>(s1: &LeanBound<'l, Self>, s2: &LeanBound<'l, Self>) -> bool {
        Self::eq(s1, s2)
    }

    /// Check if s1 is less than or equal to s2 lexicographically.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.le` in Lean4.
    pub fn le<'l>(s1: &LeanBound<'l, Self>, s2: &LeanBound<'l, Self>) -> bool {
        !Self::lt(s2, s1)
    }

    /// Compare two strings lexicographically.
    ///
    /// Returns true if s1 < s2.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.lt` in Lean4.
    pub fn lt<'l>(s1: &LeanBound<'l, Self>, s2: &LeanBound<'l, Self>) -> bool {
        unsafe { ffi::string::lean_string_lt(s1.as_ptr(), s2.as_ptr()) }
    }

    /// Check if this string starts with the given prefix.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.startsWith` in Lean4.
    #[allow(non_snake_case)]
    pub fn startsWith<'l>(s: &LeanBound<'l, Self>, prefix: &LeanBound<'l, Self>) -> bool {
        match (Self::cstr(s), Self::cstr(prefix)) {
            (Ok(s_str), Ok(prefix_str)) => s_str.starts_with(prefix_str),
            _ => false,
        }
    }

    /// Check if this string ends with the given suffix.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.endsWith` in Lean4.
    #[allow(non_snake_case)]
    pub fn endsWith<'l>(s: &LeanBound<'l, Self>, suffix: &LeanBound<'l, Self>) -> bool {
        match (Self::cstr(s), Self::cstr(suffix)) {
            (Ok(s_str), Ok(suffix_str)) => s_str.ends_with(suffix_str),
            _ => false,
        }
    }

    /// Check if a prefix is a prefix of the string.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.isPrefixOf` in Lean4.
    #[allow(non_snake_case)]
    pub fn isPrefixOf<'l>(prefix: &LeanBound<'l, Self>, s: &LeanBound<'l, Self>) -> bool {
        Self::startsWith(s, prefix)
    }

    /// Check if the string contains a character.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.contains` in Lean4.
    pub fn contains<'l>(s: &LeanBound<'l, Self>, c: char) -> bool {
        match Self::cstr(s) {
            Ok(s_str) => s_str.contains(c),
            _ => false,
        }
    }

    /// Trim whitespace from both ends.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.trim` in Lean4.
    pub fn trim<'l>(s: LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
        let lean = s.lean_token();
        match Self::cstr(&s) {
            Ok(s_str) => Self::mk(lean, s_str.trim()),
            Err(e) => Err(e),
        }
    }

    /// Trim whitespace from the left.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.trimLeft` in Lean4.
    #[allow(non_snake_case)]
    pub fn trimLeft<'l>(s: LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
        let lean = s.lean_token();
        match Self::cstr(&s) {
            Ok(s_str) => Self::mk(lean, s_str.trim_start()),
            Err(e) => Err(e),
        }
    }

    /// Trim whitespace from the right.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.trimRight` in Lean4.
    #[allow(non_snake_case)]
    pub fn trimRight<'l>(s: LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
        let lean = s.lean_token();
        match Self::cstr(&s) {
            Ok(s_str) => Self::mk(lean, s_str.trim_end()),
            Err(e) => Err(e),
        }
    }

    /// Convert string to uppercase.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.toUpper` in Lean4.
    #[allow(non_snake_case)]
    pub fn toUpper<'l>(s: LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
        let lean = s.lean_token();
        match Self::cstr(&s) {
            Ok(s_str) => Self::mk(lean, &s_str.to_uppercase()),
            Err(e) => Err(e),
        }
    }

    /// Convert string to lowercase.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.toLower` in Lean4.
    #[allow(non_snake_case)]
    pub fn toLower<'l>(s: LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
        let lean = s.lean_token();
        match Self::cstr(&s) {
            Ok(s_str) => Self::mk(lean, &s_str.to_lowercase()),
            Err(e) => Err(e),
        }
    }

    /// Capitalize the first character.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.capitalize` in Lean4.
    pub fn capitalize<'l>(s: LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
        let lean = s.lean_token();
        match Self::cstr(&s) {
            Ok(s_str) => {
                let mut chars = s_str.chars();
                match chars.next() {
                    Some(first) => {
                        let result: String = first.to_uppercase().chain(chars).collect();
                        Self::mk(lean, &result)
                    }
                    None => Self::mk(lean, ""),
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Decapitalize the first character.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.decapitalize` in Lean4.
    pub fn decapitalize<'l>(s: LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
        let lean = s.lean_token();
        match Self::cstr(&s) {
            Ok(s_str) => {
                let mut chars = s_str.chars();
                match chars.next() {
                    Some(first) => {
                        let result: String = first.to_lowercase().chain(chars).collect();
                        Self::mk(lean, &result)
                    }
                    None => Self::mk(lean, ""),
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Get the first character of the string.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.front` in Lean4.
    pub fn front<'l>(obj: &LeanBound<'l, Self>) -> Option<char> {
        Self::cstr(obj).ok().and_then(|s| s.chars().next())
    }

    /// Get the last character of the string.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.back` in Lean4.
    pub fn back<'l>(obj: &LeanBound<'l, Self>) -> Option<char> {
        Self::cstr(obj).ok().and_then(|s| s.chars().last())
    }

    /// Extract a substring from byte positions.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let s = LeanString::mk(lean, "Hello, World!")?;
    /// let sub = LeanString::extract(lean, &s, 0, 5)?;
    /// assert_eq!(LeanString::cstr(&sub)?, "Hello");
    /// ```
    pub fn extract<'l>(
        lean: Lean<'l>,
        s: &LeanBound<'l, Self>,
        start: usize,
        end: usize,
    ) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            let start_boxed = ffi::lean_box(start);
            let end_boxed = ffi::lean_box(end);
            let ptr = ffi::string::lean_string_utf8_extract(s.as_ptr(), start_boxed, end_boxed);
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Get a UTF-32 character at a byte position.
    ///
    /// # Safety
    /// - `pos` must be a valid UTF-8 boundary
    pub fn get_char<'l>(s: &LeanBound<'l, Self>, pos: usize) -> u32 {
        unsafe {
            let pos_boxed = ffi::lean_box(pos);
            ffi::string::lean_string_utf8_get(s.as_ptr(), pos_boxed)
        }
    }

    /// Get the next UTF-8 byte position.
    ///
    /// # Safety
    /// - `pos` must be a valid UTF-8 boundary
    pub fn next_pos<'l>(s: &LeanBound<'l, Self>, pos: usize) -> usize {
        unsafe {
            let pos_boxed = ffi::lean_box(pos);
            let result = ffi::string::lean_string_utf8_next(s.as_ptr(), pos_boxed);
            ffi::lean_unbox(result)
        }
    }

    /// Get the previous UTF-8 byte position.
    ///
    /// # Safety
    /// - `pos` must be a valid UTF-8 boundary
    pub fn prev_pos<'l>(s: &LeanBound<'l, Self>, pos: usize) -> usize {
        unsafe {
            let pos_boxed = ffi::lean_box(pos);
            let result = ffi::string::lean_string_utf8_prev(s.as_ptr(), pos_boxed);
            ffi::lean_unbox(result)
        }
    }

    /// Compute hash of the string.
    ///
    /// # Lean4 Reference
    /// Corresponds to `String.hash` in Lean4.
    pub fn hash<'l>(obj: &LeanBound<'l, Self>) -> u64 {
        unsafe { ffi::string::lean_string_hash(obj.as_ptr()) }
    }
}

// Implement Display for convenient printing (requires to_str conversion)
impl<'l> std::fmt::Debug for LeanBound<'l, LeanString> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match LeanString::cstr(self) {
            Ok(s) => write!(f, "LeanString({:?})", s),
            Err(_) => write!(f, "LeanString(<invalid UTF-8>)"),
        }
    }
}
