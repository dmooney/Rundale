//! Core application state shared across all UI modes.
//!
//! Contains the [`App`] struct (game state container), used by headless,
//! script, and Tauri modes.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use parish_core::character_log::CharacterLogManager;
use parish_core::session_store::DbSessionStore;
use parish_core::world::events::GameEvent;
use tokio::sync::broadcast;

use crate::config::{InferenceCategory, InferenceConfig};
use crate::inference::AnyClient;
use crate::inference::InferenceQueue;
use crate::loading::LoadingAnimation;
use crate::npc::LanguageSettings;
use crate::npc::manager::NpcManager;
use crate::persistence::AsyncDatabase;
use crate::world::WorldState;
use parish_core::game_mod::GameMod;

/// Maximum number of entries in the debug activity log.
pub const DEBUG_LOG_CAPACITY: usize = 50;

/// Inference categories that have a [`CategoryOverride`] (excludes Dialogue).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NonDialogueCategory {
    Intent,
    Simulation,
    Reaction,
}

impl TryFrom<InferenceCategory> for NonDialogueCategory {
    type Error = &'static str;
    fn try_from(cat: InferenceCategory) -> Result<Self, Self::Error> {
        match cat {
            InferenceCategory::Intent => Ok(Self::Intent),
            InferenceCategory::Simulation => Ok(Self::Simulation),
            InferenceCategory::Reaction => Ok(Self::Reaction),
            InferenceCategory::Dialogue => Err("Dialogue has no CategoryOverride"),
        }
    }
}

/// Per-category overrides for inference clients.
///
/// Replaces the repeated 5-field pattern (client, model, provider_name,
/// api_key, base_url) that used to appear 3 times for intent, simulation,
/// and reaction categories.
#[derive(Clone, Default)]
pub struct CategoryOverride {
    pub client: Option<AnyClient>,
    pub model: String,
    pub provider_name: Option<String>,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

/// Main application state.
///
/// Holds the game world state, input buffer, and control flags.
/// Shared across headless and script modes.
pub struct App {
    /// The game world state.
    pub world: WorldState,
    /// Current text in the input line.
    pub input_buffer: String,
    /// Set to true to exit the main loop.
    pub should_quit: bool,
    /// The inference queue for sending LLM requests (None if unavailable).
    pub inference_queue: Option<InferenceQueue>,
    /// Central NPC manager — owns all NPCs and handles tier assignment.
    pub npc_manager: NpcManager,

