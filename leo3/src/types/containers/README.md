# Container Types Implementation Status

## Current Status

The container wrappers are stable on the default feature set (requires Lean >= 4.22):

- `HashMap`, `HashSet`, and `RBMap` all use Lean's real runtime
  representation and real container operations.
- the supported key matrix is intentionally narrow and explicit
- the `lean_4_22` cfg gate remains to reflect the ABI requirement

## FFI Bindings

We have created FFI bindings for Lean's container functions in `leo3-ffi/src/`:
- `hashmap.rs` - Std.HashMap FFI declarations
- `hashset.rs` - Std.HashSet FFI declarations
- `rbmap.rs` - Lean.RBMap FFI declarations

These bindings declare the functions available in Lean's shared library (e.g., `l_Std_HashMap_insert`, `l_Std_HashSet_contains`, etc.).

## The Challenge: Runtime Instance Sources

Using these FFI functions requires passing Lean runtime comparison/hash objects:
- `HashMap` requires `BEq α` and `Hashable α` instances
- `HashSet` requires `BEq α` and `Hashable α` instances
- `RBMap` requires a compare closure `α → α → Ordering`

These objects must be obtained at runtime. There are several approaches to solve this:

### Approach 1: Lean-side Wrappers (Recommended)

Create Lean functions that bundle typeclass instances with operations:

```lean
-- In Lean:
@[export my_hashmap_nat_string_empty]
def myHashMapNatStringEmpty : HashMap Nat String :=
  HashMap.empty

@[export my_hashmap_nat_string_insert]
def myHashMapNatStringInsert (m : HashMap Nat String) (k : Nat) (v : String) : HashMap Nat String :=
  m.insert k v
```

Then call these from Rust:
```rust
extern "C" {
    fn my_hashmap_nat_string_empty() -> lean_obj_res;
    fn my_hashmap_nat_string_insert(m: lean_obj_arg, k: lean_obj_arg, v: lean_obj_arg) -> lean_obj_res;
}
```

### Approach 2: Eval-based Approach

Use Lean's evaluation system to execute Lean code directly:
```rust
// Pseudo-code:
let map = lean_eval("HashMap.empty : HashMap Nat String")?;
let map = lean_eval(&format!("({:?}).insert {} {}", map, key, value))?;
```

### Approach 3: Typeclass Instance Registry

Create a registry system that caches typeclass instances for common types:
```rust
pub struct TypeclassInstances {
    beq_nat: LeanBound<'static, BEqType<LeanNat>>,
    hashable_nat: LeanBound<'static, HashableType<LeanNat>>,
    // ... etc
}
```

## What Landed

### `RBMap`

`leo3/src/types/containers/rbmap.rs` now uses Lean's real runtime ABI:

- `empty` uses `l_Lean_RBMap_empty`
- `insert`, `find?`, `contains`, and `erase` use the `_redArg` entry points
- read-only queries clone the map pointer first because Lean's traversal helpers
  consume the tree argument during traversal
- `to_list`, `min`, `max`, and `size` now reflect real Lean behavior

Current supported key matrix:

- `LeanNat`
- `LeanInt`
- `LeanString`
- `LeanInt8`
- `LeanInt16`
- `LeanInt32`
- `LeanInt64`
- `LeanUInt8`
- `LeanUInt16`
- `LeanUInt32`
- `LeanUInt64`

Runtime coverage now includes:

- duplicate insert / dedup semantics
- replacement semantics for existing keys
- string-key support
- fixed-width signed integer key support (`Int8`–`Int64`)
- fixed-width unsigned integer key support (`UInt8`–`UInt64`)
- cross-family parity checks for the supported string-key,
  fixed-width signed integer key, and fixed-width unsigned integer key paths

The implementation uses exported compare closures such as `l_instOrdNat`,
`l_String_instOrd`, `l_Int8_instOrd`, `l_Int16_instOrd`, `l_Int32_instOrd`,
`l_Int64_instOrd`, `l_UInt8_instOrd`, `l_UInt16_instOrd`, `l_UInt32_instOrd`,
and `l_UInt64_instOrd`. This is intentionally narrow but real.

Note: `RBMap` does not support `LeanFloat` / `LeanFloat32` keys because Lean has
no total order (`Ord`) for floats (NaN breaks totality).

### `HashMap` / `HashSet`

`leo3/src/types/containers/hashmap.rs` and
`leo3/src/types/containers/hashset.rs` now use Lean's real runtime ABI too:

- empty construction uses reduced-arity `emptyWithCapacity` entry points
- insert / contains / get / erase use reduced-arity wrappers that accept a
  `BEq` closure and a `Hashable` closure directly
- Leo3 constructs the `BEq` closure from exported boxed `DecidableEq` functions
  such as `l_instDecidableEqNat___boxed` through Lean's
  `l_instBEqOfDecidableEq___redArg` helper, matching the compiler-generated
  runtime representation for `BEq.ofDecidableEq`
