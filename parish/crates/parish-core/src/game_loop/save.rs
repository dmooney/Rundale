//! Shared save-game and new-game helpers (#696).
//!
//! # Extraction history
//!
//! Slice 6: `load_fresh_world_and_npcs` — pure world + NPC reload.
//!
//! Slice 7: `do_save_game` — snapshot capture + persistence via
//! `Arc<dyn SessionStore>`.  Server and Tauri delegate to this; the
//! headless CLI retains its own inline implementation (different AppState
//! layout and uses `AsyncDatabase` directly rather than `SessionStore`).
//!
//! Slice 8: `do_new_game` — full new-game orchestration via
//! `Arc<dyn SessionStore>`.  Server and Tauri delegate to this; the CLI
//! continues using `handle_headless_new_game` (structurally different:
//! creates a new branch on an existing `AsyncDatabase` and calls print
//! helpers that are not part of the shared EventEmitter surface).
//!
//! # Architecture gate
//!
//! This module is backend-agnostic — it imports only `parish-core` types.
//! It must not import `axum`, `tauri`, or any crate in
//! `FORBIDDEN_FOR_BACKEND_AGNOSTIC`.

use std::path::{Path, PathBuf};

use tokio::sync::Mutex;

use crate::game_mod::{GameMod, PronunciationEntry};
use crate::ipc::{ConversationRuntimeState, EventEmitter, compute_name_hints, snapshot_from_world};
use crate::npc::manager::NpcManager;
use crate::persistence::picker::new_save_path;
use crate::persistence::{Database, GameSnapshot};
use crate::world::transport::TransportMode;
use crate::world::{DEFAULT_START_LOCATION, WorldState};

/// Loads a fresh [`WorldState`] and [`NpcManager`] for a new game.
///
/// Prefers the active game mod when `game_mod` is `Some`. Falls back to
/// legacy data files under `data_dir` when no mod is active.
///
/// This is a pure, synchronous operation — it reads from disk but does not
/// acquire any async locks or interact with any AppState.  Callers are
/// responsible for swapping the results into their live state under locks.
///
/// # Errors
///
/// Returns `Err(String)` if the world data cannot be loaded.  NPC load
/// failures are treated as soft errors (a warning is logged and an empty
/// `NpcManager` is returned).
///
/// # Parameters
///
/// - `game_mod`: the active game mod, if any.
/// - `data_dir`: legacy fallback data directory (used only when `game_mod` is
///   `None`).
pub fn load_fresh_world_and_npcs(
    game_mod: Option<&GameMod>,
    data_dir: &Path,
) -> Result<(WorldState, NpcManager), String> {
    // Prefer the game mod; fall back to legacy parish.json / world.json.
    let (world, npcs_path) = if let Some(gm) = game_mod {
        let world = crate::game_mod::world_state_from_mod(gm)
            .map_err(|e| format!("Failed to load world from mod: {}", e))?;
        (world, gm.npcs_path())
    } else {
        let parish = data_dir.join("parish.json");
        let world_path = if parish.exists() {
            parish
        } else {
            data_dir.join("world.json")
        };
        let world = WorldState::from_parish_file(&world_path, DEFAULT_START_LOCATION)
            .map_err(|e| format!("Failed to load world data from {:?}: {}", world_path, e))?;
        (world, data_dir.join("npcs.json"))
    };

    let npc_manager = NpcManager::load_from_file(&npcs_path).unwrap_or_else(|e| {
        tracing::warn!(
            path = %npcs_path.display(),
            error = %e,
            "load_fresh_world_and_npcs: failed to load NPCs; starting with empty manager",
        );
        NpcManager::new()
    });

    Ok((world, npc_manager))
}

// ── do_new_game ───────────────────────────────────────────────────────────────

/// Parameters for [`do_new_game`].
///
/// Bundles the Mutex-wrapped AppState fields and metadata needed by the
/// shared new-game orchestration.  Each runtime constructs this by borrowing
/// its `AppState` fields.
pub struct NewGameParams<'a> {
    /// Game world (Mutex-wrapped, replaced with fresh state).
    pub world: &'a Mutex<WorldState>,
    /// NPC manager (Mutex-wrapped, replaced with fresh state).
    pub npc_manager: &'a Mutex<NpcManager>,
    /// Conversation transcript (Mutex-wrapped, reset to default).
    pub conversation: &'a Mutex<ConversationRuntimeState>,
    /// Active save-file path (Mutex-wrapped, updated with new file path).
    pub save_path: &'a Mutex<Option<PathBuf>>,
    /// Active branch id (Mutex-wrapped, updated after save).
    pub current_branch_id: &'a Mutex<Option<i64>>,
    /// Active branch name (Mutex-wrapped, updated after save).
    pub current_branch_name: &'a Mutex<Option<String>>,
    /// Resolved saves directory (used to create a new save file).
    pub saves_dir: &'a Path,
    /// Active game mod, if any (used by `load_fresh_world_and_npcs`).
    pub game_mod: Option<&'a GameMod>,
    /// Legacy data directory fallback.
    pub data_dir: &'a Path,
    /// Pronunciation hints used to populate the world-update snapshot.
    pub pronunciations: &'a [PronunciationEntry],
    /// Default transport mode (used to populate the world-update snapshot).
    pub default_transport: &'a TransportMode,
    /// Backend-specific event emitter for the world-update event.
    pub emitter: &'a dyn EventEmitter,
}

