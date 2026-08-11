//! Macro surface compile tests for Leo3
//!
//! Compile-only file (no runtime logic): instantiates every macro surface form
//! so the compile-time analyzers in `leo3-binding-ir` (`analysis.rs`) and the
//! code generators in `leo3-macros-backend` (`leanfn.rs`, `leanclass.rs`,
//! `lean_instance.rs`, `derive/`) execute and are covered.
//!
//! Run with:
//! ```bash
//! LEO3_NO_LEAN=1 cargo check --locked -p leo3 --features macros --test macro_surface_compile
//! ```

#![cfg(feature = "macros")]
#![allow(clippy::ptr_arg)]
#![allow(
    dead_code,
    unused_imports,
    unused_variables,
    unused_mut,
    non_snake_case
)]

use leo3::prelude::*;

// ===========================================================================
// Section 1: #[leanfn] — plain functions, naming, crate path forms
// ===========================================================================

#[leanfn]
fn plain_add(a: u64, b: u64) -> u64 {
    a + b
}

#[leanfn(name = "lean_visible_name")]
fn named_fn(x: u64) -> u64 {
    x
}

#[leanfn(crate = leo3)]
fn crate_path_fn(x: u32) -> u32 {
    x
}

#[leanfn(name = "combo", crate = leo3)]
fn combo_fn(x: u64) -> u64 {
    x
}

#[leo3::leanfn]
fn root_path_fn(x: u64) -> u64 {
    x
}

// ===========================================================================
// Section 2: #[leanfn] — borrowed parameters (&str, &String, &[T], &[T;N],
// &Vec<T>, &[u8])
// ===========================================================================

#[leanfn]
fn borrowed_str_len(value: &str) -> u64 {
    value.len() as u64
}

#[leanfn]
#[allow(clippy::ptr_arg)]
#[allow(clippy::ptr_arg)]
fn borrowed_string_len(value: &String) -> u64 {
    value.len() as u64
}

#[leanfn]
fn borrowed_slice_sum(values: &[u64]) -> u64 {
    values.iter().sum()
}

#[leanfn]
fn borrowed_array_sum(values: &[u64; 3]) -> u64 {
    values.iter().sum()
}

#[leanfn]
#[allow(clippy::ptr_arg)]
fn borrowed_vec_u8_sum(values: &Vec<u8>) -> u64 {
    values.iter().map(|b| *b as u64).sum()
}

#[leanfn]
#[allow(clippy::ptr_arg)]
fn borrowed_vec_u64_sum(values: &Vec<u64>) -> u64 {
    values.iter().sum()
}

#[leanfn]
fn borrowed_u8_slice_sum(values: &[u8]) -> u64 {
    values.iter().map(|b| *b as u64).sum()
}

#[leanfn]
fn borrowed_byte_array_sum(values: &[u8; 8]) -> u64 {
    values.iter().map(|b| *b as u64).sum()
}

// ===========================================================================
// Section 3: #[leanfn] — borrowed returns (&'static String, &'static Vec<u8>,
// &'static [T], &'static [T;N])
// ===========================================================================

static STATIC_NAME: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| "static-name".to_string());
static STATIC_BYTES: std::sync::LazyLock<Vec<u8>> = std::sync::LazyLock::new(|| vec![2, 4, 6, 8]);
static STATIC_WORDS: std::sync::LazyLock<Vec<u64>> = std::sync::LazyLock::new(|| vec![5, 8, 13]);

#[leanfn]
fn static_string_ref() -> &'static String {
    &STATIC_NAME
}

#[leanfn]
fn static_vec_u8_ref() -> &'static Vec<u8> {
    &STATIC_BYTES
}

#[leanfn]
fn static_vec_u64_ref() -> &'static Vec<u64> {
    &STATIC_WORDS
}

#[leanfn]
fn static_word_slice() -> &'static [u64] {
    &[1, 2, 3]
}

