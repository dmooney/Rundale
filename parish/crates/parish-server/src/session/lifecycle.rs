//! Session lifecycle — create, restore, and look up sessions.
//!
//! Owns: [`get_or_create_session`], `create_session`, `restore_session`,
//! `finalize_session_entry`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parish_core::game_mod::GameMod;
use parish_core::inference::AnyClient;
use parish_core::npc::manager::NpcManager;
use parish_core::world::{DEFAULT_START_LOCATION, WorldState};

use crate::session_store_impl::DbSessionStore;
use crate::state::{AppStateParts, build_app_state};

use super::{
    GlobalState, SessionEntry,
    inference_setup::{
        build_session_client, build_session_cloud_client, init_inference_queue, init_session_save,
    },
    persistence::{SessionRegistry, is_valid_session_id},
    ticks::spawn_session_ticks,
};

// ── Public error type ─────────────────────────────────────────────────────────

/// Error returned by [`get_or_create_session`] when the server is at capacity.
///
/// The middleware maps this to `503 Service Unavailable` with a
/// `Retry-After: 30` header.  Existing sessions (returning visitors with a
/// valid cookie) are never refused — capacity is only checked when a brand-new
/// session would be created.
#[derive(Debug)]
pub struct CapacityExceededError {
    pub current: usize,
    pub cap: usize,
}

// ── Public session resolution ─────────────────────────────────────────────────

/// Returns the session for `cookie_id`, restoring or creating one as needed.
///
/// Returns `Ok((session_id, entry, is_new))` where `is_new` is `true` when a
/// fresh `parish_sid` cookie must be set on the response.
///
/// Returns `Err(CapacityExceededError)` when `global.max_concurrent_sessions`
/// is set and the server is already at capacity.  Only new-session creation is
/// gated — returning visitors whose session is already in memory or can be
/// restored from the DB are never rejected.
pub async fn get_or_create_session(
    global: &Arc<GlobalState>,
    cookie_id: Option<&str>,
) -> Result<(String, Arc<SessionEntry>, bool), CapacityExceededError> {
    // 1. Hot path: session already in memory.
    //    Reject malformed cookie values before any DB lookup or path join.
    if let Some(id) = cookie_id {
        if !is_valid_session_id(id) {
            tracing::warn!(
                cookie_value = %id,
                "get_or_create_session: invalid session ID format, treating as no session"
            );
            // Fall through to step 3: create a fresh session.
        } else {
            // 1a. Hot path: session already in memory.
            if let Some(entry) = global.sessions.get_in_memory(id) {
                entry
                    .last_active
                    .store(SessionRegistry::now_unix(), Ordering::Relaxed);
                global.sessions.update_last_active(id);
                return Ok((id.to_string(), entry, false));
            }
            // 2. Session known in DB but evicted from memory — restore it.
            if global.sessions.exists_in_db(id) {
                match restore_session(global, id).await {
                    Ok(entry) => {
                        global.sessions.insert(id.to_string(), Arc::clone(&entry));
                        global.sessions.update_last_active(id);
                        return Ok((id.to_string(), entry, false));
                    }
                    Err(e) => {
                        tracing::warn!(session_id = %id, "Session restore failed: {}. Starting fresh.", e);
                    }
                }
            }
        }
    }

    // 3. No usable session — admission control check before creating a new one.
    if let Some(cap) = global.max_concurrent_sessions {
        let current = global.sessions.active_count();
        if global.sessions.is_at_capacity(cap) {
            let rejection_count = global.sessions.rejection_count.load(Ordering::Relaxed);
            tracing::info!(
                current_session_count = current,
                capacity = cap,
                rejection_count,
                "admission-control: session capacity exceeded, rejecting new session"
            );
            return Err(CapacityExceededError { current, cap });
        }
    }

    // 4. Create a new session.
    let session_id = uuid::Uuid::new_v4().to_string();
    let entry = create_session(global, &session_id).await;
    global.sessions.persist_new(&session_id);
    global
        .sessions
        .insert(session_id.clone(), Arc::clone(&entry));
    tracing::info!(
        current_session_count = global.sessions.active_count(),
        capacity = global.max_concurrent_sessions,
        "new session created"
    );
    Ok((session_id, entry, true))
}

// ── Session creation ──────────────────────────────────────────────────────────

