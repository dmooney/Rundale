//! Shared application state and event bus for the web server.

use std::sync::Arc;

use tokio::sync::{Mutex, broadcast};

use parish_core::inference::InferenceQueue;
use parish_core::inference::openai_client::OpenAiClient;
use parish_core::npc::manager::NpcManager;
use parish_core::world::WorldState;

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
}

/// Mutable runtime configuration for provider, model, and cloud settings.
pub struct GameConfig {
    /// Display name of the current base provider.
    pub provider_name: String,
    /// Base URL for the current provider API.
    pub base_url: String,
    /// API key for the current provider.
    pub api_key: Option<String>,
    /// Model name for NPC dialogue inference.
    pub model_name: String,
    /// Cloud provider name for dialogue.
    pub cloud_provider_name: Option<String>,
    /// Cloud model name for dialogue.
    pub cloud_model_name: Option<String>,
    /// Cloud API key.
    pub cloud_api_key: Option<String>,
    /// Cloud base URL.
    pub cloud_base_url: Option<String>,
    /// Whether improv craft mode is enabled.
    pub improv_enabled: bool,
    /// Per-category provider name overrides (Dialogue=0, Simulation=1, Intent=2).
    pub category_provider: [Option<String>; 3],
    /// Per-category model name overrides.
    pub category_model: [Option<String>; 3],
    /// Per-category API key overrides.
    pub category_api_key: [Option<String>; 3],
    /// Per-category base URL overrides.
    pub category_base_url: [Option<String>; 3],
}

impl GameConfig {
    /// Returns the array index for a category.
    pub fn cat_idx(cat: parish_core::config::InferenceCategory) -> usize {
        use parish_core::config::InferenceCategory;
        match cat {
            InferenceCategory::Dialogue => 0,
            InferenceCategory::Simulation => 1,
            InferenceCategory::Intent => 2,
        }
    }
}

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
pub fn build_app_state(
    world: WorldState,
    npc_manager: NpcManager,
    client: Option<OpenAiClient>,
    config: GameConfig,
    cloud_client: Option<OpenAiClient>,
) -> Arc<AppState> {
    Arc::new(AppState {
        world: Mutex::new(world),
        npc_manager: Mutex::new(npc_manager),
        inference_queue: Mutex::new(None),
        client: Mutex::new(client),
        cloud_client: Mutex::new(cloud_client),
        config: Mutex::new(config),
        event_bus: EventBus::new(256),
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
