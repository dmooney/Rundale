//! Differential ("shadow") comparison support for [`GameTestHarness`] (#1159).
//!
//! This module canonicalizes the event stream a harness execution produces so
//! the legacy router's output and the real `game_loop`'s output can be compared
//! for *semantic* equality, ignoring incidental differences (log ids,
//! timestamps, and ordering among events that are semantically a set).
//!
//! [`normalize`] is the single definition of "same output" used by the shadow
//! wrapper. Over-normalization is the one real risk — it can mask a genuine
//! divergence — so the stripped keys and the set-semantic event names are kept
//! minimal and explicit here.
//!
//! [`GameTestHarness`]: crate::testing::GameTestHarness

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Serialises all ledger appends within a process. `cargo test` runs tests on
/// multiple threads by default, so without this two `execute()` calls could
/// interleave their JSON lines (records can exceed the OS atomic-append size),
/// corrupting the JSONL. The lock makes each record a single, whole line.
static LEDGER_LOCK: Mutex<()> = Mutex::new(());

/// Environment variable that enables shadow mode. When set to a truthy value
/// (`1`/`true`/`yes`), [`GameTestHarness::execute`] also runs the real
/// `game_loop` and records divergences. Unset ⇒ the legacy path is byte-for-byte
/// untouched.
///
/// [`GameTestHarness::execute`]: crate::testing::GameTestHarness::execute
pub const SHADOW_ENV: &str = "PARISH_HARNESS_SHADOW";

/// Environment variable overriding the divergence-ledger path. Defaults to
/// [`DEFAULT_LEDGER_PATH`].
pub const LEDGER_ENV: &str = "PARISH_HARNESS_SHADOW_LEDGER";

/// Environment variable supplying the `case` label written into each ledger
/// record (typically the corpus file under test). Defaults to `"adhoc"`.
pub const CASE_ENV: &str = "PARISH_HARNESS_SHADOW_CASE";

/// Default JSONL ledger location, relative to the cargo working directory.
pub const DEFAULT_LEDGER_PATH: &str = "target/harness-shadow-ledger.jsonl";

/// Whether shadow mode is enabled via [`SHADOW_ENV`].
pub fn is_enabled() -> bool {
    matches!(
        std::env::var(SHADOW_ENV).ok().as_deref(),
        Some("1") | Some("true") | Some("yes")
    )
}

/// The configured ledger path ([`LEDGER_ENV`] override or [`DEFAULT_LEDGER_PATH`]).
pub fn ledger_path() -> PathBuf {
    std::env::var(LEDGER_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(DEFAULT_LEDGER_PATH))
}

/// The configured case label ([`CASE_ENV`] override or `"adhoc"`).
pub fn case_label() -> String {
    std::env::var(CASE_ENV).unwrap_or_else(|_| "adhoc".to_string())
}

/// One recorded divergence between the legacy harness path (`old`) and the real
/// `game_loop` path (`new`) for a single input line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DivergenceRecord {
    /// Corpus / test label this input belongs to.
    pub case: String,
    /// The player input line that diverged.
    pub input: String,
    /// Canonical event stream the legacy path produced.
    pub old: Value,
    /// Canonical event stream the real `game_loop` produced.
    pub new: Value,
}

/// Serialises a [`Canonical`] form to a JSON array of `[name, payload]` pairs.
fn canonical_to_json(canonical: &Canonical) -> Value {
    Value::Array(
        canonical
            .0
            .iter()
            .map(|(name, payload)| Value::Array(vec![Value::String(name.clone()), payload.clone()]))
            .collect(),
    )
}

/// Reduces an event stream to the ordered list of non-empty, trimmed
/// player-visible text lines (one `("text-log", {"content": line})` per line).
///
/// This is the comparison representation for shadow mode: it focuses on *what
/// text the player sees, in what order*, which is precisely the drift the
/// consolidation targets (#985 wrong dialogue, #1028 wrong name). It folds away
/// representational differences that are not player-visible — event metadata
/// like `source` attribution, and whether multi-line content arrives as one
/// event or several — so the ledger surfaces genuine output divergences rather
/// than emitter-shape noise. Non-`text-log` events are dropped.
pub fn text_output_lines(events: &[(String, Value)]) -> Vec<(String, Value)> {
    let mut out = Vec::new();
    for (name, payload) in events {
        if name != "text-log" {
            continue;
        }
        let Some(content) = payload.get("content").and_then(|c| c.as_str()) else {
            continue;
        };
        for line in content.split('\n') {
            let trimmed = line.trim();
            if !trimmed.is_empty() {
                out.push(("text-log".to_string(), Value::String(trimmed.to_string())));
            }
        }
    }
    out
}

