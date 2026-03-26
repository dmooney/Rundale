# Game Ideas Brainstorm

> Back to [Documentation Index](../index.md) | [Architecture Overview](overview.md)

A collection of brainstormed gameplay ideas for Parish, organized by theme. These are early-stage concepts — not commitments. Each idea notes how it connects to existing systems and which development phase it would likely fit into.

---

## Emergent Social Systems

### 1. Faction & Allegiance Dynamics

NPCs organically align into factions (e.g., loyalists vs. agrarian agitators, old families vs. newcomers). The player's actions shift their standing with each group, opening or closing doors. No scripted quests — just consequences.

- **Connects to**: NPC relationships (`RelationshipKind`), reputation, gossip propagation
- **Phase fit**: 5–6 (requires gossip system and expanded NPC memory)

### 2. Reputation as Currency

A multi-dimensional reputation system: trusted/distrusted, generous/stingy, devout/skeptical, Irish-speaking/Anglicized. NPCs adjust behavior based on what they've *heard* about the player through gossip, not just direct experience.

- **Connects to**: Gossip propagation (Phase 5), NPC memory, Tier 1 prompt context
- **Phase fit**: 5 (natural extension of gossip system)

### 3. Matchmaking & Social Maneuvering

Marriage is a major social institution in 1820 Ireland. NPCs could try to match the player (or each other) with eligible partners. The player could play matchmaker, creating alliances or rivalries between families.

- **Connects to**: NPC relationships, family structures, daily schedules
- **Phase fit**: 6 (requires mature NPC relationship system)
- **Research**: [Family Life](../research/family-life.md)

### 4. Confession & Secrets

Fr. Tierney hears confessions. NPCs carry secrets (affairs, debts, stolen goods). Information leaks through gossip with distortion. The player could become a keeper — or a spreader — of secrets.

- **Connects to**: NPC knowledge/gossip fields, Fr. Tierney NPC, gossip propagation
- **Phase fit**: 5–6 (secrets as a gossip subtype)
- **Research**: [Religion & Spirituality](../research/religion-spirituality.md)

---

## Economic & Survival Gameplay

### 5. Land & Tenancy

The player could acquire a smallholding, manage crops/livestock, and deal with rent collectors. Seasonal agricultural cycles (planting, harvest) create natural rhythms. Bad weather or blight threatens livelihood.

- **Connects to**: Time system (seasons), weather system, NPC land agents
- **Phase fit**: 6+ (new player state system required)
- **Research**: [Farming & Agriculture](../research/farming-agriculture.md), [Economy & Trade](../research/economy-trade.md)

### 6. Trade Network

Establish trade routes between the parish and Roscommon/Athlone. Buy low at Connolly's shop, sell high at market day. NPCs have their own trade needs, creating a dynamic micro-economy.

- **Connects to**: Expanded world graph (Phase 5), Connolly's Shop, NPC schedules
- **Phase fit**: 5–6 (requires expanded world with towns)
- **Research**: [Economy & Trade](../research/economy-trade.md), [Transportation](../research/transportation.md)

### 7. Craft & Apprenticeship

Learn skills from NPCs: blacksmithing from Tom Flanagan, healing from Brigid Nolan, literacy from Seamus Duffy. Skills unlock new interactions and actions, gated by time investment and relationship quality.

- **Connects to**: Existing NPCs (Tom, Brigid, Seamus), NPC relationships, time system
- **Phase fit**: 6 (new player progression system)
- **Research**: [Technology & Crafts](../research/technology-crafts.md), [Education & Literacy](../research/education-literacy.md)

### 8. Poitín Economy

An underground distilling economy. Illegal but widespread. Risk of informers, excise men, and raids adds tension. Who do you trust?

- **Connects to**: NPC secrets, faction dynamics, law enforcement NPCs (future)
- **Phase fit**: 6+ (requires legal/enforcement layer)
- **Research**: [Food & Drink](../research/food-drink.md), [Crime & Secret Societies](../research/crime-secret-societies.md)

---

## Atmosphere & Mythology

### 9. Liminal Spaces & Fairy Encounters

The Fairy Fort already exists as a location. At certain times (dusk, Samhain, solstices), strange things could happen there — lights, music, missing time. Not horror, but eerie Irish folklore played straight.

- **Connects to**: Fairy Fort location, time system (festivals, time of day), mythology hooks
- **Phase fit**: 6 (mythology hooks already scaffolded)
- **Research**: [Mythology & Folklore](../research/mythology-folklore.md)

### 10. The Banshee

When an NPC is going to "die" (leave the simulation permanently), a banshee is heard the night before. Creates dread and foreshadowing. Players who investigate might learn something — or might not.

- **Connects to**: NPC lifecycle (Tier 4 life events), time system (night), ambient sound system
- **Phase fit**: 6 (requires NPC lifecycle events)
- **Research**: [Mythology & Folklore](../research/mythology-folklore.md)

### 11. Holy Well Pilgrimages

