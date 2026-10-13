// SPDX-License-Identifier: MIT OR Apache-2.0
//! The Levi collection format.
//!
//! Git-friendly plain text, one request per file. This crate owns the on-disk DTOs
//! and all (de)serialization; it converts to and from the `lev-core` domain model,
//! which is never serialized. The concrete syntax sits behind [`DocumentFormat`] so
//! it can be swapped without touching the rest of the app.
#![forbid(unsafe_code)]

mod convert;
pub mod dto;
pub mod error;
mod yaml;

use std::path::Path;

use lev_core::{Collection, RequestDef, VarSet};

pub use error::FormatError;
pub use yaml::Yaml;

/// A concrete on-disk representation (YAML today; anything later).
pub trait DocumentFormat {
    fn request_from_str(&self, source: &str) -> Result<RequestDef, FormatError>;
    fn request_to_string(&self, request: &RequestDef) -> Result<String, FormatError>;

    fn collection_from_str(&self, source: &str) -> Result<Collection, FormatError>;
    fn collection_to_string(&self, collection: &Collection) -> Result<String, FormatError>;

    fn vars_from_str(&self, source: &str) -> Result<VarSet, FormatError>;
}

pub fn load_collection(
    format: &dyn DocumentFormat,
    path: &Path,
) -> Result<Collection, FormatError> {
    let source = read(path)?;
    format.collection_from_str(&source)
}

pub fn load_request(format: &dyn DocumentFormat, path: &Path) -> Result<RequestDef, FormatError> {
    let source = read(path)?;
    format.request_from_str(&source)
}

pub fn load_vars(format: &dyn DocumentFormat, path: &Path) -> Result<VarSet, FormatError> {
    let source = read(path)?;
    format.vars_from_str(&source)
}

pub fn save_collection(
    format: &dyn DocumentFormat,
    path: &Path,
    collection: &Collection,
) -> Result<(), FormatError> {
    let text = format.collection_to_string(collection)?;
    write(path, &text)
}

pub fn save_request(
    format: &dyn DocumentFormat,
    path: &Path,
    request: &RequestDef,
) -> Result<(), FormatError> {
    let text = format.request_to_string(request)?;
    write(path, &text)
}

fn read(path: &Path) -> Result<String, FormatError> {
    std::fs::read_to_string(path).map_err(|source| FormatError::Io {
        path: path.display().to_string(),
        source,
    })
}

fn write(path: &Path, text: &str) -> Result<(), FormatError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|source| FormatError::Io {
                path: parent.display().to_string(),
                source,
            })?;
        }
    }
    std::fs::write(path, text).map_err(|source| FormatError::Io {
        path: path.display().to_string(),
        source,
    })
}
