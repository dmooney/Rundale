# Graph Report - .  (2026-08-23)

## Corpus Check
- cluster-only mode — file stats not available

## Summary
- 592 nodes · 545 edges · 80 communities (59 shown, 21 thin omitted)
- Extraction: 88% EXTRACTED · 12% INFERRED · 0% AMBIGUOUS · INFERRED: 65 edges (avg confidence: 0.9)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `d28843ca`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- Overhead Art Experiment Index
- Candidate mechanics include player vitals and fatigue, sleep, inventory and items, economy, skills, reputation, status effects, lighting, housing, seasonal agriculture, disease, transport, tasks, and difficulty or death.
- Parish Feature Inventory
- April 2026 Regression Coverage Audit
- Priority-Lane Inference Queue
- Graphics V2 Research Index
- Provenanced Fact and Belief Graph
- Game Time System (Implemented)
- Shipped Wave 1 comprises slash-command autocomplete, fifty-entry local input history, asterisk-delimited emote actions, Shift+Enter multiline input, and location travel chips.
- Rundale Historical Research Collection
- Human-facing hub for agent engineering references
- Bug Report Tool Plan
- Potato and Buttermilk Economy
- Colonial Legal Order
- Four-Judge Borda Quality Harness
- Gemma 4 Rundale Training Plan
- Central isometric Kilteevan village scene with chapel, cottages, roads, bridge, stream, and people
- Scaling Seam Checklist
- Game Quality Control System
- Illustrated Notebook Real Plan
- Accepted independent provider routing for dialogue, simulation, and intent
- The proposed Interactive Parish Diorama supersedes monolithic scene plates with a runtime-composed, scene-based graphical presentation over the existing living-world simulation.
- Irish-English Diglossia
- Run Locally Recommended Option
- Three-Layer Persistence Model
- NPC Agenda Scheduler
- Interactive Parish Diorama Runtime Compositor Plan
- Authoritative Rundale Feature-Status Matrix
- Padraig Darcy Identity Editor
- ADR-001 Named Location Graph Decision
- Rundale Documentation Hub
- NPC Portrait Pipeline
- Irregular Mortarless Fieldstone
- Harness Mock and Shadow Plan
- Historical Input Enrichment Design Portfolio
- Parish Designer Location Editor
- Rundale's full-screen map overlay presents the parish as an interactive network laid over a sepia historical Ordnance Survey-style map.
- Illustrated Parish Notebook Static UI
- docs/index.md as exhaustive authoritative documentation hub with unique ADR numbers
- The five control layers are pre-commit prevention, adversarial PR gating, weekly SQALE measurement, autonomous issue-to-fix-to-land repair, and custom architectural rules or fitness tests.
- The fully on-device iOS port is proposed but implementation-ready: design decisions are closed, no code work has started, and the target is an offline iPhone build containing UI, simulation, persistence, and local inference.
- The period-map-tiles feature combines OpenStreetMap context with historical six-inch 1829–1842 mapping, selected through a tile-source registry and a three-tier cache of user, bundled, then upstream tiles.
- Local Dialogue Rejection Sampler
- Personalization and Learning Brainstorm
- Multimodal Brainstorm
- Authored Directed Events
- Parish Designer
- The proposed Cloud Run runtime pins minimum and maximum instances to one, keeps CPU allocated for simulation ticks, reconnects WebSockets before the 60-minute limit, and uses Gemini for inference.
- Illustrated Parish Notebook UI (Retired Historical Experiment)
- Natural-Language Player Input (Implemented)
- Topology Review Gate
- Doors on Openings Audit
- Door-Fixed House References
- Google OAuth Configuration
- Quality-Tiered Source Corpus
- Phase 7 Thin-Client Cloud Server Plan
- Release Tag as Single Source of Truth
- Live Web-Mode Browser Evidence
- Paired Inference and Transcript Artifacts
- Seasonal Visibility Pacing Goal
- Natural-language prompt guidance for notable intelligence strengths and weaknesses
- Event-Driven Improvement Queue
- Rundale Project Skill Registry
- Ambient Sound System
- In-App Bug Report Tool
- Manual IPC Type Synchronization
- Harness Divergence Ledger
- Illustrated Notebook Pixi Experiment
- First-Contribution Architecture Guide
- Masked Semantic Seam Repair
- Cross-Runtime No-Op Greeting Semantics
- Backend-Preserving Notebook UI Rebuild
- Phase 5F Four-Region World Expansion
- Phase 6 Mythology Hooks-Only Plan
- Branch Switch State Refresh Contract
- Speed and Loading Nonblocking Follow-Ups
- Notebook Semantic Collapse
- Accepted separation of engine tuning in parish.toml from setting content in mods
- Maybe Bad Ideas Scope-Risk Register
- Repository Workspace Orientation

## God Nodes (most connected - your core abstractions)
1. `April 2026 Regression Coverage Audit` - 9 edges
2. `Graphics V2 Research Index` - 9 edges
3. `Subagent-Gated BU-Style Exterior Pipeline` - 8 edges
4. `Overhead Art Experiment Index` - 8 edges
5. `Rundale Historical Research Collection` - 8 edges
6. `Priority-Lane Inference Queue` - 7 edges
7. `Reproducible Clean-Context Map Reader Stage` - 7 edges
8. `Parish Feature Inventory` - 6 edges
9. `Scaling Seam Checklist` - 6 edges
10. `Gemma 4 Rundale Training Plan` - 6 edges

## Surprising Connections (you probably didn't know these)
- `Per-Category Inference Routing` --conceptually_related_to--> `ADR-013 Optional Cloud Dialogue Routing`  [INFERRED]
  features.md → adr/013-cloud-llm-dialogue.md
- `Multi-Provider Inference Graduated From Idea` --conceptually_related_to--> `Per-Category Inference Routing`  [INFERRED]
  maybe-bad-ideas.md → features.md
