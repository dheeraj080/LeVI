// SPDX-License-Identifier: MIT OR Apache-2.0
//! Load scenarios: the declarative shape the engine schedules.
//!
//! A [`Scenario`] is pure data — an [`Executor`] and an ordered list of [`Step`]s that
//! reference requests by [`Id`]. The requests themselves live in the collection.
//! Binding a scenario to them (compiling the requests, resolving the variable scope)
//! happens in the load engine, which is where the I/O lives — the same rule that keeps
//! [`prepare`](crate::prepare) pure.

use std::time::Duration;

use crate::model::Id;

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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scenario {
    pub id: Id,
    pub name: String,
    pub executor: Executor,
    pub steps: Vec<Step>,
}

impl Scenario {
    /// Whether the scenario has at least one step to run.
    pub fn is_runnable(&self) -> bool {
        !self.steps.is_empty()
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
        };
        assert!(scenario.is_runnable());
    }
}
