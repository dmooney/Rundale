# Open Questions

> Parent: [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)

Deferred design decisions that affect multiple phases. Each question includes context, options, trade-offs, and a recommended resolution timeline.

---

## 1. Exact Parish Location

**Context**: The starting area must be a real civil parish near Roscommon with interesting geography (lake, river, hills) and enough townlands to support 30-50 location nodes at full density.

**Options**:

| Parish | Barony | Features | Townlands | Notes |
|--------|--------|----------|-----------|-------|
| **Kiltoom** | Athlone South | River Shannon, Lough Ree shore, Hodson Bay | ~25 | Close to Athlone, good water features, accessible |
| **Kilbride** | Roscommon | Near Roscommon town, some lake access | ~20 | Central, but less dramatic geography |
| **Rahara** | Athlone South | Near Knockcroghery, Lough Ree | ~18 | Pottery heritage, compact |
| **Fuerty** | Athlone North | River Suck, inland, rolling farmland | ~30 | Rich agriculture, less water drama |

**Recommendation**: Kiltoom — best combination of water features (Lough Ree, Shannon), proximity to Athlone for urban contrast, and enough townlands for dense node mapping.

**Resolve by**: Phase 2 start (needed for location data file authoring).

**Depends on**: Nothing. This is a creative decision.

---

## 2. Player Character Model

**Context**: The player needs some form of in-world presence. The choice affects tutorial design, NPC interaction framing, and future quest potential.

**Options**:

**(a) Named local with history**
- Player has a name, family, job, house in the parish
- NPCs know them; relationships pre-exist
- Trade-off: rich roleplay but constraining; player can't choose who they are

**(b) Newcomer / "blow-in" arriving fresh**
- Player just moved to the parish (inherited a cottage, new job, etc.)
- NPCs don't know them; all relationships start from zero
- Natural tutorial: everything is new, NPCs explain things
- Trade-off: slightly slower start, but most flexible

**(c) Abstract observer with no in-world presence**
- Player is a disembodied presence; NPCs don't acknowledge them unless spoken to
- Simplest to implement; no player state to manage
- Trade-off: least immersive; breaks social simulation contract

**Recommendation**: Option (b) — newcomer. Provides the best balance of narrative justification ("Why am I here?"), natural onboarding, and player agency. The arrival reason can be left vague initially.

**Resolve by**: Phase 1 (affects how NPC context prompts frame the player).

