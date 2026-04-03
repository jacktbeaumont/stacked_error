# errorstack

[![CI](https://github.com/jacktbeaumont/errorstack/actions/workflows/ci.yml/badge.svg)](https://github.com/jacktbeaumont/errorstack/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/errorstack.svg)](https://crates.io/crates/errorstack)
[![docs.rs](https://img.shields.io/docsrs/errorstack)](https://docs.rs/errorstack)
[![Rust 1.70.0+](https://img.shields.io/badge/rust-1.70.0%2B-orange.svg)](https://www.rust-lang.org)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

A derive-based typed error system with first-class error stack building for Rust.

`errorstack` provides source-code location tracking and typed error chain walking for error types. The crate centres on a single derive macro, `#[derive(ErrorStack)]`, which:

- **Implements the `ErrorStack` trait** — each error gains a `location()` method that returns the call site where it was constructed and a `stack_source()` method that returns the next typed link in the error chain.
- **Generates `#[track_caller]` constructors** — one constructor per enum variant (or a single `new` method for structs) that automatically captures the caller's source-code location and accepts the source error when present. Source-bearing constructors return `impl FnOnce(SourceType) -> Self`, composing directly with `Result::map_err` without an intermediate closure.

Together these allow the creation of a `Report` to walk the full typed chain, generating a traceback for the error with source-code locations.

## Motivation

The standard `Error::source` chain erases concrete types behind `&dyn Error`, losing any additional context the error carried. Backtraces recover runtime stack frames but not the logical error frames an application constructs as it propagates failures upward. `errorstack` bridges this gap: each error records the source-code location where it was created and holds a typed reference to the next error in the chain, so the full causal history is available for both programmatic inspection and formatted display.

## Example

```rust
use errorstack::{ErrorStack, Report};

#[derive(thiserror::Error, ErrorStack, Debug)]
pub enum AppError {
    #[error("io failed: {path}")]
    Io {
        path: String,
        source: std::io::Error,
        #[location]
        location: &'static std::panic::Location<'static>,
    },

    #[error("config failed")]
    Config {
        #[stack_source]
        source: ConfigError,
        #[location]
        location: &'static std::panic::Location<'static>,
    },
}

#[derive(thiserror::Error, ErrorStack, Debug)]
#[error("invalid config: {detail}")]
pub struct ConfigError {
    detail: String,
    #[location]
    location: &'static std::panic::Location<'static>,
}

fn load_config() -> Result<(), ConfigError> {
    Err(ConfigError::new("missing field `port`".into()))
}

fn run() -> Result<(), AppError> {
    load_config().map_err(AppError::config())?;
    Ok(())
}
```

Running the above and printing the error with `Report` produces output similar to:

```text
Error: config failed
      at src/main.rs:25:21

Caused by this error:
  1: invalid config: missing field `port`
        at src/main.rs:21:9
```

## Core concepts

### The `ErrorStack` trait

`ErrorStack` extends `Error` with two methods:

- **`location`** — returns the `std::panic::Location` where the error was constructed, or `None` if location tracking is not present for this error.
- **`stack_source`** — returns the next `ErrorStack` implementor in the chain, or `None` if this error is the root cause (or if the underlying source does not implement `ErrorStack`).

The trait is typically derived rather than implemented by hand. See the [derive macro documentation](https://docs.rs/errorstack/latest/errorstack/derive.ErrorStack.html) for the full attribute reference, naming conventions, and generated constructor signatures.

### `Report`

`Report` collects an entire error chain into a list of `Entry` values, each pairing an error message with a source-code location where available.

`Report` provides a default `Display` implementation that renders the chain in a human-readable format with the outermost error first, followed by numbered causes and their locations. Callers that need a different structure — for example, emitting each frame as a structured telemetry event — can iterate over the `Entry` values directly via `Report::entries`.

## Compatibility with `thiserror`

`errorstack` uses the same field conventions as [`thiserror`](https://crates.io/crates/thiserror) and is designed to pair with it.

## License

Copyright 2026 jacktbeaumont

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

<http://www.apache.org/licenses/LICENSE-2.0>

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
