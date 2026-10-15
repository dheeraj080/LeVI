// SPDX-License-Identifier: MIT OR Apache-2.0
//! Levi metrics: the *shape* of the numbers the UI and reports consume.
//!
//! This crate fixes the k6-compatible metric names (§4.5), the run lifecycle states
//! (§4.10), the 4 Hz [`MetricSnapshot`] the aggregator emits, and the final
//! [`RunReport`].
//!
//! It is deliberately dependency-free and does **not** depend on `lev-core`: nothing
//! here is a domain type, and nothing here serializes. The per-worker HDR histograms,
//! the double-buffered flush, the aggregator, and the reporters live in `lev-load`,
//! which fills these structs in and owns turning a [`RunReport`] into JSON. Keeping
//! this crate plain data means the CLI, the protocol, and any future UI can all agree
//! on names without pulling in a histogram.
//!
//! What is deliberately *not* here: histograms, percentiles computed from samples,
//! threshold evaluation, and reporters.

#![forbid(unsafe_code)]

use std::time::Duration;

/// Identifies a load run. Opaque to clients; used to correlate snapshots.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RunId(String);

impl RunId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for RunId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a run is in its lifecycle (§4.10).
///
/// `Persist` is a step within [`RunState::Finished`], not a state of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    Idle,
    Preparing,
    Running,
    Stopping,
    Aggregating,
    Finished,
}

impl RunState {
    /// Whether the state is terminal.
    pub fn is_terminal(self) -> bool {
        matches!(self, RunState::Finished)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RunState::Idle => "idle",
            RunState::Preparing => "preparing",
            RunState::Running => "running",
            RunState::Stopping => "stopping",
            RunState::Aggregating => "aggregating",
            RunState::Finished => "finished",
        }
    }
}

/// How a finished run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// The run reached its planned duration.
    Completed,
    /// The run was stopped early, by the user or by `abortOn`.
    Aborted,
    /// The run failed before it could complete: an engine or generator fault.
    Errored,
}

impl RunOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            RunOutcome::Completed => "completed",
            RunOutcome::Aborted => "aborted",
            RunOutcome::Errored => "errored",
        }
    }
}

/// Why a run stopped before its planned duration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StopReason {
    /// The user interrupted it (Ctrl-C).
    Interrupt,
    /// A threshold named in `abortOn` failed, so the run stopped itself.
    Threshold {
        /// The metric whose threshold failed.
        metric: String,
    },
}

impl StopReason {
    /// The name a report gives this reason.
    pub fn as_str(&self) -> &'static str {
        match self {
            StopReason::Interrupt => "interrupt",
            StopReason::Threshold { .. } => "threshold",
        }
    }
}

/// The unit a threshold's observed value is in.
///
/// The unit follows the *stat*, not the metric: a `p(95)` over a duration is
/// milliseconds, a `rate` is a fraction, and a `count` is a count. A report keeps the
/// unit next to the number so a consumer never has to guess — `0.01` and `10` are both
/// plausible-looking numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdUnit {
    Milliseconds,
    /// A `0..1` fraction.
    Rate,
    Count,
}

impl ThresholdUnit {
    pub fn as_str(self) -> &'static str {
        match self {
            ThresholdUnit::Milliseconds => "ms",
            ThresholdUnit::Rate => "rate",
            ThresholdUnit::Count => "count",
        }
    }
}

/// The verdict on one threshold, and the number that produced it.
#[derive(Debug, Clone, PartialEq)]
pub struct ThresholdResult {
    /// The metric the threshold is about (k6's name).
    pub metric: String,
    /// The expression as it is written in the scenario, canonical form.
    pub expression: String,
    /// Whether the run satisfied it.
    pub passed: bool,
    /// What the run actually measured, in [`unit`](Self::unit).
    ///
    /// `None` means "no samples": the run produced no data for this stat, so the
    /// threshold **fails** rather than passing by default. A threshold that cannot be
    /// evaluated must not read as verified.
    pub observed: Option<f64>,
    /// The unit of [`observed`](Self::observed).
    pub unit: ThresholdUnit,
}

impl ThresholdResult {
    /// Whether the threshold was evaluated at all.
    pub fn was_evaluated(&self) -> bool {
        self.observed.is_some()
    }
}

