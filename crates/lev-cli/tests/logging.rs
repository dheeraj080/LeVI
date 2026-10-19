// SPDX-License-Identifier: MIT OR Apache-2.0
//! Where log lines are allowed to go, through the **real binary**.
//!
//! There is one rule worth an end-to-end test: logs are stderr, results are stdout,
//! and `levi serve`'s stdout is a machine channel that one stray line would corrupt.
//! A unit test cannot prove channel separation in a running process, so these spawn
//! it.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, ChildStdout, Command};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A fresh, unique directory for one test's collection.
fn scratch_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("levi-log-{label}-{nanos}"));
    std::fs::create_dir_all(dir.join("requests")).expect("create scratch dir");
    dir
}

fn write_collection(dir: &Path, base_url: &str) {
    std::fs::write(
        dir.join("collection.yaml"),
        format!("id: scratch\nname: Scratch\nbaseUrl: {base_url}\n"),
    )
    .expect("write collection");
}

fn write_request(dir: &Path, name: &str, body: &str) {
    std::fs::write(dir.join("requests").join(name), body).expect("write request");
}

/// One NDJSON message in. A log line arriving here would be caught by `recv` instead
/// of by the assertion at the end of the test.
async fn send(stdin: &mut ChildStdin, message: Value) {
    let mut line = message.to_string();
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .expect("write to child");
    stdin.flush().await.expect("flush to child");
}

/// One NDJSON message out, parsed. Panics on anything that is not a complete message.
async fn recv(stdout: &mut BufReader<ChildStdout>) -> Value {
    let mut line = String::new();
    let read = stdout.read_line(&mut line).await.expect("read from child");
    assert!(read > 0, "engine closed the stream without replying");
    serde_json::from_str(&line).unwrap_or_else(|error| {
        panic!("child emitted invalid json `{line}` — a log line?: {error}")
    })
}

#[tokio::test]
async fn a_quiet_run_says_nothing_on_stderr() {
    // The default level is `warn`: a tool that is working says nothing apart from its
    // result. Silence is only a promise until somebody checks it.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/quiet"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;

    let dir = scratch_dir("quiet");
    write_collection(&dir, &server.uri());
    write_request(
        &dir,
        "quiet.yaml",
        "id: quiet\nname: Quiet\nmethod: GET\nurl: /quiet\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_levi"))
        .args(["request", "requests/quiet.yaml"])
        .env_remove("LEVI_LOG")
        .env_remove("RUST_LOG")
        .current_dir(&dir)
        .output()
        .await
        .expect("run levi request");

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("HTTP 200"), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.is_empty(),
        "a working tool should be quiet:\n{stderr}"
    );
}

#[tokio::test]
async fn a_retry_is_said_out_loud_on_stderr_only() {
    // A retry is exactly the kind of event an operator wants told to them — and told
    // on the channel they are not parsing for results. The result stays clean; the
    // explanation arrives next to it, on stderr.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/flaky"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;

    let dir = scratch_dir("retry-logs");
    write_collection(&dir, &server.uri());
    write_request(
        &dir,
        "flaky.yaml",
        "id: flaky\nname: Flaky\nmethod: GET\nurl: /flaky\n",
    );

    let output = Command::new(env!("CARGO_BIN_EXE_levi"))
        .args(["request", "requests/flaky.yaml", "--retries", "2"])
        .env("LEVI_LOG", "warn")
        .env_remove("RUST_LOG")
        .current_dir(&dir)
        .output()
        .await
        .expect("run levi request");

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("3 attempts"), "{stdout}");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("transient failure"),
        "the retries were not reported:\n{stderr}"
    );
    assert!(stderr.contains("status 503"), "{stderr}");
    assert!(
        !stdout.contains("transient failure"),
        "the log line landed in the result:\n{stdout}"
    );
}

#[tokio::test]
async fn serve_logs_on_stderr_while_stdout_stays_ndjson() {
    // The serve channel is a machine channel: every line on it has to be a complete
    // JSON message. With logging turned up to `info`, the lifecycle lines must go
    // somewhere else — `recv` fails on any line that is not JSON, so a log line on
    // stdout would fail this test where it lands, not at the end.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hello"))
        .respond_with(ResponseTemplate::new(200).set_body_string("hi"))
        .mount(&server)
        .await;

    let mut command = Command::new(env!("CARGO_BIN_EXE_levi"));
    command
        .args(["serve", "--stdio"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .env("LEVI_LOG", "info")
        .env_remove("RUST_LOG")
        .kill_on_drop(true);
    let mut child = command.spawn().expect("spawn `levi serve --stdio`");
    let mut stdin = child.stdin.take().expect("child stdin");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));
    let mut stderr = BufReader::new(child.stderr.take().expect("child stderr"));

    send(
        &mut stdin,
        json!({
            "type": "hello",
            "protocolVersion": { "major": 0, "minor": 1 },
            "capabilities": ["interactive"]
        }),
    )
    .await;
    let ready = recv(&mut stdout).await;
    assert_eq!(ready["type"], json!("ready"), "{ready}");

    send(
        &mut stdin,
        json!({
            "type": "sendRequest",
            "request": {
                "id": "r1",
                "name": "hello",
                "method": "GET",
                "url": format!("{}/hello", server.uri())
            },
            "env": { "seed": 0 }
        }),
    )
    .await;
    let result = recv(&mut stdout).await;
    assert_eq!(result["type"], json!("requestResult"), "{result}");

    send(&mut stdin, json!({ "type": "stop", "graceful": true })).await;
    let status = child.wait().await.expect("wait for child");
    assert!(status.success(), "engine exited with {status}");

    // Everything the process said to stderr, now that it has exited.
    let mut logged = String::new();
    stderr
        .read_to_string(&mut logged)
        .await
        .expect("read child stderr");
    assert!(
        logged.contains("session ready"),
        "the lifecycle was not visible:\n{logged}"
    );
    assert!(
        logged.contains("engine session ending"),
        "how it ended was not visible:\n{logged}"
    );
}
