// SPDX-License-Identifier: MIT OR Apache-2.0
//! Secret resolution for the engine.
//!
//! `lev-core` defines the [`SecretStore`] trait and owns no
//! I/O. This is where I/O is allowed, so this module is where a secret's value
//! actually comes from: the process environment first, the OS keychain second.
//!
//! A secret named `payments_prod_token` is read from
//! `LEVI_SECRET_payments_prod_token`, and from the keychain entry
//! `levi/payments_prod_token` when the environment does not name it. Values are held
//! only to build the [`Redactor`] and to render templates; they are never sent over
//! the protocol.

use std::collections::BTreeMap;
use std::env;
use std::sync::{Arc, Mutex, PoisonError};

use lev_core::{CoreError, MapSecretStore, Redactor, Secret, SecretName, SecretStore};

/// Environment variables named `LEVI_SECRET_<NAME>` seed the secret store.
pub const SECRET_ENV_PREFIX: &str = "LEVI_SECRET_";

/// The service name every LeVI keychain entry lives under.
///
/// One service, one entry per secret name: `levi payments_prod_token` in the
/// Credential Manager, `levi/payments_prod_token` everywhere it is shown. A single
/// name is what makes `levi secret delete` able to say what it removed.
const KEYCHAIN_SERVICE: &str = "levi";

/// Build the runtime secret store: the process environment, then the OS keychain.
///
/// Nothing is read from either source until a collection asks for a name, and each
/// name is read at most once per process — see [`ResolvingStore`].
pub fn store_from_env() -> ResolvingStore {
    ResolvingStore::new(environment_store(), Some(Arc::new(KeychainStore)))
}

/// The process environment as an in-memory store.
fn environment_store() -> MapSecretStore {
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

/// A redactor that scrubs every one of these values.
///
/// Every log line, echo and response passes through this before it is shown or
/// crosses the protocol boundary.
pub fn redactor_for<'a>(values: impl Iterator<Item = &'a str>) -> Redactor {
    Redactor::with_secrets(values)
}

/// Read one secret from the OS keychain.
fn keychain_get(name: &SecretName) -> lev_core::Result<Secret> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, name.as_str())
        .map_err(|error| keychain_error(name.as_str(), &error))?;
    match entry.get_password() {
        Ok(value) => Ok(Secret::new(value)),
        Err(keyring::Error::NoEntry) => Err(CoreError::SecretNotFound(name.to_string())),
        Err(error) => Err(keychain_error(name.as_str(), &error)),
    }
}

fn keychain_error(name: &str, error: &keyring::Error) -> CoreError {
    CoreError::KeychainUnavailable {
        name: name.to_string(),
        reason: error.to_string(),
    }
}

/// Store a secret in the OS keychain, replacing it if the name is already there.
///
/// This is `levi secret set`. The value never touches the environment or a file
/// LeVI reads, and it is handed to the keychain exactly once.
pub fn keychain_set(name: &str, value: &str) -> lev_core::Result<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, name).map_err(|error| {
        CoreError::KeychainUnavailable {
            name: name.to_string(),
            reason: error.to_string(),
        }
    })?;
    entry
        .set_password(value)
        .map_err(|error| keychain_error(name, &error))
}

/// Remove a secret from the OS keychain.
///
/// Removing a name that is not there is success: the state the command asked for is
/// the state the keychain is in.
pub fn keychain_delete(name: &str) -> lev_core::Result<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, name).map_err(|error| {
        CoreError::KeychainUnavailable {
            name: name.to_string(),
            reason: error.to_string(),
        }
    })?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(keychain_error(name, &error)),
    }
}

/// The OS keychain as a secret store.
///
/// Windows Credential Manager, the macOS keychain, the platform secret service —
/// whatever the platform calls it, `keyring` addresses it as `levi/<name>`. It is
/// only ever asked about names the environment does not have, and only once per
/// name, so a machine with no keychain (a CI container) breaks only when a
/// collection references a keychain-only secret, not on every send.
#[derive(Debug, Default)]
pub struct KeychainStore;

impl SecretStore for KeychainStore {
    fn resolve(&self, name: &SecretName) -> lev_core::Result<Secret> {
        keychain_get(name)
    }
}

/// Where secrets actually come from at runtime: the environment, then the OS
/// keychain, with each name resolved at most once per process.
///
/// The cache matters in both directions. For the keychain it is the difference
/// between one credential-store call per process and one per request — a load run
/// must not turn into keychain hammering. For redaction it is what makes keychain
/// secrets usable at all: the redactor is built from values known up front, and
/// keychain values are not, so every value this resolves is reported through
/// [`SecretStore::newly_resolved`] and folded into the redactor *before* anything
/// can echo it.
pub struct ResolvingStore {
    env: MapSecretStore,
    fallback: Option<Arc<dyn SecretStore>>,
    resolved: Mutex<Resolved>,
}

/// The cache, and the values the redactor has not been told about yet.
#[derive(Default)]
struct Resolved {
    values: BTreeMap<String, Secret>,
    /// Values resolved but not yet reported. [`SecretStore::newly_resolved`] drains
    /// this; the cache in `values` keeps them.
    unreported: Vec<String>,
}

impl ResolvingStore {
    /// The process environment first, then the OS keychain.
    pub fn from_env() -> Self {
        Self::new(environment_store(), Some(Arc::new(KeychainStore)))
    }

    /// These in-memory values, and nothing else — no OS calls, ever.
    ///
    /// This is what tests and [`crate::Engine::from_store`] use, so a test suite can
    /// never come to depend on the machine it happens to run on.
    pub fn memory(env: MapSecretStore) -> Self {
        Self::new(env, None)
    }

