//! Session lifecycle — create, restore, and look up sessions.
//!
//! Owns: [`get_or_create_session`], `create_session`, `restore_session`,
//! `finalize_session_entry`.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use parish_core::game_mod::GameMod;
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

/// Error returned when [`get_or_create_session`] cannot resolve a session.
///
/// `message == None` denotes admission-capacity rejection. `Some(message)`
/// denotes a cold restore/create failure such as an unavailable save lock or a
/// persistence error; middleware deliberately reports those without
/// capacity-specific retry semantics.
#[derive(Debug)]
pub struct CapacityExceededError {
    pub current: usize,
    pub cap: usize,
    /// Non-capacity cold-start failure, still surfaced as 503.
    pub message: Option<String>,
}

// ── Public session resolution ─────────────────────────────────────────────────

/// Returns the session for `cookie_id`, restoring or creating one as needed.
///
/// Returns `Ok((session_id, entry, is_new))` where `is_new` is `true` when a
/// fresh `parish_sid` cookie must be set on the response.
///
/// Returns `Err(CapacityExceededError)` when a cold restore/create fails or
/// when `global.max_concurrent_sessions` is set and a brand-new session would
/// exceed capacity.
pub async fn get_or_create_session(
    global: &Arc<GlobalState>,
    cookie_id: Option<&str>,
) -> Result<(String, Arc<SessionEntry>, bool), CapacityExceededError> {
    let valid_cookie_id = cookie_id.filter(|id| is_valid_session_id(id));
    if cookie_id.is_some() && valid_cookie_id.is_none() {
        tracing::warn!(
            cookie_value = %cookie_id.unwrap_or_default(),
            "get_or_create_session: invalid session ID format, treating as no session"
        );
    }

    // Hot path stays lock-free.
    if let Some(id) = valid_cookie_id
        && let Some(entry) = global.sessions.get_in_memory(id)
    {
        entry
            .last_active
            .store(SessionRegistry::now_unix(), Ordering::Relaxed);
        global.sessions.update_last_active(id);
        return Ok((id.to_string(), entry, false));
    }

    // Serialize only cold restore/create work, then recheck. This guarantees
    // one runtime/tick owner for an evicted cookie and makes the capacity
    // decision atomic with fresh-session insertion.
    let _lifecycle_guard = global.sessions.lifecycle_gate.lock().await;
    if let Some(id) = valid_cookie_id {
        if let Some(entry) = global.sessions.get_in_memory(id) {
            entry
                .last_active
                .store(SessionRegistry::now_unix(), Ordering::Relaxed);
            global.sessions.update_last_active(id);
            return Ok((id.to_string(), entry, false));
        }
        if global.sessions.exists_in_db(id) {
            let entry = restore_session(global, id).await.map_err(|error| {
                tracing::warn!(session_id = %id, %error, "session restore failed closed");
                CapacityExceededError {
                    current: global.sessions.active_count(),
                    cap: global
                        .max_concurrent_sessions
                        .unwrap_or_else(|| global.sessions.active_count()),
                    message: Some(format!("Session unavailable: {error}")),
                }
            })?;
            global.sessions.insert(id.to_string(), Arc::clone(&entry));
            global.sessions.update_last_active(id);
            return Ok((id.to_string(), entry, false));
        }
    }

    // No usable session — admission control check before creating a new one.
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
            return Err(CapacityExceededError {
                current,
                cap,
                message: None,
            });
        }
    }

    // Create a new session. A persistence/lock failure is not registered as a
    // usable session and is returned to the caller.
    let session_id = uuid::Uuid::new_v4().to_string();
    let entry =
        create_session(global, &session_id)
            .await
            .map_err(|error| CapacityExceededError {
                current: global.sessions.active_count(),
                cap: global
                    .max_concurrent_sessions
                    .unwrap_or_else(|| global.sessions.active_count()),
                message: Some(format!("Session unavailable: {error}")),
            })?;
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

#[cfg(test)]
static INSTALLED_PERSISTENT_LOG_WORKERS: std::sync::LazyLock<
    std::sync::Mutex<std::collections::HashSet<PathBuf>>,
> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashSet::new()));

#[cfg(test)]
fn persistent_log_workers_installed(session_saves: &std::path::Path) -> bool {
    INSTALLED_PERSISTENT_LOG_WORKERS
        .lock()
        .unwrap()
        .contains(session_saves)
}

