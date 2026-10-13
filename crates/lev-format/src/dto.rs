// SPDX-License-Identifier: MIT OR Apache-2.0
//! On-disk DTOs.
//!
//! These are the *only* types that carry `serde` derives. They mirror the core
//! domain types so the file format can evolve without touching `lev-core`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// `collection.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CollectionDto {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, VarValueDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

/// A variable is either a literal string or a reference to a secret by name.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum VarValueDto {
    Secret { secret: String },
    Plain(String),
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ApiKeyLocationDto {
    #[default]
    Header,
    Query,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "camelCase")]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HeaderDto {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FormFieldDto {
    pub name: String,
    pub value: String,
}

/// The fields of a `raw` body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RawBodyDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "contentType"
    )]
    pub content_type: Option<String>,
    pub content: String,
}

/// Request body.
///
/// Untagged, single-key form — the natural, git-friendly shape:
///
/// ```yaml
/// body:
///   json: |
///     {"sku": "{{$uuid}}"}
/// ```
///
/// An absent body is `None` on [`RequestDto`], not a variant here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum BodyDto {
    Json { json: String },
    Form { form: Vec<FormFieldDto> },
    Raw { raw: RawBodyDto },
    File { file: String },
}

/// `requests/*.yaml` — one request per file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestDto {
    pub id: String,
    pub name: String,
    #[serde(default = "default_method")]
    pub method: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<HeaderDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<BodyDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthDto>,
}

/// `environments/<env>.yaml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VarsFileDto {
    #[serde(default)]
    pub vars: BTreeMap<String, VarValueDto>,
}

fn default_method() -> String {
    "GET".to_string()
}
