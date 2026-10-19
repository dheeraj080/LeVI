// SPDX-License-Identifier: MIT OR Apache-2.0
//! Levi core: the pure domain model.
//!
//! This crate has **no I/O** — no `tokio`, no `reqwest`, no filesystem — and
//! **no `serde` derives**. It owns the domain types, template resolution,
//! redaction, and the secret-resolution trait. Concrete representations (the
//! collection format in `lev-format`, the wire protocol in `lev-protocol`) convert
//! to and from these types. Everything here is testable without a runtime.
#![forbid(unsafe_code)]

pub mod assert;
pub mod context;
pub mod encoding;
pub mod error;
pub mod model;
pub mod prepare;
pub mod redact;
pub mod scenario;
pub mod secrets;
pub mod template;
pub mod threshold;

pub use assert::{Assertion, Check};
pub use context::Context;
pub use encoding::{base64_decode, base64_encode};
pub use error::{CoreError, Result};
pub use model::{
    ApiKeyLocation, Auth, Body, Collection, Id, Method, RequestDef, Scripts, VarSet, VarValue,
};
pub use prepare::{
    compile_request, prepare_request, prepare_request_with_seed, CompiledRequest, PreparedRequest,
};
pub use redact::{Redactor, MASK};
pub use scenario::{Executor, Scenario, Step};
pub use secrets::{MapSecretStore, Secret, SecretName, SecretStore};
pub use template::{DynamicFunctions, RenderContext, Template};
pub use threshold::{Comparison, Threshold, ThresholdMetric, ThresholdStat};
