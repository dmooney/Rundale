//! Core application state shared across all UI modes.
//!
//! Contains the [`App`] struct (game state container), used by headless,
//! script, and Tauri modes.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use parish_core::session_store::DbSessionStore;

use crate::config::{InferenceCategory, InferenceConfig};
use crate::inference::AnyClient;
use crate::inference::InferenceQueue;
use crate::loading::LoadingAnimation;
use crate::npc::LanguageHint;
use crate::npc::LanguageSettings;
use crate::npc::manager::NpcManager;
use crate::persistence::AsyncDatabase;
use crate::world::WorldState;
use parish_core::game_mod::GameMod;

/// Maximum number of entries in the debug activity log.
pub const DEBUG_LOG_CAPACITY: usize = 50;

/// Main application state.
///
/// Holds the game world state, input buffer, and control flags.
/// Shared across headless, script, and Tauri modes.
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
    /// Whether the Irish pronunciation sidebar is visible.
    pub sidebar_visible: bool,
    /// Pronunciation hints for secondary-language words from NPC responses.
    pub pronunciation_hints: Vec<LanguageHint>,
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
    /// Counter for rotating idle messages.
    pub idle_counter: usize,
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
    /// Path to the active save database file.
    pub save_file_path: Option<PathBuf>,
    /// Active save branch id.
    pub active_branch_id: i64,
    /// Most recent snapshot id on the active branch.
    pub latest_snapshot_id: i64,
    /// Wall-clock time of the last autosave.
    pub last_autosave: Option<Instant>,
    /// The LLM client for intent parsing (may differ from base client).
    pub intent_client: Option<AnyClient>,
    /// The model name for intent parsing.
    pub intent_model: String,
    /// Provider name for intent category (None = inherits base).
    pub intent_provider_name: Option<String>,
    /// API key for intent category.
    pub intent_api_key: Option<String>,
    /// Base URL for intent category.
    pub intent_base_url: Option<String>,
    /// The LLM client for simulation (may differ from base client).
    pub simulation_client: Option<AnyClient>,
    /// The model name for simulation.
    pub simulation_model: String,
    /// Provider name for simulation category (None = inherits base).
    pub simulation_provider_name: Option<String>,
    /// API key for simulation category.
    pub simulation_api_key: Option<String>,
    /// Base URL for simulation category.
    pub simulation_base_url: Option<String>,
    /// The LLM client for NPC arrival reactions (may differ from base client).
    pub reaction_client: Option<AnyClient>,
    /// The model name for reactions.
    pub reaction_model: String,
    /// Provider name for reaction category (None = inherits base).
    pub reaction_provider_name: Option<String>,
    /// API key for reaction category.
    pub reaction_api_key: Option<String>,
    /// Base URL for reaction category.
    pub reaction_base_url: Option<String>,
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
            sidebar_visible: false,
            pronunciation_hints: Vec::new(),
            improv_enabled: false,
            reveal_unexplored_locations: false,
            debug_sidebar_visible: false,
            debug_tab: 0,
            debug_selected_npc: None,
            debug_scroll: 0,
            debug_log: VecDeque::with_capacity(DEBUG_LOG_CAPACITY),
            idle_counter: 0,
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
            save_file_path: None,
            active_branch_id: 1,
            latest_snapshot_id: 0,
            last_autosave: None,
            intent_client: None,
            intent_model: String::new(),
            intent_provider_name: None,
            intent_api_key: None,
            intent_base_url: None,
            simulation_client: None,
            simulation_model: String::new(),
            simulation_provider_name: None,
            simulation_api_key: None,
            simulation_base_url: None,
            reaction_client: None,
            reaction_model: String::new(),
            reaction_provider_name: None,
            reaction_api_key: None,
            reaction_base_url: None,
            game_mod: None,
            flags: crate::config::FeatureFlags::default(),
            flags_path: None,
            save_lock: None,
            inference_config: InferenceConfig::default(),
            script_mode: false,
            // Placeholder — overwritten by run_headless after ensure_saves_dir() resolves
            // the real saves directory (#696 slice 8).
            session_store: Arc::new(DbSessionStore::new(PathBuf::from("saves"))),
        }
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

    /// Returns the provider name for a given inference category (or None if inheriting base).
    pub fn category_provider_name(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_provider_name.as_deref(),
            InferenceCategory::Simulation => self.simulation_provider_name.as_deref(),
            InferenceCategory::Intent => self.intent_provider_name.as_deref(),
            InferenceCategory::Reaction => self.reaction_provider_name.as_deref(),
        }
    }

    /// Returns the model name for a given inference category (empty string if inheriting base).
    pub fn category_model(&self, cat: InferenceCategory) -> &str {
        match cat {
            InferenceCategory::Dialogue => self.cloud_model_name.as_deref().unwrap_or(""),
            InferenceCategory::Simulation => &self.simulation_model,
            InferenceCategory::Intent => &self.intent_model,
            InferenceCategory::Reaction => &self.reaction_model,
        }
    }

    /// Returns the API key for a given inference category.
    pub fn category_api_key(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_api_key.as_deref(),
            InferenceCategory::Simulation => self.simulation_api_key.as_deref(),
            InferenceCategory::Intent => self.intent_api_key.as_deref(),
            InferenceCategory::Reaction => self.reaction_api_key.as_deref(),
        }
    }

    /// Returns the base URL for a given inference category.
    pub fn category_base_url(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_base_url.as_deref(),
            InferenceCategory::Simulation => self.simulation_base_url.as_deref(),
            InferenceCategory::Intent => self.intent_base_url.as_deref(),
            InferenceCategory::Reaction => self.reaction_base_url.as_deref(),
        }
    }

    /// Returns the client for a given inference category.
    pub fn category_client(&self, cat: InferenceCategory) -> Option<&AnyClient> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_client.as_ref(),
            InferenceCategory::Simulation => self.simulation_client.as_ref(),
            InferenceCategory::Intent => self.intent_client.as_ref(),
            InferenceCategory::Reaction => self.reaction_client.as_ref(),
        }
    }

    /// Sets the provider name for a given inference category.
    pub fn set_category_provider_name(&mut self, cat: InferenceCategory, name: String) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_provider_name = Some(name),
            InferenceCategory::Simulation => self.simulation_provider_name = Some(name),
            InferenceCategory::Intent => self.intent_provider_name = Some(name),
            InferenceCategory::Reaction => self.reaction_provider_name = Some(name),
        }
    }

    /// Sets the model name for a given inference category.
    pub fn set_category_model(&mut self, cat: InferenceCategory, model: String) {
        match cat {
            InferenceCategory::Dialogue => {
                self.cloud_model_name = Some(model.clone());
                self.dialogue_model = model;
            }
            InferenceCategory::Simulation => self.simulation_model = model,
            InferenceCategory::Intent => self.intent_model = model,
            InferenceCategory::Reaction => self.reaction_model = model,
        }
    }

    /// Sets the API key for a given inference category.
    pub fn set_category_api_key(&mut self, cat: InferenceCategory, key: String) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_api_key = Some(key),
            InferenceCategory::Simulation => self.simulation_api_key = Some(key),
            InferenceCategory::Intent => self.intent_api_key = Some(key),
            InferenceCategory::Reaction => self.reaction_api_key = Some(key),
        }
    }

    /// Sets the base URL for a given inference category.
    pub fn set_category_base_url(&mut self, cat: InferenceCategory, url: String) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_base_url = Some(url),
            InferenceCategory::Simulation => self.simulation_base_url = Some(url),
            InferenceCategory::Intent => self.intent_base_url = Some(url),
            InferenceCategory::Reaction => self.reaction_base_url = Some(url),
        }
    }

    /// Sets the client for a given inference category.
    pub fn set_category_client(&mut self, cat: InferenceCategory, client: AnyClient) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_client = Some(client),
            InferenceCategory::Simulation => self.simulation_client = Some(client),
            InferenceCategory::Intent => self.intent_client = Some(client),
            InferenceCategory::Reaction => self.reaction_client = Some(client),
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
            max_follow_up_turns: 2,
            idle_banter_after_secs: 25,
            auto_pause_after_secs: 300,
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
        assert!(!app.sidebar_visible);
        assert!(!app.improv_enabled);
        assert!(!app.reveal_unexplored_locations);
        assert!(app.pronunciation_hints.is_empty());
        assert_eq!(app.idle_counter, 0);
    }

    #[test]
    fn test_app_default() {
        let app = App::default();
        assert!(!app.should_quit);
        assert!(!app.sidebar_visible);
    }

    #[test]
    fn test_sidebar_toggle() {
        let mut app = App::new();
        assert!(!app.sidebar_visible);
        app.sidebar_visible = !app.sidebar_visible;
        assert!(app.sidebar_visible);
        app.sidebar_visible = !app.sidebar_visible;
        assert!(!app.sidebar_visible);
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
    fn test_pronunciation_hints_storage() {
        use crate::npc::LanguageHint;
        let mut app = App::new();
        let hint = LanguageHint {
            word: "sláinte".to_string(),
            pronunciation: "SLAWN-cha".to_string(),
            meaning: Some("Health/cheers".to_string()),
        };
        app.pronunciation_hints.push(hint.clone());
        assert_eq!(app.pronunciation_hints.len(), 1);
        assert_eq!(app.pronunciation_hints[0].word, "sláinte");
    }

    #[test]
    fn test_pronunciation_hints_truncation() {
        use crate::npc::LanguageHint;
        let mut app = App::new();
        for i in 0..25 {
            app.pronunciation_hints.push(LanguageHint {
                word: format!("word_{}", i),
                pronunciation: format!("pron_{}", i),
                meaning: None,
            });
        }
        app.pronunciation_hints.truncate(20);
        assert_eq!(app.pronunciation_hints.len(), 20);
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
