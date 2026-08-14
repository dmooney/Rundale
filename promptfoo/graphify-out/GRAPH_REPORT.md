# Graph Report - /Users/dmooney/.codex/worktrees/cec12f05-2980-42b5-9a1a-5b77ab44cb31/Rundale/promptfoo  (2026-08-09)

## Corpus Check
- Corpus is ~34,020 words - fits in a single context window. You may not need a graph.

## Summary
- 555 nodes · 706 edges · 38 communities (30 shown, 8 thin omitted)
- Extraction: 96% EXTRACTED · 4% INFERRED · 0% AMBIGUOUS · INFERRED: 28 edges (avg confidence: 0.78)
- Token cost: 0 input · 0 output

## Community Hubs (Navigation)
- Astro Render Leaderboard
- Jsonl Holdout Byte
- Rubric Sha256 Byte
- Candidate Cost Rundale
- Enumerate Fetch Candidate
- Leaderboard Benchmark Cost
- Astroj Svelte Package
- Benchmark Model Candidate
- Funnel Slice State
- Leaderboard Score Candidate
- Judge Contract Configuration
- Assert Slice Configuration
- Handler Capture Server
- Aggregate Report Meta
- Mock Handler Server
- Scatterplot Svelte Xpx
- Judge Load Common
- Brand Brandforfamily Fallbackhex
- Package Promptfoo Script
- Build Dataset Runtime
- Runtime-Faithful Benchmark Dataset
- Candidate Generate Prompt
- Fake Judge Check
- Rundale Bench Promptfoo
- Tsconfig Include Astro
- Cost Usd Pricing
- Load Dataset Perf
- Intent Assert Exact
- Astro Svelte Site
- Capture Prompt Drive
- Manifest Rundale Bench
- Prompt Passthrough Trivial
- Candidate Rundale Call
- Pin Manifest Sha256
- Rundale Bench Site
- Reproducible Model Price
- Local Dialogue Benchmark

## God Nodes (most connected - your core abstractions)
1. `loadBenchData()` - 15 edges
2. `enumerate_all()` - 14 edges
3. `slices` - 13 edges
4. `get_assert()` - 12 edges
5. `assets` - 12 edges
6. `Rundale Bench V2 Corpus Integrity Manifest` - 12 edges
7. `generate_candidate()` - 10 edges
8. `main()` - 10 edges
9. `Rundale Bench v2 Promptfoo Suite` - 10 edges
10. `Handler` - 9 edges

## Surprising Connections (you probably didn't know these)
- `Streaming Performance Probe Configuration` --conceptually_related_to--> `generate_candidate()`  [INFERRED]
  promptfooconfig.perf.yaml → rb_common.py
- `generate_candidate()` --implements--> `Runtime-Faithful Request Replay`  [EXTRACTED]
  rb_common.py → README.md
- `OpenAI-Compatible Rundale Bench Test Double` --shares_data_with--> `Nine-Axis Dialogue Judge Contract`  [INFERRED]
  scripts/mock_server.py → v2/rubrics/dialogue.system.md
- `OpenAI-Compatible Rundale Bench Test Double` --shares_data_with--> `Six-Axis Multi-Turn Judge Contract`  [INFERRED]
  scripts/mock_server.py → v2/rubrics/multiturn.system.md
- `Dialogue Slice Promptfoo Configuration` --references--> `get_assert()`  [EXTRACTED]
  promptfooconfig.dialogue.yaml → assertions/rubric_judge.py

## Import Cycles
- None detected.

