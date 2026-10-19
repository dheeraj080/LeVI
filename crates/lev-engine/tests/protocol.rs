// SPDX-License-Identifier: MIT OR Apache-2.0
//! End-to-end tests for the protocol boundary.
//!
//! These are the Phase 1 exit criteria, executable: a request that goes *through*
//! the protocol boundary, and a version mismatch that is rejected cleanly rather
//! than hanging or panicking.

use std::sync::Arc;
use std::time::Duration;

use lev_core::{MapSecretStore, Redactor};
use lev_engine::{in_process_client, Client, Engine, EngineError};
use lev_protocol::{
    in_process_channel, Capability, EnvDto, ErrorCode, LogLevel, Message, NdjsonTransport,
    RequestDto, Transport, VarValueDto, Version,
};
use serde_json::json;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// An engine whose only secret is `token = s3cr3t-token`, so redaction is provable.
fn engine_with_secret(name: &str, value: &str) -> Engine {
    let mut store = MapSecretStore::new();
    store.insert(name, value);
    let redactor = Redactor::with_secrets(store.values());
    Engine::new(Arc::new(store), redactor).expect("engine")
}

fn get_request(url: String) -> RequestDto {
    RequestDto {
        id: "r1".to_string(),
        name: "hello".to_string(),
        method: "GET".to_string(),
        url,
        headers: vec![],
        body: None,
        auth: None,
        scripts: None,
        assertions: vec![],
        timeout_ms: None,
    }
}

/// Mirror how a collection declares a secret: `vars: { token: { secret: token } }`
/// with auth referencing it as `{{token}}`. The client sends the *reference*; the
/// engine resolves the value from its own store.
fn bearer_env(secret_name: &str, base_url: Option<String>) -> EnvDto {
    let mut vars = std::collections::BTreeMap::new();
    vars.insert(
        secret_name.to_string(),
        VarValueDto::Secret {
            secret: secret_name.to_string(),
        },
    );
    EnvDto {
        vars,
        base_url,
        auth: Some(lev_protocol::AuthDto::Bearer {
            token: format!("{{{{{secret_name}}}}}"),
        }),
        seed: 0,
    }
}

