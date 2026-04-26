//! Axum route handlers for the Parish Designer editor.
//!
//! Each route mirrors a Tauri editor command in `parish-tauri/src/editor_commands.rs`
//! by calling the shared handlers in `parish_core::ipc::editor`. Mode parity is
//! maintained: the same editor works identically in Tauri and web modes.
//!
//! # Security fixes
//! - **#371** — All client-supplied paths are canonicalised and checked to live
//!   under `mods_root()` / `saves_root()` before any I/O.
//! - **#372** — One [`EditorSession`] per CF-Access email; keyed off the
//!   [`AuthContext`] injected by `cf_access_guard`.
//! - **#374/#375** — `tokio::sync::Mutex` throughout; blocking fs / SQLite I/O
//!   is wrapped in `tokio::task::spawn_blocking`.
//! - **#376** — `DefaultBodyLimit` on the editor router group + per-field
//!   validation caps enforced before updating in-memory state.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Extension;
use axum::Json;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use parish_core::editor::save_inspect::{
    BranchSummary, SaveFileSummary, SnapshotDetail, SnapshotSummary,
};
use parish_core::editor::types::{EditorDoc, EditorModSnapshot, ModSummary, ValidationReport};
use parish_core::ipc::editor::{self, EditorSaveResponse, EditorSession};

use crate::cf_auth::AuthContext;
use crate::state::AppState;

/// Per-field validation caps (issue #376).
/// Limits are in Unicode characters, not bytes (#481).
const NPC_NAME_MAX: usize = 80;
const NPC_BIO_MAX: usize = 4096;
const NPC_PERSONALITY_MAX: usize = 2048;
const NPC_RELATIONSHIPS_MAX: usize = 256;
const NPCS_PER_FILE_MAX: usize = 2000;
const LOCATION_DESCRIPTION_MAX: usize = 4096;
const LOCATIONS_PER_FILE_MAX: usize = 5000;

/// Returns `true` if the string contains control characters (U+0000–U+001F, U+007F)
/// that should not appear in user-facing text fields (#463).
fn contains_control_chars(s: &str) -> bool {
    s.chars()
        .any(|c| c.is_ascii_control() && c != '\n' && c != '\r' && c != '\t')
}

// ── Helper: extract auth email from request extensions ───────────────────────

/// Extracts the CF-Access email from the request, returning 401 if absent.
fn require_email(auth: Option<Extension<AuthContext>>) -> Result<String, (StatusCode, String)> {
    auth.map(|Extension(ctx)| ctx.email)
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "missing auth context".to_string()))
}

// ── Helper: mods root ─────────────────────────────────────────────────────────

/// Returns the canonical absolute path of the project's `mods/` directory.
fn mods_root(state: &AppState) -> PathBuf {
    if let Some(ref gm) = state.game_mod
        && let Some(parent) = gm.mod_dir.parent()
    {
        return parent.to_path_buf();
    }
    parish_core::game_mod::find_default_mod()
        .and_then(|p| p.parent().map(|pp| pp.to_path_buf()))
        .unwrap_or_else(|| {
            let fallback = PathBuf::from("mods");
            tracing::warn!(
                path = %fallback.display(),
                "Could not find mods directory from game mod or workspace — falling back to \
                 relative path. The editor may list no mods on packaged builds."
            );
            fallback
        })
}

// ── Route handlers ────────────────────────────────────────────────────────────

