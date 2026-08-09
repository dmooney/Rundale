# Independent NPC Agents

> Status: Proposed | Task: `independent-npc-agents` | Updated: 2026-07-12
>
> Related: [Cognitive LOD](cognitive-lod.md),
> [Inference Pipeline](inference-pipeline.md),
> [ADR-002](../adr/002-cognitive-lod-tiers.md), and
> [Agent Planning](ai-techniques/04-agent-planning-and-tools.md)

## Decision summary

Give every NPC a persistent agenda and an individual game-time wake deadline,
but do not give every NPC an operating-system thread or permission to mutate the
world directly. A per-session scheduler wakes due NPCs, planners produce typed
intentions from immutable snapshots, and one shared reducer validates and
applies those intentions to canonical world state.

The existing cognitive tiers remain. They become a policy for how an NPC plans
when woken, rather than the source of the NPC's apparent agency:

- Tier 1 uses immediate, player-facing reasoning.
- Tier 2 uses lightweight scene reasoning for nearby due NPCs.
- Tier 3 batches planning for distant due NPCs.
- Tier 4 uses deterministic rules.

This gives NPCs different rhythms and continuing goals without creating one
unbounded Tokio task per NPC, nondeterministic world mutation, or an inference
request per character per tick.

## Player experience

Characters should appear to continue lives rather than update in synchronized
waves. A publican may decide to close up, a messenger may continue an errand,
and two neighbours may finish a disagreement while the player is elsewhere.
When the player arrives, the resulting location, activity, mood, memories, and
unfinished goal should agree with what happened off screen.

The player should not see "agent ticks." They should see consequences: an NPC
arrived, left, changed activity, shared news, remembered an encounter, or was
interrupted by the player's arrival.

## Goals

- Let each NPC act at a cadence derived from its own agenda, schedule, events,
  and relevance.
- Preserve cognitive LOD as the inference and fidelity budget.
- Keep one authoritative, deterministic mutation path for world and NPC state.
- Make save/load restore agent continuity without serializing tasks or futures.
- Preserve server, Tauri, and headless behavior parity through shared
  orchestration in `parish-core`.
- Bound work during long time jumps, crowded scenes, and inference outages.
- Make every accepted or rejected autonomous action observable in diagnostics.

## Non-goals

- One native thread, Tokio task, model context, or inference worker per NPC.
- Continuous wall-clock cognition while game time is paused.
- Giving model output direct access to mutable world state.
- Removing cognitive tiers or running full dialogue-quality inference for all
  NPCs.
- Replacing authored schedules, mortality, weather, or gossip rules in the
  first release.
- Simulating every minute skipped during a large time jump.

## Why not one thread per NPC?

A literal one-thread-per-NPC design makes the character feel independent in
code, but weakens the properties the game needs:

- Tasks are not saveable. Their meaningful state must be externalized anyway.
- Independent mutation races on locations, relationships, gossip, and death.
- Per-NPC polling multiplies idle work across every live server session.
- Inference remains globally scarce, so threads do not create useful model
  parallelism.
- Replays and harness fixtures become timing-dependent.
- Session eviction must discover and cancel a large task tree.

The unit of independence should therefore be persisted intention and wake time,
not ownership of an execution thread.

## Existing system

`NpcManager` owns all NPCs, tier assignments, and one last-tick/in-flight state
per background tier. `advance_world` is the shared synchronous pump for weather,
schedules, tier assignment, banshee processing, gossip, and Tier 4. Server and
Tauri poll in real time and dispatch Tier 2/3 inference outside locks. Player
input can cancel background inference.

This foundation already has the right resource controls. The missing piece is
per-character continuity: Tier 2/3 wake as cohorts, and schedule movement is a
global pass. An NPC does not currently carry "what I am trying to do next" or
"when I need to reconsider it."

## Architecture

```text
                         immutable snapshot
GameClock -> AgentScheduler --------------------> Planner
    ^              |                               |
    |              | due work                      | typed AgentIntent
    |              v                               v
world events -> wake queue                    IntentReducer
                                                    |
                                      validate, order, apply
                                                    v
                                      WorldState + NpcManager
                                                    |
                                      GameEvent + next wake
```

