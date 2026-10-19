// SPDX-License-Identifier: MIT OR Apache-2.0
//! Conversions between the wire DTOs and the domain model.
//!
//! This module is the *only* place where the protocol and the domain meet. It is
//! deliberately mechanical: if it grows opinions, something belongs in `lev-core`.

use std::collections::BTreeMap;
use std::time::Duration;

use lev_core::{
    base64_decode, base64_encode, ApiKeyLocation, Assertion, Auth, Body, Check, Collection, Method,
    PreparedRequest, Redactor, RequestDef, Scripts, SecretName, Template, VarSet, VarValue,
};
use lev_http::{Response, Timings};
use lev_protocol::{
    ApiKeyLocationDto, AssertionDto, AuthDto, BodyDto, CheckDto, EchoDto, EnvDto, FormFieldDto,
    HeaderDto, RequestDto, ResponseDto, ScriptsDto, TimingsDto, VarValueDto,
};

use crate::error::EngineError;

// ---- request ----

/// Rebuild a request definition from the wire.
pub fn request_from_dto(dto: RequestDto) -> Result<RequestDef, EngineError> {
    let mut headers = Vec::with_capacity(dto.headers.len());
    for header in dto.headers {
        headers.push((
            parse_template(&header.name)?,
            parse_template(&header.value)?,
        ));
    }
    Ok(RequestDef {
        id: dto.id,
        name: dto.name,
        method: Method::parse(&dto.method),
        url: parse_template(&dto.url)?,
        headers,
        body: body_from_dto(dto.body)?,
        auth: dto.auth.map(auth_from_dto).transpose()?,
        scripts: scripts_from_dto(dto.scripts),
        assertions: dto.assertions.into_iter().map(assertion_from_dto).collect(),
        timeout: dto.timeout_ms.map(Duration::from_millis),
    })
}

/// Project a request definition onto the wire, templates intact.
pub fn request_to_dto(request: &RequestDef) -> RequestDto {
    RequestDto {
        id: request.id.clone(),
        name: request.name.clone(),
        method: request.method.as_str().to_string(),
        url: request.url.source().to_string(),
        headers: request
            .headers
            .iter()
            .map(|(name, value)| HeaderDto {
                name: name.source().to_string(),
                value: value.source().to_string(),
            })
            .collect(),
        body: body_to_dto(&request.body),
        auth: request.auth.as_ref().map(auth_to_dto),
        scripts: scripts_to_dto(&request.scripts),
        assertions: request.assertions.iter().map(assertion_to_dto).collect(),
        timeout_ms: request.timeout.map(|duration| duration.as_millis() as u64),
    }
}

fn scripts_from_dto(dto: Option<ScriptsDto>) -> Scripts {
    match dto {
        Some(dto) => Scripts {
            pre_request: dto.pre_request,
            post_response: dto.post_response,
        },
        None => Scripts::default(),
    }
}

fn scripts_to_dto(scripts: &Scripts) -> Option<ScriptsDto> {
    if scripts.is_empty() {
        return None;
    }
    Some(ScriptsDto {
        pre_request: scripts.pre_request.clone(),
        post_response: scripts.post_response.clone(),
    })
}

fn assertion_from_dto(dto: AssertionDto) -> Assertion {
    match dto {
        AssertionDto::Status { code } => Assertion::Status(code),
        AssertionDto::Header { name, contains } => Assertion::Header { name, contains },
        AssertionDto::BodyContains { text } => Assertion::BodyContains(text),
        AssertionDto::JsonPath { path, equals } => Assertion::JsonPath { path, equals },
        AssertionDto::ResponseTimeUnder { max_ms } => {
            Assertion::ResponseTimeUnder(Duration::from_millis(max_ms))
        }
    }
}

fn assertion_to_dto(assertion: &Assertion) -> AssertionDto {
    match assertion {
        Assertion::Status(code) => AssertionDto::Status { code: *code },
        Assertion::Header { name, contains } => AssertionDto::Header {
            name: name.clone(),
            contains: contains.clone(),
        },
        Assertion::BodyContains(text) => AssertionDto::BodyContains { text: text.clone() },
        Assertion::JsonPath { path, equals } => AssertionDto::JsonPath {
            path: path.clone(),
            equals: equals.clone(),
        },
        Assertion::ResponseTimeUnder(max) => AssertionDto::ResponseTimeUnder {
            max_ms: max.as_millis() as u64,
        },
    }
}

/// Project a check onto the wire. The caller must have redacted it already.
pub fn check_to_dto(check: &Check) -> CheckDto {
    CheckDto {
        name: check.name.clone(),
        passed: check.passed,
        message: check.message.clone(),
    }
}

// ---- environment ----

/// Rebuild the resolution scope from the wire.
///
/// `env.vars` is already the merged `collection < environment` scope, so the
/// returned [`Collection`] carries only the defaults the engine still applies:
/// `baseUrl` and the default `auth`.
pub fn env_from_dto(env: &EnvDto) -> Result<(Collection, VarSet), EngineError> {
    let mut vars = VarSet::new();
    for (name, value) in &env.vars {
        vars.insert(name.clone(), var_value_from_dto(value));
    }

    let collection = Collection {
        id: String::new(),
        name: String::new(),
        vars: VarSet::new(),
        auth: env.auth.clone().map(auth_from_dto).transpose()?,
        base_url: match &env.base_url {
            Some(source) => Some(parse_template(source)?),
            None => None,
        },
    };

    Ok((collection, vars))
}

