#[leo3_macros::leanfn(concrete(u64))]
fn add<T: std::ops::Add<Output = T>>(a: T, b: T) -> T {
    a + b
}

fn main() {}
