#[derive(Clone)]
#[leo3_macros::leanclass]
struct BadSetter;

#[leo3_macros::leanclass]
impl BadSetter {
    #[setter]
    fn set_value(&self, value: i32) {
        let _ = value;
    }
}

fn main() {}
