// SPDX-License-Identifier: MIT OR Apache-2.0
//! Load scenarios: the declarative shape the engine schedules.
//!
//! A [`Scenario`] is pure data — an [`Executor`] and an ordered list of [`Step`]s that
//! reference requests by [`Id`]. The requests themselves live in the collection.
//! Binding a scenario to them (compiling the requests, resolving the variable scope)
//! happens in the load engine, which is where the I/O lives — the same rule that keeps
//! [`prepare`](crate::prepare) pure.
//!
//! A scenario also carries the [`Threshold`]s the run is judged by, and the
//! [`ThresholdMetric`]s that abort it early when they fail.

use std::time::Duration;

use crate::model::Id;
use crate::threshold::{Threshold, ThresholdMetric};

/// How a scenario drives load.
///
/// Slice 1 ships the **closed** model only ([`ConstantVus`]): a fixed pool of virtual
/// users that each loop their steps for a fixed duration. The open model
/// (`ConstantRate` / `RampingRate`), ramps and staged timelines arrive in later slices.
///
/// [`ConstantVus`]: Executor::ConstantVus
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Executor {
    /// A fixed pool of virtual users, each looping its steps for a fixed duration.
    ///
    /// A virtual user is **not** a thread: it is a logical async task with its own
    /// identity and seeded RNG. Many VUs multiplex onto few worker threads, which is
    /// what makes thousands of VUs fit in a few hundred megabytes.
    ConstantVus { vus: u32, duration: Duration },
}

impl Executor {
    /// The wall-clock duration of the run.
    pub fn duration(&self) -> Duration {
        match self {
            Executor::ConstantVus { duration, .. } => *duration,
        }
    }

    /// The peak number of virtual users the executor keeps in flight.
    pub fn max_vus(&self) -> u32 {
        match self {
            Executor::ConstantVus { vus, .. } => *vus,
        }
    }
}

/// One step in a scenario: a request, and the pause that follows it.
///
/// Steps run in order, once per iteration. Weighted and grouped steps are a later
/// slice; keeping the list flat keeps the format readable and the scheduler honest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The [`Id`] of the request to send.
    pub request: Id,
    /// Pause after the step completes, before the next step starts.
    pub think_time: Option<Duration>,
}

/// A load scenario: what to run, and how hard.
///
/// `PartialEq` but not `Eq`: a threshold's value is an `f64` (milliseconds, a `0..1`
/// fraction, or a count), and so is a percentile.
#[derive(Debug, Clone, PartialEq)]
pub struct Scenario {
    pub id: Id,
    pub name: String,
    pub executor: Executor,
    pub steps: Vec<Step>,
    /// The rules that decide whether the run passed. Empty means "no verdict": the run
    /// succeeds if it completes, which is what every scenario did before thresholds.
    pub thresholds: Vec<Threshold>,
    /// Metrics whose **failure aborts the run** mid-flight rather than being reported at
    /// the end. Each name must also have a threshold; the format layer rejects a name
    /// that does not, because an abort rule with no rule behind it can never fire.
    pub abort_on: Vec<ThresholdMetric>,
    /// The run's default request deadline (`timeout: 30s`), applied to requests that
    /// do not set their own. A load run must never inherit an unbounded request:
    /// one stuck request would hold a virtual user forever.
    pub timeout: Option<Duration>,
}

impl Scenario {
    /// Whether the scenario has at least one step to run.
    pub fn is_runnable(&self) -> bool {
        !self.steps.is_empty()
    }

    /// Whether any threshold is configured.
    pub fn has_thresholds(&self) -> bool {
        !self.thresholds.is_empty()
    }

    /// Whether a failed threshold on `metric` should abort the run.
    pub fn aborts_on(&self, metric: ThresholdMetric) -> bool {
        self.abort_on.contains(&metric)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_executor_reports_its_duration_and_peak_vus() {
        let executor = Executor::ConstantVus {
            vus: 25,
            duration: Duration::from_secs(30),
        };
        assert_eq!(executor.duration(), Duration::from_secs(30));
        assert_eq!(executor.max_vus(), 25);
    }

    #[test]
    fn a_scenario_with_steps_is_runnable() {
        let scenario = Scenario {
            id: "checkout".to_string(),
            name: "Checkout".to_string(),
            executor: Executor::ConstantVus {
                vus: 1,
                duration: Duration::from_secs(1),
            },
            steps: vec![Step {
                request: "create-order".to_string(),
                think_time: None,
            }],
            thresholds: Vec::new(),
            abort_on: Vec::new(),
            timeout: None,
        };
        assert!(scenario.is_runnable());
        assert!(!scenario.has_thresholds());
    }

    #[test]
    fn a_scenario_carries_its_thresholds_and_abort_rules() {
        let threshold = Threshold::parse("http_req_failed", "rate<0.01").unwrap();
        let scenario = Scenario {
            id: "checkout".to_string(),
            name: "Checkout".to_string(),
            executor: Executor::ConstantVus {
                vus: 1,
                duration: Duration::from_secs(1),
            },
            steps: vec![Step {
                request: "create-order".to_string(),
                think_time: None,
            }],
            thresholds: vec![threshold],
            abort_on: vec![ThresholdMetric::HttpReqFailed],
            timeout: Some(Duration::from_secs(30)),
        };
        assert!(scenario.has_thresholds());
        assert_eq!(scenario.timeout, Some(Duration::from_secs(30)));
        assert!(scenario.aborts_on(ThresholdMetric::HttpReqFailed));
        assert!(!scenario.aborts_on(ThresholdMetric::HttpReqs));
    }
}
