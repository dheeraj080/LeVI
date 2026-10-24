// SPDX-License-Identifier: MIT OR Apache-2.0
//! The per-worker metrics, and their merge into the run-level aggregate.
//!
//! Each worker owns a [`WorkerMetrics`]: relaxed atomics for the counters and a mutex
//! around a live HDR histogram. The mutex is uncontended in practice — only the
//! aggregator ever locks it, and the worker holds it for the length of a single
//! record — which is what the plan's "~15 ns" flush cost refers to.
//!
//! At every flush the aggregator *swaps* each worker's histogram for a fresh one and
//! merges the drained one into the run-level histogram. Swapping rather than
//! subtracting avoids the cost of a second histogram and, more importantly, means the
//! aggregator never blocks a worker for longer than the swap.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use hdrhistogram::Histogram;
use lev_metrics::{LatencySummary, MetricsRecorder};

use crate::error::LoadError;

/// Recorded latencies are microseconds, up to one hour — the plan's range.
pub const MAX_LATENCY_MICROS: u64 = 3_600_000_000;
/// Three significant figures: enough for percentiles, cheap to merge.
pub const SIGNIFICANT_FIGURES: u8 = 3;

/// A fresh histogram with the run's fixed configuration.
pub(crate) fn new_histogram() -> Result<Histogram<u64>, LoadError> {
    Histogram::new_with_max(MAX_LATENCY_MICROS, SIGNIFICANT_FIGURES)
        .map_err(|error| LoadError::Internal(format!("latency histogram config: {error}")))
}

/// One worker's counters and live latency histogram.
///
/// Shared as an `Arc` by every VU on the worker, so it is `Send + Sync`.
#[derive(Debug)]
pub(crate) struct WorkerMetrics {
    http_reqs: AtomicU64,
    http_req_failed: AtomicU64,
    iterations: AtomicU64,
    checks: AtomicU64,
    checks_failed: AtomicU64,
    script_errors: AtomicU64,
    latency: Mutex<Histogram<u64>>,
}

impl WorkerMetrics {
    pub(crate) fn new() -> Self {
        Self {
            http_reqs: AtomicU64::new(0),
            http_req_failed: AtomicU64::new(0),
            iterations: AtomicU64::new(0),
            checks: AtomicU64::new(0),
            checks_failed: AtomicU64::new(0),
            script_errors: AtomicU64::new(0),
            // The configuration is a compile-time constant; a failure here is a bug.
            latency: Mutex::new(new_histogram().expect("the latency histogram config is valid")),
        }
    }

    /// Record one dispatched request, and whether it failed.
    pub(crate) fn record_request(&self, failed: bool) {
        self.http_reqs.fetch_add(1, Ordering::Relaxed);
        if failed {
            self.http_req_failed.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Record one completed iteration.
    pub(crate) fn record_iteration(&self) {
        self.iterations.fetch_add(1, Ordering::Relaxed);
    }

    /// Record check outcomes.
    pub(crate) fn record_checks(&self, passed: u64, failed: u64) {
        self.checks.fetch_add(passed + failed, Ordering::Relaxed);
        self.checks_failed.fetch_add(failed, Ordering::Relaxed);
    }

    /// Record a script error.
    pub(crate) fn record_script_error(&self) {
        self.script_errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Record one request's latency, in microseconds.
    pub(crate) fn record_latency(&self, micros: u64) {
        if let Ok(mut histogram) = self.latency.lock() {
            histogram.saturating_record(micros);
        }
    }

    /// Read and reset the counters. The deltas are what the aggregator wants.
    pub(crate) fn take_counters(&self) -> Counters {
        Counters {
            http_reqs: self.http_reqs.swap(0, Ordering::Relaxed),
            http_req_failed: self.http_req_failed.swap(0, Ordering::Relaxed),
            iterations: self.iterations.swap(0, Ordering::Relaxed),
            checks: self.checks.swap(0, Ordering::Relaxed),
            checks_failed: self.checks_failed.swap(0, Ordering::Relaxed),
            script_errors: self.script_errors.swap(0, Ordering::Relaxed),
        }
    }

    /// Swap the live histogram for a fresh one, returning the drained delta.
    ///
    /// `None` when the worker recorded nothing since the last flush, so the aggregator
    /// can skip an empty merge.
    fn take_latency(&self) -> Result<Option<Histogram<u64>>, LoadError> {
        let mut live = self
            .latency
            .lock()
            .map_err(|_| LoadError::Internal("the latency mutex was poisoned".to_string()))?;
        if live.is_empty() {
            return Ok(None);
        }
        let drained = std::mem::replace(&mut *live, new_histogram()?);
        Ok(Some(drained))
    }
}

/// A snapshot of one worker's counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Counters {
    pub(crate) http_reqs: u64,
    pub(crate) http_req_failed: u64,
    pub(crate) iterations: u64,
    pub(crate) checks: u64,
    pub(crate) checks_failed: u64,
    pub(crate) script_errors: u64,
}

/// Drain every worker into the run-level histogram and recorder.
///
/// Called by the aggregator at ~4 Hz and once more at the end, so the final report
/// includes everything the workers recorded.
pub(crate) fn flush(
    workers: &[Arc<WorkerMetrics>],
    run_histogram: &mut Histogram<u64>,
    recorder: &mut MetricsRecorder,
    active_vus: u32,
) -> Result<(), LoadError> {
    let mut total = Counters::default();
    for worker in workers {
        let delta = worker.take_counters();
        total.http_reqs += delta.http_reqs;
        total.http_req_failed += delta.http_req_failed;
        total.iterations += delta.iterations;
        total.checks += delta.checks;
        total.checks_failed += delta.checks_failed;
        total.script_errors += delta.script_errors;

        if let Some(delta) = worker.take_latency()? {
            run_histogram.add(&delta).map_err(|error| {
                LoadError::Internal(format!("failed to merge latency histograms: {error}"))
            })?;
        }
    }

    recorder.record_requests(total.http_reqs, total.http_req_failed);
    recorder.record_iterations(total.iterations);
    if total.checks > 0 || total.checks_failed > 0 {
        recorder.record_checks(total.checks.saturating_sub(total.checks_failed), total.checks_failed);
    }
    for _ in 0..total.script_errors {
        recorder.record_script_error();
    }
    recorder.set_vus(active_vus);
    recorder.set_latency(latency_summary(run_histogram));
    Ok(())
}

/// The percentiles of a run-level histogram.
pub(crate) fn latency_summary(histogram: &Histogram<u64>) -> LatencySummary {
    if histogram.is_empty() {
        return LatencySummary::default();
    }
    let at = |quantile: f64| Some(Duration::from_micros(histogram.value_at_quantile(quantile)));
    LatencySummary {
        p50: at(0.50),
        p90: at(0.90),
        p95: at(0.95),
        p99: at(0.99),
    }
}
