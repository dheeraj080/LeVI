// SPDX-License-Identifier: MIT OR Apache-2.0
//! Thresholds: the rules a run is judged by.
//!
//! A [`Threshold`] is one `<stat> <op> <value>` rule over one of the metrics the load
//! engine produces — `p(95)<500` on `http_req_duration`, `rate<0.01` on
//! `http_req_failed`. The expression syntax is **k6's**, deliberately: the metric names
//! are already k6's (§4.5), and the people who write load tests have these expressions
//! in their fingers.
//!
//! The parser lives here rather than in `lev-format` because it is pure domain logic
//! with no document in sight — the same reason [`Template`](crate::Template) parses in
//! `lev-core` while the YAML that carries it lives in `lev-format`. `lev-format` owns
//! the `thresholds:` mapping and calls [`Threshold::parse`] once per expression, so a
//! typo fails while *reading the file*, not at 10,000 requests per second.
//!
//! # Units
//!
//! A threshold's value is in the metric's own unit: **milliseconds** for a duration
//! stat, a **fraction** (`0..1`) for `rate`, and a **count** for `count`. So `500` in
//! `p(95)<500` means half a second, and `0.01` in `rate<0.01` means one percent — the
//! same reading k6 gives them.
//!
//! # What each metric supports
//!
//! | Metric | Stats |
//! |---|---|
//! | `http_req_duration` | `count`, `avg`, `min`, `max`, `p(N)` |
//! | `http_req_failed` | `count`, `rate` |
//! | `http_reqs`, `iterations`, `checks`, `checks_failed`, `script_errors` | `count` |
//!
//! Everything else is rejected at parse time. A threshold that cannot mean what it says
//! is worse than no threshold: `rate` over a latency trend, or `p(95)` over a counter,
//! would silently compare nonsense.

use std::fmt;

use crate::error::CoreError;

/// Which metric a threshold is about.
///
/// The names are the k6 metric names the rest of the engine commits to; the load engine
/// tags its numbers with the same strings. This enum is the domain's own copy —
/// `lev-core` sits below `lev-metrics` in the crate graph and cannot borrow it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThresholdMetric {
    HttpReqDuration,
    HttpReqs,
    HttpReqFailed,
    Iterations,
    Checks,
    ChecksFailed,
    ScriptErrors,
}

impl ThresholdMetric {
    /// Every metric a threshold may name.
    pub const ALL: [ThresholdMetric; 7] = [
        ThresholdMetric::HttpReqDuration,
        ThresholdMetric::HttpReqs,
        ThresholdMetric::HttpReqFailed,
        ThresholdMetric::Iterations,
        ThresholdMetric::Checks,
        ThresholdMetric::ChecksFailed,
        ThresholdMetric::ScriptErrors,
    ];

    /// The metric's name, as it is written in a scenario.
    pub fn name(self) -> &'static str {
        match self {
            ThresholdMetric::HttpReqDuration => "http_req_duration",
            ThresholdMetric::HttpReqs => "http_reqs",
            ThresholdMetric::HttpReqFailed => "http_req_failed",
            ThresholdMetric::Iterations => "iterations",
            ThresholdMetric::Checks => "checks",
            ThresholdMetric::ChecksFailed => "checks_failed",
            ThresholdMetric::ScriptErrors => "script_errors",
        }
    }

    /// Resolve a metric name; `None` if it names no metric we produce.
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|metric| metric.name() == name)
    }

    /// Whether the metric is a duration (so `avg` / `min` / `max` / `p(N)` apply).
    pub fn is_duration(self) -> bool {
        matches!(self, ThresholdMetric::HttpReqDuration)
    }

    /// Whether `rate` applies: only `http_req_failed`, where it is `failed / total`.
    pub fn allows_rate(self) -> bool {
        matches!(self, ThresholdMetric::HttpReqFailed)
    }

    /// The stats this metric accepts, for an error message that tells the author what
    /// they *could* have written.
    pub fn allowed_stats(self) -> &'static str {
        if self.is_duration() {
            "count, avg, min, max, p(<percentile>)"
        } else if self.allows_rate() {
            "count, rate"
        } else {
            "count"
        }
    }
}