- `Hardware-Aware Local Model Selection` --references--> `ADR-005 Ollama Local Inference Baseline`  [EXTRACTED]
  setup.md → adr/005-ollama-local-inference.md
- `Shared gameplay logic and feature parity across Tauri, CLI, and Axum modes` --conceptually_related_to--> `Accepted Axum browser-test mode sharing the Svelte frontend and Parish logic`  [INFERRED]
  agent/architecture.md → adr/023-web-testing-server.md
- `Shared Dialogue-Turn Seam Proposal` --conceptually_related_to--> `Quality Sensor and Harness Map`  [INFERRED]
  design/1172-1173-dialogue-seam.md → agent/harness.md

## Hyperedges (group relationships)
- **Trusted Input Interpretation Boundary** — adr_006_natural_language_input_structured_intent_decision, adr_008_structured_json_llm_output_typed_output_schema_decision, adr_010_prompt_injection_defenses_five_layer_defense [EXTRACTED 1.00]
- **Authentic World Authoring Pipeline** — adr_001_graph_based_world_named_location_graph_decision, adr_009_real_geography_fictional_people_authentic_geography_fictional_identity_decision, adr_011_geo_tool_osm_pipeline_separate_geo_tool_pipeline [EXTRACTED 1.00]
- **Branching Journal Persistence System** — adr_003_sqlite_wal_persistence_three_layer_persistence_strategy, adr_004_git_like_branching_saves_git_like_timeline_decision, features_branching_journal_persistence [EXTRACTED 1.00]
- **Web, desktop, and browser-test UI architecture decisions** — adr_014_web_mobile_architecture_thin_client_thick_server, adr_016_tauri_svelte_gui_tauri_svelte_replacement, adr_023_web_testing_server_axum_browser_test_mode, agent_architecture_mode_parity_contract [INFERRED 0.95]
- **NPC inference structure, intelligence, tools, and memory decision family** — adr_017_per_category_inference_providers_category_specific_provider_routing, adr_018_npc_intelligence_dimensions_six_dimension_intelligence_profile, adr_019_json_structured_output_for_npc_dialogue_native_json_response_format, adr_020_npc_tool_use_deferred_provider_tool_calling, adr_021_npc_memory_retrieval_deferred_embedding_retrieval [INFERRED 0.95]
- **Agent engineering orientation and verification reference set** — agent_readme_agent_docs_hub, agent_codebase_map_repository_navigation_index, agent_architecture_workspace_composition_and_ownership, agent_build_test_build_test_and_quality_gate_catalog, agent_code_style_rust_svelte_dependency_conventions, agent_agent_check_proof_bundle_gate, agent_driving_the_game_via_mcp_live_desktop_mcp_control [EXTRACTED 1.00]
- **Regression Coverage Audit Suite** — audits_regression_coverage_2026_04_regression_coverage_audit, audits_regression_01_game_world_game_world_coverage_report, audits_regression_02_npc_system_npc_system_coverage_report, audits_regression_03_player_input_player_input_coverage_report, audits_regression_04_persistence_persistence_coverage_report, audits_regression_05_llm_inference_llm_inference_coverage_report, audits_regression_06_gui_gui_coverage_report, audits_regression_07_mod_system_mod_system_coverage_report, audits_regression_08_runtime_modes_runtime_mode_coverage_report, audits_regression_09_dev_tools_developer_tools_coverage_report [EXTRACTED 1.00]
- **Runtime Quality Gate System** — agent_git_workflow_merge_and_proof_workflow, agent_harness_quality_sensor_map, agent_skills_project_skill_registry, agent_witness_completion_witness_gate [INFERRED 0.95]
- **Scaling-Sensitive Contract Set** — agent_scaling_rules_scaling_seam_checklist, agent_idempotency_idempotency_key_contract, agent_tracing_opentelemetry_observability_contract, agent_gotchas_runtime_and_architecture_gotchas [INFERRED 0.85]
- **Shared Runtime Observability** — design_debug_system_debug_system, design_debug_ui_debug_snapshot_ui, design_inference_pipeline_local_performance_evidence, design_game_quality_harness_architecture_game_quality_control_system [INFERRED 0.85]
- **Bounded NPC Cognition Architecture** — design_cognitive_lod_cognitive_level_of_detail, design_independent_npc_agents_independent_npc_agents, design_inference_pipeline_priority_lane_inference_queue, design_npc_system_npc_context_system [INFERRED 0.95]
- **Foundational AI First Wave** — design_ai_techniques_01_semantic_memory_and_rag_semantic_memory_first_cut, design_ai_techniques_02_structured_generation_schema_constrained_decoding, design_ai_techniques_05_inference_performance_utility_inference_lane [EXTRACTED 1.00]
- **Canonical World Simulation Signals** — design_world_geography_location_traversal_graph, design_time_system_game_time_system, design_weather_system_weather_simulation [INFERRED 0.85]
- **Grounded NPC Cognition Stack** — design_ai_techniques_01_semantic_memory_and_rag_hybrid_memory_retrieval, design_ai_techniques_04_agent_planning_and_tools_read_only_worldview_tools, design_ai_techniques_07_social_simulation_belief_and_rumor_store, design_ai_techniques_10_knowledge_graph_grounding_provenanced_fact_and_belief_graph [INFERRED 0.85]
- **Shipped Wave 1 input enrichment combines discovery, physical-action syntax, and persistent shell-style recall in one interaction surface.** — design_ideas_input_enrichment_ideas_shipped_wave_one, design_input_enrichment_01_slash_autocomplete_frontend_design, design_input_enrichment_02_emote_actions_action_syntax, design_input_enrichment_03_input_history_frontend_design [EXTRACTED 1.00]
- **Structured affect, immersion-oriented prompt policy, and sleep-time consolidation form a proposed long-horizon NPC cognition loop whose consolidation stage is intentionally deferred.** — design_ideas_emotion_driven_dialogue_and_simulation_affective_state, design_ideas_npc_prompt_immersion_ideas_prompt_context, design_ideas_npc_sleep_dream_consolidation_hierarchical_memory [INFERRED 0.85]
- **The superseded deterministic renderer, the semantic diorama compositor, and state-driven CSS effects together describe a layered visual presentation architecture over canonical simulation state.** — design_ideas_graphical_world_view_deterministic_renderer, design_ideas_parish_diorama_semantic_runtime_compositor, design_ideas_visual_effects_system_state_driven_effect_manager [INFERRED 0.85]
- **Historical Input Enrichment Portfolio** — design_input_enrichment_readme_historical_input_enrichment_designs, design_input_enrichment_06_emoji_reactions_bidirectional_emoji_reactions, design_input_enrichment_07_quick_travel_buttons_location_quick_travel, design_input_enrichment_09_smart_replies_contextual_action_suggestions, design_input_enrichment_15_tab_complete_nouns_known_noun_tab_completion [EXTRACTED 1.00]
- **Subagent-Gated Exterior Plate Pipeline** — graphics_v2_map_crop_selection_protocol_plate_first_crop_selection, graphics_v2_map_reader_stage_template_reproducible_map_reader_stage, graphics_v2_map_to_background_plate_pipeline_map_to_plate_research_pipeline, graphics_v2_map_to_bu_style_reproducible_pipeline_subagent_gated_bu_pipeline, graphics_v2_kilteevan_exterior_pipeline_run_template_five_stage_pipeline_run, graphics_v2_one_shot_background_plate_test_protocol_cleanroom_one_shot_test, graphics_v2_portable_background_plate_one_shot_template_portable_background_plate_contract [EXTRACTED 1.00]
- **Graphics V2 Evidence and Runtime Separation** — graphics_v2_agents_graphics_research_governance, graphics_v2_agents_clean_context_recipe_evidence, graphics_v2_agents_historic_map_veto_authority, graphics_v2_readme_graphics_v2_research_index, graphics_v2_cartographic_comparisons_readme_map_accuracy_comparison_evidence, graphics_v2_runtime_layers_and_independent_variables_canonical_neutral_location_plate, graphics_v2_runtime_layers_and_independent_variables_runtime_layer_stack [INFERRED 0.95]
- **County-Scale Overhead Art Evidence** — graphics_v2_overhead_art_cycle_ce_county_tile_continuity_readme_county_tile_continuity_proof, graphics_v2_overhead_art_cycle_cf_production_county_pipeline_readme_production_county_pipeline_proof, graphics_v2_overhead_art_cycle_cf_production_county_pipeline_validation_report_validation_report, graphics_v2_overhead_art_cycle_cg_single_tile_provider_tests_readme_single_tile_provider_tests [INFERRED 0.95]
- **Quality Harness Tooling Plan Set** — plans_bug_report_tool_bug_report_tool_plan, plans_game_quality_harness_game_quality_harness_plan, plans_harness_mock_shadow_harness_mock_shadow_plan, plans_harness_skill_ingest_harness_skill_ingestion_plan [INFERRED 0.95]
- **Retired Illustrated Notebook History** — plans_illustrated_notebook_real_illustrated_notebook_real_plan, plans_illustrated_notebook_roadmap_illustrated_notebook_roadmap, plans_illustrated_notebook_real_retired_status, plans_illustrated_notebook_roadmap_closed_retired_status [EXTRACTED 1.00]
- **Complementary Model Evaluation Layers** — plans_llm_quality_evals_quantitative_quality_sensor_plan, plans_promptfoo_pentest_plan_websocket_aware_security_pentest, plans_rundale_bench_public_reproducible_benchmark_plan [EXTRACTED 1.00]
- **Structural Preconditions of Famine** — research_farming_agriculture_potato_conacre_precarity, research_food_drink_potato_buttermilk_economy, research_food_drink_meal_months, research_family_life_marriage_fortune_and_inheritance, research_forthcoming_decades_structural_path_to_famine [INFERRED 0.95]
- **Oral Identity and Knowledge World** — research_education_literacy_oral_print_knowledge_network, research_irish_language_irish_english_diglossia, research_music_entertainment_oral_music_culture, research_names_naming_conventions_patronymics_and_bynames, research_mythology_folklore_living_fairy_faith [INFERRED 0.85]
- **Authority and Resistance in Rural Ireland** — research_law_governance_colonial_legal_order, research_law_governance_land_tithe_and_local_coercion, research_politics_movements_catholic_mass_mobilization, research_politics_movements_competing_political_allegiances, research_recent_history_pre1820_1798_as_living_memory, research_religion_spirituality_established_church_tension [INFERRED 0.95]
- **Location Editor Three-Region Workflow** — screenshots_location_designer_searchable_location_inventory, screenshots_location_designer_interactive_location_graph_map, screenshots_location_designer_crossroads_network_hub [INFERRED 0.85]
- **Player position, landmark topology, and current simulation status jointly contextualize navigation through the historical parish.** — screenshots_map_kilteevan_player_location, screenshots_map_landmark_network, screenshots_map_status_context [EXTRACTED 1.00]
- **NPC Authoring Workspace** — screenshots_npc_designer_parish_npc_designer_screen, screenshots_npc_designer_npc_roster_and_search, screenshots_npc_designer_padraig_darcy_identity_editor, screenshots_npc_designer_home_and_workplace_fields, screenshots_npc_designer_six_axis_intelligence_editor, screenshots_npc_designer_relationship_strength_list, screenshots_npc_designer_designer_section_navigation [EXTRACTED 1.00]
- **Onboarding Inference Power Choices** — screenshots_onboarding_local_inference_onboarding_choice_screen, screenshots_onboarding_local_inference_local_inference_option, screenshots_onboarding_local_inference_hosted_byok_option [EXTRACTED 1.00]
- **Static Illustrated Interaction Model** — screenshots_quality_harness_static_ui_scene_first_world_view, screenshots_quality_harness_static_ui_notebook_status_and_context, screenshots_quality_harness_static_ui_action_stamps_and_intent_strip, screenshots_quality_harness_static_ui_persistent_notebook_navigation_tabs [EXTRACTED 1.00]
- **First-viewport world, people, notebook, and player-intent interaction model** — screenshots_rundale_illustrated_parish_notebook_ui, screenshots_rundale_kilteevan_village_scene, screenshots_rundale_nearby_people_panel, screenshots_rundale_roisin_connolly_scene_selection, screenshots_rundale_parish_notes_notebook, screenshots_rundale_direct_action_toolbar, screenshots_rundale_natural_language_intent_input, screenshots_rundale_world_status_header [EXTRACTED 1.00]

