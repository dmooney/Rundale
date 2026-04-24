# Agent Planning & Tool-Using NPCs

**Target crate:** `crates/parish-npc/` (new `planner` module),
`crates/parish-core/` (read-only world query API), `crates/parish-inference/`
(function-calling support).

## Problem

NPCs today react locally: Tier 1 answers the player, Tier 2 shifts mood, Tier 3
summarises. There is no *deliberation*: an NPC cannot form a plan
("I'll walk to the forge, ask Séan about the debt, then go to the chapel"), and
when dialogue references a fact, it's because the fact was jammed into the
prompt — the NPC never *asked* for it.

## SOTA techniques

### 1. ReAct loop (Reason → Act → Observe)

Tier 1 / Tier 2 wraps the LLM in a scratchpad loop:

```
Thought: I don't know where Máire is right now.
Action: locate(npc="Máire")
Observation: Máire is at the market cross.
Thought: Good, I can answer the player.
Response: "She was at the cross a moment ago."
```

Stop conditions: max 3 tool calls, 400ms budget. Fallback to best-effort
answer on budget overrun.

### 2. Typed tool surface — read-only v1, mutating v2

**v1 (read-only).** Whitelist safe read tools backed by `parish-core`:

- `locate(npc) -> Location`
- `relationship(a, b) -> f32`
- `remember(npc, query) -> Vec<Memory>` (routes to semantic memory, doc 01)
- `believes(npc, subject) -> Vec<Triple>` (routes to knowledge graph, doc 10)
- `time() -> Date`
- `weather() -> Conditions`
- `recent_news(location) -> Vec<Event>`

Implement as pure `fn` on a `WorldView` snapshot so nothing can mutate during a
tool call. Schema declared via `schemars` (doc 02) and exposed to the LLM via
function-calling (Ollama/OpenAI-compat).

**v2 (mutating, gated).** Once v1 is stable, graduate a tight set of
effect-producing tools so an NPC who says *"I'll fetch the priest"* actually
dispatches Máire:

- `dispatch(npc, destination, errand)` — schedules an NPC movement.
- `start_rumour(origin, content, confidence)` — writes to the gossip spread
  (doc 07).
- `offer_item(from, to, item)` — queues a proposed transfer the recipient
  can accept/decline next tick.
- `set_appointment(a, b, when, where)` — inserts a mutual schedule entry.

Every v2 tool:

1. Emits a proposal, not a direct mutation; Tier 4 rules validate before
   commit (NPC consent, occupancy, travel time).
2. Is logged with the utterance that produced it, so contradictions can be
   audited.
3. Is behind a capability flag per archetype — a child NPC cannot
   `start_rumour` at village scale.

Biggest gameplay payoff in the brainstorm, and the natural successor to
doc 02's grammar work: schema becomes contract.

### 3. Hierarchical planning

For Tier 3 daily tick, instead of generating a freeform summary, plan:

1. **Goal** — derived from personality + open threads (debts, quarrels,
   upcoming festival).
2. **Steps** — sequence of schedulable actions with preconditions.
3. **Schedule** — merged with existing schedule resolver.

Generative Agents and Voyager show hierarchical plans are stable across long
horizons when combined with a memory store (doc 01).

### 4. Tree-of-Thought for branching decisions

For high-stakes moments (a wedding negotiation, a funeral), allow the planner
to expand 2–3 branches, evaluate each with the judge (doc 03), and pick the
best. Costly — gate per-scene.

### 5. Multi-agent coordination via blackboard

Tier 2 currently simulates each nearby NPC independently. Upgrade to a shared
scratchpad ("blackboard") for a co-located scene: each NPC posts intentions,
next pass conditions on peers. Avoids "two NPCs independently propose the same
action" artifacts.

### 6. World-model critic

Before accepting a plan, a small model validates against the rules engine
("Séan can't be at the forge at dawn — he's in Galway until Tuesday"). This is
cheap and catches the bulk of continuity bugs that reach Tier 4.

## Minimal first cut

1. Add `crates/parish-worldview` — read-only snapshot struct, deterministic.
2. Define tool schemas in `parish-schema`; hook into inference via the
   function-calling field.
3. Implement ReAct in `parish-npc::planner::react` with a hard step limit.
4. Gate `agentic-tier1` flag. Ship to hero NPCs only; measure latency before
   expanding.

## Risks

- Latency blowups from tool-call cascades. Enforce budget, cache tool results
  per turn.
- Runaway mutation: keep v1 strictly read-only, debate mutation tools in a
  follow-up ADR.
- Cloud vs local parity: cloud models do function-calling well, many local
  models do it poorly. Provide a grammar-based fallback (doc 02) where the
  model emits `tool_call: {...}` inside the structured output.

## Papers / references

- Yao et al., *ReAct: Synergizing Reasoning and Acting in Language Models* (2022).
- Yao et al., *Tree of Thoughts* (2023).
- Wang et al., *Voyager: An Open-Ended Embodied Agent with LLMs* (2023).
- Park et al., *Generative Agents* (2023) — hierarchical planning section.
- Schick et al., *Toolformer: Language Models Can Teach Themselves to Use Tools* (2023).
