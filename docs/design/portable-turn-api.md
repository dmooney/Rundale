# Design: portable turn API (convergence Stage 2)

> Status: Accepted · Plan: [mobile engine convergence](../plans/mobile-engine-convergence.md) Stage 2 ·
> Decision: [ADR-025](../adr/025-mobile-runtime-on-shared-engine.md) §1, §2

This document is the inventory and design gate for Stage 2: the API types, the
request lifecycle, and the PR sequence. It was approved in review; decisions are
recorded in §9.

## 1. Goal and exit criteria

One turn pipeline for every runtime. Submitting player input returns committed
events, an inference request, or a clarification. The host resumes an inference
request with a result, a failure, or Stop. Desktop drives the same API and
fulfils inference requests in-process. The API and its lifecycle build under the
`mobile` feature.

Exit (plan Stage 2):

- New request-lifecycle tests pass headless with a scripted inference host.
- Desktop tests and the harness walkthrough are unchanged.
- `limerick-engine --script` output is unchanged for existing fixtures
  (compared against a `main` build, excluding wall-clock-seeded ambient lines).
- CI job `rust-mobile-build` stays green, and covers the new API.

Out of scope: the save system (request and transcript tables are Stage 3),
prompts as mod files (Stage 4), `mobile/`, `endpoints/`, the FFI, and `ios-port`.

## 2. Inventory

### 2.1 Turn orchestration on `main`

All code below is in `limerick-core/src/game_loop/` unless noted.