#[leanfn]
fn static_word_array() -> &'static [u64; 3] {
    &[4, 5, 6]
}

#[leanfn]
fn static_byte_slice() -> &'static [u8] {
    &STATIC_BYTES
}

// ===========================================================================
// Section 4: #[leanfn] — Option<T> / Result<T, E> / tuple wrapper params
// containing borrowed shapes, and Option<Result<T, E>>
// ===========================================================================

#[leanfn]
fn option_borrowed_alias_score(
    name: Option<&String>,
    bytes: Option<&Vec<u8>>,
    words: Option<&Vec<u64>>,
) -> u64 {
    let mut total = 0u64;
    if let Some(name) = name {
        total += name.len() as u64;
    }
    if let Some(bytes) = bytes {
        total += bytes.len() as u64;
    }
    if let Some(words) = words {
        total += words.iter().sum::<u64>();
    }
    total
}

#[leanfn]
fn result_borrowed_alias_score(
    name: Result<&String, &String>,
    bytes: Result<&Vec<u8>, &String>,
    words: Result<&Vec<u64>, &String>,
) -> u64 {
    let mut total = 0u64;
    if let Ok(name) = name {
        total += name.len() as u64;
    }
    if let Ok(bytes) = bytes {
        total += bytes.len() as u64;
    }
    if let Ok(words) = words {
        total += words.iter().sum::<u64>();
    }
    total
}

#[leanfn]
fn tuple_borrowed_alias_score(value: (&String, &Vec<u8>, &Vec<u64>)) -> u64 {
    value.0.len() as u64 + value.1.len() as u64 + value.2.iter().sum::<u64>()
}

#[leanfn]
fn option_result_borrowed_alias_score(value: Option<Result<&Vec<u64>, &String>>) -> u64 {
    match value {
        Some(Ok(words)) => words.iter().sum(),
        Some(Err(_)) | None => 0,
    }
}

#[leanfn]
fn option_borrowed_slice_score(values: Option<&[u64]>) -> u64 {
    values.map(|values| values.iter().sum()).unwrap_or(0)
}

#[leanfn]
fn result_borrowed_slice_score(values: Result<&[u64], &[u64]>) -> u64 {
    values.map(|v| v.iter().sum()).unwrap_or(0)
}

#[leanfn]
fn tuple_borrowed_slice_score(value: (&[u64], &[u64; 3])) -> u64 {
    value.0.iter().sum::<u64>() + value.1.iter().sum::<u64>()
}

// ===========================================================================
// Section 5: #[leanfn] — concrete(Ty, name = "...") monomorphization with
// multiple instances
// ===========================================================================

#[leanfn(
    concrete(u64, name = "mono_add_u64"),
    concrete(i64, name = "mono_add_i64"),
    concrete(f64, name = "mono_add_f64")
)]
fn mono_add<T: std::ops::Add<Output = T>>(a: T, b: T) -> T {
    a + b
}

#[leanfn(concrete(u64, name = "mono_maybe_u64"))]
fn mono_maybe<T: std::ops::Add<Output = T>>(flag: bool, a: T, b: T) -> Option<T> {
    if flag {
        Some(a + b)
    } else {
        None
    }
}

// ===========================================================================
// Section 6: #[leanfn] — result / tuple / option / unit / string / bytes
// returns
// ===========================================================================

#[leanfn]
fn result_return(flag: bool) -> Result<u64, String> {
    if flag {
        Ok(7)
    } else {
        Err("nope".to_string())
    }
}

#[leanfn]
fn result_string_return(x: i64) -> Result<String, i32> {
    if x > 0 {
        Ok("ok".to_string())
    } else {
        Err(-1)
    }
}

#[leanfn]
fn tuple_return() -> (u64, bool) {
    (1, true)
}

#[leanfn]
fn option_return(flag: bool) -> Option<u64> {
    if flag {
        Some(3)
    } else {
        None
    }
}

