//! Red-black map wrapper for Lean's real `Lean.RBMap` runtime representation.
#![allow(missing_docs)]
//!
//! `LeanRBMap` is the first experimental container wrapper in Leo3 that uses
//! Lean's real runtime semantics instead of a placeholder structure. The
//! current support matrix is intentionally narrow: only key families with a
//! known exported compare closure are supported.

use crate::err::LeanResult;
use crate::ffi;
use crate::instance::LeanBound;
use crate::marker::Lean;
use crate::types::{
    LeanInt, LeanInt16, LeanInt32, LeanInt64, LeanInt8, LeanList, LeanNat, LeanOption, LeanProd,
    LeanString, LeanUInt16, LeanUInt32, LeanUInt64, LeanUInt8,
};
use std::marker::PhantomData;

pub struct LeanRBMapType<K, V> {
    _phantom: PhantomData<(K, V)>,
}

pub type LeanRBMap<'l, K, V> = LeanBound<'l, LeanRBMapType<K, V>>;

/// Key families currently supported by the real `RBMap` wrapper.
pub trait LeanRBMapKey {
    #[doc(hidden)]
    unsafe fn compare_closure() -> *mut ffi::lean_object;
}

#[cfg(not(target_os = "windows"))]
unsafe extern "C" {
    static mut l_instOrdNat: *mut ffi::lean_object;
    static mut l_instOrdInt: *mut ffi::lean_object;
    static mut l_String_instOrd: *mut ffi::lean_object;
    static mut l_Int8_instOrd: *mut ffi::lean_object;
    static mut l_Int16_instOrd: *mut ffi::lean_object;
    static mut l_Int32_instOrd: *mut ffi::lean_object;
    static mut l_Int64_instOrd: *mut ffi::lean_object;
    static mut l_UInt8_instOrd: *mut ffi::lean_object;
    static mut l_UInt16_instOrd: *mut ffi::lean_object;
    static mut l_UInt32_instOrd: *mut ffi::lean_object;
    static mut l_UInt64_instOrd: *mut ffi::lean_object;
}

macro_rules! impl_rbmap_key {
    ($ty:ty, $sym:ident, $sym_name:literal) => {
        impl LeanRBMapKey for $ty {
            unsafe fn compare_closure() -> *mut ffi::lean_object {
                #[cfg(not(target_os = "windows"))]
                {
                    $sym
                }
                #[cfg(target_os = "windows")]
                {
                    super::symbols::required_bss_global($sym_name)
                }
            }
        }
    };
}

impl_rbmap_key!(LeanNat, l_instOrdNat, "l_instOrdNat");
impl_rbmap_key!(LeanInt, l_instOrdInt, "l_instOrdInt");
impl_rbmap_key!(LeanString, l_String_instOrd, "l_String_instOrd");
impl_rbmap_key!(LeanInt8, l_Int8_instOrd, "l_Int8_instOrd");
impl_rbmap_key!(LeanInt16, l_Int16_instOrd, "l_Int16_instOrd");
impl_rbmap_key!(LeanInt32, l_Int32_instOrd, "l_Int32_instOrd");
impl_rbmap_key!(LeanInt64, l_Int64_instOrd, "l_Int64_instOrd");
impl_rbmap_key!(LeanUInt8, l_UInt8_instOrd, "l_UInt8_instOrd");
impl_rbmap_key!(LeanUInt16, l_UInt16_instOrd, "l_UInt16_instOrd");
impl_rbmap_key!(LeanUInt32, l_UInt32_instOrd, "l_UInt32_instOrd");
impl_rbmap_key!(LeanUInt64, l_UInt64_instOrd, "l_UInt64_instOrd");

#[inline]
unsafe fn owned_cmp<K: LeanRBMapKey>() -> *mut ffi::lean_object {
    let ptr = K::compare_closure();
    ffi::lean_inc(ptr);
    ptr
}

#[inline]
unsafe fn owned_view<T>(obj: &LeanBound<'_, T>) -> *mut ffi::lean_object {
    let ptr = obj.as_ptr();
    ffi::lean_inc(ptr);
    ptr
}

impl<'l, K: LeanRBMapKey, V> LeanRBMap<'l, K, V> {
    pub fn empty(lean: Lean<'l>) -> LeanResult<Self> {
        unsafe {
            let ptr = ffi::rbmap::l_Lean_RBMap_empty(owned_cmp::<K>());
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    pub fn insert(
        self,
        lean: Lean<'l>,
        key: LeanBound<'l, K>,
        value: LeanBound<'l, V>,
    ) -> LeanResult<Self> {
        unsafe {
            let ptr = ffi::rbmap::l_Lean_RBMap_insert___redArg(
                owned_cmp::<K>(),
                self.into_ptr(),
                key.into_ptr(),
                value.into_ptr(),
            );
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    pub fn find(
        &self,
        lean: Lean<'l>,
        key: &LeanBound<'l, K>,
    ) -> LeanResult<Option<LeanBound<'l, V>>> {
        unsafe {
            let ptr = ffi::rbmap::l_Lean_RBMap_find_x3f___redArg(
                owned_cmp::<K>(),
                owned_view(self),
                owned_view(key),
            );
            let opt = LeanBound::<LeanOption>::from_owned_ptr(lean, ptr);
            Ok(LeanOption::get(&opt).map(|value| value.cast()))
        }
    }

    pub fn contains(&self, _lean: Lean<'l>, key: &LeanBound<'l, K>) -> LeanResult<bool> {
        let contains = unsafe {
            ffi::rbmap::l_Lean_RBMap_contains___redArg(
                owned_cmp::<K>(),
                owned_view(self),
                owned_view(key),
            )
        };
        Ok(contains != 0)
    }

    pub fn erase(self, lean: Lean<'l>, key: &LeanBound<'l, K>) -> LeanResult<Self> {
        unsafe {
            let ptr = ffi::rbmap::l_Lean_RBMap_erase___redArg(
                owned_cmp::<K>(),
                self.into_ptr(),
                owned_view(key),
            );
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    pub fn is_empty(&self) -> bool {
        unsafe { ffi::rbmap::l_Lean_RBMap_isEmpty___redArg(self.as_ptr()) != 0 }
    }

    pub fn size(&self) -> LeanResult<usize> {
        let lean = self.lean_token();
        unsafe {
            let ptr = ffi::rbmap::l_Lean_RBMap_size___redArg(owned_view(self));
            let size = LeanBound::<LeanNat>::from_owned_ptr(lean, ptr);
            LeanNat::to_usize(&size)
        }
    }

    pub fn to_list(&self, lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanList>> {
        unsafe {
            let ptr = ffi::rbmap::l_Lean_RBMap_toList___redArg(owned_view(self));
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    pub fn min_entry(&self, lean: Lean<'l>) -> LeanResult<Option<LeanBound<'l, LeanProd>>> {
        unsafe {
            let ptr = ffi::rbmap::l_Lean_RBMap_min___redArg(owned_view(self));
            let opt = LeanBound::<LeanOption>::from_owned_ptr(lean, ptr);
            Ok(LeanOption::get(&opt).map(|value| value.cast()))
        }
    }

    pub fn max_entry(&self, lean: Lean<'l>) -> LeanResult<Option<LeanBound<'l, LeanProd>>> {
        unsafe {
            let ptr = ffi::rbmap::l_Lean_RBMap_max___redArg(owned_view(self));
            let opt = LeanBound::<LeanOption>::from_owned_ptr(lean, ptr);
            Ok(LeanOption::get(&opt).map(|value| value.cast()))
        }
    }
}
