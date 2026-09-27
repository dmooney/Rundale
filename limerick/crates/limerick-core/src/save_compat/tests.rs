use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde_json::Value;

use super::*;
use crate::persistence::{
    GameSnapshot, NewTranscriptEvent, NpcSnapshot, SAVE_FORMAT_VERSION, WorldEvent, inspect_save,
};
use crate::turn::journal_contract::{accepted, committed, one_task, state};
use crate::turn::{
    AddresseeSelection, ClarificationChoice, ClarificationPrompt, EventBuilder, ExecutionAttemptId,
    SqliteTurnJournal, StateRevision, TerminalOutcome, TranscriptEventKind, TurnEngine,
    TurnJournal, TurnRules,
};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../limerick-persistence/tests/fixtures")
}

/// A copy of persistence fixture `name` in a temporary directory.
fn copy_of(name: &str) -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("limerick_001.db");
    std::fs::copy(fixtures().join(name), &path).unwrap();
    (dir, path)
}

fn identity(id: &str, version: &str) -> ContentIdentity {
    ContentIdentity {
        id: id.to_string(),
        version: version.to_string(),
    }
}

/// A save on disk whose main branch has one snapshot captured against
/// `content`.
fn save_on_disk(content: Option<ContentIdentity>) -> (tempfile::TempDir, PathBuf, i64) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("limerick_001.db");
    let db = Database::open(&path).unwrap();
    let main = db.find_branch("main").unwrap().unwrap().id;
    let mut snapshot = state();
    snapshot.content = content;
    db.save_snapshot(main, &snapshot).unwrap();
    (dir, path, main)
}

