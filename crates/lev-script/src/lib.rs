// SPDX-License-Identifier: MIT OR Apache-2.0
//! Levi script sandbox.
//!
//! A user-authored JavaScript hook — a pre-request or post-response script — runs in a
//! sandboxed QuickJS runtime with a CPU budget and a memory limit. Scripts see the
//! `lev.*` API: `lev.request`, `lev.response`, `lev.env`, `lev.test`, `lev.expect`,
//! `lev.random` and `lev.log`.
//!
//! The runtime and its context are pooled: created once per [`Sandbox`] and reused
//! across every invocation, which is what keeps per-iteration overhead low enough for a
//! load run. A failure — a thrown error, a runaway loop, an out-of-memory allocation —
//! is contained by the sandbox and never poisons the next script.
//!
//! ```no_run
//! use lev_script::{Limits, Sandbox, ScriptInput, ScriptRequest};
//!
//! let sandbox = Sandbox::new(Limits::default())?;
//! let request = ScriptRequest {
//!     method: "GET".to_string(),
//!     url: "https://api.test/".to_string(),
//!     headers: Vec::new(),
//!     body: String::new(),
//!     timeout: None,
//! };
//! let outcome = sandbox.run(
//!     "lev.test('has a url', function () { lev.expect(lev.request.url).toContain('http'); });",
//!     ScriptInput::pre_request(request),
//! )?;
//! assert!(outcome.checks.iter().all(|check| check.passed));
//! # Ok::<(), lev_script::ScriptError>(())
//! ```
#![forbid(unsafe_code)]

mod error;
mod limits;
mod model;
mod sandbox;

pub use error::ScriptError;
pub use limits::Limits;
pub use model::{Phase, ScriptInput, ScriptOutcome, ScriptRequest, ScriptResponse};
pub use sandbox::Sandbox;
