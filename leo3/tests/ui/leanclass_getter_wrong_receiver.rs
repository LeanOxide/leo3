#[derive(Clone)]
#[leo3_macros::leanclass]
struct BadGetter;

#[leo3_macros::leanclass]
impl BadGetter {
    #[getter]
    fn value(&mut self) -> i32 {
        0
    }
}

fn main() {}
