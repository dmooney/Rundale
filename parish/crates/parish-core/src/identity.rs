//! Trait seams for identity and session bookkeeping (#615).
//!
//! `saves/sessions.db` currently conflates two concerns:
//!
//! 1. **Identity** — which OAuth provider identity maps to which in-game
//!    account (`oauth_accounts` table).
//! 2. **Session bookkeeping** — which session cookie is active, when it was
//!    last used, and whether it is eligible for eviction (`sessions` table).
//!
//! Splitting these into two traits (`IdentityStore` and `SessionRegistry`)
//! lets future backends (e.g. a cloud-backed auth service) implement only
//! the interface they need, without owning the full SQLite connection.
//!
//! # Zero-behaviour contract
//!
//! Both traits are implemented by [`SqliteIdentityStore`] and
//! [`SqliteSessionRegistry`] in `parish-server/src/session_store_impl.rs`,
//! which share the same `Arc<Mutex<Connection>>` on `saves/sessions.db`.
//! No schema migrations are required and no observable behaviour changes for
//! the current OAuth or session-lifecycle flows.
//!
//! # `account_id` vs `session_id`
//!
//! The current schema uses `session_id` as the account key.  In the default
//! implementation, `account_id == session_id`.  The trait surface uses
//! `account_id` to leave room for a future schema that decouples accounts
//! from sessions (e.g. an account that may own multiple sessions).

/// Identity management: links OAuth provider credentials to game accounts.
///
/// The default implementation stores rows in the `oauth_accounts` table of
/// `saves/sessions.db`, where `account_id == session_id`.
pub trait IdentityStore: Send + Sync + 'static {
    /// Looks up the game `account_id` linked to an OAuth identity.
    ///
    /// Returns `None` when no link exists (new user or unlinked provider).
    fn lookup_by_provider(&self, provider: &str, provider_user_id: &str) -> Option<String>;

    /// Links (or re-links) an OAuth identity to a game account.
    ///
    /// If a row already exists for `(provider, provider_user_id)`, it is
    /// replaced atomically. This satisfies the "re-link after restore"
    /// flow in `auth.rs` where a stale session is replaced by a live one.
    fn link_provider(
        &self,
        provider: &str,
        provider_user_id: &str,
        account_id: &str,
        display_name: &str,
    );

    /// Returns the `(provider_user_id, display_name)` for the Google account
    /// linked to `account_id`, if any.
    ///
    /// Used by `GET /api/auth/status` to determine logged-in state and the
    /// name to display in the UI.
    fn get_account(&self, account_id: &str) -> Option<(String, String)>;

    /// Records a new account in the persistent store.
    ///
    /// Idempotent: a second call for the same `account_id` is a no-op.
    fn create_account(&self, account_id: &str);
}

/// Session bookkeeping: tracks which session IDs are registered and active.
///
/// The default implementation stores rows in the `sessions` table of
/// `saves/sessions.db`.  The in-memory DashMap cache of live [`SessionEntry`]
/// objects lives in the concrete `SqliteSessionStore` struct in
/// `parish-server/src/session.rs`, not behind this trait, because it carries
/// `JoinHandle`s and `CancellationToken`s that are not persistence concerns.
pub trait SessionRegistry: Send + Sync + 'static {
    /// Returns `true` when `session_id` has a row in the persistent store.
    ///
    /// Used to distinguish "cookie points to a known session" from "never
    /// seen this cookie" at request time.
    fn lookup(&self, session_id: &str) -> bool;

    /// Inserts a new session row into the persistent store.
    ///
    /// Idempotent: a second call for the same `session_id` is a no-op
    /// (`INSERT OR IGNORE`).
    fn register(&self, session_id: &str);

    /// Updates the `last_active` timestamp for `session_id` to the current
    /// wall-clock time.
    fn touch(&self, session_id: &str);

    /// Removes sessions from the **in-memory** cache that have been idle
    /// longer than `max_age`.
    ///
    /// Distinct from [`evict_idle`]: this clears only the runtime map.
    /// Persistent (disk) cleanup is handled by [`evict_idle`].
    fn cleanup_stale(&self, max_age: std::time::Duration);

    /// Purges sessions from both the persistent store **and** their saves
    /// directories when idle longer than `max_age`.
    ///
    /// Returns the count of sessions purged.  Security constraints inherited
    /// from the default implementation:
    /// - Only UUID-shaped IDs (`hex + hyphens`) are ever passed to
    ///   `remove_dir_all` (prevents path traversal).
    /// - After joining with `saves_root`, the resolved path must still be
    ///   under `saves_root` (guard against symlink attacks).
    fn evict_idle(&self, saves_root: &std::path::Path, max_age: std::time::Duration) -> usize;
}
