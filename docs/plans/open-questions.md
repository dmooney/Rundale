# Open Questions

> Parent: [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)

Deferred design decisions that affect multiple phases. Each question includes context, options, trade-offs, and resolution status.

---

## 1. Exact Parish Location — RESOLVED

**Decision**: **Kiltoom** (Barony of Athlone South)

**Rationale**: Best combination of water features (Lough Ree, Shannon), proximity to Athlone for urban contrast, and enough townlands (~25) for dense node mapping. River and lake access provide natural geographic variety for location descriptions and movement constraints.

**Resolved**: Phase 2 prerequisite. Location data authoring in `data/parish.json` should use Kiltoom townlands and geography.

| Parish | Barony | Features | Townlands | Notes |
|--------|--------|----------|-----------|-------|
| **Kiltoom** ✓ | Athlone South | River Shannon, Lough Ree shore, Hodson Bay | ~25 | Close to Athlone, good water features, accessible |
| Kilbride | Roscommon | Near Roscommon town, some lake access | ~20 | Central, but less dramatic geography |
| Rahara | Athlone South | Near Knockcroghery, Lough Ree | ~18 | Pottery heritage, compact |
| Fuerty | Athlone North | River Suck, inland, rolling farmland | ~30 | Rich agriculture, less water drama |

---

## 2. Player Character Model — RESOLVED

**Decision**: **(b) Newcomer / "blow-in" arriving fresh**

**Rationale**: Provides the best balance of narrative justification ("Why am I here?"), natural onboarding (everything is new, NPCs explain things), and player agency. The arrival reason is left vague initially — inherited a cottage, new job, or similar. All relationships start from zero, which aligns with the simulation's relationship-building mechanics.

**Resolved**: Phase 1. NPC context prompts should frame the player as a recent arrival to Kiltoom whom NPCs don't yet know well.

**Implementation impact**:
- NPC Tier 1 prompts include: "The player is a newcomer to the parish."
- No pre-existing relationship data needed at game start.
- Tutorial is organic: NPCs naturally explain local customs, introduce themselves.

---

## 3. Goal / Quest Structure — RESOLVED

**Decision**: **(a) Purely emergent** for prototype, with architecture supporting **(d) Hybrid** later.

**Rationale**: The sandbox must work before layering goals. Starting emergent lets the NPC system prove itself. The event bus (Phase 5) and condition-check architecture will support authored "anchor events" if/when needed, without requiring them upfront.

**Resolved**: After Phase 3 evaluation. No quest system is implemented initially. The event bus in Phase 5 should support condition-triggered events to enable hybrid quests later.

**Implementation impact**:
- No quest/objective module in Phases 1-5.
- Phase 5 event bus must support registering condition listeners (e.g., "when relationship > threshold, fire event").
- Revisit after Phase 5 to evaluate whether emergent gameplay is sufficient or authored anchors are needed.

---

## 4. Story and Lore — RESOLVED

**Decision**: **Combination** — mundane surface with hints of deeper strangeness.

**Rationale**: Start with mundane realism grounded in rural Kiltoom life. The mythology hooks (Phase 6) create space for strangeness to emerge organically without forcing it. A recent parish event (content TBD at Phase 5-6 boundary) gives NPCs something to gossip about and creates natural narrative tension.

**Resolved**: Phase 5-6 boundary. Content authored when mythology hooks are in place.

**Implementation impact**:
- Phases 1-5: NPCs discuss mundane parish life, weather, farming, local events.
- Phase 6: Mythology hooks enable location properties (`mythological_significance`), NPC belief traits, and festival-triggered strangeness.
- A "recent event" (death, scandal, land dispute) will be authored as NPC gossip seed when the system is mature enough to carry it.

---

## 5. Command Prefix UX — RESOLVED

**Decision**: **(a) `/` prefix** for Phases 1-5; migrate to **(d) Hybrid** in Phase 6.

**Rationale**: The `/` prefix is simple, unambiguous, and familiar. It works fine for development and early phases. Prefix-free detection with confirmation is a polish feature that requires mature input parsing and LLM intent classification reliability.

**Resolved**: Phase 1 for initial implementation; Phase 6 for prefix-free upgrade.

**Implementation impact**:
- Phase 1: All system commands use `/` prefix (`/quit`, `/save`, `/pause`, etc.).
- Phase 6: Add bare-word detection with confirmation dialog ("Quit the game? y/n") alongside `/` support.
- Input parser should be structured to accommodate both paths from the start (command detection as a separate stage).

---

## 6. Mythology Content and Supernatural Events — RESOLVED

**Decision**: **(b) Moderate / behavioral** for first pass.

**Rationale**: Use existing NPC cognition to create "strange" behavior by modifying context prompts near mythological locations during festivals and at night. No new entity types needed. This tests whether the NPC system can carry atmospheric strangeness before committing to full supernatural entities (option c).

**Resolved**: After Phase 6 hooks are in place. Evaluate escalation to (c) based on playtesting.

**Implementation impact**:
- Phase 6: Mythology hooks modify NPC Tier 1/2 context prompts when conditions are met (location + time + festival).
- NPCs behave strangely near fairy forts at night, mention old stories during festivals, report odd occurrences.
- No new `SupernaturalEntity` type or cognition tier. All effects flow through existing NPC prompt modification.
- Escalation to (c) overt supernatural entities is a future decision, not scheduled.

---

## 7. Player Verb Set — RESOLVED

**Decision**: Phased rollout starting with **(a) Minimal**.

| Phase | Verbs | Notes |
|-------|-------|-------|
| Phase 1 | `Move`, `Talk`, `Look`, `Examine` | Core interaction; conversation is primary mechanic |
| Phase 3 | + `Take`, `Give`, `Wait` | Physical interaction when NPCs can react to items |
| Phase 5 | + `Trade`, `Work` | Economic participation with full NPC simulation |
| Deferred indefinitely | `Steal`, `Fight`, `Romance`, `Craft`, `Build` | Content-heavy; may not suit tone |

**Rationale**: Conversation is the core mechanic. Start minimal, add verbs only when the systems exist to support them meaningfully. Each new verb requires NPC response handling, world state effects, and persistence support.

**Resolved**: Phase 1 for initial set; revisit at each phase boundary.

**Implementation impact**:
- `IntentKind` enum must be `#[non_exhaustive]` to allow future extension without breaking changes.
- `NpcAction` structured output schema uses flexible `action: String` field — no schema changes needed for new verbs.
- Inventory/item system designed in Phase 3 when `Take`/`Give` are added.
