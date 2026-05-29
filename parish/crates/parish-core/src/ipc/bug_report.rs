//! Bug-report orchestration — backend-agnostic.
//!
//! A single place that turns an in-app (or MCP-driven) bug report into a
//! well-formed GitHub issue on the configured repository. Every entry point
//! (`parish-tauri`, `parish-server`, the MCP bridge) gathers the runtime state
//! it owns — a [`WorldSnapshot`], a [`DebugSnapshot`], a screenshot, an
//! optional save summary — folds it into a [`BugReportState`] via
//! [`BugReportState::from_snapshots`], and hands it here. This module owns the
//! GitHub API calls, the issue-body composition, and the offline/dry-run
//! fallback, so the three runtimes can never drift (rule #12).
//!
//! ## Screenshot handling
//!
//! GitHub has no REST endpoint for issue attachments — the web-UI drag-drop
//! flow uploads to a private signed-upload pipeline that isn't part of the
//! public API. So we commit the PNG into the repository via the Contents API
//! and embed the returned raw URL in the issue body, which renders inline.
//!
//! ## Dry-run / no-token
//!
//! When no token is configured, or `PARISH_BUG_REPORT_DRY_RUN=1` is set, the
//! report is composed and written to disk under the caller-provided bundle
//! root instead of hitting the network. This makes the whole flow provable
//! offline (CI, agent-check, sandboxes where `api.github.com` is blocked) and
//! gives a graceful, non-panicking fallback when credentials are missing.

use std::path::Path;

use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::debug_snapshot::DebugSnapshot;
use crate::ipc::types::WorldSnapshot;

/// Default target repository when `PARISH_BUG_REPORT_REPO` is unset.
pub const DEFAULT_REPO: &str = "dmooney/rundale";
const GITHUB_API: &str = "https://api.github.com";
const USER_AGENT: &str = "parish-bug-reporter";
/// Labels applied to every auto-filed issue, so they are filterable.
const ISSUE_LABELS: &[&str] = &["bug", "agent-filed"];
/// Maximum number of log lines included per section, to keep issues readable.
const LOG_TAIL: usize = 15;

// ── Payload types (serde snake_case — mirrored in ui/src/lib/types.ts) ────────

/// A bug report submitted from the toolbar button, a debug-panel record, or
/// the `parish_file_bug` MCP tool.
///
/// Wire form is camelCase to match the frontend's `command()` payloads and the
/// `parish-server` request-body convention. The MCP tool sends snake_case
/// (`title`/`description`/`context`) which round-trips identically since those
/// are single-word fields; only `screenshot_data_url` differs, and the MCP
/// path never sends it.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BugReportRequest {
    /// Short issue title.
    pub title: String,
    /// Free-text description of what went wrong.
    #[serde(default)]
    pub description: String,
    /// Optional `data:image/png;base64,…` screenshot captured by the frontend.
    /// When absent the entry point may capture one itself (e.g. the Tauri
    /// `request-screenshot` round-trip) or proceed without.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_data_url: Option<String>,
    /// Optional debug-panel record this report was filed from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<BugContext>,
}

/// A specific debug-panel record attached as extra context, captured when the
/// user clicks the 🐛 button next to a log/record row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugContext {
    /// Record family: `"inference"`, `"event"`, `"conversation"`, etc.
    pub kind: String,
    /// Short human-readable label for the record.
    pub label: String,
    /// The serialized record itself (rendered as a JSON code block).
    #[serde(default)]
    pub detail: Value,
}

/// Outcome of a bug-report submission.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugReportResult {
    /// Whether a GitHub issue was actually created (false in dry-run/offline).
    pub created: bool,
    /// URL of the created issue, when `created`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_url: Option<String>,
    /// Number of the created issue, when `created`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub issue_number: Option<u64>,
    /// Raw URL of the uploaded screenshot, when one was committed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screenshot_url: Option<String>,
    /// On-disk path of the composed bundle, when written (dry-run/offline).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_path: Option<String>,
    /// Human-readable summary suitable for a toast or log line.
    pub message: String,
}