/// Percentiles of a latency distribution, in the units recorded.
///
/// `None` means "no samples yet" rather than "0", so a UI can distinguish an empty
/// run from an instantaneous one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LatencySummary {
    pub p50: Option<Duration>,
    pub p90: Option<Duration>,
    pub p95: Option<Duration>,
    pub p99: Option<Duration>,
}

impl LatencySummary {
    pub fn is_empty(&self) -> bool {
        self.p50.is_none() && self.p90.is_none() && self.p95.is_none() && self.p99.is_none()
    }
}

/// One aggregate sample of a run, emitted at ~4 Hz.
///
/// Snapshots carry **aggregates only** — raw samples never reach the UI. When the
/// aggregator falls behind, snapshots are dropped rather than queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetricSnapshot {
    /// Monotonic sequence number, so a client can detect drops.
    pub seq: u64,
    /// Time since the run started.
    pub elapsed: Duration,
    /// `http_reqs`: every request dispatched.
    pub http_reqs: u64,
    /// `http_req_failed`: requests that did not succeed — a transport-level failure
    /// (DNS, connect, TLS, reset) or a response with a 4xx/5xx status.
    pub http_req_failed: u64,
    /// `iterations`: completed scenario iterations.
    pub iterations: u64,
    /// `checks`: every check evaluated (pass or fail).
    pub checks: u64,
    /// `checks_failed`: checks that did not pass.
    pub checks_failed: u64,
    /// `script_errors`: script invocations that failed (compile, runtime, budget).
    pub script_errors: u64,
    /// `vus`: active virtual users at the sample point.
    pub vus: u32,
    /// `scheduler_lag`: how far the generator has fallen behind its schedule.
    ///
    /// Surfaced as a first-class metric because generator saturation is otherwise
    /// indistinguishable from target slowdown, and users blame the network.
    pub scheduler_lag: Duration,
    /// `http_req_duration`.
    pub latency: LatencySummary,
}

impl MetricSnapshot {
    /// An all-zero snapshot, useful as a `before the first flush` value.
    pub fn empty(seq: u64) -> Self {
        Self {
            seq,
            elapsed: Duration::ZERO,
            http_reqs: 0,
            http_req_failed: 0,
            iterations: 0,
            checks: 0,
            checks_failed: 0,
            script_errors: 0,
            vus: 0,
            scheduler_lag: Duration::ZERO,
            latency: LatencySummary::default(),
        }
    }
}

/// The final report for a finished run.
///
/// Plain data: no `serde` here, and no histogram — `lev-load` computes the percentiles
/// from the merged HDR and fills in [`LatencySummary`], then a reporter serializes it.
/// `started_at_unix_ms` is the *only* wall-clock value; everything measured uses a
/// monotonic clock, so a clock adjustment mid-run cannot corrupt the numbers.
#[derive(Debug, Clone, PartialEq)]
pub struct RunReport {
    pub run_id: RunId,
    pub outcome: RunOutcome,
    /// Why the run stopped early, if at all.
    pub stopped_by: Option<StopReason>,
    /// Unix milliseconds at which the run started (wall clock, for report timestamps).
    pub started_at_unix_ms: u64,
    /// The duration the executor planned to run for.
    pub planned_duration: Duration,
    /// The run's actual wall-clock duration.
    pub duration: Duration,
    /// The peak number of virtual users that were in flight.
    pub max_vus: u32,
    pub http_reqs: u64,
    pub http_req_failed: u64,
    pub iterations: u64,
    pub checks: u64,
    pub checks_failed: u64,
    pub script_errors: u64,
    pub scheduler_lag: Duration,
    pub latency: LatencySummary,
    /// The verdict on each configured threshold, in the order configured.
    pub thresholds: Vec<ThresholdResult>,
}

impl RunReport {
    /// Mean requests per second over the run.
    pub fn requests_per_second(&self) -> f64 {
        let seconds = self.duration.as_secs_f64();
        if seconds > 0.0 {
            self.http_reqs as f64 / seconds
        } else {
            0.0
        }
    }
}