The Well could become a pilgrimage site with healing properties (believed or real). NPCs visit for ailments. Seasonal "pattern days" draw crowds from outside the parish, bringing news and strangers.

- **Connects to**: The Well location, NPC schedules, festival system, Brigid Nolan (healer)
- **Phase fit**: 5–6 (enriches existing locations and schedules)
- **Research**: [Religion & Spirituality](../research/religion-spirituality.md), [Medicine & Health](../research/medicine-health.md)

### 12. Weather as Storytelling

A prolonged fog could strand travelers. A storm could damage the bridge, cutting off part of the world graph temporarily. Weather becomes a plot device, not just atmosphere.

- **Connects to**: Weather state machine (Phase 5), world graph edges, movement system
- **Phase fit**: 5 (natural extension of weather system)
- **Research**: [Flora, Fauna & Landscape](../research/flora-fauna-landscape.md)

---

## Political & Historical Tensions

### 13. Tithe Resistance

The tithe war is brewing historically. NPCs could debate, resist, or comply with tithes to the established church. The player's stance has real consequences — support the priest or the protesters?

- **Connects to**: NPC factions, Fr. Tierney, reputation system, gossip
- **Phase fit**: 6 (requires faction/reputation systems)
- **Research**: [Law & Governance](../research/law-governance.md), [Religion & Spirituality](../research/religion-spirituality.md)

### 14. The Hedge School Under Threat

Seamus Duffy's school operates in a legal gray area. An informer or a new magistrate could threaten it. The player could help protect it, becoming entangled in local politics.

- **Connects to**: Seamus Duffy NPC, Schoolhouse location, NPC secrets/knowledge
- **Phase fit**: 6 (emergent political events)
- **Research**: [Education & Literacy](../research/education-literacy.md), [Law & Governance](../research/law-governance.md)

### 15. Land Agent Visits

Periodic visits from the absentee landlord's agent create pressure. Eviction threats, rent increases, demands. The parish must respond collectively — and the player can influence how.

- **Connects to**: Expanded world graph (agents arrive from outside), NPC factions, time system
- **Phase fit**: 5–6 (requires external NPC visitors)
- **Research**: [Demographics & Social Structure](../research/demographics-social-structure.md), [Economy & Trade](../research/economy-trade.md)

### 16. Catholic Emancipation Movement

Daniel O'Connell's campaign is gaining steam by 1820. News arrives slowly via letters and travelers. NPCs take sides. Political meetings at the pub become tense.

- **Connects to**: Letter Office location, Darcy's Pub, NPC knowledge, gossip propagation
- **Phase fit**: 6 (requires information-from-outside-parish system)
- **Research**: [Politics & Movements](../research/politics-movements.md), [Recent History](../research/recent-history-pre1820.md)

---

## Player Identity & Integration

### 17. The "Blow-In" Arc

The player starts as an outsider. Trust is earned slowly. Early on, NPCs are guarded. Over time (measured in game-days, not quests), doors open. Some NPCs never fully accept you.

- **Connects to**: NPC relationships (player-specific), reputation, Tier 1 prompts
- **Phase fit**: 5 (adjust NPC prompt templates to reflect familiarity over time)

### 18. Irish Language Progression

The Focail sidebar could become interactive. Learn words from NPCs, use them in conversation. NPCs respond more warmly to Irish speakers. A bilingual gameplay layer.

- **Connects to**: Focail UI panel, NPC dialogue, player state
- **Phase fit**: 6 (new player progression + UI interaction)
- **Research**: [Irish Language](../research/irish-language.md)

### 19. Letter Writing

The Letter Office exists as a location. The player could send and receive letters to/from outside the parish — family, contacts in Dublin, emigrated relatives in America. A window to the wider world and a source of narrative hooks.

- **Connects to**: Letter Office location, time system (delivery delays), world outside the parish
- **Phase fit**: 5–6 (new interaction type at existing location)
- **Research**: [Transportation](../research/transportation.md)

### 20. Dreams & Visions

When sleeping, the player could experience dream sequences — fragments of memory, folklore imagery, or premonitions. A way to inject narrative without breaking the emergent sandbox.

- **Connects to**: Time system (night/sleep), mythology hooks, ambient atmosphere
- **Phase fit**: 6 (mythology layer)
- **Research**: [Mythology & Folklore](../research/mythology-folklore.md)

---

## Priority Assessment

Ideas that build most naturally on existing systems and could be implemented soonest:

| Priority | Ideas | Rationale |
|----------|-------|-----------|
| **High** | 2 (Reputation), 12 (Weather as storytelling), 17 (Blow-in arc) | Extend existing systems with minimal new architecture |
| **Medium** | 1 (Factions), 4 (Secrets), 9 (Fairy encounters), 11 (Holy well) | Require gossip/memory systems from Phase 5 |
| **Lower** | 5 (Land), 7 (Crafts), 8 (Poitín), 18 (Irish language) | Need new player state/progression systems |

---

*Document created: 2026-03-26*
