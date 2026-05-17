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
    if let Ok(s) = std::env::var(USER_DATA_DIR_ENV) {
        let trimmed = s.trim();
        if !trimmed.is_empty() {
            // Anchor relative overrides to the cwd at startup so a later
            // cwd change can't redirect save I/O (rule #9).
            let p = absolutise(PathBuf::from(trimmed));
            ensure_dir(&p);
            return p;
        }
    }
    let p = platform_data_dir(app_name).unwrap_or_else(|| {
        tracing::warn!(
            "neither PARISH_USER_DATA_DIR nor HOME/APPDATA are set — \
             anchoring user-data dir at the startup cwd as a last resort"
        );
        absolutise(PathBuf::from("."))
    });
    ensure_dir(&p);
    p
}

/// Anchors a relative path to the process's startup-time cwd, so a later
/// `set_current_dir` cannot redirect file I/O. Absolute paths pass through.
pub fn absolutise(p: PathBuf) -> PathBuf {
    if p.is_absolute() {
        p
    } else {
        std::env::current_dir().map(|cwd| cwd.join(&p)).unwrap_or(p)
    }
}

/// Best-effort directory creation with a warning log on failure.
///
/// Called once at startup from sync init paths — matches the convention
/// used by `parish_config::user_config::resolve_user_config_dir`. Resolution
/// runs before any Tokio executor is serving requests, so synchronous
/// `std::fs` here does not block a runtime worker.
fn ensure_dir(p: &std::path::Path) {
    if let Err(e) = std::fs::create_dir_all(p) {
        tracing::warn!(path = %p.display(), error = %e, "failed to create user data directory");
    }
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
    // XDG basedir spec requires absolute paths in XDG_DATA_HOME; reject
    // relative values rather than letting cwd-drift back in (rule #9).
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME").filter(|s| !s.is_empty()) {
        let p = PathBuf::from(xdg);
        if p.is_absolute() {
            return Some(p.join(&leaf));
        }
        tracing::warn!(
            xdg = %p.display(),
            "ignoring relative XDG_DATA_HOME — falling back to $HOME/.local/share"
        );
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

/// Process-wide test-only lock for serialising tests that mutate any of the
/// path-resolution env vars (`PARISH_USER_DATA_DIR`, `PARISH_SAVES_DIR`,
/// `PARISH_TILE_CACHE_DIR`, `HOME`, `XDG_DATA_HOME`, `APPDATA`).
///
/// Cargo runs unit tests within one binary in parallel threads by default;
/// without a single shared lock, picker tests and paths tests race on the
/// same process env. Exposed via `pub(crate)` so the picker test module can
/// share the exact same mutex.
#[cfg(test)]
pub(crate) fn env_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

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
    fn linux_relative_xdg_data_home_falls_back_to_dot_local_share() {
        let _gate = env_test_lock();
        let _restore_root = EnvGuard::capture(USER_DATA_DIR_ENV);
        let _restore_xdg = EnvGuard::capture("XDG_DATA_HOME");
        let _restore_home = EnvGuard::capture("HOME");

        let tmp = TempDir::new().unwrap();
        // SAFETY: gated.
        unsafe {
            std::env::remove_var(USER_DATA_DIR_ENV);
            // Relative XDG_DATA_HOME violates the basedir spec — must be
            // ignored so saves don't end up cwd-relative.
            std::env::set_var("XDG_DATA_HOME", "relative/path");
            std::env::set_var("HOME", tmp.path());
        }

        let resolved = resolve_user_data_dir("Rundale");
        assert_eq!(resolved, tmp.path().join(".local/share/rundale"));
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