## Communities (80 total, 21 thin omitted)

### Community 0 - "Overhead Art Experiment Index"
Cohesion: 0.05
Nodes (40): B2 Legend Retry, D No-Legend Variant, Cycle CB Direct-Overhead Style Matrix, Legend Semantic Leak Failure, OS Legend as Symbol Aid, Source Map Layout Authority, Flat Human Glyph Direction, Cycle CC Overhead Character Concepts (+32 more)

### Community 1 - "Candidate mechanics include player vitals and fatigue, sleep, inventory and items, economy, skills, reputation, status effects, lighting, housing, seasonal agriculture, disease, transport, tasks, and difficulty or death."
Cohesion: 0.06
Nodes (35): The proposed deterministic emotional state combines valence, arousal, dominance, social warmth, and stress load with curated human-readable labels and appraisal tags., Emotion-driven dialogue and simulation is a brainstorm for a structured affect layer that influences both moment-to-moment speech and longer-running NPC behavior., A dialogue-policy selector maps affect and social appraisal to bounded strategies such as de-escalate, probe, deflect, bond, withdraw, or confront, then lets archetype style shape the expression., The recommended rollout starts with a small label-plus-intensity model and six dialogue policies, exposes hidden structured fields and risk flags, and evaluates stability before adding contagion, memory, and simulation gates., The game-ideas catalogue is explicitly early-stage brainstorming rather than a commitment, covering twenty possible social, economic, political, linguistic, folkloric, and narrative systems., The brainstorm prioritizes reputation, weather storytelling, and the blow-in arc highest; factions, secrets, fairy encounters, and the holy well are medium; land, crafts, poitín, and Irish progression follow later., Candidate systems include factions, multidimensional reputation, secrets and confession, tenancy and land agents, trade and craft, Catholic Emancipation, a blow-in newcomer arc, Irish-language progression, and letters., The proposed dependency order begins with player vitals, then sleep, inventory, economy, status effects, reputation, skills, agriculture, quests, and transport. (+27 more)

