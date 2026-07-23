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

/// The graphical webview's identity and current capture readiness.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GraphicalReadiness {
    pub launch_token: String,
    pub ready: bool,
    pub error: Option<String>,
}

/// Channels for one agent-requested capture. Receipt is acknowledged separately
/// from completion so a missing frontend fails promptly instead of consuming
/// the full rasterization timeout.
pub(crate) struct PendingScreenshot {
    pub started: Option<tokio::sync::oneshot::Sender<()>>,
    pub completion: tokio::sync::oneshot::Sender<Result<ScreenshotInfo, String>>,
}

/// Reads the current desktop graphical-session token. The frontend must echo
/// this value when reporting that its screenshot listener and first Pixi frame
/// are ready; a token is unique to each native process launch.
#[tauri::command]
pub async fn get_graphical_readiness(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<GraphicalReadiness, String> {
    Ok(graphical_readiness(state.inner()))
}

pub(crate) fn graphical_readiness(state: &Arc<AppState>) -> GraphicalReadiness {
    GraphicalReadiness {
        launch_token: state.graphical_launch_token.clone(),
        ready: state
            .graphical_ready
            .load(std::sync::atomic::Ordering::Acquire),
        error: state
            .graphical_error
            .lock()
            .expect("graphical error lock poisoned")
            .clone(),
    }
}

/// Marks the active graphical webview ready for MCP screenshots.
#[tauri::command]
pub async fn report_graphical_ready(
    launch_token: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    if launch_token != state.graphical_launch_token {
        return Err("graphical readiness token belongs to a different launch".into());
    }
    state
        .graphical_ready
        .store(true, std::sync::atomic::Ordering::Release);
    *state
        .graphical_error
        .lock()
        .expect("graphical error lock poisoned") = None;
    Ok(())
}

/// Records a frontend initialization failure so MCP readiness explains why a
/// graphical session cannot produce a trustworthy capture.
#[tauri::command]
pub async fn report_graphical_error(
    launch_token: String,
    error: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    if launch_token != state.graphical_launch_token {
        return Err("graphical readiness token belongs to a different launch".into());
    }
    state
        .graphical_ready
        .store(false, std::sync::atomic::Ordering::Release);
    *state
        .graphical_error
        .lock()
        .expect("graphical error lock poisoned") = Some(error);
    Ok(())
}

/// Invalidates graphical readiness when the mounted frontend is torn down.
#[tauri::command]
pub async fn report_graphical_unready(
    launch_token: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    if launch_token != state.graphical_launch_token {
        return Err("graphical readiness token belongs to a different launch".into());
    }
    state
        .graphical_ready
        .store(false, std::sync::atomic::Ordering::Release);
    Ok(())
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

/// Outcome of inspecting decoded PNG pixels for real content (#1301).
///
/// A capture is `Blank` when every sampled pixel is (near) the same colour —
/// the signature of a degenerate frame: the native `xcap` path grabbing a
/// window that left the compositor, or the `html-to-image` fallback rasterising
/// an unmounted / invisible app shell. Such frames encode to a perfectly valid,
/// full-resolution PNG, so length and decode checks pass; only pixel content
/// distinguishes them from a real screenshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PngContent {
    /// The image carries varied pixels — a real capture.
    HasContent,
    /// The image is a single solid colour (within tolerance) — degenerate.
    Blank,
}

/// Per-channel tolerance for "this pixel matches the reference colour".
///
/// JPEG-free PNG captures of a solid fill are usually bit-identical, but a
/// faint anti-aliased gradient or a near-white shell can wobble by a couple of
/// levels; a small tolerance keeps the classifier from calling a genuinely
/// blank frame "content" because of 1-LSB noise, without flagging a real render
/// (which varies by far more than this across its pixels).
const BLANK_CHANNEL_TOLERANCE: u8 = 6;

/// Decodes `png_bytes` and classifies the frame as [`PngContent::Blank`] or
/// [`PngContent::HasContent`] by checking whether every pixel is (within
/// [`BLANK_CHANNEL_TOLERANCE`]) the same colour as the first pixel.
///
/// Pure on the input bytes for unit testing. Returns `Err` when the bytes are
/// not a decodable PNG — a capture we cannot even read is never reported as a
/// success, and must not be silently treated as "blank" either (#1301 AC #5).
///
/// Works on every platform (the `png` crate is a direct, un-gated dependency),
/// so the same guard protects the native `xcap` path and the `html-to-image`
/// fallback identically — mode parity (rule #2).
pub fn classify_png_content(png_bytes: &[u8]) -> Result<PngContent, String> {
    let decoder = png::Decoder::new(std::io::Cursor::new(png_bytes));
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("decode screenshot PNG header: {e}"))?;
    let mut buf = vec![0u8; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("decode screenshot PNG frame: {e}"))?;

    let channels = info.color_type.samples();
    let bytes_per_sample = match info.bit_depth {
        png::BitDepth::Eight => 1,
        png::BitDepth::Sixteen => 2,
        // 1/2/4-bit indexed/grayscale frames are tiny and not produced by either
        // capture path; treat them as content rather than risk a false "blank".
        _ => return Ok(PngContent::HasContent),
    };
    let pixel_stride = channels * bytes_per_sample;
    if pixel_stride == 0 {
        return Ok(PngContent::HasContent);
    }
    let frame = &buf[..info.buffer_size().min(buf.len())];
    let mut pixels = frame.chunks_exact(pixel_stride);
    let Some(first) = pixels.next() else {
        // Zero-pixel frame: nothing rendered → blank.
        return Ok(PngContent::Blank);
    };
    // Compare only the most-significant byte of each sample so the tolerance is
    // meaningful for both 8- and 16-bit depths.
    let msb = |px: &[u8], ch: usize| px[ch * bytes_per_sample];
    for px in pixels {
        for ch in 0..channels {
            let d = msb(px, ch).abs_diff(msb(first, ch));
            if d > BLANK_CHANNEL_TOLERANCE {
                return Ok(PngContent::HasContent);
            }
        }
    }
    Ok(PngContent::Blank)
}

