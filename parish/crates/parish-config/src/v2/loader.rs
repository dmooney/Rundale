use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use thiserror::Error;

use super::{
    INFERENCE_SCHEMA_VERSION, ProjectConfigV2, UserConfigV2, validate_project_config,
    validate_user_config,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigDocumentKind {
    Project,
    User,
}

impl std::fmt::Display for ConfigDocumentKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Project => "project",
            Self::User => "user",
        })
    }
}

#[derive(Debug, Error)]
pub enum ConfigLoadError {
    #[error("read {kind} config {path}: {source}")]
    Read {
        kind: ConfigDocumentKind,
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("parse {kind} config {path}: {message}")]
    Parse {
        kind: ConfigDocumentKind,
        path: PathBuf,
        message: String,
    },
    #[error(
        "{kind} config {path} uses schema_version={found}; Rundale requires schema_version={expected}. Legacy [provider]/[cloud] files are not migrated; archive the file and replace it from the v2 example"
    )]
    UnsupportedVersion {
        kind: ConfigDocumentKind,
        path: PathBuf,
        found: String,
        expected: u8,
    },
    #[error("invalid {kind} config {path}: {message}")]
    Validation {
        kind: ConfigDocumentKind,
        path: PathBuf,
        message: String,
    },
    #[error("write user config {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("serialize user config {path}: {message}")]
    Serialize { path: PathBuf, message: String },
    #[error(
        "removed configuration field [{field}] in {path}; use inference loadouts/routes from schema v2"
    )]
    RemovedField { path: PathBuf, field: String },
}

