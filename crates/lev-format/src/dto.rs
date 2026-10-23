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

/// The fields of a `bytes` body: binary content, written as base64.
///
/// ```yaml
/// body:
///   bytes:
///     contentType: image/png
///     base64: iVBORw0KGgo=
/// ```
///
/// This is how a body read from a `file` is spelled once it has been resolved: the
/// loader swaps the reference for the bytes it read.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BytesBodyDto {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "contentType"
    )]
    pub content_type: Option<String>,
    pub base64: String,
}

/// One part of a `multipart` body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MultipartPartDto {
    pub name: String,
    /// Text content. Exactly one of `value`, `file` or `base64` must be present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// A path, relative to the collection directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The filename sent to the server. Defaults to the path's file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    /// File bytes, written as base64 after a `file:` part has been resolved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
}

/// The fields of a `graphql` body.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GraphqlBodyDto {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variables: Option<String>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        rename = "operationName"
    )]
    pub operation_name: Option<String>,
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
/// A body takes **exactly one** of these keys, and deserialization enforces that: two keys
/// (or none) is an error naming what it found, rather than a silent pick that drops the
/// other. An absent body is `None` on [`RequestDto`], not a variant here.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(untagged)]
pub enum BodyDto {
    Json { json: String },
    Form { form: Vec<FormFieldDto> },
    Multipart { multipart: Vec<MultipartPartDto> },
    Graphql { graphql: GraphqlBodyDto },
    Raw { raw: RawBodyDto },
    File { file: String },
    Bytes { bytes: BytesBodyDto },
}

/// The on-disk body shape: every key optional, so deserialization can see *all* of them
/// and count. `#[serde(untagged)]` on [`BodyDto`] would instead take the first variant that
/// matched and silently drop the rest — exactly the mistake this exists to prevent.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BodyFields {
    json: Option<String>,
    form: Option<Vec<FormFieldDto>>,
    multipart: Option<Vec<MultipartPartDto>>,
    graphql: Option<GraphqlBodyDto>,
    raw: Option<RawBodyDto>,
    file: Option<String>,
    bytes: Option<BytesBodyDto>,
}

impl<'de> Deserialize<'de> for BodyDto {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let BodyFields {
            json,
            form,
            multipart,
            graphql,
            raw,
            file,
            bytes,
        } = BodyFields::deserialize(deserializer)?;

        let refused = |present: [Option<&str>; 7]| {
            let present: Vec<&str> = present.into_iter().flatten().collect();
            let found = if present.is_empty() {
                "found an empty body".to_string()
            } else {
                let names: Vec<String> = present.iter().map(|name| format!("`{name}`")).collect();
                format!("found {}", names.join(" and "))
            };
            <D::Error as serde::de::Error>::custom(format!(
                "a body takes exactly one of `json`, `form`, `multipart`, `graphql`, `raw`, \
                 `file`, `bytes`; {found}"
            ))
        };

        match (json, form, multipart, graphql, raw, file, bytes) {
            (Some(json), None, None, None, None, None, None) => Ok(BodyDto::Json { json }),
            (None, Some(form), None, None, None, None, None) => Ok(BodyDto::Form { form }),
            (None, None, Some(multipart), None, None, None, None) => {
                Ok(BodyDto::Multipart { multipart })
            }
            (None, None, None, Some(graphql), None, None, None) => Ok(BodyDto::Graphql { graphql }),
            (None, None, None, None, Some(raw), None, None) => Ok(BodyDto::Raw { raw }),
            (None, None, None, None, None, Some(file), None) => Ok(BodyDto::File { file }),
            (None, None, None, None, None, None, Some(bytes)) => Ok(BodyDto::Bytes { bytes }),
            (json, form, multipart, graphql, raw, file, bytes) => Err(refused([
                json.as_ref().map(|_| "json"),
                form.as_ref().map(|_| "form"),
                multipart.as_ref().map(|_| "multipart"),
                graphql.as_ref().map(|_| "graphql"),
                raw.as_ref().map(|_| "raw"),
                file.as_ref().map(|_| "file"),
                bytes.as_ref().map(|_| "bytes"),
            ])),
        }
    }
}

