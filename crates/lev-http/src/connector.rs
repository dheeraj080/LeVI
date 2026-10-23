// SPDX-License-Identifier: MIT OR Apache-2.0
//! Where a request's connection phases are measured, how TLS is configured, and how
//! a proxy is reached.
//!
//! A pooled client connects once and reuses that connection, so most requests never
//! resolve, connect or handshake at all — and when they do, the work belongs to a
//! *connection*, not to a request. Reporting a connect of zero for the 4,999th request
//! would be a measurement-shaped lie; reporting nothing is the truth, and it is what
//! this does.
//!
//! The carrier is a **task-local**. That is deliberate: a task-local can only be read
//! by the task that installed it, so a connection opened on somebody else's task — the
//! pool lets a second request attach to an attempt already in flight — is never
//! attributed to a request that did not do the work. When two requests share one
//! connection, whichever of them polled the attempt reports the phases; the other
//! reports none, which is also true.

use std::cell::RefCell;
use std::error::Error as StdError;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{ready, Context, Poll};
use std::time::{Duration, Instant};

use base64::Engine as _;
use hyper::header::HeaderValue;
use hyper::Uri;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::connect::dns::{GaiResolver, Name};
use hyper_util::client::legacy::connect::proxy::Tunnel;
use hyper_util::client::legacy::connect::HttpConnector;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::ring;
use rustls::pki_types::{
    CertificateDer, PrivateKeyDer, PrivatePkcs1KeyDer, PrivatePkcs8KeyDer, PrivateSec1KeyDer,
    ServerName, UnixTime,
};
use rustls::{DigitallySignedStruct, RootCertStore, SignatureScheme};
use tower_service::Service;
use url::Url;

use crate::config::{ProxyConfig, TlsConfig};
use crate::error::HttpError;
use crate::timing::Timings;

pub(crate) type BoxError = Box<dyn StdError + Send + Sync>;

/// The raw spans a connection recorded while it was being established.
///
/// These are not yet the three phases: the plain connector's call covers DNS *and*
/// TCP, and the TLS connector's call covers that *and* the handshake. Each span is
/// therefore measured at the one seam that can see it, and turned into `dns`/
/// `connect`/`tls` by subtraction — once, where the phases are read.
#[derive(Debug, Default, Clone, Copy)]
struct Spans {
    /// The resolver's own call.
    dns: Option<Duration>,
    /// DNS + TCP (plus a CONNECT round trip through a proxy), as timed by the
    /// connector that got the stream.
    connect: Option<Duration>,
    /// DNS + TCP + TLS, as timed by the TLS connector. Set for `https` only.
    handshake: Option<Duration>,
}

tokio::task_local! {
    static SPANS: RefCell<Spans>;
}

/// Run `future` with a fresh [`Spans`] cell, returning what the connector recorded.
///
/// The cell starts empty and is only filled by a connector polled inside this scope,
/// so a request served from the pool comes back with nothing to report.
pub(crate) async fn measuring<T>(future: impl Future<Output = T>) -> (T, Option<Timings>) {
    let (value, spans) = SPANS
        .scope(RefCell::new(Spans::default()), async {
            let value = future.await;
            let spans = SPANS.try_with(|cell| *cell.borrow()).ok();
            (value, spans)
        })
        .await;

    // Subtracting with `saturating_sub` keeps a measurement artefact — a phase that
    // completed before its own timer was read — at zero instead of turning it into a
    // huge number, while a phase that really ran always reports positively.
    let Some(spans) = spans.filter(|spans| spans.connect.is_some()) else {
        return (value, None);
    };
    let dns = spans.dns.unwrap_or_default();
    let connect = spans.connect.unwrap_or_default();
    let reported = Timings {
        dns: Some(dns),
        connect: Some(connect.saturating_sub(dns)),
        tls: spans
            .handshake
            .map(|handshake| handshake.saturating_sub(connect)),
        ..Timings::default()
    };
    (value, Some(reported))
}

