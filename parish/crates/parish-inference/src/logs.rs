//! Bounded ring-buffer log of inference call entries, for the debug panel.

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::file_log::InferenceFileLog;
use crate::queue::InferencePriority;

/// A single logged inference call for the debug panel.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InferenceLogEntry {
    /// Unique request ID.
    pub request_id: u64,
    /// Wall-clock timestamp (e.g. "14:32:05").
    pub timestamp: String,
    /// Model name used for this request.
    pub model: String,
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
    /// Number of streamed tokens observed (streaming only). Each entry
    /// counts a non-empty `delta.content` chunk; reasoning chunks are
    /// not surfaced through this channel and are excluded.
    pub output_tokens: Option<u64>,
    /// Temperature sent to provider (if any). Plumbed through so the on-disk
    /// inference log can record it in OpenTelemetry GenAI form.
    pub temperature: Option<f32>,
    /// Priority lane the request travelled through.
    pub priority: InferencePriority,
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
}
