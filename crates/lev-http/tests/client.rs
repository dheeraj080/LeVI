// SPDX-License-Identifier: MIT OR Apache-2.0
//! Integration tests for the HTTP transport.
//!
//! A self-contained hyper server on an ephemeral port stands in for the target, so
//! the suite proves "a request really goes out and comes back" without a live server
//! or network access. Injected-latency accuracy testing (the correctness proof for
//! the whole tool) lands with the load engine in Phase 3.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use lev_core::{Method, PreparedRequest};
use lev_http::{ClientConfig, HttpClient, HttpError};

/// Start an echo server on an ephemeral port; returns its address.
async fn start_echo_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(connection) => connection,
                Err(_) => break,
            };
            tokio::spawn(async move {
                let service = service_fn(echo);
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    });

    address
}

async fn echo(request: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let method = request.method().to_string();
    let body = request.into_body().collect().await.unwrap().to_bytes();
    let payload = format!(
        "{{\"method\":\"{method}\",\"body\":\"{}\"}}",
        String::from_utf8_lossy(&body)
    );
    Ok(Response::new(Full::new(Bytes::from(payload))))
}

fn request(method: Method, url: String, body: Vec<u8>) -> PreparedRequest {
    PreparedRequest {
        method,
        url,
        headers: vec![("accept".to_string(), "application/json".to_string())],
        body,
        timeout: None,
    }
}

#[tokio::test]
async fn gets_a_response_and_records_timing() {
    let address = start_echo_server().await;
    let client = HttpClient::new().unwrap();

    let response = client
        .send(request(
            Method::Get,
            format!("http://{address}/ping"),
            vec![],
        ))
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    assert!(String::from_utf8_lossy(&response.body).contains("\"method\":\"GET\""));
    assert!(response.timings.total >= response.timings.ttfb);
}

#[tokio::test]
async fn posts_a_body() {
    let address = start_echo_server().await;
    let client = HttpClient::new().unwrap();

    let response = client
        .send(request(
            Method::Post,
            format!("http://{address}/orders"),
            b"{\"sku\":\"A1\"}".to_vec(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    let text = String::from_utf8_lossy(&response.body);
    assert!(text.contains("\"method\":\"POST\""));
    assert!(text.contains("sku"));
}

#[tokio::test]
async fn a_bad_url_is_reported_as_an_error() {
    let client = HttpClient::new().unwrap();
    let error = client
        .send(request(Method::Get, "not a url".to_string(), vec![]))
        .await
        .unwrap_err();
    assert!(matches!(error, lev_http::HttpError::InvalidUrl(_)));
}

#[tokio::test]
async fn discarding_a_body_still_counts_it() {
    let address = start_echo_server().await;
    let client = HttpClient::new().unwrap();

    // The load path never keeps the body, but it must still read it (so the pooled
    // connection stays healthy) and report its true length.
    let prepared = request(
        Method::Post,
        format!("http://{address}/orders"),
        b"{\"sku\":\"A1\"}".to_vec(),
    );
    let expected_len = client.send(prepared.clone()).await.unwrap().body.len() as u64;

    let response = client.send_discarding(prepared).await.unwrap();
    assert_eq!(response.status, 200);
    assert_eq!(response.body_len, expected_len);
    assert!(response.body_len > 0);
    assert!(response.timings.total >= response.timings.ttfb);
}

/// Accept connections and never answer — a target that hangs.
async fn start_hung_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            // Hold the connection open without ever writing a response. The task
            // dies with the test runtime, so there is no shutdown to manage.
            tokio::spawn(async move {
                let _stream = stream;
                tokio::time::sleep(Duration::from_secs(60)).await;
            });
        }
    });

    address
}

#[tokio::test]
async fn a_hung_target_cannot_outlive_the_requests_own_deadline() {
    let address = start_hung_server().await;
    // The client is configured with *no* default, so if this returns, the bound
    // provably came from the request and not from the client config.
    let client = HttpClient::with_config(ClientConfig::without_request_timeout()).unwrap();
    let mut prepared = request(Method::Get, format!("http://{address}/slow"), vec![]);
    prepared.timeout = Some(Duration::from_millis(150));

    let started = Instant::now();
    let error = client.send(prepared).await.unwrap_err();

    assert!(
        matches!(error, HttpError::Timeout { timeout } if timeout == Duration::from_millis(150)),
        "expected a timeout, got {error:?}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the deadline did not bound the send"
    );
}

#[tokio::test]
async fn a_hung_target_cannot_outlive_the_client_default() {
    let address = start_hung_server().await;
    let client = HttpClient::with_config(ClientConfig {
        request_timeout: Some(Duration::from_millis(150)),
        ..ClientConfig::default()
    })
    .unwrap();
    let prepared = request(Method::Get, format!("http://{address}/slow"), vec![]);

    let started = Instant::now();
    let error = client.send(prepared).await.unwrap_err();

    assert!(
        matches!(error, HttpError::Timeout { .. }),
        "expected a timeout, got {error:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[tokio::test]
async fn the_load_path_is_bounded_too() {
    // The discarding path must carry the same guarantee: a stuck response body
    // holds a virtual user otherwise.
    let address = start_hung_server().await;
    let client = HttpClient::with_config(ClientConfig::without_request_timeout()).unwrap();
    let mut prepared = request(Method::Get, format!("http://{address}/slow"), vec![]);
    prepared.timeout = Some(Duration::from_millis(150));

    let error = client.send_discarding(prepared).await.unwrap_err();
    assert!(
        matches!(error, HttpError::Timeout { .. }),
        "expected a timeout, got {error:?}"
    );
}

#[tokio::test]
async fn a_requests_own_deadline_beats_the_client_default() {
    let address = start_echo_server().await;
    // The client default is impossible to meet; only the request's own deadline
    // can be in force for this send to succeed.
    let client = HttpClient::with_config(ClientConfig {
        request_timeout: Some(Duration::from_nanos(1)),
        ..ClientConfig::default()
    })
    .unwrap();
    let mut prepared = request(Method::Get, format!("http://{address}/ping"), vec![]);
    prepared.timeout = Some(Duration::from_secs(10));

    let response = client.send(prepared).await.unwrap();
    assert_eq!(response.status, 200);
}
