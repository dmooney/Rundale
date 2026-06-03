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

use serde_json::Value;

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
        events[i..j].sort_by(|a, b| a.1.to_string().cmp(&b.1.to_string()));
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
}
