//! Bounded ring-buffer log of inference call entries, for the debug panel.

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::file_log::InferenceFileLog;
use crate::google_client::{GenerationResult, ProviderCallError};
use crate::queue::InferencePriority;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// A single logged inference call for the debug panel.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InferenceLogEntry {
    /// Unique request ID.
    pub request_id: u64,
    /// Wall-clock timestamp (e.g. "14:32:05").
    pub timestamp: String,
    /// Model name used for this request.
    pub model: String,
    /// Provider id and concrete wire API used for this call.
    pub provider: String,
    pub api_mode: String,
    /// Gameplay inference role.
    pub role: parish_config::InferenceCategory,
    /// Concrete workload within the high-level role.
    pub subrole: parish_config::InferenceSubrole,
    /// Whether this was a streaming request.
    pub streaming: bool,
    /// Request duration in milliseconds.
    pub duration_ms: u64,
    /// Prompt length in characters.
    pub prompt_len: usize,
    /// Response length in characters.
    pub response_len: usize,
    /// Error message if the request failed (None = success).
    pub error: Option<String>,
    /// System prompt sent (if any).
    pub system_prompt: Option<String>,
    /// User prompt text.
    pub prompt_text: String,
    /// Full response text (empty on error).
    pub response_text: String,
    /// Max tokens limit sent to provider (if any).
    pub max_tokens: Option<u32>,
    /// Time-to-first-token in milliseconds (streaming only; None for
    /// non-streaming requests, which never observe an intermediate token).
    pub ttft_ms: Option<u64>,
    /// Exact provider-reported visible output tokens when available.
    pub output_tokens: Option<u64>,
    /// Number of visible streaming chunks, deliberately distinct from tokens.
    pub stream_chunks: Option<u64>,
    pub input_tokens: Option<u64>,
    pub cached_tokens: Option<u64>,
    pub thought_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub thinking_level: Option<parish_config::ThinkingLevel>,
    pub requested_service_tier: Option<parish_config::ServiceTier>,
    pub effective_service_tier: Option<String>,
    pub provider_request_id: Option<String>,
    pub terminal_status: Option<String>,
    pub retry_count: u32,
    /// HTTP response status when the provider supplied one.
    pub http_status: Option<u16>,
    /// Stable machine-readable error class for troubleshooting.
    pub failure_kind: Option<String>,
    /// Visible output received before a failed/cancelled stream ended.
    pub partial_output_len: usize,
    /// Whether the provider reported a lower service tier than requested.
    pub tier_downgraded: bool,
    /// Usage-based Gemini 3.6 Flash estimate using the checked pricing snapshot.
    pub estimated_cost_usd: Option<f64>,
    /// Stable system-prefix fingerprint used to correlate implicit-cache reuse.
    pub prompt_prefix_hash: Option<String>,
    pub prompt_prefix_len: Option<usize>,
    /// Temperature sent to provider (if any). Plumbed through so the on-disk
    /// inference log can record it in OpenTelemetry GenAI form.
    pub temperature: Option<f32>,
    /// Priority lane the request travelled through.
    pub priority: InferencePriority,
}

impl Default for InferenceLogEntry {
    fn default() -> Self {
        Self {
            request_id: 0,
            timestamp: String::new(),
            model: String::new(),
            provider: "unknown".to_string(),
            api_mode: "legacy".to_string(),
            role: parish_config::InferenceCategory::Dialogue,
            subrole: parish_config::InferenceSubrole::Dialogue,
            streaming: false,
            duration_ms: 0,
            prompt_len: 0,
            response_len: 0,
            error: None,
            system_prompt: None,
            prompt_text: String::new(),
            response_text: String::new(),
            max_tokens: None,
            ttft_ms: None,
            output_tokens: None,
            stream_chunks: None,
            input_tokens: None,
            cached_tokens: None,
            thought_tokens: None,
            total_tokens: None,
            thinking_level: None,
            requested_service_tier: None,
            effective_service_tier: None,
            provider_request_id: None,
            terminal_status: None,
            retry_count: 0,
            http_status: None,
            failure_kind: None,
            partial_output_len: 0,
            tier_downgraded: false,
            estimated_cost_usd: None,
            prompt_prefix_hash: None,
            prompt_prefix_len: None,
            temperature: None,
            priority: InferencePriority::Interactive,
        }
    }
}

