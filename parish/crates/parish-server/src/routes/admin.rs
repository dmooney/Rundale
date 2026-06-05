//! Admin-only command guard and related validation helpers.
//!
//! Covers:
//! - `PARISH_ADMIN_EMAILS` parsing and caching
//! - [`check_admin`] / [`check_admin_against`] / [`check_admin_no_config`]
//! - [`is_admin_command`] — which parsed commands require admin access
//! - [`validate_branch_name`] — branch name rules (#335)
//! - [`validate_addressed_to`] — `addressed_to` list rules (#752)

use axum::http::StatusCode;

use parish_core::input::Command;

// ── #335 — Branch name validation ───────────────────────────────────────────

/// Validates a branch name: non-empty, ≤ 64 chars, ASCII alphanumerics/`_`/`-`/` ` only.
///
/// Returns `Err(StatusCode::BAD_REQUEST)` on any violation.
pub fn validate_branch_name(name: &str) -> Result<(), StatusCode> {
    if name.is_empty() || name.len() > 64 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == ' ')
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

// ── #752 — addressed_to validation ──────────────────────────────────────────

/// Validates the `addressed_to` field from `POST /api/submit-input`.
///
/// Rules (mode-parity with the Tauri path in `parish-tauri`):
/// - At most **10** entries (prevents unbounded NPC-chip spam).
/// - Each name is at most **100** characters.
///
/// Returns `Err(StatusCode::BAD_REQUEST)` on any violation.
pub fn validate_addressed_to(addressed_to: &[String]) -> Result<(), StatusCode> {
    if addressed_to.len() > 10 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if addressed_to.iter().any(|name| name.len() > 100) {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

// ── #332 — Admin-only command guard ─────────────────────────────────────────

/// Parses a comma-separated list of emails into a `HashSet`, trimming
/// whitespace and dropping empty entries. Extracted so the caching layer
/// above can be unit-tested without env-var mutation.
pub fn parse_admin_emails(list: &str) -> std::collections::HashSet<String> {
    list.split(',')
        .map(|e| e.trim().to_string())
        .filter(|e| !e.is_empty())
        .collect()
}

/// Returns the parsed admin email set, lazily initialized from the
/// `PARISH_ADMIN_EMAILS` env var (comma-separated). `None` means the env
/// var was unset at the moment of first access.
///
/// The result is cached for the lifetime of the process (#480). This both
/// removes per-request env-var parsing overhead and prevents surprise
/// mid-flight authorization changes from a stray `std::env::set_var` — a
/// property we rely on for the security guarantee of `check_admin`.
pub fn admin_emails() -> Option<&'static std::collections::HashSet<String>> {
    use std::collections::HashSet;
    static CACHE: std::sync::OnceLock<Option<HashSet<String>>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| {
            std::env::var("PARISH_ADMIN_EMAILS")
                .ok()
                .map(|s| parse_admin_emails(&s))
        })
        .as_ref()
}

/// Returns `Ok(())` if the caller is permitted to run an admin command, or
/// `Err(StatusCode::FORBIDDEN)` otherwise.
///
/// `emails` is the parsed allow-list; pass `admin_emails()` at production
/// call sites. Accepting the set as a parameter (rather than calling
/// `admin_emails()` internally) keeps the `OnceCell` cache out of the
/// function body so unit tests can supply an isolated set without touching
/// global state (#605).
///
/// If `emails` is `None` the env var was unset: **allowed** in debug builds
/// (local dev), **denied** in release builds (fail-closed, #480).
pub fn check_admin(
    email: &str,
    cmd: &str,
    emails: Option<&std::collections::HashSet<String>>,
) -> Result<(), StatusCode> {
    match emails {
        Some(set) => {
            if set.contains(email) {
                Ok(())
            } else {
                tracing::warn!(user = %email, command = %cmd, "admin command rejected");
                Err(StatusCode::FORBIDDEN)
            }
        }
        None => {
            if cfg!(debug_assertions) {
                Ok(())
            } else {
                tracing::warn!(user = %email, command = %cmd, "admin command rejected — PARISH_ADMIN_EMAILS unset");
                Err(StatusCode::FORBIDDEN)
            }
        }
    }
}

/// Testable variant of [`check_admin`] that accepts an explicit admin email
/// rather than reading from the `PARISH_ADMIN_EMAILS` environment variable.
///
/// `admin_email` mirrors the single-value form used in tests: `Some(email)`
/// means that address is the sole admin; `None` means no admin is configured
/// (follows the same fail-closed rule as the env-var path in release builds).
///
/// Used by isolation tests (codex P1) so they compile against the public
/// surface without requiring `routes::check_admin` to be `pub` or relying on
/// the `OnceCell`-cached env var.
pub fn check_admin_against(
    email: &str,
    cmd: &str,
    admin_email: Option<&str>,
) -> Result<(), StatusCode> {
    match admin_email {
        Some(admin) => {
            if email == admin {
                Ok(())
            } else {
                tracing::warn!(user = %email, command = %cmd, "admin command rejected");
                Err(StatusCode::FORBIDDEN)
            }
        }
        None => check_admin_no_config(email, cmd, cfg!(debug_assertions)),
    }
}

/// Implements the fail-closed / fail-open logic for the unconfigured-admin
/// case, parameterised on `is_debug` so both branches are unit-testable
/// without a release build.
///
/// - `is_debug = true`  → `Ok(())` (fail-open for local dev)
/// - `is_debug = false` → `Err(FORBIDDEN)` (fail-closed in production)
pub fn check_admin_no_config(email: &str, cmd: &str, is_debug: bool) -> Result<(), StatusCode> {
    if is_debug {
        Ok(())
    } else {
        tracing::warn!(user = %email, command = %cmd, "admin command rejected — no admin configured");
        Err(StatusCode::FORBIDDEN)
    }
}

/// Returns `true` if the parsed command is an admin-only operation.
///
/// Admin commands are provider/key/model operations (both display and mutation)
/// that are gated by `PARISH_ADMIN_EMAILS`. Operates on the parsed `Command`
/// variant rather than raw text to avoid false-matching in-game dialogue.
pub fn is_admin_command(cmd: &Command) -> bool {
    matches!(
        cmd,
        Command::SetKey(_)
            | Command::ShowKey
            | Command::SetProvider(_)
            | Command::ShowProvider
            | Command::SetModel(_)
            | Command::ShowModel
            | Command::SetCloudProvider(_)
            | Command::SetCloudModel(_)
            | Command::SetCloudKey(_)
            | Command::ShowCloud
            | Command::ShowCloudModel
            | Command::ShowCloudKey
            | Command::SetCategoryProvider(_, _)
            | Command::SetCategoryModel(_, _)
            | Command::SetCategoryKey(_, _)
            | Command::ShowCategoryProvider(_)
            | Command::ShowCategoryModel(_)
            | Command::ShowCategoryKey(_)
    )
}
