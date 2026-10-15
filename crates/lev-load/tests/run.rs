// SPDX-License-Identifier: MIT OR Apache-2.0
//! An end-to-end load run against a mock target with **injected latency**.
//!
//! This is the load engine's accuracy gate (Phase 3): if the measured p50 does not
//! land near the latency the target was told to add, the numbers are wrong no matter
//! how pretty the report is. It runs on real threads with a real clock, so it is short
//! (about a second) and asserts loose bounds.

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use lev_core::{
    Collection, Executor, MapSecretStore, Method, PreparedRequest, RequestDef, Scenario, Step,
    Template, Threshold, ThresholdMetric, VarSet,
};
use lev_load::{run_with, CompiledScenario, HttpSender, RunConfig, SendResult, Sender};
use lev_metrics::{RunId, RunOutcome, StopReason};
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn compiled_scenario(
    base_url: &str,
    vus: u32,
    duration: Duration,
    thresholds: Vec<Threshold>,
    abort_on: Vec<ThresholdMetric>,
) -> Arc<CompiledScenario> {
    let mut requests = BTreeMap::new();
    requests.insert(
        "thing".to_string(),
        RequestDef {
            id: "thing".to_string(),
            name: "Thing".to_string(),
            method: Method::Get,
            url: Template::parse("/thing").unwrap(),
            headers: Vec::new(),
            body: Default::default(),
            auth: None,
            scripts: Default::default(),
            assertions: Vec::new(),
        },
    );

    let scenario = Scenario {
        id: "scenario".to_string(),
        name: "Scenario".to_string(),
        executor: Executor::ConstantVus { vus, duration },
        steps: vec![Step {
            request: "thing".to_string(),
            think_time: None,
        }],
        thresholds,
        abort_on,
    };

    let collection = Collection {
        base_url: Some(Template::parse(base_url).unwrap()),
        ..Collection::empty()
    };

    Arc::new(CompiledScenario::compile(&scenario, &collection, &requests).unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_measures_roughly_the_injected_latency() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/thing"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(50)))
        .mount(&server)
        .await;

    let scenario = compiled_scenario(&server.uri(), 2, Duration::from_millis(1200), Vec::new(), Vec::new());
    let config = RunConfig::new(
        RunId::new("run-1"),
        scenario,
        VarSet::new(),
        Arc::new(MapSecretStore::new()),
        0,
    )
    .with_workers(2);

    let mut reporters: Vec<Box<dyn lev_load::Reporter>> = Vec::new();
    let report = run_with(config, Arc::new(HttpSender::new().unwrap()), &mut reporters)
        .await
        .unwrap();

    assert_eq!(report.outcome, RunOutcome::Completed);
    assert_eq!(report.http_req_failed, 0);
    assert!(report.http_reqs > 0, "expected some requests");
    assert_eq!(report.max_vus, 2);

    // The target adds 50ms; a closed model at 2 VUs should see a p50 in that ballpark.
    let p50 = report.latency.p50.expect("a p50");
    assert!(
        p50 >= Duration::from_millis(40) && p50 <= Duration::from_millis(500),
        "p50 was {p50:?}, expected ~50ms"
    );

    // 2 VUs / 50ms is ~40 rps; assert a loose floor to catch a stalled run.
    assert!(
        report.requests_per_second() > 5.0,
        "rps was {}",
        report.requests_per_second()
    );
}

/// A target that always fails, for the threshold tests.
async fn failing_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/thing"))
        .respond_with(ResponseTemplate::new(500))
        .mount(&server)
        .await;
    server
}