/// `GET /api/editor-list-mods`
pub async fn editor_list_mods(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<Vec<ModSummary>>, (StatusCode, String)> {
    let root = mods_root(&state);
    tokio::task::spawn_blocking(move || {
        editor::handle_editor_list_mods(&root).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
}

/// `POST /api/editor-open-mod` with JSON body `{ "modPath": "..." }`
pub async fn editor_open_mod(
    Extension(state): Extension<Arc<AppState>>,
    auth: Option<Extension<AuthContext>>,
    Json(body): Json<EditorOpenModBody>,
) -> Result<Json<EditorModSnapshot>, (StatusCode, String)> {
    let email = require_email(auth)?;
    let path = PathBuf::from(&body.mod_path);
    let root = mods_root(&state);

    // Canonicalise + containment check (fix #371).
    let canonical =
        editor::validate_within(&path, &root).map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    // Load the snapshot on a blocking thread (fix #374).
    let canonical2 = canonical.clone();
    let snapshot: EditorModSnapshot = tokio::task::spawn_blocking(move || {
        parish_core::editor::mod_io::load_mod_snapshot(&canonical2)
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;

    // Store into the per-user session (fix #372; tokio Mutex — fix #375).
    // `generation` bumps because this is a snapshot-replacement event
    // (different mod; lineage change). In-flight update_* requests
    // captured a pre-open generation and must reject their writebacks
    // when they re-acquire the lock.
    let mut sessions = state.editor_sessions.lock().await;
    let session = sessions.entry(email).or_insert_with(EditorSession::default);
    session.snapshot = Some(snapshot.clone());
    session.version = session.version.wrapping_add(1);
    session.generation = session.generation.wrapping_add(1);

    Ok(Json(snapshot))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorOpenModBody {
    pub mod_path: String,
}

/// `GET /api/editor-get-snapshot`
pub async fn editor_get_snapshot(
    Extension(state): Extension<Arc<AppState>>,
    auth: Option<Extension<AuthContext>>,
) -> Result<Json<EditorModSnapshot>, (StatusCode, String)> {
    let email = require_email(auth)?;
    let sessions = state.editor_sessions.lock().await;
    let snapshot = sessions
        .get(&email)
        .and_then(|s| s.snapshot.clone())
        .ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "no mod is open in the editor".to_string(),
            )
        })?;
    Ok(Json(snapshot))
}

/// `GET /api/editor-validate`
///
/// Validation iterates every NPC and location and is CPU-bound (up to 2000
/// NPCs × 5000 locations per #376 caps). The snapshot is cloned out of the
/// editor-sessions lock and validated on a blocking thread so the lock does
/// not serialise concurrent editor requests behind this computation (#500).
/// The freshly validated snapshot is written back only when the session
/// version is unchanged — otherwise a concurrent update already bumped the
/// version and the stale clone would clobber it. This is a read-only
/// operation from the client's perspective so there is no 409 path; the
/// update handlers keep their Tauri-parity in-place mutation semantics.
pub async fn editor_validate(
    Extension(state): Extension<Arc<AppState>>,
    auth: Option<Extension<AuthContext>>,
) -> Result<Json<ValidationReport>, (StatusCode, String)> {
    let email = require_email(auth)?;

    let (mut snapshot, captured_version) = {
        let sessions = state.editor_sessions.lock().await;
        let session = sessions.get(&email).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "no mod is open in the editor".to_string(),
            )
        })?;
        let snap = session.snapshot.clone().ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "no mod is open in the editor".to_string(),
            )
        })?;
        (snap, session.version)
    };

    let snapshot = tokio::task::spawn_blocking(move || {
        parish_core::editor::validate::validate_snapshot(&mut snapshot);
        snapshot
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let report = snapshot.validation.clone();

    // Only write the validated snapshot back if no concurrent update bumped
    // the version while we were validating — otherwise we'd clobber newer
    // data with a stale clone. The caller still receives the report we
    // computed; they can re-validate if they want it applied to the current
    // session state.
    {
        let mut sessions = state.editor_sessions.lock().await;
        if let Some(session) = sessions.get_mut(&email)
            && session.version == captured_version
        {
            session.snapshot = Some(snapshot);
        }
    }

    Ok(Json(report))
}

/// `POST /api/editor-update-npcs` with JSON body `{ "npcs": ... }`
///
/// Enforces per-field caps (#376):
/// - NPC name ≤ 80 chars
/// - NPC bio ≤ 4096 chars
/// - NPC personality ≤ 2048 chars
/// - relationships per NPC ≤ 256
/// - NPCs per file ≤ 2000
pub async fn editor_update_npcs(
    Extension(state): Extension<Arc<AppState>>,
    auth: Option<Extension<AuthContext>>,
    Json(body): Json<EditorUpdateNpcsBody>,
) -> Result<Json<ValidationReport>, (StatusCode, String)> {
    let email = require_email(auth)?;
    let npcs: parish_core::npc::NpcFile = serde_json::from_value(body.npcs)
        .map_err(|e| (StatusCode::BAD_REQUEST, format!("invalid NPC data: {e}")))?;

    // Per-field validation caps (fix #376).
    if npcs.npcs.len() > NPCS_PER_FILE_MAX {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "too many NPCs: {} (max {NPCS_PER_FILE_MAX})",
                npcs.npcs.len()
            ),
        ));
    }
    for npc in &npcs.npcs {
        if contains_control_chars(&npc.name) {
            return Err((
                StatusCode::BAD_REQUEST,
                "NPC name contains invalid control characters".to_string(),
            ));
        }
        if npc.name.chars().count() > NPC_NAME_MAX {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "NPC name too long: {} chars (max {NPC_NAME_MAX})",
                    npc.name.chars().count()
                ),
            ));
        }
        if let Some(ref bio) = npc.brief_description {
            if contains_control_chars(bio) {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!(
                        "NPC bio contains invalid control characters for '{}'",
                        npc.name,
                    ),
                ));
            }
            if bio.chars().count() > NPC_BIO_MAX {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!(
                        "NPC bio too long for '{}': {} chars (max {NPC_BIO_MAX})",
                        npc.name,
                        bio.chars().count()
                    ),
                ));
            }
        }
        if contains_control_chars(&npc.personality) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "NPC personality contains invalid control characters for '{}'",
                    npc.name,
                ),
            ));
        }
        if npc.personality.chars().count() > NPC_PERSONALITY_MAX {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "NPC personality too long: {} chars (max {NPC_PERSONALITY_MAX})",
                    npc.personality.chars().count()
                ),
            ));
        }
        if npc.relationships.len() > NPC_RELATIONSHIPS_MAX {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "too many relationships for NPC {}: {} (max {NPC_RELATIONSHIPS_MAX})",
                    npc.name,
                    npc.relationships.len()
                ),
            ));
        }
    }

    // Offload the CPU-bound validate_snapshot to a blocking thread so
    // the editor_sessions lock isn't held across the O(NPCs × locations)
    // work (#464). We clone the current snapshot out under a brief
    // lock, mutate the clone's npcs field + run validate on the clone
    // inside spawn_blocking, then re-acquire the lock and either
    // install wholesale (fast path, no concurrent edit) or overlay the
    // mutated field + re-validate under-lock (slow path, concurrent
    // edit happened during our spawn_blocking window).
    //
    // codex-#574 P1: capture the clone's mod_path and session version
    // so the write-back rejects any request whose session was torn
    // down (editor_close) or re-opened on a different mod
    // (editor_open_mod) while we were validating. Without this guard
    // a stale request could mutate a freshly-opened session.
    //
    // codex-#574 P2: when the session version changed concurrently,
    // the report from our clone doesn't describe the post-merge
    // state. Re-run validate on the merged snapshot under the lock so
    // the returned report matches what's actually stored. This
    // holds the lock across validate on the contended path only; the
    // fast path (no concurrent edit) stays fully offloaded.
    let (snapshot_clone, captured_version, captured_generation, captured_mod_path) = {
        let sessions = state.editor_sessions.lock().await;
        let s = sessions.get(&email).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "no mod is open in the editor".to_string(),
            )
        })?;
        let snap = s.snapshot.clone().ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "no mod is open in the editor".to_string(),
            )
        })?;
        let mod_path = snap.mod_path.clone();
        (snap, s.version, s.generation, mod_path)
    };
    let validated = tokio::task::spawn_blocking(move || {
        let mut s = snapshot_clone;
        s.npcs = npcs;
        parish_core::editor::validate::validate_snapshot(&mut s);
        s
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let validation = {
        let mut sessions = state.editor_sessions.lock().await;
        let session = sessions.get_mut(&email).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "no mod is open in the editor".to_string(),
            )
        })?;
        // codex-#574 P1 (round 2): `generation` bumps only on
        // snapshot-replacement events (open / reload / save / close).
        // If it changed during our spawn_blocking window the snapshot
        // we validated is from a different lineage — overlaying our
        // stale field would silently undo the save/reload. Reject
        // with 409 so the client re-reads and retries. Version-only
        // mismatch (same generation) still goes through the overlay
        // path below — that's just a peer update_locations race.
        if session.generation != captured_generation {
            return Err((
                StatusCode::CONFLICT,
                "editor session was reloaded/saved/reopened during update; retry".to_string(),
            ));
        }
        let snap = session.snapshot.as_mut().ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "no mod is open in the editor".to_string(),
            )
        })?;
        // Defense-in-depth: generation alone should catch mod swaps,
        // but cross-check mod_path too.
        if snap.mod_path != captured_mod_path {
            return Err((
                StatusCode::CONFLICT,
                "editor session was re-opened on a different mod during update; retry".to_string(),
            ));
        }
        if session.version == captured_version {
            // Fast path: no concurrent edit. Install wholesale.
            *snap = validated;
        } else {
            // Peer-update race: overlay our field and re-validate
            // under lock so the stored report describes the merged
            // state (codex-#574 P2).
            snap.npcs = validated.npcs;
            parish_core::editor::validate::validate_snapshot(snap);
        }
        session.version = session.version.wrapping_add(1);
        snap.validation.clone()
    };

    Ok(Json(validation))
}

