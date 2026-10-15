// SPDX-License-Identifier: MIT OR Apache-2.0
//! Levi's load engine.
//!
//! This crate turns a compiled scenario into a run: it spawns worker threads, puts a
//! pool of virtual users on each, merges their per-worker HDR histograms four times a
//! second, and hands the numbers to reporters.
//!
//! # Shape of a run
//!
//! ```text
//!   Coordinating runtime (this one)            Worker thread (one per `workers`)
//!   +--------------------------------+         +-------------------------------+
//!   |  flush loop: every 250 ms      |         |  current_thread runtime       |
//!   |  drain each worker, merge into |         |  N VU tasks, each with its    |
//!   |  the run histogram, snapshot   |<--------|  own seeded render context    |
//!   |  -> reporters                  |  atomics + HDR mutex                |
//!   +--------------------------------+         +-------------------------------+
//! ```
//!
//! A **VU is not a thread**: it is a logical async task, so ten thousand of them fit
//! where ten thousand threads would not. A worker *is* a thread, with its own
//! single-threaded runtime, so the timing-critical path never migrates between cores.
//!
//! # What slice 1 covers
//!
//! The closed ([`lev_core::Executor::ConstantVus`]) executor, think time, per-worker
//! HDR percentiles, a 4 Hz aggregator, and a JSON report.
//!
//! # What slice 2 adds
//!
//! Thresholds and the verdict they produce ([`crate::threshold`]), the live `abortOn`
//! rule that stops a run as soon as it is failing, and a graceful stop: a
//! [`CancellationToken`] every VU checks, so Ctrl-C (or `abortOn`) ends the run with a
//! partial report flagged `aborted` instead of a hard kill.
//!
//! Still not here, and named so the gaps stay on the record: open/ramping executors,
//! coordinated-omission correction, scripts and assertions under load, and a request
//! timeout in the transport — which is what would bound the drain of a *stuck* request.

#![forbid(unsafe_code)]

mod clock;
mod error;
mod exec;
mod metrics;
mod scenario;
mod sender;
mod threshold;

pub mod report;

pub use clock::{Clock, TokioClock};
pub use error::LoadError;
pub use report::{report_to_json, JsonReporter, Reporter};
pub use scenario::{CompiledScenario, CompiledStep};
pub use sender::{HttpSender, SendResult, Sender};

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use lev_core::secrets::SecretStore;
use lev_core::{Context, VarSet};
use lev_metrics::{MetricsRecorder, RunId, RunOutcome, RunReport, StopReason};
use tokio_util::sync::CancellationToken;

use crate::exec::FLUSH_INTERVAL;
use crate::metrics::{flush, new_histogram, WorkerMetrics};
use crate::threshold::{abort_trigger, evaluate_thresholds};

/// How long an aborted run waits for in-flight iterations to finish before it reports.
///
/// A graceful stop means "stop scheduling, drain what is in flight". The drain cannot be
/// unbounded — a hung target must not hang Ctrl-C — so it gets a deadline, after which
/// the report is built from what was measured. The plan calls this `gracefulStop`.
pub const DEFAULT_GRACEFUL_STOP: Duration = Duration::from_secs(5);

/// Everything one run needs: what to run, with what scope, and how hard.
///
/// The clock is part of the config, not a hidden global, so a test can run a nominal
/// hour in microseconds by handing in a paused one.
pub struct RunConfig {
    /// Correlates snapshots and the final report.
    pub run_id: RunId,
    /// The scenario, already bound to compiled requests.
    pub scenario: Arc<CompiledScenario>,
    /// The merged variable scope (global < collection < environment).
    pub vars: VarSet,
    /// Where secrets are read from. Never serialized.
    pub secrets: Arc<dyn SecretStore>,
    /// The base RNG seed. Each VU derives its own, so runs are reproducible.
    pub seed: u64,
    /// Worker threads; VUs are dealt out across them.
    pub workers: usize,
    /// The time source.
    pub clock: Arc<dyn Clock>,
    /// The stop signal. Cancelling it stops scheduling new iterations; the VUs finish
    /// the request they are on, and the run reports what it measured.
    pub cancel: CancellationToken,
    /// How long to wait for in-flight iterations after a stop.
    pub graceful_stop: Duration,
}

impl RunConfig {
    /// A config with a sensible worker count and the production clock.
    pub fn new(
        run_id: RunId,
        scenario: Arc<CompiledScenario>,
        vars: VarSet,
        secrets: Arc<dyn SecretStore>,
        seed: u64,
    ) -> Self {
        Self {
            run_id,
            scenario,
            vars,
            secrets,
            seed,
            workers: default_worker_count(),
            clock: Arc::new(TokioClock::new()),
            cancel: CancellationToken::new(),
            graceful_stop: DEFAULT_GRACEFUL_STOP,
        }
    }

    /// Override the worker-thread count (`--workers`).
    pub fn with_workers(mut self, workers: usize) -> Self {
        self.workers = workers;
        self
    }

