// SPDX-License-Identifier: MIT OR Apache-2.0
//! Conversions between the on-disk DTOs and the core domain model.

use std::collections::BTreeMap;
use std::time::Duration;

use lev_core::{
    base64_decode, base64_encode, ApiKeyLocation, Assertion, Auth, Body, Collection, Executor,
    Method, MultipartPart, MultipartValue, RequestDef, Scenario, Scripts, SecretName, Step,
    Template, Threshold, ThresholdMetric, VarSet, VarValue,
};

use crate::dto::{
    ApiKeyLocationDto, AssertionDto, AuthDto, BodyDto, BytesBodyDto, CollectionDto,
    ConstantRateDto, ConstantVusDto, DurationDto, ExecutorDto, FormFieldDto, GraphqlBodyDto,
    HeaderAssertionDto, HeaderDto, JsonPathAssertionDto, MultipartPartDto, RampingRateDto,
    RawBodyDto, RequestDto, ScenarioDto, ScriptsDto, StepDto, VarValueDto,
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
        timeout: dto.timeout.as_ref().map(duration_to_core).transpose()?,
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
        timeout: request
            .timeout
            .map(|duration| DurationDto::Text(format_duration(duration))),
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
        Some(BodyDto::Multipart { multipart }) => Body::Multipart(multipart_to_core(multipart)?),
        Some(BodyDto::Graphql { graphql }) => Body::Graphql {
            query: graphql.query,
            variables: graphql.variables,
            operation_name: graphql.operation_name,
        },
        Some(BodyDto::Raw { raw }) => Body::Raw {
            content_type: raw.content_type,
            content: raw.content,
        },
        Some(BodyDto::File { file }) => Body::File(Template::parse(&file)?),
        Some(BodyDto::Bytes { bytes }) => Body::Bytes {
            content_type: bytes.content_type,
            content: base64_decode(&bytes.base64)?,
        },
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
        Body::Multipart(parts) => Some(BodyDto::Multipart {
            multipart: parts
                .iter()
                .map(|part| {
                    let (value, file, filename, content_type, base64) = match &part.value {
                        MultipartValue::Text(value) => {
                            (Some(value.clone()), None, None, None, None)
                        }
                        MultipartValue::File {
                            path,
                            filename,
                            content_type,
                        } => (
                            None,
                            Some(path.source().to_string()),
                            filename.clone(),
                            content_type.clone(),
                            None,
                        ),
                        MultipartValue::Bytes {
                            filename,
                            content_type,
                            content,
                        } => (
                            None,
                            None,
                            Some(filename.clone()),
                            content_type.clone(),
                            Some(base64_encode(content)),
                        ),
                    };
                    MultipartPartDto {
                        name: part.name.clone(),
                        value,
                        file,
                        filename,
                        content_type,
                        base64,
                    }
                })
                .collect(),
        }),
        Body::Graphql {
            query,
            variables,
            operation_name,
        } => Some(BodyDto::Graphql {
            graphql: GraphqlBodyDto {
                query: query.clone(),
                variables: variables.clone(),
                operation_name: operation_name.clone(),
            },
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
        Body::Bytes {
            content_type,
            content,
        } => Some(BodyDto::Bytes {
            bytes: BytesBodyDto {
                content_type: content_type.clone(),
                base64: base64_encode(content),
            },
        }),
    }
}

fn multipart_to_core(parts: Vec<MultipartPartDto>) -> Result<Vec<MultipartPart>, FormatError> {
    parts
        .into_iter()
        .map(|part| {
            if part.name.trim().is_empty() {
                return Err(FormatError::InvalidBody(
                    "a multipart part needs a non-empty `name`".to_string(),
                ));
            }
            let chosen = [
                part.value.is_some(),
                part.file.is_some(),
                part.base64.is_some(),
            ]
            .into_iter()
            .filter(|present| *present)
            .count();
            if chosen != 1 {
                return Err(FormatError::InvalidBody(format!(
                    "multipart part `{}` takes exactly one of `value`, `file`, `base64`",
                    part.name
                )));
            }
            let value = if let Some(value) = part.value {
                MultipartValue::Text(value)
            } else if let Some(file) = part.file {
                MultipartValue::File {
                    path: Template::parse(&file)?,
                    filename: part.filename,
                    content_type: part.content_type,
                }
            } else {
                let base64 = part.base64.unwrap_or_default();
                MultipartValue::Bytes {
                    filename: part.filename.unwrap_or_else(|| "blob".to_string()),
                    content_type: part.content_type,
                    content: base64_decode(&base64)?,
                }
            };
            Ok(MultipartPart {
                name: part.name,
                value,
            })
        })
        .collect()
}

// ---- scenario ----

pub fn scenario_to_core(dto: ScenarioDto) -> Result<Scenario, FormatError> {
    let executor = if let Some(constant_vus) = dto.executor.constant_vus {
        if constant_vus.vus == 0 {
            return Err(FormatError::InvalidScenario(
                "executor `constantVus.vus` must be at least 1".to_string(),
            ));
        }
        Executor::ConstantVus {
            vus: constant_vus.vus,
            duration: duration_to_core(&constant_vus.duration)?,
        }
    } else if let Some(constant_rate) = dto.executor.constant_rate {
        if constant_rate.rate <= 0.0 {
            return Err(FormatError::InvalidScenario(
                "executor `constantRate.rate` must be greater than 0".to_string(),
            ));
        }
        Executor::ConstantRate {
            rate: constant_rate.rate,
            duration: duration_to_core(&constant_rate.duration)?,
        }
    } else if let Some(ramping_rate) = dto.executor.ramping_rate {
        if ramping_rate.start_rate < 0.0 || ramping_rate.end_rate < 0.0 {
            return Err(FormatError::InvalidScenario(
                "executor `rampingRate` rates must be non-negative".to_string(),
            ));
        }
        Executor::RampingRate {
            start_rate: ramping_rate.start_rate,
            end_rate: ramping_rate.end_rate,
            duration: duration_to_core(&ramping_rate.duration)?,
        }
    } else {
        return Err(FormatError::InvalidScenario(
            "no executor configured (expected `constantVus`, `constantRate`, or `rampingRate`)"
                .to_string(),
        ));
    };

    let mut steps = Vec::with_capacity(dto.steps.len());
    for step in dto.steps {
        steps.push(Step {
            request: step.request,
            think_time: step.think_time.as_ref().map(duration_to_core).transpose()?,
        });
    }
    if steps.is_empty() {
        return Err(FormatError::InvalidScenario(
            "scenario has no steps".to_string(),
        ));
    }

    let mut thresholds: Vec<Threshold> = Vec::new();
    for (metric_name, expressions) in &dto.thresholds {
        for expression in expressions {
            thresholds.push(
                Threshold::parse(metric_name, expression)
                    .map_err(|error| FormatError::InvalidScenario(error.to_string()))?,
            );
        }
    }

    let mut abort_on: Vec<ThresholdMetric> = Vec::new();
    for name in &dto.abort_on {
        let metric = ThresholdMetric::parse(name).ok_or_else(|| {
            FormatError::InvalidScenario(format!(
                "scenario: abortOn refers to unknown metric `{name}`"
            ))
        })?;
        abort_on.push(metric);
    }
    for metric in &abort_on {
        if !thresholds.iter().any(|t| t.metric == *metric) {
            return Err(FormatError::InvalidScenario(format!(
                "scenario: abortOn refers to metric `{}` but no threshold is defined for it",
                metric
            )));
        }
    }

    Ok(Scenario {
        id: dto.id,
        name: dto.name,
        executor,
        steps,
        thresholds,
        abort_on,
        timeout: dto.timeout.as_ref().map(duration_to_core).transpose()?,
    })
}

pub fn scenario_from_core(scenario: &Scenario) -> ScenarioDto {
    let executor = match &scenario.executor {
        Executor::ConstantVus { vus, duration } => ExecutorDto {
            constant_vus: Some(ConstantVusDto {
                vus: *vus,
                duration: DurationDto::Text(format_duration(*duration)),
            }),
            constant_rate: None,
            ramping_rate: None,
        },
        Executor::ConstantRate { rate, duration } => ExecutorDto {
            constant_vus: None,
            constant_rate: Some(ConstantRateDto {
                rate: *rate,
                duration: DurationDto::Text(format_duration(*duration)),
            }),
            ramping_rate: None,
        },
        Executor::RampingRate {
            start_rate,
            end_rate,
            duration,
        } => ExecutorDto {
            constant_vus: None,
            constant_rate: None,
            ramping_rate: Some(RampingRateDto {
                start_rate: *start_rate,
                end_rate: *end_rate,
                duration: DurationDto::Text(format_duration(*duration)),
            }),
        },
    };

    let mut thresholds_map: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for threshold in &scenario.thresholds {
        thresholds_map
            .entry(threshold.metric.name().to_string())
            .or_default()
            .push(threshold.expression());
    }
    let abort_on: Vec<String> = scenario
        .abort_on
        .iter()
        .map(|metric| metric.name().to_string())
        .collect();

    ScenarioDto {
        id: scenario.id.clone(),
        name: scenario.name.clone(),
        executor,
        steps: scenario
            .steps
            .iter()
            .map(|step| StepDto {
                request: step.request.clone(),
                think_time: step
                    .think_time
                    .map(|duration| DurationDto::Text(format_duration(duration))),
            })
            .collect(),
        thresholds: thresholds_map,
        abort_on,
        timeout: scenario
            .timeout
            .map(|duration| DurationDto::Text(format_duration(duration))),
    }
}

fn duration_to_core(dto: &DurationDto) -> Result<Duration, FormatError> {
    match dto {
        DurationDto::Seconds(seconds) => Ok(Duration::from_secs(*seconds)),
        DurationDto::Text(text) => parse_duration(text),
    }
}

/// Parse a duration: `250ms`, `30s`, `2m`, `1h`, or a bare number of seconds.
pub fn parse_duration(input: &str) -> Result<Duration, FormatError> {
    let text = input.trim();
    let digits_end = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(digits_end);

    let value: u64 = number
        .parse()
        .map_err(|_| FormatError::InvalidDuration(input.to_string()))?;

    let micros = match unit.trim() {
        "us" => return Ok(Duration::from_micros(value)),
        "ms" => value as u128 * 1_000,
        "s" | "" => value as u128 * 1_000_000,
        "m" => value as u128 * 60 * 1_000_000,
        "h" => value as u128 * 3_600 * 1_000_000,
        _ => return Err(FormatError::InvalidDuration(input.to_string())),
    };
    if micros > u64::MAX as u128 {
        return Err(FormatError::InvalidDuration(input.to_string()));
    }
    Ok(Duration::from_micros(micros as u64))
}

/// Render a duration in the canonical on-disk form: the largest whole unit.
pub fn format_duration(duration: Duration) -> String {
    let micros = duration.as_micros();
    if !micros.is_multiple_of(1_000) {
        return format!("{micros}us");
    }
    let millis = micros / 1_000;
    if millis == 0 {
        "0ms".to_string()
    } else if millis.is_multiple_of(3_600_000) {
        format!("{}h", millis / 3_600_000)
    } else if millis.is_multiple_of(60_000) {
        format!("{}m", millis / 60_000)
    } else if millis.is_multiple_of(1_000) {
        format!("{}s", millis / 1_000)
    } else {
        format!("{millis}ms")
    }
}
