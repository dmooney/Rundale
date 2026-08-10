//! Persistent JSONL inference call log for user-zippable bug reports.
//!
//! Every inference call recorded into the in-memory `BoundedInferenceLog`
//! also passes through here. A bounded mpsc channel hands serialised records
//! to a dedicated writer task that owns a `BufWriter<File>` and flushes per
//! line for crash-durability.
//!
//! ## Backpressure
//!
//! The worker MUST NOT block on disk, so the channel is bounded (cap
//! [`CHANNEL_CAPACITY`]) and the sender uses `try_send`. When the channel
//! is full, a counter (`dropped`) is incremented and we warn once per
//! [`DROP_WARNING_INTERVAL`] drops. Inference never blocks on the log.
//!
//! ## Failure handling
//!
//! If the file can't be opened at startup or write fails mid-session,
//! [`InferenceFileLog::set_enabled`] flips to `false`, one
//! `tracing::warn!` is emitted, and `record()` becomes a silent no-op.
//! Disk problems never crash inference.
//!
//! ## Scrubbing
//!
//! All free-text fields hitting disk pass through
//! [`crate::secret_scrub::scrub`] so API-key shapes get replaced with
//! `[REDACTED:*]` markers. The in-memory `BoundedInferenceLog` keeps the
//! unredacted entry; the debug panel applies its own `redact_call_log`
//! step at read time.

use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use chrono::Utc;
use parish_config::Provider;
use serde_json::{Map, Value};
use tokio::fs::{File, OpenOptions, create_dir_all};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc;

use crate::{InferenceLogEntry, InferencePriority};

/// Bounded channel capacity. 256 is enough headroom for several seconds of
/// worst-case inference throughput while keeping the in-flight memory cost
/// modest (each `JsonlRecord` is a single owned `String`).
const CHANNEL_CAPACITY: usize = 256;

/// Schema version embedded in every line. Bump when the field set changes.
const SCHEMA_VERSION: u32 = 1;

/// Emit a "N records were dropped" warning every N drops, not on every drop.
const DROP_WARNING_INTERVAL: u64 = 32;

/// A serialised JSONL line awaiting write.
struct JsonlRecord(String);

/// `/inference-log on|off|status|path` slash command sub-actions.
///
/// Mirrors `parish_input::commands::InferenceLogSub`; duplicated here so
/// `parish-inference` doesn't pull `parish-input` as a dependency just to
/// describe the toggle. The two enums convert via [`From`] in `parish-core`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogToggleAction {
    On,
    Off,
    Status,
    Path,
}

/// Apply a [`LogToggleAction`] to a writer and return the player-visible
/// reply. Each Parish runtime calls this from its `SystemCommandHost`
/// implementation so the wording stays consistent across CLI / server /
/// desktop.
pub fn apply_toggle(log: &InferenceFileLog, action: LogToggleAction) -> String {
    match action {
        LogToggleAction::On => {
            log.set_enabled(true);
            format!("Inference log enabled. Writing to {}", log.path().display())
        }
        LogToggleAction::Off => {
            log.set_enabled(false);
            "Inference log disabled. The file remains on disk; existing entries are unchanged."
                .to_string()
        }
        LogToggleAction::Status => {
            let state = if log.is_enabled() { "ON" } else { "OFF" };
            let path = log.path().display().to_string();
            if path.is_empty() {
                format!("Inference log: {state} (no file — disabled handle)")
            } else {
                format!("Inference log: {state} — {path}")
            }
        }
        LogToggleAction::Path => {
            let path = log.path().display().to_string();
            if path.is_empty() {
                "Inference log: (no file — disabled handle)".to_string()
            } else {
                format!("Inference log: {path}")
            }
        }
    }
}

