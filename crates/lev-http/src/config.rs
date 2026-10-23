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

use std::path::PathBuf;
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

/// How many redirects are followed when [`ClientConfig::follow_redirects`] is on.
pub const DEFAULT_MAX_REDIRECTS: u32 = 10;

/// An HTTP proxy, with an optional bypass list.
///
/// Only `http://` proxies are supported: the connector establishes a CONNECT
/// tunnel and then speaks TLS to the origin through it. A proxy URL may carry
/// `user:password`, which becomes a `Proxy-Authorization: Basic` header on the
/// CONNECT request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyConfig {
    /// `http://[user:password@]host[:port]`.
    pub url: String,
    /// Comma-separated hosts (or `*`) that are reached directly. A leading dot is
    /// accepted; `example.com` also matches `api.example.com`.
    pub no_proxy: Vec<String>,
}

impl ProxyConfig {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            no_proxy: Vec::new(),
        }
    }

    pub fn with_no_proxy<I, S>(mut self, entries: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.no_proxy = entries.into_iter().map(Into::into).collect();
        self
    }
}

/// TLS trust and client-certificate settings.
///
/// `insecure` disables certificate and hostname verification. It exists for local
/// development against a self-signed target and is never a default; every other
/// field adds trust rather than removing it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TlsConfig {
    /// Accept any certificate. Dangerous; opt-in only.
    pub insecure: bool,
    /// Extra PEM (or DER) roots, on top of the platform's native roots.
    pub ca_certs: Vec<PathBuf>,
    /// A PEM certificate chain to present to the server (mTLS).
    pub client_cert: Option<PathBuf>,
    /// The private key matching `client_cert` (PKCS#8, PKCS#1 or SEC1 PEM).
    pub client_key: Option<PathBuf>,
}

/// Deadlines, redirects, cookies, TLS and pool tuning for an [`crate::HttpClient`].
///
/// `Default` is production-sane: bounded connect and request deadlines, redirects
/// followed up to [`DEFAULT_MAX_REDIRECTS`], an in-memory cookie jar, and a pooled
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
    /// Follow 301, 302, 303, 307 and 308 responses.
    pub follow_redirects: bool,
    /// The most redirects one send may follow before it fails.
    pub max_redirects: u32,
    /// Maintain cookies from `Set-Cookie` and send them back on matching requests.
    pub cookie_jar: bool,
    /// Advertise and decode `gzip`, `deflate` and `br` response bodies.
    pub compression: bool,
    /// Refuse a buffered response body larger than this many bytes.
    ///
    /// Load mode never buffers, so this is an interactive-path bound only.
    pub max_response_bytes: Option<u64>,
    /// Send through an HTTP proxy unless `no_proxy` matches.
    pub proxy: Option<ProxyConfig>,
    /// TLS roots, client certificate, and the explicit-insecure switch.
    pub tls: TlsConfig,
    /// The `User-Agent` sent when the request does not carry one.
    pub user_agent: Option<String>,
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
            follow_redirects: true,
            max_redirects: DEFAULT_MAX_REDIRECTS,
            cookie_jar: true,
            compression: true,
            max_response_bytes: None,
            proxy: None,
            tls: TlsConfig::default(),
            user_agent: Some(format!("levi/{}", env!("CARGO_PKG_VERSION"))),
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

    /// Stop following redirects; the 3xx response is returned to the caller.
    pub fn without_redirects(mut self) -> Self {
        self.follow_redirects = false;
        self
    }

    /// Follow at most `max` redirects.
    pub fn with_max_redirects(mut self, max: u32) -> Self {
        self.follow_redirects = true;
        self.max_redirects = max;
        self
    }

    /// Do not keep or send cookies.
    pub fn without_cookies(mut self) -> Self {
        self.cookie_jar = false;
        self
    }

    /// Do not advertise or decode compressing encodings.
    pub fn without_compression(mut self) -> Self {
        self.compression = false;
        self
    }

    /// Refuse a buffered response larger than `limit` bytes.
    pub fn with_max_response_bytes(mut self, limit: u64) -> Self {
        self.max_response_bytes = Some(limit);
        self
    }

    /// Route requests through `proxy`, except hosts in `no_proxy`.
    pub fn with_proxy(mut self, proxy: ProxyConfig) -> Self {
        self.proxy = Some(proxy);
        self
    }

    /// Use this TLS configuration.
    pub fn with_tls(mut self, tls: TlsConfig) -> Self {
        self.tls = tls;
        self
    }
}