/// Attaches the two persistent log writers after the save/session commit.
///
/// `app_state` is deliberately built with detached handles first. The Arc is
/// still uniquely owned here, so installing the live handles is an infallible
/// publication step and no writer task exists on any earlier failure path.
fn install_persistent_log_workers(
    app_state: &mut Arc<crate::state::AppState>,
    session_saves: &std::path::Path,
    log_to_disk: bool,
    base_url: &str,
) {
    let state = Arc::get_mut(app_state)
        .expect("session AppState must remain uniquely owned before runtime publication");
    let inference_file_log = parish_core::inference::file_log::InferenceFileLog::spawn(
        session_saves,
        log_to_disk,
        Some(base_url),
    );
    let chat_transcript_log = parish_core::chat_transcript::ChatTranscriptLog::spawn_with_flag(
        session_saves,
        inference_file_log.session_id().to_string(),
        inference_file_log.enabled_flag(),
    );
    state.inference_file_log = inference_file_log;
    state.chat_transcript_log = chat_transcript_log;
    #[cfg(test)]
    INSTALLED_PERSISTENT_LOG_WORKERS
        .lock()
        .unwrap()
        .insert(session_saves.to_path_buf());
}

async fn create_session(
    global: &Arc<GlobalState>,
    session_id: &str,
) -> Result<Arc<SessionEntry>, String> {
    let session_saves = global.saves_dir.join(session_id);
    std::fs::create_dir_all(&session_saves).map_err(|error| error.to_string())?;

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
    .map_err(|error| error.to_string())?;

    let (clients, config) = build_session_client(global);
    let client = clients
        .as_ref()
        .map(|clients| clients.dialogue_client().0.clone());
    let cloud_client = build_session_cloud_client(global);
    let game_mod = global.game_mod.clone();

    let flags_path = global.data_dir.join("parish-flags.json");
    let session_store = Arc::new(DbSessionStore::new(global.saves_dir.clone()));

    let log_to_disk = parish_core::inference::file_log::resolve_enabled(
        false,
        global.inference_config.log_to_disk,
    );
    let log_base_url = config.base_url.clone();

    let mut app_state = build_app_state(AppStateParts {
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
        inference_file_log: parish_core::inference::file_log::InferenceFileLog::disabled(),
        chat_transcript_log: parish_core::chat_transcript::ChatTranscriptLog::disabled(),
    });

    init_session_save(&app_state, &session_saves).await?;

    // Register the durable session only after its initial save + active marker
    // committed, but before inference/tick workers are started. A sessions.db
    // failure must never leave an unregistered in-process runtime running.
    global
        .sessions
        .try_persist_new(session_id)
        .map_err(|error| format!("failed to register session: {error}"))?;

    install_persistent_log_workers(&mut app_state, &session_saves, log_to_disk, &log_base_url);
    Ok(finalize_session_entry(app_state, clients).await)
}