#[leanfn]
fn unit_return(x: u64) {
    let _ = x;
}

#[leanfn]
fn greet(name: String) -> String {
    format!("Hello, {}!", name)
}

#[leanfn]
fn vec_u8_param_sum(values: Vec<u8>) -> u64 {
    values.iter().map(|b| *b as u64).sum()
}

#[leanfn]
fn option_u8_slice_count(values: Option<&[u8]>) -> u64 {
    values.map(|v| v.len() as u64).unwrap_or(0)
}

#[leanfn]
fn result_u8_slice_count(values: Result<&[u8], String>) -> u64 {
    values.map(|v| v.len() as u64).unwrap_or(0)
}

#[leanfn]
fn option_vec_u8_count(values: Option<Vec<u8>>) -> u64 {
    values.map(|v| v.len() as u64).unwrap_or(0)
}

#[leanfn]
fn bytes_out(n: u64) -> Vec<u8> {
    vec![1, 2, 3].into_iter().take(n as usize).collect()
}

// ===========================================================================
// Section 7: #[leanfn] — scalar returns of every width
// (u8..u64, i8..i64, usize, isize, f32, f64, bool, char)
// ===========================================================================

#[leanfn]
fn scalar_u8(x: u8) -> u8 {
    x
}

#[leanfn]
fn scalar_u16(x: u16) -> u16 {
    x
}

#[leanfn]
fn scalar_u32(x: u32) -> u32 {
    x
}

#[leanfn]
fn scalar_u64(x: u64) -> u64 {
    x
}

#[leanfn]
fn scalar_i8(x: i8) -> i8 {
    x
}

#[leanfn]
fn scalar_i16(x: i16) -> i16 {
    x
}

#[leanfn]
fn scalar_i32(x: i32) -> i32 {
    x
}

#[leanfn]
fn scalar_i64(x: i64) -> i64 {
    x
}

#[leanfn]
fn scalar_usize(x: usize) -> usize {
    x
}

#[leanfn]
fn scalar_isize(x: isize) -> isize {
    x
}

#[leanfn]
fn scalar_f32(x: f32) -> f32 {
    x
}

#[leanfn]
fn scalar_f64(x: f64) -> f64 {
    x
}

#[leanfn]
fn scalar_bool(x: bool) -> bool {
    x
}

#[leanfn]
fn scalar_char(x: char) -> char {
    x
}

// ===========================================================================
// Section 8: #[leanclass] — structs with #[get] / #[set] field accessors
// ===========================================================================

#[derive(Clone, Debug, PartialEq)]
#[leanclass]
struct Player {
    #[get]
    name: String,
    #[get]
    #[set]
    score: i64,
    #[set]
    active: bool,
    #[get]
    ratio: f64,
}

#[leanclass]
impl Player {
    fn new(name: String, score: i64) -> Self {
        Player {
            name,
            score,
            active: false,
            ratio: 0.0,
        }
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }
}

#[derive(Clone, Debug, PartialEq)]
#[leanclass]
struct Inventory {
    #[get]
    #[set]
    items: Vec<String>,
    #[set]
    owner: String,
}

#[leanclass]
impl Inventory {
    fn new(owner: String) -> Self {
        Inventory {
            items: Vec::new(),
            owner,
        }
    }
}

// ===========================================================================
// Section 9: #[leanclass] — every receiver style, #[name = "..."], and
// #[getter] / #[setter] (bare and name = "...") methods; class name override
// on both struct and impl
// ===========================================================================

#[derive(Clone)]
#[leanclass(name = "CounterBox")]
struct CounterBox {
    value: i64,
}

#[leanclass(name = "CounterBox")]
impl CounterBox {
    // No receiver (static constructor).
    fn new(initial: i64) -> Self {
        CounterBox { value: initial }
    }

    // &self receiver.
    fn peek(&self) -> i64 {
        self.value
    }

