//! Per-visitor session management.
//!
//! Each visitor gets an isolated [`AppState`], identified by a UUID stored in
//! a `parish_sid` cookie.  Sessions survive server restarts because they are
//! persisted in `saves/sessions.db` and their game state lives in
//! `saves/<session_id>/parish_NNN.db`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dashmap::DashMap;
use tokio::task::JoinHandle;

use parish_core::game_mod::{GameMod, PronunciationEntry};
use parish_core::inference::{AnyClient, InferenceQueue, spawn_inference_worker};
use parish_core::ipc::{GameConfig, ThemePalette};
use parish_core::npc::manager::NpcManager;
use parish_core::world::transport::TransportConfig;
use parish_core::world::{LocationId, WorldState};

use crate::state::{AppState, UiConfigSnapshot, build_app_state};

// ── Public types ─────────────────────────────────────────────────────────────

/// Google OAuth credentials (optional — feature disabled when absent).
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    /// Public base URL of the server, e.g. `https://yourapp.railway.app`.
    /// Used to construct the OAuth redirect URI.
    pub base_url: String,
}

/// Server-wide state shared by all sessions — one instance per process.
pub struct GlobalState {
    /// All active sessions, backed by `saves/sessions.db`.
    pub sessions: SessionRegistry,
    /// Google OAuth config; `None` disables the login flow.
    pub oauth_config: Option<OAuthConfig>,
    /// Directory containing game data files (`world.json`, `npcs.json`, …).
    pub data_dir: PathBuf,
    /// Resolved path to the world file (`parish.json` or `world.json`).
    pub world_path: PathBuf,
    /// Root saves directory (`saves/`).
    pub saves_dir: PathBuf,
    /// Loaded game mod (for themes, reaction templates, pronunciations).
    pub game_mod: Option<GameMod>,
    /// Pronunciation entries extracted from the game mod.
    pub pronunciations: Vec<PronunciationEntry>,
    /// UI config (splash text, hints label, accent colour).
    pub ui_config: UiConfigSnapshot,
    /// Fixed UI theme palette.
    pub theme_palette: ThemePalette,
    /// Transport mode configuration.
    pub transport: TransportConfig,
    /// Template game config cloned into each new session.
    pub template_config: GameConfig,
    /// Child `ollama serve` process handle (no-op for non-Ollama providers).
    /// Held for the server's lifetime so dropping `GlobalState` stops the
    /// server. Wrapped in a `Mutex` so the struct stays `Sync`.
    pub ollama_process: tokio::sync::Mutex<parish_core::inference::client::OllamaProcess>,
}

/// A single visitor's isolated game session.
pub struct SessionEntry {
    /// The game state for this visitor.
    pub app_state: Arc<AppState>,
    /// Unix timestamp of the last API request from this session.
    pub last_active: AtomicU64,
    /// Background tick task handles — dropped when the session is evicted.
    _tick_handles: Vec<JoinHandle<()>>,
}

// ── SessionRegistry ──────────────────────────────────────────────────────────

/// In-memory session map backed by a SQLite persistence store.
pub struct SessionRegistry {
    sessions: DashMap<String, Arc<SessionEntry>>,
    db: std::sync::Mutex<rusqlite::Connection>,
}