/// Bounded ring buffer of inference call log entries.
///
/// Enforces a hard `max_entries` cap independent of `VecDeque::capacity()`.
/// `VecDeque::with_capacity` rounds up to the next power of two and reallocates
/// on overflow, so using `capacity()` as a bound leaks memory exponentially —
/// see issue #340. This struct stores the configured cap explicitly and evicts
/// the oldest entry whenever `push` would exceed it.
#[derive(Debug)]
pub struct BoundedInferenceLog {
    entries: VecDeque<InferenceLogEntry>,
    max_entries: usize,
}

impl BoundedInferenceLog {
    /// Creates an empty log bounded to `max_entries`. A value of 0 is treated
    /// as 1 so that a `push` always leaves exactly one entry in the log.
    pub fn new(max_entries: usize) -> Self {
        let cap = max_entries.max(1);
        Self {
            entries: VecDeque::with_capacity(cap),
            max_entries: cap,
        }
    }

    /// Appends `entry`, evicting the oldest entries until `len <= max_entries`.
    pub fn push(&mut self, entry: InferenceLogEntry) {
        while self.entries.len() >= self.max_entries {
            self.entries.pop_front();
        }
        self.entries.push_back(entry);
    }

    /// Iterates over stored entries, oldest first.
    pub fn iter(&self) -> std::collections::vec_deque::Iter<'_, InferenceLogEntry> {
        self.entries.iter()
    }

    /// Returns the current number of stored entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the log is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the configured maximum number of entries.
    pub fn max_entries(&self) -> usize {
        self.max_entries
    }
}

/// Shared bounded ring buffer of inference call log entries.
pub type InferenceLog = Arc<Mutex<BoundedInferenceLog>>;

/// Common audit destination for direct category clients that intentionally
/// bypass the single-flight dialogue worker.
#[derive(Clone)]
pub struct InferenceAuditSink {
    pub log: InferenceLog,
    pub file_log: InferenceFileLog,
    deferred: Option<DeferredInferenceAudit>,
}

impl InferenceAuditSink {
    pub fn new(log: InferenceLog, file_log: InferenceFileLog) -> Self {
        Self {
            log,
            file_log,
            deferred: None,
        }
    }

    /// Returns a sink that buffers direct-call records in the same atomic
    /// audit scope used by queued inference during a staged player turn.
    pub fn with_deferred(&self, deferred: DeferredInferenceAudit) -> Self {
        Self {
            log: self.log.clone(),
            file_log: self.file_log.clone(),
            deferred: Some(deferred),
        }
    }

    async fn record(&self, entry: InferenceLogEntry) {
        let provider = parish_config::Provider::from_str_loose(&entry.provider)
            .unwrap_or_else(|_| parish_config::Provider::simulator());
        if let Some(deferred) = &self.deferred {
            let priority = entry.priority;
            deferred
                .record(
                    entry,
                    self.log.clone(),
                    self.file_log.clone(),
                    provider,
                    priority,
                )
                .await;
        } else {
            self.file_log.record(&entry, &provider, entry.priority);
            self.log.lock().await.push(entry);
        }
    }
}

/// Request-scoped builder that turns a detailed direct-provider result into
/// the same debug/JSONL record shape emitted by the queued worker.
pub struct DirectInferenceAudit {
    sink: Option<InferenceAuditSink>,
    request_id: u64,
    model: String,
    prompt: String,
    system: Option<String>,
    subrole: parish_config::InferenceSubrole,
    streaming: bool,
    max_tokens: Option<u32>,
    thinking_level: Option<parish_config::ThinkingLevel>,
    requested_service_tier: Option<parish_config::ServiceTier>,
    temperature: Option<f32>,
    priority: InferencePriority,
    started: Instant,
}

static DIRECT_REQUEST_ID: AtomicU64 = AtomicU64::new(10_000_000);