impl fmt::Display for ThresholdMetric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// How a threshold reduces the metric's samples to one number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThresholdStat {
    /// How many samples/events the metric saw.
    Count,
    /// The fraction of requests that failed. `http_req_failed` only.
    Rate,
    /// The mean. Duration metrics only.
    Avg,
    /// The smallest sample. Duration metrics only.
    Min,
    /// The largest sample. Duration metrics only.
    Max,
    /// A percentile, in `0 < N <= 100` (`p(95)`, `p(99.9)`). Duration metrics only.
    Percentile(f64),
}

/// The comparison in a threshold expression.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
    Ne,
}

impl Comparison {
    /// Compare an observed value against a threshold value.
    pub fn holds(self, observed: f64, value: f64) -> bool {
        match self {
            Comparison::Lt => observed < value,
            Comparison::Le => observed <= value,
            Comparison::Gt => observed > value,
            Comparison::Ge => observed >= value,
            Comparison::Eq => observed == value,
            Comparison::Ne => observed != value,
        }
    }

    fn parse(text: &str) -> Option<Self> {
        match text {
            "<" => Some(Comparison::Lt),
            "<=" => Some(Comparison::Le),
            ">" => Some(Comparison::Gt),
            ">=" => Some(Comparison::Ge),
            "==" => Some(Comparison::Eq),
            "!=" => Some(Comparison::Ne),
            _ => None,
        }
    }
}

impl fmt::Display for Comparison {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Comparison::Lt => "<",
            Comparison::Le => "<=",
            Comparison::Gt => ">",
            Comparison::Ge => ">=",
            Comparison::Eq => "==",
            Comparison::Ne => "!=",
        })
    }
}

/// A single pass/fail rule over one metric.
///
/// The value is in the metric's own unit: milliseconds for a duration stat, a `0..1`
/// fraction for `rate`, a count for `count`. See the [module docs](self).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Threshold {
    /// The metric this rule is about.
    pub metric: ThresholdMetric,
    /// How the metric's samples are reduced to one number.
    pub stat: ThresholdStat,
    /// The comparison applied to that number.
    pub comparison: Comparison,
    /// The value it is compared against, in the metric's unit.
    pub value: f64,
}

impl Threshold {
    /// Parse one threshold expression for a named metric.
    ///
    /// `metric_name` and `expression` are exactly what the scenario said, so a failure
    /// can quote both back:
    ///
    /// ```
    /// use lev_core::threshold::Threshold;
    ///
    /// let threshold = Threshold::parse("http_req_duration", "p(95)<500").unwrap();
    /// assert_eq!(threshold.to_string(), "p(95)<500");
    ///
    /// assert!(Threshold::parse("http_reqs", "p(95)<500").is_err());
    /// ```
    pub fn parse(metric_name: &str, expression: &str) -> Result<Self, CoreError> {
        let invalid = |reason: String| {
            CoreError::InvalidThreshold(format!("threshold `{expression}`: {reason}"))
        };

        let metric = ThresholdMetric::parse(metric_name.trim()).ok_or_else(|| {
            invalid(format!(
                "unknown metric `{}` (known: {})",
                metric_name.trim(),
                known_metrics()
            ))
        })?;

        let text = expression.trim();
        let split = text
            .find(['<', '>', '=', '!'])
            .ok_or_else(|| invalid("expected a comparison like `p(95)<500`".to_string()))?;

        let stat_text = text[..split].trim();
        let rest = &text[split..];
        // Two-character operators first, so `>=` is not read as `>` followed by garbage.
        let (op_text, value_text) = if rest.starts_with("<=")
            || rest.starts_with(">=")
            || rest.starts_with("==")
            || rest.starts_with("!=")
        {
            (&rest[..2], &rest[2..])
        } else {
            (&rest[..1], &rest[1..])
        };

        let comparison = Comparison::parse(op_text).ok_or_else(|| {
            invalid(format!(
                "`{op_text}` is not a comparison (use `>`, `>=`, `<`, `<=`, `==`, `!=`)"
            ))
        })?;

        let stat = parse_stat(stat_text, metric).map_err(invalid)?;

        let value_text = value_text.trim();
        let value = value_text.parse::<f64>().map_err(|_| {
            invalid(format!(
                "`{value_text}` is not a number; thresholds read `500` as milliseconds, `0.01` as one percent"
            ))
        })?;
        if !value.is_finite() || value < 0.0 {
            return Err(invalid(format!(
                "`{value_text}` must be a finite, non-negative number"
            )));
        }

        Ok(Self {
            metric,
            stat,
            comparison,
            value,
        })
    }

