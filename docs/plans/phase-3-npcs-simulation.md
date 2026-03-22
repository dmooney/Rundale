# Plan: Phase 3 — Multiple NPCs & Simulation

> Parent: [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)
>
> **Status: Planned**

## Goal

Populate the parish with 5-10 NPCs that follow daily schedules, maintain relationships, interact with each other at Tier 2 fidelity, and provide rich Tier 1 dialogue when the player is present.

## Prerequisites

- Phase 2 complete: world graph with multiple locations, movement, time advancement
- Inference pipeline working (Phase 1)

## Tasks

1. **Extend `Npc` struct in `src/npc/mod.rs`**
   - Add fields: `home: LocationId`, `workplace: Option<LocationId>`, `schedule: DailySchedule`, `relationships: HashMap<NpcId, Relationship>`, `memory: ShortTermMemory`, `knowledge: Vec<String>`
   - `DailySchedule` struct: `weekday: Vec<ScheduleEntry>`, `weekend: Vec<ScheduleEntry>`, `overrides: HashMap<Season, Vec<ScheduleEntry>>`
   - `ScheduleEntry` struct: `start_hour: u8`, `end_hour: u8`, `location: LocationId`, `activity: String`

2. **Implement `Relationship` struct in `src/npc/mod.rs`**
   - Fields: `target: NpcId`, `kind: RelationshipKind`, `strength: f32` (-1.0 to 1.0), `history: Vec<RelationshipEvent>`
   - `RelationshipKind` enum: `Family`, `Friend`, `Neighbor`, `Rival`, `Enemy`, `Romantic`, `Professional`
   - `RelationshipEvent` struct: `timestamp: DateTime<Utc>`, `description: String`, `delta: f32`

3. **Implement `ShortTermMemory` in `src/npc/mod.rs`**
   - Ring buffer of last 20 `MemoryEntry` items
   - `MemoryEntry` struct: `timestamp: DateTime<Utc>`, `content: String`, `participants: Vec<NpcId>`, `location: LocationId`
   - `fn add(&mut self, entry: MemoryEntry)` — push, evict oldest if full
   - `fn recent(&self, n: usize) -> Vec<&MemoryEntry>` — last N entries
   - `fn context_string(&self) -> String` — format memories as prompt context

4. **Implement `NpcManager` in `src/npc/mod.rs`**
   - `NpcManager` struct: `npcs: HashMap<NpcId, Npc>`, `tier_assignments: HashMap<NpcId, CogTier>`
   - `CogTier` enum: `Tier1`, `Tier2`, `Tier3`, `Tier4`
   - `fn assign_tiers(&mut self, player_location: LocationId, graph: &WorldGraph)` — Tier1: same location, Tier2: 1-2 edges away, Tier3: 3+ edges, Tier4: far away
   - `fn npcs_at(&self, location: LocationId) -> Vec<&Npc>`
   - `fn get_mut(&mut self, id: NpcId) -> Option<&mut Npc>`

5. **Implement Tier 1 tick in `src/npc/mod.rs`**
   - `async fn tick_tier1(npc: &mut Npc, world: &WorldState, player_input: Option<&str>, queue: &InferenceQueue) -> Result<NpcAction>`
   - Build full context: system prompt (personality, backstory, mood) + world context (location, time, weather, who else is here) + memory context + player action
   - Send inference request, await `NpcAction` structured JSON response
   - Apply action: update `npc.mood`, add to memory, generate dialogue text for TUI

6. **Implement Tier 2 tick in `src/npc/mod.rs`**
   - `async fn tick_tier2(npcs: &mut [&mut Npc], world: &WorldState, queue: &InferenceQueue) -> Result<Vec<Tier2Event>>`
   - Group NPCs by location; for each group, build a lighter prompt: "These people are together at {location}. Briefly describe what happens."
   - `Tier2Event` struct: `location: LocationId`, `participants: Vec<NpcId>`, `summary: String`, `relationship_changes: Vec<(NpcId, NpcId, f32)>`
   - Tick rate: every 5 game-minutes

