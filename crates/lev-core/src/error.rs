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

    /// A store that reads the OS credential store was asked for a secret and the
    /// store itself could not be read — a locked keychain, no session bus, a denied
    /// prompt. Distinct from [`CoreError::SecretNotFound`]: nothing here says the
    /// secret is absent, so a caller should not treat it as a naming mistake.
    #[error("the OS keychain could not be read for `{name}`: {reason}")]
    KeychainUnavailable { name: String, reason: String },

    #[error("invalid url `{0}`")]
    InvalidUrl(String),

    #[error("unsupported body: {0}")]
    UnsupportedBody(String),

    #[error("invalid base64: {0}")]
    InvalidBase64(String),

    #[error("invalid threshold: {0}")]
    InvalidThreshold(String),
}

/// Convenience alias.
pub type Result<T> = std::result::Result<T, CoreError>;
