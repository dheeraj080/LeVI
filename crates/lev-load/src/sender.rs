// SPDX-License-Identifier: MIT OR Apache-2.0
//! The seam between the executor and the network.
//!
//! The executor talks to a [`Sender`], not to `lev_http::HttpClient` directly. That is
//! what makes the executor's timing logic testable without a socket: a test can hand it
//! a sender that returns instantly and drive the clock by hand.
//!
//! `Sender` returns a boxed future rather than being generic so it stays object-safe —
//! the runner holds one `Arc<dyn Sender>` shared by every VU.

use std::future::Future;
use std::pin::Pin;

use lev_core::PreparedRequest;
use lev_http::{ClientConfig, HttpClient, Response};

use crate::error::LoadError;

/// The outcome of one send: the full response or `None` on a transport failure
/// (DNS, connect, TLS, reset, bad URL). The executor measures latency itself, around
/// the call, so failures are timed the same way successes are.
pub type SendResult = Option<Response>;

/// Sends a prepared request.
pub trait Sender: Send + Sync + 'static {
    fn send<'a>(
        &'a self,
        request: PreparedRequest,
    ) -> Pin<Box<dyn Future<Output = SendResult> + Send + 'a>>;
}

/// The real sender: the pooled hyper client.
pub struct HttpSender {
    client: HttpClient,
}

impl HttpSender {
    /// A sender with the transport's default config (bounded connect and request).
    pub fn new() -> Result<Self, LoadError> {
        Self::with_config(ClientConfig::default())
    }

    /// A sender with an explicit transport config — how a run applies its own
    /// default request deadline (`timeout:` / `--timeout`).
    ///
    /// Retries are pinned off here, whatever the config says. A load report is a
    /// statement about what the target saw: a retry loop would fold three requests
    /// into one sample and one latency, and the failure behind the first two would
    /// vanish from the run entirely.
    pub fn with_config(config: ClientConfig) -> Result<Self, LoadError> {
        let config = ClientConfig {
            retries: 0,
            retry_non_idempotent: false,
            ..config
        };
        Ok(Self {
            client: HttpClient::with_config(config)?,
        })
    }
}

impl Sender for HttpSender {
    fn send<'a>(
        &'a self,
        request: PreparedRequest,
    ) -> Pin<Box<dyn Future<Output = SendResult> + Send + 'a>> {
        Box::pin(async move { self.client.send(request).await.ok() })
    }
}