    /// Render the expression as the writer emits it, canonical form.
    pub fn expression(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for Threshold {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}{}{}",
            self.stat,
            self.comparison,
            format_number(self.value)
        )
    }
}

impl fmt::Display for ThresholdStat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThresholdStat::Count => f.write_str("count"),
            ThresholdStat::Rate => f.write_str("rate"),
            ThresholdStat::Avg => f.write_str("avg"),
            ThresholdStat::Min => f.write_str("min"),
            ThresholdStat::Max => f.write_str("max"),
            ThresholdStat::Percentile(percentile) => {
                write!(f, "p({})", format_number(*percentile))
            }
        }
    }
}

/// Parse the `stat` half of an expression, rejecting a stat the metric cannot answer.
fn parse_stat(text: &str, metric: ThresholdMetric) -> Result<ThresholdStat, String> {
    let unsupported = |what: &str| {
        format!(
            "`{what}` needs a different metric; `{metric}` supports {}",
            metric.allowed_stats()
        )
    };

    match text {
        "count" => Ok(ThresholdStat::Count),
        "rate" if metric.allows_rate() => Ok(ThresholdStat::Rate),
        "avg" if metric.is_duration() => Ok(ThresholdStat::Avg),
        "min" if metric.is_duration() => Ok(ThresholdStat::Min),
        "max" if metric.is_duration() => Ok(ThresholdStat::Max),
        "rate" | "avg" | "min" | "max" => Err(unsupported(text)),
        other => {
            let inner = other
                .strip_prefix("p(")
                .and_then(|rest| rest.strip_suffix(')'))
                .ok_or_else(|| {
                    format!(
                        "unknown stat `{other}` for `{metric}` (supports {})",
                        metric.allowed_stats()
                    )
                })?;
            if !metric.is_duration() {
                return Err(unsupported(other));
            }
            let percentile = inner
                .trim()
                .trim_end_matches('%')
                .trim()
                .parse::<f64>()
                .map_err(|_| format!("`{other}` is not a percentile like `p(95)`"))?;
            if !(percentile > 0.0 && percentile <= 100.0) {
                return Err(format!(
                    "percentile {inner} is out of range (0 < p <= 100)"
                ));
            }
            Ok(ThresholdStat::Percentile(percentile))
        }
    }
}