/// Record one span into the cell, if this task has one.
fn record(span: impl FnOnce(&mut Spans)) {
    let _ = SPANS.try_with(|cell| span(&mut cell.borrow_mut()));
}

/// The resolver, timed. `HttpConnector` resolves through a service, so this is the
/// one seam that can see DNS on its own.
#[derive(Clone)]
pub(crate) struct TimedResolver {
    inner: GaiResolver,
}

impl Default for TimedResolver {
    fn default() -> Self {
        Self {
            inner: GaiResolver::new(),
        }
    }
}

impl Service<Name> for TimedResolver {
    type Response = <GaiResolver as Service<Name>>::Response;
    type Error = <GaiResolver as Service<Name>>::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, name: Name) -> Self::Future {
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let start = Instant::now();
            let addrs = inner.call(name).await?;
            record(|spans| spans.dns = Some(start.elapsed()));
            Ok(addrs)
        })
    }
}

/// The plain connector, timed: DNS plus TCP connect, as one span.
#[derive(Clone)]
pub(crate) struct TimedConnector {
    inner: HttpConnector<TimedResolver>,
}

impl TimedConnector {
    fn new(connect_timeout: Duration) -> Self {
        let mut inner = HttpConnector::new_with_resolver(TimedResolver::default());
        inner.set_connect_timeout(Some(connect_timeout));
        Self { inner }
    }
}

impl Service<Uri> for TimedConnector {
    type Response = <HttpConnector<TimedResolver> as Service<Uri>>::Response;
    type Error = <HttpConnector<TimedResolver> as Service<Uri>>::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let mut inner = self.inner.clone();
        Box::pin(async move {
            let start = Instant::now();
            let stream = inner.call(uri).await?;
            record(|spans| spans.connect = Some(start.elapsed()));
            Ok(stream)
        })
    }
}

/// Choose between a direct connection and an HTTP CONNECT tunnel per destination.
///
/// `Tunnel` normally speaks to an HTTPS proxy over TLS; here it wraps the plain
/// connector and only uses the HTTP CONNECT method. The tunnel works for both
/// `http` and `https` destinations: after the 200, the client sends origin-form
/// requests through the socket, exactly as if it had connected directly.
#[derive(Clone)]
pub(crate) struct ProxyConnector {
    direct: TimedConnector,
    proxy: Option<Tunnel<TimedConnector>>,
    no_proxy: Arc<Vec<NoProxyRule>>,
}

impl ProxyConnector {
    pub(crate) fn new(
        connect_timeout: Duration,
        proxy: Option<&ProxyConfig>,
    ) -> Result<Self, HttpError> {
        let direct = TimedConnector::new(connect_timeout);
        let (proxy, no_proxy) = match proxy {
            Some(config) => (
                Some(build_tunnel(config, direct.clone())?),
                Arc::new(parse_no_proxy(&config.no_proxy)),
            ),
            None => (None, Arc::new(Vec::new())),
        };
        Ok(Self {
            direct,
            proxy,
            no_proxy,
        })
    }

    fn through_proxy(&self, uri: &Uri) -> bool {
        self.proxy.is_some() && !self.no_proxy.iter().any(|rule| rule.matches(uri))
    }
}

impl Service<Uri> for ProxyConnector {
    type Response = <TimedConnector as Service<Uri>>::Response;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        ready!(self.direct.poll_ready(cx)).map_err(|error| Box::new(error) as BoxError)?;
        if let Some(proxy) = &mut self.proxy {
            ready!(proxy.poll_ready(cx)).map_err(|error| Box::new(error) as BoxError)?;
        }
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let mut direct = self.direct.clone();
        let proxy = self.proxy.clone();
        let through_proxy = self.through_proxy(&uri);
        Box::pin(async move {
            if through_proxy {
                let mut proxy = proxy.expect("checked by through_proxy");
                // `Tunnel` defaults a missing port to 443. An `http://` destination
                // almost never means 443, so make the destination port explicit.
                let destination = destination_uri(&uri)?;
                let start = Instant::now();
                let stream = proxy
                    .call(destination)
                    .await
                    .map_err(|error| Box::new(error) as BoxError)?;
                // This replaces the inner connector's span: from the request's point
                // of view, "connect" includes the CONNECT round trip.
                record(|spans| spans.connect = Some(start.elapsed()));
                return Ok(stream);
            }
            direct
                .call(uri)
                .await
                .map_err(|error| Box::new(error) as BoxError)
        })
    }
}

