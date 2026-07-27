# External Object Borrow Extraction — Design Exploration

Status: design record (2026-07-27). Tracks W-55.

## Problem

`FromLean` for external objects is clone-based:

```rust
impl<'l, T: ExternalClass + Clone> FromLean<'l> for T {
    type Source = LeanExternalType<T>;
    fn from_lean(obj: &LeanBound<'l, Self::Source>) -> LeanResult<Self> {
        let external: &LeanExternal<T> = unsafe { std::mem::transmute(obj) };
        Ok(external.get_ref().clone())
    }
}
```

Performance-sensitive callers may want zero-copy extraction through a generic
trait rather than the wrapper-layer API.

## Current Borrow Surface

Borrow-first access already exists on the wrapper layer
(`leo3/src/external.rs`):

| Method | Signature | Cost | Precondition |
| --- | --- | --- | --- |
| `get_ref()` / `borrow()` | `&self -> &T` | zero-copy | correct type `T` |
| `try_get_mut()` | `&mut self -> Option<&mut T>` | zero-copy | `lean_is_exclusive()` |
| `try_take_inner()` | `&mut self -> Option<T>` | move, no clone | `lean_is_exclusive()` |
| `std::borrow::Borrow<T>` | `&self -> &T` | zero-copy | correct type `T` |

These are already zero-copy. The question is whether a *trait-level* borrow
extraction (e.g. `FromLeanBorrowed`) adds value.

## Lean External Object Lifecycle

- An external object is a small Lean object (`lean_external_object`) with an
  `m_class` pointer and an `m_data` pointer to a heap-allocated `Box<T>`.
- Lifetime is managed by Lean's reference-counting GC. The finalizer
  (`finalize_external<T>`) runs when RC reaches 0 and drops the `Box<T>`.
- `lean_is_exclusive(obj)` returns true when RC == 1 (unique ownership).
- `lean_inc` / `lean_dec` adjust the reference count; shared objects (RC > 1)
  are immutable from Rust's perspective unless the caller ensures exclusivity.

## Rust Borrow Checker Compatibility

A `&T` obtained via `get_ref()` is valid as long as the owning `LeanBound<'l,
LeanExternalType<T>>` is alive. The `'l` lifetime ties the bound object to the
Lean runtime token, and the `&self` borrow on `LeanBound` ensures the Rust
reference cannot outlive the Lean object. This is sound.

A hypothetical trait:

```rust
pub trait FromLeanBorrowed<'l>: Sized {
    type Source;
    fn from_lean_borrowed<'a>(obj: &'a LeanBound<'l, Self::Source>) -> &'a Self;
}
```

is implementable for `T: ExternalClass` with `Source = LeanExternalType<T>`:

```rust
impl<'l, T: ExternalClass> FromLeanBorrowed<'l> for T {
    type Source = LeanExternalType<T>;
    fn from_lean_borrowed<'a>(obj: &'a LeanBound<'l, Self::Source>) -> &'a Self {
        let external: &LeanExternal<T> = unsafe { std::mem::transmute(obj) };
        external.get_ref()
    }
}
```

This compiles and is memory-safe. The question is whether it earns its place.

## Evaluation

### Arguments for a trait

1. Generic code could write `fn extract<T: FromLeanBorrowed>(...)` and avoid
   clones uniformly.
2. Mirrors PyO3's `FromPyObjectBound` pattern.

### Arguments against

1. **Narrow applicability.** Only external objects store a Rust `T` inline.
   All other `FromLean` types (`String`, `Vec<T>`, scalars, containers)
   construct a new Rust value from the Lean representation — there is no `&T`
   to borrow. A trait that only one family of types implements is a niche
   abstraction, not a general contract.

2. **The wrapper API already covers the use case.** Callers who know they are
   dealing with an external object can call `LeanExternal::borrow()` directly.
   The zero-copy path is already available; a trait adds indirection without
   new capability.

3. **`Clone` bound is deliberate.** The `FromLean` contract is "extract an
   owned Rust value from a Lean object." For external objects this means
   cloning the inner value, which is the correct semantic when the Lean object
   may be shared (RC > 1). Borrowing through a generic trait would silently
   produce a reference tied to the Lean object's lifetime, which is a
   different (and more restrictive) contract.

4. **Macro integration cost.** `#[leanfn]` and `#[leanclass]` wrappers
   generate code against `FromLean`. Adding a parallel `FromLeanBorrowed`
   path would require the macros to choose between clone and borrow at codegen
   time, adding complexity for a narrow gain.

5. **PyO3 comparison is not 1:1.** PyO3's `FromPyObjectBound` exists because
   Python objects are always heap-allocated and GIL-bound. Lean external
   objects are the *only* Lean type that embeds a Rust value; the analogy
   does not extend to the rest of the conversion matrix.

### Mutable borrow extraction

A `FromLeanBorrowedMut` variant would additionally require an exclusivity
check (`lean_is_exclusive`) at runtime, making it fallible. This is already
exposed as `try_get_mut()`. A trait-level version would need to return
`Option<&mut Self>` or `LeanResult<&mut Self>`, which is awkward for generic
code and does not improve on the direct API.

## Decision

**Do not add `FromLeanBorrowed` or a similar trait at this time.**

The clone-based `FromLean` contract for external objects is the right generic
boundary. Zero-copy borrow access is already available through the
`LeanExternal<T>` wrapper methods (`borrow()`, `get_ref()`, `try_get_mut()`,
`try_take_inner()`), which is the correct abstraction level for this
capability.

If a future use case demonstrates that generic code genuinely needs
trait-level borrow extraction across multiple type families (not just external
objects), the design above can be revisited. Until then, the wrapper-layer API
is sufficient and the `FromLean` clone contract remains the stable surface.

## References

- `leo3/src/external.rs` — `LeanExternal`, `ExternalClass`, `FromLean` impl
- `leo3/src/conversion.rs` — `FromLean` / `IntoLean` trait definitions
- `leo3/src/instance.rs` — `LeanBound`, `LeanBorrowed`
- `leo3-ffi/src/inline/external.rs` — `lean_alloc_external`,
  `lean_get_external_data`
- `docs/contracts.md` — conversion matrix and external object contract
