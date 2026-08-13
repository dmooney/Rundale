//! Persistent JSONL chat-transcript log.
//!
//! Sibling of `parish_inference::file_log::InferenceFileLog`. A
//! [`GameEvent`]-bus subscriber (alongside `CharacterLogManager` /
//! `LocationLogManager`) that captures the player-visible chat stream — NPC
//! dialogue, player travel, off-screen NPC interactions, weather shifts and
//! festivals — so a "this NPC line was weird" report can be followed back to
//! the inference call that produced it via the shared `parish.request_id`.
//!
//! ## Correlation
//!
//! `npc_dialogue` records carry `parish.request_id` (plumbed through
//! `GameEvent::DialogueOccurred`) matching the corresponding inference-log
//! line. Non-LLM events (travel, weather, …) leave that field absent.
//!
//! ## Lifecycle
//!
//! - File path: `{saves_dir}/inference_logs/{session_id}.transcript.jsonl`,
//!   where `session_id` matches the paired inference log so file pairing
//!   is by name.
//! - Enable flag is shared with `InferenceFileLog` via [`Self::spawn_with_flag`]
//!   so `/inference-log off` silences both.
//! - Disabled / detached handles are silent no-ops.
//!
//! ## Why this lives in `parish-core`
//!
//! Chat events are a core orchestration concern shared across all three
//! entry points (rule #12). It depends only on `parish-inference` (for the
//! shared `secret_scrub` redaction) plus `tokio` / `serde_json` / `chrono`
//! — none of the backend crates (`tauri` / `axum` / `tower*`) the
//! architecture-fitness test forbids, so the gate is satisfied.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use chrono::Utc;
use parish_inference::secret_scrub::scrub;
use serde_json::{Map, Value};
use tokio::fs::{File, OpenOptions, create_dir_all};
use tokio::io::{AsyncWriteExt, BufWriter};
use tokio::sync::mpsc;

use parish_npc::manager::NpcManager;
use parish_types::events::GameEvent;
use parish_world::WorldState;

const CHANNEL_CAPACITY: usize = 256;
const SCHEMA_VERSION: u32 = 1;
const DROP_WARNING_INTERVAL: u64 = 32;

/// User-visible chat event categories recorded in the transcript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChatEventType {
    /// LLM-generated NPC dialogue line (carries `parish.request_id`).
    NpcDialogue,
    /// The player travelled between two locations.
    PlayerMove,
    /// An off-screen NPC↔NPC interaction beat (Tier 2 narrative summary).
    NpcInteraction,
    /// System narration (weather shifts, festivals, life events).
    SystemMessage,
}

impl ChatEventType {
    fn as_str(self) -> &'static str {
        match self {
            ChatEventType::NpcDialogue => "npc_dialogue",
            ChatEventType::PlayerMove => "player_move",
            ChatEventType::NpcInteraction => "npc_interaction",
            ChatEventType::SystemMessage => "system_message",
        }
    }
}

/// Optional metadata attached to a transcript record.
///
/// All fields are skipped when absent, so non-LLM events naturally omit
/// `request_id` and player-move events naturally omit `npc_id`.
#[derive(Debug, Clone, Default)]
struct ChatEventMeta {
    location_id: Option<u32>,
    npc_id: Option<u32>,
    npc_name: Option<String>,
    request_id: Option<u64>,
}

struct JsonlRecord(String);

/// Handle to the chat transcript writer.
///
/// Same shape as `InferenceFileLog` — `Clone` handle, bounded mpsc to a
/// writer task, shared `Arc<AtomicBool>` enable flag.
#[derive(Clone)]
pub struct ChatTranscriptLog {
    tx: Option<mpsc::Sender<JsonlRecord>>,
    enabled: Arc<AtomicBool>,
    path: PathBuf,
    session_id: String,
    event_counter: Arc<AtomicU64>,
    dropped: Arc<AtomicU64>,
}

