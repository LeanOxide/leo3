#[derive(Clone)]
#[leo3_macros::leanclass]
struct BadGetterParams;

#[leo3_macros::leanclass]
impl BadGetterParams {
    #[getter]
    fn value(&self, extra: i32) -> i32 {
        extra
    }
}

fn main() {}
