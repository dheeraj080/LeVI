// SPDX-License-Identifier: MIT OR Apache-2.0
//! Turn a [`Collection`] + [`RequestDef`] + variable scope into a fully resolved
//! request that a transport can send. Pure: no I/O, so it is trivially testable.

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
    let mut ctx = Context::with_seed(vars, secrets, seed);

    let base = match &collection.base_url {
        Some(template) => Some(template.render(&mut ctx)?),
        None => None,
    };
    let rendered = request.url.render(&mut ctx)?;
    let mut url = join_url(base.as_deref(), &rendered)?;

    let mut headers: Vec<(String, String)> = Vec::with_capacity(request.headers.len() + 2);
    for (name, value) in &request.headers {
        headers.push((name.render(&mut ctx)?, value.render(&mut ctx)?));
    }

    let (body, default_content_type) = render_body(&request.body, &mut ctx)?;

    if let Some(auth) = request.auth.as_ref().or(collection.auth.as_ref()) {
        apply_auth(auth, secrets, &mut ctx, &mut headers, &mut url)?;
    }

    if let Some(content_type) = default_content_type {
        if !has_header(&headers, "content-type") {
            headers.push(("content-type".to_string(), content_type));
        }
    }

    Ok(PreparedRequest {
        method: request.method.clone(),
        url,
        headers,
        body,
    })
}

fn render_body(body: &Body, ctx: &mut Context<'_>) -> Result<(Vec<u8>, Option<String>)> {
    match body {
        Body::None => Ok((Vec::new(), None)),
        Body::Json(source) => {
            let rendered = Template::parse(source)?.render(ctx)?;
            Ok((rendered.into_bytes(), Some("application/json".to_string())))
        }
        Body::Raw {
            content_type,
            content,
        } => {
            let rendered = Template::parse(content)?.render(ctx)?;
            Ok((rendered.into_bytes(), content_type.clone()))
        }
        Body::Form(fields) => {
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
        Body::File(template) => {
            let reference = template.render(ctx)?;
            Err(CoreError::UnsupportedBody(format!(
                "file body `{reference}` must be resolved by a layer that can do I/O"
            )))
        }
    }
}

fn apply_auth(
    auth: &Auth,
    secrets: &dyn SecretStore,
    ctx: &mut Context<'_>,
    headers: &mut Vec<(String, String)>,
    url: &mut String,
) -> Result<()> {
    match auth {
        Auth::Bearer(token) => {
            let token = token.render(ctx)?;
            headers.push(("authorization".to_string(), format!("Bearer {token}")));
        }
        Auth::Basic { username, password } => {
            let user = username.render(ctx)?;
            let pass = resolve_value(password, secrets)?;
            let encoded =
                base64::engine::general_purpose::STANDARD.encode(format!("{user}:{pass}"));
            headers.push(("authorization".to_string(), format!("Basic {encoded}")));
        }
        Auth::ApiKey {
            name,
            value,
            location,
        } => {
            let value = resolve_value(value, secrets)?;
            match location {
                ApiKeyLocation::Header => headers.push((name.clone(), value)),
                ApiKeyLocation::Query => append_query(url, name, &value),
            }
        }
    }
    Ok(())
}

fn resolve_value(value: &VarValue, secrets: &dyn SecretStore) -> Result<String> {
    match value {
        VarValue::Plain(value) => Ok(value.clone()),
        VarValue::Secret(name) => Ok(secrets.resolve(name)?.expose().to_string()),
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
}
