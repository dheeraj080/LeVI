// SPDX-License-Identifier: MIT OR Apache-2.0
//! Phase 2 exit criteria, exercised through the protocol boundary.
//!
//! A request with a pre-request script, a post-response script, and three assertions;
//! a deliberately runaway script killed by the CPU budget *without* killing the
//! session; and a script that cannot leak a secret through a log or a check.

use std::sync::Arc;
use std::time::Duration;

use lev_core::{MapSecretStore, Redactor};
use lev_engine::{in_process_client, Engine};
use lev_protocol::{AssertionDto, BodyDto, EnvDto, ErrorCode, RequestDto, ScriptsDto, VarValueDto};
use lev_script::Limits;
use wiremock::matchers::{body_string, header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn engine_with_secret(name: &str, value: &str) -> Engine {
    let mut store = MapSecretStore::new();
    store.insert(name, value);
    let redactor = Redactor::with_secrets(store.values());
    Engine::new(Arc::new(store), redactor).expect("engine")
}

fn bare_request(method: &str, url: String) -> RequestDto {
    RequestDto {
        id: "r1".to_string(),
        name: "r".to_string(),
        method: method.to_string(),
        url,
        headers: vec![],
        body: None,
        auth: None,
        scripts: None,
        assertions: vec![],
        timeout_ms: None,
    }
}

#[tokio::test]
async fn a_request_with_two_scripts_and_three_assertions_reports_checks() {
    let server = MockServer::start().await;
    // The matchers require the mutations the pre-request script makes, so a 201 proves
    // the script actually rewrote the request before it was sent.
    Mock::given(method("POST"))
        .and(path("/orders"))
        .and(header("x-trace", "abc"))
        .and(body_string("{\"sku\":\"A1\"}"))
        .respond_with(
            ResponseTemplate::new(201)
                .insert_header("content-type", "application/json")
                .set_body_string("{\"id\":7,\"ok\":true}"),
        )
        .mount(&server)
        .await;

    let engine = engine_with_secret("token", "unused");
    let (mut client, handle) = in_process_client(engine, 8).await.expect("connect");

    let mut request = bare_request("POST", format!("{}/orders", server.uri()));
    request.headers = vec![];
    request.body = Some(BodyDto::Json {
        content: "{}".to_string(),
    });
    request.scripts = Some(ScriptsDto {
        pre_request: Some(
            "lev.request.headers['X-Trace'] = 'abc'; lev.request.body = '{\"sku\":\"A1\"}';"
                .to_string(),
        ),
        post_response: Some(
            "lev.test('the id came back', function () { lev.expect(lev.response.json.id).toBe(7); });"
                .to_string(),
        ),
    });
    request.assertions = vec![
        AssertionDto::Status { code: 201 },
        AssertionDto::BodyContains {
            text: "\"ok\":true".to_string(),
        },
        AssertionDto::JsonPath {
            path: "id".to_string(),
            equals: Some("7".to_string()),
        },
    ];

    let outcome = client
        .send_request(request, EnvDto::default())
        .await
        .expect("request");

    assert_eq!(outcome.response.status, 201);
    // One check from the post-response script plus the three assertions.
    assert_eq!(outcome.checks.len(), 4, "{:?}", outcome.checks);
    assert!(
        outcome.checks.iter().all(|check| check.passed),
        "{:?}",
        outcome.checks
    );
    assert!(outcome.checks_passed());

    client.stop(true).await.expect("stop");
    drop(client);
    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn a_failing_assertion_is_a_check_not_an_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/thing"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;

    let engine = engine_with_secret("token", "unused");
    let (mut client, handle) = in_process_client(engine, 8).await.expect("connect");

    let mut request = bare_request("GET", format!("{}/thing", server.uri()));
    request.assertions = vec![
        AssertionDto::Status { code: 200 },
        AssertionDto::Status { code: 404 },
    ];

    let outcome = client
        .send_request(request, EnvDto::default())
        .await
        .expect("a failed assertion is still a successful request");

    assert_eq!(outcome.response.status, 200);
    assert_eq!(outcome.checks_failed(), 1);
    assert!(!outcome.checks_passed());

    client.stop(true).await.expect("stop");
    drop(client);
    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn a_runaway_script_is_killed_and_the_session_survives() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ok"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let engine = Engine::with_script_limits(
        Arc::new(MapSecretStore::new()),
        Redactor::new(),
        Limits::with_cpu_budget(Duration::from_millis(200)),
    )
    .expect("engine");
    let (mut client, handle) = in_process_client(engine, 8).await.expect("connect");

    // The runaway fails *this* request with a configuration error...
    let mut runaway = bare_request("GET", format!("{}/ok", server.uri()));
    runaway.scripts = Some(ScriptsDto {
        pre_request: Some("while (true) {}".to_string()),
        post_response: None,
    });
    let error = client
        .send_request(runaway, EnvDto::default())
        .await
        .expect_err("the runaway must fail the request");
    assert_eq!(error.code(), ErrorCode::Config);

    // ...and the very next request on the same session runs normally.
    let outcome = client
        .send_request(
            bare_request("GET", format!("{}/ok", server.uri())),
            EnvDto::default(),
        )
        .await
        .expect("the session must survive a runaway script");
    assert_eq!(outcome.response.status, 200);

    client.stop(true).await.expect("stop");
    drop(client);
    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn a_script_cannot_leak_a_secret_through_a_log_or_a_check() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/me"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let engine = engine_with_secret("token", "s3cr3t-token");
    let (mut client, handle) = in_process_client(engine, 8).await.expect("connect");

    // The script both logs the secret and names a failed check after it.
    let mut request = bare_request("GET", format!("{}/me", server.uri()));
    request.scripts = Some(ScriptsDto {
        pre_request: Some(
            "lev.log('the token is ' + lev.env.get('token')); \
             lev.test('token is ' + lev.env.get('token'), false);"
                .to_string(),
        ),
        post_response: None,
    });

    let mut env = EnvDto::default();
    env.vars.insert(
        "token".to_string(),
        VarValueDto::Secret {
            secret: "token".to_string(),
        },
    );

    let outcome = client.send_request(request, env).await.expect("request");

    let wire = serde_json::to_string(&(&outcome.logs, &outcome.checks)).unwrap();
    assert!(!wire.contains("s3cr3t-token"), "secret leaked: {wire}");
    assert!(wire.contains("***REDACTED***"), "{wire}");
    assert_eq!(outcome.checks_failed(), 1);

    client.stop(true).await.expect("stop");
    drop(client);
    handle.await.expect("join").expect("serve");
}
