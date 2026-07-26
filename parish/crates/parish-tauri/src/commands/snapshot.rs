//! Read-only world/map/NPC/theme/debug snapshot commands.

use std::sync::Arc;

use parish_core::ipc::compute_name_hints;

use crate::{AppState, MapData, MapLocation, NpcInfo, ThemePalette, WorldSnapshot};

// ── Helper: build a WorldSnapshot from locked world state ────────────────────

/// Builds a [`WorldSnapshot`] from a locked world state reference.
///
/// Used both by the `get_world_snapshot` command and by the background
/// idle-tick task in `lib.rs`. Includes name pronunciation hints when
/// NPC manager and pronunciation data are provided.
pub fn get_world_snapshot_inner(
    world: &parish_core::world::WorldState,
    npc_manager: Option<&parish_core::npc::manager::NpcManager>,
    pronunciations: &[parish_core::game_mod::PronunciationEntry],
) -> WorldSnapshot {
    let mut snapshot = snapshot_from_world(world);
    if let Some(npc_mgr) = npc_manager {
        snapshot.name_hints = compute_name_hints(world, npc_mgr, pronunciations);
    }
    snapshot
}

/// Converts a core [`parish_core::ipc::WorldSnapshot`] into the Tauri-specific
/// [`WorldSnapshot`] (which includes additional fields like `name_hints`).
pub(super) fn snapshot_from_world(world: &parish_core::world::WorldState) -> WorldSnapshot {
    let core = parish_core::ipc::snapshot_from_world(world);
    WorldSnapshot {
        location_id: core.location_id,
        location_name: core.location_name,
        location_description: core.location_description,
        time_label: core.time_label,
        hour: core.hour,
        minute: core.minute,
        weather: core.weather,
        season: core.season,
        festival: core.festival,
        paused: core.paused,
        inference_paused: core.inference_paused,
        game_epoch_ms: core.game_epoch_ms,
        speed_factor: core.speed_factor,
        name_hints: vec![],
        active_tasks: core.active_tasks,
        day_of_week: core.day_of_week,
        turn_in_flight: core.turn_in_flight,
    }
}

pub(super) async fn emit_world_update(state: &Arc<AppState>, app: &tauri::AppHandle) {
    use tauri::Emitter;
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = get_world_snapshot_inner(&world, Some(&npc_manager), &state.pronunciations);
    let _ = app.emit(crate::events::EVENT_WORLD_UPDATE, snapshot);
}

// ── Commands ─────────────────────────────────────────────────────────────────

/// Returns a snapshot of the current world state (location, time, weather, season).
#[tauri::command]
pub async fn get_world_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<WorldSnapshot, String> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let snapshot = get_world_snapshot_inner(&world, Some(&npc_manager), &state.pronunciations);
    Ok(snapshot)
}

