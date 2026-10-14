// SPDX-License-Identifier: MIT OR Apache-2.0
//! `levi run` through the **real binary**: a scenario, a collection, a mock target,
//! and the exit-code contract.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::process::Command;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn scratch_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("levi-run-{label}-{nanos}"));
    std::fs::create_dir_all(dir.join("requests")).expect("create requests dir");
    std::fs::create_dir_all(dir.join("load")).expect("create load dir");
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
async fn a_run_completes_and_writes_a_report() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/thing"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{\"ok\":true}"))
        .mount(&server)
        .await;

    let dir = scratch_dir("ok");
    write_collection(&dir, &server.uri());
    std::fs::write(
        dir.join("requests/thing.yaml"),
        "id: thing\nname: A thing\nmethod: GET\nurl: /thing\n",
    )
    .expect("write request");
    // A short scenario: one VU, 300ms, so the test is quick.
    std::fs::write(
        dir.join("load/scenario.yaml"),
        "id: smoke\nname: Smoke\nexecutor:\n  constantVus:\n    vus: 1\n    duration: 300ms\nsteps:\n  - request: thing\n",
    )
    .expect("write scenario");

    let report_path = dir.join("report.json");
    let output = Command::new(env!("CARGO_BIN_EXE_levi"))
        .args([
            "run",
            "load/scenario.yaml",
            "--workers",
            "1",
            "--out",
            report_path.to_str().unwrap(),
        ])
        .current_dir(&dir)
        .output()
        .await
        .expect("run levi run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("http_reqs"), "{stdout}");

    let text = std::fs::read_to_string(&report_path).expect("report written");
    let report: serde_json::Value = serde_json::from_str(&text).expect("report is json");
    assert_eq!(report["outcome"], "completed");
    assert!(
        report["metrics"]["http_reqs"].as_u64().unwrap() > 0,
        "{text}"
    );
    assert_eq!(report["metrics"]["http_req_failed"], 0);

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_scenario_naming_an_unknown_request_exits_two() {
    let dir = scratch_dir("bad");
    write_collection(&dir, "http://127.0.0.1:1");
    std::fs::write(
        dir.join("requests/thing.yaml"),
        "id: thing\nname: A thing\nmethod: GET\nurl: /thing\n",
    )
    .expect("write request");
    std::fs::write(
        dir.join("load/scenario.yaml"),
        "id: smoke\nname: Smoke\nexecutor:\n  constantVus:\n    vus: 1\n    duration: 100ms\nsteps:\n  - request: does-not-exist\n",
    )
    .expect("write scenario");

    let output = Command::new(env!("CARGO_BIN_EXE_levi"))
        .args(["run", "load/scenario.yaml"])
        .current_dir(&dir)
        .output()
        .await
        .expect("run levi run");

    assert_eq!(output.status.code(), Some(2), "{output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("unknown request"), "{stderr}");

    std::fs::remove_dir_all(&dir).ok();
}
