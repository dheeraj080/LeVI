// SPDX-License-Identifier: MIT OR Apache-2.0
//! Reporters: where a run's numbers go.
//!
//! The load engine emits snapshots at ~4 Hz and one final [`RunReport`]. A reporter is
//! the sink for both. Slice 1 ships [`JsonReporter`]; the plan's CSV / JUnit / HTML /
//! Prometheus reporters are later slices and are just more implementations of the same
//! trait.

use std::path::PathBuf;
use std::time::Duration;

use lev_metrics::{MetricSnapshot, RunReport, StopReason, ThresholdResult};
use serde_json::{json, Value};

use crate::error::LoadError;

/// A sink for run progress and the final report.
///
/// `on_snapshot` is called during the run; `on_finish` exactly once, after the last
/// flush. Both may fail, and a reporter failure aborts the run.
pub trait Reporter: Send {
    fn on_snapshot(&mut self, snapshot: &MetricSnapshot) -> Result<(), LoadError>;
    fn on_finish(&mut self, report: &RunReport) -> Result<(), LoadError>;
}

/// Writes the final [`RunReport`] to a JSON file.
///
/// Slice 1 writes the summary report only; a per-snapshot timeseries sidecar is a
/// later slice, which is why `on_snapshot` does nothing here.
pub struct JsonReporter {
    path: PathBuf,
}

impl JsonReporter {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The file this reporter writes.
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Reporter for JsonReporter {
    fn on_snapshot(&mut self, _snapshot: &MetricSnapshot) -> Result<(), LoadError> {
        Ok(())
    }

    fn on_finish(&mut self, report: &RunReport) -> Result<(), LoadError> {
        let text = serde_json::to_string_pretty(&report_to_json(report))
            .map_err(|error| LoadError::Report(error.to_string()))?;
        std::fs::write(&self.path, text).map_err(|error| {
            LoadError::Report(format!(
                "failed to write `{}`: {error}",
                self.path.display()
            ))
        })
    }
}

/// The on-disk shape of a report.
///
/// Hand-built rather than derived, so `lev-metrics` stays free of `serde` and the JSON
/// can use k6's metric names for the latency block.
pub fn report_to_json(report: &RunReport) -> Value {
    let stopped_by = report.stopped_by.as_ref().map(|reason| match reason {
        StopReason::Interrupt => json!({"kind": "interrupt"}),
        StopReason::Threshold { metric } => json!({
            "kind": "threshold",
            "metric": metric,
        }),
    });

    let thresholds: Vec<Value> = report
        .thresholds
        .iter()
        .map(threshold_result_to_json)
        .collect();

    json!({
        "runId": report.run_id.as_str(),
        "outcome": report.outcome.as_str(),
        "stoppedBy": stopped_by,
        "startedAtUnixMs": report.started_at_unix_ms,
        "plannedDurationMs": millis(report.planned_duration),
        "durationMs": millis(report.duration),
        "maxVus": report.max_vus,
        "requestsPerSecond": report.requests_per_second(),
        "metrics": {
            "http_reqs": report.http_reqs,
            "http_req_failed": report.http_req_failed,
            "iterations": report.iterations,
            "checks": report.checks,
            "checks_failed": report.checks_failed,
            "script_errors": report.script_errors,
            "scheduler_lag_ms": millis(report.scheduler_lag),
            "http_req_duration": {
                "p50": opt_millis(report.latency.p50),
                "p90": opt_millis(report.latency.p90),
                "p95": opt_millis(report.latency.p95),
                "p99": opt_millis(report.latency.p99),
            }
        },
        "thresholds": thresholds
    })
}

fn threshold_result_to_json(result: &ThresholdResult) -> Value {
    json!({
        "metric": result.metric,
        "expression": result.expression,
        "passed": result.passed,
        "observed": result.observed,
        "unit": result.unit.as_str(),
    })
}

fn millis(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

fn opt_millis(duration: Option<Duration>) -> Option<f64> {
    duration.map(millis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lev_metrics::{LatencySummary, RunId, RunOutcome};

    fn sample_report() -> RunReport {
        RunReport {
            run_id: RunId::new("run-1"),
            outcome: RunOutcome::Completed,
            stopped_by: None,
            started_at_unix_ms: 1_700_000_000_000,
            planned_duration: Duration::from_secs(10),
            duration: Duration::from_secs(10),
            max_vus: 8,
            http_reqs: 4000,
            http_req_failed: 3,
            iterations: 4000,
            checks: 0,
            checks_failed: 0,
            script_errors: 0,
            scheduler_lag: Duration::from_millis(1),
            latency: LatencySummary {
                p50: Some(Duration::from_micros(1_500)),
                p90: Some(Duration::from_micros(3_000)),
                p95: None,
                p99: Some(Duration::from_millis(9)),
            },
            thresholds: Vec::new(),
        }
    }

    #[test]
    fn the_json_report_uses_k6_metric_names() {
        let value = report_to_json(&sample_report());
        assert_eq!(value["metrics"]["http_reqs"], 4000);
        assert_eq!(value["metrics"]["http_req_duration"]["p50"], 1.5);
        assert_eq!(value["metrics"]["http_req_duration"]["p99"], 9.0);
        // A missing percentile is `null`, not zero.
        assert!(value["metrics"]["http_req_duration"]["p95"].is_null());
        assert_eq!(value["outcome"], "completed");
        assert_eq!(value["requestsPerSecond"], 400.0);
        assert_eq!(value["metrics"]["http_req_failed"], 3);
        assert!(value["thresholds"].is_array());
        assert!(value["thresholds"].as_array().unwrap().is_empty());
    }

    #[test]
    fn the_json_reporter_writes_the_file() {
        let dir = std::env::temp_dir().join(format!("levi-report-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("report.json");

        let mut reporter = JsonReporter::new(&path);
        reporter.on_finish(&sample_report()).unwrap();

        let text = std::fs::read_to_string(&path).unwrap();
        let value: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["metrics"]["iterations"], 4000);

        std::fs::remove_dir_all(&dir).ok();
    }
}
