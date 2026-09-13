//! Demo and screenshot stub endpoints — desktop-only features.
//!
//! Demo mode is a Tauri-only feature; these handlers return 501 Not Implemented
//! so the server path stays aligned with Tauri for MCP / parity tests.
//! Screenshot capture rides the live Tauri window (no DOM in headless mode),
//! so those stubs also return 501.

use axum::Json;
use axum::http::StatusCode;

// ── Demo mode stubs (desktop-only feature) ──────────────────────────────────

/// `GET /api/demo-config` — demo mode is a Tauri-only desktop feature.
pub async fn get_demo_config() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Demo mode is only available in the desktop app."
        })),
    )
}

/// `GET /api/demo-context` — demo mode is a Tauri-only desktop feature.
pub async fn get_demo_context() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Demo mode is only available in the desktop app."
        })),
    )
}

/// `POST /api/llm-player-action` — demo mode is a Tauri-only desktop feature.
pub async fn get_llm_player_action() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Demo mode is only available in the desktop app."
        })),
    )
}

// ── Screenshot stubs (Tauri-only feature) ───────────────────────────────────
//
// Player-triggered screenshots ride the live Tauri window (the Svelte UI
// captures via `html-to-image` and posts the data URL through the desktop
// IPC). The headless server has no DOM to capture and no GTK display to
// snapshot, so both endpoints return 501. Same shape as the demo stubs
// above: keep the path so MCP / parity tests stay aligned, but signal
// "Tauri-only" to the caller.

/// `POST /api/save-screenshot` — screenshots are a Tauri-only desktop feature.
pub async fn save_screenshot() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Screenshot capture is only available in the desktop app."
        })),
    )
}

/// `GET /api/latest-screenshot` — screenshots are a Tauri-only desktop feature.
pub async fn get_latest_screenshot() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Screenshot capture is only available in the desktop app."
        })),
    )
}

/// `POST /api/take-screenshot` — agent-triggered screenshot capture. Only
/// works in the Tauri desktop mode where a live browser window is available
/// for `html-to-image` to render; the headless server has no DOM to capture.
pub async fn take_screenshot() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(serde_json::json!({
            "error": "Agent-triggered screenshot capture is only available in the desktop app."
        })),
    )
}
