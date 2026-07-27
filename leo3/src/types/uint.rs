//! Lean unsigned integer type wrappers.
//!
//! Provides wrappers for UInt8, UInt16, UInt32, UInt64, and USize types.

use crate::err::LeanResult;
use crate::ffi;
use crate::ffi::inline::*;
use crate::ffi::object::{
    lean_ctor_get_uint32, lean_ctor_get_uint64, lean_ctor_set_uint32, lean_ctor_set_uint64,
};
use crate::instance::LeanBound;
use crate::marker::Lean;
use crate::types::LeanChar;
use paste::paste;

macro_rules! uint_type {
    (
        name: $name:ident,
        rust: $rust:ty,
        bits: $bits:expr,
        size_ty: $size_ty:ty,
        prefix: $prefix:ident,
        conversions: [ $( $method:ident => ($target:ident, $target_rust:ty, $ffi_fn:ident) ),* $(,)? ],
    ) => {
        paste! {
            #[doc = concat!("A Lean ", stringify!($bits), "-bit unsigned integer.")]
            pub struct $name {
                _private: (),
            }

            #[allow(non_snake_case, missing_docs)]
            impl $name {
                pub const SIZE: $size_ty = (1 as $size_ty) << $bits;
                pub const MIN: $rust = <$rust>::MIN;
                pub const MAX: $rust = <$rust>::MAX;

                pub fn mk<'l>(lean: Lean<'l>, value: $rust) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let ptr = ffi::inline::lean_box(value as usize);
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                pub fn [<to_ $rust>]<'l>(obj: &LeanBound<'l, Self>) -> $rust {
                    unsafe { ffi::inline::lean_unbox(obj.as_ptr()) as $rust }
                }

                pub fn add<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _add>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn sub<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _sub>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn mul<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _mul>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn div<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _div>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn mod_<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _mod>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn neg<'l>(lean: Lean<'l>, a: &LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _neg>](Self::[<to_ $rust>](a));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn land<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _land>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn lor<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _lor>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn xor<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _xor>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn complement<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _complement>](Self::[<to_ $rust>](a));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn shiftLeft<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _shift_left>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn shiftRight<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _shift_right>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn decEq<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_eq>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn decLt<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_lt>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn decLe<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_le>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn isValidChar<'l>(obj: &LeanBound<'l, Self>) -> bool {
                    let val = Self::[<to_ $rust>](obj) as u64;
                    if val > u32::MAX as u64 {
                        return false;
                    }
                    char::from_u32(val as u32).is_some()
                }

                pub fn toChar<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, LeanChar>> {
                    let val = Self::[<to_ $rust>](obj) as u64;
                    if val > u32::MAX as u64 {
                        return Err(crate::err::LeanError::conversion(
                            "Value out of range for Unicode scalar",
                        ));
                    }
                    match char::from_u32(val as u32) {
                        Some(c) => LeanChar::mk(lean, c),
                        None => Err(crate::err::LeanError::conversion(
                            "Invalid Unicode scalar value",
                        )),
                    }
                }

                pub fn toInt<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, crate::types::LeanInt>> {
                    let nat = Self::toNat(obj, lean)?;
                    crate::types::LeanInt::ofNat(lean, nat)
                }

                pub fn ofInt<'l>(
                    lean: Lean<'l>,
                    int: &LeanBound<'l, crate::types::LeanInt>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    let val = crate::types::LeanInt::to_i64(int).unwrap_or(0) as $rust;
                    Self::mk(lean, val)
                }

                pub fn toNat<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, crate::types::LeanNat>> {
                    unsafe {
                        let ptr = [<$prefix _to_nat>](Self::[<to_ $rust>](obj));
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                pub fn ofNat<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let val = [<$prefix _of_nat>](nat.as_ptr()) as $rust;
                        Self::mk(lean, val)
                    }
                }

                pub fn ofNatTruncate<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    Self::ofNat(lean, nat)
                }

                pub fn ofNatLT<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    Self::ofNat(lean, nat)
                }

                pub fn le<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_le>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn lt<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_lt>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn toFloat<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, crate::types::LeanFloat>> {
                    unsafe {
                        let ptr = ffi::inline::lean_box_float(Self::[<to_ $rust>](obj) as f64);
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                pub fn toFloat32<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, crate::types::LeanFloat32>> {
                    unsafe {
                        let ptr = ffi::inline::lean_box_float32([<$prefix _to_float32>](Self::[<to_ $rust>](obj)));
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                $(
                    pub fn $method<'l>(
                        obj: &LeanBound<'l, Self>,
                        lean: Lean<'l>,
                    ) -> LeanResult<LeanBound<'l, $target>> {
                        unsafe {
                            let val = $ffi_fn(Self::[<to_ $rust>](obj));
                            $target::mk(lean, val as $target_rust)
                        }
                    }
                )*

                pub fn log2<'l>(obj: &LeanBound<'l, Self>) -> $rust {
                    unsafe { [<$prefix _log2>](Self::[<to_ $rust>](obj)) as $rust }
                }
            }

            impl<'l> std::fmt::Debug for LeanBound<'l, $name> {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, concat!(stringify!($name), "({})"), $name::[<to_ $rust>](self))
                }
            }
        }
    };
}

