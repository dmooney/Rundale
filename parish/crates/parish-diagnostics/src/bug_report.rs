//! Bug-report orchestration — backend-agnostic.
//!
//! A single place that turns an in-app (or MCP-driven) bug report into a
//! well-formed GitHub issue on the configured repository. Every entry point
//! (`parish-tauri`, `parish-server`, the MCP bridge) gathers the runtime state
//! it owns — a world snapshot (any [`WorldSnapshotFields`]), a [`DebugSnapshot`],
//! a screenshot, an optional save summary — folds it into a [`BugReportState`]
//! via
//! [`BugReportState::from_snapshots`], and hands it here. This module owns the
//! GitHub API calls, the issue-body composition, and the offline/dry-run
//! fallback, so the three runtimes can never drift (rule #12).
//!
//! ## Screenshot handling
//!
//! Screenshots are uploaded as assets on the stable `bug-evidence` GitHub
//! release and the returned browser URL is embedded in the issue body.
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

/// The world-snapshot fields a bug report folds into its game-state slice.
///
/// `parish-diagnostics` is a backend-agnostic leaf crate and cannot depend on
/// `parish-core` (where the concrete `WorldSnapshot` IPC type lives) without a
/// dependency cycle. This trait is the seam: `parish-core` implements it for
/// its `WorldSnapshot` so [`BugReportState::from_snapshots`] reads exactly the
/// scalar fields it needs without reaching back into the orchestration crate.
pub trait WorldSnapshotFields {
    /// Player's current location name.
    fn location_name(&self) -> &str;
    /// Time-of-day label (e.g. "Evening").
    fn time_label(&self) -> &str;
    /// Game hour (0–23).
    fn hour(&self) -> u8;
    /// Game minute (0–59).
    fn minute(&self) -> u8;
    /// Day-of-week name.
    fn day_of_week(&self) -> &str;
    /// Current weather label.
    fn weather(&self) -> &str;
    /// Current season label.
    fn season(&self) -> &str;
    /// Festival name, if today is a festival day.
    fn festival(&self) -> Option<&str>;
    /// Whether the clock is player-paused.
    fn paused(&self) -> bool;
}

/// Default target repository when `PARISH_BUG_REPORT_REPO` is unset.
pub const DEFAULT_REPO: &str = "dmooney/rundale";
const GITHUB_API: &str = "https://api.github.com";
const BUG_EVIDENCE_RELEASE_TAG: &str = "bug-evidence";
const USER_AGENT: &str = "parish-bug-reporter";
/// Labels applied to every auto-filed issue, so they are filterable.
const ISSUE_LABELS: &[&str] = &["bug", "agent-filed"];
/// Maximum number of log lines included per section, to keep issues readable.
const LOG_TAIL: usize = 15;
/// GitHub's hard limit for issue bodies (characters).
const GITHUB_BODY_LIMIT: usize = 65_536;
/// Safe ceiling we target: strictly ≤ 90% of the GitHub limit
/// (65,536 × 0.90 = 58,982.4), per repo rule 15 on external API payloads.
///
/// `pub` so tests can assert against the same constant.
pub const BODY_BUDGET: usize = 58_982;
/// Byte budget allocated to the entire diagnostic payload section.
///
/// Non-diagnostic sections (description + game state + recent logs) are
/// typically 1–4 KB combined. We reserve 40 KB for the diagnostic section;
/// the rest flows to the fixed content above.
const DIAGNOSTIC_BUDGET: usize = 40_000;

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

// ── Black-box diagnostic payload (#1331) ──────────────────────────────────────
//
// The MCP automated-QA loop needs the *context stack* to reproduce local-LLM
// drift: the raw prompt/response history, the deterministic engine state at
// failure time, and the last raw player intent. We capture all three into one
// structured payload and append it to every bug report, alongside the
// screenshot + logs + game-state sections the report already carries.

/// A single raw LLM prompt/response pair, drawn from the in-memory inference
/// call log. Carries the full text (not just lengths) so a "this NPC line was
/// poisoned" report can be replayed against the exact prompt that produced it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LlmExchange {
    /// Inference request id (correlates with the chat-transcript log).
    pub request_id: u64,
    /// Wall-clock timestamp the call was logged at.
    pub timestamp: String,
    /// Model that served the request.
    pub model: String,
    /// System prompt sent, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    /// User prompt text.
    pub prompt: String,
    /// Response text (empty on error).
    pub response: String,
    /// Error message when the call failed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// "Black box" diagnostic payload attached to a bug report (#1331).