/// Returns one source-consistent reconnect replacement payload.
#[tauri::command]
pub async fn get_reconnect_state(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<parish_core::ipc::ReconnectState, String> {
    let _persistence_guard = state.persistence_gate.lock().await;
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    let conversation = state.conversation.lock().await;
    let config = state.config.lock().await;
    Ok(parish_core::ipc::build_reconnect_state(
        &world,
        &npc_manager,
        state.transport.default_mode(),
        config.reveal_unexplored_locations,
        &state.pronunciations,
        conversation.conversation_in_progress,
    ))
}

/// Returns the map data: visited locations with coordinates, edges, and player position.
///
/// Includes visited locations (fully enriched) and the frontier — unvisited
/// locations adjacent to any visited location — so the player can see where
/// to explore next. Frontier locations are marked with `visited: false`.
#[tauri::command]
pub async fn get_map(state: tauri::State<'_, Arc<AppState>>) -> Result<MapData, String> {
    let world = state.world.lock().await;
    let config = state.config.lock().await;
    let transport = state.transport.default_mode();
    let core_map =
        parish_core::ipc::build_map_data(&world, transport, config.reveal_unexplored_locations);

    let player_loc = world.player_location;
    let (player_lat, player_lon) = world
        .graph
        .get(player_loc)
        .map(|data| (data.lat, data.lon))
        .unwrap_or((0.0, 0.0));

    Ok(MapData {
        locations: core_map
            .locations
            .into_iter()
            .map(|l| MapLocation {
                id: l.id,
                name: l.name,
                lat: l.lat,
                lon: l.lon,
                adjacent: l.adjacent,
                hops: l.hops,
                indoor: l.indoor,
                travel_minutes: l.travel_minutes,
                visited: l.visited,
            })
            .collect(),
        edges: core_map.edges,
        player_location: core_map.player_location,
        player_lat,
        player_lon,
        edge_traversals: core_map.edge_traversals,
        transport_label: core_map.transport_label,
        transport_id: core_map.transport_id,
    })
}

/// Returns the list of NPCs currently at the player's location.
#[tauri::command]
pub async fn get_npcs_here(state: tauri::State<'_, Arc<AppState>>) -> Result<Vec<NpcInfo>, String> {
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    Ok(parish_core::ipc::build_npcs_here(&world, &npc_manager))
}

/// Returns the canonical deterministic Parish engine state (#1331).
///
/// Backs the `parish_engine_state` MCP tool and the `/api/engine-state` route.
/// Read-only; gated behind the default-on `engine-state` kill switch (rule #6).
#[tauri::command]
pub async fn get_engine_state(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<parish_core::ipc::EngineState, String> {
    if state.config.lock().await.flags.is_disabled("engine-state") {
        return Err("the engine-state feature is disabled".to_string());
    }
    let world = state.world.lock().await;
    let npc_manager = state.npc_manager.lock().await;
    Ok(parish_core::ipc::build_engine_state(&world, &npc_manager))
}

/// Returns the current palette as CSS hex colours.
///
/// Resolution order:
/// 1. Mod-provided time-of-day keyframes → interpolated palette for the
///    current game hour.
/// 2. Mod-provided static `[theme.palette]` (no keyframes) → returned as-is.
/// 3. No mod loaded → `neutral_grey_palette()` so the prompt overlay renders.
#[tauri::command]
pub async fn get_theme(state: tauri::State<'_, Arc<AppState>>) -> Result<ThemePalette, String> {
    use chrono::Timelike;
    use parish_core::config::PaletteConfig;
    use parish_palette::{compute_palette_with_keyframes, neutral_grey_palette};
    let raw = if !state.theme_keyframes.is_empty() {
        let world = state.world.lock().await;
        let now = world.clock.now();
        compute_palette_with_keyframes(
            now.hour(),
            now.minute(),
            &state.theme_keyframes,
            &PaletteConfig::default(),
        )
    } else if let Some(p) = state.static_raw_palette {
        p
    } else {
        neutral_grey_palette()
    };
    Ok(ThemePalette::from(raw))
}

/// Returns a debug snapshot of all game state for the debug panel.
///
/// Aggregates clock, world graph, NPC state, events, and inference config
/// into a single serializable [`DebugSnapshot`].
#[tauri::command]
pub async fn get_debug_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<parish_core::debug_snapshot::DebugSnapshot, String> {
    Ok(super::admin::build_app_debug_snapshot(&state).await)
}

/// Returns the UI configuration from the loaded game mod.
///
/// The frontend uses this to set sidebar labels, accent colours, etc.
#[tauri::command]
pub async fn get_ui_config(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<crate::UiConfigSnapshot, String> {
    Ok(state.ui_config.clone())
}

/// Returns the latest provider-bootstrap status for the startup overlay.
#[tauri::command]
pub async fn get_setup_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<crate::SetupStatusSnapshot, String> {
    Ok(state
        .setup_status
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone())
}

/// Toggles the desktop window between fullscreen and windowed mode.
///
/// Bound to F11 in the frontend (desktop only — the web build lets the
/// browser handle native F11). Returns the resulting fullscreen state.
#[tauri::command]
pub async fn toggle_fullscreen(window: tauri::WebviewWindow) -> Result<bool, String> {
    let target = !window.is_fullscreen().map_err(|e| e.to_string())?;
    window.set_fullscreen(target).map_err(|e| e.to_string())?;
    Ok(target)
}