async fn create_session(global: &Arc<GlobalState>, session_id: &str) -> Arc<SessionEntry> {
    let session_saves = global.saves_dir.join(session_id);
    std::fs::create_dir_all(&session_saves).ok();

    let world_path = global.world_path.clone();
    let data_dir = global.data_dir.clone();
    let (world, npc_manager) = tokio::task::spawn_blocking(move || {
        let world = WorldState::from_parish_file(&world_path, DEFAULT_START_LOCATION)
            .unwrap_or_else(|e| {
                tracing::warn!("Session init: failed to load world: {}. Using default.", e);
                WorldState::new()
            });
        let mut npc_manager = NpcManager::load_from_file(&data_dir.join("npcs.json"))
            .unwrap_or_else(|e| {
                tracing::warn!("Session init: failed to load npcs.json: {}. No NPCs.", e);
                NpcManager::new()
            });
        npc_manager.assign_tiers(&world, &[]);
        (world, npc_manager)
    })
    .await
    .expect("session init blocking task panicked");

    let (client, config) = build_session_client(global);
    let cloud_client = build_session_cloud_client(global);
    let game_mod = global.game_mod.clone();

    let flags_path = global.data_dir.join("parish-flags.json");
    let session_store = Arc::new(DbSessionStore::new(session_saves.clone()));

    let log_to_disk = parish_core::inference::file_log::resolve_enabled(
        false,
        global.inference_config.log_to_disk,
    );
    let inference_file_log = parish_core::inference::file_log::InferenceFileLog::spawn(
        &session_saves,
        log_to_disk,
        Some(&config.base_url),
    );
    let chat_transcript_log = parish_core::chat_transcript::ChatTranscriptLog::spawn_with_flag(
        &session_saves,
        inference_file_log.session_id().to_string(),
        inference_file_log.enabled_flag(),
    );

    let app_state = build_app_state(AppStateParts {
        session_id: session_id.to_string(),
        world,
        npc_manager,
        client: client.clone(),
        config,
        cloud_client,
        transport: global.transport.clone(),
        ui_config: global.ui_config.clone(),
        theme_palette: global.theme_palette.clone(),
        saves_dir: session_saves.clone(),
        data_dir: global.data_dir.clone(),
        game_mod,
        flags_path,
        inference_config: global.inference_config.clone(), // (#417) propagate TOML-configured timeouts
        session_store,
        inference_file_log,
        chat_transcript_log,
    });

    if let Err(e) = init_session_save(&app_state, &session_saves).await {
        tracing::warn!("Session initial save failed: {}", e);
    }

    finalize_session_entry(app_state, client).await
}

/// Shared tail of session entry construction: starts the inference queue
/// (if a client is configured), spawns background ticks, and returns the
/// wrapped [`SessionEntry`].
pub(super) async fn finalize_session_entry(
    app_state: Arc<crate::state::AppState>,
    client: Option<AnyClient>,
) -> Arc<SessionEntry> {
    if let Some(ref c) = client {
        init_inference_queue(&app_state, c.clone()).await;
    }

    let shutdown_token = tokio_util::sync::CancellationToken::new();
    let handles = spawn_session_ticks(Arc::clone(&app_state), shutdown_token.clone());

    Arc::new(SessionEntry {
        app_state,
        last_active: AtomicU64::new(SessionRegistry::now_unix()),
        _shutdown_token: shutdown_token,
        _tick_handles: handles,
    })
}

// ── Session restoration ───────────────────────────────────────────────────────

