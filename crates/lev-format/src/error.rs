// SPDX-License-Identifier: MIT OR Apache-2.0
//! Errors from loading or saving the collection format.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum FormatError {
    #[error("yaml error: {0}")]
    Yaml(#[from] serde_yaml_ng::Error),

    #[error("failed to read `{path}`: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Core(#[from] lev_core::CoreError),
}
