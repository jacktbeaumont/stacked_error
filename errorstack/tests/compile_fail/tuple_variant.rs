use errorstack::ErrorStack;

#[derive(ErrorStack)]
enum AppError {
    Failure(String),
}

fn main() {}