### Community 2 - "Parish Feature Inventory"
Cohesion: 0.07
Nodes (29): ADR-002 Four-Tier Resource Allocation, State Inflation and Cross-Tier Coherence, Crash Recovery Without Gameplay Stutter, ADR-003 Three-Layer WAL Persistence, ADR-004 Git-Like Timeline Decision, Independent Branch Clocks, Background-Lane Critic Escape Hatch, ADR-005 Ollama Local Inference Baseline (+21 more)

### Community 3 - "April 2026 Regression Coverage Audit"
Cohesion: 0.08
Nodes (24): Rundale Merge and Proof Workflow, Quality Sensor and Harness Map, Completion Witness Gate, Game World Regression Coverage Report, Unwired Travel Encounter Gap, NPC System Regression Coverage Report, Tier 3 and Tier 4 NPC Coverage Gap, Player Input Regression Coverage Report (+16 more)

### Community 4 - "Priority-Lane Inference Queue"
Cohesion: 0.09
Nodes (24): Cognitive Level-of-Detail, Four-Tier Cognition Policy, Bounded Debug Telemetry, Debug System, Debug Snapshot UI, Inference Call Inspector, Curated and Generated Geography Provenance, parish-geo-tool (+16 more)

### Community 5 - "Graphics V2 Research Index"
Cohesion: 0.13
Nodes (23): Clean-Context Recipe Evidence Discipline, Graphics V2 Research Governance, Historic Map Veto Authority, Graphics V2 Map-Accuracy Comparison Evidence, Beechwood BJ Notebook Visual Target, Cycle BL Soft-Garden Next Run, Murphy C and Connolly C Gameplay-Fit Targets, Historically Anchored Interior Cutaway Grammar (+15 more)

### Community 6 - "Provenanced Fact and Belief Graph"
Cohesion: 0.10
Nodes (22): Hybrid Memory Retrieval, Semantic Memory and RAG Brainstorm, Semantic Memory First Cut, Schema-Constrained Decoding, Structured Generation Brainstorm, Two-Segment Streaming, Agent Planning and Tools Brainstorm, Bounded ReAct Loop (+14 more)

### Community 7 - "Game Time System (Implemented)"
Cohesion: 0.11
Nodes (18): Adversarial Safety Suite, AI Evaluation and Safety Brainstorm, Golden Transcript Evaluation, Anchored Candidate Generation, Human Review and Provenance Gate, LLM-Assisted Authoring Brainstorm, GameTestHarness, Game Testing Strategy (Implemented) (+10 more)