fn build_tunnel(
    config: &ProxyConfig,
    connector: TimedConnector,
) -> Result<Tunnel<TimedConnector>, HttpError> {
    let parsed = Url::parse(&config.url)
        .map_err(|error| HttpError::InvalidProxy(format!("`{}`: {error}", config.url)))?;
    if parsed.scheme() != "http" {
        return Err(HttpError::InvalidProxy(
            "only `http://` proxies are supported".to_string(),
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| HttpError::InvalidProxy("proxy URL has no host".to_string()))?;
    let port = parsed.port_or_known_default().unwrap_or(80);
    let authority = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    let proxy_uri: Uri = format!("http://{authority}")
        .parse()
        .map_err(|error| HttpError::InvalidProxy(format!("`{authority}`: {error}")))?;

    let mut tunnel = Tunnel::new(proxy_uri, connector);
    let username = parsed.username();
    if !username.is_empty() {
        let password = parsed.password().unwrap_or("");
        let credentials = format!("{username}:{password}");
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        let header = HeaderValue::from_str(&format!("Basic {encoded}"))
            .map_err(|error| HttpError::InvalidProxy(error.to_string()))?;
        tunnel = tunnel.with_auth(header);
    }
    Ok(tunnel)
}

/// A destination URI with an explicit port, for `Tunnel`.
fn destination_uri(uri: &Uri) -> Result<Uri, BoxError> {
    let scheme = uri.scheme_str().unwrap_or("http");
    let host = uri
        .host()
        .ok_or_else(|| Box::new(HttpError::InvalidUrl(uri.to_string())) as BoxError)?;
    let port = uri
        .port_u16()
        .unwrap_or(if scheme == "https" { 443 } else { 80 });
    let authority = if host.contains(':') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    };
    Uri::builder()
        .scheme(scheme)
        .authority(authority)
        .path_and_query("/")
        .build()
        .map_err(|error| Box::new(HttpError::InvalidUrl(error.to_string())) as BoxError)
}

#[derive(Debug, Clone)]
struct NoProxyRule {
    host: String,
    port: Option<u16>,
}

impl NoProxyRule {
    fn matches(&self, uri: &Uri) -> bool {
        if self.host == "*" {
            return true;
        }
        let Some(host) = uri.host() else {
            return false;
        };
        let host = host.to_ascii_lowercase();
        let host_matches = host == self.host
            || host
                .strip_suffix(&self.host)
                .is_some_and(|prefix| prefix.ends_with('.'));
        if !host_matches {
            return false;
        }
        self.port.is_none_or(|port| {
            let actual = uri
                .port_u16()
                .unwrap_or(if uri.scheme_str() == Some("https") {
                    443
                } else {
                    80
                });
            actual == port
        })
    }
}

fn parse_no_proxy(entries: &[String]) -> Vec<NoProxyRule> {
    let mut rules = Vec::new();
    for entry in entries {
        for item in entry.split(',') {
            let item = item.trim();
            if item.is_empty() {
                continue;
            }
            if item == "*" {
                rules.push(NoProxyRule {
                    host: "*".to_string(),
                    port: None,
                });
                continue;
            }
            let item = item
                .strip_prefix("http://")
                .or_else(|| item.strip_prefix("https://"))
                .unwrap_or(item);
            // A host:port split is only unambiguous when there is one colon (an
            // IPv6 literal without brackets is not a supported entry).
            let (host, port) = match item.rsplit_once(':') {
                Some((host, port))
                    if !host.contains(':') && port.chars().all(|c| c.is_ascii_digit()) =>
                {
                    (host, port.parse::<u16>().ok())
                }
                _ => (item, None),
            };
            let host = host
                .trim_start_matches("*.")
                .trim_start_matches('.')
                .to_ascii_lowercase();
            if !host.is_empty() {
                rules.push(NoProxyRule { host, port });
            }
        }
    }
    rules
}

/// Build a rustls configuration from native roots plus any explicit trust anchors
/// and client certificate.
fn build_tls(tls: &TlsConfig) -> Result<rustls::ClientConfig, HttpError> {
    let _ = ring::default_provider().install_default();

    let mut roots = RootCertStore::empty();
    let native = rustls_native_certs::load_native_certs();
    for certificate in native.certs {
        if let Err(error) = roots.add(certificate) {
            tracing::warn!(%error, "ignoring an unreadable native root certificate");
        }
    }
    for error in native.errors {
        tracing::warn!(%error, "could not load a native root certificate store");
    }
    for path in &tls.ca_certs {
        for certificate in read_certificates(path)? {
            roots
                .add(certificate)
                .map_err(|error| HttpError::Tls(format!("`{}`: {error}", path.display())))?;
        }
    }

    let builder = rustls::ClientConfig::builder().with_root_certificates(roots);
    let mut config = match (&tls.client_cert, &tls.client_key) {
        (Some(cert), Some(key)) => builder
            .with_client_auth_cert(read_certificates(cert)?, read_private_key(key)?)
            .map_err(|error| HttpError::Tls(error.to_string()))?,
        (Some(_), None) => {
            return Err(HttpError::Tls(
                "a client certificate needs a client key".to_string(),
            ))
        }
        (None, Some(_)) => {
            return Err(HttpError::Tls(
                "a client key needs a client certificate".to_string(),
            ))
        }
        (None, None) => builder.with_no_client_auth(),
    };

    if tls.insecure {
        config
            .dangerous()
            .set_certificate_verifier(Arc::new(NoCertificateVerification));
    }
    Ok(config)
}

fn read_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>, HttpError> {
    let bytes = std::fs::read(path)
        .map_err(|error| HttpError::Tls(format!("failed to read `{}`: {error}", path.display())))?;
    let text = String::from_utf8_lossy(&bytes);
    let blocks = pem_blocks(&text, "CERTIFICATE")
        .map_err(|error| HttpError::Tls(format!("`{}`: {error}", path.display())))?;
    if blocks.is_empty() {
        // A `.der` file is a single certificate with no framing.
        if bytes.first() == Some(&0x30) {
            return Ok(vec![CertificateDer::from(bytes)]);
        }
        return Err(HttpError::Tls(format!(
            "`{}` contains no certificate",
            path.display()
        )));
    }
    Ok(blocks.into_iter().map(CertificateDer::from).collect())
}

fn read_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, HttpError> {
    let bytes = std::fs::read(path)
        .map_err(|error| HttpError::Tls(format!("failed to read `{}`: {error}", path.display())))?;
    let text = String::from_utf8_lossy(&bytes);
    for (label, kind) in [
        ("PRIVATE KEY", KeyKind::Pkcs8),
        ("RSA PRIVATE KEY", KeyKind::Pkcs1),
        ("EC PRIVATE KEY", KeyKind::Sec1),
    ] {
        let blocks = pem_blocks(&text, label)
            .map_err(|error| HttpError::Tls(format!("`{}`: {error}", path.display())))?;
        if let Some(block) = blocks.into_iter().next() {
            return Ok(match kind {
                KeyKind::Pkcs8 => PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(block)),
                KeyKind::Pkcs1 => PrivateKeyDer::Pkcs1(PrivatePkcs1KeyDer::from(block)),
                KeyKind::Sec1 => PrivateKeyDer::Sec1(PrivateSec1KeyDer::from(block)),
            });
        }
    }
    if bytes.first() == Some(&0x30) {
        return Ok(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(bytes)));
    }
    Err(HttpError::Tls(format!(
        "`{}` contains no supported private key",
        path.display()
    )))
}

