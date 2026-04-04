use errorstack::{ErrorStack, Report};

// ── Test error types ──

#[derive(thiserror::Error, ErrorStack, Debug)]
#[error("top: {detail}")]
struct TopError {
    detail: String,
    #[stack_source]
    source: MiddleError,
    #[location]
    location: &'static std::panic::Location<'static>,
}

#[derive(thiserror::Error, ErrorStack, Debug)]
#[error("middle: {detail}")]
struct MiddleError {
    detail: String,
    #[stack_source]
    source: BottomError,
    #[location]
    location: &'static std::panic::Location<'static>,
}

#[derive(thiserror::Error, ErrorStack, Debug)]
#[error("bottom: {detail}")]
struct BottomError {
    detail: String,
    #[location]
    location: &'static std::panic::Location<'static>,
}

/// Error with a plain (non-ErrorStack) source, used to test the untyped tail
/// fallthrough.
#[derive(thiserror::Error, ErrorStack, Debug)]
#[error("plain source")]
struct PlainSourceError {
    source: std::io::Error,
    #[location]
    location: &'static std::panic::Location<'static>,
}

/// Error with no source and no location.
#[derive(thiserror::Error, ErrorStack, Debug)]
#[error("leaf")]
struct LeafError {}

// ── Tests ──

#[test]
fn report_single() {
    let err = LeafError {};
    let report = Report::new(&err);
    let entries: Vec<_> = report.entries().collect();
    assert_eq!(entries.len(), 1, "single error should produce one entry");
    assert_eq!(entries[0].message(), "leaf", "message should match Display");
}

#[test]
fn report_chain_typed() {
    let bottom = BottomError::new("connection refused".into());
    let middle = MiddleError::new("service unavailable".into())(bottom);
    let top = TopError::new("request failed".into())(middle);
    let report = Report::new(&top);
    let entries: Vec<_> = report.entries().collect();
    let messages: Vec<_> = entries.iter().map(|e| e.message().to_owned()).collect();
    assert_eq!(
        messages,
        vec![
            "top: request failed",
            "middle: service unavailable",
            "bottom: connection refused"
        ],
        "typed chain should produce entries in source order"
    );
    assert!(
        entries.iter().all(|e| e.location().is_some()),
        "all typed entries should have locations"
    );
}

#[test]
fn report_source_tail() {
    let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
    let bridge = PlainSourceError::new(io_err);
    let report = Report::new(&bridge);
    let messages: Vec<_> = report.entries().map(|e| e.message().to_owned()).collect();
    assert_eq!(
        messages,
        vec!["plain source", "file not found"],
        "should fall through to Error::source() for non-ErrorStack tail"
    );
    let tail = report.entries().last().expect("should have entries");
    assert!(
        tail.location().is_none(),
        "non-ErrorStack tail should have no location"
    );
}

#[test]
fn report_display_single_cause() {
    let bottom = BottomError::new("invalid key".into());
    let middle = MiddleError::new("authentication failed".into())(bottom);
    let output = Report::new(&middle).to_string();
    assert!(
        output.contains("Caused by this error:"),
        "single cause should use singular header"
    );
    assert!(
        !output.contains("these errors"),
        "single cause should not use plural header"
    );
}

#[test]
fn report_display_multiple_causes() {
    let bottom = BottomError::new("connection refused".into());
    let middle = MiddleError::new("service unavailable".into())(bottom);
    let top = TopError::new("request failed".into())(middle);
    let output = Report::new(&top).to_string();
    assert!(
        output.contains("Caused by these errors (recent errors listed first):"),
        "multiple causes should use plural header"
    );
    assert!(
        !output.contains("this error:"),
        "multiple causes should not use singular header"
    );
}

#[test]
fn report_debug_delegates() {
    let err = BottomError::new("unexpected state".into());
    let report = Report::new(&err);
    assert_eq!(
        format!("{report}"),
        format!("{report:?}"),
        "Debug should delegate to Display"
    );
}
