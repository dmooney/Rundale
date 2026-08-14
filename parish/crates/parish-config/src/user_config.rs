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

use crate::{ServiceTier, ThinkingLevel};

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
    /// Base reasoning effort inherited by categories that do not override it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    /// Base output-token cap inherited by categories that do not override it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    /// Base synchronous service tier. `None` means provider default/Standard.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    /// Separate cap for Tier 2 simulation calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier2_max_output_tokens: Option<u32>,
    /// Separate cap for Tier 3 simulation calls.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tier3_max_output_tokens: Option<u32>,
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
        Ok(body) => {
            let config = toml::from_str::<UserConfig>(&body)
                .map_err(|e| ParishError::Config(format!("parse {}: {e}", path.display())))?;
            validate_inference_profile(&config)?;
            Ok(config)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(UserConfig::default()),
        Err(e) => Err(ParishError::Config(format!("read {}: {e}", path.display()))),
    }
}

fn validate_inference_profile(config: &UserConfig) -> Result<(), ParishError> {
    fn check_cap(field: &str, value: Option<u32>) -> Result<(), ParishError> {
        if let Some(value) = value
            && !(1..=65_536).contains(&value)
        {
            return Err(ParishError::Config(format!(
                "{field}={value} must be between 1 and 65536"
            )));
        }
        Ok(())
    }

    check_cap("max_output_tokens", config.max_output_tokens)?;
    for (category, override_) in &config.category_overrides {
        if crate::InferenceCategory::from_name(category).is_none() {
            return Err(ParishError::Config(format!(
                "category_overrides contains unknown category {category:?}; expected dialogue, simulation, intent, or reaction"
            )));
        }
        check_cap(
            &format!("category_overrides.{category}.max_output_tokens"),
            override_.max_output_tokens,
        )?;
        check_cap(
            &format!("category_overrides.{category}.tier2_max_output_tokens"),
            override_.tier2_max_output_tokens,
        )?;
        check_cap(
            &format!("category_overrides.{category}.tier3_max_output_tokens"),
            override_.tier3_max_output_tokens,
        )?;
        if category != "simulation"
            && (override_.tier2_max_output_tokens.is_some()
                || override_.tier3_max_output_tokens.is_some())
        {
            return Err(ParishError::Config(format!(
                "Tier 2/3 token caps are only valid for category_overrides.simulation, not {category}"
            )));
        }
    }
    Ok(())
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
    use serial_test::serial;
    use tempfile::TempDir;

    #[test]
    fn load_returns_default_on_missing_file() {
        let dir = TempDir::new().unwrap();
        let cfg = load_user_config(dir.path()).unwrap();
        assert_eq!(cfg, UserConfig::default());
    }

    #[test]
    fn load_malformed_user_config_returns_config_error() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(USER_CONFIG_FILENAME), "provider = {{{{").unwrap();

        let err = load_user_config(dir.path()).unwrap_err().to_string();
        assert!(err.contains("parse"), "got: {err}");
        assert!(err.contains(USER_CONFIG_FILENAME), "got: {err}");
    }

    #[test]
    fn invalid_cap_error_includes_field_and_value() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(USER_CONFIG_FILENAME),
            "max_output_tokens = 70000\n",
        )
        .unwrap();
        let error = load_user_config(dir.path()).unwrap_err().to_string();
        assert!(error.contains("max_output_tokens=70000"), "{error}");
    }

    #[test]
    fn unknown_inference_category_is_rejected_at_load() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(USER_CONFIG_FILENAME),
            "[category_overrides.typo]\nmax_output_tokens = 100\n",
        )
        .unwrap();
        let error = load_user_config(dir.path()).unwrap_err().to_string();
        assert!(error.contains("unknown category \"typo\""), "{error}");
    }

    #[test]
    fn save_then_load_round_trip() {
        let dir = TempDir::new().unwrap();
        let mut cfg = UserConfig {
            provider: Some("anthropic".to_string()),
            base_url: None,
            model: Some("claude-opus-4-7".to_string()),
            category_overrides: BTreeMap::new(),
            ..Default::default()
        };
        cfg.category_overrides.insert(
            "simulation".to_string(),
            CategoryOverride {
                provider: Some("groq".to_string()),
                model: Some("llama-3.1-8b-instant".to_string()),
                base_url: None,
                ..Default::default()
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
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        save_user_config(dir.path(), &cfg).unwrap();
        let body = std::fs::read_to_string(dir.path().join(USER_CONFIG_FILENAME)).unwrap();
        assert!(
            !body.contains("api_key"),
            "user config must never serialize an api_key field; got:\n{body}"
        );
    }

    #[test]
    fn explicit_standard_category_tier_survives_priority_parent_round_trip() {
        let dir = TempDir::new().unwrap();
        let cfg = UserConfig {
            service_tier: Some(ServiceTier::Priority),
            category_overrides: BTreeMap::from([(
                "dialogue".to_string(),
                CategoryOverride {
                    service_tier: Some(ServiceTier::Standard),
                    ..Default::default()
                },
            )]),
            ..Default::default()
        };
        save_user_config(dir.path(), &cfg).unwrap();
        let round = load_user_config(dir.path()).unwrap();
        assert_eq!(round, cfg);
        let body = std::fs::read_to_string(dir.path().join(USER_CONFIG_FILENAME)).unwrap();
        assert!(body.contains("service_tier = \"standard\""));
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
    #[serial(parish_env)]
    fn resolve_user_config_dir_respects_env_var() {
        let dir = TempDir::new().unwrap();
        let target = dir.path().join("custom-dir");
        // SAFETY: serialised by #[serial(parish_env)]
        unsafe { std::env::set_var(USER_CONFIG_DIR_ENV, &target) };
        let resolved = resolve_user_config_dir();
        unsafe { std::env::remove_var(USER_CONFIG_DIR_ENV) };
        assert_eq!(resolved, target);
        assert!(target.exists());
    }
}
