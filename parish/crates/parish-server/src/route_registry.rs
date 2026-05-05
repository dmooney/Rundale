//! Registry of the HTTP API route paths that `lib.rs` registers on the Axum
//! router.
//!
//! Only IPC-parity routes are listed here — infrastructure endpoints that have
//! no Tauri command counterpart (`/api/health`, `/api/ws`, `/api/session-init`,
//! `/api/auth/status`, etc.) are intentionally excluded.
//!
//! The list here must stay in sync with the `.route(...)` calls in `lib.rs`.
//! The integration test `parish-core/tests/wiring_parity.rs` uses it to assert
//! that every Tauri command has a corresponding HTTP route (and vice versa) so
//! that drift between the two backends is caught before it ships.

/// Canonical list of HTTP API route paths (IPC surface only) registered by
/// this crate, in the same order the Tauri `EXPECTED_COMMANDS` list uses.
pub const EXPECTED_HTTP_ROUTES: &[&str] = &[
    // ── core game routes ──────────────────────────────────────────────────
    "/api/world-snapshot",
    "/api/setup-snapshot",
    "/api/map",
    "/api/npcs-here",
    "/api/theme",
    "/api/ui-config",
    "/api/debug-snapshot",
    "/api/submit-input",
    "/api/discover-save-files",
    "/api/save-game",
    "/api/load-branch",
    "/api/create-branch",
    "/api/new-save-file",
    "/api/new-game",
    "/api/save-state",
    "/api/react-to-message",
    // ── editor routes ─────────────────────────────────────────────────────
    "/api/editor-list-mods",
    "/api/editor-open-mod",
    "/api/editor-get-snapshot",
    "/api/editor-validate",
    "/api/editor-update-npcs",
    "/api/editor-update-locations",
    "/api/editor-save",
    "/api/editor-reload",
    "/api/editor-close",
    "/api/editor-list-saves",
    "/api/editor-list-branches",
    "/api/editor-list-snapshots",
    "/api/editor-read-snapshot",
];
