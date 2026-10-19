// SPDX-License-Identifier: MIT OR Apache-2.0
//! Where a request's connection phases are measured, and why they are often absent.
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
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use hyper::Uri;
use hyper_rustls::{HttpsConnector, HttpsConnectorBuilder};
use hyper_util::client::legacy::connect::dns::{GaiResolver, Name};
use hyper_util::client::legacy::connect::HttpConnector;
use tower_service::Service;

use crate::error::HttpError;
use crate::timing::Timings;

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
    /// DNS + TCP, as timed by the plain connector.
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

/// The connector the client is built over: plain when the URI says `http`, TLS when
/// it says `https`, with the handshake timed as the span the plain connector did not
/// already claim.
///
/// The connect timeout is set on the plain connector, so a dead route fails as a
/// *connect* rather than as a generic request timeout.
#[derive(Clone)]
pub(crate) struct TimedHttpsConnector {
    inner: HttpsConnector<TimedConnector>,
}

impl TimedHttpsConnector {
    pub(crate) fn new(connect_timeout: Duration) -> Result<Self, HttpError> {
        let mut plain = HttpConnector::new_with_resolver(TimedResolver::default());
        plain.set_connect_timeout(Some(connect_timeout));

        let inner = HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|error| HttpError::Tls(error.to_string()))?
            .https_or_http()
            .enable_http1()
            .enable_http2()
            .wrap_connector(TimedConnector { inner: plain });
        Ok(Self { inner })
    }
}

impl Service<Uri> for TimedHttpsConnector {
    type Response = <HttpsConnector<TimedConnector> as Service<Uri>>::Response;
    type Error = <HttpsConnector<TimedConnector> as Service<Uri>>::Error;
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
