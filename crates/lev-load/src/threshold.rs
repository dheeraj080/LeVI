// SPDX-License-Identifier: MIT OR Apache-2.0
//! Turning thresholds into a verdict: the numbers in, a pass/fail out.
//!
//! Evaluation is a pure function of the run's **cumulative** numbers, so the same code
//! answers both questions this slice asks:
//!
//! - *live*, at every 4 Hz flush, so `abortOn` can stop a run that is already failing
//!   rather than letting it hammer a broken target for the full duration; and
//! - *finally*, once the workers have stopped, which is the verdict the report carries
//!   and the CLI exits on.
//!
//! Cumulative rather than per-interval is deliberate: a threshold is a statement about
//! the run, and a per-interval rule would fail a run because one 250 ms window was slow.
//! It also means a live verdict can flip back to passing, which is why `abortOn` cannot
//! use the last-seen verdict — see [`abort_trigger`].
//!
//! The domain types live in `lev-core`, the verdict shape in `lev-metrics`, and the
//! percentile math here, because only `lev-load` owns the merged histogram.

use hdrhistogram::Histogram;
use lev_core::{Threshold, ThresholdMetric, ThresholdStat};
use lev_metrics::{MetricSnapshot, ThresholdResult, ThresholdUnit};

/// Evaluate every threshold against the run so far.
///
/// The results come back in the same order as `thresholds`, and are never empty for a
/// threshold that is: a stat with no samples fails rather than passing by default, with
/// `observed: None` to say so.
pub(crate) fn evaluate_thresholds(
    thresholds: &[Threshold],
    snapshot: &MetricSnapshot,
    histogram: &Histogram<u64>,
) -> Vec<ThresholdResult> {
    thresholds
        .iter()
        .map(|threshold| evaluate_one(threshold, snapshot, histogram))
        .collect()
}

/// Which threshold — if any — should abort the run right now.
///
/// Returns the metric named in `abortOn` whose threshold failed, or `None`. Only metrics
/// the scenario actually aborts on can trigger; a plain failing threshold is reported at
/// the end, never fatal mid-run.
pub(crate) fn abort_trigger(
    thresholds: &[Threshold],
    abort_on: &[ThresholdMetric],
    results: &[ThresholdResult],
) -> Option<ThresholdMetric> {
    thresholds
        .iter()
        .zip(results)
        .find(|(threshold, result)| {
            !result.passed && abort_on.contains(&threshold.metric)
        })
        .map(|(threshold, _)| threshold.metric)
}

fn evaluate_one(
    threshold: &Threshold,
    snapshot: &MetricSnapshot,
    histogram: &Histogram<u64>,
) -> ThresholdResult {
    let (observed, unit) = observe(threshold, snapshot, histogram);
    let passed = observed.is_some_and(|value| threshold.comparison.holds(value, threshold.value));

    ThresholdResult {
        metric: threshold.metric.name().to_string(),
        expression: threshold.expression(),
        passed,
        observed,
        unit,
    }
}