/// Project a merged scope onto the wire.
///
/// `vars` must already be the merged `collection < environment` overlay; this
/// function does not apply precedence.
pub fn env_to_dto(collection: &Collection, vars: &VarSet, seed: u64) -> EnvDto {
    EnvDto {
        vars: vars
            .entries
            .iter()
            .map(|(name, value)| (name.clone(), var_value_to_dto(value)))
            .collect::<BTreeMap<_, _>>(),
        base_url: collection
            .base_url
            .as_ref()
            .map(|template| template.source().to_string()),
        auth: collection.auth.as_ref().map(auth_to_dto),
        seed,
    }
}

// ---- results ----

/// A redacted view of the request as actually sent.
pub fn echo_to_dto(prepared: &PreparedRequest, redactor: &Redactor) -> EchoDto {
    EchoDto {
        method: prepared.method.as_str().to_string(),
        url: redactor.redact(&prepared.url),
        headers: prepared
            .headers
            .iter()
            .map(|(name, value)| HeaderDto {
                name: name.clone(),
                value: redactor.redact_header_value(name, value),
            })
            .collect(),
        body_len: prepared.body.len(),
    }
}

/// A redacted view of the response.
///
/// The body is scrubbed of known secret values here, before it can cross the
/// boundary, because a target may echo a token straight back.
pub fn response_to_dto(response: &Response, redactor: &Redactor) -> ResponseDto {
    let text = String::from_utf8_lossy(&response.body);
    let body = redactor.redact(&text);
    ResponseDto {
        status: response.status,
        version: response.version.clone(),
        headers: response
            .headers
            .iter()
            .map(|(name, value)| HeaderDto {
                name: name.clone(),
                value: redactor.redact_header_value(name, value),
            })
            .collect(),
        body_len: body.len(),
        body,
        timings: timings_to_dto(&response.timings),
    }
}

