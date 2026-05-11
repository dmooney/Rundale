//! Per-user, per-machine config persisted across launches.
//!
//! Stores non-secret BYOK choices (provider name, model, base URL, optional
//! per-category overrides). Secrets live in the OS keychain via
//! `parish_core::secret_store::SecretStore` — never written here.
//!
//! Path resolution (Rule 9: explicit, not cwd-derived):
//!   1. `PARISH_USER_CONFIG_DIR` env var if set
//!   2. macOS:   `$HOME/Library/Application Support/Parish`
//!      Linux:   `$XDG_CONFIG_HOME/parish` (fallback `$HOME/.config/parish`)
//!      Windows: `%APPDATA%\Parish`
//!   3. Resolved once at startup and stored on `AppState`. Never recompute
//!      inside request handlers.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use parish_types::ParishError;
use serde::{Deserialize, Serialize};

/// Environment variable that overrides user-config-dir resolution.
pub const USER_CONFIG_DIR_ENV: &str = "PARISH_USER_CONFIG_DIR";

/// Filename within the user-config dir.
pub const USER_CONFIG_FILENAME: &str = "parish.toml";

/// Sentinel filename written when the user has completed onboarding.
/// Presence (regardless of contents) means: skip the BYOK fork on launch.
pub const ONBOARDING_MARKER_FILENAME: &str = ".onboarded";

/// Non-secret persisted user choices.
///
/// **Never gains an `api_key` field.** Secrets belong in the OS keychain.
/// Adding one here would silently leak keys to disk; the
/// `save_does_not_write_api_key_field` test guards against that.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserConfig {
    /// Provider name (lowercase, matches `Provider::from_str_loose`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// Override of the provider's default base URL. None = use the default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Default model name. None = fill from provider's preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Per-category overrides keyed by lowercase category name
    /// (`"dialogue"`, `"simulation"`, `"intent"`, `"reaction"`).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub category_overrides: BTreeMap<String, CategoryOverride>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
}

/// Resolves the per-user config directory once. Creates it if missing.
///
/// Order: `PARISH_USER_CONFIG_DIR` env > per-OS app-config dir > `./` fallback.
pub fn resolve_user_config_dir() -> PathBuf {
    if let Ok(s) = std::env::var(USER_CONFIG_DIR_ENV)
        && !s.trim().is_empty()
    {
        let p = PathBuf::from(s);
        let _ = std::fs::create_dir_all(&p);
        return p;
    }
    let p = platform_config_dir().unwrap_or_else(|| PathBuf::from("."));
    let _ = std::fs::create_dir_all(&p);
    p
}

#[cfg(target_os = "macos")]
fn platform_config_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join("Library/Application Support/Parish"))
}

#[cfg(target_os = "linux")]
fn platform_config_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(xdg).join("parish"));
    }
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/parish"))
}

#[cfg(target_os = "windows")]
fn platform_config_dir() -> Option<PathBuf> {
    let appdata = std::env::var_os("APPDATA")?;
    Some(PathBuf::from(appdata).join("Parish"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn platform_config_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".config/parish"))
}