macro_rules! uint_heap_type {
    (
        name: $name:ident,
        rust: $rust:ty,
        bits: $bits:expr,
        size_ty: $size_ty:ty,
        ctor_bytes: $ctor_bytes:expr,
        storage: $storage_suffix:ident,
        prefix: $prefix:ident,
        conversions: [ $( $method:ident => ($target:ident, $target_rust:ty, $ffi_fn:ident) ),* $(,)? ],
    ) => {
        paste! {
            #[doc = concat!("A Lean ", stringify!($bits), "-bit unsigned integer.")]
            pub struct $name {
                _private: (),
            }

            #[allow(non_snake_case, missing_docs)]
            impl $name {
                pub const SIZE: $size_ty = (1 as $size_ty) << $bits;
                pub const MIN: $rust = <$rust>::MIN;
                pub const MAX: $rust = <$rust>::MAX;

                pub fn mk<'l>(lean: Lean<'l>, value: $rust) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let ptr = ffi::lean_alloc_ctor(0, 0, $ctor_bytes);
                        [<lean_ctor_set_ $storage_suffix>](ptr, 0, value as _);
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                pub fn [<to_ $rust>]<'l>(obj: &LeanBound<'l, Self>) -> $rust {
                    unsafe { [<lean_ctor_get_ $storage_suffix>](obj.as_ptr(), 0) as $rust }
                }

                pub fn add<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _add>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn sub<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _sub>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn mul<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _mul>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn div<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _div>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn mod_<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _mod>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn neg<'l>(lean: Lean<'l>, a: &LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _neg>](Self::[<to_ $rust>](a));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn land<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _land>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn lor<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _lor>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn xor<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _xor>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn complement<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _complement>](Self::[<to_ $rust>](a));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn shiftLeft<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _shift_left>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn shiftRight<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _shift_right>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn decEq<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_eq>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn decLt<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_lt>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn decLe<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_le>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn isValidChar<'l>(obj: &LeanBound<'l, Self>) -> bool {
                    let val = Self::[<to_ $rust>](obj) as u64;
                    if val > u32::MAX as u64 {
                        return false;
                    }
                    char::from_u32(val as u32).is_some()
                }

                pub fn toChar<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, LeanChar>> {
                    let val = Self::[<to_ $rust>](obj) as u64;
                    if val > u32::MAX as u64 {
                        return Err(crate::err::LeanError::conversion(
                            "Value out of range for Unicode scalar",
                        ));
                    }
                    match char::from_u32(val as u32) {
                        Some(c) => LeanChar::mk(lean, c),
                        None => Err(crate::err::LeanError::conversion(
                            "Invalid Unicode scalar value",
                        )),
                    }
                }

                pub fn toInt<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, crate::types::LeanInt>> {
                    let nat = Self::toNat(obj, lean)?;
                    crate::types::LeanInt::ofNat(lean, nat)
                }

                pub fn ofInt<'l>(
                    lean: Lean<'l>,
                    int: &LeanBound<'l, crate::types::LeanInt>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    let val = crate::types::LeanInt::to_i64(int).unwrap_or(0) as $rust;
                    Self::mk(lean, val)
                }

                pub fn toNat<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, crate::types::LeanNat>> {
                    unsafe {
                        let ptr = [<$prefix _to_nat>](Self::[<to_ $rust>](obj));
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                pub fn ofNat<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let val = [<$prefix _of_nat>](nat.as_ptr()) as $rust;
                        Self::mk(lean, val)
                    }
                }

                pub fn ofNatTruncate<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    Self::ofNat(lean, nat)
                }

                pub fn ofNatLT<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    Self::ofNat(lean, nat)
                }

                pub fn le<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_le>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn lt<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_lt>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn toFloat<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, crate::types::LeanFloat>> {
                    unsafe {
                        let ptr = ffi::inline::lean_box_float(Self::[<to_ $rust>](obj) as f64);
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                pub fn toFloat32<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, crate::types::LeanFloat32>> {
                    unsafe {
                        let ptr = ffi::inline::lean_box_float32([<$prefix _to_float32>](Self::[<to_ $rust>](obj)));
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                $(
                    pub fn $method<'l>(
                        obj: &LeanBound<'l, Self>,
                        lean: Lean<'l>,
                    ) -> LeanResult<LeanBound<'l, $target>> {
                        unsafe {
                            let val = $ffi_fn(Self::[<to_ $rust>](obj));
                            $target::mk(lean, val as $target_rust)
                        }
                    }
                )*

                pub fn log2<'l>(obj: &LeanBound<'l, Self>) -> $rust {
                    unsafe { [<$prefix _log2>](Self::[<to_ $rust>](obj)) as $rust }
                }
            }

            impl<'l> std::fmt::Debug for LeanBound<'l, $name> {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, concat!(stringify!($name), "({})"), $name::[<to_ $rust>](self))
                }
            }
        }
    };
}