/// Shared new-game implementation used by the Axum server and Tauri desktop.
///
/// Reloads world and NPCs from the active game mod (or legacy data files),
/// resets conversation state, captures an initial snapshot, and persists it
/// via [`SessionStore`].  Emits a `world-update` event via the supplied
/// [`EventEmitter`].
///
/// # CLI note
///
/// The headless CLI uses its own `handle_headless_new_game` because it
/// creates a new branch on an existing `AsyncDatabase` (rather than creating
/// a fresh save file) and calls print helpers (`print_location_arrival`,
/// `print_arrival_reactions`) that are not part of the `EventEmitter` surface.
pub async fn do_new_game(p: NewGameParams<'_>) -> Result<(), String> {
    // Load fresh world and NPCs.
    let (fresh_world, mut fresh_npcs) = load_fresh_world_and_npcs(p.game_mod, p.data_dir)?;
    fresh_npcs.assign_tiers(&fresh_world, &[]);

    // Swap live state — hold both locks together to prevent a window where a
    // handler sees the new world with the old NPC manager (#696).
    let snapshot = {
        let mut world = p.world.lock().await;
        let mut npc_manager = p.npc_manager.lock().await;
        *world = fresh_world;
        *npc_manager = fresh_npcs;
        GameSnapshot::capture(&world, &npc_manager)
    };

    // Reset conversation transcript so stale dialogue from the previous game
    // does not bleed into NPC conversations in the new game (#281).
    {
        let mut conv = p.conversation.lock().await;
        *conv = ConversationRuntimeState::new();
    }

    // Create a new save file and persist the initial snapshot.
    //
    // NOTE: We use `Database::open` directly (not `session_store.save_snapshot`)
    // because `new_save_path` creates a DIFFERENT file from the one `DbSessionStore`
    // would find via `first_db_path` (which returns the alphabetically-first existing
    // `.db` file).  Routing through SessionStore here would write the snapshot to the
    // PREVIOUS save file, corrupting it.  The `session_store` field is wired in for
    // future use by load/branch/journal paths; the new-game file-creation step remains
    // a direct `Database::open` call.
    let new_path = new_save_path(p.saves_dir);
    let new_path_clone = new_path.clone();
    let branch_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&new_path_clone).map_err(|e| e.to_string())?;
        let branch = db
            .find_branch("main")
            .map_err(|e| e.to_string())?
            .ok_or("Failed to find main branch in new save")?;
        db.save_snapshot(branch.id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(branch.id)
    })
    .await
    .map_err(|e| e.to_string())??;

    // Update save state slots.
    *p.save_path.lock().await = Some(new_path);
    *p.current_branch_id.lock().await = Some(branch_id);
    *p.current_branch_name.lock().await = Some("main".to_string());

    // Emit world-update so the frontend reflects the reset state.
    {
        let world = p.world.lock().await;
        let npc_manager = p.npc_manager.lock().await;
        let mut ws = snapshot_from_world(&world, p.default_transport);
        ws.name_hints = compute_name_hints(&world, &npc_manager, p.pronunciations);
        p.emitter.emit_event(
            "world-update",
            serde_json::to_value(&ws).unwrap_or(serde_json::Value::Null),
        );
    }

    Ok(())
}

// ── do_save_game ──────────────────────────────────────────────────────────────

/// Shared save-game implementation for Axum server and Tauri desktop.
///
/// Captures a snapshot of current game state and persists it to the active
/// save file.  If no save file exists yet, creates a new one in `saves_dir`.
///
/// Returns a human-readable success message.
///
/// # CLI note
///
/// The headless CLI retains its own inline `do_autosave_if_needed` because it
/// uses `AsyncDatabase` directly (via `app.db`) rather than going through the
/// `SessionStore` trait.
pub async fn do_save_game(
    world: &Mutex<WorldState>,
    npc_manager: &Mutex<NpcManager>,
    save_path: &Mutex<Option<PathBuf>>,
    current_branch_id: &Mutex<Option<i64>>,
    current_branch_name: &Mutex<Option<String>>,
    saves_dir: &Path,
) -> Result<String, String> {
    let snapshot = {
        let world = world.lock().await;
        let npc_manager = npc_manager.lock().await;
        GameSnapshot::capture(&world, &npc_manager)
    };

    let mut save_path_guard = save_path.lock().await;
    let mut branch_id_guard = current_branch_id.lock().await;
    let mut branch_name_guard = current_branch_name.lock().await;

    let db_path = if let Some(ref path) = *save_path_guard {
        path.clone()
    } else {
        let path = new_save_path(saves_dir);
        *save_path_guard = Some(path.clone());
        path
    };

    let existing_branch_id = *branch_id_guard;
    let (resolved_branch_id, resolved_branch_name) =
        tokio::task::spawn_blocking(move || -> Result<(i64, String), String> {
            let db = Database::open(&db_path).map_err(|e| e.to_string())?;
            let branch_id = if let Some(id) = existing_branch_id {
                id
            } else {
                let branch = db.find_branch("main").map_err(|e| e.to_string())?;
                branch.map(|b| b.id).unwrap_or(1)
            };
            db.save_snapshot(branch_id, &snapshot)
                .map_err(|e| e.to_string())?;
            Ok((branch_id, "main".to_string()))
        })
        .await
        .map_err(|e| e.to_string())??;

    if branch_id_guard.is_none() {
        *branch_id_guard = Some(resolved_branch_id);
        *branch_name_guard = Some(resolved_branch_name.clone());
    }

    let filename = save_path_guard
        .as_ref()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "save".to_string());
    let branch_name = branch_name_guard.as_deref().unwrap_or("main");
    Ok(format!(
        "Game saved to {} (branch: {}).",
        filename, branch_name
    ))
}