///
/// Bundled in addition to the human-readable logs so an auto-QA agent (or a
/// fix-agent) has the full machine-readable context stack to reproduce
/// context-poisoning during local inference.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DiagnosticPayload {
    /// Raw LLM prompt/response pairs (oldest → newest, capped).
    pub llm_history: Vec<LlmExchange>,
    /// The exact `get_engine_state` snapshot at the time of failure, as JSON.
    /// `Null` when the entry point could not capture one.
    #[serde(default)]
    pub engine_state: Value,
    /// The last raw user intent / action passed to the engine, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_user_intent: Option<String>,
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
/// Built by entry points from a world snapshot + [`DebugSnapshot`] via
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
    /// "Black box" diagnostic payload (#1331): raw LLM prompt/response history,
    /// the engine-state snapshot, and the last raw user intent. Built by
    /// [`BugReportState::with_diagnostic`] and rendered as a dedicated section.
    pub diagnostic: DiagnosticPayload,
}

impl BugReportState {
    /// Folds a world + debug snapshot (and optional save summary) into the
    /// compact slice embedded in the issue. Each log section is capped to the
    /// most recent [`LOG_TAIL`] entries.
    pub fn from_snapshots(
        world: &impl WorldSnapshotFields,
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
                let err = e
                    .error
                    .as_deref()
                    .map(|x| format!(" — {x}"))
                    .unwrap_or_default();
                format!(
                    "[{}] #{} {} {} {}ms{}",
                    e.timestamp, e.request_id, e.model, status, e.duration_ms, err
                )
            })
            .collect();

        // Auto-capture the raw LLM prompt/response history from the in-memory
        // call log (#1331). Unlike `inference_calls` (which keeps only the
        // summary line) this carries the full prompt + response text so a
        // context-poisoning report is reproducible. Capped to the same
        // `LOG_TAIL` window to keep the payload bounded.
        let llm_history: Vec<LlmExchange> = debug
            .inference
            .call_log
            .iter()
            .skip(tail(debug.inference.call_log.len()))
            .map(|e| LlmExchange {
                request_id: e.request_id,
                timestamp: e.timestamp.clone(),
                model: e.model.clone(),
                system_prompt: e.system_prompt.clone(),
                prompt: e.prompt_text.clone(),
                response: e.response_text.clone(),
                error: e.error.clone(),
            })
            .collect();

        Self {
            location: world.location_name().to_string(),
            time_label: world.time_label().to_string(),
            hour: world.hour(),
            minute: world.minute(),
            day_of_week: world.day_of_week().to_string(),
            weather: world.weather().to_string(),
            season: world.season().to_string(),
            festival: world.festival().map(str::to_string),
            paused: world.paused(),
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
            diagnostic: DiagnosticPayload {
                llm_history,
                engine_state: Value::Null,
                last_user_intent: None,
            },
        }
    }

    /// Layers the engine-state snapshot and last raw user intent into the
    /// diagnostic payload (#1331). The LLM history is already auto-captured by
    /// [`Self::from_snapshots`]; the caller supplies the two values it owns:
    /// the canonical `get_engine_state` JSON and the last input the player
    /// submitted to the engine.
    #[must_use]
    pub fn with_diagnostic(
        mut self,
        engine_state: Value,
        last_user_intent: Option<String>,
    ) -> Self {
        self.diagnostic.engine_state = engine_state;
        self.diagnostic.last_user_intent = last_user_intent;
        self
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
    /// Force dry-run (compose + write to disk, never touch the network).
    pub dry_run: bool,
    /// GitHub REST API base (default [`GITHUB_API`]). Overridable via
    /// `PARISH_BUG_REPORT_API_BASE` — primarily so tests can point at a mock.
    pub api_base: String,
}

impl GitHubBugConfig {
    /// Resolves configuration from the environment.
    pub fn from_env() -> Self {
        // Token precedence: explicit env vars, then a best-effort fall back to the
        // `gh` CLI credential so a logged-in developer files real issues without
        // exporting a token by hand. (`is_offline()` still forces dry-run mode.)
        let token = first_non_empty_env(&["PARISH_BUG_REPORT_TOKEN", "GITHUB_TOKEN", "GH_TOKEN"])
            .or_else(gh_cli_token);
        let repo =
            non_empty_env("PARISH_BUG_REPORT_REPO").unwrap_or_else(|| DEFAULT_REPO.to_string());
        let api_base =
            non_empty_env("PARISH_BUG_REPORT_API_BASE").unwrap_or_else(|| GITHUB_API.to_string());
        let dry_run = env_is_truthy("PARISH_BUG_REPORT_DRY_RUN");
        Self {
            token,
            repo,
            dry_run,
            api_base,
        }
    }

