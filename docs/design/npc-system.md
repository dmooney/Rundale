# NPC System

> Parent: [Architecture Overview](overview.md) | [Docs Index](../index.md) | ADR: [008](../adr/008-structured-json-llm-output.md)

## Entity Data Model

Each NPC has:

- **Identity**: Name, age, physical description, occupation
- **Personality**: Traits, values, temperament (used as LLM system prompt)
- **Intelligence**: Multidimensional profile (6 axes, 1-5 scale) — injected as direct behavioral guidance, not coded tags — see [ADR 018](../adr/018-npc-intelligence-dimensions.md)
- **Location**: Current node, home node, workplace node
- **Schedule**: Daily routine patterns (varies by day of week, season, weather)
- **Relationships**: Weighted edges to other NPCs (family, friend, rival, enemy, romantic, etc.)
- **Memory**:
  - Short-term: Last few interactions, current goals, immediate observations
  - Long-term: Key events, major relationship changes, grudges, secrets
  - Consider embedding-based retrieval for relevant long-term memories
- **Physical State**: Health, energy, hunger (if applicable)
- **Knowledge**: What they know about the world — public events, gossip, secrets

## NPC Context Construction

For each LLM inference call, build a context from these layers:

1. **System prompt**: personality, intelligence guidance (behavioral directives only), current emotional state. NPC dialogue is pure speech — no parenthetical stage directions. Physical actions are tracked in JSON metadata only.
2. **Public knowledge**: weather, time, season, major recent events
3. **Personal knowledge**: their relationships (by name), recent experiences, secrets
4. **Immediate situation**: where they are, who's present (with relationship context), what just happened
5. **Conversation history**: recent exchanges at this location (last 3), with scene continuity cues
6. **Witness awareness**: overheard conversations from bystander memory

## Conversation Awareness

NPCs are aware of conversations happening around them, not just conversations directed at them.

### Witness Memory System

When the player talks to NPC A at a location where NPCs B and C are also present:
- B and C each receive a short-term memory entry: `"Overheard: a newcomer said '...' and {A} replied '...'"`
- These memories appear in B's and C's context when the player talks to them next
- This creates natural conversational flow: "I heard what you said to Padraig..."

### Conversation Log

A per-location ring buffer (`ConversationLog` on `WorldState`) tracks the last 30 exchanges globally. Recent exchanges at the current location are injected into the context prompt under "What's been said here", giving NPCs awareness of what's been discussed.

### Scene Continuity

If the player has recently spoken to the same NPC, a cue is injected: "You are already in conversation with this newcomer. Do not re-introduce yourself." This prevents NPCs from re-greeting on every utterance.

### Prompt Quality

- Relationships reference NPCs by name ("Niamh Darcy: Family, very close") not by ID
- "Also present" includes relationship context ("Niamh Darcy, the Publican's Daughter — Family to you")
- Knowledge framed as "WHAT'S ON YOUR MIND" for natural grounding

## Gossip & Information Propagation

NPCs share information through conversation. A public event gets injected into the shared knowledge base. Private information (gossip, secrets) spreads through NPC-to-NPC interactions, potentially getting distorted. The player can learn about offscreen events through NPC dialogue organically.

## Structured Output Schema

All LLM responses for NPC behavior should be structured JSON:

```json
{
  "action": "speak|move|trade|work|rest|observe",
  "target": "player|npc_id|location|item",
  "dialogue": "What they say (if speaking)",
  "mood": "current emotional state",
  "internal_thought": "what they're actually thinking (hidden from player)",
  "knowledge_gained": ["any new information learned"],
  "relationship_changes": [{"npc_id": "...", "delta": 0.0}]
}
```

## Structured Emotion System

Grounded in Anthropic's April 2026 paper *Emotion Concepts and their Function
in a Large Language Model*. Each NPC carries an `EmotionState` alongside their
freeform `mood` string; the state is authoritative, the mood is re-derived.

**Model shape** (`parish_types::emotion`):

- **Family vector** — eight scalar intensities in `[0, 1]`: Joy, Sadness, Fear,
  Anger, Disgust, Surprise, Shame, Affection.
- **PAD coordinates** — Pleasure/Arousal/Dominance, each in `[-1, 1]`.
- **Temperament** — static per-NPC tuning: `cheerfulness`, `reactivity`,
  `persistence`. Validated at load time (`Temperament::validate`) so
  out-of-range mod values surface as a `ParishError::Setup` rather than silent
  clamping.

**Non-linear behavioural gates** (`EmotionGates`) fire at threshold crossings,
capturing the paper's finding that extreme emotion produces qualitatively
different behaviour from moderate emotion:

