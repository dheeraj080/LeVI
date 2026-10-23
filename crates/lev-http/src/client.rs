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

use bytes::{Bytes, BytesMut};
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::header::{
    HeaderMap, HeaderName, HeaderValue, ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, COOKIE,
    LOCATION, SET_COOKIE, USER_AGENT,
};
use hyper::{Request, Response as HyperResponse};
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use url::Url;

use lev_core::{Method, PreparedRequest};

use crate::config::ClientConfig;
use crate::connector::{measuring, TimedHttpsConnector};
use crate::cookies::CookieJar;
use crate::error::HttpError;
use crate::timing::Timings;

type Connector = TimedHttpsConnector;
type RequestBody = Full<Bytes>;

/// A pooled HTTP/1.1 + HTTP/2 client.
pub struct HttpClient {
    client: Client<Connector, RequestBody>,
    config: ClientConfig,
    cookies: Option<CookieJar>,
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
    /// How many redirects this response followed before it was reported.
    pub redirects: u32,
}

/// A load-mode response: the status and body length, never the body.
///
/// The body is streamed and counted so the connection can be reused, then dropped.
#[derive(Debug, Clone)]
pub struct LoadResponse {
    pub status: u16,
    pub body_len: u64,
    pub timings: Timings,
    pub redirects: u32,
}

/// The outcome of one attempt, before it is shaped into an interactive or load
/// response.
struct Exchange {
    status: u16,
    version: String,
    headers: Vec<(String, String)>,
    body: Option<Bytes>,
    body_len: u64,
    timings: Timings,
    redirects: u32,
}

impl HttpClient {
    /// Build a client with the default [`ClientConfig`]: bounded connect, bounded
    /// request, redirects and cookies on, pooled connections with a finite idle life.
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
        let connector =
            TimedHttpsConnector::new(config.connect_timeout, &config.tls, config.proxy.as_ref())?;

        let mut builder = Client::builder(TokioExecutor::new());
        builder.pool_idle_timeout(config.pool_idle_timeout);
        if let Some(max_idle) = config.pool_max_idle_per_host {
            builder.pool_max_idle_per_host(max_idle);
        }
        let client = builder.build(connector);
        let cookies = config.cookie_jar.then(CookieJar::new);
        Ok(Self {
            client,
            config,
            cookies,
        })
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

