# Plan: Phase 5 — Full LOD & Scale

> Parent: [Roadmap](../requirements/roadmap.md) | [Docs Index](../index.md)

## Goal

Complete the cognitive LOD system with Tier 3 batch inference and Tier 4 CPU rules engine, expand the world beyond the starting parish, implement weather simulation, seasonal effects, gossip propagation, and NPC long-term memory.

## Prerequisites

- Phase 4 complete: persistence working, autosave, branch system
- Tier 1 and Tier 2 NPC cognition working (Phase 3)
- Inference pipeline handles multiple concurrent requests (Phase 1)

## Tasks

1. **Implement Tier 3 batch inference in `src/npc/mod.rs`**
   - `async fn tick_tier3(npcs: &mut [&mut Npc], world: &WorldState, queue: &InferenceQueue) -> Result<Vec<Tier3Update>>`
   - Build single bulk prompt: "Here are {N} people and their current states: [{npc summaries}]. Simulate {hours} hours. Return JSON array of updates."
   - `Tier3Update` struct: `npc_id: NpcId`, `new_location: Option<LocationId>`, `mood: String`, `activity_summary: String`, `relationship_changes: Vec<(NpcId, f32)>`
   - Parse JSON array response, distribute updates to individual NPCs
   - Tick rate: every 1 in-game day (~every 20 real seconds)
   - Use smaller model (8B/3B) via `InferenceRequest.model` field

2. **Implement Tier 4 rules engine in `src/npc/tier4.rs`** (new file)
   - `fn tick_tier4(npcs: &mut [&mut Npc], world: &WorldState, rng: &mut impl Rng) -> Vec<Tier4Event>`
   - No LLM: deterministic/random state transitions
   - Rules: seasonal work patterns (planting/harvest), weather-driven schedule changes, random life events (illness 2%/season, new relationship 5%/season, death 0.5%/year for elderly)
   - `Tier4Event` enum: `Birth { parent_ids }`, `Death { npc_id }`, `TradeCompleted { buyer, seller }`, `SeasonalShift { npc_id, new_schedule }`, `Illness { npc_id }`, `Recovery { npc_id }`
   - Tick rate: once per in-game season (~30-45 real minutes)
   - Run on `tokio::task::spawn_blocking` to use CPU cores without blocking async runtime

3. **Implement tier inflation (distant -> close)**
   - `fn inflate_npc_context(npc: &Npc, recent_tier3_updates: &[Tier3Update], recent_tier4_events: &[Tier4Event]) -> String`
   - Produces a narrative summary: "You are {name}. Recently, you've been {activity_summary}. Your mood has been {mood}. {relationship_changes_narrative}."
   - Called when `NpcManager::assign_tiers` promotes an NPC from Tier3/4 to Tier1/2
   - Inject summary into NPC's short-term memory as a synthetic `MemoryEntry`

4. **Implement tier deflation (close -> distant)**
   - `fn deflate_npc_state(npc: &Npc) -> NpcSummary`
   - `NpcSummary` struct: `npc_id: NpcId`, `location: LocationId`, `mood: String`, `recent_activity: String`, `key_relationship_changes: Vec<(NpcId, f32)>`
   - Called when NPC demoted from Tier1/2 to Tier3/4
   - Compact short-term memory into a single summary string stored on the NPC

5. **Implement event bus in `src/world/events.rs`** (new file)
   - `EventBus` struct wrapping `tokio::sync::broadcast::Sender<WorldEvent>`
   - `fn publish(&self, event: WorldEvent)` — broadcast to all subscribers
   - Each tier tick subscribes and processes relevant events
   - Cross-tier propagation: Tier 1 dialogue revealing a secret -> event -> Tier 3 NPC learns about it next tick
   - Also feeds into persistence journal (Phase 4)

