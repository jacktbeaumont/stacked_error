use errorstack::ErrorStack;

#[derive(ErrorStack)]
enum AppError {
    Conflict {
        #[location]
        loc1: &'static std::panic::Location<'static>,
        #[location]
        loc2: &'static std::panic::Location<'static>,
    },
}

fn main() {}
