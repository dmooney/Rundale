use std::collections::HashMap;
use std::sync::Mutex;

use parish_core::identity::{IdentityStore, SessionRegistry};

/// In-memory mock of [`IdentityStore`] for contract testing.
///
/// Stores OAuth links as `(provider, provider_user_id) → account_id` and
/// account display names as `account_id → (provider_user_id, display_name)`.
struct MockIdentityStore {
    links: Mutex<HashMap<(String, String), String>>,
    accounts: Mutex<HashMap<String, (String, String)>>,
}

impl MockIdentityStore {
    fn new() -> Self {
        Self {
            links: Mutex::new(HashMap::new()),
            accounts: Mutex::new(HashMap::new()),
        }
    }
}

impl IdentityStore for MockIdentityStore {
    fn lookup_by_provider(&self, provider: &str, provider_user_id: &str) -> Option<String> {
        self.links
            .lock()
            .unwrap()
            .get(&(provider.to_string(), provider_user_id.to_string()))
            .cloned()
    }

    fn link_provider(
        &self,
        provider: &str,
        provider_user_id: &str,
        account_id: &str,
        display_name: &str,
    ) {
        self.links.lock().unwrap().insert(
            (provider.to_string(), provider_user_id.to_string()),
            account_id.to_string(),
        );
        self.accounts.lock().unwrap().insert(
            account_id.to_string(),
            (provider_user_id.to_string(), display_name.to_string()),
        );
    }

    fn get_account(&self, account_id: &str) -> Option<(String, String)> {
        self.accounts.lock().unwrap().get(account_id).cloned()
    }

    fn create_account(&self, account_id: &str) {
        // In the real implementation this inserts a session row.
        // For the mock, we just ensure the account exists in the accounts map
        // if not already present.
        self.accounts
            .lock()
            .unwrap()
            .entry(account_id.to_string())
            .or_insert_with(|| ("".to_string(), "".to_string()));
    }
}

/// In-memory mock of [`SessionRegistry`] for contract testing.
struct MockSessionRegistry {
    sessions: Mutex<HashMap<String, bool>>,
}

impl MockSessionRegistry {
    fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
        }
    }
}

impl SessionRegistry for MockSessionRegistry {
    fn lookup(&self, session_id: &str) -> bool {
        self.sessions
            .lock()
            .unwrap()
            .get(session_id)
            .copied()
            .unwrap_or(false)
    }

    fn register(&self, session_id: &str) {
        self.sessions
            .lock()
            .unwrap()
            .insert(session_id.to_string(), true);
    }

    fn touch(&self, _session_id: &str) {
        // In-memory: no-op, timestamp tracking is a real-DB concern.
    }

    fn cleanup_stale(&self, _max_age: std::time::Duration) {
        // In-memory: no-op.
    }

    fn evict_idle(&self, _saves_root: &std::path::Path, _max_age: std::time::Duration) -> usize {
        // In-memory: no-op, always returns 0.
        0
    }
}

// ── IdentityStore contract tests ───────────────────────────────────────────

#[test]
fn identity_lookup_by_provider_returns_none_for_unknown() {
    let store = MockIdentityStore::new();
    assert_eq!(store.lookup_by_provider("google", "unknown_sub"), None);
}

#[test]
fn identity_link_and_lookup_by_provider_roundtrip() {
    let store = MockIdentityStore::new();
    store.link_provider("google", "sub_abc", "sess_001", "Alice");
    assert_eq!(
        store.lookup_by_provider("google", "sub_abc"),
        Some("sess_001".to_string())
    );
}

#[test]
fn identity_link_replaces_existing_mapping() {
    let store = MockIdentityStore::new();
    store.link_provider("google", "sub_abc", "sess_001", "Alice");
    store.link_provider("google", "sub_abc", "sess_002", "Alice v2");
    assert_eq!(
        store.lookup_by_provider("google", "sub_abc"),
        Some("sess_002".to_string()),
        "link_provider should replace existing mapping"
    );
}

