use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;

use parish_core::game_loop::save::{
    NewGameParams, SaveGameParams, do_new_game, do_save_game, load_fresh_world_and_npcs,
};
use parish_core::game_mod::GameMod;
use parish_core::ipc::ConversationRuntimeState;
use parish_core::ipc::event_emitter::EventEmitter;
use parish_core::npc::manager::NpcManager;
use parish_core::persistence::Database;
use parish_core::session_store::DbSessionStore;
use parish_core::world::events::GameEvent;
use parish_core::world::transport::TransportMode;
use parish_core::world::{LocationId, WorldState};
use tempfile::TempDir;
use tokio::sync::Mutex;

/// Returns the path to the `mods/rundale` fixture directory relative to the
/// repo root.
fn rundale_mod_dir() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir.join("../../../mods/rundale")
}

/// A no-op EventEmitter for tests that don't need to inspect events.
struct NoopEmitter;

impl EventEmitter for NoopEmitter {
    fn emit_event(&self, _name: &str, _payload: serde_json::Value) {}
}

#[derive(Default)]
struct RecordingEmitter(std::sync::Mutex<Vec<String>>);

impl EventEmitter for RecordingEmitter {
    fn emit_event(&self, name: &str, _payload: serde_json::Value) {
        self.0.lock().unwrap().push(name.to_string());
    }
}

fn save_files(dir: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "db"))
        .collect();
    paths.sort();
    paths
}

// ── load_fresh_world_and_npcs ────────────────────────────────────────────────

#[test]
fn load_fresh_world_and_npcs_with_mod_loads_world_and_npcs() {
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).expect("failed to load rundale mod");

    let (world, npc_manager) = load_fresh_world_and_npcs(Some(&game_mod), &mod_dir)
        .expect("load_fresh_world_and_npcs failed");

    assert_eq!(
        world.player_location,
        LocationId(15),
        "player should start at the mod's start location"
    );
    let npc_count: usize = npc_manager.all_npcs().count();
    assert!(
        npc_count > 0,
        "should have loaded NPCs from mod (got {npc_count})"
    );
}

#[test]
fn load_fresh_world_and_npcs_with_mod_loads_rundale_world_graph() {
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).unwrap();

    let (world, _) = load_fresh_world_and_npcs(Some(&game_mod), &mod_dir).unwrap();

    let loc_count = world.graph.location_ids().len();
    assert!(
        loc_count > 10,
        "rundale world should have many locations (got {loc_count})"
    );

    let start_loc = world.graph.get(LocationId(15));
    assert!(
        start_loc.is_some(),
        "start location (id 15) should exist in the graph"
    );
    if let Some(start) = start_loc {
        assert_eq!(start.name, "Kilteevan Village");
    }
}

#[test]
fn load_fresh_world_and_npcs_without_mod_uses_data_dir() {
    let mod_dir = rundale_mod_dir();

    let (world, npc_manager) = load_fresh_world_and_npcs(None, &mod_dir).unwrap();

    assert!(
        world.graph.location_ids().len() > 10,
        "world should load from data dir"
    );
    let npc_count: usize = npc_manager.all_npcs().count();
    assert!(
        npc_count > 0,
        "NPCs should load from data dir (got {npc_count})"
    );
}

#[test]
fn load_fresh_world_and_npcs_without_mod_returns_empty_npcs_when_file_missing() {
    let tmp = TempDir::new().unwrap();
    let src = rundale_mod_dir().join("world.json");
    std::fs::copy(&src, tmp.path().join("world.json")).unwrap();

    let (world, npc_manager) = load_fresh_world_and_npcs(None, tmp.path()).unwrap();

    assert!(
        world.graph.location_ids().len() > 10,
        "world should load from copied world.json"
    );
    let npc_count: usize = npc_manager.all_npcs().count();
    assert_eq!(
        npc_count, 0,
        "NPC manager should be empty when npcs.json is missing"
    );
}