#[derive(serde::Deserialize)]
pub struct EditorUpdateNpcsBody {
    pub npcs: serde_json::Value,
}

/// `POST /api/editor-update-locations` with JSON body `{ "locations": [...] }`
///
/// Enforces per-field caps (#376):
/// - location description ≤ 4096 chars
/// - locations per file ≤ 5000
pub async fn editor_update_locations(
    Extension(state): Extension<Arc<AppState>>,
    auth: Option<Extension<AuthContext>>,
    Json(body): Json<EditorUpdateLocationsBody>,
) -> Result<Json<ValidationReport>, (StatusCode, String)> {
    let email = require_email(auth)?;
    let locations: Vec<parish_core::world::graph::LocationData> =
        serde_json::from_value(body.locations).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                format!("invalid location data: {e}"),
            )
        })?;

    // Per-field validation caps (fix #376).
    if locations.len() > LOCATIONS_PER_FILE_MAX {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "too many locations: {} (max {LOCATIONS_PER_FILE_MAX})",
                locations.len()
            ),
        ));
    }
    for loc in &locations {
        if contains_control_chars(&loc.description_template) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "location description contains invalid control characters for '{}'",
                    loc.name,
                ),
            ));
        }
        if loc.description_template.chars().count() > LOCATION_DESCRIPTION_MAX {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "location description too long for '{}': {} chars (max {LOCATION_DESCRIPTION_MAX})",
                    loc.name,
                    loc.description_template.chars().count()
                ),
            ));
        }
    }

    // Same clone-validate-install pattern as editor_update_npcs (#464
    // + codex-#574 P1/P2): offload CPU-bound validate to
    // spawn_blocking, guard the write-back against session identity
    // changes (editor_close or editor_open_mod on a different mod),
    // and re-validate under lock on the contended path so the stored
    // report matches the post-merge state.
    let (snapshot_clone, captured_version, captured_generation, captured_mod_path) = {
        let sessions = state.editor_sessions.lock().await;
        let s = sessions.get(&email).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "no mod is open in the editor".to_string(),
            )
        })?;
        let snap = s.snapshot.clone().ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "no mod is open in the editor".to_string(),
            )
        })?;
        let mod_path = snap.mod_path.clone();
        (snap, s.version, s.generation, mod_path)
    };
    let validated = tokio::task::spawn_blocking(move || {
        let mut s = snapshot_clone;
        s.locations = locations;
        parish_core::editor::validate::validate_snapshot(&mut s);
        s
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let validation = {
        let mut sessions = state.editor_sessions.lock().await;
        let session = sessions.get_mut(&email).ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "no mod is open in the editor".to_string(),
            )
        })?;
        // codex-#574 P1 (round 2): see editor_update_npcs for the
        // rationale. generation guards against open/reload/save/close
        // races; version-only mismatch goes through the safe overlay
        // path below.
        if session.generation != captured_generation {
            return Err((
                StatusCode::CONFLICT,
                "editor session was reloaded/saved/reopened during update; retry".to_string(),
            ));
        }
        let snap = session.snapshot.as_mut().ok_or_else(|| {
            (
                StatusCode::BAD_REQUEST,
                "no mod is open in the editor".to_string(),
            )
        })?;
        if snap.mod_path != captured_mod_path {
            return Err((
                StatusCode::CONFLICT,
                "editor session was re-opened on a different mod during update; retry".to_string(),
            ));
        }
        if session.version == captured_version {
            *snap = validated;
        } else {
            snap.locations = validated.locations;
            parish_core::editor::validate::validate_snapshot(snap);
        }
        session.version = session.version.wrapping_add(1);
        snap.validation.clone()
    };

    Ok(Json(validation))
}

