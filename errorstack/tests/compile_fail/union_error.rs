use errorstack::ErrorStack;

#[derive(ErrorStack)]
union Payload {
    int: u32,
    float: f32,
}

fn main() {}