/// The number a threshold is compared against, and its unit.
///
/// `None` means the run produced no samples for this stat — an empty histogram has no
/// average, no minimum and no percentile — so the comparison cannot be made.
fn observe(
    threshold: &Threshold,
    snapshot: &MetricSnapshot,
    histogram: &Histogram<u64>,
) -> (Option<f64>, ThresholdUnit) {
    match threshold.stat {
        // `count` is the only stat every metric can answer.
        ThresholdStat::Count => {
            let count = match threshold.metric {
                ThresholdMetric::HttpReqDuration => histogram.len(),
                ThresholdMetric::HttpReqs => snapshot.http_reqs,
                ThresholdMetric::HttpReqFailed => snapshot.http_req_failed,
                ThresholdMetric::Iterations => snapshot.iterations,
                ThresholdMetric::Checks => snapshot.checks,
                ThresholdMetric::ChecksFailed => snapshot.checks_failed,
                ThresholdMetric::ScriptErrors => snapshot.script_errors,
            };
            (Some(count as f64), ThresholdUnit::Count)
        }
        ThresholdStat::Rate => {
            // `lev-core` only accepts `rate` on `http_req_failed`, where the rate is the
            // fraction of dispatched requests that did not succeed.
            let rate = if snapshot.http_reqs > 0 {
                Some(snapshot.http_req_failed as f64 / snapshot.http_reqs as f64)
            } else {
                None
            };
            (rate, ThresholdUnit::Rate)
        }
        ThresholdStat::Avg | ThresholdStat::Min | ThresholdStat::Max => {
            if histogram.is_empty() {
                return (None, ThresholdUnit::Milliseconds);
            }
            // `mean` is computed from bucket midpoints (three significant figures), so
            // an average read this way is accurate to ~0.1%, not to the microsecond.
            // That is inherent to keeping one histogram per worker instead of every
            // sample, and it is far tighter than the run's own jitter.
            let micros = match threshold.stat {
                ThresholdStat::Avg => histogram.mean(),
                ThresholdStat::Min => histogram.min() as f64,
                ThresholdStat::Max => histogram.max() as f64,
                _ => unreachable!("matched above"),
            };
            (Some(micros / 1_000.0), ThresholdUnit::Milliseconds)
        }
        ThresholdStat::Percentile(percentile) => {
            if histogram.is_empty() {
                return (None, ThresholdUnit::Milliseconds);
            }
            let micros = histogram.value_at_quantile((percentile / 100.0).clamp(0.0, 1.0));
            (Some(micros as f64 / 1_000.0), ThresholdUnit::Milliseconds)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lev_metrics::MetricsRecorder;

    /// A histogram holding one sample per `samples` entry, in microseconds.
    fn histogram_of(samples_micros: &[u64]) -> Histogram<u64> {
        let mut histogram = Histogram::new_with_max(3_600_000_000, 3).unwrap();
        for micros in samples_micros {
            histogram.record(*micros).unwrap();
        }
        histogram
    }

    /// Cumulative counters and latency, as the aggregator would report them.
    fn snapshot(http_reqs: u64, failed: u64, iterations: u64) -> MetricSnapshot {
        let mut recorder = MetricsRecorder::new();
        recorder.record_requests(http_reqs, failed);
        recorder.record_iterations(iterations);
        recorder.snapshot(std::time::Duration::from_secs(1), std::time::Duration::ZERO)
    }

    fn evaluate(threshold: &str, metric: &str, snapshot: &MetricSnapshot, histogram: &Histogram<u64>) -> ThresholdResult {
        let parsed = Threshold::parse(metric, threshold).unwrap();
        evaluate_thresholds(&[parsed], snapshot, histogram)
            .into_iter()
            .next()
            .unwrap()
    }

    #[test]
    fn a_latency_percentile_passes_when_it_is_under_the_bound() {
        // 100 samples from 1ms to 100ms; p(95) is ~95ms.
        let samples: Vec<u64> = (1..=100).map(|n| n * 1_000).collect();
        let histogram = histogram_of(&samples);
        let counters = snapshot(100, 0, 10);

        let passed = evaluate("p(95)<200", "http_req_duration", &counters, &histogram);
        assert!(passed.passed, "{passed:?}");
        assert_eq!(passed.unit, ThresholdUnit::Milliseconds);
        let observed = passed.observed.unwrap();
        assert!((90.0..=100.0).contains(&observed), "p95 was {observed}");

        let failed = evaluate("p(95)<50", "http_req_duration", &counters, &histogram);
        assert!(!failed.passed);
        assert_eq!(failed.observed.unwrap(), observed);
    }

    #[test]
    fn the_average_min_and_max_read_the_histogram() {
        let histogram = histogram_of(&[10_000, 20_000, 30_000]);
        let counters = snapshot(3, 0, 1);

        // The arithmetic mean is 20ms, but an HDR mean is computed from bucket midpoints
        // (three significant figures), so it lands a hair off. The bound is set with
        // that tolerance in mind; what matters is that `avg` reads the histogram and the
        // comparison is the one written.
        let avg = evaluate("avg<=21", "http_req_duration", &counters, &histogram);
        assert!(avg.passed, "{avg:?}");
        assert!((19.0..=21.0).contains(&avg.observed.unwrap()), "{avg:?}");

        let too_tight = evaluate("avg<19", "http_req_duration", &counters, &histogram);
        assert!(!too_tight.passed, "a mean of 20ms must not pass avg<19");

        let min = evaluate("min>=10", "http_req_duration", &counters, &histogram);
        assert!(min.passed);
        assert!((9.9..=10.1).contains(&min.observed.unwrap()), "{min:?}");

        let max = evaluate("max<100", "http_req_duration", &counters, &histogram);
        assert!(max.passed);
        assert!((29.9..=30.1).contains(&max.observed.unwrap()), "{max:?}");
    }

    #[test]
    fn a_count_threshold_reads_the_counters() {
        let histogram = histogram_of(&[1_000, 2_000]);
        let counters = snapshot(500, 4, 50);

        let requests = evaluate("count>=100", "http_reqs", &counters, &histogram);
        assert!(requests.passed);
        assert_eq!(requests.observed, Some(500.0));
        assert_eq!(requests.unit, ThresholdUnit::Count);

        let iterations = evaluate("count>100", "iterations", &counters, &histogram);
        assert!(!iterations.passed);
        assert_eq!(iterations.observed, Some(50.0));

        // On a duration metric, `count` is the number of *samples*.
        let samples = evaluate("count==2", "http_req_duration", &counters, &histogram);
        assert!(samples.passed, "{samples:?}");
        assert_eq!(samples.observed, Some(2.0));
    }

    #[test]
    fn a_rate_threshold_reads_the_failure_fraction() {
        let histogram = histogram_of(&[1_000]);
        let counters = snapshot(100, 1, 10);

        // 1 failure in 100 requests is 1%.
        let strict = evaluate("rate<0.01", "http_req_failed", &counters, &histogram);
        assert!(!strict.passed);
        assert_eq!(strict.observed, Some(0.01));
        assert_eq!(strict.unit, ThresholdUnit::Rate);

        let loose = evaluate("rate<0.02", "http_req_failed", &counters, &histogram);
        assert!(loose.passed);
    }

    #[test]
    fn a_run_with_no_samples_fails_rather_than_passing() {
        let empty = Histogram::new_with_max(3_600_000_000, 3).unwrap();
        let counters = snapshot(0, 0, 0);

        // A percentile of nothing cannot be verified, so it must not read as "passed".
        let percentile = evaluate("p(95)<500", "http_req_duration", &counters, &empty);
        assert!(!percentile.passed);
        assert_eq!(percentile.observed, None);
        assert!(!percentile.was_evaluated());

        // A rate with no requests to divide by is the same story.
        let rate = evaluate("rate<0.01", "http_req_failed", &counters, &empty);
        assert!(!rate.passed);
        assert_eq!(rate.observed, None);

        // A count is always answerable, and zero counts pass a sane bound.
        let count = evaluate("count==0", "http_reqs", &counters, &empty);
        assert!(count.passed);
        assert_eq!(count.observed, Some(0.0));
    }

    #[test]
    fn results_carry_the_expression_and_metric_for_the_report() {
        let histogram = histogram_of(&[1_000]);
        let counters = snapshot(1, 0, 1);
        let result = evaluate("p(99)<250", "http_req_duration", &counters, &histogram);
        assert_eq!(result.metric, "http_req_duration");
        assert_eq!(result.expression, "p(99)<250");
    }

    #[test]
    fn evaluation_keeps_the_configured_order() {
        let histogram = histogram_of(&[1_000, 5_000]);
        let counters = snapshot(2, 1, 2);
        let thresholds = vec![
            Threshold::parse("http_req_failed", "rate<0.01").unwrap(),
            Threshold::parse("http_req_duration", "p(50)<100").unwrap(),
        ];

        let results = evaluate_thresholds(&thresholds, &counters, &histogram);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].metric, "http_req_failed");
        assert_eq!(results[1].metric, "http_req_duration");
        // 1 of 2 requests failed: 50% > 1%.
        assert!(!results[0].passed);
        assert!(results[1].passed);
    }

    #[test]
    fn only_an_abort_on_metric_that_failed_triggers_an_abort() {
        let histogram = histogram_of(&[1_000, 5_000]);
        let counters = snapshot(2, 2, 2);
        let thresholds = vec![
            Threshold::parse("http_req_failed", "rate<0.01").unwrap(),
            Threshold::parse("http_req_duration", "p(50)<100").unwrap(),
        ];
        let results = evaluate_thresholds(&thresholds, &counters, &histogram);
        assert!(!results[0].passed);
        assert!(results[1].passed);

        // Not named in `abortOn`: a failure, but not a reason to stop the run.
        assert_eq!(abort_trigger(&thresholds, &[], &results), None);
        assert_eq!(
            abort_trigger(&thresholds, &[ThresholdMetric::HttpReqs], &results),
            None
        );
        // Named: the run stops on `http_req_failed`.
        assert_eq!(
            abort_trigger(&thresholds, &[ThresholdMetric::HttpReqFailed], &results),
            Some(ThresholdMetric::HttpReqFailed)
        );

        // A passing threshold never aborts, even when its metric is listed.
        assert_eq!(
            abort_trigger(&thresholds, &[ThresholdMetric::HttpReqDuration], &results),
            None
        );
    }
}
