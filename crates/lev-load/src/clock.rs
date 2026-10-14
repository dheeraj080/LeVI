// SPDX-License-Identifier: MIT OR Apache-2.0
//! The time source, made injectable so scheduler logic can be tested in microseconds.
//!
//! The engine never calls `tokio::time` directly for a timing decision; it asks a
//! [`Clock`]. Production uses [`TokioClock`], which under `tokio::time::pause()` is
//! virtual, so the executor's tests run a nominal thirty seconds in no wall time.

use std::future::Future;
use std::pin::Pin;

use tokio::time::Instant;

/// A monotonic time source.
///
/// Only a monotonic clock is used for scheduling and latency; the report's
/// `startedAt` wall-clock stamp is taken separately, so a clock adjustment mid-run
/// cannot corrupt a measurement.
pub trait Clock: Send + Sync + 'static {
    /// The current instant.
    fn now(&self) -> Instant;

    /// Resolve once `deadline` has been reached.
    fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn Future<Output = ()> + Send + '_>>;
}

/// The production clock: `tokio`'s timer.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioClock;

impl TokioClock {
    pub fn new() -> Self {
        Self
    }
}

impl Clock for TokioClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn sleep_until(&self, deadline: Instant) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(tokio::time::sleep_until(deadline))
    }
}