fn config(scenario: Arc<CompiledScenario>, cancel: CancellationToken) -> RunConfig {
    RunConfig::new(
        RunId::new("run-1"),
        scenario,
        VarSet::new(),
        Arc::new(MapSecretStore::new()),
        0,
    )
    .with_workers(2)
    .with_cancel(cancel)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_threshold_is_reported_without_aborting_the_run() {
    let server = failing_server().await;
    let threshold = Threshold::parse("http_req_failed", "rate<0.01").unwrap();
    let scenario = compiled_scenario(
        &server.uri(),
        2,
        Duration::from_millis(600),
        vec![threshold],
        Vec::new(),
    );

    let mut reporters: Vec<Box<dyn lev_load::Reporter>> = Vec::new();
    let report = run_with(
        config(scenario, CancellationToken::new()),
        Arc::new(HttpSender::new().unwrap()),
        &mut reporters,
    )
    .await
    .unwrap();

    // The run did what it was told: it completed, and every request failed.
    assert_eq!(report.outcome, RunOutcome::Completed);
    assert_eq!(report.stopped_by, None);
    assert!(report.http_req_failed > 0);

    // And the verdict says so, with the number behind it.
    assert_eq!(report.thresholds.len(), 1);
    let result = &report.thresholds[0];
    assert_eq!(result.metric, "http_req_failed");
    assert_eq!(result.expression, "rate<0.01");
    assert!(!result.passed);
    assert_eq!(result.observed, Some(1.0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abort_on_ends_a_failing_run_early() {
    let server = failing_server().await;
    let threshold = Threshold::parse("http_req_failed", "rate<0.01").unwrap();
    // A duration long enough that stopping early is unmistakable.
    let scenario = compiled_scenario(
        &server.uri(),
        2,
        Duration::from_secs(5),
        vec![threshold],
        vec![ThresholdMetric::HttpReqFailed],
    );

    let mut reporters: Vec<Box<dyn lev_load::Reporter>> = Vec::new();
    let report = run_with(
        config(scenario, CancellationToken::new()),
        Arc::new(HttpSender::new().unwrap()),
        &mut reporters,
    )
    .await
    .unwrap();

    assert_eq!(report.outcome, RunOutcome::Aborted);
    assert_eq!(
        report.stopped_by,
        Some(StopReason::Threshold {
            metric: "http_req_failed".to_string()
        })
    );
    // It stopped at the first flush with failures, not after five seconds.
    assert!(
        report.duration < Duration::from_secs(2),
        "aborted too late: {:?}",
        report.duration
    );
    // The verdict is still on the record.
    assert!(report.thresholds.iter().any(|t| !t.passed));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_run_that_fails_a_plain_threshold_still_measures_everything() {
    // Two thresholds, one of which passes, to prove the report keeps the order and the
    // verdicts separate.
    let server = failing_server().await;
    let thresholds = vec![
        Threshold::parse("http_req_failed", "rate<0.01").unwrap(),
        Threshold::parse("http_reqs", "count>0").unwrap(),
    ];
    let scenario = compiled_scenario(
        &server.uri(),
        2,
        Duration::from_millis(400),
        thresholds,
        Vec::new(),
    );

    let mut reporters: Vec<Box<dyn lev_load::Reporter>> = Vec::new();
    let report = run_with(
        config(scenario, CancellationToken::new()),
        Arc::new(HttpSender::new().unwrap()),
        &mut reporters,
    )
    .await
    .unwrap();

    assert_eq!(report.thresholds.len(), 2);
    assert!(!report.thresholds[0].passed, "the rate threshold should fail");
    assert!(report.thresholds[1].passed, "the count threshold should pass");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pre_cancelled_run_reports_aborted_and_sends_nothing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/thing"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let scenario = compiled_scenario(&server.uri(), 2, Duration::from_secs(30), Vec::new(), Vec::new());

    let cancel = CancellationToken::new();
    cancel.cancel();

    let mut reporters: Vec<Box<dyn lev_load::Reporter>> = Vec::new();
    let report = run_with(
        config(scenario, cancel),
        Arc::new(HttpSender::new().unwrap()),
        &mut reporters,
    )
    .await
    .unwrap();

    assert_eq!(report.outcome, RunOutcome::Aborted);
    assert_eq!(report.stopped_by, Some(StopReason::Interrupt));
    assert_eq!(report.http_reqs, 0, "nothing should have been sent");
    assert_eq!(report.max_vus, 0);
}

/// A sender that never returns: the request is in flight when the run is stopped.
struct StuckSender;

impl Sender for StuckSender {
    fn send<'a>(
        &'a self,
        _request: PreparedRequest,
    ) -> Pin<Box<dyn Future<Output = SendResult> + Send + 'a>> {
        Box::pin(std::future::pending())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stuck_request_does_not_hang_a_stopped_run() {
    let scenario = compiled_scenario(
        "http://127.0.0.1:1",
        2,
        Duration::from_secs(30),
        Vec::new(),
        Vec::new(),
    );

    let cancel = CancellationToken::new();
    let cancel_for_task = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(100)).await;
        cancel_for_task.cancel();
    });

    let wall = std::time::Instant::now();
    let mut reporters: Vec<Box<dyn lev_load::Reporter>> = Vec::new();
    let report = run_with(
        config(scenario, cancel).with_graceful_stop(Duration::from_millis(200)),
        Arc::new(StuckSender),
        &mut reporters,
    )
    .await
    .unwrap();

    // The two VUs are wedged inside `send` forever. Ctrl-C must still produce a report,
    // bounded by `graceful_stop` rather than by the stuck request.
    assert_eq!(report.outcome, RunOutcome::Aborted);
    assert_eq!(report.stopped_by, Some(StopReason::Interrupt));
    assert!(
        wall.elapsed() < Duration::from_secs(5),
        "the stop took {:?}; the drain bound did not hold",
        wall.elapsed()
    );
}
