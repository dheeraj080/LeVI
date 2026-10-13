// SPDX-License-Identifier: MIT OR Apache-2.0
//! Conversions between the on-disk DTOs and the core domain model.

use std::collections::BTreeMap;
use std::time::Duration;

use lev_core::{
    ApiKeyLocation, Assertion, Auth, Body, Collection, Method, RequestDef, Scripts, SecretName,
    Template, VarSet, VarValue,
};

use crate::dto::{
    ApiKeyLocationDto, AssertionDto, AuthDto, BodyDto, CollectionDto, FormFieldDto,
    HeaderAssertionDto, HeaderDto, JsonPathAssertionDto, RawBodyDto, RequestDto, ScriptsDto,
    VarValueDto,
};
use crate::error::FormatError;

pub fn collection_to_core(dto: CollectionDto) -> Result<Collection, FormatError> {
    let base_url = match dto.base_url {
        Some(source) => Some(Template::parse(&source)?),
        None => None,
    };
    Ok(Collection {
        id: dto.id,
        name: dto.name,
        vars: vars_to_core(dto.vars)?,
        auth: dto.auth.map(auth_to_core).transpose()?,
        base_url,
    })
}

pub fn collection_from_core(collection: &Collection) -> CollectionDto {
    CollectionDto {
        id: collection.id.clone(),
        name: collection.name.clone(),
        vars: vars_from_core(&collection.vars),
        auth: collection.auth.as_ref().map(auth_from_core),
        base_url: collection
            .base_url
            .as_ref()
            .map(|template| template.source().to_string()),
    }
}

pub fn request_to_core(dto: RequestDto) -> Result<RequestDef, FormatError> {
    let mut headers = Vec::with_capacity(dto.headers.len());
    for header in dto.headers {
        headers.push((
            Template::parse(&header.name)?,
            Template::parse(&header.value)?,
        ));
    }
    Ok(RequestDef {
        id: dto.id,
        name: dto.name,
        method: Method::parse(&dto.method),
        url: Template::parse(&dto.url)?,
        headers,
        body: body_to_core(dto.body)?,
        auth: dto.auth.map(auth_to_core).transpose()?,
        scripts: scripts_to_core(dto.scripts),
        assertions: dto.assertions.into_iter().map(assertion_to_core).collect(),
    })
}

pub fn request_from_core(request: &RequestDef) -> RequestDto {
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
        body: body_from_core(&request.body),
        auth: request.auth.as_ref().map(auth_from_core),
        scripts: scripts_from_core(&request.scripts),
        assertions: request.assertions.iter().map(assertion_from_core).collect(),
    }
}

fn scripts_to_core(dto: ScriptsDto) -> Scripts {
    Scripts {
        pre_request: dto.pre_request,
        post_response: dto.post_response,
    }
}

fn scripts_from_core(scripts: &Scripts) -> ScriptsDto {
    ScriptsDto {
        pre_request: scripts.pre_request.clone(),
        post_response: scripts.post_response.clone(),
    }
}

fn assertion_to_core(dto: AssertionDto) -> Assertion {
    match dto {
        AssertionDto::Status { status } => Assertion::Status(status),
        AssertionDto::Header { header } => Assertion::Header {
            name: header.name,
            contains: header.contains,
        },
        AssertionDto::BodyContains { body_contains } => Assertion::BodyContains(body_contains),
        AssertionDto::JsonPath { json_path } => Assertion::JsonPath {
            path: json_path.path,
            equals: json_path.equals,
        },
        AssertionDto::ResponseTime {
            response_time_under_ms,
        } => Assertion::ResponseTimeUnder(Duration::from_millis(response_time_under_ms)),
    }
}