// ── do_new_game ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn do_new_game_creates_save_file_and_updates_state() {
    let tmp = TempDir::new().unwrap();
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).unwrap();

    let world = Mutex::new(WorldState::new());
    let npc_manager = Mutex::new(NpcManager::new());
    let conversation = Mutex::new(ConversationRuntimeState::new());
    let save_path: Mutex<Option<PathBuf>> = Mutex::new(None);
    let current_branch_id: Mutex<Option<i64>> = Mutex::new(None);
    let current_branch_name: Mutex<Option<String>> = Mutex::new(None);
    let save_lock = Mutex::new(None);
    let game_events: Mutex<VecDeque<GameEvent>> = Mutex::new(VecDeque::new());
    let emitter = NoopEmitter;
    let session_store = DbSessionStore::new(tmp.path().to_path_buf());

    let params = NewGameParams {
        world: &world,
        npc_manager: &npc_manager,
        conversation: &conversation,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: tmp.path(),
        session_store: &session_store,
        session_id: "",
        game_mod: Some(&game_mod),
        data_dir: &mod_dir,
        pronunciations: &game_mod.pronunciations,
        default_transport: game_mod.transport.default_mode(),
        emitter: &emitter,
        game_events: &game_events,
    };

    do_new_game(params).await.expect("do_new_game failed");

    {
        let sp = save_path.lock().await;
        assert!(sp.is_some(), "save_path should be set after do_new_game");
        let path = sp.as_ref().unwrap();
        assert!(path.exists(), "save file should exist on disk");
        assert!(path.to_string_lossy().contains("parish_001.db"));
    }

    {
        let bid = current_branch_id.lock().await;
        assert!(bid.is_some(), "branch_id should be set");
        assert_eq!(*bid, Some(1), "first branch should be id 1");
    }

    {
        let bname = current_branch_name.lock().await;
        assert_eq!(bname.as_deref(), Some("main"));
    }

    {
        let w = world.lock().await;
        assert_eq!(
            w.player_location,
            LocationId(15),
            "world should have been replaced with fresh state from mod"
        );
    }

    {
        let nm = npc_manager.lock().await;
        let npc_count: usize = nm.all_npcs().count();
        assert!(
            npc_count > 0,
            "NPC manager should be populated after new game (got {npc_count})"
        );
    }

    let snapshot_count = count_snapshots_in_db(&save_path.lock().await.clone().unwrap());
    assert_eq!(
        snapshot_count, 1,
        "exactly one snapshot should exist in the new save"
    );
}

#[tokio::test]
async fn do_new_game_resets_conversation_state() {
    let tmp = TempDir::new().unwrap();
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).unwrap();

    let world = Mutex::new(WorldState::new());
    let npc_manager = Mutex::new(NpcManager::new());
    let mut conv_state = ConversationRuntimeState::new();
    conv_state.location = Some(LocationId(99));
    let conversation = Mutex::new(conv_state);
    let save_path: Mutex<Option<PathBuf>> = Mutex::new(None);
    let current_branch_id: Mutex<Option<i64>> = Mutex::new(None);
    let current_branch_name: Mutex<Option<String>> = Mutex::new(None);
    let save_lock = Mutex::new(None);
    let game_events: Mutex<VecDeque<GameEvent>> = Mutex::new(VecDeque::new());
    let emitter = NoopEmitter;
    let session_store = DbSessionStore::new(tmp.path().to_path_buf());

    let params = NewGameParams {
        world: &world,
        npc_manager: &npc_manager,
        conversation: &conversation,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: tmp.path(),
        session_store: &session_store,
        session_id: "",
        game_mod: Some(&game_mod),
        data_dir: &mod_dir,
        pronunciations: &game_mod.pronunciations,
        default_transport: game_mod.transport.default_mode(),
        emitter: &emitter,
        game_events: &game_events,
    };

    do_new_game(params).await.unwrap();

    let conv = conversation.lock().await;
    assert!(
        conv.location.is_none(),
        "conversation location should reset"
    );
    assert!(conv.transcript.is_empty(), "transcript should be cleared");
}