/// Rejects a degenerate capture before it is reported as a success (#1301).
///
/// Returns `Err` with a caller-facing message when the PNG is blank (single
/// solid colour) or cannot be decoded, and `Ok(())` when it carries content.
/// This is the single shared guard both capture paths run, so a blank frame can
/// never reach `parish_file_bug` as a successful `ScreenshotInfo`.
pub fn reject_blank_capture(png_bytes: &[u8]) -> Result<(), String> {
    match classify_png_content(png_bytes)? {
        PngContent::HasContent => Ok(()),
        PngContent::Blank => Err(
            "captured screenshot is blank (single solid colour) — the app window \
             was not capturable (e.g. not composited / off-screen); refusing to \
             report an empty image as a successful capture"
                .into(),
        ),
    }
}

/// Internal save-screenshot implementation shared with the MCP bridge / web
/// route. Decodes the `data_url`, rejects a blank/degenerate frame (#1301),
/// writes the PNG, and updates [`AppState::latest_screenshot_path`].
pub(crate) async fn do_save_screenshot(
    state: &Arc<AppState>,
    data_url: String,
) -> Result<ScreenshotInfo, String> {
    let bytes = decode_data_url_png(&data_url)?;
    reject_blank_capture(&bytes)?;
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

const SCREENSHOT_RECEIPT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

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

/// Classification of the host display session, used to fail a capture fast when
/// the OS cannot composite the app window at all (#1402).
///
/// `Active` — an on-console, unlocked session (capture can work). `Locked` — the
/// screen is locked, so the window server isn't compositing app windows and both
/// the native xcap path and the html-to-image fallback would hang. `LoggedOut` —
/// no active console session (login window / fast-user-switched away).
/// `Unknown` — the platform could not be queried; never block on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplaySession {
    Active,
    Locked,
    LoggedOut,
    Unknown,
}

/// Maps the two macOS session-dictionary signals to a [`DisplaySession`].
///
/// `on_console` is `kCGSSessionOnConsoleKey` (this session owns the console);
/// `screen_locked` is `CGSSessionScreenIsLocked` (present + true when locked). A
/// locked screen takes precedence; an off-console session is logged-out; an
/// on-console unlocked session is active; absent signals are `Unknown` so an
/// indeterminate probe never blocks a capture. Pure for unit testing.
#[cfg(any(target_os = "macos", test))]
fn classify_session(on_console: Option<bool>, screen_locked: Option<bool>) -> DisplaySession {
    if screen_locked == Some(true) {
        return DisplaySession::Locked;
    }
    match on_console {
        Some(true) => DisplaySession::Active,
        Some(false) => DisplaySession::LoggedOut,
        None => DisplaySession::Unknown,
    }
}

