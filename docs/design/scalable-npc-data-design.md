Context
Parish currently has 8 hand-authored NPCs in data/npcs.json. Each has ~200 words of personality, 4+ relationships, schedules, and knowledge. At this scale, editing a JSON file is manageable. At 1,000+ NPCs it becomes painful; at 1M it is impossible. We need a database-backed architecture, a build-time procedural generation pipeline, management tooling, and a principled approach to NPC data richness that complements the existing cognitive LOD system.
Key design decision: pre-generate all Sketched NPCs upfront. At ~200 bytes each, 1M NPCs = ~200MB, 6.8M (all-Ireland) = ~1.4GB. These are trivial sizes for SQLite. Pre-generation means no runtime generation stalls, full referential integrity from day one, and cross-references that always resolve (e.g. an NPC in Kiltoom mentions a cousin in Galway — that cousin exists).
This is a documentation-only deliverable: docs/design/npc-scaling.md and docs/adr/018-npc-data-depth-tiers.md.

Plan: Write docs/design/npc-scaling.md
1. Demographic Reality of 1820 Ireland
Ground all generation in historical truth:

Parish scale: Kiltoom parish ~1,500-3,000 souls, 20-40 townlands, 5-20 households per townland
County Roscommon: ~250,000 across ~60 parishes. 1M NPCs = ~5-6 counties
Age pyramid (pre-transition): 0-14: 40%, 15-44: 40%, 45-64: 15%, 65+: 5%
Occupations: Tenant farmers 35%, laborers 30%, servants 10%, craftsmen 8%, shopkeepers 3%, clergy 0.5%, gentry 2%, others 10%
Family structure: Avg household 5-7, stem family system, marriage age men ~28 / women ~25, 6-8 children per marriage
Language: Connacht majority Irish-speaking with increasing bilingualism
Religion: ~85% Catholic, ~14% Church of Ireland, ~1% Presbyterian
Social networks: Dunbar's ~150-200 recognized by name, 15-50 active relationships, 5-15 intimate

2. The Two-Axis Tier Model
Two independent tier systems (the key architectural insight):
AxisControlsAssigned WhenValuesData DepthHow much static data existsAt build time (Sketched) or on promotionSketched / Elaborated / AuthoredCognitive LODHow much compute is spent nowEvery time player movesTier 1-4 (existing system)
These are orthogonal. An Authored NPC far away runs at Tier 4. A Sketched NPC the player stumbles upon gets promoted to Elaborated and jumps to Tier 1.
Data Depth tiers:

Sketched (millions, pre-generated): name, surname, sex, birth_year, parish, townland, occupation, religion, social_class, household_id. ~200 bytes. Procedurally generated at build time, no LLM. Every NPC in Ireland exists from the start.
Elaborated (2K-10K, runtime promotion): All Sketched fields + personality (2-3 sentences), 2-5 knowledge items, 3-8 explicit relationships, schedule template, mood. ~2KB. LLM-generated on first encounter or via batch elaboration.
Authored (50-200, hand-crafted): All Elaborated fields + multi-paragraph personality, 10+ knowledge items, 10+ relationships with history, custom schedule overrides, backstory, secrets, narrative flags. ~5-10KB. Human-authored or LLM + human review. The existing 8 NPCs become Authored.

Interaction matrix (Data Depth x Cognitive LOD):
Tier 1 (same loc)Tier 2 (nearby)Tier 3 (distant)Tier 4 (far)AuthoredFull LLM + rich contextLLM + relationshipsBatch summaryRules engineElaboratedFull LLM + generated contextLLM + basic contextBatch summaryRules engineSketchedPROMOTE first, then Tier 1PROMOTE first, then Tier 2SkipSkip
3. Promotion Mechanics
Sketched -> Elaborated triggers:

