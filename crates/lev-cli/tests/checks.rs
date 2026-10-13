// SPDX-License-Identifier: MIT OR Apache-2.0
//! Phase 2 checks through the **real binary**: a request file with scripts and
//! assertions, and the exit-code contract (0 when checks pass, 1 when one fails).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::process::Command;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// A fresh, unique directory for one test's collection.
fn scratch_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("levi-cli-{label}-{nanos}"));
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

#[tokio::test]
async fn passing_checks_exit_zero() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/thing"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "application/json")
                .set_body_string("{\"ok\":true,\"id\":7}"),
        )
        .mount(&server)
        .await;

    let dir = scratch_dir("pass");
    write_collection(&dir, &server.uri());
    write_request(
        &dir,
        "thing.yaml",
        r#"id: thing
name: A thing
method: GET
url: /thing
assertions:
  - status: 200
  - bodyContains: '"ok":true'
  - jsonPath:
      path: id
      equals: "7"
scripts:
  postResponse: |
    lev.test('id is 7', function () { lev.expect(lev.response.json.id).toBe(7); });
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_levi"))
        .args(["request", "requests/thing.yaml"])
        .current_dir(&dir)
        .output()
        .await
        .expect("run levi request");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(0), "stdout:\n{stdout}");
    assert!(stdout.contains("checks"), "{stdout}");
    assert!(stdout.contains("PASS"), "{stdout}");
    assert!(!stdout.contains("FAIL"), "{stdout}");
}

#[tokio::test]
async fn a_failing_check_exits_one_and_explains_itself() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/thing"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{}"))
        .mount(&server)
        .await;

    let dir = scratch_dir("fail");
    write_collection(&dir, &server.uri());
    write_request(
        &dir,
        "bad.yaml",
        r#"id: bad
name: A failing request
method: GET
url: /thing
assertions:
  - status: 404
  - status: 200
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_levi"))
        .args(["request", "requests/bad.yaml"])
        .current_dir(&dir)
        .output()
        .await
        .expect("run levi request");

    // Exit code 1 is the check-failure contract.
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("FAIL"), "{stdout}");
    assert!(stdout.contains("status is 404"), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("checks failed"), "{stderr}");
}

#[tokio::test]
async fn a_broken_script_is_a_configuration_error() {
    let dir = scratch_dir("script");
    write_collection(&dir, "http://127.0.0.1:1");
    write_request(
        &dir,
        "broken.yaml",
        r#"id: broken
name: Broken script
method: GET
url: /thing
scripts:
  preRequest: |
    let x = ;
"#,
    );

    let output = Command::new(env!("CARGO_BIN_EXE_levi"))
        .args(["request", "requests/broken.yaml"])
        .current_dir(&dir)
        .output()
        .await
        .expect("run levi request");

    // A script is part of the request definition, so a broken one is exit 2.
    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("error:"), "{stderr}");
}
