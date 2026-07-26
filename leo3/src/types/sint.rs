//! Lean signed integer type wrappers.
//!
//! Provides wrappers for Int8, Int16, Int32, Int64, and ISize types.
//!
//! In Lean4, these are structures that wrap UInt types using two's complement.
//! ```lean
//! structure Int8 where
//!   ofUInt8 :: toUInt8 : UInt8
//! ```

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

macro_rules! sint_scalar_type {
    (
        name: $name:ident,
        rust: $rust:ty,
        unsigned: $unsigned:ty,
        bits: $bits:expr,
        size_ty: $size_ty:ty,
        ops_prefix: $ops_prefix:ident,
        nat_prefix: $nat_prefix:ident,
        conversions: [ $( $method:ident => ($target:ident, $target_rust:ty, $ffi_fn:ident) ),* $(,)? ],
    ) => {
        paste! {
            #[doc = concat!("A Lean ", stringify!($bits), "-bit signed integer.")]
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
                        let ptr = ffi::inline::lean_box(value as $unsigned as usize);
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                pub fn [<to_ $rust>]<'l>(obj: &LeanBound<'l, Self>) -> $rust {
                    unsafe { ffi::inline::lean_unbox(obj.as_ptr()) as $unsigned as $rust }
                }

                pub fn add<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _add>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn sub<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _sub>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn mul<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _mul>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn div<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _div>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn mod_<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _mod>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn neg<'l>(lean: Lean<'l>, a: &LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _neg>](Self::[<to_ $rust>](a) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn abs<'l>(lean: Lean<'l>, a: &LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _abs>](Self::[<to_ $rust>](a) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn land<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _land>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn lor<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _lor>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn xor<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _xor>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn complement<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _complement>](Self::[<to_ $rust>](a) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn shiftLeft<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result =
                            [<$ops_prefix _shift_left>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn shiftRight<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result =
                            [<$ops_prefix _shift_right>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn decEq<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_eq>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _) }
                }

                pub fn decLt<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_lt>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _) }
                }

                pub fn decLe<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_le>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _) }
                }

                pub fn isValidChar<'l>(obj: &LeanBound<'l, Self>) -> bool {
                    let val = Self::[<to_ $rust>](obj) as i128;
                    if val < 0 || val > u32::MAX as i128 {
                        return false;
                    }
                    char::from_u32(val as u32).is_some()
                }

                pub fn toChar<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, LeanChar>> {
                    let val = Self::[<to_ $rust>](obj) as i128;
                    if val < 0 {
                        return Err(crate::err::LeanError::conversion(
                            "Negative values cannot be converted to char",
                        ));
                    }
                    if val > u32::MAX as i128 {
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
                    unsafe {
                        let val = Self::[<to_ $rust>](obj) as i64;
                        let ptr = ffi::inline::lean_int64_to_int(val);
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
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
                    let val = Self::[<to_ $rust>](obj);
                    if val < 0 {
                        return Err(crate::err::LeanError::conversion(
                            "Negative values cannot be converted to Nat",
                        ));
                    }
                    unsafe {
                        let ptr = [<$nat_prefix _to_nat>](val as _);
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                pub fn ofNat<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let val = [<$nat_prefix _of_nat>](nat.as_ptr()) as $rust;
                        Self::mk(lean, val)
                    }
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
                        let ptr = ffi::inline::lean_box_float32([<$ops_prefix _to_float32>](Self::[<to_ $rust>](obj) as _));
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                $(
                    pub fn $method<'l>(
                        obj: &LeanBound<'l, Self>,
                        lean: Lean<'l>,
                    ) -> LeanResult<LeanBound<'l, $target>> {
                        unsafe {
                            let val = $ffi_fn(Self::[<to_ $rust>](obj) as _);
                            $target::mk(lean, val as $target_rust)
                        }
                    }
                )*

                pub fn ofNatTruncate<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    Self::ofNat(lean, nat)
                }

                pub fn ofIntTruncate<'l>(
                    lean: Lean<'l>,
                    int: &LeanBound<'l, crate::types::LeanInt>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    Self::ofInt(lean, int)
                }

                pub fn le<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_le>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _) }
                }

                pub fn lt<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_lt>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _) }
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