/// Returns `Err` with a clear, condition-specific message when a capture cannot
/// possibly succeed because the display is not being composited; `Ok(())`
/// otherwise. `Unknown` is treated as "proceed". Pure for unit testing.
fn precheck_session(session: DisplaySession) -> Result<(), String> {
    match session {
        DisplaySession::Locked => Err("screen is locked — macOS is not compositing the app \
             window, so it cannot be captured; unlock the screen and retry"
            .into()),
        DisplaySession::LoggedOut => Err("no active desktop session — the user is logged out or \
             switched away (login window / fast user switching), so there is no compositor to \
             capture; log back in and retry"
            .into()),
        DisplaySession::Active | DisplaySession::Unknown => Ok(()),
    }
}

/// The caller-facing message for "the app window could not be enumerated for
/// native capture". Named so it is unit-testable and consistent across paths.
#[cfg(any(target_os = "macos", target_os = "windows", test))]
fn no_window_error() -> String {
    "no Parish/Rundale window found to capture — the window may be minimized, on another Space \
     or display, or the screen is asleep/locked"
        .to_string()
}

/// Queries the current macOS login-session state via
/// `CGSessionCopyCurrentDictionary`, mapping it through [`classify_session`].
///
/// Non-macOS hosts return `Unknown` so the precheck proceeds, preserving prior
/// behavior (the capture timeout still bounds a stuck attempt there).
#[cfg(target_os = "macos")]
fn current_display_session() -> DisplaySession {
    use std::ffi::{CString, c_void};
    use std::os::raw::c_char;

    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGSessionCopyCurrentDictionary() -> *const c_void;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFDictionaryGetValue(dict: *const c_void, key: *const c_void) -> *const c_void;
        fn CFBooleanGetValue(boolean: *const c_void) -> u8;
        fn CFStringCreateWithCString(
            alloc: *const c_void,
            c_str: *const c_char,
            encoding: u32,
        ) -> *const c_void;
        fn CFRelease(cf: *const c_void);
    }
    const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;

    // SAFETY: every returned pointer is null-checked before use. The session
    // dictionary (create-rule) and each key string (create-rule) are released
    // exactly once; dictionary values are get-rule (borrowed) and never released.
    unsafe {
        let dict = CGSessionCopyCurrentDictionary();
        if dict.is_null() {
            // No session dictionary at all => no console session is attached.
            return DisplaySession::LoggedOut;
        }
        let read_bool = |key: &str| -> Option<bool> {
            let c_key = CString::new(key).ok()?;
            let cf_key = CFStringCreateWithCString(
                std::ptr::null(),
                c_key.as_ptr(),
                K_CF_STRING_ENCODING_UTF8,
            );
            if cf_key.is_null() {
                return None;
            }
            let value = CFDictionaryGetValue(dict, cf_key);
            CFRelease(cf_key);
            if value.is_null() {
                None
            } else {
                Some(CFBooleanGetValue(value) != 0)
            }
        };
        let on_console = read_bool("kCGSSessionOnConsoleKey");
        let screen_locked = read_bool("CGSSessionScreenIsLocked");
        CFRelease(dict);
        classify_session(on_console, screen_locked)
    }
}

#[cfg(not(target_os = "macos"))]
fn current_display_session() -> DisplaySession {
    // Lock / login-session detection is macOS-specific for now; other platforms
    // proceed as before (the existing capture timeout still bounds a stuck call).
    DisplaySession::Unknown
}

/// Wakes the display on macOS so a screen that is merely ASLEEP — which reports
/// `CGSSessionScreenIsLocked = true` and would otherwise fail the precheck as if
/// the session were password-locked — is composited again before a capture.
///
/// Best-effort: spawns `caffeinate -u` (declare-user-activity), bounded with
/// `-t 5` so it does not hold the screen on. A genuinely locked session stays
/// locked after this, so the re-probe in [`do_take_screenshot`] still fails
/// correctly; only a slept-but-unlocked screen is recovered. A spawn failure
/// leaves the original behavior unchanged. No-op on non-macOS hosts (their
/// session probe never returns `Locked`, so this is never reached there).
#[cfg(target_os = "macos")]
fn wake_display() {
    // `-u` wakes the display (declare user activity); `-d` HOLDS a
    // PreventUserIdleDisplaySleep power assertion — the same mechanism apps use to
    // keep the screen on — so the panel cannot idle back to sleep mid-capture.
    // Without `-d`, `-u` only flicks the display on momentarily and the screen
    // re-sleeps before the capture completes, yielding a blank/timed-out frame.
    // `-t 90` bounds the assertion past the capture deadline (default 45 s); the
    // process releases it afterward so the screen is free to sleep again.
    let _ = tokio::process::Command::new("caffeinate")
        .args(["-u", "-d", "-t", "90"])
        .spawn();
}

