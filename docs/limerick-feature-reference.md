# Limerick capability reference

This inventory preserves the engine, established clients, and developer tooling
previously described in the root README. It is implementation reference, not a
mobile delivery plan or a claim of native iPhone support. For the current game
direction and cross-client status, see the [Rundale README](../README.md) and
[mobile convergence plan](plans/mobile-engine-convergence.md).

## Retained-client screenshots

These screenshots document earlier desktop and Designer surfaces. They are not
screenshots of the native iPhone app or the current mobile product direction.

- [Desktop game](screenshots/rundale.png).
- [Map](screenshots/map.png) ([thumbnail](screenshots/thumbnails/map-thumbnail.png)).
- [Ledger](screenshots/ledger.png) ([thumbnail](screenshots/thumbnails/ledger-thumbnail.png)).
- [Designer NPC editor](screenshots/limerick-npc-designer.png).
- [Designer location editor](screenshots/limerick-location-designer.png).

## World simulation

- **Location graph** with fuzzy (Jaro-Winkler) name resolution, prose-described edges, and per-edge traversal counts that drive a "worn paths" map visualization.
- **Hybrid geography**: locations can be real-world (geocoded from OSM), author-pinned, or fully fictional, with relative anchors that let fictional clusters subordinate to a real place.
- **Game clock** with seven time-of-day phases (Midnight → Night) and a configurable real-to-game speed factor (Slowest 80 min/day → Ludicrous ~100 sec/day) tunable at runtime via `/speed`.
- **Four seasons** with seasonal NPC schedules, weather biases, and Tier 4 life-event rates.
- **Weather state machine** — seven states (Clear → PartlyCloudy → Overcast → LightRain → HeavyRain → Storm, plus Fog), adjacent-state-only transitions, 2-hour minimum dwell, season-biased probabilities. NPCs seek shelter in heavy rain.
- **Weather-gated travel** — hazard-tagged routes can become impassable in storms or slower in heavy rain and fog, with alternate-route pathfinding where available.
- **Travel & encounters** — per-edge travel time from lat/lon and transport mode (walk vs. horse/cart), with time-of-day-weighted en-route encounters.
- **Festivals** — Imbolc, Bealtaine, Lughnasa, Samhain trigger relationship boosts and narrative hooks.
- **Attend to the land** — three deterministic, offline actions reveal different layers of a place: `/listen` hears its weather-, shelter-, season-, and time-aware soundscape; `/omen` notices a present detail that might be taken as a sign, while refusing to predict the future; and `/folklore` recalls the location's exact authored tradition without inventing one when no account comes readily to mind. Natural discussion of omens, folklore, or listening to the world can add a brief atmospheric cue while the underlying conversational turn continues. The default-on `place-listening` flag controls all three.

## NPCs — cognitive level-of-detail

A four-tier simulation that scales hundreds of NPCs at varying fidelity based on proximity to the player:

- **Tier 1 (interactive)** — full LLM dialogue, conversation history, gossip recall, memory-augmented prompts; routed through the highest-priority inference lane.
- **Tier 2 (nearby)** — lighter LLM ticks every ~5 game-minutes within ~100 m, producing mood/relationship deltas and overheard conversations.
- **Tier 3 (distant)** — daily batch inference, 10 NPCs per LLM call, on the lowest-priority lane.
- **Tier 4 (far)** — CPU-only probabilistic rules: birth/death/illness/marriage/trade per season, no LLM cost.
- **Memory** — 20-entry short-term ring buffer per NPC with auto-promotion to keyword-indexed long-term memory; persists across tier deflation.
- **Gossip network** — 60 % transmission probability with 20 % distortion on each hop; bystanders overhear and propagate.
- **Six-axis intelligence profile** (verbal, analytical, emotional, practical, wisdom, creative) shapes prompt guidance and speech patterns.
- **Season-aware schedules** with hourly activity/location entries and per-season overrides.
- **Autonomous NPC chains** — after a player turn, NPCs may chain up to three follow-on exchanges driven by relationship strength and mood.
- **Off-screen social simulation** — NPCs interact with one another independent of the player's presence. Tier 2 and Tier 3 inference ticks resolve relationship events, mood shifts, and story beats between non-player characters; outcomes are persisted to world state, progress each NPC's personal story, and surface later as gossip. The world moves forward whether the player is there to witness it or not.
- **Anachronism filter** — ~60-term registry (each entry tagged with origin year and category) flags out-of-period vocabulary in player input so NPCs can react with authentic confusion instead of going along with it.

