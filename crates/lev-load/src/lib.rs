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
//! HDR percentiles, a 4 Hz aggregator, and a JSON report. Deliberately not yet here:
//! open/ramping executors, coordinated-omission correction, thresholds and `abortOn`,
//! graceful stop beyond the planned duration, and scripts or assertions under load.
//! Those are the next slices; they extend this crate rather than replace it.

#![forbid(unsafe_code)]

mod clock;
mod error;
mod exec;
mod metrics;
mod scenario;
mod sender;

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
use lev_metrics::{MetricsRecorder, RunId, RunOutcome, RunReport};

use crate::exec::FLUSH_INTERVAL;
use crate::metrics::{flush, new_histogram, WorkerMetrics};

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
        let seed = config.seed;
        handles.push(std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build a worker runtime");
            runtime.block_on(exec::run_vus(
                scenario, vars, secrets, sender, clock, metrics, base_index, count, deadline, seed,
            ));
        }));
        base_index += count;
    }

    // The aggregator: flush at 4 Hz until the planned duration elapses.
    let mut recorder = MetricsRecorder::new();
    let mut run_histogram = new_histogram()?;
    let mut max_scheduler_lag = Duration::ZERO;
    let mut next_flush = started_at + FLUSH_INTERVAL;
    loop {
        let now = config.clock.now();
        if now >= deadline {
            break;
        }
        let wake = next_flush.min(deadline);
        config.clock.sleep_until(wake).await;

        let elapsed = config.clock.now().saturating_duration_since(started_at);
        let scheduler_lag = config.clock.now().saturating_duration_since(next_flush);
        max_scheduler_lag = max_scheduler_lag.max(scheduler_lag);
        flush(&per_worker, &mut run_histogram, &mut recorder, scenario.vus)?;

        let snapshot = recorder.snapshot(elapsed, scheduler_lag);
        for reporter in reporters.iter_mut() {
            reporter.on_snapshot(&snapshot)?;
        }
        next_flush += FLUSH_INTERVAL;
    }

    // Workers stop themselves at the deadline; join so the final flush sees everything.
    for handle in handles {
        tokio::task::spawn_blocking(move || handle.join())
            .await
            .map_err(|error| LoadError::Internal(format!("worker join failed: {error}")))?
            .map_err(|_| LoadError::Internal("a worker thread panicked".to_string()))?;
    }

    let duration = config.clock.now().saturating_duration_since(started_at);
    flush(&per_worker, &mut run_histogram, &mut recorder, scenario.vus)?;

    // Build the final report from one last snapshot, so counters and latency are the
    // same numbers the last reporter saw.
    let final_snapshot = recorder.snapshot(duration, max_scheduler_lag);
    let report = RunReport {
        run_id: config.run_id,
        outcome: RunOutcome::Completed,
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
    };

    for reporter in reporters.iter_mut() {
        reporter.on_finish(&report)?;
    }
    Ok(report)
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