    /// Async resolver for use on a Tokio runtime. Identical to [`from_env`],
    /// but the blocking `gh auth token` subprocess (via [`gh_cli_token`]) runs
    /// on the blocking pool so it never stalls an async worker thread. Prefer
    /// this from IPC/request handlers; keep [`from_env`] for sync callers/tests.
    ///
    /// [`from_env`]: Self::from_env
    pub async fn from_env_async() -> Self {
        tokio::task::spawn_blocking(Self::from_env)
            .await
            .unwrap_or_else(|_| Self::from_env())
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

/// Best-effort GitHub token from the `gh` CLI (`gh auth token`).
///
/// Returns `None` when `gh` is absent from `PATH`, the user is not
/// authenticated, or the command otherwise fails — callers treat that as
/// "no token" and fall back to offline mode.
fn gh_cli_token() -> Option<String> {
    let output = std::process::Command::new("gh")
        .args(["auth", "token"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    parse_gh_token(&output.stdout)
}

/// Extracts a non-empty, trimmed token from `gh auth token` stdout.
fn parse_gh_token(stdout: &[u8]) -> Option<String> {
    let token = String::from_utf8_lossy(stdout).trim().to_string();
    if token.is_empty() { None } else { Some(token) }
}

// ── Body-budget helpers ───────────────────────────────────────────────────────

/// Truncates `text` to fit within `budget` bytes, keeping the **tail** (most
/// recent content is most useful for debugging) and prepending a
/// `[truncated N chars — oldest content dropped]\n` marker when a cut was made.
///
/// `budget` is measured in bytes (UTF-8). Splitting is on a char boundary so
/// the result is always valid UTF-8.
///
/// # Special cases
/// - `budget == 0` → returns the marker with an empty body.
/// - `text.len() <= budget` → returns `text` unchanged (no allocation).
pub fn truncate_to_budget(text: &str, budget: usize) -> String {
    if text.len() <= budget {
        return text.to_owned();
    }
    let dropped = text.len() - budget;
    // Start from `dropped` bytes in, then advance to the next char boundary.
    let mut start = dropped;
    while start < text.len() && !text.is_char_boundary(start) {
        start += 1;
    }
    let kept = &text[start..];
    // Count chars in the dropped prefix (not bytes) for an accurate marker.
    let dropped_chars = text[..start].chars().count();
    format!("[truncated {dropped_chars} chars — oldest content dropped]\n{kept}")
}

// ── Issue-body composition (pure) ─────────────────────────────────────────────

/// Builds the markdown body of the GitHub issue from the gathered state.
///
/// Pure and deterministic given its inputs — unit-tested in isolation. The
/// screenshot section is emitted only when `screenshot_url` is supplied.
///
/// The composed body is capped at [`BODY_BUDGET`] bytes before returning.
/// When the diagnostic payload would push the body over the limit, its content
/// is truncated (keeping the tail — most recent entries) with a
/// `[truncated N chars]` marker so readers know data was dropped. This prevents
/// GitHub 422 "body too long" errors (limit: [`GITHUB_BODY_LIMIT`] chars).
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

    // Compute how much budget remains for the diagnostic section.
    // We give it up to DIAGNOSTIC_BUDGET bytes, further constrained by what
    // is left of BODY_BUDGET minus what we have already written.
    let used_so_far = s.len();
    // Reserve ~512 bytes for the footer and filed-from context that follows.
    let remaining = BODY_BUDGET.saturating_sub(used_so_far).saturating_sub(512);
    let diag_budget = remaining.min(DIAGNOSTIC_BUDGET);
    push_diagnostic_section(&mut s, &state.diagnostic, diag_budget);

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

    // Hard cap: if somehow the body still exceeds the budget (e.g. a very large
    // description or context blob), truncate so the body + marker stay ≤ budget.
    if s.len() > BODY_BUDGET {
        // The marker itself is ~50 bytes; subtract 60 to ensure body+marker ≤ BODY_BUDGET.
        let target_len = BODY_BUDGET.saturating_sub(60);
        let mut end = target_len;
        while end > 0 && !s.is_char_boundary(end) {
            end -= 1;
        }
        // Count chars in the dropped suffix for an accurate marker.
        let dropped_chars = s[end..].chars().count();
        s.truncate(end);
        s.push_str(&format!(
            "\n[truncated {dropped_chars} chars — body exceeded budget]"
        ));
    }

    // Emit a warning in debug builds so a developer knows truncation happened.
    debug_assert!(
        s.len() <= GITHUB_BODY_LIMIT,
        "composed issue body ({} bytes) still exceeds the GitHub limit ({GITHUB_BODY_LIMIT})",
        s.len()
    );

    s
}

/// Renders the "black box" diagnostic payload section (#1331): the raw LLM
/// prompt/response history, the engine-state snapshot, and the last raw user
/// intent. Always emitted so a fix-agent knows the section exists even when a
/// part is empty.
///
/// `budget` is the maximum number of bytes this section may consume in `out`.
/// When a sub-section's serialized text would push the section past its budget,
/// the text is truncated (tail kept, `[truncated N chars]` marker prefixed) via
/// [`truncate_to_budget`]. This prevents GitHub 422 "body too long" errors.
fn push_diagnostic_section(out: &mut String, diag: &DiagnosticPayload, budget: usize) {
    let start_len = out.len();

    out.push_str("## Diagnostic payload\n\n");

    // ── Last user intent ──────────────────────────────────────────────────────
    out.push_str("### Last user intent\n\n");
    match diag.last_user_intent.as_deref().map(str::trim) {
        Some(intent) if !intent.is_empty() => {
            // Intent is typically short; apply budget defensively.
            let used = out.len() - start_len;
            let remaining = budget.saturating_sub(used).saturating_sub(16);
            let intent_text = truncate_to_budget(intent, remaining);
            out.push_str("```\n");
            out.push_str(&intent_text);
            out.push_str("\n```\n\n");
        }
        _ => out.push_str("_none_\n\n"),
    }

    // ── Engine state ──────────────────────────────────────────────────────────
    out.push_str("### Engine state (get_engine_state)\n\n");
    if diag.engine_state.is_null() {
        out.push_str("_none_\n\n");
    } else {
        let json = serde_json::to_string_pretty(&diag.engine_state)
            .unwrap_or_else(|_| diag.engine_state.to_string());
        // Reserve at least half the remaining budget for LLM history (more
        // diagnostic value). Engine state gets the other half.
        let used = out.len() - start_len;
        let remaining = budget.saturating_sub(used);
        // Split: engine state gets at most half of what's left (with fence overhead).
        let engine_budget = (remaining / 2).saturating_sub(16);
        let json_text = truncate_to_budget(&json, engine_budget);
        out.push_str("```json\n");
        out.push_str(&json_text);
        out.push_str("\n```\n\n");
    }

    // ── LLM prompt/response history ───────────────────────────────────────────
    out.push_str("### LLM prompt/response history\n\n");
    if diag.llm_history.is_empty() {
        out.push_str("_none_\n\n");
    } else {
        let json =
            serde_json::to_string_pretty(&diag.llm_history).unwrap_or_else(|_| "[]".to_string());
        let used = out.len() - start_len;
        let remaining = budget.saturating_sub(used).saturating_sub(16);
        let json_text = truncate_to_budget(&json, remaining);
        out.push_str("```json\n");
        out.push_str(&json_text);
        out.push_str("\n```\n\n");
    }
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
        return write_offline_bundle(req, state, screenshot_png, bundle_root, &id).await;
    }

    // Live path: upload screenshot (best-effort inline render), then file.
    // The screenshot is best-effort — the issue itself is the core value, so a
    // failed upload (e.g. an issues-only token, or a protected branch that
    // rejects the release asset upload) must not abort the report. Fall back to
    // filing without an image.
    let screenshot_url = match screenshot_png {
        Some(bytes) if !bytes.is_empty() => match upload_screenshot(http, cfg, &id, bytes).await {
            Ok(url) => Some(url),
            Err(e) => {
                tracing::warn!(
                    "bug-report: screenshot upload failed, filing issue without image: {e}"
                );
                None
            }
        },
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

async fn write_offline_bundle(
    req: &BugReportRequest,
    state: &BugReportState,
    screenshot_png: Option<&[u8]>,
    bundle_root: &Path,
    id: &str,
) -> Result<BugReportResult, BugReportError> {
    // Async I/O so a dry-run filing never blocks the Tokio executor that runs
    // the game loop / server (matches the tokio::fs convention used elsewhere
    // in the runtime handlers).
    let dir = bundle_root.join(id);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| BugReportError::Io(e.to_string()))?;

    let mut screenshot_note = None;
    if let Some(bytes) = screenshot_png.filter(|b| !b.is_empty()) {
        let png = dir.join("screenshot.png");
        tokio::fs::write(&png, bytes)
            .await
            .map_err(|e| BugReportError::Io(e.to_string()))?;
        screenshot_note = Some(png.display().to_string());
    }

    let body = compose_issue_body(req, state, None);
    let issue_md = format!("# {}\n\n{body}", req.title.trim());
    let issue_path = dir.join("issue.md");
    tokio::fs::write(&issue_path, &issue_md)
        .await
        .map_err(|e| BugReportError::Io(e.to_string()))?;

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

/// Uploads the screenshot to the stable bug-evidence release and returns its
/// browser download URL. The release lookup is deliberately separate from the
/// upload so tests and callers can prove that no Contents API commit occurs.
async fn upload_screenshot(
    http: &reqwest::Client,
    cfg: &GitHubBugConfig,
    id: &str,
    bytes: &[u8],
) -> Result<String, BugReportError> {
    let token = cfg.token.as_deref().unwrap_or_default();
    let release_url = format!(
        "{}/repos/{}/releases/tags/{BUG_EVIDENCE_RELEASE_TAG}",
        cfg.api_base, cfg.repo
    );
    let release_resp = http
        .get(&release_url)
        .bearer_auth(token)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| BugReportError::Http(e.to_string()))?;
    let release_status = release_resp.status();
    let release: Value = parse_json(release_resp).await?;
    if !release_status.is_success() {
        return Err(BugReportError::GitHub {
            status: release_status.as_u16(),
            body: release.to_string(),
        });
    }
    let upload_url = release
        .get("upload_url")
        .and_then(|u| u.as_str())
        .map(|u| u.replace("{?name,label}", ""))
        .ok_or_else(|| BugReportError::GitHub {
            status: release_status.as_u16(),
            body: "missing release.upload_url in response".into(),
        })?;
    let upload_url = format!("{upload_url}?name={id}.png");
    let resp = http
        .post(&upload_url)
        .bearer_auth(token)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .header(reqwest::header::CONTENT_TYPE, "image/png")
        .body(bytes.to_vec())
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
    payload
        .get("browser_download_url")
        .and_then(|u| u.as_str())
        .map(String::from)
        .ok_or_else(|| BugReportError::GitHub {
            status: status.as_u16(),
            body: "missing asset.browser_download_url in response".into(),
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
    let url = format!("{}/repos/{}/issues", cfg.api_base, cfg.repo);
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
            diagnostic: DiagnosticPayload {
                llm_history: vec![LlmExchange {
                    request_id: 3,
                    timestamp: "20:03".into(),
                    model: "gemma".into(),
                    system_prompt: Some("You are Seán.".into()),
                    prompt: "Player says: Evening".into(),
                    response: "Aye, grand evening.".into(),
                    error: None,
                }],
                engine_state: json!({"active_scene": {"location_name": "Darcy's Pub"}}),
                last_user_intent: Some("go to the pub".into()),
            },
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

    fn offline_cfg() -> GitHubBugConfig {
        GitHubBugConfig {
            token: None,
            repo: DEFAULT_REPO.into(),
            dry_run: true,
            api_base: GITHUB_API.into(),
        }
    }

    fn live_cfg(api_base: &str) -> GitHubBugConfig {
        GitHubBugConfig {
            token: Some("test-token".into()),
            repo: "acme/widgets".into(),
            dry_run: false,
            api_base: api_base.into(),
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

    // ── Diagnostic payload (#1331) ────────────────────────────────────────────

    #[test]
    fn body_renders_diagnostic_payload_with_all_three_parts() {
        let body = compose_issue_body(&request(), &state(), None);
        assert!(body.contains("## Diagnostic payload"));
        // 1. Last user intent.
        assert!(body.contains("### Last user intent"));
        assert!(body.contains("go to the pub"));
        // 2. Engine-state snapshot JSON.
        assert!(body.contains("### Engine state (get_engine_state)"));
        assert!(body.contains("\"active_scene\""));
        // 3. Raw LLM prompt/response history (full text, not just lengths).
        assert!(body.contains("### LLM prompt/response history"));
        assert!(body.contains("You are Seán."));
        assert!(body.contains("Aye, grand evening."));
        assert!(body.contains("\"request_id\": 3"));
    }

    #[test]
    fn diagnostic_section_renders_none_placeholders_when_empty() {
        // Default state has an empty diagnostic payload.
        let body = compose_issue_body(&request(), &BugReportState::default(), None);
        assert!(body.contains("## Diagnostic payload"));
        assert!(body.contains("### Last user intent\n\n_none_"));
        assert!(body.contains("### Engine state (get_engine_state)\n\n_none_"));
        assert!(body.contains("### LLM prompt/response history\n\n_none_"));
    }

    #[test]
    fn with_diagnostic_layers_engine_state_and_intent() {
        let s = BugReportState::default()
            .with_diagnostic(json!({"clock": {"hour": 9}}), Some("look".into()));
        assert_eq!(s.diagnostic.engine_state["clock"]["hour"], 9);
        assert_eq!(s.diagnostic.last_user_intent.as_deref(), Some("look"));
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

    /// Regression test for #1222: game-events log section must render as a
    /// fenced code block (not `_none_`) when game events are present, and each
    /// entry must follow the `[timestamp] kind — summary` format produced by
    /// `BugReportState::from_snapshots`.
    #[test]
    fn game_events_render_as_fenced_block_when_present() {
        let s = BugReportState {
            game_events: vec![
                "[10:00 1820-03-20] NpcArrived — Brigid arrived at The Mill".into(),
                "[10:05 1820-03-20] WeatherChanged — Weather: LightRain".into(),
            ],
            ..Default::default()
        };
        let body = compose_issue_body(&request(), &s, None);

        // The section must NOT show the empty placeholder.
        assert!(
            !body.contains("### Game events\n\n_none_"),
            "game events section must not be empty when events are present"
        );
        // The section must be wrapped in a fenced code block.
        assert!(
            body.contains("### Game events\n\n```\n[10:00 1820-03-20] NpcArrived"),
            "game events must open with a fenced code block"
        );
        assert!(
            body.contains("WeatherChanged — Weather: LightRain\n```"),
            "game events code block must include all entries and close correctly"
        );
    }

    /// Regression test for #1222 ordering: `from_snapshots` takes the most
    /// recent `LOG_TAIL` entries in chronological order (oldest → newest),
    /// matching the order shown in the debug panel.
    #[test]
    fn game_events_tail_is_oldest_to_newest() {
        let body = compose_issue_body(&request(), &state(), None);
        // The fixture state has one game event.
        assert!(
            body.contains("### Game events\n\n```\n[20:00] WeatherChanged"),
            "single game event must appear in the fenced block"
        );
    }

    #[test]
    fn empty_description_is_noted() {
        let mut req = request();
        req.description = "   ".into();
        let body = compose_issue_body(&req, &state(), None);
        assert!(body.contains("_No description provided._"));
    }

    #[test]
    fn gh_token_parsing_trims_and_rejects_empty() {
        assert_eq!(
            parse_gh_token(b"gho_abc123\n"),
            Some("gho_abc123".to_string())
        );
        assert_eq!(parse_gh_token(b"  gho_xyz  "), Some("gho_xyz".to_string()));
        assert_eq!(parse_gh_token(b"\n\n  \n"), None);
        assert_eq!(parse_gh_token(b""), None);
    }

    #[test]
    fn config_token_precedence_and_dry_run() {
        let keys = [
            "PARISH_BUG_REPORT_TOKEN",
            "GITHUB_TOKEN",
            "GH_TOKEN",
            "PARISH_BUG_REPORT_REPO",
            "PARISH_BUG_REPORT_DRY_RUN",
            "PARISH_BUG_REPORT_API_BASE",
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
        assert_eq!(cfg.api_base, GITHUB_API);
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
        let cfg = offline_cfg();
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
        let cfg = offline_cfg();
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

    #[tokio::test]
    async fn live_path_uploads_then_files_issue() {
        use wiremock::matchers::{header, method, path, path_regex};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // Release lookup, then binary asset upload. (The report UUID is
        // generated inside the call, so match on method/path only.)
        Mock::given(method("GET"))
            .and(path("/repos/acme/widgets/releases/tags/bug-evidence"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "upload_url": format!("{}/upload/{{?name,label}}", server.uri())
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(r"/upload/.*"))
            .and(header("content-type", "image/png"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "browser_download_url": "https://github.com/acme/widgets/releases/download/bug-evidence/x.png"
            })))
            .mount(&server)
            .await;
        // Issues API: creation succeeds.
        Mock::given(method("POST"))
            .and(path("/repos/acme/widgets/issues"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "html_url": "https://github.com/acme/widgets/issues/42",
                "number": 42
            })))
            .mount(&server)
            .await;

        let png = [0x89u8, 0x50, 0x4e, 0x47];
        let result = create_bug_report(
            &reqwest::Client::new(),
            &live_cfg(&server.uri()),
            &request(),
            &state(),
            Some(&png),
            std::path::Path::new("/unused"),
        )
        .await
        .expect("live filing should succeed");

        assert!(result.created);
        assert_eq!(result.issue_number, Some(42));
        assert_eq!(
            result.issue_url.as_deref(),
            Some("https://github.com/acme/widgets/issues/42")
        );
        assert_eq!(
            result.screenshot_url.as_deref(),
            Some("https://github.com/acme/widgets/releases/download/bug-evidence/x.png")
        );
        let requests = server.received_requests().await.expect("requests");
        assert_eq!(
            requests.len(),
            3,
            "release lookup, asset upload, issue create"
        );
        assert_eq!(requests[0].method, "GET");
        assert_eq!(
            requests[0].url.path(),
            "/repos/acme/widgets/releases/tags/bug-evidence"
        );
        assert_eq!(requests[1].method, "POST");
        assert!(requests[1].url.path().starts_with("/upload/"));
        let query: Vec<_> = requests[1].url.query_pairs().collect();
        assert_eq!(query.len(), 1);
        assert_eq!(query[0].0, "name");
        let asset_name = query[0].1.as_ref();
        let report_id = asset_name.strip_suffix(".png").expect("PNG asset name");
        uuid::Uuid::parse_str(report_id).expect("UUID asset name");
        assert_eq!(requests[1].body, png);
        assert_eq!(requests[2].method, "POST");
        assert_eq!(requests[2].url.path(), "/repos/acme/widgets/issues");
        assert!(
            requests
                .iter()
                .all(|r| !r.url.path().contains("/contents/"))
        );
    }

    #[tokio::test]
    async fn screenshot_upload_failure_still_files_issue() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // Release lookup rejects the upload (e.g. missing release/token) —
        // the report must still be filed, without an image.
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(403).set_body_json(json!({
                "message": "Resource not accessible by personal access token"
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/repos/acme/widgets/issues"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({
                "html_url": "https://github.com/acme/widgets/issues/7",
                "number": 7
            })))
            .mount(&server)
            .await;

        let png = [0x89u8, 0x50, 0x4e, 0x47];
        let result = create_bug_report(
            &reqwest::Client::new(),
            &live_cfg(&server.uri()),
            &request(),
            &state(),
            Some(&png),
            std::path::Path::new("/unused"),
        )
        .await
        .expect("issue should still be filed when the screenshot upload fails");

        assert!(
            result.created,
            "issue must be created despite upload failure"
        );
        assert_eq!(result.issue_number, Some(7));
        assert!(
            result.screenshot_url.is_none(),
            "no screenshot URL when the upload failed"
        );
    }

    #[tokio::test]
    async fn issue_creation_failure_surfaces_error() {
        use wiremock::matchers::method;
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let server = MockServer::start().await;
        // No screenshot, and the Issues API itself fails → error propagates.
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(422).set_body_json(json!({"message": "Validation Failed"})),
            )
            .mount(&server)
            .await;

        let err = create_bug_report(
            &reqwest::Client::new(),
            &live_cfg(&server.uri()),
            &request(),
            &state(),
            None,
            std::path::Path::new("/unused"),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, BugReportError::GitHub { status: 422, .. }));
    }

    // ── Body-budget / truncation tests (#1375) ────────────────────────────────

    /// `truncate_to_budget` — no-op when content fits.
    #[test]
    fn truncate_to_budget_no_op_when_fits() {
        let text = "hello world";
        assert_eq!(truncate_to_budget(text, 100), text);
        assert_eq!(truncate_to_budget(text, text.len()), text);
    }

    /// `truncate_to_budget` — keeps the tail and prepends the marker.
    #[test]
    fn truncate_to_budget_keeps_tail_with_marker() {
        // 20 chars, budget 10 → keep last 10.
        let text = "01234567890123456789"; // 20 bytes
        let result = truncate_to_budget(text, 10);
        // The kept tail must be at the end of the original string.
        assert!(
            result.ends_with("0123456789"),
            "tail not preserved: {result:?}"
        );
        assert!(result.contains("[truncated"), "marker missing: {result:?}");
        assert!(
            result.contains("10 chars"),
            "dropped char count missing: {result:?}"
        );
    }

    /// `truncate_to_budget` — marker reports char count, not byte count, for
    /// multi-byte UTF-8 strings. Regression test for gemini finding #1.
    #[test]
    fn truncate_to_budget_reports_char_count_not_bytes() {
        // "é" is 2 bytes but 1 char. Build a string where byte count ≠ char count.
        // "aaa" (3 bytes/chars) + "é" (2 bytes, 1 char) = 5 bytes, 4 chars total.
        // With a budget of 3 bytes we keep "aé" won't fit cleanly — keep "aaa" tail.
        // More deterministic: 10 × "é" = 20 bytes, 10 chars. Budget = 10 bytes.
        // That aligns on a 2-byte boundary exactly, so start = 10, dropped prefix = first 5 × "é" (10 bytes, 5 chars).
        let text = "éééééééééé"; // 10 × 'é' = 20 bytes, 10 chars
        assert_eq!(text.len(), 20);
        assert_eq!(text.chars().count(), 10);
        let result = truncate_to_budget(text, 10); // keep last 5 chars (10 bytes)
        // Marker must say "5 chars", not "10" (bytes).
        assert!(
            result.contains("5 chars"),
            "marker must report char count (5), not byte count (10): {result:?}"
        );
        assert!(
            result.ends_with("ééééé"),
            "tail must be preserved: {result:?}"
        );
    }

    /// `truncate_to_budget` — budget 0 produces only the marker.
    #[test]
    fn truncate_to_budget_zero_budget() {
        let result = truncate_to_budget("some content", 0);
        assert!(
            result.contains("[truncated"),
            "marker missing at zero budget"
        );
        // No original content should remain (the marker itself is not original content).
        assert!(
            !result.contains("some content"),
            "original content must not appear at zero budget"
        );
    }

    /// `truncate_to_budget` — never splits on a non-char-boundary (UTF-8 safety).
    #[test]
    fn truncate_to_budget_utf8_safe() {
        // "é" is 2 bytes; with a budget of 1 we must not produce invalid UTF-8.
        let text = "aé"; // 3 bytes: 'a'(1) + 'é'(2)
        let result = truncate_to_budget(text, 1); // 1-byte budget → can only keep 'a'
        assert!(
            std::str::from_utf8(result.as_bytes()).is_ok(),
            "result is not valid UTF-8"
        );
    }

    /// Builds an oversized diagnostic payload (15 LLM exchanges × 4 KB each +
    /// a large engine-state JSON) and asserts the composed body fits in the budget.
    fn oversized_state() -> BugReportState {
        let big_text: String = "x".repeat(4_096);
        let llm_history: Vec<LlmExchange> = (0..15)
            .map(|i| LlmExchange {
                request_id: i,
                timestamp: format!("20:{i:02}"),
                model: "gemma".into(),
                system_prompt: Some(big_text.clone()),
                prompt: big_text.clone(),
                response: big_text.clone(),
                error: None,
            })
            .collect();
        // ~6 KB engine-state blob.
        let engine_state = json!({"data": "y".repeat(6_000)});
        BugReportState {
            location: "Darcy's Pub".into(),
            time_label: "Evening".into(),
            hour: 20,
            minute: 0,
            day_of_week: "Friday".into(),
            weather: "LightRain".into(),
            season: "Autumn".into(),
            festival: None,
            paused: false,
            player_location_id: 7,
            visited_count: 4,
            player_name: None,
            provider: "ollama".into(),
            model: "gemma".into(),
            save_summary: None,
            text_log: vec![],
            game_events: vec![],
            debug_events: vec![],
            conversations: vec![],
            inference_calls: vec![],
            diagnostic: DiagnosticPayload {
                llm_history,
                engine_state,
                last_user_intent: Some("go to the pub".into()),
            },
        }
    }

    #[test]
    fn body_len_capped_under_github_limit() {
        // Literal pin: 90% of GitHub's 65,536-char issue-body limit. Keeps the
        // budget from drifting above the rule-15 ceiling unnoticed.
        assert_eq!(BODY_BUDGET, 58_982);
        let body = compose_issue_body(&request(), &oversized_state(), None);
        assert!(
            body.len() <= BODY_BUDGET,
            "body length {} exceeds BODY_BUDGET {BODY_BUDGET}",
            body.len()
        );
    }

    /// The hard-cap path (very large description that bypasses diagnostic
    /// pre-truncation) must not push the body+marker over `BODY_BUDGET`.
    /// Regression test for gemini finding #2.
    #[test]
    fn hard_cap_body_plus_marker_within_budget() {
        // Build a state with a description large enough to trigger the hard cap.
        let huge_description = "A".repeat(BODY_BUDGET + 1_000);
        let mut req = request();
        req.description = huge_description;
        let body = compose_issue_body(&req, &BugReportState::default(), None);
        assert!(
            body.len() <= BODY_BUDGET,
            "hard-cap body (including marker) length {} exceeds BODY_BUDGET {BODY_BUDGET}",
            body.len()
        );
        // The marker must be present to confirm the hard-cap ran.
        assert!(
            body.contains("[truncated") && body.contains("body exceeded budget"),
            "hard-cap truncation marker missing from oversized-description body"
        );
    }

    #[test]
    fn body_truncation_marker_present() {
        let body = compose_issue_body(&request(), &oversized_state(), None);
        assert!(
            body.contains("[truncated"),
            "truncation marker missing from oversized body"
        );
    }

    #[test]
    fn body_sections_present_after_truncation() {
        let body = compose_issue_body(&request(), &oversized_state(), None);
        assert!(
            body.contains("## Description"),
            "Description section missing"
        );
        assert!(body.contains("## Game state"), "Game state section missing");
        assert!(
            body.contains("## Recent logs"),
            "Recent logs section missing"
        );
        assert!(
            body.contains("## Diagnostic payload"),
            "Diagnostic payload section missing"
        );
    }

    #[tokio::test]
    async fn dry_run_oversized_payload_writes_valid_bundle() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = offline_cfg();
        let result = create_bug_report(
            &reqwest::Client::new(),
            &cfg,
            &request(),
            &oversized_state(),
            None,
            tmp.path(),
        )
        .await
        .expect("dry-run with oversized payload must not error");

        assert!(!result.created, "dry-run must not create a real issue");
        let bundle = result.bundle_path.expect("bundle_path must be set");
        let contents = std::fs::read_to_string(&bundle).expect("issue.md must be readable");
        // Rule #14: validate content, not just the envelope.
        assert!(!contents.is_empty(), "issue.md must not be empty");
        assert!(
            contents.contains("## Game state"),
            "issue.md must contain ## Game state section"
        );
        assert!(
            contents.contains("[truncated"),
            "issue.md must contain the truncation marker"
        );
        assert!(
            contents.len() <= BODY_BUDGET + 512, // allow for title prefix
            "bundle body too large: {} bytes",
            contents.len()
        );
    }
}