## LLM inference

- **15 inference providers** out of the box: Ollama, LM Studio, vllm-mlx (Apple Silicon native), OpenAI, Anthropic (native `/v1/messages` API, not the OpenAI-compatibility shim), Google Gemini (native Interactions API), OpenRouter, Groq, xAI Grok, Mistral, DeepSeek, Together AI, Custom (any OpenAI-compatible base URL), and a built-in offline Simulator that needs no model download. Google Gemini 3.7 Flash at Low thinking is the default cloud model across Dialogue, Simulation, Intent, and Reaction, with role-specific output caps, implicit-cache usage telemetry, and Google's Standard service tier. (Additional providers are available via mod-loaded configurations — Cohere, GitHub Models, Qwen, Zhipu, OpenCode Zen, and others.) Local profiles are available for macOS (vllm-mlx) and Linux/Windows (vLLM/Ollama), but none currently passes Rundale's production dialogue promotion gate; first-run setup labels them experimental and recommends BYOK cloud for player dialogue.
- **Per-category routing** — Dialogue, Simulation, Intent, and Reaction can each use a different provider/model/key, switchable at runtime via dot-notation commands (`/provider.dialogue`, `/model.intent`, `/key.simulation`).
- **Measured cloud-dialogue default** — Google's native recommended preset uses `gemini-3.7-flash` with the qualified Low-thinking, 4,096-token production profile. A 12-interaction live soak averaged **$0.00460 per NPC dialogue interaction** (0.46¢; observed range $0.00372–$0.00552) at Google's promotional Standard rates through December 31, 2026. At a human pace of roughly 15 billable NPC exchanges per hour—one every four minutes—that is about **$0.069 per gameplay hour**; a 10–20-exchange pace is approximately $0.046–$0.092/hour, excluding background-model traffic. The retained multi-family judgments, individual API calls, and latency evidence are published in the local qualification dashboard rather than inferred from general-purpose benchmarks.
- **Three-lane priority queue** — Interactive (player dialogue) preempts Background (Tier 2) preempts Batch (Tier 3); a slow batch call cannot block your conversation.
- **Token streaming** with bounded back-pressure (1024-token channel) so a slow consumer never OOMs the engine.
- **Structured JSON output** — NPC turns return `{mood, action, internal_thought, irish_words}`; partial JSON is recovered on truncation.
- **Reachability + timeout knobs** — request, streaming, model-load, and download timeouts all configurable per-environment.
- **Bounded inference log** — recent calls (model, latency, sizes, errors) surface in the debug panel without unbounded memory growth.
- **Five-layer prompt-injection defence** (ADR-010) — role separation, delimited input with "sandwiched" instructions, input sanitisation at the system boundary, strict output parsing/validation, and output filtering before display.

## Player experience

- **Free-text dialogue** parsed by an LLM intent extractor (Move / Talk / Look / Examine / Interact), with a regex fallback.
- **`@mention` targeting** to address a specific NPC in a crowded room.
- **Slash-command surface** spanning save management, time control, provider config, debug, theming, and map switching — the same set works in the GUI, web, and CLI.
- **Chat-first play screen** — the readable transcript, enriched command input, nearby people, language hints, map context, and status are available on the existing desktop and responsive-web routes.
- **Responsive illustrated context** — approved watercolor scene plates, NPC portraits, and a selected map icon render as ordinary responsive DOM images without a canvas renderer.
- **Coordinated secondary surfaces** — Map, Save/Load, Debug, Mod, Bug Report, and shortcuts share one presentation-neutral coordinator with focus restoration and required-mod blocking.
- **Streaming responses** rendered word-by-word in the visible transcript with smooth per-chunk timing.
- **Emote rendering** — `*nods thoughtfully*` italicized inline.
- **Message reactions** — emoji palette persisted with the save and shown beneath transcript messages.
- **Enriched chat input** — plain text, NPC addressing, slash/model/location completion, input history, multiline editing, and quick travel submit through the existing engine path.
- **Focail sidebar/mobile panel** — Irish vocabulary and NPC names accumulate with pronunciation hints as you encounter them.
- **Durable assigned work** — concrete NPC jobs enter an authoritative task ledger; matching physical actions advance them from assigned to in progress, publish semantic events, survive save/load and journal recovery, and appear independently of the input draft. Gated by the default-on `player-task-progression` flag.

## Persistence & branching