#[cfg(not(target_os = "macos"))]
fn wake_display() {}

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

/// Whether the game clock was player-paused at the moment a capture began.
///
/// Used to restore the pause state after window-raise capture (#1355): raising
/// the window to front can trigger a focus event that, depending on whether
/// #1357's auto-focus-pause flag is set, might resume the clock. Reading and
/// restoring the state around the capture is a safe, explicit guard.
///
/// Pure — no Tauri or xcap types — so it is exercisable in unit tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerPauseState {
    /// The clock was player-paused before the capture.
    Paused,
    /// The clock was not player-paused before the capture.
    Running,
}

/// Reads the current player-pause state from the game clock.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) async fn read_player_pause_state(state: &Arc<AppState>) -> PlayerPauseState {
    let world = state.world.lock().await;
    if world.clock.is_paused() {
        PlayerPauseState::Paused
    } else {
        PlayerPauseState::Running
    }
}

/// Restores the player-pause state to what it was before a capture.
///
/// Called after the native window-raise capture path has finished so that
/// the act of foregrounding the window (which may have triggered a focus
/// event) does not permanently change the clock state. Only player-pause is
/// touched; inference-pause is independent and left alone.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) async fn restore_player_pause_state(state: &Arc<AppState>, prior: PlayerPauseState) {
    let mut world = state.world.lock().await;
    match prior {
        PlayerPauseState::Paused => world.clock.pause(),
        PlayerPauseState::Running => world.clock.resume(),
    }
}