### Community 8 - "Shipped Wave 1 comprises slash-command autocomplete, fifty-entry local input history, asterisk-delimited emote actions, Shift+Enter multiline input, and location travel chips."
Cohesion: 0.12
Nodes (18): Voice transcription is the next major input path, while private whispering is deliberately later because it requires end-to-end conversational privacy rather than only a frontend syntax treatment., Wave 1 resolves input conflicts with a unified mention/slash dropdown, history arrows only when no dropdown owns the keys and the cursor is at a boundary line, and raw emote text passed through for backend interpretation., Shipped Wave 1 comprises slash-command autocomplete, fifty-entry local input history, asterisk-delimited emote actions, Shift+Enter multiline input, and location travel chips., The input-enrichment brainstorm records Wave 1 as shipped, the known-noun tab completion as shipped, and later voice, reaction, whisper, reply, preview, tone, and suggestion work at differing future priorities., A static CommandDescriptor registry supplies names, descriptions, argument hints, debug visibility, and dynamically crossed provider, model, and key commands for dialogue, simulation, and intent categories., The registry is intentionally frontend-only because backend command parsing already exists; a server-driven registry is deferred until commands become mod-driven or runtime-dynamic., Slash autocomplete is a low-effort frontend design that reuses mention-dropdown infrastructure to filter, describe, navigate, and insert valid system commands beginning at input position zero., Emote actions use matched asterisks to distinguish physical behavior from speech, render action segments in italics, and allow pure action or mixed action-plus-dialogue turns. (+10 more)

### Community 9 - "Rundale Historical Research Collection"
Cohesion: 0.15
Nodes (18): Ambient Audio License Contract, Period-Plausible Ambient Audio Catalog, Architecture and Housing in 1820s Ireland, Class-Legible Architectural World Nodes, Clothing and Textiles in 1820s Ireland, Clothing as an NPC Class Marker, Crime and Secret Societies in 1820s Ireland, Crown Justice and Community Justice (+10 more)

### Community 10 - "Human-facing hub for agent engineering references"
Cohesion: 0.12
Nodes (17): Accepted thin-client, thick-server web and mobile architecture, Accepted GUI-only ambient audio playback with rodio and graph-based propagation, Accepted replacement of egui/eframe with Tauri 2 and Svelte, Accepted Axum browser-test mode sharing the Svelte frontend and Parish logic, Frontend transport adaptation between Tauri IPC and browser HTTP/WebSocket, Local GitHub Actions reproduction with act, Docker, and just recipes, Live-process proof requirement for runtime-shipping changes, PR proof-bundle gate linking acceptance criteria, evidence, and judge verdict (+9 more)

### Community 12 - "Bug Report Tool Plan"
Cohesion: 0.13
Nodes (15): Bug Report Tool Plan, Dry-Run Evidence Bundle, MCP File-Bug Surface, Bug Tool Runtime Parity, Shared Core Bug Orchestration, Deterministic Scoring Contract, Game Quality Harness Plan, Harness-Owned SQLite Persistence (+7 more)

### Community 13 - "Potato and Buttermilk Economy"
Cohesion: 0.14
Nodes (15): Kinship Household Economy, Marriage, Fortune, and Inheritance Strategy, Potato and Conacre Precarity, Rundale Agricultural System, Seasonal Farming Calendar, Bog and Hedgerow Landscape, Landscape as Active Character, Food as Class and Social Signal (+7 more)

### Community 14 - "Colonial Legal Order"
Cohesion: 0.15
Nodes (14): Colonial Legal Order, Land, Tithe, and Local Coercion, Community Folk Healthcare, Disease of Poverty System, Living Fairy Faith, Supernatural Ambiguity Principle, Catholic Mass Mobilization, Competing Political Allegiances (+6 more)

### Community 15 - "Four-Judge Borda Quality Harness"
Cohesion: 0.17
Nodes (12): Four-Judge Borda Quality Harness, Quantitative LLM Quality Sensor Plan, Shared Training, Regression, and Serving Signal, Debug Snapshot Disclosure Baseline, Promptfoo Security Taxonomy and Gates, WebSocket-Aware Promptfoo Security Pentest, Five Inference-Category Benchmark Slices, Holdout and Judge-Drift Controls (+4 more)

### Community 16 - "Gemma 4 Rundale Training Plan"
Cohesion: 0.18
Nodes (11): Background-Lane Critic, Bilingual Phase Split, Four-Axis Judge Stack, Gemma 4 Rundale Training Plan, Gemma 4 9B QLoRA, Iterated Direct Preference Optimization, Judge Calibration Gate, Proposed Training Status (+3 more)

### Community 17 - "Central isometric Kilteevan village scene with chapel, cottages, roads, bridge, stream, and people"
Cohesion: 0.18
Nodes (11): Direct action toolbar for Talk, Ask, Help, Observe, and Leave, Illustrated parchment Parish Notebook game interface, Central isometric Kilteevan village scene with chapel, cottages, roads, bridge, stream, and people, Map and Time controls framing the current world view, Natural-language intent input suggesting a question for Roisin, Nearby roster of Roisin Connolly, Padraig Darcy, Siobhan Murphy, and Fr. Declan Tierney, Parish Notes notebook summarizing place, scene, conditions, and next actions, Persistent Notes, People, Places, Rumours, and Journal tabs (+3 more)

### Community 18 - "Scaling Seam Checklist"
Cohesion: 0.22
Nodes (9): Runtime and Architecture Gotchas, Idempotency-Key Retry Contract, Accepted Risk: In-Memory Idempotency Cache, EventBus and Topic Push Boundary, ModSource Content Boundary, Request Identity and Tracing Boundary, Scaling Seam Checklist, SessionStore Persistence Boundary (+1 more)

### Community 19 - "Game Quality Control System"
Cohesion: 0.22
Nodes (9): Game Quality Control System, Gate Plus Quality Scoring, Harness Finding-to-Fix Closed Loop, Live Harness Proof Contract, parish-harness Crate, Single Harness Persistence Seam, Quality-Harness Skill Run Ingest, Cold MCP Registration (+1 more)

