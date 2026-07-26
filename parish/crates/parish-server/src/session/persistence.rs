//! Session persistence — in-memory registry backed by SQLite sessions.db.
//!
//! Owns [`SessionRegistry`] (CRUD against sessions.db, OAuth linking, stale-session
//! eviction) and the session-ID validation helper [`is_valid_session_id`].

use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;

use crate::session_store_impl::initialize_sessions_schema;

use super::SessionEntry;

// ── SessionRegistry ──────────────────────────────────────────────────────────

/// In-memory session map backed by a SQLite persistence store.
pub struct SessionRegistry {
    pub(super) sessions: DashMap<String, std::sync::Arc<SessionEntry>>,
    pub(super) db: std::sync::Mutex<rusqlite::Connection>,
    /// Serializes cold restore/create admission. Hot in-memory lookups never
    /// take this gate; cold callers recheck after acquiring it.
    pub(super) lifecycle_gate: tokio::sync::Mutex<()>,
    /// Running count of session-creation rejections since process start.
    pub rejection_count: AtomicU64,
}

impl SessionRegistry {
    /// Opens (or creates) `saves/sessions.db` and runs schema migrations.
    pub fn open(saves_dir: &Path) -> rusqlite::Result<Self> {
        let db_path = saves_dir.join("sessions.db");
        let conn = rusqlite::Connection::open(&db_path)?;
        initialize_sessions_schema(&conn)?;
        Ok(Self {
            sessions: DashMap::new(),
            db: std::sync::Mutex::new(conn),
            lifecycle_gate: tokio::sync::Mutex::new(()),
            rejection_count: AtomicU64::new(0),
        })
    }

    /// Returns the number of sessions currently held in memory.
    pub fn active_count(&self) -> usize {
        self.sessions.len()
    }

