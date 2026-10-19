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

/// Write a collection, a request, and a scenario that drives it.
fn write_scenario_with_requests(dir: &Path, base_url: &str, scenario_yaml: &str) {
    std::fs::create_dir_all(dir.join("requests")).expect("create requests dir");
    std::fs::create_dir_all(dir.join("load")).expect("create load dir");
    write_collection(dir, base_url);
    std::fs::write(
        dir.join("requests/thing.yaml"),
        "id: thing\nname: A thing\nmethod: GET\nurl: /thing\n",
    )
    .expect("write request");
    std::fs::write(dir.join("load/scenario.yaml"), scenario_yaml).expect("write scenario");
}

#[tokio::test]
async fn a_failed_threshold_exits_one_and_prints_the_verdict() {
    // Every request 500s, so `rate<0.01` must fail and the run must still complete.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/thing"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let dir = scratch_dir("threshold");
    write_scenario_with_requests(
        &dir,
        &server.uri(),
        "id: fail\nname: Fail\nexecutor:\n  constantVus:\n    vus: 1\n    duration: 400ms\nsteps:\n  - request: thing\nthresholds:\n  http_req_failed:\n    - rate<0.01\n",
    );

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
        Some(1),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stderr.contains("threshold"), "stderr:\n{stderr}");
    // The verdict is printed with the number behind it, not just a yes/no.
    assert!(stdout.contains("thresholds"), "{stdout}");
    assert!(stdout.contains("FAIL"), "{stdout}");
    assert!(stdout.contains("http_req_failed rate<0.01"), "{stdout}");

    // The report carries the same verdict for machines, and the run did complete.
    let text = std::fs::read_to_string(&report_path).expect("report written");
    let report: serde_json::Value = serde_json::from_str(&text).expect("report is json");
    assert_eq!(report["outcome"], "completed");
    assert!(
        report["thresholds"].as_array().unwrap().len() == 1,
        "{text}"
    );
    assert_eq!(report["thresholds"][0]["metric"], "http_req_failed");
    assert_eq!(report["thresholds"][0]["expression"], "rate<0.01");
    assert_eq!(report["thresholds"][0]["passed"], false);
    assert_eq!(report["thresholds"][0]["observed"], 1.0);
    assert_eq!(report["thresholds"][0]["unit"], "rate");

    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn abort_on_stops_a_failing_run_early() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/thing"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;

    let dir = scratch_dir("aborton");
    // Five seconds of plan; `abortOn` should cut it short at the first flush with
    // failures, well before the duration is up.
    write_scenario_with_requests(
        &dir,
        &server.uri(),
        "id: abort\nname: Abort\nexecutor:\n  constantVus:\n    vus: 1\n    duration: 5s\nsteps:\n  - request: thing\nthresholds:\n  http_req_failed:\n    - rate<0.01\nabortOn:\n  - http_req_failed\n",
    );

    let report_path = dir.join("report.json");
    let started = std::time::Instant::now();
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
    let elapsed = started.elapsed();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // A failed threshold is the actionable signal, so it outranks the abort.
    assert_eq!(
        output.status.code(),
        Some(1),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("stopped by"),
        "the abort should be on the record:\n{stdout}"
    );
    assert!(
        stdout.contains("threshold (http_req_failed)"),
        "stdout:\n{stdout}"
    );
    assert!(elapsed < std::time::Duration::from_secs(5), "{elapsed:?}");

    let text = std::fs::read_to_string(&report_path).expect("report written");
    let report: serde_json::Value = serde_json::from_str(&text).expect("report is json");
    assert_eq!(report["outcome"], "aborted");
    assert_eq!(report["stoppedBy"]["kind"], "threshold");
    assert_eq!(report["stoppedBy"]["metric"], "http_req_failed");
    assert_eq!(report["thresholds"][0]["passed"], false);

    std::fs::remove_dir_all(&dir).ok();
}

/// Ctrl-C through the real binary, on a platform that has signals to send.
///
/// This is the user-facing half of a graceful stop: SIGINT must yield a partial report
/// and exit `4`, not a hard kill. The engine-level pieces (the token, the drain) are
/// covered in `lev-load`; what only this test can prove is that the signal reaches the
/// token.
#[cfg(unix)]
#[tokio::test]
async fn ctrl_c_stops_a_run_with_a_partial_report() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/thing"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let dir = scratch_dir("interrupt");
    write_scenario_with_requests(
        &dir,
        &server.uri(),
        "id: long\nname: Long\nexecutor:\n  constantVus:\n    vus: 1\n    duration: 30s\nsteps:\n  - request: thing\n",
    );

    let mut child = Command::new(env!("CARGO_BIN_EXE_levi"))
        .args(["run", "load/scenario.yaml", "--workers", "1"])
        .current_dir(&dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn levi run");

    // Let it get going: a few flushes in, with requests actually measured.
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // `kill -INT` is what Ctrl-C delivers; the binary catches it and stops gracefully.
    let status = std::process::Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .expect("send SIGINT");
    assert!(status.success(), "could not signal the child");

    // The run planned 30 seconds; stopping must take well under that.
    let output = tokio::time::timeout(std::time::Duration::from_secs(8), child.wait_with_output())
        .await
        .expect("the run should stop promptly after SIGINT")
        .expect("wait for levi");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(4),
        "stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains("aborted"), "stdout:\n{stdout}");
    assert!(stderr.contains("run aborted"), "stderr:\n{stderr}");
}