async fn restore_session(
    global: &Arc<GlobalState>,
    session_id: &str,
) -> Result<Arc<SessionEntry>, String> {
    let session_saves = global.saves_dir.join(session_id);
    if !session_saves.exists() {
        return Err(format!(
            "saves directory {} does not exist",
            session_saves.display()
        ));
    }

    // Select the most recently modified `.db` file.  In normal play there is
    // only one save per session, but branching can create additional files.
    // Using mtime rather than alphabetical order avoids restoring a stale
    // branch when newer ones exist (#632).
    let saves_for_scan = session_saves.clone();
    let db_path = tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
        let mut entries: Vec<(PathBuf, std::time::SystemTime)> = std::fs::read_dir(&saves_for_scan)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "db"))
            .filter_map(|e| {
                let meta = e.metadata().ok()?;
                let mtime = meta.modified().ok()?;
                Some((e.path(), mtime))
            })
            .collect();
        entries.sort_by_key(|b| std::cmp::Reverse(b.1));
        entries
            .into_iter()
            .next()
            .map(|(p, _)| p)
            .ok_or_else(|| "no save files found".to_string())
    })
    .await
    .map_err(|e| e.to_string())??;

    // Load snapshot from the first branch.
    let db_path_clone = db_path.clone();
    let (snapshot, branch_id, branch_name) = tokio::task::spawn_blocking(move || {
        use parish_core::persistence::Database;
        let db = Database::open(&db_path_clone).map_err(|e| e.to_string())?;
        let branches = db.list_branches().map_err(|e| e.to_string())?;
        let branch = branches.into_iter().next().ok_or("no branches")?;
        let (_, snapshot) = db
            .load_latest_snapshot(branch.id)
            .map_err(|e| e.to_string())?
            .ok_or("no snapshots")?;
        Ok::<_, String>((snapshot, branch.id, branch.name))
    })
    .await
    .map_err(|e| e.to_string())??;

    // Load fresh static world data, then apply the saved snapshot.
    let world_path = global.world_path.clone();
    let data_dir = global.data_dir.clone();
    let (mut world, mut npc_manager) = tokio::task::spawn_blocking(move || {
        let world = WorldState::from_parish_file(&world_path, DEFAULT_START_LOCATION)
            .unwrap_or_else(|_| WorldState::new());
        let npc_manager = NpcManager::load_from_file(&data_dir.join("npcs.json"))
            .unwrap_or_else(|_| NpcManager::new());
        (world, npc_manager)
    })
    .await
    .map_err(|e| e.to_string())?;

    snapshot.restore(&mut world, &mut npc_manager);
    // Gate: clear in-memory introduced set so NPCs must be re-introduced each
    // session (#1396, npc-dialogue-grounding flag, default-on).
    if !global
        .template_config
        .flags
        .is_disabled("npc-dialogue-grounding")
    {
        npc_manager.clear_introduced_for_session();
    }
    npc_manager.assign_tiers(&world, &[]);

    let (client, config) = build_session_client(global);
    let cloud_client = build_session_cloud_client(global);
    let game_mod: Option<GameMod> = global.game_mod.clone();

    let flags_path = global.data_dir.join("parish-flags.json");
    let session_store = Arc::new(DbSessionStore::new(session_saves.clone()));

    // Persistent inference + transcript logs for this session. Same
    // session_id is embedded in both filenames so they pair on disk.
    let log_to_disk = parish_core::inference::file_log::resolve_enabled(
        false, // server has no --no-inference-log flag; env var wins
        global.inference_config.log_to_disk,
    );
    let inference_file_log = parish_core::inference::file_log::InferenceFileLog::spawn(
        &session_saves,
        log_to_disk,
        Some(&config.base_url),
    );
    let chat_transcript_log = parish_core::chat_transcript::ChatTranscriptLog::spawn_with_flag(
        &session_saves,
        inference_file_log.session_id().to_string(),
        inference_file_log.enabled_flag(),
    );

    let app_state = build_app_state(AppStateParts {
        session_id: session_id.to_string(),
        world,
        npc_manager,
        client: client.clone(),
        config,
        cloud_client,
        transport: global.transport.clone(),
        ui_config: global.ui_config.clone(),
        theme_palette: global.theme_palette.clone(),
        saves_dir: session_saves.clone(),
        data_dir: global.data_dir.clone(),
        game_mod,
        flags_path,
        inference_config: global.inference_config.clone(), // (#417) propagate TOML-configured timeouts
        session_store,
        inference_file_log,
        chat_transcript_log,
    });

    if let Some(ref c) = client {
        init_inference_queue(&app_state, c.clone()).await;
    }

    // Acquire advisory lock on the restored save file so another server
    // instance (or a headless CLI) cannot concurrently write to it (#425).
    // If a peer already holds the lock we log a warning and continue:
    // refusing to start would leave the user with no session at all, and
    // per-process ownership makes strict mutual exclusion across
    // containers out of scope for this handler. The lock is stored on
    // AppState.save_lock so it lives for the session's lifetime.
    let locked = parish_core::persistence::SaveFileLock::try_acquire(&db_path);
    if locked.is_none() {
        tracing::warn!(
            path = %db_path.display(),
            session_id = %session_id,
            "SaveFileLock::try_acquire returned None on session resume — save file appears in use by another instance",
        );
    }
    *app_state.save_lock.lock().await = locked;
    *app_state.save_path.lock().await = Some(db_path);
    *app_state.current_branch_id.lock().await = Some(branch_id);
    *app_state.current_branch_name.lock().await = Some(branch_name);

    Ok(finalize_session_entry(app_state, client).await)
}
