use errorstack::ErrorStack;

#[derive(ErrorStack)]
enum AppError {
    Conflict {
        source: std::io::Error,
        #[source]
        other: std::io::Error,
    },
}

fn main() {}
