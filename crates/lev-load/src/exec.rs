// SPDX-License-Identifier: MIT OR Apache-2.0
//! The closed executor: a fixed pool of virtual users looping their steps.
//!
//! A VU is a logical async task, not a thread. Each one owns a long-lived
//! [`Context`] — so `{{$uuid}}` advances every iteration instead of repeating — and
//! loops its steps until the run's deadline. Between steps it honours think time
//! through the injected [`Clock`], so the whole loop can be driven in virtual time.
//!
//! A VU also watches a [`CancellationToken`], which is how a graceful stop works:
//! the token goes off between steps, so the request in flight is allowed to finish and
//! the VU leaves at the next safe point rather than being killed mid-send.

use std::sync::Arc;
use std::time::Duration;

use lev_core::secrets::SecretStore;
use lev_core::{Context, VarSet};
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::clock::Clock;
use crate::metrics::WorkerMetrics;
use crate::scenario::CompiledScenario;
use crate::sender::Sender;

/// How often the aggregator flushes: 4 Hz, per the plan.
pub(crate) const FLUSH_INTERVAL: Duration = Duration::from_millis(250);

/// Run `count` virtual users, numbered from `base_index`, until `deadline` or a stop.
///
/// Every task shares one [`WorkerMetrics`], so a worker's numbers are its own. The
/// function returns once all its VUs have finished.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn run_vus(
    scenario: Arc<CompiledScenario>,
    vars: VarSet,
    secrets: Arc<dyn SecretStore>,
    sender: Arc<dyn Sender>,
    clock: Arc<dyn Clock>,
    metrics: Arc<WorkerMetrics>,
    cancel: CancellationToken,
    base_index: u32,
    count: u32,
    deadline: Instant,
    seed: u64,
) {
    let mut tasks = Vec::with_capacity(count as usize);
    for offset in 0..count {
        let index = base_index + offset;
        tasks.push(tokio::spawn(run_one_vu(
            index,
            seed,
            deadline,
            Arc::clone(&scenario),
            vars.clone(),
            Arc::clone(&secrets),
            Arc::clone(&sender),
            Arc::clone(&clock),
            Arc::clone(&metrics),
            cancel.clone(),
        )));
    }
    for task in tasks {
        // A VU task never panics; if one did, the worker still finishes its peers.
        let _ = task.await;
    }
}

/// One virtual user: an iteration counter, a long-lived render context, and a loop.
#[allow(clippy::too_many_arguments)]
async fn run_one_vu(
    index: u32,
    seed: u64,
    deadline: Instant,
    scenario: Arc<CompiledScenario>,
    vars: VarSet,
    secrets: Arc<dyn SecretStore>,
    sender: Arc<dyn Sender>,
    clock: Arc<dyn Clock>,
    metrics: Arc<WorkerMetrics>,
    cancel: CancellationToken,
) {
    // Each VU gets its own seed, so two VUs never send the same `{{$uuid}}` stream.
    let seed = seed.wrapping_add(u64::from(index));
    // One context for the VU's whole life: the seeded RNG keeps advancing, so dynamic
    // functions differ every iteration (a fresh context per iteration would not).
    let mut ctx = Context::with_seed(vars, secrets.as_ref(), seed);

    // The label is what makes a stop *graceful*: leaving between steps (rather than
    // mid-send) means the request already in flight still counts, and an iteration that
    // was cut short is not counted as a completed one.
    'vu: while clock.now() < deadline && !cancel.is_cancelled() {
        for step in &scenario.steps {
            if cancel.is_cancelled() {
                break 'vu;
            }

            let prepared = match step.request.prepare(&mut ctx) {
                Ok(prepared) => prepared,
                Err(_) => {
                    // `validate` rules out a bad request up front, so a failure here is
                    // a runtime surprise: count it as a failed request, never panic.
                    metrics.record_request(true);
                    continue;
                }
            };

            let started = clock.now();
            let status = sender.send(prepared).await;
            let latency = clock.now().saturating_duration_since(started);

            metrics.record_request(!status.is_some_and(is_success));
            metrics.record_latency(u64::try_from(latency.as_micros()).unwrap_or(u64::MAX));

            if let Some(think_time) = step.think_time {
                // Never sleep past the deadline. Without the clamp, the trailing think
                // time of the final iteration could push past `deadline` on a *paused*
                // clock without the outer `while` ever seeing the clock move — an
                // infinite spin. Clamping keeps a run terminating in virtual time too.
                let wake = (clock.now() + think_time).min(deadline);
                // A stop during think time should end the wait, not be served after it:
                // otherwise a long think time would look like a hung run on Ctrl-C.
                tokio::select! {
                    _ = clock.sleep_until(wake) => {}
                    _ = cancel.cancelled() => break 'vu,
                }
            }
        }
        metrics.record_iteration();
    }
}