/// Shared tail of session entry construction: starts the inference queue
/// (if a client is configured), spawns background ticks, and returns the
/// wrapped [`SessionEntry`].
pub(super) async fn finalize_session_entry(
    app_state: Arc<crate::state::AppState>,
    clients: Option<parish_core::inference::InferenceClients>,
) -> Arc<SessionEntry> {
    if let Some(ref c) = clients {
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

#[derive(Debug)]
struct SessionResumeCandidate {
    db_path: PathBuf,
    remembered_branch: Option<(i64, String)>,
}

fn select_session_resume_candidate(
    session_saves: &std::path::Path,
) -> Result<SessionResumeCandidate, String> {
    match parish_core::persistence::read_active_save_identity_candidate(session_saves) {
        Ok(Some(identity)) => {
            return Ok(SessionResumeCandidate {
                db_path: identity.save_path,
                remembered_branch: Some((identity.branch_id, identity.branch_name)),
            });
        }
        Ok(None) => {}
        Err(error) => {
            return Err(format!(
                "invalid active-save marker in {}: {error}",
                session_saves.display()
            ));
        }
    }

    let mut entries: Vec<(PathBuf, std::time::SystemTime)> = std::fs::read_dir(session_saves)
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
    let db_path = entries
        .into_iter()
        .next()
        .map(|(p, _)| p)
        .ok_or_else(|| "no save files found".to_string())?;
    Ok(SessionResumeCandidate {
        db_path,
        remembered_branch: None,
    })
}

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

    // Prefer the exact save+branch committed by the last successful lifecycle
    // operation. Older installations have no marker, so retain the mtime +
    // first-branch fallback for compatibility.
    let saves_for_scan = session_saves.clone();
    let candidate =
        tokio::task::spawn_blocking(move || select_session_resume_candidate(&saves_for_scan))
            .await
            .map_err(|e| e.to_string())??;
    let db_path = candidate.db_path;

    // Lock the selected path before any SQLite open, migration, branch read,
    // snapshot read, or journal recovery. A locked remembered save is
    // unavailable; never fall through to a different ledger.
    let candidate_lock =
        parish_core::persistence::SaveFileLock::try_acquire(&db_path).ok_or_else(|| {
            format!(
                "save file {} is locked by another Parish instance",
                db_path.display()
            )
        })?;
    let branch_path = db_path.clone();
    let remembered_branch = candidate.remembered_branch.clone();
    let (branch_id, branch_name) =
        tokio::task::spawn_blocking(move || -> Result<(i64, String), String> {
            let db = parish_core::persistence::Database::open(&branch_path)
                .map_err(|e| e.to_string())?;
            let branches = db.list_branches().map_err(|e| e.to_string())?;
            let branch = if let Some((remembered_id, remembered_name)) = remembered_branch {
                branches
                    .into_iter()
                    .find(|branch| branch.id == remembered_id && branch.name == remembered_name)
                    .ok_or_else(|| {
                        format!("remembered branch {remembered_name} ({remembered_id}) is missing")
                    })?
            } else {
                branches.into_iter().next().ok_or("no branches")?
            };
            Ok((branch.id, branch.name))
        })
        .await
        .map_err(|e| e.to_string())??;
    let session_store = Arc::new(DbSessionStore::new(global.saves_dir.clone()));
    let recovery = parish_core::session_store::load_recovery_bundle(
        session_store.as_ref(),
        session_id,
        &db_path,
        branch_id,
    )
    .await
    .map_err(|error| error.to_string())?
    .ok_or_else(|| "no snapshots".to_string())?;

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

    recovery.restore(&mut world, &mut npc_manager);
    npc_manager.assign_tiers(&world, &[]);

    let (clients, config) = build_session_client(global);
    let client = clients
        .as_ref()
        .map(|clients| clients.dialogue_client().0.clone());
    let cloud_client = build_session_cloud_client(global);
    let game_mod: Option<GameMod> = global.game_mod.clone();

    let flags_path = global.data_dir.join("parish-flags.json");
    // Persistent inference + transcript logs for this session. Same
    // session_id is embedded in both filenames so they pair on disk.
    let log_to_disk = parish_core::inference::file_log::resolve_enabled(
        false, // server has no --no-inference-log flag; env var wins
        global.inference_config.log_to_disk,
    );
    let log_base_url = config.base_url.clone();

    let mut app_state = build_app_state(AppStateParts {
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
        inference_file_log: parish_core::inference::file_log::InferenceFileLog::disabled(),
        chat_transcript_log: parish_core::chat_transcript::ChatTranscriptLog::disabled(),
    });

    let prepared_binding = app_state
        .session_store
        .prepare_active_save(session_id, &db_path)
        .map_err(|error| error.to_string())?;
    // A valid marker is already the commit record and is deliberately not
    // rewritten on resume. Legacy selection creates the marker before live
    // publication and fails closed if it cannot be persisted.
    if candidate.remembered_branch.is_none() {
        parish_core::persistence::write_active_save_identity(
            &session_saves,
            &db_path,
            branch_id,
            &branch_name,
        )
        .map_err(|error| error.to_string())?;
    }
    prepared_binding.commit();
    *app_state.save_lock.lock().await = Some(candidate_lock);
    app_state
        .save_identity
        .replace(db_path.clone(), branch_id, branch_name.clone())
        .await;

    install_persistent_log_workers(&mut app_state, &session_saves, log_to_disk, &log_base_url);
    Ok(finalize_session_entry(app_state, clients).await)
}

#[cfg(test)]
mod resume_identity_tests {
    use super::*;
    use std::num::NonZeroUsize;

    use lru::LruCache;
    use parish_core::npc::manager::NpcManager;
    use parish_core::persistence::{Database, GameSnapshot, write_active_save_identity};
    use parish_core::world::WorldState;

    #[test]
    fn exact_marker_beats_legacy_file_and_branch_order() {
        let temp = tempfile::tempdir().unwrap();
        let first_path = temp.path().join("parish_001.db");
        let second_path = temp.path().join("parish_002.db");
        let first_db = Database::open(&first_path).unwrap();
        let first_main = first_db.find_branch("main").unwrap().unwrap();
        let fork_id = first_db
            .create_branch("remembered-fork", Some(first_main.id))
            .unwrap();
        first_db
            .save_snapshot(
                fork_id,
                &GameSnapshot::capture(&WorldState::new(), &NpcManager::new()),
            )
            .unwrap();
        Database::open(&second_path).unwrap();
        write_active_save_identity(temp.path(), &first_path, fork_id, "remembered-fork").unwrap();

        let selected = select_session_resume_candidate(temp.path()).unwrap();
        let (selected_branch, selected_name) = selected.remembered_branch.unwrap();

        assert_eq!(
            std::fs::canonicalize(selected.db_path).unwrap(),
            std::fs::canonicalize(first_path).unwrap()
        );
        assert_eq!(selected_branch, fork_id);
        assert_eq!(selected_name, "remembered-fork");
    }

    #[test]
    fn malformed_present_marker_never_uses_legacy_resume_selection() {
        let temp = tempfile::tempdir().unwrap();
        Database::open(&temp.path().join("parish_001.db")).unwrap();
        std::fs::write(temp.path().join(".active-save.json"), b"{malformed").unwrap();

        let error = select_session_resume_candidate(temp.path())
            .expect_err("a present invalid marker must fail closed");

        assert!(error.contains("invalid active-save marker"));
    }

    fn test_global_state(saves_dir: &std::path::Path) -> Arc<GlobalState> {
        std::fs::create_dir_all(saves_dir).unwrap();
        let sessions = SessionRegistry::open(saves_dir).unwrap();
        let identity_conn = crate::session_store_impl::open_sessions_db(saves_dir).unwrap();
        let identity_store: Arc<dyn parish_core::identity::IdentityStore> = Arc::new(
            crate::session_store_impl::SqliteIdentityStore::new(identity_conn),
        );
        let data_dir =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../mods/rundale");
        let ui_config = crate::state::UiConfigSnapshot {
            hints_label: "test".to_string(),
            default_accent: "#000".to_string(),
            splash_text: String::new(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            auto_pause_timeout_seconds: 300,
            app_icon_url: None,
            favicon_url: None,
            map_overlay: None,
            base_mod_required: false,
        };
        let mut flags = parish_core::config::FeatureFlags::default();
        flags.disable(parish_core::character_log::FEATURE_FLAG);
        flags.disable(parish_core::location_log::FEATURE_FLAG);
        let template_config = crate::state::GameConfig {
            inference_routes_v2: Default::default(),
            inference_subrole_routes_v2: Default::default(),
            inference_configuration_epoch: 0,
            provider_name: String::new(),
            base_url: String::new(),
            api_key: None,
            model_name: String::new(),
            cloud_provider_name: None,
            cloud_model_name: None,
            cloud_api_key: None,
            cloud_base_url: None,
            improv_enabled: false,
            max_follow_up_turns: 2,
            idle_banter_after_secs: 25,
            auto_pause_after_secs: 60,
            category_provider: Default::default(),
            category_model: Default::default(),
            category_api_key: Default::default(),
            category_base_url: Default::default(),
            inference_profile_override: Default::default(),
            category_inference_profile: Default::default(),
            flags,
            category_rate_limit: Default::default(),
            active_tile_source: String::new(),
            tile_sources: Vec::new(),
            reveal_unexplored_locations: false,
            auto_setup_model: None,
        };

        Arc::new(GlobalState {
            inference_runtime_v2: None,
            sessions,
            identity_store,
            oauth_config: None,
            data_dir: data_dir.clone(),
            world_path: data_dir.join("world.json"),
            saves_dir: saves_dir.to_path_buf(),
            game_mod: None,
            pronunciations: Vec::new(),
            ui_config,
            theme_palette: parish_core::game_mod::default_theme_palette(),
            transport: parish_core::world::transport::TransportConfig::default(),
            template_config,
            inference_config: parish_core::config::InferenceConfig::default(),
            runtime_processes: tokio::sync::Mutex::new(
                parish_core::inference::client::RuntimeProcesses::none(),
            ),
            tile_cache: parish_core::tile_cache::TileCache::new(
                saves_dir.join("tile-cache"),
                Default::default(),
            ),
            idempotency_cache: tokio::sync::Mutex::new(LruCache::new(
                NonZeroUsize::new(crate::session::IDEMPOTENCY_CACHE_CAPACITY).unwrap(),
            )),
            max_concurrent_sessions: None,
        })
    }

    fn create_snapshot_db(path: &std::path::Path) -> (i64, String) {
        let db = Database::open(path).unwrap();
        let branch = db.find_branch("main").unwrap().unwrap();
        let mut npc_manager = NpcManager::new();
        let mut npc = parish_core::npc::Npc::new_test_npc();
        npc.id = parish_core::npc::NpcId(42);
        npc.name = "Durable Test Neighbour".to_string();
        npc.occupation = "Weaver".to_string();
        npc_manager.add_npc(npc);
        npc_manager.mark_introduced(parish_core::npc::NpcId(42));
        db.save_snapshot(
            branch.id,
            &GameSnapshot::capture(&WorldState::new(), &npc_manager),
        )
        .unwrap();
        (branch.id, branch.name)
    }

    fn seed_restorable_session(global: &Arc<GlobalState>, session_id: &str) -> std::path::PathBuf {
        let session_saves = global.saves_dir.join(session_id);
        std::fs::create_dir_all(&session_saves).unwrap();
        let save_path = session_saves.join("parish_001.db");
        let (branch_id, branch_name) = create_snapshot_db(&save_path);
        write_active_save_identity(&session_saves, &save_path, branch_id, &branch_name).unwrap();
        global.sessions.try_persist_new(session_id).unwrap();
        save_path
    }

    #[cfg(unix)]
    struct ExternalSaveLock {
        child: std::process::Child,
        lock_path: std::path::PathBuf,
    }

    #[cfg(unix)]
    impl ExternalSaveLock {
        fn acquire(save_path: &std::path::Path) -> Self {
            let child = std::process::Command::new("sleep")
                .arg("60")
                .spawn()
                .expect("spawn external lock owner");
            let lock_path = parish_core::persistence::SaveFileLock::lock_path_for(save_path);
            std::fs::write(&lock_path, child.id().to_string()).unwrap();
            Self { child, lock_path }
        }
    }

    #[cfg(unix)]
    impl Drop for ExternalSaveLock {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_file(&self.lock_path);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn externally_locked_remembered_save_fails_closed_without_fallback_or_write() {
        let temp = tempfile::tempdir().unwrap();
        let global = test_global_state(temp.path());
        let session_id = "aaaaaaaa-aaaa-4aaa-aaaa-aaaaaaaaaaaa";
        let remembered_path = seed_restorable_session(&global, session_id);
        let fallback_path = global.saves_dir.join(session_id).join("parish_002.db");
        create_snapshot_db(&fallback_path);

        let marker_path = global.saves_dir.join(session_id).join(".active-save.json");
        // Make the remembered ledger unreadable. The externally-held lock must
        // still be the observed failure, proving SQLite is not opened first.
        std::fs::write(&remembered_path, b"locked candidate must not be opened").unwrap();
        let remembered_before = std::fs::read(&remembered_path).unwrap();
        let fallback_before = std::fs::read(&fallback_path).unwrap();
        let marker_before = std::fs::read(&marker_path).unwrap();
        let _external_lock = ExternalSaveLock::acquire(&remembered_path);

        let error = match get_or_create_session(&global, Some(session_id)).await {
            Ok(_) => panic!("locked remembered save must not fall back"),
            Err(error) => error,
        };

        assert!(
            error
                .message
                .as_deref()
                .is_some_and(|message| message.contains("locked")),
            "cold restore should report the save lock failure: {error:?}"
        );
        assert!(global.sessions.get_in_memory(session_id).is_none());
        assert_eq!(global.sessions.active_count(), 0);
        assert!(
            !persistent_log_workers_installed(&global.saves_dir.join(session_id)),
            "failed cold restore must not spawn persistent log workers"
        );
        assert_eq!(std::fs::read(&remembered_path).unwrap(), remembered_before);
        assert_eq!(std::fs::read(&fallback_path).unwrap(), fallback_before);
        assert_eq!(std::fs::read(&marker_path).unwrap(), marker_before);
    }

    #[tokio::test]
    async fn concurrent_same_cookie_cold_restores_share_one_runtime_owner() {
        let temp = tempfile::tempdir().unwrap();
        let global = test_global_state(temp.path());
        let session_id = "bbbbbbbb-bbbb-4bbb-bbbb-bbbbbbbbbbbb";
        seed_restorable_session(&global, session_id);

        let first = get_or_create_session(&global, Some(session_id));
        let second = get_or_create_session(&global, Some(session_id));
        let (first, second) = tokio::join!(first, second);
        let (_, first_entry, first_is_new) = first.unwrap();
        let (_, second_entry, second_is_new) = second.unwrap();

        assert!(!first_is_new);
        assert!(!second_is_new);
        assert!(
            Arc::ptr_eq(&first_entry, &second_entry),
            "the cold lifecycle gate must publish one shared SessionEntry"
        );
        assert_eq!(global.sessions.active_count(), 1);
        assert!(
            first_entry
                .app_state
                .npc_manager
                .lock()
                .await
                .is_introduced(parish_core::npc::NpcId(42)),
            "server cold restore must preserve durable identity knowledge"
        );
        assert!(
            !first_entry._tick_handles.is_empty(),
            "the one published runtime owns its background tick set"
        );
        assert!(
            persistent_log_workers_installed(&global.saves_dir.join(session_id)),
            "persistent log workers start only for the committed runtime"
        );

        first_entry._shutdown_token.cancel();
        global.sessions.sessions.remove(session_id);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn fresh_session_lock_failure_registers_no_session_and_starts_no_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let global = test_global_state(temp.path());
        let session_id = "cccccccc-cccc-4ccc-cccc-cccccccccccc";
        let session_saves = global.saves_dir.join(session_id);
        std::fs::create_dir_all(&session_saves).unwrap();
        let save_path = session_saves.join("parish_001.db");
        let _external_lock = ExternalSaveLock::acquire(&save_path);

        let result = create_session(&global, session_id).await;

        assert!(result.is_err());
        assert!(!global.sessions.exists_in_db(session_id));
        assert!(global.sessions.get_in_memory(session_id).is_none());
        assert_eq!(global.sessions.active_count(), 0);
        assert!(!persistent_log_workers_installed(&session_saves));
    }

    #[tokio::test]
    async fn fresh_session_marker_failure_registers_no_session_and_starts_no_runtime() {
        let temp = tempfile::tempdir().unwrap();
        let global = test_global_state(temp.path());
        let session_id = "dddddddd-dddd-4ddd-dddd-dddddddddddd";
        let session_saves = global.saves_dir.join(session_id);
        std::fs::create_dir_all(session_saves.join(".active-save.json")).unwrap();

        let result = create_session(&global, session_id).await;

        assert!(result.is_err(), "marker rename over a directory must fail");
        assert!(!global.sessions.exists_in_db(session_id));
        assert!(global.sessions.get_in_memory(session_id).is_none());
        assert_eq!(global.sessions.active_count(), 0);
        assert!(!persistent_log_workers_installed(&session_saves));
    }

    #[tokio::test]
    async fn session_registry_failure_occurs_before_runtime_publication() {
        let temp = tempfile::tempdir().unwrap();
        let global = test_global_state(temp.path());
        let session_id = "eeeeeeee-eeee-4eee-eeee-eeeeeeeeeeee";
        global
            .sessions
            .db
            .lock()
            .unwrap()
            .execute("DROP TABLE sessions", [])
            .unwrap();

        let result = create_session(&global, session_id).await;

        let error = match result {
            Ok(_) => panic!("unpersistable session must not become live"),
            Err(error) => error,
        };
        assert!(error.contains("failed to register session"));
        assert!(global.sessions.get_in_memory(session_id).is_none());
        assert_eq!(global.sessions.active_count(), 0);
        assert!(
            !persistent_log_workers_installed(&global.saves_dir.join(session_id)),
            "registry failure must occur before any persistent log worker"
        );
    }
}
