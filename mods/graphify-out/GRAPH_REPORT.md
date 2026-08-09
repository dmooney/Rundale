# Graph Report - .  (2026-08-09)

## Corpus Check
- 37 files · ~157,940 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 317 nodes · 429 edges · 26 communities (25 shown, 1 thin omitted)
- Extraction: 87% EXTRACTED · 13% INFERRED · 0% AMBIGUOUS · INFERRED: 57 edges (avg confidence: 0.9)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Rundale Cast and Pronunciation
- Kilteevan World and Festivals
- Testbed Prompt Context
- Rundale Content Validation
- NPC Prompt Contracts
- Testbed Location Grid
- Mods Technical Debt
- Mod Registry and Selection
- Lough Ree Folklore
- Testbed Map Blueprint
- Illustrated Rural App Icon
- Embossed Monogram App Icon
- Testbed Ambient Cycle
- Manuscript Style App Icon
- Celtic Rural App Icon
- Forty Eight Pixel Icon
- Sepia Village App Icon
- Celtic Village App Icon
- Compact Celtic App Icon
- Prompt Template Overrides
- Sixteen Pixel Favicon
- Celtic Monogram Favicon
- Testbed Scope Guard
- Dark Monogram Icon
- Historical Anachronism Guard
- Empty Testbed Festivals

## God Nodes (most connected - your core abstractions)
1. `Rundale Parish Cast` - 24 edges
2. `Irish Name and Place Pronunciation Lexicon` - 24 edges
3. `Kilteevan Village` - 17 edges
4. `Rundale Mod Content Scope` - 13 edges
5. `Tier 1 Character System Prompt` - 11 edges
6. `The Crossroads` - 10 edges
7. `Ambient Road Encounters by Time of Day` - 9 edges
8. `Tier 2 Reduced-Fidelity System Prompt` - 9 edges
9. `1820 Anachronism Guard` - 8 edges
10. `Aoife Brennan — Hedge School Teacher` - 8 edges

## Surprising Connections (you probably didn't know these)
- `CLAUDE Agent Scope Alias` --references--> `Parish Mod Registry`  [EXTRACTED]
  CLAUDE.md → AGENTS.md
- `TD-005 Provider Configuration Drift` --conceptually_related_to--> `Provider Configuration Schema`  [INFERRED]
  TODO.md → AGENTS.md
- `Provider Mod Conventions Test` --references--> `Featured Provider Visibility`  [INFERRED]
  TODO.md → AGENTS.md
- `Rundale Mod Content Scope` --references--> `Gaelic Seasonal Festival Calendar`  [EXTRACTED]
  rundale/AGENTS.md → rundale/festivals.json
- `Rundale Mod Content Scope` --references--> `Rundale Parish Cast`  [EXTRACTED]
  rundale/AGENTS.md → rundale/npcs.json