/// k6's `http_req_failed` rule: 2xx and 3xx are successes.
fn is_success(status: u16) -> bool {
    (200..400).contains(&status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::future::Future;
    use std::pin::Pin;
    use std::time::Duration as StdDuration;

    use lev_core::{
        Collection, Executor, MapSecretStore, Method, RequestDef, Scenario, Step, Template, VarSet,
    };
    use lev_metrics::MetricsRecorder;

    use crate::metrics::{flush, new_histogram};
    use crate::scenario::CompiledScenario;
    use crate::sender::SendResult;
    use crate::TokioClock;

    /// A sender that returns instantly, so a paused run advances only on think time.
    struct InstantSender;

    impl Sender for InstantSender {
        fn send<'a>(
            &'a self,
            _request: lev_core::PreparedRequest,
        ) -> Pin<Box<dyn Future<Output = SendResult> + Send + 'a>> {
            Box::pin(async { Some(200) })
        }
    }

    fn build_scenario(
        vus: u32,
        duration: StdDuration,
        think_time: Option<StdDuration>,
    ) -> Arc<CompiledScenario> {
        let mut requests = BTreeMap::new();
        requests.insert(
            "ping".to_string(),
            RequestDef {
                id: "ping".to_string(),
                name: "Ping".to_string(),
                method: Method::Get,
                url: Template::parse("https://example.test/ping").unwrap(),
                headers: Vec::new(),
                body: Default::default(),
                auth: None,
                scripts: Default::default(),
                assertions: Vec::new(),
            },
        );
        let scenario = Scenario {
            id: "s".to_string(),
            name: "S".to_string(),
            executor: Executor::ConstantVus { vus, duration },
            steps: vec![Step {
                request: "ping".to_string(),
                think_time,
            }],
            thresholds: Vec::new(),
            abort_on: Vec::new(),
        };
        Arc::new(CompiledScenario::compile(&scenario, &Collection::empty(), &requests).unwrap())
    }

    /// The correctness gate for the executor: with an injected clock, N VUs x a known
    /// number of iterations, in zero wall time.
    #[tokio::test(start_paused = true)]
    async fn a_paused_clock_runs_the_planned_iterations_in_no_wall_time() {
        let wall_start = std::time::Instant::now();

        // 10 iterations per VU: think time 100ms, duration 1s.
        let scenario = build_scenario(
            4,
            StdDuration::from_secs(1),
            Some(StdDuration::from_millis(100)),
        );
        let metrics = Arc::new(WorkerMetrics::new());
        let clock: Arc<dyn Clock> = Arc::new(TokioClock::new());
        let deadline = clock.now() + scenario.duration;

        run_vus(
            Arc::clone(&scenario),
            VarSet::new(),
            Arc::new(MapSecretStore::new()),
            Arc::new(InstantSender),
            Arc::clone(&clock),
            Arc::clone(&metrics),
            CancellationToken::new(),
            0,
            4,
            deadline,
            0,
        )
        .await;

        let counters = metrics.take_counters();
        assert_eq!(counters.iterations, 40, "4 VUs x 10 iterations");
        assert_eq!(counters.http_reqs, 40);
        assert_eq!(counters.http_req_failed, 0);

        // A nominal second passed on the clock, but no real time did.
        assert!(clock.now() >= deadline);
        assert!(
            wall_start.elapsed() < StdDuration::from_secs(1),
            "the run should not have slept in real time, took {:?}",
            wall_start.elapsed()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn think_time_is_honoured_between_iterations() {
        // No think time and instant sends would spin forever, so a small think time is
        // what makes the iteration count finite and predictable.
        let scenario = build_scenario(
            1,
            StdDuration::from_millis(500),
            Some(StdDuration::from_millis(100)),
        );
        let metrics = Arc::new(WorkerMetrics::new());
        let clock: Arc<dyn Clock> = Arc::new(TokioClock::new());
        let deadline = clock.now() + scenario.duration;

        run_vus(
            scenario,
            VarSet::new(),
            Arc::new(MapSecretStore::new()),
            Arc::new(InstantSender),
            clock,
            Arc::clone(&metrics),
            CancellationToken::new(),
            0,
            1,
            deadline,
            0,
        )
        .await;

        assert_eq!(metrics.take_counters().iterations, 5);
    }

    #[test]
    fn failures_are_counted_as_requests_too() {
        // A transport failure (`None`) is a failed request, not a missing one.
        assert!(!Option::<u16>::None.is_some_and(is_success));
        assert!(!Some(500).is_some_and(is_success));
        assert!(Some(201).is_some_and(is_success));
        assert!(Some(302).is_some_and(is_success));
    }

    #[tokio::test(start_paused = true)]
    async fn latency_flows_through_the_flush_into_the_recorder() {
        let scenario = build_scenario(
            1,
            StdDuration::from_millis(300),
            Some(StdDuration::from_millis(100)),
        );
        let metrics = Arc::new(WorkerMetrics::new());
        let clock: Arc<dyn Clock> = Arc::new(TokioClock::new());
        let deadline = clock.now() + scenario.duration;

        // Record one known latency, plus a normal run.
        metrics.record_latency(1_234);

        run_vus(
            scenario,
            VarSet::new(),
            Arc::new(MapSecretStore::new()),
            Arc::new(InstantSender),
            clock,
            Arc::clone(&metrics),
            CancellationToken::new(),
            0,
            1,
            deadline,
            0,
        )
        .await;

        let mut histogram = new_histogram().unwrap();
        let mut recorder = MetricsRecorder::new();
        flush(&[Arc::clone(&metrics)], &mut histogram, &mut recorder, 1).unwrap();
        let snapshot = recorder.snapshot(StdDuration::from_millis(300), StdDuration::ZERO);

        assert_eq!(snapshot.iterations, 3);
        assert_eq!(snapshot.http_reqs, 3);
        // The histogram saw the injected sample dominate the top percentile. The exact
        // value is bucketed to three significant figures, so assert a range.
        let p99 = snapshot.latency.p99.unwrap();
        assert!(
            p99 >= StdDuration::from_micros(1_200) && p99 <= StdDuration::from_millis(2),
            "expected p99 near 1.234ms, got {p99:?}"
        );
    }
}
