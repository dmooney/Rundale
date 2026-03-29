//! Core application state shared across all UI modes.
//!
//! Contains the [`App`] struct (game state container) and [`ScrollState`],
//! used by headless, script, and Tauri modes.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Instant;

use crate::config::InferenceCategory;
use crate::inference::InferenceQueue;
use crate::inference::openai_client::OpenAiClient;
use crate::loading::LoadingAnimation;
use crate::npc::LanguageHint;
use crate::npc::manager::NpcManager;
use crate::persistence::AsyncDatabase;
use crate::world::WorldState;
use parish_core::game_mod::GameMod;

/// Maximum number of entries in the debug activity log.
pub const DEBUG_LOG_CAPACITY: usize = 50;

/// Scroll state for the main text panel.
///
/// Tracks the scroll offset and whether auto-scroll (follow new output)
/// is active. When the user scrolls up, auto-scroll is disabled until
/// they press End or scroll back to the bottom.
#[derive(Debug, Clone)]
pub struct ScrollState {
    /// Current scroll offset in lines from the top.
    pub offset: u16,
    /// Whether to auto-scroll to the bottom on new content.
    pub auto_scroll: bool,
}

impl ScrollState {
    /// Creates a new scroll state with auto-scroll enabled.
    pub fn new() -> Self {
        Self {
            offset: 0,
            auto_scroll: true,
        }
    }

    /// Scrolls up by the given number of lines.
    pub fn scroll_up(&mut self, lines: u16) {
        self.offset = self.offset.saturating_add(lines);
        self.auto_scroll = false;
    }

    /// Scrolls down by the given number of lines.
    pub fn scroll_down(&mut self, lines: u16, max_offset: u16) {
        self.offset = self.offset.saturating_sub(lines);
        if self.offset == 0 {
            self.auto_scroll = true;
        }
        // Clamp — offset is distance from bottom, so 0 = bottom
        let _ = max_offset; // kept for API clarity; clamping happens in update()
    }

    /// Scrolls to the top of the text log.
    pub fn scroll_to_top(&mut self, max_offset: u16) {
        self.offset = max_offset;
        self.auto_scroll = false;
    }

    /// Scrolls to the bottom and re-enables auto-scroll.
    pub fn scroll_to_bottom(&mut self) {
        self.offset = 0;
        self.auto_scroll = true;
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        Self::new()
    }
}

