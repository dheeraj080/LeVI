// SPDX-License-Identifier: MIT OR Apache-2.0
//! The data a script sees and the data it produces.
//!
//! These types are the *interface* between the engine and the sandbox. They are plain
//! owned data on purpose: a job crosses a thread boundary (the pooled runtime lives on
//! its own thread), so nothing here borrows.

use std::collections::BTreeMap;
use std::time::Duration;

use lev_core::{Check, PreparedRequest};

/// Which hook is about to run. It decides what `lev.response` is bound to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Runs after resolution and before dispatch. May mutate the request.
    PreRequest,
    /// Runs after the response arrives. `lev.response` is bound.
    PostResponse,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::PreRequest => "pre-request",
            Phase::PostResponse => "post-response",
        }
    }
}

/// The request as a script sees it.
///
/// `headers` is exposed to JavaScript as a plain object (`name -> value`). A pre-request
/// script that touches headers therefore collapses a duplicated request header name to
/// one entry; this is documented and rare. `body` is the body as lossy UTF-8.
///
/// `timeout` is carried for round-tripping only — JavaScript cannot bind it — so a
/// pre-request rewrite cannot silently drop the request's deadline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptRequest {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub timeout: Option<Duration>,
}

impl From<&PreparedRequest> for ScriptRequest {
    fn from(prepared: &PreparedRequest) -> Self {
        Self {
            method: prepared.method.as_str().to_string(),
            url: prepared.url.clone(),
            headers: prepared.headers.clone(),
            body: String::from_utf8_lossy(&prepared.body).into_owned(),
            timeout: prepared.timeout,
        }
    }
}

impl ScriptRequest {
    /// Fold script mutations back into a request ready to dispatch.
    ///
    /// The method is normalised through [`lev_core::Method::parse`] so a script can set
    /// it from a string, and the body is taken as UTF-8 bytes. The deadline survives
    /// the round trip (see [`ScriptRequest`]).
    pub fn into_prepared(self) -> PreparedRequest {
        PreparedRequest {
            method: lev_core::Method::parse(&self.method),
            url: self.url,
            headers: self.headers,
            body: self.body.into_bytes(),
            timeout: self.timeout,
        }
    }
}

/// The response as a script sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct ScriptResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    /// Total time in milliseconds, for `responseTimeUnder` style checks.
    pub time_ms: f64,
}

/// One unit of work for the sandbox.
#[derive(Debug, Clone)]
pub struct ScriptInput {
    pub phase: Phase,
    /// The resolved request. For [`Phase::PreRequest`] a script may change it.
    pub request: ScriptRequest,
    /// The response, bound only for [`Phase::PostResponse`].
    pub response: Option<ScriptResponse>,
    /// The resolved variable scope. Secrets included — see the crate docs for why this
    /// is safe (the sandbox is inside the engine and every output is redacted).
    pub vars: BTreeMap<String, String>,
    /// Seed for `lev.random`, so runs are reproducible.
    pub seed: u64,
}

impl ScriptInput {
    /// A pre-request input with no variables.
    pub fn pre_request(request: ScriptRequest) -> Self {
        Self {
            phase: Phase::PreRequest,
            request,
            response: None,
            vars: BTreeMap::new(),
            seed: 0,
        }
    }
}

/// What a script produced.
#[derive(Debug, Clone)]
pub struct ScriptOutcome {
    /// The request after any pre-request mutations (unchanged for post-response).
    pub request: ScriptRequest,
    /// The variable scope after any `lev.env.set` calls.
    pub vars: BTreeMap<String, String>,
    /// Every `lev.test(...)` / `lev.expect(...)` outcome, in order.
    pub checks: Vec<Check>,
    /// Everything the script logged. Must be redacted before it is shown.
    pub logs: Vec<String>,
}