    /// One attempt: follow redirects, send, buffer, and time the exchange.
    ///
    /// The response carries `attempt`, so a retry ends up reported instead of being
    /// presented as one slow request.
    async fn send_once(
        &self,
        request: PreparedRequest,
        attempt: u32,
    ) -> Result<Response, HttpError> {
        let exchange = self.execute(request, attempt, true).await?;
        let response = Response {
            status: exchange.status,
            version: exchange.version,
            headers: exchange.headers,
            body: exchange.body.unwrap_or_default(),
            timings: exchange.timings,
            attempts: attempt,
            redirects: exchange.redirects,
        };
        tracing::debug!(
            status = response.status,
            attempts = response.attempts,
            redirects = response.redirects,
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
        let exchange = self.execute(request, 1, false).await?;
        Ok(LoadResponse {
            status: exchange.status,
            body_len: exchange.body_len,
            timings: exchange.timings,
            redirects: exchange.redirects,
        })
    }

    /// The deadline for one send: the request's own timeout if it has one, else the
    /// client default. `None` means unbounded (test-only; see [`ClientConfig`]).
    fn deadline_for(&self, request: &PreparedRequest) -> Option<Duration> {
        request.timeout.or(self.config.request_timeout)
    }

    /// The one exchange loop: redirect, cookie, decode, time.
    ///
    /// `keep_body` is the difference between the interactive and load paths. When it
    /// is false the body is streamed through a counter and dropped, and compression is
    /// not advertised, so the reported byte count is wire traffic.
    async fn execute(
        &self,
        request: PreparedRequest,
        attempt: u32,
        keep_body: bool,
    ) -> Result<Exchange, HttpError> {
        let timeout = self.deadline_for(&request);
        let deadline = timeout.map(|timeout| (timeout, Instant::now() + timeout));
        tracing::debug!(method = request.method.as_str(), attempt, "sending request");

        let mut method = request.method;
        let mut url = request.url;
        let mut headers = request.headers;
        let mut request_body = request.body;
        let mut redirects = 0u32;
        let mut totals = Timings::default();

        loop {
            let outgoing = build_request(
                &method,
                &url,
                &headers,
                request_body.clone(),
                &self.config,
                self.cookies.as_ref(),
                keep_body && self.config.compression,
            )?;

            let start = Instant::now();
            let exchange = async {
                let response: HyperResponse<Incoming> = self.client.request(outgoing).await?;
                let ttfb = start.elapsed();

                let (parts, incoming) = response.into_parts();
                let (body, body_len) =
                    read_body(incoming, keep_body, self.config.max_response_bytes).await?;
                Ok::<_, HttpError>((parts, body, body_len, ttfb))
            };

            // The remaining time, not a fresh budget per hop: a redirect is part of
            // the same request, and a chain that never finishes must not reset its
            // own deadline ten times.
            let budget = match deadline {
                Some((timeout, at)) => {
                    let remaining = at.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(HttpError::Timeout { timeout });
                    }
                    Some((timeout, remaining))
                }
                None => None,
            };
            let (exchanged, phases) = measuring(await_deadline(exchange, budget)).await;
            let (mut parts, mut buffered, mut body_len, ttfb) = exchanged?;
            let total = start.elapsed();
            totals = merge_timings(totals, timings_since(ttfb, total, phases));

            if let Some(jar) = &self.cookies {
                if let Ok(parsed) = Url::parse(&url) {
                    jar.store_headers(&parsed, parts.headers.get_all(SET_COOKIE).iter());
                }
            }

            let status = parts.status.as_u16();
            let location = parts
                .headers
                .get(LOCATION)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string);

            if self.config.follow_redirects && is_redirect(status) {
                if let Some(location) = location {
                    if redirects >= self.config.max_redirects {
                        return Err(HttpError::TooManyRedirects {
                            max: self.config.max_redirects,
                        });
                    }
                    let next_url = resolve_location(&url, &location)?;
                    let next_method = redirected_method(status, &method);
                    if matches!(next_method, Method::Get | Method::Head) {
                        request_body.clear();
                        strip_entity_headers(&mut headers);
                    }
                    if !same_origin(&url, &next_url) {
                        strip_credentials(&mut headers);
                    }
                    tracing::debug!(
                        status,
                        redirects = redirects + 1,
                        from = %redacted_url(&url),
                        to = %redacted_url(&next_url),
                        "following redirect"
                    );
                    method = next_method;
                    url = next_url;
                    redirects += 1;
                    continue;
                }
            }

            if keep_body && self.config.compression {
                if let Some(encoding) = parts
                    .headers
                    .get(CONTENT_ENCODING)
                    .and_then(|value| value.to_str().ok())
                    .map(str::to_string)
                {
                    let raw = buffered.take().unwrap_or_default();
                    let decoded =
                        crate::encoding::decode(&encoding, raw, self.config.max_response_bytes)?;
                    if let Some(limit) = self.config.max_response_bytes {
                        if decoded.len() as u64 > limit {
                            return Err(HttpError::ResponseTooLarge { limit });
                        }
                    }
                    body_len = decoded.len() as u64;
                    buffered = Some(decoded);
                    // The body the caller receives is decoded; leaving the header
                    // would claim otherwise and make a second decode look correct.
                    parts.headers.remove(CONTENT_ENCODING);
                    parts.headers.remove(CONTENT_LENGTH);
                }
            }

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

            return Ok(Exchange {
                status,
                version: format!("{:?}", parts.version),
                headers,
                body: buffered,
                body_len,
                timings: totals,
                redirects,
            });
        }
    }
}

/// Run one exchange under an optional `(reported timeout, remaining time)` pair.
async fn await_deadline<T>(
    exchange: impl std::future::Future<Output = Result<T, HttpError>>,
    deadline: Option<(Duration, Duration)>,
) -> Result<T, HttpError> {
    match deadline {
        Some((timeout, remaining)) => tokio::time::timeout(remaining, exchange)
            .await
            .map_err(|_| HttpError::Timeout { timeout })?,
        None => exchange.await,
    }
}

/// Read an incoming body, either buffering it under a limit or only counting it.
async fn read_body(
    mut incoming: Incoming,
    keep: bool,
    limit: Option<u64>,
) -> Result<(Option<Bytes>, u64), HttpError> {
    let mut buffer = keep.then(BytesMut::new);
    let mut body_len = 0u64;
    while let Some(frame) = incoming.frame().await {
        let frame = frame.map_err(|error| HttpError::Body(error.to_string()))?;
        if let Some(chunk) = frame.data_ref() {
            body_len += chunk.len() as u64;
            if let (Some(limit), Some(_)) = (limit, &buffer) {
                if body_len > limit {
                    return Err(HttpError::ResponseTooLarge { limit });
                }
            }
            if let Some(buffer) = &mut buffer {
                buffer.extend_from_slice(chunk);
            }
        }
    }
    Ok((buffer.map(BytesMut::freeze), body_len))
}

