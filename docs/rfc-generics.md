# RFC: Generics Support in `#[leanfn]` / `#[leanclass]`

Status: design record (2026-07-27). Tracks W-77.

## Problem

Users want to expose generic Rust functions to Lean:

```rust
#[leanfn]
fn add<T: Add<Output = T>>(a: T, b: T) -> T {
    a + b
}
```

Lean's `extern` functions are monomorphic — each exported symbol has a fixed
C ABI signature. There is no runtime polymorphism or type-parameter dispatch
on the Lean side. A generic Rust function cannot be exported as a single Lean
symbol.

## Why General Generics Are Not Feasible

1. **Lean extern is monomorphic.** A Lean `extern "symbol"` declaration binds
   one symbol to one concrete type signature. There is no mechanism for Lean to
   dispatch a single extern symbol over multiple type instantiations at
   runtime.

2. **C ABI has no type parameters.** The generated C wrapper
   (`extern "C" fn ...`) must have a fixed parameter and return type. Rust
   generics are erased at compile time; the FFI boundary cannot carry type
   information.

3. **Lean's typeclass resolution is compile-time.** Even if Rust could
   generate a dispatch table, Lean would need to select the correct
   instantiation at elaboration time, which requires the Lean side to know
   all concrete types in advance — exactly what monomorphization provides.

4. **Unbounded instantiation.** A generic function has infinitely many
   possible instantiations. Leo3 cannot generate wrappers for types it does
   not know about at compile time.

## Monomorphization Subset: Feasible

A *bounded* monomorphization subset is feasible: the user explicitly declares
which concrete instantiations to export.

### Proposed Syntax

```rust
#[leanfn(concrete(u64, name = "add_u64"), concrete(i64, name = "add_i64"))]
fn add<T: Add<Output = T> + IntoLean + FromLean>(a: T, b: T) -> T {
    a + b
}
```

Each `concrete(Ty, name = "...")` annotation instructs the macro to generate
a separate, fully monomorphized C ABI wrapper:

- `add_u64`: `extern "C" fn(u64, u64) -> u64`
- `add_i64`: `extern "C" fn(i64, i64) -> i64`

Plus corresponding metadata entries and Lean-visible declarations.

### Design Constraints

1. **Explicit enumeration.** The user must list every concrete type. No
   inference or blanket generation.

2. **Unique names.** Each `concrete` instance must have a distinct
   `name = "..."` to avoid symbol collisions.

3. **Trait bounds must be satisfied.** The concrete type must satisfy all
   trait bounds on the generic parameter, including `IntoLean` / `FromLean`
   for conversion at the FFI boundary.

4. **Metadata schema extension.** Each concrete instance appears as a
   separate export in `__leo3_module_metadata()`, with its own FFI symbol
   name and Lean-visible type signature.

5. **`#[leanclass]` interaction.** Generic structs and impl blocks remain
   unsupported (see compile-fail tests in `leo3/tests/ui/`). The
   monomorphization subset applies only to `#[leanfn]` free functions in the
   initial design.

### What This Does Not Cover

- Generic `#[leanclass]` structs (`struct Foo<T> { ... }`)
- Generic impl blocks (`impl<T> Foo { ... }`)
- Generic methods inside non-generic impls (`fn method<T>(&self, x: T)`)
- Higher-kinded or lifetime-generic parameters

These remain compile errors with clear diagnostics (see
`leo3/tests/ui/leanclass_generic_*.rs`).

## Evaluation

### Arguments for the monomorphization subset

1. Covers the common use case: "I have a generic function but only need 2-3
   concrete versions in Lean."
2. Zero runtime cost — each wrapper is a direct monomorphized call.
3. Explicit and predictable — no hidden code generation.
4. Aligns with how C++ template instantiation and Rust's own
   monomorphization work conceptually.

### Arguments against / risks

1. Adds macro complexity for a potentially narrow use case.
2. Users must understand the FFI monomorphization model.
3. Trait bound checking at macro expansion time requires careful error
   messages.
4. Does not generalize to `#[leanclass]` without significant additional
   design work.

## Decision

**General generics are not feasible and will not be supported.**

**A monomorphization subset (`concrete(Ty, name = "...")`) is feasible and
should be implemented when there is user demand.** The design is recorded here
for future reference. Implementation is tracked in W-77.

Until then, users who need multiple type instantiations should write separate
non-generic functions:

```rust
#[leanfn]
fn add_u64(a: u64, b: u64) -> u64 { a + b }

#[leanfn]
fn add_i64(a: i64, b: i64) -> i64 { a + b }
```

## References

- `leo3/tests/ui/leanclass_generic_struct.rs` — generic struct rejection
- `leo3/tests/ui/leanclass_generic_impl.rs` — generic impl rejection
- `leo3/tests/ui/leanclass_generic_method.rs` — generic method rejection
- `leo3/tests/ui/leanclass_unsupported_generic_type.rs` — unsupported
  generic type in declarations
- `docs/contracts.md` — declaration grammar and rejection list
- `leo3-macros/src/lib.rs` — `#[leanfn]` macro entry point
- `leo3-binding-ir/` — semantic analysis and metadata schema
