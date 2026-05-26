//! Backend-agnostic secret storage for BYOK (Bring Your Own Key) credentials.
//!
//! Implementations live in runtime crates: `parish-tauri` provides
//! `KeyringSecretStore` (OS keychain), while `parish-engine` and `parish-server`
//! use env-var or no-op stubs in v1. Tests use `InMemorySecretStore`.

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SecretStoreError {
    #[error("secret not found")]
    NotFound,
    #[error("permission denied: {0}")]
    Permission(String),
    #[error("backend error: {0}")]
    Backend(String),
}

pub trait SecretStore: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, SecretStoreError>;
    fn set(&self, account: &str, secret: &str) -> Result<(), SecretStoreError>;
    fn delete(&self, account: &str) -> Result<(), SecretStoreError>;
}

/// Account-naming helper. Use one record per provider so users can switch
/// providers without losing previously-entered keys.
pub fn provider_account(provider_lowercase: &str) -> String {
    format!("provider:{provider_lowercase}")
}

/// Process-local secret store. Used in tests and as the default fallback for
/// runtimes that haven't wired a real backend (e.g. headless CLI in v1).
#[derive(Debug, Default)]
pub struct InMemorySecretStore {
    inner: Mutex<HashMap<String, String>>,
}

impl InMemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl SecretStore for InMemorySecretStore {
    fn get(&self, account: &str) -> Result<Option<String>, SecretStoreError> {
        Ok(self.inner.lock().unwrap().get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), SecretStoreError> {
        self.inner
            .lock()
            .unwrap()
            .insert(account.to_string(), secret.to_string());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), SecretStoreError> {
        self.inner.lock().unwrap().remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_set_get_round_trip() {
        let s = InMemorySecretStore::new();
        s.set("provider:anthropic", "sk-ant-xxx").unwrap();
        assert_eq!(
            s.get("provider:anthropic").unwrap(),
            Some("sk-ant-xxx".to_string())
        );
    }

    #[test]
    fn in_memory_get_returns_none_when_missing() {
        let s = InMemorySecretStore::new();
        assert_eq!(s.get("provider:nope").unwrap(), None);
    }

    #[test]
    fn in_memory_delete_then_get_returns_none() {
        let s = InMemorySecretStore::new();
        s.set("provider:openai", "sk-xxx").unwrap();
        s.delete("provider:openai").unwrap();
        assert_eq!(s.get("provider:openai").unwrap(), None);
    }

    #[test]
    fn in_memory_set_overwrites_existing() {
        let s = InMemorySecretStore::new();
        s.set("provider:openai", "old").unwrap();
        s.set("provider:openai", "new").unwrap();
        assert_eq!(s.get("provider:openai").unwrap(), Some("new".to_string()));
    }

    #[test]
    fn provider_account_format() {
        assert_eq!(provider_account("anthropic"), "provider:anthropic");
        assert_eq!(provider_account("openrouter"), "provider:openrouter");
    }

    #[test]
    fn delete_missing_account_is_idempotent() {
        let s = InMemorySecretStore::new();
        s.delete("provider:never-existed").unwrap();
    }

    #[test]
    fn distinct_accounts_isolated() {
        let s = InMemorySecretStore::new();
        s.set("provider:anthropic", "ant").unwrap();
        s.set("provider:openai", "oai").unwrap();
        assert_eq!(s.get("provider:anthropic").unwrap().as_deref(), Some("ant"));
        assert_eq!(s.get("provider:openai").unwrap().as_deref(), Some("oai"));
        s.delete("provider:anthropic").unwrap();
        assert_eq!(s.get("provider:anthropic").unwrap(), None);
        assert_eq!(s.get("provider:openai").unwrap().as_deref(), Some("oai"));
    }
}
