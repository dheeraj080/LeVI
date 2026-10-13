// SPDX-License-Identifier: MIT OR Apache-2.0
//! Errors from the script sandbox.

use std::time::Duration;

use thiserror::Error;

/// Anything that can go wrong compiling or running a script.
///
/// A script failure is *contained*: it never poisons the sandbox, so the next script
/// runs on the same pooled runtime. That is what "a runaway script must not hang a VU"
/// means in practice.
#[derive(Debug, Error)]
pub enum ScriptError {
    /// The script did not parse. An author mistake, so callers map it to a config error.
    #[error("script syntax error: {0}")]
    Compile(String),

    /// The script threw, or called a `lev.*` function incorrectly.
    #[error("script error: {0}")]
    Runtime(String),

    /// The script exceeded its CPU budget and was interrupted.
    #[error("script exceeded its {budget_ms}ms CPU budget and was interrupted")]
    Timeout { budget_ms: u128 },

    /// The script exceeded the runtime's memory limit.
    #[error("script exceeded the memory limit: {0}")]
    Memory(String),

    /// A failure in the harness itself, not in the user's script.
    #[error("script runtime failure: {0}")]
    Internal(String),
}

impl ScriptError {
    /// Build a [`ScriptError::Timeout`] from a budget.
    pub fn timeout(budget: Duration) -> Self {
        ScriptError::Timeout {
            budget_ms: budget.as_millis(),
        }
    }
}