impl ChatTranscriptLog {
    /// Permanently-disabled handle for test fixtures and synthetic AppStates.
    pub fn disabled() -> Self {
        Self {
            tx: None,
            enabled: Arc::new(AtomicBool::new(false)),
            path: PathBuf::new(),
            session_id: String::new(),
            event_counter: Arc::new(AtomicU64::new(0)),
            dropped: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Spawn a writer task with its own enable flag.
    pub fn spawn(saves_dir: &Path, session_id: String, enabled: bool) -> Self {
        let flag = Arc::new(AtomicBool::new(enabled));
        Self::spawn_with_flag(saves_dir, session_id, flag)
    }

    /// Spawn a writer task sharing an existing `Arc<AtomicBool>` enable flag
    /// (so the same `/inference-log off` toggle silences both writers).
    ///
    /// The writer task is spawned even when the flag starts `false`, so a
    /// later `/inference-log on` begins writing without a restart. The file
    /// is opened lazily on the first accepted record, so a session that
    /// stays opted-out never creates an empty transcript on disk.
    pub fn spawn_with_flag(saves_dir: &Path, session_id: String, enabled: Arc<AtomicBool>) -> Self {
        let dir = saves_dir.join("inference_logs");
        let path = dir.join(format!("{session_id}.transcript.jsonl"));

        let (tx, rx) = mpsc::channel::<JsonlRecord>(CHANNEL_CAPACITY);
        let writer_enabled = enabled.clone();
        let writer_path = path.clone();
        tokio::spawn(async move {
            run_writer(dir, writer_path, rx, writer_enabled).await;
        });

        if enabled.load(Ordering::Relaxed) {
            tracing::info!(
                target: "parish_core::chat_transcript",
                "Chat transcript logging enabled. File: {}",
                path.display()
            );
        }

        Self {
            tx: Some(tx),
            enabled,
            path,
            session_id,
            event_counter: Arc::new(AtomicU64::new(0)),
            dropped: Arc::new(AtomicU64::new(0)),
        }
    }

    /// The absolute path the writer is targeting.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Session id (matches the paired inference log filename).
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Whether records are being written.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Runtime enable/disable toggle.
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    /// Records a relevant [`GameEvent`] to the transcript.
    ///
    /// This is the bus-subscriber entry point: each runtime subscribes to
    /// `world.event_bus` and forwards every event here, mirroring
    /// `CharacterLogManager`/`LocationLogManager`. Only player-visible
    /// chat events are written; mechanical events (mood/relationship
    /// deltas, NPC arrivals/departures) are ignored. Silent no-op when
    /// disabled or detached.
    ///
    /// `world` / `npc_manager` are borrowed to resolve display names and the
    /// player's current location, matching the sibling log managers' shape.
    pub fn process_event(&self, event: &GameEvent, world: &WorldState, npc_manager: &NpcManager) {
        if !self.is_enabled() || self.tx.is_none() {
            return;
        }

        let npc_name = |id: parish_npc::NpcId| -> Option<String> {
            npc_manager
                .get(id)
                .map(|npc| npc_manager.display_name(npc).to_string())
        };
        let loc_name = |id: parish_world::LocationId| -> String {
            world
                .graph
                .get(id)
                .map(|d| d.name.clone())
                .unwrap_or_else(|| format!("location {}", id.0))
        };

        match event {
            GameEvent::ReactionRecorded { .. } => {}
            GameEvent::DialogueOccurred {
                npc_id,
                summary,
                npc_said,
                request_id,
                ..
            } => {
                let text = npc_said.clone().unwrap_or_else(|| summary.clone());
                if text.trim().is_empty() {
                    return;
                }
                self.emit(
                    ChatEventType::NpcDialogue,
                    &text,
                    ChatEventMeta {
                        location_id: Some(world.player_location.0),
                        npc_id: Some(npc_id.0),
                        npc_name: npc_name(*npc_id),
                        request_id: *request_id,
                    },
                );
            }
            GameEvent::PlayerMoved { from, to, .. } => {
                let text = format!("Travelled from {} to {}.", loc_name(*from), loc_name(*to));
                self.emit(
                    ChatEventType::PlayerMove,
                    &text,
                    ChatEventMeta {
                        location_id: Some(to.0),
                        ..ChatEventMeta::default()
                    },
                );
            }
            GameEvent::NpcInteraction {
                participants,
                location,
                summary,
                ..
            } => {
                if summary.trim().is_empty() {
                    return;
                }
                self.emit(
                    ChatEventType::NpcInteraction,
                    summary,
                    ChatEventMeta {
                        location_id: Some(location.0),
                        npc_id: participants.first().map(|n| n.0),
                        npc_name: participants.first().and_then(|n| npc_name(*n)),
                        request_id: None,
                    },
                );
            }
            GameEvent::WeatherChanged { new_weather, .. } => {
                self.emit(
                    ChatEventType::SystemMessage,
                    &format!("The weather turns {new_weather}."),
                    ChatEventMeta::default(),
                );
            }
            GameEvent::FestivalStarted { name, .. } => {
                self.emit(
                    ChatEventType::SystemMessage,
                    &format!("{name} begins in the parish."),
                    ChatEventMeta::default(),
                );
            }
            GameEvent::PlayerTaskAssigned { task, .. } => {
                self.emit(
                    ChatEventType::SystemMessage,
                    &format!("New task: {}", task.description),
                    ChatEventMeta {
                        location_id: Some(task.location.0),
                        npc_id: Some(task.assigned_by.0),
                        npc_name: npc_name(task.assigned_by),
                        request_id: None,
                    },
                );
            }
            GameEvent::PlayerTaskProgressed { task, .. } => {
                self.emit(
                    ChatEventType::SystemMessage,
                    &format!("Task in progress: {}", task.description),
                    ChatEventMeta {
                        location_id: Some(task.location.0),
                        npc_id: Some(task.assigned_by.0),
                        npc_name: npc_name(task.assigned_by),
                        request_id: None,
                    },
                );
            }
            // Mechanical / non-chat events are not part of the transcript.
            GameEvent::MoodChanged { .. }
            | GameEvent::RelationshipChanged { .. }
            | GameEvent::NpcArrived { .. }
            | GameEvent::NpcDeparted { .. }
            | GameEvent::NpcActivity { .. }
            | GameEvent::GossipSpread { .. }
            | GameEvent::AddressedAbsentNpc { .. }
            | GameEvent::LifeEvent { .. } => {}
        }
    }

    /// Serialises one record and queues it for the writer task. Internal;
    /// callers use [`Self::process_event`].
    fn emit(&self, event: ChatEventType, text: &str, meta: ChatEventMeta) {
        let Some(tx) = self.tx.as_ref() else {
            return;
        };
        let event_id = self.event_counter.fetch_add(1, Ordering::Relaxed);
        let line = match serialise(event, text, &meta, &self.session_id, event_id) {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!(
                    target: "parish_core::chat_transcript",
                    "failed to serialise transcript event: {err}",
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
                        target: "parish_core::chat_transcript",
                        "chat transcript writer is behind; {} event(s) dropped so far",
                        prev + 1
                    );
                }
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                self.set_enabled(false);
            }
        }
    }
}

