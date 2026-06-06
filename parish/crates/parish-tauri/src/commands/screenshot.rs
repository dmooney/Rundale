//! Screenshot capture — save, read, agent-triggered capture, and round-trip callbacks.

use std::sync::Arc;

use crate::AppState;

// ── Screenshot capture (player-triggered, MCP-readable) ──────────────────────

/// Metadata describing the most-recently-saved screenshot.
///
/// The image is written to `<saves_dir>/screenshots/parish-<ISO-timestamp>.png`
/// by the Svelte frontend (which calls `html-to-image` and posts the resulting
/// `data:image/png;base64,...` URL to the `save_screenshot` command). Once
/// written, the absolute path is cached on `AppState::latest_screenshot_path`
/// so the `parish_latest_screenshot` MCP tool can report it without scanning
/// the directory.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ScreenshotInfo {
    /// Absolute filesystem path to the PNG.
    pub path: String,
    /// ISO-8601 UTC timestamp the file was written (`YYYY-MM-DDTHH:MM:SSZ`).
    pub taken_at: String,
    /// Size of the PNG payload in bytes.
    pub size_bytes: u64,
}

/// Decodes a `data:image/png;base64,...` URL into the raw PNG byte payload.
///
/// Returns `Err` if the URL is malformed, has the wrong MIME type, or the
/// base64 segment fails to decode.
pub fn decode_data_url_png(data_url: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    const PREFIX: &str = "data:image/png;base64,";
    let b64 = data_url
        .strip_prefix(PREFIX)
        .ok_or_else(|| format!("expected data URL to start with `{PREFIX}`"))?;
    base64::engine::general_purpose::STANDARD
        .decode(b64.as_bytes())
        .map_err(|e| format!("base64 decode failed: {e}"))
}

/// Writes the decoded PNG bytes under `<saves_dir>/screenshots/parish-<ISO>.png`
/// and returns the [`ScreenshotInfo`] metadata for the newly created file.
///
/// Pure on `(saves_dir, png_bytes, now)` — no AppState, no Tauri handle — so
/// it can be unit-tested in isolation. The `now` callback returns the UTC
/// timestamp used both in the filename and the response (it is parameterised
/// so tests can pin the value).
pub fn write_screenshot_to_disk(
    saves_dir: &std::path::Path,
    png_bytes: &[u8],
    now: chrono::DateTime<chrono::Utc>,
) -> Result<ScreenshotInfo, String> {
    let dir = saves_dir.join("screenshots");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("failed to create {}: {e}", dir.display()))?;

    // Filenames must be filesystem-safe on every platform we support, so use
    // `-` instead of `:` in the time component. `format!("{:?}", ts)` would
    // include sub-second precision plus the trailing `Z`; we keep the stem
    // tidy by formatting with the second-precision strftime template.
    let stem = now.format("parish-%Y-%m-%dT%H-%M-%SZ").to_string();
    let path = dir.join(format!("{stem}.png"));

    std::fs::write(&path, png_bytes)
        .map_err(|e| format!("failed to write {}: {e}", path.display()))?;

    let size_bytes = png_bytes.len() as u64;
    let path_string = path.to_string_lossy().to_string();
    let taken_at = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
    Ok(ScreenshotInfo {
        path: path_string,
        taken_at,
        size_bytes,
    })
}

/// Internal save-screenshot implementation shared with the MCP bridge / web
/// route. Decodes the `data_url`, writes the PNG, and updates
/// [`AppState::latest_screenshot_path`].
pub(crate) async fn do_save_screenshot(
    state: &Arc<AppState>,
    data_url: String,
) -> Result<ScreenshotInfo, String> {
    let bytes = decode_data_url_png(&data_url)?;
    let info = write_screenshot_to_disk(&state.saves_dir, &bytes, chrono::Utc::now())?;
    *state.latest_screenshot_path.lock().await = Some(std::path::PathBuf::from(&info.path));
    Ok(info)
}