pub fn load_project_config_v2(path: &Path) -> Result<ProjectConfigV2, ConfigLoadError> {
    let Some(body) = read_optional(path, ConfigDocumentKind::Project)? else {
        return Ok(ProjectConfigV2::default());
    };
    let config = parse_versioned(&body, path, ConfigDocumentKind::Project)?;
    validate_project_config(&config).map_err(|error| ConfigLoadError::Validation {
        kind: ConfigDocumentKind::Project,
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    Ok(config)
}

pub fn load_user_config_v2(path: &Path) -> Result<UserConfigV2, ConfigLoadError> {
    let Some(body) = read_optional(path, ConfigDocumentKind::User)? else {
        return Ok(UserConfigV2::default());
    };
    let config = parse_versioned(&body, path, ConfigDocumentKind::User)?;
    validate_user_config(&config).map_err(|error| ConfigLoadError::Validation {
        kind: ConfigDocumentKind::User,
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    Ok(config)
}

pub fn save_user_config_v2(path: &Path, config: &UserConfigV2) -> Result<(), ConfigLoadError> {
    validate_user_config(config).map_err(|error| ConfigLoadError::Validation {
        kind: ConfigDocumentKind::User,
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let body = toml::to_string_pretty(config).map_err(|error| ConfigLoadError::Serialize {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ConfigLoadError::Write {
            path: path.to_path_buf(),
            source,
        })?;
    }
    let temporary = path.with_extension(format!("toml.{}.tmp", std::process::id()));
    let mut file = std::fs::File::create(&temporary).map_err(|source| ConfigLoadError::Write {
        path: temporary.clone(),
        source,
    })?;
    use std::io::Write as _;
    file.write_all(body.as_bytes())
        .and_then(|()| file.sync_all())
        .map_err(|source| ConfigLoadError::Write {
            path: temporary.clone(),
            source,
        })?;
    atomic_replace(&temporary, path).map_err(|source| ConfigLoadError::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Atomically replaces `destination` with a prepared same-directory file.
/// Windows `rename` does not replace an existing destination, so use the OS
/// replacement primitive there; Unix rename already has replacement semantics.
pub(super) fn atomic_replace(source: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(not(windows))]
    {
        std::fs::rename(source, destination)
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        let source: Vec<u16> = source
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let destination: Vec<u16> = destination
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: both buffers are NUL-terminated and live for the duration
        // of the call. Paths originate from Rust Path values, not raw pointers.
        let replaced = unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if replaced == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

/// Archives an invalid/legacy user document and writes a fresh v2 document.
/// The original bytes are preserved under a collision-resistant suffix; no
/// fields are migrated and an existing archive is never overwritten.
pub fn archive_and_reset_user_config_v2(path: &Path) -> Result<Option<PathBuf>, ConfigLoadError> {
    // Render, validate, write, and fsync the replacement before moving the
    // user's bytes. After this point only same-directory renames remain.
    let candidate = path.with_extension(format!("toml.reset.{}.candidate", std::process::id()));
    save_user_config_v2(&candidate, &UserConfigV2::default())?;
    let archive = if path.exists() {
        let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%.9fZ");
        let legacy = std::fs::read_to_string(path)
            .ok()
            .and_then(|body| toml::from_str::<toml::Value>(&body).ok())
            .and_then(|value| {
                value
                    .get("schema_version")
                    .and_then(toml::Value::as_integer)
            })
            .is_none_or(|version| version < 2);
        let kind = if legacy { "v1" } else { "invalid" };
        let archive = path.with_file_name(format!(
            "{}.{kind}.bak.{stamp}",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("parish.toml")
        ));
        if let Err(source) = std::fs::rename(path, &archive) {
            let _ = std::fs::remove_file(&candidate);
            return Err(ConfigLoadError::Write {
                path: archive,
                source,
            });
        }
        Some(archive)
    } else {
        None
    };
    if let Err(source) = std::fs::rename(&candidate, path) {
        if let Some(archive) = &archive
            && let Err(restore) = std::fs::rename(archive, path)
        {
            return Err(ConfigLoadError::Serialize {
                path: path.to_path_buf(),
                message: format!(
                    "install clean configuration failed: {source}; restoring original from {} also failed: {restore}",
                    archive.display()
                ),
            });
        }
        return Err(ConfigLoadError::Write {
            path: path.to_path_buf(),
            source,
        });
    }
    Ok(archive)
}

fn read_optional(path: &Path, kind: ConfigDocumentKind) -> Result<Option<String>, ConfigLoadError> {
    match std::fs::read_to_string(path) {
        Ok(body) => Ok(Some(body)),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(ConfigLoadError::Read {
            kind,
            path: path.to_path_buf(),
            source,
        }),
    }
}

fn parse_versioned<T: DeserializeOwned>(
    body: &str,
    path: &Path,
    kind: ConfigDocumentKind,
) -> Result<T, ConfigLoadError> {
    let value: toml::Value = toml::from_str(body).map_err(|error| ConfigLoadError::Parse {
        kind,
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    for removed in ["provider", "cloud"] {
        if value.get(removed).is_some() {
            return Err(ConfigLoadError::RemovedField {
                path: path.to_path_buf(),
                field: removed.to_string(),
            });
        }
    }
    let version = value.get("schema_version");
    if version.and_then(toml::Value::as_integer) != Some(i64::from(INFERENCE_SCHEMA_VERSION)) {
        return Err(ConfigLoadError::UnsupportedVersion {
            kind,
            path: path.to_path_buf(),
            found: version.map_or_else(|| "missing".to_string(), ToString::to_string),
            expected: INFERENCE_SCHEMA_VERSION,
        });
    }
    value.try_into().map_err(|error| ConfigLoadError::Parse {
        kind,
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_documents_have_v2_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let project = load_project_config_v2(&directory.path().join("project.toml")).unwrap();
        let user = load_user_config_v2(&directory.path().join("user.toml")).unwrap();
        assert_eq!(project.schema_version, INFERENCE_SCHEMA_VERSION);
        assert_eq!(user.schema_version, INFERENCE_SCHEMA_VERSION);
    }

    #[test]
    fn legacy_config_gets_actionable_hard_break() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("parish.toml");
        std::fs::write(&path, "[provider]\nname = 'openai'\n").unwrap();
        let error = load_project_config_v2(&path).unwrap_err().to_string();
        assert!(
            error.contains("removed configuration field [provider]"),
            "{error}"
        );
    }

    #[test]
    fn cloud_root_wins_over_version_error_for_targeted_recovery() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("parish.toml");
        std::fs::write(&path, "schema_version = 1\n[cloud]\nprovider = 'openai'\n").unwrap();
        let error = load_user_config_v2(&path).unwrap_err().to_string();
        assert!(
            error.contains("removed configuration field [cloud]"),
            "{error}"
        );
    }

    #[test]
    fn save_replaces_an_existing_user_document() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("parish.toml");
        std::fs::write(&path, "old bytes").unwrap();
        save_user_config_v2(&path, &UserConfigV2::default()).unwrap();
        let loaded = load_user_config_v2(&path).unwrap();
        assert_eq!(loaded.schema_version, INFERENCE_SCHEMA_VERSION);
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("parish.toml");
        std::fs::write(&path, "schema_version = 2\nlegacy = true\n").unwrap();
        let error = load_user_config_v2(&path).unwrap_err().to_string();
        assert!(error.contains("unknown field"), "{error}");
    }

    #[test]
    fn user_save_round_trips_without_secret_values() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("parish.toml");
        let mut config = UserConfigV2::default();
        config.inference.providers.insert(
            "gateway".into(),
            super::super::CustomProviderDefinition {
                display_name: "Gateway".into(),
                default_endpoint: Some("chat".into()),
                endpoints: std::collections::BTreeMap::from([(
                    "chat".into(),
                    super::super::CustomEndpointDefinition {
                        inference_base_url: "https://example.test".into(),
                        discovery_base_url: None,
                        inference_adapter: super::super::InferenceAdapter::OpenaiChatV1,
                        discovery_adapter: super::super::DiscoveryAdapter::None,
                        auth_adapter: super::super::AuthAdapter::Bearer,
                        default_reasoning_dialect: super::super::ReasoningDialect::None,
                        allow_insecure_http: false,
                        default_openai_generation_wire: Some(
                            super::super::OpenAiChatGenerationWire {
                                output_limit_field: super::super::OutputLimitField::MaxTokens,
                                structured_output: std::collections::BTreeSet::from([
                                    super::super::StructuredOutputMode::PromptValidatedJson,
                                ]),
                            },
                        ),
                    },
                )]),
                models: std::collections::BTreeMap::new(),
            },
        );
        config.credential_bindings.insert(
            "custom:gateway".into(),
            super::super::CredentialBinding {
                env: Some("OPENAI_API_KEY".into()),
            },
        );
        save_user_config_v2(&path, &config).unwrap();
        let body = std::fs::read_to_string(&path).unwrap();
        assert!(!body.contains("sk-"));
        assert_eq!(load_user_config_v2(&path).unwrap().schema_version, 2);
    }
}
