// SPDX-License-Identifier: MIT OR Apache-2.0
//! The YAML implementation of [`DocumentFormat`], backed by `serde-saphyr`.
//!
//! The parser is deliberately isolated here so the concrete syntax can change
//! without touching `lev-core` or the rest of the app.

use lev_core::{Collection, RequestDef, VarSet};

use crate::convert;
use crate::dto::{CollectionDto, RequestDto, VarsFileDto};
use crate::error::FormatError;
use crate::DocumentFormat;

/// YAML documents: `collection.yaml`, `requests/*.yaml`, `environments/*.yaml`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Yaml;

impl DocumentFormat for Yaml {
    fn request_from_str(&self, source: &str) -> Result<RequestDef, FormatError> {
        let dto: RequestDto = serde_saphyr::from_str(source)?;
        convert::request_to_core(dto)
    }

    fn request_to_string(&self, request: &RequestDef) -> Result<String, FormatError> {
        let dto = convert::request_from_core(request);
        Ok(serde_saphyr::to_string(&dto)?)
    }

    fn collection_from_str(&self, source: &str) -> Result<Collection, FormatError> {
        let dto: CollectionDto = serde_saphyr::from_str(source)?;
        convert::collection_to_core(dto)
    }

    fn collection_to_string(&self, collection: &Collection) -> Result<String, FormatError> {
        let dto = convert::collection_from_core(collection);
        Ok(serde_saphyr::to_string(&dto)?)
    }

    fn vars_from_str(&self, source: &str) -> Result<VarSet, FormatError> {
        let dto: VarsFileDto = serde_saphyr::from_str(source)?;
        convert::vars_to_core(dto.vars)
    }
}