/// Resolves the effective on/off setting using the precedence
/// CLI flag > `PARISH_INFERENCE_LOG` env var > config.
///
/// `cli_disable` is the value of a `--no-inference-log`-style flag
/// (true → disable). `config_enabled` is the TOML setting. Env values
/// matching `off`, `0`, `false`, `no` (case-insensitive) flip to disabled;
/// anything else (including unset) leaves the chain unchanged so the
/// CLI / TOML setting wins.
pub fn resolve_enabled(cli_disable: bool, config_enabled: bool) -> bool {
    if cli_disable {
        return false;
    }
    if let Ok(raw) = std::env::var("PARISH_INFERENCE_LOG") {
        let trimmed = raw.trim().to_ascii_lowercase();
        match trimmed.as_str() {
            "off" | "0" | "false" | "no" => return false,
            "on" | "1" | "true" | "yes" => return true,
            _ => {}
        }
    }
    config_enabled
}

/// Handle to the inference disk-log writer.
///
/// Cheap to `clone()` (all fields are `Arc`/`Option<Sender>`), so callers
/// can store one on `AppState` and clone into background tasks. A
/// [`Self::disabled`] handle is used for test fixtures and synthetic
/// AppStates that have no real session on disk.
#[derive(Clone)]
pub struct InferenceFileLog {
    tx: Option<mpsc::Sender<JsonlRecord>>,
    enabled: Arc<AtomicBool>,
    path: PathBuf,
    session_id: String,
    base_url_host: Option<String>,
    dropped: Arc<AtomicU64>,
}

impl InferenceFileLog {
    /// Creates a writer that's permanently disabled (no file, no task).
    ///
    /// Used by synthetic `AppState` fixtures (Tauri MCP bridge, tests) where
    /// no persistent logging is wanted.
    pub fn disabled() -> Self {
        Self {
            tx: None,
            enabled: Arc::new(AtomicBool::new(false)),
            path: PathBuf::new(),
            session_id: String::new(),
            base_url_host: None,
            dropped: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Spawns a writer task that owns the file handle.
    ///
    /// `saves_dir` is the directory where save files live; the log file is
    /// written to `{saves_dir}/inference_logs/{ISO-no-colons}-{pid}.jsonl`.
    /// `enabled` controls whether records are written; the slash command
    /// can flip this at runtime.
    /// `base_url` (when supplied) is parsed for its host and embedded in
    /// every record as `parish.base_url_host` so OpenAI-compatible
    /// providers (Ollama, Groq, OpenRouter, …) are disambiguated.
    ///
    /// The writer task is spawned even when `enabled` is `false` so that a
    /// later `/inference-log on` can begin writing without a restart. The
    /// file itself is opened lazily on the first accepted record, so a
    /// session that stays opted-out never touches the disk.
    pub fn spawn(saves_dir: &Path, enabled: bool, base_url: Option<&str>) -> Self {
        let session_id = format!("{}-{}", Utc::now().format("%Y%m%dT%H%M%SZ"), process::id());
        let dir = saves_dir.join("inference_logs");
        let path = dir.join(format!("{session_id}.jsonl"));
        let base_url_host = base_url.and_then(extract_host);

        let enabled_flag = Arc::new(AtomicBool::new(enabled));
        let dropped = Arc::new(AtomicU64::new(0));

        let (tx, rx) = mpsc::channel::<JsonlRecord>(CHANNEL_CAPACITY);
        let writer_enabled = enabled_flag.clone();
        let writer_path = path.clone();
        tokio::spawn(async move {
            run_writer(dir, writer_path, rx, writer_enabled).await;
        });

        if enabled {
            tracing::info!(
                target: "parish_inference::file_log",
                "Inference logging enabled. File: {}",
                path.display()
            );
        }

        Self {
            tx: Some(tx),
            enabled: enabled_flag,
            path,
            session_id,
            base_url_host,
            dropped,
        }
    }

    /// Returns the absolute path the writer is targeting.
    ///
    /// May be a directory the writer failed to create; check
    /// [`Self::is_enabled`] to see whether records are actually being written.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the per-session id that's embedded in every line and the
    /// filename. Useful for cross-correlation with the chat transcript log.
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Whether records are being written. Toggled by `/inference-log on|off`
    /// at runtime, or flipped to `false` automatically on disk failure.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Runtime enable/disable toggle. Cheap atomic flip; the writer task
    /// stays alive but ignores incoming records while disabled.
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    /// Returns the shared enable flag so the chat transcript writer can
    /// share the same toggle.
    pub fn enabled_flag(&self) -> Arc<AtomicBool> {
        self.enabled.clone()
    }

    /// Serialises one inference call and queues it for writing.
    ///
    /// Cheap: builds the JSON line on the calling task (so we don't pin a
    /// `Value` in the channel) and `try_send`s. Returns immediately on
    /// success or drop; never blocks. Disabled / detached handles are
    /// silent no-ops.
    pub fn record(
        &self,
        entry: &InferenceLogEntry,
        provider: &Provider,
        priority: InferencePriority,
    ) {
        if !self.is_enabled() {
            return;
        }
        let Some(tx) = self.tx.as_ref() else {
            return;
        };

        let line = match serialise(
            entry,
            provider,
            priority,
            &self.session_id,
            self.base_url_host.as_deref(),
        ) {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!(
                    target: "parish_inference::file_log",
                    "failed to serialise inference log entry (request_id={}): {err}",
                    entry.request_id
                );
                return;
            }
        };

        match tx.try_send(JsonlRecord(line)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                let prev = self.dropped.fetch_add(1, Ordering::Relaxed);
                if (prev + 1).is_multiple_of(DROP_WARNING_INTERVAL) {
                    tracing::warn!(
                        target: "parish_inference::file_log",
                        "inference log writer is behind; {} record(s) dropped so far",
                        prev + 1
                    );
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                // Writer task exited (likely from a fatal disk error). Stop
                // attempting future writes.
                self.set_enabled(false);
            }
        }
    }
}

fn extract_host(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    // authority = host[:port], up to the first '/', stripping any userinfo.
    let authority = after_scheme
        .split('/')
        .next()
        .unwrap_or(after_scheme)
        .split('@')
        .next_back()
        .unwrap_or(after_scheme);
    // IPv6 literals are bracketed (`[::1]:8080`) — keep the bracketed host and
    // drop only the trailing `:port`. Naive `split(':')` would mangle them.
    let host = if let Some(rest) = authority.strip_prefix('[') {
        match rest.split_once(']') {
            Some((inner, _port)) => inner,
            None => rest,
        }
    } else {
        authority.split(':').next().unwrap_or(authority)
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_string())
    }
}