    /// Inject a clock (tests).
    pub fn with_clock(mut self, clock: Arc<dyn Clock>) -> Self {
        self.clock = clock;
        self
    }

    /// Share a stop signal, so an outside actor (Ctrl-C) can end the run gracefully.
    pub fn with_cancel(mut self, cancel: CancellationToken) -> Self {
        self.cancel = cancel;
        self
    }

    /// Override how long a stopped run waits for in-flight iterations.
    pub fn with_graceful_stop(mut self, graceful_stop: Duration) -> Self {
        self.graceful_stop = graceful_stop;
        self
    }
}

fn default_worker_count() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(4)
}

/// Run a scenario against the real network and return its report.
pub async fn run(
    config: RunConfig,
    reporters: &mut [Box<dyn Reporter>],
) -> Result<RunReport, LoadError> {
    let sender: Arc<dyn Sender> = Arc::new(HttpSender::new()?);
    run_with(config, sender, reporters).await
}

/// Run a scenario with an explicit [`Sender`].
///
/// The executor only ever talks to the [`Sender`] trait, so tests can substitute one
/// that returns instantly and pair it with a paused clock.
pub async fn run_with(
    config: RunConfig,
    sender: Arc<dyn Sender>,
    reporters: &mut [Box<dyn Reporter>],
) -> Result<RunReport, LoadError> {
    let scenario = Arc::clone(&config.scenario);
    if scenario.vus == 0 {
        return Err(LoadError::Config(format!(
            "scenario `{}` has no virtual users",
            scenario.id
        )));
    }

    // Compile the scope once against every step, so a typo'd variable or a missing
    // secret fails in `Preparing` rather than silently becoming 10,000 failed requests.
    validate(&config)?;

    let workers = config.workers.max(1).min(scenario.vus as usize);
    let started_at = config.clock.now();
    let started_at_unix_ms = unix_millis();
    let deadline = started_at + scenario.duration;
    let cancel = config.cancel.clone();

    // One metrics cell per worker; VUs are dealt out round-robin by count.
    let per_worker: Vec<Arc<WorkerMetrics>> = (0..workers)
        .map(|_| Arc::new(WorkerMetrics::new()))
        .collect();
    let counts = distribute(scenario.vus, workers);

    let mut handles = Vec::with_capacity(workers);
    let mut base_index = 0u32;
    for (index, count) in counts.iter().copied().enumerate() {
        if count == 0 {
            continue;
        }
        let scenario = Arc::clone(&scenario);
        let vars = config.vars.clone();
        let secrets = Arc::clone(&config.secrets);
        let sender = Arc::clone(&sender);
        let clock = Arc::clone(&config.clock);
        let metrics = Arc::clone(&per_worker[index]);
        let cancel = cancel.clone();
        let seed = config.seed;
        handles.push(std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build a worker runtime");
            runtime.block_on(exec::run_vus(
                scenario,
                vars,
                secrets,
                sender,
                clock,
                metrics,
                cancel,
                base_index,
                count,
                deadline,
                seed,
            ));
        }));
        base_index += count;
    }

    // The aggregator: flush at 4 Hz until the planned duration elapses, the run is
    // cancelled, or `abortOn` fires.
    let mut recorder = MetricsRecorder::new();
    let mut run_histogram = new_histogram()?;
    let mut max_scheduler_lag = Duration::ZERO;
    let mut next_flush = started_at + FLUSH_INTERVAL;
    let mut stop_reason: Option<StopReason> = None;
    loop {
        let now = config.clock.now();
        if now >= deadline {
            break;
        }
        let wake = next_flush.min(deadline);
        // Wake on the flush *or* the stop signal, whichever comes first, so a Ctrl-C is
        // acted on immediately rather than after up to a quarter second of load.
        tokio::select! {
            _ = config.clock.sleep_until(wake) => {}
            _ = cancel.cancelled() => {
                stop_reason = Some(StopReason::Interrupt);
                break;
            }
        }

        let elapsed = config.clock.now().saturating_duration_since(started_at);
        let scheduler_lag = config.clock.now().saturating_duration_since(next_flush);
        max_scheduler_lag = max_scheduler_lag.max(scheduler_lag);
        flush(&per_worker, &mut run_histogram, &mut recorder, scenario.vus)?;

        let snapshot = recorder.snapshot(elapsed, scheduler_lag);
        for reporter in reporters.iter_mut() {
            reporter.on_snapshot(&snapshot)?;
        }
        next_flush += FLUSH_INTERVAL;

        // `abortOn`: judge the run so far and stop a run that is already failing, rather
        // than letting it hammer a broken target for the rest of its duration. This is
        // the only reason a threshold can end a run early.
        if !scenario.abort_on.is_empty() {
            let results = evaluate_thresholds(&scenario.thresholds, &snapshot, &run_histogram);
            if let Some(metric) =
                abort_trigger(&scenario.thresholds, &scenario.abort_on, &results)
            {
                stop_reason = Some(StopReason::Threshold {
                    metric: metric.name().to_string(),
                });
                cancel.cancel();
                break;
            }
        }
    }

    // Workers stop themselves at the deadline. A stopped run gives in-flight iterations
    // a bounded window to finish before the report is built from what was measured.
    if stop_reason.is_some() {
        drain_workers(handles, config.graceful_stop).await;
    } else {
        join_workers(handles).await?;
    }

    let duration = config.clock.now().saturating_duration_since(started_at);
    // A stopped run has no virtual users in flight; saying otherwise would overstate it.
    let active_vus = if stop_reason.is_some() { 0 } else { scenario.vus };
    flush(&per_worker, &mut run_histogram, &mut recorder, active_vus)?;

    // Build the final report from one last snapshot, so counters and latency are the
    // same numbers the last reporter saw. The verdict is taken here, on the completed
    // numbers, and is the one the report carries — a live `abortOn` failure and the
    // final verdict can differ, because a partial run is not a full run.
    let final_snapshot = recorder.snapshot(duration, max_scheduler_lag);
    let thresholds = evaluate_thresholds(&scenario.thresholds, &final_snapshot, &run_histogram);
    let report = RunReport {
        run_id: config.run_id,
        outcome: if stop_reason.is_some() {
            RunOutcome::Aborted
        } else {
            RunOutcome::Completed
        },
        stopped_by: stop_reason,
        started_at_unix_ms,
        planned_duration: scenario.duration,
        duration,
        max_vus: recorder.max_vus(),
        http_reqs: final_snapshot.http_reqs,
        http_req_failed: final_snapshot.http_req_failed,
        iterations: final_snapshot.iterations,
        checks: final_snapshot.checks,
        checks_failed: final_snapshot.checks_failed,
        script_errors: final_snapshot.script_errors,
        scheduler_lag: final_snapshot.scheduler_lag,
        latency: final_snapshot.latency,
        thresholds,
    };

    for reporter in reporters.iter_mut() {
        reporter.on_finish(&report)?;
    }
    Ok(report)
}