## Hyperedges (group relationships)
- **Promptfoo Slice Configuration Matrix** — promptfooconfig_dialogue_dialogue_slice_config, promptfooconfig_gaeilge_gaeilge_slice_config, promptfooconfig_intent_intent_slice_config, promptfooconfig_multiturn_multiturn_slice_config, promptfooconfig_perf_performance_probe_config, promptfooconfig_reaction_reaction_slice_config, promptfooconfig_tier2_sim_tier2_simulation_config, promptfooconfig_tier3_sim_tier3_simulation_config [EXTRACTED 1.00]
- **Candidate Generation and Grading Flow** — rb_common_generate_candidate, assertions_intent_assert_get_assert, assertions_schema_assert_get_assert, assertions_rubric_judge_get_assert, rb_common_judge_item [INFERRED 0.95]
- **Benchmark Exploration Interface** — bench_site_src_components_leaderboard_leaderboard_component, bench_site_src_components_scatterplot_scatterplot_component, readme_append_only_leaderboard, readme_bootstrap_confidence_intervals, readme_gameplay_cost_projection [INFERRED 0.85]
- **Runtime-Faithful Prompt Capture and Dataset Pipeline** — scripts_capture_prompts_runtime_prompt_capture_orchestrator, scripts_capture_server_byte_exact_wire_request_capture, scripts_build_runtime_datasets_runtime_faithful_dataset_builder [EXTRACTED 1.00]
- **Candidate Selection and Tiered Evaluation Pipeline** — scripts_enumerate_candidates_provider_candidate_enumerator, catalog_candidates_viable_candidate_catalog, config_pricing_gameplay_cost_estimator, scripts_funnel_phased_candidate_funnel [INFERRED 0.85]
- **Benchmark Scoring, Publication, and Site Consumption Pipeline** — scripts_funnel_phased_candidate_funnel, scripts_leaderboard_benchmark_of_record_writer, bench_site_src_lib_data_benchmark_site_data_loader, bench_site_src_pages_index_leaderboard_overview, bench_site_src_pages_models_slug_candidate_detail_page [INFERRED 0.85]
- **Promptfoo V2 Reproducibility Chain** — scripts_load_dataset_promptfoo_dataset_loader, scripts_pin_manifest_reproducibility_manifest_pinner, scripts_test_v2_offline_contract_suite, v2_manifest_corpus_integrity_manifest, v2_perf_ids_performance_prompt_sample [INFERRED 0.95]
- **Versioned Judge Contract Lineage** — v2_rubrics_dialogue_system_dialogue_judge_contract, v2_rubrics_judge_sonnet_v1_dialogue_judge_v1_config, v2_rubrics_judge_sonnet_v2_dialogue_judge_v2_config, v2_rubrics_multiturn_system_multiturn_judge_contract, v2_rubrics_judge_multiturn_v1_multiturn_judge_v1_config, v2_rubrics_judge_multiturn_v2_multiturn_judge_v2_config [EXTRACTED 1.00]
- **Offline Evaluation Toolchain** — scripts_load_dataset_promptfoo_dataset_loader, scripts_mock_server_openai_compatible_test_double, scripts_report_failure_aware_result_aggregator, scripts_test_v2_offline_contract_suite, v2_rubrics_dialogue_system_dialogue_judge_contract, v2_rubrics_multiturn_system_multiturn_judge_contract [INFERRED 0.85]

## Communities (38 total, 8 thin omitted)

### Community 0 - "Astro Render Leaderboard"
Cohesion: 0.05
Nodes (46): activeTiers, colMax, dir, filtered, focus(), focusCat, query, sortBy() (+38 more)

### Community 1 - "Jsonl Holdout Byte"
Cohesion: 0.04
Nodes (49): bytes, records, sha256, bytes, records, sha256, bytes, records (+41 more)

### Community 2 - "Rubric Sha256 Byte"
Cohesion: 0.05
Nodes (37): assets, perf.ids.json, rubrics/dialogue.system.md, rubrics/gaeilge.system.md, rubrics/judge_gaeilge_v1.json, rubrics/judge_multiturn_v1.json, rubrics/judge_reaction_v1.json, rubrics/judge_sim_v1.json (+29 more)

### Community 3 - "Candidate Cost Rundale"
Cohesion: 0.08
Nodes (30): Leaderboard and Candidate Catalog Metadata Join, Bootstrap Confidence Interval and CI-Aware Rank Explanation, Gameplay Cost, Value, and Efficiency Frontier Explanation, Gameplay-Token-Weighted Benchmark Methodology, Free, Budget, Mid, and Premium Gameplay Cost Tiers, Cheapest-Provider Model Family Deduplication, Four-Part Real-Time Game Model Viability Filter, Rundale Viable Model Candidate Catalog (+22 more)

### Community 4 - "Enumerate Fetch Candidate"
Cohesion: 0.14
Nodes (28): Any, cost_tier(), _enrich_from_cache(), enumerate_all(), _f(), family_key(), fetch_id_only(), fetch_opencode_go() (+20 more)

### Community 5 - "Leaderboard Benchmark Cost"
Cohesion: 0.08
Nodes (27): Model Brand and Cost-Tier Identity, Best-for Category Focus Sorting, CI-Aware Rank Bands, Efficiency Frontier Marker, Interactive Benchmark Leaderboard, Relative In-Cell Score Bars, Tier and Model Search Filtering, Unfunded Benchmark Empty State (+19 more)

### Community 6 - "Astroj Svelte Package"
Cohesion: 0.08
Nodes (25): astro, @astrojs/check, @astrojs/svelte, dependencies, astro, @astrojs/svelte, simple-icons, svelte (+17 more)