Player enters a location where a Sketched NPC is present
Player asks about a Sketched NPC by name
A game event makes a Sketched NPC relevant (inherits land, commits crime, arrives in player's parish)
An Elaborated NPC mentions a Sketched NPC in conversation

Process: Build an LLM prompt with the NPC's pre-existing demographic skeleton (name, age, occupation, household members, townland) and generate personality, knowledge, mood. ~1-5 second LLM call. The skeleton is already consistent — promotion only adds richness.
Latency mitigation: Trigger promotion eagerly when the player starts traveling toward a location, not on arrival. The travel narration covers the generation time.
Fallback (LLM unavailable): Template-based personality: "A quiet {occupation} who goes about their business."
Elaborated -> Authored: Never automatic. Flagged when NPC accumulates >N player interactions or is involved in >M significant events. Requires human review.
Fog of war principle: The absence of detail must be invisible. The player never sees a stub NPC. Promotion happens before the player encounters the NPC.
4. Database Schema
Replace data/npcs.json with SQLite tables. The JSON format remains as import/export. The game ships with (or downloads) a pre-built parish-world.db.
Key tables:

Geographic hierarchy: provinces, counties, baronies, parishes, townlands, locations, location_connections, parish_connections
NPC core: npcs table with data_tier column (0/1/2), demographic fields, nullable rich fields (personality, mood), runtime state (current_location, state)
Households: households table (townland, head, dwelling_type, land_acres). The fundamental unit of social organization.
Occupations: Template table with frequency weights for generation
Relationships: Sparse adjacency list (from_npc_id, to_npc_id, kind, subkind, strength). Critical optimization: household and townland relationships are implicit (derived from shared household_id/townland_id), only non-default relationships stored explicitly. Reduces 30M rows to 2-5M.
Schedule templates: ~50 shared templates (e.g. "farmer_spring", "publican") with location-type entries ("home", "workplace", "pub") resolved at runtime. Authored NPCs can override with custom entries.
Knowledge, memories, relationship events: Append-only tables indexed by NPC id (empty at build time for Sketched NPCs, populated on promotion or during gameplay).
Player interaction tracking: Records every encounter for promotion scoring.

Indexes: On parish_id, townland_id, current_location, household_id, data_tier, surname.
5. Runtime Architecture
Active Set model — only ~2,000 NPCs in memory at once:

Player's parish: all Elaborated+ NPCs loaded
Adjacent parishes: only NPCs at boundary locations
Everything else: stays in SQLite (already pre-generated, ready to query)

Loading/unloading: On parish transition, load new parish NPCs from DB, flush dirty state for unloaded NPCs via spawn_blocking.
Lazy schedule resolution: NPCs outside the active set are NOT ticked. When loaded, their position is computed from their schedule template + current game time. They "snap" to correct positions (invisible to player since they've never been observed).
Spatial indexing: Within a parish, BFS over 30-50 locations (existing system, fine). Between parishes, hierarchical query: same location -> adjacent locations -> same parish -> adjacent parishes. All queries hit pre-populated SQLite indexes.
Schedule ticking: Only active set NPCs (~2,000) are ticked. Cost: ~50us per tick, same as current.
6. Build-Time Generation Pipeline
All Sketched NPCs are pre-generated at build time, not at runtime. The generation pipeline is a build tool (like geo_tool), not part of the game binary. It produces a parish-world.db that ships with the game.
Pipeline mirrors geo_tool geographic blocks:
geo_tool extracts parishes/townlands/locations from OSM (geographic blocks)
    |
demographic seeder runs per-parish: households -> NPCs -> implicit relationships
    |
cross-parish pass: long-distance family ties, trade connections, clergy networks
    |
single parish-world.db file (~200MB for 1M, ~1.4GB for all-Ireland 6.8M)
    |
ships with game (or downloaded as a "world pack")
Unit of generation: the household (not individual NPC).
generate_household(townland, rng):
  1. Choose type by weighted random (cottier 35%, small farm 30%, strong farm 10%, craftsman 8%, ...)
  2. Generate head (male, 28-60, occupation from type)
  3. Generate spouse (~80% chance if age 25+, same surname)
  4. Generate children (Poisson mean=5.5, adjusted for mother's age)
  5. Optional: elderly parent (30%), servants (gentry/strong farmers), lodgers
  6. Assign to townland, create household record
Parish seeding: Distribute target population across townlands proportional to area, generate households until population reached, then generate cross-household social network (meitheal groups, godparent bonds, friendships biased by age/townland/occupation).
Name generation: Period-appropriate Irish names frequency-weighted by region. Male: Padraig, Sean, Michael, Thomas, James... Female: Mary, Bridget, Margaret, Catherine... Surnames frequency-weighted for Roscommon: Kelly, Murphy, Brennan, O'Brien, Flanagan... Eldest son named after paternal grandfather (convention).
Why build-time, not runtime:

No generation stalls when the player travels somewhere new
Full referential integrity — every relationship target, every household member, every family link exists
Cross-parish references work (NPC mentions cousin in Galway — cousin exists)
Deterministic and reproducible (seeded RNG)
The game binary stays simple — it only does promotion (Sketched -> Elaborated), never skeleton generation

7. Tooling: parish-npc CLI
New binary in src/bin/parish_npc/:
parish-npc generate-world --counties roscommon,galway  # build the world DB
parish-npc generate-parish Kiltoom --pop 2000          # seed one parish
parish-npc list --parish Kiltoom --occupation Farmer
parish-npc show 12345
parish-npc search "Darcy"
parish-npc edit 12345 --mood cheerful
parish-npc promote 12345                               # Sketched -> Elaborated
parish-npc elaborate --parish Kiltoom --batch 50        # batch LLM elaboration
parish-npc validate --parish Kiltoom
parish-npc validate --all                               # full world consistency check
parish-npc stats                                        # population counts, tier distributions
parish-npc export --parish Kiltoom > kiltoom.json
parish-npc import < kiltoom.json
parish-npc family-tree 12345
parish-npc relationships 12345
Consistency validator checks: referential integrity, household structure, age consistency, occupation distribution, naming conventions, schedule template resolution, all Elaborated+ have personality.
Debug panel extensions: NPC browser in TUI debug mode, data tier indicators, promotion queue status.
8. World Expansion
Hierarchical world graph: townland -> parish -> barony -> county -> province.

Within parish: BFS over 30-50 locations (current system)
Between adjacent parishes: boundary edges with travel times
Long-distance: abstracted multi-day journeys with encounter opportunities

Locations are also pre-generated via geo_tool OSM pipeline, stored in the same parish-world.db. The geographic and demographic data are generated together, block by block.
Scale: Starting parish fully detailed (30-50 locations, ~2000 NPCs pre-elaborated). All other parishes have locations + Sketched NPCs ready. The player can go anywhere — the world is already there.
9. Data Lifecycle
Creation (build time): geo_tool geographic blocks -> demographic seeder -> Sketched NPCs -> parish-world.db. Optionally: batch LLM elaboration for the starting parish to pre-promote key NPCs.
Promotion (runtime): Sketched -> Elaborated via LLM on first encounter. Elaborated -> Authored via human review.
Mutation (runtime): Mood drift (Tier 2 events), relationship evolution, knowledge accumulation, aging (derived from birth_year + game clock), life events (marriage, children = new NPC creation, death, emigration, eviction). All mutations written to the player's save DB, not the world DB.
Archival: Dead/emigrated NPCs set state = 'dead'/'emigrated', never deleted. Remain queryable for genealogy, graveyard descriptions, folklore, and historical context.
Two-database model: parish-world.db is read-only (the pre-generated world). parish-save.db is the player's save file (promotions, mutations, memories, events). On load, the game overlays save data onto world data.
Schema migration: schema_version table + numbered migration files in both databases, applied on startup.
10. Emergent Social Dynamics

Marriages: Two NPCs with sufficient relationship strength, appropriate age, regular co-location -> courtship event -> household merge
Feuds: Relationship strength below -0.5 between households -> cascading effects on all members
Economic shifts: Rent increases (Tier 4 events) -> emigration pressure -> NPCs leave
News propagation: Major events spread through gossip network at ~1-2 parishes per game-day
Historical events (1820-1845): Catholic Association 1823, Catholic Emancipation 1829, Tithe War 1831, increasing emigration 1830s — modeled as world events triggering Tier 4 batch updates

11. Performance Estimates
Operation8 NPCs (now)2,000 (parish)1M (pre-generated)Active set load<1ms~50ms~100ms (2000 NPCs from DB)NPCs at location<1us~20us~1ms (SQLite index)Tier assignment BFS<1us~100us~100us (per parish)Schedule tick (active)<1us~50us~50us (active set only)Relationship query<1us~1ms~2msPromotion (LLM)N/A1-5s1-5sWorld DB file sizeN/A~5MB~200MB (1M) / ~1.4GB (6.8M)Build-time generationN/A~1s~5-10min (1M)

Files to Create/Modify
FileActionDescriptiondocs/design/npc-scaling.mdCreateFull design doc (all sections above, expanded into prose with SQL schema, pseudocode, diagrams)docs/adr/018-npc-data-depth-tiers.mdCreateADR for the two-axis tier model + pre-generation decisiondocs/design/overview.mdEditAdd NPC scaling link to Subsystem Deep-Dives list (~line 121)docs/index.mdEditAdd NPC Scaling row to Design Documents table (~line 30) and ADR table (~line 52)docs/adr/README.mdEditAdd ADR-018 row to ADR Index table (~line 27)docs/requirements/roadmap.mdEditAdd NPC Scaling items to Phase 5 checklist (~line 79)
Verification

All new docs render correctly as Markdown
Links between docs are valid (cross-check all [text](path) references)
No code changes — docs only
cargo test still passes (no code touched)
ADR-018 follows existing ADR format (Status/Context/Decision/Consequences/Alternatives/Related)
Design doc covers all 11 sections from the plan above