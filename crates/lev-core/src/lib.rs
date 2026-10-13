// SPDX-License-Identifier: MIT OR Apache-2.0
//! Levi core: the pure domain model.
//!
//! This crate has **no I/O** — no `tokio`, no `reqwest`, no filesystem — and
//! **no `serde` derives**. It owns the domain types, template resolution,
//! redaction, and the secret-resolution trait. Concrete representations (the
//! collection format in `lev-format`, the wire protocol in `lev-protocol`) convert
//! to and from these types. Everything here is testable without a runtime.
#![forbid(unsafe_code)]

pub mod context;
pub mod error;
pub mod model;
pub mod prepare;
pub mod redact;
pub mod secrets;
pub mod template;

pub use context::Context;
pub use error::{CoreError, Result};
pub use model::{
    ApiKeyLocation, Auth, Body, Collection, Id, Method, RequestDef, Scripts, VarSet, VarValue,
};
pub use prepare::{prepare_request, prepare_request_with_seed, PreparedRequest};
pub use redact::{Redactor, MASK};
pub use secrets::{MapSecretStore, Secret, SecretName, SecretStore};
pub use template::{DynamicFunctions, RenderContext, Template};