### Community 20 - "Illustrated Notebook Real Plan"
Cohesion: 0.22
Nodes (9): Clean Notebook Render Boundary, Illustrated Notebook Real Plan, Overlay Object Continuity, Retired Notebook Renderer Status, Closed and Retired Roadmap Status, Historical Notebook North Star, Illustrated Notebook Roadmap, Residual Deferred Notebook Risks (+1 more)

### Community 21 - "Accepted independent provider routing for dialogue, simulation, and intent"
Cohesion: 0.25
Nodes (8): Accepted independent provider routing for dialogue, simulation, and intent, Per-category configuration precedence from base config through runtime overrides, Incremental dialogue extraction from a partial JSON stream, Accepted provider-native JSON structured output for Tier 1 dialogue, Tool-use deferral pending quality evals, reliable small local models, or schema pressure, Proposed and deferred provider-native NPC tool calling, Candidate top-K memory retrieval with SQLite vectors and recency fallback, Proposed and deferred embedding-based long-term NPC memory retrieval

### Community 22 - "The proposed Interactive Parish Diorama supersedes monolithic scene plates with a runtime-composed, scene-based graphical presentation over the existing living-world simulation."
Cohesion: 0.25
Nodes (8): The earlier direction proposed a backend-agnostic parish-sprite crate that deterministically renders 24-by-32 NPC sprites and 160-by-120 scenes from hashed recipes, reusable parts, templates, and palette tints., Critical rendering should remain deterministic and free of runtime LLM calls so snapshots, replay, CLI, web, and desktop behavior remain verifiable and mode-equivalent., Graphical World View is superseded by the Interactive Parish Diorama design, although its deterministic procedural pixel-art and mode-parity principles remain relevant., A developer-only parish-art-tool generates and curates bounded props, sprites, references, manifests, and preview compositions; gameplay performs no image generation and every accepted asset passes post-processing and human review., The proposed Interactive Parish Diorama supersedes monolithic scene plates with a runtime-composed, scene-based graphical presentation over the existing living-world simulation., The engine owns scene geometry, layers, exits, waterways, buildings, props, z-order, hotspots, and NPC slots, then composes curated asset atoms while treating full generated plates only as references or temporary underlays., The diorama is a presentation layer that reuses the 22-node world graph, live NPC schedules and introduction semantics, time, weather, palette, transport, and asset validation without changing dialogue, world semantics, or save format., The first shippable proof is two exterior scenes, one pub interior, a common asset pack, a small representative NPC sprite set, complete connection hotspots, and structural scene-versus-engine verification before expansion.

### Community 24 - "Irish-English Diglossia"
Cohesion: 0.25
Nodes (8): Hedge-School Education, Oral and Print Knowledge Network, Irish-English Diglossia, Language Choice as Social Signal, Oral Music and Story Culture, Seasonal Social Gatherings, Dual Irish and English Names, Patronymics and Bynames

### Community 25 - "Run Locally Recommended Option"
Cohesion: 0.25
Nodes (8): Dual MLX Model Setup, Hosted API BYOK Option, Run Locally Recommended Option, 48 GB Unified-Memory Mac Fit, One-Time Nine-Gigabyte Weight Download, Onboarding Inference Choice Screen, Operating-System Keychain Storage, Supported Hosted Provider Set

### Community 26 - "Three-Layer Persistence Model"
Cohesion: 0.29
Nodes (7): Continuous Save System (Implemented Phase 4), SQLite WAL Branch DAG, Three-Layer Persistence Model, Data Depth and Cognitive LOD, Pre-Generated SQLite Population, Scalable NPC Data Design (Partial), Read-Only World Database plus Save Overlay

### Community 27 - "NPC Agenda Scheduler"
Cohesion: 0.29
Nodes (7): NPC Agenda Scheduler, Independent NPC Approval Stop Point, Independent NPC Agents Plan, Independent NPC Kill Switch, Proposed NPC Agent Status, Revision-Stamped NPC Intents, NPC Schedule Shadow Mode

### Community 28 - "Interactive Parish Diorama Runtime Compositor Plan"
Cohesion: 0.29
Nodes (7): Engine-Owned Semantic Scene Layout, Human-Curated Asset Transaction, Interactive Parish Diorama Runtime Compositor Plan, Three-Scene Diorama Vertical Slice, Manifest-to-Translator Bijection, No-Build MCP Cold Registration, Cold-Shim Proxy Handoff Protocol

### Community 29 - "Authoritative Rundale Feature-Status Matrix"
Cohesion: 0.67
Nodes (3): Authoritative Rundale Feature-Status Matrix, Parallel Portfolio Roadmap Model, Rundale-Bench In-Progress Status

### Community 30 - "Padraig Darcy Identity Editor"
Cohesion: 0.29
Nodes (7): Parish Designer Section Navigation, Home and Workplace Fields, NPC Roster and Search Sidebar, Padraig Darcy Identity Editor, Parish NPC Designer Screen, NPC Relationship Strength List, Six-Axis Intelligence Slider Editor

### Community 31 - "ADR-001 Named Location Graph Decision"
Cohesion: 0.40
Nodes (6): ADR-001 Named Location Graph Decision, Text-Adventure Spatial Abstraction, ADR-009 Authentic Geography, Fictional Identity, Geographic Data License Obligations, Description Provenance Tiers, ADR-011 Separate Geo-Tool OSM Pipeline

### Community 32 - "Rundale Documentation Hub"
Cohesion: 0.33
Nodes (6): ADR-012 Progressive Documentation Hierarchy, Documentation Status Reconciliation Contract, Rundale Documentation Hub, 1820s Ireland Research Archive, Purpose-Based Document Taxonomy, Roadmap Status Authority