#[derive(serde::Deserialize)]
pub struct EditorUpdateLocationsBody {
    pub locations: serde_json::Value,
}

/// `POST /api/editor-save` with JSON body `{ "docs": ["npcs", "world", ...] }`
pub async fn editor_save(
    Extension(state): Extension<Arc<AppState>>,
    auth: Option<Extension<AuthContext>>,
    Json(body): Json<EditorSaveBody>,
) -> Result<Json<EditorSaveResponse>, (StatusCode, String)> {
    let email = require_email(auth)?;
    let docs = body.docs;

    // Clone snapshot out of session so we can do blocking I/O outside the lock.
    // Capture the session version too — on write-back we refuse to clobber a
    // concurrent update that bumped the version in between (codex P2).
    let (snapshot_opt, captured_version) = {
        let sessions = state.editor_sessions.lock().await;
        let s = sessions.get(&email);
        (
            s.and_then(|s| s.snapshot.clone()),
            s.map(|s| s.version).unwrap_or(0),
        )
    };
    let mut snapshot = snapshot_opt.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "no mod is open in the editor".to_string(),
        )
    })?;
    let saved_mod_path = snapshot.mod_path.clone();

    // Blocking I/O outside the Tokio lock (fix #374).
    let docs_for_save = docs.clone();
    let result = tokio::task::spawn_blocking(move || {
        parish_core::editor::persist::save_mod(&mut snapshot, &docs_for_save)
            .map(|r| (r.was_saved(), r.report().clone(), snapshot))
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;

    let (was_saved, report, updated_snapshot) = result;

    // Write the snapshot back only if no other request mutated the session
    // while save_mod was running on the blocking pool. Otherwise we'd clobber
    // a concurrent editor_update_{npcs,locations} with the stale clone.
    {
        let mut sessions = state.editor_sessions.lock().await;
        if let Some(session) = sessions.get_mut(&email) {
            if session.version != captured_version {
                return Err((
                    StatusCode::CONFLICT,
                    "editor session was modified during save; retry".to_string(),
                ));
            }
            session.snapshot = Some(updated_snapshot);
            session.version = session.version.wrapping_add(1);
            // Generation bumps **only** on a successful disk write
            // (codex P1 round 3 on #574). save_mod can return
            // `Blocked` when validation fails — in that case nothing
            // hit disk and the snapshot lineage is unchanged, so
            // an in-flight update_* request that captured the
            // pre-attempt generation should still be allowed to
            // commit. Bumping generation on a blocked save would
            // 409 those requests and silently drop user edits while
            // they are actively trying to fix validation errors.
            if was_saved {
                session.generation = session.generation.wrapping_add(1);
            }
        }
    }

    // If the saved mod is the live game mod and world.json changed, reload it.
    if was_saved
        && docs.contains(&EditorDoc::World)
        && is_active_game_mod(&state, Some(&saved_mod_path))
    {
        reload_live_world_from_disk(&state)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))?;
    }

    Ok(Json(EditorSaveResponse {
        saved: was_saved,
        validation: report,
    }))
}

#[derive(serde::Deserialize)]
pub struct EditorSaveBody {
    pub docs: Vec<EditorDoc>,
}

fn is_active_game_mod(state: &AppState, path: Option<&std::path::Path>) -> bool {
    let Some(path) = path else {
        return false;
    };
    let Ok(path) = path.canonicalize() else {
        return false;
    };
    let Some(active_mod) = state
        .game_mod
        .as_ref()
        .map(|gm| gm.mod_dir.clone())
        .or_else(parish_core::game_mod::find_default_mod)
    else {
        return false;
    };
    let Ok(active_mod) = active_mod.canonicalize() else {
        return false;
    };
    path == active_mod
}

