// SPDX-License-Identifier: MIT OR Apache-2.0
//! The domain model.
//!
//! These types are **never serialized**: there is no `serde` here on purpose. The
//! on-disk representation lives in `lev-format`, the wire representation in
//! `lev-protocol`, and both convert to and from these types.

use std::collections::BTreeMap;
use std::time::Duration;

use crate::assert::Assertion;
use crate::secrets::SecretName;
use crate::template::Template;

/// Stable identifier (slug) for a collection or request.
pub type Id = String;

/// Variable layers. The caller applies precedence by merging: the collection's `vars`
/// first, then the environment overlay that wins over them, then anything a script adds
/// at run time with `lev.env.set`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VarSet {
    pub entries: BTreeMap<String, VarValue>,
}

impl VarSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, name: &str) -> Option<&VarValue> {
        self.entries.get(name)
    }

    pub fn insert(&mut self, name: impl Into<String>, value: VarValue) {
        self.entries.insert(name.into(), value);
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Return a new set with `overlay` taking precedence over `self`.
    pub fn merged_with(&self, overlay: &VarSet) -> VarSet {
        let mut out = self.clone();
        for (name, value) in &overlay.entries {
            out.entries.insert(name.clone(), value.clone());
        }
        out
    }
}

/// A variable value is either a literal or a reference to a secret.
#[derive(Debug, Clone, PartialEq)]
pub enum VarValue {
    Plain(String),
    Secret(SecretName),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiKeyLocation {
    Header,
    Query,
}

/// Authentication, applied at collection level (default) or request level (override).
#[derive(Debug, Clone, PartialEq)]
pub enum Auth {
    Bearer(Template),
    Basic {
        username: Template,
        password: VarValue,
    },
    ApiKey {
        name: String,
        value: VarValue,
        location: ApiKeyLocation,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Custom(String),
}

impl Method {
    pub fn as_str(&self) -> &str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Patch => "PATCH",
            Method::Delete => "DELETE",
            Method::Head => "HEAD",
            Method::Options => "OPTIONS",
            Method::Custom(other) => other.as_str(),
        }
    }

    /// Parse a method, falling back to [`Method::Custom`] for anything unrecognised.
    pub fn parse(value: &str) -> Method {
        match value.trim().to_ascii_uppercase().as_str() {
            "GET" => Method::Get,
            "POST" => Method::Post,
            "PUT" => Method::Put,
            "PATCH" => Method::Patch,
            "DELETE" => Method::Delete,
            "HEAD" => Method::Head,
            "OPTIONS" => Method::Options,
            other => Method::Custom(other.to_string()),
        }
    }
}

/// One part of a `multipart/form-data` body.
#[derive(Debug, Clone, PartialEq)]
pub struct MultipartPart {
    /// The form field name.
    pub name: String,
    pub value: MultipartValue,
}

/// What one multipart part carries.
#[derive(Debug, Clone, PartialEq)]
pub enum MultipartValue {
    /// Text, templated at send time.
    Text(String),
    /// A file reference. Resolved to [`MultipartValue::Bytes`] by the loader before
    /// the request is sent, because `lev-core` is pure and cannot read the disk.
    File {
        path: Template,
        filename: Option<String>,
        content_type: Option<String>,
    },
    /// File bytes, already read and carried verbatim.
    Bytes {
        filename: String,
        content_type: Option<String>,
        content: Vec<u8>,
    },
}

/// Request body. `Json`, `Graphql` and text parts are stored as source text so they
/// can be templated.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum Body {
    #[default]
    None,
    Json(String),
    Form(Vec<(String, String)>),
    Raw {
        content_type: Option<String>,
        content: String,
    },
    /// A `multipart/form-data` body. File parts are resolved by the loader, exactly
    /// like [`Body::File`].
    Multipart(Vec<MultipartPart>),
    /// A GraphQL operation, transported as the standard JSON envelope.
    Graphql {
        query: String,
        variables: Option<String>,
        operation_name: Option<String>,
    },
    /// A body read from a file. `lev-core` is pure, so the reference is resolved by a
    /// loader that can do I/O (`lev-format`), which replaces it with [`Body::Bytes`]
    /// before the request reaches an engine.
    File(Template),
    /// Bytes, already read from a file (or authored inline as base64) and carried
    /// verbatim. Nothing here is templated: a file is a file.
    Bytes {
        content_type: Option<String>,
        content: Vec<u8>,
    },
}

/// Pre- and post-request scripts.
///
/// Authored as JavaScript and executed in the `lev-script` sandbox: the pre-request
/// script may mutate the resolved request, the post-response script may inspect the
/// response and record checks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scripts {
    pub pre_request: Option<String>,
    pub post_response: Option<String>,
}

impl Scripts {
    /// True when neither hook is present, so callers can skip the sandbox entirely.
    pub fn is_empty(&self) -> bool {
        self.pre_request.is_none() && self.post_response.is_none()
    }
}

/// A collection of requests plus shared variables, auth, and base URL.
#[derive(Debug, Clone, PartialEq)]
pub struct Collection {
    pub id: Id,
    pub name: String,
    pub vars: VarSet,
    pub auth: Option<Auth>,
    pub base_url: Option<Template>,
}

impl Collection {
    /// An anonymous collection, used when a request file stands alone.
    pub fn empty() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            vars: VarSet::new(),
            auth: None,
            base_url: None,
        }
    }
}

/// A single request definition.
#[derive(Debug, Clone, PartialEq)]
pub struct RequestDef {
    pub id: Id,
    pub name: String,
    pub method: Method,
    pub url: Template,
    pub headers: Vec<(Template, Template)>,
    pub body: Body,
    pub auth: Option<Auth>,
    pub scripts: Scripts,
    /// Declarative expectations, evaluated against the response after any
    /// post-response script. Each becomes a [`Check`](crate::Check) in the result.
    pub assertions: Vec<Assertion>,
    /// The request's own deadline, as authored (`timeout: 5s`). Overrides the
    /// transport's default request timeout; `None` means "use the transport's".
    pub timeout: Option<Duration>,
}
