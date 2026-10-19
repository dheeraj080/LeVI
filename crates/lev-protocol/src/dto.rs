// SPDX-License-Identifier: MIT OR Apache-2.0
//! The wire DTOs.
//!
//! These are the **only** types that cross the boundary. They mirror the domain
//! model in `lev-core` but are a separate representation on purpose: renaming a
//! domain field must not become a breaking IPC change.
//!
//! Two conventions are worth calling out:
//!
//! - The wire is JSON, so it uses `camelCase` and JSON-native shapes. The on-disk
//!   YAML format is *different* — tags, keys and body shapes differ — which is the
//!   point: the two representations evolve independently.
//! - Anything that could contain a secret is **already redacted** before it is put
//!   in a DTO. Secret *values* never appear on the wire; secrets are referenced by
//!   name ([`VarValueDto::Secret`]) and resolved inside the engine.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One header, preserving order and duplicate names.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderDto {
    pub name: String,
    pub value: String,
}

/// One field of a form body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FormFieldDto {
    pub name: String,
    pub value: String,
}

/// A variable is either a literal or a reference to a secret **by name**.
///
/// The secret's value is never sent over the wire: the engine resolves the name
/// against its own store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VarValueDto {
    Secret { secret: String },
    Plain(String),
}

/// Where an API key goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ApiKeyLocationDto {
    #[default]
    Header,
    Query,
}

/// Authentication.
///
/// Tagged `kind` on the wire — deliberately *not* the on-disk `type` tag, so the
/// file format and the protocol can diverge without a coordinated change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AuthDto {
    Bearer {
        token: String,
    },
    Basic {
        username: String,
        password: VarValueDto,
    },
    ApiKey {
        name: String,
        value: VarValueDto,
        #[serde(default)]
        location: ApiKeyLocationDto,
    },
}

/// The request body in JSON-native shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BodyDto {
    /// A JSON body, stored as source text so it can still be templated.
    Json {
        content: String,
    },
    Form {
        fields: Vec<FormFieldDto>,
    },
    Raw {
        #[serde(
            rename = "contentType",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        content_type: Option<String>,
        content: String,
    },
    /// A body read from a file at send time.
    File {
        path: String,
    },
    /// A binary body, carried base64 so the engine needs no filesystem access.
    ///
    /// The CLI resolves a `file` body to one of these before sending, so a remote or
    /// sandboxed engine never opens a path itself. A later agent that confines paths
    /// to a root of its own can resolve `file` directly.
    Bytes {
        #[serde(
            rename = "contentType",
            default,
            skip_serializing_if = "Option::is_none"
        )]
        content_type: Option<String>,
        base64: String,
    },
}

/// The two JavaScript hooks, carried to the engine as source text.
///
/// Scripts are executable code, so they only ever cross the in-process channel or
/// stdio to a child the user themselves spawned — never to a remote agent (Phase 6 will
/// require an explicit opt-in).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScriptsDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pre_request: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub post_response: Option<String>,
}

impl ScriptsDto {
    pub fn is_empty(&self) -> bool {
        self.pre_request.is_none() && self.post_response.is_none()
    }
}

/// A declarative expectation, tagged `kind` on the wire like the other unions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AssertionDto {
    Status {
        code: u16,
    },
    Header {
        name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        contains: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    BodyContains {
        text: String,
    },
    JsonPath {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        equals: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    ResponseTimeUnder {
        max_ms: u64,
    },
}

/// One check outcome: from a declarative assertion or a script's `lev.test(...)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CheckDto {
    pub name: String,
    pub passed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// A request as authored: templates are intact and resolved by the engine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestDto {
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<HeaderDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scripts: Option<ScriptsDto>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<AssertionDto>,
    /// The request's own deadline in milliseconds; the engine cuts the send off there.
    /// Absent means "use the engine's default request timeout" — never "unbounded".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}

/// The scope a request is resolved against.
///
/// This is the collection defaults plus the selected environment overlay, already
/// merged by the client into the precedence order
/// `global < collection < environment`. The engine still owns template resolution,
/// secret resolution and auth application — this DTO only carries configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvDto {
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, VarValueDto>,
    /// The collection base URL, used to resolve a relative request URL.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// The collection-level default auth, applied when the request has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthDto>,
    /// Seed for dynamic functions, so a run is reproducible.
    #[serde(default)]
    pub seed: u64,
}

