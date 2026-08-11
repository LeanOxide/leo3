//! Lean byte array type wrapper.

use crate::conversion::FromLean;
use crate::err::LeanResult;
use crate::ffi;
use crate::instance::LeanBound;
use crate::marker::Lean;

/// A Lean byte array.
///
/// ByteArray in Lean4 is defined as:
/// ```lean
/// structure ByteArray where
///   data : Array UInt8
/// ```
///
/// However, at the runtime level, it's represented as a scalar array
/// (LEAN_SCALAR_ARRAY) containing bytes.
pub struct LeanByteArray {
    _private: (),
}

impl<'l> FromLean<'l> for LeanBound<'l, LeanByteArray> {
    type Source = LeanByteArray;

    fn from_lean(obj: &LeanBound<'l, Self::Source>) -> LeanResult<Self> {
        Ok(obj.clone())
    }
}

impl LeanByteArray {
    /// Create an empty byte array.
    ///
    /// # Lean4 Reference
    /// Corresponds to `ByteArray.empty` in Lean4.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use leo3::prelude::*;
    ///
    /// leo3::with_lean(|lean| {
    ///     let ba = LeanByteArray::empty(lean)?;
    ///     assert_eq!(LeanByteArray::size(&ba), 0);
    ///     Ok(())
    /// })
    /// ```
    pub fn empty<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            let ptr = ffi::array::lean_mk_empty_byte_array(ffi::lean_box(0));
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Create a byte array with a specific initial capacity.
    ///
    /// # Lean4 Reference
    /// Corresponds to `ByteArray.emptyWithCapacity` in Lean4.
    pub fn with_capacity<'l>(lean: Lean<'l>, capacity: usize) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            let ptr = ffi::array::lean_mk_empty_byte_array(ffi::lean_box(capacity));
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Create a byte array from a Rust byte slice.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let ba = LeanByteArray::from_bytes(lean, b"Hello, World!")?;
    /// assert_eq!(LeanByteArray::size(&ba), 13);
    /// ```
    pub fn from_bytes<'l>(lean: Lean<'l>, bytes: &[u8]) -> LeanResult<LeanBound<'l, Self>> {
        let mut ba = Self::with_capacity(lean, bytes.len())?;
        for &byte in bytes {
            ba = Self::push(ba, byte)?;
        }
        Ok(ba)
    }

    /// Get the size (number of bytes) in the byte array.
    ///
    /// # Lean4 Reference
    /// Corresponds to `ByteArray.size` in Lean4.
    pub fn size<'l>(obj: &LeanBound<'l, Self>) -> usize {
        unsafe { ffi::inline::lean_sarray_size(obj.as_ptr()) }
    }

    /// Get a byte at a specific index (unchecked).
    ///
    /// # Safety
    /// The caller must ensure that `index < size`.
    pub unsafe fn uget<'l>(obj: &LeanBound<'l, Self>, index: usize) -> u8 {
        ffi::inline::lean_byte_array_uget(obj.as_ptr(), index)
    }

    /// Get a byte at a specific index (checked).
    ///
    /// Returns `None` if index is out of bounds.
    ///
    /// # Lean4 Reference
    /// Corresponds to `ByteArray.get?` in Lean4.
    pub fn get<'l>(obj: &LeanBound<'l, Self>, index: usize) -> Option<u8> {
        if index < Self::size(obj) {
            unsafe { Some(Self::uget(obj, index)) }
        } else {
            None
        }
    }

    /// Push a byte to the end of the array.
    ///
    /// # Lean4 Reference
    /// Corresponds to `ByteArray.push` in Lean4.
    pub fn push<'l>(obj: LeanBound<'l, Self>, byte: u8) -> LeanResult<LeanBound<'l, Self>> {
        let lean = obj.lean_token();
        unsafe {
            let ptr = ffi::array::lean_byte_array_push(obj.into_ptr(), byte);
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Set a byte at a specific index (unchecked).
    ///
    /// # Safety
    /// The caller must ensure that `index < size`.
    ///
    /// # Lean4 Reference
    /// Similar to array indexing with assignment in Lean4.
    pub unsafe fn uset<'l>(
        obj: LeanBound<'l, Self>,
        index: usize,
        byte: u8,
    ) -> LeanResult<LeanBound<'l, Self>> {
        let lean = obj.lean_token();
        let ptr = ffi::inline::lean_byte_array_uset(obj.into_ptr(), index, byte);
        Ok(LeanBound::from_owned_ptr(lean, ptr))
    }

    /// Set a byte at a specific index (checked).
    ///
    /// Returns the original array if index is out of bounds.
    pub fn set<'l>(
        obj: LeanBound<'l, Self>,
        index: usize,
        byte: u8,
    ) -> LeanResult<LeanBound<'l, Self>> {
        if index < Self::size(&obj) {
            unsafe { Self::uset(obj, index, byte) }
        } else {
            Ok(obj)
        }
    }

    /// Convert the byte array to a Rust `Vec<u8>`.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let ba = LeanByteArray::from_bytes(lean, b"Hello")?;
    /// let vec = LeanByteArray::to_vec(&ba);
    /// assert_eq!(vec, b"Hello");
    /// ```
    pub fn to_vec<'l>(obj: &LeanBound<'l, Self>) -> Vec<u8> {
        let size = Self::size(obj);
        let mut vec = Vec::with_capacity(size);
        for i in 0..size {
            unsafe {
                vec.push(Self::uget(obj, i));
            }
        }
        vec
    }

    /// Convert the byte array to a Rust byte slice.
    ///
    /// # Safety
    /// The returned slice is only valid while the LeanBound object is alive.
    pub unsafe fn as_slice<'l>(obj: &LeanBound<'l, Self>) -> &'l [u8] {
        let size = Self::size(obj);
        let ptr = ffi::inline::lean_sarray_cptr(obj.as_ptr());
        std::slice::from_raw_parts(ptr, size)
    }

    /// Check if the byte array is empty.
    #[allow(non_snake_case)]
    pub fn isEmpty<'l>(obj: &LeanBound<'l, Self>) -> bool {
        Self::size(obj) == 0
    }

    /// Get the capacity of the byte array.
    pub fn capacity<'l>(obj: &LeanBound<'l, Self>) -> usize {
        unsafe { ffi::inline::lean_sarray_capacity(obj.as_ptr()) }
    }
}

// Implement Debug for convenient printing
impl<'l> std::fmt::Debug for LeanBound<'l, LeanByteArray> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LeanByteArray(size: {})", LeanByteArray::size(self))
    }
}
