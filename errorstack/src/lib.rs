//! A derive-based typed error system with first-class error stack building.
//!
//! `errorstack` provides source-code location tracking and typed error chain
//! walking for error types. The crate centres on a single derive macro,
//! `#[derive(ErrorStack)]`, which:
//!
//! - **Implements the [`ErrorStack`] trait** — each error gains a
//!   [`location`](ErrorStack::location) method that returns the call site
//!   where it was constructed and a
//!   [`stack_source`](ErrorStack::stack_source) method that returns the
//!   next typed link in the error chain.
//! - **Generates `#[track_caller]` constructors** — one constructor per
//!   enum variant (or a single `new` method for structs) that
//!   automatically captures the caller's source-code location and accepts
//!   the source error when present. Source-bearing constructors return
//!   `impl FnOnce(SourceType) -> Self`, composing directly with
//!   [`Result::map_err`] without an intermediate closure.
//!
//! Together these allow the creation of a [`Report`] to walk the full typed
//! chain, generating a traceback for the error with source-code locations.
//!
//! # Motivation
//!
//! The standard [`Error::source`](std::error::Error::source) chain erases
//! concrete types behind `&dyn Error`, losing any additional context the
//! error carried. Backtraces recover runtime stack frames but not the
//! logical error frames an application constructs as it propagates
//! failures upward. `errorstack` bridges this gap: each error records the
//! source-code location where it was created and holds a typed reference to
//! the next error in the chain, so the full causal history is available for
//! both programmatic inspection and formatted display.
//!
//! # Quick start
//!
//! ```
//! use errorstack::{ErrorStack, Report};
//!
//! #[derive(thiserror::Error, ErrorStack, Debug)]
//! pub enum AppError {
//!     #[error("io failed: {path}")]
//!     Io {
//!         path: String,
//!         source: std::io::Error,
//!         #[location]
//!         location: &'static std::panic::Location<'static>,
//!     },
//!
//!     #[error("config failed")]
//!     Config {
//!         #[stack_source]
//!         source: ConfigError,
//!         #[location]
//!         location: &'static std::panic::Location<'static>,
//!     },
//! }
//!
//! #[derive(thiserror::Error, ErrorStack, Debug)]
//! #[error("invalid config: {detail}")]
//! pub struct ConfigError {
//!     detail: String,
//!     #[location]
//!     location: &'static std::panic::Location<'static>,
//! }
//!
//! fn load_config() -> Result<(), AppError> {
//!     let inner = ConfigError::new("missing field `port`".into());
//!     Err(AppError::config()(inner))
//! }
//!
//! let err = load_config().unwrap_err();
//! let report = Report::new(&err);
//! assert_eq!(report.entries().count(), 2);
//! ```
//!
//! Printing `report` produces output similar to:
//!
//! ```text
//! Error: config failed
//!       at src/main.rs:14:9
//!
//! Caused by this error:
//!   1: invalid config: missing field `port`
//!         at src/main.rs:13:17
//! ```
//!
//! # Core concepts
//!
//! ## The [`ErrorStack`] trait
//!
//! [`ErrorStack`] extends [`Error`](std::error::Error) with two methods:
//!
//! - [`location`](ErrorStack::location) — returns the
//!   [`std::panic::Location`] where the error was constructed, or [`None`]
//!   if location tracking is not present for this error.
//! - [`stack_source`](ErrorStack::stack_source) — returns the next
//!   [`ErrorStack`] implementor in the chain, or [`None`] if this error is
//!   the root cause (or if the underlying source does not implement
//!   [`ErrorStack`]).
//!
//! The trait is typically derived rather than implemented by hand. See the
//! [derive macro documentation](derive@ErrorStack) for the full attribute
//! reference, naming conventions, and generated constructor signatures.
//!
//! ## [`Report`]
//!
//! [`Report`] collects an entire error chain into a list of [`Entry`]
//! values, each pairing an error message with a source-code
//! location where available.
//!
//! [`Report`] provides a default [`Display`](std::fmt::Display)
//! implementation that renders the chain in a human-readable format with
//! the outermost error first, followed by numbered causes and their
//! locations. Callers that need a different structure — for example,
//! emitting each frame as a structured telemetry event — can iterate over
//! the [`Entry`] values directly via [`Report::entries`].
//!
//! # Compatibility with `thiserror`
//!
//! `errorstack` uses the same field conventions as
//! [`thiserror`](https://crates.io/crates/thiserror) and is designed to
//! pair with it.
pub use errorstack_derive::ErrorStack;