impl SessionRegistry {
    /// Opens (or creates) `saves/sessions.db` and runs schema migrations.
    pub fn open(saves_dir: &Path) -> rusqlite::Result<Self> {
        let db_path = saves_dir.join("sessions.db");
        let conn = rusqlite::Connection::open(&db_path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sessions (
                id           TEXT PRIMARY KEY,
                created_at   TEXT NOT NULL,
                last_active  TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS oauth_accounts (
                provider         TEXT NOT NULL,
                provider_user_id TEXT NOT NULL,
                session_id       TEXT NOT NULL,
                display_name     TEXT NOT NULL DEFAULT '',
                PRIMARY KEY (provider, provider_user_id)
            );",
        )?;
        // Idempotent migration: add display_name to existing DBs that predate this column.
        let _ = conn.execute_batch(
            "ALTER TABLE oauth_accounts ADD COLUMN display_name TEXT NOT NULL DEFAULT ''",
        );
        Ok(Self {
            sessions: DashMap::new(),
            db: std::sync::Mutex::new(conn),
        })
    }

    pub fn now_unix() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn now_iso() -> String {
        chrono::Utc::now().to_rfc3339()
    }

    /// Returns `true` if `session_id` is recorded in sessions.db.
    pub fn exists_in_db(&self, session_id: &str) -> bool {
        let db = self.db.lock().unwrap();
        db.query_row("SELECT 1 FROM sessions WHERE id = ?1", [session_id], |_| {
            Ok(())
        })
        .is_ok()
    }

    /// Inserts a new session row into sessions.db.
    pub fn persist_new(&self, session_id: &str) {
        let now = Self::now_iso();
        let db = self.db.lock().unwrap();
        if let Err(e) = db.execute(
            "INSERT OR IGNORE INTO sessions (id, created_at, last_active) VALUES (?1, ?2, ?2)",
            rusqlite::params![session_id, now],
        ) {
            tracing::warn!(session_id = %session_id, error = %e, "persist_new failed");
        }
    }

    /// Updates the `last_active` timestamp for a session in sessions.db.
    pub fn update_last_active(&self, session_id: &str) {
        let now = Self::now_iso();
        let db = self.db.lock().unwrap();
        if let Err(e) = db.execute(
            "UPDATE sessions SET last_active = ?1 WHERE id = ?2",
            rusqlite::params![now, session_id],
        ) {
            tracing::warn!(session_id = %session_id, error = %e, "update_last_active failed");
        }
    }

    /// Returns the session_id linked to an OAuth identity, if any.
    pub fn find_by_oauth(&self, provider: &str, provider_user_id: &str) -> Option<String> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT session_id FROM oauth_accounts
             WHERE provider = ?1 AND provider_user_id = ?2",
            rusqlite::params![provider, provider_user_id],
            |row| row.get(0),
        )
        .ok()
    }

    /// Associates an OAuth identity with a session_id, storing the user's display name.
    pub fn link_oauth(
        &self,
        provider: &str,
        provider_user_id: &str,
        session_id: &str,
        display_name: &str,
    ) {
        let db = self.db.lock().unwrap();
        match db.execute(
            "INSERT OR REPLACE INTO oauth_accounts
             (provider, provider_user_id, session_id, display_name) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![provider, provider_user_id, session_id, display_name],
        ) {
            Ok(rows) => tracing::info!(
                provider = %provider,
                provider_user_id = %provider_user_id,
                session_id = %session_id,
                display_name = %display_name,
                rows = rows,
                "link_oauth stored account"
            ),
            Err(e) => tracing::error!(
                provider = %provider,
                provider_user_id = %provider_user_id,
                session_id = %session_id,
                error = %e,
                "link_oauth DB write failed"
            ),
        }
    }

    /// Returns a session from the in-memory map.
    pub fn get_in_memory(&self, session_id: &str) -> Option<Arc<SessionEntry>> {
        self.sessions.get(session_id).map(|e| Arc::clone(&*e))
    }

    /// Inserts a session into the in-memory map.
    pub fn insert(&self, session_id: String, entry: Arc<SessionEntry>) {
        self.sessions.insert(session_id, entry);
    }

    /// Returns the Google `(sub, display_name)` linked to `session_id`, if any.
    ///
    /// Used by `GET /api/auth/status` to check whether the session has a
    /// linked Google account and what name to display.
    pub fn google_account_for_session(&self, session_id: &str) -> Option<(String, String)> {
        let db = self.db.lock().unwrap();
        db.query_row(
            "SELECT provider_user_id, display_name FROM oauth_accounts \
             WHERE session_id = ?1 AND provider = 'google'",
            rusqlite::params![session_id],
            |row: &rusqlite::Row<'_>| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .ok()
    }

    /// Removes sessions that have been idle longer than `max_age`.
    ///
    /// The sessions' background tick tasks are implicitly cancelled when
    /// their `JoinHandle`s are dropped via the evicted `SessionEntry`.
    pub fn cleanup_stale(&self, max_age: Duration) {
        let cutoff = Self::now_unix().saturating_sub(max_age.as_secs());
        self.sessions
            .retain(|_, entry| entry.last_active.load(Ordering::Relaxed) >= cutoff);
    }
}

