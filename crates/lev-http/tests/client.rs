// SPDX-License-Identifier: MIT OR Apache-2.0
//! Integration tests for the HTTP transport.
//!
//! A self-contained hyper server on an ephemeral port stands in for the target, so
//! the suite proves "a request really goes out and comes back" without a live server
//! or network access. Injected-latency accuracy testing (the correctness proof for
//! the whole tool) lands with the load engine in Phase 3.

use std::convert::Infallible;
use std::net::SocketAddr;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use lev_core::{Method, PreparedRequest};
use lev_http::HttpClient;

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
