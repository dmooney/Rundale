//! Per-user data directory resolution for saves + tile cache.
//!
//! Saves and the map tile cache co-locate under a single platform-native
//! user-data root, named after the active mod's `app_name` (Rundale →
//! `Rundale`). Resolution order (Rule 9: explicit, not cwd-derived):
//!
//!   1. `PARISH_USER_DATA_DIR` env var (test isolation + ops override)
//!   2. macOS:   `$HOME/Library/Application Support/<app_name>`
//!      Linux:   `$XDG_DATA_HOME/<app_name_lower>` (fallback `$HOME/.local/share/<app_name_lower>`)
//!      Windows: `%APPDATA%\<app_name>`
//!   3. `./` fallback (only if HOME/APPDATA are missing — degenerate environment)
//!
//! Resolved once at startup and stored on `AppState` / `GlobalState`. Never
//! call from a request handler.

use std::path::PathBuf;

/// Environment variable that overrides user-data-dir resolution.
pub const USER_DATA_DIR_ENV: &str = "PARISH_USER_DATA_DIR";

/// Fallback app name when no mod is loaded (engine-only run).
pub const DEFAULT_APP_NAME: &str = "Parish";

/// Resolves the per-user data directory for the given app name. Creates it if missing.
///
/// `app_name` typically comes from `ModMeta::app_name()` (which honours an
/// explicit `save_root` field on `mod.toml`, falling back to `name`). For
/// engine-only runs with no mod loaded, pass [`DEFAULT_APP_NAME`].
pub fn resolve_user_data_dir(app_name: &str) -> PathBuf {
    if let Ok(s) = std::env::var(USER_DATA_DIR_ENV)
        && !s.trim().is_empty()
    {
        let p = PathBuf::from(s);
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    let p = platform_data_dir(app_name).unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&p);
    p
}

#[cfg(target_os = "macos")]
fn platform_data_dir(app_name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library/Application Support")
            .join(app_name),
    )
}

#[cfg(target_os = "linux")]
fn platform_data_dir(app_name: &str) -> Option<PathBuf> {
    let leaf = app_name.to_lowercase();
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(xdg).join(&leaf));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".local/share").join(&leaf))
}

#[cfg(target_os = "windows")]
fn platform_data_dir(app_name: &str) -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join(app_name))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn platform_data_dir(app_name: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    let leaf = app_name.to_lowercase();
    Some(PathBuf::from(home).join(".local/share").join(&leaf))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Process-wide gate that serialises tests which mutate
    /// [`USER_DATA_DIR_ENV`] (and platform-specific HOME-like vars).
    fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    struct EnvGuard {
        key: &'static str,
        prev: Option<std::ffi::OsString>,
    }
    impl EnvGuard {
        fn capture(key: &'static str) -> Self {
            EnvGuard {
                key,
                prev: std::env::var_os(key),
            }
        }
    }
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            // SAFETY: env mutation is gated by `env_test_lock`.
            match self.prev.take() {
                Some(v) => unsafe { std::env::set_var(self.key, v) },
                None => unsafe { std::env::remove_var(self.key) },
            }
        }
    }

    #[test]
    fn env_override_wins_and_creates_dir() {
        let _gate = env_test_lock();
        let _restore = EnvGuard::capture(USER_DATA_DIR_ENV);

        let tmp = TempDir::new().unwrap();
        let explicit = tmp.path().join("custom_root");
        // SAFETY: env mutation is gated by `env_test_lock`.
        unsafe { std::env::set_var(USER_DATA_DIR_ENV, &explicit) };

        let resolved = resolve_user_data_dir("Rundale");
        assert_eq!(resolved, explicit);
        assert!(resolved.is_absolute());
        assert!(resolved.is_dir());
    }

    #[test]
    fn empty_env_var_is_ignored() {
        let _gate = env_test_lock();
        let _restore = EnvGuard::capture(USER_DATA_DIR_ENV);

        // SAFETY: env mutation is gated by `env_test_lock`.
        unsafe { std::env::set_var(USER_DATA_DIR_ENV, "   ") };

        // Should fall through to platform_data_dir (or "." in degenerate envs).
        let resolved = resolve_user_data_dir("Rundale");
        assert!(resolved.is_dir());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_path_under_application_support() {
        let _gate = env_test_lock();
        let _restore_root = EnvGuard::capture(USER_DATA_DIR_ENV);
        let _restore_home = EnvGuard::capture("HOME");

        let tmp = TempDir::new().unwrap();
        // SAFETY: gated.
        unsafe {
            std::env::remove_var(USER_DATA_DIR_ENV);
            std::env::set_var("HOME", tmp.path());
        }

        let resolved = resolve_user_data_dir("Rundale");
        assert_eq!(
            resolved,
            tmp.path().join("Library/Application Support/Rundale")
        );
        assert!(resolved.is_dir());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_path_uses_xdg_data_home_when_set() {
        let _gate = env_test_lock();
        let _restore_root = EnvGuard::capture(USER_DATA_DIR_ENV);
        let _restore_xdg = EnvGuard::capture("XDG_DATA_HOME");

        let tmp = TempDir::new().unwrap();
        // SAFETY: gated.
        unsafe {
            std::env::remove_var(USER_DATA_DIR_ENV);
            std::env::set_var("XDG_DATA_HOME", tmp.path());
        }

        let resolved = resolve_user_data_dir("Rundale");
        assert_eq!(resolved, tmp.path().join("rundale"));
        assert!(resolved.is_dir());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_fallback_under_dot_local_share() {
        let _gate = env_test_lock();
        let _restore_root = EnvGuard::capture(USER_DATA_DIR_ENV);
        let _restore_xdg = EnvGuard::capture("XDG_DATA_HOME");
        let _restore_home = EnvGuard::capture("HOME");

        let tmp = TempDir::new().unwrap();
        // SAFETY: gated.
        unsafe {
            std::env::remove_var(USER_DATA_DIR_ENV);
            std::env::remove_var("XDG_DATA_HOME");
            std::env::set_var("HOME", tmp.path());
        }

        let resolved = resolve_user_data_dir("Rundale");
        assert_eq!(resolved, tmp.path().join(".local/share/rundale"));
        assert!(resolved.is_dir());
    }
}
