#[leo3_macros::leanfn(concrete(u64, i64, name = "add_bad"))]
fn add<T: std::ops::Add<Output = T>>(a: T, b: T) -> T {
    a + b
}

fn main() {}