macro_rules! sint_heap_type {
    (
        name: $name:ident,
        rust: $rust:ty,
        unsigned: $unsigned:ty,
        bits: $bits:expr,
        size_ty: $size_ty:ty,
        ctor_bytes: $ctor_bytes:expr,
        storage: $storage_suffix:ident,
        ops_prefix: $ops_prefix:ident,
        nat_prefix: $nat_prefix:ident,
        conversions: [ $( $method:ident => ($target:ident, $target_rust:ty, $ffi_fn:ident) ),* $(,)? ],
    ) => {
        paste! {
            #[doc = concat!("A Lean ", stringify!($bits), "-bit signed integer.")]
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
                        let result = [<$ops_prefix _add>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn sub<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _sub>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn mul<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _mul>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn div<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _div>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn mod_<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _mod>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn neg<'l>(lean: Lean<'l>, a: &LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _neg>](Self::[<to_ $rust>](a) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn abs<'l>(lean: Lean<'l>, a: &LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _abs>](Self::[<to_ $rust>](a) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn land<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _land>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn lor<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _lor>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn xor<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _xor>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn complement<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _complement>](Self::[<to_ $rust>](a) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn shiftLeft<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result =
                            [<$ops_prefix _shift_left>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn shiftRight<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result =
                            [<$ops_prefix _shift_right>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn decEq<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_eq>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _) }
                }

                pub fn decLt<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_lt>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _) }
                }

                pub fn decLe<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_le>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _) }
                }

                pub fn isValidChar<'l>(obj: &LeanBound<'l, Self>) -> bool {
                    let val = Self::[<to_ $rust>](obj) as i128;
                    if val < 0 || val > u32::MAX as i128 {
                        return false;
                    }
                    char::from_u32(val as u32).is_some()
                }

                pub fn toChar<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, LeanChar>> {
                    let val = Self::[<to_ $rust>](obj) as i128;
                    if val < 0 {
                        return Err(crate::err::LeanError::conversion(
                            "Negative values cannot be converted to char",
                        ));
                    }
                    if val > u32::MAX as i128 {
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
                    unsafe {
                        let val = Self::[<to_ $rust>](obj) as i64;
                        let ptr = ffi::inline::lean_int64_to_int(val);
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
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
                    let val = Self::[<to_ $rust>](obj);
                    if val < 0 {
                        return Err(crate::err::LeanError::conversion(
                            "Negative values cannot be converted to Nat",
                        ));
                    }
                    unsafe {
                        let ptr = [<$nat_prefix _to_nat>](val as _);
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                pub fn ofNat<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let val = [<$nat_prefix _of_nat>](nat.as_ptr()) as $rust;
                        Self::mk(lean, val)
                    }
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
                        let ptr = ffi::inline::lean_box_float32([<$ops_prefix _to_float32>](Self::[<to_ $rust>](obj) as _));
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                $(
                    pub fn $method<'l>(
                        obj: &LeanBound<'l, Self>,
                        lean: Lean<'l>,
                    ) -> LeanResult<LeanBound<'l, $target>> {
                        unsafe {
                            let val = $ffi_fn(Self::[<to_ $rust>](obj) as _);
                            $target::mk(lean, val as $target_rust)
                        }
                    }
                )*

                pub fn ofNatTruncate<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    Self::ofNat(lean, nat)
                }

                pub fn ofIntTruncate<'l>(
                    lean: Lean<'l>,
                    int: &LeanBound<'l, crate::types::LeanInt>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    Self::ofInt(lean, int)
                }

                pub fn le<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_le>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _) }
                }

                pub fn lt<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_lt>](Self::[<to_ $rust>](a) as _, Self::[<to_ $rust>](b) as _) }
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