## Hyperedges (group relationships)
- **Parish Mod Registry Architecture** — agents_parish_mod_registry, agents_mod_manifest, agents_active_mod_selection, agents_base_game_mods, agents_provider_registration_mods [EXTRACTED 1.00]
- **Provider Catalog Contract** — agents_provider_registration_mods, agents_provider_mod_convention, agents_provider_config_schema, agents_featured_provider_visibility [EXTRACTED 1.00]
- **Resolved Mod Quality Debt** — todo_mods_technical_debt_ledger, todo_td_001_npc_catalog_layout, todo_td_002_world_graph_validation, todo_td_003_testbed_prompt_contract, todo_td_004_stale_comment_cleanup, todo_td_005_provider_config_drift, todo_no_open_mod_technical_debt [EXTRACTED 1.00]
- **Gallagher Forge Household** — rundale_npcs_seamus_gallagher, rundale_npcs_maire_gallagher, rundale_npcs_colm_gallagher, rundale_npcs_aoife_brennan [EXTRACTED 1.00]
- **Duffy Mill Household** — rundale_npcs_cormac_duffy, rundale_npcs_nora_duffy, rundale_npcs_brendan_duffy [EXTRACTED 1.00]
- **Walsh Boatman Household** — rundale_npcs_eamon_walsh, rundale_npcs_kathleen_walsh, rundale_npcs_ciaran_walsh [EXTRACTED 1.00]
- **Tier 1 Prompt Assembly** — rundale_prompts_agents_tier_1_dual_prompt_assembly, rundale_prompts_tier1_system_tier_1_character_role, rundale_prompts_tier1_context_tier_1_scene_context, rundale_prompts_tier1_context_runtime_scene_state, rundale_prompts_tier1_context_player_action_context [EXTRACTED 1.00]
- **Tier 1 Response Contract** — rundale_prompts_tier1_system_in_character_dialogue_only, rundale_prompts_tier1_system_concise_dialogue_contract, rundale_prompts_tier1_system_tier_1_metadata_contract, rundale_prompts_tier1_system_physical_action_metadata, rundale_prompts_tier1_system_post_reply_mood_metadata, rundale_prompts_tier1_system_internal_thought_metadata, rundale_prompts_tier1_system_language_hint_metadata [EXTRACTED 1.00]
- **Tier 2 State Update Contract** — rundale_prompts_tier2_system_tier_2_json_output_contract, rundale_prompts_tier2_system_ambient_interaction_summary, rundale_prompts_tier2_system_mood_changes, rundale_prompts_tier2_system_relationship_changes, rundale_prompts_tier2_system_bounded_relationship_delta [EXTRACTED 1.00]
- **Seven-Phase Testbed Ambient Day Cycle** — testbed_encounters_dawn_test_cycle_chime, testbed_encounters_morning_calibration_sequence, testbed_encounters_midday_status_indicators, testbed_encounters_afternoon_status_code_stream, testbed_encounters_dusk_grid_lighting, testbed_encounters_night_background_processes, testbed_encounters_midnight_nominal_rest_state [EXTRACTED 1.00]
- **Testbed Validation Team** — testbed_npcs_alpha, testbed_npcs_beta, testbed_npcs_gamma [INFERRED 0.85]
- **Five-Location Testbed Grid** — testbed_world_origin, testbed_world_north_station, testbed_world_east_station, testbed_world_south_station, testbed_world_west_station [EXTRACTED 1.00]
- **Testbed NPC Prompting Stack** — testbed_prompts_tier1_context_runtime_context_template, testbed_prompts_tier1_system_character_system_prompt, testbed_prompts_tier2_system_reduced_fidelity_system_prompt [INFERRED 0.95]
- **Rundale Favicon Visual Identity** — rundale_assets_icons_app_favicon_32_rundale_favicon, rundale_assets_icons_app_favicon_32_ornamental_r_monogram, rundale_assets_icons_app_favicon_32_celtic_interlace_ornament, rundale_assets_icons_app_favicon_32_copper_gold_earth_tone_palette [EXTRACTED 1.00]
- **Rundale Visual Identity** — rundale_assets_icons_app_icon_1024_rundale_app_icon, rundale_assets_icons_app_icon_1024_ornate_initial_r, rundale_assets_icons_app_icon_1024_celtic_interlace, rundale_assets_icons_app_icon_1024_irish_rural_village, rundale_assets_icons_app_icon_1024_illuminated_manuscript_style [INFERRED 0.85]
- **Illuminated Rural Irish Identity** — rundale_assets_icons_app_icon_128_rundale_app_icon, rundale_assets_icons_app_icon_128_illuminated_letter_r, rundale_assets_icons_app_icon_128_celtic_interlace, rundale_assets_icons_app_icon_128_irish_rural_landscape, rundale_assets_icons_app_icon_128_sepia_parchment_palette, rundale_assets_icons_app_icon_128_rounded_square_emblem [EXTRACTED 1.00]
- **Historical Irish Icon Composition** — rundale_assets_icons_app_icon_180_rundale_app_icon, rundale_assets_icons_app_icon_180_ornate_insular_letter_r, rundale_assets_icons_app_icon_180_illuminated_manuscript_knotwork, rundale_assets_icons_app_icon_180_rural_irish_landscape, rundale_assets_icons_app_icon_180_sepia_parchment_palette [EXTRACTED 1.00]
- **Rundale App Icon Visual Identity** — rundale_assets_icons_app_icon_256_ornate_r_monogram, rundale_assets_icons_app_icon_256_celtic_interlace, rundale_assets_icons_app_icon_256_illuminated_manuscript_style, rundale_assets_icons_app_icon_256_rural_village_and_fields, rundale_assets_icons_app_icon_256_warm_parchment_palette [INFERRED 0.85]
- **Rundale App Icon Visual Identity** — rundale_assets_icons_app_icon_512_rundale_app_icon, rundale_assets_icons_app_icon_512_ornamental_r_monogram, rundale_assets_icons_app_icon_512_celtic_interlace_and_scrollwork, rundale_assets_icons_app_icon_512_thatched_stone_rural_village, rundale_assets_icons_app_icon_512_sepia_amber_illumination, rundale_assets_icons_app_icon_512_rounded_square_icon_composition [EXTRACTED 1.00]
- **Rundale Visual Identity** — rundale_assets_icons_app_icon_64_rundale_compact_app_icon, rundale_assets_icons_app_icon_64_ornate_initial_r, rundale_assets_icons_app_icon_64_celtic_interlace, rundale_assets_icons_app_icon_64_irish_rural_village, rundale_assets_icons_app_icon_64_illuminated_manuscript_style [INFERRED 0.85]