There is one `AgentScheduler` per game session. It owns a min-heap index of
`(next_wake_at, npc_id, generation)` entries. The authoritative agenda remains
on the NPC; heap entries are disposable indexes and are rebuilt after load.
Stale heap entries are ignored by comparing their generation with the current
agenda generation.

The flow has three explicit phases:

1. **Collect:** drain a bounded number of due NPCs, then capture immutable
   planning snapshots by acquiring and releasing the world and NPC locks in
   separate scopes. Never hold both locks at once; stamp copied facts with
   their revisions and mark work in flight.
2. **Plan:** release all game-state locks, then run deterministic rules or LLM
   inference. The output is a typed `AgentIntent`, never a mutation callback.
3. **Commit:** reacquire locks in the established order, validate the intent
   against current state, apply it through one reducer, emit semantic events,
   and schedule the NPC's next wake.

The collect and commit orchestration belongs in `parish-core`. Runtime crates
only provide lifecycle wiring, cancellation tokens, and event emission.

## Data model

### Persisted agent state

Add an `AgentAgenda` to `Npc`, with `Default` and `serde(default)` in
`NpcSnapshot` for old saves:

```rust
pub struct AgentAgenda {
    pub goal: Option<AgentGoal>,
    pub activity: AgentActivity,
    pub plan: VecDeque<PlanStep>,
    pub next_wake_at: DateTime<Utc>,
    pub last_wake_at: Option<DateTime<Utc>>,
    pub blocked_until: Option<DateTime<Utc>>,
    pub interruptibility: Interruptibility,
    pub generation: u64,
}
```

`AgentGoal` is structured: a bounded `GoalKind`, optional NPC/location target,
a short display-safe summary, creation/expiry times, and urgency. Free-form
model text may explain a goal but cannot define an executable operation.

`AgentActivity` describes what the NPC is currently doing and the expected end
time. It is useful for prompts and debug views, but it does not replace
`NpcState::Present`/transit or the authored schedule.

`PlanStep` contains only typed, schedulable actions. The initial vocabulary is:

- `FollowSchedule`
- `MoveTo(LocationId)`
- `Work { activity, until }`
- `Rest { until }`
- `ConverseWith(NpcId)`
- `ShareGossip { with, gossip_id }`
- `Observe`
- `ReconsiderAt(DateTime<Utc>)`

Transfers, appointments, economy, and other side effects require later typed
variants and reducer rules. They are not encoded as arbitrary tool strings.

### Ephemeral scheduler state

`NpcManager` owns:

```rust
pub struct AgentScheduler {
    due: BinaryHeap<Reverse<WakeKey>>,
    in_flight: HashMap<NpcId, InFlightAgentWork>,
    pending_wakes: VecDeque<AgentWake>,
}
```

This state is per session and is not serialized. On new game or restore, the
heap is rebuilt from all NPC agendas. `in_flight` is always cleared on load,
branch switch, shutdown, or cancellation.

### Intent envelope

```rust
pub struct AgentIntentEnvelope {
    pub actor: NpcId,
    pub intent: AgentIntent,
    pub source: PlannerSource,
    pub proposed_at: DateTime<Utc>,
    pub agenda_generation: u64,
    pub observed_world_revision: u64,
    pub preconditions: IntentPreconditions,
}
```

Preconditions include the actor's expected state and location plus any target
NPC/location assumptions. The reducer returns `Applied`, `Rejected`,
`Superseded`, or `Deferred`, each with a reason and next-wake decision.

`observed_world_revision` is diagnostic context, not a global compare-and-swap
guard. Unrelated world activity will normally advance it while inference is in
flight. Commit validation therefore checks the agenda generation and the
localized `IntentPreconditions`; it rejects only when facts the intent actually
depends on have changed.