impl DirectInferenceAudit {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sink: Option<InferenceAuditSink>,
        model: &str,
        prompt: &str,
        system: Option<&str>,
        subrole: parish_config::InferenceSubrole,
        streaming: bool,
        max_tokens: Option<u32>,
        thinking_level: Option<parish_config::ThinkingLevel>,
        requested_service_tier: Option<parish_config::ServiceTier>,
        temperature: Option<f32>,
        priority: InferencePriority,
    ) -> Self {
        Self {
            sink,
            request_id: DIRECT_REQUEST_ID.fetch_add(1, Ordering::Relaxed),
            model: model.to_string(),
            prompt: prompt.to_string(),
            system: system.map(str::to_string),
            subrole,
            streaming,
            max_tokens,
            thinking_level,
            requested_service_tier,
            temperature,
            priority,
            started: Instant::now(),
        }
    }

    pub async fn record(
        self,
        result: Result<GenerationResult, ProviderCallError>,
    ) -> Result<GenerationResult, ProviderCallError> {
        let Some(sink) = self.sink else {
            return result;
        };
        let elapsed = self.started.elapsed().as_millis() as u64;
        let (text, error, partial_output_len, metadata) = match &result {
            Ok(ok) => (ok.text.clone(), None, 0, ok.metadata.clone()),
            Err(err) => (
                err.partial_text.clone(),
                Some(err.message.clone()),
                err.partial_text.len(),
                (*err.metadata).clone(),
            ),
        };
        let provider_request_id = metadata
            .interaction_id
            .as_deref()
            .map(super::worker::safe_provider_request_id);
        let failure_kind = error
            .as_deref()
            .map(|message| super::worker::classify_provider_failure(message, metadata.http_status));
        let tier_downgraded = matches!(
            metadata.requested_service_tier,
            Some(parish_config::ServiceTier::Priority)
        ) && metadata.effective_service_tier.as_deref() == Some("standard");
        let estimated_cost_usd = super::worker::estimate_gemini_36_cost(&metadata);
        let prefix = self.system.as_deref();
        let entry = InferenceLogEntry {
            request_id: self.request_id,
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
            model: self.model,
            provider: metadata.provider,
            api_mode: metadata.api_mode,
            role: self.subrole.category(),
            subrole: self.subrole,
            streaming: self.streaming,
            duration_ms: metadata.duration_ms.max(elapsed),
            prompt_len: self.prompt.len(),
            response_len: text.len(),
            error,
            system_prompt: self.system.clone(),
            prompt_text: self.prompt,
            response_text: text,
            max_tokens: self.max_tokens,
            ttft_ms: metadata.ttft_ms,
            output_tokens: metadata.usage.output_tokens,
            stream_chunks: Some(metadata.stream_chunks).filter(|count| *count > 0),
            input_tokens: metadata.usage.input_tokens,
            cached_tokens: metadata.usage.cached_tokens,
            thought_tokens: metadata.usage.thought_tokens,
            total_tokens: metadata.usage.total_tokens,
            thinking_level: self.thinking_level,
            requested_service_tier: metadata
                .requested_service_tier
                .or(self.requested_service_tier),
            effective_service_tier: metadata.effective_service_tier,
            provider_request_id,
            terminal_status: metadata.terminal_status,
            retry_count: metadata.retry_count,
            http_status: metadata.http_status,
            failure_kind,
            partial_output_len,
            tier_downgraded,
            estimated_cost_usd,
            prompt_prefix_hash: prefix.map(super::worker::stable_prefix_hash),
            prompt_prefix_len: prefix.map(str::len),
            temperature: self.temperature,
            priority: self.priority,
        };
        sink.record(entry).await;
        result
    }
}

/// Request-scoped buffer for inference audit records.
///
/// Atomic player turns run provider calls against cloned game state before
/// their durable journal commit. Their debug-ring and JSONL records must not
/// become observable unless that candidate is installed. A scoped
/// [`InferenceQueue`](crate::InferenceQueue) attaches this buffer to each
/// request; the worker records here instead of the live sinks, and the turn
/// coordinator calls [`Self::commit`] only after journal commit and canonical
/// state installation.
#[derive(Clone, Default)]
pub struct DeferredInferenceAudit {
    inner: Arc<Mutex<DeferredInferenceAuditInner>>,
}

#[derive(Default)]
struct DeferredInferenceAuditInner {
    status: DeferredInferenceAuditStatus,
    records: Vec<DeferredInferenceAuditRecord>,
}

#[derive(Default)]
enum DeferredInferenceAuditStatus {
    #[default]
    Pending,
    Committed,
    Discarded,
}