    /// Whether improv craft mode is enabled for NPC dialogue.
    pub improv_enabled: bool,
    /// Whether map APIs should reveal all unexplored locations.
    pub reveal_unexplored_locations: bool,
    /// Whether the debug sidebar panel is visible.
    pub debug_sidebar_visible: bool,
    /// Active debug panel tab index (0=Overview, 1=NPCs, 2=World, 3=Events, 4=Inference).
    pub debug_tab: usize,
    /// Selected NPC index in the NPC tab (-1 = none, >=0 = index into sorted list).
    pub debug_selected_npc: Option<usize>,
    /// Scroll offset within the active debug tab.
    pub debug_scroll: u16,
    /// Rolling activity log for the debug panel.
    pub debug_log: VecDeque<String>,
    /// The LLM client for inference requests.
    pub client: Option<AnyClient>,
    /// Current model name.
    pub model_name: String,
    /// Display name of the current provider.
    pub provider_name: String,
    /// Base URL for the current provider.
    pub base_url: String,
    /// API key for the current provider.
    pub api_key: Option<String>,
    /// Cloud provider name for dialogue (None = local only).
    pub cloud_provider_name: Option<String>,
    /// Cloud model name for dialogue.
    pub cloud_model_name: Option<String>,
    /// Cloud client for dialogue inference.
    pub cloud_client: Option<AnyClient>,
    /// Cloud API key.
    pub cloud_api_key: Option<String>,
    /// Cloud base URL.
    pub cloud_base_url: Option<String>,
    /// The model name used by the dialogue inference queue.
    pub dialogue_model: String,
    /// Loading animation state, active while waiting for LLM inference.
    pub loading_animation: Option<LoadingAnimation>,
    /// Async database handle for persistence (None if persistence is disabled).
    pub db: Option<Arc<AsyncDatabase>>,
    /// Saves directory resolved once at startup (#771). Mid-game `/load`
    /// reads this instead of re-probing the cwd.
    pub saves_dir: Option<PathBuf>,
    /// Path to the active save database file.
    pub save_file_path: Option<PathBuf>,
    /// Active save branch id.
    pub active_branch_id: i64,
    /// Most recent snapshot id on the active branch.
    pub latest_snapshot_id: i64,
    /// Wall-clock time of the last autosave.
    pub last_autosave: Option<Instant>,
    /// Intent category override.
    pub intent: CategoryOverride,
    /// Simulation category override.
    pub simulation: CategoryOverride,
    /// Reaction category override.
    pub reaction: CategoryOverride,
    /// Base and per-category native inference tuning resolved at startup.
    pub inference_profile_override: parish_core::config::InferenceProfileOverride,
    pub category_inference_profile: HashMap<
        parish_core::config::InferenceCategory,
        parish_core::config::InferenceProfileOverride,
    >,
    pub inference_routes_v2:
        HashMap<parish_core::config::InferenceCategory, parish_core::config::ResolvedRoute>,
    pub inference_subrole_routes_v2:
        HashMap<parish_core::config::InferenceSubrole, parish_core::config::ResolvedRoute>,
    pub inference_configuration_epoch: u64,
    /// Loaded game mod data (None if no mod directory was found or specified).
    pub game_mod: Option<GameMod>,
    /// Runtime feature flags (loaded from parish-flags.json at startup).
    pub flags: crate::config::FeatureFlags,
    /// Path to the flags persistence file (None disables persistence).
    pub flags_path: Option<PathBuf>,
    /// Advisory file lock for the currently active save file.
    pub save_lock: Option<crate::persistence::SaveFileLock>,
    /// TOML-configured inference timeouts (from `parish.toml`).
    /// Used by rebuild paths so `/provider` switches honour the configured
    /// timeouts instead of falling back to compiled-in defaults. (#417)
    pub inference_config: InferenceConfig,
    /// True when stdin is not a terminal — lock failures are hard errors.
    pub script_mode: bool,
    /// Trait-erased per-session persistence (#696, slice 8).
    ///
    /// CLI is single-user; handlers pass session_id = "" so the store
    /// resolves to the flat `saves/parish_NNN.db` layout.
    pub session_store: std::sync::Arc<dyn parish_core::session_store::SessionStore>,
    /// Per-character markdown log writer (gated by `character-logs` flag).
    pub character_log: Option<Arc<CharacterLogManager>>,
    /// Drains [`GameEvent`]s in the REPL loop into `character_log`. Owned
    /// here rather than spawned on a task because `App` fields aren't
    /// shareable across threads — see `character_log::process_event` in
    /// the REPL loop for the pump.
    pub character_log_rx: Option<broadcast::Receiver<GameEvent>>,
    /// Per-location markdown log writer (gated by `location-logs` flag).
    pub location_log: Option<Arc<parish_core::location_log::LocationLogManager>>,
    /// Drains [`GameEvent`]s into `location_log` on the same cadence as
    /// `character_log_rx`. Independent receiver so both writers see every
    /// event without contention.
    pub location_log_rx: Option<broadcast::Receiver<GameEvent>>,
    /// App-name slug used to resolve the user-data dir for log writers.
    /// Set at startup from the active mod; needed to rebuild log managers
    /// when the active branch changes (#1011).
    pub log_app_name: String,
    /// Branch id the current `character_log` / `location_log` managers
    /// were built against. When `active_branch_id` diverges (e.g. after
    /// `/load <branch>` or `/fork <branch>`), the drain pumps rebuild
    /// the managers so events land under the new branch's log dir.
    pub log_managers_branch: Option<i64>,
    /// Persistent on-disk inference log handle. Defaults to `disabled()`
    /// until `run_headless` constructs a real writer.
    pub inference_file_log: parish_core::inference::file_log::InferenceFileLog,
    /// Persistent on-disk chat transcript log handle. Defaults to `disabled()`
    /// until `run_headless` constructs a real writer.
    pub chat_transcript_log: parish_core::chat_transcript::ChatTranscriptLog,
    /// Drains [`GameEvent`]s into `chat_transcript_log` on the same cadence as
    /// `character_log_rx`. `None` when transcript logging is disabled.
    pub chat_transcript_rx: Option<broadcast::Receiver<GameEvent>>,
}

impl App {
    /// Creates a new App with default world state.
    pub fn new() -> Self {
        Self {
            world: WorldState::new(),
            input_buffer: String::new(),
            should_quit: false,
            inference_queue: None,
            npc_manager: NpcManager::new(),
            improv_enabled: false,
            reveal_unexplored_locations: false,
            debug_sidebar_visible: false,
            debug_tab: 0,
            debug_selected_npc: None,
            debug_scroll: 0,
            debug_log: VecDeque::with_capacity(DEBUG_LOG_CAPACITY),
            client: None,
            model_name: String::new(),
            provider_name: String::from("ollama"),
            base_url: String::new(),
            api_key: None,
            cloud_provider_name: None,
            cloud_model_name: None,
            cloud_client: None,
            cloud_api_key: None,
            cloud_base_url: None,
            dialogue_model: String::new(),
            loading_animation: None,
            db: None,
            saves_dir: None,
            save_file_path: None,
            active_branch_id: 1,
            latest_snapshot_id: 0,
            last_autosave: None,
            intent: CategoryOverride::default(),
            simulation: CategoryOverride::default(),
            reaction: CategoryOverride::default(),
            inference_profile_override: Default::default(),
            category_inference_profile: Default::default(),
            inference_routes_v2: Default::default(),
            inference_subrole_routes_v2: Default::default(),
            inference_configuration_epoch: 0,
            game_mod: None,
            flags: crate::config::FeatureFlags::default(),
            flags_path: None,
            save_lock: None,
            inference_config: InferenceConfig::default(),
            script_mode: false,
            // Placeholder — overwritten by run_headless after
            // resolve_project_saves_dir() resolves the real saves directory
            // under the platform user-data root (#696 slice 8, #771).
            session_store: Arc::new(DbSessionStore::new(PathBuf::from("saves"))),
            character_log: None,
            character_log_rx: None,
            location_log: None,
            location_log_rx: None,
            log_app_name: String::new(),
            log_managers_branch: None,
            // Default to disabled; `run_headless` constructs a real writer
            // once the saves directory and `--no-inference-log` flag are known.
            inference_file_log: parish_core::inference::file_log::InferenceFileLog::disabled(),
            chat_transcript_log: parish_core::chat_transcript::ChatTranscriptLog::disabled(),
            chat_transcript_rx: None,
        }
    }

