#[derive(Clone)]
#[leo3_macros::leanclass]
struct BadField {
    #[get]
    name: &'static str,
}

fn main() {}
