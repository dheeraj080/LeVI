// SPDX-License-Identifier: MIT OR Apache-2.0
//! Secret handling.
//!
//! A secret is split in two: a [`SecretName`] (safe to store on disk, log, and put in
//! a report) and a [`Secret`] value (zeroized on drop, never printed). Resolution is
//! abstracted behind [`SecretStore`], which has **no I/O** — the OS-keychain
//! implementation lives in a higher crate that is allowed to do I/O.

use std::collections::BTreeMap;

use zeroize::Zeroize;

use crate::error::{CoreError, Result};

/// The name of a secret. This is what a collection file references.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SecretName(String);

impl SecretName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for SecretName {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl From<String> for SecretName {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl std::fmt::Display for SecretName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A resolved secret value.
///
/// The bytes are zeroized on drop, `Debug` never reveals them, and it is never
/// serialized. It should only ever be handed to the transport at send time.
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Explicit, greppable access to the raw value. Use only where the value is
    /// actually needed (building an auth header, signing, etc.).
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl Clone for Secret {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(***REDACTED***)")
    }
}

/// Where secret values come from. No I/O here: implementations in higher crates may
/// read the OS keychain; tests and the CLI use [`MapSecretStore`].
pub trait SecretStore: Send + Sync {
    fn resolve(&self, name: &SecretName) -> Result<Secret>;
}

/// An in-memory secret store.
#[derive(Debug, Clone, Default)]
pub struct MapSecretStore {
    values: BTreeMap<String, String>,
}

impl MapSecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.values.insert(name.into(), value.into());
    }

    /// The raw values, for building a [`crate::Redactor`]. Never log these.
    pub fn values(&self) -> impl Iterator<Item = &str> {
        self.values.values().map(String::as_str)
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl SecretStore for MapSecretStore {
    fn resolve(&self, name: &SecretName) -> Result<Secret> {
        self.values
            .get(name.as_str())
            .map(|v| Secret::new(v.clone()))
            .ok_or_else(|| CoreError::SecretNotFound(name.as_str().to_string()))
    }
}
