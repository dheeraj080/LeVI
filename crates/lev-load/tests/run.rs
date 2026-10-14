// SPDX-License-Identifier: MIT OR Apache-2.0
//! An end-to-end load run against a mock target with **injected latency**.
//!
//! This is the load engine's accuracy gate (Phase 3): if the measured p50 does not
//! land near the latency the target was told to add, the numbers are wrong no matter
//! how pretty the report is. It runs on real threads with a real clock, so it is short
//! (about a second) and asserts loose bounds.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use lev_core::{
    Collection, Executor, MapSecretStore, Method, RequestDef, Scenario, Step, Template, VarSet,
};
use lev_load::{run_with, CompiledScenario, HttpSender, RunConfig};
use lev_metrics::{RunId, RunOutcome};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn compiled_scenario(base_url: &str, vus: u32, duration: Duration) -> Arc<CompiledScenario> {
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

    let scenario = compiled_scenario(&server.uri(), 2, Duration::from_millis(1200));
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