- **Crash-safe SQLite** in write-ahead-log mode — three-table schema (`branches`, `snapshots`, `journal_events`); readers never block writers, so autosave can fire mid-conversation without hitching.
- **Git-style branching** — `/fork <name>` creates a non-destructive branch from the current state; `/load` switches; `/branches` lists.
- **Autosave** every 45 s (configurable) plus manual `/save` and graceful-shutdown autosave on `/quit`.
- **Append-only journal** of game events alongside snapshots, enabling deterministic replay from any snapshot + subsequent events.
- **Cross-process save lock** prevents two instances from corrupting the same save.
- **Save picker** in both GUI (DAG visualization of branches) and headless modes.

## Desktop GUI (Tauri 2 + Svelte 5)

- **Chat-first Svelte play surface** — the default desktop and mobile viewport uses semantic DOM controls and a readable transcript.
- **Responsive composition** — desktop keeps map and nearby context beside chat; mobile keeps chat and input primary with explicit Map and People & Words controls.
- **MapLibre GL parish overlay** with historic 1840s OS Ireland tiles or modern OSM, custom SVG icons per location type, traversal-weighted edges, and click-to-travel, opened from the Map card or `M`.
- **Animated travel** — when the player moves between locations the map smoothly pans and zooms to the destination, interpolating both center and zoom level across the journey's duration so the post-travel view is already framed when the player arrives.
- **Status and context chrome** — location, time, weather, season, festival, and pause state remain legible while secondary tools stay out of the primary conversation flow.
- **Three themes** selectable with `/theme` — default cream/parchment, Solarized Light, Solarized Dark — driven by CSS custom properties and persisted in `localStorage` so reloads don't flash the wrong palette.
- **Coordinated Debug records** (F12) — eight tabs (Overview, NPCs, World, Weather, Gossip, Conversations, Events, Inference) in one modal surface.
- **Bug reporter** — opened from Developer tools or a 🐛 next to a debug record; it captures the visible DOM game state, recent logs, and current game state and files a GitHub issue on the configured repo (`dmooney/rundale` by default), embedding the screenshot inline. Per-record buttons attach the exact inference call / event / conversation as context. Every report also carries a "black box" diagnostic payload — the raw LLM prompt/response history, the canonical `get_engine_state` snapshot, and the last raw user intent — so local-inference drift is reproducible. Also available to auto-QA agents via the `limerick_file_bug` MCP tool. Gated by the default-on `bug-report` flag; configured via `LIMERICK_BUG_REPORT_TOKEN` / `LIMERICK_BUG_REPORT_REPO`, with `LIMERICK_BUG_REPORT_DRY_RUN=1` writing the report to disk instead of filing.
- **MCP automated-QA loop** — the `limerick_engine_state` MCP tool exposes the canonical, deterministic engine state (active scene, clock, weather, player, NPCs, gossip grapevine) so an agent can assert the UI resolved each state transition. The `limerick/scripts/limerick-mcp-audit.sh` lifecycle script wraps a strict Init → Execute → Validate (UI vs `get_engine_state`) → Teardown (file a bug on mismatch, kill the backend cleanly) loop. Gated by the default-on `engine-state` flag.
- **Save picker** (F5) with a DAG visualization of branches and inline fork form.
- **Keyboard shortcuts** — F2 screenshot, F5 Ledger, F10 demo, F11 fullscreen, F12 Debug, M map, `?` help, Tab through semantic controls, Enter activate/send, and Esc close a dismissible surface or stop the demo.
- **Limerick Designer** — integrated GUI editor at `/editor` for authoring NPCs, locations, schedules, and mod data without touching JSON directly; see the [Limerick Designer](#limerick-designer-gui-editor) section below.
- **Accessibility** — ARIA-labelled controls, visible focus rings, semantic HTML, WCAG-AA contrast across all theme variants.

## Web server

- **Axum backend** in `crates/limerick-server` serves the same Svelte UI over HTTP + WebSocket, one isolated session per `limerick_sid` cookie.
- **Auth** — Cloudflare Access JWT validation in production, optional Google OAuth, loopback bypass for local dev, fail-closed when misconfigured.
- **WebSocket events** for world updates, streaming tokens, theme changes, and map source switches.
- **Per-session save isolation** — game state lives under `<user-data>/saves/<session_id>/` and survives restarts. The user-data root is platform-native (`~/Library/Application Support/Rundale` on macOS, `$XDG_DATA_HOME/rundale` on Linux, `%APPDATA%\Rundale` on Windows) and named after the active mod's `save_root`. Override with `LIMERICK_SAVES_DIR` (saves), `LIMERICK_TILE_CACHE_DIR` (tile cache), or `LIMERICK_USER_DATA_DIR` (root).
- **Prometheus-style `/metrics`** for auth failures, session counts, and inference call stats.
- **Deploy artifacts** — multi-stage `Dockerfile` in `deploy/`.

## Headless / CLI

- **`limerick-engine`** — single-process binary with two modes: `--headless` (stdin/stdout REPL), `--script FILE` (deterministic batch driver), no flag (Tauri-launch). HTTP serving is no longer muxed in — `limerick-server` is now a runnable binary in its own right.
- **Plain stdin/stdout REPL** for scripting, fixtures, and headless servers.
- **Interactive save picker** with the same branch model as the GUI.
- **ANSI-coloured output** matching the GUI palette (NPC names, system messages, errors).
- **`--script <file>`** mode for deterministic JSON-in/JSON-out execution — the backbone of the test harness.
- **The full slash-command surface** works identically to the GUI.

## Thin HTTP client (`limerick-client`)

- **Separate `limerick` binary** that talks to a running `limerick-server` over HTTP — no engine in-process, no game state owned locally.
- **Four modes:** `limerick "<cmd>"` single-shot, `limerick --script <file>` for batch fixtures, `limerick` no-arg REPL, `limerick --json "<cmd>"` for raw `CommandResponse` JSON suitable for piping into `jq` / automation.
- **Cookie persistence** — the server's `limerick_sid` cookie is saved between runs so subsequent invocations resume the same save branch.
- **Use cases:** CI scripts, agent harnesses, lightweight terminal play against a remote or local server, anything that doesn't want to boot the full engine just to issue a command.

## Modding & content

- **`mod.toml` manifest** declares world, NPCs, prompts, anachronisms, festivals, encounters, transport, pronunciations, UI overrides, and loading-screen text.
- **`world.json`** — locations with id, description templates, lat/lon, indoor/public flags, edge connections, mythological significance, and a `geo_kind` (real / manual / fictional).
- **`npcs.json`** — full NPC schema with personality, six-axis intelligence, home/workplace, mood, and per-season hourly schedules.
- **Editable prompt templates** — separate Tier 1 system, Tier 1 context, and Tier 2 system files plus a configurable historical-period preamble.
- **Anachronism registry** — JSON file of dated terms; modders can extend it for other periods.
- **Festivals, encounters, transport speeds, and Irish-word pronunciations** are all data-driven.
- **Backend-agnostic loading** — the same mod loads identically in Tauri, the web server, and the test harness.

## Limerick Designer (GUI editor)

A GUI editor embedded in the SvelteKit UI at the `/editor` route, accessible from both the Tauri desktop app and the web server (`LIMERICK_ENABLE_EDITOR=1`). Follows the mode-parity rule — every editor command is implemented once in `limerick-core` and wired to both backends.

- **Mod browser** — lists all mods under `mods/`, switch between them without restarting.
- **NPC editor** — edit identity, six-axis intelligence (tunable via sliders), home/workplace (location picker, no id-memorizing), knowledge items, gossip seeds, and relationships with automatic bidirectional bookkeeping.
- **Schedule timeline** — read-only 24-hour SVG band per season/day-type showing when each NPC is where.
- **Location editor** — description templates with live placeholder preview (`{time}`, `{weather}`, `{npcs_present}`), lat/lon, indoor/public flags, and connection editing with enforced bidirectional edges.
- **Cross-reference validator** — runs `WorldGraph::validate()` plus orphan NPC homes/workplaces, broken relationship targets, and schedule location refs; click any issue to jump to the field.
- **Save inspector** — browse `.db` save files, branches, and snapshots; view deserialized world state (clock, weather, NPCs, gossip network, conversation log); export a snapshot as a fixture JSON.
- **Deterministic JSON writer** — stable key ordering and 2-space indentation on every save so `git diff` stays clean even after a no-op round-trip.
- **Running-game isolation** — the editor operates on a fresh in-memory copy of mod files and never touches the live game session; a warning banner appears when the loaded mod matches the one being edited.

## Developer & modder tooling

- **`limerick-geo-tool`** — Overpass-API CLI that pulls real Irish features into `world.json` by named area or bounding box, with cached responses, dry-run preview, hand-curated merge mode, and a `realign-coords` utility for snapping to historical map coordinates.
- **`limerick-npc-tool`** — SQLite-backed NPC builder: bulk-generate parish or county populations with seedable randomness and 1820s demographic weights, query/filter by parish/occupation/tier, edit moods, promote tiers, batch-elaborate backstories with an LLM, validate referential integrity, and export/import JSON. Also splits the monolithic `mods/rundale/npcs.json` catalogue into per-NPC source files (`split-catalog`) and re-joins them into a byte-identical canonical file (`join-catalog`), with a standalone `validate-catalog` integrity pass.
- **`limerick-harness`** — headless game quality-control harness: runs automated multi-turn playtests where an LLM plays the player and an LLM judges the finished transcript, against `limerick-server` over HTTP. Each run captures canonical engine state and a rendered telemetry "state-frame" per turn—not a player-visible UI screenshot; evaluates deterministic hard-fail **gates** (crash / parser-reject / timeout / empty-turn-burn); scores ~7 quality **axes** (0–100) when gates pass; records findings; and persists everything to SQLite plus on-disk artifacts. The Tauri bridge does not expose the `/api/command` endpoint this client uses, so desktop/UI quality is covered separately by the live MCP quality harness and Playwright lanes. Run knobs (engine models per category, feature flags, player persona, judge rubric pinned by sha256) are content-addressed for exact A/B comparison and correlated with git history. The player/judge seam runs either deterministic scripted actors (CI, no key) or `limerick-inference`-backed LLMs (Anthropic / OpenAI-compat / local vllm-mlx). Drive with `cargo run -p limerick-harness -- run --config <cfg> --turns N` against a running server. For a **fully headless real-model game** to drive, boot the web server with `limerick-server --headless-models` (or `LIMERICK_HEADLESS_MODELS=1`): it detect-reuses (or spawns) the bundled vllm-mlx Qwen two-slot loadout and binds the four inference categories to it, so `POST /api/command` produces genuine NPC dialogue. The harness applies per-run **BYOK** model overrides (`engine_models.<category>`) through runtime slash commands over `/api/command`, resolving provider keys from the harness environment at apply-time (never persisted into the content-addressed run config). For unattended CI/cron runs, `limerick-harness run --player api --judge api` is driven solely by env API keys—no Claude Code session, MCP, or subagent queue—and `--player`/`--judge` select each actor's driver independently.
- **`limerick-scenario`** — versioned YAML regression runner for agents and CI. Every step drives the shipping `limerick_core::game_loop`, mocks only inference, and evaluates explicit assertions over emitted IPC events and post-step state. Run all scenarios with `just scenario-test` or print one JSON report with `just scenario-run <file>`.
- **Legacy script harness** — `test_*.txt` fixtures in `testing/fixtures/` retain compatibility coverage through structured `ScriptResult` output. One-off demonstrations are separated under `testing/proofs/` and are not counted as regression tests merely because they execute without crashing.
- **Eval rubrics & baselines** — snapshot `Vec<ScriptResult>` JSONs in `testing/evals/baselines/`, with structural rubrics that gate against empty look descriptions, frozen clocks, and anachronistic vocabulary.
- **Architecture fitness tests** — `crates/limerick-core/tests/architecture_fitness.rs` mechanically enforces leaf-crate purity (no `tauri`/`axum`/`tower` in shared logic), CLI-vs-leaf duplication bans, and orphaned-module detection. Each failure prints a self-correcting hint.
- **`justfile`** with ~50 recipes grouping build, test, harness, lint, screenshots, deps, geo/NPC tooling, Ollama control, and local CI via `act`.
- **Witness-marker scan** — `just witness-scan` rejects AI completion stubs (the usual `todo!` and ellipsis-comment patterns) in changed files.
- **Doc-path validator** — `just check-doc-paths` ensures every backtick-cited file path in `docs/` actually exists.
- **Frontend test stack** — Vitest unit tests, Playwright E2E with mocked Tauri IPC, screenshot baselines (`just screenshots`).

## Documentation

- **`docs/index.md`** is the master hub — phase status, design overview, ADR index, plans, research, and agent guides.
- **Architectural Decision Records** record the rationale behind graph-based worlds, cognitive LOD, SQLite write-ahead-log persistence, git-like branching, JSON-structured LLM output, real geography, per-category inference, and the geo-tool OSM pipeline.
- **Historical research archive** — religion, family, education, crafts, food, transportation, and Hiberno-English dialect notes informing NPC dialogue.
- **`docs/agent/`** — slim, indexed reference for AI coding agents (build, architecture, style, gotchas, harness, skills, git workflow), linked from `AGENTS.md`.