/// Errors that can occur while filing a bug report.
#[derive(Debug, thiserror::Error)]
pub enum BugReportError {
    /// The `bug-report` feature flag is disabled.
    #[error("the bug-report feature is disabled")]
    Disabled,
    /// The request had an empty title.
    #[error("bug report title must not be empty")]
    EmptyTitle,
    /// The screenshot data URL could not be decoded.
    #[error("invalid screenshot image: {0}")]
    BadImage(String),
    /// Transport-level HTTP failure talking to GitHub.
    #[error("http error talking to GitHub: {0}")]
    Http(String),
    /// GitHub returned a non-success status.
    #[error("GitHub API error (status {status}): {body}")]
    GitHub {
        /// HTTP status code.
        status: u16,
        /// Response body (truncated).
        body: String,
    },
    /// Failed to write the offline bundle to disk.
    #[error("failed to write bug-report bundle: {0}")]
    Io(String),
}

// ── Captured state slice ──────────────────────────────────────────────────────

/// The slice of game state embedded in a bug report.
///
/// Built by entry points from a [`WorldSnapshot`] + [`DebugSnapshot`] via
/// [`BugReportState::from_snapshots`], kept small and owned so that body
/// composition stays pure and trivially unit-testable.
#[derive(Debug, Clone, Default)]
pub struct BugReportState {
    /// Player's current location name.
    pub location: String,
    /// Time-of-day label (e.g. "Evening").
    pub time_label: String,
    /// Game hour (0–23).
    pub hour: u8,
    /// Game minute (0–59).
    pub minute: u8,
    /// Day of week.
    pub day_of_week: String,
    /// Current weather.
    pub weather: String,
    /// Current season.
    pub season: String,
    /// Festival name, if any.
    pub festival: Option<String>,
    /// Whether the clock is player-paused.
    pub paused: bool,
    /// Player's current location id.
    pub player_location_id: u32,
    /// Number of locations the player has visited.
    pub visited_count: usize,
    /// Player's name, if introduced.
    pub player_name: Option<String>,
    /// Base inference provider name.
    pub provider: String,
    /// Base inference model name.
    pub model: String,
    /// Save / branch summary line, if available.
    pub save_summary: Option<String>,
    /// Recent text-log lines (oldest → newest).
    pub text_log: Vec<String>,
    /// Recent game-event lines (preformatted).
    pub game_events: Vec<String>,
    /// Recent debug-event lines (preformatted).
    pub debug_events: Vec<String>,
    /// Recent conversation lines (preformatted).
    pub conversations: Vec<String>,
    /// Recent inference-call lines (preformatted, errors flagged).
    pub inference_calls: Vec<String>,
}