**Depends on**: Parish selection (#1) for the arrival story.

---

## 3. Goal / Quest Structure

**Context**: Intentionally deferred. The sandbox must work before layering goals on top. However, the architecture should not preclude future quest systems.

**Options**:

**(a) Purely emergent** — Goals arise organically from NPC relationships and events. No authored content.
- Pro: maximizes the living world feel
- Con: risk of aimlessness; hard to create narrative tension

**(b) Authored quest lines** — Hand-written story arcs triggered by conditions.
- Pro: guaranteed compelling content
- Con: conflicts with emergent NPC behavior; expensive to author

**(c) Seasonal/annual objectives** — Soft goals tied to the calendar (prepare for Samhain, help with the harvest, organize the parish fete).
- Pro: natural pacing; uses existing time system
- Con: repetitive across years

**(d) Hybrid** — Emergent base with authored "anchor events" that create structure without railroading.
- Pro: best of both worlds
- Con: most complex to implement

**Recommendation**: Start with (a) for prototype. Design NPC system to support (d) later by ensuring events can trigger condition checks.

**Resolve by**: After Phase 3 (need working NPC relationships to evaluate what emerges naturally).

**Depends on**: Phase 3 NPC system, Phase 5 event bus.

---

## 4. Story and Lore

**Context**: What is the narrative frame? Why is the player here? What makes this parish interesting beyond being a simulation?

**Options**:

- **Mundane realism**: The parish is ordinary. Interest comes from the people, their lives, their secrets.
- **Underlying tension**: Something happened recently (a death, a scandal, a land dispute) that the player gradually uncovers through NPC gossip.
- **Mythological undercurrent**: The parish sits on thin ground between the mundane and the otherworld. Subtle strangeness at the margins.
- **Combination**: Mundane surface with hints of something deeper. The player decides how much to engage with the strange.

**Recommendation**: Combination. Start with mundane realism. The mythology hooks (Phase 6) create space for the strange to emerge without forcing it. A recent parish event (deferred content) gives NPCs something to talk about.

**Resolve by**: Phase 5-6 boundary (mythology hooks create the structural opportunity).

**Depends on**: Parish selection (#1), player character (#2), NPC system maturity (Phase 3).

---

## 5. Command Prefix UX

**Context**: Currently using `/` prefix for system commands. The design doc envisions prefix-free fuzzy matching with inline confirmation.

**Options**:

**(a) Keep `/` prefix** — Simple, unambiguous, familiar from games and chat systems.
- Pro: zero false positives; easy to implement
- Con: breaks immersion; "gamey"

**(b) Use `~` or another prefix** — Same approach, different character.
- Pro: less common, feels less like a chat command
- Con: still a prefix; arbitrary choice

**(c) Prefix-free with confirmation** — Type "quit", system detects it matches a command, shows "Quit the game? y/n".
- Pro: most immersive; design doc's preferred direction
- Con: false positives ("Tell him to quit whining" triggers quit detection); more complex input pipeline

**(d) Hybrid** — `/` always works; additionally, bare command words are detected with confirmation.
- Pro: power users use `/`, casual users get confirmation flow
- Con: two code paths to maintain

**Recommendation**: Start with (a), migrate to (d) in Phase 6. The `/` prefix works fine for a prototype. Prefix-free detection is a polish feature.

**Resolve by**: Phase 1 for initial implementation; Phase 6 for prefix-free upgrade.

**Depends on**: Input parsing maturity (Phase 1), LLM intent parsing reliability.

---

## 6. Mythology Content and Supernatural Events

**Context**: Phase 6 installs structural hooks (mythological location properties, festival events, NPC beliefs). The question is what content fills those hooks.

**Options**:

**(a) Subtle / atmospheric only** — Strange descriptions at night near the fairy fort. NPCs mention old stories. Nothing overt happens.
- Pro: maintains realism; lets player's imagination do the work
- Con: hooks exist but deliver nothing tangible

**(b) Moderate / behavioral** — NPCs behave strangely near festival dates. Livestock go missing near the fairy fort. An NPC claims to have seen something.
- Pro: emergent storytelling through NPC system; testable via Tier 1 prompts
- Con: must be carefully tuned to avoid feeling scripted

**(c) Overt / supernatural entities** — Fairy folk, pooka, banshee as NPC-like entities with their own cognition tier. Interact with the player.
- Pro: unique selling point; Irish mythology is underexplored in games
- Con: huge scope; requires its own NPC type; risks tonal whiplash

**Recommendation**: (b) for first pass. Use existing NPC cognition to create "strange" behavior by modifying context prompts near mythological locations during festivals and at night. No new entity types needed. Evaluate whether to escalate to (c) based on how (b) feels in practice.

**Resolve by**: After Phase 6 hooks are in place.

**Depends on**: Phase 5 seasonal system, Phase 6 mythology hooks, Phase 3 NPC cognition.

---

## 7. Player Verb Set

**Context**: Beyond movement and conversation, what actions can the player perform? This determines the `IntentKind` enum, the structured output schema, and the world interaction model.

**Options**:

**(a) Minimal** — `Move`, `Talk`, `Look`, `Examine`
- Pro: focused; conversation is the core mechanic; fast to implement
- Con: limited agency; player is essentially a walking camera

**(b) Moderate** — Above + `Take`, `Give`, `Trade`, `Work`, `Wait`
- Pro: physical interaction with the world; economic participation
- Con: needs inventory system, item model, trade logic

**(c) Expansive** — Above + `Steal`, `Fight`, `Romance`, `Craft`, `Build`
- Pro: full simulation; maximum player expression
- Con: massive scope; each verb needs NPC response handling, world state effects, persistence

**Recommendation**: Start with (a) for Phase 1. Add `Take`, `Give`, `Wait` in Phase 3 when NPCs can react to items. Defer `Trade`, `Work` to Phase 5. Defer `Steal`, `Fight`, `Romance` indefinitely — these are content-heavy and may not suit the tone.

**Implementation notes**: The `IntentKind` enum should be non-exhaustive (`#[non_exhaustive]`) to allow future extension. The `NpcAction` structured output schema already includes a flexible `action: String` field that can accommodate new verbs without schema changes.

**Resolve by**: Phase 1 for initial set; revisit at each phase boundary.

**Depends on**: NPC response capabilities, world interaction model, item/inventory system (not yet designed).