fn provider_label(provider: &Provider) -> &str {
    // `Provider` is a mod-backed struct; its `id()` is the canonical lowercase
    // slug ("anthropic", "openai", "ollama", …) — exactly the value wanted for
    // the OpenTelemetry `gen_ai.system` field.
    provider.id()
}

fn priority_label(priority: InferencePriority) -> &'static str {
    match priority {
        InferencePriority::Interactive => "interactive",
        InferencePriority::Background => "background",
        InferencePriority::Batch => "batch",
    }
}

fn classify_error(message: &str) -> &'static str {
    let lower = message.to_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        "timeout"
    } else if lower.contains("http") || lower.contains("status") || lower.contains("404") {
        "http_error"
    } else {
        "other"
    }
}

fn serialise(
    entry: &InferenceLogEntry,
    provider: &Provider,
    priority: InferencePriority,
    session_id: &str,
    base_url_host: Option<&str>,
) -> Result<String, serde_json::Error> {
    use crate::secret_scrub::scrub;

    let mut map = Map::new();
    map.insert(
        "timestamp".to_string(),
        Value::String(Utc::now().to_rfc3339()),
    );
    map.insert(
        "gen_ai.system".to_string(),
        Value::String(provider_label(provider).to_string()),
    );
    map.insert(
        "gen_ai.request.model".to_string(),
        Value::String(entry.model.clone()),
    );
    map.insert(
        "gen_ai.provider.name".to_string(),
        Value::String(entry.provider.clone()),
    );
    map.insert(
        "parish.inference.api_mode".to_string(),
        Value::String(entry.api_mode.clone()),
    );
    map.insert(
        "parish.inference.role".to_string(),
        Value::String(entry.role.name().to_string()),
    );
    map.insert(
        "parish.inference.subrole".to_string(),
        serde_json::to_value(entry.subrole)?,
    );
    if let Some(max) = entry.max_tokens {
        map.insert(
            "gen_ai.request.max_tokens".to_string(),
            Value::Number(max.into()),
        );
    }
    if let Some(temp) = entry.temperature
        && let Some(num) = serde_json::Number::from_f64(temp as f64)
    {
        map.insert("gen_ai.request.temperature".to_string(), Value::Number(num));
    }
    map.insert(
        "gen_ai.response.duration_ms".to_string(),
        Value::Number(entry.duration_ms.into()),
    );
    if let Some(out) = entry.output_tokens {
        map.insert(
            "gen_ai.usage.output_tokens".to_string(),
            Value::Number(out.into()),
        );
    }
    for (key, value) in [
        ("gen_ai.usage.input_tokens", entry.input_tokens),
        ("gen_ai.usage.cached_tokens", entry.cached_tokens),
        ("gen_ai.usage.thought_tokens", entry.thought_tokens),
        ("gen_ai.usage.total_tokens", entry.total_tokens),
        ("parish.stream_chunks", entry.stream_chunks),
    ] {
        if let Some(value) = value {
            map.insert(key.to_string(), Value::Number(value.into()));
        }
    }
    if let Some(level) = entry.thinking_level {
        map.insert(
            "gen_ai.request.thinking_level".to_string(),
            Value::String(
                serde_json::to_value(level)?
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ),
        );
    }
    if let Some(tier) = entry.requested_service_tier {
        map.insert(
            "gen_ai.request.service_tier".to_string(),
            Value::String(
                serde_json::to_value(tier)?
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            ),
        );
    }
    if let Some(tier) = entry.effective_service_tier.as_ref() {
        map.insert(
            "gen_ai.response.service_tier".to_string(),
            Value::String(tier.clone()),
        );
    }
    if let Some(status) = entry.terminal_status.as_ref() {
        map.insert(
            "gen_ai.response.status".to_string(),
            Value::String(status.clone()),
        );
    }
    map.insert(
        "parish.retry_count".to_string(),
        Value::Number(entry.retry_count.into()),
    );
    if let Some(status) = entry.http_status {
        map.insert(
            "http.response.status_code".to_string(),
            Value::Number(status.into()),
        );
    }
    if let Some(kind) = entry.failure_kind.as_ref() {
        map.insert("error.type".to_string(), Value::String(kind.clone()));
    }
    map.insert(
        "parish.partial_output_len".to_string(),
        Value::Number((entry.partial_output_len as u64).into()),
    );
    map.insert(
        "parish.service_tier_downgraded".to_string(),
        Value::Bool(entry.tier_downgraded),
    );
    if let Some(cost) = entry
        .estimated_cost_usd
        .and_then(serde_json::Number::from_f64)
    {
        map.insert("parish.estimated_cost_usd".to_string(), Value::Number(cost));
        map.insert(
            "parish.pricing_snapshot_date".to_string(),
            Value::String("2026-08-09".to_string()),
        );
    }
    if let Some(hash) = entry.prompt_prefix_hash.as_ref() {
        map.insert(
            "parish.prompt_prefix_hash".to_string(),
            Value::String(hash.clone()),
        );
    }
    if let Some(len) = entry.prompt_prefix_len {
        map.insert(
            "parish.prompt_prefix_len".to_string(),
            Value::Number((len as u64).into()),
        );
    }
    if let Some(ttft) = entry.ttft_ms {
        map.insert("parish.ttft_ms".to_string(), Value::Number(ttft.into()));
    }
    map.insert(
        "gen_ai.prompt".to_string(),
        Value::String(scrub(&entry.prompt_text).into_owned()),
    );
    if let Some(sys) = entry.system_prompt.as_ref() {
        map.insert(
            "gen_ai.prompt.system".to_string(),
            Value::String(scrub(sys).into_owned()),
        );
    }
    map.insert(
        "gen_ai.completion".to_string(),
        Value::String(scrub(&entry.response_text).into_owned()),
    );
    if let Some(err) = entry.error.as_ref() {
        map.insert(
            "error.type".to_string(),
            Value::String(classify_error(err).to_string()),
        );
        map.insert(
            "error.message".to_string(),
            Value::String(scrub(err).into_owned()),
        );
    }
    map.insert(
        "parish.request_id".to_string(),
        Value::Number(entry.request_id.into()),
    );
    map.insert(
        "parish.session_id".to_string(),
        Value::String(session_id.to_string()),
    );
    map.insert(
        "parish.priority".to_string(),
        Value::String(priority_label(priority).to_string()),
    );
    map.insert("parish.streaming".to_string(), Value::Bool(entry.streaming));
    map.insert(
        "parish.prompt_chars".to_string(),
        Value::Number(entry.prompt_len.into()),
    );
    map.insert(
        "parish.completion_chars".to_string(),
        Value::Number(entry.response_len.into()),
    );
    if let Some(host) = base_url_host {
        map.insert(
            "parish.base_url_host".to_string(),
            Value::String(host.to_string()),
        );
    }
    map.insert(
        "parish.schema_version".to_string(),
        Value::Number(SCHEMA_VERSION.into()),
    );

    let mut line = serde_json::to_string(&Value::Object(map))?;
    line.push('\n');
    Ok(line)
}