### Community 33 - "NPC Portrait Pipeline"
Cohesion: 0.33
Nodes (6): Clean-Context Portrait Audit, Illustrated Notebook Portrait Treatment, NPC Portrait Pipeline, Roster-Driven Portrait Briefing, Six-NPC Two-Candidate Pilot, Small-UI Portrait Acceptance Gate

### Community 34 - "Irregular Mortarless Fieldstone"
Cohesion: 0.33
Nodes (6): Blocky Stone Boundary Caveat, BZ Subagent-Gated Proof, Recipe Evidence versus Visual Target, Irish Dry-Stone Wall References, Irregular Mortarless Fieldstone, Wall Material, Not Layout, Authority

### Community 35 - "Harness Mock and Shadow Plan"
Cohesion: 0.33
Nodes (6): Capturing Harness Emitter, Harness Mock and Shadow Plan, Legacy Harness Default Path, Pre-State Shadow Divergence Ledger, Real-Loop Harness Execution, Scriptable Mock Inference Client

### Community 36 - "Historical Input Enrichment Design Portfolio"
Cohesion: 0.60
Nodes (5): Bidirectional Emoji Reactions Design, Location Quick-Travel Buttons Design, Contextual Action Suggestions Design, Known-Noun Tab Completion Design, Historical Input Enrichment Design Portfolio

### Community 38 - "Parish Designer Location Editor"
Cohesion: 0.40
Nodes (5): The Crossroads Network Hub, Interactive Location Graph Map, Parish Designer Location Editor, Searchable Location Inventory, World Authoring Navigation

### Community 39 - "Rundale's full-screen map overlay presents the parish as an interactive network laid over a sepia historical Ordnance Survey-style map."
Cohesion: 0.40
Nodes (5): Rundale's full-screen map overlay presents the parish as an interactive network laid over a sepia historical Ordnance Survey-style map., The player's current position is visibly anchored at Kilteevan Village with a house-shaped marker and a highlighted connection into the parish network., Named map nodes include St. Brigid's Church, Crossroads, Letter Office, Forge, Weaver's Cottage, Mill, Lime Kiln, and Murphy's Farm, joined by route lines., Dense period cartographic labels, field boundaries, roads, waterways, and monochrome engraving visually ground the game network in a historical rural landscape., The surrounding desktop status bar grounds the map in Kilteevan Village at 08:03 on a clear spring Monday morning while the simulation is paused.

### Community 40 - "Illustrated Parish Notebook Static UI"
Cohesion: 0.40
Nodes (5): Action Stamps and Intent Strip, Illustrated Parish Notebook Static UI, Notebook Status and Context Surfaces, Persistent Notebook Navigation Tabs, Scene-First Rural World View

### Community 41 - "docs/index.md as exhaustive authoritative documentation hub with unique ADR numbers"
Cohesion: 0.50
Nodes (4): docs/index.md as exhaustive authoritative documentation hub with unique ADR numbers, Accepted purpose-based documentation hierarchy with controlled status vocabularies, ADR 022 and 023 renumbering after the former 018 collision, Authoritative ADR index and decision-record section contract

### Community 42 - "The five control layers are pre-commit prevention, adversarial PR gating, weekly SQALE measurement, autonomous issue-to-fix-to-land repair, and custom architectural rules or fitness tests."
Cohesion: 0.50
Nodes (4): The five control layers are pre-commit prevention, adversarial PR gating, weekly SQALE measurement, autonomous issue-to-fix-to-land repair, and custom architectural rules or fitness tests., The committed rollout order is pipeline-first: autonomous repair, adversarial review, foundation checks, measurement, then architectural hardening, because the repair path must exist before scanners create more debt findings., Debt Shield is a proposed hyper-aggressive technical-debt control system spanning prevention, pull-request review, measurement, autonomous repair, and architectural enforcement., Only classified non-logic debt fixes may auto-land after CI; risky behavior, security, dependency, schema, or public-API changes remain human-reviewed, with budgets, turn caps, squash commits, and revertability as safety controls.

### Community 43 - "The fully on-device iOS port is proposed but implementation-ready: design decisions are closed, no code work has started, and the target is an offline iPhone build containing UI, simulation, persistence, and local inference."
Cohesion: 0.50
Nodes (4): A Mac with Xcode, a physical iPhone 15 Pro or newer, a paid Apple developer account, ODR-packaged models, and measured prompt tuning are prerequisites; headless work can land scaffolding but cannot prove a runnable or shippable iOS backend., The fully on-device iOS port is proposed but implementation-ready: design decisions are closed, no code work has started, and the target is an offline iPhone build containing UI, simulation, persistence, and local inference., The locked iOS architecture introduces a dynamic InferenceBackend abstraction and a statically linked LiteRT-LM C ABI backend using Gemma 4 E2B by default, with token streaming and E4B as a device-gated quality option., Models are delivered through Apple On-Demand Resources, saves resolve under Application Support, mod assets ship as Tauri resources, and safe-area, touch, map zoom, and keyboard occlusion are explicit UI contracts.

### Community 44 - "The period-map-tiles feature combines OpenStreetMap context with historical six-inch 1829–1842 mapping, selected through a tile-source registry and a three-tier cache of user, bundled, then upstream tiles."
Cohesion: 0.50
Nodes (4): Map Evolution is labeled a brainstorm or RFC rather than a blanket commitment, while its internal status table records several map phases and period tiles as completed and later work as partial or future., Historical map usage requires corrected CC-BY attribution and treats the period map as a historically grounded visual layer rather than an unlicensed anonymous background., The project deliberately does not bulk-populate historical tiles because of service behavior, traffic, WAF, and licensing coordination; it ships on-demand caching and requires NLS contact before launch., The period-map-tiles feature combines OpenStreetMap context with historical six-inch 1829–1842 mapping, selected through a tile-source registry and a three-tier cache of user, bundled, then upstream tiles.