    /// Captures and saves a world snapshot asynchronously.
    /// Updates latest_snapshot_id and last_autosave on success.
    pub async fn capture_and_save_async(&mut self, branch_id: i64) -> Option<i64> {
        let db = self.db.as_ref()?;
        let snapshot = crate::persistence::GameSnapshot::capture(&self.world, &self.npc_manager);
        match db.save_snapshot(branch_id, &snapshot).await {
            Ok(snap_id) => {
                if let (Some(saves_dir), Some(save_path)) =
                    (self.saves_dir.as_ref(), self.save_file_path.as_ref())
                {
                    let branch_name = match db
                        .list_branches()
                        .await
                        .ok()?
                        .into_iter()
                        .find(|branch| branch.id == branch_id)
                    {
                        Some(branch) => branch.name,
                        None => return None,
                    };
                    if parish_core::persistence::write_active_save_identity(
                        saves_dir,
                        save_path,
                        branch_id,
                        &branch_name,
                    )
                    .is_err()
                    {
                        return None;
                    }
                }
                self.latest_snapshot_id = snap_id;
                self.last_autosave = Some(std::time::Instant::now());
                Some(snap_id)
            }
            Err(_) => None,
        }
    }

    /// Reloads world state and NPCs from the active game mod.
    pub fn reload_mod_world_and_npcs(&mut self) -> Result<(), String> {
        let gm = self
            .game_mod
            .as_ref()
            .ok_or("No game mod loaded.".to_string())?;
        let world = parish_core::game_mod::world_state_from_mod(gm)
            .map_err(|e| format!("Failed to load world: {}", e))?;
        self.world = world;
        let npcs_path = gm.npcs_path();
        if npcs_path.exists() {
            let mgr = NpcManager::load_from_file(&npcs_path)
                .map_err(|e| format!("Failed to load NPCs: {}", e))?;
            self.npc_manager = mgr;
        }
        self.npc_manager.assign_tiers(&self.world, &[]);
        Ok(())
    }