/// Internal latest-screenshot reader shared with the MCP bridge / web route.
///
/// Re-stat`s the file each call so a screenshot deleted out from under the
/// session is reported as missing rather than reused indefinitely.
pub(crate) async fn do_get_latest_screenshot(
    state: &Arc<AppState>,
) -> Result<Option<ScreenshotInfo>, String> {
    let Some(path) = state.latest_screenshot_path.lock().await.clone() else {
        return Ok(None);
    };
    // Use tokio::fs::metadata so the stat call doesn't block the async
    // executor under load (this handler may be invoked from the MCP bridge
    // while other Tokio tasks are waiting on the same worker thread).
    let metadata = match tokio::fs::metadata(&path).await {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    let modified = metadata
        .modified()
        .map_err(|e| format!("stat({}): {e}", path.display()))?;
    let taken_at = chrono::DateTime::<chrono::Utc>::from(modified)
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    Ok(Some(ScreenshotInfo {
        path: path.to_string_lossy().to_string(),
        taken_at,
        size_bytes: metadata.len(),
    }))
}

/// Persists a screenshot captured by the frontend.
///
/// `data_url` is a `data:image/png;base64,...` string produced by
/// `html-to-image` in the Svelte UI. The PNG is decoded and written to
/// `<saves_dir>/screenshots/parish-<ISO-timestamp>.png`; the resulting path
/// is cached on [`AppState::latest_screenshot_path`] so the MCP
/// `parish_latest_screenshot` tool can read it back without rescanning.
#[tauri::command]
pub async fn save_screenshot(
    data_url: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<ScreenshotInfo, String> {
    do_save_screenshot(&state, data_url).await
}

/// Reads metadata for the most recently captured screenshot, if any.
///
/// Returns `Ok(None)` when no screenshot has been captured this session, or
/// when the cached path no longer exists on disk.
#[tauri::command]
pub async fn get_latest_screenshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<Option<ScreenshotInfo>, String> {
    do_get_latest_screenshot(&state).await
}

/// Agent-triggered screenshot capture. Registered as a Tauri command for
/// wiring-parity with the `/api/take-screenshot` bridge route.
///
/// In Tauri mode this is not invoked directly by the frontend — use the MCP
/// tool `parish_take_screenshot` instead. The bridge handler also calls
/// `do_take_screenshot` directly for a shared implementation.
#[tauri::command]
pub async fn take_screenshot(
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<ScreenshotInfo, String> {
    do_take_screenshot(&state, &app).await
}

/// Capture deadline for the agent screenshot round-trip.
///
/// Reads `PARISH_SCREENSHOT_TIMEOUT_SECS` (whole seconds); defaults to 45 and
/// ignores unparseable or zero values. The full `.app-shell` `html-to-image`
/// pass can exceed the old hardcoded 15 s under live local-inference load
/// (#1160), so the deadline is configurable and longer by default.
fn screenshot_timeout() -> std::time::Duration {
    let secs = std::env::var("PARISH_SCREENSHOT_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|&n| n > 0)
        .unwrap_or(45);
    std::time::Duration::from_secs(secs)
}

/// Decides whether a late-completing capture can still be returned after the
/// oneshot deadline expired.
///
/// The frontend may finish `html-to-image` and write the PNG just after the
/// backend gave up waiting; without this, that image is orphaned and the
/// caller gets a hard error even though a valid screenshot exists (#1160).
/// Returns the newest on-disk screenshot only when it was taken at or after
/// the request started — i.e. it is THIS request landing late, not a stale
/// image from an earlier capture. `taken_at` is second-precision, so the
/// start is floored to the second to keep a same-second capture eligible.
///
/// Pure on `(started_at, latest)` for unit testing.
fn resolve_late_screenshot(
    started_at: chrono::DateTime<chrono::Utc>,
    latest: Option<ScreenshotInfo>,
) -> Option<ScreenshotInfo> {
    let info = latest?;
    let taken = chrono::NaiveDateTime::parse_from_str(&info.taken_at, "%Y-%m-%dT%H:%M:%SZ")
        .ok()?
        .and_utc();
    let start_floor =
        chrono::DateTime::from_timestamp(started_at.timestamp(), 0).unwrap_or(started_at);
    if taken >= start_floor {
        Some(info)
    } else {
        None
    }
}

/// True when a captured OS window belongs to the Parish/Rundale desktop app.
///
/// Matched on the owning application name: the dev binary reports
/// `parish-tauri`, a packaged build reports its bundle name (`Rundale`).
/// Pure for unit testing the window-selection rule.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn is_app_window(app_name: &str) -> bool {
    let a = app_name.to_lowercase();
    a.contains("parish") || a.contains("rundale")
}

/// Captures the Parish/Rundale desktop window as PNG bytes using native OS
/// screen capture (`xcap`).
///
/// This is the primary capture path because the gameplay MapLibre minimap is a
/// WebGL canvas that `html-to-image` cannot read — it draws cross-origin tiles,
/// which taint the canvas and block `toDataURL()`, so the map serialises blank
/// (#1160 follow-up). Native capture reads the real composited pixels, map
/// included. Picks the largest non-minimised app window. Blocking — call from
/// `spawn_blocking`.
///
/// macOS/Windows only — the Linux xcap backend needs PipeWire system libs, so
/// Linux desktop uses the html-to-image fallback instead.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn capture_app_window_png() -> Result<Vec<u8>, String> {
    let windows = xcap::Window::all().map_err(|e| format!("enumerate windows: {e}"))?;
    let mut best: Option<(u64, xcap::Window)> = None;
    for w in windows {
        if w.is_minimized().unwrap_or(false) {
            continue;
        }
        if !is_app_window(&w.app_name().unwrap_or_default()) {
            continue;
        }
        let area = u64::from(w.width().unwrap_or(0)) * u64::from(w.height().unwrap_or(0));
        if best.as_ref().is_none_or(|(a, _)| area > *a) {
            best = Some((area, w));
        }
    }
    let (_, win) = best.ok_or("no Parish/Rundale window found to capture")?;
    let img = win
        .capture_image()
        .map_err(|e| format!("capture window: {e}"))?;
    let mut buf = Vec::new();
    img.write_to(
        &mut std::io::Cursor::new(&mut buf),
        xcap::image::ImageFormat::Png,
    )
    .map_err(|e| format!("encode png: {e}"))?;
    if buf.is_empty() {
        return Err("captured image encoded to 0 bytes".into());
    }
    Ok(buf)
}

/// Native-capture path: grab the window pixels, write them under
/// `<saves_dir>/screenshots/`, and update `latest_screenshot_path` so it
/// behaves identically to the frontend round-trip from the caller's view.
#[cfg(any(target_os = "macos", target_os = "windows"))]
async fn do_take_screenshot_native(state: &Arc<AppState>) -> Result<ScreenshotInfo, String> {
    let png = tokio::task::spawn_blocking(capture_app_window_png)
        .await
        .map_err(|e| format!("capture task join: {e}"))??;
    let now = chrono::Utc::now();
    let info = write_screenshot_to_disk(&state.saves_dir, &png, now)?;
    *state.latest_screenshot_path.lock().await = Some(std::path::PathBuf::from(&info.path));
    Ok(info)
}

/// Agent-triggered screenshot capture.
///
/// Tries native window capture first ([`do_take_screenshot_native`]) so the
/// WebGL minimap appears in the image. If native capture is unavailable (no
/// window, missing screen-recording permission, headless), it falls back to the
/// frontend `html-to-image` round-trip in [`do_take_screenshot_frontend`]
/// (which carries the #1160 deadline + late-capture handling). The map may be
/// blank in the fallback, but a screenshot is still produced.
pub(crate) async fn do_take_screenshot(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) -> Result<ScreenshotInfo, String> {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    match do_take_screenshot_native(state).await {
        Ok(info) => return Ok(info),
        Err(e) => {
            tracing::warn!(
                "native screenshot failed ({e}); falling back to frontend html-to-image capture"
            );
        }
    }
    do_take_screenshot_frontend(state, app).await
}

/// Frontend `html-to-image` round-trip — the fallback capture path.
///
/// Generates a UUID request ID, stashes a `oneshot::Sender` in
/// `AppState::pending_screenshots`, emits `request-screenshot` to the live
/// frontend window, and awaits the result for up to [`screenshot_timeout`]
/// (default 45 s, `PARISH_SCREENSHOT_TIMEOUT_SECS`).
///
/// On emit failure the pending entry is cleaned up immediately so the map
/// does not grow unbounded. On timeout the entry is removed; if a capture
/// from this request landed on disk just after the deadline we return it via
/// [`resolve_late_screenshot`] rather than erroring (#1160).
/// The frontend delivers the result via `notify_screenshot_captured` (success)
/// or `notify_screenshot_error` (failure).
async fn do_take_screenshot_frontend(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) -> Result<ScreenshotInfo, String> {
    use tauri::Emitter;
    use tokio::sync::oneshot;
    let started_at = chrono::Utc::now();
    let timeout = screenshot_timeout();
    let request_id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = oneshot::channel();
    {
        let mut pending = state.pending_screenshots.lock().await;
        pending.insert(request_id.clone(), tx);
    }
    if let Err(e) = app.emit(
        crate::events::EVENT_REQUEST_SCREENSHOT,
        serde_json::json!({"request_id": request_id}),
    ) {
        let mut pending = state.pending_screenshots.lock().await;
        pending.remove(&request_id);
        return Err(e.to_string());
    }
    match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(result)) => result,
        Ok(Err(_)) => Err("screenshot request cancelled".into()),
        Err(_) => {
            {
                let mut pending = state.pending_screenshots.lock().await;
                pending.remove(&request_id);
            }
            // The capture may have completed just after the deadline; if a
            // fresh PNG landed on disk, return it instead of a hard error.
            let latest = do_get_latest_screenshot(state).await.ok().flatten();
            match resolve_late_screenshot(started_at, latest) {
                Some(info) => Ok(info),
                None => Err(format!(
                    "screenshot capture timed out after {} s",
                    timeout.as_secs()
                )),
            }
        }
    }
}