async fn run_writer(
    dir: PathBuf,
    path: PathBuf,
    mut rx: mpsc::Receiver<JsonlRecord>,
    enabled: Arc<AtomicBool>,
) {
    // Open lazily on the first accepted record. The task is spawned even
    // when logging starts disabled (so `/inference-log on` can enable it
    // mid-session); deferring the open means an opted-out session that is
    // never toggled on leaves no empty file on disk.
    let mut writer: Option<BufWriter<File>> = None;

    // Note: the `enabled` flag is checked at the send site (`record`); the
    // writer does not re-check it to avoid racing a runtime toggle that
    // would silently drop already-accepted records. The flag is still
    // flipped on disk failure below to stop further writes.
    while let Some(JsonlRecord(line)) = rx.recv().await {
        if writer.is_none() {
            match open_log_file(&dir, &path).await {
                Ok(file) => writer = Some(BufWriter::new(file)),
                Err(err) => {
                    tracing::warn!(
                        target: "parish_inference::file_log",
                        "could not open inference log file {}: {err}; disabling disk log",
                        path.display()
                    );
                    enabled.store(false, Ordering::Relaxed);
                    return;
                }
            }
        }
        let writer = writer.as_mut().expect("writer opened above");
        if let Err(err) = writer.write_all(line.as_bytes()).await {
            tracing::warn!(
                target: "parish_inference::file_log",
                "inference log write failed: {err}; disabling disk log"
            );
            enabled.store(false, Ordering::Relaxed);
            return;
        }
        // Flush per line for crash-durability; the call is cheap because
        // BufWriter only holds whatever this single line needed.
        if let Err(err) = writer.flush().await {
            tracing::warn!(
                target: "parish_inference::file_log",
                "inference log flush failed: {err}; disabling disk log"
            );
            enabled.store(false, Ordering::Relaxed);
            return;
        }
    }
    if let Some(mut writer) = writer {
        let _ = writer.flush().await;
    }
}