LLM planners must deserialize the typed intent with `serde`, then validate the
schema and semantic bounds before commit. If a provider wraps the object in
prose, extraction scans for the first complete JSON object with a balanced
brace counter that understands quoted strings and escapes. It must not use a
`find('{')`/`rfind('}')` slice, which can absorb trailing commentary or a later
object. Missing, ambiguous, malformed, or out-of-bounds output becomes a
bounded parse failure and retry/defer result, never a partially trusted intent.

## Wake semantics

NPCs wake for either a deadline or an interruption:

- Their agenda reaches `next_wake_at`.
- An authored schedule boundary is reached.
- A relevant world event occurs at their location or concerns a known NPC.
- The player addresses them, arrives nearby, or changes their tier.
- A dependency becomes available, an intent is rejected, or inference ends.

Wake requests are coalesced per NPC. An earlier deadline wins; repeated events
add reasons to a bounded set rather than queueing repeated work.

All deadlines use game time. Pausing the game pauses agents. Real-time polling
only notices that game time has advanced; it is not itself simulation time.

### Time jumps and catch-up

A `/wait 480` must not replay 480 minute-level thoughts. When the clock jumps:

- Drain at most `max_agent_wakes_per_pump` work items.
- Collapse missed low-importance wakes into one catch-up snapshot covering the
  elapsed interval.
- Preserve hard boundaries such as death, travel completion, and the final
  authored schedule destination.
- Reschedule overflow fairly by `(overdue duration, urgency, npc_id)`.
- Emit counters for due, applied, deferred, collapsed, cancelled, and rejected
  work.

The cap is configured once in shared engine config, not separately in each
runtime.

## Cognitive LOD policy

The tiers decide planner cost and output detail after an NPC wakes:

| Tier   | Wake behavior                                                    | Planning path                              | Output                                                |
| ------ | ---------------------------------------------------------------- | ------------------------------------------ | ----------------------------------------------------- |
| Tier 1 | Immediate on player interaction or local event; no idle polling  | Full dialogue/reaction path                | Rich response plus typed intent when needed           |
| Tier 2 | Individual deadlines; due co-located NPCs coalesced into a scene | Short simulation prompt on Background lane | Mood, relationship, activity, conversation, next wake |
| Tier 3 | Individual deadlines collected into bounded batches              | Batch simulation on Batch lane             | Goal/activity summary, broad state delta, next wake   |
| Tier 4 | Sparse deadlines and event wakes                                 | CPU rules                                  | Schedule, life-event, need, or no-op intent           |

This changes cadence, not capacity. Tier 2 still avoids a wasteful solo scene
LLM call: a lone due NPC uses deterministic schedule/activity rules or is
deferred until a meaningful event. Tier 3 still batches requests, but each
batch result updates separate agendas and wake deadlines.

Tier changes do not discard the agenda. Inflation enriches the same goal and
activity; deflation compresses plan detail into the existing summary fields.

## Scenes and multi-agent conflicts

When several due NPCs share a location, the scheduler creates one `AgentScene`
rather than independent prompts. The scene planner can propose one intent per
participant. This preserves the current Tier 2 grouping benefit and prevents
two NPCs from independently inventing incompatible versions of the same
conversation.

Commit ordering is deterministic:

1. hard world rules and death/travel completion;
2. direct player interaction;
3. already-started activities;
4. urgency;
5. proposed game time;
6. stable NPC id tie-break.

Every intent is still validated immediately before application. If an earlier
intent invalidates a later one, the later intent is rejected or deferred and
the affected NPC wakes again with the rejection reason.

## Schedule relationship

Authored schedules remain the baseline obligation and source of hard time
boundaries. In the first release, scheduler-generated movement must produce the
same destinations and `GameEvent`s as `tick_schedules`; the old schedule pass
and the new intent path run in shadow comparison before ownership switches.

After parity is proven, an agenda may temporarily deviate from a schedule only
through an explicit reason with an expiry, such as illness, danger, a direct
errand, or an active player conversation. When the exception ends, the NPC
reconciles with the current schedule entry rather than replaying missed stops.

## Concurrency and ownership rules