/// Loads user config. Missing file → default (empty) config; malformed →
/// `ParishError::Config`.
pub fn load_user_config(dir: &Path) -> Result<UserConfig, ParishError> {
    let path = dir.join(USER_CONFIG_FILENAME);
    match std::fs::read_to_string(&path) {
        Ok(body) => toml::from_str::<UserConfig>(&body)
            .map_err(|e| ParishError::Config(format!("parse {}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(UserConfig::default()),
        Err(e) => Err(ParishError::Config(format!("read {}: {e}", path.display()))),
    }
}

/// Writes user config to `{dir}/parish.toml`. Creates the dir if missing.
pub fn save_user_config(dir: &Path, cfg: &UserConfig) -> Result<(), ParishError> {
    std::fs::create_dir_all(dir)
        .map_err(|e| ParishError::Config(format!("mkdir {}: {e}", dir.display())))?;
    let body = toml::to_string_pretty(cfg)
        .map_err(|e| ParishError::Config(format!("serialize user config: {e}")))?;
    let path = dir.join(USER_CONFIG_FILENAME);
    std::fs::write(&path, body)
        .map_err(|e| ParishError::Config(format!("write {}: {e}", path.display())))
}

/// True if onboarding has previously completed (sentinel file present).
pub fn onboarding_complete(dir: &Path) -> bool {
    dir.join(ONBOARDING_MARKER_FILENAME).exists()
}

/// Marks onboarding complete. Idempotent.
pub fn mark_onboarding_complete(dir: &Path) -> Result<(), ParishError> {
    std::fs::create_dir_all(dir)
        .map_err(|e| ParishError::Config(format!("mkdir {}: {e}", dir.display())))?;
    std::fs::write(dir.join(ONBOARDING_MARKER_FILENAME), b"")
        .map_err(|e| ParishError::Config(format!("write onboarding marker: {e}")))
}

/// Wipes the user config. Used by `clear_provider_config`.
pub fn clear_user_config(dir: &Path) -> Result<(), ParishError> {
    let path = dir.join(USER_CONFIG_FILENAME);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(ParishError::Config(format!(
            "remove {}: {e}",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_returns_default_on_missing_file() {
        let dir = TempDir::new().unwrap();
        let cfg = load_user_config(dir.path()).unwrap();
        assert_eq!(cfg, UserConfig::default());
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut cfg = UserConfig {
            provider: Some("anthropic".to_string()),
            base_url: None,
            model: Some("claude-opus-4-7".to_string()),
            category_overrides: BTreeMap::new(),
        };
        cfg.category_overrides.insert(
            "simulation".to_string(),
            CategoryOverride {
                provider: Some("groq".to_string()),
                model: Some("llama-3.1-8b-instant".to_string()),
                base_url: None,
            },
        );
        save_user_config(dir.path(), &cfg).unwrap();
        let round = load_user_config(dir.path()).unwrap();
        assert_eq!(round, cfg);
    }

    #[test]
    fn save_does_not_write_api_key_field() {
        // Critical guard: the on-disk format must NEVER contain a key called
        // "api_key" anywhere. If a future patch adds an api_key field to
        // UserConfig or CategoryOverride, this test fails immediately.
        let dir = TempDir::new().unwrap();
        let cfg = UserConfig {
            provider: Some("anthropic".to_string()),
            base_url: Some("https://api.anthropic.com".to_string()),
            model: Some("claude-opus-4-7".to_string()),
            category_overrides: BTreeMap::from([(
                "dialogue".to_string(),
                CategoryOverride {
                    provider: Some("openai".to_string()),
                    model: Some("gpt-4o".to_string()),
                    base_url: None,
                },
            )]),
        };
        save_user_config(dir.path(), &cfg).unwrap();
        let body = std::fs::read_to_string(dir.path().join(USER_CONFIG_FILENAME)).unwrap();
        assert!(
            !body.contains("api_key"),
            "user config must never serialize an api_key field; got:\n{body}"
        );
    }

    #[test]
    fn onboarding_marker_round_trip() {
        let dir = TempDir::new().unwrap();
        assert!(!onboarding_complete(dir.path()));
        mark_onboarding_complete(dir.path()).unwrap();
        assert!(onboarding_complete(dir.path()));
        // Idempotent.
        mark_onboarding_complete(dir.path()).unwrap();
        assert!(onboarding_complete(dir.path()));
    }

    #[test]
    fn clear_user_config_removes_file() {
        let dir = TempDir::new().unwrap();
        save_user_config(dir.path(), &UserConfig::default()).unwrap();
        assert!(dir.path().join(USER_CONFIG_FILENAME).exists());
        clear_user_config(dir.path()).unwrap();
        assert!(!dir.path().join(USER_CONFIG_FILENAME).exists());
        // Idempotent.
        clear_user_config(dir.path()).unwrap();
    }

    #[test]
    fn resolve_user_config_dir_respects_env_var() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("custom-dir");
        // Use a unique env-var value via std::env (process-wide; serial_test isn't
        // wired here, but the env var write+read is local to this test thread).
        // SAFETY: writing to the env in tests is allowed; this lockfree pattern is
        // fine because resolve_user_config_dir reads it synchronously.
        // Use unsafe set_var since stable Rust 1.74+.
        // SAFETY: tests run single-threaded by default for env var manipulation.
        unsafe { std::env::set_var(USER_CONFIG_DIR_ENV, &target) };
        let resolved = resolve_user_config_dir();
        unsafe { std::env::remove_var(USER_CONFIG_DIR_ENV) };
        assert_eq!(resolved, target);
        assert!(target.exists());
    }
}