/// #1395 — `do_new_game` must clear the world-event ring buffer so that stale
/// events from game A do not bleed into game B's `parish_turn` responses.
///
/// The `read_events_since` cursor derives from `events.len()`, so clearing the
/// deque also resets the cursor to 0.
#[tokio::test]
async fn do_new_game_clears_game_events_ring_buffer() {
    use chrono::Utc;
    use parish_core::npc::NpcId;
    use parish_core::world::LocationId;
    use parish_core::world::events::GameEvent;

    let tmp = TempDir::new().unwrap();
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).unwrap();

    let world = Mutex::new(WorldState::new());
    let npc_manager = Mutex::new(NpcManager::new());
    let conversation = Mutex::new(ConversationRuntimeState::new());
    let save_path: Mutex<Option<PathBuf>> = Mutex::new(None);
    let current_branch_id: Mutex<Option<i64>> = Mutex::new(None);
    let current_branch_name: Mutex<Option<String>> = Mutex::new(None);
    let save_lock = Mutex::new(None);
    let game_events: Mutex<VecDeque<GameEvent>> = Mutex::new(VecDeque::new());
    let emitter = NoopEmitter;
    let session_store = DbSessionStore::new(tmp.path().to_path_buf());

    // Pre-populate game_events with stale events from "game A".
    {
        let mut events = game_events.lock().await;
        events.push_back(GameEvent::NpcArrived {
            npc_id: NpcId(1),
            location: LocationId(1),
            timestamp: Utc::now(),
        });
        events.push_back(GameEvent::WeatherChanged {
            new_weather: "Rain".to_string(),
            timestamp: Utc::now(),
        });
        assert_eq!(events.len(), 2, "pre-condition: two stale events");
    }

    let params = NewGameParams {
        world: &world,
        npc_manager: &npc_manager,
        conversation: &conversation,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: tmp.path(),
        session_store: &session_store,
        session_id: "",
        game_mod: Some(&game_mod),
        data_dir: &mod_dir,
        pronunciations: &game_mod.pronunciations,
        default_transport: game_mod.transport.default_mode(),
        emitter: &emitter,
        game_events: &game_events,
    };

    do_new_game(params).await.expect("do_new_game failed");

    // After new-game the ring must be empty — no bleed from game A.
    let events_after = game_events.lock().await;
    assert!(
        events_after.is_empty(),
        "game_events must be cleared on new-game; found {} stale event(s)",
        events_after.len()
    );
    // Cursor (derived from len()) is also 0 — rebaselined for game B.
    assert_eq!(
        events_after.len(),
        0,
        "event cursor derived from len() must be 0 after new-game"
    );
}

#[tokio::test]
async fn do_new_game_without_mod_fallback_to_data_dir_errors() {
    let tmp = TempDir::new().unwrap();
    let data_dir = TempDir::new().unwrap();

    let world = Mutex::new(WorldState::new());
    let npc_manager = Mutex::new(NpcManager::new());
    let conversation = Mutex::new(ConversationRuntimeState::new());
    let save_path: Mutex<Option<PathBuf>> = Mutex::new(None);
    let current_branch_id: Mutex<Option<i64>> = Mutex::new(None);
    let current_branch_name: Mutex<Option<String>> = Mutex::new(None);
    let save_lock = Mutex::new(None);
    let game_events: Mutex<VecDeque<GameEvent>> = Mutex::new(VecDeque::new());
    let emitter = NoopEmitter;
    let session_store = DbSessionStore::new(tmp.path().to_path_buf());

    let params = NewGameParams {
        world: &world,
        npc_manager: &npc_manager,
        conversation: &conversation,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: tmp.path(),
        session_store: &session_store,
        session_id: "",
        game_mod: None,
        data_dir: data_dir.path(),
        pronunciations: &[],
        default_transport: &TransportMode::walking(),
        emitter: &emitter,
        game_events: &game_events,
    };

    let result = do_new_game(params).await;
    assert!(result.is_err(), "should fail when no mod and no data files");
    let err = result.unwrap_err();
    assert!(
        err.contains("Failed to load world"),
        "error should mention world load failure: {err}"
    );
}

