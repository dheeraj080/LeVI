// SPDX-License-Identifier: MIT OR Apache-2.0
//! Engine errors, and how each one maps to a wire [`ErrorCode`].

use lev_protocol::ErrorCode;
use thiserror::Error;

/// Anything that can go wrong while serving a protocol session.
#[derive(Debug, Error)]
pub enum EngineError {
    /// The transport failed.
    #[error(transparent)]
    Transport(#[from] lev_protocol::TransportError),

    /// The conversation broke the protocol.
    #[error(transparent)]
    Protocol(#[from] lev_protocol::ProtocolError),

    /// The request, collection or environment is invalid.
    #[error("{0}")]
    Config(String),

    /// A template or variable could not be resolved.
    #[error(transparent)]
    Core(#[from] lev_core::CoreError),

    /// A pre-request or post-response script failed to compile or run.
    #[error(transparent)]
    Script(#[from] lev_script::ScriptError),

    /// The request could not be sent.
    #[error(transparent)]
    Http(#[from] lev_http::HttpError),

    /// An internal engine failure: a bug, or a background task that could not finish.
    #[error("{0}")]
    Internal(String),

    /// The peer asked for something this build does not implement.
    #[error("`{0}` is not supported by this build")]
    Unsupported(String),

    /// The peer reported a failure. Surfaced by [`crate::Client`].
    #[error("{code}: {msg}")]
    Remote { code: ErrorCode, msg: String },
}

impl EngineError {
    /// The stable wire code a client branches on.
    pub fn code(&self) -> ErrorCode {
        match self {
            EngineError::Transport(_) => ErrorCode::Engine,
            EngineError::Protocol(_) => ErrorCode::BadMessage,
            EngineError::Config(_) => ErrorCode::Config,
            EngineError::Core(_) => ErrorCode::Config,
            // A script is part of the request definition, so a broken one is the
            // author's mistake, not an engine fault — including a runaway loop.
            EngineError::Script(_) => ErrorCode::Config,
            EngineError::Http(error) => {
                if http_is_config(error) {
                    ErrorCode::Config
                } else {
                    ErrorCode::Engine
                }
            }
            EngineError::Internal(_) => ErrorCode::Internal,
            EngineError::Unsupported(_) => ErrorCode::Unsupported,
            EngineError::Remote { code, .. } => *code,
        }
    }
}

/// A malformed URL, header, method or TLS setup is the author's mistake, not a
/// target failure — so it is a configuration error, not an engine error.
fn http_is_config(error: &lev_http::HttpError) -> bool {
    matches!(
        error,
        lev_http::HttpError::InvalidUrl(_)
            | lev_http::HttpError::InvalidHeader(_)
            | lev_http::HttpError::InvalidMethod(_)
            | lev_http::HttpError::Tls(_)
    )
}

impl From<lev_protocol::NegotiationError> for EngineError {
    fn from(error: lev_protocol::NegotiationError) -> Self {
        EngineError::Protocol(lev_protocol::ProtocolError::Negotiation(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_mistakes_are_not_engine_errors() {
        assert_eq!(
            EngineError::Config("bad".to_string()).code(),
            ErrorCode::Config
        );
        assert_eq!(
            EngineError::Core(lev_core::CoreError::InvalidUrl("x".to_string())).code(),
            ErrorCode::Config
        );
        assert_eq!(
            EngineError::Http(lev_http::HttpError::InvalidUrl("x".to_string())).code(),
            ErrorCode::Config
        );
    }

    #[test]
    fn transport_failures_are_engine_errors() {
        assert_eq!(
            EngineError::Transport(lev_protocol::TransportError::Closed).code(),
            ErrorCode::Engine
        );
    }

    #[test]
    fn a_script_failure_is_a_configuration_error() {
        // A runaway script is the author's mistake, not an engine fault.
        let error = EngineError::Script(lev_script::ScriptError::Runtime("boom".to_string()));
        assert_eq!(error.code(), ErrorCode::Config);
    }

    #[test]
    fn a_remote_error_keeps_its_code() {
        let error = EngineError::Remote {
            code: ErrorCode::Unsupported,
            msg: "no".to_string(),
        };
        assert_eq!(error.code(), ErrorCode::Unsupported);
    }
}
