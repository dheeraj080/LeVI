// SPDX-License-Identifier: MIT OR Apache-2.0
//! Sandbox limits.
//!
//! Two limits are enforced by the QuickJS runtime itself, not by convention: an
//! interrupt handler stops a runaway script at the CPU budget, and the memory limit
//! stops one that allocates without bound. Both are per *script invocation*, so one
//! bad script cannot take the process down with it.

use std::time::Duration;

/// How much a single script invocation is allowed to consume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Wall-clock CPU budget for one invocation of one hook.
    pub cpu_budget: Duration,
    /// Heap limit for the pooled QuickJS runtime, in bytes.
    pub memory_limit: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            // Generous for a script that is meant to be glue, tight enough that an
            // accidental `while (true)` is caught in well under a second.
            cpu_budget: Duration::from_millis(1_000),
            memory_limit: 64 * 1024 * 1024,
        }
    }
}

impl Limits {
    /// Limits with an explicit CPU budget, keeping the default memory limit.
    pub fn with_cpu_budget(cpu_budget: Duration) -> Self {
        Self {
            cpu_budget,
            ..Self::default()
        }
    }
}