fn refusal(result: Result<SaveInspection, LimerickError>) -> String {
    match result {
        Err(LimerickError::SaveIncompatible(reason)) => reason,
        other => panic!("expected a refusal, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn content_compatibility_uses_the_content_id_not_its_version() {
    let (_dir, path, _) = save_on_disk(Some(identity("rundale", "1.0.0")));
    check_save(&path, Some(&identity("rundale", "1.4.2"))).unwrap();
    check_save(&path, None).unwrap();

    let before = std::fs::read(&path).unwrap();
    let reason = refusal(check_save(&path, Some(&identity("testbed", "1.0.0"))));
    assert!(reason.contains("content 'rundale'"), "{reason}");
    assert!(is_incompatible(&LimerickError::SaveIncompatible(reason)));
    assert_eq!(std::fs::read(&path).unwrap(), before, "left untouched");

    // A snapshot from before content identity opens against any content.
    let (_dir, legacy, _) = save_on_disk(None);
    check_save(&legacy, Some(&identity("testbed", "1.0.0"))).unwrap();
}

#[test]
fn request_records_must_read_as_requests() {
    let (_dir, path) = copy_of("turn_journal_v2_save.db");
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("UPDATE requests SET record = '{\"id\":\"r1\"}' WHERE rowid = 1;")
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    // Valid JSON passes the storage-level check but not the request check.
    inspect_save(&path).unwrap();
    let reason = refusal(check_save(&path, None));
    assert!(reason.contains("turn request"), "{reason}");
    assert!(open_checked(&path, None).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before, "left untouched");
}

#[test]
fn an_unreadable_save_is_refused_with_the_compatibility_message() {
    let (_dir, path) = copy_of("unreadable_state_save.db");
    let before = std::fs::read(&path).unwrap();
    let error = open_checked(&path, Some(&identity("rundale", "1.0.0"))).unwrap_err();
    assert!(is_incompatible(&error), "{error}");
    assert_eq!(refusal_message(&path, &error), INCOMPATIBLE_SAVE_MESSAGE);
    assert!(INCOMPATIBLE_SAVE_MESSAGE.contains("/new"));
    assert_eq!(std::fs::read(&path).unwrap(), before, "byte-identical");
}

#[tokio::test]
async fn an_unknown_event_kind_survives_the_sqlite_journal_verbatim() {
    let (_dir, path, main) = save_on_disk(None);
    let journal = SqliteTurnJournal::open(&path, main).unwrap();
    let (mut record, command) = accepted("r1");
    journal.accept(record.clone(), vec![command]).await.unwrap();
    drop(journal);

    // A newer build journals an event of a kind this build does not know,
    // with a field this build does not know either.
    let raw = r#"{"id":"r1:-:9","request_id":"r1","kind":"harvest_festival","content":"Bonfires on the hill.","festival":{"name":"Lúnasa","day":1}}"#;
    Database::open(&path)
        .unwrap()
        .turn_journal_transaction(|writer| {
            writer
                .append_event(
                    main,
                    NewTranscriptEvent {
                        event_id: "r1:-:9",
                        request_id: Some("r1"),
                        kind: "harvest_festival",
                        event: raw,
                    },
                )
                .map(|_| ())
        })
        .unwrap();

    // This build reopens the save, reads the event as `Unknown`, and keeps
    // journaling turns around it.
    let reopened = SqliteTurnJournal::open(&path, main).unwrap();
    let events = reopened.events().unwrap();
    let unknown = events
        .iter()
        .find(|event| event.event.id.as_str() == "r1:-:9")
        .expect("the unknown event reads back");
    assert_eq!(
        unknown.event.kind,
        TranscriptEventKind::Unknown("harvest_festival".to_string())
    );
    assert_eq!(
        unknown.event.content.as_deref(),
        Some("Bonfires on the hill.")
    );
    let commit = committed(&mut record, one_task());
    reopened.update(record.clone(), Vec::new()).await.unwrap();
    reopened.commit(commit).await.unwrap();
    let engine = TurnEngine::restore(Arc::new(reopened), TurnRules::default())
        .await
        .unwrap();
    assert_eq!(engine.revision(), StateRevision(1));

    // The stored row is exactly what the newer build wrote.
    let db = Database::open(&path).unwrap();
    let row = db
        .transcript_events(main, 0)
        .unwrap()
        .into_iter()
        .find(|row| row.event_id == "r1:-:9")
        .unwrap();
    assert_eq!(row.kind, "harvest_festival");
    assert_eq!(row.event, raw);

    // And it is shown as one neutral fallback line.
    assert_eq!(
        transcript_fallback_lines(&db, main).unwrap(),
        vec![FALLBACK_LINE.to_string()]
    );
}

#[test]
fn only_events_this_build_cannot_present_get_fallback_lines() {
    let (_dir, path) = copy_of("turn_journal_v2_save.db");
    let main = Database::open(&path)
        .unwrap()
        .find_branch("main")
        .unwrap()
        .unwrap()
        .id;
    assert!(
        transcript_fallback_lines_at(&path, main)
            .unwrap()
            .is_empty()
    );

    let (_dir, future) = copy_of("future_format_unknown_event_save.db");
    assert_eq!(
        transcript_fallback_lines_at(&future, main).unwrap(),
        vec![FALLBACK_LINE.to_string()]
    );

    // A known kind whose payload does not read, and more unknown events
    // than are listed.
    let db = Database::open(&path).unwrap();
    db.turn_journal_transaction(|writer| {
        writer.append_event(
            main,
            NewTranscriptEvent {
                event_id: "broken",
                request_id: None,
                kind: "narration",
                event: "{\"kind\":",
            },
        )?;
        for n in 0..MAX_FALLBACK_LINES + 3 {
            let id = format!("future-{n}");
            let event = format!(r#"{{"id":"{id}","kind":"omen"}}"#);
            writer.append_event(
                main,
                NewTranscriptEvent {
                    event_id: &id,
                    request_id: None,
                    kind: "omen",
                    event: &event,
                },
            )?;
        }
        Ok::<_, LimerickError>(())
    })
    .unwrap();
    assert_eq!(
        transcript_fallback_lines(&db, main).unwrap().len(),
        MAX_FALLBACK_LINES
    );
}

// ── Save format shape ─────────────────────────────────────────────────────

/// Environment variable that records the shape of a new format version.
const RECORD_SHAPE: &str = "LIMERICK_RECORD_SAVE_FORMAT_SHAPE";

/// Fails when what a save stores changes without a save format version
/// bump. The shape of format N is recorded in
/// `limerick-persistence/tests/fixtures/save_format_vN.shape`: the SQLite
/// schema of a new save, and the JSON field paths and value types of a
/// snapshot, every world event, a request record, and a transcript event.
///
/// To change the saved data: bump `SAVE_FORMAT_VERSION`, add a fixture saved
/// by the previous format, and run this test once with
/// `LIMERICK_RECORD_SAVE_FORMAT_SHAPE=1` to record the new version's shape.
/// A recorded shape is never overwritten.
#[test]
fn save_format_shape() {
    let current = save_format_shape_text();
    let path = fixtures().join(format!("save_format_v{SAVE_FORMAT_VERSION}.shape"));
    match std::fs::read_to_string(&path) {
        Ok(recorded) => {
            if recorded != current {
                let recorded_lines: BTreeSet<&str> = recorded.lines().collect();
                let current_lines: BTreeSet<&str> = current.lines().collect();
                let removed: Vec<_> = recorded_lines.difference(&current_lines).collect();
                let added: Vec<_> = current_lines.difference(&recorded_lines).collect();
                panic!(
                    "the saved data shape changed without a save format version bump.\n\
                     Bump SAVE_FORMAT_VERSION (now {SAVE_FORMAT_VERSION}) in \
                     limerick-persistence/src/database/format.rs, check in a fixture \
                     saved by format {SAVE_FORMAT_VERSION}, and record the new shape \
                     with {RECORD_SHAPE}=1.\nremoved: {removed:#?}\nadded: {added:#?}"
                );
            }
        }
        Err(_) if std::env::var_os(RECORD_SHAPE).is_some() => {
            std::fs::write(&path, &current).unwrap();
        }
        Err(error) => panic!(
            "no recorded shape for save format {SAVE_FORMAT_VERSION} at {} ({error}); \
             record it with {RECORD_SHAPE}=1",
            path.display()
        ),
    }
}

fn save_format_shape_text() -> String {
    let mut text = format!(
        "# Shape of save format {SAVE_FORMAT_VERSION}. Recorded by \
         limerick-core save_compat::tests::save_format_shape; never edit by hand.\n"
    );
    text.push_str("[sqlite]\n");
    for line in sqlite_schema() {
        text.push_str(&line);
        text.push('\n');
    }
    let samples: Vec<(&str, Value)> = vec![
        ("snapshot", serde_json::to_value(sample_snapshot()).unwrap()),
        (
            "world_events",
            serde_json::to_value(sample_world_events()).unwrap(),
        ),
        ("request", serde_json::to_value(sample_request()).unwrap()),
        (
            "transcript_event",
            serde_json::to_value(sample_transcript_event()).unwrap(),
        ),
    ];
    for (name, value) in samples {
        text.push_str(&format!("[{name}]\n"));
        let mut paths = BTreeSet::new();
        json_shape(&value, "$", &mut paths);
        for path in paths {
            text.push_str(&path);
            text.push('\n');
        }
    }
    text
}

/// The schema of a new save: every table and index, whitespace-normalized,
/// and its stamped version.
fn sqlite_schema() -> Vec<String> {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("limerick_001.db");
    drop(Database::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    let version: u32 = conn
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .unwrap();
    let mut stmt = conn
        .prepare(
            "SELECT type, name, COALESCE(sql, '') FROM sqlite_master
             WHERE name NOT LIKE 'sqlite_%' ORDER BY type, name",
        )
        .unwrap();
    let mut lines: Vec<String> = stmt
        .query_map([], |row| {
            let sql: String = row.get(2)?;
            Ok(format!(
                "{} {}: {}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                sql.split_whitespace().collect::<Vec<_>>().join(" ")
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    lines.push(format!("user_version: {version}"));
    lines
}

/// Every field path and value type in `value`. Object keys that are ids
/// (all digits) collapse to `#`, so the shape does not depend on data.
fn json_shape(value: &Value, path: &str, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            if map.is_empty() {
                out.insert(format!("{path}: {{}}"));
            }
            for (key, child) in map {
                let key = if !key.is_empty() && key.chars().all(|c| c.is_ascii_digit()) {
                    "#"
                } else {
                    key.as_str()
                };
                json_shape(child, &format!("{path}.{key}"), out);
            }
        }
        Value::Array(items) => {
            if items.is_empty() {
                out.insert(format!("{path}: []"));
            }
            for item in items {
                json_shape(item, &format!("{path}[]"), out);
            }
        }
        Value::Null => {
            out.insert(format!("{path}: null"));
        }
        Value::Bool(_) => {
            out.insert(format!("{path}: bool"));
        }
        Value::Number(_) => {
            out.insert(format!("{path}: number"));
        }
        Value::String(_) => {
            out.insert(format!("{path}: string"));
        }
    }
}

/// A snapshot with every optional part present.
fn sample_snapshot() -> GameSnapshot {
    use chrono::TimeZone;
    let at = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 9, 30, 0).unwrap();
    let mut snapshot = state();
    let npc = crate::npc::Npc::new_test_npc();
    snapshot.npcs.push(NpcSnapshot::from_npc(&npc));
    snapshot.last_tier2_game_time = Some(at);
    snapshot.last_tier3_game_time = Some(at);
    snapshot.last_tier4_game_time = Some(at);
    snapshot.introduced_npcs.insert(npc.id);
    snapshot.npcs_who_know_player_name.insert(npc.id);
    snapshot.visited_order = vec![crate::world::LocationId(1)];
    snapshot.edge_traversals.insert(
        (crate::world::LocationId(1), crate::world::LocationId(2)),
        3,
    );
    snapshot.player_name = Some("Ciarán".to_string());
    snapshot
        .player_progress
        .assign_task(
            "Dig over the potato patch.",
            npc.id,
            crate::world::LocationId(1),
            at,
        )
        .unwrap();
    snapshot.content = Some(identity("rundale", "1.0.0"));
    snapshot
}

/// One of every world event. The match makes a new variant fail to compile
/// until it is added here.
fn sample_world_events() -> Vec<WorldEvent> {
    use crate::npc::NpcId;
    use crate::world::LocationId;
    use chrono::TimeZone;
    let at = chrono::Utc.with_ymd_and_hms(1820, 3, 20, 9, 30, 0).unwrap();
    let events = vec![
        WorldEvent::ReactionRecorded {
            npc_id: NpcId(1),
            direction: limerick_types::ReactionDirection::NpcToPlayer,
            emoji: "🙂".to_string(),
            context: "a kind word".to_string(),
            timestamp: at,
        },
        WorldEvent::PlayerMoved {
            from: LocationId(1),
            to: LocationId(2),
            minutes: Some(13),
        },
        WorldEvent::NpcMoved {
            npc_id: NpcId(1),
            from: LocationId(1),
            to: LocationId(2),
        },
        WorldEvent::NpcMoodChanged {
            npc_id: NpcId(1),
            mood: "content".to_string(),
        },
        WorldEvent::RelationshipChanged {
            npc_a: NpcId(1),
            npc_b: NpcId(2),
            delta: 0.5,
        },
        WorldEvent::DialogueOccurred {
            npc_id: NpcId(1),
            player_said: "Good day.".to_string(),
            npc_said: "God bless ye.".to_string(),
        },
        WorldEvent::WeatherChanged {
            new_weather: "Rain".to_string(),
        },
        WorldEvent::MemoryAdded {
            npc_id: NpcId(1),
            content: "Met a stranger.".to_string(),
        },
        WorldEvent::ClockAdvanced { minutes: 5 },
        WorldEvent::PlayerTaskStateChanged {
            task: one_task().remove(0),
        },
    ];
    for event in &events {
        match event {
            WorldEvent::ReactionRecorded { .. }
            | WorldEvent::PlayerMoved { .. }
            | WorldEvent::NpcMoved { .. }
            | WorldEvent::NpcMoodChanged { .. }
            | WorldEvent::RelationshipChanged { .. }
            | WorldEvent::DialogueOccurred { .. }
            | WorldEvent::WeatherChanged { .. }
            | WorldEvent::MemoryAdded { .. }
            | WorldEvent::ClockAdvanced { .. }
            | WorldEvent::PlayerTaskStateChanged { .. } => {}
        }
    }
    events
}

fn sample_prompt() -> ClarificationPrompt {
    ClarificationPrompt {
        question: "Which Mícheál do you mean?".to_string(),
        choices: vec![ClarificationChoice {
            id: "choose-1".to_string(),
            label: "Mícheál Connolly".to_string(),
            entity_id: Some("7".to_string()),
        }],
        reference: Some("Mícheál".to_string()),
    }
}

/// A committed request with every optional part present.
fn sample_request() -> RequestRecord {
    let (mut record, _) = accepted("r1");
    record.draft_id = Some("draft-1".to_string());
    record.addressed_to = vec!["Mícheál".to_string()];
    let _ = committed(&mut record, Vec::new());
    record.complete(StateRevision(1)).unwrap();
    record.pending_clarification = Some(sample_prompt());
    record.selected_addressees = vec![AddresseeSelection {
        reference: "Mícheál".to_string(),
        choice: sample_prompt().choices.remove(0),
    }];
    record.resolved_intent = Some(crate::input::PlayerIntent {
        intent: crate::input::IntentKind::Talk,
        target: Some("Mícheál".to_string()),
        dialogue: Some("Good day.".to_string()),
        atmosphere: None,
        raw: "say good day to Mícheál".to_string(),
    });
    record
}

/// A transcript event with every optional part present.
fn sample_transcript_event() -> PendingEvent {
    let mut builder = EventBuilder::new(
        crate::turn::LogicalRequestId::new("r1"),
        Some(ExecutionAttemptId::new("a1")),
        0,
    );
    let mut event = builder.response_completed(TerminalOutcome::Succeeded, Some(StateRevision(1)));
    event.item_id = Some(crate::turn::LogicalRequestId::new("r1").command_item());
    event.speaker = Some("Padraig".to_string());
    event.content = Some("God bless ye.".to_string());
    event.clarification = Some(sample_prompt());
    event.metadata.insert("retry".to_string(), "1".to_string());
    event
}

#[test]
fn shape_paths_collapse_ids_and_record_types() {
    let mut out = BTreeSet::new();
    json_shape(
        &serde_json::json!({"npcs": {"7": {"mood": "calm"}}, "log": [], "n": null}),
        "$",
        &mut out,
    );
    assert_eq!(
        out.into_iter().collect::<Vec<_>>(),
        vec!["$.log: []", "$.n: null", "$.npcs.#.mood: string"]
    );
    let _: &Path = fixtures().as_path();
}