macro_rules! platform_sint_type {
    (
        name: $name:ident,
        rust: $rust:ty,
        ops_prefix: $ops_prefix:ident,
        nat_prefix: $nat_prefix:ident,
        conversions: [ $( $method:ident => ($target:ident, $target_rust:ty, $ffi_fn:ident) ),* $(,)? ],
    ) => {
        paste! {
            #[doc = concat!("A Lean platform-sized signed integer (", stringify!($name), ").")]
            pub struct $name {
                _private: (),
            }

            #[allow(non_snake_case, missing_docs)]
            impl $name {
                #[cfg(target_pointer_width = "64")]
                pub const SIZE: u128 = 18446744073709551616; // 2^64

                #[cfg(target_pointer_width = "32")]
                pub const SIZE: u64 = 4294967296; // 2^32

                #[cfg(target_pointer_width = "64")]
                pub const MIN: $rust = -9223372036854775808; // -(2^63)

                #[cfg(target_pointer_width = "32")]
                pub const MIN: $rust = -2147483648; // -(2^31)

                #[cfg(target_pointer_width = "64")]
                pub const MAX: $rust = 9223372036854775807; // 2^63 - 1

                #[cfg(target_pointer_width = "32")]
                pub const MAX: $rust = 2147483647; // 2^31 - 1

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
                        let result = [<$ops_prefix _add>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn sub<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _sub>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn mul<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _mul>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn div<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _div>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn mod_<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _mod>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn neg<'l>(lean: Lean<'l>, a: &LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _neg>](Self::[<to_ $rust>](a) as usize);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn abs<'l>(lean: Lean<'l>, a: &LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _abs>](Self::[<to_ $rust>](a) as usize);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn land<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _land>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn lor<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _lor>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn xor<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _xor>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn complement<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result = [<$ops_prefix _complement>](Self::[<to_ $rust>](a) as usize);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn shiftLeft<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result =
                            [<$ops_prefix _shift_left>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn shiftRight<'l>(
                    lean: Lean<'l>,
                    a: &LeanBound<'l, Self>,
                    b: &LeanBound<'l, Self>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let result =
                            [<$ops_prefix _shift_right>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize);
                        Self::mk(lean, result as $rust)
                    }
                }

                pub fn decEq<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_eq>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize) }
                }

                pub fn decLt<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_lt>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize) }
                }

                pub fn decLe<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_le>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize) }
                }

                pub fn isValidChar<'l>(obj: &LeanBound<'l, Self>) -> bool {
                    let val = Self::[<to_ $rust>](obj);
                    if val < 0 || val > u32::MAX as $rust {
                        return false;
                    }
                    let uval = val as u32;
                    uval < 0xD800 || (0xE000..=0x10FFFF).contains(&uval)
                }

                pub fn toChar<'l>(
                    obj: &LeanBound<'l, Self>,
                    lean: Lean<'l>,
                ) -> LeanResult<LeanBound<'l, LeanChar>> {
                    let val = Self::[<to_ $rust>](obj);
                    if val < 0 {
                        return Err(crate::err::LeanError::conversion(
                            "Negative values cannot be converted to char",
                        ));
                    }
                    if val > u32::MAX as $rust {
                        return Err(crate::err::LeanError::conversion(
                            "Value out of range for Unicode scalar",
                        ));
                    }
                    let uval = val as u32;
                    match char::from_u32(uval) {
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
                    unsafe {
                        let val = Self::[<to_ $rust>](obj) as i64;
                        let ptr = ffi::inline::lean_int64_to_int(val);
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
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
                    let val = Self::[<to_ $rust>](obj);
                    if val < 0 {
                        return Err(crate::err::LeanError::conversion(
                            "Negative values cannot be converted to Nat",
                        ));
                    }
                    unsafe {
                        let ptr = [<$nat_prefix _to_nat>](val as _);
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                pub fn ofNat<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    unsafe {
                        let val = [<$nat_prefix _of_nat>](nat.as_ptr()) as $rust;
                        Self::mk(lean, val)
                    }
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
                        let ptr =
                            ffi::inline::lean_box_float32([<$ops_prefix _to_float32>](Self::[<to_ $rust>](obj)));
                        Ok(LeanBound::from_owned_ptr(lean, ptr))
                    }
                }

                $(
                    pub fn $method<'l>(
                        obj: &LeanBound<'l, Self>,
                        lean: Lean<'l>,
                    ) -> LeanResult<LeanBound<'l, $target>> {
                        unsafe {
                            let val = $ffi_fn(Self::[<to_ $rust>](obj) as usize);
                            $target::mk(lean, val as $target_rust)
                        }
                    }
                )*

                pub fn ofNatTruncate<'l>(
                    lean: Lean<'l>,
                    nat: &LeanBound<'l, crate::types::LeanNat>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    Self::ofNat(lean, nat)
                }

                pub fn ofIntTruncate<'l>(
                    lean: Lean<'l>,
                    int: &LeanBound<'l, crate::types::LeanInt>,
                ) -> LeanResult<LeanBound<'l, Self>> {
                    Self::ofInt(lean, int)
                }

                pub fn le<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_le>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize) }
                }

                pub fn lt<'l>(a: &LeanBound<'l, Self>, b: &LeanBound<'l, Self>) -> bool {
                    unsafe { [<$ops_prefix _dec_lt>](Self::[<to_ $rust>](a) as usize, Self::[<to_ $rust>](b) as usize) }
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

