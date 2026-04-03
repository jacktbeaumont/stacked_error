// Checks that `#[stack_source]` on a field whose type does not implement
// `ErrorStack` produces a compile error.
use errorstack::ErrorStack;

#[derive(ErrorStack)]
enum AppError {
    Invalid {
        #[stack_source]
        not_a_source: String,
    },
}

fn main() {}