#[tokio::test]
async fn do_new_game_marker_failure_removes_candidate_and_preserves_live_context_for_retry() {
    let tmp = TempDir::new().unwrap();
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).unwrap();

    let old_path = tmp.path().join("parish_001.db");
    Database::open(&old_path).unwrap();
    let old_lock = parish_core::persistence::SaveFileLock::try_acquire(&old_path).unwrap();
    let marker_obstruction = tmp.path().join(".active-save.json");
    std::fs::create_dir(&marker_obstruction).unwrap();

    let mut old_world = WorldState::new();
    old_world.player_location = LocationId(777);
    let world = Mutex::new(old_world);
    let npc_manager = Mutex::new(NpcManager::new());
    let mut old_conversation = ConversationRuntimeState::new();
    old_conversation.location = Some(LocationId(777));
    let conversation = Mutex::new(old_conversation);
    let save_path = Mutex::new(Some(old_path.clone()));
    let current_branch_id = Mutex::new(Some(1));
    let current_branch_name = Mutex::new(Some("main".to_string()));
    let save_lock = Mutex::new(Some(old_lock));
    let mut old_events = VecDeque::new();
    old_events.push_back(GameEvent::WeatherChanged {
        new_weather: "Rain".to_string(),
        timestamp: chrono::Utc::now(),
    });
    let game_events = Mutex::new(old_events);
    let emitter = RecordingEmitter::default();
    let session_store = DbSessionStore::new(tmp.path().to_path_buf());

    let result = do_new_game(NewGameParams {
        world: &world,
        npc_manager: &npc_manager,
        conversation: &conversation,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: tmp.path(),
        session_store: &session_store,
        session_id: "",
        game_mod: Some(&game_mod),
        data_dir: &mod_dir,
        pronunciations: &game_mod.pronunciations,
        default_transport: game_mod.transport.default_mode(),
        emitter: &emitter,
        game_events: &game_events,
    })
    .await;

    assert!(result.is_err(), "the obstructed marker must fail closed");
    assert_eq!(save_files(tmp.path()), vec![old_path.clone()]);
    assert_eq!(world.lock().await.player_location, LocationId(777));
    assert_eq!(conversation.lock().await.location, Some(LocationId(777)));
    assert_eq!(save_path.lock().await.as_ref(), Some(&old_path));
    assert_eq!(*current_branch_id.lock().await, Some(1));
    assert_eq!(current_branch_name.lock().await.as_deref(), Some("main"));
    assert!(save_lock.lock().await.is_some());
    assert_eq!(game_events.lock().await.len(), 1);
    assert!(emitter.0.lock().unwrap().is_empty());

    std::fs::remove_dir(&marker_obstruction).unwrap();
    do_new_game(NewGameParams {
        world: &world,
        npc_manager: &npc_manager,
        conversation: &conversation,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: tmp.path(),
        session_store: &session_store,
        session_id: "",
        game_mod: Some(&game_mod),
        data_dir: &mod_dir,
        pronunciations: &game_mod.pronunciations,
        default_transport: game_mod.transport.default_mode(),
        emitter: &emitter,
        game_events: &game_events,
    })
    .await
    .expect("retry should reuse and publish the cleaned candidate path");

    assert_eq!(
        save_path.lock().await.as_deref(),
        Some(tmp.path().join("parish_002.db").as_path())
    );
}

// ── do_save_game ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn do_save_game_without_existing_path_creates_new_save() {
    let tmp = TempDir::new().unwrap();
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).unwrap();

    let (world_state, npc_manager_state) =
        load_fresh_world_and_npcs(Some(&game_mod), &mod_dir).unwrap();
    let world = Mutex::new(world_state);
    let npc_manager = Mutex::new(npc_manager_state);
    let save_path: Mutex<Option<PathBuf>> = Mutex::new(None);
    let current_branch_id: Mutex<Option<i64>> = Mutex::new(None);
    let current_branch_name: Mutex<Option<String>> = Mutex::new(None);
    let save_lock = Mutex::new(None);
    let session_store = DbSessionStore::new(tmp.path().to_path_buf());

    let msg = do_save_game(SaveGameParams {
        world: &world,
        npc_manager: &npc_manager,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: tmp.path(),
        session_store: &session_store,
        session_id: "",
    })
    .await
    .expect("do_save_game failed");

    assert!(
        msg.contains("Game saved to"),
        "success message should mention save: {msg}"
    );

    {
        let sp = save_path.lock().await;
        assert!(sp.is_some(), "save_path should be set");
        assert!(sp.as_ref().unwrap().exists(), "save file should exist");
    }
    assert!(
        save_lock.lock().await.is_some(),
        "new saves must retain their advisory lock"
    );
}

