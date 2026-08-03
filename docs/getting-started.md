# Getting Started with Leo3

This tutorial walks you through installing Leo3, writing your first exported
function and class, and running the project's test suite.

## Prerequisites

- **Rust** ≥ 1.88 (install via [rustup](https://rustup.rs))
- **Lean 4.25.2** (install via [elan](https://github.com/leanprover/elan))

```bash
# Install elan + Lean (if you don't have them yet)
curl https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh -sSf | sh
elan toolchain install leanprover/lean4:v4.25.2

# Verify
lean --version   # Lean (version 4.25.2, ...)
rustc --version  # rustc 1.88.x
```

## Installation

Add `leo3` to your `Cargo.toml`:

```toml
[dependencies]
leo3 = "0.3.1"
```

To use the procedural macros (`#[leanfn]`, `#[leanclass]`, etc.), enable the
`macros` feature:

```toml
[dependencies]
leo3 = { version = "0.3.1", features = ["macros"] }
```

## Hello, Lean!

Every Leo3 program initializes the Lean runtime and enters a `with_lean` scope
that gives you a `Lean<'l>` token — proof that the runtime is ready.

```rust,no_run
use leo3::prelude::*;

fn main() -> LeanResult<()> {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        let s = LeanString::mk(lean, "Hello from Rust!")?;
        println!("{}", LeanString::cstr(&s)?);

        let n = LeanNat::from_usize(lean, 42)?;
        println!("{}", LeanNat::to_usize(&n)?);
        Ok(())
    })
}
```

Key points:

- `prepare_freethreaded_lean()` pays the one-time initialization cost eagerly.
- `with_lean(|lean| { ... })` attaches the current thread and hands you the
  `Lean<'l>` token. All Lean object operations require this token.

## Your First `#[leanfn]`

`#[leanfn]` exports a Rust function so it can be called from Lean through the
FFI boundary. Enable the `macros` feature, then annotate any function:

```rust,no_run
use leo3::prelude::*;

#[leo3::leanfn]
fn add(a: u64, b: u64) -> u64 {
    a + b
}

fn main() -> LeanResult<()> {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        let a = LeanUInt64::mk(lean, 20)?;
        let b = LeanUInt64::mk(lean, 22)?;

        let result_ptr = unsafe { add(a.into_ptr(), b.into_ptr()) };
        let result = LeanBound::<LeanUInt64>::from_owned_ptr(lean, result_ptr);
        println!("20 + 22 = {}", LeanUInt64::to_u64(&result));
        Ok(())
    })
}
```

The macro generates:

- An `extern "C"` wrapper with the Lean calling convention.
- A metadata accessor (`__leo3_metadata_add()`) describing the binding schema.

### Supported parameter types

Beyond the base conversion matrix (`u8`–`u64`, `i8`–`i64`, `f32`, `f64`,
`bool`, `char`, `String`, `Vec<T>`, `Option<T>`, `Result<T, E>`, pairs),
`#[leanfn]` also accepts borrowed forms: `&str`, `&String`, `&[T]`, `&[T; N]`,
`&Vec<T>`, `&[u8]`, `&Vec<u8>`.

## Your First `#[leanclass]`

`#[leanclass]` exposes a Rust struct as a Lean external class with
auto-generated FFI wrappers and Lean declaration strings.

```rust,no_run
use leo3::prelude::*;

#[derive(Clone)]
#[leo3::leanclass]
struct Counter {
    value: i64,
}

#[leo3::leanclass]
impl Counter {
    fn new() -> Self {
        Counter { value: 0 }
    }

    fn get(&self) -> i64 {
        self.value
    }

    fn increment(&mut self) {
        self.value += 1;
    }
}

fn main() -> LeanResult<()> {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|lean| {
        println!("Lean class declaration:\n{}", COUNTER_LEAN_CLASS_DECL);
        println!("Lean method declarations:\n{}", COUNTER_LEAN_METHODS_DECL);
        Ok(())
    })
}
```

### Receiver semantics

| Rust receiver | Lean-visible behavior |
|---|---|
| *(none)* | Static constructor: `A -> ... -> R` |
| `&self` | Shared borrow: `Self -> A -> ... -> R` |
| `&mut self`, returns `()` | Copy-on-write mutation: `Self -> A -> ... -> Self` |
| `&mut self`, returns `R` | Copy-on-write + value: `Self -> A -> ... -> Prod Self R` |

`&mut self` and `self` methods require the struct to implement `Clone`.

## Modules with `#[leanmodule]`

Group exported functions into a Lean module:

```rust,no_run
use leo3::prelude::*;

#[leo3::leanmodule(name = "MyMath")]
mod my_math {
    use leo3::prelude::*;

    #[leo3::leanfn(name = "my_math_add")]
    pub fn add(a: u64, b: u64) -> u64 {
        a + b
    }
}

fn main() -> LeanResult<()> {
    leo3::prepare_freethreaded_lean();

    leo3::with_lean(|_lean| {
        println!("Rust call: add(3, 4) = {}", my_math::add(3, 4));
        Ok(())
    })
}
```

## Running the Example

The repository ships a full end-to-end example combining all three macros:

```bash
cargo run --example macro_pipeline --features macros
```

## Generating Lean-side Declarations

`#[leanmodule]` and `#[leanclass]` embed structured metadata directly into the
compiled `cdylib`. The `leo3-codegen` CLI reads that metadata from the binary
(no Lean runtime required) and emits the matching Lean 4 `extern` declaration
files:

```bash
# Build your cdylib
cargo build --release

# Generate .lean files into lean/MyLib/
cargo run -p leo3-codegen -- \
    target/release/libmy_crate.so -o lean/MyLib
```

For a module declared with `#[leanmodule(name = "FixtureModule")]` containing
`#[leanfn]` exports, this produces `FixtureModule.lean`:

```lean
-- Generated by leo3-codegen. Do not edit.
-- Module: FixtureModule

@[extern "fixture_add"] opaque fixture_add : UInt64 → UInt64 → UInt64

@[extern "fixture_banner"] opaque fixture_banner : String → Int32 → String
```

For a `#[leanclass]` struct, this produces `<ClassName>.lean` with the class
type declaration and one `@[extern]` declaration per method:

```lean
-- Generated by leo3-codegen. Do not edit.
-- Class: FixtureCounter

opaque FixtureCounter.ffi : NonemptyType
def FixtureCounter : Type := FixtureCounter.ffi.val
instance : Nonempty FixtureCounter := FixtureCounter.ffi.property

@[extern "__lean_ffi_FixtureCounter_new"] opaque FixtureCounter.new : Int32 → FixtureCounter
@[extern "__lean_ffi_FixtureCounter_get"] opaque FixtureCounter.get : FixtureCounter → Int32
@[extern "__lean_ffi_FixtureCounter_increment"] opaque FixtureCounter.increment : FixtureCounter → FixtureCounter
```

The class type goes through `NonemptyType` (the same pattern the standard
library uses for `IO.RealWorld`): a bare `opaque FixtureCounter : Type` would
fail to elaborate, because every `opaque` declaration needs an `Inhabited` or
`Nonempty` instance for its type.

Generated declarations follow Lean's extern calling convention exactly, so
the same cdylib symbols work both for direct calls and higher-order use:
fixed-width scalars (`UInt8`–`UInt64`, `Int8`–`Int64`, `USize`, `ISize`,
`Float32`, `Float`, `Bool`, `Char`) cross the boundary unboxed as raw C
values, while `String`, containers, `Option`/`Except`/`Prod` and class
objects cross as boxed `lean_object*` values.

The tool discovers metadata by scanning the cdylib's symbol table for
`__leo3_module_metadata_json_*` and `__leo3_class_metadata_json_*` symbols, so
it works on ELF (Linux), Mach-O (macOS), and PE (Windows) binaries.

For the full install-to-Lake-project walkthrough, cross-platform extraction
details, and current limitations, see the
[`leo3-codegen` guide](codegen.md).

## Running Tests

Leo3 uses a tiered test strategy. Most tests can run **without** a Lean
installation by setting `LEO3_NO_LEAN=1`:

```bash
# Compile-only smoke tests (no Lean required)
LEO3_NO_LEAN=1 cargo test --locked --workspace --exclude leo3 --lib
LEO3_NO_LEAN=1 cargo test --locked -p leo3 --no-default-features --test test_features
LEO3_NO_LEAN=1 cargo test --locked -p leo3 --features macros --test test_compile_error

# Runtime tests (requires Lean 4.25.2)
cargo test --locked -p leo3 --features runtime-tests \
  --test basic --test nat_ops --test string_ops --test array_ops

# Macro runtime tests
cargo test --locked -p leo3 --features "macros,runtime-tests" \
  --test test_leanfn_macro --test test_leanclass --test test_macro_pipeline

# Full suite
cargo test --locked --all-features --workspace
```

See [TESTING.md](../TESTING.md) for the complete CI tier map.

## Lake Integration (Lean-side)

A Lean project can call Rust code compiled as a `cdylib` through Lake's native
linking support. The repository ships two working templates:

- `examples/lake-integration/` — hand-written raw `extern "C"` functions with
  hand-written Lean declarations (the minimal path, covered below).
- `examples/class-integration/` — the macro pipeline: `#[leanclass]` /
  `#[leanfn]` / `#[leanmodule]` plus `leo3-codegen`-generated declarations,
  covering external objects, methods and `#[getter]` / `#[setter]` accessors.

### Project layout

```text
examples/lake-integration/
├── native/                  # Rust cdylib
│   ├── Cargo.toml
│   ├── build.rs
│   └── src/lib.rs
└── lean/                    # Lean lake package
    ├── lakefile.lean
    ├── lean-toolchain
    ├── Leo3Example/
    │   ├── NativeMath.lean  # @[extern] declarations
    │   └── Accumulator.lean
    ├── Leo3Example.lean     # library root (imports)
    └── Main.lean            # executable entry point
```

### Step 1: Write the Rust cdylib

Export plain `extern "C"` functions. Scalar types (`u64`, `i64`) pass directly;
`String` values are `lean_object*` pointers that you manipulate through
`leo3::ffi` helpers:

```rust
use std::ffi::CStr;
use std::os::raw::c_char;
use leo3::ffi::inline::{lean_dec, lean_string_cstr};
use leo3::ffi::object::{lean_obj_arg, lean_obj_res};
use leo3::ffi::string::lean_mk_string;

#[no_mangle]
pub extern "C" fn native_add(a: u64, b: u64) -> u64 {
    a + b
}

#[no_mangle]
pub unsafe extern "C" fn native_greet(name: lean_obj_arg) -> lean_obj_res {
    let cstr = lean_string_cstr(name);
    let rust_str = CStr::from_ptr(cstr).to_string_lossy().into_owned();
    lean_dec(name);
    let greeting = format!("Hello, {rust_str}! (from Rust)");
    let c_greeting = std::ffi::CString::new(greeting).unwrap();
    lean_mk_string(c_greeting.as_ptr() as *const c_char)
}
```

Build with `LEO3_NO_LEAN=1` so the cdylib does not link `libleanshared` itself
(the host Lean executable provides those symbols at runtime):

```bash
cd native
LEO3_NO_LEAN=1 cargo build --release
```

### Step 2: Write Lean `@[extern]` declarations

Each Rust function needs a matching Lean declaration. The C ABI must agree:

| Lean type | C type in Rust |
|-----------|----------------|
| `UInt64` | `u64` |
| `Int64` | `i64` |
| `Float` | `f64` |
| `Bool` | `u8` |
| `String` | `lean_obj_arg` / `lean_obj_res` |
| `Array T` | `lean_obj_arg` / `lean_obj_res` |

```lean
-- Leo3Example/NativeMath.lean
@[extern "native_add"] opaque native_add : UInt64 → UInt64 → UInt64
@[extern "native_greet"] opaque native_greet : String → String
```

### Step 3: Configure the lakefile

Use `moreLinkArgs` to point the linker at the cdylib:

```lean
import Lake
open Lake DSL

package «MyProject» where
  leanOptions := #[⟨`autoImplicit, false⟩]

@[default_target]
lean_lib «MyProject» where
  moreLinkArgs := #["-L", "../native/target/release", "-l", "my_native_lib"]

lean_exe «app» where
  root := `Main
  moreLinkArgs := #["-L", "../native/target/release", "-l", "my_native_lib"]
```

### Step 4: Build and run

```bash
cd lean
lake build app
LD_LIBRARY_PATH=../native/target/release .lake/build/bin/app
```

Expected output:

```text
native_add(20, 22) = 42
native_greet("Lean") = Hello, Lean! (from Rust)
```

The `#[leanfn]` / `#[leanclass]` macros generate wrappers with this same
mixed ABI (scalars unboxed, everything else boxed), so macro-built cdylibs
work with the generated declarations directly — see
`examples/class-integration/` for an end-to-end template that replaces step 2
with `leo3-codegen`.

### Important notes

- Build the cdylib with `LEO3_NO_LEAN=1` to avoid duplicate Lean runtime
  symbols. The Lean executable already links `libleanshared`.
- Set `LD_LIBRARY_PATH` (Linux) or `DYLD_LIBRARY_PATH` (macOS) at runtime so
  the dynamic linker finds the cdylib.

## Next Steps

- [Architecture overview](architecture.md) — crate layout and design decisions
- [Contracts](contracts.md) — API stability and semantic guarantees
- [`leo3-codegen` guide](codegen.md) — generate Lean `extern` declarations from a cdylib
- [Contributing guide](contributing.md) — development workflow
- [PyO3 alignment notes](pyo3-alignment.md) — mapping between PyO3 and Leo3 concepts
