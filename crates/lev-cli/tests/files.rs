// SPDX-License-Identifier: MIT OR Apache-2.0
//! `file:` bodies through the **real binary**: a fixture is read once, relative to the
//! collection, and sent byte-for-byte.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::process::Command;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A fresh, unique collection directory with `requests/` and `fixtures/`.
fn scratch_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("levi-cli-{label}-{nanos}"));
    std::fs::create_dir_all(dir.join("requests")).expect("create requests dir");
    std::fs::create_dir_all(dir.join("fixtures")).expect("create fixtures dir");
    dir
}

fn write_collection(dir: &Path, base_url: &str) {
    std::fs::write(
        dir.join("collection.yaml"),
        format!("id: scratch\nname: Scratch\nbaseUrl: {base_url}\n"),
    )
    .expect("write collection");
}

#[tokio::test]
async fn a_file_body_is_uploaded_byte_for_byte() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/upload"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&server)
        .await;

    let dir = scratch_dir("file-body");
    write_collection(&dir, &server.uri());
    // Deliberately not valid UTF-8, so a lossy trip through a string would show up.
    let png = vec![0x89u8, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0xff, 0x00];
    std::fs::write(dir.join("fixtures/logo.png"), &png).expect("write fixture");
    std::fs::write(
        dir.join("requests/upload.yaml"),
        "id: upload\nname: Upload\nmethod: POST\nurl: /upload\nbody:\n  file: fixtures/logo.png\n",
    )
    .expect("write request");

    let output = Command::new(env!("CARGO_BIN_EXE_levi"))
        .args(["request", "requests/upload.yaml"])
        .current_dir(&dir)
        .output()
        .await
        .expect("run levi request");

    assert_eq!(
        output.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let received = server.received_requests().await.expect("requests recorded");
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].body, png, "the body was not the file's bytes");
    assert_eq!(
        received[0]
            .headers
            .get("content-type")
            .map(|value| value.to_str().unwrap()),
        Some("image/png"),
        "the extension did not become a content type"
    );
}

#[tokio::test]
async fn a_file_that_escapes_the_collection_exits_two() {
    let dir = scratch_dir("file-escape");
    write_collection(&dir, "http://127.0.0.1:1");
    let secret = dir.parent().unwrap().join("levi-probe-secret.txt");
    std::fs::write(&secret, b"secret").expect("write neighbour");
    std::fs::write(
        dir.join("requests/escape.yaml"),
        "id: escape\nname: Escape\nmethod: POST\nurl: /upload\nbody:\n  file: ../levi-probe-secret.txt\n",
    )
    .expect("write request");

    let output = Command::new(env!("CARGO_BIN_EXE_levi"))
        .args(["request", "requests/escape.yaml"])
        .current_dir(&dir)
        .output()
        .await
        .expect("run levi request");

    // A request file must not be able to read outside its collection: exit 2, and
    // nothing leaves the machine.
    assert_eq!(output.status.code(), Some(2), "stdout: {output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("escapes"), "{stderr}");

    let _ = std::fs::remove_file(&secret);
}

#[tokio::test]
async fn a_missing_fixture_is_caught_by_validate() {
    let dir = scratch_dir("file-missing");
    write_collection(&dir, "http://127.0.0.1:1");
    std::fs::write(
        dir.join("requests/upload.yaml"),
        "id: upload\nname: Upload\nmethod: POST\nurl: /upload\nbody:\n  file: fixtures/absent.png\n",
    )
    .expect("write request");

    let output = Command::new(env!("CARGO_BIN_EXE_levi"))
        .args(["validate", "."])
        .current_dir(&dir)
        .output()
        .await
        .expect("run levi validate");

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("absent.png"), "{stderr}");
}
