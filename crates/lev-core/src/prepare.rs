// SPDX-License-Identifier: MIT OR Apache-2.0
//! Turn a [`Collection`] + [`RequestDef`] + variable scope into a fully resolved
//! request that a transport can send. Pure: no I/O, so it is trivially testable.
//!
//! The work is split in two so the load path stays cheap:
//!
//! 1. [`compile_request`] parses every template **once** into a [`CompiledRequest`].
//!    This is scope-independent, so it happens during `Preparing`.
//! 2. [`CompiledRequest::prepare`] renders the parsed templates against a [`Context`].
//!    This is the per-iteration work, and the only part that touches the scope.
//!
//! [`prepare_request`] is the one-shot convenience the interactive path uses: it
//! compiles and renders in a single call, so both paths share exactly one resolver.
//! Body templates (`json`, `raw`) are the reason the split exists at all: they are
//! stored as source text, and re-parsing them on every iteration at 10k RPS is real
//! overhead. A long-lived [`Context`] also means the dynamic functions
//! (`{{$uuid}}`, `{{$randomInt}}`) keep advancing instead of repeating themselves.

use std::time::Duration;

use base64::Engine as _;

use crate::context::Context;
use crate::error::{CoreError, Result};
use crate::model::{ApiKeyLocation, Auth, Body, Collection, Method, RequestDef, VarSet, VarValue};
use crate::secrets::SecretStore;
use crate::template::Template;

/// A request with every template resolved. This is what `lev-http` sends.
#[derive(Debug, Clone, PartialEq)]
pub struct PreparedRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// The effective deadline for this send: the authored `timeout:` if there is
    /// one, carried through resolution so the transport never has to re-derive it.
    pub timeout: Option<Duration>,
}

/// A request whose templates have been parsed once, ready to render many times.
///
/// Compiling depends only on the collection and the request — never on the variable
/// scope — so it is safe to compile once and render per iteration. Renderers are
/// cheap: every template walk is linear and nothing is re-parsed.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledRequest {
    method: Method,
    base_url: Option<Template>,
    url: Template,
    headers: Vec<(Template, Template)>,
    body: CompiledBody,
    auth: Option<CompiledAuth>,
    timeout: Option<Duration>,
}

/// A request body with its templates parsed. Mirrors [`Body`], with the source text
/// of `json` / `raw` / `file` replaced by a parsed [`Template`].
#[derive(Debug, Clone, PartialEq)]
enum CompiledBody {
    None,
    Json(Template),
    Form(Vec<(String, String)>),
    Raw {
        content_type: Option<String>,
        content: Template,
    },
    File(Template),
}

/// Auth with its templates parsed. The `VarValue` fields are resolved at render time
/// through the [`Context`], so the secret store is still the only source of values.
#[derive(Debug, Clone, PartialEq)]
enum CompiledAuth {
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

/// Parse every template in a request once.
///
/// Auth falls back to the collection default exactly as [`prepare_request`] does, so
/// the two paths cannot disagree about which credentials apply.
pub fn compile_request(collection: &Collection, request: &RequestDef) -> Result<CompiledRequest> {
    Ok(CompiledRequest {
        method: request.method.clone(),
        base_url: collection.base_url.clone(),
        url: request.url.clone(),
        headers: request.headers.clone(),
        body: compile_body(&request.body)?,
        auth: request
            .auth
            .as_ref()
            .or(collection.auth.as_ref())
            .map(compile_auth),
        timeout: request.timeout,
    })
}

impl CompiledRequest {
    /// Render the parsed templates against a context.
    pub fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        let base = match &self.base_url {
            Some(template) => Some(template.render(ctx)?),
            None => None,
        };
        let rendered = self.url.render(ctx)?;
        let mut url = join_url(base.as_deref(), &rendered)?;

        let mut headers: Vec<(String, String)> = Vec::with_capacity(self.headers.len() + 2);
        for (name, value) in &self.headers {
            headers.push((name.render(ctx)?, value.render(ctx)?));
        }

        let (body, default_content_type) = self.body.render(ctx)?;

        if let Some(auth) = &self.auth {
            auth.apply(ctx, &mut headers, &mut url)?;
        }

        if let Some(content_type) = default_content_type {
            if !has_header(&headers, "content-type") {
                headers.push(("content-type".to_string(), content_type));
            }
        }

        Ok(PreparedRequest {
            method: self.method.clone(),
            url,
            headers,
            body,
            timeout: self.timeout,
        })
    }

    /// The HTTP method, without rendering.
    pub fn method(&self) -> &Method {
        &self.method
    }
}