/// Creates the log directory (if needed) and opens the session file for
/// append. Called lazily by [`run_writer`] on the first record.
async fn open_log_file(dir: &Path, path: &Path) -> std::io::Result<File> {
    create_dir_all(dir).await?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
}

/// Provided so `parish-core` can wire its chat transcript writer with the
/// same shape — it doesn't depend on this module directly, but the test
/// harness in this crate validates `JsonlRecord` shape via `serialise`.
#[cfg(test)]
mod tests {
    use super::*;

    fn openai() -> Provider {
        Provider::from_str_loose("openai").expect("openai builtin registered")
    }

    fn sample_entry(id: u64) -> InferenceLogEntry {
        InferenceLogEntry {
            request_id: id,
            timestamp: "12:00:00".to_string(),
            model: "test-model".to_string(),
            streaming: false,
            duration_ms: 42,
            prompt_len: 6,
            response_len: 5,
            error: None,
            system_prompt: Some("be helpful".to_string()),
            prompt_text: "prompt".to_string(),
            response_text: "world".to_string(),
            max_tokens: Some(100),
            ttft_ms: Some(15),
            output_tokens: Some(5),
            temperature: Some(0.7),
            priority: InferencePriority::Interactive,
            ..Default::default()
        }
    }

    #[test]
    fn extract_host_handles_common_urls() {
        assert_eq!(
            extract_host("https://api.openai.com/v1"),
            Some("api.openai.com".into())
        );
        assert_eq!(
            extract_host("http://localhost:11434"),
            Some("localhost".into())
        );
        assert_eq!(
            extract_host("no-scheme.example/x"),
            Some("no-scheme.example".into())
        );
    }