struct DeferredInferenceAuditRecord {
    entry: InferenceLogEntry,
    log: InferenceLog,
    file_log: InferenceFileLog,
    provider: parish_config::Provider,
    priority: InferencePriority,
}

impl DeferredInferenceAudit {
    /// Buffers one completed provider call while the surrounding turn is
    /// pending. If the coordinator has already committed or discarded the
    /// scope, late records are respectively delivered or ignored.
    pub async fn record(
        &self,
        entry: InferenceLogEntry,
        log: InferenceLog,
        file_log: InferenceFileLog,
        provider: parish_config::Provider,
        priority: InferencePriority,
    ) {
        let mut inner = self.inner.lock().await;
        match inner.status {
            DeferredInferenceAuditStatus::Pending => {
                inner.records.push(DeferredInferenceAuditRecord {
                    entry,
                    log,
                    file_log,
                    provider,
                    priority,
                });
            }
            DeferredInferenceAuditStatus::Committed => {
                drop(inner);
                deliver_inference_audit(entry, log, file_log, provider, priority).await;
            }
            DeferredInferenceAuditStatus::Discarded => {}
        }
    }

    /// Makes every buffered record visible after the staged turn is durable
    /// and installed. Idempotent so adapter cleanup can safely retry it.
    pub async fn commit(&self) {
        let records = {
            let mut inner = self.inner.lock().await;
            match inner.status {
                DeferredInferenceAuditStatus::Committed => return,
                DeferredInferenceAuditStatus::Discarded => return,
                DeferredInferenceAuditStatus::Pending => {
                    inner.status = DeferredInferenceAuditStatus::Committed;
                    std::mem::take(&mut inner.records)
                }
            }
        };
        for record in records {
            deliver_inference_audit(
                record.entry,
                record.log,
                record.file_log,
                record.provider,
                record.priority,
            )
            .await;
        }
    }

    /// Explicitly rejects the pending records. Dropping an uncommitted scope
    /// is also safe, but this closes the door on a late worker completion.
    pub async fn discard(&self) {
        let mut inner = self.inner.lock().await;
        if matches!(inner.status, DeferredInferenceAuditStatus::Pending) {
            inner.status = DeferredInferenceAuditStatus::Discarded;
            inner.records.clear();
        }
    }
}

async fn deliver_inference_audit(
    entry: InferenceLogEntry,
    log: InferenceLog,
    file_log: InferenceFileLog,
    provider: parish_config::Provider,
    priority: InferencePriority,
) {
    file_log.record(&entry, &provider, priority);
    log.lock().await.push(entry);
}

/// Creates a new empty inference log with capacity from config.
pub fn new_inference_log_with_config(config: &parish_config::InferenceConfig) -> InferenceLog {
    Arc::new(Mutex::new(BoundedInferenceLog::new(config.log_capacity)))
}