/// Resolve a request with the default RNG seed.
pub fn prepare_request(
    collection: &Collection,
    request: &RequestDef,
    vars: VarSet,
    secrets: &dyn SecretStore,
) -> Result<PreparedRequest> {
    prepare_request_with_seed(collection, request, vars, secrets, 0)
}

/// Resolve a request with an explicit RNG seed, so `{{$uuid}}` etc. are reproducible.
pub fn prepare_request_with_seed(
    collection: &Collection,
    request: &RequestDef,
    vars: VarSet,
    secrets: &dyn SecretStore,
    seed: u64,
) -> Result<PreparedRequest> {
    let compiled = compile_request(collection, request)?;
    let mut ctx = Context::with_seed(vars, secrets, seed);
    compiled.prepare(&mut ctx)
}

fn compile_body(body: &Body) -> Result<CompiledBody> {
    Ok(match body {
        Body::None => CompiledBody::None,
        Body::Json(source) => CompiledBody::Json(Template::parse(source)?),
        Body::Form(fields) => CompiledBody::Form(fields.clone()),
        Body::Raw {
            content_type,
            content,
        } => CompiledBody::Raw {
            content_type: content_type.clone(),
            content: Template::parse(content)?,
        },
        Body::File(template) => CompiledBody::File(template.clone()),
    })
}

fn compile_auth(auth: &Auth) -> CompiledAuth {
    match auth {
        Auth::Bearer(token) => CompiledAuth::Bearer(token.clone()),
        Auth::Basic { username, password } => CompiledAuth::Basic {
            username: username.clone(),
            password: password.clone(),
        },
        Auth::ApiKey {
            name,
            value,
            location,
        } => CompiledAuth::ApiKey {
            name: name.clone(),
            value: value.clone(),
            location: *location,
        },
    }
}

impl CompiledBody {
    fn render(&self, ctx: &mut Context<'_>) -> Result<(Vec<u8>, Option<String>)> {
        match self {
            CompiledBody::None => Ok((Vec::new(), None)),
            CompiledBody::Json(template) => {
                let rendered = template.render(ctx)?;
                Ok((rendered.into_bytes(), Some("application/json".to_string())))
            }
            CompiledBody::Raw {
                content_type,
                content,
            } => {
                let rendered = content.render(ctx)?;
                Ok((rendered.into_bytes(), content_type.clone()))
            }
            CompiledBody::Form(fields) => {
                let encoded = fields
                    .iter()
                    .map(|(name, value)| format!("{}={}", form_encode(name), form_encode(value)))
                    .collect::<Vec<_>>()
                    .join("&");
                Ok((
                    encoded.into_bytes(),
                    Some("application/x-www-form-urlencoded".to_string()),
                ))
            }
            CompiledBody::File(template) => {
                let reference = template.render(ctx)?;
                Err(CoreError::UnsupportedBody(format!(
                    "file body `{reference}` must be resolved by a layer that can do I/O"
                )))
            }
        }
    }
}

impl CompiledAuth {
    fn apply(
        &self,
        ctx: &mut Context<'_>,
        headers: &mut Vec<(String, String)>,
        url: &mut String,
    ) -> Result<()> {
        match self {
            CompiledAuth::Bearer(token) => {
                let token = token.render(ctx)?;
                headers.push(("authorization".to_string(), format!("Bearer {token}")));
            }
            CompiledAuth::Basic { username, password } => {
                let user = username.render(ctx)?;
                let pass = ctx.resolve_value(password)?;
                let encoded =
                    base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
                headers.push(("authorization".to_string(), format!("Basic {encoded}")));
            }
            CompiledAuth::ApiKey {
                name,
                value,
                location,
            } => {
                let value = ctx.resolve_value(value)?;
                match location {
                    ApiKeyLocation::Header => headers.push((name.clone(), value)),
                    ApiKeyLocation::Query => append_query(url, name, &value),
                }
            }
        }
        Ok(())
    }
}

fn has_header(headers: &[(String, String)], name: &str) -> bool {
    headers.iter().any(|(n, _)| n.eq_ignore_ascii_case(name))
}

fn join_url(base: Option<&str>, url: &str) -> Result<String> {
    if url.starts_with("http://") || url.starts_with("https://") {
        return Ok(url.to_string());
    }
    match base {
        Some(base) => Ok(format!(
            "{}/{}",
            base.trim_end_matches('/'),
            url.trim_start_matches('/')
        )),
        None => Err(CoreError::InvalidUrl(format!(
            "{url} is relative and the collection has no baseUrl"
        ))),
    }
}

