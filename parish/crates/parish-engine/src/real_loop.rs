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

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use tokio::sync::Mutex;

use parish_core::game_loop::{
    GameLoopContext, handle_game_input, handle_staged_game_input_with_journal,
    handle_system_command, input_may_mutate_tasks,
};
use parish_core::ipc::{CapturingEmitter, EventEmitter};
use parish_core::npc::reactions::ReactionTemplates;

use crate::command_host::CliCommandHost;
use crate::inference::{AnyClient, InferenceWorkerConfig, MockClient};
use crate::input::{self, InputResult};
use crate::testing::GameTestHarness;

impl GameTestHarness {
    /// Returns the scriptable mock client backing the real-loop path. Enqueue
    /// canned completions on it (e.g. `harness.mock().push_for("Peig", "...")`)
    /// to script NPC dialogue deterministically.
    pub fn mock(&self) -> Arc<MockClient> {
        Arc::clone(&self.mock)
    }

    /// Whether shadow mode is active for this harness.
    pub fn shadow_enabled(&self) -> bool {
        self.shadow_enabled
    }

    /// Enables shadow mode, directing divergence records to `ledger` and
    /// labelling them with `case`. Tests use this to opt in with an isolated
    /// ledger path instead of relying on the process-global env var.
    pub fn enable_shadow(&mut self, ledger: std::path::PathBuf, case: impl Into<String>) {
        self.shadow_enabled = true;
        self.shadow_ledger = ledger;
        self.shadow_case = case.into();
    }

