#[derive(Clone)]
#[leo3_macros::leanclass]
struct BadSetterParams;

#[leo3_macros::leanclass]
impl BadSetterParams {
    #[setter]
    fn set_value(&mut self, a: i32, b: i32) {
        let _ = (a, b);
    }
}

fn main() {}
