// SPDX-License-Identifier: MIT OR Apache-2.0
//! The single HTTP client used by both the interactive and load paths.
//!
//! Built on `hyper_util`'s pooled legacy client with a `rustls` connector, so the
//! request you send once is the request you send 50,000 times. Bodies are read into
//! memory here for interactive use; load mode will discard them (Phase 3).

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

use crate::error::HttpError;
use crate::timing::Timings;

type Connector = HttpsConnector<HttpConnector>;
type RequestBody = Full<Bytes>;

/// A pooled HTTP/1.1 + HTTP/2 client.
pub struct HttpClient {
    client: Client<Connector, RequestBody>,
}

/// A fully buffered response, with timing.
#[derive(Debug, Clone)]
pub struct Response {
    pub status: u16,
    pub version: String,
    pub headers: Vec<(String, String)>,
    pub body: Bytes,
    pub timings: Timings,
}

impl HttpClient {
    /// Build a client that trusts the OS certificate store.
    pub fn new() -> Result<Self, HttpError> {
        let connector = HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|error| HttpError::Tls(error.to_string()))?
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .build();
        let client = Client::builder(TokioExecutor::new()).build(connector);
        Ok(Self { client })
    }

    /// Send a resolved request and buffer the response.
    pub async fn send(&self, request: PreparedRequest) -> Result<Response, HttpError> {
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
        let outgoing = builder
            .body(Full::new(Bytes::from(request.body)))
            .map_err(|error| HttpError::InvalidHeader(error.to_string()))?;

        let start = Instant::now();
        let response: HyperResponse<Incoming> = self.client.request(outgoing).await?;
        let ttfb = start.elapsed();

        let (parts, incoming) = response.into_parts();
        let collected = incoming
            .collect()
            .await
            .map_err(|error| HttpError::Body(error.to_string()))?;
        let body = collected.to_bytes();
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
            timings: Timings {
                total,
                ttfb,
                receive: total.saturating_sub(ttfb),
                dns: None,
                connect: None,
                tls: None,
            },
        })
    }
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new().expect("failed to build the default HTTP client")
    }
}