    /// Returns `true` when the number of live sessions is at or above `cap`.
    ///
    /// Callers should check this before creating a new session and return
    /// `503 Service Unavailable` if it holds. Increments `rejection_count`
    /// when at capacity so callers don't have to manage that separately.
    pub fn is_at_capacity(&self, cap: usize) -> bool {
        let current = self.active_count();
        if current >= cap {
            self.rejection_count.fetch_add(1, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn now_unix() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn now_iso() -> String {
        chrono::Utc::now().to_rfc3339()
    }

    /// Returns `true` if `session_id` is recorded in sessions.db.
    pub fn exists_in_db(&self, session_id: &str) -> bool {
        let db = self.db.lock().unwrap();
        db.query_row("SELECT 1 FROM sessions WHERE id = ?1", [session_id], |_| {
            Ok(())
        })
        .is_ok()
    }

    /// Inserts a new session row into sessions.db and reports persistence
    /// failures to lifecycle callers.
    ///
    /// Cold-session construction uses this fallible form before it starts any
    /// runtime workers. That prevents a process-local session from becoming
    /// live when its durable registry row could not be committed.
    pub(super) fn try_persist_new(&self, session_id: &str) -> rusqlite::Result<()> {
        let now = Self::now_iso();
        let db = self.db.lock().unwrap();
        db.execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, last_active) VALUES (?1, ?2, ?2)",
            rusqlite::params![session_id, now],
        )
        .map(|_| ())
    }

    /// Best-effort compatibility wrapper for legacy callers that already own
    /// a live session. New lifecycle code must use [`Self::try_persist_new`].
    pub fn persist_new(&self, session_id: &str) {
        if let Err(e) = self.try_persist_new(session_id) {
            tracing::warn!(session_id = %session_id, error = %e, "persist_new failed");
        }
    }

    /// Updates the `last_active` timestamp for a session in sessions.db.
    pub fn update_last_active(&self, session_id: &str) {
        let now = Self::now_iso();
        let db = self.db.lock().unwrap();
        if let Err(e) = db.execute(
            "UPDATE sessions SET last_active = ?1 WHERE id = ?2",
            rusqlite::params![now, session_id],
        ) {
            tracing::warn!(session_id = %session_id, error = %e, "update_last_active failed");
        }
    }

    /// Returns the session_id linked to an OAuth identity, if any.
    pub fn find_by_oauth(&self, provider: &str, provider_user_id: &str) -> Option<String> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT session_id FROM oauth_accounts
             WHERE provider = ?1 AND provider_user_id = ?2",
            rusqlite::params![provider, provider_user_id],
            |row| row.get(0),
        )
        .ok()
    }

    /// Associates an OAuth identity with a session_id, storing the user's display name.
    pub fn link_oauth(
        &self,
        provider: &str,
        provider_user_id: &str,
        session_id: &str,
        display_name: &str,
    ) {
        let db = self.db.lock().unwrap();
        match db.execute(
            "INSERT OR REPLACE INTO oauth_accounts
             (provider, provider_user_id, session_id, display_name) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![provider, provider_user_id, session_id, display_name],
        ) {
            Ok(rows) => tracing::info!(
                provider = %provider,
                provider_user_id = %provider_user_id,
                session_id = %session_id,
                display_name = %display_name,
                rows = rows,
                "link_oauth stored account"
            ),
            Err(e) => tracing::error!(
                provider = %provider,
                provider_user_id = %provider_user_id,
                session_id = %session_id,
                error = %e,
                "link_oauth DB write failed"
            ),
        }
    }

    /// Returns a session from the in-memory map.
    pub fn get_in_memory(&self, session_id: &str) -> Option<std::sync::Arc<SessionEntry>> {
        self.sessions
            .get(session_id)
            .map(|e| std::sync::Arc::clone(&*e))
    }

    /// Inserts a session into the in-memory map.
    pub fn insert(&self, session_id: String, entry: std::sync::Arc<SessionEntry>) {
        self.sessions.insert(session_id, entry);
    }

    /// Removes sessions that have been idle longer than `max_age`.
    ///
    /// The sessions' background tick tasks are implicitly cancelled when
    /// their `JoinHandle`s are dropped via the evicted `SessionEntry`.
    pub fn cleanup_stale(&self, max_age: Duration) {
        let cutoff = Self::now_unix().saturating_sub(max_age.as_secs());
        self.sessions
            .retain(|_, entry| entry.last_active.load(Ordering::Relaxed) >= cutoff);
    }

    /// Purges sessions abandoned for longer than `max_age` from disk
    /// (sessions.db row + saves/<session_id>/ directory).
    ///
    /// Distinct from [`cleanup_stale`]: that one only clears the
    /// in-memory map. Disk state is what needs removing here (#482) —
    /// otherwise long-running deployments accumulate dead sessions
    /// forever. `max_age` is expected to be much longer than the
    /// in-memory TTL (e.g. 30 days vs 2 hours) so users can still
    /// restore a session from the cookie on their next visit for
    /// reasonable idle windows.
    ///
    /// Returns the number of sessions purged, so the caller can log
    /// the scope of the sweep.
    pub fn purge_expired_disk_sessions(&self, saves_root: &Path, max_age: Duration) -> usize {
        let cutoff_secs = Self::now_unix().saturating_sub(max_age.as_secs());
        let cutoff = match chrono::DateTime::<chrono::Utc>::from_timestamp(cutoff_secs as i64, 0) {
            Some(dt) => dt.to_rfc3339(),
            None => {
                tracing::warn!(
                    cutoff_secs = cutoff_secs,
                    "purge_expired_disk_sessions: cutoff timestamp out of range, skipping sweep"
                );
                return 0;
            }
        };

        // Find expired session ids + drop their sessions.db rows in a
        // single transaction so the filesystem cleanup below can't get
        // out of sync with the DB if the process dies mid-sweep.
        let expired_ids: Vec<String> = {
            let db = match self.db.lock() {
                Ok(g) => g,
                Err(poisoned) => poisoned.into_inner(),
            };
            let mut collected = Vec::new();
            let select_result = (|| -> rusqlite::Result<()> {
                let mut stmt = db.prepare("SELECT id FROM sessions WHERE last_active < ?1")?;
                let mut rows = stmt.query([&cutoff])?;
                while let Some(row) = rows.next()? {
                    collected.push(row.get::<_, String>(0)?);
                }
                Ok(())
            })();
            if let Err(e) = select_result {
                tracing::warn!(error = %e, "purge_expired_disk_sessions: DB read failed");
                return 0;
            }
            // Drop rows for the ids we collected inside an explicit
            // transaction.  Both DELETEs must commit atomically: if the
            // process crashes between them, oauth_accounts rows would be
            // left pointing at a non-existent session_id, letting the
            // next login for that OAuth identity silently resurrect a
            // ghost session (#593, #482).
            //
            // Invariant: DB rows are deleted *before* filesystem cleanup
            // (see below).  A residual saves/<id>/ directory with no DB
            // row is harmless; an oauth_accounts row pointing at a missing
            // sessions row is not.
            if !collected.is_empty() {
                let tx_result = (|| -> rusqlite::Result<()> {
                    let tx = db.unchecked_transaction()?;
                    let placeholders = vec!["?"; collected.len()].join(",");
                    let params: Vec<&dyn rusqlite::ToSql> = collected
                        .iter()
                        .map(|s| s as &dyn rusqlite::ToSql)
                        .collect();
                    let sql = format!("DELETE FROM sessions WHERE id IN ({placeholders})");
                    tx.execute(&sql, params.as_slice())?;
                    // Also drop oauth links for those sessions — otherwise
                    // the next login for the same provider_user_id would
                    // resurrect a dead session_id. (#482 sibling concern.)
                    let oauth_sql =
                        format!("DELETE FROM oauth_accounts WHERE session_id IN ({placeholders})");
                    tx.execute(&oauth_sql, params.as_slice())?;
                    tx.commit()
                })();
                if let Err(e) = tx_result {
                    tracing::warn!(error = %e, "purge_expired_disk_sessions: DB delete failed");
                    return 0;
                }
            }
            collected
        };

        if expired_ids.is_empty() {
            return 0;
        }

        // Best-effort filesystem cleanup. A failure here is logged but
        // doesn't undo the DB delete — a residual saves/<id>/ directory
        // with no DB row is harmless (eventually reaped by OS-level
        // cleanup or a later sweep once we have directory-age scanning).
        //
        // #595 — Validate each session ID before building a path so that a
        // corrupted or tampered DB row cannot cause remove_dir_all to delete
        // directories outside the saves root.  Two layers of defence:
        //   1. Allowlist check: the ID must consist only of lowercase hex
        //      digits and hyphens (UUID v4 format).  Anything else — including
        //      `..`, `/`, `\`, or unusual chars — is rejected before we even
        //      call Path::join.
        //   2. Containment check: after joining, canonicalize the candidate
        //      path and assert it starts with the canonicalized saves root.
        //      This catches any edge case the regex might miss.
        let canonical_saves_root = match saves_root.canonicalize() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "purge_expired_disk_sessions: cannot canonicalize saves_root, skipping fs cleanup"
                );
                return expired_ids.len();
            }
        };

        for id in &expired_ids {
            // Layer 1: allowlist — UUID v4 looks like
            // `xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx` (hex + hyphens only).
            // Uses the shared `is_valid_session_id` helper (same rule as
            // cookie ingress in `get_or_create_session`).
            if !is_valid_session_id(id) {
                tracing::warn!(
                    session_id = %id,
                    "purge_expired_disk_sessions: rejected unsafe session ID, skipping fs remove"
                );
                continue;
            }

            let session_dir = saves_root.join(id);
            if !session_dir.exists() {
                continue;
            }

            // Layer 2: containment — canonicalize the resolved path and verify
            // it stays inside the saves root (guards against symlink tricks or
            // any bypass of the allowlist above).
            let canonical_dir = match session_dir.canonicalize() {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        session_id = %id,
                        path = %session_dir.display(),
                        error = %e,
                        "purge_expired_disk_sessions: cannot canonicalize session dir, skipping"
                    );
                    continue;
                }
            };
            if !canonical_dir.starts_with(&canonical_saves_root) {
                tracing::warn!(
                    session_id = %id,
                    path = %canonical_dir.display(),
                    saves_root = %canonical_saves_root.display(),
                    "purge_expired_disk_sessions: path escapes saves root, skipping fs remove"
                );
                continue;
            }

            match std::fs::remove_dir_all(&session_dir) {
                Ok(()) => {
                    tracing::info!(
                        session_id = %id,
                        path = %session_dir.display(),
                        "purge_expired_disk_sessions: removed saves directory"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        session_id = %id,
                        path = %session_dir.display(),
                        error = %e,
                        "purge_expired_disk_sessions: failed to remove saves directory"
                    );
                }
            }
        }

        expired_ids.len()
    }
}