/// The two JavaScript hooks, authored as block scalars.
///
/// ```yaml
/// scripts:
///   preRequest: |
///     lev.env.set('trace', lev.random.uuid());
///   postResponse: |
///     lev.test('ok', lev.response.status === 200);
/// ```
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
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

/// The `header` assertion payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HeaderAssertionDto {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contains: Option<String>,
}

/// The `jsonPath` assertion payload.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JsonPathAssertionDto {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub equals: Option<String>,
}

/// A declarative expectation, in the most readable single-key form:
///
/// ```yaml
/// assertions:
///   - status: 200
///   - header: { name: content-type, contains: application/json }
///   - bodyContains: '"ok":true'
///   - jsonPath: { path: id, equals: "7" }
///   - responseTimeUnderMs: 500
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum AssertionDto {
    Status {
        status: u16,
    },
    Header {
        header: HeaderAssertionDto,
    },
    #[serde(rename_all = "camelCase")]
    BodyContains {
        body_contains: String,
    },
    #[serde(rename_all = "camelCase")]
    JsonPath {
        json_path: JsonPathAssertionDto,
    },
    #[serde(rename_all = "camelCase")]
    ResponseTime {
        response_time_under_ms: u64,
    },
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
    #[serde(default, skip_serializing_if = "ScriptsDto::is_empty")]
    pub scripts: ScriptsDto,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assertions: Vec<AssertionDto>,
    /// The request's own deadline: `timeout: 5s`. Overrides the run or transport
    /// default; absent means "use the default".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<DurationDto>,
}

/// `environments/<env>.yaml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct VarsFileDto {
    #[serde(default)]
    pub vars: BTreeMap<String, VarValueDto>,
}

/// A duration in the on-disk format.
///
/// `"30s"`, `"500ms"`, `"2m"`, `"1h"` — or a bare number, which is read as seconds.
/// A unit is the readable form and is what the writer emits; the bare number exists so
/// `duration: 30` is not a surprising parse error.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum DurationDto {
    Text(String),
    Seconds(u64),
}

/// The `executor` block: how the scenario drives load.
///
/// ```yaml
/// executor:
///   constantVus: { vus: 10, duration: 30s }
/// ```
///
/// A struct with optional variants rather than an untagged enum, so an unknown or
/// missing executor produces a clear "no executor configured" error instead of an
/// opaque untagged-deserialization failure. Later slices add `rampingVus` /
/// `constantRate` here without touching the shape of a scenario.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ExecutorDto {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constant_vus: Option<ConstantVusDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constant_rate: Option<ConstantRateDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ramping_rate: Option<RampingRateDto>,
}

/// A fixed pool of virtual users run for a fixed duration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConstantVusDto {
    pub vus: u32,
    pub duration: DurationDto,
}

/// A constant arrival rate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConstantRateDto {
    pub rate: f64,
    pub duration: DurationDto,
}

/// A ramping arrival rate.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RampingRateDto {
    pub start_rate: f64,
    pub end_rate: f64,
    pub duration: DurationDto,
}

/// One step: a request id, and the pause that follows it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StepDto {
    pub request: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub think_time: Option<DurationDto>,
}

/// `load/flows/*.yaml` — a load scenario.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ScenarioDto {
    pub id: String,
    pub name: String,
    pub executor: ExecutorDto,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<StepDto>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub thresholds: BTreeMap<String, Vec<String>>,
    #[serde(default, rename = "abortOn", skip_serializing_if = "Vec::is_empty")]
    pub abort_on: Vec<String>,
    /// The run's default request deadline: `timeout: 30s`. Applies to steps whose
    /// request sets none; the CLI's `--timeout` overrides it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<DurationDto>,
}

fn default_method() -> String {
    "GET".to_string()
}
