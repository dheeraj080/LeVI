// SPDX-License-Identifier: MIT OR Apache-2.0
//! Phase 1 exit criteria, exercised against the **real binary**.
//!
//! These tests spawn `levi serve --stdio` as a child process and talk to it over
//! NDJSON on its stdin/stdout — the same path a spawned load run will use. Nothing
//! is mocked except the target server.

use std::process::Stdio;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

struct Session {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Session {
    async fn start() -> Self {
        Self::start_with_env(&[]).await
    }

    async fn start_with_env(env: &[(&str, &str)]) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_levi"));
        command
            .args(["serve", "--stdio"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .kill_on_drop(true);
        for (key, value) in env {
            command.env(key, value);
        }

        let mut child = command.spawn().expect("spawn `levi serve --stdio`");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");
        Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        }
    }

    async fn send(&mut self, message: Value) {
        let mut line = message.to_string();
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .await
            .expect("write to child");
        self.stdin.flush().await.expect("flush to child");
    }

    async fn recv(&mut self) -> Value {
        let mut line = String::new();
        let read = self
            .stdout
            .read_line(&mut line)
            .await
            .expect("read from child");
        assert!(read > 0, "engine closed the stream without replying");
        serde_json::from_str(&line)
            .unwrap_or_else(|error| panic!("child emitted invalid json `{line}`: {error}"))
    }

    async fn handshake(&mut self) {
        self.send(json!({
            "type": "hello",
            "protocolVersion": { "major": 0, "minor": 1 },
            "capabilities": ["interactive"]
        }))
        .await;
        let ready = self.recv().await;
        assert_eq!(ready["type"], json!("ready"), "{ready}");
    }
}

#[tokio::test]
async fn serve_stdio_answers_a_request_in_a_child_process() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/hello"))
        .respond_with(ResponseTemplate::new(200).set_body_string("hi"))
        .mount(&server)
        .await;

    let mut session = Session::start().await;
    session.handshake().await;

    session
        .send(json!({
            "type": "sendRequest",
            "request": {
                "id": "r1",
                "name": "hello",
                "method": "GET",
                "url": format!("{}/hello", server.uri())
            },
            "env": { "seed": 0 }
        }))
        .await;

    let result = session.recv().await;
    assert_eq!(result["type"], json!("requestResult"), "{result}");
    assert_eq!(result["response"]["status"], json!(200));
    assert_eq!(result["response"]["body"], json!("hi"));
    // The echo travels back with the result, so a UI can show what was sent.
    assert_eq!(result["request"]["method"], json!("GET"));
    assert_eq!(
        result["request"]["url"],
        json!(format!("{}/hello", server.uri()))
    );

    session
        .send(json!({ "type": "stop", "graceful": true }))
        .await;
    let status = session.child.wait().await.expect("wait for child");
    assert!(status.success(), "engine exited with {status}");
}

#[tokio::test]
async fn serve_stdio_rejects_an_incompatible_protocol_version() {
    let mut session = Session::start().await;
    session
        .send(json!({
            "type": "hello",
            "protocolVersion": { "major": 7, "minor": 0 },
            "capabilities": []
        }))
        .await;

    let reply = session.recv().await;
    assert_eq!(reply["type"], json!("error"), "{reply}");
    assert_eq!(reply["code"], json!("protocolVersionMismatch"));
    assert!(
        reply["msg"]
            .as_str()
            .unwrap_or_default()
            .contains("incompatible"),
        "{reply}"
    );

    // A rejected handshake ends the session cleanly, with a non-error exit status.
    let status = session.child.wait().await.expect("wait for child");
    assert!(status.success(), "engine exited with {status}");
}

#[tokio::test]
async fn serve_stdio_redacts_secrets_before_they_reach_the_client() {
    let server = MockServer::start().await;
    // A target that reflects the credential straight back.
    Mock::given(method("GET"))
        .and(path("/echo"))
        .respond_with(ResponseTemplate::new(200).set_body_string("token=s3cr3t-token"))
        .mount(&server)
        .await;

    let mut session = Session::start_with_env(&[("LEVI_SECRET_token", "s3cr3t-token")]).await;
    session.handshake().await;

    session
        .send(json!({
            "type": "sendRequest",
            "request": {
                "id": "r1",
                "name": "echo",
                "method": "GET",
                "url": format!("{}/echo", server.uri()),
                "auth": { "kind": "bearer", "token": "{{token}}" }
            },
            "env": {
                "seed": 0,
                "vars": { "token": { "secret": "token" } }
            }
        }))
        .await;

    let result = session.recv().await;
    assert_eq!(result["type"], json!("requestResult"), "{result}");

    // The real secret was sent (the target echoed it), but it must not appear in
    // anything the client can see.
    let wire = result.to_string();
    assert!(!wire.contains("s3cr3t-token"), "secret leaked: {wire}");
    assert!(wire.contains("***REDACTED***"), "{wire}");

    // The authorization header on the echo is masked outright.
    let authorization = result["request"]["headers"]
        .as_array()
        .expect("headers")
        .iter()
        .find(|header| header["name"].as_str() == Some("authorization"))
        .expect("authorization header on the echo");
    assert_eq!(authorization["value"], json!("***REDACTED***"));

    session
        .send(json!({ "type": "stop", "graceful": true }))
        .await;
    let status = session.child.wait().await.expect("wait for child");
    assert!(status.success());
}

#[tokio::test]
async fn a_missing_request_file_is_a_configuration_error() {
    // The exit-code contract is a CI contract; check it against the real binary.
    let output = Command::new(env!("CARGO_BIN_EXE_levi"))
        .args(["request", "definitely-not-here.yaml"])
        .output()
        .await
        .expect("run levi request");

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error:"), "{stderr}");
}

#[tokio::test]
async fn an_unknown_command_is_a_configuration_error() {
    let output = Command::new(env!("CARGO_BIN_EXE_levi"))
        .arg("teleport")
        .output()
        .await
        .expect("run levi");

    assert_eq!(output.status.code(), Some(2), "{output:?}");
}