    /// After the legacy [`GameTestHarness::execute`] path has run, replays the
    /// same input through the real `game_loop` on the rolled-back pre-state,
    /// compares the player-visible (`text-log`) output, and appends a record to
    /// the ledger on divergence. The post-legacy state is restored afterwards
    /// so the legacy timeline the caller sees is unaffected. Panics in the real
    /// loop are swallowed and recorded as a divergence rather than propagated —
    /// shadow mode must never destabilize the legacy run.
    pub(crate) fn shadow_compare_after_legacy(
        &mut self,
        input: &str,
        pre_snapshot: parish_core::persistence::snapshot::GameSnapshot,
        legacy_lines: Vec<String>,
    ) {
        let legacy_events: Vec<(String, serde_json::Value)> = crate::shadow::text_output_lines(
            &legacy_lines
                .iter()
                .map(|line| {
                    (
                        "text-log".to_string(),
                        serde_json::json!({ "content": line }),
                    )
                })
                .collect::<Vec<_>>(),
        );

        // Preserve the post-legacy state, roll back to the pre-state, run the
        // real loop, then restore the post-legacy state. The snapshot covers
        // world + NPCs; `App` config (flags, provider, theme) is captured and
        // restored separately, since a shadowed system command (e.g. `/flag`)
        // mutates config and the real-loop replay must not leak that into the
        // legacy timeline.
        let post_snapshot = parish_core::persistence::snapshot::GameSnapshot::capture(
            &self.app.world,
            &self.app.npc_manager,
        );
        let post_config = self.app.snapshot_config();
        pre_snapshot.restore(&mut self.app.world, &mut self.app.npc_manager);

        let real = std::panic::catch_unwind(AssertUnwindSafe(|| self.execute_via_real_loop(input)));

        post_snapshot.restore(&mut self.app.world, &mut self.app.npc_manager);
        self.app.apply_config(&post_config);

        let real_events =
            real.unwrap_or_else(|_| vec![("real-loop-panic".to_string(), serde_json::Value::Null)]);
        let real_text = crate::shadow::text_output_lines(&real_events);

        if let Some(record) =
            crate::shadow::compare(&self.shadow_case, input, &legacy_events, &real_text)
            && let Err(e) = crate::shadow::append_ledger(&self.shadow_ledger, &record)
        {
            tracing::warn!(error = %e, "shadow ledger append failed");
        }
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
                // REPL uses (see command_host.rs docs). The whole App is moved
                // out, so on a panic mid-command we must move it back before
                // unwinding or `self.app` would be left default — corrupting the
                // harness (the shadow wrapper relies on this safety).
                let app_arc = Arc::new(Mutex::new(std::mem::take(&mut self.app)));
                let host =
                    CliCommandHost::new_capturing(Arc::clone(&app_arc), Arc::clone(&emitter));
                let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    rt.block_on(handle_system_command(&host, cmd, trimmed))
                        .expect("real-loop system command failed")
                }));
                drop(host);
                self.app = Arc::into_inner(app_arc)
                    .expect("real-loop host dropped: Arc should have exactly 1 reference")
                    .into_inner();
                if let Err(payload) = outcome {
                    std::panic::resume_unwind(payload);
                }
            }
            InputResult::GameInput(text) => {
                self.run_game_input_real(&rt, &text, Arc::clone(&emitter));
            }
        }

        emitter.drain()
    }

    /// Drives [`handle_game_input`] over the harness's state with the mock
    /// client and the capturing emitter wired in.
    ///
    /// The world / NPC state is moved out of `self.app` for the call and moved
    /// back afterwards — including on panic — so the harness is never left with
    /// default state. The panic is re-raised after restoration so standalone
    /// callers still observe it (the shadow wrapper swallows it).
    fn run_game_input_real(
        &mut self,
        rt: &tokio::runtime::Runtime,
        text: &str,
        emitter: Arc<CapturingEmitter>,
    ) {
        let transport = crate::headless::default_transport(&self.app);

        // Mod-derived context that the shared loop needs. Gathered before the
        // world/NPC state is moved out below.
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
        let config_snapshot = self.app.snapshot_config();
        let active_branch_id = self.app.active_branch_id;
        let db_sync = self.db_sync.as_ref();

        // Move the live world / NPC state into Mutex containers for the borrow
        // struct.
        let world = Mutex::new(std::mem::take(&mut self.app.world));
        let npc_manager = Mutex::new(std::mem::take(&mut self.app.npc_manager));
        let config = Mutex::new(config_snapshot);
        // Reuse the harness-level persistent conversation state so session-level
        // data (e.g. `seen_openers_this_location` for cross-turn opener dedup,
        // #1492) accumulates across successive `execute_via_real_loop` calls.
        let conversation = std::sync::Arc::clone(&self.real_loop_conversation);
        // Filled inside the runtime below with a mock-backed queue so the
        // dialogue path runs against the real worker rather than short-circuiting
        // on a missing LLM (#1172 dialogue parity).
        let inference_queue = Mutex::new(None);
        let mock = Arc::clone(&self.mock);
        let client = Mutex::new(Some(AnyClient::Mock(Arc::clone(&self.mock))));
        let cloud_client = Mutex::new(None);
        let templates = ReactionTemplates::default();
        let dyn_emitter: Arc<dyn EventEmitter> = emitter;

        // catch_unwind at the synchronous block_on boundary so the borrows held
        // by the context don't outlive the unwind and the moved-out state is
        // always restored below.
        let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
            rt.block_on(async {
                // Spin up a mock-backed inference worker so the dialogue path
                // (handle_npc_conversation -> run_npc_turn) exercises the real
                // queue. The worker serves scripted completions from `mock`;
                // dropping the queue and aborting the worker at the end of the
                // call tears it down with the per-call runtime.
                let (itx, irx) = tokio::sync::mpsc::channel(16);
                let (btx, brx) = tokio::sync::mpsc::channel(32);
                let (xtx, xrx) = tokio::sync::mpsc::channel(64);
                let worker = parish_core::inference::spawn_inference_worker(
                    AnyClient::Mock(Arc::clone(&mock)),
                    InferenceWorkerConfig {
                        interactive_rx: irx,
                        background_rx: brx,
                        batch_rx: xrx,
                        log: parish_core::inference::new_inference_log(),
                        file_log: parish_core::inference::file_log::InferenceFileLog::disabled(),
                        provider: parish_core::config::Provider::default(),
                        timeout_config: inference_config.clone(),
                    },
                );
                *inference_queue.lock().await =
                    Some(parish_core::inference::InferenceQueue::new(itx, btx, xtx));

                let ctx = GameLoopContext {
                    world: &world,
                    npc_manager: &npc_manager,
                    config: &config,
                    conversation: &conversation,
                    inference_queue: &inference_queue,
                    emitter: Arc::clone(&dyn_emitter),
                    inference_config: &inference_config,
                    pronunciations: &pronunciations,
                    client: &client,
                    cloud_client: &cloud_client,
                    language,
                    inference_failure_messages: &failure_messages,
                    idle_messages: &idle_messages,
                };
                let must_stage = {
                    let world = world.lock().await;
                    input_may_mutate_tasks(&world, text)
                };
                if must_stage {
                    let result = handle_staged_game_input_with_journal(
                        &ctx,
                        Vec::new(),
                        text.to_string(),
                        Vec::new(),
                        &transport,
                        &templates,
                        move |tasks| async move {
                            crate::testing::persist_task_mutation_batch(
                                db_sync,
                                active_branch_id,
                                &tasks,
                            )
                            .map_err(parish_core::error::ParishError::Database)
                        },
                    )
                    .await;
                    if let Err(error) = result {
                        dyn_emitter.emit_event(
                            "text-log",
                            serde_json::to_value(parish_core::ipc::text_log(
                                "system",
                                format!("Failed to persist player task changes: {error}"),
                            ))
                            .unwrap_or(serde_json::Value::Null),
                        );
                    }
                } else {
                    handle_game_input(
                        &ctx,
                        text.to_string(),
                        Vec::new(),
                        &transport,
                        &templates,
                        || None,
                    )
                    .await;
                }

                // Tear down the worker: drop the queue (closing the sender side)
                // and abort the spawned task so it never outlives the call.
                *inference_queue.lock().await = None;
                worker.abort();
            });
        }));

        // Move mutated state back into the app, and fold config changes back.
        let new_config = config.into_inner();
        self.app.world = world.into_inner();
        self.app.npc_manager = npc_manager.into_inner();
        self.app.apply_config(&new_config);

        if let Err(payload) = outcome {
            std::panic::resume_unwind(payload);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::testing::{ActionResult, GameTestHarness};

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

    /// C7 — with shadow mode off, `execute` writes no ledger and behaves
    /// exactly as the legacy path. Shadow is force-disabled here so the test is
    /// independent of the ambient `PARISH_HARNESS_SHADOW` env (the corpus CI
    /// run sets it on process-wide).
    #[test]
    fn shadow_disabled_writes_no_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let ledger = dir.path().join("ledger.jsonl");

        let mut h = GameTestHarness::new();
        h.shadow_enabled = false;
        h.shadow_ledger = ledger.clone();
        assert!(!h.shadow_enabled());

        let before = h.player_location().to_string();
        let _ = h.execute("look");
        // Legacy behaviour intact (look doesn't move the player).
        assert_eq!(h.player_location(), before);
        assert!(
            !ledger.exists(),
            "disabled shadow must not write a ledger file"
        );
    }

    /// C5 — with shadow mode on, `execute` runs both engines without disturbing
    /// the legacy result, and any ledger it writes contains only well-formed
    /// divergence records for the executed input.
    #[test]
    fn shadow_enabled_runs_both_paths_and_logs_valid_records() {
        use crate::shadow::DivergenceRecord;

        let dir = tempfile::tempdir().unwrap();
        let ledger = dir.path().join("ledger.jsonl");

        let mut h = GameTestHarness::new();
        h.enable_shadow(ledger.clone(), "real_loop_it");
        assert!(h.shadow_enabled());

        let start = h.player_location().to_string();
        // A movement command: legacy result must be unaffected by the shadow run.
        let result = h.execute("look");
        assert!(matches!(result, ActionResult::Looked { .. }));
        // Legacy state preserved (shadow rolled back its own real-loop mutation).
        assert_eq!(h.player_location(), start);

        // If a ledger was written, every line is a valid record for this input.
        if ledger.exists() {
            let contents = std::fs::read_to_string(&ledger).unwrap();
            for line in contents.lines() {
                let rec: DivergenceRecord = serde_json::from_str(line)
                    .unwrap_or_else(|e| panic!("ledger line not a DivergenceRecord: {e}: {line}"));
                assert_eq!(rec.case, "real_loop_it");
                assert_eq!(rec.input, "look");
                assert_ne!(rec.old, rec.new, "a record implies the forms differ");
            }
        }
    }
}