// ── Public session resolution ─────────────────────────────────────────────────

/// Returns the session for `cookie_id`, restoring or creating one as needed.
///
/// Returns `(session_id, entry, is_new)` where `is_new` is `true` when a
/// fresh `parish_sid` cookie must be set on the response.
pub async fn get_or_create_session(
    global: &Arc<GlobalState>,
    cookie_id: Option<&str>,
) -> (String, Arc<SessionEntry>, bool) {
    // 1. Hot path: session already in memory.
    if let Some(id) = cookie_id {
        if let Some(entry) = global.sessions.get_in_memory(id) {
            entry
                .last_active
                .store(SessionRegistry::now_unix(), Ordering::Relaxed);
            global.sessions.update_last_active(id);
            return (id.to_string(), entry, false);
        }
        // 2. Session known in DB but evicted from memory — restore it.
        if global.sessions.exists_in_db(id) {
            match restore_session(global, id).await {
                Ok(entry) => {
                    global.sessions.insert(id.to_string(), Arc::clone(&entry));
                    global.sessions.update_last_active(id);
                    return (id.to_string(), entry, false);
                }
                Err(e) => {
                    tracing::warn!(session_id = %id, "Session restore failed: {}. Starting fresh.", e);
                }
            }
        }
    }

    // 3. No usable session — create a new one.
    let session_id = uuid::Uuid::new_v4().to_string();
    let entry = create_session(global, &session_id).await;
    global.sessions.persist_new(&session_id);
    global
        .sessions
        .insert(session_id.clone(), Arc::clone(&entry));
    (session_id, entry, true)
}

// ── Session creation ──────────────────────────────────────────────────────────