async fn reload_live_world_from_disk(state: &Arc<AppState>) -> Result<(), String> {
    let game_mod = state
        .game_mod
        .clone()
        .or_else(|| {
            parish_core::game_mod::find_default_mod()
                .and_then(|dir| parish_core::game_mod::GameMod::load(&dir).ok())
        })
        .ok_or_else(|| "active game mod not found".to_string())?;

    let snapshot = {
        let mut world = state.world.lock().await;
        parish_core::editor::reload_world_graph_preserving_runtime(&mut world, &game_mod)
            .map_err(|e| format!("failed to reload world graph: {e}"))?;
        let mut npc_manager = state.npc_manager.lock().await;
        // Graph was replaced wholesale — discard the cached BFS distances so
        // the next assign_tiers call recomputes from the new topology.
        npc_manager.invalidate_bfs_cache();
        let transport = state.transport.default_mode();
        let mut ws = parish_core::ipc::snapshot_from_world(&world, transport);
        ws.name_hints =
            parish_core::ipc::compute_name_hints(&world, &npc_manager, &state.pronunciations);
        ws
    };

    state.event_bus.emit("world-update", &snapshot);
    Ok(())
}

/// `POST /api/editor-reload`
pub async fn editor_reload(
    Extension(state): Extension<Arc<AppState>>,
    auth: Option<Extension<AuthContext>>,
) -> Result<Json<EditorModSnapshot>, (StatusCode, String)> {
    let email = require_email(auth)?;

    // Get the mod_path from the session.
    let mod_path = {
        let sessions = state.editor_sessions.lock().await;
        sessions
            .get(&email)
            .and_then(|s| s.snapshot.as_ref().map(|snap| snap.mod_path.clone()))
            .ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    "no mod is open in the editor".to_string(),
                )
            })?
    };

    // Re-load from disk on a blocking thread (fix #374).
    let snapshot: EditorModSnapshot = tokio::task::spawn_blocking(move || {
        parish_core::editor::mod_io::load_mod_snapshot(&mod_path)
            .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))??;

    // Write back into the session. Reload re-reads the mod file from
    // disk and replaces the in-memory snapshot, so this is a
    // lineage-changing event — bump generation so any in-flight
    // update_* reject their writebacks (codex P1 on #574).
    let mut sessions = state.editor_sessions.lock().await;
    let session = sessions.entry(email).or_insert_with(EditorSession::default);
    session.snapshot = Some(snapshot.clone());
    session.version = session.version.wrapping_add(1);
    session.generation = session.generation.wrapping_add(1);

    Ok(Json(snapshot))
}

/// `POST /api/editor-close`
pub async fn editor_close(
    Extension(state): Extension<Arc<AppState>>,
    auth: Option<Extension<AuthContext>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let email = require_email(auth)?;
    let mut sessions = state.editor_sessions.lock().await;
    if let Some(session) = sessions.get_mut(&email) {
        session.snapshot = None;
        session.version = session.version.wrapping_add(1);
        // Close clears the snapshot — in-flight update_* must reject.
        session.generation = session.generation.wrapping_add(1);
    }
    Ok(StatusCode::NO_CONTENT)
}

// ── Save inspector (read-only) ──────────────────────────────────────────────