/// How often a draining run checks whether a worker has finished.
const DRAIN_POLL: Duration = Duration::from_millis(2);

/// Join every worker after a run reaches its planned duration.
///
/// Workers stop themselves at the deadline, so this always returns; the blocking join is
/// handed to the blocking pool rather than blocking the coordinating runtime.
async fn join_workers(handles: Vec<std::thread::JoinHandle<()>>) -> Result<(), LoadError> {
    for handle in handles {
        tokio::task::spawn_blocking(move || handle.join())
            .await
            .map_err(|error| LoadError::Internal(format!("worker join failed: {error}")))?
            .map_err(|_| LoadError::Internal("a worker thread panicked".to_string()))?;
    }
    Ok(())
}

/// Wait out a stop, but only for `window`.
///
/// Every VU checks the stop signal between steps, so this normally returns almost
/// immediately: the workers are finishing the one request they were on. The window
/// exists for the other case — a request that never comes back — because an operator who
/// pressed Ctrl-C must get a report, not a hang. A worker still in flight when the
/// window closes is left to finish on its own.
///
/// The window is measured with the **real** clock, not the run's [`Clock`]: it is a bound
/// on how long a human waits, not a measurement, and it must not be affected by the
/// virtual time a test injects. (A request timeout in `lev-http` is what would let the
/// in-flight case be bounded by the run itself; that is a later slice.)
async fn drain_workers(handles: Vec<std::thread::JoinHandle<()>>, window: Duration) {
    let deadline = std::time::Instant::now() + window;
    for handle in handles {
        while !handle.is_finished() {
            if std::time::Instant::now() >= deadline {
                return;
            }
            tokio::time::sleep(DRAIN_POLL).await;
        }
        let _ = handle.join();
    }
}

/// Render every step once, to catch a bad request before the run starts.
fn validate(config: &RunConfig) -> Result<(), LoadError> {
    let mut ctx = Context::with_seed(config.vars.clone(), config.secrets.as_ref(), config.seed);
    for step in &config.scenario.steps {
        step.request.prepare(&mut ctx).map_err(|error| {
            LoadError::Config(format!(
                "scenario `{}` step `{}`: {error}",
                config.scenario.id, step.request_id
            ))
        })?;
    }
    Ok(())
}

/// Split `vus` across `workers` as evenly as possible; the remainder goes to the first
/// few workers, so `sum(counts) == vus` exactly.
fn distribute(vus: u32, workers: usize) -> Vec<u32> {
    let workers = workers as u32;
    let base = vus / workers;
    let remainder = vus % workers;
    (0..workers)
        .map(|index| base + u32::from(index < remainder))
        .collect()
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vus_are_distributed_without_loss() {
        assert_eq!(distribute(10, 3), vec![4, 3, 3]);
        assert_eq!(distribute(9, 3), vec![3, 3, 3]);
        assert_eq!(distribute(1, 4), vec![1, 0, 0, 0]);
        let counts = distribute(1000, 7);
        assert_eq!(counts.iter().sum::<u32>(), 1000);
        assert_eq!(counts.len(), 7);
    }
}
