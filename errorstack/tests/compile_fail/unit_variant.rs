use errorstack::ErrorStack;

#[derive(ErrorStack)]
enum AppError {
    Failure,
}

fn main() {}