/// Raises the Tauri main window to the front and gives it focus so that
/// `CGWindowListCopyWindowInfo(OptionOnScreenOnly)` can enumerate it for
/// xcap capture (#1355).
///
/// xcap on macOS calls `CGWindowListCopyWindowInfo` with the
/// `OptionOnScreenOnly` flag, which only lists windows that are composited
/// by the Window Server — a window on a different Space, behind a full-screen
/// app, or with the display asleep is not composited and cannot be found.
/// Calling `unminimize` + `show` + `set_focus` on the Tauri `WebviewWindow`
/// brings the window back onto the active Space so the subsequent xcap
/// enumeration succeeds.
///
/// Returns `Ok(true)` when the window was found and raised, `Ok(false)` when
/// it was already focused (no action needed), and `Err` when the window label
/// `"main"` does not resolve — the caller should fall through to the
/// html-to-image path or return an immediate error rather than hanging.
///
/// Side effect note: raising the window to front may fire a focus event. The
/// caller is responsible for reading the pause state before this call and
/// restoring it afterwards via [`restore_player_pause_state`]. When #1357's
/// focus-auto-pause flag is active this restore is load-bearing; without that
/// flag it is a no-op double-apply and still safe.
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn raise_main_window(app: &tauri::AppHandle) -> Result<bool, String> {
    use tauri::Manager as _;
    let win = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found — cannot raise for capture".to_string())?;

    // If the window is already focused we need no intervention; return early so
    // we do not disturb the user's window arrangement unnecessarily.
    if win.is_focused().unwrap_or(false) {
        return Ok(false);
    }

    // Unminimize first: a minimised window is not on-screen even after set_focus.
    if win.is_minimized().unwrap_or(false) {
        win.unminimize()
            .map_err(|e| format!("unminimize main window: {e}"))?;
    }

    // show() restores the window if it was hidden; set_focus() brings it to the
    // front of the window stack and activates the app. Together they ensure the
    // Window Server composites the window so xcap can capture it.
    win.show().map_err(|e| format!("show main window: {e}"))?;
    win.set_focus()
        .map_err(|e| format!("focus main window: {e}"))?;

    Ok(true)
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
    let (_, win) = best.ok_or_else(no_window_error)?;
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
///
/// On macOS/Windows, if the native xcap enumeration fails (window not on the
/// active Space, display asleep, etc.), this path first raises the Tauri main
/// window to the front via [`raise_main_window`], records the player-pause
/// state before raising, attempts capture, then restores the pause state so
/// the window-raise focus event cannot leave the clock in the wrong state
/// (#1355).
#[cfg(any(target_os = "macos", target_os = "windows"))]
async fn do_take_screenshot_native(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) -> Result<ScreenshotInfo, String> {
    // First attempt: capture without disturbing the window stack.
    match tokio::task::spawn_blocking(capture_app_window_png)
        .await
        .map_err(|e| format!("capture task join: {e}"))?
    {
        Ok(png) => {
            // Fast path: window was already composited and captured cleanly.
            reject_blank_capture(&png)?;
            let now = chrono::Utc::now();
            let info = write_screenshot_to_disk(&state.saves_dir, &png, now)?;
            *state.latest_screenshot_path.lock().await = Some(std::path::PathBuf::from(&info.path));
            return Ok(info);
        }
        Err(first_err) => {
            tracing::debug!(
                "native capture attempt 1 failed ({first_err}); \
                 raising main window and retrying"
            );
        }
    }

    // Second attempt: raise the window to front so CGWindowListCopyWindowInfo
    // can enumerate it (#1355). Read pause state before raising so we can
    // restore it if the focus event changes it.
    let prior_pause = read_player_pause_state(state).await;

    // raise_main_window is a synchronous Tauri call; run it on the async
    // executor (it does no blocking I/O, just IPC to the window server).
    raise_main_window(app)?;

    // Brief yield: the Window Server may not finish compositing the window
    // before the next xcap enumeration if we call it immediately. A short
    // sleep is preferable to a busy-poll; 120 ms is enough for one display
    // refresh cycle on a 60 Hz panel without noticeably delaying the capture.
    tokio::time::sleep(std::time::Duration::from_millis(120)).await;

    let png_result = tokio::task::spawn_blocking(capture_app_window_png)
        .await
        .map_err(|e| format!("capture task join: {e}"))?;

    // Restore pause state regardless of whether the capture succeeded or
    // failed — the window-raise must never leave the clock in a different
    // state from what it entered with.
    restore_player_pause_state(state, prior_pause).await;

    let png = png_result?;

    // Guard against a degenerate capture (window left the compositor → solid
    // frame) before reporting success (#1301). On a blank native frame we
    // return Err so do_take_screenshot falls back to the html-to-image path,
    // which is itself guarded; if both are blank the caller sees an error
    // rather than a bundled empty image.
    reject_blank_capture(&png)?;
    let now = chrono::Utc::now();
    let info = write_screenshot_to_disk(&state.saves_dir, &png, now)?;
    *state.latest_screenshot_path.lock().await = Some(std::path::PathBuf::from(&info.path));
    Ok(info)
}

/// Agent-triggered screenshot capture.
///
/// Tries native window capture first ([`do_take_screenshot_native`]) so the
/// WebGL minimap appears in the image. The native path raises the Tauri main
/// window to the front if xcap cannot find it on the active Space, captures,
/// then restores the prior pause state (#1355). If native capture is
/// unavailable (missing screen-recording permission, headless, or window-raise
/// also failed), it falls back to the frontend `html-to-image` round-trip in
/// [`do_take_screenshot_frontend`] (which carries the #1160 deadline +
/// late-capture handling). The map may be blank in the fallback, but a
/// screenshot is still produced.
pub(crate) async fn do_take_screenshot(
    state: &Arc<AppState>,
    app: &tauri::AppHandle,
) -> Result<ScreenshotInfo, String> {
    if !state
        .graphical_ready
        .load(std::sync::atomic::Ordering::Acquire)
    {
        return Err("graphical UI is not ready for screenshots".into());
    }
    // Fail fast when the OS cannot composite the window at all (screen locked /
    // logged out). Otherwise the native path errors quickly but the html-to-image
    // fallback waits on a webview that can't render while locked, hanging for the
    // full capture deadline before a vague timeout (#1402).
    //
    // A merely ASLEEP display reports `CGSSessionScreenIsLocked = true`, so a
    // `Locked` probe can mean either a password-locked session OR just a slept
    // screen. Wake the display and re-probe: a genuinely locked screen stays
    // `Locked` (precheck still fails), but a slept-but-unlocked screen clears the
    // flag and composites fine once awake — so a screenshot no longer fails black
    // just because the panel idled off (#1402 follow-up).
    let session = current_display_session();
    let session = if matches!(session, DisplaySession::Locked) {
        wake_display();
        // A cold wake from display sleep needs the window server several seconds
        // to relight the panel and re-composite app windows; capturing too soon
        // yields a blank frame. 5 s is comfortably inside the 45 s capture deadline.
        tokio::time::sleep(std::time::Duration::from_millis(5000)).await;
        current_display_session()
    } else {
        session
    };
    precheck_session(session)?;

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    match do_take_screenshot_native(state, app).await {
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
    let (started_tx, started_rx) = oneshot::channel();
    let (completion_tx, completion_rx) = oneshot::channel();
    {
        let mut pending = state.pending_screenshots.lock().await;
        pending.insert(
            request_id.clone(),
            PendingScreenshot {
                started: Some(started_tx),
                completion: completion_tx,
            },
        );
    }
    if let Err(e) = app.emit(
        crate::events::EVENT_REQUEST_SCREENSHOT,
        serde_json::json!({"request_id": request_id}),
    ) {
        let mut pending = state.pending_screenshots.lock().await;
        pending.remove(&request_id);
        return Err(e.to_string());
    }
    match tokio::time::timeout(SCREENSHOT_RECEIPT_TIMEOUT, started_rx).await {
        Ok(Ok(())) => {}
        Ok(Err(_)) => {
            state.pending_screenshots.lock().await.remove(&request_id);
            return Err(
                "graphical UI cancelled the screenshot request before acknowledging it".into(),
            );
        }
        Err(_) => {
            state.pending_screenshots.lock().await.remove(&request_id);
            return Err(format!(
                "graphical UI did not acknowledge the screenshot request within {} s",
                SCREENSHOT_RECEIPT_TIMEOUT.as_secs()
            ));
        }
    }
    match tokio::time::timeout(timeout, completion_rx).await {
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
                    "screenshot capture timed out after {} s — the app window may be occluded, \
                     off-screen, or on an inactive display; bring it to the foreground and retry",
                    timeout.as_secs()
                )),
            }
        }
    }
}