/// An error within a typed error stack, preserving full error context as
/// errors propagate up the call stack.
///
/// Each error may carry the source-code [`location`] where it was constructed
/// and a reference to the next error in the stack via [`stack_source`].
///
/// Typically derived via `#[derive(ErrorStack)]` rather than implemented by
/// hand.
///
/// [`stack_source`]: ErrorStack::stack_source
/// [`location`]: ErrorStack::location
pub trait ErrorStack: std::error::Error + Send + Sync + 'static {
    /// Returns the source code location where this error was constructed,
    /// or [`None`] if location tracking is not available for this error.
    fn location(&self) -> Option<&'static std::panic::Location<'static>>;

    /// Returns the next error in the chain, or [`None`] if this is the root
    /// cause.
    fn stack_source(&self) -> Option<&dyn ErrorStack> {
        None
    }
}

impl std::error::Error for Box<dyn ErrorStack + Send + Sync> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        (**self).source()
    }
}

impl ErrorStack for Box<dyn ErrorStack + Send + Sync> {
    fn location(&self) -> Option<&'static std::panic::Location<'static>> {
        (**self).location()
    }

    fn stack_source(&self) -> Option<&dyn ErrorStack> {
        (**self).stack_source()
    }
}

/// A single entry in an error report, pairing an error message with an
/// optional source-code [`Location`].
///
/// [`Location`]: std::panic::Location
pub struct Entry {
    message: String,
    location: Option<&'static std::panic::Location<'static>>,
}

impl Entry {
    /// Returns the display message for this entry.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the source-code location where this error was constructed, if
    /// the error implements location tracking.
    pub fn location(&self) -> Option<&'static std::panic::Location<'static>> {
        self.location
    }
}

/// A collected summary of an entire error chain, suitable for display or
/// structured inspection.
///
/// `Report` walks the typed [`ErrorStack::stack_source`] chain to extract
/// source-code locations, then falls back to [`Error::source`] to capture
/// any remaining non-[`ErrorStack`] causes.
///
/// # Examples
///
/// ```
/// # use std::error::Error;
/// # use errorstack::{ErrorStack, Report};
/// # use std::fmt;
/// #
/// # #[derive(Debug)]
/// # struct RootError;
/// # impl fmt::Display for RootError {
/// #     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
/// #         write!(f, "root cause")
/// #     }
/// # }
/// # impl Error for RootError {}
/// # impl ErrorStack for RootError {
/// #     fn location(&self) -> Option<&'static std::panic::Location<'static>> { None }
/// # }
/// #
/// let err = RootError;
/// let report = Report::new(&err);
/// println!("{report}");
/// ```
///
/// [`Error::source`]: std::error::Error::source
pub struct Report {
    entries: Vec<Entry>,
}

impl Report {
    /// Walks the error chain rooted at `err`, collecting each error's message
    /// and source-code location into an [`Entry`].
    ///
    /// At each step, [`ErrorStack::stack_source`] is used to traverse the
    /// chain and extract source-code locations. When a link does not implement
    /// [`ErrorStack`], the walk falls back to [`Error::source`].
    ///
    /// [`Error::source`]: std::error::Error::source
    pub fn new(err: &dyn ErrorStack) -> Self {
        let mut entries = Vec::new();

        let mut current: &dyn ErrorStack = err;
        entries.push(Entry {
            message: current.to_string(),
            location: current.location(),
        });

        let mut last_as_error: &dyn std::error::Error = current;
        while let Some(next) = current.stack_source() {
            entries.push(Entry {
                message: next.to_string(),
                location: next.location(),
            });
            last_as_error = next;
            current = next;
        }

        // Fall through to untyped Error::source() chain.
        let mut source = last_as_error.source();
        while let Some(err) = source {
            entries.push(Entry {
                message: err.to_string(),
                location: None,
            });
            source = err.source();
        }

        Self { entries }
    }

    /// Returns an iterator over the [`Entry`] values in this report, from the
    /// outermost error to the root cause.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }
}

impl<'a> IntoIterator for &'a Report {
    type Item = &'a Entry;
    type IntoIter = std::slice::Iter<'a, Entry>;

    fn into_iter(self) -> Self::IntoIter {
        self.entries.iter()
    }
}

impl std::fmt::Display for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Some(head) = self.entries.first() else {
            return Ok(());
        };

        write!(f, "Error: {}", head.message())?;
        if let Some(loc) = head.location() {
            write!(f, "\n      at {loc}")?;
        }

        let causes = &self.entries[1..];
        if causes.is_empty() {
            return Ok(());
        }

        if causes.len() == 1 {
            write!(f, "\n\nCaused by this error:")?;
        } else {
            write!(
                f,
                "\n\nCaused by these errors (recent errors listed first):"
            )?;
        }

        for (i, entry) in causes.iter().enumerate() {
            write!(f, "\n  {}: {}", i + 1, entry.message())?;
            if let Some(loc) = entry.location() {
                write!(f, "\n        at {loc}")?;
            }
        }

        Ok(())
    }
}

impl std::fmt::Debug for Report {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(self, f)
    }
}
