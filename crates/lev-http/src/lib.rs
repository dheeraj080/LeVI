// SPDX-License-Identifier: MIT OR Apache-2.0
//! Levi HTTP transport.
//!
//! One abstraction for REST/GraphQL (and, later, gRPC), used by both the
//! interactive and load paths so their behaviour cannot diverge. Built on `hyper`
//! directly — never `reqwest` — so per-phase timing can be recorded.
#![forbid(unsafe_code)]

mod client;
pub mod config;
mod connector;
mod cookies;
mod encoding;
pub mod error;
pub mod timing;

pub use client::{HttpClient, LoadResponse, Response};
pub use config::{ClientConfig, ProxyConfig, TlsConfig};
pub use error::HttpError;
pub use timing::Timings;