    /// Rebuilds `self.character_log` / `self.location_log` when the active
    /// branch has changed since the managers were last constructed (#1011,
    /// #1034). Without this the writers keep appending to the original
    /// branch's log directory after `/load <branch>` or `/fork <branch>`.
    ///
    /// Called at the tail of each REPL drain (headless and script harness),
    /// so the next event in the new branch lands under `logs/branch-<new>/`.
    pub fn rebind_log_managers_if_branch_changed(&mut self) {
        let current = self.active_branch_id;
        if self.log_managers_branch == Some(current) {
            return;
        }
        if self.log_app_name.is_empty() {
            return;
        }
        let app_name = self.log_app_name.clone();
        if self.character_log.is_some() {
            let enabled = !self
                .flags
                .is_disabled(parish_core::character_log::FEATURE_FLAG);
            let manager =
                parish_core::character_log::CharacterLogManager::new(&app_name, current, enabled);
            if manager.enabled()
                && let Err(e) = manager.write_all_profiles(&self.world, &self.npc_manager)
            {
                tracing::warn!(
                    error = %e,
                    "character-log profile write failed after branch switch"
                );
            }
            self.character_log = Some(Arc::new(manager));
        }
        if self.location_log.is_some() {
            let enabled = !self
                .flags
                .is_disabled(parish_core::location_log::FEATURE_FLAG);
            let manager =
                parish_core::location_log::LocationLogManager::new(&app_name, current, enabled);
            if manager.enabled()
                && let Err(e) = manager.write_all_profiles(&self.world, &self.npc_manager)
            {
                tracing::warn!(
                    error = %e,
                    "location-log profile write failed after branch switch"
                );
            }
            self.location_log = Some(Arc::new(manager));
        }
        self.log_managers_branch = Some(current);
    }

    /// Returns the language settings derived from the active game mod.
    ///
    /// Falls back to plain `"en"` / no native language when no mod is loaded.
    pub fn language_settings(&self) -> LanguageSettings {
        self.game_mod
            .as_ref()
            .map(|gm| {
                LanguageSettings::new(
                    gm.player_language().to_string(),
                    gm.native_language().map(str::to_string),
                )
            })
            .unwrap_or_else(LanguageSettings::english_only)
    }

    /// Returns the [`CategoryOverride`] for categories that have one (not Dialogue).
    fn category_override(&self, cat: NonDialogueCategory) -> &CategoryOverride {
        match cat {
            NonDialogueCategory::Intent => &self.intent,
            NonDialogueCategory::Simulation => &self.simulation,
            NonDialogueCategory::Reaction => &self.reaction,
        }
    }

    /// Returns the mutable [`CategoryOverride`] for categories that have one (not Dialogue).
    fn category_override_mut(&mut self, cat: NonDialogueCategory) -> &mut CategoryOverride {
        match cat {
            NonDialogueCategory::Intent => &mut self.intent,
            NonDialogueCategory::Simulation => &mut self.simulation,
            NonDialogueCategory::Reaction => &mut self.reaction,
        }
    }