7. **Implement NPC schedule following**
   - `fn desired_location(npc: &Npc, clock: &GameClock) -> LocationId` — check schedule for current time/day, return target location
   - In world tick: for each NPC, if `npc.location != desired_location`, move NPC (update `npc.location`)
   - NPCs don't teleport: calculate traversal time, mark NPC as `in_transit` during movement
   - `NpcState` enum: `Present(LocationId)`, `InTransit { from: LocationId, to: LocationId, arrives_at: DateTime<Utc> }`

8. **Create initial NPC data file: `data/npcs.json`**
   - 8 NPCs with distinct personalities:
     - Padraig Darcy (publican, 58, gregarious, knows everyone's business)
     - Siobhan Murphy (farmer, 45, practical, sharp wit)
     - Fr. Declan Tierney (parish priest, 62, kind but traditional)
     - Roisin Connolly (shopkeeper, 38, ambitious, modernizer)
     - Tommy O'Brien (farmer, 70, storyteller, set in his ways)
     - Aoife Brennan (teacher, 29, idealistic, recently returned from Dublin)
     - Mick Flanagan (retired guard, 65, observant, dry humor)
     - Niamh Darcy (Padraig's daughter, 22, restless, wants to leave)
   - Each with schedule, home, workplace, initial relationships to 3+ other NPCs

9. **Implement "overhear" mechanic**
   - When Tier 2 events resolve at a location 1 edge from the player, store the summary
   - `fn check_overhear(events: &[Tier2Event], player_location: LocationId, graph: &WorldGraph) -> Vec<String>`
   - Surface overheard snippets in the TUI: "You catch a few words drifting from the direction of the pub..."

10. **Integrate NPC ticks into main game loop**
    - After each player action, run `assign_tiers`, then `tick_tier1` for immediate NPCs, `tick_tier2` for nearby NPCs
    - Tier 2 ticks run on a timer (every 5 game-minutes), not per player action
    - Display NPC dialogue/actions in text log

11. **Write tests**
    - `test_tier_assignment`: place player at pub, assert publican is Tier1, nearby farmer is Tier2
    - `test_schedule_movement`: advance clock to evening, assert NPC has moved from workplace to pub
    - `test_short_term_memory`: add 25 entries, assert only last 20 retained
    - `test_relationship_graph`: verify bidirectional relationship queries
    - `test_overhear_range`: assert events 1 edge away are overhearable, 2+ edges are not

## Design References

- [Cognitive LOD](../design/cognitive-lod.md)
- [Architecture Overview](../design/overview.md)

## Key Decisions

- [ADR-002: Cognitive LOD Tiers](../adr/002-cognitive-lod-tiers.md)
- [ADR-005: Ollama Local Inference](../adr/005-ollama-local-inference.md)

## Acceptance Criteria

- 8 NPCs populate the parish, each at their scheduled location based on time of day
- Talking to an NPC at the same location produces contextual Tier 1 dialogue
- Tier 2 NPC-NPC interactions generate summaries visible via overhear mechanic
- NPC mood and relationships update based on interactions
- NPCs remember recent interactions (short-term memory appears in dialogue context)
- `cargo test` passes all NPC, schedule, and relationship tests

## Resolved Issues

- **NPC-NPC Tier 2 dialogue**: Use **fully generated** dialogue via Ollama. Template-based dialogue would feel repetitive and undermine the living-world premise. The Tier 2 prompt includes both NPCs' personalities and relationship context, producing natural conversation. If inference throughput is a bottleneck, reduce Tier 2 frequency rather than switching to templates.
- **Memory capacity**: Start with **20 entries** as designed. This is a soft cap — when full, oldest low-salience memories are evicted. Tune based on testing: if NPCs forget important events too quickly, increase to 30-40. The memory list is included in Tier 1/2 prompts, so capacity is ultimately bounded by the model's context window minus the rest of the prompt (~2K tokens for 20 entries).
- **NPC schedule conflicts**: Use a **graceful fallback**. When an NPC arrives at a scheduled interaction location but their counterpart is absent, they perform their secondary scheduled activity (or idle at the location). The interaction is silently skipped. No rescheduling or queuing — the simulation is best-effort, and missed encounters are realistic (people don't always meet when planned).
