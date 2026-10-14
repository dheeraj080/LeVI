// SPDX-License-Identifier: MIT OR Apache-2.0
//! Errors from the load engine.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoadError {
    /// The scenario, scope, or options are wrong: a missing request, an unreachable
    /// variable, a nonsensical worker count. This is the author's mistake.
    #[error("configuration error: {0}")]
    Config(String),

    /// A domain-level failure while compiling or rendering a request.
    #[error(transparent)]
    Core(#[from] lev_core::CoreError),

    /// The transport failed to set up (TLS roots, connector).
    #[error(transparent)]
    Http(#[from] lev_http::HttpError),

    /// A reporter could not write its output.
    #[error("report error: {0}")]
    Report(String),

    /// A filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// A bug in the engine: a poisoned lock, a failed merge, a panicked worker.
    #[error("internal error: {0}")]
    Internal(String),
}
