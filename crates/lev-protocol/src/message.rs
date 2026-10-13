// SPDX-License-Identifier: MIT OR Apache-2.0
//! The message envelope.
//!
//! One flat enum, internally tagged by `type`. A flat enum keeps the NDJSON
//! transport trivial (one object per line) and keeps the in-process channel
//! carrying exactly the same values it would serialize to.

use serde::{Deserialize, Serialize};

use crate::dto::{EchoDto, EnvDto, RequestDto, ResponseDto};
use crate::version::Version;

/// A capability the engine advertises during the handshake.
///
/// The client must not rely on a capability the engine did not advertise; that is
/// what lets an older engine serve a newer UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    /// `SendRequest`: single interactive requests.
    Interactive,
    /// `LoadRunRequest`: load runs (Phase 3).
    Load,
    /// GraphQL bodies and introspection (Phase 4).
    Graphql,
    /// gRPC and reflection (Phase 4).
    Grpc,
}

/// Severity of a [`Message::Log`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// A stable, machine-readable error code.
///
/// Clients branch on this, never on the message text — the text is for humans and
/// may change at any time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    /// The peer's protocol major differs from ours.
    ProtocolVersionMismatch,
    /// The message could not be parsed as a protocol message.
    BadMessage,
    /// The request, collection or environment is invalid.
    Config,
    /// The request could not be sent (transport, TLS, timeout, DNS).
    Engine,
    /// The message is not supported by this build.
    Unsupported,
    /// A bug in the engine.
    Internal,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::ProtocolVersionMismatch => "ProtocolVersionMismatch",
            ErrorCode::BadMessage => "BadMessage",
            ErrorCode::Config => "Config",
            ErrorCode::Engine => "Engine",
            ErrorCode::Unsupported => "Unsupported",
            ErrorCode::Internal => "Internal",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Every message on the wire, in both directions.
///
/// Direction is enforced by the receiving side, not by the type: a flat enum is
/// what makes the NDJSON encoding one object per line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum Message {
    // ---- client -> engine ----
    /// The mandatory first message. The engine replies `Ready` or `Error`.
    Hello {
        protocol_version: Version,
        #[serde(default)]
        capabilities: Vec<Capability>,
    },

    /// Send one request and wait for exactly one `RequestResult`.
    SendRequest { request: RequestDto, env: EnvDto },

    /// Ask the engine to wind down. `graceful` drains in-flight work (Phase 3).
    Stop {
        #[serde(default)]
        graceful: bool,
    },

    // ---- engine -> client ----
    /// The handshake succeeded; `version` is the negotiated version.
    Ready { version: Version, engine: String },

    /// The result of a `SendRequest`. Both halves are already redacted.
    RequestResult {
        request: EchoDto,
        response: ResponseDto,
    },

    /// A log line. Always redacted; may be dropped under back-pressure.
    Log { level: LogLevel, msg: String },

    /// A failure. `code` is the contract; `msg` is for humans.
    Error { code: ErrorCode, msg: String },
}

impl Message {
    /// The wire tag of this message, for error messages and logging.
    pub fn type_name(&self) -> &'static str {
        match self {
            Message::Hello { .. } => "hello",
            Message::SendRequest { .. } => "sendRequest",
            Message::Stop { .. } => "stop",
            Message::Ready { .. } => "ready",
            Message::RequestResult { .. } => "requestResult",
            Message::Log { .. } => "log",
            Message::Error { .. } => "error",
        }
    }

    /// Build an error message from a code and anything printable.
    pub fn error(code: ErrorCode, msg: impl Into<String>) -> Self {
        Message::Error {
            code,
            msg: msg.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn json(message: &Message) -> String {
        serde_json::to_string(message).unwrap()
    }

    #[test]
    fn hello_round_trips_and_is_tagged() {
        let hello = Message::Hello {
            protocol_version: Version::new(0, 1),
            capabilities: vec![Capability::Interactive],
        };
        let encoded = json(&hello);
        assert!(encoded.contains("\"type\":\"hello\""), "{encoded}");
        assert!(encoded.contains("\"protocolVersion\""), "{encoded}");
        assert!(encoded.contains("\"interactive\""), "{encoded}");
        assert_eq!(serde_json::from_str::<Message>(&encoded).unwrap(), hello);
    }

    #[test]
    fn every_variant_round_trips() {
        let messages = vec![
            Message::Stop { graceful: true },
            Message::Ready {
                version: Version::new(0, 1),
                engine: "0.0.0".to_string(),
            },
            Message::Log {
                level: LogLevel::Warn,
                msg: "careful".to_string(),
            },
            Message::error(ErrorCode::Config, "bad"),
        ];
        for message in messages {
            let encoded = json(&message);
            assert_eq!(serde_json::from_str::<Message>(&encoded).unwrap(), message);
        }
    }

    #[test]
    fn an_unknown_type_is_a_clean_decode_error() {
        let error = serde_json::from_str::<Message>("{\"type\":\"teleport\"}").unwrap_err();
        assert!(error.to_string().contains("teleport"), "{error}");
    }

    #[test]
    fn error_codes_have_stable_names() {
        assert_eq!(
            ErrorCode::ProtocolVersionMismatch.as_str(),
            "ProtocolVersionMismatch"
        );
    }
}