/// Per-phase timings, in milliseconds, for the wire.
///
/// Durations become `f64` milliseconds because that is what a UI wants to render;
/// the engine keeps `Duration` internally.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingsDto {
    pub total_ms: f64,
    pub ttfb_ms: f64,
    pub receive_ms: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tls_ms: Option<f64>,
}

/// The request as actually sent, **already redacted by the engine**.
///
/// Giving the client a redacted echo means the UI can show exactly what went out
/// without ever holding a secret or a live auth header.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EchoDto {
    pub method: String,
    pub url: String,
    pub headers: Vec<HeaderDto>,
    pub body_len: usize,
}

/// The response, **already redacted by the engine**.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResponseDto {
    pub status: u16,
    pub version: String,
    pub headers: Vec<HeaderDto>,
    /// The response body as lossy UTF-8, with known secrets scrubbed.
    pub body: String,
    /// Length of the (redacted) body in bytes.
    pub body_len: usize,
    pub timings: TimingsDto,
    /// How many attempts this response took.
    ///
    /// A client that retried a transient failure reports more than one, so a response
    /// that looks slow can be read as what it actually was: several requests, of
    /// which only this one answered. Absent on the wire means 1.
    #[serde(default = "one_attempt")]
    pub attempts: u32,
}

/// The value of [`ResponseDto::attempts`] for a client that never retried anything.
fn one_attempt() -> u32 {
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::Message;

    #[test]
    fn var_value_is_either_a_plain_string_or_a_secret_name() {
        let plain: VarValueDto = serde_json::from_str("\"hello\"").unwrap();
        assert_eq!(plain, VarValueDto::Plain("hello".to_string()));

        let secret: VarValueDto = serde_json::from_str("{\"secret\":\"token\"}").unwrap();
        assert_eq!(
            secret,
            VarValueDto::Secret {
                secret: "token".to_string()
            }
        );
    }

    #[test]
    fn auth_uses_the_kind_tag_and_camel_case() {
        let auth = AuthDto::ApiKey {
            name: "X-Api-Key".to_string(),
            value: VarValueDto::Secret {
                secret: "token".to_string(),
            },
            location: ApiKeyLocationDto::Query,
        };
        let json = serde_json::to_string(&auth).unwrap();
        assert!(json.contains("\"kind\":\"apiKey\""), "{json}");
        assert!(json.contains("\"location\":\"query\""), "{json}");
        assert_eq!(serde_json::from_str::<AuthDto>(&json).unwrap(), auth);
    }

    #[test]
    fn body_uses_the_kind_tag() {
        let body = BodyDto::Json {
            content: "{}".to_string(),
        };
        assert_eq!(
            serde_json::to_string(&body).unwrap(),
            "{\"kind\":\"json\",\"content\":\"{}\"}"
        );

        let raw = BodyDto::Raw {
            content_type: Some("text/plain".to_string()),
            content: "hi".to_string(),
        };
        let json = serde_json::to_string(&raw).unwrap();
        assert!(json.contains("\"contentType\":\"text/plain\""), "{json}");
    }

    #[test]
    fn a_binary_body_crosses_the_wire_as_base64() {
        // The engine has no filesystem access, so a body read from a file arrives as
        // bytes. Base64 is lossless and the content type rides along, or a target
        // could not tell a PNG from a blob.
        let body = BodyDto::Bytes {
            content_type: Some("image/png".to_string()),
            base64: "iVBORw0KGgo=".to_string(),
        };
        let json = serde_json::to_string(&body).unwrap();
        assert!(json.contains("\"kind\":\"bytes\""), "{json}");
        assert!(json.contains("\"contentType\":\"image/png\""), "{json}");
        assert_eq!(serde_json::from_str::<BodyDto>(&json).unwrap(), body);
    }

    #[test]
    fn request_uses_camel_case_field_names() {
        let request = RequestDto {
            id: "r1".to_string(),
            name: "R".to_string(),
            method: "GET".to_string(),
            url: "/".to_string(),
            headers: vec![],
            body: None,
            auth: None,
            scripts: None,
            assertions: vec![],
            timeout_ms: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        // Empty collections are omitted entirely.
        assert!(!json.contains("headers"), "{json}");
        assert_eq!(serde_json::from_str::<RequestDto>(&json).unwrap(), request);
    }

    #[test]
    fn a_deadline_crosses_the_wire_in_milliseconds() {
        let request = RequestDto {
            id: "r1".to_string(),
            name: "R".to_string(),
            method: "GET".to_string(),
            url: "/".to_string(),
            headers: vec![],
            body: None,
            auth: None,
            scripts: None,
            assertions: vec![],
            timeout_ms: Some(5_000),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"timeoutMs\":5000"), "{json}");
        assert_eq!(serde_json::from_str::<RequestDto>(&json).unwrap(), request);
    }

    #[test]
    fn scripts_and_assertions_round_trip() {
        let request = RequestDto {
            id: "r1".to_string(),
            name: "R".to_string(),
            method: "POST".to_string(),
            url: "/things".to_string(),
            headers: vec![],
            body: None,
            auth: None,
            scripts: Some(ScriptsDto {
                pre_request: Some("lev.log('hi');".to_string()),
                post_response: None,
            }),
            assertions: vec![
                AssertionDto::Status { code: 201 },
                AssertionDto::Header {
                    name: "content-type".to_string(),
                    contains: Some("json".to_string()),
                },
                AssertionDto::ResponseTimeUnder { max_ms: 500 },
            ],
            timeout_ms: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("\"preRequest\":\"lev.log('hi');\""), "{json}");
        assert!(json.contains("\"kind\":\"status\""), "{json}");
        assert!(json.contains("\"maxMs\":500"), "{json}");
        assert_eq!(serde_json::from_str::<RequestDto>(&json).unwrap(), request);
    }

    #[test]
    fn request_result_carries_checks_and_logs() {
        let result = Message::RequestResult {
            request: EchoDto {
                method: "GET".to_string(),
                url: "https://api.test/".to_string(),
                headers: vec![],
                body_len: 0,
            },
            response: ResponseDto {
                status: 200,
                version: "HTTP/1.1".to_string(),
                headers: vec![],
                body: String::new(),
                body_len: 0,
                timings: TimingsDto::default(),
                attempts: 1,
            },
            checks: vec![CheckDto {
                name: "status is 200".to_string(),
                passed: true,
                message: None,
            }],
            logs: vec!["hello".to_string()],
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"checks\""), "{json}");
        assert!(json.contains("\"logs\""), "{json}");
        assert_eq!(serde_json::from_str::<Message>(&json).unwrap(), result);
    }

    #[test]
    fn a_response_without_attempts_is_one() {
        // What an engine that predates v0.3 sends. The field is new, and its absence
        // is the whole of the old contract: one attempt, nothing retried.
        let json = "{\"status\":200,\"version\":\"HTTP/1.1\",\"headers\":[],\"body\":\"\",\"bodyLen\":0,\"timings\":{\"totalMs\":1.0,\"ttfbMs\":1.0,\"receiveMs\":0.0}}";
        let response: ResponseDto = serde_json::from_str(json).unwrap();
        assert_eq!(response.attempts, 1);
    }

    #[test]
    fn env_carries_the_base_url_and_a_seed() {
        let mut env = EnvDto {
            base_url: Some("https://api.test".to_string()),
            seed: 7,
            ..EnvDto::default()
        };
        env.vars.insert(
            "host".to_string(),
            VarValueDto::Plain("api.test".to_string()),
        );
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("\"baseUrl\":\"https://api.test\""), "{json}");
        assert!(json.contains("\"seed\":7"), "{json}");
    }
}