#[tokio::test]
async fn do_save_game_with_existing_path_writes_snapshot() {
    let tmp = TempDir::new().unwrap();
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).unwrap();

    let (world_state, npc_manager_state) =
        load_fresh_world_and_npcs(Some(&game_mod), &mod_dir).unwrap();
    let world = Mutex::new(world_state);
    let npc_manager = Mutex::new(npc_manager_state);

    let db_path = tmp.path().join("parish_001.db");
    {
        let db = Database::open(&db_path).unwrap();
        let _main = db.find_branch("main").unwrap().unwrap();
    }
    let save_path: Mutex<Option<PathBuf>> = Mutex::new(Some(db_path.clone()));
    let current_branch_id: Mutex<Option<i64>> = Mutex::new(Some(1));
    let current_branch_name: Mutex<Option<String>> = Mutex::new(Some("main".to_string()));
    let save_lock = Mutex::new(None);
    let session_store = DbSessionStore::new(tmp.path().to_path_buf());

    let msg = do_save_game(SaveGameParams {
        world: &world,
        npc_manager: &npc_manager,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: tmp.path(),
        session_store: &session_store,
        session_id: "",
    })
    .await
    .expect("do_save_game with existing path failed");

    assert!(msg.contains("Game saved to"), "success message: {msg}");

    let snapshot_count = count_snapshots_in_db(&db_path);
    assert_eq!(
        snapshot_count, 1,
        "one snapshot should exist after initial save"
    );
}

#[tokio::test]
async fn do_save_game_multiple_saves_accumulate_snapshots() {
    let tmp = TempDir::new().unwrap();
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).unwrap();

    let (world_state, npc_manager_state) =
        load_fresh_world_and_npcs(Some(&game_mod), &mod_dir).unwrap();
    let world = Mutex::new(world_state);
    let npc_manager = Mutex::new(npc_manager_state);

    let db_path = tmp.path().join("parish_001.db");
    {
        let db = Database::open(&db_path).unwrap();
        let _main = db.find_branch("main").unwrap().unwrap();
    }
    let save_path: Mutex<Option<PathBuf>> = Mutex::new(Some(db_path.clone()));
    let current_branch_id: Mutex<Option<i64>> = Mutex::new(Some(1));
    let current_branch_name: Mutex<Option<String>> = Mutex::new(Some("main".to_string()));
    let save_lock = Mutex::new(None);
    let session_store = DbSessionStore::new(tmp.path().to_path_buf());

    do_save_game(SaveGameParams {
        world: &world,
        npc_manager: &npc_manager,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: tmp.path(),
        session_store: &session_store,
        session_id: "",
    })
    .await
    .unwrap();

    do_save_game(SaveGameParams {
        world: &world,
        npc_manager: &npc_manager,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: tmp.path(),
        session_store: &session_store,
        session_id: "",
    })
    .await
    .unwrap();

    let snapshot_count = count_snapshots_in_db(&db_path);
    assert_eq!(snapshot_count, 2, "two saves should produce two snapshots");
}