fn serialise(
    event: ChatEventType,
    text: &str,
    meta: &ChatEventMeta,
    session_id: &str,
    event_id: u64,
) -> Result<String, serde_json::Error> {
    let mut map = Map::new();
    map.insert(
        "timestamp".to_string(),
        Value::String(Utc::now().to_rfc3339()),
    );
    map.insert(
        "parish.session_id".to_string(),
        Value::String(session_id.to_string()),
    );
    map.insert(
        "parish.event_id".to_string(),
        Value::Number(event_id.into()),
    );
    map.insert(
        "event_type".to_string(),
        Value::String(event.as_str().to_string()),
    );
    map.insert("text".to_string(), Value::String(scrub(text).into_owned()));
    if let Some(loc) = meta.location_id {
        map.insert("parish.location_id".to_string(), Value::Number(loc.into()));
    }
    if let Some(npc) = meta.npc_id {
        map.insert("parish.npc_id".to_string(), Value::Number(npc.into()));
    }
    if let Some(name) = meta.npc_name.as_ref() {
        map.insert(
            "parish.npc_name".to_string(),
            Value::String(scrub(name).into_owned()),
        );
    }
    if let Some(req) = meta.request_id {
        map.insert("parish.request_id".to_string(), Value::Number(req.into()));
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
    // mid-session); deferring the open keeps an opted-out session that is
    // never toggled on from leaving an empty transcript on disk.
    let mut writer: Option<BufWriter<File>> = None;

    // Note: the `enabled` flag is checked at the send site (`process_event`)
    // so by the time a record is on the channel it was admitted under
    // `enabled=true`. The writer task itself does not re-check the flag —
    // doing so would race with a runtime toggle and silently drop accepted
    // records. The flag is still flipped on disk failure below to stop
    // further writes.
    while let Some(JsonlRecord(line)) = rx.recv().await {
        if writer.is_none() {
            match open_transcript_file(&dir, &path).await {
                Ok(file) => writer = Some(BufWriter::new(file)),
                Err(err) => {
                    tracing::warn!(
                        target: "parish_core::chat_transcript",
                        "could not open transcript file {}: {err}; disabling transcript",
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
                target: "parish_core::chat_transcript",
                "transcript write failed: {err}; disabling transcript"
            );
            enabled.store(false, Ordering::Relaxed);
            return;
        }
        if let Err(err) = writer.flush().await {
            tracing::warn!(
                target: "parish_core::chat_transcript",
                "transcript flush failed: {err}; disabling transcript"
            );
            enabled.store(false, Ordering::Relaxed);
            return;
        }
    }
    if let Some(mut writer) = writer {
        let _ = writer.flush().await;
    }
}

/// Creates the log directory (if needed) and opens the transcript file for
/// append. Called lazily by [`run_writer`] on the first record.
async fn open_transcript_file(dir: &Path, path: &Path) -> std::io::Result<File> {
    create_dir_all(dir).await?;
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialise_npc_dialogue_carries_request_id() {
        let meta = ChatEventMeta {
            location_id: Some(7),
            npc_id: Some(42),
            npc_name: Some("Ciarán".to_string()),
            request_id: Some(123),
        };
        let line = serialise(
            ChatEventType::NpcDialogue,
            "Sláinte, traveller.",
            &meta,
            "sess-1",
            0,
        )
        .unwrap();
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(v["event_type"], "npc_dialogue");
        assert_eq!(v["text"], "Sláinte, traveller.");
        assert_eq!(v["parish.request_id"], 123);
        assert_eq!(v["parish.npc_id"], 42);
        assert_eq!(v["parish.location_id"], 7);
        assert_eq!(v["parish.session_id"], "sess-1");
        assert_eq!(v["parish.event_id"], 0);
        assert_eq!(v["parish.schema_version"], 1);
    }

    #[test]
    fn serialise_player_move_has_no_request_id() {
        let meta = ChatEventMeta {
            location_id: Some(3),
            ..ChatEventMeta::default()
        };
        let line = serialise(ChatEventType::PlayerMove, "Travelled.", &meta, "sess-1", 5).unwrap();
        let v: Value = serde_json::from_str(line.trim()).unwrap();
        let obj = v.as_object().unwrap();
        assert!(
            !obj.contains_key("parish.request_id"),
            "player_move must not carry a request_id"
        );
        assert!(!obj.contains_key("parish.npc_id"), "player_move has no npc");
        assert_eq!(v["event_type"], "player_move");
    }

    #[test]
    fn serialise_scrubs_secrets() {
        let meta = ChatEventMeta::default();
        let line = serialise(
            ChatEventType::NpcDialogue,
            "my key is sk-proj-AAAAAAAAAAAAAAAAAAAAAAAA",
            &meta,
            "s",
            0,
        )
        .unwrap();
        assert!(line.contains("[REDACTED:openai_key]"));
        assert!(!line.contains("AAAAAAAAAAAAAAAAAAAAAAAA"));
    }

    async fn read_lines(path: &std::path::Path, timeout_ms: u64) -> Vec<String> {
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

    fn dialogue_event() -> GameEvent {
        GameEvent::DialogueOccurred {
            npc_id: parish_npc::NpcId(1),
            location: parish_world::LocationId(1),
            summary: "greeted the player".into(),
            player_said: Some("hello".into()),
            npc_said: Some("Well met.".into()),
            request_id: Some(7),
            timestamp: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn process_event_writes_dialogue_with_request_id() {
        let tmp = tempfile::tempdir().unwrap();
        let log = ChatTranscriptLog::spawn(tmp.path(), "sess".to_string(), true);
        let path = log.path().to_path_buf();
        let world = WorldState::new();
        let npcs = NpcManager::new();
        log.process_event(&dialogue_event(), &world, &npcs);
        drop(log);
        let lines = read_lines(&path, 1000).await;
        assert_eq!(lines.len(), 1);
        let v: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(v["event_type"], "npc_dialogue");
        assert_eq!(v["text"], "Well met.");
        assert_eq!(v["parish.request_id"], 7);
    }

    #[tokio::test]
    async fn process_event_ignores_mechanical_events() {
        let tmp = tempfile::tempdir().unwrap();
        let log = ChatTranscriptLog::spawn(tmp.path(), "sess".to_string(), true);
        let path = log.path().to_path_buf();
        let world = WorldState::new();
        let npcs = NpcManager::new();
        log.process_event(
            &GameEvent::MoodChanged {
                npc_id: parish_npc::NpcId(1),
                new_mood: "content".into(),
                location: parish_world::LocationId(1),
                timestamp: chrono::Utc::now(),
            },
            &world,
            &npcs,
        );
        drop(log);
        let lines = read_lines(&path, 300).await;
        assert!(lines.is_empty(), "mechanical events must not be recorded");
    }

    #[tokio::test]
    async fn process_event_records_chat_relevant_variants() {
        use parish_world::LocationId;
        let tmp = tempfile::tempdir().unwrap();
        let log = ChatTranscriptLog::spawn(tmp.path(), "sess".to_string(), true);
        let path = log.path().to_path_buf();
        let world = WorldState::new();
        let npcs = NpcManager::new();
        let now = chrono::Utc::now();

        log.process_event(
            &GameEvent::PlayerMoved {
                from: LocationId(1),
                to: LocationId(2),
                timestamp: now,
            },
            &world,
            &npcs,
        );
        log.process_event(
            &GameEvent::NpcInteraction {
                participants: vec![parish_npc::NpcId(1), parish_npc::NpcId(2)],
                location: LocationId(3),
                summary: "argued over a fence".into(),
                timestamp: now,
            },
            &world,
            &npcs,
        );
        log.process_event(
            &GameEvent::WeatherChanged {
                new_weather: "Storm".into(),
                timestamp: now,
            },
            &world,
            &npcs,
        );
        log.process_event(
            &GameEvent::FestivalStarted {
                name: "Bealtaine".into(),
                timestamp: now,
                location: None,
            },
            &world,
            &npcs,
        );
        drop(log);

        let lines = read_lines(&path, 1000).await;
        let kinds: Vec<String> = lines
            .iter()
            .map(|l| {
                serde_json::from_str::<Value>(l).unwrap()["event_type"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                "player_move",
                "npc_interaction",
                "system_message",
                "system_message"
            ]
        );
        // The NPC-interaction beat carries the lead participant + location;
        // the player-move line records the destination but no request id.
        let interaction: Value = serde_json::from_str(&lines[1]).unwrap();
        assert_eq!(interaction["text"], "argued over a fence");
        assert_eq!(interaction["parish.location_id"], 3);
        let mv: Value = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(mv["parish.location_id"], 2);
        assert!(mv.as_object().unwrap().get("parish.request_id").is_none());
    }

    #[tokio::test]
    async fn shared_flag_silences_writer() {
        let tmp = tempfile::tempdir().unwrap();
        let flag = Arc::new(AtomicBool::new(true));
        let log = ChatTranscriptLog::spawn_with_flag(
            tmp.path(),
            "test-session".to_string(),
            flag.clone(),
        );
        let path = log.path().to_path_buf();
        let world = WorldState::new();
        let npcs = NpcManager::new();
        log.process_event(&dialogue_event(), &world, &npcs);
        flag.store(false, Ordering::Relaxed);
        log.process_event(&dialogue_event(), &world, &npcs);
        drop(log);
        let lines = read_lines(&path, 1000).await;
        assert_eq!(lines.len(), 1, "second record dropped while flag was off");
    }

    #[tokio::test]
    async fn disabled_handle_is_noop() {
        let log = ChatTranscriptLog::disabled();
        assert!(!log.is_enabled());
        let world = WorldState::new();
        let npcs = NpcManager::new();
        log.process_event(&dialogue_event(), &world, &npcs);
        assert!(log.path().as_os_str().is_empty());
    }

    /// Regression: starting with the shared flag OFF must still allow a later
    /// `/inference-log on` to begin capturing the transcript. The writer is
    /// spawned eagerly; the file opens lazily on the first admitted record.
    #[tokio::test]
    async fn start_disabled_then_enable_begins_writing() {
        let tmp = tempfile::tempdir().unwrap();
        let flag = Arc::new(AtomicBool::new(false));
        let log = ChatTranscriptLog::spawn_with_flag(tmp.path(), "sess".to_string(), flag.clone());
        let path = log.path().to_path_buf();
        let world = WorldState::new();
        let npcs = NpcManager::new();
        // While off: dropped, no file created.
        log.process_event(&dialogue_event(), &world, &npcs);
        assert!(!path.exists(), "no file should exist while logging is off");
        // Flip on at runtime — must now write.
        flag.store(true, Ordering::Relaxed);
        log.process_event(&dialogue_event(), &world, &npcs);
        drop(log);
        let lines = read_lines(&path, 1000).await;
        assert_eq!(lines.len(), 1, "record after enable must be written");
    }
}
