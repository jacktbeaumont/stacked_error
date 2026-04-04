use errorstack::ErrorStack;

// ── Test error types ──

#[derive(thiserror::Error, ErrorStack, Debug)]
pub enum MyError {
    #[error("io failed: {path}")]
    Io {
        path: String,
        source: std::io::Error,
        #[location]
        location: &'static std::panic::Location<'static>,
    },

    #[error("not found: {id}")]
    NotFound {
        id: String,
        #[location]
        location: &'static std::panic::Location<'static>,
    },

    #[error("inner failed")]
    Inner {
        #[stack_source]
        source: InnerError,
        #[location]
        location: &'static std::panic::Location<'static>,
    },

    #[error("bare error")]
    Bare { message: String },
}

#[derive(thiserror::Error, ErrorStack, Debug)]
#[error("inner: {detail}")]
pub struct InnerError {
    detail: String,
    #[location]
    location: &'static std::panic::Location<'static>,
}

/// Error using `#[source]` attribute on a non-`source`-named field.
#[derive(thiserror::Error, ErrorStack, Debug)]
pub enum AttrSourceError {
    #[error("wrapped")]
    Wrapped {
        #[source]
        cause: std::io::Error,
        #[location]
        location: &'static std::panic::Location<'static>,
    },
}

/// Error using `#[stack_source]` on a non-`source`-named field.
/// `#[stack_source]` implies `#[source]`, so no explicit `#[source]` is needed.
#[derive(thiserror::Error, ErrorStack, Debug)]
pub enum ImpliedSourceError {
    #[error("implied")]
    Implied {
        #[stack_source]
        cause: InnerError,
        #[location]
        location: &'static std::panic::Location<'static>,
    },
}

/// Error with a boxed `dyn ErrorStack` source.
#[derive(thiserror::Error, ErrorStack, Debug)]
pub enum BoxedError {
    #[error("boxed inner")]
    Boxed {
        #[stack_source]
        source: Box<dyn ErrorStack + Send + Sync>,
        #[location]
        location: &'static std::panic::Location<'static>,
    },
}

/// Error with optional source fields.
#[derive(thiserror::Error, ErrorStack, Debug)]
pub enum OptionalSourceError {
    #[error("maybe io: {path}")]
    MaybeIo {
        path: String,
        source: Option<std::io::Error>,
        #[location]
        location: &'static std::panic::Location<'static>,
    },

    #[error("maybe inner")]
    MaybeInner {
        #[stack_source]
        source: Option<InnerError>,
        #[location]
        location: &'static std::panic::Location<'static>,
    },
}

/// Struct with an optional stack source.
#[derive(thiserror::Error, ErrorStack, Debug)]
#[error("optional struct: {detail}")]
pub struct OptionalStructError {
    detail: String,
    #[stack_source]
    source: Option<InnerError>,
    #[location]
    location: &'static std::panic::Location<'static>,
}

// ── Tests ──

#[test]
fn constructor_with_source() {
    let err = MyError::io("test.txt".into())(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "file missing",
    ));
    assert!(
        matches!(&err, MyError::Io { path, .. } if path == "test.txt"),
        "should produce Io variant with correct path"
    );
}

#[test]
fn constructor_without_source() {
    let err = MyError::not_found("abc".into());
    assert!(
        matches!(&err, MyError::NotFound { id, .. } if id == "abc"),
        "should produce NotFound variant with correct id"
    );
}

#[test]
fn location_captured() {
    let expected_line = line!() + 1;
    let err = MyError::not_found("item-42".into());
    let loc = err.location().expect("should have location");
    assert!(
        loc.file().contains("derive.rs"),
        "location should reference this file"
    );
    assert_eq!(
        loc.line(),
        expected_line,
        "location should match the constructor call site"
    );
}

#[test]
fn location_optional() {
    let err = MyError::bare("unlocated".into());
    assert!(
        err.location().is_none(),
        "variant without #[location] should return None"
    );
}

#[test]
fn next_stack() {
    let inner = InnerError::new("connection refused".into());
    let err = MyError::inner()(inner);
    assert!(
        err.stack_source().is_some(),
        "should return Some for #[stack_source]"
    );
}

#[test]
fn next_plain() {
    let err = MyError::io("data.csv".into())(std::io::Error::other("read failed"));
    assert!(
        err.stack_source().is_none(),
        "should return None without #[stack_source]"
    );
}

