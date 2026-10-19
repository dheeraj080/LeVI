// SPDX-License-Identifier: MIT OR Apache-2.0
//! Integration tests for the HTTP transport.
//!
//! A self-contained hyper server on an ephemeral port stands in for the target, so
//! the suite proves "a request really goes out and comes back" without a live server
//! or network access. Injected-latency accuracy testing (the correctness proof for
//! the whole tool) lands with the load engine in Phase 3.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
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

// ---- retries ----

/// A server that answers a fixed list of statuses, then 200 forever.
///
/// The list is the test's script: "the target refused twice, then answered" is one
/// argument, not a timing. The returned counter is how many requests actually
/// arrived — the number a load report would have to be honest about.
async fn start_flaky_server(statuses: Vec<u16>) -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let script = Arc::new(std::sync::Mutex::new(statuses));

    let served = count.clone();
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(connection) => connection,
                Err(_) => break,
            };
            let served = served.clone();
            let script = script.clone();
            tokio::spawn(async move {
                let service = service_fn(move |_request: Request<Incoming>| {
                    let served = served.clone();
                    let script = script.clone();
                    async move {
                        let index = served.fetch_add(1, Ordering::SeqCst);
                        let status = {
                            let script = script.lock().expect("script");
                            if index < script.len() {
                                script[index]
                            } else {
                                200
                            }
                        };
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(status)
                                .body(Full::new(Bytes::from("ok")))
                                .unwrap(),
                        )
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    });

    (address, count)
}

/// A client tuned so a retry test does not sit out a real backoff.
fn retrying(retries: u32) -> ClientConfig {
    ClientConfig::default()
        .with_retries(retries)
        .with_retry_backoff(Duration::from_millis(5))
}

fn get(url: String) -> PreparedRequest {
    request(Method::Get, url, vec![])
}

#[tokio::test]
async fn a_flaky_target_is_retried_until_it_answers() {
    let (address, count) = start_flaky_server(vec![503, 503]).await;
    let client = HttpClient::with_config(retrying(3)).unwrap();

    let response = client
        .send(get(format!("http://{address}/flaky")))
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.attempts, 3);
    assert_eq!(
        count.load(Ordering::SeqCst),
        3,
        "the target saw exactly what the response says it saw"
    );
}

#[tokio::test]
async fn retries_are_off_until_somebody_asks_for_them() {
    let (address, count) = start_flaky_server(vec![503]).await;
    let client = HttpClient::new().unwrap();

    let response = client
        .send(get(format!("http://{address}/flaky")))
        .await
        .unwrap();

    assert_eq!(response.status, 503);
    assert_eq!(response.attempts, 1);
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_five_hundred_is_reported_not_retried() {
    // 500 says the target processed the request and failed. Repeating it may fail
    // the same way — or succeed twice, which is worse.
    let (address, count) = start_flaky_server(vec![500]).await;
    let client = HttpClient::with_config(retrying(3)).unwrap();

    let response = client
        .send(get(format!("http://{address}/boom")))
        .await
        .unwrap();

    assert_eq!(response.status, 500);
    assert_eq!(response.attempts, 1);
    assert_eq!(count.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_post_is_not_retried_by_default() {
    let (address, count) = start_flaky_server(vec![503]).await;
    let client = HttpClient::with_config(retrying(3)).unwrap();

    let response = client
        .send(request(
            Method::Post,
            format!("http://{address}/orders"),
            b"{\"sku\":\"A1\"}".to_vec(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status, 503);
    assert_eq!(
        count.load(Ordering::SeqCst),
        1,
        "a retried POST can create two orders"
    );
}

#[tokio::test]
async fn a_post_is_retried_when_the_flag_says_so() {
    let (address, count) = start_flaky_server(vec![503]).await;
    let client = HttpClient::with_config(retrying(3).retrying_non_idempotent()).unwrap();

    let response = client
        .send(request(
            Method::Post,
            format!("http://{address}/orders"),
            b"{\"sku\":\"A1\"}".to_vec(),
        ))
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.attempts, 2);
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_deadline_that_expires_is_retried() {
    // The first request is answered too late to count; the second answers at once.
    // The client default is impossible to meet on purpose, so the request's own
    // deadline is provably what bounds a send and what the retry reaches past.
    let (address, count) = start_slow_then_fast_server(Duration::from_millis(400)).await;
    let client = HttpClient::with_config(ClientConfig {
        request_timeout: Some(Duration::from_nanos(1)),
        ..retrying(2)
    })
    .unwrap();

    let mut prepared = request(Method::Get, format!("http://{address}/slow"), vec![]);
    prepared.timeout = Some(Duration::from_millis(100));

    let response = client.send(prepared).await.unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.attempts, 2);
    assert_eq!(count.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn a_refused_connection_is_retried() {
    // A port that was bound and let go: connecting is refused, not answered, which is
    // a transport failure worth another attempt.
    let address = {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        listener.local_addr().unwrap()
    };
    let client = HttpClient::with_config(retrying(2)).unwrap();

    let error = client
        .send(get(format!("http://{address}/nowhere")))
        .await
        .unwrap_err();

    assert!(matches!(error, HttpError::Request(_)), "{error:?}");
}

#[tokio::test]
async fn the_wait_follows_the_servers_retry_after() {
    // `Retry-After: 1` is longer than the 5ms backoff, so a client that ignored it
    // would be back in milliseconds. A real server may ask for far more than a test
    // can sit through, which is why the config caps what it is obeyed for.
    let address = start_rate_limited_server().await;
    let client = HttpClient::with_config(retrying(2)).unwrap();

    let started = Instant::now();
    let response = client
        .send(get(format!("http://{address}/limited")))
        .await
        .unwrap();

    assert_eq!(response.status, 200);
    assert_eq!(response.attempts, 2);
    assert!(
        started.elapsed() >= Duration::from_secs(1),
        "the server was not listened to: {:?}",
        started.elapsed()
    );
}

/// A server whose first request is answered after `delay`, and every later one at once.
async fn start_slow_then_fast_server(delay: Duration) -> (SocketAddr, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));

    let counter = requests.clone();
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(connection) => connection,
                Err(_) => break,
            };
            let counter = counter.clone();
            tokio::spawn(async move {
                let service = service_fn(move |_request: Request<Incoming>| {
                    let counter = counter.clone();
                    async move {
                        if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                            tokio::time::sleep(delay).await;
                        }
                        Ok::<_, Infallible>(
                            Response::builder()
                                .status(200)
                                .body(Full::new(Bytes::from("ok")))
                                .unwrap(),
                        )
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    });

    (address, requests)
}

/// A server that answers `429 Retry-After: 1` once, then 200.
async fn start_rate_limited_server() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let requests = Arc::new(AtomicUsize::new(0));

    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(connection) => connection,
                Err(_) => break,
            };
            let requests = requests.clone();
            tokio::spawn(async move {
                let service = service_fn(move |_request: Request<Incoming>| {
                    let requests = requests.clone();
                    async move {
                        let first = requests.fetch_add(1, Ordering::SeqCst) == 0;
                        let response = if first {
                            Response::builder()
                                .status(429)
                                .header("retry-after", "1")
                                .body(Full::new(Bytes::from("slow down")))
                                .unwrap()
                        } else {
                            Response::builder()
                                .status(200)
                                .body(Full::new(Bytes::from("ok")))
                                .unwrap()
                        };
                        Ok::<_, Infallible>(response)
                    }
                });
                let _ = hyper::server::conn::http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    });

    address
}