/// The k6-compatible metric names we commit to (§4.5).
///
/// Using k6's names is a deliberate adoption move: existing Grafana dashboards and
/// mental models port over for free.
pub mod metric_names {
    pub const HTTP_REQS: &str = "http_reqs";
    pub const HTTP_REQ_DURATION: &str = "http_req_duration";
    pub const HTTP_REQ_WAITING: &str = "http_req_waiting";
    pub const HTTP_REQ_CONNECTING: &str = "http_req_connecting";
    pub const HTTP_REQ_TLS_HANDSHAKING: &str = "http_req_tls_handshaking";
    pub const HTTP_REQ_SENDING: &str = "http_req_sending";
    pub const HTTP_REQ_RECEIVING: &str = "http_req_receiving";
    pub const HTTP_REQ_BLOCKED: &str = "http_req_blocked";
    pub const HTTP_REQ_FAILED: &str = "http_req_failed";
    pub const ITERATIONS: &str = "iterations";
    /// Check outcomes (§4.7: "Checks/assertions -> metrics").
    pub const CHECKS: &str = "checks";
    pub const CHECKS_FAILED: &str = "checks_failed";
    pub const SCRIPT_ERRORS: &str = "script_errors";
    pub const VUS: &str = "vus";
    pub const SCHEDULER_LAG: &str = "scheduler_lag";
    pub const MAX_VUS_HIT: &str = "max_vus_hit";
}

/// The run-level aggregator.
///
/// The load engine's aggregator feeds it *deltas* every flush (per-worker counters are
/// reset by the swap), and sets the run-level [`LatencySummary`] computed from the
/// merged HDR. It stays single-threaded (`&mut self`): the per-worker atomics and
/// histograms live in `lev-load`, which owns the worker threads.
#[derive(Debug, Default)]
pub struct MetricsRecorder {
    seq: u64,
    http_reqs: u64,
    http_req_failed: u64,
    iterations: u64,
    checks: u64,
    checks_failed: u64,
    script_errors: u64,
    vus: u32,
    max_vus: u32,
    latency: LatencySummary,
}