macro_rules! platform_uint_type {
    (
        name: $name:ident,
        rust: $rust:ty,
        prefix: $prefix:ident,
        conversions: [ $( $method:ident => ($target:ident, $target_rust:ty, $ffi_fn:ident) ),* $(,)? ],
    ) => {
        paste! {
            #[doc = concat!("A Lean platform-sized unsigned integer (", stringify!($name), ").")]
            pub struct $name {
                _private: (),
            }

            #[allow(non_snake_case, missing_docs)]
            impl $name {
                #[cfg(target_pointer_width = "64")]
                pub const SIZE: u128 = 18446744073709551616; // 2^64

                #[cfg(target_pointer_width = "32")]
                pub const SIZE: u64 = 4294967296; // 2^32

                pub const MIN: $rust = <$rust>::MIN;

                #[cfg(target_pointer_width = "64")]
                pub const MAX: $rust = 18446744073709551615; // 2^64 - 1

                #[cfg(target_pointer_width = "32")]
                pub const MAX: $rust = 4294967295; // 2^32 - 1

                pub fn mk<'l>(lean: Lean<'l>, value: $rust) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        if std::mem::size_of::<$rust>() == 8 {
                            let ptr = ffi::lean_alloc_ctor(0, 0, 8);
                            lean_ctor_set_uint64(ptr, 0, value as u64);
                            Ok(LeanBound::from_owned_ptr(lean, ptr))
                        } else {
                            let ptr = ffi::lean_alloc_ctor(0, 0, 4);
                            lean_ctor_set_uint32(ptr, 0, value as u32);
                            Ok(LeanBound::from_owned_ptr(lean, ptr))
                        }
                    }
                }

                pub fn [<to_ $rust>]<'l>(obj: &LeanBound<'l, Self>) -> $rust {
                    if std::mem::size_of::<$rust>() == 8 {
                        unsafe { lean_ctor_get_uint64(obj.as_ptr(), 0) as $rust }
                    } else {
                        unsafe { lean_ctor_get_uint32(obj.as_ptr(), 0) as $rust }
                    }
                }

                pub fn add<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _add>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn sub<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _sub>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn mul<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _mul>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn div<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _div>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn mod_<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _mod>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn neg<'l>(lean: Lean<'l>, a: &LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _neg>](Self::[<to_ $rust>](a));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn land<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _land>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn lor<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _lor>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn xor<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _xor>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn complement<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _complement>](Self::[<to_ $rust>](a));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn shiftLeft<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _shift_left>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn shiftRight<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$prefix _shift_right>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b));
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn decEq<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_eq>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn decLt<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_lt>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn decLe<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_le>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn isValidChar<'l>(obj: &LeanBound<'l, Self>) -> bool {
                    let val = Self::[<to_ $rust>](obj) as u64;
                    val <= u32::MAX as u64 && char::from_u32(val as u32).is_some()
                }

                pub fn toChar<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, LeanChar>> {
                    let val = Self::[<to_ $rust>](obj) as u64;
                    if val > u32::MAX as u64 {
                        return Err(crate::err::LeanError::conversion(
                            "Value out of range for Unicode scalar",
                        ));
                    }
                    match char::from_u32(val as u32) {
                        Some(c) => LeanChar::mk(lean, c),
                        None => Err(crate::err::LeanError::conversion(
                            "Invalid Unicode scalar value",
                        )),
                    }
                }

                pub fn toInt<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, crate::types::LeanInt>> {
                    let nat = Self::toNat(obj, lean)?;
                    crate::types::LeanInt::ofNat(lean, nat)
                }

                pub fn ofInt<'l>(
                    lean: Lean<'l>,
                    int: &LeanBound<'l, crate::types::LeanInt>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    let val = crate::types::LeanInt::to_i64(int).unwrap_or(0) as $rust;
                    Self::mk(lean, val)
                }

                pub fn toNat<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, crate::types::LeanNat>> {
                    unsafe {
                        let ptr = [<$prefix _to_nat>](Self::[<to_ $rust>](obj));
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                pub fn ofNat<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let val = [<$prefix _of_nat>](nat.as_ptr()) as $rust;
                        Self::mk(lean, val)
                    }
                }

                pub fn ofNatTruncate<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    Self::ofNat(lean, nat)
                }

                pub fn ofNatLT<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    Self::ofNat(lean, nat)
                }

                pub fn le<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_le>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn lt<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$prefix _dec_lt>](Self::[<to_ $rust>](a), Self::[<to_ $rust>](b)) }
                }

                pub fn toFloat<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, crate::types::LeanFloat>> {
                    unsafe {
                        let ptr = ffi::inline::lean_box_float(Self::[<to_ $rust>](obj) as f64);
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                pub fn toFloat32<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, crate::types::LeanFloat32>> {
                    unsafe {
                        let ptr = ffi::inline::lean_box_float32([<$prefix _to_float32>](Self::[<to_ $rust>](obj)));
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                $(
                    pub fn $method<'l>(
                        obj: &LeanBound<'l, Self>,
                        lean: Lean<'l>,
                    ) -> LeanResult<LeanBound<'l, $target>> {
                        unsafe {
                            let val = $ffi_fn(Self::[<to_ $rust>](obj));
                            $target::mk(lean, val as $target_rust)
                        }
                    }
                )*

                pub fn log2<'l>(obj: &LeanBound<'l, Self>) -> $rust {
                    unsafe { [<$prefix _log2>](Self::[<to_ $rust>](obj)) as $rust }
                }
            }

            impl<'l> std::fmt::Debug for LeanBound<'l, $name> {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    write!(f, concat!(stringify!($name), "({})"), $name::[<to_ $rust>](self))
                }
            }
        }
    };
}