## Communities (26 total, 1 thin omitted)

### Community 0 - "Rundale Cast and Pronunciation"
Cohesion: 0.07
Nodes (58): Aoife Brennan — Hedge School Teacher, Brendan Duffy — Miller's Son, Brigid Ni Fhatharta — Midwife and Bean Feasa, Catholic Emancipation Movement, Ciaran Walsh — Shore-Exploring Child, Colm Gallagher — Blacksmith's Apprentice, Cormac Duffy — Miller, Eamon Walsh — Boatman (+50 more)

### Community 1 - "Kilteevan World and Festivals"
Cohesion: 0.08
Nodes (29): Aiden Carney Traveller Persona, Three-to-Five-Turn Anti-Loitering Policy, First-Person Input Constraint, Traveller Identity Continuity, Movement Command Grammar, Three Locations per Eight Turns Objective, Bealtaine — Hilltop Bonfires, Gaelic Seasonal Festival Calendar (+21 more)

### Community 2 - "Testbed Prompt Context"
Cohesion: 0.08
Nodes (29): Current Testbed State, Location Name Context, Nearby Agents Context, Player Input Context, Recent Events Context, Tier 1 Runtime Context Template, Time Context, Weather Context (+21 more)

### Community 3 - "Rundale Content Validation"
Cohesion: 0.08
Nodes (24): Rundale Content Validation Workflow, Historical Geospatial Provenance, Rundale Mod Content Scope, Schema-Compatible Content Evolution, Stable NPC Identity Contract, Temporal and Location Trigger Integrity, 1820 Anachronism Guard, Future Materials and Medicines (+16 more)

### Community 4 - "NPC Prompt Contracts"
Cohesion: 0.09
Nodes (24): Tier 1 Dual-Prompt Assembly, Player Action Context, Runtime Scene State, Tier 1 Scene Context, Concise Dialogue Contract, Conversation Farewell Discipline, Dignified Irish Character Portrayal, Hiberno-English Period Register (+16 more)

### Community 5 - "Testbed Location Grid"
Cohesion: 0.18
Nodes (20): Acceptance-Criteria Suite, Alpha, Beta, Testbed Connection Graph, Cyan Fringe at 14:00, Engine Command Output Format, Gamma, Hour-Boundary Off-by-One Suspicion (+12 more)

### Community 6 - "Mods Technical Debt"
Cohesion: 0.16
Nodes (19): Featured Provider Visibility, Provider Configuration Schema, World Alias Collision Validation, Byte-Identical NPC Catalog Round Trip, Cross-File World Content Validation, Debt Scanner False-Positive Prevention, Issue 1201 Dead-Code and Stale-Doc Cleanup, Issue 1203 Runtime Path and Configuration Scaling (+11 more)

### Community 7 - "Mod Registry and Selection"
Cohesion: 0.19
Nodes (13): Active Mod Selection, Additive Mod Schema Compatibility, Base Game Mods, Mod Loading Pipeline, Mod Manifest, Mod Validation Workflow, Parish Mod Registry, Provider Mod Naming Convention (+5 more)

### Community 8 - "Lough Ree Folklore"
Cohesion: 0.22
Nodes (9): Lough Ree Monster Sightings, Lough Ree — lock REE, Bog as Preserver of Bodies, Butter, and Memory, Lough Ree Shore, Lough Ree Wurm Folklore, O'Brien's Farm, Sídhe of the Fairy Fort, The Bog Road (+1 more)

### Community 9 - "Testbed Map Blueprint"
Cohesion: 0.22
Nodes (9): Blueprint Aesthetic, Cross-Shaped Location Grid, East Station, Engine Testbed, North Station, Origin, Parish Game Engine, South Station (+1 more)

### Community 10 - "Illustrated Rural App Icon"
Cohesion: 0.25
Nodes (8): Farmland and Stone Walls, Illuminated-Manuscript Knotwork, Lit Cottage Window, Ornate Insular Letter R, Rundale App Icon, Rural Irish Landscape, Sepia Parchment Palette, Thatched Cottages

### Community 11 - "Embossed Monogram App Icon"
Cohesion: 0.25
Nodes (8): Antique Embossed Effect, Compact Square Icon Composition, Letter R Monogram, Mottled Green Background, Ornate Serif Letterform, Rundale App Icon, Rundale Brand Identity, Warm Gold-Bronze Outline

### Community 12 - "Testbed Ambient Cycle"
Cohesion: 0.25
Nodes (8): Afternoon Status-Code Stream, Testbed Ambient Day Cycle, Dawn Test-Cycle Chime, Dusk Grid-Lighting Dimming, Midday Status Indicators, Midnight Nominal Rest State, Morning Calibration Sequence, Night Background Processes

