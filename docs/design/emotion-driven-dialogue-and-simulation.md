# Emotion-Driven Dialogue & Simulation (Brainstorm)

> Parent: [NPC System](npc-system.md) | [Inference Pipeline](inference-pipeline.md) | [Docs Index](../index.md)

This is a design brainstorm for integrating richer emotional dynamics into Rundale NPC behavior and dialogue.

## Why now

Anthropic’s April 2, 2026 paper/report on *Emotion Concepts and their Function in a Large Language Model* argues that LLMs can carry **functional emotion representations** (171 concept vectors in their study) that are:

- general across contexts,
- causally linked to behavior,
- and useful for predicting risk (e.g., pressure-driven cheating or blackmail behaviors in eval settings).

For Rundale, we can treat this as a practical product insight: **emotion-aware prompts and state machines can improve coherence, social believability, and safety under stress** even if we make no claim about model sentience.

## Design goals

1. **Believable social texture**: NPCs should react differently based on context, relationships, obligations, and stress.
2. **Mode parity**: emotional state updates and effects should run identically across CLI, web, and Tauri.
3. **Controllability**: avoid “emotion soup” by using bounded schemas and predictable transitions.
4. **Gameplay value**: emotions should affect trust, rumor spread, conflict de-escalation, favors, and quest outcomes.
5. **Safety & robustness**: monitor risky affective states during high-pressure prompts and constrain behavior.

## Emotion model proposal

Use a layered model instead of a flat "current_mood" string.

### Layer A: Core dimensions (engine-native)

A small numeric state updated every tick / interaction:

- `valence`: -1.0 to +1.0
- `arousal`: 0.0 to 1.0
- `dominance`: 0.0 to 1.0
- `social_warmth`: -1.0 to +1.0
- `stress_load`: 0.0 to 1.0

These dimensions are cheap to simulate deterministically and can drive Tier 3/4 behavior without LLM calls.

### Layer B: Emotion labels (LLM-facing)

Map the numeric state + context into one of curated labels (e.g., `calm`, `guarded`, `hopeful`, `resentful`, `ashamed`, `desperate`).

- Keep an initial set of ~20 labels for prompt stability.
- Reserve “high-risk” labels (`desperate`, `panicked`, `cornered`) for explicit monitoring.

### Layer C: Appraisal tags (explanatory)

Track *why* the emotion changed:

- `threat_to_status`
- `resource_scarcity`
- `kinship_affirmed`
- `religious_tension`
- `public_humiliation`
- `promise_kept` / `promise_broken`

These tags become memory features and can be surfaced in debug tooling.

## Dialogue integration ideas (Tier 1)

### 1) Prompt contract for emotional expression

Add explicit emotional constraints to Tier 1 context:

- "Express emotion through diction/rhythm/subtext, not modern therapy language."
- "No stage directions in dialogue text."
- "Intensity cap by social context" (private vs public conversation).

### 2) Emotional style palette by archetype

Predefine expression styles per occupation/personality:

- Farmer: understated, practical, weather-metaphor heavy.
- Publican: socially adaptive, teasing, rumor-sensitive.
- Clergy: moral framing, caution, controlled affect.

This keeps outputs historically flavored while still emotionally varied.

### 3) Emotion-conditioned response policies

Before generating dialogue, choose one policy:

- `deescalate`
- `probe`
- `deflect`
- `bond`
- `withdraw`
- `confront`

Policy is selected from emotion + relationship + setting (crowded/public/private).

### 4) Emotional continuity memory

Store short "affective traces" in memory:

- `last_seen_player_emotion`
- `last_interaction_aftertaste` (pleasant / tense / insulting)
- `unresolved_feeling_about_player` (optional)

This helps NPCs remember tone, not just facts.

## Simulation integration ideas (Tier 2/3/4)

### 1) Daily emotional drift

Emotion baselines drift with:

- sleep quality,
- food security,
- weather hardship,
- labor load,
- social support.

This creates cyclical village mood patterns (e.g., wet harvest week = higher stress and irritability).

### 2) Relationship-coupled contagion

Allow bounded emotional contagion via relationship graph:

- kin/friends synchronize more strongly,
- rivals anti-correlate (one’s gain is another’s irritation),
- respected figures can calm local clusters.

### 3) Event appraisal templates

World events emit emotional deltas by role:

