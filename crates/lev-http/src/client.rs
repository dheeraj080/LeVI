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
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;

use lev_core::{Method, PreparedRequest};

use crate::config::ClientConfig;
use crate::connector::{measuring, TimedHttpsConnector};
use crate::error::HttpError;
use crate::timing::Timings;

type Connector = TimedHttpsConnector;
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
        let connector = TimedHttpsConnector::new(config.connect_timeout)?;

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
        tracing::debug!(method = request.method.as_str(), attempt, "sending request");
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
        // Wrapped, not awaited directly: the connector records the phases of the
        // connection this request opens *into this task's cell*, which is the only
        // way they can be attributed to the request that opened it.
        let (exchanged, phases) = measuring(await_deadline(exchange, deadline)).await;
        let (parts, body, ttfb) = exchanged?;
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

        let response = Response {
            status: parts.status.as_u16(),
            version: format!("{:?}", parts.version),
            headers,
            body,
            timings: timings_since(ttfb, total, phases),
            attempts: attempt,
        };
        tracing::debug!(
            status = response.status,
            attempts = response.attempts,
            total_ms = response.timings.total.as_millis() as u64,
            "request answered"
        );
        Ok(response)
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

        let (server_wait, reason) = match outcome {
            Outcome::Response(response) if is_transient_status(response.status) => {
                let wait = parse_retry_after(&response.headers)
                    .map(|wait| wait.min(self.config.retry_after_max));
                // The status is safe to say out loud; headers and body are not the
                // transport's to print, and a log line must never become a way around
                // the redaction the engine does.
                (wait, format!("status {}", response.status))
            }
            Outcome::Response(_) => return None,
            Outcome::Failure(error) if is_transient_error(error) => {
                (None, transient_kind(error).to_string())
            }
            Outcome::Failure(_) => return None,
        };

        let backoff = self
            .config
            .retry_backoff
            .saturating_mul(1u32 << (attempt - 1).min(16))
            .min(self.config.retry_backoff_max);
        let pause = server_wait.map_or(backoff, |wait| wait.max(backoff));
        tracing::warn!(
            attempt,
            max_attempts,
            wait_ms = pause.as_millis() as u64,
            reason = %reason,
            "transient failure, asking again"
        );
        Some(pause)
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
    ///
    /// It logs nothing per request either. At ten thousand a second a line per request
    /// is noise nobody can read, and the run's own counters are the record of what
    /// happened; the events worth a line at that rate (a run starting, a threshold
    /// failing) are logged where they are decided, not here.
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
        let (exchanged, phases) = measuring(await_deadline(exchange, deadline)).await;
        let (parts, body_len, ttfb) = exchanged?;
        let total = start.elapsed();

        Ok(LoadResponse {
            status: parts.status.as_u16(),
            body_len,
            timings: timings_since(ttfb, total, phases),
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

/// Assemble the timings of one attempt.
///
/// `phases` is what the connector recorded for the connection this request opened:
/// nothing at all when it was served from the pool, which is the common case and the
/// honest answer. See [`crate::connector`] for how that attribution is made.
fn timings_since(
    ttfb: std::time::Duration,
    total: std::time::Duration,
    phases: Option<Timings>,
) -> Timings {
    Timings {
        total,
        ttfb,
        receive: total.saturating_sub(ttfb),
        ..phases.unwrap_or_default()
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

/// A fixed label for *why* a send is being retried.
///
/// The error's own `Display` can carry the URL, and a URL can carry a token in its
/// query, so a log line logs the class of the failure instead of the message: enough
/// to diagnose, nothing that could route around the engine's redaction.
fn transient_kind(error: &HttpError) -> &'static str {
    match error {
        HttpError::Timeout { .. } => "deadline exceeded",
        HttpError::Body(_) => "response body interrupted",
        _ => "connection failed",
    }
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