impl BugReportState {
    /// Folds a world + debug snapshot (and optional save summary) into the
    /// compact slice embedded in the issue. Each log section is capped to the
    /// most recent [`LOG_TAIL`] entries.
    pub fn from_snapshots(
        world: &WorldSnapshot,
        debug: &DebugSnapshot,
        save_summary: Option<String>,
    ) -> Self {
        let tail = |n: usize| n.saturating_sub(LOG_TAIL);

        let text_log = {
            let all = &debug.world.text_log_tail;
            all[tail(all.len())..].to_vec()
        };
        let game_events = debug
            .event_bus
            .recent_events
            .iter()
            .skip(tail(debug.event_bus.recent_events.len()))
            .map(|e| format!("[{}] {} — {}", e.timestamp, e.kind, e.summary))
            .collect();
        let debug_events = debug
            .events
            .iter()
            .skip(tail(debug.events.len()))
            .map(|e| format!("[{}] [{}] {}", e.timestamp, e.category, e.message))
            .collect();
        let conversations = debug
            .conversations
            .exchanges
            .iter()
            .skip(tail(debug.conversations.exchanges.len()))
            .map(|c| {
                format!(
                    "[{}] @ {} — player: {} | {}: {}",
                    c.timestamp, c.location_name, c.player_input, c.speaker_name, c.npc_dialogue
                )
            })
            .collect();
        let inference_calls = debug
            .inference
            .call_log
            .iter()
            .skip(tail(debug.inference.call_log.len()))
            .map(|e| {
                let status = if e.error.is_some() { "ERROR" } else { "ok" };
                let err = e.error.as_deref().map(|x| format!(" — {x}")).unwrap_or_default();
                format!(
                    "[{}] #{} {} {} {}ms{}",
                    e.timestamp, e.request_id, e.model, status, e.duration_ms, err
                )
            })
            .collect();

        Self {
            location: world.location_name.clone(),
            time_label: world.time_label.clone(),
            hour: world.hour,
            minute: world.minute,
            day_of_week: world.day_of_week.clone(),
            weather: world.weather.clone(),
            season: world.season.clone(),
            festival: world.festival.clone(),
            paused: world.paused,
            player_location_id: debug.world.player_location_id,
            visited_count: debug.world.visited_count,
            player_name: debug.world.player_name.clone(),
            provider: debug.inference.provider_name.clone(),
            model: debug.inference.model_name.clone(),
            save_summary,
            text_log,
            game_events,
            debug_events,
            conversations,
            inference_calls,
        }
    }
}

// ── GitHub configuration ──────────────────────────────────────────────────────

/// Where and how bug reports are filed, resolved from the environment.
#[derive(Debug, Clone)]
pub struct GitHubBugConfig {
    /// API token (resolved from `PARISH_BUG_REPORT_TOKEN` ‖ `GITHUB_TOKEN` ‖
    /// `GH_TOKEN`). `None` forces the offline/disk path.
    pub token: Option<String>,
    /// `owner/repo` target (default [`DEFAULT_REPO`]).
    pub repo: String,
    /// Branch to commit screenshots to. `None` ⇒ the repo's default branch.
    pub asset_branch: Option<String>,
    /// Force dry-run (compose + write to disk, never touch the network).
    pub dry_run: bool,
}

impl GitHubBugConfig {
    /// Resolves configuration from the environment.
    pub fn from_env() -> Self {
        let token =
            first_non_empty_env(&["PARISH_BUG_REPORT_TOKEN", "GITHUB_TOKEN", "GH_TOKEN"]);
        let repo =
            non_empty_env("PARISH_BUG_REPORT_REPO").unwrap_or_else(|| DEFAULT_REPO.to_string());
        let asset_branch = non_empty_env("PARISH_BUG_REPORT_ASSET_BRANCH");
        let dry_run = env_is_truthy("PARISH_BUG_REPORT_DRY_RUN");
        Self {
            token,
            repo,
            asset_branch,
            dry_run,
        }
    }

    /// Whether this submission will avoid the network — either because dry-run
    /// is forced or because no token is available to authenticate with.
    pub fn is_offline(&self) -> bool {
        self.dry_run || self.token.is_none()
    }
}

fn non_empty_env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn first_non_empty_env(keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|k| non_empty_env(k))
}

