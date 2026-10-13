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

/// A request body in JSON-native shape.
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn request_uses_camel_case_field_names() {
        let request = RequestDto {
            id: "r1".to_string(),
            name: "R".to_string(),
            method: "GET".to_string(),
            url: "/".to_string(),
            headers: vec![],
            body: None,
            auth: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        // Empty collections are omitted entirely.
        assert!(!json.contains("headers"), "{json}");
        assert_eq!(serde_json::from_str::<RequestDto>(&json).unwrap(), request);
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