fn append_query(url: &mut String, name: &str, value: &str) {
    let separator = if url.contains('?') { '&' } else { '?' };
    url.push(separator);
    url.push_str(&query_encode(name));
    url.push('=');
    url.push_str(&query_encode(value));
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

/// Percent-encode for use in a query string value.
fn query_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        if is_unreserved(byte) {
            out.push(byte as char);
        } else {
            out.push('%');
            out.push_str(&format!("{byte:02X}"));
        }
    }
    out
}

/// Encode for `application/x-www-form-urlencoded` (space becomes `+`).
fn form_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b' ' => out.push('+'),
            b if is_unreserved(b) => out.push(b as char),
            b => {
                out.push('%');
                out.push_str(&format!("{b:02X}"));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ApiKeyLocation;
    use crate::secrets::{MapSecretStore, SecretName};

    fn collection(base: Option<&str>) -> Collection {
        Collection {
            id: "c".into(),
            name: "c".into(),
            vars: VarSet::new(),
            auth: None,
            base_url: base.map(|value| Template::parse(value).unwrap()),
        }
    }

    fn request(url: &str) -> RequestDef {
        RequestDef {
            id: "r".into(),
            name: "r".into(),
            method: Method::Get,
            url: Template::parse(url).unwrap(),
            headers: Vec::new(),
            body: Body::None,
            auth: None,
            scripts: Default::default(),
            assertions: Vec::new(),
            timeout: None,
        }
    }

    fn vars(pairs: &[(&str, &str)]) -> VarSet {
        let mut set = VarSet::new();
        for (name, value) in pairs {
            set.insert(*name, VarValue::Plain((*value).to_string()));
        }
        set
    }

    fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
        headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    #[test]
    fn joins_a_relative_url_onto_the_base() {
        let store = MapSecretStore::new();
        let prepared = prepare_request(
            &collection(Some("https://api.test/v1/")),
            &request("/orders"),
            vars(&[]),
            &store,
        )
        .unwrap();
        assert_eq!(prepared.url, "https://api.test/v1/orders");
    }

    #[test]
    fn keeps_an_absolute_url() {
        let store = MapSecretStore::new();
        let prepared = prepare_request(
            &collection(Some("https://api.test")),
            &request("https://other.test/ping"),
            vars(&[]),
            &store,
        )
        .unwrap();
        assert_eq!(prepared.url, "https://other.test/ping");
    }

    #[test]
    fn relative_url_without_base_is_an_error() {
        let store = MapSecretStore::new();
        let result = prepare_request(&collection(None), &request("/orders"), vars(&[]), &store);
        assert!(matches!(result, Err(CoreError::InvalidUrl(_))));
    }

    #[test]
    fn applies_bearer_auth_from_a_variable() {
        let store = MapSecretStore::new();
        let mut req = request("https://api.test/me");
        req.auth = Some(Auth::Bearer(Template::parse("{{token}}").unwrap()));
        let prepared =
            prepare_request(&collection(None), &req, vars(&[("token", "abc")]), &store).unwrap();
        assert_eq!(
            header(&prepared.headers, "authorization"),
            Some("Bearer abc")
        );
    }

    #[test]
    fn applies_basic_auth_with_base64() {
        let store = MapSecretStore::new();
        let mut req = request("https://api.test/me");
        req.auth = Some(Auth::Basic {
            username: Template::parse("user").unwrap(),
            password: VarValue::Plain("pass".to_string()),
        });
        let prepared = prepare_request(&collection(None), &req, vars(&[]), &store).unwrap();
        assert_eq!(
            header(&prepared.headers, "authorization"),
            Some("Basic dXNlcjpwYXNz")
        );
    }

    #[test]
    fn applies_api_key_as_a_query_parameter() {
        let store = MapSecretStore::new();
        let mut req = request("https://api.test/me");
        req.auth = Some(Auth::ApiKey {
            name: "api_key".to_string(),
            value: VarValue::Plain("k123".to_string()),
            location: ApiKeyLocation::Query,
        });
        let prepared = prepare_request(&collection(None), &req, vars(&[]), &store).unwrap();
        assert_eq!(prepared.url, "https://api.test/me?api_key=k123");
    }

    #[test]
    fn json_body_sets_a_default_content_type() {
        let store = MapSecretStore::new();
        let mut req = request("https://api.test/orders");
        req.method = Method::Post;
        req.body = Body::Json("{\"sku\":\"{{sku}}\"}".to_string());
        let prepared =
            prepare_request(&collection(None), &req, vars(&[("sku", "A1")]), &store).unwrap();
        assert_eq!(prepared.body, b"{\"sku\":\"A1\"}");
        assert_eq!(
            header(&prepared.headers, "content-type"),
            Some("application/json")
        );
    }

    #[test]
    fn explicit_content_type_header_wins() {
        let store = MapSecretStore::new();
        let mut req = request("https://api.test/orders");
        req.method = Method::Post;
        req.body = Body::Json("{}".to_string());
        req.headers = vec![(
            Template::parse("Content-Type").unwrap(),
            Template::parse("application/vnd.api+json").unwrap(),
        )];
        let prepared = prepare_request(&collection(None), &req, vars(&[]), &store).unwrap();
        assert_eq!(
            header(&prepared.headers, "content-type"),
            Some("application/vnd.api+json")
        );
    }

    #[test]
    fn form_body_is_url_encoded() {
        let store = MapSecretStore::new();
        let mut req = request("https://api.test/login");
        req.method = Method::Post;
        req.body = Body::Form(vec![
            ("user".to_string(), "a b".to_string()),
            ("pw".to_string(), "p&q".to_string()),
        ]);
        let prepared = prepare_request(&collection(None), &req, vars(&[]), &store).unwrap();
        assert_eq!(prepared.body, b"user=a+b&pw=p%26q");
    }

    #[test]
    fn unresolved_variable_is_an_error() {
        let store = MapSecretStore::new();
        let result = prepare_request(
            &collection(None),
            &request("https://api.test/{{missing}}"),
            vars(&[]),
            &store,
        );
        assert!(matches!(result, Err(CoreError::UnresolvedVariable(_))));
    }

    #[test]
    fn missing_secret_is_an_error() {
        let store = MapSecretStore::new();
        let mut req = request("https://api.test/me");
        req.auth = Some(Auth::Basic {
            username: Template::parse("user").unwrap(),
            password: VarValue::Secret(SecretName::new("nope")),
        });
        let result = prepare_request(&collection(None), &req, vars(&[]), &store);
        assert!(matches!(result, Err(CoreError::SecretNotFound(_))));
    }

    #[test]
    fn compiling_then_rendering_matches_the_one_shot_path() {
        let store = MapSecretStore::new();
        let collection = collection(Some("https://api.test"));
        let mut req = request("/orders/{{id}}");
        req.method = Method::Post;
        req.body = Body::Json("{\"sku\":\"{{sku}}\"}".to_string());
        req.auth = Some(Auth::Bearer(Template::parse("{{token}}").unwrap()));
        let scope = vars(&[("id", "7"), ("sku", "A1"), ("token", "abc")]);

        let one_shot =
            prepare_request_with_seed(&collection, &req, scope.clone(), &store, 5).unwrap();

        let compiled = compile_request(&collection, &req).unwrap();
        let mut ctx = Context::with_seed(scope, &store, 5);
        let two_step = compiled.prepare(&mut ctx).unwrap();

        assert_eq!(one_shot, two_step);
    }

    #[test]
    fn a_long_lived_context_advances_dynamic_functions() {
        let store = MapSecretStore::new();
        let collection = collection(None);
        let req = request("https://api.test/{{$uuid}}");
        let compiled = compile_request(&collection, &req).unwrap();

        // The load path renders the same request many times against one context; the
        // seeded RNG must keep advancing, or every iteration would send the same uuid.
        let mut ctx = Context::with_seed(vars(&[]), &store, 9);
        let first = compiled.prepare(&mut ctx).unwrap();
        let second = compiled.prepare(&mut ctx).unwrap();
        assert_ne!(first.url, second.url);
    }

    #[test]
    fn compile_reports_a_body_template_error_once() {
        let mut req = request("https://api.test/orders");
        req.body = Body::Json("{\"sku\": \"{{ oops\"".to_string());
        let result = compile_request(&collection(None), &req);
        assert!(matches!(result, Err(CoreError::TemplateParse { .. })));
    }

    #[test]
    fn the_authored_timeout_survives_compilation() {
        // The deadline is scope-independent, so it belongs on the compiled request:
        // the load path must not re-derive it per iteration.
        let store = MapSecretStore::new();
        let mut req = request("https://api.test/slow");
        req.timeout = Some(Duration::from_secs(5));
        let compiled = compile_request(&collection(None), &req).unwrap();
        let mut ctx = Context::with_seed(vars(&[]), &store, 0);
        let prepared = compiled.prepare(&mut ctx).unwrap();
        assert_eq!(prepared.timeout, Some(Duration::from_secs(5)));

        let one_shot = prepare_request(&collection(None), &req, vars(&[]), &store).unwrap();
        assert_eq!(one_shot.timeout, Some(Duration::from_secs(5)));
    }
}
