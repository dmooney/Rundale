use std::path::Path;
use std::path::PathBuf;

use parish_core::game_loop::save::{
    NewGameParams, do_new_game, do_save_game, load_fresh_world_and_npcs,
};
use parish_core::game_mod::GameMod;
use parish_core::ipc::ConversationRuntimeState;
use parish_core::ipc::event_emitter::EventEmitter;
use parish_core::npc::manager::NpcManager;
use parish_core::persistence::Database;
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
    let emitter = NoopEmitter;

    let params = NewGameParams {
        world: &world,
        npc_manager: &npc_manager,
        conversation: &conversation,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        saves_dir: tmp.path(),
        game_mod: Some(&game_mod),
        data_dir: &mod_dir,
        pronunciations: &game_mod.pronunciations,
        default_transport: game_mod.transport.default_mode(),
        emitter: &emitter,
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
    let emitter = NoopEmitter;

    let params = NewGameParams {
        world: &world,
        npc_manager: &npc_manager,
        conversation: &conversation,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        saves_dir: tmp.path(),
        game_mod: Some(&game_mod),
        data_dir: &mod_dir,
        pronunciations: &game_mod.pronunciations,
        default_transport: game_mod.transport.default_mode(),
        emitter: &emitter,
    };

    do_new_game(params).await.unwrap();

    let conv = conversation.lock().await;
    assert!(
        conv.location.is_none(),
        "conversation location should reset"
    );
    assert!(conv.transcript.is_empty(), "transcript should be cleared");
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
    let emitter = NoopEmitter;

    let params = NewGameParams {
        world: &world,
        npc_manager: &npc_manager,
        conversation: &conversation,
        save_path: &save_path,
        current_branch_id: &current_branch_id,
        current_branch_name: &current_branch_name,
        saves_dir: tmp.path(),
        game_mod: None,
        data_dir: data_dir.path(),
        pronunciations: &[],
        default_transport: &TransportMode::walking(),
        emitter: &emitter,
    };

    let result = do_new_game(params).await;
    assert!(result.is_err(), "should fail when no mod and no data files");
    let err = result.unwrap_err();
    assert!(
        err.contains("Failed to load world"),
        "error should mention world load failure: {err}"
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

    let msg = do_save_game(
        &world,
        &npc_manager,
        &save_path,
        &current_branch_id,
        &current_branch_name,
        tmp.path(),
    )
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

    let msg = do_save_game(
        &world,
        &npc_manager,
        &save_path,
        &current_branch_id,
        &current_branch_name,
        tmp.path(),
    )
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

    do_save_game(
        &world,
        &npc_manager,
        &save_path,
        &current_branch_id,
        &current_branch_name,
        tmp.path(),
    )
    .await
    .unwrap();

    do_save_game(
        &world,
        &npc_manager,
        &save_path,
        &current_branch_id,
        &current_branch_name,
        tmp.path(),
    )
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

    let msg = do_save_game(
        &world,
        &npc_manager,
        &save_path,
        &current_branch_id,
        &current_branch_name,
        tmp.path(),
    )
    .await
    .expect("do_save_game with no branch should auto-resolve");

    assert!(msg.contains("main"), "should mention 'main' branch: {msg}");

    let bid = current_branch_id.lock().await;
    assert!(bid.is_some(), "branch_id should be populated after save");
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
