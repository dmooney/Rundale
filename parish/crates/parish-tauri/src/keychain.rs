//! OS keychain implementation of `parish_core::secret_store::SecretStore`.
//!
//! Uses the `keyring` crate to talk to:
//!   - macOS Keychain Services
//!   - Windows Credential Manager
//!   - Linux Secret Service (libsecret / KWallet)
//!
//! Service name `com.parish.rundale` matches the bundle id; one record per
//! provider (`provider:anthropic`, `provider:openrouter`, …) so users can
//! switch providers without losing prior keys.

use parish_core::secret_store::{SecretStore, SecretStoreError};

const SERVICE: &str = "com.parish.rundale";

/// Backed by the OS keychain via the `keyring` crate.
#[derive(Debug, Default)]
pub struct KeyringSecretStore;

impl KeyringSecretStore {
    pub fn new() -> Self {
        Self
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry, SecretStoreError> {
        keyring::Entry::new(SERVICE, account).map_err(map_keyring_err)
    }
}

impl SecretStore for KeyringSecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, SecretStoreError> {
        let entry = self.entry(account)?;
        match entry.get_password() {
            Ok(s) => Ok(Some(s)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_keyring_err(e)),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretStoreError> {
        let entry = self.entry(account)?;
        entry.set_password(secret).map_err(map_keyring_err)
    }

    fn delete(&self, account: &str) -> Result<(), SecretStoreError> {
        let entry = self.entry(account)?;
        match entry.delete_credential() {
            Ok(()) => Ok(()),
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_keyring_err(e)),
        }
    }
}

fn map_keyring_err(e: keyring::Error) -> SecretStoreError {
    match e {
        keyring::Error::NoEntry => SecretStoreError::NotFound,
        keyring::Error::PlatformFailure(inner) => {
            SecretStoreError::Backend(format!("platform: {inner}"))
        }
        keyring::Error::NoStorageAccess(inner) => {
            SecretStoreError::Permission(format!("no storage access: {inner}"))
        }
        other => SecretStoreError::Backend(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On a CI box without an unlocked keychain (Linux containers, headless
    /// macOS runners) Keyring will return PlatformFailure / NoStorageAccess.
    /// Treat those as "skip" so we don't flake CI; locally on a developer
    /// machine the round-trip exercises the real backend.
    #[test]
    fn round_trip_or_skip() {
        let store = KeyringSecretStore::new();
        let account = "provider:rundale-test-suite";
        let secret = "sk-test-roundtrip";
        match store.set(account, secret) {
            Ok(()) => {
                let got = store.get(account).unwrap();
                assert_eq!(got.as_deref(), Some(secret));
                store.delete(account).unwrap();
                assert_eq!(store.get(account).unwrap(), None);
            }
            Err(SecretStoreError::Permission(_)) | Err(SecretStoreError::Backend(_)) => {
                eprintln!("keyring backend unavailable in this environment; skipping");
            }
            Err(e) => panic!("unexpected error: {e}"),
        }
    }
}