/// Called as soon as the frontend receives a screenshot request. This is a
/// receipt, not a success response; rasterization remains covered by the
/// normal capture deadline.
#[tauri::command]
pub async fn notify_screenshot_started(
    request_id: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    let mut pending = state.pending_screenshots.lock().await;
    if let Some(request) = pending.get_mut(&request_id) {
        if let Some(tx) = request.started.take() {
            let _ = tx.send(());
        }
    }
    Ok(())
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
    if let Some(request) = pending.remove(&request_id) {
        let _ = request.completion.send(Ok(info));
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
    if let Some(request) = pending.remove(&request_id) {
        let _ = request.completion.send(Err(error));
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

    /// Encodes an 8×8 RGBA PNG from a per-pixel colour closure, for exercising
    /// the blank-capture guard with synthetic frames (#1301). Returns the raw
    /// PNG byte payload.
    fn make_rgba_png(mut color: impl FnMut(u32, u32) -> [u8; 4]) -> Vec<u8> {
        const W: u32 = 8;
        const H: u32 = 8;
        let mut raw = Vec::with_capacity((W * H * 4) as usize);
        for y in 0..H {
            for x in 0..W {
                raw.extend_from_slice(&color(x, y));
            }
        }
        let mut out = Vec::new();
        {
            let mut enc = png::Encoder::new(std::io::Cursor::new(&mut out), W, H);
            enc.set_color(png::ColorType::Rgba);
            enc.set_depth(png::BitDepth::Eight);
            let mut writer = enc.write_header().unwrap();
            writer.write_image_data(&raw).unwrap();
        }
        out
    }

    /// A solid-fill PNG of the given RGBA colour — the degenerate "blank" shape.
    fn solid_png(rgba: [u8; 4]) -> Vec<u8> {
        make_rgba_png(|_, _| rgba)
    }

    /// A multi-colour PNG (a content-bearing capture) — alternating pixels.
    fn content_png() -> Vec<u8> {
        make_rgba_png(|x, y| {
            if (x + y) % 2 == 0 {
                [0x10, 0x40, 0x90, 0xff]
            } else {
                [0xf0, 0xe0, 0x30, 0xff]
            }
        })
    }

    fn content_data_url() -> String {
        use base64::Engine as _;
        let b64 = base64::engine::general_purpose::STANDARD.encode(content_png());
        format!("data:image/png;base64,{b64}")
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

        // Use a content-bearing PNG: do_save_screenshot now rejects a blank /
        // single-colour frame (#1301), and the 1×1 fixture is a single pixel.
        let info = do_save_screenshot(&state, content_data_url())
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

    // ── #1301 blank-capture detection guard ──────────────────────────────────

    #[test]
    fn classify_png_content_flags_all_white_as_blank() {
        // The exact #1301 failure: a full-resolution, syntactically valid PNG
        // that is solid white. Decodes fine, but carries no content.
        let png = solid_png([0xff, 0xff, 0xff, 0xff]);
        assert_eq!(classify_png_content(&png).unwrap(), PngContent::Blank);
    }

    #[test]
    fn classify_png_content_flags_solid_nonwhite_as_blank() {
        // Any single solid colour is degenerate, not just white (a black or
        // mid-grey capture of a non-composited window is equally empty).
        assert_eq!(
            classify_png_content(&solid_png([0x00, 0x00, 0x00, 0xff])).unwrap(),
            PngContent::Blank,
        );
        assert_eq!(
            classify_png_content(&solid_png([0x80, 0x80, 0x80, 0xff])).unwrap(),
            PngContent::Blank,
        );
    }

    #[test]
    fn classify_png_content_accepts_multicolour_render() {
        // A real capture with varied pixels passes the guard.
        assert_eq!(
            classify_png_content(&content_png()).unwrap(),
            PngContent::HasContent,
        );
    }

    #[test]
    fn classify_png_content_tolerates_near_uniform_noise() {
        // 1-LSB wobble across an otherwise solid frame must still read blank,
        // so faint anti-aliasing noise doesn't smuggle an empty frame through.
        let png = make_rgba_png(|x, _| {
            let n = (x % 2) as u8; // 0/1 difference < BLANK_CHANNEL_TOLERANCE
            [0xff - n, 0xff, 0xff, 0xff]
        });
        assert_eq!(classify_png_content(&png).unwrap(), PngContent::Blank);
    }

    #[test]
    fn classify_png_content_errors_on_undecodable_bytes() {
        // Corrupt bytes are an error, never silently "blank" (AC #5): we must
        // not report success on an image we could not even read.
        let err = classify_png_content(b"not a png at all").unwrap_err();
        assert!(err.contains("decode screenshot PNG"), "got: {err}");
    }

    #[test]
    fn reject_blank_capture_errs_on_blank_and_oks_on_content() {
        let err = reject_blank_capture(&solid_png([0xff, 0xff, 0xff, 0xff])).unwrap_err();
        assert!(err.contains("blank"), "error should name the cause: {err}");
        reject_blank_capture(&content_png()).expect("content capture must pass the guard");
    }

    #[tokio::test]
    async fn do_save_screenshot_rejects_blank_frame() {
        // End-to-end at the shared chokepoint: a blank data URL must NOT be
        // reported as a successful ScreenshotInfo, and must NOT update
        // latest_screenshot_path (so parish_file_bug can't bundle it).
        use base64::Engine as _;
        let tmp = tempfile::tempdir().unwrap();
        let mut state = test_app_state();
        let s = std::sync::Arc::get_mut(&mut state).unwrap();
        s.saves_dir = tmp.path().to_path_buf();

        let blank_b64 =
            base64::engine::general_purpose::STANDARD.encode(solid_png([0xff, 0xff, 0xff, 0xff]));
        let blank_url = format!("data:image/png;base64,{blank_b64}");

        let err = do_save_screenshot(&state, blank_url)
            .await
            .expect_err("blank capture must be rejected");
        assert!(err.contains("blank"), "got: {err}");

        // No file path was cached — the blank image is not discoverable.
        assert!(state.latest_screenshot_path.lock().await.is_none());
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

    // ── #1355 pause-state restore around window-raise capture ────────────────

    /// Helper: construct an AppState with the clock in a specific pause state.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn state_with_clock_paused(paused: bool) -> std::sync::Arc<AppState> {
        let mut state = test_app_state();
        {
            let s = std::sync::Arc::get_mut(&mut state)
                .expect("test_app_state must hand back a unique Arc");
            if paused {
                s.world.get_mut().clock.pause();
            }
            // Ensure not inference-paused so the two flags don't interfere.
        }
        state
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[tokio::test]
    async fn read_player_pause_state_reflects_clock() {
        let paused_state = state_with_clock_paused(true);
        assert_eq!(
            read_player_pause_state(&paused_state).await,
            PlayerPauseState::Paused,
            "paused clock should read as Paused"
        );

        let running_state = state_with_clock_paused(false);
        assert_eq!(
            read_player_pause_state(&running_state).await,
            PlayerPauseState::Running,
            "running clock should read as Running"
        );
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[tokio::test]
    async fn restore_player_pause_state_reapplies_paused() {
        // Start paused, simulate a focus event that resumes the clock, then
        // restore — the clock should end up paused again.
        let state = state_with_clock_paused(true);
        let prior = read_player_pause_state(&state).await;
        assert_eq!(prior, PlayerPauseState::Paused);

        // Simulate focus event resuming the clock.
        state.world.lock().await.clock.resume();
        assert!(
            !state.world.lock().await.clock.is_paused(),
            "sanity: clock should be running after resume"
        );

        restore_player_pause_state(&state, prior).await;
        assert!(
            state.world.lock().await.clock.is_paused(),
            "clock should be paused again after restore"
        );
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[tokio::test]
    async fn restore_player_pause_state_reapplies_running() {
        // Start running, simulate a focus event that pauses the clock, then
        // restore — the clock should end up running again.
        let state = state_with_clock_paused(false);
        let prior = read_player_pause_state(&state).await;
        assert_eq!(prior, PlayerPauseState::Running);

        // Simulate focus event pausing the clock.
        state.world.lock().await.clock.pause();
        assert!(
            state.world.lock().await.clock.is_paused(),
            "sanity: clock should be paused after pause()"
        );

        restore_player_pause_state(&state, prior).await;
        assert!(
            !state.world.lock().await.clock.is_paused(),
            "clock should be running again after restore"
        );
    }

    #[cfg(any(target_os = "macos", target_os = "windows"))]
    #[tokio::test]
    async fn restore_does_not_disturb_inference_pause() {
        // Inference-pause is a separate flag and must not be cleared by the
        // player-pause restore path.
        let state = state_with_clock_paused(false);
        {
            let mut world = state.world.lock().await;
            world.clock.inference_pause();
        }
        let prior = read_player_pause_state(&state).await;
        assert_eq!(prior, PlayerPauseState::Running);

        // Restore does not touch inference_paused.
        restore_player_pause_state(&state, prior).await;

        let world = state.world.lock().await;
        assert!(!world.clock.is_paused(), "player-pause should remain off");
        assert!(
            world.clock.is_inference_paused(),
            "inference-pause must not be disturbed by player-pause restore"
        );
    }

    // ── #1402 display-session precheck (fail fast on locked / logged-out) ─────

    #[test]
    fn classify_session_maps_all_signal_combinations() {
        // Locked takes precedence regardless of console state.
        assert_eq!(
            classify_session(Some(true), Some(true)),
            DisplaySession::Locked,
        );
        assert_eq!(
            classify_session(Some(false), Some(true)),
            DisplaySession::Locked,
        );
        // On-console + unlocked => active.
        assert_eq!(
            classify_session(Some(true), Some(false)),
            DisplaySession::Active,
        );
        assert_eq!(classify_session(Some(true), None), DisplaySession::Active);
        // Off-console (and not locked) => logged out.
        assert_eq!(
            classify_session(Some(false), Some(false)),
            DisplaySession::LoggedOut,
        );
        assert_eq!(
            classify_session(Some(false), None),
            DisplaySession::LoggedOut
        );
        // No signals at all => unknown (must not block a capture).
        assert_eq!(classify_session(None, None), DisplaySession::Unknown);
    }

    #[test]
    fn precheck_session_fails_fast_on_locked_and_logged_out() {
        let locked = precheck_session(DisplaySession::Locked).unwrap_err();
        assert!(
            locked.contains("locked"),
            "locked message should name the cause: {locked}"
        );
        let out = precheck_session(DisplaySession::LoggedOut).unwrap_err();
        assert!(
            out.contains("logged out") || out.contains("no active desktop session"),
            "logged-out message should name the cause: {out}"
        );
    }

    #[test]
    fn precheck_session_proceeds_on_active_and_unknown() {
        // An active session and an indeterminate probe both proceed — the latter
        // so a query failure never blocks a legitimate capture.
        precheck_session(DisplaySession::Active).expect("active session must proceed");
        precheck_session(DisplaySession::Unknown).expect("unknown session must proceed");
    }

    #[test]
    fn no_window_error_names_likely_causes() {
        let msg = no_window_error();
        assert!(msg.contains("minimized"), "got: {msg}");
        assert!(
            msg.contains("Space") || msg.contains("display"),
            "got: {msg}"
        );
        assert!(msg.contains("locked"), "got: {msg}");
    }
}
