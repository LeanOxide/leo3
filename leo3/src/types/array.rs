//! Lean array type wrapper.

use crate::err::{LeanError, LeanResult};
use crate::ffi;
use crate::instance::{LeanAny, LeanBound};
use crate::marker::Lean;
use crate::types::LeanList;

/// A Lean array object.
///
/// Arrays in Lean4 are dynamic arrays of Lean objects.
pub struct LeanArray {
    _private: (),
}

impl LeanArray {
    /// Create an empty Lean array.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.empty` in Lean4.
    /// ```lean
    /// def Array.empty : Array α := emptyWithCapacity 0
    /// ```
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use leo3::prelude::*;
    ///
    /// leo3::with_lean(|lean| {
    ///     let arr = LeanArray::empty(lean)?;
    ///     assert!(LeanArray::isEmpty(&arr));
    ///     Ok(())
    /// })
    /// ```
    pub fn empty<'l>(lean: Lean<'l>) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            let ptr = ffi::array::lean_mk_empty_array();
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Create an empty Lean array with pre-allocated capacity.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.emptyWithCapacity` in Lean4.
    /// ```lean
    /// @[extern "lean_mk_empty_array_with_capacity"]
    /// def Array.emptyWithCapacity (c : @& Nat) : Array α
    /// ```
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// use leo3::prelude::*;
    ///
    /// leo3::with_lean(|lean| {
    ///     let arr = LeanArray::emptyWithCapacity(lean, 100)?;
    ///     assert_eq!(LeanArray::size(&arr), 0);
    ///     assert_eq!(LeanArray::capacity(&arr), 100);
    ///     Ok(())
    /// })
    /// ```
    #[allow(non_snake_case)]
    pub fn emptyWithCapacity<'l>(
        lean: Lean<'l>,
        capacity: usize,
    ) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            let cap_boxed = ffi::lean_box(capacity);
            let ptr = ffi::array::lean_mk_empty_array_with_capacity(cap_boxed);
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Get the size of the array.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.size` in Lean4.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let arr = LeanArray::empty(lean)?;
    /// assert_eq!(LeanArray::size(&arr), 0);
    /// ```
    pub fn size<'l>(obj: &LeanBound<'l, Self>) -> usize {
        unsafe { ffi::array::lean_array_size(obj.as_ptr()) }
    }

    /// Get the capacity of the array.
    ///
    /// The capacity is the number of elements that can be stored
    /// without reallocation.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let arr = LeanArray::emptyWithCapacity(lean, 100)?;
    /// assert_eq!(LeanArray::capacity(&arr), 100);
    /// ```
    pub fn capacity<'l>(obj: &LeanBound<'l, Self>) -> usize {
        unsafe { ffi::array::lean_array_capacity(obj.as_ptr()) }
    }

    /// Check if the array is empty.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.isEmpty` in Lean4.
    #[allow(non_snake_case)]
    pub fn isEmpty<'l>(obj: &LeanBound<'l, Self>) -> bool {
        Self::size(obj) == 0
    }

    /// Push an element to the end of the array.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let arr = LeanArray::empty(lean)?;
    /// let elem = LeanNat::from_usize(lean, 42)?;
    /// let arr = LeanArray::push(arr, elem.unbind())?;
    /// assert_eq!(LeanArray::size(&arr), 1);
    /// ```
    pub fn push<'l>(
        arr: LeanBound<'l, Self>,
        elem: LeanBound<'l, LeanAny>,
    ) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            let lean = arr.lean_token();
            let ptr = ffi::array::lean_array_push(arr.into_ptr(), elem.into_ptr());
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Get an element from the array at the given index.
    ///
    /// Returns `None` if the index is out of bounds.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let elem = LeanArray::get(&arr, 0);
    /// ```
    pub fn get<'l>(obj: &LeanBound<'l, Self>, index: usize) -> Option<LeanBound<'l, LeanAny>> {
        if index >= Self::size(obj) {
            return None;
        }

        unsafe {
            let lean = obj.lean_token();
            let ptr = ffi::array::lean_array_uget(obj.as_ptr(), index);
            Some(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Set an element in the array at the given index.
    ///
    /// Returns the modified array. If the array is exclusive (RC == 1),
    /// it's modified in-place. Otherwise, a copy is made.
    ///
    /// # Errors
    ///
    /// Returns an error if index is out of bounds.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let new_elem = LeanNat::from_usize(lean, 100)?;
    /// let arr = LeanArray::set(arr, 0, new_elem.unbind())?;
    /// ```
    pub fn set<'l>(
        arr: LeanBound<'l, Self>,
        index: usize,
        value: LeanBound<'l, LeanAny>,
    ) -> LeanResult<LeanBound<'l, Self>> {
        if index >= Self::size(&arr) {
            return Err(LeanError::out_of_bounds(index, Self::size(&arr)));
        }

        unsafe {
            let lean = arr.lean_token();
            let ptr = ffi::array::lean_array_uset(arr.into_ptr(), index, value.into_ptr());
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Pop the last element from the array.
    ///
    /// Returns the modified array with the last element removed.
    ///
    /// # Errors
    ///
    /// Returns an error if the array is empty.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let arr = LeanArray::pop(arr)?;
    /// ```
    pub fn pop<'l>(arr: LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
        if Self::isEmpty(&arr) {
            return Err(LeanError::out_of_bounds(0, 0));
        }

        unsafe {
            let lean = arr.lean_token();
            let ptr = ffi::array::lean_array_pop(arr.into_ptr());
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Swap two elements in the array.
    ///
    /// # Errors
    ///
    /// Returns an error if either index is out of bounds.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let arr = LeanArray::swap(arr, 0, 1)?;
    /// ```
    pub fn swap<'l>(
        arr: LeanBound<'l, Self>,
        i: usize,
        j: usize,
    ) -> LeanResult<LeanBound<'l, Self>> {
        let size = Self::size(&arr);
        if i >= size || j >= size {
            let bad_idx = if i >= size { i } else { j };
            return Err(LeanError::out_of_bounds(bad_idx, size));
        }

        unsafe {
            let lean = arr.lean_token();
            let ptr = ffi::array::lean_array_uswap(arr.into_ptr(), i, j);
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Create an array with `n` copies of the given value.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.replicate` in Lean4.
    /// ```lean
    /// @[extern "lean_mk_array"]
    /// def Array.replicate (n : @& Nat) (v : α) : Array α
    /// ```
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let val = LeanNat::from_usize(lean, 42)?;
    /// let arr = LeanArray::replicate(10, val.unbind())?;
    /// assert_eq!(LeanArray::size(&arr), 10);
    /// ```
    pub fn replicate<'l>(
        n: usize,
        value: LeanBound<'l, LeanAny>,
    ) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            let lean = value.lean_token();
            let n_boxed = ffi::lean_box(n);
            let ptr = ffi::array::lean_mk_array(n_boxed, value.into_ptr());
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Get an element from the array with a default value if out of bounds.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.getD` in Lean4.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let default = LeanNat::from_usize(lean, 0)?;
    /// let elem = LeanArray::getD(&arr, 100, default.unbind())?;
    /// ```
    #[allow(non_snake_case)]
    pub fn getD<'l>(
        obj: &LeanBound<'l, Self>,
        index: usize,
        default: LeanBound<'l, LeanAny>,
    ) -> LeanResult<LeanBound<'l, LeanAny>> {
        if index >= Self::size(obj) {
            return Ok(default);
        }

        unsafe {
            let lean = obj.lean_token();
            let ptr = ffi::array::lean_array_uget(obj.as_ptr(), index);
            // Need to decrement the default value since we're not using it
            ffi::lean_dec(default.into_ptr());
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Get the last element of the array.
    ///
    /// Returns `None` if the array is empty.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.back?` in Lean4.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// if let Some(last) = LeanArray::back(&arr) {
    ///     // Process last element
    /// }
    /// ```
    pub fn back<'l>(obj: &LeanBound<'l, Self>) -> Option<LeanBound<'l, LeanAny>> {
        let size = Self::size(obj);
        if size == 0 {
            return None;
        }
        Self::get(obj, size - 1)
    }

    /// Create an array from a Lean list.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.mk` in Lean4.
    /// ```lean
    /// structure Array (α : Type u) where
    ///   mk :: toList : List α
    /// ```
    ///
    /// # Safety
    ///
    /// The list object must be a valid Lean list.
    pub unsafe fn mk<'l>(list: LeanBound<'l, LeanAny>) -> LeanResult<LeanBound<'l, Self>> {
        let lean = list.lean_token();
        let ptr = ffi::array::lean_array_mk(list.into_ptr());
        Ok(LeanBound::from_owned_ptr(lean, ptr))
    }

    /// Create an array with pre-allocated capacity.
    ///
    /// This is useful for building arrays when you know the final size upfront.
    /// The array is created with size 0 but with capacity for `capacity` elements.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let arr = LeanArray::with_capacity(lean, 100)?;
    /// // arr.size() == 0, but has space for 100 elements
    /// ```
    pub fn with_capacity<'l>(lean: Lean<'l>, capacity: usize) -> LeanResult<LeanBound<'l, Self>> {
        unsafe {
            // Create empty array
            let nil_ptr = ffi::lean_alloc_ctor(0, 0, 0);
            let mut ptr = ffi::array::lean_array_mk(nil_ptr);

            // Expand capacity to accommodate requested size
            // We need to expand multiple times to reach the desired capacity
            if capacity > 0 {
                // Lean doubles capacity with each expansion
                // Start by expanding once to get initial capacity
                ptr = ffi::array::lean_copy_expand_array(ptr, true);

                // Keep expanding until we have enough capacity
                while ffi::array::lean_array_capacity(ptr) < capacity {
                    ptr = ffi::array::lean_copy_expand_array(ptr, true);
                }
            }

            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Push an element to the array without checking if a reallocation is needed.
    ///
    /// This is faster than `push` when you've pre-allocated capacity.
    ///
    /// # Safety
    ///
    /// The array must have sufficient capacity (size < capacity).
    /// Use `with_capacity` to ensure this.
    pub unsafe fn push_unchecked<'l>(
        arr: LeanBound<'l, Self>,
        elem: LeanBound<'l, LeanAny>,
    ) -> LeanResult<LeanBound<'l, Self>> {
        let lean = arr.lean_token();
        let ptr = arr.as_ptr();
        let size = ffi::array::lean_array_size(ptr);
        let capacity = ffi::array::lean_array_capacity(ptr);

        debug_assert!(
            size < capacity,
            "push_unchecked called without sufficient capacity"
        );

        // Ensure exclusive ownership
        let ptr = ffi::array::lean_ensure_exclusive_array(arr.into_ptr());

        // Set element at current size position
        let data_ptr = ffi::array::lean_array_cptr(ptr);
        *data_ptr.add(size) = elem.into_ptr();

        // Increment size
        ffi::array::lean_array_set_size(ptr, size + 1);

        Ok(LeanBound::from_owned_ptr(lean, ptr))
    }

    /// Create a singleton array containing one element.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.singleton` in Lean4.
    pub fn singleton<'l>(elem: LeanBound<'l, LeanAny>) -> LeanResult<LeanBound<'l, Self>> {
        let lean = elem.lean_token();
        let arr = Self::empty(lean)?;
        Self::push(arr, elem)
    }

    /// Create an array containing 0, 1, 2, ..., n-1.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.range` in Lean4.
    pub fn range<'l>(lean: Lean<'l>, n: usize) -> LeanResult<LeanBound<'l, Self>> {
        let mut arr = Self::emptyWithCapacity(lean, n)?;
        for i in 0..n {
            unsafe {
                let nat_ptr = ffi::lean_box(i);
                let elem = LeanBound::from_owned_ptr(lean, nat_ptr);
                arr = Self::push(arr, elem)?;
            }
        }
        Ok(arr)
    }

    /// Extract a sub-array from start to end (exclusive).
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.extract` in Lean4.
    pub fn extract<'l>(
        arr: &LeanBound<'l, Self>,
        start: usize,
        end: usize,
    ) -> LeanResult<LeanBound<'l, Self>> {
        let lean = arr.lean_token();
        let size = Self::size(arr);
        let start = start.min(size);
        let end = end.min(size);
        let count = end.saturating_sub(start);

        let mut result = Self::emptyWithCapacity(lean, count)?;
        for i in start..end {
            if let Some(elem) = Self::get(arr, i) {
                result = Self::push(result, elem)?;
            }
        }
        Ok(result)
    }

    /// Convert the array to a Lean list.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.toList` in Lean4.
    #[allow(non_snake_case)]
    pub fn toList<'l>(arr: LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, LeanList>> {
        unsafe {
            let lean = arr.lean_token();
            let ptr = ffi::array::lean_array_to_list(arr.into_ptr());
            Ok(LeanBound::from_owned_ptr(lean, ptr))
        }
    }

    /// Reverse the array.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.reverse` in Lean4.
    pub fn reverse<'l>(arr: LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
        let lean = arr.lean_token();
        let size = Self::size(&arr);
        if size <= 1 {
            return Ok(arr);
        }

        let mut result = Self::emptyWithCapacity(lean, size)?;
        for i in (0..size).rev() {
            if let Some(elem) = Self::get(&arr, i) {
                result = Self::push(result, elem)?;
            }
        }
        Ok(result)
    }

    /// Take the first n elements.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.take` in Lean4.
    pub fn take<'l>(arr: &LeanBound<'l, Self>, n: usize) -> LeanResult<LeanBound<'l, Self>> {
        Self::extract(arr, 0, n)
    }

    /// Drop the first n elements.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.drop` in Lean4.
    pub fn drop<'l>(arr: &LeanBound<'l, Self>, n: usize) -> LeanResult<LeanBound<'l, Self>> {
        Self::extract(arr, n, Self::size(arr))
    }

    /// Append another array to this array.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.append` in Lean4 (also `++` operator).
    pub fn append<'l>(
        xs: LeanBound<'l, Self>,
        ys: &LeanBound<'l, Self>,
    ) -> LeanResult<LeanBound<'l, Self>> {
        let ys_size = Self::size(ys);
        if ys_size == 0 {
            return Ok(xs);
        }
        let mut result = xs;
        for i in 0..ys_size {
            if let Some(elem) = Self::get(ys, i) {
                result = Self::push(result, elem)?;
            }
        }
        Ok(result)
    }

    /// Check if array contains an element using equality function.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.contains` in Lean4.
    pub fn contains<'l, F>(arr: &LeanBound<'l, Self>, elem: &LeanBound<'l, LeanAny>, eq: F) -> bool
    where
        F: Fn(&LeanBound<'l, LeanAny>, &LeanBound<'l, LeanAny>) -> bool,
    {
        let size = Self::size(arr);
        for i in 0..size {
            if let Some(e) = Self::get(arr, i) {
                if eq(&e, elem) {
                    return true;
                }
            }
        }
        false
    }

    /// Check if all elements satisfy a predicate.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.all` in Lean4.
    pub fn all<'l, F>(arr: &LeanBound<'l, Self>, pred: F) -> bool
    where
        F: Fn(&LeanBound<'l, LeanAny>) -> bool,
    {
        let size = Self::size(arr);
        for i in 0..size {
            if let Some(elem) = Self::get(arr, i) {
                if !pred(&elem) {
                    return false;
                }
            }
        }
        true
    }

    /// Check if any element satisfies a predicate.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.any` in Lean4.
    pub fn any<'l, F>(arr: &LeanBound<'l, Self>, pred: F) -> bool
    where
        F: Fn(&LeanBound<'l, LeanAny>) -> bool,
    {
        let size = Self::size(arr);
        for i in 0..size {
            if let Some(elem) = Self::get(arr, i) {
                if pred(&elem) {
                    return true;
                }
            }
        }
        false
    }

    /// Find the first element satisfying a predicate.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.find?` in Lean4.
    pub fn find<'l, F>(arr: &LeanBound<'l, Self>, pred: F) -> Option<LeanBound<'l, LeanAny>>
    where
        F: Fn(&LeanBound<'l, LeanAny>) -> bool,
    {
        let size = Self::size(arr);
        for i in 0..size {
            if let Some(elem) = Self::get(arr, i) {
                if pred(&elem) {
                    return Some(elem);
                }
            }
        }
        None
    }

    /// Find index of first element satisfying a predicate.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.findIdx?` in Lean4.
    #[allow(non_snake_case)]
    pub fn findIdx<'l, F>(arr: &LeanBound<'l, Self>, pred: F) -> Option<usize>
    where
        F: Fn(&LeanBound<'l, LeanAny>) -> bool,
    {
        let size = Self::size(arr);
        for i in 0..size {
            if let Some(elem) = Self::get(arr, i) {
                if pred(&elem) {
                    return Some(i);
                }
            }
        }
        None
    }

    /// Filter the array with a predicate.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.filter` in Lean4.
    pub fn filter<'l, F>(arr: LeanBound<'l, Self>, pred: F) -> LeanResult<LeanBound<'l, Self>>
    where
        F: Fn(&LeanBound<'l, LeanAny>) -> bool,
    {
        let lean = arr.lean_token();
        let size = Self::size(&arr);
        let mut result = Self::empty(lean)?;

        for i in 0..size {
            if let Some(elem) = Self::get(&arr, i) {
                if pred(&elem) {
                    result = Self::push(result, elem)?;
                }
            }
        }
        Ok(result)
    }

    /// Map a function over the array.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.map` in Lean4.
    pub fn map<'l, F>(arr: LeanBound<'l, Self>, f: F) -> LeanResult<LeanBound<'l, Self>>
    where
        F: Fn(LeanBound<'l, LeanAny>) -> LeanResult<LeanBound<'l, LeanAny>>,
    {
        let lean = arr.lean_token();
        let size = Self::size(&arr);
        let mut result = Self::emptyWithCapacity(lean, size)?;

        for i in 0..size {
            if let Some(elem) = Self::get(&arr, i) {
                let mapped = f(elem)?;
                result = Self::push(result, mapped)?;
            }
        }
        Ok(result)
    }

    /// Fold left over the array.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.foldl` in Lean4.
    pub fn foldl<'l, A, F>(arr: &LeanBound<'l, Self>, init: A, f: F) -> A
    where
        F: Fn(A, LeanBound<'l, LeanAny>) -> A,
    {
        let size = Self::size(arr);
        let mut acc = init;
        for i in 0..size {
            if let Some(elem) = Self::get(arr, i) {
                acc = f(acc, elem);
            }
        }
        acc
    }

    /// Fold right over the array.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.foldr` in Lean4.
    pub fn foldr<'l, A, F>(arr: &LeanBound<'l, Self>, init: A, f: F) -> A
    where
        F: Fn(LeanBound<'l, LeanAny>, A) -> A,
    {
        let size = Self::size(arr);
        let mut acc = init;
        for i in (0..size).rev() {
            if let Some(elem) = Self::get(arr, i) {
                acc = f(elem, acc);
            }
        }
        acc
    }

    /// Count elements satisfying a predicate.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.countP` in Lean4.
    #[allow(non_snake_case)]
    pub fn countP<'l, F>(arr: &LeanBound<'l, Self>, pred: F) -> usize
    where
        F: Fn(&LeanBound<'l, LeanAny>) -> bool,
    {
        let size = Self::size(arr);
        let mut count = 0;
        for i in 0..size {
            if let Some(elem) = Self::get(arr, i) {
                if pred(&elem) {
                    count += 1;
                }
            }
        }
        count
    }

    /// Flatten an array of arrays.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.flatten` in Lean4.
    ///
    /// # Safety
    /// All elements must be arrays.
    pub unsafe fn flatten<'l>(arr: LeanBound<'l, Self>) -> LeanResult<LeanBound<'l, Self>> {
        let lean = arr.lean_token();
        let size = Self::size(&arr);

        // Pre-calculate total capacity to avoid repeated reallocations.
        let mut total = 0;
        for i in 0..size {
            if let Some(inner) = Self::get(&arr, i) {
                let inner_arr: LeanBound<'l, Self> = inner.cast();
                total += Self::size(&inner_arr);
            }
        }

        let mut result = Self::emptyWithCapacity(lean, total)?;
        for i in 0..size {
            if let Some(inner) = Self::get(&arr, i) {
                let inner_arr: LeanBound<'l, Self> = inner.cast();
                let inner_size = Self::size(&inner_arr);
                for j in 0..inner_size {
                    if let Some(elem) = Self::get(&inner_arr, j) {
                        result = Self::push_unchecked(result, elem)?;
                    }
                }
            }
        }
        Ok(result)
    }

    /// Zip two arrays together.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.zip` in Lean4.
    ///
    /// Returns an array of pairs. The result length is the minimum of the two input lengths.
    pub fn zip<'l>(
        xs: &LeanBound<'l, Self>,
        ys: &LeanBound<'l, Self>,
    ) -> LeanResult<LeanBound<'l, Self>> {
        let lean = xs.lean_token();
        let size = Self::size(xs).min(Self::size(ys));
        let mut result = Self::emptyWithCapacity(lean, size)?;

        for i in 0..size {
            if let (Some(x), Some(y)) = (Self::get(xs, i), Self::get(ys, i)) {
                // Create a pair (constructor 0 with 2 fields)
                unsafe {
                    let pair_ptr = ffi::lean_alloc_ctor(0, 2, 0);
                    ffi::lean_ctor_set(pair_ptr, 0, x.into_ptr());
                    ffi::lean_ctor_set(pair_ptr, 1, y.into_ptr());
                    let pair = LeanBound::from_owned_ptr(lean, pair_ptr);
                    result = Self::push(result, pair)?;
                }
            }
        }
        Ok(result)
    }

    /// Check if xs is a prefix of ys.
    ///
    /// # Lean4 Reference
    /// Corresponds to `Array.isPrefixOf` in Lean4.
    #[allow(non_snake_case)]
    pub fn isPrefixOf<'l, F>(xs: &LeanBound<'l, Self>, ys: &LeanBound<'l, Self>, eq: F) -> bool
    where
        F: Fn(&LeanBound<'l, LeanAny>, &LeanBound<'l, LeanAny>) -> bool,
    {
        let xs_size = Self::size(xs);
        let ys_size = Self::size(ys);

        if xs_size > ys_size {
            return false;
        }

        for i in 0..xs_size {
            if let (Some(x), Some(y)) = (Self::get(xs, i), Self::get(ys, i)) {
                if !eq(&x, &y) {
                    return false;
                }
            }
        }
        true
    }
}

// Implement Debug for convenient printing
impl<'l> std::fmt::Debug for LeanBound<'l, LeanArray> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LeanArray(size: {})", LeanArray::size(self))
    }
}
