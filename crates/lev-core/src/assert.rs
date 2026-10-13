// SPDX-License-Identifier: MIT OR Apache-2.0
//! Assertions and checks.
//!
//! An [`Assertion`] is a *declarative* expectation authored in a request file. A
//! [`Check`] is the *outcome* of one expectation. Both a declarative assertion and an
//! imperative `lev.test(...)` call from a script produce a [`Check`], so a report can
//! present them in one list without caring where they came from.
//!
//! Evaluation is deliberately **not** here: a `JsonPath` check needs to parse JSON, and
//! `lev-core` has no `serde` and no I/O. The engine evaluates; this module only defines
//! the shape.

use std::time::Duration;

/// A declarative expectation evaluated against a response.
///
/// This is a deliberately small, portable subset — it is the part of Postman's
/// `pm.test` surface that most requests actually use, expressed as data so it can be
/// authored in YAML, diffed in git, and evaluated without running code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Assertion {
    /// The response status equals this code exactly.
    Status(u16),
    /// The named header exists, and its value contains `contains` when that is set.
    Header {
        name: String,
        contains: Option<String>,
    },
    /// The response body contains this text.
    BodyContains(String),
    /// A dot path into the JSON body resolves, and equals `equals` when that is set.
    ///
    /// The path is a dot path (`id`, `user.name`, `items.0.sku`) — no wildcards or
    /// filters. That subset covers the overwhelming majority of real assertions.
    JsonPath {
        path: String,
        equals: Option<String>,
    },
    /// The total response time is under this bound.
    ResponseTimeUnder(Duration),
}

impl Assertion {
    /// A short human-readable name, used as the [`Check`] name in reports.
    pub fn name(&self) -> String {
        match self {
            Assertion::Status(code) => format!("status is {code}"),
            Assertion::Header { name, contains } => match contains {
                Some(text) => format!("header `{name}` contains `{text}`"),
                None => format!("header `{name}` exists"),
            },
            Assertion::BodyContains(text) => format!("body contains `{text}`"),
            Assertion::JsonPath { path, equals } => match equals {
                Some(value) => format!("json path `{path}` equals `{value}`"),
                None => format!("json path `{path}` exists"),
            },
            Assertion::ResponseTimeUnder(max) => {
                format!("response time under {}ms", max.as_millis())
            }
        }
    }
}

/// The outcome of one check.
///
/// `message` is present on failure (why it failed) and may be present on success.
/// A check never carries a secret: the engine redacts it before it crosses a boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub name: String,
    pub passed: bool,
    pub message: Option<String>,
}

impl Check {
    /// A check that passed.
    pub fn pass(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            message: None,
        }
    }

    /// A check that failed, with the reason.
    pub fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: false,
            message: Some(message.into()),
        }
    }

    /// A check with an explicit pass flag and an optional message.
    pub fn new(name: impl Into<String>, passed: bool, message: Option<String>) -> Self {
        Self {
            name: name.into(),
            passed,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assertions_have_readable_names() {
        assert_eq!(Assertion::Status(200).name(), "status is 200");
        assert_eq!(
            Assertion::Header {
                name: "content-type".to_string(),
                contains: Some("json".to_string()),
            }
            .name(),
            "header `content-type` contains `json`"
        );
        assert_eq!(
            Assertion::ResponseTimeUnder(Duration::from_millis(500)).name(),
            "response time under 500ms"
        );
    }

    #[test]
    fn checks_construct_concisely() {
        assert!(Check::pass("ok").passed);
        let failed = Check::fail("nope", "because");
        assert!(!failed.passed);
        assert_eq!(failed.message.as_deref(), Some("because"));
    }
}
