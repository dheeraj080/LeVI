// SPDX-License-Identifier: MIT OR Apache-2.0
//! Evaluating declarative assertions against a response.
//!
//! This lives in the engine because a `jsonPath` assertion has to parse JSON, and
//! `lev-core` deliberately owns no `serde`. The engine sees the **unredacted**
//! response; the resulting check messages are redacted before they cross the boundary.

use lev_core::{Assertion, Check};
use lev_http::Response;
use serde_json::Value;

/// Evaluate every assertion, in order, against the response.
pub fn evaluate(assertions: &[Assertion], response: &Response) -> Vec<Check> {
    assertions
        .iter()
        .map(|assertion| evaluate_one(assertion, response))
        .collect()
}

fn evaluate_one(assertion: &Assertion, response: &Response) -> Check {
    let name = assertion.name();
    match assertion {
        Assertion::Status(code) => {
            if response.status == *code {
                Check::pass(name)
            } else {
                Check::fail(
                    name,
                    format!("expected status {code}, got {}", response.status),
                )
            }
        }
        Assertion::Header {
            name: header,
            contains,
        } => match find_header(&response.headers, header) {
            None => Check::fail(name, format!("header `{header}` is absent")),
            Some(value) => match contains {
                Some(needle) if !value.contains(needle.as_str()) => Check::fail(
                    name,
                    format!("header `{header}` is `{value}`, which does not contain `{needle}`"),
                ),
                _ => Check::pass(name),
            },
        },
        Assertion::BodyContains(text) => {
            let body = String::from_utf8_lossy(&response.body);
            if body.contains(text.as_str()) {
                Check::pass(name)
            } else {
                Check::fail(name, format!("body does not contain `{text}`"))
            }
        }
        Assertion::JsonPath { path, equals } => {
            let body = String::from_utf8_lossy(&response.body);
            let json: Value = match serde_json::from_str(&body) {
                Ok(json) => json,
                Err(error) => return Check::fail(name, format!("body is not valid JSON: {error}")),
            };
            match resolve(&json, path) {
                None => Check::fail(name, format!("json path `{path}` was not found")),
                Some(value) => match equals {
                    Some(expected) if &value_text(value) != expected => Check::fail(
                        name,
                        format!(
                            "json path `{path}` is `{}`, expected `{expected}`",
                            value_text(value)
                        ),
                    ),
                    _ => Check::pass(name),
                },
            }
        }
        Assertion::ResponseTimeUnder(max) => {
            if response.timings.total <= *max {
                Check::pass(name)
            } else {
                Check::fail(
                    name,
                    format!(
                        "took {}ms, budget {}ms",
                        response.timings.total.as_millis(),
                        max.as_millis()
                    ),
                )
            }
        }
    }
}

fn find_header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

/// Resolve a dot path (`id`, `user.name`, `items.0.sku`) into a JSON value.
///
/// A deliberate subset: no wildcards, filters, or slices. Array indices are numeric
/// segments.
fn resolve<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = root;
    for segment in path.split('.') {
        current = match current {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use lev_http::Timings;
    use std::time::Duration;

    fn response(status: u16, body: &str) -> Response {
        Response {
            status,
            version: "HTTP/1.1".to_string(),
            headers: vec![
                ("content-type".to_string(), "application/json".to_string()),
                ("X-Trace".to_string(), "abc123".to_string()),
            ],
            body: Bytes::from(body.to_string()),
            timings: Timings {
                total: Duration::from_millis(12),
                ..Timings::default()
            },
        }
    }

    fn single(assertion: Assertion, response: &Response) -> Check {
        evaluate(std::slice::from_ref(&assertion), response)
            .into_iter()
            .next()
            .expect("one check")
    }

    #[test]
    fn status_matches_or_reports_the_real_one() {
        let response = response(201, "{}");
        assert!(single(Assertion::Status(201), &response).passed);
        let failed = single(Assertion::Status(200), &response);
        assert!(!failed.passed);
        assert!(failed.message.unwrap().contains("got 201"));
    }

    #[test]
    fn headers_match_case_insensitively() {
        let response = response(200, "{}");
        assert!(
            single(
                Assertion::Header {
                    name: "Content-Type".to_string(),
                    contains: Some("json".to_string()),
                },
                &response
            )
            .passed
        );
        assert!(
            !single(
                Assertion::Header {
                    name: "content-type".to_string(),
                    contains: Some("xml".to_string()),
                },
                &response
            )
            .passed
        );
    }

    #[test]
    fn body_contains_and_json_path() {
        let response = response(
            200,
            "{\"ok\":true,\"user\":{\"id\":7,\"roles\":[\"admin\"]}}",
        );
        assert!(
            single(
                Assertion::BodyContains("\"ok\":true".to_string()),
                &response
            )
            .passed
        );
        assert!(
            single(
                Assertion::JsonPath {
                    path: "user.id".to_string(),
                    equals: Some("7".to_string()),
                },
                &response
            )
            .passed
        );
        assert!(
            single(
                Assertion::JsonPath {
                    path: "user.roles.0".to_string(),
                    equals: Some("admin".to_string()),
                },
                &response
            )
            .passed
        );
        let missing = single(
            Assertion::JsonPath {
                path: "user.nope".to_string(),
                equals: None,
            },
            &response,
        );
        assert!(!missing.passed);
    }

    #[test]
    fn json_path_on_a_non_json_body_fails_cleanly() {
        let response = response(200, "not json");
        let check = single(
            Assertion::JsonPath {
                path: "a".to_string(),
                equals: None,
            },
            &response,
        );
        assert!(!check.passed);
        assert!(check.message.unwrap().contains("not valid JSON"));
    }

    #[test]
    fn response_time_budget() {
        let response = response(200, "{}");
        assert!(
            single(
                Assertion::ResponseTimeUnder(Duration::from_millis(50)),
                &response
            )
            .passed
        );
        assert!(
            !single(
                Assertion::ResponseTimeUnder(Duration::from_millis(5)),
                &response
            )
            .passed
        );
    }
}