#[test]
fn identity_get_account_returns_none_for_unknown() {
    let store = MockIdentityStore::new();
    assert_eq!(store.get_account("no_such_account"), None);
}

#[test]
fn identity_get_account_returns_linked_info() {
    let store = MockIdentityStore::new();
    store.link_provider("google", "sub_def", "sess_002", "Brigid");
    let account = store.get_account("sess_002");
    assert_eq!(account, Some(("sub_def".to_string(), "Brigid".to_string())));
}

#[test]
fn identity_create_account_preserves_existing() {
    let store = MockIdentityStore::new();
    store.link_provider("google", "sub_ghi", "sess_003", "Cillian");
    store.create_account("sess_003");
    let account = store.get_account("sess_003");
    assert!(
        account.is_some(),
        "create_account must not erase existing data"
    );
}

#[test]
fn identity_multiple_providers_can_link_to_same_account() {
    let store = MockIdentityStore::new();
    store.link_provider("google", "sub_a", "sess_001", "Alice");
    store.link_provider("discord", "user_b", "sess_001", "Alice");
    assert_eq!(
        store.lookup_by_provider("google", "sub_a"),
        Some("sess_001".to_string())
    );
    assert_eq!(
        store.lookup_by_provider("discord", "user_b"),
        Some("sess_001".to_string())
    );
}

#[test]
fn identity_different_accounts_have_independent_provider_mappings() {
    let store = MockIdentityStore::new();
    store.link_provider("google", "sub_x", "sess_x", "Xavier");
    store.link_provider("google", "sub_y", "sess_y", "Yvonne");
    assert_eq!(
        store.lookup_by_provider("google", "sub_x"),
        Some("sess_x".to_string())
    );
    assert_eq!(
        store.lookup_by_provider("google", "sub_y"),
        Some("sess_y".to_string())
    );
}

// ── SessionRegistry contract tests ─────────────────────────────────────────

#[test]
fn session_lookup_returns_false_for_unknown() {
    let registry = MockSessionRegistry::new();
    assert!(!registry.lookup("unknown_session"));
}

#[test]
fn session_register_and_lookup_roundtrip() {
    let registry = MockSessionRegistry::new();
    registry.register("sess_001");
    assert!(registry.lookup("sess_001"));
}

#[test]
fn session_register_is_idempotent() {
    let registry = MockSessionRegistry::new();
    registry.register("sess_001");
    registry.register("sess_001");
    assert!(registry.lookup("sess_001"));
}

#[test]
fn session_multiple_registrations_are_independent() {
    let registry = MockSessionRegistry::new();
    registry.register("sess_001");
    registry.register("sess_002");
    assert!(registry.lookup("sess_001"));
    assert!(registry.lookup("sess_002"));
}

#[test]
fn session_touch_does_not_panic() {
    let registry = MockSessionRegistry::new();
    registry.register("sess_001");
    registry.touch("sess_001");
    registry.touch("unknown_session");
    assert!(registry.lookup("sess_001"));
}

#[test]
fn session_cleanup_stale_does_not_panic() {
    let registry = MockSessionRegistry::new();
    registry.register("sess_001");
    registry.cleanup_stale(std::time::Duration::from_secs(3600));
    assert!(registry.lookup("sess_001"));
}

#[test]
fn session_evict_idle_returns_zero_for_in_memory() {
    let registry = MockSessionRegistry::new();
    registry.register("sess_001");
    let tmp = tempfile::tempdir().unwrap();
    let evicted = registry.evict_idle(tmp.path(), std::time::Duration::from_secs(1));
    assert_eq!(evicted, 0);
    assert!(registry.lookup("sess_001"));
}

#[test]
fn session_unregistered_session_not_found() {
    let registry = MockSessionRegistry::new();
    registry.register("sess_001");
    assert!(
        !registry.lookup("sess_002"),
        "different session must not be found"
    );
}
