// SPDX-License-Identifier: MIT OR Apache-2.0
//! Transport configuration: deadlines and pool tuning.
//!
//! Timeouts live here, in one struct, so "how long is a request allowed to take" is a
//! question with a single answer per client instead of a constant buried at each call
//! site. Two deadlines matter:
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
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            request_timeout: Some(DEFAULT_REQUEST_TIMEOUT),
            pool_idle_timeout: Some(DEFAULT_POOL_IDLE_TIMEOUT),
            pool_max_idle_per_host: None,
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
}
