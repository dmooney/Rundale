//! Shared application state and event bus for the web server.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::{Mutex, broadcast};

use parish_core::inference::InferenceQueue;
use parish_core::inference::openai_client::OpenAiClient;
use parish_core::npc::manager::NpcManager;
use parish_core::world::WorldState;
use parish_core::world::transport::TransportConfig;

/// UI configuration snapshot returned by the `/api/ui-config` endpoint.
#[derive(serde::Serialize, Clone)]
pub struct UiConfigSnapshot {
    /// Label for the language-hints sidebar panel.
    pub hints_label: String,
    /// Default accent colour (CSS hex string).
    pub default_accent: String,
    /// Splash text displayed on game start (Zork-style).
    pub splash_text: String,
}

/// Current save state for display in the StatusBar.
#[derive(serde::Serialize, Clone)]
pub struct SaveState {
    /// Filename of the current save file (e.g. "parish_001.db"), or None.
    pub filename: Option<String>,
    /// Current branch database id, or None.
    pub branch_id: Option<i64>,
    /// Current branch name, or None.
    pub branch_name: Option<String>,
}

/// Shared mutable game state for the web server.
///
/// Mirrors the Tauri `AppState` but uses an [`EventBus`] for push events
/// instead of a Tauri `AppHandle`.
pub struct AppState {
    /// The game world (clock, player position, graph, weather).
    pub world: Mutex<WorldState>,
    /// NPC manager (all NPCs, tier assignment, schedule ticking).
    pub npc_manager: Mutex<NpcManager>,
    /// Inference request queue (None if no provider configured).
    pub inference_queue: Mutex<Option<InferenceQueue>>,
    /// Local LLM client (None if no provider is configured).
    pub client: Mutex<Option<OpenAiClient>>,
    /// Cloud LLM client for dialogue (None if not configured).
    pub cloud_client: Mutex<Option<OpenAiClient>>,
    /// Mutable runtime configuration.
    pub config: Mutex<GameConfig>,
    /// Broadcast channel for pushing events to WebSocket clients.
    pub event_bus: EventBus,
    /// Transport mode configuration from the loaded game mod.
    pub transport: TransportConfig,
    /// UI configuration from the loaded game mod.
    pub ui_config: UiConfigSnapshot,
    /// Directory where save files are stored.
    pub saves_dir: PathBuf,
    /// Directory containing game data files (world.json, npcs.json, etc.).
    pub data_dir: PathBuf,
    /// Path to the currently active save file.
    pub save_path: Mutex<Option<PathBuf>>,
    /// Current branch database id.
    pub current_branch_id: Mutex<Option<i64>>,
    /// Current branch name.
    pub current_branch_name: Mutex<Option<String>>,
    /// LLM client for NPC arrival reactions (None if not configured).
    pub reaction_client: Mutex<Option<OpenAiClient>>,
    /// Model name for reaction inference.
    pub reaction_model: Mutex<String>,
    /// Loaded game mod data (for reaction templates, etc.).
    pub game_mod: Option<parish_core::game_mod::GameMod>,
}

// GameConfig is now shared across all backends via parish-core.
pub use parish_core::ipc::GameConfig;

/// A JSON-serializable server event pushed to WebSocket clients.
#[derive(Clone, Debug, serde::Serialize)]
pub struct ServerEvent {
    /// Event name (e.g. "stream-token", "text-log").
    pub event: String,
    /// JSON payload for this event.
    pub payload: serde_json::Value,
}

/// Broadcast channel for server-push events.
///
/// Events emitted here are forwarded to all connected WebSocket clients.
pub struct EventBus {
    tx: broadcast::Sender<ServerEvent>,
}

impl EventBus {
    /// Creates a new event bus with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Sends an event to all subscribers. Returns the number of receivers.
    pub fn send(&self, event: ServerEvent) -> usize {
        match self.tx.send(event) {
            Ok(count) => count,
            Err(_) => {
                tracing::warn!("EventBus: broadcast failed — no active subscribers");
                0
            }
        }
    }

    /// Emits a named event with a serializable payload.
    pub fn emit<T: serde::Serialize>(&self, event_name: &str, payload: &T) {
        match serde_json::to_value(payload) {
            Ok(value) => {
                self.send(ServerEvent {
                    event: event_name.to_string(),
                    payload: value,
                });
            }
            Err(e) => {
                tracing::warn!(event = %event_name, error = %e, "EventBus: failed to serialize event payload");
            }
        }
    }

    /// Creates a new receiver for this bus.
    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.tx.subscribe()
    }
}

/// Creates the shared [`AppState`] from game data.
// AppState is a flat bundle of all server-wide singletons; a builder pattern
// would add complexity without benefit, so the many-argument constructor is intentional.
#[allow(clippy::too_many_arguments)]
pub fn build_app_state(
    world: WorldState,
    npc_manager: NpcManager,
    client: Option<OpenAiClient>,
    config: GameConfig,
    cloud_client: Option<OpenAiClient>,
    transport: TransportConfig,
    ui_config: UiConfigSnapshot,
    saves_dir: PathBuf,
    data_dir: PathBuf,
    game_mod: Option<parish_core::game_mod::GameMod>,
) -> Arc<AppState> {
    // Reaction client defaults to the base client (can be overridden later).
    let reaction_client = client.clone();
    Arc::new(AppState {
        world: Mutex::new(world),
        npc_manager: Mutex::new(npc_manager),
        inference_queue: Mutex::new(None),
        client: Mutex::new(client),
        cloud_client: Mutex::new(cloud_client),
        config: Mutex::new(config),
        event_bus: EventBus::new(256),
        transport,
        ui_config,
        saves_dir,
        data_dir,
        save_path: Mutex::new(None),
        current_branch_id: Mutex::new(None),
        current_branch_name: Mutex::new(None),
        reaction_client: Mutex::new(reaction_client),
        reaction_model: Mutex::new(String::new()),
        game_mod,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_bus_send_and_subscribe() {
        let bus = EventBus::new(16);
        let mut rx = bus.subscribe();
        bus.emit("test-event", &serde_json::json!({"key": "value"}));
        let event = rx.try_recv().unwrap();
        assert_eq!(event.event, "test-event");
        assert_eq!(event.payload["key"], "value");
    }

    #[test]
    fn event_bus_no_subscribers() {
        let bus = EventBus::new(16);
        // No subscribers — should not panic
        let count = bus.send(ServerEvent {
            event: "orphan".to_string(),
            payload: serde_json::Value::Null,
        });
        assert_eq!(count, 0);
    }
}