- Leo3 passes owned references to exported `Hashable` closures such as
  `l_instHashableNat`, matching the C ABI ownership contract for
  `lean_obj_arg`
- read-only queries clone the map/set pointer first because the Lean runtime
  helpers consume the structure argument during traversal

Current supported key matrix:

- `LeanNat`
- `LeanInt`
- `LeanString`
- `LeanInt8`
- `LeanInt16`
- `LeanInt32`
- `LeanInt64`
- `LeanUInt8`
- `LeanUInt16`
- `LeanUInt32`
- `LeanUInt64`
- `LeanFloat`
- `LeanFloat32`

Fixed-width signed wrappers (`LeanInt8`–`LeanInt64`) and unsigned wrappers
(`LeanUInt8`–`LeanUInt64`) now use Lean's unboxed scalar ABI representation,
aligned with Lean's container typeclass instances. This allows them to be used
directly as container keys without additional representation work.

Floating-point keys (`LeanFloat`, `LeanFloat32`) use Lean's exported `BEq`
instances (`l_instBEqFloat`, `l_instBEqFloat32`), which implement IEEE 754
bitwise equality. Leo3 supplies a matching `Hashable` closure that hashes the
IEEE 754 bit pattern (normalizing `+0.0` / `-0.0` to one bucket so equal values
land in the same bucket). NaN keys are stored but never found: `NaN != NaN`
under IEEE 754 equality, so `contains` / `find` / `get` return false / none for
NaN keys, and repeated NaN inserts into a `HashSet` create duplicate entries.
Floats do not support `RBMap` keys because Lean has no total order (`Ord`) for
them.

Current runtime tests exercise:

- duplicate insert behavior for `HashSet`
- replacement semantics for `HashMap` / `RBMap`
- string-key support across all three families
- fixed-width signed integer key support across all three families
- fixed-width unsigned integer key support across all three families
- floating-point key support across `HashMap` and `HashSet`
- NaN key semantics for `LeanFloat` / `LeanFloat32`
- `HashSet<String>` duplicate-insert coverage as a normal runtime test, not an
  ignored one
- parity checks for equivalent final states across `HashMap`, `HashSet`, and
  `RBMap`

## Recommended Next Steps

1. **For specific use cases**: Use Approach 1 - create Lean wrappers for the exact container types you need

2. **For general library support**: Implement Approach 3 - create a typeclass instance registry for common types

3. **Current implementation**:
   - all three container families now have narrow real implementations
   - widening the supported matrix should happen only when the instance source
     remains explicit and testable

## Available FFI Functions

All functions from the Lean standard library are available through the FFI bindings:

### HashMap
- `l_Std_HashMap_empty` / `l_Std_HashMap_emptyWithCapacity`
- `l_Std_DHashMap_insert` / `l_Std_DHashMap_insertIfNew`
- `l_Std_HashMap_contains`
- `l_Std_HashMap_erase`
- `l_Std_HashMap_alter`
- `l_Std_HashMap_filter`
- `l_Std_HashMap_fold` / `l_Std_HashMap_foldM`
- `l_Std_DHashMap_toList` / `l_Std_DHashMap_toArray`
- ... (see `leo3-ffi/src/hashmap.rs` for full list)

### HashSet
- `l_Std_HashSet_empty` / `l_Std_HashSet_emptyWithCapacity`
- `l_Std_HashSet_insert`
- `l_Std_HashSet_contains`
- `l_Std_HashSet_erase`
- `l_Std_HashSet_filter`
- `l_Std_HashSet_all` / `l_Std_HashSet_any`
- `l_Std_HashSet_fold` / `l_Std_HashSet_foldM`
- `l_Std_HashSet_toList` / `l_Std_HashSet_toArray`
- ... (see `leo3-ffi/src/hashset.rs` for full list)

### RBMap
- `l_Lean_RBMap_empty`
- `l_Lean_RBMap_insert`
- `l_Lean_RBMap_find_x3f` (`find?`) / `l_Lean_RBMap_findD`
- `l_Lean_RBMap_contains`
- `l_Lean_RBMap_erase`
- `l_Lean_RBMap_filter` / `l_Lean_RBMap_filterMap`
- `l_Lean_RBMap_fold` / `l_Lean_RBMap_foldM`
- `l_Lean_RBMap_toList` / `l_Lean_RBMap_toArray`
- ... (see `leo3-ffi/src/rbmap.rs` for full list)

## Function Name Consistency

All FFI function names match the Lean reference manual:
- Lean: `HashMap.insert` → FFI: `l_Std_HashMap_insert` (but uses DHashMap internally)
- Lean: `HashSet.contains` → FFI: `l_Std_HashSet_contains`
- Lean: `RBMap.find?` → FFI: `l_Lean_RBMap_find_x3f` (? encoded as _x3f)
- Lean: `RBMap.find!` → FFI: `l_Lean_RBMap_find_x21` (! encoded as _x21)

The naming convention is: `l_` + namespace path with underscores, and special characters encoded in hex.