    // &mut self -> ().
    fn bump(&mut self) {
        self.value += 1;
    }

    // &mut self -> R (non-unit return).
    fn bump_and_get(&mut self) -> i64 {
        self.value += 1;
        self.value
    }

    // self (owned receiver).
    fn consume(self) -> i64 {
        self.value
    }

    // Lean-visible name override via #[name = "..."].
    #[name = "twice"]
    fn times_two(&self) -> i64 {
        self.value * 2
    }

    // #[getter] with a name override.
    #[getter(name = "contents")]
    fn get_contents(&self) -> i64 {
        self.value
    }

    // Bare #[getter].
    #[getter]
    fn raw_value(&self) -> i64 {
        self.value
    }

    // #[setter] with a name override.
    #[setter(name = "assign")]
    fn assign_value(&mut self, v: i64) {
        self.value = v;
    }

    // Bare #[setter].
    #[setter]
    fn set_value(&mut self, v: i64) {
        self.value = v;
    }

    // Result return.
    fn checked_double(&self) -> Result<u64, String> {
        if self.value > i64::MAX / 2 {
            Err("overflow".to_string())
        } else {
            Ok((self.value * 2) as u64)
        }
    }

    // Vec<T> parameter.
    fn sum_values(&self, values: Vec<i64>) -> i64 {
        values.iter().sum()
    }
}

// ===========================================================================
// Section 10: #[leanclass] — Vec<T> / Option<T> / Result<T, E> / pair params
// and returns; String and scalar returns; Self return
// ===========================================================================

#[derive(Clone)]
#[leanclass]
struct TypeShowcase;

#[leanclass]
impl TypeShowcase {
    fn make() -> Self {
        TypeShowcase
    }

    fn shapes(
        &self,
        xs: Vec<u64>,
        flag: Option<bool>,
        pair: (u64, bool),
        value: Result<String, i32>,
    ) -> Result<Vec<u64>, (String, i32)> {
        let _ = (flag, pair, value);
        Ok(xs)
    }

    fn scalars(&self, a: usize, b: isize, c: f32, d: u8, e: i16, f: char) -> char {
        let _ = (a, b, c, d, e);
        f
    }

    fn strings(&self, s: String) -> String {
        s
    }

    fn maybe(&self, flag: bool) -> Option<u64> {
        if flag {
            Some(1)
        } else {
            None
        }
    }

    fn pair_ret(&self) -> (u64, String) {
        (1, "x".to_string())
    }
}

#[derive(Clone)]
#[leanclass]
struct WordBag {
    words: Vec<String>,
}

#[leanclass]
impl WordBag {
    fn new(words: Vec<String>) -> Self {
        WordBag { words }
    }

    fn count(&self) -> u64 {
        self.words.len() as u64
    }

    fn add(&mut self, word: String) {
        self.words.push(word);
    }

    fn all(&self) -> Vec<String> {
        self.words.clone()
    }

    fn lookup(&self, index: u64) -> Option<String> {
        self.words.get(index as usize).cloned()
    }
}

// ===========================================================================
// Section 11: #[leanmodule] — plain, name = "...", bare identifier, dotted
// name, exports = [...], crate = path, inner mod submodules, concrete
// instances inside a module
// ===========================================================================

#[leanmodule]
mod plain_module {
    use leo3::prelude::leanfn;

    #[leanfn(name = "plain_mod_add")]
    pub fn plain_mod_add(a: u64, b: u64) -> u64 {
        a + b
    }
}

#[leanmodule(name = "NamedModule")]
mod named_module {
    use leo3::prelude::leanfn;

    #[leanfn(name = "named_mod_add")]
    pub fn named_mod_add(a: u64, b: u64) -> u64 {
        a + b
    }
}

#[leanmodule(BareIdentModule)]
mod bare_module {
    use leo3::prelude::leanfn;