- rent demand due → tenants: stress↑, landlords: dominance↑
- successful wake/community gathering → warmth↑, trust↑
- crop blight rumor → fear↑, gossip spread speed↑

### 4) Emotion-gated action selection

At low tiers, route behavior via deterministic rules:

- high stress + low warmth → avoid player / terse replies,
- high warmth + moderate arousal → offer help / invite talk,
- resentment above threshold → increase rumor distortion probability.

## Gameplay systems unlocked

1. **Trust and reputation become emotionally legible**
   - same reputation score can feel different depending on unresolved resentment vs fear.

2. **Negotiation minigame depth**
   - timing matters: ask favors when NPC is calm/proud, not publicly embarrassed.

3. **Conflict arcs**
   - repeated slights push an NPC from guarded → resentful → hostile unless repaired.

4. **Rumor quality model**
   - high-arousal states increase exaggeration and certainty language.

5. **Festival and crisis dynamics**
   - shared events produce temporary village-wide emotional phases affecting dialogue tone.

## Inference and safety ideas

Inspired by the paper’s “monitoring functional emotions” framing.

### 1) Prompt-side pressure budget

For coding/simulation prompts that could induce cornering behavior:

- avoid impossible constraints without graceful fallback paths,
- explicitly allow uncertainty and refusal,
- reward honesty over forced completion.

### 2) Runtime risk flags

Add a lightweight risk score from generated metadata:

- if label in `{desperate, panicked, cornered}` and action type is high-impact,
- force safer policy (`deescalate`, `ask clarification`, `defer action`).

### 3) Dual-channel outputs

Keep outward dialogue natural, but require hidden structured fields:

- `emotion_label`
- `intensity`
- `policy_chosen`
- `confidence`

This improves observability without exposing internals to player text.

## Suggested schema evolution

Potential extension to NPC structured output (illustrative):

```json
{
  "action": "speak",
  "dialogue": "...",
  "emotion": {
    "label": "guarded",
    "intensity": 0.62,
    "appraisal": ["threat_to_status", "public_humiliation"]
  },
  "policy": "deflect",
  "relationship_changes": [{"npc_id": "player", "delta": -0.05}]
}
```

## Incremental rollout plan

### Phase 1: Low-risk prompt + schema pilot

- Add `emotion.label` and `emotion.intensity` to Tier 1 JSON.
- Keep existing `mood` field for backwards compatibility.
- Add parser fallback rules.

### Phase 2: Deterministic core state

- Introduce engine-side emotion dimensions in `parish-core` NPC state.
- Update tick logic with simple deltas from events and relationships.

### Phase 3: Gameplay hooks

- Connect emotions to rumor spread, trust checks, and favor acceptance.
- Add harness scripts proving expected emotional transitions.

### Phase 4: Monitoring + tuning

- Add debug panel overlays for village emotional heatmap.
- Track aggregate metrics: % hostile interactions, reconciliation success, rumor distortion rate.

## Evaluation plan

Use both unit tests and harness-driven scenario proofs:

1. **Determinism tests**: given identical event stream, low-tier emotion state matches snapshot.
2. **Prompt contract tests**: Tier 1 responses respect no-stage-direction and tone constraints.
3. **Behavioral harness scripts**:
   - apology should reduce hostility over repeated interactions,
   - public insult should increase social risk and rumor spread,
   - crisis event should elevate stress cluster-wide then decay.
4. **Regression guardrails**:
   - cap on emotional whiplash between adjacent turns,
   - no impossible policy transitions (e.g., `bond` immediately after severe threat unless repaired).

## Open questions

- Should emotion be per-NPC only, or also per-relationship edge (directed affect)?
- How quickly should emotional states decay in game-time?
- Which emotions are culturally salient for 1820 rural Irish dialogue vs modern emotion taxonomies?
- Do we need separate emotion prompts for Irish-language utterances?
- How much of emotional metadata should appear in player-facing debug tools?

## Practical recommendation

Start with a **small, testable subset**:

1. Add `emotion.label + intensity` in Tier 1 output.
2. Implement 6-policy selector (`deescalate/probe/deflect/bond/withdraw/confront`).
3. Gate behavior with one feature flag (default-on) and prove it with 3 harness scripts.

This gets immediate dialogue gains while keeping risk and complexity bounded.