fn assertion_from_core(assertion: &Assertion) -> AssertionDto {
    match assertion {
        Assertion::Status(status) => AssertionDto::Status { status: *status },
        Assertion::Header { name, contains } => AssertionDto::Header {
            header: HeaderAssertionDto {
                name: name.clone(),
                contains: contains.clone(),
            },
        },
        Assertion::BodyContains(text) => AssertionDto::BodyContains {
            body_contains: text.clone(),
        },
        Assertion::JsonPath { path, equals } => AssertionDto::JsonPath {
            json_path: JsonPathAssertionDto {
                path: path.clone(),
                equals: equals.clone(),
            },
        },
        Assertion::ResponseTimeUnder(max) => AssertionDto::ResponseTime {
            response_time_under_ms: max.as_millis() as u64,
        },
    }
}

pub fn vars_to_core(vars: BTreeMap<String, VarValueDto>) -> Result<VarSet, FormatError> {
    let mut set = VarSet::new();
    for (name, value) in vars {
        set.insert(name, var_value_to_core(value)?);
    }
    Ok(set)
}

pub fn vars_from_core(vars: &VarSet) -> BTreeMap<String, VarValueDto> {
    vars.entries
        .iter()
        .map(|(name, value)| (name.clone(), var_value_from_core(value)))
        .collect()
}

pub fn var_value_to_core(dto: VarValueDto) -> Result<VarValue, FormatError> {
    Ok(match dto {
        VarValueDto::Plain(value) => VarValue::Plain(value),
        VarValueDto::Secret { secret } => VarValue::Secret(SecretName::new(secret)),
    })
}

pub fn var_value_from_core(value: &VarValue) -> VarValueDto {
    match value {
        VarValue::Plain(value) => VarValueDto::Plain(value.clone()),
        VarValue::Secret(name) => VarValueDto::Secret {
            secret: name.as_str().to_string(),
        },
    }
}

fn auth_to_core(dto: AuthDto) -> Result<Auth, FormatError> {
    Ok(match dto {
        AuthDto::Bearer { token } => Auth::Bearer(Template::parse(&token)?),
        AuthDto::Basic { username, password } => Auth::Basic {
            username: Template::parse(&username)?,
            password: var_value_to_core(password)?,
        },
        AuthDto::ApiKey {
            name,
            value,
            location,
        } => Auth::ApiKey {
            name,
            value: var_value_to_core(value)?,
            location: match location {
                ApiKeyLocationDto::Header => ApiKeyLocation::Header,
                ApiKeyLocationDto::Query => ApiKeyLocation::Query,
            },
        },
    })
}

fn auth_from_core(auth: &Auth) -> AuthDto {
    match auth {
        Auth::Bearer(token) => AuthDto::Bearer {
            token: token.source().to_string(),
        },
        Auth::Basic { username, password } => AuthDto::Basic {
            username: username.source().to_string(),
            password: var_value_from_core(password),
        },
        Auth::ApiKey {
            name,
            value,
            location,
        } => AuthDto::ApiKey {
            name: name.clone(),
            value: var_value_from_core(value),
            location: match location {
                ApiKeyLocation::Header => ApiKeyLocationDto::Header,
                ApiKeyLocation::Query => ApiKeyLocationDto::Query,
            },
        },
    }
}

fn body_to_core(dto: Option<BodyDto>) -> Result<Body, FormatError> {
    Ok(match dto {
        None => Body::None,
        Some(BodyDto::Json { json }) => Body::Json(json),
        Some(BodyDto::Form { form }) => Body::Form(
            form.into_iter()
                .map(|field| (field.name, field.value))
                .collect(),
        ),
        Some(BodyDto::Raw { raw }) => Body::Raw {
            content_type: raw.content_type,
            content: raw.content,
        },
        Some(BodyDto::File { file }) => Body::File(Template::parse(&file)?),
    })
}

fn body_from_core(body: &Body) -> Option<BodyDto> {
    match body {
        Body::None => None,
        Body::Json(source) => Some(BodyDto::Json {
            json: source.clone(),
        }),
        Body::Form(fields) => Some(BodyDto::Form {
            form: fields
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
            raw: RawBodyDto {
                content_type: content_type.clone(),
                content: content.clone(),
            },
        }),
        Body::File(template) => Some(BodyDto::File {
            file: template.source().to_string(),
        }),
    }
}
