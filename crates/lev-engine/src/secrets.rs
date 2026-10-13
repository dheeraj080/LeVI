// SPDX-License-Identifier: MIT OR Apache-2.0
//! Secret resolution for the engine.
//!
//! `lev-core` defines the [`SecretStore`](lev_core::SecretStore) trait and owns no
//! I/O. This is where I/O is allowed, so `lev-engine` reads secrets from the
//! process environment until the OS keychain lands.
//!
//! A secret named `payments_prod_token` is read from `LEVI_SECRET_payments_prod_token`.
//! Values are held only to build the [`Redactor`] and to render templates; they are
//! never sent over the protocol.

use std::env;

use lev_core::{MapSecretStore, Redactor};

/// Environment variables named `LEVI_SECRET_<NAME>` seed the secret store.
pub const SECRET_ENV_PREFIX: &str = "LEVI_SECRET_";

/// Build a secret store from the process environment.
pub fn store_from_env() -> MapSecretStore {
    let mut store = MapSecretStore::new();
    for (key, value) in env::vars_os() {
        let (Some(key), Some(value)) = (key.to_str(), value.to_str()) else {
            continue;
        };
        if let Some(name) = key.strip_prefix(SECRET_ENV_PREFIX) {
            store.insert(name.to_string(), value);
        }
    }
    store
}

/// A redactor that scrubs every value in `store`.
///
/// Every log line, echo and response passes through this before it is shown or
/// crosses the protocol boundary.
pub fn redactor_for(store: &MapSecretStore) -> Redactor {
    Redactor::with_secrets(store.values())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redactor_scrubs_values_from_the_store() {
        let mut store = MapSecretStore::new();
        store.insert("token", "s3cr3t");
        let redactor = redactor_for(&store);
        assert!(!redactor.redact("Bearer s3cr3t").contains("s3cr3t"));
        assert_eq!(
            redactor.redact_header_value("authorization", "Bearer s3cr3t"),
            lev_core::MASK
        );
    }
}