async fn create_session(global: &Arc<GlobalState>, session_id: &str) -> Arc<SessionEntry> {
    let session_saves = global.saves_dir.join(session_id);
    std::fs::create_dir_all(&session_saves).ok();

    let world_path = global.world_path.clone();
    let data_dir = global.data_dir.clone();
    let (world, npc_manager) = tokio::task::spawn_blocking(move || {
        let world = WorldState::from_parish_file(&world_path, LocationId(15)).unwrap_or_else(|e| {
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
    let app_state = build_app_state(
        world,
        npc_manager,
        client.clone(),
        config,
        cloud_client,
        global.transport.clone(),
        global.ui_config.clone(),
        global.theme_palette.clone(),
        session_saves.clone(),
        global.data_dir.clone(),
        game_mod,
        flags_path,
    );

    if let Some(ref c) = client {
        init_inference_queue(&app_state, c.clone()).await;
    }

    if let Err(e) = init_session_save(&app_state, &session_saves).await {
        tracing::warn!("Session initial save failed: {}", e);
    }

    let handles = spawn_session_ticks(Arc::clone(&app_state));

    Arc::new(SessionEntry {
        app_state,
        last_active: AtomicU64::new(SessionRegistry::now_unix()),
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

    // Find the first (alphabetically) .db file.
    let db_path = {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&session_saves)
            .map_err(|e| e.to_string())?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|ext| ext == "db"))
            .collect();
        files.sort();
        files.into_iter().next().ok_or("no save files found")?
    };

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
        let world = WorldState::from_parish_file(&world_path, LocationId(15))
            .unwrap_or_else(|_| WorldState::new());
        let npc_manager = NpcManager::load_from_file(&data_dir.join("npcs.json"))
            .unwrap_or_else(|_| NpcManager::new());
        (world, npc_manager)
    })
    .await
    .map_err(|e| e.to_string())?;

    snapshot.restore(&mut world, &mut npc_manager);
    npc_manager.assign_tiers(&world, &[]);

    let (client, config) = build_session_client(global);
    let cloud_client = build_session_cloud_client(global);
    let game_mod = global.game_mod.clone();

    let flags_path = global.data_dir.join("parish-flags.json");
    let app_state = build_app_state(
        world,
        npc_manager,
        client.clone(),
        config,
        cloud_client,
        global.transport.clone(),
        global.ui_config.clone(),
        global.theme_palette.clone(),
        session_saves.clone(),
        global.data_dir.clone(),
        game_mod,
        flags_path,
    );

    if let Some(ref c) = client {
        init_inference_queue(&app_state, c.clone()).await;
    }

    *app_state.save_path.lock().await = Some(db_path);
    *app_state.current_branch_id.lock().await = Some(branch_id);
    *app_state.current_branch_name.lock().await = Some(branch_name);

    let handles = spawn_session_ticks(Arc::clone(&app_state));

    Ok(Arc::new(SessionEntry {
        app_state,
        last_active: AtomicU64::new(SessionRegistry::now_unix()),
        _tick_handles: handles,
    }))
}

// ── Helpers ───────────────────────────────────────────────────────────────────

async fn init_inference_queue(app_state: &Arc<AppState>, client: AnyClient) {
    let (interactive_tx, interactive_rx) = tokio::sync::mpsc::channel(16);
    let (background_tx, background_rx) = tokio::sync::mpsc::channel(32);
    let (batch_tx, batch_rx) = tokio::sync::mpsc::channel(64);
    let worker = spawn_inference_worker(
        client,
        interactive_rx,
        background_rx,
        batch_rx,
        app_state.inference_log.clone(),
    );
    let queue = InferenceQueue::new(interactive_tx, background_tx, batch_tx);
    *app_state.inference_queue.lock().await = Some(queue);
    *app_state.worker_handle.lock().await = Some(worker);
}

/// Saves the initial world snapshot into `saves/<session_id>/parish_001.db`.
async fn init_session_save(app_state: &Arc<AppState>, session_saves: &Path) -> Result<(), String> {
    use parish_core::persistence::Database;
    use parish_core::persistence::picker::new_save_path;
    use parish_core::persistence::snapshot::GameSnapshot;

    let snapshot = {
        let world = app_state.world.lock().await;
        let npc_manager = app_state.npc_manager.lock().await;
        GameSnapshot::capture(&world, &npc_manager)
    };

    let save_path = new_save_path(session_saves);
    let save_path_clone = save_path.clone();

    let branch_id = tokio::task::spawn_blocking(move || -> Result<i64, String> {
        let db = Database::open(&save_path_clone).map_err(|e| e.to_string())?;
        let branch_id = db.create_branch("main", None).map_err(|e| e.to_string())?;
        db.save_snapshot(branch_id, &snapshot)
            .map_err(|e| e.to_string())?;
        Ok(branch_id)
    })
    .await
    .map_err(|e| e.to_string())??;

    *app_state.save_path.lock().await = Some(save_path);
    *app_state.current_branch_id.lock().await = Some(branch_id);
    *app_state.current_branch_name.lock().await = Some("main".to_string());

    Ok(())
}

/// Spawns the three per-session background tasks and returns their handles.
fn spawn_session_ticks(state: Arc<AppState>) -> Vec<JoinHandle<()>> {
    let mut handles = Vec::with_capacity(3);

    // ── World tick (5 s) ─────────────────────────────────────────────────────
    {
        let s = Arc::clone(&state);
        handles.push(tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;

                {
                    let world = s.world.lock().await;
                    let npc_manager = s.npc_manager.lock().await;
                    let transport = s.transport.default_mode();
                    let mut snap = parish_core::ipc::snapshot_from_world(&world, transport);
                    snap.name_hints = parish_core::ipc::compute_name_hints(
                        &world,
                        &npc_manager,
                        &s.pronunciations,
                    );
                    s.event_bus.emit("world-update", &snap);
                }

                {
                    // Snapshot the banshee flag outside the world/npc locks to avoid
                    // nesting config → world, which inverts the project-wide
                    // lock order.
                    let banshee_enabled = {
                        let cfg = s.config.lock().await;
                        !cfg.flags.is_disabled("banshee")
                    };

                    let mut world = s.world.lock().await;
                    let mut npc_mgr = s.npc_manager.lock().await;

                    let season = world.clock.season();
                    let now = world.clock.now();
                    let mut rng = rand::thread_rng();
                    if let Some(new_weather) = world.weather_engine.tick(now, season, &mut rng) {
                        world.weather = new_weather;
                        world.event_bus.publish(
                            parish_core::world::events::GameEvent::WeatherChanged {
                                new_weather: new_weather.to_string(),
                                timestamp: world.clock.now(),
                            },
                        );
                    }

                    npc_mgr.tick_schedules(&world.clock, &world.graph, world.weather);
                    npc_mgr.assign_tiers(&world, &[]);

                    // Banshee tick — herald and finalise doomed NPCs.
                    if banshee_enabled {
                        let world = &mut *world;
                        let _ = npc_mgr.tick_banshee(
                            &world.clock,
                            &world.graph,
                            &mut world.text_log,
                            &world.event_bus,
                            world.player_location,
                        );
                    }

                    if !world.gossip_network.is_empty() {
                        let groups = npc_mgr.tier2_groups();
                        let mut rng = rand::thread_rng();
                        for npc_ids in groups.values() {
                            if npc_ids.len() >= 2 {
                                parish_core::npc::ticks::propagate_gossip_at_location(
                                    npc_ids,
                                    &mut world.gossip_network,
                                    &mut rng,
                                );
                            }
                        }
                    }
                }
            }
        }));
    }

    // ── Inactivity tick (1 s) ────────────────────────────────────────────────
    {
        let s = Arc::clone(&state);
        handles.push(tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                crate::routes::tick_inactivity(&s).await;
            }
        }));
    }

    // ── Autosave tick (60 s) ─────────────────────────────────────────────────
    {
        let s = Arc::clone(&state);
        handles.push(tokio::spawn(async move {
            use parish_core::persistence::Database;
            use parish_core::persistence::snapshot::GameSnapshot;
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;

                let save_path = s.save_path.lock().await.clone();
                let branch_id = *s.current_branch_id.lock().await;

                if let (Some(path), Some(bid)) = (save_path, branch_id) {
                    let world = s.world.lock().await;
                    let npc_manager = s.npc_manager.lock().await;
                    let snapshot = GameSnapshot::capture(&world, &npc_manager);
                    drop(npc_manager);
                    drop(world);

                    match Database::open(&path) {
                        Ok(db) => match db.save_snapshot(bid, &snapshot) {
                            Ok(_) => tracing::debug!("Session autosave complete"),
                            Err(e) => tracing::warn!("Session autosave failed: {}", e),
                        },
                        Err(e) => tracing::warn!("Session autosave DB open failed: {}", e),
                    }
                }
            }
        }));
    }

    handles
}

