use leo3::prelude::*;

/// Module exporting one `#[leanfn]` function per arity 0..=8 so the dynamic
/// loading path (`LeanFunction::call0`..`call8`) can be exercised end to end.
///
/// Each `ma_*` function returns `10 * arity + sum(args)`, so a call with
/// arguments `1..=k` yields `10 * k + k * (k + 1) / 2` — a value the test can
/// recompute independently.
#[leanmodule(name = "MultiArityModule")]
#[allow(unused_imports)]
mod multi_arity_module {
    use leo3::prelude::*;

    #[leanfn(name = "ma_zero")]
    pub fn zero() -> u64 {
        0
    }

    #[leanfn(name = "ma_one")]
    pub fn one(a: u64) -> u64 {
        10 + a
    }

    #[leanfn(name = "ma_two")]
    pub fn two(a: u64, b: u64) -> u64 {
        20 + a + b
    }

    #[leanfn(name = "ma_three")]
    pub fn three(a: u64, b: u64, c: u64) -> u64 {
        30 + a + b + c
    }

    #[leanfn(name = "ma_four")]
    pub fn four(a: u64, b: u64, c: u64, d: u64) -> u64 {
        40 + a + b + c + d
    }

    #[leanfn(name = "ma_five")]
    pub fn five(a: u64, b: u64, c: u64, d: u64, e: u64) -> u64 {
        50 + a + b + c + d + e
    }

    #[leanfn(name = "ma_six")]
    pub fn six(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64) -> u64 {
        60 + a + b + c + d + e + f
    }

    #[leanfn(name = "ma_seven")]
    pub fn seven(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64) -> u64 {
        70 + a + b + c + d + e + f + g
    }

    #[leanfn(name = "ma_eight")]
    pub fn eight(a: u64, b: u64, c: u64, d: u64, e: u64, f: u64, g: u64, h: u64) -> u64 {
        80 + a + b + c + d + e + f + g + h
    }

    /// String-typed round trip through the boxed `_boxed` companion.
    #[leanfn(name = "ma_greet")]
    pub fn greet(name: String, count: u64) -> String {
        format!("{name} x{count}")
    }
}