enum KeyKind {
    Pkcs8,
    Pkcs1,
    Sec1,
}

/// Extract every PEM block with `label`, base64-decoding its body.
fn pem_blocks(text: &str, label: &str) -> Result<Vec<Vec<u8>>, String> {
    let begin = format!("-----BEGIN {label}-----");
    let end = format!("-----END {label}-----");
    let mut blocks = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find(&begin) {
        let after_begin = &rest[start + begin.len()..];
        let Some(finish) = after_begin.find(&end) else {
            return Err(format!("unterminated `{label}` PEM block"));
        };
        let encoded: String = after_begin[..finish]
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|error| format!("invalid `{label}` base64: {error}"))?;
        blocks.push(decoded);
        rest = &after_begin[finish + end.len()..];
    }
    Ok(blocks)
}

/// Explicitly insecure certificate verification.
#[derive(Debug)]
struct NoCertificateVerification;

impl ServerCertVerifier for NoCertificateVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        ring::default_provider()
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// The connector the client is built over: direct or through a CONNECT tunnel, then
/// TLS when the URI says `https`, with the handshake timed as the span the plain
/// connector did not already claim.
///
/// The connect timeout is set on the plain connector, so a dead route fails as a
/// *connect* rather than as a generic request timeout.
#[derive(Clone)]
pub(crate) struct TimedHttpsConnector {
    inner: HttpsConnector<ProxyConnector>,
}