/// The known metric names, for an error message.
fn known_metrics() -> String {
    ThresholdMetric::ALL
        .iter()
        .map(|metric| metric.name())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Render a threshold number without a trailing `.0`, so `500.0` reads back as `500`
/// and `0.01` keeps its digits. Round-trip fidelity matters: the format writer emits
/// what this returns and the parser must read it back unchanged.
fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{value:.0}")
    } else {
        format!("{value}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_common_expressions_parse() {
        let duration = Threshold::parse("http_req_duration", "p(95)<500").unwrap();
        assert_eq!(duration.metric, ThresholdMetric::HttpReqDuration);
        assert_eq!(duration.stat, ThresholdStat::Percentile(95.0));
        assert_eq!(duration.comparison, Comparison::Lt);
        assert_eq!(duration.value, 500.0);

        let rate = Threshold::parse("http_req_failed", "rate<0.01").unwrap();
        assert_eq!(rate.stat, ThresholdStat::Rate);
        assert_eq!(rate.value, 0.01);

        let count = Threshold::parse("http_reqs", "count>=100").unwrap();
        assert_eq!(count.stat, ThresholdStat::Count);
        assert_eq!(count.comparison, Comparison::Ge);
        assert_eq!(count.value, 100.0);

        let average = Threshold::parse("http_req_duration", "avg <= 250.5").unwrap();
        assert_eq!(average.stat, ThresholdStat::Avg);
        assert_eq!(average.comparison, Comparison::Le);
        assert_eq!(average.value, 250.5);

        let high = Threshold::parse("http_req_duration", "p(99.9)<1500").unwrap();
        assert_eq!(high.stat, ThresholdStat::Percentile(99.9));
    }

    #[test]
    fn expressions_render_canonically_and_round_trip() {
        for (metric, input) in [
            ("http_req_duration", "p(95)<500"),
            ("http_req_duration", "avg<=250.5"),
            ("http_req_duration", "max>1000"),
            ("http_req_duration", "min<5"),
            ("http_req_duration", "p(99.9)<1500"),
            ("http_req_duration", "count>0"),
            ("http_req_failed", "rate<0.01"),
            ("http_req_failed", "count<5"),
            ("iterations", "count>=10"),
        ] {
            let threshold = Threshold::parse(metric, input).unwrap();
            // Spacing is normalised; semantics must not be.
            assert_eq!(threshold.to_string(), input, "{input}");
            let reparsed = Threshold::parse(metric, &threshold.to_string()).unwrap();
            assert_eq!(threshold, reparsed, "{input} did not round-trip");
        }

        // Loose spacing parses to the same canonical form.
        let spaced = Threshold::parse("http_req_duration", "  p(95)  <  500  ").unwrap();
        assert_eq!(spaced.to_string(), "p(95)<500");
    }

    #[test]
    fn an_unknown_metric_is_rejected_with_the_known_ones() {
        let error = Threshold::parse("http_req_latency", "p(95)<500").unwrap_err();
        let message = error.to_string();
        assert!(message.contains("unknown metric `http_req_latency`"), "{message}");
        assert!(message.contains("http_req_duration"), "{message}");
    }

    #[test]
    fn a_stat_the_metric_cannot_answer_is_rejected() {
        // A percentile of a counter is nonsense.
        let error = Threshold::parse("http_reqs", "p(95)<500").unwrap_err();
        assert!(error.to_string().contains("supports count"), "{error}");

        // A rate only exists where there is a total to divide by.
        let error = Threshold::parse("http_req_duration", "rate<0.01").unwrap_err();
        assert!(error.to_string().contains("count, avg, min, max"), "{error}");

        // Not everything is a trend.
        let error = Threshold::parse("checks_failed", "avg<1").unwrap_err();
        assert!(error.to_string().contains("supports count"), "{error}");
    }

    #[test]
    fn malformed_expressions_are_rejected() {
        let cases = [
            ("http_req_duration", "p(95)"),
            ("http_req_duration", "p(95)~500"),
            ("http_req_duration", "p(95)=500"),
            ("http_req_duration", "p(95)<"),
            ("http_req_duration", "p(95)<fast"),
            ("http_req_duration", "p(95)<-1"),
            ("http_req_duration", "p(0)<500"),
            ("http_req_duration", "p(101)<500"),
            ("http_req_duration", "wat<500"),
            ("http_req_duration", "<500"),
        ];
        for (metric, expression) in cases {
            let error = match Threshold::parse(metric, expression) {
                Ok(parsed) => panic!("`{expression}` should not parse, got `{parsed}`"),
                Err(error) => error,
            };
            let message = error.to_string();
            assert!(
                message.starts_with("invalid threshold"),
                "`{expression}` should be an invalid threshold, got: {message}"
            );
        }
    }

    #[test]
    fn comparisons_hold_as_written() {
        assert!(Comparison::Lt.holds(1.0, 2.0));
        assert!(!Comparison::Lt.holds(2.0, 2.0));
        assert!(Comparison::Le.holds(2.0, 2.0));
        assert!(Comparison::Gt.holds(3.0, 2.0));
        assert!(Comparison::Ge.holds(2.0, 2.0));
        assert!(Comparison::Eq.holds(2.0, 2.0));
        assert!(Comparison::Ne.holds(1.0, 2.0));
    }

    #[test]
    fn metric_names_are_the_ones_the_engine_reports() {
        // These are k6's names (§4.5); the load engine tags its numbers with them.
        assert_eq!(ThresholdMetric::HttpReqDuration.name(), "http_req_duration");
        assert_eq!(ThresholdMetric::HttpReqFailed.name(), "http_req_failed");
        assert_eq!(ThresholdMetric::ScriptErrors.name(), "script_errors");
        assert_eq!(
            ThresholdMetric::parse("checks_failed"),
            Some(ThresholdMetric::ChecksFailed)
        );
        assert_eq!(ThresholdMetric::parse("nope"), None);
    }
}