- `panic_truth` (`fear > 0.9`) — NPC blurts honest answers they'd normally hide.
- `public_outburst` (`anger ∈ [0.5, 0.85]`) — NPC speaks up unprompted,
  confrontationally. `anger > 0.85` drops out of this band — extreme anger
  produces disclosure, not aggression.
- `withdraws_silent` (`sadness > 0.8` OR `shame > 0.8`) — NPC speaks little.
- `effusive` (`joy > 0.85` OR `affection > 0.7 && arousal > 0.5`) — verbose,
  generous.

Gates influence autonomous speaker selection in
`parish_core::npc::autonomous::pick_next_speaker_with_config` and are surfaced
as prose in `EmotionState::prompt_guidance` for Tier 1 preambles.

### Leaf projection

`project_top_k` picks the 3 closest entries from a 171-leaf affective lexicon
for prompt injection. Scoring: family-match bonus + PAD distance +
`INTENSITY_GAP_WEIGHT (=2.0) × |leaf.family_weight − dom_intensity|`. The
high intensity-gap weight ensures a high-intensity state picks a high-weight
leaf (e.g. `furious` over `annoyed` at `anger=0.95`) even when PAD has
drifted from the family signature.

### Propagation

- **Decay** — each family and each PAD axis exponentially decays toward
  baseline with the half-life implied by `Temperament::persistence`. Runs on
  `Command::Wait`, `apply_movement`, and `GameTestHarness::advance_time`; it
  does **not** run on `Command::Tick` because Tick re-runs scheduling at the
  current game time without advancing the clock.
- **Contagion** — `propagate_emotion_contagion(fraction)` leaks family
  intensities along relationships with `strength > 0.6`. Runs at 5% per
  Tier 2 cycle and 2% immediately after Tier 1 player-driven dialogue,
  capped per family per tick by `MAX_CONTAGION_DELTA`.
- **Grief on death** — `Tier4Event::Death` walks the dead NPC's
  relationships and applies a sadness impulse to each surviving relative
  scaled by `strength.max(0) × 0.5`. Enemies (negative strength) are
  untouched.

### Kill-switch

The runtime feature flag `emotions` (toggled in-game with `/flag enable|disable
emotions`) gates the prompt-layer use of the system:

- Tier 1/2/3 prompt preambles fall back to the pre-feature `mood: {string}`
  shape when disabled.
- `pick_next_speaker_with_config` reverts to arousal-only scoring (no gate
  influence).
- Decay and contagion **always run** regardless of the flag — toggling
  mid-session reveals live state rather than a stale snapshot.

The check uses `GameConfig::emotions_enabled()` which reads
`FeatureFlags::is_disabled("emotions")` — the flag is default-on (CLAUDE.md
rule-6 kill-switch semantics).

### Event bus integration

`GameEvent::EmotionChanged { npc_id, family, delta, cause, timestamp }` is
emitted from `NpcManager::apply_tier4_events` for illness, recovery, grief,
birth joy, trade joy, and festival gatherings. The journal bridge treats it
as informational (no state-mutation replay needed — emotion state is
re-derivable from impulses after restart).

### IPC shape

- `NpcInfo` (sidebar): adds optional `top_leaves` and `active_gates` fields
  so the `<MoodIcon>` tooltip shows a richer read than the bare mood string.
- `NpcDebug.emotion` (debug panel): `EmotionDebug { label, top_leaves,
  families, pleasure, arousal, dominance, active_gates }` for the deep-dive.

## Related

- [Cognitive LOD](cognitive-lod.md) — Tier system determines inference fidelity per NPC
- [Inference Pipeline](inference-pipeline.md) — How NPC context is sent to Ollama
- [Weather System](weather-system.md) — Weather affects NPC schedules and behavior
- [World & Geography](world-geography.md) — NPCs are bound to location nodes
- [ADR 008: Structured JSON LLM Output](../adr/008-structured-json-llm-output.md)

## Source Modules

- [`crates/parish-core/src/npc/`](../../crates/parish-core/src/npc/) — NPC data model, behavior, cognition tiers
- [`crates/parish-core/src/npc/conversation.rs`](../../crates/parish-core/src/npc/conversation.rs) — ConversationLog, ConversationExchange
- [`crates/parish-core/src/npc/ticks.rs`](../../crates/parish-core/src/npc/ticks.rs) — Prompt builders, witness memories, response processing
- [`crates/parish-core/src/npc/memory.rs`](../../crates/parish-core/src/npc/memory.rs) — Short-term and long-term memory
- [`crates/parish-types/src/emotion.rs`](../../crates/parish-types/src/emotion.rs) — `EmotionState`, `EmotionGates`, `Temperament`, decay
- [`crates/parish-types/src/emotion_leaves.rs`](../../crates/parish-types/src/emotion_leaves.rs) — 171-leaf lexicon, `project_top_k`