/// `GET /api/editor-list-saves`
pub async fn editor_list_saves(
    Extension(state): Extension<Arc<AppState>>,
) -> Result<Json<Vec<SaveFileSummary>>, (StatusCode, String)> {
    let saves_dir = state.saves_dir.clone();
    tokio::task::spawn_blocking(move || {
        editor::handle_editor_list_saves(&saves_dir)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePathBody {
    pub save_path: String,
}

/// `POST /api/editor-list-branches`
pub async fn editor_list_branches(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<SavePathBody>,
) -> Result<Json<Vec<BranchSummary>>, (StatusCode, String)> {
    let raw = PathBuf::from(&body.save_path);
    let canonical = editor::validate_within(&raw, state.saves_dir.as_path())
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    tokio::task::spawn_blocking(move || {
        editor::handle_editor_list_branches(&canonical).map_err(|e| (StatusCode::BAD_REQUEST, e))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavePathBranchBody {
    pub save_path: String,
    pub branch_id: i64,
}

/// `POST /api/editor-list-snapshots`
pub async fn editor_list_snapshots(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<SavePathBranchBody>,
) -> Result<Json<Vec<SnapshotSummary>>, (StatusCode, String)> {
    let raw = PathBuf::from(&body.save_path);
    let canonical = editor::validate_within(&raw, state.saves_dir.as_path())
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let branch_id = body.branch_id;
    tokio::task::spawn_blocking(move || {
        editor::handle_editor_list_snapshots(&canonical, branch_id)
            .map_err(|e| (StatusCode::BAD_REQUEST, e))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
}

/// `POST /api/editor-read-snapshot`
pub async fn editor_read_snapshot(
    Extension(state): Extension<Arc<AppState>>,
    Json(body): Json<SavePathBranchBody>,
) -> Result<Json<Option<SnapshotDetail>>, (StatusCode, String)> {
    let raw = PathBuf::from(&body.save_path);
    let canonical = editor::validate_within(&raw, state.saves_dir.as_path())
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    let branch_id = body.branch_id;
    tokio::task::spawn_blocking(move || {
        editor::handle_editor_read_snapshot(&canonical, branch_id)
            .map_err(|e| (StatusCode::BAD_REQUEST, e))
    })
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .map(Json)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::Extension;

    fn make_auth(email: &str) -> Option<Extension<AuthContext>> {
        Some(Extension(AuthContext {
            email: email.to_string(),
        }))
    }

    #[tokio::test]
    async fn editor_open_mod_rejects_path_traversal() {
        let state = crate::routes::tests::test_app_state();
        let body = EditorOpenModBody {
            mod_path: "../../etc/passwd".to_string(),
        };
        let result =
            editor_open_mod(Extension(state), make_auth("test@example.com"), Json(body)).await;
        assert!(result.is_err());
        let (status, _msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn editor_open_mod_rejects_absolute_escape() {
        let state = crate::routes::tests::test_app_state();
        let body = EditorOpenModBody {
            mod_path: "/etc".to_string(),
        };
        let result =
            editor_open_mod(Extension(state), make_auth("test@example.com"), Json(body)).await;
        assert!(result.is_err());
        let (status, _msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn editor_open_mod_rejects_dotdot_path() {
        let state = crate::routes::tests::test_app_state();
        let body = EditorOpenModBody {
            mod_path: "mods/../../etc".to_string(),
        };
        let result =
            editor_open_mod(Extension(state), make_auth("test@example.com"), Json(body)).await;
        assert!(result.is_err());
        let (status, _msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn editor_open_mod_no_auth_returns_401() {
        let state = crate::routes::tests::test_app_state();
        let body = EditorOpenModBody {
            mod_path: "mods/rundale".to_string(),
        };
        let result = editor_open_mod(Extension(state), None, Json(body)).await;
        assert!(result.is_err());
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn editor_list_branches_rejects_path_traversal() {
        let state = crate::routes::tests::test_app_state();
        let body = SavePathBody {
            save_path: "../../etc/passwd".to_string(),
        };
        let result = editor_list_branches(Extension(state), Json(body)).await;
        assert!(result.is_err());
        let (status, _msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn editor_list_snapshots_rejects_path_traversal() {
        let state = crate::routes::tests::test_app_state();
        let body = SavePathBranchBody {
            save_path: "../../etc/passwd".to_string(),
            branch_id: 1,
        };
        let result = editor_list_snapshots(Extension(state), Json(body)).await;
        assert!(result.is_err());
        let (status, _msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn editor_read_snapshot_rejects_path_traversal() {
        let state = crate::routes::tests::test_app_state();
        let body = SavePathBranchBody {
            save_path: "../../etc/passwd".to_string(),
            branch_id: 1,
        };
        let result = editor_read_snapshot(Extension(state), Json(body)).await;
        assert!(result.is_err());
        let (status, _msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn editor_update_npcs_rejects_too_many_npcs() {
        let state = crate::routes::tests::test_app_state();
        // Build a payload with 2001 NPCs.
        let npcs: Vec<serde_json::Value> = (0..2001u32)
            .map(|i| {
                serde_json::json!({
                    "id": i,
                    "name": "Test",
                    "age": 30,
                    "occupation": "Farmer",
                    "personality": "stoic",
                    "home": 1,
                    "workplace": null,
                    "mood": "neutral",
                    "relationships": [],
                    "schedule": []
                })
            })
            .collect();
        let body = EditorUpdateNpcsBody {
            npcs: serde_json::json!({ "npcs": npcs }),
        };
        let result =
            editor_update_npcs(Extension(state), make_auth("user@example.com"), Json(body)).await;
        assert!(result.is_err());
        let (status, msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            msg.contains("too many NPCs"),
            "expected 'too many NPCs', got: {msg}"
        );
    }

    #[tokio::test]
    async fn editor_update_npcs_rejects_long_name() {
        let state = crate::routes::tests::test_app_state();
        let long_name = "A".repeat(NPC_NAME_MAX + 1);
        let npcs = serde_json::json!({
            "npcs": [{
                "id": 1,
                "name": long_name,
                "age": 30,
                "occupation": "Farmer",
                "personality": "stoic",
                "home": 1,
                "workplace": null,
                "mood": "neutral",
                "relationships": [],
                "schedule": []
            }]
        });
        let body = EditorUpdateNpcsBody { npcs };
        let result =
            editor_update_npcs(Extension(state), make_auth("user@example.com"), Json(body)).await;
        assert!(result.is_err());
        let (status, msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            msg.contains("name too long"),
            "expected 'name too long', got: {msg}"
        );
    }

    #[tokio::test]
    async fn editor_update_npcs_accepts_irish_fada_names() {
        let state = crate::routes::tests::test_app_state();
        // 25 Unicode chars, but 27 bytes in UTF-8 (two 2-byte fadas).
        // Must be accepted under the 80-char limit.
        let irish_name = "Pádraig Ó Flaithbheartaigh";
        assert!(irish_name.len() > irish_name.chars().count());
        let npcs = serde_json::json!({
            "npcs": [{
                "id": 1,
                "name": irish_name,
                "age": 30,
                "occupation": "Farmer",
                "personality": "stoic",
                "home": 1,
                "workplace": null,
                "mood": "neutral",
                "relationships": [],
                "schedule": []
            }]
        });
        let body = EditorUpdateNpcsBody { npcs };
        // This should not error — requires an open session to succeed,
        // but the validation runs before the session check, so we expect
        // a session error (BAD_REQUEST "no mod is open"), not a name error.
        let result =
            editor_update_npcs(Extension(state), make_auth("user@example.com"), Json(body)).await;
        match result {
            Ok(_) => {} // fine — session happened to exist
            Err((_, msg)) => {
                assert!(
                    !msg.contains("name too long"),
                    "Irish fada name should not be rejected: {msg}"
                );
            }
        }
    }

    #[tokio::test]
    async fn editor_update_npcs_rejects_null_byte_in_name() {
        let state = crate::routes::tests::test_app_state();
        let npcs = serde_json::json!({
            "npcs": [{
                "id": 1,
                "name": "Test\x00Evil",
                "age": 30,
                "occupation": "Farmer",
                "personality": "stoic",
                "home": 1,
                "workplace": null,
                "mood": "neutral",
                "relationships": [],
                "schedule": []
            }]
        });
        let body = EditorUpdateNpcsBody { npcs };
        let result =
            editor_update_npcs(Extension(state), make_auth("user@example.com"), Json(body)).await;
        assert!(result.is_err());
        let (status, msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            msg.contains("control characters"),
            "expected control character rejection, got: {msg}"
        );
    }

    #[tokio::test]
    async fn editor_update_npcs_rejects_control_chars_in_personality() {
        let state = crate::routes::tests::test_app_state();
        let npcs = serde_json::json!({
            "npcs": [{
                "id": 1,
                "name": "Valid Name",
                "age": 30,
                "occupation": "Farmer",
                "personality": "stoic\x01and\x02quiet",
                "home": 1,
                "workplace": null,
                "mood": "neutral",
                "relationships": [],
                "schedule": []
            }]
        });
        let body = EditorUpdateNpcsBody { npcs };
        let result =
            editor_update_npcs(Extension(state), make_auth("user@example.com"), Json(body)).await;
        assert!(result.is_err());
        let (status, msg) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert!(
            msg.contains("control characters"),
            "expected control character rejection, got: {msg}"
        );
    }

    #[test]
    fn contains_control_chars_permits_whitespace() {
        assert!(!contains_control_chars("hello\nworld"));
        assert!(!contains_control_chars("hello\tworld"));
        assert!(!contains_control_chars("hello\r\nworld"));
        assert!(!contains_control_chars("Pádraig Ó Flaithbheartaigh"));
    }

    #[test]
    fn contains_control_chars_rejects_nulls_and_low_ascii() {
        assert!(contains_control_chars("hello\x00world"));
        assert!(contains_control_chars("hello\x01world"));
        assert!(contains_control_chars("hello\x7Fworld"));
        assert!(contains_control_chars("\x02"));
    }

    #[tokio::test]
    async fn editor_sessions_are_isolated_per_user() {
        let state = crate::routes::tests::test_app_state();

        // Open a session for alice with a bad path (we just want different sessions).
        {
            let mut sessions = state.editor_sessions.lock().await;
            let alice_session = sessions
                .entry("alice@example.com".to_string())
                .or_insert_with(EditorSession::default);
            // Manually mark alice as having a snapshot so we can check bob doesn't see it.
            alice_session.snapshot = Some(EditorModSnapshot {
                mod_path: PathBuf::from("/tmp/alice_mod"),
                manifest: parish_core::editor::types::EditorManifest {
                    id: "alice_mod".to_string(),
                    name: "Alice Mod".to_string(),
                    title: None,
                    version: "0.1.0".to_string(),
                    description: String::new(),
                    start_date: String::new(),
                    start_location: 0,
                    period_year: 1820,
                },
                npcs: parish_core::npc::NpcFile { npcs: vec![] },
                locations: vec![],
                festivals: vec![],
                encounters: parish_core::game_mod::EncounterTable {
                    by_time: Default::default(),
                },
                anachronisms: parish_core::game_mod::AnachronismData {
                    context_alert_prefix: String::new(),
                    context_alert_suffix: String::new(),
                    terms: vec![],
                },
                validation: ValidationReport::default(),
            });
        }

        // Bob should have no session yet.
        let result =
            editor_get_snapshot(Extension(Arc::clone(&state)), make_auth("bob@example.com")).await;
        assert!(result.is_err(), "bob should have no session");
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);

        // Alice should still see her session.
        let result = editor_get_snapshot(
            Extension(Arc::clone(&state)),
            make_auth("alice@example.com"),
        )
        .await;
        assert!(result.is_ok(), "alice should have a session");
        let Json(snap) = result.unwrap();
        assert_eq!(snap.manifest.id, "alice_mod");
    }

    /// Builds a minimal valid snapshot suitable for seeding an editor session
    /// in the concurrency tests below. The single location at id 1 is the
    /// home for any NPCs added in the tests.
    fn seed_snapshot() -> EditorModSnapshot {
        let loc: parish_core::world::graph::LocationData =
            serde_json::from_value(serde_json::json!({
                "id": 1,
                "name": "Home",
                "description_template": "A small cottage.",
                "indoor": true,
                "public": true,
                "connections": []
            }))
            .expect("seed location must deserialize");
        EditorModSnapshot {
            mod_path: PathBuf::from("/tmp/test_mod"),
            manifest: parish_core::editor::types::EditorManifest {
                id: "test_mod".to_string(),
                name: "Test Mod".to_string(),
                title: None,
                version: "0.1.0".to_string(),
                description: String::new(),
                start_date: String::new(),
                start_location: 1,
                period_year: 1820,
            },
            npcs: parish_core::npc::NpcFile { npcs: vec![] },
            locations: vec![loc],
            festivals: vec![],
            encounters: parish_core::game_mod::EncounterTable {
                by_time: Default::default(),
            },
            anachronisms: parish_core::game_mod::AnachronismData {
                context_alert_prefix: String::new(),
                context_alert_suffix: String::new(),
                terms: vec![],
            },
            validation: ValidationReport::default(),
        }
    }

    /// Seeds a session for the given email and returns the version it was
    /// created with so callers can assert version bumps.
    async fn seed_session(state: &Arc<AppState>, email: &str) -> u64 {
        let mut sessions = state.editor_sessions.lock().await;
        let session = sessions
            .entry(email.to_string())
            .or_insert_with(EditorSession::default);
        session.snapshot = Some(seed_snapshot());
        session.version
    }

    #[tokio::test]
    async fn editor_validate_returns_report_without_bumping_version() {
        let state = crate::routes::tests::test_app_state();
        let email = "valid@example.com";
        let start_version = seed_session(&state, email).await;

        let result = editor_validate(Extension(Arc::clone(&state)), make_auth(email)).await;
        assert!(
            result.is_ok(),
            "validate should succeed for a seeded session"
        );
        let Json(report) = result.unwrap();
        let total_issues = report.errors.len() + report.warnings.len();

        // editor_validate is read-only from the caller's perspective: it must
        // not bump the session version (concurrent reads still see the same
        // snapshot generation).
        let sessions = state.editor_sessions.lock().await;
        let session = sessions.get(email).expect("session missing");
        assert_eq!(session.version, start_version);
        // The validated snapshot should have been written back with the same
        // report the caller received.
        let snap = session.snapshot.as_ref().expect("snapshot missing");
        assert_eq!(
            snap.validation.errors.len() + snap.validation.warnings.len(),
            total_issues,
            "validated snapshot should reflect the returned report"
        );
    }

    #[tokio::test]
    async fn editor_validate_no_session_returns_400() {
        let state = crate::routes::tests::test_app_state();
        let result = editor_validate(Extension(state), make_auth("nobody@example.com")).await;
        let (status, _) = result.unwrap_err();
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn editor_update_npcs_applies_update_and_bumps_version() {
        let state = crate::routes::tests::test_app_state();
        let email = "update@example.com";
        let start_version = seed_session(&state, email).await;

        let body = EditorUpdateNpcsBody {
            npcs: serde_json::json!({
                "npcs": [{
                    "id": 42,
                    "name": "Padraig",
                    "age": 30,
                    "occupation": "Farmer",
                    "personality": "stoic",
                    "home": 1,
                    "workplace": null,
                    "mood": "neutral",
                    "relationships": [],
                    "schedule": []
                }]
            }),
        };
        let result =
            editor_update_npcs(Extension(Arc::clone(&state)), make_auth(email), Json(body)).await;
        assert!(result.is_ok(), "update should succeed: {:?}", result.err());

        let sessions = state.editor_sessions.lock().await;
        let session = sessions.get(email).expect("session missing");
        assert_eq!(session.version, start_version.wrapping_add(1));
        let snap = session.snapshot.as_ref().expect("snapshot missing");
        assert_eq!(snap.npcs.npcs.len(), 1);
        assert_eq!(snap.npcs.npcs[0].name, "Padraig");
    }

    #[tokio::test]
    async fn editor_update_locations_applies_update_and_bumps_version() {
        let state = crate::routes::tests::test_app_state();
        let email = "locs@example.com";
        let start_version = seed_session(&state, email).await;

        let body = EditorUpdateLocationsBody {
            locations: serde_json::json!([
                {
                    "id": 1,
                    "name": "Home",
                    "description_template": "A small cottage.",
                    "indoor": true,
                    "public": true,
                    "connections": []
                },
                {
                    "id": 2,
                    "name": "Market",
                    "description_template": "Wares on trestles.",
                    "indoor": false,
                    "public": true,
                    "connections": []
                }
            ]),
        };
        let result =
            editor_update_locations(Extension(Arc::clone(&state)), make_auth(email), Json(body))
                .await;
        assert!(result.is_ok(), "update should succeed: {:?}", result.err());

        let sessions = state.editor_sessions.lock().await;
        let session = sessions.get(email).expect("session missing");
        assert_eq!(session.version, start_version.wrapping_add(1));
        let snap = session.snapshot.as_ref().expect("snapshot missing");
        assert_eq!(snap.locations.len(), 2);
    }
}