sint_scalar_type! {
    name: LeanInt8,
    rust: i8,
    unsigned: u8,
    bits: 8,
    size_ty: u32,
    ops_prefix: lean_int8,
    nat_prefix: lean_uint8,
    conversions: [
        toInt16 => (LeanInt16, i16, lean_int8_to_int16),
        toInt32 => (LeanInt32, i32, lean_int8_to_int32),
        toInt64 => (LeanInt64, i64, lean_int8_to_int64),
        toISize => (LeanISize, isize, lean_int8_to_isize),
    ],
}

sint_scalar_type! {
    name: LeanInt16,
    rust: i16,
    unsigned: u16,
    bits: 16,
    size_ty: u32,
    ops_prefix: lean_int16,
    nat_prefix: lean_uint16,
    conversions: [
        toInt8 => (LeanInt8, i8, lean_int16_to_int8),
        toInt32 => (LeanInt32, i32, lean_int16_to_int32),
        toInt64 => (LeanInt64, i64, lean_int16_to_int64),
        toISize => (LeanISize, isize, lean_int16_to_isize),
    ],
}

sint_scalar_type! {
    name: LeanInt32,
    rust: i32,
    unsigned: u32,
    bits: 32,
    size_ty: u64,
    ops_prefix: lean_int32,
    nat_prefix: lean_uint32,
    conversions: [
        toInt8 => (LeanInt8, i8, lean_int32_to_int8),
        toInt16 => (LeanInt16, i16, lean_int32_to_int16),
        toInt64 => (LeanInt64, i64, lean_int32_to_int64),
        toISize => (LeanISize, isize, lean_int32_to_isize),
    ],
}

sint_heap_type! {
    name: LeanInt64,
    rust: i64,
    unsigned: u64,
    bits: 64,
    size_ty: u128,
    ctor_bytes: 8,
    storage: uint64,
    ops_prefix: lean_int64,
    nat_prefix: lean_uint64,
    conversions: [
        toInt8 => (LeanInt8, i8, lean_int64_to_int8),
        toInt16 => (LeanInt16, i16, lean_int64_to_int16),
        toInt32 => (LeanInt32, i32, lean_int64_to_int32),
        toISize => (LeanISize, isize, lean_int64_to_isize),
    ],
}

platform_sint_type! {
    name: LeanISize,
    rust: isize,
    ops_prefix: lean_isize,
    nat_prefix: lean_usize,
    conversions: [
        toInt8 => (LeanInt8, i8, lean_isize_to_int8),
        toInt16 => (LeanInt16, i16, lean_isize_to_int16),
        toInt32 => (LeanInt32, i32, lean_isize_to_int32),
        toInt64 => (LeanInt64, i64, lean_isize_to_int64),
    ],
}
