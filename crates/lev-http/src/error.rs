// SPDX-License-Identifier: MIT OR Apache-2.0
//! Errors from the transport layer.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("invalid url `{0}`")]
    InvalidUrl(String),

    #[error("invalid header: {0}")]
    InvalidHeader(String),

    #[error("invalid method `{0}`")]
    InvalidMethod(String),

    #[error("tls setup failed: {0}")]
    Tls(String),

    #[error("invalid proxy: {0}")]
    InvalidProxy(String),

    #[error("too many redirects (limit {max})")]
    TooManyRedirects { max: u32 },

    #[error("redirect location is not a valid URL: {0}")]
    InvalidRedirect(String),

    #[error("response body exceeds the {limit} byte limit")]
    ResponseTooLarge { limit: u64 },

    #[error("unsupported content encoding `{0}`")]
    UnsupportedEncoding(String),

    #[error("request failed: {0}")]
    Request(#[from] hyper_util::client::legacy::Error),

    #[error("failed to read response body: {0}")]
    Body(String),

    /// The request exceeded its deadline: the target (or the network) did not
    /// answer in time. This is a *target* failure, not a configuration mistake,
    /// so it maps to an `engine` error on the wire.
    #[error("request timed out after {timeout:?}")]
    Timeout { timeout: std::time::Duration },
}