/// Compares two event streams for the same input. Returns `Some(record)` iff
/// their canonical forms differ; `None` when they match.
pub fn compare(
    case: &str,
    input: &str,
    legacy: &[(String, Value)],
    real: &[(String, Value)],
) -> Option<DivergenceRecord> {
    let old = normalize(legacy);
    let new = normalize(real);
    if old == new {
        return None;
    }
    Some(DivergenceRecord {
        case: case.to_string(),
        input: input.to_string(),
        old: canonical_to_json(&old),
        new: canonical_to_json(&new),
    })
}

/// Appends one divergence record as a single JSON line to the ledger at `path`,
/// creating parent directories as needed. Best-effort: I/O errors are returned
/// for the caller to log, never panicked.
pub fn append_ledger(path: &Path, record: &DivergenceRecord) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut line = serde_json::to_string(record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    line.push('\n');
    // Serialise the open+write so parallel test threads can't interleave lines.
    let _guard = LEDGER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(line.as_bytes())
}

/// Object keys removed before comparison because they vary run-to-run without
/// carrying game-observable meaning: the monotonic text-log id (`"msg-1"`,
/// `"msg-2"`, …) and any wall-clock/relative timestamp.
const INCIDENTAL_KEYS: &[&str] = &["id", "timestamp", "ts", "request_id"];

/// Event names whose consecutive occurrences are order-independent — they
/// describe a *set* of simultaneous happenings (e.g. each co-located NPC
/// reacting to the player), so a different emission order is not a divergence.
/// Kept deliberately small; add a name here only with evidence the order is
/// genuinely incidental.
const SET_SEMANTIC_EVENTS: &[&str] = &["npc-reaction", "reaction"];

/// The normalized, comparison-ready form of an event stream.
///
/// Two executions are considered to have produced the "same output" iff their
/// `Canonical` forms are equal.
#[derive(Debug, Clone, PartialEq)]
pub struct Canonical(pub Vec<(String, Value)>);

/// Canonicalizes an event stream for shadow comparison.
///
/// 1. Strips [`INCIDENTAL_KEYS`] recursively from every payload.
/// 2. Sorts each contiguous run of same-named [`SET_SEMANTIC_EVENTS`] so their
///    emission order doesn't register as a difference.
///
/// All other ordering is preserved, because for the rest of the engine the
/// order events are emitted in is semantically meaningful (a movement narration
/// before an arrival description is not the same as the reverse).
pub fn normalize(events: &[(String, Value)]) -> Canonical {
    let mut out: Vec<(String, Value)> = events
        .iter()
        .map(|(name, payload)| (name.clone(), strip_incidental(payload)))
        .collect();
    sort_set_semantic_runs(&mut out);
    Canonical(out)
}

/// Returns `payload` with every [`INCIDENTAL_KEYS`] entry removed, recursing
/// into nested objects and arrays.
fn strip_incidental(payload: &Value) -> Value {
    match payload {
        Value::Object(map) => Value::Object(
            map.iter()
                .filter(|(k, _)| !INCIDENTAL_KEYS.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), strip_incidental(v)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(strip_incidental).collect()),
        other => other.clone(),
    }
}

