// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Levi wire protocol.
//!
//! One boundary, three transports. The UI, the CLI, and a remote agent all speak
//! these DTOs; only the *transport* varies:
//!
//! - **in-process typed channel** — interactive requests, no encoding at all;
//! - **NDJSON over stdio** — a spawned child process, the default for load runs;
//! - socket / WebSocket — the desktop sidecar and remote agents (Phase 6).
//!
//! # Why this crate is deliberately pure
//!
//! It depends on `serde`, `tokio` and nothing else in the workspace. It cannot see
//! `lev-core`, so it is *impossible* for a domain type to leak onto the wire — the
//! compiler enforces the rule instead of a reviewer. Conversions between these DTOs
//! and the domain model live in `lev-engine`.
//!
//! The v0 contract is documented in `PROTOCOL.md`.

#![forbid(unsafe_code)]

mod dto;
mod error;
mod message;
mod transport;
mod version;

pub use dto::{
    ApiKeyLocationDto, AuthDto, BodyDto, EchoDto, EnvDto, FormFieldDto, HeaderDto, RequestDto,
    ResponseDto, TimingsDto, VarValueDto,
};
pub use error::{ProtocolError, TransportError};
pub use message::{Capability, ErrorCode, LogLevel, Message};
pub use transport::{in_process_channel, InProcessTransport, NdjsonTransport, Transport};
pub use version::{negotiate, NegotiationError, Version, PROTOCOL_VERSION};
