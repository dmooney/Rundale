# Drama Manager / AI Director

**Target crate:** `crates/parish-npc/` (new `director` module) or a new
`crates/parish-director/`, a new Tier 5 lane in `crates/parish-inference/`.

## Problem

Rundale is a sandbox. Without narrative pressure, days blur into each other —
the same schedules, the same gossip cadence, no sense of momentum. Tier 4
rules produce *events* (births, illness, festivals), but not *arcs*. Players
report "nothing is happening" even when plenty is happening.

## SOTA technique

An AI Director borrowed from storylet / drama-management research (Mateas,
Riedl, *Left 4 Dead*'s director, Versu):

- Runs once per in-game day (or on an adaptive cadence driven by player
  idleness / engagement).
- Reads a compact world snapshot: mood aggregate, gossip volume, relationship
  deltas, open threads, player recency-of-contact per NPC, recent player
  choices.
- Outputs a small set of **directed events** as structured records
  (`DirectedEvent { kind, subjects, urgency, rationale }`).
- The simulation integrates events into Tier 2/3 ticks — the landlord comes
  to call, a stranger is seen on the road, Séan's illness worsens.

It is not a puppet master: it nudges, respecting NPC autonomy and the rules
engine. NPCs can refuse; the director adapts.

## Event archetypes (seed list)

- **Pressure:** landlord visit, rent due, bad harvest signal.
- **Entrance:** travelling stranger, returning emigrant, hedge-school master.
- **Crisis:** illness that could spread, livestock loss, fire.
- **Celebration:** impromptu gathering, courtship news, name day.
- **Reveal:** a hidden belief becomes public (gated by knowledge graph,
  doc 10).

Archetypes are authored in `mods/rundale/director.toml`; the LLM selects and
parameterises them, never invents them. Keeps output bounded and diegetic.

## Director prompt shape

```
World mood: subdued, rain for three days.
Player has not met the priest in 6 game-days.
Gossip volume: low in Ballygar, rising in Kilteevan.
Open threads: Máire's debt (unresolved), Séan's illness (day 2).

Pick at most 2 events from the archetype list. Prefer those that:
- pull on open threads the player has invested in;
- introduce a new relationship edge only if mood is stagnating;
- do not repeat an archetype fired in the last 3 days.
```

Output is grammar-constrained (doc 02) to the `DirectedEvent` schema.

## Player modelling feedback loop

Use the dialogue feedback channel from doc 06:

- Players who linger on NPCs → directed events pull on those NPCs.
- Players who travel → directed events seed at destination.
- Players who avoid combat / confrontation → pressure events soften.

The director becomes a responsive pacing engine, not a random-encounter
table.

## Minimal first cut

1. Author `mods/rundale/director.toml` with 10 archetypes.
2. Add `DirectedEvent` struct in `parish-types`; persist per-save.
3. Add a daily Tier 5 job that calls the director model (start with cloud
   Tier 1 quality; migrate to local after grammar + corpus are stable).
4. Tier 2 reads pending events and factors them into NPC context.
5. Flag `ai-director`; default off; expose director log in `/debug`.

## Effort & sequencing

Roughly 1.5 engineer-weeks. Best landed after doc 02 (grammar) and doc 10
(knowledge graph) so the director can reason about what *can* be revealed.

## Risks

- Over-direction produces "theme-park" feel. Keep event budget tight (≤ 2
  per day), and decay urgency if unused.
- Drama cliché collapse. Rotate archetypes, penalise recent repeats in the
  prompt itself.
- Latency on daily boundary. Director is Batch-lane; not on the hot path.

## Papers / references

- Mateas & Stern, *Façade* (2005) — dramatic beats and manager.
- Riedl & Bulitko, *Interactive Narrative: An Intelligent Systems Approach* (2013).
- Booth, *The AI Director of Left 4 Dead* (GDC 2009).
- Evans & Short, *Versu* (2014) — storylet / character-driven narrative.
- Kreminski et al., *Why Are We Like This? The AI Architecture of a Co-Creative
  Storytelling Game* (2022).
