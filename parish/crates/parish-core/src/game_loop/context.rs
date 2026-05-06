//! [`GameLoopContext`] — borrow struct for shared orchestration functions.

use std::sync::Arc;

use tokio::sync::Mutex;

use crate::config::InferenceConfig;
use crate::game_mod::PronunciationEntry;
use crate::inference::{AnyClient, InferenceQueue};
use crate::ipc::{ConversationRuntimeState, EventEmitter, GameConfig};
use crate::npc::LanguageSettings;
use crate::npc::manager::NpcManager;
use crate::world::WorldState;

/// A short-lived borrow of all game-loop state required by the shared
/// orchestration functions.
///
/// Callers construct this by borrowing their backend-specific `AppState` fields.
/// The struct holds only references (lifetime `'a`) to Mutex containers — no
/// guards — so it composes with existing `Arc<AppState>` patterns without
/// restructuring.
///
/// The emitter is held as `Arc<dyn EventEmitter>` (not a reference) so it can
/// be cloned cheaply into background tasks (e.g. the token-streaming task in
/// [`super::run_npc_turn`]).
///
/// # Lock ordering
///
/// Callers and callee functions must acquire guards in the documented order:
///
/// ```text
/// world → npc_manager → inference_queue → conversation → config → client → cloud_client
/// ```
pub struct GameLoopContext<'a> {
    /// The game world (clock, player location, graph, weather).
    pub world: &'a Mutex<WorldState>,
    /// NPC manager (all NPCs, tier assignment).
    pub npc_manager: &'a Mutex<NpcManager>,
    /// Mutable runtime configuration (model, flags, etc.).
    pub config: &'a Mutex<GameConfig>,
    /// Local conversation transcript and inactivity tracking.
    pub conversation: &'a Mutex<ConversationRuntimeState>,
    /// Inference request queue (None if no provider is configured).
    pub inference_queue: &'a Mutex<Option<InferenceQueue>>,
    /// Backend-specific event emitter, cloneable into background tasks.
    pub emitter: Arc<dyn EventEmitter>,
    /// TOML-configured inference timeouts.
    pub inference_config: &'a InferenceConfig,
    /// Name pronunciation hints from the loaded game mod.
    pub pronunciations: &'a [PronunciationEntry],
    /// Base LLM client (None if no provider is configured).
    pub client: &'a Mutex<Option<AnyClient>>,
    /// Cloud LLM client for dialogue (None if not configured).
    pub cloud_client: &'a Mutex<Option<AnyClient>>,
    /// Language settings derived from the active mod manifest.
    ///
    /// Injected into every dialogue prompt builder so NPCs use locale-correct
    /// spelling and code-switch naturally when `native` is set.
    pub language: LanguageSettings,
}
