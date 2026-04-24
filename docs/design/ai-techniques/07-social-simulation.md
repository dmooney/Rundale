# Social Simulation — Beliefs, Gossip, Multi-Agent

**Target crate:** `crates/parish-npc/` (new `beliefs` + `gossip` modules),
Tier 2 tick overhaul, `crates/parish-core/` social graph.

## Problem

Tier 4 rules already propagate gossip probabilistically, and Tier 2 moves mood
between adjacent NPCs. But:

- NPCs have no explicit *belief about what another NPC knows*. They "just
  know" village facts because those facts are in their prompt.
- Gossip is a scalar diffusion; the *content* doesn't mutate as it spreads.
  Real rumours distort.
- Tier 2 simulates each nearby NPC independently. Two NPCs can independently
  decide to comfort the same third — no coordination.

## SOTA techniques

### 1. Theory-of-mind / nested belief state

Each NPC carries, per other NPC it cares about, a small belief record:

```
believes(séan, that: "máire owes margaret 12 shillings", confidence: 0.7, learned: <date>)
```

Tier 1/2 prompts retrieve not the ground truth but *this NPC's belief set*.
An NPC who hasn't heard the news genuinely doesn't know it — no more prompt
leakage.

Implemented as a sub-store inside `LongTermMemory` keyed by subject NPC id.
Updated by both Tier 2 reasoning and by explicit gossip events (below).

### 2. Rumour mutation via summarisation

Each time a rumour passes between NPCs, run a tiny LLM call:

- Input: original rumour + teller's persona + listener's persona.
- Output: listener's now-internal version, possibly embellished or distorted.

Over hops, "Séan missed mass" becomes "Séan's refusing the sacraments" and
eventually "Séan's turned Protestant". Emergent game narrative at ~zero
design cost.

### 3. Multi-agent debate / CAMEL-style pairs

For co-located scenes (two NPCs + player), replace the independent Tier 2
passes with a joint pass:

- Shared scratchpad, turn-taking governed by the scene director (doc 04
  planner).
- Each NPC's output is conditioned on the prior utterance *and* its own
  belief state.

Society-of-Mind and MAD (Multi-Agent Debate) show this produces more
coherent scenes than independent sampling.

### 4. Graph neural spread model

Replace the scalar diffusion rule with a GNN operating on the village
relationship graph:

- Nodes: NPCs (feature: personality, role, current mood, known facts set).
- Edges: weighted relationships + physical co-location windows.
- Task: predict propagation probability per (fact, pair, tick).

Train on synthetic rollouts; deploy as a CPU-side inference every Tier 3
tick. Interprets personality ("the publican tells everyone") without a rule
per role.

### 5. Reputation & social credit

Maintain per-NPC reputation dimensions (piety, thrift, trustworthiness) that
update via a small rules network. These feed into how rumours are received
and how fast they propagate.

### 6. Emergent factions via community detection

Run Louvain / Leiden clustering on the relationship graph weekly in-game.
Emergent clusters become implicit factions. Tier 3 prompts can reference
faction membership without any authored faction list.

### 7. Agent-based economy (stretch)

Tier 4 already handles births/deaths. A Schelling-style micro-economy layer
would give NPCs simple resource goals (bread, peat, rent), feeding Tier 3
plans (doc 04). Keep classical — LLM is too expensive for per-tick market
clearing.

## Minimal first cut

1. Add `BeliefStore` as a thin wrapper over `LongTermMemory` keyed by subject.
2. Instrument Tier 2 ticks to write beliefs (not just global events).
3. Replace the current co-located-NPC loop with a single scene prompt that
   lists each NPC's current beliefs.
4. Flag `rumour-mutation` — on, add the one-shot distort pass per gossip hop.
5. Offline tooling: visualise the belief/rumour graph in the Designer Editor.

## Risks

- Combinatorial blow-up of beliefs. Cap per-NPC belief records at N=30 with
  eviction by confidence × recency.
- Rumour mutation can drift into nonsense; add a "sanity" LLM pass that
  rejects mutations violating a corpus of world invariants.
- Multi-agent scenes are slow. Budget tokens; fall back to sequential if
  over-latency.

## Papers / references

- Park et al., *Generative Agents* (2023) — reflection + relationship memory.
- Li et al., *CAMEL: Communicative Agents for "Mind" Exploration of LLMs* (2023).
- Du et al., *Improving Factuality and Reasoning via Multiagent Debate* (2023).
- Rabinowitz et al., *Machine Theory of Mind* (2018).
- Blondel et al., *Fast Unfolding of Communities in Large Networks* (2008).