### Community 48 - "Local Dialogue Rejection Sampler"
Cohesion: 0.67
Nodes (3): Critic and Self-Refinement Loop, Dialogue Quality Loops Brainstorm, Local Dialogue Rejection Sampler

### Community 49 - "Personalization and Learning Brainstorm"
Cohesion: 0.67
Nodes (3): Opt-In Local Learning, Persisted Player Profile, Personalization and Learning Brainstorm

### Community 50 - "Multimodal Brainstorm"
Cohesion: 0.67
Nodes (3): Deterministic Visual Assets, Multimodal Brainstorm, Offline Voice Pipeline

### Community 51 - "Authored Directed Events"
Cohesion: 0.67
Nodes (3): AI Director Brainstorm, Authored Directed Events, Nudges, Not Puppetry

### Community 52 - "Parish Designer"
Cohesion: 0.67
Nodes (3): Clean Mod Round-Trip Invariant, Parish Designer, Running-Game Isolation

### Community 53 - "The proposed Cloud Run runtime pins minimum and maximum instances to one, keeps CPU allocated for simulation ticks, reconnects WebSockets before the 60-minute limit, and uses Gemini for inference."
Cohesion: 0.67
Nodes (3): Cloud Run hosting is a feasibility design, not an implemented deployment: second-generation Cloud Run can host Rundale only with a deliberately constrained single-instance topology., Cloud persistence requires SQLite-safe shared storage, preferably GCS FUSE with DELETE journaling only after validation or Filestore as fallback, while public deployment requires an explicit authentication layer such as IAM, IAP, in-app OAuth, or Cloudflare Access., The proposed Cloud Run runtime pins minimum and maximum instances to one, keeps CPU allocated for simulation ticks, reconnects WebSockets before the 60-minute limit, and uses Gemini for inference.

### Community 54 - "Illustrated Parish Notebook UI (Retired Historical Experiment)"
Cohesion: 0.67
Nodes (3): Conservative State Derivation, Illustrated Parish Notebook UI (Retired Historical Experiment), World-First Notebook Layout

### Community 55 - "Natural-Language Player Input (Implemented)"
Cohesion: 0.67
Nodes (3): Feature-Gated Debug Commands, Natural-Language Player Input (Implemented), NPC @Mention Targeting

### Community 56 - "Topology Review Gate"
Cohesion: 0.67
Nodes (3): Imagegen Optional Local Panels, Masked Local Seam Repair, Topology Review Gate

### Community 57 - "Doors on Openings Audit"
Cohesion: 0.67
Nodes (3): Doors on Openings Audit, Isomorphic Constant-Scale Audit, Per-Building Door Audit

### Community 58 - "Door-Fixed House References"
Cohesion: 0.67
Nodes (3): Black-Void Door Rejection, Door-Fixed House References, Style Crop Policy

### Community 59 - "Google OAuth Configuration"
Cohesion: 0.67
Nodes (3): Exact OAuth Redirect URI Contract, Google OAuth Configuration, OAuth Silent Disable Behavior

### Community 60 - "Quality-Tiered Source Corpus"
Cohesion: 0.67
Nodes (3): Feature-Tagged Substrate Floor, Licensing and Access Exclusions, Quality-Tiered Source Corpus

### Community 61 - "Phase 7 Thin-Client Cloud Server Plan"
Cohesion: 0.67
Nodes (3): Egui-Everywhere Client Strategy, Remote Session Lifecycle and Observability, Phase 7 Thin-Client Cloud Server Plan

### Community 64 - "Release Tag as Single Source of Truth"
Cohesion: 0.67
Nodes (3): Linux-Only Release Scope, Release Tag as Single Source of Truth, Two-Level Release Dry Run

### Community 65 - "Live Web-Mode Browser Evidence"
Cohesion: 0.67
Nodes (3): Live Web-Mode Browser Evidence, Persistence Route Verification, Web Transport Parity Defects

### Community 66 - "Paired Inference and Transcript Artifacts"
Cohesion: 0.67
Nodes (3): Paired Inference and Transcript Artifacts, Request-ID Dialogue Traceability, Inference Log Secret Redaction Boundary

## Knowledge Gaps
- **201 isolated node(s):** `Runtime Mode Parity`, `Mod-Driven Engine Content Separation`, `Purpose-Based Document Taxonomy`, `Active Visual Client Direction`, `1820s Ireland Research Archive` (+196 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **21 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **What connects `Runtime Mode Parity`, `Mod-Driven Engine Content Separation`, `Purpose-Based Document Taxonomy` to the rest of the system?**
  _201 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Overhead Art Experiment Index` be split into smaller, more focused modules?**
  _Cohesion score 0.052564102564102565 - nodes in this community are weakly interconnected._
- **Should `Candidate mechanics include player vitals and fatigue, sleep, inventory and items, economy, skills, reputation, status effects, lighting, housing, seasonal agriculture, disease, transport, tasks, and difficulty or death.` be split into smaller, more focused modules?**
  _Cohesion score 0.058823529411764705 - nodes in this community are weakly interconnected._
- **Should `Parish Feature Inventory` be split into smaller, more focused modules?**
  _Cohesion score 0.07389162561576355 - nodes in this community are weakly interconnected._
- **Should `April 2026 Regression Coverage Audit` be split into smaller, more focused modules?**
  _Cohesion score 0.08333333333333333 - nodes in this community are weakly interconnected._
- **Should `Priority-Lane Inference Queue` be split into smaller, more focused modules?**
  _Cohesion score 0.09057971014492754 - nodes in this community are weakly interconnected._
- **Should `Graphics V2 Research Index` be split into smaller, more focused modules?**
  _Cohesion score 0.13438735177865613 - nodes in this community are weakly interconnected._