    /// An in-memory environment plus a second store to ask when it has no answer.
    ///
    /// The keychain is the production second source; a test passes a fake to prove
    /// the ordering without a real credential store.
    pub fn with_fallback(env: MapSecretStore, fallback: Arc<dyn SecretStore>) -> Self {
        Self::new(env, Some(fallback))
    }

    fn new(env: MapSecretStore, fallback: Option<Arc<dyn SecretStore>>) -> Self {
        Self {
            env,
            fallback,
            resolved: Mutex::new(Resolved::default()),
        }
    }

    /// The values knowable without asking the second store — for seeding a
    /// [`Redactor`] before any keychain name has been resolved.
    pub fn known_values(&self) -> impl Iterator<Item = &str> {
        self.env.values()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Resolved> {
        self.resolved.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl SecretStore for ResolvingStore {
    fn resolve(&self, name: &SecretName) -> lev_core::Result<Secret> {
        if let Some(secret) = self.lock().values.get(name.as_str()) {
            return Ok(secret.clone());
        }
        let secret = match self.env.resolve(name) {
            Ok(secret) => secret,
            Err(CoreError::SecretNotFound(_)) => match &self.fallback {
                Some(fallback) => fallback.resolve(name)?,
                None => return Err(CoreError::SecretNotFound(name.to_string())),
            },
            Err(error) => return Err(error),
        };
        let mut resolved = self.lock();
        resolved.values.insert(name.to_string(), secret.clone());
        resolved.unreported.push(secret.expose().to_string());
        Ok(secret)
    }

    fn newly_resolved(&self) -> Vec<String> {
        std::mem::take(&mut self.lock().unreported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store_with(secret: &str, value: &str) -> MapSecretStore {
        let mut store = MapSecretStore::new();
        store.insert(secret, value);
        store
    }

    #[test]
    fn the_environment_wins_over_the_second_store() {
        let fallback = Arc::new(store_with("token", "from-the-keychain"));
        let store = ResolvingStore::with_fallback(store_with("token", "from-the-env"), fallback);

        let resolved = store
            .resolve(&SecretName::new("token"))
            .expect("both sources have it");
        assert_eq!(resolved.expose(), "from-the-env");
    }

    #[test]
    fn the_second_store_answers_what_the_environment_does_not() {
        let fallback = Arc::new(store_with("keychain_only", "from-the-keychain"));
        let store = ResolvingStore::with_fallback(MapSecretStore::new(), fallback);

        let resolved = store
            .resolve(&SecretName::new("keychain_only"))
            .expect("the keychain has it");
        assert_eq!(resolved.expose(), "from-the-keychain");
    }

    #[test]
    fn memory_only_stores_never_reach_a_keychain() {
        // No second source exists to ask, so a miss is a naming mistake and nothing
        // else — the shape that keeps a test suite independent of the host machine.
        let store = ResolvingStore::memory(MapSecretStore::new());
        let error = store
            .resolve(&SecretName::new("absent"))
            .expect_err("nothing has this");
        assert!(matches!(error, CoreError::SecretNotFound(_)), "{error}");
    }

    #[test]
    fn each_name_is_asked_for_once_and_then_served_from_the_cache() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        struct Counting {
            store: MapSecretStore,
            reads: AtomicUsize,
        }
        impl SecretStore for Counting {
            fn resolve(&self, name: &SecretName) -> lev_core::Result<Secret> {
                self.reads.fetch_add(1, Ordering::SeqCst);
                self.store.resolve(name)
            }
        }

        let fallback = Arc::new(Counting {
            store: store_with("token", "asked-for-once"),
            reads: AtomicUsize::new(0),
        });
        let store =
            ResolvingStore::with_fallback(MapSecretStore::new(), Arc::clone(&fallback) as _);

        for _ in 0..4 {
            let resolved = store.resolve(&SecretName::new("token")).expect("present");
            assert_eq!(resolved.expose(), "asked-for-once");
        }
        assert_eq!(fallback.reads.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn newly_resolved_reports_each_value_once_and_then_silence() {
        let fallback = Arc::new(store_with("a", "first"));
        let store = ResolvingStore::with_fallback(store_with("b", "second"), fallback);

        assert!(store.newly_resolved().is_empty(), "nothing resolved yet");
        store.resolve(&SecretName::new("a")).expect("present");
        store.resolve(&SecretName::new("b")).expect("present");
        store.resolve(&SecretName::new("a")).expect("cached");
        assert_eq!(store.newly_resolved(), vec!["first", "second"]);

        // Reported once, never again: the redactor keeps the value, the report does
        // not keep arriving.
        store.resolve(&SecretName::new("a")).expect("cached");
        assert!(store.newly_resolved().is_empty());
    }

    #[test]
    fn a_keychain_failure_is_not_reported_as_a_missing_secret() {
        struct Broken;
        impl SecretStore for Broken {
            fn resolve(&self, name: &SecretName) -> lev_core::Result<Secret> {
                Err(CoreError::KeychainUnavailable {
                    name: name.to_string(),
                    reason: "the session bus is not there".to_string(),
                })
            }
        }

        let store = ResolvingStore::with_fallback(MapSecretStore::new(), Arc::new(Broken));
        let error = store
            .resolve(&SecretName::new("token"))
            .expect_err("the fallback is broken");
        assert!(
            matches!(error, CoreError::KeychainUnavailable { .. }),
            "{error}"
        );
    }
}
