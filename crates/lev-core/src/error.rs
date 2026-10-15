// SPDX-License-Identifier: MIT OR Apache-2.0
//! Error type for the pure domain layer.

use thiserror::Error;

/// Everything that can go wrong while resolving or validating a request,
/// before any I/O happens.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("template parse error at byte {pos}: {msg}")]
    TemplateParse { pos: usize, msg: String },

    #[error("unknown dynamic function `${{{0}}}`")]
    UnknownFunction(String),

    #[error("unresolved variable `{0}`")]
    UnresolvedVariable(String),

    #[error("secret `{0}` not found")]
    SecretNotFound(String),

    #[error("invalid url `{0}`")]
    InvalidUrl(String),

    #[error("unsupported body: {0}")]
    UnsupportedBody(String),

    #[error("invalid threshold: {0}")]
    InvalidThreshold(String),
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, CoreError>;