    #[leanfn(name = "bare_mod_add")]
    pub fn bare_mod_add(a: u64, b: u64) -> u64 {
        a + b
    }
}

#[leanmodule(name = "Foo.Bar.baz")]
mod dotted_module {
    use leo3::prelude::leanfn;

    #[leanfn(name = "dotted_mod_add")]
    pub fn dotted_mod_add(a: u64, b: u64) -> u64 {
        a + b
    }
}

#[leanmodule(name = "ExplicitExports", exports = ["sel_add", "sel_double"])]
mod explicit_module {
    use leo3::prelude::leanfn;

    #[leanfn(name = "sel_add")]
    pub fn sel_add(a: u64, b: u64) -> u64 {
        a + b
    }

    #[leanfn(name = "sel_double")]
    pub fn sel_double(x: u64) -> u64 {
        x * 2
    }

    #[leanfn(name = "sel_unexported")]
    pub fn sel_unexported(x: u64) -> u64 {
        x
    }
}

#[leanmodule(name = "CratePathModule", crate = leo3)]
mod crate_path_module {
    use leo3::prelude::leanfn;

    #[leanfn(name = "crate_mod_add")]
    pub fn crate_mod_add(a: u64, b: u64) -> u64 {
        a + b
    }
}

#[leanmodule(name = "SubmoduleHost")]
mod submodule_host {
    use leo3::prelude::leanfn;

    #[leanfn(name = "host_add")]
    pub fn host_add(a: u64, b: u64) -> u64 {
        a + b
    }

    pub mod inner_sub {
        use super::*;

        #[leanfn(name = "inner_add")]
        pub fn inner_add(a: u64, b: u64) -> u64 {
            a + b
        }

        pub mod nested_deep {
            use super::*;

            #[leanfn(name = "deep_add")]
            pub fn deep_add(a: u64, b: u64) -> u64 {
                a + b
            }
        }
    }
}

#[leanmodule(name = "ConcreteModule")]
mod concrete_module {
    use leo3::prelude::leanfn;

    #[leanfn(concrete(u64, name = "mod_mono_u64"))]
    pub fn mod_mono<T: std::ops::Add<Output = T>>(a: T, b: T) -> T {
        a + b
    }
}

// ===========================================================================
// Section 12: #[derive(IntoLean, FromLean)] — structs (named, tuple, unit),
// transparent, skip, default, rename, with, and enums with tag
// ===========================================================================

#[derive(Debug, PartialEq, IntoLean, FromLean)]
struct PlainPoint {
    x: u64,
    y: u64,
}

#[derive(Debug, PartialEq, IntoLean, FromLean)]
struct TuplePair(u64, u64);

#[derive(Debug, PartialEq, IntoLean, FromLean)]
struct UnitStruct;

#[derive(Debug, PartialEq, IntoLean, FromLean)]
#[lean(transparent)]
struct UserId(u64);

#[derive(Debug, PartialEq, IntoLean, FromLean)]
#[lean(transparent)]
struct Email {
    address: String,
}

#[derive(Debug, PartialEq, IntoLean, FromLean)]
struct WithSkip {
    id: u64,
    #[lean(skip)]
    cached_name: String,
}

#[derive(Debug, PartialEq, IntoLean, FromLean)]
struct WithDefault {
    label: String,
    #[lean(default)]
    retries: u64,
}

#[derive(Debug, PartialEq, IntoLean, FromLean)]
struct WithRename {
    #[lean(rename = "user_id")]
    id: u64,
    name: String,
}

// `with` helpers: the IntoLean direction passes (field value, lean token);
// the FromLean direction passes a borrowed LeanBound. Each derive uses its
// own helper below.
fn with_double_u64<'l>(
    value: u64,
    lean: leo3::Lean<'l>,
) -> leo3::err::LeanResult<leo3::instance::LeanBound<'l, leo3::types::LeanUInt64>> {
    leo3::conversion::IntoLean::into_lean(value * 2, lean)
}