impl TimedHttpsConnector {
    pub(crate) fn new(
        connect_timeout: Duration,
        tls: &TlsConfig,
        proxy: Option<&ProxyConfig>,
    ) -> Result<Self, HttpError> {
        let plain = ProxyConnector::new(connect_timeout, proxy)?;
        let inner = HttpsConnectorBuilder::new()
            .with_tls_config(build_tls(tls)?)
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .wrap_connector(plain);
        Ok(Self { inner })
    }
}

impl Service<Uri> for TimedHttpsConnector {
    type Response = <HttpsConnector<ProxyConnector> as Service<Uri>>::Response;
    type Error = <HttpsConnector<ProxyConnector> as Service<Uri>>::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, uri: Uri) -> Self::Future {
        let mut inner = self.inner.clone();
        let secure = uri.scheme_str() == Some("https");
        Box::pin(async move {
            let start = Instant::now();
            let stream = inner.call(uri).await?;
            if secure {
                record(|spans| spans.handshake = Some(start.elapsed()));
            }
            Ok(stream)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uri(value: &str) -> Uri {
        value.parse().unwrap()
    }

    #[test]
    fn no_proxy_entries_match_hosts_and_subdomains() {
        let rules = parse_no_proxy(&["example.com, .internal.test, 127.0.0.1:8080".to_string()]);
        assert!(rules[0].matches(&uri("http://example.com/")));
        assert!(rules[0].matches(&uri("https://api.example.com/")));
        assert!(!rules[0].matches(&uri("https://notexample.com/")));
        assert!(rules[1].matches(&uri("http://internal.test/")));
        assert!(rules[2].matches(&uri("http://127.0.0.1:8080/")));
        assert!(!rules[2].matches(&uri("http://127.0.0.1:9090/")));
    }

    #[test]
    fn a_destination_gets_an_explicit_default_port() {
        assert_eq!(
            destination_uri(&uri("http://api.test/orders")).unwrap(),
            uri("http://api.test:80/")
        );
        assert_eq!(
            destination_uri(&uri("https://api.test/orders")).unwrap(),
            uri("https://api.test:443/")
        );
    }
}
