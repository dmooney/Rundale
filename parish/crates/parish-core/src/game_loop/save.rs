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
//! owns an `AsyncDatabase` directly and calls print
//! helpers that are not part of the shared EventEmitter surface).
//!
//! TD-008: `render_branches_text` / `render_branch_log_text` — pure
//! text-rendering helpers extracted from the near-identical duplicates in
//! `parish-server` and `parish-tauri` (rule #12).
//!
//! # Architecture gate
//!
//! This module is backend-agnostic — it imports only `parish-core` types.
//! It must not import `axum`, `tauri`, or any crate in
//! `FORBIDDEN_FOR_BACKEND_AGNOSTIC`.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use tokio::sync::Mutex;

use crate::game_mod::{GameMod, PronunciationEntry};
use crate::ipc::{
    ConversationRuntimeState, EventEmitter, compute_name_hints,
    emit_game_context_reset_then_world_update, snapshot_from_world,
};
use crate::npc::manager::NpcManager;
use crate::persistence::picker::new_save_path;
use crate::persistence::{Database, GameSnapshot, SaveFileLock, write_active_save_identity};
use crate::session_store::SessionStore;
use crate::world::events::GameEvent;
use crate::world::transport::TransportMode;
use crate::world::{DEFAULT_START_LOCATION, WorldState};

fn path_with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut suffixed = OsString::from(path.as_os_str());
    suffixed.push(suffix);
    PathBuf::from(suffixed)
}