/// Main application state.
///
/// Holds the game world state, input buffer, scroll state, and control flags.
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
    /// Scroll state for the main text panel.
    pub scroll: ScrollState,
    /// Whether the Irish pronunciation sidebar is visible.
    pub sidebar_visible: bool,
    /// Pronunciation hints for secondary-language words from NPC responses.
    pub pronunciation_hints: Vec<LanguageHint>,
    /// Whether improv craft mode is enabled for NPC dialogue.
    pub improv_enabled: bool,
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
    pub client: Option<OpenAiClient>,
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
    pub cloud_client: Option<OpenAiClient>,
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
    /// Active save branch id.
    pub active_branch_id: i64,
    /// Most recent snapshot id on the active branch.
    pub latest_snapshot_id: i64,
    /// Wall-clock time of the last autosave.
    pub last_autosave: Option<Instant>,
    /// The LLM client for intent parsing (may differ from base client).
    pub intent_client: Option<OpenAiClient>,
    /// The model name for intent parsing.
    pub intent_model: String,
    /// Provider name for intent category (None = inherits base).
    pub intent_provider_name: Option<String>,
    /// API key for intent category.
    pub intent_api_key: Option<String>,
    /// Base URL for intent category.
    pub intent_base_url: Option<String>,
    /// The LLM client for simulation (may differ from base client).
    pub simulation_client: Option<OpenAiClient>,
    /// The model name for simulation.
    pub simulation_model: String,
    /// Provider name for simulation category (None = inherits base).
    pub simulation_provider_name: Option<String>,
    /// API key for simulation category.
    pub simulation_api_key: Option<String>,
    /// Base URL for simulation category.
    pub simulation_base_url: Option<String>,
    /// Loaded game mod data (None if no mod directory was found or specified).
    pub game_mod: Option<GameMod>,
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
            scroll: ScrollState::new(),
            sidebar_visible: false,
            pronunciation_hints: Vec::new(),
            improv_enabled: false,
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
            game_mod: None,
        }
    }

    /// Returns the provider name for a given inference category (or None if inheriting base).
    pub fn category_provider_name(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_provider_name.as_deref(),
            InferenceCategory::Simulation => self.simulation_provider_name.as_deref(),
            InferenceCategory::Intent => self.intent_provider_name.as_deref(),
        }
    }

    /// Returns the model name for a given inference category (empty string if inheriting base).
    pub fn category_model(&self, cat: InferenceCategory) -> &str {
        match cat {
            InferenceCategory::Dialogue => self.cloud_model_name.as_deref().unwrap_or(""),
            InferenceCategory::Simulation => &self.simulation_model,
            InferenceCategory::Intent => &self.intent_model,
        }
    }

    /// Returns the API key for a given inference category.
    pub fn category_api_key(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_api_key.as_deref(),
            InferenceCategory::Simulation => self.simulation_api_key.as_deref(),
            InferenceCategory::Intent => self.intent_api_key.as_deref(),
        }
    }

    /// Returns the base URL for a given inference category.
    pub fn category_base_url(&self, cat: InferenceCategory) -> Option<&str> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_base_url.as_deref(),
            InferenceCategory::Simulation => self.simulation_base_url.as_deref(),
            InferenceCategory::Intent => self.intent_base_url.as_deref(),
        }
    }

    /// Returns the client for a given inference category.
    pub fn category_client(&self, cat: InferenceCategory) -> Option<&OpenAiClient> {
        match cat {
            InferenceCategory::Dialogue => self.cloud_client.as_ref(),
            InferenceCategory::Simulation => self.simulation_client.as_ref(),
            InferenceCategory::Intent => self.intent_client.as_ref(),
        }
    }

    /// Sets the provider name for a given inference category.
    pub fn set_category_provider_name(&mut self, cat: InferenceCategory, name: String) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_provider_name = Some(name),
            InferenceCategory::Simulation => self.simulation_provider_name = Some(name),
            InferenceCategory::Intent => self.intent_provider_name = Some(name),
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
        }
    }

    /// Sets the API key for a given inference category.
    pub fn set_category_api_key(&mut self, cat: InferenceCategory, key: String) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_api_key = Some(key),
            InferenceCategory::Simulation => self.simulation_api_key = Some(key),
            InferenceCategory::Intent => self.intent_api_key = Some(key),
        }
    }

    /// Sets the base URL for a given inference category.
    pub fn set_category_base_url(&mut self, cat: InferenceCategory, url: String) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_base_url = Some(url),
            InferenceCategory::Simulation => self.simulation_base_url = Some(url),
            InferenceCategory::Intent => self.intent_base_url = Some(url),
        }
    }

    /// Sets the client for a given inference category.
    pub fn set_category_client(&mut self, cat: InferenceCategory, client: OpenAiClient) {
        match cat {
            InferenceCategory::Dialogue => self.cloud_client = Some(client),
            InferenceCategory::Simulation => self.simulation_client = Some(client),
            InferenceCategory::Intent => self.intent_client = Some(client),
        }
    }

    /// Pushes an entry to the debug activity log (ring buffer).
    pub fn debug_event(&mut self, msg: String) {
        if self.debug_log.len() >= DEBUG_LOG_CAPACITY {
            self.debug_log.pop_front();
        }
        self.debug_log.push_back(msg);
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
        assert!(app.scroll.auto_scroll);
        assert_eq!(app.scroll.offset, 0);
        assert!(!app.sidebar_visible);
        assert!(!app.improv_enabled);
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
    fn test_scroll_state_new() {
        let scroll = ScrollState::new();
        assert_eq!(scroll.offset, 0);
        assert!(scroll.auto_scroll);
    }

    #[test]
    fn test_scroll_up_disables_auto() {
        let mut scroll = ScrollState::new();
        scroll.scroll_up(5);
        assert_eq!(scroll.offset, 5);
        assert!(!scroll.auto_scroll);
    }

    #[test]
    fn test_scroll_down_reenables_auto_at_bottom() {
        let mut scroll = ScrollState::new();
        scroll.scroll_up(3);
        assert!(!scroll.auto_scroll);

        scroll.scroll_down(3, 100);
        assert_eq!(scroll.offset, 0);
        assert!(scroll.auto_scroll);
    }

    #[test]
    fn test_scroll_down_partial() {
        let mut scroll = ScrollState::new();
        scroll.scroll_up(10);
        scroll.scroll_down(3, 100);
        assert_eq!(scroll.offset, 7);
        assert!(!scroll.auto_scroll);
    }

    #[test]
    fn test_scroll_down_clamps_at_zero() {
        let mut scroll = ScrollState::new();
        scroll.scroll_up(2);
        scroll.scroll_down(10, 100);
        assert_eq!(scroll.offset, 0);
        assert!(scroll.auto_scroll);
    }

    #[test]
    fn test_scroll_to_top() {
        let mut scroll = ScrollState::new();
        scroll.scroll_to_top(50);
        assert_eq!(scroll.offset, 50);
        assert!(!scroll.auto_scroll);
    }

    #[test]
    fn test_scroll_to_bottom() {
        let mut scroll = ScrollState::new();
        scroll.scroll_up(20);
        scroll.scroll_to_bottom();
        assert_eq!(scroll.offset, 0);
        assert!(scroll.auto_scroll);
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
