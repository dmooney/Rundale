# NPC Inference Prompt Immersion Ideas

This document proposes prompt-level improvements for Tier 1 NPC interactions to increase felt immersion while preserving existing safety and structured-output constraints.

## 1) Add a hidden "micro-objective" line per turn

Inject one private objective for the current turn into context, for example:

- `Current private objective: learn why the player is here before sharing gossip.`
- `Current private objective: avoid mentioning debt unless directly asked.`

This gives each response directional intent, reducing generic politeness loops.

## 2) Add "social risk" and "stakes" context

Add short fields to context:

- `Social risk right now: medium (neighbors can overhear)`
- `Personal stakes: high (rent due in 2 days)`

NPCs become more situationally strategic and less uniformly friendly.

## 3) Include a one-line "subtext" target

Prompt for internal acting motivation each turn:

- `Subtext for this reply: be warm on the surface, but probe for trustworthiness.`

This tends to generate layered dialogue without exposing chain-of-thought.

## 4) Add sensory anchors from location + weather + time

Provide 2–3 concrete sensory cues in context (sound, smell, touch) derived from world state:

- peat smoke in clothes
- wet cobblestones underfoot
- distant cart wheels

This produces grounded lines that feel present-tense and embodied.

## 5) Add conversation rhythm guidance

Use lightweight style constraints:

- default 1–2 short sentences unless emotionally charged
- ask at most one question per turn
- avoid repeating greeting formulas in consecutive turns

This avoids repetitive cadence and keeps exchanges dynamic.

## 6) Add relationship-intent coupling

For each relationship mention, include an implication:

- `Bridget (strained): avoid praising her brother in public.`

It converts static labels into actionable behavior.

## 7) Add "what changed since last turn" delta

Inject explicit deltas, not just full memory blocks:

- `Since your last reply: player defended you in front of Seamus.`

Deltas improve continuity and help NPCs react to immediate developments.

## 8) Add taboo / boundary list per NPC

Include 1–3 boundaries to preserve voice consistency:

- `Avoid discussing: family debts, landlord dispute details.`

This creates distinct limits and encourages reveal pacing.

## 9) Add an escalation ladder

Include a scene pressure value and a rule:

- `Scene tension: 2/5. Escalate by one step only if player presses.`

Helps drama feel gradual instead of jumping between extremes.

## 10) Add dialogue-act target in metadata

Keep existing output format, but optionally add a compact field:

- `dialogue_act: greet|probe|deflect|warn|joke|confide|threaten`

This can support anti-repetition logic and future adaptive steering.

## 11) Add anti-anachronism fallback behavior

When uncertain about modern references, instruct NPC to reinterpret in-world:

- treat unknown modern term as rumor, foreign invention, or misunderstanding

This preserves immersion instead of producing awkward refusals.

## 12) Add lightweight memory salience tags

Tag recalled memories by relevance and emotional weight before injection:

- `[high relevance][high emotion]`
- `[low relevance][neutral]`

Prioritization reduces noise and improves coherence.

## Suggested rollout order

1. Context-only additions (micro-objective, deltas, sensory anchors, stakes).
2. Rhythm and escalation rules.
3. Optional `dialogue_act` metadata and tuning based on logs.

## Metrics to validate immersion gains

- Lower repeated n-gram rate across consecutive NPC turns.
- Higher reference rate to immediate scene details.
- Higher continuity score (mentions of recent-turn deltas).
- Stable JSON-parse success rate (no regression in structured output).