- There is one scheduler per session, never a global scheduler.
- No world or NPC lock is held across inference or channel awaits.
- Async workers hold snapshots and cancellation tokens only.
- Workers submit data back to the reducer; they cannot call `NpcManager::get_mut`
  or publish gameplay events directly.
- Player input cancels lower-priority planning as it does today. A late result
  with a stale generation or failed localized precondition is rejected;
  unrelated changes to the global world revision are diagnostic only.
- Bounded queues apply backpressure. Saturation defers NPC work in game time;
  it does not spawn more workers.
- Task handles are owned by the session lifecycle and observe the existing
  shutdown token.

## Persistence and replay

Persist `AgentAgenda` in every `NpcSnapshot`. Do not persist the heap,
in-flight requests, channels, cancellation tokens, or wall-clock timestamps.

On restore:

1. Restore NPC agendas with backward-compatible defaults.
2. Clear ephemeral work.
3. Rebuild the heap from `next_wake_at`.
4. Seed missing agendas from authored schedule and current game time.
5. Drain overdue work through the normal bounded catch-up path.

An applied autonomous mutation must be represented by existing or new
`WorldEvent`/`GameEvent` variants so journal replay reaches the same canonical
state without rerunning inference. Intent proposal text is diagnostic data,
not replay input.

## Events and observability

Do not broadcast an event for every scheduler wake. Use an internal bounded
wake queue for scheduling and publish semantic game events only when something
meaningful happens.

Add diagnostics for:

- actor, tier, goal, activity, and next wake;
- planner source and latency;
- intent kind and outcome;
- rejection/defer reason;
- queue depth, overdue count, catch-up collapses, and work by tier;
- cancellation and stale-result counts.

Add `/debug agents [npc]`. Its stable text/JSON representation is the harness
signal for agenda continuity and independent deadlines. Player-facing prose
continues to come from the existing text log and semantic events.

## Runtime integration

`parish-core::game_loop::advance_world` remains the shared pump. It gains the
pure scheduling phase or calls an adjacent shared `advance_agent_scheduler`
helper after clock-dependent world rules and tier assignment. The helper
returns due work; it does not await inference.

A shared async dispatcher in `parish-core` handles planner execution and commit
through injected inference/event traits. Server and Tauri keep thin per-session
pollers that call this orchestration. Script/headless mode invokes the same
collect and reducer seams synchronously with deterministic planner stubs.

Existing Tier 2/3 dispatch loops remain during migration and are removed only
after shadow parity and live gameplay proof.

## Affected subsystems

- `parish-npc`: agenda, goals, intents, scheduler index, tier planner policy,
  scene grouping, and reducer-facing state transitions.
- `parish-core`: shared collect/dispatch/commit orchestration and world-pump
  integration.
- `parish-types`: semantic event variants only where existing events cannot
  represent an accepted action.
- `parish-inference` / `parish-providers`: existing priority and cancellation
  APIs; no provider-specific dependency in agent logic.
- `parish-persistence`: snapshot fields, backward-compatible defaults,
  restore/rebuild, and journal replay for new mutations.
- `parish-config`: bounded work and cadence knobs plus the feature flag.
- `parish-engine`: `/debug agents` harness rendering and deterministic fixture
  support only; no duplicated orchestration.
- `parish-server` and `parish-tauri`: thin lifecycle wiring using existing
  session cancellation.
- `mods/rundale`: optional authored goal/exception data in a later phase; none
  required for the first scheduler slice.

## Feature flag

Gate behavior with `config.flags.is_enabled("independent-npc-agents")`. The
flag is default-on when the feature ships. While disabled, the current schedule
and Tier 2/3 cycles remain authoritative. During rollout, shadow comparison may
run without committing agent intents so parity can be measured safely.

## Rollout

1. **Agenda and observability:** persist default agendas, rebuild the heap, and
   expose `/debug agents`; no autonomous behavior changes.
2. **Deterministic schedule ownership:** generate schedule movement intents,
   shadow them against `tick_schedules`, then switch ownership after parity.
