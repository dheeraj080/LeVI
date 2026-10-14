// SPDX-License-Identifier: MIT OR Apache-2.0
//! Errors from loading or saving the collection format.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FormatError {
    #[error("yaml parse error: {0}")]
    YamlParse(#[from] serde_saphyr::DeserializeError),

    #[error("yaml render error: {0}")]
    YamlRender(#[from] serde_saphyr::SerializeError),

    #[error("failed to read `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid duration `{0}`: expected e.g. `250ms`, `30s`, `2m`, `1h`")]
    InvalidDuration(String),

    #[error("invalid scenario: {0}")]
    InvalidScenario(String),

    #[error(transparent)]
    Core(#[from] lev_core::CoreError),
}
