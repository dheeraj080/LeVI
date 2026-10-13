// SPDX-License-Identifier: MIT OR Apache-2.0
//! The domain model.
//!
//! These types are **never serialized**: there is no `serde` here on purpose. The
//! on-disk representation lives in `lev-format`, the wire representation in
//! `lev-protocol`, and both convert to and from these types.

use std::collections::BTreeMap;

use crate::secrets::SecretName;
use crate::template::Template;

/// Stable identifier (slug) for a collection or request.
pub type Id = String;

/// Variable layers. Precedence is applied by the caller: global < collection <
/// environment < runtime < iteration.
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

/// Request body. `Json` is stored as source text so it can be templated.
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
    /// A body read from a file at send time. Requires I/O, so resolution lives
    /// outside `lev-core`; carrying the reference here keeps the model complete.
    File(Template),
}

/// Pre- and post-request scripts. Execution is Phase 2; the shape is fixed here.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scripts {
    pub pre_request: Option<String>,
    pub post_response: Option<String>,
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
}
