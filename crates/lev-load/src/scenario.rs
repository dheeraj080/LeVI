// SPDX-License-Identifier: MIT OR Apache-2.0
//! Binding a declarative [`Scenario`] to real requests.
//!
//! `lev-core` owns the scenario *shape* (ids and executor settings); this is where the
//! ids are resolved against the collection, each request is compiled once (see
//! `lev_core::compile_request`), and the executor's knobs are flattened into the form
//! the runner wants. Everything that could fail happens here, during `Preparing`, so
//! the hot loop only ever renders and sends.

use std::collections::BTreeMap;
use std::time::Duration;

use lev_core::{
    compile_request, Collection, CompiledRequest, Executor, Id, RequestDef, Scenario, Threshold,
    ThresholdMetric,
};

use crate::error::LoadError;

/// One step, with its request already compiled.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledStep {
    /// The request id, kept for error messages and (later) per-step metrics.
    pub request_id: Id,
    /// The request, parsed once.
    pub request: CompiledRequest,
    /// Pause after this step, before the next.
    pub think_time: Option<Duration>,
}

/// A scenario bound to its requests, ready to run.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledScenario {
    pub id: Id,
    pub name: String,
    /// Virtual users the closed executor keeps in flight.
    pub vus: u32,
    /// How long the run is planned to last.
    pub duration: Duration,
    pub steps: Vec<CompiledStep>,
    /// The rules the run is judged by, parsed once.
    pub thresholds: Vec<Threshold>,
    /// Metrics whose failure stops the run mid-flight.
    pub abort_on: Vec<ThresholdMetric>,
}

impl CompiledScenario {
    /// Resolve every step's request id against `requests`, compiling each request once.
    ///
    /// `collection` supplies the `baseUrl` and default `auth`, exactly as the
    /// interactive path uses them, so a load run resolves a request the same way a
    /// single send does.
    pub fn compile(
        scenario: &Scenario,
        collection: &Collection,
        requests: &BTreeMap<Id, RequestDef>,
    ) -> Result<Self, LoadError> {
        let (vus, duration) = match &scenario.executor {
            Executor::ConstantVus { vus, duration } => (*vus, *duration),
        };
        if vus == 0 {
            return Err(LoadError::Config(format!(
                "scenario `{}` has no virtual users",
                scenario.id
            )));
        }
        if scenario.steps.is_empty() {
            return Err(LoadError::Config(format!(
                "scenario `{}` has no steps",
                scenario.id
            )));
        }

        let mut steps = Vec::with_capacity(scenario.steps.len());
        for step in &scenario.steps {
            let request = requests.get(&step.request).ok_or_else(|| {
                LoadError::Config(format!(
                    "scenario `{}` references unknown request `{}`",
                    scenario.id, step.request
                ))
            })?;
            steps.push(CompiledStep {
                request_id: step.request.clone(),
                request: compile_request(collection, request)?,
                think_time: step.think_time,
            });
        }

        Ok(Self {
            id: scenario.id.clone(),
            name: scenario.name.clone(),
            vus,
            duration,
            steps,
            thresholds: scenario.thresholds.clone(),
            abort_on: scenario.abort_on.clone(),
        })
    }
}