    pub fn category_provider_name(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_provider_name.as_deref(),
            cat => self
                .category_override(cat.try_into().expect("Dialogue already handled"))
                .provider_name
                .as_deref(),
        }
    }

    pub fn category_model(&self, cat: InferenceCategory) -> &str {
        match cat {
            InferenceCategory::Dialogue => self.cloud_model_name.as_deref().unwrap_or(""),
            cat => {
                &self
                    .category_override(cat.try_into().expect("Dialogue already handled"))
                    .model
            }
        }
    }

    pub fn category_api_key(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_api_key.as_deref(),
            cat => self
                .category_override(cat.try_into().expect("Dialogue already handled"))
                .api_key
                .as_deref(),
        }
    }

    pub fn category_base_url(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_base_url.as_deref(),
            cat => self
                .category_override(cat.try_into().expect("Dialogue already handled"))
                .base_url
                .as_deref(),
        }
    }

    pub fn category_client(&self, cat: InferenceCategory) -> Option<&AnyClient> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_client.as_ref(),
            cat => self
                .category_override(cat.try_into().expect("Dialogue already handled"))
                .client
                .as_ref(),
        }
    }

    pub fn set_category_provider_name(&mut self, cat: InferenceCategory, name: String) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_provider_name = Some(name),
            cat => {
                self.category_override_mut(cat.try_into().expect("Dialogue already handled"))
                    .provider_name = Some(name)
            }
        }
    }

    pub fn set_category_model(&mut self, cat: InferenceCategory, model: String) {
        match cat {
            InferenceCategory::Dialogue => {
                self.cloud_model_name = Some(model.clone());
                self.dialogue_model = model;
            }
            cat => {
                self.category_override_mut(cat.try_into().expect("Dialogue already handled"))
                    .model = model
            }
        }
    }

    pub fn set_category_api_key(&mut self, cat: InferenceCategory, key: String) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_api_key = Some(key),
            cat => {
                self.category_override_mut(cat.try_into().expect("Dialogue already handled"))
                    .api_key = Some(key)
            }
        }
    }

    pub fn set_category_base_url(&mut self, cat: InferenceCategory, url: String) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_base_url = Some(url),
            cat => {
                self.category_override_mut(cat.try_into().expect("Dialogue already handled"))
                    .base_url = Some(url)
            }
        }
    }

    pub fn set_category_client(&mut self, cat: InferenceCategory, client: AnyClient) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_client = Some(client),
            cat => {
                self.category_override_mut(cat.try_into().expect("Dialogue already handled"))
                    .client = Some(client)
            }
        }
    }

    /// Pushes an entry to the debug activity log (ring buffer).
    pub fn debug_event(&mut self, msg: String) {
        if self.debug_log.len() >= DEBUG_LOG_CAPACITY {
            self.debug_log.pop_front();
        }
        self.debug_log.push_back(msg);
    }

    /// Creates a [`GameConfig`] snapshot from this App's flat config fields.
    ///
    /// Used to pass config state to the shared [`parish_core::ipc::handle_command`]
    /// function without migrating all App fields to a nested GameConfig struct.
    pub fn snapshot_config(&self) -> parish_core::ipc::GameConfig {
        use parish_core::ipc::GameConfig;

        let mut cfg = GameConfig {
            provider_name: self.provider_name.clone(),
            base_url: self.base_url.clone(),
            api_key: self.api_key.clone(),
            model_name: self.model_name.clone(),
            cloud_provider_name: self.cloud_provider_name.clone(),
            cloud_model_name: self.cloud_model_name.clone(),
            cloud_api_key: self.cloud_api_key.clone(),
            cloud_base_url: self.cloud_base_url.clone(),
            improv_enabled: self.improv_enabled,
            reveal_unexplored_locations: self.reveal_unexplored_locations,
            flags: self.flags.clone(),
            ..GameConfig::default()
        };

        // Copy per-category overrides
        for cat in InferenceCategory::ALL {
            if let Some(p) = self.category_provider_name(cat) {
                cfg.category_provider.insert(cat, p.to_string());
            }
            let model = self.category_model(cat);
            if !model.is_empty() {
                cfg.category_model.insert(cat, model.to_string());
            }
            if let Some(k) = self.category_api_key(cat) {
                cfg.category_api_key.insert(cat, k.to_string());
            }
            if let Some(u) = self.category_base_url(cat) {
                cfg.category_base_url.insert(cat, u.to_string());
            }
        }
        cfg.inference_profile_override = self.inference_profile_override;
        cfg.category_inference_profile = self.category_inference_profile.clone();
        cfg.inference_routes_v2 = self.inference_routes_v2.clone();
        cfg.inference_subrole_routes_v2 = self.inference_subrole_routes_v2.clone();
        cfg.inference_configuration_epoch = self.inference_configuration_epoch;

        cfg
    }

    /// Applies changes from a [`GameConfig`] back to this App's flat fields.
    ///
    /// Called after [`parish_core::ipc::handle_command`] mutates the config.
    pub fn apply_config(&mut self, cfg: &parish_core::ipc::GameConfig) {
        self.provider_name = cfg.provider_name.clone();
        self.base_url = cfg.base_url.clone();
        self.api_key = cfg.api_key.clone();
        self.model_name = cfg.model_name.clone();
        self.cloud_provider_name = cfg.cloud_provider_name.clone();
        self.cloud_model_name = cfg.cloud_model_name.clone();
        self.cloud_api_key = cfg.cloud_api_key.clone();
        self.cloud_base_url = cfg.cloud_base_url.clone();
        self.improv_enabled = cfg.improv_enabled;
        self.reveal_unexplored_locations = cfg.reveal_unexplored_locations;
        self.flags = cfg.flags.clone();
        self.inference_profile_override = cfg.inference_profile_override;
        self.category_inference_profile = cfg.category_inference_profile.clone();
        self.inference_routes_v2 = cfg.inference_routes_v2.clone();
        self.inference_subrole_routes_v2 = cfg.inference_subrole_routes_v2.clone();
        self.inference_configuration_epoch = cfg.inference_configuration_epoch;

        // Apply per-category overrides
        for cat in InferenceCategory::ALL {
            if let Some(p) = cfg.category_provider.get(&cat) {
                self.set_category_provider_name(cat, p.clone());
            }
            if let Some(m) = cfg.category_model.get(&cat) {
                self.set_category_model(cat, m.clone());
            }
            if let Some(k) = cfg.category_api_key.get(&cat) {
                self.set_category_api_key(cat, k.clone());
            }
            if let Some(u) = cfg.category_base_url.get(&cat) {
                self.set_category_base_url(cat, u.clone());
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_new() {
        let app = App::new();
        assert!(!app.should_quit);
        assert!(app.input_buffer.is_empty());
        assert!(app.inference_queue.is_none());
        assert_eq!(app.npc_manager.npc_count(), 0);
        assert!(!app.improv_enabled);
        assert!(!app.reveal_unexplored_locations);
    }

    #[test]
    fn test_app_default() {
        let app = App::default();
        assert!(!app.should_quit);
    }

    #[test]
    fn test_improv_toggle() {
        let mut app = App::new();
        assert!(!app.improv_enabled);
        app.improv_enabled = !app.improv_enabled;
        assert!(app.improv_enabled);
        app.improv_enabled = !app.improv_enabled;
        assert!(!app.improv_enabled);
    }

    #[test]
    fn test_debug_event() {
        let mut app = App::new();
        app.debug_event("test event".to_string());
        assert_eq!(app.debug_log.len(), 1);
        assert_eq!(app.debug_log[0], "test event");
    }

    #[test]
    fn test_debug_event_capacity() {
        let mut app = App::new();
        for i in 0..DEBUG_LOG_CAPACITY + 5 {
            app.debug_event(format!("event {}", i));
        }
        assert_eq!(app.debug_log.len(), DEBUG_LOG_CAPACITY);
        // Oldest entries should have been evicted
        assert!(app.debug_log[0].contains("event 5"));
    }
}