impl MetricsRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one dispatched request, and whether it failed.
    pub fn record_request(&mut self, failed: bool) {
        self.record_requests(1, u64::from(failed));
    }

    /// Record a batch of dispatched requests and how many of them failed.
    ///
    /// The load engine aggregates per flush, so it never calls the singular form on
    /// the hot path.
    pub fn record_requests(&mut self, total: u64, failed: u64) {
        self.http_reqs += total;
        self.http_req_failed += failed.min(total);
    }

    /// Record completed iterations.
    pub fn record_iterations(&mut self, count: u64) {
        self.iterations += count;
    }

    /// Record a batch of check outcomes.
    pub fn record_checks(&mut self, passed: u64, failed: u64) {
        self.checks += passed + failed;
        self.checks_failed += failed;
    }

    /// Record a script invocation that failed (compile, runtime, or budget).
    pub fn record_script_error(&mut self) {
        self.script_errors += 1;
    }

    /// Set the active virtual-user count at the sample point.
    pub fn set_vus(&mut self, vus: u32) {
        self.vus = vus;
        self.max_vus = self.max_vus.max(vus);
    }

    /// Set the run-level latency summary, computed from the merged HDR.
    pub fn set_latency(&mut self, latency: LatencySummary) {
        self.latency = latency;
    }

    /// The peak active virtual-user count seen so far.
    pub fn max_vus(&self) -> u32 {
        self.max_vus
    }

    /// Produce the next snapshot, advancing the sequence number.
    pub fn snapshot(&mut self, elapsed: Duration, scheduler_lag: Duration) -> MetricSnapshot {
        self.seq += 1;
        MetricSnapshot {
            seq: self.seq,
            elapsed,
            http_reqs: self.http_reqs,
            http_req_failed: self.http_req_failed,
            iterations: self.iterations,
            checks: self.checks,
            checks_failed: self.checks_failed,
            script_errors: self.script_errors,
            vus: self.vus,
            scheduler_lag,
            latency: self.latency,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_accumulate_and_snapshots_are_sequenced() {
        let mut recorder = MetricsRecorder::new();
        recorder.record_request(false);
        recorder.record_request(true);
        recorder.record_iterations(3);
        recorder.set_vus(50);

        let first = recorder.snapshot(Duration::from_secs(1), Duration::ZERO);
        assert_eq!(first.seq, 1);
        assert_eq!(first.http_reqs, 2);
        assert_eq!(first.http_req_failed, 1);
        assert_eq!(first.iterations, 3);
        assert_eq!(first.vus, 50);
        assert!(first.latency.is_empty());

        let second = recorder.snapshot(Duration::from_secs(2), Duration::ZERO);
        assert_eq!(second.seq, 2);
        // Counters are cumulative, not per-interval.
        assert_eq!(second.http_reqs, 2);
    }

    #[test]
    fn an_empty_snapshot_reports_no_latency() {
        let snapshot = MetricSnapshot::empty(0);
        assert!(snapshot.latency.is_empty());
        assert_eq!(snapshot.http_reqs, 0);
        assert_eq!(snapshot.elapsed, Duration::ZERO);
    }

    #[test]
    fn checks_and_script_errors_are_counted() {
        let mut recorder = MetricsRecorder::new();
        recorder.record_checks(3, 1);
        recorder.record_checks(2, 0);
        recorder.record_script_error();
        let snapshot = recorder.snapshot(Duration::from_secs(1), Duration::ZERO);
        assert_eq!(snapshot.checks, 6);
        assert_eq!(snapshot.checks_failed, 1);
        assert_eq!(snapshot.script_errors, 1);
    }

    #[test]
    fn only_finished_is_terminal() {
        assert!(RunState::Finished.is_terminal());
        for state in [
            RunState::Idle,
            RunState::Preparing,
            RunState::Running,
            RunState::Stopping,
            RunState::Aggregating,
        ] {
            assert!(!state.is_terminal(), "{state:?}");
        }
    }

    #[test]
    fn metric_names_are_k6_compatible() {
        assert_eq!(metric_names::HTTP_REQS, "http_reqs");
        assert_eq!(metric_names::HTTP_REQ_WAITING, "http_req_waiting");
        assert_eq!(
            metric_names::HTTP_REQ_TLS_HANDSHAKING,
            "http_req_tls_handshaking"
        );
        assert_eq!(metric_names::CHECKS, "checks");
        assert_eq!(metric_names::CHECKS_FAILED, "checks_failed");
        assert_eq!(metric_names::SCRIPT_ERRORS, "script_errors");
        assert_eq!(metric_names::VUS, "vus");
    }

    #[test]
    fn batched_counters_and_latency_reach_the_snapshot() {
        let mut recorder = MetricsRecorder::new();
        recorder.record_requests(1000, 7);
        recorder.record_iterations(250);
        recorder.set_vus(10);
        recorder.set_vus(3);
        recorder.set_latency(LatencySummary {
            p50: Some(Duration::from_millis(12)),
            p90: None,
            p95: None,
            p99: Some(Duration::from_millis(80)),
        });

        let snapshot = recorder.snapshot(Duration::from_secs(1), Duration::ZERO);
        assert_eq!(snapshot.http_reqs, 1000);
        assert_eq!(snapshot.http_req_failed, 7);
        assert_eq!(snapshot.iterations, 250);
        // `vus` is the *active* count at the sample point; `max_vus` remembers the peak.
        assert_eq!(snapshot.vus, 3);
        assert_eq!(recorder.max_vus(), 10);
        assert_eq!(snapshot.latency.p50, Some(Duration::from_millis(12)));
        assert_eq!(snapshot.latency.p99, Some(Duration::from_millis(80)));

        // A later snapshot carries the same cumulative latency and counters.
        let next = recorder.snapshot(Duration::from_secs(2), Duration::ZERO);
        assert_eq!(next.http_reqs, 1000);
        assert_eq!(next.latency.p50, Some(Duration::from_millis(12)));
    }

    #[test]
    fn failed_requests_never_exceed_the_total() {
        let mut recorder = MetricsRecorder::new();
        // Defensive: a bad delta cannot make failures outnumber requests.
        recorder.record_requests(3, 5);
        let snapshot = recorder.snapshot(Duration::from_secs(1), Duration::ZERO);
        assert_eq!(snapshot.http_reqs, 3);
        assert_eq!(snapshot.http_req_failed, 3);
    }

    #[test]
    fn a_run_report_computes_requests_per_second() {
        let report = RunReport {
            run_id: RunId::new("r1"),
            outcome: RunOutcome::Completed,
            stopped_by: None,
            started_at_unix_ms: 0,
            planned_duration: Duration::from_secs(10),
            duration: Duration::from_secs(10),
            max_vus: 10,
            http_reqs: 5000,
            http_req_failed: 1,
            iterations: 500,
            checks: 0,
            checks_failed: 0,
            script_errors: 0,
            scheduler_lag: Duration::from_millis(2),
            latency: LatencySummary::default(),
            thresholds: Vec::new(),
        };
        assert!((report.requests_per_second() - 500.0).abs() < f64::EPSILON);
        assert_eq!(RunOutcome::Completed.as_str(), "completed");
    }
}