fn with_extract_u64<'l>(
    value: &leo3::instance::LeanBound<'l, leo3::types::LeanUInt64>,
) -> leo3::err::LeanResult<u64> {
    leo3::conversion::FromLean::from_lean(value)
}

#[derive(Debug, PartialEq, IntoLean)]
struct WithIntoCustom {
    #[lean(with = with_double_u64)]
    value: u64,
}

#[derive(Debug, PartialEq, FromLean)]
struct WithFromCustom {
    #[lean(with = with_extract_u64)]
    value: u64,
}

// `with` combined with `default` fallback (FromLean only).
#[derive(Debug, PartialEq, FromLean)]
struct WithWithDefault {
    #[lean(with = with_extract_u64, default)]
    value: u64,
}

// Multiple attribute kinds combined on one struct. (`with` cannot be
// combined here: the same field attribute feeds both the IntoLean and
// FromLean derives, which call it with different signatures.)
#[derive(Debug, PartialEq, IntoLean, FromLean)]
struct MixedAttrs {
    #[lean(rename = "primary")]
    key: u64,
    #[lean(skip)]
    scratch: Vec<u8>,
    #[lean(default)]
    attempts: u32,
    extra: u64,
}

#[derive(Debug, PartialEq, IntoLean, FromLean)]
enum Color {
    Red,
    Green,
    Blue,
}

#[derive(Debug, PartialEq, IntoLean, FromLean)]
enum Shape {
    Circle(u64),
    Rect(u64, u64),
    Point { x: u64, y: u64 },
}

#[derive(Debug, PartialEq, IntoLean, FromLean)]
enum Protocol {
    #[lean(tag = 1)]
    Hello,
    #[lean(tag = 2)]
    Data(u64, String),
    #[lean(tag = 5)]
    Bye { reason: String },
}

#[derive(Debug, PartialEq, IntoLean, FromLean)]
enum MyResult<T, E> {
    Ok(T),
    Err(E),
}

// ===========================================================================
// Section 13: #[lean_instance] — Hashable+BEq, Ord, Repr, ToString, and the
// combined Hashable+BEq+Ord form on external classes
// ===========================================================================

#[derive(Clone, Debug)]
#[leanclass]
struct HashPoint {
    x: u64,
    y: u64,
}

#[lean_instance(Hashable, BEq)]
impl HashPoint {
    fn hash(&self) -> u64 {
        self.x ^ self.y
    }

    fn beq(&self, other: &Self) -> bool {
        self.x == other.x && self.y == other.y
    }
}

#[derive(Clone, Debug)]
#[leanclass]
struct OrdPoint {
    x: u64,
}

#[lean_instance(Ord)]
impl OrdPoint {
    fn compare(&self, other: &Self) -> std::cmp::Ordering {
        self.x.cmp(&other.x)
    }
}

#[derive(Clone, Debug)]
#[leanclass]
struct ReprPoint {
    x: u64,
}

#[lean_instance(Repr)]
impl ReprPoint {
    fn repr(&self) -> String {
        format!("ReprPoint({})", self.x)
    }
}

#[derive(Clone, Debug)]
#[leanclass]
struct DisplayPoint {
    x: u64,
}

#[lean_instance(ToString)]
impl DisplayPoint {
    #[allow(clippy::inherent_to_string)]
    fn to_string(&self) -> String {
        format!("DisplayPoint({})", self.x)
    }
}

#[derive(Clone, Debug)]
#[leanclass]
struct TriPoint {
    x: u64,
}

#[lean_instance(Hashable, BEq, Ord)]
impl TriPoint {
    fn hash(&self) -> u64 {
        self.x
    }

    fn beq(&self, other: &Self) -> bool {
        self.x == other.x
    }

    fn compare(&self, other: &Self) -> std::cmp::Ordering {
        self.x.cmp(&other.x)
    }
}

fn main() {}