/// Turn a prepared request into a hyper request. Shared by both send paths.
fn build_request(
    method: &Method,
    url: &str,
    headers: &[(String, String)],
    body: Vec<u8>,
    config: &ClientConfig,
    cookies: Option<&CookieJar>,
    advertise_compression: bool,
) -> Result<Request<RequestBody>, HttpError> {
    let uri = url
        .parse::<hyper::Uri>()
        .map_err(|_| HttpError::InvalidUrl(url.to_string()))?;
    let method = hyper::Method::from_bytes(method.as_str().as_bytes())
        .map_err(|_| HttpError::InvalidMethod(method.as_str().to_string()))?;

    let mut map = HeaderMap::new();
    for (name, value) in headers {
        let name = HeaderName::from_bytes(name.as_bytes())
            .map_err(|_| HttpError::InvalidHeader(name.clone()))?;
        let value = HeaderValue::from_str(value)
            .map_err(|_| HttpError::InvalidHeader(format!("{name}: {value}")))?;
        map.append(name, value);
    }

    if advertise_compression && !map.contains_key(ACCEPT_ENCODING) {
        map.insert(
            ACCEPT_ENCODING,
            HeaderValue::from_static("gzip, deflate, br"),
        );
    }
    if !map.contains_key(USER_AGENT) {
        if let Some(user_agent) = &config.user_agent {
            if let Ok(value) = HeaderValue::from_str(user_agent) {
                map.insert(USER_AGENT, value);
            }
        }
    }
    if !map.contains_key(COOKIE) {
        if let Some(jar) = cookies {
            if let Ok(parsed) = Url::parse(url) {
                if let Some(cookie) = jar.header_for(&parsed) {
                    if let Ok(value) = HeaderValue::from_str(&cookie) {
                        map.insert(COOKIE, value);
                    }
                }
            }
        }
    }

    let mut request = Request::new(Full::new(Bytes::from(body)));
    *request.method_mut() = method;
    *request.uri_mut() = uri;
    *request.headers_mut() = map;
    Ok(request)
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

/// Add the timings of one redirect hop to the exchange's total.
///
/// Wall-clock phases add; connection phases add only when both hops reported one,
/// because `None` means "this hop reused a connection", not zero.
fn merge_timings(mut into: Timings, next: Timings) -> Timings {
    into.total += next.total;
    into.ttfb += next.ttfb;
    into.receive += next.receive;
    into.dns = sum_optional(into.dns, next.dns);
    into.connect = sum_optional(into.connect, next.connect);
    into.tls = sum_optional(into.tls, next.tls);
    into
}

fn sum_optional(left: Option<Duration>, right: Option<Duration>) -> Option<Duration> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left + right),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// How a redirect changes the method, following the common browser/client rules.
fn redirected_method(status: u16, method: &Method) -> Method {
    match status {
        303 if !matches!(method, Method::Head) => Method::Get,
        301 | 302 if !matches!(method, Method::Get | Method::Head) => Method::Get,
        _ => method.clone(),
    }
}

/// Resolve a `Location` value against the URL that produced it.
fn resolve_location(current: &str, location: &str) -> Result<String, HttpError> {
    if location.starts_with("http://") || location.starts_with("https://") {
        Url::parse(location)
            .map_err(|error| HttpError::InvalidRedirect(format!("`{location}`: {error}")))?;
        return Ok(location.to_string());
    }
    let base = Url::parse(current)
        .map_err(|error| HttpError::InvalidRedirect(format!("`{current}`: {error}")))?;
    let next = base
        .join(location)
        .map_err(|error| HttpError::InvalidRedirect(format!("`{location}`: {error}")))?;
    Ok(next.to_string())
}

fn same_origin(left: &str, right: &str) -> bool {
    let (Ok(left), Ok(right)) = (Url::parse(left), Url::parse(right)) else {
        return false;
    };
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

/// Drop headers that describe a request body that is no longer being sent.
fn strip_entity_headers(headers: &mut Vec<(String, String)>) {
    headers.retain(|(name, _)| {
        !matches!(
            name.to_ascii_lowercase().as_str(),
            "content-type" | "content-length" | "transfer-encoding" | "content-encoding"
        )
    });
}

/// Never carry credentials or a session cookie to another origin.
fn strip_credentials(headers: &mut Vec<(String, String)>) {
    headers.retain(|(name, _)| {
        !matches!(
            name.to_ascii_lowercase().as_str(),
            "authorization" | "cookie" | "proxy-authorization" | "host"
        )
    });
}

/// A URL with its query removed, for a debug line. A query can carry a token.
fn redacted_url(url: &str) -> String {
    match url.split_once('?') {
        Some((base, _)) => format!("{base}?<redacted>"),
        None => url.to_string(),
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

    #[test]
    fn a_redirect_resolves_a_relative_location() {
        assert_eq!(
            resolve_location("https://api.test/v1/orders", "/v2/orders").unwrap(),
            "https://api.test/v2/orders"
        );
        assert_eq!(
            resolve_location("https://api.test/v1/orders", "detail").unwrap(),
            "https://api.test/v1/detail"
        );
    }

    #[test]
    fn the_usual_redirect_method_rules_hold() {
        assert_eq!(
            redirected_method(303, &Method::Post),
            Method::Get,
            "303 says GET"
        );
        assert_eq!(redirected_method(303, &Method::Head), Method::Head);
        assert_eq!(redirected_method(307, &Method::Post), Method::Post);
        assert_eq!(redirected_method(308, &Method::Put), Method::Put);
        assert_eq!(redirected_method(302, &Method::Get), Method::Get);
    }

    #[test]
    fn a_cross_origin_redirect_drops_credentials() {
        assert!(!same_origin("https://api.test/a", "https://evil.test/b"));
        assert!(same_origin("https://api.test/a", "https://api.test:443/b"));
        assert!(!same_origin("http://api.test/a", "https://api.test/b"));
    }
}