#[test]
fn next_stack_boxed() {
    let inner = InnerError::new("type-erased cause".into());
    let boxed: Box<dyn ErrorStack + Send + Sync> = Box::new(inner);
    let err = BoxedError::boxed()(boxed);
    let stack_src = err
        .stack_source()
        .expect("boxed #[stack_source] should return Some");
    assert!(
        stack_src.location().is_some(),
        "boxed source should preserve location"
    );
}

#[test]
fn source_by_attribute() {
    let err = AttrSourceError::wrapped()(std::io::Error::other("permission denied"));
    assert!(
        matches!(&err, AttrSourceError::Wrapped { .. }),
        "should produce Wrapped variant via #[source] attribute"
    );
    // std::io::Error doesn't impl ErrorStack, so stack_source is None
    assert!(
        err.stack_source().is_none(),
        "#[source] without #[stack_source] should return None"
    );
}

#[test]
fn multiple_variants() {
    // All variants compile and produce distinct results.
    let io_err = MyError::io("config.yaml".into())(std::io::Error::other("not found"));
    let nf_err = MyError::not_found("user-7".into());
    let inner_err = MyError::inner()(InnerError::new("timeout".into()));
    let bare_err = MyError::bare("unexpected state".into());

    assert!(matches!(io_err, MyError::Io { .. }), "io variant");
    assert!(
        matches!(nf_err, MyError::NotFound { .. }),
        "not_found variant"
    );
    assert!(matches!(inner_err, MyError::Inner { .. }), "inner variant");
    assert!(matches!(bare_err, MyError::Bare { .. }), "bare variant");
}

#[test]
fn struct_error() {
    let err = InnerError::new("missing field".into());
    let loc = err.location().expect("struct should have location");
    assert!(
        loc.file().contains("derive.rs"),
        "struct location should reference this file"
    );
}

#[test]
fn location_through_closure() {
    // The location should be captured at the outer call site, not inside the closure.
    let expected_line = line!() + 1;
    let make = MyError::io("output.log".into());
    let err = make(std::io::Error::other("disk full"));
    let loc = err.location().expect("should have location");
    assert_eq!(
        loc.line(),
        expected_line,
        "location should match outer call site, not closure invocation"
    );
}

#[test]
fn stack_source_implies_source() {
    let inner = InnerError::new("cascade failure".into());
    let err = ImpliedSourceError::implied()(inner);
    assert!(
        err.stack_source().is_some(),
        "#[stack_source] should imply source and enable typed chain walking"
    );
}

#[test]
fn optional_source_enum() {
    let without = OptionalSourceError::maybe_io("a.txt".into());
    assert!(
        matches!(&without, OptionalSourceError::MaybeIo { source: None, .. }),
        "sourceless constructor should set source to None"
    );

    let with = OptionalSourceError::maybe_io_with("b.txt".into())(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        "gone",
    ));
    assert!(
        matches!(
            &with,
            OptionalSourceError::MaybeIo {
                source: Some(_),
                ..
            }
        ),
        "_with constructor should wrap source in Some"
    );

    let absent = OptionalSourceError::maybe_inner();
    assert!(
        absent.stack_source().is_none(),
        "optional stack_source should return None when source is absent"
    );

    let present = OptionalSourceError::maybe_inner_with()(InnerError::new("deep".into()));
    assert!(
        present.stack_source().is_some(),
        "optional stack_source should return Some when source is present"
    );
}

#[test]
fn optional_source_struct() {
    let without = OptionalStructError::new("no cause".into());
    assert!(
        without.stack_source().is_none(),
        "struct new() should set optional source to None"
    );

    let with = OptionalStructError::new_with("has cause".into())(InnerError::new("cause".into()));
    assert!(
        with.stack_source().is_some(),
        "struct new_with() should enable stack_source"
    );
}

#[test]
fn optional_source_location() {
    let expected_line = line!() + 1;
    let without = OptionalSourceError::maybe_io("a.txt".into());
    let loc = without.location().expect("should have location");
    assert_eq!(
        loc.line(),
        expected_line,
        "sourceless constructor should capture location"
    );

    let expected_line_with = line!() + 1;
    let make = OptionalSourceError::maybe_io_with("b.txt".into());
    let with = make(std::io::Error::other("fail"));
    let loc_with = with.location().expect("should have location");
    assert_eq!(
        loc_with.line(),
        expected_line_with,
        "_with constructor should capture location at outer call site"
    );
}
