#[derive(Clone)]
#[leo3_macros::leanclass]
struct BadBoth;

#[leo3_macros::leanclass]
impl BadBoth {
    #[getter]
    #[setter]
    fn value(&self) -> i32 {
        0
    }
}

fn main() {}