#[tokio::test]
async fn a_request_travels_through_the_protocol_and_comes_back() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hello"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"ok\":true}"))
        .mount(&server)
        .await;

    let engine = engine_with_secret("token", "s3cr3t-token");
    let (mut client, handle) = in_process_client(engine, 8).await.expect("connect");

    let outcome = client
        .send_request(
            get_request(format!("{}/hello", server.uri())),
            bearer_env("token", None),
        )
        .await
        .expect("request");
    let echo = outcome.echo;
    let response = outcome.response;

    assert_eq!(response.status, 200);
    assert_eq!(response.body, "{\"ok\":true}");
    assert_eq!(echo.method, "GET");
    assert!(!response.timings.total_ms.is_nan());

    client.stop(true).await.expect("stop");
    drop(client);
    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn the_engine_applies_collection_auth_and_a_relative_url() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/orders"))
        .and(header("authorization", "Bearer s3cr3t-token"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let engine = engine_with_secret("token", "s3cr3t-token");
    let (mut client, handle) = in_process_client(engine, 8).await.expect("connect");

    let outcome = client
        .send_request(
            get_request("/v1/orders".to_string()),
            bearer_env("token", Some(server.uri())),
        )
        .await
        .expect("request");
    let echo = outcome.echo;
    let response = outcome.response;

    // The matcher required the real bearer header, so a 200 proves auth was applied.
    assert_eq!(response.status, 200);
    assert_eq!(echo.url, format!("{}/v1/orders", server.uri()));
    // ...and the echo the client sees never contains the secret.
    let authorization = echo
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("authorization"))
        .expect("authorization header");
    assert_eq!(authorization.value, lev_core::MASK);

    client.stop(true).await.expect("stop");
    drop(client);
    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn a_secret_echoed_by_the_target_is_redacted_before_it_crosses_the_boundary() {
    let server = MockServer::start().await;
    // A hostile/naive target that reflects the credential straight back.
    Mock::given(method("GET"))
        .and(path("/echo"))
        .respond_with(ResponseTemplate::new(200).set_body_string("token=s3cr3t-token"))
        .mount(&server)
        .await;

    let engine = engine_with_secret("token", "s3cr3t-token");
    let (mut client, handle) = in_process_client(engine, 8).await.expect("connect");

    let response = client
        .send_request(
            get_request(format!("{}/echo", server.uri())),
            bearer_env("token", None),
        )
        .await
        .expect("request")
        .response;

    assert!(!response.body.contains("s3cr3t-token"), "{}", response.body);
    assert!(response.body.contains(lev_core::MASK), "{}", response.body);

    client.stop(true).await.expect("stop");
    drop(client);
    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn an_incompatible_major_version_is_rejected_cleanly() {
    let engine = engine_with_secret("token", "x");
    let (mut client, engine_end) = in_process_channel(4);
    let handle = tokio::spawn(async move { engine.serve(engine_end).await });

    // A future client speaking protocol 1.x.
    client
        .send(Message::Hello {
            protocol_version: Version::new(1, 0),
            capabilities: vec![Capability::Interactive],
        })
        .await
        .unwrap();

    match client.recv().await.unwrap() {
        Some(Message::Error { code, msg }) => {
            assert_eq!(code, ErrorCode::ProtocolVersionMismatch);
            assert!(msg.contains("incompatible protocol"), "{msg}");
        }
        other => panic!("expected a version-mismatch error, got {other:?}"),
    }

    // The engine closed the session rather than leaving the client hanging.
    drop(client);
    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn a_minor_version_difference_negotiates_down() {
    let engine = engine_with_secret("token", "x");
    let (mut client, engine_end) = in_process_channel(4);
    let handle = tokio::spawn(async move { engine.serve(engine_end).await });

    client
        .send(Message::Hello {
            protocol_version: Version::new(0, 9),
            capabilities: vec![],
        })
        .await
        .unwrap();

    match client.recv().await.unwrap() {
        Some(Message::Ready { version, engine }) => {
            // We speak 0.1, the peer 0.9 -> the agreed version is 0.1.
            assert_eq!(version, lev_protocol::PROTOCOL_VERSION);
            assert!(!engine.is_empty());
        }
        other => panic!("expected `ready`, got {other:?}"),
    }

    client.send(Message::Stop { graceful: true }).await.unwrap();
    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn a_missing_first_message_is_reported_as_such() {
    let engine = engine_with_secret("token", "x");
    let (mut client, engine_end) = in_process_channel(4);
    let handle = tokio::spawn(async move { engine.serve(engine_end).await });

    client.send(Message::Stop { graceful: true }).await.unwrap();
    match client.recv().await.unwrap() {
        Some(Message::Error { code, msg }) => {
            assert_eq!(code, ErrorCode::BadMessage);
            assert!(msg.contains("expected `hello`"), "{msg}");
        }
        other => panic!("expected a bad-message error, got {other:?}"),
    }

    drop(client);
    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn an_unresolvable_variable_is_a_configuration_error() {
    let engine = engine_with_secret("token", "x");
    let (mut client, handle) = in_process_client(engine, 8).await.expect("connect");

    let error = client
        .send_request(
            get_request("https://api.test/{{missing}}".to_string()),
            EnvDto::default(),
        )
        .await
        .expect_err("should fail");

    assert_eq!(error.code(), ErrorCode::Config);

    client.stop(true).await.expect("stop");
    drop(client);
    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn a_deadline_crosses_the_wire_and_bounds_a_slow_target() {
    // The target answers, but far too late: the request's `timeoutMs` must cut it
    // off, and a target that failed to answer in time is an *engine* error — the
    // request definition was fine, the network was not.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(60)))
        .mount(&server)
        .await;

    let engine = engine_with_secret("token", "s3cr3t-token");
    let (mut client, handle) = in_process_client(engine, 8).await.expect("connect");

    let mut request = get_request(format!("{}/slow", server.uri()));
    request.timeout_ms = Some(150);

    let started = std::time::Instant::now();
    let error = client
        .send_request(request, bearer_env("token", None))
        .await
        .expect_err("the deadline should fire first");

    assert_eq!(error.code(), ErrorCode::Engine);
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the deadline did not bound the send"
    );

    client.stop(true).await.expect("stop");
    drop(client);
    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn a_client_and_engine_speak_the_same_protocol_over_stdio() {
    // The same session, over the transport `levi serve --stdio` uses — no process.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/ping"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;

    let (near, far) = tokio::io::duplex(64 * 1024);
    let (near_read, near_write) = tokio::io::split(near);
    let (far_read, far_write) = tokio::io::split(far);

    let engine = engine_with_secret("token", "x");
    let handle = tokio::spawn(async move {
        engine
            .serve(NdjsonTransport::new(far_read, far_write))
            .await
    });

    let mut client = Client::connect(
        NdjsonTransport::new(near_read, near_write),
        vec![Capability::Interactive],
    )
    .await
    .expect("connect");
    assert_eq!(client.version(), lev_protocol::PROTOCOL_VERSION);

    let response = client
        .send_request(
            get_request(format!("{}/ping", server.uri())),
            EnvDto::default(),
        )
        .await
        .expect("request")
        .response;
    assert_eq!(response.status, 204);

    client.stop(true).await.expect("stop");
    drop(client);
    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn wire_dtos_are_plain_json() {
    // A guard on the protocol's *shape*: this is what the UI will parse.
    let request = get_request("https://api.test/".to_string());
    let encoded = serde_json::to_value(Message::SendRequest {
        request,
        env: EnvDto {
            vars: [("id".to_string(), VarValueDto::Plain("42".to_string()))]
                .into_iter()
                .collect(),
            ..EnvDto::default()
        },
    })
    .unwrap();

    assert_eq!(encoded["type"], json!("sendRequest"));
    assert_eq!(encoded["request"]["method"], json!("GET"));
    assert_eq!(encoded["env"]["vars"]["id"], json!("42"));
}

#[tokio::test]
async fn a_message_from_the_wrong_direction_is_rejected_without_killing_the_session() {
    let engine = engine_with_secret("token", "x");
    let (mut client, engine_end) = in_process_channel(4);
    let handle = tokio::spawn(async move { engine.serve(engine_end).await });

    // Complete the handshake first.
    client
        .send(Message::Hello {
            protocol_version: lev_protocol::PROTOCOL_VERSION,
            capabilities: vec![],
        })
        .await
        .unwrap();
    assert!(matches!(
        client.recv().await.unwrap(),
        Some(Message::Ready { .. })
    ));

    // `log` is an engine -> client message. A client sending one is a mistake, and
    // must produce an error rather than a desynchronised stream.
    client
        .send(Message::Log {
            level: lev_protocol::LogLevel::Info,
            msg: "wrong way".to_string(),
        })
        .await
        .unwrap();

    match client.recv().await.unwrap() {
        Some(Message::Error { code, msg }) => {
            assert_eq!(code, ErrorCode::Unsupported);
            assert!(msg.contains("not supported"), "{msg}");
        }
        other => panic!("expected an unsupported error, got {other:?}"),
    }

    // The session survives the mistake.
    client.send(Message::Stop { graceful: true }).await.unwrap();
    drop(client);
    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn a_closed_transport_is_an_engine_error() {
    // Drop the engine's end before the handshake: the client must report a clean
    // transport error rather than panicking on a dead channel.
    let (client_end, engine_end) = in_process_channel(4);
    drop(engine_end);

    let result = Client::connect(client_end, vec![Capability::Interactive]).await;
    assert!(matches!(result, Err(EngineError::Transport(_))));
}

#[tokio::test]
async fn a_shutdown_signal_ends_the_session_cleanly_and_says_why() {
    let (client_end, engine_end) = in_process_channel(8);
    let (shutdown, wait) = tokio::sync::oneshot::channel::<()>();

    let engine = engine_with_secret("token", "x");
    let handle = tokio::spawn(async move {
        engine
            .serve_with_shutdown(engine_end, async move {
                let _ = wait.await;
            })
            .await
    });

    let mut client = Client::connect(client_end, vec![Capability::Interactive])
        .await
        .expect("connect");
    shutdown.send(()).expect("signal");

    // The engine explains why the stream is ending instead of just closing it.
    match client.next().await.expect("next") {
        Some(Message::Log { level, msg }) => {
            assert_eq!(level, LogLevel::Info);
            assert!(msg.contains("signal"), "{msg}");
        }
        other => panic!("expected a shutdown log, got {other:?}"),
    }

    handle.await.expect("join").expect("serve");
}

#[tokio::test]
async fn a_shutdown_waits_for_the_request_already_in_flight() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/slow"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("{}")
                .set_delay(Duration::from_millis(500)),
        )
        .mount(&server)
        .await;

    let (client_end, engine_end) = in_process_channel(8);
    let (shutdown, wait) = tokio::sync::oneshot::channel::<()>();

    let engine = engine_with_secret("token", "s3cr3t-token");
    let handle = tokio::spawn(async move {
        engine
            .serve_with_shutdown(engine_end, async move {
                let _ = wait.await;
            })
            .await
    });

    let mut client = Client::connect(client_end, vec![Capability::Interactive])
        .await
        .expect("connect");

    // Stop the engine while the target is still thinking: graceful means the
    // request in flight is answered, not truncated.
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = shutdown.send(());
    });

    let outcome = client
        .send_request(
            get_request(format!("{}/slow", server.uri())),
            bearer_env("token", None),
        )
        .await
        .expect("the in-flight request must still be answered");

    assert_eq!(outcome.response.status, 200);

    handle.await.expect("join").expect("serve");
}
