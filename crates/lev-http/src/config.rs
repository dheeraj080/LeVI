// SPDX-License-Identifier: MIT OR Apache-2.0
//! Transport configuration: deadlines, retries and pool tuning.
//!
//! Timeouts live here, in one struct, so "how long is a request allowed to take" is a
//! question with a single answer per client instead of a constant buried at each call
//! site. Retries live here for the same reason: they are a property of the *client*,
//! not of a request in a collection, so a load run — which must report every request
//! the target actually saw — cannot acquire them by reading a file somebody else
//! wrote. Two deadlines matter:
//!
//! - the **connect** timeout bounds TCP/TLS establishment, so a black-holed route
//!   fails in seconds rather than hanging the whole request;
//! - the **request** timeout bounds the whole exchange (dispatch through last body
//!   byte). A target that never responds must not hang a run — the load path's
//!   graceful-stop drain exists because this was missing.
//!
//! A request may carry its own deadline ([`lev_core::PreparedRequest::timeout`]);
//! that one wins over the client default.

use std::time::Duration;

/// How long establishing a connection (TCP + TLS) may take.
///
/// Shorter than [`DEFAULT_REQUEST_TIMEOUT`] on purpose: a dead route should fail
/// fast and say *connect*, not be reported as a generic request timeout.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

/// The request deadline when neither the request nor the caller sets one.
///
/// Deliberately finite: the only defensible default for a load tool is one that
/// cannot hang.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// How long an idle pooled connection is kept before it is closed.
pub const DEFAULT_POOL_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// The wait before the second attempt. It doubles for each further attempt.
pub const DEFAULT_RETRY_BACKOFF: Duration = Duration::from_millis(200);

/// The longest a single wait may grow to, however many attempts have been made.
pub const DEFAULT_RETRY_BACKOFF_MAX: Duration = Duration::from_secs(5);

/// The longest a server's `Retry-After` is obeyed.
///
/// A server may name an hour; a command that has to finish must not wait for one.
pub const DEFAULT_RETRY_AFTER_MAX: Duration = Duration::from_secs(30);

/// Deadlines and pool tuning for an [`crate::HttpClient`].
///
/// `Default` is production-sane: a bounded connect, a bounded request, and a pooled
/// connection with a finite idle life. Tests that need a hung target to stay hung
/// opt out with [`ClientConfig::without_request_timeout`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    /// Bound on connection establishment (TCP + TLS).
    pub connect_timeout: Duration,
    /// Default bound on the whole request. `None` means no deadline — a test-only
    /// escape hatch; production paths always leave this `Some`.
    pub request_timeout: Option<Duration>,
    /// How long an idle connection stays in the pool. `None` keeps it forever.
    pub pool_idle_timeout: Option<Duration>,
    /// Cap on idle connections per host. `None` is the pool's own default
    /// (unbounded), which is what a load run against one host wants: the cap would
    /// otherwise silently close the very connections the run is reusing.
    pub pool_max_idle_per_host: Option<usize>,
    /// How many times a transient failure is retried before it is reported.
    ///
    /// 0 — the default — means one attempt, exactly as before. Retrying costs the
    /// target a second request, so it is off unless somebody asks for it.
    pub retries: u32,
    /// Whether a method that is not safe to send twice may be retried.
    ///
    /// A retried POST can create two orders, so a plain `retries` covers GET, HEAD,
    /// OPTIONS, PUT and DELETE only. Lifting that is possible, deliberate and visible
    /// — never a default.
    pub retry_non_idempotent: bool,
    /// The wait before the second attempt; it doubles for each further one.
    pub retry_backoff: Duration,
    /// The longest a single wait may grow to.
    pub retry_backoff_max: Duration,
    /// The longest a server's `Retry-After` is obeyed.
    pub retry_after_max: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            request_timeout: Some(DEFAULT_REQUEST_TIMEOUT),
            pool_idle_timeout: Some(DEFAULT_POOL_IDLE_TIMEOUT),
            pool_max_idle_per_host: None,
            retries: 0,
            retry_non_idempotent: false,
            retry_backoff: DEFAULT_RETRY_BACKOFF,
            retry_backoff_max: DEFAULT_RETRY_BACKOFF_MAX,
            retry_after_max: DEFAULT_RETRY_AFTER_MAX,
        }
    }
}

impl ClientConfig {
    /// A config with no request deadline: the request may hang until the OS gives up.
    ///
    /// Only tests want this — it is how a "a stuck target is bounded" test proves
    /// the bound is the timeout's doing, not luck.
    pub fn without_request_timeout() -> Self {
        Self {
            request_timeout: None,
            ..Self::default()
        }
    }

    /// Retry a transient failure `retries` more times, on an idempotent request.
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.retries = retries;
        self
    }

    /// Also retry methods that are not safe to send twice.
    ///
    /// Only for a target known to treat them as safe: the honest default above is a
    /// question, not an oversight.
    pub fn retrying_non_idempotent(mut self) -> Self {
        self.retry_non_idempotent = true;
        self
    }

    /// Shorten the whole retry schedule, for tests that must not sit out a real
    /// backoff.
    pub fn with_retry_backoff(mut self, backoff: Duration) -> Self {
        self.retry_backoff = backoff;
        self.retry_backoff_max = backoff;
        self
    }
}
