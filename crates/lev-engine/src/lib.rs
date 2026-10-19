// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Levi engine: the protocol-facing facade.
//!
//! This is the only crate that knows about both the wire protocol and the domain
//! model. It:
//!
//! - converts `lev-protocol` DTOs to and from `lev-core` types
//!   ([`convert`] — the protocol crate cannot see `lev-core`, so the compiler stops
//!   a domain type leaking onto the wire);
//! - resolves secrets from the environment and the OS keychain ([`secrets`]);
//! - prepares and sends a request through `lev-http`;
//! - **redacts before anything crosses the boundary**, so a secret value can never
//!   reach a client;
//! - serves a protocol session over any [`Transport`], which is what makes
//!   `levi serve --stdio` and an in-process request the same code path.
//!
//! [`Transport`]: lev_protocol::Transport

#![forbid(unsafe_code)]

pub mod checks;
mod client;
pub mod convert;
mod engine;
pub mod error;
pub mod secrets;

pub use client::{in_process_client, Client, RequestOutcome};
pub use engine::{Dispatch, Engine};
pub use error::EngineError;
pub use secrets::{
    keychain_delete, keychain_set, redactor_for, store_from_env, KeychainStore, ResolvingStore,
    SECRET_ENV_PREFIX,
};

/// The engine version reported to clients in the `Ready` handshake.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
