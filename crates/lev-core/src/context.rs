// SPDX-License-Identifier: MIT OR Apache-2.0
//! A concrete [`RenderContext`] that resolves variables (including secrets) and
//! the built-in dynamic functions.

use crate::error::{CoreError, Result};
use crate::model::{VarSet, VarValue};
use crate::secrets::SecretStore;
use crate::template::{DynamicFunctions, RenderContext};

/// Resolves `{{ name }}` from a merged [`VarSet`] and `{{$func}}` from the built-in
/// dynamic functions.
pub struct Context<'a> {
    vars: VarSet,
    secrets: &'a dyn SecretStore,
    dynamic: DynamicFunctions,
}

impl<'a> Context<'a> {
    pub fn new(vars: VarSet, secrets: &'a dyn SecretStore) -> Self {
        Self::with_seed(vars, secrets, 0)
    }

    pub fn with_seed(vars: VarSet, secrets: &'a dyn SecretStore, seed: u64) -> Self {
        Self {
            vars,
            secrets,
            dynamic: DynamicFunctions::new(seed),
        }
    }

    pub fn vars(&self) -> &VarSet {
        &self.vars
    }

    /// Resolve a [`VarValue`] that is stored inline (not under a variable name):
    /// a literal, or a secret read from the store.
    ///
    /// Auth fields carry their value this way rather than by variable name, so this
    /// is the one place a `Secret` is turned into text during preparation.
    pub fn resolve_value(&self, value: &VarValue) -> Result<String> {
        match value {
            VarValue::Plain(value) => Ok(value.clone()),
            VarValue::Secret(name) => Ok(self.secrets.resolve(name)?.expose().to_string()),
        }
    }
}

impl RenderContext for Context<'_> {
    fn var(&self, name: &str) -> Result<String> {
        match self.vars.get(name) {
            Some(VarValue::Plain(value)) => Ok(value.clone()),
            Some(VarValue::Secret(secret_name)) => {
                let secret = self.secrets.resolve(secret_name)?;
                Ok(secret.expose().to_string())
            }
            None => Err(CoreError::UnresolvedVariable(name.to_string())),
        }
    }

    fn func(&mut self, name: &str) -> Result<String> {
        self.dynamic
            .call(name)
            .ok_or_else(|| CoreError::UnknownFunction(name.to_string()))
    }
}