### Community 13 - "Manuscript Style App Icon"
Cohesion: 0.38
Nodes (7): Celtic Interlace Ornament, Illuminated-Manuscript Style, Ornate R Monogram, Rounded-Square App Badge, Rundale App Icon, Rural Village and Fields, Warm Parchment Palette

### Community 14 - "Celtic Rural App Icon"
Cohesion: 0.40
Nodes (6): Celtic Interlace Ornament, Illuminated Letter R, Irish Rural Landscape, Rounded-Square Emblem, Rundale App Icon, Sepia Parchment Palette

### Community 15 - "Forty Eight Pixel Icon"
Cohesion: 0.33
Nodes (6): Bronze and Dark Brown Palette, Engraved Dimensional Linework, Ornate R Monogram, Rounded Dark Icon Field, Rundale 48 px App Icon, Decorative Serif Letterform

### Community 16 - "Sepia Village App Icon"
Cohesion: 0.40
Nodes (6): Celtic Interlace and Scrollwork, Ornamental R Monogram, Rounded-Square Icon Composition, Rundale App Icon, Sepia and Amber Illumination, Thatched-Stone Rural Village

### Community 17 - "Celtic Village App Icon"
Cohesion: 0.50
Nodes (5): Celtic Interlace, Illuminated Manuscript Style, Irish Rural Village, Ornate Initial R, Rundale App Icon

### Community 18 - "Compact Celtic App Icon"
Cohesion: 0.50
Nodes (5): Celtic Interlace, Illuminated Manuscript Style, Irish Rural Village, Ornate Initial R, Rundale Compact App Icon

### Community 19 - "Prompt Template Overrides"
Cohesion: 0.40
Nodes (5): Case-Sensitive Placeholder Substitution, Mod Prompt Override Pattern, Rundale Prompt Templates, Runtime Prompt Loading, CLAUDE Prompt Scope Alias

### Community 20 - "Sixteen Pixel Favicon"
Cohesion: 0.50
Nodes (4): Bronze and Dark Brown Palette, Ornate R Monogram, Rundale 16 px Favicon, Small-Scale App Brand Mark

### Community 21 - "Celtic Monogram Favicon"
Cohesion: 0.50
Nodes (4): Celtic Interlace Ornament, Copper-Gold Earth-Tone Palette, Ornamental R Monogram, Rundale Favicon

### Community 22 - "Testbed Scope Guard"
Cohesion: 0.50
Nodes (4): Testbed Conceptual Scope Guard, Empty Testbed Term Catalogue, Natural Redirection, Out-of-Scope Term Alert

### Community 23 - "Dark Monogram Icon"
Cohesion: 0.67
Nodes (3): Antique Gold R Monogram, Dark Brown Rounded Icon Tile, Rundale App Icon

### Community 24 - "Historical Anachronism Guard"
Cohesion: 0.67
Nodes (3): Anachronism Correction Loop, 1820 Rural Roscommon World Facts, Historical Anachronism Guard

## Knowledge Gaps
- **95 isolated node(s):** `Provider Mod Naming Convention`, `CLAUDE Agent Scope Alias`, `relative_to Anchor Validation`, `World Alias Collision Validation`, `Cross-File World Content Validation` (+90 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **1 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Rundale Mod Content Scope` connect `Rundale Content Validation` to `Rundale Cast and Pronunciation`, `Kilteevan World and Festivals`?**
  _High betweenness centrality (0.054) - this node is a cross-community bridge._
- **Why does `Rundale Parish Cast` connect `Rundale Cast and Pronunciation` to `Rundale Content Validation`?**
  _High betweenness centrality (0.039) - this node is a cross-community bridge._
- **Why does `Kilteevan Village` connect `Kilteevan World and Festivals` to `Rundale Cast and Pronunciation`, `Rundale Content Validation`?**
  _High betweenness centrality (0.036) - this node is a cross-community bridge._
- **Are the 2 inferred relationships involving `Tier 1 Character System Prompt` (e.g. with `Tier 1 Runtime Context Template` and `Tier 2 Reduced-Fidelity System Prompt`) actually correct?**
  _`Tier 1 Character System Prompt` has 2 INFERRED edges - model-reasoned connections that need verification._
- **What connects `Provider Mod Naming Convention`, `CLAUDE Agent Scope Alias`, `relative_to Anchor Validation` to the rest of the system?**
  _95 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Rundale Cast and Pronunciation` be split into smaller, more focused modules?**
  _Cohesion score 0.07138535995160314 - nodes in this community are weakly interconnected._
- **Should `Kilteevan World and Festivals` be split into smaller, more focused modules?**
  _Cohesion score 0.07881773399014778 - nodes in this community are weakly interconnected._