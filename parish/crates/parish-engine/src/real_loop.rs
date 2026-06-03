//! Real-loop execution path for [`GameTestHarness`] (#1159).
//!
//! The legacy harness ([`crate::testing`]) reimplements game-input routing,
//! NPC dialogue, and system-command dispatch in parallel to the shipping
//! engine — which is exactly where harness behavior drifts from
//! `parish_core::game_loop` (#985, #1028). This module drives the **real**
//! `game_loop` instead, mocking only the LLM boundary
//! ([`crate::inference::MockClient`]) and capturing the emitted events via a
//! [`CapturingEmitter`].
//!
//! It is the foundation for differential ("shadow") testing: running the same
//! input through both engines and recording where their event streams differ.
//! Nothing here changes the legacy path — [`GameTestHarness::execute`] is
//! untouched; [`GameTestHarness::execute_via_real_loop`] is an additive
//! parallel entry point.

use std::sync::Arc;

use tokio::sync::Mutex;

use parish_core::game_loop::{GameLoopContext, handle_game_input, handle_system_command};
use parish_core::ipc::{CapturingEmitter, ConversationRuntimeState, EventEmitter};
use parish_core::npc::reactions::ReactionTemplates;

use crate::command_host::CliCommandHost;
use crate::inference::{AnyClient, MockClient};
use crate::input::{self, InputResult};
use crate::testing::GameTestHarness;

impl GameTestHarness {
    /// Returns the scriptable mock client backing the real-loop path. Enqueue
    /// canned completions on it (e.g. `harness.mock().push_for("Peig", "...")`)
    /// to script NPC dialogue deterministically.
    pub fn mock(&self) -> Arc<MockClient> {
        Arc::clone(&self.mock)
    }

    /// Executes a single input line through the **real** `parish_core::game_loop`
    /// (rather than the legacy harness router) and returns every event the loop
    /// emitted, in order, as `(name, payload)` pairs.
    ///
    /// System commands are dispatched through
    /// [`parish_core::game_loop::handle_system_command`] (via a capturing
    /// [`CliCommandHost`]); free-form text goes through
    /// [`parish_core::game_loop::handle_game_input`] with the mock client and a
    /// [`CapturingEmitter`] injected. State is moved out of `self.app` for the
    /// duration of the call and moved back afterwards, so the harness's own
    /// world/NPC state advances exactly as the real engine would advance it.
    pub fn execute_via_real_loop(&mut self, line: &str) -> Vec<(String, serde_json::Value)> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        // The shared handlers are async; the harness is sync. A current-thread
        // runtime drives them (and any tasks they spawn) to completion.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime for real-loop harness");

        let emitter = Arc::new(CapturingEmitter::new());

        match input::classify_input(trimmed) {
            InputResult::SystemCommand(cmd) => {
                // Move App into an Arc<Mutex<App>> so the shared SystemCommandHost
                // can borrow it — the same temporary-ownership dance the headless
                // REPL uses (see command_host.rs docs).
                let app_val = std::mem::take(&mut self.app);
                let app_arc = Arc::new(Mutex::new(app_val));
                let host = CliCommandHost::new_capturing(Arc::clone(&app_arc), Arc::clone(&emitter));
                rt.block_on(handle_system_command(&host, cmd));
                self.app = Arc::into_inner(app_arc)
                    .expect("real-loop host dropped: Arc should have exactly 1 reference")
                    .into_inner();
            }
            InputResult::GameInput(text) => {
                rt.block_on(self.run_game_input_real(&text, Arc::clone(&emitter)));
            }
        }

        emitter.drain()
    }

    /// Drives [`handle_game_input`] over the harness's state with the mock
    /// client and the capturing emitter wired in.
    async fn run_game_input_real(&mut self, text: &str, emitter: Arc<CapturingEmitter>) {
        let transport = crate::headless::default_transport(&self.app);

        // Mod-derived context that the shared loop needs. Cloned up front so the
        // borrows below don't conflict with the &mut moves of world/npc_manager.
        let language = self.app.language_settings();
        let inference_config = self.app.inference_config.clone();
        let (pronunciations, idle_messages, failure_messages) = match &self.app.game_mod {
            Some(gm) => (
                gm.pronunciations.clone(),
                gm.loading.idle_messages.clone(),
                gm.loading.inference_failure_messages.clone(),
            ),
            None => (Vec::new(), Vec::new(), Vec::new()),
        };

        // Move the live world / NPC state into Mutex containers for the borrow
        // struct, then move them back once the loop returns.
        let world = Mutex::new(std::mem::take(&mut self.app.world));
        let npc_manager = Mutex::new(std::mem::take(&mut self.app.npc_manager));
        let config = Mutex::new(self.app.snapshot_config());
        let conversation = Mutex::new(ConversationRuntimeState::new());
        let inference_queue = Mutex::new(None);
        let client = Mutex::new(Some(AnyClient::Mock(Arc::clone(&self.mock))));
        let cloud_client = Mutex::new(None);
        let templates = ReactionTemplates::default();
        let dyn_emitter: Arc<dyn EventEmitter> = emitter;

        let ctx = GameLoopContext {
            world: &world,
            npc_manager: &npc_manager,
            config: &config,
            conversation: &conversation,
            inference_queue: &inference_queue,
            emitter: dyn_emitter,
            inference_config: &inference_config,
            pronunciations: &pronunciations,
            client: &client,
            cloud_client: &cloud_client,
            language,
            inference_failure_messages: &failure_messages,
            idle_messages: &idle_messages,
        };

        handle_game_input(
            &ctx,
            text.to_string(),
            Vec::new(),
            &transport,
            &templates,
            || None,
        )
        .await;

        // Move mutated state back into the app, and fold config changes back.
        let new_config = config.into_inner();
        self.app.world = world.into_inner();
        self.app.npc_manager = npc_manager.into_inner();
        self.app.apply_config(&new_config);
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::GameTestHarness;

    /// C3 — `look` routed through the real game_loop emits a non-empty
    /// `text-log` describing the start location.
    #[test]
    fn real_loop_look_emits_text_log_describing_start_location() {
        let mut h = GameTestHarness::new();
        let start = h.player_location().to_string();
        assert!(!start.is_empty());
        // The rendered description references the place by its distinctive
        // first word (e.g. "Kilteevan") rather than the full display name
        // ("Kilteevan Village"), so match on that token.
        let landmark = start.split_whitespace().next().unwrap_or(&start);

        let events = h.execute_via_real_loop("look");

        let text_logs: Vec<String> = events
            .iter()
            .filter(|(name, _)| name == "text-log")
            .filter_map(|(_, payload)| {
                payload
                    .get("content")
                    .and_then(|t| t.as_str())
                    .map(str::to_string)
            })
            .collect();

        assert!(
            !text_logs.is_empty(),
            "expected at least one text-log event from look; got {events:?}"
        );
        let joined = text_logs.join("\n");
        assert!(
            joined.contains(landmark),
            "look output should describe start location {landmark:?}; got {joined:?}"
        );
    }
}