#[tokio::test]
async fn do_save_game_without_existing_branch_auto_resolves_main() {
    let tmp = TempDir::new().unwrap();
    let mod_dir = rundale_mod_dir();
    let game_mod = GameMod::load(&mod_dir).unwrap();

    let (world_state, npc_manager_state) =
        load_fresh_world_and_npcs(Some(&game_mod), &mod_dir).unwrap();
    let world = Mutex::new(world_state);
    let npc_manager = Mutex::new(npc_manager_state);

    let db_path = tmp.path().join("parish_001.db");
    {
        let db = Database::open(&db_path).unwrap();
        let _main = db.find_branch("main").unwrap().unwrap();
    }
    let save_path: Mutex<Option<PathBuf>> = Mutex::new(Some(db_path.clone()));
    let current_branch_id: Mutex<Option<i64>> = Mutex::new(None);
    let current_branch_name: Mutex<Option<String>> = Mutex::new(None);
    let save_lock = Mutex::new(None);
    let session_store = DbSessionStore::new(tmp.path().to_path_buf());

    let msg = do_save_game(SaveGameParams {
        world: &world,
        npc_manager: &npc_manager,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: tmp.path(),
        session_store: &session_store,
        session_id: "",
    })
    .await
    .expect("do_save_game with no branch should auto-resolve");

    assert!(msg.contains("main"), "should mention 'main' branch: {msg}");

    let bid = current_branch_id.lock().await;
    assert!(bid.is_some(), "branch_id should be populated after save");
}

#[tokio::test]
async fn do_save_game_failure_does_not_publish_partial_identity() {
    let tmp = TempDir::new().unwrap();
    let invalid_saves_dir = tmp.path().join("not-a-directory");
    std::fs::write(&invalid_saves_dir, b"file").unwrap();
    let world = Mutex::new(WorldState::new());
    let npc_manager = Mutex::new(NpcManager::new());
    let save_path: Mutex<Option<PathBuf>> = Mutex::new(None);
    let current_branch_id: Mutex<Option<i64>> = Mutex::new(None);
    let current_branch_name: Mutex<Option<String>> = Mutex::new(None);
    let save_lock = Mutex::new(None);
    let session_store = DbSessionStore::new(tmp.path().to_path_buf());

    let result = do_save_game(SaveGameParams {
        world: &world,
        npc_manager: &npc_manager,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: &invalid_saves_dir,
        session_store: &session_store,
        session_id: "",
    })
    .await;

    assert!(result.is_err());
    assert!(save_path.lock().await.is_none());
    assert!(current_branch_id.lock().await.is_none());
    assert!(current_branch_name.lock().await.is_none());
    assert!(save_lock.lock().await.is_none());
}

#[tokio::test]
async fn do_save_game_marker_failure_removes_candidate_and_retry_reuses_filename() {
    let tmp = TempDir::new().unwrap();
    let marker_obstruction = tmp.path().join(".active-save.json");
    std::fs::create_dir(&marker_obstruction).unwrap();
    let world = Mutex::new(WorldState::new());
    let npc_manager = Mutex::new(NpcManager::new());
    let save_path: Mutex<Option<PathBuf>> = Mutex::new(None);
    let current_branch_id: Mutex<Option<i64>> = Mutex::new(None);
    let current_branch_name: Mutex<Option<String>> = Mutex::new(None);
    let save_lock = Mutex::new(None);
    let session_store = DbSessionStore::new(tmp.path().to_path_buf());

    let result = do_save_game(SaveGameParams {
        world: &world,
        npc_manager: &npc_manager,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: tmp.path(),
        session_store: &session_store,
        session_id: "",
    })
    .await;

    assert!(result.is_err(), "the obstructed marker must fail closed");
    assert!(save_files(tmp.path()).is_empty());
    assert!(save_path.lock().await.is_none());
    assert!(current_branch_id.lock().await.is_none());
    assert!(current_branch_name.lock().await.is_none());
    assert!(save_lock.lock().await.is_none());

    std::fs::remove_dir(&marker_obstruction).unwrap();
    do_save_game(SaveGameParams {
        world: &world,
        npc_manager: &npc_manager,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        save_lock: &save_lock,
        saves_dir: tmp.path(),
        session_store: &session_store,
        session_id: "",
    })
    .await
    .expect("retry should reuse the cleaned candidate filename");

    assert_eq!(
        save_path.lock().await.as_deref(),
        Some(tmp.path().join("parish_001.db").as_path())
    );
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Returns the snapshot count for the main branch of a SQLite save file.
fn count_snapshots_in_db(path: &Path) -> usize {
    let db = Database::open(path).expect("failed to open db for counting");
    let branch = db
        .find_branch("main")
        .expect("find_branch failed")
        .expect("main branch must exist");
    db.branch_log(branch.id).expect("branch_log failed").len()
}