/// Creates a new empty inference log with default capacity.
pub fn new_inference_log() -> InferenceLog {
    new_inference_log_with_config(&parish_config::InferenceConfig::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google_client::{ProviderMetadata, ProviderUsage};

    fn log_entry(request_id: u64) -> InferenceLogEntry {
        InferenceLogEntry {
            request_id,
            timestamp: "00:00:00".to_string(),
            model: "test".to_string(),
            streaming: false,
            duration_ms: 0,
            prompt_len: 0,
            response_len: 0,
            error: None,
            system_prompt: None,
            prompt_text: String::new(),
            response_text: String::new(),
            max_tokens: None,
            ttft_ms: None,
            output_tokens: None,
            temperature: None,
            priority: InferencePriority::Interactive,
            ..Default::default()
        }
    }

    /// Regression test for issue #340: the ring buffer must enforce its
    /// configured cap regardless of `VecDeque::capacity()`'s rounded-up value.
    #[test]
    fn bounded_inference_log_enforces_configured_cap() {
        let mut log = BoundedInferenceLog::new(50);
        for i in 0..1000u64 {
            log.push(log_entry(i));
        }
        assert_eq!(log.len(), 50, "log must never exceed its configured cap");
        assert_eq!(log.max_entries(), 50);
        // Oldest entry should have been evicted; we should see the last 50 IDs.
        let ids: Vec<u64> = log.iter().map(|e| e.request_id).collect();
        assert_eq!(ids.first().copied(), Some(950));
        assert_eq!(ids.last().copied(), Some(999));
    }

    /// A zero cap is clamped to 1 so pushes always leave one entry.
    #[test]
    fn bounded_inference_log_zero_cap_is_clamped() {
        let mut log = BoundedInferenceLog::new(0);
        assert_eq!(log.max_entries(), 1);
        log.push(log_entry(1));
        log.push(log_entry(2));
        assert_eq!(log.len(), 1);
        assert_eq!(log.iter().next().unwrap().request_id, 2);
    }

    #[test]
    fn bounded_inference_log_is_empty_and_len() {
        let mut log = BoundedInferenceLog::new(4);
        assert!(log.is_empty());
        assert_eq!(log.len(), 0);
        log.push(log_entry(1));
        assert!(!log.is_empty());
        assert_eq!(log.len(), 1);
    }

    #[tokio::test]
    async fn direct_audit_preserves_google_usage_and_failure_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let log = new_inference_log();
        let file_log = InferenceFileLog::spawn(temp.path(), false, None);
        let sink = InferenceAuditSink::new(log.clone(), file_log);
        let audit = DirectInferenceAudit::new(
            Some(sink),
            "gemini-3.6-flash",
            "hello",
            Some("stable prefix"),
            parish_config::InferenceSubrole::MessageReaction,
            false,
            Some(1024),
            Some(parish_config::ThinkingLevel::Low),
            Some(parish_config::ServiceTier::Standard),
            None,
            InferencePriority::Interactive,
        );
        let error = ProviderCallError {
            message: "Google Interactions HTTP 429: quota exhausted".to_string(),
            partial_text: "partial".to_string(),
            metadata: Box::new(ProviderMetadata {
                provider: "google".to_string(),
                api_mode: "google-interactions".to_string(),
                model: "gemini-3.6-flash".to_string(),
                interaction_id: Some("interaction-id-that-is-not-safe-to-show-in-full".to_string()),
                http_status: Some(429),
                terminal_status: Some("failed".to_string()),
                requested_service_tier: Some(parish_config::ServiceTier::Standard),
                effective_service_tier: Some("standard".to_string()),
                retry_count: 2,
                usage: ProviderUsage {
                    input_tokens: Some(100),
                    cached_tokens: Some(40),
                    thought_tokens: Some(5),
                    output_tokens: Some(7),
                    total_tokens: Some(112),
                },
                ttft_ms: Some(12),
                duration_ms: 25,
                stream_chunks: 1,
            }),
        };

        let returned = audit.record(Err(error)).await.unwrap_err();
        assert_eq!(returned.metadata.http_status, Some(429));
        let guard = log.lock().await;
        let entry = guard.iter().next().unwrap();
        assert_eq!(
            entry.subrole,
            parish_config::InferenceSubrole::MessageReaction
        );
        assert_eq!(entry.failure_kind.as_deref(), Some("rate-limited"));
        assert_eq!(entry.partial_output_len, 7);
        assert_eq!(entry.cached_tokens, Some(40));
        assert_eq!(entry.thought_tokens, Some(5));
        assert_eq!(entry.retry_count, 2);
        assert!(entry.estimated_cost_usd.is_some());
        assert_eq!(
            entry
                .provider_request_id
                .as_deref()
                .unwrap()
                .chars()
                .count(),
            17
        );
    }

    #[tokio::test]
    async fn direct_audit_obeys_deferred_commit_and_discard() {
        let temp = tempfile::tempdir().unwrap();
        let log = new_inference_log();
        let file_log = InferenceFileLog::spawn(temp.path(), false, None);
        let base = InferenceAuditSink::new(log.clone(), file_log);

        let committed = DeferredInferenceAudit::default();
        base.with_deferred(committed.clone())
            .record(log_entry(41))
            .await;
        assert!(log.lock().await.is_empty());
        committed.commit().await;
        assert_eq!(log.lock().await.len(), 1);

        let discarded = DeferredInferenceAudit::default();
        base.with_deferred(discarded.clone())
            .record(log_entry(42))
            .await;
        discarded.discard().await;
        assert_eq!(log.lock().await.len(), 1);
    }
}