/// Called by the frontend after successfully capturing a screenshot in
/// response to a `request-screenshot` Tauri event. Delivers the
/// `ScreenshotInfo` through the pending oneshot channel so the waiting
/// bridge handler can return it to the MCP client.
///
/// Internal round-trip callback — no HTTP equivalent on `parish-server`.
#[tauri::command]
pub async fn notify_screenshot_captured(
    request_id: String,
    info: ScreenshotInfo,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let mut pending = state.pending_screenshots.lock().await;
    if let Some(tx) = pending.remove(&request_id) {
        let _ = tx.send(Ok(info));
    }
    Ok(())
}

/// Called by the frontend when screenshot capture fails (e.g. `html-to-image`
/// error or `save_screenshot` IPC error). Immediately unblocks the waiting
/// bridge handler with the error message instead of letting it time out.
///
/// Internal round-trip callback — no HTTP equivalent on `parish-server`.
#[tauri::command]
pub async fn notify_screenshot_error(
    request_id: String,
    error: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let mut pending = state.pending_screenshots.lock().await;
    if let Some(tx) = pending.remove(&request_id) {
        let _ = tx.send(Err(error));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::cmd_tests::test_app_state;

    /// A 1×1 transparent PNG, as the smallest valid payload to round-trip.
    const ONE_PIXEL_PNG_B64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkAAIAAAoAAv/lxKUAAAAASUVORK5CYII=";

    fn one_pixel_data_url() -> String {
        format!("data:image/png;base64,{ONE_PIXEL_PNG_B64}")
    }

    #[test]
    fn decode_data_url_png_accepts_well_formed_url() {
        let bytes = decode_data_url_png(&one_pixel_data_url()).unwrap();
        // PNG magic header is 8 bytes starting with 0x89 'P' 'N' 'G'.
        assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
    }

    #[test]
    fn decode_data_url_png_rejects_wrong_prefix() {
        let err = decode_data_url_png("data:image/jpeg;base64,xxxx").unwrap_err();
        assert!(err.contains("data:image/png;base64,"), "got: {err}");
    }

    #[test]
    fn decode_data_url_png_rejects_invalid_base64() {
        let err = decode_data_url_png("data:image/png;base64,***not-base64***").unwrap_err();
        assert!(err.contains("base64 decode failed"), "got: {err}");
    }

    #[test]
    fn write_screenshot_to_disk_creates_file_and_reports_size() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = decode_data_url_png(&one_pixel_data_url()).unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-05-09T12:34:56Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let info = write_screenshot_to_disk(tmp.path(), &bytes, now).unwrap();

        let path = std::path::PathBuf::from(&info.path);
        assert!(path.exists(), "PNG should be written to {}", info.path);
        assert!(
            info.path.ends_with("parish-2026-05-09T12-34-56Z.png"),
            "filename should embed the timestamp; got {}",
            info.path
        );
        assert_eq!(info.size_bytes, bytes.len() as u64);
        assert_eq!(info.taken_at, "2026-05-09T12:34:56Z");

        // The directory was auto-created.
        assert!(tmp.path().join("screenshots").is_dir());
    }

    #[tokio::test]
    async fn do_save_screenshot_round_trips_through_app_state() {
        // Override saves_dir to a fresh tempdir so the test is hermetic
        // (otherwise the screenshot would land under the workspace `saves/`
        // dir and pollute later runs).
        let tmp = tempfile::tempdir().unwrap();
        let mut state = test_app_state();
        let s = std::sync::Arc::get_mut(&mut state)
            .expect("test_app_state must hand back a unique Arc");
        s.saves_dir = tmp.path().to_path_buf();

        let info = do_save_screenshot(&state, one_pixel_data_url())
            .await
            .expect("screenshot should save");

        assert!(std::path::PathBuf::from(&info.path).exists());

        // The latest_screenshot_path is now populated.
        let latest = state.latest_screenshot_path.lock().await.clone();
        assert_eq!(
            latest.map(|p| p.to_string_lossy().to_string()),
            Some(info.path.clone()),
        );

        // get_latest_screenshot reads the same file back.
        let read_back = do_get_latest_screenshot(&state)
            .await
            .expect("read back must succeed")
            .expect("a screenshot should be cached");
        assert_eq!(read_back.path, info.path);
        assert_eq!(read_back.size_bytes, info.size_bytes);
    }

    #[tokio::test]
    async fn do_get_latest_screenshot_returns_none_when_unset() {
        let state = test_app_state();
        let info = do_get_latest_screenshot(&state).await.unwrap();
        assert!(info.is_none(), "no screenshot taken yet → None");
    }

    #[tokio::test]
    async fn do_get_latest_screenshot_returns_none_when_file_missing() {
        let mut state = test_app_state();
        let s = std::sync::Arc::get_mut(&mut state).unwrap();
        // Point at a path that doesn't exist on disk.
        *s.latest_screenshot_path.get_mut() =
            Some(std::path::PathBuf::from("/no/such/parish-screenshot.png"));
        let info = do_get_latest_screenshot(&state).await.unwrap();
        assert!(info.is_none(), "missing file on disk → None");
    }

    // ── #1160 screenshot deadline + late-capture fallback ────────────────────

    #[test]
    fn screenshot_timeout_default_env_and_garbage() {
        // One test (not several) so the shared process env var is mutated
        // sequentially and cannot race a sibling test running in parallel.
        // SAFETY: edition-2024 env mutation; confined to this single test.
        unsafe { std::env::remove_var("PARISH_SCREENSHOT_TIMEOUT_SECS") };
        assert_eq!(screenshot_timeout().as_secs(), 45, "default is 45 s");

        unsafe { std::env::set_var("PARISH_SCREENSHOT_TIMEOUT_SECS", "90") };
        assert_eq!(screenshot_timeout().as_secs(), 90, "env override honoured");

        // Zero and non-numeric values fall back to the default.
        unsafe { std::env::set_var("PARISH_SCREENSHOT_TIMEOUT_SECS", "0") };
        assert_eq!(screenshot_timeout().as_secs(), 45, "zero ignored");
        unsafe { std::env::set_var("PARISH_SCREENSHOT_TIMEOUT_SECS", "soon") };
        assert_eq!(screenshot_timeout().as_secs(), 45, "garbage ignored");

        unsafe { std::env::remove_var("PARISH_SCREENSHOT_TIMEOUT_SECS") };
    }

    fn info_at(taken_at: &str) -> ScreenshotInfo {
        ScreenshotInfo {
            path: "/tmp/parish-shot.png".into(),
            taken_at: taken_at.into(),
            size_bytes: 1234,
        }
    }

    #[test]
    fn resolve_late_screenshot_returns_capture_from_this_request() {
        let started = chrono::DateTime::parse_from_rfc3339("2026-05-31T23:59:33Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        // Same second (taken_at is floored to the second) → eligible.
        let info = resolve_late_screenshot(started, Some(info_at("2026-05-31T23:59:33Z")));
        assert!(
            info.is_some(),
            "same-second late capture should be returned"
        );
        // Strictly newer → eligible.
        let info = resolve_late_screenshot(started, Some(info_at("2026-05-31T23:59:40Z")));
        assert!(info.is_some(), "newer capture should be returned");
    }

    #[test]
    fn resolve_late_screenshot_rejects_stale_or_missing() {
        let started = chrono::DateTime::parse_from_rfc3339("2026-05-31T23:59:33Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        // Older than the request start → stale, must not be returned.
        assert!(resolve_late_screenshot(started, Some(info_at("2026-05-31T23:59:10Z"))).is_none());
        // No screenshot on disk at all.
        assert!(resolve_late_screenshot(started, None).is_none());
        // Unparseable timestamp → treated as not eligible.
        assert!(resolve_late_screenshot(started, Some(info_at("not-a-date"))).is_none());
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[test]
    fn is_app_window_matches_parish_and_rundale_only() {
        // Dev binary and packaged bundle names (case-insensitive).
        assert!(is_app_window("parish-tauri"));
        assert!(is_app_window("Parish"));
        assert!(is_app_window("Rundale"));
        assert!(is_app_window("RUNDALE"));
        // Unrelated apps are ignored so we never capture the wrong window.
        assert!(!is_app_window("Safari"));
        assert!(!is_app_window("Terminal"));
        assert!(!is_app_window(""));
    }
}