| Function                                               | Role                                                                                                                   | Inference inside the turn                                                                                                                                                                                         | Authoritative mutations                                                                                                               |
| ------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `input::handle_game_input`                             | Entry for free-form text: echo, intent, atmosphere cue, route to move/look/examine/interact/dialogue                   | **Intent** (`parse_intent_with_profile_and_audit`, direct `AnyClient`, local parser first)                                                                                                                        | `record_player_input`, `clock.inference_pause/resume`                                                                                 |
| `input::try_handle_move` → `movement::handle_movement` | Full desktop travel: `apply_movement` (weather-aware), encounter roll, arrival reactions, world update                 | **Travel encounter** (`enrich_travel_encounter_with_profile_and_audit`, 15 s, canned fallback); **arrival reactions** (`stream_reaction_texts_with_profile`, one streamed call per reacting NPC, canned fallback) | `apply_movement` (location, clock, tiers, `PlayerMoved`), encounter line in `text_log`, conversation location sync                    |
| `input::handle_interact`                               | Narrated physical action                                                                                               | none                                                                                                                                                                                                              | `apply_player_action` (task progress)                                                                                                 |
| `npc_turn::handle_npc_conversation`                    | Addressee resolution, absent-NPC lines, serialization claim (#1379), Phase 1 addressed turns, Phase 2 autonomous chain | per speaker via `run_npc_turn`                                                                                                                                                                                    | transcript lines, `AddressedAbsentNpc`, `conversation_in_progress`, clock pause                                                       |
| `npc_turn::run_npc_turn`                               | Prompt setup, one Tier-1 dialogue request, canonical apply, presentation                                               | **Dialogue** via `InferenceQueue` (interactive priority, tokens drained and quarantined, 30 s timeout)                                                                                                            | `detect_and_record_player_name`, referent context, `apply_npc_dialogue_turn_with_validation` (log, memories, task assignment, events) |
| `npc_turn::run_idle_banter`                            | Autonomous chatter on inactivity                                                                                       | dialogue via `run_npc_turn`                                                                                                                                                                                       | as above                                                                                                                              |
| `staged_turn::handle_staged_game_input_with_journal`   | Runs `handle_game_input` on a cloned candidate, journals the task batch, installs, then publishes                      | as wrapped                                                                                                                                                                                                        | candidate install under lock order                                                                                                    |
| `reactions::emit_npc_reactions`                        | Post-turn emoji/line reactions to the player's message (background, fire-and-forget)                                   | **Reaction** (outside the turn)                                                                                                                                                                                   | reaction records via persistence callback                                                                                             |
| `world_pump::advance_world` and tier 2/3/4 dispatch    | Background simulation                                                                                                  | **Simulation** (outside the turn)                                                                                                                                                                                 | world, NPCs, gossip                                                                                                                   |

Addressee resolution is spread over `ipc::handlers::{extract_npc_mentions,
resolve_npc_targets, resolve_addressed_targets}`, `input::explicit_talk_recipient_clause`,
and `NpcManager::{find_by_name, find_by_role_at}`. Both lookups return `None`
for an ambiguous match, so an ambiguous explicit addressee is reported as
"X is not here." today. That is the gap clarification fills.

### 2.2 Entry points

| Entry point                         | Path to the pipeline                                                                                                                                                                                                                                                           | Holds a turn gate                                                                                   |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------- |
| Server `POST /api/submit-input`     | `routes/input.rs::handle_game_input`: staged path when `input_may_mutate_tasks`, otherwise live path with loading animation and before/after `world-update`; then `emit_npc_reactions`                                                                                         | `persistence_gate` for the whole request                                                            |
| Tauri `submit_input` and MCP bridge | `commands/input.rs::do_submit_input_locked`, same staged/live fork as the server                                                                                                                                                                                               | `persistence_gate` (world tick, inactivity tick, reactions, saves, and editor reloads also take it) |
| Headless REPL (`limerick-engine`)   | `headless.rs::handle_headless_game_input`: **a second pipeline**. Own intent call, own movement (`handle_headless_movement`), own dialogue (`stream_headless_npc_dialogue`, `apply_npc_response`), own `@mention` parsing. Only task-bearing turns use the shared staged path. | `&mut App` borrow                                                                                   |
| Script harness (`--script`)         | `testing.rs::GameTestHarness::execute`: legacy local router, `parse_intent_local` only, canned NPC responses. Not `game_loop`.                                                                                                                                                 | n/a                                                                                                 |
| Real-loop harness                   | `real_loop.rs::execute_via_real_loop`: `handle_game_input` / staged path with a mock client; `shadow.rs` compares it with the legacy router                                                                                                                                    | n/a                                                                                                 |
| `limerick-client`, `limerick-mcp`   | HTTP clients of the server or Tauri bridge; no pipeline of their own                                                                                                                                                                                                           | n/a                                                                                                 |

The headless REPL is the only `deterministic_capability`-style duplicate on
`main` (tracked in #2023, fixed by PR 8). The script harness's router is also a duplicate, but its output is a
Stage 2 invariant; see §7.

### 2.3 Mobile build blockers (measured)

Removing the `desktop` gates on `game_loop`, `game_session`, and `ipc` and
running `cargo check`/`clippy -D warnings` with `--no-default-features
--features mobile` on `0fd0ec5e1` fails only on:

- `ipc::editor` (uses `limerick_editor`) and `ipc::bug_report` (uses
  `limerick_diagnostics`);
- `GameConfig::{vllm_mlx_extra_slots, vllm_extra_slots}` (use the desktop-only
  `inference::client` slot types) and the `InferenceCategoryConfig` impl for
  `limerick_diagnostics`;
- one import in `game_session.rs` that reaches `InferenceLogEntry` through the
  diagnostics re-export instead of `limerick_inference`.

Gating those five items (and the matching re-exports) is enough for a clean
mobile `check` and `clippy`. Nothing needs to be duplicated or moved to a new
crate.

Decisions (review of this document):

- `ipc::editor` stays desktop-only; the Designer does not ship on mobile.
- `ipc::bug_report` stays desktop-only in Stage 2. Mobile bug reporting (shared
  report composition, a delivery sink that keeps the GitHub token off the
  device) is tracked in #2022.
- The vLLM slot helpers and the diagnostics trait impl are desktop-only by
  nature (local model processes, debug panel). The `InferenceLogEntry` import is
  corrected to `limerick_inference`, not gated.

### 2.4 Oracle on `origin/ios-port`

Behaviour to port as tests (not code):

- `limerick-core/src/mobile/mod.rs` tests: `acceptance_precedes_endpoint_and_has_grounding`,
  `stop_wins_and_late_candidate_cannot_commit`, `accepted_restart_becomes_interrupted_and_does_not_rerun`,
  `successful_candidate_commits_one_exchange_and_retry_is_rejected`,
  `failed_attempt_can_retry_with_new_attempt_identity`,
  `correlated_endpoint_failure_is_terminal_and_retryable`,
  `endpoint_failure_requires_the_invocation_base_revision`,
  `cumulative_frames_are_bounded_and_duplicate_frames_are_ignored`,
  `phase3_ambiguity_survives_resume_and_selection_continues_original_request`,
  `phase3_explicit_full_name_tag_selects_one_person_without_clarification`,
  `explicit_absent_addressee_does_not_fall_back_to_present_npc`,
  `leading_vocative_still_selects_explicit_present_npc`,
  `dialogue_body_mention_does_not_address_absent_npc`,
  `phase3_natural_travel_commits_once_and_updates_scene_and_schedule`,
  `sqlite_success_events_have_distinct_durable_sequences`,
  `sqlite_failure_and_retry_events_remain_durable_and_monotonic`.
- `limerick-persistence/src/mobile/mod.rs` journal contract (against the Stage 2
  in-memory journal; the SQLite versions are Stage 3):
  `duplicate_events_are_idempotent_but_conflicting_payload_is_rejected`,
  `stale_generation_has_no_side_effects`,
  `sqlite_failure_rolls_back_generation_state_request_and_event_together`.
- `limerick-mobile-ffi` test `unresolved_clarification_survives_bounded_restart_projection`.
- RundaleKit `SessionReducer` tests (`testCompletionCommitsAndLateOldAttemptCannotWin`,
  `testRetryCreatesCurrentAttemptBeforeRejectingOldAttemptEvents`,
  `testLateCallbacksFromCancelledAttemptCannotCommit`,
  `testFailedCompletionCannotAdvanceCommittedStateRevision`,
  `testFixtureClarificationContinuesSameLogicalRequest`,
  `testRestoredAdapterSuppressesOpeningReplayAndInterruptsActiveAttempt`) and the
  `scene-changed.json` fixture. These test the Swift reducer, so the port asserts
  the engine-side contract they rely on: after an attempt's terminal event no
  event carries that attempt; a retry's progress event precedes any event of the
  new attempt; only a `succeeded` terminal carries a committed state revision;
  sequences are strictly increasing; the command item id is stable across
  attempts; clarification selection continues the same logical request.

Deliberate divergences from the oracle:

- Provisional stream text is not shown as dialogue. `ios-port` displayed raw
  frames; Rule 33 forbids publishing candidate text before the canonical apply
  validator accepts it. Frames are accepted for liveness and sequencing only.
- No single-location `WorldGraph` exception, no content bundle, no
  `DefaultHasher` fingerprints, no second parser or resolver.
- Slash-command handling (`deterministic_capability`) is not ported; see §9 Q3.

## 3. Shape of the design

**Host-yield inference over the existing async pipeline.** The orchestration in
§2.1 keeps its async shape. Every inference call inside a turn goes through one
seam, `TurnInference`. A `TurnEngine` runs each execution attempt as a task
against an isolated candidate copy of the session state. When the attempt asks
for inference, the engine suspends it and returns the request to the host. The
host resumes it with an outcome. When the attempt finishes, the engine commits
the candidate atomically or discards it.

Desktop calls the same `submit` / `resume` API and fulfils each request with an
in-process adapter that reproduces today's provider calls. Mobile (Stage 5)
fulfils requests through Limerick Endpoints. One pipeline, one lifecycle; only
the fulfiller differs.

Rejected alternative: rewrite the turn as an explicit resumable state machine
(ios-port's shape). Every call site in §2.1 would become a hand-written
continuation (intent, per-NPC dialogue, the autonomous chain, encounter
enrichment, N arrival reactions), about 3,000 lines of desktop orchestration
rewritten at once. That maximizes risk to the desktop invariance requirement and
duplicates control flow the async code already expresses. The cost of the chosen
shape is that an executing attempt cannot survive process death; the product
spec already requires such an attempt to become `Interrupted`, not to resume.

## 4. API

Module `limerick_core::turn` (portable; builds under `mobile`). Sketches below
omit derives and docs; all public types are `Serialize + Deserialize`.

### 4.1 Identities

```rust
pub struct LogicalRequestId(String);   // host-supplied or engine-minted (UUID v4)
pub struct ExecutionAttemptId(String); // engine-minted per attempt (UUID v4)
pub struct InferenceCallId(String);    // "{attempt}#{n}", n = call ordinal in the attempt
pub struct TranscriptItemId(String);   // "{request}:command", "{attempt}:{ordinal}"
pub struct TranscriptEventId(String);  // "{request}:{attempt|-}:{ordinal}", deterministic
pub struct EventSequence(u64);         // session-monotonic, assigned at durable append
pub struct StateRevision(u64);         // +1 per committed authoritative change
```

Deterministic ids make redelivery and replay idempotent: the same attempt output
always has the same event and item ids, so a journal append of an existing id
with an identical payload is a no-op and with a different payload is an error.

### 4.2 Entry points

```rust
impl TurnEngine {
    pub async fn submit(&mut self, live: &GameLoopContext<'_>, input: TurnInput) -> Result<TurnStep, TurnError>;
    pub async fn resume(&mut self, live: &GameLoopContext<'_>, resolution: InferenceResolution) -> Result<TurnStep, TurnError>;
    pub async fn stop(&mut self, live: &GameLoopContext<'_>, attempt: &ExecutionAttemptId) -> Result<TurnStep, TurnError>;
    pub async fn retry(&mut self, live: &GameLoopContext<'_>, request: &LogicalRequestId) -> Result<TurnStep, TurnError>;
    pub async fn answer_clarification(&mut self, live: &GameLoopContext<'_>, request: &LogicalRequestId, choice: &str) -> Result<TurnStep, TurnError>;
    pub async fn recover(&mut self) -> Result<Vec<TranscriptEvent>, TurnError>; // after restart
}

pub struct TurnInput {
    pub request_id: Option<LogicalRequestId>,
    pub text: String,
    pub addressed_to: Vec<String>,  // chip selections, as today
    pub draft_id: Option<String>,   // echoed on the accepted command event
}

pub struct TurnStep {
    pub request_id: LogicalRequestId,
    pub attempt_id: Option<ExecutionAttemptId>,
    pub events: Vec<TranscriptEvent>,           // durable, committed or terminal
    pub emissions: Vec<(String, serde_json::Value)>, // existing wire events, released now
    pub status: TurnStatus,
}

pub enum TurnStatus {
    AwaitingInference(InferenceCall),
    AwaitingClarification(ClarificationPrompt),
    Completed { outcome: TerminalOutcome, revision: Option<StateRevision> },
    Ignored(IgnoredReason), // stale, late, or duplicate callback: no effects
}

pub enum TerminalOutcome { Succeeded, Cancelled, Failed, Interrupted }
```

`live` is the runtime's existing `GameLoopContext`, borrowed per call, so the
engine never needs `'static` access to runtime state. Commit installs into it.

### 4.3 Inference seam

```rust
pub struct InferenceCall {
    pub id: InferenceCallId,
    pub request_id: LogicalRequestId,
    pub attempt_id: ExecutionAttemptId,
    pub base_revision: StateRevision,
    pub subrole: InferenceSubrole,        // Intent | Dialogue | TravelEncounter | ArrivalReaction
    pub model: String,
    pub system: Option<String>,
    pub prompt: String,
    pub generation: GenerationSettings,   // max tokens, temperature, penalties, thinking, profile
    pub response_format: ResponseShape,   // Text | JsonObject | JsonSchema(..)
}

pub struct InferenceResolution {
    pub call_id: InferenceCallId,
    pub attempt_id: ExecutionAttemptId,
    pub base_revision: StateRevision,
    pub outcome: InferenceOutcome,
}

pub enum InferenceOutcome {
    Completed { text: String, metadata: ProviderMetadata },
    Failed { kind: InferenceFailureKind, message: String },
}

pub enum InferenceFailureKind { Transport, Protocol, Truncated, TimedOut, Interrupted }
```

Responsibility split, per Rules 33 and 37:

- The host validates transport and termination: a non-success finish reason
  (for example `length`) is `Failed { Truncated }`, never `Completed`.
- The engine validates meaning: intent output through
  `intent_from_structured_output` / `validated_intent`, dialogue through
  `parse_npc_stream_response_with_disposition` and
  `apply_npc_dialogue_turn_with_validation`. The host never applies semantic
  guards and never publishes candidate text.

Inside the pipeline, call sites use `ctx.inference.complete(call).await`, where
`ctx.inference: Arc<dyn TurnInference>` is a new `GameLoopContext` field. Two
implementations:

- `HostYield` (portable): used by `TurnEngine`. `complete` sends the call to the
  engine and awaits a oneshot; the engine returns `AwaitingInference` to the
  host.
- `InProcessInference` (desktop, lives next to `InferenceQueue`): performs
  today's exact calls. Dialogue goes through the queue at interactive priority
  with the token channel drained and discarded, and the
  `inference-response-timeout` flag moves here unchanged. Intent, encounter, and
  reaction calls use `generate_detailed_with_format` with today's parameters and
  `DirectInferenceAudit` records. Desktop hosts drive the engine with a helper,
  `drive_in_process(engine, live, input, &InProcessInference)`, that loops
  `AwaitingInference` → `resume`.

Stage 4 adds an Endpoint reference (role name and version) and structured inputs
to `InferenceCall`, so an Endpoint host can execute the published definition.
Stage 2 carries the rendered prompt, which is what desktop needs.

Per-role failure policy is unchanged from desktop and shared by all hosts:

| Subrole                     | On `Failed`                                        | Attempt result                             |
| --------------------------- | -------------------------------------------------- | ------------------------------------------ |
| Intent                      | classification is `Unknown` (today's fallback)     | continues                                  |
| TravelEncounter             | canned encounter text                              | continues                                  |
| ArrivalReaction             | canned reaction / empty-placeholder finalisation   | continues                                  |
| Dialogue (player-initiated) | `DIALOGUE_RETRY_MESSAGE`, `stream-turn-end` failed | attempt ends `Failed`, candidate discarded |
| Dialogue (autonomous chain) | chain stops (today's `break`)                      | continues                                  |

### 4.4 Transcript events

```rust
pub struct TranscriptEvent {
    pub id: TranscriptEventId,
    pub sequence: EventSequence,
    pub request_id: Option<LogicalRequestId>,
    pub attempt_id: Option<ExecutionAttemptId>,
    pub item_id: Option<TranscriptItemId>,
    pub kind: TranscriptEventKind,
    pub speaker: Option<String>,
    pub content: Option<String>,
    pub terminal_outcome: Option<TerminalOutcome>,
    pub state_revision: Option<StateRevision>, // only on a Succeeded terminal
    pub clarification: Option<ClarificationPrompt>,
    pub metadata: BTreeMap<String, String>,
}

pub enum TranscriptEventKind {
    PlayerCommand, Narration, NpcDialogue, ActionResult, SceneChanged,
    ClarificationRequired, ClarificationSelected, Progress, Error, ResponseCompleted,
}
```

Transcript events are projected from the committed wire emissions by one
function, `project_emissions`. `text-log` payloads map by source and subtype;
a completed `stream-turn-end` becomes `NpcDialogue`; a `world-update` with a new
location becomes `SceneChanged`. A test enumerates every event name the
pipeline can emit and requires each to be either mapped or listed as
presentation-only (`stream-token`, `travel-start`, `dialogue-quality`, and
similar), so a new emission cannot silently bypass the transcript. The wire
emissions themselves are unchanged, so existing desktop UIs keep working.

## 5. Lifecycle

### 5.1 State machine

| From                                   | Event                                          | To                                                          | Effects                                                                                                                                     |
| -------------------------------------- | ---------------------------------------------- | ----------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| (none)                                 | `submit`, no other request open                | `Accepted`                                                  | request record + `PlayerCommand` event durably journaled **before** any interpretation or inference                                         |
| (none)                                 | `submit` while a request is open               | rejected `RequestInProgress`                                | none                                                                                                                                        |
| (none)                                 | `submit` with an existing request id           | rejected `AlreadyCommitted` / `NotRetryable`                | none                                                                                                                                        |
| `Accepted`                             | attempt starts                                 | `Executing`                                                 | candidate cloned from live state; `base_revision` captured                                                                                  |
| `Executing`                            | pipeline calls inference                       | `Executing` (awaiting call n)                               | `AwaitingInference` returned                                                                                                                |
| `Executing`                            | `resume` matching attempt, call, revision      | `Executing`                                                 | pipeline continues                                                                                                                          |
| `Executing`                            | pipeline finds an ambiguous explicit addressee | `AwaitingClarification`                                     | candidate discarded; prompt and resolved intent journaled; `ClarificationRequired` event                                                    |
| `AwaitingClarification`                | `answer_clarification` with a known choice     | `Executing` (same attempt)                                  | `ClarificationSelected`; pipeline reruns from the original input with the chosen addressee and the journaled intent (no second intent call) |
| `AwaitingClarification`                | chosen NPC no longer present                   | `Completed` (`Succeeded`)                                   | "X is no longer here." narration; no other change                                                                                           |
| `Executing`                            | pipeline finishes                              | `Completed` (`Succeeded`)                                   | one atomic journal commit, then candidate install, then event-bus publish, then emission release; revision +1 if state changed              |
| `Executing`                            | player-initiated dialogue fails                | `Failed`                                                    | candidate discarded; `Error` + `ResponseCompleted(failed)` journaled                                                                        |
| `Executing`                            | `stop(current attempt)`                        | `Cancelled`                                                 | attempt task aborted; candidate discarded; terminal events journaled                                                                        |
| `Executing`                            | journal commit fails                           | `Executing` rolled back to not-committed, returned as error | nothing installed or published (today's staged-turn guarantee)                                                                              |
| `Executing` / `Accepted`               | process restart (`recover`)                    | `Interrupted`                                               | "The previous response was interrupted; you can retry it." + terminal; never re-run automatically                                           |
| `Failed` / `Cancelled` / `Interrupted` | `retry`                                        | `Executing` (new attempt id)                                | `Progress` (retry) event precedes any new-attempt event                                                                                     |
| `Completed(Succeeded)`                 | `retry`                                        | rejected `AlreadyCommitted`                                 | none                                                                                                                                        |
| any terminal                           | `resume` / `stop` / frame for that attempt     | unchanged                                                   | `Ignored`; no event, no state change                                                                                                        |
| any                                    | `resume` with wrong call id or base revision   | unchanged                                                   | `Ignored`                                                                                                                                   |

`AwaitingClarification` survives restart; `Accepted` and `Executing` become
`Interrupted`. At most one request is open (`Accepted`, `Executing`, or
`AwaitingClarification`) per session. Submitting new input while a
clarification is pending cancels the pending request first (terminal
`Cancelled`), so the player is never blocked by an unanswered question.

### 5.2 Candidate isolation and commit

Every attempt runs on a candidate: `clone_for_staged_turn` of the world, a clone
of `NpcManager` and `ConversationRuntimeState`, and a deferred inference-audit
sink. This generalizes today's staged path from task-bearing turns to all turns,
and `handle_staged_game_input*` becomes the engine's commit step instead of a
second entry point. Consequences:

- Stop, failure, clarification, and interruption have zero authoritative
  effects, including pre-inference mutations such as `record_player_input`,
  player-name detection, and transcript pushes.
- The commit is atomic: task batch, request terminal, and transcript events go
  to the journal in one call; only after it succeeds is the candidate installed
  (transplanting the live event bus), semantic events published, the audit
  record revealed, and emissions released.
- Concurrency is unchanged: server and Tauri already hold `persistence_gate`
  across the whole turn, and every other live-state mutator takes the same
  gate; the headless REPL is serialized by `&mut App`. PR 5 confirms that
  tier-2/3 result application takes the gate too.
- Cost: one world and NPC clone per turn. Task-bearing turns already pay it. PR 5
  measures the clone for `mods/rundale` and records the number in the PR.

### 5.3 Presentation timing

- Acceptance releases the player's echo immediately (the existing prelude
  emission and `PlayerCommand` event), after the acceptance is journaled.
- Progress is live: the runtime's loading animation (`spawn_loading`) and the
  before-turn `world-update` still fire during inference.
- Everything else is released at commit, in the order produced. Today's
  task-bearing turns already behave this way on both desktop UIs.

## 6. Durability seam

```rust
pub trait TurnJournal: Send + Sync {
    fn accept(&self, record: &RequestRecord, events: &[TranscriptEvent]) -> BoxFuture<'_, Result<(), JournalError>>;
    fn begin_attempt(&self, record: &RequestRecord, events: &[TranscriptEvent]) -> BoxFuture<'_, Result<(), JournalError>>;
    fn await_clarification(&self, record: &RequestRecord, events: &[TranscriptEvent]) -> BoxFuture<'_, Result<(), JournalError>>;
    fn commit(&self, commit: &TurnCommit) -> BoxFuture<'_, Result<(), JournalError>>;
    fn finish_uncommitted(&self, record: &RequestRecord, events: &[TranscriptEvent]) -> BoxFuture<'_, Result<(), JournalError>>;
    fn open_requests(&self) -> BoxFuture<'_, Result<Vec<RequestRecord>, JournalError>>;
    fn next_sequence(&self) -> BoxFuture<'_, Result<EventSequence, JournalError>>;
}

pub struct TurnCommit {
    pub record: RequestRecord,            // terminal Succeeded, committed revision
    pub events: Vec<TranscriptEvent>,
    pub task_mutations: Vec<PlayerTask>,  // today's durable per-turn state
}
```

Contract (tested against every implementation): each call is atomic; an event id
already present with an identical payload is a no-op, with a different payload
is an error; a failed call leaves no partial write; sequences are strictly
increasing.

Stage 2 implementations:

- `MemoryTurnJournal`: complete contract in memory, with fault injection for
  tests.
- `SessionStoreTurnJournal` (desktop): `commit` appends the task batch through
  the existing `append_task_mutations` in the same way the staged path does
  today; request records and transcript events are held in memory. Desktop
  acceptance is therefore **not** durable across a crash in Stage 2, which is
  no worse than today (desktop has no request records at all).

What Stage 3 must supply:

1. `requests` and `transcript_events` tables in the existing
   `limerick-persistence` database (with a migration), keyed by the ids above,
   with a unique event id index and a durable sequence counter.
2. One SQLite transaction for `commit` covering the request terminal, the
   transcript events, the task batch, and the authoritative state delta
   (journal entry or snapshot), so desktop autosave and turn commit cannot
   disagree.
3. Durable `accept`, `begin_attempt`, `await_clarification`, and
   `finish_uncommitted`, so `recover` sees open requests after a crash.
4. Forward compatibility per ADR-025 §4: unknown `TranscriptEventKind` values
   are preserved verbatim and rendered as a fallback line (the Stage 2 enum
   gets an `Unknown { raw }` arm and a round-trip test so Stage 3 does not have
   to change the type).
5. The save format version bump and prior-format fixtures.

## 7. Desktop invariance

Unchanged by design:

- The dialogue validation guards. Their fragility is tracked separately in
  #2024 and is out of Stage 2 scope.

- Prompts, generation parameters, audit records, timeouts, guards, and apply
  logic (the in-process adapter makes the same calls; PR 3 proves request
  equality with a recording client).
- The wire emission names and payloads, and the ordering within a turn.
- `limerick-engine --script` output: `GameTestHarness::execute` stays on its
  legacy router, which does not use `game_loop`. `shadow.rs` remains the tool
  for tracking its convergence with the real loop.
- The harness walkthrough (`just verify`), which runs `--script` fixtures.

Accepted consequences. The project is reset around mobile: desktop must keep
working and its gates stay green, but it gets no UI feature work. These desktop
changes fall out of the shared engine and are accepted without desktop UI
changes; each PR records before/after evidence:

1. **Failed dialogue commits nothing.** Today a failed player-initiated dialogue
   turn keeps its pre-inference mutations (the player's transcript line,
   recorded input, a detected player name) and, in multi-NPC turns, earlier
   speakers' lines. Under the lifecycle the whole attempt is `Failed` and
   retryable. Required so a retry cannot duplicate a line.
2. **Committed output is released at commit.** In a multi-addressee or
   autonomous-chain turn, the first NPC's line appears when the whole turn
   commits instead of when that NPC finishes. Single-NPC turns look the same,
   because the canonical line was already released only after the apply
   validator.
3. **Arrival reactions are released as whole lines.** Today they stream raw
   provider tokens to the UI, which Rule 33 forbids for other dialogue. Canned
   fallbacks and per-turn `stream-turn-end` events are kept.
4. **An ambiguous explicit addressee asks which person**, instead of reporting
   "X is not here." (flag `addressee-clarification`, default on, per Rule 6).
   Desktop shows the question and choices as a system line; the next typed
   input cancels the pending request (§5.1). No desktop UI work.
5. **The headless REPL uses the shared pipeline.** Its output becomes the
   staged renderer's (`committed_headless_lines`), which task-bearing turns
   already use. It gains full travel, encounters, arrival reactions, guards,
   and the shared resolver, which it lacks today.

## 8. PR sequence

Each PR starts from current `origin/main` in its own worktree and carries
`cargo fmt --check`, `clippy -D warnings` (default and mobile), `cargo test
--workspace`, `just verify`, a `--script` diff against `main` for every existing
fixture, and a proof bundle per [agent-check](../agent/agent-check.md). Live runs
on the inference path use a local scripted OpenAI-compatible server (real HTTP,
canned replies, disclosed in the evidence).

| #   | Title                                                                | Content                                                                                                                                                                                                                                                                                                            | Proof                                                                                                    |
| --- | -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------- |
| 0   | `docs(design): portable turn API inventory and design`               | This document.                                                                                                                                                                                                                                                                                                     | docs checks only                                                                                         |
| 1   | `build(core): compile the game loop under the mobile feature`        | The five gates in §2.3; `game_loop`, `game_session`, `ipc` no longer desktop-only; CI mobile job adds `clippy -D warnings` and `cargo test -p limerick-core --lib` for the mobile configuration. No behaviour change.                                                                                              | headless live run with scripted server; `--script` diff                                                  |
| 2   | `refactor(npc): one addressee resolver with an ambiguity result`     | `NpcManager::resolve_reference` returns `Unique`, `Ambiguous(ids)`, or `NotFound` (names first, role vocative fallback kept); `find_by_name` / `find_by_role_at` and the `ipc` resolvers become wrappers. Ambiguous still maps to today's handling.                                                                | resolver unit tests; real-loop test                                                                      |
| 3   | `refactor(core): route turn inference through a TurnInference seam`  | `InferenceCall` / `InferenceOutcome`, `ctx.inference`, `InProcessInference`; intent, dialogue, encounter, and arrival-reaction calls use it. Behaviour-preserving, including reaction streaming.                                                                                                                   | recording-client equality tests (prompt, system, params, audit per subrole); scripted-server live run    |
| 4   | `feat(core): request lifecycle types and journal contract`           | `turn::{ids, RequestRecord, phases, TranscriptEvent, TurnJournal, MemoryTurnJournal, project_emissions}`; pure state-machine functions; journal contract tests; emission-coverage test. Not wired to runtimes.                                                                                                     | unit and contract tests (ported persistence oracle); mobile check                                        |
| 5   | `feat(core): TurnEngine with host-yield inference and staged commit` | `TurnEngine`, `HostYield`, `drive_in_process`; universal candidate staging; gate-participation audit; clone cost measured. Lifecycle integration tests with a scripted host, ported from §2.4, including full travel with encounter and arrival reactions, Stop, late callbacks, retry, failure, restart recovery. | `turn_lifecycle` tests headless with scripted host; mobile `cargo test`                                  |
| 6   | `feat(core): clarify ambiguous addressees`                           | `Ambiguous` becomes `AwaitingClarification`; `answer_clarification`; clarification survives `recover`; flag `addressee-clarification`. A leading name or role vocative in free text ("Widow, any news?") is passed to the resolver (today it is answered by whoever is first).                                     | ported clarification tests; real-loop test                                                               |
| 7   | `refactor(server,tauri): submit input through the TurnEngine`        | Server and Tauri (including the MCP bridge) call `drive_in_process`; the staged/live fork is removed; `execute_via_real_loop` drives the engine. Intentional changes 1-3 land here.                                                                                                                                | server and Tauri bridge live runs against the scripted server, before/after transcripts; real-loop tests |
| 8   | `refactor(engine): headless REPL on the TurnEngine`                  | Delete `handle_headless_game_input`, `stream_headless_npc_dialogue`, `apply_npc_response`, `handle_headless_movement`, `print_arrival_reactions`, and the local `@mention` path. Intentional change 5.                                                                                                             | headless live run with scripted server, before/after                                                     |
| 9   | `docs: record the portable turn API`                                 | This document to Implemented; architecture, codebase map, plan status, LEARNINGS.                                                                                                                                                                                                                                  | docs checks                                                                                              |

PRs 2 and 4 are independent of each other and of 3; 5 needs 3 and 4; 6 needs 2
and 5; 7 needs 5 and 6; 8 needs 7.

## 9. Decisions

Decided in review:

- The design in §3-§6 is approved.
- `ipc::editor` stays desktop-only; mobile bug reporting is #2022; headless
  drift is #2023; guards stay unchanged, revisit in #2024.
- §7 consequences are accepted; desktop gets no UI work (mobile-first reset).
- The script harness stays on its legacy router (required for unchanged
  `--script` output).
- `/`-commands stay on the shared `handle_system_command` path and are not
  lifecycle requests in Stage 2. Mobile observation commands (`/look`, `/time`,
  `/weather`, `/map`) may join the lifecycle in Stage 5.
- Post-turn reactions, idle banter, and tier-2/3/4 simulation keep in-process
  inference; mobile runs without them until the background inference seam
  (plan item, #2025) lands.
- No iOS target build in CI; `rust-mobile-build` stays a host-target check.