fn build_session_client(global: &GlobalState) -> (Option<AnyClient>, GameConfig) {
    let config = global.template_config.clone();
    let client = if config.provider_name == "simulator" {
        Some(AnyClient::simulator())
    } else if config.model_name.is_empty() && config.provider_name != "ollama" {
        None
    } else {
        let provider_enum = parish_core::config::Provider::from_str_loose(&config.provider_name)
            .unwrap_or_default();
        Some(parish_core::inference::build_client(
            &provider_enum,
            &config.base_url,
            config.api_key.as_deref(),
            &parish_core::config::InferenceConfig::default(),
        ))
    };
    (client, config)
}

fn build_session_cloud_client(global: &GlobalState) -> Option<AnyClient> {
    let config = &global.template_config;
    config.cloud_api_key.as_deref().map(|key| {
        let provider_enum = config
            .cloud_provider_name
            .as_deref()
            .and_then(|p| parish_core::config::Provider::from_str_loose(p).ok())
            .unwrap_or(parish_core::config::Provider::OpenRouter);
        parish_core::inference::build_client(
            &provider_enum,
            config
                .cloud_base_url
                .as_deref()
                .unwrap_or("https://openrouter.ai/api"),
            Some(key),
            &parish_core::config::InferenceConfig::default(),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that a fresh DB round-trips the Google OAuth link:
    /// after `link_oauth`, both `find_by_oauth` and
    /// `google_account_for_session` return the stored values.
    ///
    /// This is the exact flow the callback + status endpoint use, so if
    /// this test passes but the UI shows the user as signed out, the bug
    /// is elsewhere (cookies, middleware, frontend).
    #[test]
    fn oauth_link_round_trips_on_fresh_db() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = SessionRegistry::open(tmp.path()).unwrap();
        reg.persist_new("sess_abc");
        reg.link_oauth("google", "sub_123", "sess_abc", "John Doe");

        assert_eq!(
            reg.find_by_oauth("google", "sub_123"),
            Some("sess_abc".to_string()),
            "find_by_oauth should return the linked session_id"
        );
        assert_eq!(
            reg.google_account_for_session("sess_abc"),
            Some(("sub_123".to_string(), "John Doe".to_string())),
            "google_account_for_session should return (sub, display_name)"
        );
    }

    /// Verifies the migration from a pre-display_name schema to the
    /// current schema: opening a DB that was created with the old schema
    /// should add the `display_name` column, and subsequent link_oauth
    /// + google_account_for_session calls should work end-to-end.
    #[test]
    fn oauth_link_round_trips_on_migrated_db() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("sessions.db");

        // Simulate an existing DB created with the pre-display_name schema.
        {
            let conn = rusqlite::Connection::open(&db_path).unwrap();
            conn.execute_batch(
                "CREATE TABLE sessions (
                    id TEXT PRIMARY KEY,
                    created_at TEXT NOT NULL,
                    last_active TEXT NOT NULL
                );
                CREATE TABLE oauth_accounts (
                    provider         TEXT NOT NULL,
                    provider_user_id TEXT NOT NULL,
                    session_id       TEXT NOT NULL,
                    PRIMARY KEY (provider, provider_user_id)
                );",
            )
            .unwrap();
            // Insert a row that predates the display_name column.
            conn.execute(
                "INSERT INTO oauth_accounts (provider, provider_user_id, session_id) \
                 VALUES ('google', 'legacy_sub', 'legacy_sess')",
                [],
            )
            .unwrap();
        }

        // Re-open through SessionRegistry — this should ADD COLUMN display_name.
        let reg = SessionRegistry::open(tmp.path()).unwrap();

        // Legacy row has empty display_name (default).
        assert_eq!(
            reg.google_account_for_session("legacy_sess"),
            Some(("legacy_sub".to_string(), String::new())),
        );

        // New link writes the display_name column correctly.
        reg.persist_new("sess_new");
        reg.link_oauth("google", "sub_new", "sess_new", "Jane Doe");
        assert_eq!(
            reg.google_account_for_session("sess_new"),
            Some(("sub_new".to_string(), "Jane Doe".to_string())),
        );
    }
}
