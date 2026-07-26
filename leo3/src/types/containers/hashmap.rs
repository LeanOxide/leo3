//! Hash map wrapper for Lean's real `Std.HashMap` runtime representation.
#![allow(missing_docs)]

use crate::err::LeanResult;
use crate::ffi;
use crate::instance::LeanBound;
use crate::marker::Lean;
use crate::types::{
    LeanInt, LeanInt16, LeanInt32, LeanInt64, LeanInt8, LeanList, LeanNat, LeanOption, LeanProd,
    LeanString,
};
use std::ffi::c_void;
use std::marker::PhantomData;

pub struct LeanHashMapType<K, V> {
    _phantom: PhantomData<(K, V)>,
}

pub type LeanHashMap<'l, K, V> = LeanBound<'l, LeanHashMapType<K, V>>;

pub trait LeanHashKey {
    #[doc(hidden)]
    unsafe fn decidable_eq_boxed() -> *mut c_void;
    #[doc(hidden)]
    unsafe fn hash_closure() -> *mut ffi::lean_object;
}

#[cfg(not(target_os = "windows"))]
unsafe extern "C" {
    static mut l_instHashableNat: *mut ffi::lean_object;
    static mut l_instHashableInt: *mut ffi::lean_object;
    static mut l_instHashableString: *mut ffi::lean_object;
    static mut l_instHashableInt8: *mut ffi::lean_object;
    static mut l_instHashableInt16: *mut ffi::lean_object;
    static mut l_instHashableInt32: *mut ffi::lean_object;
    static mut l_instHashableInt64: *mut ffi::lean_object;

    fn l_instDecidableEqNat___boxed(
        a: *mut ffi::lean_object,
        b: *mut ffi::lean_object,
    ) -> *mut ffi::lean_object;
    fn l_Int_decEq___boxed(
        a: *mut ffi::lean_object,
        b: *mut ffi::lean_object,
    ) -> *mut ffi::lean_object;
    fn l_instDecidableEqString___boxed(
        a: *mut ffi::lean_object,
        b: *mut ffi::lean_object,
    ) -> *mut ffi::lean_object;
    fn l_instDecidableEqInt8___boxed(
        a: *mut ffi::lean_object,
        b: *mut ffi::lean_object,
    ) -> *mut ffi::lean_object;
    fn l_instDecidableEqInt16___boxed(
        a: *mut ffi::lean_object,
        b: *mut ffi::lean_object,
    ) -> *mut ffi::lean_object;
    fn l_instDecidableEqInt32___boxed(
        a: *mut ffi::lean_object,
        b: *mut ffi::lean_object,
    ) -> *mut ffi::lean_object;
    fn l_instDecidableEqInt64___boxed(
        a: *mut ffi::lean_object,
        b: *mut ffi::lean_object,
    ) -> *mut ffi::lean_object;

    fn l_instBEqOfDecidableEq___redArg(dec_eq: *mut ffi::lean_object) -> *mut ffi::lean_object;
}

macro_rules! impl_hash_key {
    ($ty:ty, $eq_fn:ident, $eq_name:literal, $hash_sym:ident, $hash_name:literal) => {
        impl LeanHashKey for $ty {
            unsafe fn decidable_eq_boxed() -> *mut c_void {
                #[cfg(not(target_os = "windows"))]
                {
                    $eq_fn as *mut c_void
                }
                #[cfg(target_os = "windows")]
                {
                    super::symbols::required_function($eq_name)
                }
            }

            unsafe fn hash_closure() -> *mut ffi::lean_object {
                #[cfg(not(target_os = "windows"))]
                {
                    $hash_sym
                }
                #[cfg(target_os = "windows")]
                {
                    super::symbols::required_bss_global($hash_name)
                }
            }
        }
    };
}

impl_hash_key!(
    LeanNat,
    l_instDecidableEqNat___boxed,
    "l_instDecidableEqNat___boxed",
    l_instHashableNat,
    "l_instHashableNat"
);
impl_hash_key!(
    LeanInt,
    l_Int_decEq___boxed,
    "l_Int_decEq___boxed",
    l_instHashableInt,
    "l_instHashableInt"
);
impl_hash_key!(
    LeanString,
    l_instDecidableEqString___boxed,
    "l_instDecidableEqString___boxed",
    l_instHashableString,
    "l_instHashableString"
);
impl_hash_key!(
    LeanInt8,
    l_instDecidableEqInt8___boxed,
    "l_instDecidableEqInt8___boxed",
    l_instHashableInt8,
    "l_instHashableInt8"
);
impl_hash_key!(
    LeanInt16,
    l_instDecidableEqInt16___boxed,
    "l_instDecidableEqInt16___boxed",
    l_instHashableInt16,
    "l_instHashableInt16"
);
impl_hash_key!(
    LeanInt32,
    l_instDecidableEqInt32___boxed,
    "l_instDecidableEqInt32___boxed",
    l_instHashableInt32,
    "l_instHashableInt32"
);
impl_hash_key!(
    LeanInt64,
    l_instDecidableEqInt64___boxed,
    "l_instDecidableEqInt64___boxed",
    l_instHashableInt64,
    "l_instHashableInt64"
);

