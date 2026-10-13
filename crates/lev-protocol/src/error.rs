// SPDX-License-Identifier: MIT OR Apache-2.0
//! Protocol-level and transport-level errors.

use thiserror::Error;

use crate::version::NegotiationError;

/// A message could not be interpreted as part of the protocol conversation.
#[derive(Debug, Error)]
pub enum ProtocolError {
    /// The peer's protocol version is incompatible with ours.
    #[error(transparent)]
    Negotiation(#[from] NegotiationError),

    /// A message arrived in the wrong direction, or before the handshake.
    #[error("unexpected message `{0}`")]
    Unexpected(String),

    /// The message was structurally valid but semantically wrong.
    #[error("{0}")]
    Malformed(String),
}

/// A transport failed to read or write a message.
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),

    #[error("malformed json: {0}")]
    Decode(#[from] serde_json::Error),

    #[error("transport closed")]
    Closed,
}