### Community 7 - "Benchmark Model Candidate"
Cohesion: 0.10
Nodes (21): Model Family to Brand Resolver, Deterministic Fallback Brand Identity, Low-Luminance Brand Icon Theme Adaptation, Benchmark Site Data Loader, Confidence-Interval-Aware Ranking, Crash-Safe Category Normalization, Latest-Run Board with Full Candidate History, Quality-Cost Pareto Efficiency Frontier (+13 more)

### Community 8 - "Funnel Slice State"
Cohesion: 0.18
Nodes (18): _dataset(), estimate_phase_usd(), load_candidates(), load_run_state(), main(), _parse(), Path, Phased funnel runner with a budget guard (REQ: phased funnel).  Funnels the enum (+10 more)

### Community 9 - "Leaderboard Score Candidate"
Cohesion: 0.16
Nodes (17): _bootstrap_ci(), build_candidate_rows(), _catalog_prices(), _category_scores(), _intent_item_scores(), main(), _overall(), Path (+9 more)

### Community 10 - "Judge Contract Configuration"
Cohesion: 0.19
Nodes (19): Promptfoo Dataset and Performance Test Loader, OpenAI-Compatible Rundale Bench Test Double, V2 Reproducibility Manifest Pinner, Failure-Aware V2 Result Aggregator, Rundale Bench V2 Offline Contract Suite, Rundale Bench V2 Corpus Integrity Manifest, Performance Warmup and Measurement Prompt Set, Nine-Axis Dialogue Judge Contract (+11 more)