6. **Expand world graph: `data/roscommon.json`**
   - Add Roscommon town: ~10 nodes (Main Street, Market Square, County Hospital, Train Station, Roscommon Castle, Abbey Hotel, GAA Grounds, Industrial Estate, Library, Shopping Centre)
   - Add Athlone: ~5 nodes (Town Centre, Athlone Castle, Shannon Bridge, Luan Gallery, Train Station)
   - Add Dublin: ~5 nodes (O'Connell Street, Trinity College, Heuston Station, Phoenix Park, Temple Bar)
   - Connect parish to Roscommon (30 min traversal), Roscommon to Athlone (40 min), Athlone to Dublin (2 hours)
   - Load multiple data files or merge into one `data/world.json`

7. **Implement `WeatherState` machine in `src/world/weather.rs`** (new file)
   - `WeatherState` enum: `Clear`, `PartlyCloudy`, `Overcast`, `LightRain`, `HeavyRain`, `Fog`, `Storm`
   - `WeatherEngine` struct: `current: WeatherState`, `since: DateTime<Utc>`
   - `fn tick(&mut self, clock: &GameClock, season: Season, rng: &mut impl Rng) -> Option<WeatherState>` — returns Some if weather changed
   - Transition probabilities vary by season: more rain in autumn/winter, more clear in summer
   - Minimum duration per state: 2 game-hours to avoid rapid flipping
   - Publish `WeatherChanged` event on transition

8. **Weather affects NPC behavior**
   - Modify `desired_location()` in NPC schedule: if raining and NPC is scheduled outdoors, override to nearest indoor location
   - Modify Tier 2 prompts: include weather, affects conversation topics ("Desperate weather today")
   - Modify TUI palette: apply weather modifiers from Phase 1 color system (desaturate for overcast, cool for rain)

9. **Implement seasonal cycle effects**
   - `fn seasonal_schedule_overrides(npc: &Npc, season: Season) -> Vec<ScheduleEntry>` — farmers work longer in summer, school closed in summer, pub busier in winter evenings
   - Festival event hooks: when `GameClock` crosses Imbolc/Bealtaine/Lughnasa/Samhain, publish `FestivalEvent` via EventBus
   - NPCs aware of festivals: inject into context ("It's Bealtaine. The community is...")

10. **Implement gossip propagation in `src/npc/gossip.rs`** (new file)
    - `GossipItem` struct: `content: String`, `source: NpcId`, `known_by: HashSet<NpcId>`, `distortion_level: u8`, `timestamp: DateTime<Utc>`
    - `GossipNetwork` struct: `items: Vec<GossipItem>`
    - `fn propagate(network: &mut GossipNetwork, interaction: &Tier2Event)` — when two NPCs interact, transfer gossip with 60% probability, 20% chance of distortion (modify content slightly)
    - Player learns gossip through NPC dialogue (injected into Tier 1 context)

11. **Implement NPC long-term memory in `src/npc/memory.rs`** (new file)
    - `LongTermMemory` struct: `entries: Vec<LongTermEntry>`
    - `LongTermEntry`: `timestamp: DateTime<Utc>`, `content: String`, `importance: f32`, `keywords: Vec<String>`
    - `fn store(&mut self, entry: LongTermEntry)` — add with importance scoring
    - `fn recall(&self, query: &str, limit: usize) -> Vec<&LongTermEntry>` — keyword-based retrieval: score entries by keyword overlap with query, return top N
    - Promote from short-term: when short-term memory evicts an entry, score importance, store if above threshold (importance > 0.5)
    - Include top 3 recalled memories in Tier 1 context construction

12. **Write tests**
    - `test_tier3_batch_response_parsing`: mock JSON array response, assert correct distribution to NPCs
    - `test_tier4_deterministic_rules`: seed RNG, run tier4 tick, assert expected events
    - `test_tier_inflation_produces_context`: inflate distant NPC, verify summary string is non-empty and includes recent activity
    - `test_weather_state_transitions`: verify all transitions are valid, minimum duration respected
    - `test_gossip_propagation`: create gossip item, propagate through 3 interactions, verify spread and distortion
    - `test_long_term_memory_recall`: store 10 entries with keywords, recall by query, assert relevance ordering

## Design References

- [Cognitive LOD](../design/cognitive-lod.md)
- [Weather System](../design/weather-system.md)
- [World & Geography](../design/world-geography.md)

## Key Decisions

- [ADR-002: Cognitive LOD Tiers](../adr/002-cognitive-lod-tiers.md)
- [ADR-005: Ollama Local Inference](../adr/005-ollama-local-inference.md)

## Acceptance Criteria

- All four cognitive tiers operational: Tier 1 (full dialogue), Tier 2 (NPC-NPC summaries), Tier 3 (batch daily updates), Tier 4 (seasonal rules)
- NPC state is coherent across tier transitions: player approaches a distant NPC and they reference their recent activities
- Weather changes over time, visibly affects TUI palette and NPC behavior
- Seasonal festivals fire events at correct calendar dates
- Gossip spreads between NPCs; player can learn about distant events through conversation
- World graph extends beyond the parish with sparser detail at greater distances
- `cargo test` passes all new tests

## Resolved Issues

- **Tier 3 batch prompt size**: Target **8-10 NPCs per batch call** using the 8B model with a 4K context window. Each NPC summary is ~100-150 tokens (name, location, current activity, mood), plus ~500 tokens for the system prompt and output format. This fits comfortably in 4K. If the model supports 8K+, increase to 15-20 NPCs per batch. The batch size should be a configurable constant, tuned during testing.
- **Gossip distortion**: Use **simple string mutation** for Phase 5. Rules: drop adjectives, swap names with low probability, exaggerate quantities, shift emotional tone. LLM-based rephrasing is more realistic but costs an inference call per gossip transmission, which is prohibitive at scale. Revisit for LLM rephrasing in a future polish pass if the simple mutations feel too mechanical.
- **Long-term memory**: Use **keyword-based retrieval** for Phase 5. Memories are tagged with keywords (NPC names, locations, event types) at creation time, and retrieval filters by keyword overlap with the current context. Embedding-based retrieval is more accurate but requires running an embedding model alongside the generation models, doubling Ollama throughput requirements. Defer embeddings to a future phase if keyword retrieval proves insufficient.
- **Performance budget**: Enforce a strict priority queue: **Tier 1 > Tier 2 > Tier 3 > Tier 4**. Tier 1/2 inference requests always preempt Tier 3/4 in the queue. Tier 3 batch ticks run every 5 game-minutes; Tier 4 summary ticks run every 30 game-minutes. If a lower-tier tick cannot complete before the next one is due, it is skipped (not queued). This ensures player-facing responsiveness is never degraded by background simulation.
