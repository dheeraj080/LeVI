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

use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::{Request, Response as HyperResponse};
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use lev_core::{Method, PreparedRequest};

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
    /// How many attempts this response took. 1 is the common case; more means an
    /// earlier attempt failed transiently and the client asked again. The timings
    /// describe *this* attempt — the waits between attempts are not server latency.
    pub attempts: u32,
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
    ///
    /// A transient failure is retried per [`ClientConfig::retries`]: up to that many
    /// more attempts, on a backoff that doubles. A retry needs the request built again
    /// and building consumes it, so a client with retries configured keeps one spare
    /// copy of the request — and a client without, which is every load-path request,
    /// keeps none.
    pub async fn send(&self, request: PreparedRequest) -> Result<Response, HttpError> {
        let max_attempts = self.max_attempts(&request);
        let spare = if max_attempts > 1 {
            Some(request.clone())
        } else {
            None
        };

        let mut attempt = 1u32;
        let mut current = Some(request);
        loop {
            let owned = current.take().expect("each attempt owns its request");
            match self.send_once(owned, attempt).await {
                Ok(response) => {
                    match self.pause_before(attempt, max_attempts, Outcome::Response(&response)) {
                        Some(delay) => {
                            tokio::time::sleep(delay).await;
                            attempt += 1;
                            current = spare.clone();
                        }
                        None => return Ok(response),
                    }
                }
                Err(error) => {
                    match self.pause_before(attempt, max_attempts, Outcome::Failure(&error)) {
                        Some(delay) => {
                            tokio::time::sleep(delay).await;
                            attempt += 1;
                            current = spare.clone();
                        }
                        None => return Err(error),
                    }
                }
            }
        }
    }

    /// One attempt: build, send, buffer, and time the exchange.
    ///
    /// The response carries `attempt`, so a retry ends up reported instead of being
    /// presented as one slow request.
    async fn send_once(
        &self,
        request: PreparedRequest,
        attempt: u32,
    ) -> Result<Response, HttpError> {
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
            attempts: attempt,
        })
    }

    /// How many attempts this client is allowed to make for `request`.
    ///
    /// A method that is not safe to send twice gets exactly one, unless the config
    /// says otherwise out loud: a retried POST can create two orders, and no default
    /// gets to make that decision.
    fn max_attempts(&self, request: &PreparedRequest) -> u32 {
        if self.config.retries == 0 {
            return 1;
        }
        if self.config.retry_non_idempotent || is_idempotent(&request.method) {
            self.config.retries.saturating_add(1)
        } else {
            1
        }
    }

    /// How long to wait before another attempt, or `None` if this outcome stands.
    ///
    /// The pause is an exponential backoff, never longer than the config allows, and
    /// at least as long as the server asked for when it answered `Retry-After` — a
    /// server that says "come back in two seconds" is better informed than a schedule.
    fn pause_before(
        &self,
        attempt: u32,
        max_attempts: u32,
        outcome: Outcome<'_>,
    ) -> Option<Duration> {
        if attempt >= max_attempts {
            return None;
        }

        let server_wait = match outcome {
            Outcome::Response(response) if is_transient_status(response.status) => {
                parse_retry_after(&response.headers)
                    .map(|wait| wait.min(self.config.retry_after_max))
            }
            Outcome::Response(_) => return None,
            Outcome::Failure(error) if is_transient_error(error) => None,
            Outcome::Failure(_) => return None,
        };

        let backoff = self
            .config
            .retry_backoff
            .saturating_mul(1u32 << (attempt - 1).min(16))
            .min(self.config.retry_backoff_max);
        Some(server_wait.map_or(backoff, |wait| wait.max(backoff)))
    }

    /// Send a resolved request, counting and discarding the response body.
    ///
    /// This is the load path: the body is read to keep the connection healthy, but
    /// only its length is kept, so memory does not scale with response size. The
    /// deadline covers the body too — a target that dribbles bytes forever is a
    /// target that would otherwise hang the run.
    ///
    /// It never retries, whatever the config says. A load report is a statement about
    /// what the target saw; a retry loop would fold three requests into one sample and
    /// quietly call the slowest of them "the" latency. The load sender builds its
    /// client with retries pinned to zero, so this is a property of the spine rather
    /// than a convention somebody has to remember.
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

/// What one attempt produced, as far as a retry decision is concerned.
enum Outcome<'a> {
    /// A response arrived. Its `Retry-After` may still be worth honouring.
    Response(&'a Response),
    /// The exchange failed: a refusal, a reset, a deadline.
    Failure(&'a HttpError),
}

/// Statuses that mean "this one might work next time".
///
/// A 429 says the target is rate limiting, and 502/503/504 say a hop in the middle is
/// unhappy. A 500 is deliberately absent: the target *did* process the request and
/// failed, and repeating it may either fail the same way or, worse, succeed twice.
fn is_transient_status(status: u16) -> bool {
    matches!(status, 429 | 502 | 503 | 504)
}

/// Transport failures worth trying again: a refusal, a reset, a deadline, a body that
/// stopped part-way.
///
/// Configuration mistakes are not among them. They would fail identically twice, and
/// a backoff would only make a bad URL expensive instead of immediate.
fn is_transient_error(error: &HttpError) -> bool {
    matches!(
        error,
        HttpError::Request(_) | HttpError::Body(_) | HttpError::Timeout { .. }
    )
}

/// Whether `method` is safe to send twice (RFC 9110 §9.2.2).
fn is_idempotent(method: &Method) -> bool {
    matches!(
        method,
        Method::Get | Method::Head | Method::Options | Method::Put | Method::Delete
    )
}

/// `Retry-After` as a wait, when the server gave the seconds form.
///
/// The HTTP-date form is ignored: it needs a correct clock, and a clock is the one
/// thing a retry loop must not trust. Ignoring a wait is the safe direction — the
/// backoff applies instead, and the request goes out sooner rather than later.
fn parse_retry_after(headers: &[(String, String)]) -> Option<Duration> {
    headers
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case("retry-after"))
        .and_then(|(_, value)| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_is_read_as_seconds() {
        let headers = |value: &str| vec![("Retry-After".to_string(), value.to_string())];
        assert_eq!(
            parse_retry_after(&headers("2")),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            parse_retry_after(&headers(" 3 ")),
            Some(Duration::from_secs(3))
        );
        // The date form has no clock to be read with, and a word is not a wait.
        assert_eq!(
            parse_retry_after(&headers("Wed, 21 Oct 2026 07:28:00 GMT")),
            None
        );
        assert_eq!(parse_retry_after(&headers("soon")), None);
        assert_eq!(parse_retry_after(&headers("12, 34")), None);
        assert_eq!(parse_retry_after(&[]), None);
    }

    #[test]
    fn only_a_method_that_survives_a_second_send_is_idempotent() {
        assert!(is_idempotent(&Method::Get));
        assert!(is_idempotent(&Method::Delete));
        assert!(!is_idempotent(&Method::Post));
        assert!(!is_idempotent(&Method::Patch));
        assert!(!is_idempotent(&Method::Custom("PURGE".to_string())));
    }

    #[test]
    fn a_five_hundred_is_not_transient_but_a_five_zero_three_is() {
        assert!(!is_transient_status(500));
        assert!(!is_transient_status(200));
        for status in [429, 502, 503, 504] {
            assert!(is_transient_status(status), "{status}");
        }
    }
}