/// Sorts each maximal contiguous run of identically-named set-semantic events
/// by the stable string form of their payloads.
fn sort_set_semantic_runs(events: &mut [(String, Value)]) {
    let mut i = 0;
    while i < events.len() {
        let name = events[i].0.clone();
        if !SET_SEMANTIC_EVENTS.contains(&name.as_str()) {
            i += 1;
            continue;
        }
        // Extend the run over consecutive same-named events.
        let mut j = i + 1;
        while j < events.len() && events[j].0 == name {
            j += 1;
        }
        events[i..j].sort_by_key(|a| a.1.to_string());
        i = j;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn ignores_incidental_id_and_timestamp_differences() {
        let a = vec![(
            "text-log".to_string(),
            json!({ "content": "You look around.", "id": "msg-1", "source": "system", "timestamp": 1000 }),
        )];
        let b = vec![(
            "text-log".to_string(),
            json!({ "content": "You look around.", "id": "msg-2", "source": "system", "timestamp": 2000 }),
        )];
        assert_eq!(
            normalize(&a),
            normalize(&b),
            "streams differing only in id/timestamp must canonicalize equal"
        );
    }

    #[test]
    fn distinguishes_semantic_content_differences() {
        let a = vec![(
            "text-log".to_string(),
            json!({ "content": "You go north.", "id": "msg-1" }),
        )];
        let b = vec![(
            "text-log".to_string(),
            json!({ "content": "You go south.", "id": "msg-1" }),
        )];
        assert_ne!(
            normalize(&a),
            normalize(&b),
            "different log text is a real divergence"
        );
    }

    #[test]
    fn set_semantic_events_are_order_independent() {
        let a = vec![
            ("reaction".to_string(), json!({ "npc": "Peig" })),
            ("reaction".to_string(), json!({ "npc": "Tadhg" })),
        ];
        let b = vec![
            ("reaction".to_string(), json!({ "npc": "Tadhg" })),
            ("reaction".to_string(), json!({ "npc": "Peig" })),
        ];
        assert_eq!(
            normalize(&a),
            normalize(&b),
            "reaction order within a turn is incidental"
        );
    }

    #[test]
    fn ordered_events_remain_order_sensitive() {
        let a = vec![
            ("text-log".to_string(), json!({ "content": "first" })),
            ("text-log".to_string(), json!({ "content": "second" })),
        ];
        let b = vec![
            ("text-log".to_string(), json!({ "content": "second" })),
            ("text-log".to_string(), json!({ "content": "first" })),
        ];
        assert_ne!(
            normalize(&a),
            normalize(&b),
            "text-log order carries meaning and must not be sorted away"
        );
    }

    #[test]
    fn text_output_lines_folds_source_and_multiline_noise() {
        // Legacy: two separate events, no source. Real: one multi-line event
        // with a source field and a blank line. Same visible text ⇒ equal.
        let legacy = vec![
            (
                "text-log".to_string(),
                json!({ "content": "You walk north." }),
            ),
            ("text-log".to_string(), json!({ "content": "You arrive." })),
        ];
        let real = vec![
            (
                "text-log".to_string(),
                json!({ "content": "You walk north.\n\nYou arrive.", "source": "system", "id": "msg-3" }),
            ),
            ("world-update".to_string(), json!({ "location": "x" })),
        ];
        assert_eq!(text_output_lines(&legacy), text_output_lines(&real));
    }

    #[test]
    fn text_output_lines_preserves_genuine_text_differences() {
        let a = vec![(
            "text-log".to_string(),
            json!({ "content": "Padraig: hello" }),
        )];
        let b = vec![(
            "text-log".to_string(),
            json!({ "content": "LLM is not configured", "source": "system" }),
        )];
        assert_ne!(text_output_lines(&a), text_output_lines(&b));
    }

    // ── Shadow comparison + ledger (C5) ──────────────────────────────────────

    #[test]
    fn compare_returns_none_on_match() {
        // Identical apart from incidental id ⇒ no divergence.
        let legacy = vec![(
            "text-log".to_string(),
            json!({ "content": "You look around.", "id": "msg-1" }),
        )];
        let real = vec![(
            "text-log".to_string(),
            json!({ "content": "You look around.", "id": "msg-7" }),
        )];
        assert!(compare("case", "look", &legacy, &real).is_none());
    }

    #[test]
    fn compare_returns_record_on_divergence() {
        let legacy = vec![(
            "text-log".to_string(),
            json!({ "content": "Peig says hello." }),
        )];
        let real = vec![("text-log".to_string(), json!({ "content": "..." }))];
        let record =
            compare("dialogue", "say hi", &legacy, &real).expect("expected a divergence record");
        assert_eq!(record.case, "dialogue");
        assert_eq!(record.input, "say hi");
        assert_ne!(record.old, record.new);
    }

    #[test]
    fn append_ledger_writes_one_json_line_per_record() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/ledger.jsonl");
        let record = DivergenceRecord {
            case: "c".to_string(),
            input: "look".to_string(),
            old: json!([["text-log", { "content": "a" }]]),
            new: json!([["text-log", { "content": "b" }]]),
        };
        append_ledger(&path, &record).unwrap();
        append_ledger(&path, &record).unwrap();

        let contents = std::fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = contents.lines().collect();
        assert_eq!(lines.len(), 2, "one JSON line per record");
        let parsed: DivergenceRecord = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed, record, "record round-trips through the ledger");
    }
}