fn env_is_truthy(key: &str) -> bool {
    match std::env::var(key) {
        Ok(v) => matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

// ── Issue-body composition (pure) ─────────────────────────────────────────────

/// Builds the markdown body of the GitHub issue from the gathered state.
///
/// Pure and deterministic given its inputs — unit-tested in isolation. The
/// screenshot section is emitted only when `screenshot_url` is supplied.
pub fn compose_issue_body(
    req: &BugReportRequest,
    state: &BugReportState,
    screenshot_url: Option<&str>,
) -> String {
    let mut s = String::new();

    s.push_str("## Description\n\n");
    if req.description.trim().is_empty() {
        s.push_str("_No description provided._\n\n");
    } else {
        s.push_str(req.description.trim());
        s.push_str("\n\n");
    }

    if let Some(url) = screenshot_url {
        s.push_str("## Screenshot\n\n");
        s.push_str(&format!("![screenshot]({url})\n\n"));
    }

    s.push_str("## Game state\n\n");
    s.push_str(&format!("- **Location:** {}\n", state.location));
    s.push_str(&format!(
        "- **Time:** {} ({:02}:{:02}), {}\n",
        state.time_label, state.hour, state.minute, state.day_of_week
    ));
    s.push_str(&format!("- **Weather:** {}\n", state.weather));
    s.push_str(&format!("- **Season:** {}\n", state.season));
    if let Some(festival) = &state.festival {
        s.push_str(&format!("- **Festival:** {festival}\n"));
    }
    s.push_str(&format!("- **Paused:** {}\n", state.paused));
    s.push_str(&format!(
        "- **Player location id / visited:** {} / {}\n",
        state.player_location_id, state.visited_count
    ));
    if let Some(name) = &state.player_name {
        s.push_str(&format!("- **Player name:** {name}\n"));
    }
    s.push_str(&format!(
        "- **Inference provider/model:** {} / {}\n",
        state.provider, state.model
    ));
    if let Some(save) = &state.save_summary {
        s.push_str(&format!("- **Save:** {save}\n"));
    }
    s.push('\n');

    s.push_str("## Recent logs\n\n");
    push_log_section(&mut s, "Text log", &state.text_log);
    push_log_section(&mut s, "Game events", &state.game_events);
    push_log_section(&mut s, "Debug events", &state.debug_events);
    push_log_section(&mut s, "Recent conversations", &state.conversations);
    push_log_section(&mut s, "Inference calls", &state.inference_calls);

    if let Some(ctx) = &req.context {
        s.push_str("## Filed-from context\n\n");
        s.push_str(&format!("**{}** — {}\n\n", ctx.kind, ctx.label));
        let detail =
            serde_json::to_string_pretty(&ctx.detail).unwrap_or_else(|_| ctx.detail.to_string());
        if detail != "null" {
            s.push_str("```json\n");
            s.push_str(&detail);
            s.push_str("\n```\n\n");
        }
    }

    s.push_str("---\n_Filed via the in-app bug reporter._\n");
    s
}

fn push_log_section(out: &mut String, heading: &str, lines: &[String]) {
    out.push_str(&format!("### {heading}\n\n"));
    if lines.is_empty() {
        out.push_str("_none_\n\n");
        return;
    }
    out.push_str("```\n");
    for line in lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str("```\n\n");
}

// ── Orchestration ─────────────────────────────────────────────────────────────

/// Decodes a `data:[mime];base64,<payload>` URL into raw bytes.
pub fn decode_data_url(data_url: &str) -> Result<Vec<u8>, BugReportError> {
    let comma = data_url
        .find(',')
        .ok_or_else(|| BugReportError::BadImage("missing base64 payload".into()))?;
    let payload = &data_url[comma + 1..];
    base64::engine::general_purpose::STANDARD
        .decode(payload.trim())
        .map_err(|e| BugReportError::BadImage(e.to_string()))
}

/// Files a bug report: uploads the screenshot, composes the body, and creates
/// the GitHub issue — or, when offline, composes and writes the bundle to disk.
///
/// `bundle_root` is the caller-resolved directory under which offline bundles
/// are written (per rule #9, the caller owns runtime-path resolution).
pub async fn create_bug_report(
    http: &reqwest::Client,
    cfg: &GitHubBugConfig,
    req: &BugReportRequest,
    state: &BugReportState,
    screenshot_png: Option<&[u8]>,
    bundle_root: &Path,
) -> Result<BugReportResult, BugReportError> {
    if req.title.trim().is_empty() {
        return Err(BugReportError::EmptyTitle);
    }
    let id = uuid::Uuid::new_v4().to_string();

    if cfg.is_offline() {
        return write_offline_bundle(req, state, screenshot_png, bundle_root, &id);
    }

    // Live path: upload screenshot (best-effort inline render), then file.
    let screenshot_url = match screenshot_png {
        Some(bytes) if !bytes.is_empty() => Some(upload_screenshot(http, cfg, &id, bytes).await?),
        _ => None,
    };

    let body = compose_issue_body(req, state, screenshot_url.as_deref());
    let (issue_url, issue_number) = create_issue(http, cfg, req.title.trim(), &body).await?;

    Ok(BugReportResult {
        created: true,
        message: format!("Filed issue #{issue_number}: {issue_url}"),
        issue_url: Some(issue_url),
        issue_number: Some(issue_number),
        screenshot_url,
        bundle_path: None,
    })
}

fn write_offline_bundle(
    req: &BugReportRequest,
    state: &BugReportState,
    screenshot_png: Option<&[u8]>,
    bundle_root: &Path,
    id: &str,
) -> Result<BugReportResult, BugReportError> {
    let dir = bundle_root.join(id);
    std::fs::create_dir_all(&dir).map_err(|e| BugReportError::Io(e.to_string()))?;

    let mut screenshot_note = None;
    if let Some(bytes) = screenshot_png {
        if !bytes.is_empty() {
            let png = dir.join("screenshot.png");
            std::fs::write(&png, bytes).map_err(|e| BugReportError::Io(e.to_string()))?;
            screenshot_note = Some(png.display().to_string());
        }
    }

    let body = compose_issue_body(req, state, None);
    let issue_md = format!("# {}\n\n{body}", req.title.trim());
    let issue_path = dir.join("issue.md");
    std::fs::write(&issue_path, &issue_md).map_err(|e| BugReportError::Io(e.to_string()))?;

    let message = match &screenshot_note {
        Some(p) => format!(
            "Dry-run: composed bug report at {} (screenshot {})",
            issue_path.display(),
            p
        ),
        None => format!("Dry-run: composed bug report at {}", issue_path.display()),
    };

    Ok(BugReportResult {
        created: false,
        message,
        issue_url: None,
        issue_number: None,
        screenshot_url: None,
        bundle_path: Some(issue_path.display().to_string()),
    })
}

/// Commits the screenshot via the Contents API and returns its raw URL.
async fn upload_screenshot(
    http: &reqwest::Client,
    cfg: &GitHubBugConfig,
    id: &str,
    bytes: &[u8],
) -> Result<String, BugReportError> {
    let token = cfg.token.as_deref().unwrap_or_default();
    let path = format!("bug-reports/{id}.png");
    let url = format!("{GITHUB_API}/repos/{}/contents/{path}", cfg.repo);
    let mut body = json!({
        "message": format!("chore(bug-report): screenshot {id}"),
        "content": base64::engine::general_purpose::STANDARD.encode(bytes),
    });
    if let Some(branch) = &cfg.asset_branch {
        body["branch"] = Value::String(branch.clone());
    }

    let resp = http
        .put(&url)
        .bearer_auth(token)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .json(&body)
        .send()
        .await
        .map_err(|e| BugReportError::Http(e.to_string()))?;

    let status = resp.status();
    let payload: Value = parse_json(resp).await?;
    if !status.is_success() {
        return Err(BugReportError::GitHub {
            status: status.as_u16(),
            body: payload.to_string(),
        });
    }
    // `content.download_url` is the raw URL that renders inline in markdown.
    payload
        .get("content")
        .and_then(|c| c.get("download_url"))
        .and_then(|u| u.as_str())
        .map(String::from)
        .ok_or_else(|| BugReportError::GitHub {
            status: status.as_u16(),
            body: "missing content.download_url in response".into(),
        })
}

/// Creates the issue and returns `(html_url, number)`.
async fn create_issue(
    http: &reqwest::Client,
    cfg: &GitHubBugConfig,
    title: &str,
    body: &str,
) -> Result<(String, u64), BugReportError> {
    let token = cfg.token.as_deref().unwrap_or_default();
    let url = format!("{GITHUB_API}/repos/{}/issues", cfg.repo);
    let payload = json!({
        "title": title,
        "body": body,
        "labels": ISSUE_LABELS,
    });

    let resp = http
        .post(&url)
        .bearer_auth(token)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .json(&payload)
        .send()
        .await
        .map_err(|e| BugReportError::Http(e.to_string()))?;

    let status = resp.status();
    let value: Value = parse_json(resp).await?;
    if !status.is_success() {
        return Err(BugReportError::GitHub {
            status: status.as_u16(),
            body: value.to_string(),
        });
    }
    let issue_url = value
        .get("html_url")
        .and_then(|u| u.as_str())
        .unwrap_or_default()
        .to_string();
    let number = value
        .get("number")
        .and_then(|n| n.as_u64())
        .unwrap_or_default();
    Ok((issue_url, number))
}

async fn parse_json(resp: reqwest::Response) -> Result<Value, BugReportError> {
    let text = resp
        .text()
        .await
        .map_err(|e| BugReportError::Http(e.to_string()))?;
    if text.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| BugReportError::Http(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> BugReportState {
        BugReportState {
            location: "Darcy's Pub".into(),
            time_label: "Evening".into(),
            hour: 20,
            minute: 5,
            day_of_week: "Friday".into(),
            weather: "LightRain".into(),
            season: "Autumn".into(),
            festival: Some("Samhain".into()),
            paused: false,
            player_location_id: 7,
            visited_count: 4,
            player_name: Some("Aoife".into()),
            provider: "ollama".into(),
            model: "gemma".into(),
            save_summary: Some("branch 2: dusk".into()),
            text_log: vec!["You enter the pub.".into(), "Seán nods.".into()],
            game_events: vec!["[20:00] WeatherChanged — to LightRain".into()],
            debug_events: vec!["[20:01] [system] tick".into()],
            conversations: vec!["[20:04] @ Darcy's Pub — player: Evening | Seán: Aye.".into()],
            inference_calls: vec!["[20:03] #3 gemma ERROR 900ms — timeout".into()],
        }
    }

    fn request() -> BugReportRequest {
        BugReportRequest {
            title: "NPC stuck in transit".into(),
            description: "Seán never arrives at the pub.".into(),
            screenshot_data_url: None,
            context: None,
        }
    }

    #[test]
    fn body_has_core_sections() {
        let body = compose_issue_body(&request(), &state(), None);
        assert!(body.contains("## Description"));
        assert!(body.contains("## Game state"));
        assert!(body.contains("## Recent logs"));
        assert!(body.contains("Darcy's Pub"));
        assert!(body.contains("Samhain"));
        assert!(body.contains("branch 2: dusk"));
        // No screenshot URL provided ⇒ no screenshot section.
        assert!(!body.contains("## Screenshot"));
        // Error inference entry is surfaced.
        assert!(body.contains("ERROR"));
        assert!(body.contains("timeout"));
    }

    #[test]
    fn body_includes_screenshot_only_when_url_present() {
        let body = compose_issue_body(&request(), &state(), Some("https://raw.example/x.png"));
        assert!(body.contains("## Screenshot"));
        assert!(body.contains("![screenshot](https://raw.example/x.png)"));
    }

    #[test]
    fn body_renders_filed_from_context() {
        let mut req = request();
        req.context = Some(BugContext {
            kind: "inference".into(),
            label: "call #3".into(),
            detail: json!({"request_id": 3, "error": "timeout"}),
        });
        let body = compose_issue_body(&req, &state(), None);
        assert!(body.contains("## Filed-from context"));
        assert!(body.contains("inference"));
        assert!(body.contains("\"request_id\": 3"));
    }

    #[test]
    fn empty_log_sections_render_none() {
        let body = compose_issue_body(&request(), &BugReportState::default(), None);
        assert!(body.contains("### Text log\n\n_none_"));
    }

    #[test]
    fn empty_description_is_noted() {
        let mut req = request();
        req.description = "   ".into();
        let body = compose_issue_body(&req, &state(), None);
        assert!(body.contains("_No description provided._"));
    }

    #[test]
    fn config_token_precedence_and_dry_run() {
        let keys = [
            "PARISH_BUG_REPORT_TOKEN",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "PARISH_BUG_REPORT_REPO",
            "PARISH_BUG_REPORT_DRY_RUN",
        ];
        let saved: Vec<_> = keys.iter().map(|k| (*k, std::env::var(k).ok())).collect();
        for k in keys {
            unsafe { std::env::remove_var(k) };
        }

        unsafe {
            std::env::set_var("GH_TOKEN", "from-gh");
            std::env::set_var("GITHUB_TOKEN", "from-github");
            std::env::set_var("PARISH_BUG_REPORT_TOKEN", "from-parish");
        }
        let cfg = GitHubBugConfig::from_env();
        assert_eq!(cfg.token.as_deref(), Some("from-parish"));
        assert_eq!(cfg.repo, DEFAULT_REPO);
        assert!(!cfg.dry_run);
        assert!(!cfg.is_offline());

        unsafe {
            std::env::remove_var("PARISH_BUG_REPORT_TOKEN");
            std::env::set_var("PARISH_BUG_REPORT_DRY_RUN", "1");
            std::env::set_var("PARISH_BUG_REPORT_REPO", "acme/widgets");
        }
        let cfg = GitHubBugConfig::from_env();
        assert_eq!(cfg.token.as_deref(), Some("from-github"));
        assert_eq!(cfg.repo, "acme/widgets");
        assert!(cfg.dry_run);
        assert!(cfg.is_offline());

        for (k, v) in saved {
            match v {
                Some(val) => unsafe { std::env::set_var(k, val) },
                None => unsafe { std::env::remove_var(k) },
            }
        }
    }

    #[tokio::test]
    async fn dry_run_writes_bundle_without_network() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = GitHubBugConfig {
            token: None,
            repo: DEFAULT_REPO.into(),
            asset_branch: None,
            dry_run: true,
        };
        let png = [0x89u8, 0x50, 0x4e, 0x47]; // PNG magic bytes
        let result = create_bug_report(
            &reqwest::Client::new(),
            &cfg,
            &request(),
            &state(),
            Some(&png),
            tmp.path(),
        )
        .await
        .expect("dry-run should succeed offline");

        assert!(!result.created);
        let bundle = result.bundle_path.expect("bundle path");
        let contents = std::fs::read_to_string(&bundle).unwrap();
        assert!(contents.contains("# NPC stuck in transit"));
        assert!(contents.contains("## Game state"));
        let png_path = std::path::Path::new(&bundle).with_file_name("screenshot.png");
        assert!(png_path.exists());
    }

    #[tokio::test]
    async fn empty_title_is_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = GitHubBugConfig {
            token: None,
            repo: DEFAULT_REPO.into(),
            asset_branch: None,
            dry_run: true,
        };
        let mut req = request();
        req.title = "  ".into();
        let err = create_bug_report(
            &reqwest::Client::new(),
            &cfg,
            &req,
            &state(),
            None,
            tmp.path(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, BugReportError::EmptyTitle));
    }

    #[test]
    fn decode_data_url_roundtrip() {
        let encoded = base64::engine::general_purpose::STANDARD.encode([1u8, 2, 3, 4]);
        let url = format!("data:image/png;base64,{encoded}");
        assert_eq!(decode_data_url(&url).unwrap(), vec![1, 2, 3, 4]);
        assert!(decode_data_url("not-a-data-url").is_err());
    }
}