3. **Tier 2 scenes:** replace the global Tier 2 due timestamp with individual
   wake deadlines while preserving co-location batching and cancellation.
4. **Tier 3 planning:** batch due distant NPCs and write individual goals,
   activities, summaries, and wake deadlines.
5. **Tier 4 integration:** represent sparse rule outcomes as intents through
   the same reducer.
6. **Cleanup:** remove superseded tier tick state and duplicated dispatch loops,
   update ADR-002 to describe tiers as planner policy rather than agent clocks.

Each stage must preserve a kill switch and old-save compatibility. No stage
removes its predecessor until deterministic fixtures, mode-parity tests, and a
live play proof pass.

## Verification strategy

Unit and property tests:

- heap ordering, stale generation removal, wake coalescing, and fairness;
- time-jump collapse and per-pump work caps;
- intent precondition validation and deterministic conflict ordering;
- cancellation and late-result rejection;
- schedule shadow parity;
- old-save defaults plus agenda round-trip and heap rebuild;
- no world/NPC lock held during planner awaits.

Integration tests:

- identical fixture output across headless and the real loop;
- two NPCs with different deadlines wake independently;
- a co-located Tier 2 group produces one scene request, not one call per NPC;
- moving the player changes planner fidelity without erasing goals;
- save/load does not duplicate an already-applied action;
- paused game time produces no autonomous actions.

Gameplay proof:

- Run `parish/testing/proofs/play_independent-npc-agents.txt`.
- Capture `/debug agents` before and after waits, movement, save, and load.
- Confirm semantic output shows autonomous consequences without scheduler
  jargon leaking into player-facing prose.
- Run `/parish-engine prove independent-npc-agents` and judge continuity,
  coherence, latency, and absence of synchronized NPC behavior.

## Risks and mitigations

| Risk                               | Mitigation                                                                                                         |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| Runaway wake loops                 | Require every outcome to advance `next_wake_at`; cap wakes per pump and per NPC per game minute.                   |
| Stale LLM decisions                | Generation tokens, diagnostic revision stamps, localized typed preconditions, and commit-time validation.          |
| Inference starvation               | Preserve priority lanes, cancel on player input, batch by scene/tier, and defer background work.                   |
| Save/load duplication              | Persist committed agenda generation, clear in-flight work, and replay mutations from events rather than proposals. |
| Deadlocks                          | Snapshot each state domain in a separate lock scope, release before await, and centralize commit orchestration.    |
| NPCs ignore schedules              | Keep schedules authoritative in the first release; deviations require typed, expiring reasons.                     |
| Large time jumps explode work      | Collapse missed wakes and preserve only hard boundaries plus final state.                                          |
| Agent behavior becomes invisible   | Stable debug view, per-outcome tracing, counters, and harness assertions.                                          |
| Server session cost grows linearly | One bounded scheduler and worker pool per session, not one task per NPC.                                           |

## Alternatives considered

### Keep global Tier 2/3 cycles unchanged

This is operationally simple and already bounded, but NPCs continue to lack
individual goals and deadlines. It does not deliver the desired sense of
independent pace.

### One Tokio task per NPC

This gives attractive local code but poor persistence, cancellation,
determinism, and server scaling. Tasks would still need centralized mutation
and inference arbitration, leaving their main benefit cosmetic.

### Actor model with one mailbox per NPC

Mailboxes are useful conceptually, but one runtime actor per NPC is unnecessary
for the first implementation. The persisted agenda plus coalesced wake queue
provides actor-like isolation with fewer lifecycle objects. A mailbox facade
can be added later if cross-NPC messaging becomes complex.

### Remove cognitive LOD

Independent cadence does not solve inference scarcity. Removing tiers would
either saturate the model or flatten every NPC to low fidelity. LOD and agency
solve different problems and should remain separate.

## Recommendation

Proceed with the scheduler-and-intent design. Treat "independent NPC" as a
gameplay and state-model property, not a threading topology. The first
implementation should stop after agenda persistence, debug observability, and
deterministic schedule-intent parity; only then migrate Tier 2 and Tier 3
inference.