uint_type! {
    name: LeanUInt8,
    rust: u8,
    bits: 8,
    size_ty: u32,
    prefix: lean_uint8,
    conversions: [
        toUInt16 => (LeanUInt16, u16, lean_uint8_to_uint16),
        toUInt32 => (LeanUInt32, u32, lean_uint8_to_uint32),
        toUInt64 => (LeanUInt64, u64, lean_uint8_to_uint64),
        toUSize => (LeanUSize, usize, lean_uint8_to_usize),
    ],
}

uint_type! {
    name: LeanUInt16,
    rust: u16,
    bits: 16,
    size_ty: u32,
    prefix: lean_uint16,
    conversions: [
        toUInt8 => (LeanUInt8, u8, lean_uint16_to_uint8),
        toUInt32 => (LeanUInt32, u32, lean_uint16_to_uint32),
        toUInt64 => (LeanUInt64, u64, lean_uint16_to_uint64),
        toUSize => (LeanUSize, usize, lean_uint16_to_usize),
    ],
}

uint_type! {
    name: LeanUInt32,
    rust: u32,
    bits: 32,
    size_ty: u64,
    prefix: lean_uint32,
    conversions: [
        toUInt8 => (LeanUInt8, u8, lean_uint32_to_uint8),
        toUInt16 => (LeanUInt16, u16, lean_uint32_to_uint16),
        toUInt64 => (LeanUInt64, u64, lean_uint32_to_uint64),
        toUSize => (LeanUSize, usize, lean_uint32_to_usize),
    ],
}

uint_heap_type! {
    name: LeanUInt64,
    rust: u64,
    bits: 64,
    size_ty: u128,
    ctor_bytes: 8,
    storage: uint64,
    prefix: lean_uint64,
    conversions: [
        toUInt8 => (LeanUInt8, u8, lean_uint64_to_uint8),
        toUInt16 => (LeanUInt16, u16, lean_uint64_to_uint16),
        toUInt32 => (LeanUInt32, u32, lean_uint64_to_uint32),
        toUSize => (LeanUSize, usize, lean_uint64_to_usize),
    ],
}

platform_uint_type! {
    name: LeanUSize,
    rust: usize,
    prefix: lean_usize,
    conversions: [
        toUInt8 => (LeanUInt8, u8, lean_usize_to_uint8),
        toUInt16 => (LeanUInt16, u16, lean_usize_to_uint16),
        toUInt32 => (LeanUInt32, u32, lean_usize_to_uint32),
        toUInt64 => (LeanUInt64, u64, lean_usize_to_uint64),
    ],
}
