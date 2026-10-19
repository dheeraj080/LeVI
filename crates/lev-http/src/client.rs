// SPDX-License-Identifier: MIT OR Apache-2.0
//! The single HTTP client used by both the interactive and load paths.
//!
//! Built on `hyper_util`'s pooled legacy client with a `rustls` connector, so the
//! request you send once is the request you send 50,000 times.
//!
//! The two paths differ in exactly one place: [`HttpClient::send`] buffers the
//! response body (interactive), while [`HttpClient::send_discarding`] streams it and
//! keeps only the byte count (load mode). Buffering bodies in a load run is the
//! number-one memory bug in naive load tools, so the load path cannot do it by
//! accident — it does not have a buffering call to reach for.

use std::time::Instant;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request, Response as HyperResponse};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use lev_core::PreparedRequest;

use crate::config::ClientConfig;
use crate::error::HttpError;
use crate::timing::Timings;

type Connector = HttpsConnector<HttpConnector>;
type RequestBody = Full<Bytes>;

/// A pooled HTTP/1.1 + HTTP/2 client.
pub struct HttpClient {
    client: Client<Connector, RequestBody>,
    config: ClientConfig,
}

/// A fully buffered response, with timing. Interactive path only.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub version: String,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
    pub timings: Timings,
}

/// A load-mode response: the status and body length, never the body.
///
/// The body is streamed and counted so the connection can be reused, then dropped.
#[derive(Debug, Clone)]
pub struct LoadResponse {
    pub status: u16,
    pub body_len: u64,
    pub timings: Timings,
}

impl HttpClient {
    /// Build a client with the default [`ClientConfig`]: bounded connect, bounded
    /// request, pooled connections with a finite idle life.
    pub fn new() -> Result<Self, HttpError> {
        Self::with_config(ClientConfig::default())
    }

    /// Build a client from an explicit config.
    ///
    /// The connect timeout is set on the connector itself (so a dead route fails as a
    /// connect failure, fast); the request deadline is applied per send, because the
    /// deadline that matters is the one the request carries, falling back to the
    /// config's.
    pub fn with_config(config: ClientConfig) -> Result<Self, HttpError> {
        let mut http = HttpConnector::new();
        http.set_connect_timeout(Some(config.connect_timeout));
        let connector = HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|error| HttpError::Tls(error.to_string()))?
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .wrap_connector(http);

        let mut builder = Client::builder(TokioExecutor::new());
        builder.pool_idle_timeout(config.pool_idle_timeout);
        if let Some(max_idle) = config.pool_max_idle_per_host {
            builder.pool_max_idle_per_host(max_idle);
        }
        let client = builder.build(connector);
        Ok(Self { client, config })
    }

    /// The config this client was built with.
    pub fn config(&self) -> &ClientConfig {
        &self.config
    }

    /// Send a resolved request and buffer the response.
    ///
    /// Bounded by `request.timeout`, falling back to the config's request timeout.
    pub async fn send(&self, request: PreparedRequest) -> Result<Response, HttpError> {
        let deadline = self.deadline_for(&request);
        let outgoing = build_request(request)?;

        let start = Instant::now();
        let exchange = async {
            let response: HyperResponse<Incoming> = self.client.request(outgoing).await?;
            let ttfb = start.elapsed();

            let (parts, incoming) = response.into_parts();
            let collected = incoming
                .collect()
                .await
                .map_err(|error| HttpError::Body(error.to_string()))?;
            Ok::<_, HttpError>((parts, collected.to_bytes(), ttfb))
        };
        let (parts, body, ttfb) = await_deadline(exchange, deadline).await?;
        let total = start.elapsed();

        let headers = parts
            .headers
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    value.to_str().unwrap_or("<binary>").to_string(),
                )
            })
            .collect();

        Ok(Response {
            status: parts.status.as_u16(),
            version: format!("{:?}", parts.version),
            headers,
            body,
            timings: timings_since(ttfb, total),
        })
    }

    /// Send a resolved request, counting and discarding the response body.
    ///
    /// This is the load path: the body is read to keep the connection healthy, but
    /// only its length is kept, so memory does not scale with response size. The
    /// deadline covers the body too — a target that dribbles bytes forever is a
    /// target that would otherwise hang the run.
    pub async fn send_discarding(
        &self,
        request: PreparedRequest,
    ) -> Result<LoadResponse, HttpError> {
        let deadline = self.deadline_for(&request);
        let outgoing = build_request(request)?;

        let start = Instant::now();
        let exchange = async {
            let response: HyperResponse<Incoming> = self.client.request(outgoing).await?;
            let ttfb = start.elapsed();

            let (parts, mut incoming) = response.into_parts();
            let mut body_len: u64 = 0;
            while let Some(frame) = incoming.frame().await {
                let frame = frame.map_err(|error| HttpError::Body(error.to_string()))?;
                if let Some(chunk) = frame.data_ref() {
                    body_len += chunk.len() as u64;
                }
            }
            Ok::<_, HttpError>((parts, body_len, ttfb))
        };
        let (parts, body_len, ttfb) = await_deadline(exchange, deadline).await?;
        let total = start.elapsed();

        Ok(LoadResponse {
            status: parts.status.as_u16(),
            body_len,
            timings: timings_since(ttfb, total),
        })
    }

    /// The deadline for one send: the request's own timeout if it has one, else the
    /// client default. `None` means unbounded (test-only; see [`ClientConfig`]).
    fn deadline_for(&self, request: &PreparedRequest) -> Option<std::time::Duration> {
        request.timeout.or(self.config.request_timeout)
    }
}

/// Run one exchange under an optional deadline.
async fn await_deadline<T>(
    exchange: impl std::future::Future<Output = Result<T, HttpError>>,
    deadline: Option<std::time::Duration>,
) -> Result<T, HttpError> {
    match deadline {
        Some(timeout) => tokio::time::timeout(timeout, exchange)
            .await
            .map_err(|_| HttpError::Timeout { timeout })?,
        None => exchange.await,
    }
}

/// Turn a prepared request into a hyper request. Shared by both send paths.
fn build_request(request: PreparedRequest) -> Result<Request<RequestBody>, HttpError> {
    let uri = request
        .url
        .parse::<hyper::Uri>()
        .map_err(|_| HttpError::InvalidUrl(request.url.clone()))?;
    let method = hyper::Method::from_bytes(request.method.as_str().as_bytes())
        .map_err(|_| HttpError::InvalidMethod(request.method.as_str().to_string()))?;

    let mut builder = Request::builder().method(method).uri(uri);
    for (name, value) in &request.headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    builder
        .body(Full::new(Bytes::from(request.body)))
        .map_err(|error| HttpError::InvalidHeader(error.to_string()))
}

fn timings_since(ttfb: std::time::Duration, total: std::time::Duration) -> Timings {
    Timings {
        total,
        ttfb,
        receive: total.saturating_sub(ttfb),
        dns: None,
        connect: None,
        tls: None,
    }
}
