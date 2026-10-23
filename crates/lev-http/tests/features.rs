// SPDX-License-Identifier: MIT OR Apache-2.0
//! End-to-end tests for the transport features that a real API client needs:
//! redirects, cookies, compression, and a bounded response.
//!
//! Each test runs against a small hyper server on an ephemeral port, so it proves the
//! behaviour over a socket rather than by calling a helper.

use std::convert::Infallible;
use std::io::Write;
use std::net::SocketAddr;

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

use lev_core::{Method, PreparedRequest};
use lev_http::{ClientConfig, HttpClient, HttpError};

async fn start_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();

    tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service_fn(handle))
                    .await;
            });
        }
    });

    address
}

async fn handle(request: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let path = request.uri().path().to_string();
    let method = request.method().to_string();
    let cookie = request
        .headers()
        .get("cookie")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = request.into_body().collect().await.unwrap().to_bytes();

    let response = match path.as_str() {
        "/redirect" => Response::builder()
            .status(StatusCode::FOUND)
            .header("location", "/final")
            .body(Full::new(Bytes::from_static(b"redirecting")))
            .unwrap(),
        "/see-other" => Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header("location", "/final")
            .body(Full::new(Bytes::new()))
            .unwrap(),
        "/final" => {
            let payload = format!(
                "{{\"path\":\"final\",\"method\":\"{}\",\"body\":\"{}\"}}",
                method,
                String::from_utf8_lossy(&body)
            );
            Response::new(Full::new(Bytes::from(payload)))
        }
        "/login" => Response::builder()
            .header("set-cookie", "session=abc; Path=/; HttpOnly")
            .body(Full::new(Bytes::from_static(b"ok")))
            .unwrap(),
        "/whoami" => Response::new(Full::new(Bytes::from(cookie))),
        "/gzip" => {
            let mut encoder =
                flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
            encoder.write_all(b"decoded body").unwrap();
            let compressed = encoder.finish().unwrap();
            Response::builder()
                .header("content-encoding", "gzip")
                .body(Full::new(Bytes::from(compressed)))
                .unwrap()
        }
        "/large" => Response::new(Full::new(Bytes::from(vec![b'x'; 8 * 1024]))),
        other => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Full::new(Bytes::from(other.to_string())))
            .unwrap(),
    };
    Ok(response)
}

fn get(url: String) -> PreparedRequest {
    PreparedRequest {
        method: Method::Get,
        url,
        headers: Vec::new(),
        body: Vec::new(),
        timeout: None,
    }
}

#[tokio::test]
async fn follows_a_redirect_and_reports_how_many() {
    let address = start_server().await;
    let client = HttpClient::new().unwrap();

    let response = client
        .send(get(format!("http://{address}/redirect")))
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.redirects, 1);
    let body = String::from_utf8_lossy(&response.body);
    assert!(body.contains("\"path\":\"final\""), "{body}");
}

#[tokio::test]
async fn a_303_turns_a_post_into_a_get_without_its_body() {
    let address = start_server().await;
    let client = HttpClient::new().unwrap();

    let response = client
        .send(PreparedRequest {
            method: Method::Post,
            url: format!("http://{address}/see-other"),
            headers: vec![("content-type".to_string(), "text/plain".to_string())],
            body: b"payload".to_vec(),
            timeout: None,
        })
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.redirects, 1);
    let body = String::from_utf8_lossy(&response.body);
    assert!(body.contains("\"method\":\"GET\""), "{body}");
    assert!(body.contains("\"body\":\"\""), "{body}");
}

#[tokio::test]
async fn a_cookie_is_kept_and_sent_on_the_next_request() {
    let address = start_server().await;
    let client = HttpClient::new().unwrap();

    client
        .send(get(format!("http://{address}/login")))
        .await
        .unwrap();
    let response = client
        .send(get(format!("http://{address}/whoami")))
        .await
        .unwrap();

    assert_eq!(&response.body[..], b"session=abc");
}

#[tokio::test]
async fn gzip_is_decoded_and_its_header_is_removed() {
    let address = start_server().await;
    let client = HttpClient::new().unwrap();

    let response = client
        .send(get(format!("http://{address}/gzip")))
        .await
        .unwrap();

    assert_eq!(&response.body[..], b"decoded body");
    assert!(
        !response
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("content-encoding")),
        "a decoded body must not still claim to be compressed"
    );
}

#[tokio::test]
async fn a_response_over_the_limit_is_refused_instead_of_buffered() {
    let address = start_server().await;
    let client =
        HttpClient::with_config(ClientConfig::default().with_max_response_bytes(1024)).unwrap();

    let error = client
        .send(get(format!("http://{address}/large")))
        .await
        .unwrap_err();
    assert!(
        matches!(error, HttpError::ResponseTooLarge { limit: 1024 }),
        "{error:?}"
    );
}

#[tokio::test]
async fn redirects_can_be_turned_off() {
    let address = start_server().await;
    let client = HttpClient::with_config(ClientConfig::default().without_redirects()).unwrap();

    let response = client
        .send(get(format!("http://{address}/redirect")))
        .await
        .unwrap();

    assert_eq!(response.status, 302);
    assert_eq!(response.redirects, 0);
}