/// Removes every file that SQLite or the advisory-lock layer can create for a
/// save candidate that never reached its active-identity commit record.
///
/// This is deliberately restricted to a newly-generated exact save path. It
/// must never be called for an already-published save because removing that
/// database would destroy canonical state.
fn abort_uncommitted_save_candidate(
    save_path: &Path,
    candidate_lock: &mut Option<SaveFileLock>,
    primary_error: impl Into<String>,
) -> String {
    // Release the advisory guard before removing its sidecar.
    drop(candidate_lock.take());

    let mut cleanup_errors = Vec::new();
    for path in [
        save_path.to_path_buf(),
        path_with_suffix(save_path, "-wal"),
        path_with_suffix(save_path, "-shm"),
        path_with_suffix(save_path, "-journal"),
        SaveFileLock::lock_path_for(save_path),
    ] {
        if let Err(error) = fs::remove_file(&path)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            cleanup_errors.push(format!("{}: {error}", path.display()));
        }
    }

    let primary_error = primary_error.into();
    if cleanup_errors.is_empty() {
        primary_error
    } else {
        format!(
            "{primary_error}; additionally failed to clean uncommitted save candidate: {}",
            cleanup_errors.join(", ")
        )
    }
}

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
    /// Advisory lock for the active save file.
    pub save_lock: &'a Mutex<Option<SaveFileLock>>,
    /// Resolved saves directory (used to create a new save file).
    pub saves_dir: &'a Path,
    /// Persistence backend rebound to the newly-created exact save file.
    pub session_store: &'a dyn SessionStore,
    /// UUID session key on the server; empty in single-user runtimes.
    pub session_id: &'a str,
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
    /// World-event ring buffer (Mutex-wrapped, cleared on new-game so stale
    /// events from the prior game do not bleed into `parish_turn` responses
    /// on the next game (#1395)).
    pub game_events: &'a Mutex<VecDeque<GameEvent>>,
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
/// The headless CLI uses its own `handle_headless_new_game` because it owns
/// an `AsyncDatabase` directly and calls print helpers (`print_location_arrival`,
/// `print_arrival_reactions`) that are not part of the `EventEmitter` surface.
pub async fn do_new_game(p: NewGameParams<'_>) -> Result<(), String> {
    // Load fresh world and NPCs.
    let (mut fresh_world, mut fresh_npcs) = load_fresh_world_and_npcs(p.game_mod, p.data_dir)?;
    fresh_npcs.assign_tiers(&fresh_world, &[]);

    // Persist a complete candidate before touching any live state or identity.
    // A failed open/save/bind therefore leaves the old world, save identity,
    // and advisory lock intact.
    let snapshot = GameSnapshot::capture(&fresh_world, &fresh_npcs);
    //
    // NOTE: We use `Database::open` directly (not `session_store.save_snapshot`)
    // because `new_save_path` creates a DIFFERENT file from the one `DbSessionStore`
    // would find via `first_db_path` (which returns the alphabetically-first existing
    // `.db` file).  Routing through SessionStore here would write the snapshot to the
    // PREVIOUS save file, corrupting it.  The `session_store` field is wired in for
    // future use by load/branch/journal paths; the new-game file-creation step remains
    // a direct `Database::open` call.
    let new_path = new_save_path(p.saves_dir);
    let mut candidate_lock = Some(
        SaveFileLock::try_acquire(&new_path)
            .ok_or_else(|| format!("Could not lock new save file {}", new_path.display()))?,
    );
    let new_path_clone = new_path.clone();
    let branch_id_result = tokio::task::spawn_blocking(move || -> Result<i64, String> {
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
    .map_err(|e| e.to_string())
    .and_then(|result| result);
    let branch_id = match branch_id_result {
        Ok(branch_id) => branch_id,
        Err(error) => {
            return Err(abort_uncommitted_save_candidate(
                &new_path,
                &mut candidate_lock,
                error,
            ));
        }
    };

    let prepared_binding = match p.session_store.prepare_active_save(p.session_id, &new_path) {
        Ok(prepared_binding) => prepared_binding,
        Err(error) => {
            return Err(abort_uncommitted_save_candidate(
                &new_path,
                &mut candidate_lock,
                error.to_string(),
            ));
        }
    };
    if let Err(error) = write_active_save_identity(p.saves_dir, &new_path, branch_id, "main") {
        return Err(abort_uncommitted_save_candidate(
            &new_path,
            &mut candidate_lock,
            error.to_string(),
        ));
    }

    // The marker above is the commit record. Everything below is an
    // infallible in-process publication of the already-durable candidate.
    prepared_binding.commit();

    // Commit the candidate under both canonical locks. The retained event bus
    // advances exactly once so queued prior-context envelopes can be rejected.
    {
        let mut world = p.world.lock().await;
        let mut npc_manager = p.npc_manager.lock().await;
        let retained_event_bus = std::mem::take(&mut world.event_bus);
        retained_event_bus.advance_context_epoch();
        fresh_world.event_bus = retained_event_bus;
        *world = fresh_world;
        *npc_manager = fresh_npcs;
    }

    // Reset conversation and event cursors only after persistence succeeded.
    *p.conversation.lock().await = ConversationRuntimeState::new();
    p.game_events.lock().await.clear();

    // Update the identity triple while holding the save-path guard so readers
    // cannot combine the new file with the previous branch id.
    let mut save_path = p.save_path.lock().await;
    let mut current_branch_id = p.current_branch_id.lock().await;
    let mut current_branch_name = p.current_branch_name.lock().await;
    *save_path = Some(new_path.clone());
    *current_branch_id = Some(branch_id);
    *current_branch_name = Some("main".to_string());
    drop(current_branch_name);
    drop(current_branch_id);
    drop(save_path);
    *p.save_lock.lock().await = candidate_lock.take();

    // Emit world-update so the frontend reflects the reset state.
    {
        let world = p.world.lock().await;
        let npc_manager = p.npc_manager.lock().await;
        let mut ws = snapshot_from_world(&world);
        ws.name_hints = compute_name_hints(&world, &npc_manager, p.pronunciations);
        emit_game_context_reset_then_world_update(
            p.emitter,
            serde_json::to_value(&ws).unwrap_or(serde_json::Value::Null),
        );
    }

    Ok(())
}

// ── do_save_game ──────────────────────────────────────────────────────────────

/// Parameters for [`do_save_game`].
///
/// Groups the canonical world state, save identity, advisory lock, and
/// persistence backend that must participate in one outer save operation.
/// Runtime callers hold their per-session persistence barrier while passing
/// these borrowed fields.
pub struct SaveGameParams<'a> {
    /// Game world captured into the durable snapshot.
    pub world: &'a Mutex<WorldState>,
    /// NPC manager captured into the durable snapshot.
    pub npc_manager: &'a Mutex<NpcManager>,
    /// Active save-file path.
    pub save_path: &'a Mutex<Option<PathBuf>>,
    /// Active branch database id.
    pub current_branch_id: &'a Mutex<Option<i64>>,
    /// Active branch display name.
    pub current_branch_name: &'a Mutex<Option<String>>,
    /// Advisory lock retained for a newly-created save file.
    pub save_lock: &'a Mutex<Option<SaveFileLock>>,
    /// Directory in which new save files and the active-identity marker live.
    pub saves_dir: &'a Path,
    /// Session-scoped persistence backend.
    pub session_store: &'a dyn SessionStore,
    /// Session identity used to bind the persistence backend.
    pub session_id: &'a str,
}

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
pub async fn do_save_game(p: SaveGameParams<'_>) -> Result<String, String> {
    let snapshot = {
        let world = p.world.lock().await;
        let npc_manager = p.npc_manager.lock().await;
        GameSnapshot::capture(&world, &npc_manager)
    };

    let (existing_path, existing_branch_id, existing_branch_name) = {
        let save_path_guard = p.save_path.lock().await;
        let branch_id_guard = p.current_branch_id.lock().await;
        let branch_name_guard = p.current_branch_name.lock().await;
        (
            save_path_guard.clone(),
            *branch_id_guard,
            branch_name_guard.clone(),
        )
    };
    let db_path = existing_path
        .clone()
        .unwrap_or_else(|| new_save_path(p.saves_dir));
    let mut candidate_lock = if existing_path.is_none() {
        Some(
            SaveFileLock::try_acquire(&db_path)
                .ok_or_else(|| format!("Could not lock new save file {}", db_path.display()))?,
        )
    } else {
        None
    };
    let db_path_for_save = db_path.clone();
    let save_result = tokio::task::spawn_blocking(move || -> Result<(i64, String), String> {
        let db = Database::open(&db_path_for_save).map_err(|e| e.to_string())?;
        let branch = if let Some(id) = existing_branch_id {
            db.list_branches()
                .map_err(|e| e.to_string())?
                .into_iter()
                .find(|branch| branch.id == id)
                .ok_or_else(|| format!("Branch id {id} does not exist"))?
        } else {
            db.find_branch("main")
                .map_err(|e| e.to_string())?
                .ok_or_else(|| "main branch missing from save file".to_string())?
        };
        db.save_snapshot(branch.id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok((branch.id, branch.name))
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|result| result);
    let (resolved_branch_id, resolved_branch_name) = match save_result {
        Ok(identity) => identity,
        Err(error) if existing_path.is_none() => {
            return Err(abort_uncommitted_save_candidate(
                &db_path,
                &mut candidate_lock,
                error,
            ));
        }
        Err(error) => return Err(error),
    };

    // Verify the identity observed before persistence is still current, then
    // bind and commit all identity fields. Production callers hold their
    // outer persistence barrier; this check keeps the shared seam defensive.
    let mut save_path_guard = p.save_path.lock().await;
    let mut branch_id_guard = p.current_branch_id.lock().await;
    let mut branch_name_guard = p.current_branch_name.lock().await;
    if *save_path_guard != existing_path
        || *branch_id_guard != existing_branch_id
        || *branch_name_guard != existing_branch_name
    {
        drop(branch_name_guard);
        drop(branch_id_guard);
        drop(save_path_guard);
        let error = "save identity changed while the snapshot was being persisted; retry";
        return Err(if existing_path.is_none() {
            abort_uncommitted_save_candidate(&db_path, &mut candidate_lock, error)
        } else {
            error.to_string()
        });
    }
    let prepared_binding = match p.session_store.prepare_active_save(p.session_id, &db_path) {
        Ok(prepared_binding) => prepared_binding,
        Err(error) => {
            drop(branch_name_guard);
            drop(branch_id_guard);
            drop(save_path_guard);
            return Err(if existing_path.is_none() {
                abort_uncommitted_save_candidate(&db_path, &mut candidate_lock, error.to_string())
            } else {
                error.to_string()
            });
        }
    };
    if let Err(error) = write_active_save_identity(
        p.saves_dir,
        &db_path,
        resolved_branch_id,
        &resolved_branch_name,
    ) {
        drop(branch_name_guard);
        drop(branch_id_guard);
        drop(save_path_guard);
        return Err(if existing_path.is_none() {
            abort_uncommitted_save_candidate(&db_path, &mut candidate_lock, error.to_string())
        } else {
            error.to_string()
        });
    }

    // The marker above is the commit record. Everything below is an
    // infallible in-process publication of the already-durable candidate.
    prepared_binding.commit();
    *save_path_guard = Some(db_path.clone());
    *branch_id_guard = Some(resolved_branch_id);
    *branch_name_guard = Some(resolved_branch_name.clone());

    let filename = db_path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "save".to_string());
    drop(branch_name_guard);
    drop(branch_id_guard);
    drop(save_path_guard);
    if let Some(candidate_lock) = candidate_lock.take() {
        *p.save_lock.lock().await = Some(candidate_lock);
    }
    Ok(format!(
        "Game saved to {} (branch: {}).",
        filename, resolved_branch_name
    ))
}

// ── Text rendering helpers (TD-008, rule #12) ─────────────────────────────────

/// Renders a branch list as a human-readable text block for the `/branches`
/// command.
///
/// Used by both `parish-server` (`do_list_branches_inner`) and `parish-tauri`
/// (`do_list_branches_text`).  Keeping the rendering here ensures a single
/// source of truth for headers, format strings, and the active-branch marker.
///
/// Returns `"No branches found."` when `branches` is empty.
pub fn render_branches_text(
    branches: &[crate::persistence::BranchInfo],
    current_branch_id: Option<i64>,
) -> String {
    if branches.is_empty() {
        return "No branches found.".to_string();
    }
    let mut lines = vec!["Branches:".to_string()];
    for b in branches {
        let marker = if Some(b.id) == current_branch_id {
            " *"
        } else {
            ""
        };
        let parent = b
            .parent_branch_id
            .and_then(|pid| branches.iter().find(|bb| bb.id == pid))
            .map(|bb| format!(" (from {})", bb.name))
            .unwrap_or_default();
        lines.push(format!("  {}{}{}", b.name, parent, marker));
    }
    lines.join("\n")
}

/// Renders a branch snapshot log as a human-readable text block for the `/log`
/// command.
///
/// Used by both `parish-server` (`do_branch_log_inner`) and `parish-tauri`
/// (`do_branch_log_text`).  Timestamps are formatted via
/// [`crate::persistence::format_timestamp`].
///
/// Returns `"No snapshots yet on this branch."` when `log` is empty.
pub fn render_branch_log_text(
    branch_name: &str,
    log: &[crate::persistence::SnapshotInfo],
) -> String {
    if log.is_empty() {
        return "No snapshots yet on this branch.".to_string();
    }
    let mut lines = vec![format!("Save log for branch '{}':", branch_name)];
    for (i, info) in log.iter().enumerate() {
        let time = crate::persistence::format_timestamp(&info.real_time);
        lines.push(format!("  {}. {} (game: {})", i + 1, time, info.game_time));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{BranchInfo, SnapshotInfo};

    fn make_branch(id: i64, name: &str, parent: Option<i64>) -> BranchInfo {
        BranchInfo {
            id,
            name: name.to_string(),
            created_at: "2026-01-01T00:00:00+00:00".to_string(),
            parent_branch_id: parent,
        }
    }

    fn make_snapshot(game_time: &str, real_time: &str) -> SnapshotInfo {
        SnapshotInfo {
            id: 1,
            game_time: game_time.to_string(),
            real_time: real_time.to_string(),
        }
    }

    #[test]
    fn render_branches_empty() {
        assert_eq!(render_branches_text(&[], None), "No branches found.");
    }

    #[test]
    fn render_branches_marks_active() {
        let branches = vec![make_branch(1, "main", None), make_branch(2, "dev", Some(1))];
        let text = render_branches_text(&branches, Some(2));
        assert!(text.contains("main"), "should list main");
        // Active-branch marker appears after the parent annotation: "  dev (from main) *"
        assert!(
            text.contains("dev") && text.contains(" *"),
            "active branch should have marker"
        );
        assert!(text.contains("(from main)"), "dev should show parent");
    }

    #[test]
    fn render_branches_no_marker_when_none_active() {
        let branches = vec![make_branch(1, "main", None)];
        let text = render_branches_text(&branches, None);
        assert!(!text.contains(" *"), "no marker when no active branch");
    }

    #[test]
    fn render_branch_log_empty() {
        assert_eq!(
            render_branch_log_text("main", &[]),
            "No snapshots yet on this branch."
        );
    }

    #[test]
    fn render_branch_log_formats_entries() {
        let log = vec![make_snapshot(
            "1820-03-01T08:00:00+00:00",
            "2026-03-24T16:05:33+00:00",
        )];
        let text = render_branch_log_text("main", &log);
        assert!(
            text.starts_with("Save log for branch 'main':"),
            "should have header"
        );
        assert!(text.contains("  1."), "should have numbered entry");
        assert!(
            text.contains("(game: 1820-03-01T08:00:00+00:00)"),
            "should include game time"
        );
    }
}