    #[test]
    fn serialise_includes_required_fields() {
        let entry = sample_entry(123);
        let line = serialise(
            &entry,
            &openai(),
            InferencePriority::Interactive,
            "sess-abc",
            Some("api.example.com"),
        )
        .unwrap();
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["parish.request_id"], 123);
        assert_eq!(v["parish.session_id"], "sess-abc");
        assert_eq!(v["parish.priority"], "interactive");
        assert_eq!(v["parish.streaming"], false);
        assert_eq!(v["parish.base_url_host"], "api.example.com");
        assert_eq!(v["gen_ai.system"], "openai");
        assert_eq!(v["gen_ai.request.model"], "test-model");
        assert_eq!(v["gen_ai.request.max_tokens"], 100);
        assert_eq!(v["gen_ai.completion"], "world");
        assert_eq!(v["parish.schema_version"], 1);
    }

    #[test]
    fn serialise_scrubs_secrets_in_prompt_and_response() {
        let mut entry = sample_entry(1);
        entry.prompt_text = "tell me about sk-proj-AAAAAAAAAAAAAAAAAAAAAAAA please".into();
        entry.response_text = "use Bearer abcdefghijklmnopqrstuvwxyz1234".into();
        let line = serialise(&entry, &openai(), InferencePriority::Background, "s", None).unwrap();
        assert!(line.contains("[REDACTED:openai_key]"));
        assert!(line.contains("Bearer [REDACTED:bearer_token]"));
        assert!(!line.contains("AAAAAAAAAAAAAAAAAAAAAAAA"));
    }

    #[test]
    fn classify_error_buckets_messages() {
        assert_eq!(classify_error("inference timed out after 30s"), "timeout");
        assert_eq!(classify_error("HTTP 404 not found"), "http_error");
        assert_eq!(classify_error("parse failure"), "other");
    }

    /// Polls until the file exists and contains at least one line, up to a
    /// generous timeout. The writer task runs cooperatively in the
    /// single-threaded test runtime; we need to yield to it after dropping
    /// our sender clone.
    async fn read_log_lines(path: &Path, timeout_ms: u64) -> Vec<String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        loop {
            if let Ok(text) = tokio::fs::read_to_string(path).await
                && !text.is_empty()
            {
                return text.lines().map(str::to_string).collect();
            }
            if std::time::Instant::now() >= deadline {
                return tokio::fs::read_to_string(path)
                    .await
                    .map(|t| t.lines().map(str::to_string).collect())
                    .unwrap_or_default();
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test]
    async fn writer_appends_one_line_per_record() {
        let tmp = tempfile::tempdir().unwrap();
        let log = InferenceFileLog::spawn(tmp.path(), true, Some("https://api.openai.com"));
        assert!(log.is_enabled());
        let path = log.path().to_path_buf();
        for i in 0..3 {
            log.record(&sample_entry(i), &openai(), InferencePriority::Interactive);
        }
        // Drop our sender clone so the writer task drains, flushes, and exits.
        drop(log);
        let lines = read_log_lines(&path, 1000).await;
        assert_eq!(lines.len(), 3, "one JSONL line per record");
        for line in &lines {
            let v: Value = serde_json::from_str(line).expect("each line is valid JSON");
            assert_eq!(v["gen_ai.system"], "openai");
        }
    }

    #[tokio::test]
    async fn opt_out_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let log = InferenceFileLog::spawn(tmp.path(), false, None);
        assert!(!log.is_enabled());
        log.record(&sample_entry(1), &openai(), InferencePriority::Interactive);
        drop(log);
        // The directory should not exist (or be empty).
        let logs_dir = tmp.path().join("inference_logs");
        if logs_dir.exists() {
            let mut dir = tokio::fs::read_dir(&logs_dir).await.unwrap();
            assert!(
                dir.next_entry().await.unwrap().is_none(),
                "no file should be created when disabled"
            );
        }
    }

    #[tokio::test]
    async fn runtime_toggle_off_then_on() {
        let tmp = tempfile::tempdir().unwrap();
        let log = InferenceFileLog::spawn(tmp.path(), true, None);
        let path = log.path().to_path_buf();
        log.record(&sample_entry(1), &openai(), InferencePriority::Interactive);
        log.set_enabled(false);
        log.record(&sample_entry(2), &openai(), InferencePriority::Interactive);
        log.set_enabled(true);
        log.record(&sample_entry(3), &openai(), InferencePriority::Interactive);
        drop(log);
        let lines = read_log_lines(&path, 1000).await;
        assert_eq!(
            lines.len(),
            2,
            "second record should be skipped while disabled"
        );
    }

    #[tokio::test]
    async fn disabled_handle_is_noop() {
        let log = InferenceFileLog::disabled();
        assert!(!log.is_enabled());
        log.record(&sample_entry(1), &openai(), InferencePriority::Interactive);
        assert!(log.path().as_os_str().is_empty());
    }

    /// Regression: a session that starts with logging OFF must still be able
    /// to begin writing after a runtime `/inference-log on`. The writer task
    /// is spawned eagerly; the file is opened lazily on the first record, so
    /// nothing hits disk until the toggle flips and a real call arrives.
    #[tokio::test]
    async fn start_disabled_then_enable_begins_writing() {
        let tmp = tempfile::tempdir().unwrap();
        let log = InferenceFileLog::spawn(tmp.path(), false, None);
        let path = log.path().to_path_buf();
        assert!(!log.is_enabled());
        // While off: nothing recorded, no file created.
        log.record(&sample_entry(1), &openai(), InferencePriority::Interactive);
        assert!(!path.exists(), "no file should exist while logging is off");
        // Flip on at runtime — must now write.
        log.set_enabled(true);
        log.record(&sample_entry(2), &openai(), InferencePriority::Interactive);
        drop(log);
        let lines = read_log_lines(&path, 1000).await;
        assert_eq!(lines.len(), 1, "record after enable must be written");
        let v: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["parish.request_id"], 2);
    }

    #[test]
    fn extract_host_handles_ipv6_and_userinfo() {
        assert_eq!(extract_host("http://[::1]:8080/v1"), Some("::1".into()));
        assert_eq!(
            extract_host("https://[2001:db8::1]:443"),
            Some("2001:db8::1".into())
        );
        assert_eq!(
            extract_host("https://user:pass@api.example.com/v1"),
            Some("api.example.com".into())
        );
    }

    #[test]
    fn resolve_enabled_precedence() {
        // CLI disable flag wins outright.
        assert!(!resolve_enabled(true, true));
        // No CLI flag, no env → config value decides.
        unsafe { std::env::remove_var("PARISH_INFERENCE_LOG") };
        assert!(resolve_enabled(false, true));
        assert!(!resolve_enabled(false, false));
        // Env overrides config in both directions.
        unsafe { std::env::set_var("PARISH_INFERENCE_LOG", "off") };
        assert!(!resolve_enabled(false, true));
        unsafe { std::env::set_var("PARISH_INFERENCE_LOG", "on") };
        assert!(resolve_enabled(false, false));
        // Unrecognised env value leaves the config setting untouched.
        unsafe { std::env::set_var("PARISH_INFERENCE_LOG", "maybe") };
        assert!(resolve_enabled(false, true));
        unsafe { std::env::remove_var("PARISH_INFERENCE_LOG") };
    }

    #[test]
    fn apply_toggle_reports_each_action() {
        let log = InferenceFileLog::disabled();
        assert!(apply_toggle(&log, LogToggleAction::On).contains("enabled"));
        assert!(log.is_enabled());
        assert!(apply_toggle(&log, LogToggleAction::Off).contains("disabled"));
        assert!(!log.is_enabled());
        // The disabled handle has an empty path, so status/path report the
        // "no file" wording rather than a path.
        assert!(apply_toggle(&log, LogToggleAction::Status).contains("OFF"));
        assert!(apply_toggle(&log, LogToggleAction::Path).contains("no file"));
    }
}
