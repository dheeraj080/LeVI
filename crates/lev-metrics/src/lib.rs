// SPDX-License-Identifier: MIT OR Apache-2.0
//! Levi metrics — **Phase 1 scaffold**.
//!
//! This crate fixes the *shape* of the metrics the UI will consume: the k6-compatible
//! metric names (§4.5), the run lifecycle states (§4.10), and [`MetricSnapshot`], the
//! aggregate the engine emits at 4 Hz.
//!
//! It is deliberately dependency-free and does **not** depend on `lev-core`: nothing
//! here is a domain type. The per-worker HDR histograms, the double-buffered flush,
//! and the aggregator land in Phase 3, at which point this crate grows into the real
//! pipeline. Until then it exists so the protocol and the UI can agree on names early.
//!
//! What is deliberately *not* here yet: histograms, percentiles computed from samples,
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
    /// `http_req_failed`: requests the target rejected at the transport level.
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

/// A placeholder aggregator.
///
/// Phase 1 keeps only counters and sequence numbering; Phase 3 replaces the latency
/// fields with per-worker HDR histograms merged at flush time. It is intentionally
/// single-threaded (`&mut self`): the per-worker-atomic design arrives with the
/// workers.
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
}

impl MetricsRecorder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record one dispatched request, and whether it failed at the transport level.
    pub fn record_request(&mut self, failed: bool) {
        self.http_reqs += 1;
        if failed {
            self.http_req_failed += 1;
        }
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
    }

    /// Produce the next snapshot, advancing the sequence number.
    ///
    /// Latency is left empty until the histogram lands in Phase 3.
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
            latency: LatencySummary::default(),
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
}