// ── Session ID validation ─────────────────────────────────────────────────────

/// Returns `true` when `id` is a structurally valid session ID.
///
/// A valid ID contains only lowercase hex digits and hyphens (the UUID v4
/// character set: `xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx`).  The check also
/// rejects `..` to make it impossible to construct a path-traversal sequence
/// before the value ever reaches `Path::join`.
///
/// This is the single source of truth for session-ID validation; both
/// [`super::lifecycle::get_or_create_session`] (cookie ingress) and
/// [`SessionRegistry::purge_expired_disk_sessions`] (DB-sourced IDs) call
/// this helper.
pub fn is_valid_session_id(id: &str) -> bool {
    !id.is_empty() && !id.contains("..") && id.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use super::{SessionRegistry, is_valid_session_id};

    /// Overwrites sessions.id's last_active to a fixed ISO timestamp so
    /// tests can pin "how idle" a row is without sleeping through the
    /// real retention window.
    fn backdate_session(reg: &SessionRegistry, session_id: &str, last_active_iso: &str) {
        let db = reg.db.lock().unwrap();
        db.execute(
            "UPDATE sessions SET last_active = ?1 WHERE id = ?2",
            rusqlite::params![last_active_iso, session_id],
        )
        .unwrap();
    }

    /// Verifies that a fresh DB round-trips the Google OAuth link:
    /// after `link_oauth`, both `find_by_oauth` and the identity store's
    /// `get_account` return the stored values.
    ///
    /// This is the exact flow the callback + status endpoint use, so if
    /// this test passes but the UI shows the user as signed out, the bug
    /// is elsewhere (cookies, middleware, frontend).
    #[test]
    fn oauth_link_round_trips_on_fresh_db() {
        use parish_core::identity::IdentityStore as _;

        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        reg.persist_new("sess_abc");
        reg.link_oauth("google", "sub_123", "sess_abc", "John Doe");

        assert_eq!(
            reg.find_by_oauth("google", "sub_123"),
            Some("sess_abc".to_string()),
            "find_by_oauth should return the linked session_id"
        );

        let conn = crate::session_store_impl::open_sessions_db(tmp.path()).unwrap();
        let store = crate::session_store_impl::SqliteIdentityStore::new(conn);
        assert_eq!(
            store.get_account("sess_abc"),
            Some(("sub_123".to_string(), "John Doe".to_string())),
            "get_account should return (sub, display_name)"
        );
    }

    /// Verifies the migration from a pre-display_name schema to the
    /// current schema: opening a DB that was created with the old schema
    /// should add the `display_name` column, and subsequent link_oauth
    /// + identity-store `get_account` calls should work end-to-end.
    #[test]
    fn oauth_link_round_trips_on_migrated_db() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("sessions.db");

        // Simulate an existing DB created with the pre-display_name schema.
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE sessions (
                    id TEXT PRIMARY KEY,
                    created_at TEXT NOT NULL,
                    last_active TEXT NOT NULL
                );
                CREATE TABLE oauth_accounts (
                    provider         TEXT NOT NULL,
                    provider_user_id TEXT NOT NULL,
                    session_id       TEXT NOT NULL,
                    PRIMARY KEY (provider, provider_user_id)
                );",
            )
            .unwrap();
            // Insert a row that predates the display_name column.
            conn.execute(
                "INSERT INTO oauth_accounts (provider, provider_user_id, session_id) \
                 VALUES ('google', 'legacy_sub', 'legacy_sess')",
                [],
            )
            .unwrap();
        }

        // Re-open through SessionRegistry — this should ADD COLUMN display_name.
        let reg = SessionRegistry::open(tmp.path()).unwrap();

        let conn = crate::session_store_impl::open_sessions_db(tmp.path()).unwrap();
        let store = crate::session_store_impl::SqliteIdentityStore::new(conn);

        use parish_core::identity::IdentityStore as _;

        // Legacy row has empty display_name (default).
        assert_eq!(
            store.get_account("legacy_sess"),
            Some(("legacy_sub".to_string(), String::new())),
        );

        // New link writes the display_name column correctly.
        reg.persist_new("sess_new");
        reg.link_oauth("google", "sub_new", "sess_new", "Jane Doe");
        assert_eq!(
            store.get_account("sess_new"),
            Some(("sub_new".to_string(), "Jane Doe".to_string())),
        );
    }

    // ── #466 gossip budget round-robin ──────────────────────────────────────
    //
    // The budgeting math (`budgeted_round_robin`) and its unit tests moved to
    // `parish_core::game_loop::world_pump` in #1159 — the server now drives the
    // single shared `advance_world` pump with `GossipMode::Budgeted`.

    // ── #482 disk-session purge ─────────────────────────────────────────────

    #[test]
    fn purge_expired_removes_old_row_and_save_dir() {
        // Use a valid UUID v4 format ID — the #595 path-traversal guard
        // requires session IDs to be hex+hyphen only (matching UUID v4).
        let expired_id = "e1111111-1111-4111-a111-111111111111";
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        reg.persist_new(expired_id);
        // Fresh row + fake saves/<id>/ directory.
        let save_dir = tmp.path().join(expired_id);
        std::fs::create_dir_all(&save_dir).unwrap();
        std::fs::write(save_dir.join("parish_001.db"), b"fake").unwrap();
        // Backdate to 90 days ago so any reasonable retention sweep
        // picks it up.
        let old = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        backdate_session(&reg, expired_id, &old);

        let purged = reg.purge_expired_disk_sessions(tmp.path(), Duration::from_secs(30 * 86_400));
        assert_eq!(purged, 1);
        assert!(!reg.exists_in_db(expired_id));
        assert!(
            !save_dir.exists(),
            "saves directory must be deleted after purge"
        );
    }

    #[test]
    fn purge_expired_preserves_recent_sessions() {
        // Use a valid UUID v4 format ID — the #595 path-traversal guard
        // requires session IDs to be hex+hyphen only (matching UUID v4).
        let recent_id = "ece11111-1111-4111-a111-111111111111";
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        reg.persist_new(recent_id);
        let save_dir = tmp.path().join(recent_id);
        std::fs::create_dir_all(&save_dir).unwrap();

        // last_active set to `now` by persist_new — well inside the
        // 30-day retention window.
        let purged = reg.purge_expired_disk_sessions(tmp.path(), Duration::from_secs(30 * 86_400));
        assert_eq!(purged, 0);
        assert!(reg.exists_in_db(recent_id));
        assert!(save_dir.exists());
    }

    #[test]
    fn purge_expired_drops_linked_oauth_rows() {
        // Use a valid UUID v4 format ID — the #595 path-traversal guard
        // requires session IDs to be hex+hyphen only (matching UUID v4).
        let expired_linked_id = "e1111111-1111-4111-a111-111111111112";
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        reg.persist_new(expired_linked_id);
        reg.link_oauth("google", "sub_legacy", expired_linked_id, "Old User");
        let old = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        backdate_session(&reg, expired_linked_id, &old);

        let purged = reg.purge_expired_disk_sessions(tmp.path(), Duration::from_secs(30 * 86_400));
        assert_eq!(purged, 1);
        // The OAuth link is gone too — otherwise a fresh login for
        // `sub_legacy` would resurrect a dead session_id with no DB row.
        assert_eq!(reg.find_by_oauth("google", "sub_legacy"), None);
    }

    #[test]
    fn purge_expired_handles_missing_save_dir_gracefully() {
        // Use a valid UUID v4 format ID — the #595 path-traversal guard
        // requires session IDs to be hex+hyphen only (matching UUID v4).
        let ghost_id = "abb51111-1111-4111-a111-111111111111";
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        reg.persist_new(ghost_id);
        // No saves/<id>/ directory was ever created. Purge must still
        // delete the DB row and return 1 — filesystem absence is fine.
        let old = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        backdate_session(&reg, ghost_id, &old);

        let purged = reg.purge_expired_disk_sessions(tmp.path(), Duration::from_secs(30 * 86_400));
        assert_eq!(purged, 1);
        assert!(!reg.exists_in_db(ghost_id));
    }

    // ── #595 path traversal guard ────────────────────────────────────────────

    /// A session ID containing `..` must not cause `remove_dir_all` to
    /// operate outside the saves root.  The traversal ID is rejected before
    /// the filesystem is touched; a sibling directory must survive intact.
    #[test]
    fn purge_expired_rejects_path_traversal_id() {
        let outer = tempfile::tempdir().unwrap();
        // The "saves root" lives one level below outer so there is a parent
        // directory to try to traverse into.
        let saves_root = outer.path().join("saves");
        std::fs::create_dir_all(&saves_root).unwrap();

        // A sibling directory that a traversal payload would try to delete.
        let sibling = outer.path().join("sensitive");
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("secret.txt"), b"do not delete").unwrap();

        // Set up a SessionRegistry using saves_root as the root.
        let reg = SessionRegistry::open(&saves_root).unwrap();

        // Directly insert a row with a traversal ID (bypassing the normal
        // UUID generation path to simulate a tampered/corrupted DB).
        {
            let db = reg.db.lock().unwrap();
            db.execute(
                "INSERT INTO sessions (id, created_at, last_active) VALUES (?1, ?2, ?2)",
                rusqlite::params!["../sensitive", "2000-01-01T00:00:00Z"],
            )
            .unwrap();
        }

        // Create a fake directory at saves_root/../sensitive to give
        // remove_dir_all something to hit if the guard fails.
        // (sibling already exists above — that's the target.)

        let purged = reg
            .purge_expired_disk_sessions(&saves_root, Duration::from_secs(0 /* always expired */));

        // The DB row is deleted (purge still counts it).
        assert_eq!(purged, 1);
        // The sibling directory must NOT have been removed.
        assert!(
            sibling.exists(),
            "path traversal guard must prevent deletion of directories outside saves root"
        );
        assert!(
            sibling.join("secret.txt").exists(),
            "sensitive file must survive"
        );
    }

    /// IDs with non-hex/non-hyphen characters (including `/` and `\`) are
    /// rejected by the allowlist even if they don't look like `..` traversals.
    #[test]
    fn purge_expired_rejects_ids_with_unsafe_characters() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();

        // Directly insert rows with unsafe IDs.
        let unsafe_ids = [
            "../../etc/passwd",
            "foo/bar",
            "foo\\bar",
            "abc def",
            "abc\0def",
        ];
        {
            let db = reg.db.lock().unwrap();
            for id in &unsafe_ids {
                db.execute(
                    "INSERT INTO sessions (id, created_at, last_active) VALUES (?1, ?2, ?2)",
                    rusqlite::params![id, "2000-01-01T00:00:00Z"],
                )
                .unwrap();
            }
        }

        // None of these should cause a panic or an out-of-root deletion.
        let purged = reg.purge_expired_disk_sessions(tmp.path(), Duration::from_secs(0));
        // All rows are deleted from the DB.
        assert_eq!(purged, unsafe_ids.len());
        // The saves root itself is intact.
        assert!(tmp.path().exists(), "saves root must still exist");
    }

    /// A well-formed UUID session ID must still be cleaned up normally —
    /// the path-traversal guard must not break the happy path.
    #[test]
    fn purge_expired_uuid_id_still_cleaned_up() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        let id = "a1b2c3d4-e5f6-4789-abcd-ef0123456789";
        reg.persist_new(id);
        let save_dir = tmp.path().join(id);
        std::fs::create_dir_all(&save_dir).unwrap();

        let old = (chrono::Utc::now() - chrono::Duration::days(90)).to_rfc3339();
        backdate_session(&reg, id, &old);

        let purged = reg.purge_expired_disk_sessions(tmp.path(), Duration::from_secs(30 * 86_400));
        assert_eq!(purged, 1);
        assert!(!save_dir.exists(), "save directory must be removed");
    }

    // ── Admission control unit tests (#620) ──────────────────────────────────

    /// `active_count` reflects the current in-memory session count.
    #[test]
    fn active_count_reflects_in_memory_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        assert_eq!(reg.active_count(), 0);

        // Insert a dummy entry — Arc::new with a dummy SessionEntry requires
        // fields we can't easily construct here, so just verify the count at 0.
        // The is_at_capacity / active_count pairing is tested in the next test.
        assert_eq!(
            reg.active_count(),
            0,
            "fresh registry must report 0 sessions"
        );
    }

    /// `is_at_capacity` returns false when under the cap and true at/above it.
    #[test]
    fn is_at_capacity_returns_false_below_cap_true_at_cap() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();

        // With 0 sessions and cap=1, not at capacity.
        assert!(
            !reg.is_at_capacity(1),
            "0 sessions, cap=1: should not be at capacity"
        );
        // rejection_count must not have incremented.
        assert_eq!(reg.rejection_count.load(Ordering::Relaxed), 0);

        // Artificially reach the cap by checking is_at_capacity(0): cap=0 means
        // every new attempt is rejected.
        assert!(
            reg.is_at_capacity(0),
            "0 sessions, cap=0: should be at capacity"
        );
        assert_eq!(
            reg.rejection_count.load(Ordering::Relaxed),
            1,
            "rejection_count must increment"
        );

        // A second rejection increments again.
        assert!(reg.is_at_capacity(0));
        assert_eq!(reg.rejection_count.load(Ordering::Relaxed), 2);
    }

    /// `rejection_count` is not incremented for calls that are under the cap.
    #[test]
    fn rejection_count_not_incremented_below_cap() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        // active_count() == 0 < cap=10 → no rejection
        let _ = reg.is_at_capacity(10);
        let _ = reg.is_at_capacity(10);
        assert_eq!(reg.rejection_count.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn is_valid_session_id_accepts_uuid_v4() {
        assert!(is_valid_session_id("a1b2c3d4-e5f6-4789-abcd-ef0123456789"));
    }

    #[test]
    fn is_valid_session_id_rejects_traversal() {
        assert!(!is_valid_session_id("../etc/passwd"));
        assert!(!is_valid_session_id("foo/bar"));
        assert!(!is_valid_session_id(""));
    }
}
