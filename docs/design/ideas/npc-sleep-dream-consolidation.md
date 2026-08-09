# NPC Sleep & Dream-Based Memory Consolidation

> Back to [Documentation Index](../../index.md) | [NPC System](../npc-system.md) | [Emotion-Driven Dialogue & Simulation](emotion-driven-dialogue-and-simulation.md) | [Night Visions (player dreams)](night-visions.md)

> Status: **DEFERRED** — design captured for a future implementation. Do
> **not** implement until PR #443 (`feat(emotion): structured character
emotion system`) has landed and completed human playtest. See the
> "Conflict Analysis" section for why. This document is the _NPC-side_
> counterpart to [night-visions.md](night-visions.md), which covers
> _player_ dreams; the two are distinct features that share thematic
> ground.

> Branch when implementation begins: `claude/npc-sleep-dream-mechanics-2jgwH`

---

## Context

Today, NPCs in Rundale "sleep" only in the sense that their scheduled `activity` string is `"sleeping"` (see `mods/rundale/npcs.json` — e.g., Padraig Darcy sleeps 00–05h, winter extends to 00–06h, Sunday lie-ins). There is no sleep _state_, no fatigue, no dream content, and no memory consolidation. NPCs currently accumulate memory in a 20-entry `ShortTermMemory` ring buffer (`crates/parish-npc/src/memory.rs:18–157`) plus a 50-entry keyword-indexed `LongTermMemory`. As game sessions lengthen, short-term memory overflow means NPCs forget days in bulk without any abstraction pass.

This plan adds:

1. A real `NpcState::Sleeping` with a `fatigue` scalar and gameplay consequences for missed sleep.
2. A dream-time memory consolidation pass that runs when an NPC exits their sleep window: the day's `ShortTermMemory` is reflected into higher-level summaries, older summaries are recompacted further, and a small set of "core memories" are promoted so they never decay. Patterned on the Generative Agents reflection tree plus an explicit core-memory tier.

The outcome: NPCs retain coherent multi-week narratives ("Padraig still grumbles about the time you stiffed him on the tab, but the details have blurred into 'that English traveller who made trouble'"), dialogue gets richer late-game, and skipped sleep has real cost.

---

## User decisions (captured during planning)

1. **Sleep scope:** Medium — add `NpcState::Sleeping` + `fatigue: f32`. Not the heavy REM/NREM phase model.
2. **Consolidation architecture:** Hybrid — Generative Agents-style reflection tree _plus_ an explicit "core memory" tier that is exempt from compaction.
3. **Trigger:** End of each NPC's individual sleep window (when their schedule's `activity` transitions out of `"sleeping"`). Not a single global midnight cron; not a capacity-overflow trigger.
4. **Dream purpose:** Consolidate the day's memories into a detailed summary; recompact older summaries further over time; natural forgetting as a side effect; core memories elevated to never compact.

---

## Conflict Analysis — why this is deferred

PR #443 (emotion system) is open, not yet playtested, and touches several of the same files and design surfaces. Implementing sleep/dreams on top of an unvalidated emotion system would:

- create merge conflicts in `Npc`, `NpcSnapshot`, Tier 4 apply paths, and the config-flag file;
- lock in design decisions (how fatigue interacts with emotion decay, how dreams surface emotional state) before the emotion model has been validated by real play;
- miss the cleanest integration point — dreams are far more interesting when they can draw on `EmotionState` (grief dreams, anxious dreams, joyful dreams).

### File overlap table

| File                                               | #443 uses                                                                                             | This plan will add                                                              | Overlap                 |
| -------------------------------------------------- | ----------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------- | ----------------------- |
| `crates/parish-npc/src/lib.rs`                     | `Npc.emotion`, `Npc.temperament`, `set_emotion`, `apply_emotion_impulse`, `NpcMetadata.emotion_delta` | `Npc.sleep_state`, `Npc.fatigue`, `Npc.core_memories`, `Npc.consolidations`     | **High** — same struct  |
| `crates/parish-npc/src/manager.rs`                 | Tier 4 emits structured emotion impulses                                                              | Tier 4 fatigue-biased rules; end-of-sleep-window detection                      | **High**                |
| `crates/parish-npc/src/ticks.rs`                   | `decay_emotions_tick`, `propagate_contagion`, emotion deltas on Tier 2/3 schemas                      | New `run_dream_consolidation` pass; fatigue tick                                | Medium                  |
| `crates/parish-persistence/src/snapshot.rs`        | `#[serde(default)]` for `emotion` / `temperament`; legacy-mood reseed                                 | Same pattern for `sleep_state` / `fatigue` / `consolidations` / `core_memories` | Medium — mechanical     |
| `crates/parish-config/src/engine.rs`               | `NpcConfig.emotions_enabled` flag                                                                     | `NpcConfig.dreams_enabled` flag                                                 | Medium                  |
| `crates/parish-engine/src/debug.rs` + `testing.rs` | `/debug emotion`, `/stub-emotion`                                                                     | `/debug dreams`, `/stub-fatigue`, `/force-dream`                                | Low — parallel patterns |
| `crates/parish-core/prompts/*.prompt.yml`          | `npc_tier1` emotion preamble                                                                          | New `npc_dream_consolidation.prompt.yml`                                        | Low — new file          |
| `crates/parish-npc/src/memory.rs`                  | (untouched by #443)                                                                                   | Consolidation logic, core-memory marker, decay of old summaries                 | None                    |
| `crates/parish-npc/src/types.rs`                   | (untouched by #443)                                                                                   | Extend `NpcState` with `Sleeping` variant                                       | None                    |

### Design coupling (bigger reason than files)

- **Fatigue ↔ emotion reactivity:** a tired NPC's `Temperament::reactivity` should probably be scaled down, or their `decay_emotions_tick` baseline should drift. Building fatigue without this hook = wasted tuning.
- **Dream content ↔ emotion state:** the richest consolidation prompt takes the NPC's current PAD vector and family intensities as inputs so grief colours the summary. Pre-emotion dreams would have to be rewritten later.
- **Core-memory promotion ↔ emotion intensity:** an event that spiked `sadness > 0.8` or `joy > 0.85` is a natural core-memory trigger. Without the gates, we'd invent a weaker heuristic.

---

## Existing substrate (confirmed by exploration)

These exist today and should be **reused**, not reinvented:

- **`ShortTermMemory`** (ring buffer, cap 20) — `crates/parish-npc/src/memory.rs:18–80`. `MemoryEntry { timestamp, content, participants, location, kind }`. Kinds: `SpokeWithPlayer`, `SpokeWithNpc`, `OverheardConversation`, `ReceivedGossip`.
- **`LongTermMemory`** (scored, keyword-indexed, cap 50) — `crates/parish-npc/src/memory.rs:80–157`. `try_promote()` already uses importance ≥ 0.5 (player involvement + emotional-word detection). This is the **existing analogue** of "core memory" — we extend it with a never-compact tier rather than building parallel storage.
- **`Npc.last_activity: Option<String>`** — `crates/parish-npc/src/lib.rs:105`. Written by Tier 3. Becomes one input to the dream prompt.
- **`NpcState` enum** — `crates/parish-npc/src/types.rs:472–484`. Currently `Present` / `InTransit`. Add `Sleeping`.
- **`ScheduleEntry.activity: String`** — `crates/parish-npc/src/types.rs:319`. The literal string `"sleeping"` (and substrings like "sleeping in the rooms above the pub", "sleeping late on the Lord's day") in `mods/rundale/npcs.json` is our sleep-window signal. Use substring match, not exact equality.
- **Tier 3 batch pipeline** — `crates/parish-npc/src/ticks.rs::tick_tier3`, output type `Tier3Update { activity_summary, mood, new_location, relationship_changes }`. Async LLM call via `parish_inference`. We piggyback on this infra for dream consolidation — same batching pattern, different prompt and response schema.
- **Prompts infra** — `crates/parish-core/src/prompts/mod.rs`. `.prompt.yml` files with `include_str!()` + `LazyLock<PromptFile>` parsing. `{{key}}` substitution. Add `npc_dream_consolidation.prompt.yml` following the same shape.
- **Persistence** — `crates/parish-persistence/src/snapshot.rs:70–145`. NPC is a JSON blob inside SQLite snapshots. Add `#[serde(default)]` on all new `Npc` fields so old saves load cleanly (same backwards-compat story #443 uses).
- **Feature flags** — `crates/parish-config/src/engine.rs`. Follow the `NpcConfig.emotions_enabled` pattern exactly for `dreams_enabled` and `sleep_state_enabled`.
- **Deflation/inflation** — `crates/parish-npc/src/transitions.rs:37–123`. `deflate_npc_state` already captures up to 3 recent memories into `NpcSummary` when an NPC leaves Tier 1. The dream summary can feed this pipeline — post-consolidation, the deflated summary becomes a recompaction-of-summaries rather than a raw-memory snapshot.
- **Relationships, mood** — already per-NPC (`HashMap<NpcId, Relationship>`, `mood: String`). Dream prompt takes these as context.

---

## Design — data model

Add to `crates/parish-npc/src/lib.rs` `Npc` struct (all `#[serde(default)]` for snapshot compat):

```rust
/// Current wakefulness state. Orthogonal to NpcState::Present/InTransit.
sleep_state: SleepState,

/// 0.0 = fully rested, 1.0 = exhausted. Ticked up when awake, down when Sleeping.
fatigue: f32,

/// Hierarchical consolidated summaries, newest first.
/// Each level n is a compaction of multiple level n-1 entries.
/// Level 0 = last night's dream. Level 1 = a week-ish. Level 2 = a season. etc.
consolidations: Vec<ConsolidationEntry>,

/// Core memories, never compacted, never decayed.
/// Promoted during consolidation when importance score or emotion gate fires.
core_memories: Vec<CoreMemory>,

/// Track last consolidation to avoid re-running in the same sleep window.
last_consolidation_at: Option<DateTime<Utc>>,
```

New types in `crates/parish-npc/src/memory.rs`:

```rust
pub enum SleepState { Awake, Sleeping }

pub struct ConsolidationEntry {
    pub timestamp: DateTime<Utc>,     // when consolidated (end of sleep)
    pub covers_from: DateTime<Utc>,   // earliest source-memory time
    pub covers_to: DateTime<Utc>,     // latest source-memory time
    pub level: u8,                    // 0 = nightly, 1+ = recompactions
    pub summary: String,              // LLM-generated prose
    pub key_participants: Vec<NpcId>, // for retrieval/indexing
    pub importance: f32,              // max importance of source memories
}

pub struct CoreMemory {
    pub timestamp: DateTime<Utc>,
    pub content: String,
    pub participants: Vec<NpcId>,
    pub reason: CorePromotionReason, // why it was elevated
}

pub enum CorePromotionReason {
    ImportanceThreshold,      // importance >= CORE_MEMORY_THRESHOLD (e.g. 0.9)
    EmotionGate(EmotionFamily), // #443 gate fired (panic_truth, effusive, etc.)
    FirstMeeting(NpcId),      // canonical "first time I met X"
    LifeEvent,                // Tier 4 birth/death/marriage involving this NPC
    ExplicitTag,              // modder or narrative designer tagged it
}
```

Extend `crates/parish-npc/src/types.rs` `NpcState`:

```rust
pub enum NpcState {
    Present,
    InTransit,
    Sleeping, // new
}
```

`NpcState::Sleeping` overrides presence: dialogue requests to a sleeping NPC either fail with "She's asleep" or (optional) succeed at a fatigue cost and emit a waking event.

---

## Design — behavioural rules

### Entering / exiting sleep

In `crates/parish-npc/src/ticks.rs`, during each game-clock advancement:

1. For each NPC, compute their _current_ scheduled activity via `SeasonalSchedule::entry_at(hour)` (already exists).
2. If previous-tick activity did NOT contain `"sleeping"` and current-tick activity DOES → set `sleep_state = Sleeping`, emit `GameEvent::NpcSleepStart`.
3. If previous-tick activity DID contain `"sleeping"` and current-tick activity does NOT → this is the **consolidation trigger**:
   - set `sleep_state = Awake`;
   - if `dreams_enabled`, enqueue a dream-consolidation job for this NPC;
   - decrement `fatigue` proportional to how long they slept (full night ≈ 1.0 back to 0);
   - emit `GameEvent::NpcSleepEnd`.

### Fatigue dynamics

```text
// Per-tick, while Awake:
fatigue += AWAKE_FATIGUE_RATE * dt_hours;  // e.g. 0.05/hour, so ~16h awake = 0.8

// Per-tick, while Sleeping:
fatigue -= SLEEP_RECOVERY_RATE * dt_hours; // e.g. 0.18/hour, so 6h sleep ≈ full recovery

fatigue = fatigue.clamp(0.0, 1.0);
```

Gameplay effects of high fatigue (gated behind `sleep_state_enabled`):

- `fatigue > 0.8`: Tier 4 probabilities for `Illness` doubled; irritability signal exposed to emotion system (when #443 is in, bias `decay_emotions_tick` baseline toward negative PAD).
- `fatigue > 0.9`: NPC may enter unscheduled `Sleeping` state if in a safe location (their home) — a "collapse into bed" check in Tier 3.
- `fatigue < 0.2`: eligible for `effusive`/energetic behaviour in autonomous speaker scoring.

### End-of-sleep dream consolidation

Runs once per sleep-window-end per NPC, async, batched across NPCs like Tier 3. Procedure:

1. **Gather inputs for this NPC:**
   - `ShortTermMemory` entries since `last_consolidation_at` (or since sleep started, whichever older).
   - `last_activity` string.
   - Current `mood` (and `EmotionState` + top leaves once #443 has landed).
   - Top-3 relationships by change-since-last-consolidation.
   - Immediate prior `consolidations[0]` if any (for narrative continuity).
2. **Run LLM prompt** `npc_dream_consolidation.prompt.yml` returning structured JSON:

```json
{
  "summary": "A prose dream-as-summary of yesterday, 2-4 sentences.",
  "key_participants": ["npc_id_1", "npc_id_2"],
  "candidate_core_memories": [
    { "content": "...", "reason": "ImportanceThreshold", "participants": [] }
  ],
  "forgotten_details": ["list of trivia the NPC lets slip"]
}
```

3. **Write-back:**
   - Push a new `ConsolidationEntry { level: 0, ... }` to the front of `consolidations`.
   - Promote each `candidate_core_memory` into `core_memories` (de-duplicate by content hash).
   - Clear `ShortTermMemory` entries that are now covered by the consolidation AND are not already in `LongTermMemory` or `core_memories`.
   - Update `last_consolidation_at`.

### Hierarchical recompaction (progressive forgetting)

After writing the level-0 entry, check if higher-level recompaction should fire:

- If `consolidations` contains ≥ 7 level-0 entries: take the oldest 7, run the dream-consolidation prompt again with `level = 1` and those seven summaries as input (instead of raw memories). Replace them with a single level-1 entry.
- If ≥ 4 level-1 entries: recompact into a level-2 entry. (Analogous threshold for each level.)
- Core memories are _never_ touched by recompaction — they're stored separately.

Tunable constants (put in a `MemoryConfig` or constants block):

```text
const LEVEL_0_BATCH: usize = 7;   // week → weekly summary
const LEVEL_1_BATCH: usize = 4;   // month-ish
const LEVEL_2_BATCH: usize = 3;   // season-ish
const CORE_MEMORY_IMPORTANCE_THRESHOLD: f32 = 0.9;
```

This yields logarithmic memory growth in dream entries per NPC regardless of game length.

---

## Design — retrieval

When dialogue with an NPC runs (Tier 1 prompt assembly), memory retrieval should prefer:

1. **Always include:** all `core_memories` (short list, never compacted).
2. **Always include:** the most recent `ConsolidationEntry` at level 0 ("yesterday").
3. **Fuzzy-retrieve:** up to N `ConsolidationEntry` items at any level whose `key_participants` or keyword match the current dialogue partner / topic (reuse `LongTermMemory`'s keyword index pattern in `memory.rs`).
4. **Recent raw:** `ShortTermMemory` entries since last consolidation (unconsolidated today-so-far).
5. **Existing LongTermMemory** stays as-is; it's the highly-scored individual moments.

Prompt shape for Tier 1 gains a new section (templated):

```text
You remember:
CORE: <core_memories summaries joined>
YESTERDAY: <consolidations[0].summary>
LATELY: <relevant higher-level consolidations>
JUST NOW: <ShortTermMemory since last sleep>
```

---

## Prompt file — `npc_dream_consolidation.prompt.yml`

Location: `crates/parish-core/prompts/npc_dream_consolidation.prompt.yml`. Follow the existing format (messages: `[role, content]`, `{{key}}` substitution).

System message sketch (condensed — full text to be written at implementation):

```text
You are helping simulate the inner life of an NPC in 1820 rural Ireland.
It is dawn. The NPC is waking. Summarise their just-ended day as a dream —
the shape of what happened, compressed. Preserve the emotional texture.
Some details should blur or drop. Anything that meaningfully changed the
NPC's relationships or worldview should be marked as a candidate core memory.

Return JSON matching this schema:
{ "summary": string,
  "key_participants": string[],
  "candidate_core_memories": [{ content, reason, participants }],
  "forgotten_details": string[] }
```

User message template includes: NPC name, age, occupation, current `mood`, current `EmotionState` (when #443 is in), yesterday's `ShortTermMemory` entries rendered as bullet points, previous night's summary (if any), top relationships with deltas.

Recompaction at higher levels uses the same prompt with a different user-message template that feeds summaries-of-summaries instead of raw memories, and a hint about the time-span being compressed.

---

## Feature flags

Add to `crates/parish-config/src/engine.rs` `NpcConfig`:

- `sleep_state_enabled: bool` (default `true`) — if off, `NpcState::Sleeping` never entered; fatigue not tracked or surfaced.
- `dreams_enabled: bool` (default `true`) — if off, no consolidation runs; only raw memories and existing `LongTermMemory` are used. Useful for offline / cheap-mode / automated-test runs where Ollama round-trips are undesirable.

Both follow the `emotions_enabled` pattern from #443 exactly: kill-switch for _externally visible behaviour_, but underlying state evolution (fatigue ticking when enabled) is independent. Document in PR body with the same "flag reveals accurate state" wording.

---

## Persistence

Update `crates/parish-persistence/src/snapshot.rs` `NpcSnapshot`:

- Add `#[serde(default)]` fields for all new `Npc` struct fields (`sleep_state`, `fatigue`, `consolidations`, `core_memories`, `last_consolidation_at`).
- Old saves re-hydrate with: `Awake`, `0.0`, empty vecs, `None`. First post-upgrade sleep then seeds the consolidation pipeline.
- No new tables needed — these live inside the same NPC JSON blob.
- No schema migration needed (inline migration in `database.rs` is unaffected).

---

## Debug / harness surfaces

Following the #443 `/debug emotion` / `/stub-emotion` pattern:

- `/debug dreams <npc>` → print `sleep_state`, `fatigue`, last 3 `consolidations` (with levels and time spans), count of `core_memories`, head of list.
- `/stub-fatigue <npc> <value>` → set fatigue directly for testing.
- `/force-dream <npc>` → run dream-consolidation synchronously right now regardless of schedule (test harness).
- Add a gameplay-proof fixture `parish/testing/proofs/play_prove_dreams.txt` that:
  1. Has the player converse extensively with one NPC across an in-game day.
  2. Advances time through the night.
  3. Uses `/debug dreams` to confirm a `level: 0` consolidation entry was created.
  4. Converses again the next day and checks that the NPC references the summarised form (not verbatim) of yesterday's interaction.
  5. Time-jumps 7+ in-game days, confirms a level-1 recompaction fires.
  6. Confirms core memories survive across multiple recompactions.

This fixture is the `/prove` target per CLAUDE.md rule #4.

---

## Non-negotiable engineering rules (CLAUDE.md)

1. Shared logic (sleep detection, consolidation, fatigue dynamics) goes in `crates/parish-core` / `crates/parish-npc` only. Do not duplicate into `parish-engine`. ✓
2. **Mode parity:** Tauri, headless CLI, web server must all run sleep/dream identically. Thread the flags through all three entry points (same gap #443 had to close in audit items #14 / #28).
3. **Tests with behaviour changes:** unit tests for sleep entry/exit detection, fatigue clamp, recompaction threshold, core-memory de-dup, snapshot round-trip. Integration test for a full night → consolidation → next-day Tier 1 prompt including consolidation summary.
4. **Gameplay proof:** `/prove dreams` fixture described above. Unit tests are not sufficient.
5. No `#[allow]` attributes without justification.
6. Feature flags (`sleep_state_enabled`, `dreams_enabled`), default-on, documented in PR body.
7. **Acceptance criteria:** write `.proofs/<task-id>/acceptance-criteria.md` and the verification fixture before implementation.

---

## Verification

1. `just check` — fmt + clippy + workspace test suite green.
2. `just verify` — harness walkthrough green.
3. `cargo run --manifest-path parish/Cargo.toml -p parish-engine -- --script parish/testing/proofs/play_prove_dreams.txt` — passes.
4. Manual Tauri session: start a new game, play ≥ 1 in-game day, advance through night, open dev-mode NPC debug panel, confirm a consolidation entry is visible with a sensible summary.
5. Manual regression on #443 coverage (assuming it has landed): `/prove emotions` still passes unchanged; the combined `/debug emotion` + `/debug dreams` shows fatigue biasing emotion decay baseline as designed.
6. Save a game mid-way through, reload, confirm `consolidations` and `core_memories` survived the round-trip.

---

## Files to modify when implementation begins (after #443)

**New files:**

- `crates/parish-core/prompts/npc_dream_consolidation.prompt.yml`
- `parish/testing/proofs/play_prove_dreams.txt`

**Extend:**

- `crates/parish-npc/src/lib.rs` — `Npc` new fields.
- `crates/parish-npc/src/memory.rs` — `SleepState`, `ConsolidationEntry`, `CoreMemory`, `CorePromotionReason`, consolidation + recompaction functions.
- `crates/parish-npc/src/types.rs` — `NpcState::Sleeping` variant.
- `crates/parish-npc/src/ticks.rs` — sleep entry/exit detection, fatigue tick, dream-consolidation dispatch (Tier 3-style batch).
- `crates/parish-npc/src/manager.rs` — fatigue-biased Tier 4 rules, unscheduled collapse-into-sleep check.
- `crates/parish-npc/src/transitions.rs` — feed `consolidations[0]` into `deflate_npc_state` so deflated NPCs carry their dream summary.
- `crates/parish-npc/src/data.rs` — load any optional sleep/dream tuning from `npcs.json` (e.g. personality-specific sleep needs) if designers want it later; minimal initially.
- `crates/parish-persistence/src/snapshot.rs` — snapshot fields with `#[serde(default)]`.
- `crates/parish-config/src/engine.rs` — `sleep_state_enabled`, `dreams_enabled` flags.
- `crates/parish-engine/src/debug.rs` + `crates/parish-engine/src/testing.rs` — `/debug dreams`, `/stub-fatigue`, `/force-dream`.
- `crates/parish-core/src/prompts/mod.rs` — register new prompt file via `include_str!` + `LazyLock<PromptFile>`.
- Tauri + server entry points to thread config flags to NPC tick calls (parity).
- `docs/design/npc-system.md` — add a "Sleep & Dream Consolidation" cross-link to this doc.

---

## Research appendix — SOTA agent long-term memory (May 2026)

Summarised from web search performed during planning. The design above deliberately reuses patterns validated in this literature rather than inventing new ones.

### Foundational

- **Generative Agents (Park et al., UIST 2023)** — memory stream of natural-language events; importance × recency × relevance retrieval; recursive reflection trees abstracting leaves into higher-level insights. Core inspiration for the hybrid reflection-tree approach here. https://arxiv.org/abs/2304.03442
- **Survey: Memory Mechanism of LLM-based Agents (Zhang et al., 2024)** — three-stage taxonomy: construction, update, query. https://arxiv.org/abs/2404.13501

### Memory-as-OS / retrieval paradigms

- **MemGPT** — OS-style hierarchy; LLM pages memory between in-context and external storage via tools. https://arxiv.org/abs/2310.08560
- **Mem0** — production-ready scalable memory; entity/relation triplets into a knowledge graph with conflict resolution. +26% over OpenAI memory on LLM-as-judge. https://arxiv.org/abs/2504.19413
- **A-Mem (2025)** — store/retrieve/update/summarise/discard as callable tools trained via GRPO; large token reductions vs baseline. https://arxiv.org/pdf/2502.12110

### Sleep- and dream-inspired (most relevant)

- **LightMem (2025)** — Atkinson-Shiffrin staged memory; _offline sleep-time update_ decoupled from online inference. Closest architectural precedent for our end-of-sleep-window trigger. https://arxiv.org/html/2510.18866v1
- **"Language Models Need Sleep" / SleepGate** — key decay, learned gating, consolidation modules; a dreaming phase generates synthetic curricula via RL. https://openreview.net/forum?id=iiZy6xyVVE
- **Active Dreaming Memory** — biologically-inspired episodic consolidation for lifelong learning. https://www.researchgate.net/publication/398306877
- **Learning to Forget (2025)** — sleep-inspired consolidation specifically to resolve proactive interference. Supports the "natural forgetting is a feature" angle.

### Episodic/semantic hierarchy + forgetting

- **Position: Episodic Memory is the Missing Piece for Long-Term LLM Agents (2025)** — argues episodic memory (autobiographical events) is the missing cognitive layer. https://arxiv.org/pdf/2502.06975
- **Human-Like Remembering and Forgetting (ACT-R-inspired)** — HAI 2025. ACT-R activation decay in agent memory.
- **MemoryBank** — exponential Ebbinghaus decay in multi-turn dialogues; reinforcement-on-access resets the curve. Relevant to how often-accessed `ConsolidationEntry` items should resist recompaction.

### Benchmarks (for later evaluation)

- **LoCoMo** — very long-term conversational memory eval, 35-session dialogues with explicit personas and temporal event graphs. https://arxiv.org/abs/2402.17753
- **LongMemEval** — five memory abilities: info extraction, multi-session reasoning, temporal reasoning, knowledge updates, abstention. https://arxiv.org/abs/2410.10813

### Role-play / character-specific

- **Character-LLM (Shao et al., EMNLP 2023)** — trainable agents acting as specific characters with detailed persona knowledge. https://arxiv.org/abs/2310.10158
- **Survey: Role-Playing Language Agents (2024)** — persona, memory handling, prompt engineering, symbolic + neural decision logic. https://arxiv.org/abs/2404.18231

### Design patterns that recur across this literature (and drive this plan)

1. **Decouple online write from offline consolidate** — our end-of-sleep trigger.
2. **Hierarchical abstraction** episodic → semantic → reflection — our `level 0 / 1 / 2` consolidations.
3. **Importance/salience scoring to gate promotion** — our `CORE_MEMORY_IMPORTANCE_THRESHOLD` reusing the existing `try_promote` signal.
4. **Graph-shaped over vector-shaped** for relational recall — we defer this; Parish already has a bidirectional relationship graph and we index `ConsolidationEntry` by `key_participants` rather than building a new graph store.
5. **Explicit forgetting as a first-class operation** — our natural decay + never-compacted core memories tier.