#[inline]
pub(super) unsafe fn beq_closure<K: LeanHashKey>() -> *mut ffi::lean_object {
    let dec_eq = ffi::inline::lean_alloc_closure(K::decidable_eq_boxed(), 2, 0);

    #[cfg(not(target_os = "windows"))]
    {
        l_instBEqOfDecidableEq___redArg(dec_eq)
    }

    #[cfg(target_os = "windows")]
    {
        let beq_of_decidable_eq: unsafe extern "C" fn(
            *mut ffi::lean_object,
        ) -> *mut ffi::lean_object = std::mem::transmute(super::symbols::required_function(
            "l_instBEqOfDecidableEq___redArg",
        ));
        beq_of_decidable_eq(dec_eq)
    }
}

#[inline]
unsafe fn owned_hash<K: LeanHashKey>() -> *mut ffi::lean_object {
    let ptr = K::hash_closure();
    ffi::lean_inc(ptr);
    ptr
}

#[inline]
unsafe fn owned_view<T>(obj: &LeanBound<'_, T>) -> *mut ffi::lean_object {
    let ptr = obj.as_ptr();
    ffi::lean_inc(ptr);
    ptr
}

impl<'l, K: LeanHashKey, V> LeanHashMap<'l, K, V> {
    pub fn empty(lean: Lean<'l>) -> LeanResult<Self> {
        unsafe {
            let ptr =
                ffi::hashmap::l_Std_HashMap_emptyWithCapacity___redArg(ffi::inline::lean_box(8));
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
            let beq = beq_closure::<K>();
            let ptr = ffi::hashmap::l_Std_HashMap_insert___redArg(
                beq,
                owned_hash::<K>(),
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
            let beq = beq_closure::<K>();
            let ptr = ffi::hashmap::l_Std_HashMap_get_x3f___redArg(
                beq,
                owned_hash::<K>(),
                owned_view(self),
                owned_view(key),
            );
            let opt = LeanBound::<LeanOption>::from_owned_ptr(lean, ptr);
            Ok(LeanOption::get(&opt).map(|value| value.cast()))
        }
    }

    pub fn contains(&self, _lean: Lean<'l>, key: &LeanBound<'l, K>) -> LeanResult<bool> {
        let contains = unsafe {
            let beq = beq_closure::<K>();
            ffi::hashmap::l_Std_HashMap_contains___redArg(
                beq,
                owned_hash::<K>(),
                owned_view(self),
                owned_view(key),
            )
        };
        Ok(contains != 0)
    }

    pub fn erase(self, lean: Lean<'l>, key: &LeanBound<'l, K>) -> LeanResult<Self> {
        unsafe {
            let beq = beq_closure::<K>();
            let ptr = ffi::hashmap::l_Std_HashMap_erase___redArg(
                beq,
                owned_hash::<K>(),
                self.into_ptr(),
                owned_view(key),
            );
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    pub fn size(&self) -> LeanResult<usize> {
        let lean = self.lean_token();
        unsafe {
            let ptr = ffi::hashmap::l_Std_HashMap_size___redArg(owned_view(self));
            let size = LeanBound::<LeanNat>::from_owned_ptr(lean, ptr);
            LeanNat::to_usize(&size)
        }
    }

    pub fn is_empty(&self) -> LeanResult<bool> {
        Ok(self.size()? == 0)
    }

    pub fn to_list(&self, lean: Lean<'l>) -> LeanResult<LeanBound<'l, LeanList>> {
        unsafe {
            let ptr = ffi::hashmap::l_Std_HashMap_toList___redArg(owned_view(self));
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    pub fn from_list(lean: Lean<'l>, list: LeanBound<'l, LeanList>) -> LeanResult<Self> {
        let mut map = Self::empty(lean)?;
        let mut current = list;
        while !crate::types::LeanList::isEmpty(&current) {
            let pair =
                crate::types::LeanList::head(&current).expect("non-empty list should have head");
            let pair: LeanBound<'l, LeanProd> = pair.cast();
            let key: LeanBound<'l, K> = LeanProd::fst(&pair).cast();
            let value: LeanBound<'l, V> = LeanProd::snd(&pair).cast();
            map = map.insert(lean, key, value)?;
            current =
                crate::types::LeanList::tail(&current).expect("non-empty list should have tail");
        }
        Ok(map)
    }
}
