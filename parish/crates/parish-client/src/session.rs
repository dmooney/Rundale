//! Persistent session-cookie storage for the thin CLI client.
//!
//! The server issues a `parish_sid` cookie; the CLI persists it between
//! invocations so a `parish "look"` followed by `parish "go north"` reuse the
//! same server-side session.
//!
//! ## Path resolution (rule #9)
//!
//! The cookie file lives under the shared per-user data root resolved by
//! [`parish_persistence::paths::resolve_user_data_dir`], the same helper the
//! engine, server, and Tauri entry points use for saves + tile cache. That
//! routes the previously ad-hoc `$HOME/parish/session` fallback through the
//! canonical resolver, which:
//!
//! * honours the `PARISH_USER_DATA_DIR` env override (test isolation + ops),
//! * uses the platform-native data dir (`~/Library/Application Support/Parish`
//!   on macOS, `$XDG_DATA_HOME/parish` on Linux, `%APPDATA%\Parish` on
//!   Windows), and
//! * only falls back to a degenerate cwd-relative path when neither the env
//!   override nor `HOME`/`APPDATA` is set.
//!
//! The CLI is mod-agnostic, so it resolves under the engine fallback app name
//! ([`parish_persistence::paths::DEFAULT_APP_NAME`] = `"Parish"`); the cookie
//! file is `<user-data-dir>/session`.

use std::fs;
use std::path::PathBuf;

use parish_persistence::paths::{DEFAULT_APP_NAME, resolve_user_data_dir};

/// Leaf filename of the persisted session cookie, under the per-user data dir.
const SESSION_FILE: &str = "session";

/// Resolves the path to the persisted session-cookie file.
///
/// Routed through [`resolve_user_data_dir`] so the location matches the rest of
/// the engine's per-user state and honours `PARISH_USER_DATA_DIR` (rule #9).
fn session_path() -> PathBuf {
    resolve_user_data_dir(DEFAULT_APP_NAME).join(SESSION_FILE)
}

/// Loads the persisted session id, if any. Returns `None` when the file is
/// absent, empty, or whitespace-only.
pub fn load() -> Option<String> {
    let path = session_path();
    fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Persists the session id, creating the per-user data directory if needed.
pub fn save(sid: &str) -> std::io::Result<()> {
    let path = session_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, sid)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    /// Serialises tests that mutate `PARISH_USER_DATA_DIR` so they do not race
    /// on the process-wide environment.
    fn env_lock() -> MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    struct EnvGuard {
        prev: Option<std::ffi::OsString>,
    }
    impl EnvGuard {
        fn set(dir: &std::path::Path) -> Self {
            let prev = std::env::var_os("PARISH_USER_DATA_DIR");
            // SAFETY: env mutation is serialised by `env_lock`.
            unsafe { std::env::set_var("PARISH_USER_DATA_DIR", dir) };
            Self { prev }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: env mutation is serialised by `env_lock`.
            match self.prev.take() {
                Some(v) => unsafe { std::env::set_var("PARISH_USER_DATA_DIR", v) },
                None => unsafe { std::env::remove_var("PARISH_USER_DATA_DIR") },
            }
        }
    }

    // AC-15/AC-16: the env override routes resolution through the persistence
    // helper, and a save→load round-trip works through the real resolved path
    // (not a hand-written file path) — the branch that was previously untested.
    #[test]
    fn save_load_round_trip_honours_env_override() {
        let _gate = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let _restore = EnvGuard::set(tmp.path());

        // The resolved path must live under the override dir.
        let path = session_path();
        assert!(
            path.starts_with(tmp.path()),
            "session path {} should resolve under the PARISH_USER_DATA_DIR override {}",
            path.display(),
            tmp.path().display()
        );

        assert_eq!(load(), None, "no session file yet");
        save("sid-abc-123").expect("save should create dirs and write");
        assert_eq!(load(), Some("sid-abc-123".to_string()));
    }

    // AC-16: an empty / whitespace-only file resolves to None.
    #[test]
    fn empty_and_whitespace_files_load_as_none() {
        let _gate = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let _restore = EnvGuard::set(tmp.path());

        save("   \n\t ").expect("save should write whitespace");
        assert_eq!(load(), None, "whitespace-only session must be None");

        save("").expect("save should write empty");
        assert_eq!(load(), None, "empty session must be None");
    }

    // AC-16: load returns None when the file does not exist.
    #[test]
    fn missing_file_loads_as_none() {
        let _gate = env_lock();
        let tmp = tempfile::tempdir().unwrap();
        let _restore = EnvGuard::set(tmp.path());
        assert_eq!(load(), None);
    }
}