fn timings_to_dto(timings: &Timings) -> TimingsDto {
    TimingsDto {
        total_ms: millis(timings.total),
        ttfb_ms: millis(timings.ttfb),
        receive_ms: millis(timings.receive),
        dns_ms: timings.dns.map(millis),
        connect_ms: timings.connect.map(millis),
        tls_ms: timings.tls.map(millis),
    }
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

// ---- leaf conversions ----

fn parse_template(source: &str) -> Result<Template, EngineError> {
    Ok(Template::parse(source)?)
}

fn var_value_from_dto(dto: &VarValueDto) -> VarValue {
    match dto {
        VarValueDto::Plain(value) => VarValue::Plain(value.clone()),
        VarValueDto::Secret { secret } => VarValue::Secret(SecretName::new(secret.clone())),
    }
}

fn var_value_to_dto(value: &VarValue) -> VarValueDto {
    match value {
        VarValue::Plain(value) => VarValueDto::Plain(value.clone()),
        VarValue::Secret(name) => VarValueDto::Secret {
            secret: name.as_str().to_string(),
        },
    }
}

fn auth_from_dto(dto: AuthDto) -> Result<Auth, EngineError> {
    Ok(match dto {
        AuthDto::Bearer { token } => Auth::Bearer(parse_template(&token)?),
        AuthDto::Basic { username, password } => Auth::Basic {
            username: parse_template(&username)?,
            password: var_value_from_dto(&password),
        },
        AuthDto::ApiKey {
            name,
            value,
            location,
        } => Auth::ApiKey {
            name,
            value: var_value_from_dto(&value),
            location: match location {
                ApiKeyLocationDto::Header => ApiKeyLocation::Header,
                ApiKeyLocationDto::Query => ApiKeyLocation::Query,
            },
        },
    })
}

fn auth_to_dto(auth: &Auth) -> AuthDto {
    match auth {
        Auth::Bearer(token) => AuthDto::Bearer {
            token: token.source().to_string(),
        },
        Auth::Basic { username, password } => AuthDto::Basic {
            username: username.source().to_string(),
            password: var_value_to_dto(password),
        },
        Auth::ApiKey {
            name,
            value,
            location,
        } => AuthDto::ApiKey {
            name: name.clone(),
            value: var_value_to_dto(value),
            location: match location {
                ApiKeyLocation::Header => ApiKeyLocationDto::Header,
                ApiKeyLocation::Query => ApiKeyLocationDto::Query,
            },
        },
    }
}

fn body_from_dto(dto: Option<BodyDto>) -> Result<Body, EngineError> {
    Ok(match dto {
        None => Body::None,
        Some(BodyDto::Json { content }) => Body::Json(content),
        Some(BodyDto::Form { fields }) => Body::Form(
            fields
                .into_iter()
                .map(|field| (field.name, field.value))
                .collect(),
        ),
        Some(BodyDto::Raw {
            content_type,
            content,
        }) => Body::Raw {
            content_type,
            content,
        },
        Some(BodyDto::File { path }) => Body::File(parse_template(&path)?),
        Some(BodyDto::Bytes {
            content_type,
            base64,
        }) => Body::Bytes {
            content_type,
            content: base64_decode(&base64)?,
        },
    })
}

fn body_to_dto(body: &Body) -> Option<BodyDto> {
    match body {
        Body::None => None,
        Body::Json(content) => Some(BodyDto::Json {
            content: content.clone(),
        }),
        Body::Form(fields) => Some(BodyDto::Form {
            fields: fields
                .iter()
                .map(|(name, value)| FormFieldDto {
                    name: name.clone(),
                    value: value.clone(),
                })
                .collect(),
        }),
        Body::Raw {
            content_type,
            content,
        } => Some(BodyDto::Raw {
            content_type: content_type.clone(),
            content: content.clone(),
        }),
        Body::File(template) => Some(BodyDto::File {
            path: template.source().to_string(),
        }),
        Body::Bytes {
            content_type,
            content,
        } => Some(BodyDto::Bytes {
            content_type: content_type.clone(),
            base64: base64_encode(content),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lev_core::Method;

    fn sample_request() -> RequestDef {
        RequestDef {
            id: "r1".to_string(),
            name: "Create".to_string(),
            method: Method::Post,
            url: Template::parse("/orders/{{id}}").unwrap(),
            headers: vec![(
                Template::parse("X-Trace").unwrap(),
                Template::parse("{{$uuid}}").unwrap(),
            )],
            body: Body::Json("{\"sku\":\"{{sku}}\"}".to_string()),
            auth: Some(Auth::Bearer(Template::parse("{{token}}").unwrap())),
            scripts: Default::default(),
            assertions: Vec::new(),
            timeout: Some(Duration::from_secs(3)),
        }
    }

    #[test]
    fn a_request_survives_a_round_trip_through_the_wire() {
        let original = sample_request();
        let dto = request_to_dto(&original);
        let rebuilt = request_from_dto(dto).unwrap();
        assert_eq!(original, rebuilt);
    }

    #[test]
    fn secret_references_stay_names_on_the_wire() {
        let mut request = sample_request();
        request.auth = Some(Auth::Basic {
            username: Template::parse("user").unwrap(),
            password: VarValue::Secret(SecretName::new("payments_token")),
        });
        let json = serde_json::to_string(&request_to_dto(&request)).unwrap();
        assert!(json.contains("\"secret\":\"payments_token\""), "{json}");
        // The name crosses the wire; a value never does.
        assert!(!json.contains("Bearer"));
    }

    #[test]
    fn an_environment_round_trips_its_scope() {
        let collection = Collection {
            id: "c".to_string(),
            name: "C".to_string(),
            vars: VarSet::new(),
            auth: Some(Auth::Bearer(Template::parse("{{token}}").unwrap())),
            base_url: Some(Template::parse("https://{{host}}").unwrap()),
        };
        let mut vars = VarSet::new();
        vars.insert("host", VarValue::Plain("api.test".to_string()));
        vars.insert("token", VarValue::Secret(SecretName::new("tok")));

        let dto = env_to_dto(&collection, &vars, 42);
        assert_eq!(dto.seed, 42);
        assert_eq!(dto.base_url.as_deref(), Some("https://{{host}}"));

        let (rebuilt, rebuilt_vars) = env_from_dto(&dto).unwrap();
        assert_eq!(rebuilt.base_url.unwrap().source(), "https://{{host}}");
        assert_eq!(rebuilt_vars.entries, vars.entries);
    }

    #[test]
    fn a_broken_template_is_a_configuration_error() {
        let dto = RequestDto {
            id: "r".to_string(),
            name: "r".to_string(),
            method: "GET".to_string(),
            url: "{{ unclosed".to_string(),
            headers: vec![],
            body: None,
            auth: None,
            scripts: None,
            assertions: vec![],
            timeout_ms: None,
        };
        let error = request_from_dto(dto).unwrap_err();
        assert_eq!(error.code(), lev_protocol::ErrorCode::Config);
    }

    #[test]
    fn scripts_and_assertions_survive_the_wire() {
        let mut original = sample_request();
        original.scripts = Scripts {
            pre_request: Some("lev.log('pre');".to_string()),
            post_response: Some("lev.test('ok', true);".to_string()),
        };
        original.assertions = vec![
            Assertion::Status(200),
            Assertion::Header {
                name: "content-type".to_string(),
                contains: Some("json".to_string()),
            },
            Assertion::BodyContains("ok".to_string()),
            Assertion::JsonPath {
                path: "id".to_string(),
                equals: Some("7".to_string()),
            },
            Assertion::ResponseTimeUnder(Duration::from_millis(500)),
        ];

        let rebuilt = request_from_dto(request_to_dto(&original)).unwrap();
        assert_eq!(original, rebuilt);
    }

    #[test]
    fn duration_converts_to_milliseconds() {
        assert_eq!(millis(Duration::from_millis(1500)), 1500.0);
        assert_eq!(millis(Duration::from_micros(250)), 0.25);
    }
}