### Community 11 - "Assert Slice Configuration"
Cohesion: 0.12
Nodes (15): Code-Recomputed Dialogue Overall, Empty Output Bench-Bug Exclusion, get_assert(), Degenerate Loop and Fabrication Hard Fails, Code-Recomputed Multiturn Overall, Configurable-API-judge assertion for the LLM-graded slices.  Slices: dialogue (5, Deterministic Simulation Schema Validity, get_assert() (+7 more)

### Community 12 - "Handler Capture Server"
Cohesion: 0.19
Nodes (9): _canned_reply(), _fill_schema(), Handler, main(), BaseHTTPRequestHandler, Recording OpenAI-compatible stub server for runtime-faithful prompt capture (REQ, Synthesize a minimal value satisfying a JSON schema so the engine accepts     th, A short, valid reply. If a JSON schema/object is requested, emit valid JSON; (+1 more)

### Community 13 - "Aggregate Report Meta"
Cohesion: 0.28
Nodes (15): aggregate_intent(), aggregate_perf(), aggregate_quality(), _candidate(), _fmt(), _is_warmup(), main(), _meta() (+7 more)

### Community 14 - "Mock Handler Server"
Cohesion: 0.22
Nodes (8): _axes_for(), _content_for(), Handler, _judge_envelope(), BaseHTTPRequestHandler, Minimal OpenAI-compatible mock for a LIVE end-to-end rundale-bench v2 run withou, Produce a minimal valid instance of a (subset) JSON schema., _synth()

### Community 15 - "Scatterplot Svelte Xpx"
Cohesion: 0.17
Nodes (9): frontier, frontierPath, hover, paidXs, xDom, xPx(), xTicks, xVal() (+1 more)

### Community 16 - "Judge Load Common"
Cohesion: 0.21
Nodes (12): extract_json(), judge_item(), load_judge_config(), load_rubric(), load_rubric_system(), Shared bridge for rundale-bench v2 (promptfoo).  Imports the shared HTTP layer (, Read config/judge.yaml (env-overridable). Tiny hand parser so PyYAML is     not, Load a copied judge JSON (rubric text + rubric_sha256 + axes). (+4 more)

### Community 17 - "Brand Brandforfamily Fallbackhex"
Cohesion: 0.26
Nodes (11): Brand, brandForFamily(), fallbackHex(), fallbackInitials(), FAMILY_TO_SLUG, hslToHex(), isLowLum(), loadIcon() (+3 more)

### Community 18 - "Package Promptfoo Script"
Cohesion: 0.17
Nodes (11): description, devDependencies, promptfoo, name, private, scripts, manifest, report (+3 more)

### Community 19 - "Build Dataset Runtime"
Cohesion: 0.35
Nodes (10): build_multiturn(), classify(), _dedup(), _key(), load_captures(), main(), _persona(), Build runtime-faithful bench datasets from captured engine requests (REQ 2 + 3). (+2 more)

### Community 20 - "Runtime-Faithful Benchmark Dataset"
Cohesion: 0.20
Nodes (11): Engine Request Slice Classification, System-and-User Content Hash Deduplication, Fixed Fifteen Percent Holdout Split, Frozen Legacy Gold Labels and Grading Schema Reuse, Captured-Persona Multiturn Failure-Mode Probes, Runtime-Faithful Benchmark Dataset Builder, Runtime Prompt Capture Orchestrator, Two-Sweep Engine Inference Tour (+3 more)

### Community 21 - "Candidate Generate Prompt"
Cohesion: 0.20
Nodes (10): Streaming Performance Probe Configuration, _gaeilge_candidate_prompt(), Gaeilge Fluency Candidate Contract, generate_candidate(), Intent Parser Prompt and Strict Schema, Run one candidate call for `rec` on `slice_name`. Returns     {output, prompt_to, Rundale Bench v2 Shared Bridge, SLICE_META Judge Metadata Registry (+2 more)

### Community 23 - "Rundale Bench Promptfoo"
Cohesion: 0.22
Nodes (9): _generate_multiturn(), Run a scripted multi-turn conversation, chaining the candidate's own     replies, Seven-Dimension NPC Inference Benchmark, Viable Model Candidate Enumeration, Judge Cost Excluded from Run Spend, Multiturn Dialogue Failure Modes, Rundale Bench v2 Promptfoo Suite, Runtime-Faithful Request Replay (+1 more)

### Community 24 - "Tsconfig Include Astro"
Cohesion: 0.25
Nodes (7): exclude, extends, include, **/*, astro/tsconfigs/strict, .astro/types.d.ts, dist

### Community 25 - "Cost Usd Pricing"
Cohesion: 0.33
Nodes (5): estimate_cost(), gameplay_cost(), Pricing + game-time token profile for rundale-bench v2 (promptfoo).  `COSTS` is, USD/min, USD/hr and per-category USD/min for a provider's price.      Port of ru, USD for one call. Unknown models cost 0.0. Mirrors eval_lib.estimate_cost.

### Community 26 - "Load Dataset Perf"
Cohesion: 0.47
Nodes (5): generate_perf_tests(), generate_tests(), _load_records(), Promptfoo dataset generator: a frozen v2 slice → promptfoo test cases.  Referenc, Perf slice: warmup (discarded) + measure ids from perf.ids.json, against     dia

### Community 27 - "Intent Assert Exact"
Cohesion: 0.40
Nodes (4): Exact Intent Label and Jaccard Grading, get_assert(), Deterministic intent grader (no judge) — exact label + Jaccard.  Reuses rundale-, Intent Slice Promptfoo Configuration

### Community 28 - "Astro Svelte Site"
Cohesion: 0.40
Nodes (5): Rundale GitHub Pages Astro Deployment, Astro Svelte Integration, Benchmark Site Astro Package, Astro 7 and Svelte 5 Site Dependencies, Strict Astro TypeScript Configuration

### Community 30 - "Manifest Rundale Bench"
Cohesion: 0.67
Nodes (3): Manifest Report and Offline Test Scripts, Promptfoo 0.121 Development Dependency, Rundale Bench v2 Package Manifest

## Knowledge Gaps
- **183 isolated node(s):** `name`, `version`, `private`, `type`, `dev` (+178 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **8 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `Rundale Bench v2 Promptfoo Suite` connect `Rundale Bench Promptfoo` to `Judge Load Common`, `Leaderboard Benchmark Cost`, `Candidate Generate Prompt`?**
  _High betweenness centrality (0.032) - this node is a cross-community bridge._
- **Why does `slices` connect `Jsonl Holdout Byte` to `Rubric Sha256 Byte`?**
  _High betweenness centrality (0.019) - this node is a cross-community bridge._
- **Why does `generate_candidate()` connect `Candidate Generate Prompt` to `Judge Load Common`, `Rundale Bench Promptfoo`?**
  _High betweenness centrality (0.017) - this node is a cross-community bridge._
- **What connects `name`, `version`, `private` to the rest of the system?**
  _183 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `Astro Render Leaderboard` be split into smaller, more focused modules?**
  _Cohesion score 0.05081967213114754 - nodes in this community are weakly interconnected._
- **Should `Jsonl Holdout Byte` be split into smaller, more focused modules?**
  _Cohesion score 0.04081632653061224 - nodes in this community are weakly interconnected._
- **Should `Rubric Sha256 Byte` be split into smaller, more focused modules?**
  _Cohesion score 0.05263157894736842 - nodes in this community are weakly interconnected._