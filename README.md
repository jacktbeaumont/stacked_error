# errorstack

[![CI](https://github.com/jacktbeaumont/errorstack/actions/workflows/ci.yml/badge.svg)](https://github.com/jacktbeaumont/errorstack/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/errorstack.svg)](https://crates.io/crates/errorstack)
[![docs.rs](https://img.shields.io/docsrs/errorstack)](https://docs.rs/errorstack)
[![Rust 1.70.0+](https://img.shields.io/badge/rust-1.70.0%2B-orange.svg)](https://www.rust-lang.org)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

`errorstack` is a typed error system with error stacks and source-code location tracking for Rust.

The crate provides the `ErrorStack` trait, which extends `std::error::Error` with two methods: `stack_source()` returning the next typed link in the error chain, and `location()` returning the source-code location where the error was constructed.

The trait is mainly used via `#[derive(ErrorStack)]`, which implements `ErrorStack` based on field names and attributes. The macro also generates helper constructors that automatically capture the call-site location, and when a source field is present return a closure allowing ergonomic chaining with `Result::map_err`.

An `ErrorStack` can be converted into a `Report`, which walks the full typed chain and produces a traceback with source-code locations.

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

See the [crate documentation](https://docs.rs/errorstack) and the [derive macro documentation](https://docs.rs/errorstack/latest/errorstack/derive.ErrorStack.html) for more information.

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
