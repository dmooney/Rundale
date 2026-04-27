# Rundale

An Irish Living World Text Adventure, set in 1820 rural Ireland. Powered by the custom **Parish** engine.

The player arrives as a newcomer to Kilteevan Village in the civil parish of Kilteevan (barony of Ballintobber), about two miles south-east of Roscommon town in County Roscommon. Locally the area falls under the Catholic parish of Roscommon and Kilteevan. NPCs are driven by LLM inference. A cognitive level-of-detail (LOD) system simulates hundreds of NPCs at varying fidelity based on proximity to the player. The geography is based on real early 19th century Ireland. The characters and establishments are fictional.

## Features

### World simulation

- **Location graph** with fuzzy (Jaro-Winkler) name resolution, prose-described edges, and per-edge traversal counts that drive a "worn paths" map visualization.
- **Hybrid geography**: locations can be real-world (geocoded from OSM), author-pinned, or fully fictional, with relative anchors that let fictional clusters subordinate to a real place.
- **Game clock** with seven time-of-day phases (Midnight → Evening) and a configurable real-to-game speed factor (Slowest 80 min/day → Ludicrous ~100 sec/day) tunable at runtime via `/speed`.
- **Four seasons** with seasonal NPC schedules, weather biases, and Tier 4 life-event rates.
- **Weather state machine** — seven states (Clear → PartlyCloudy → Overcast → LightRain → HeavyRain → Storm, plus Fog), adjacent-state-only transitions, 2-hour minimum dwell, season-biased probabilities. NPCs seek shelter in heavy rain.
- **Travel & encounters** — per-edge travel time from lat/lon and transport mode (walk vs. horse/cart), with time-of-day-weighted en-route encounters.
- **Festivals** — Imbolc, Bealtaine, Lughnasa, Samhain trigger relationship boosts and narrative hooks.
- **Mythology hooks** — locations carry a `mythological_significance` field that's surfaced into NPC prompts and reserved for future folklore systems.

### NPCs — cognitive level-of-detail

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

### LLM inference

- **14 inference providers** out of the box: Ollama, LM Studio, vLLM, OpenAI, Anthropic (native `/v1/messages` API, not the OpenAI-compatibility shim), Google Gemini, OpenRouter, Groq, xAI Grok, Mistral, DeepSeek, Together AI, Custom (any OpenAI-compatible base URL), and a built-in offline Simulator that needs no model download.
- **Per-category routing** — Dialogue, Simulation, and Intent can each use a different provider/model/key, switchable at runtime via dot-notation commands (`/provider.dialogue`, `/model.intent`, `/key.simulation`).
- **Three-lane priority queue** — Interactive (player dialogue) preempts Background (Tier 2) preempts Batch (Tier 3); a slow batch call cannot block your conversation.
- **Token streaming** with bounded back-pressure (1024-token channel) so a slow consumer never OOMs the engine.
- **Structured JSON output** — NPC turns return `{mood, action, internal_thought, irish_words}`; partial JSON is recovered on truncation.
- **Reachability + timeout knobs** — request, streaming, model-load, and download timeouts all configurable per-environment.
- **Bounded inference log** — recent calls (model, latency, sizes, errors) surface in the debug panel without unbounded memory growth.
- **Five-layer prompt-injection defence** (ADR-010) — role separation, delimited input with "sandwiched" instructions, input sanitisation at the system boundary, strict output parsing/validation, and output filtering before display.

### Player experience

- **Free-text dialogue** parsed by an LLM intent extractor (Move / Talk / Look / Examine / Interact), with a regex fallback.
- **`@mention` targeting** to address a specific NPC in a crowded room.
- **Slash-command surface** spanning save management, time control, provider config, debug, theming, and map switching — the same set works in the GUI, web, and CLI.
- **Streaming responses** rendered word-by-word with smooth per-chunk timing.
- **Emote rendering** — `*nods thoughtfully*` italicized inline.
- **Message reactions** — emoji palette persisted with the save.
- **Mention + slash autocomplete**, tab completion for known nouns, and a 50-entry input history.
- **Quick-travel chips** for adjacent locations rendered below the input.
- **Pronunciation sidebar** — Irish vocabulary and NPC names accumulate with IPA hints as you encounter them.

### Persistence & branching

- **Crash-safe SQLite** in write-ahead-log mode — three-table schema (`branches`, `snapshots`, `journal_events`); readers never block writers, so autosave can fire mid-conversation without hitching.
- **Git-style branching** — `/fork <name>` creates a non-destructive branch from the current state; `/load` switches; `/branches` lists.
- **Autosave** every 45 s (configurable) plus manual `/save` and graceful-shutdown autosave on `/quit`.
- **Append-only journal** of game events alongside snapshots, enabling deterministic replay from any snapshot + subsequent events.
- **Cross-process save lock** prevents two instances from corrupting the same save.
- **Save picker on startup** in both GUI (DAG visualization of branches) and headless modes.

### Desktop GUI (Tauri 2 + Svelte 5)

- **Three-panel layout** — interactive map, scrollable chat with streaming responses, NPC/language sidebar — collapsing to a single tabbed column under 768 px.
- **MapLibre GL minimap + full-screen overlay** with historic 1840s OS Ireland tiles or modern OSM, custom SVG icons per location type, traversal-weighted edges, and click-to-travel.
- **Animated travel** — when the player moves between locations the map smoothly pans and zooms to the destination, interpolating both center and zoom level across the journey's duration so the post-travel view is already framed when the player arrives.
- **Status bar** — location, time-of-day label, weather, season, festival indicator, pause indicator, digital clock animated client-side.
- **Three themes** selectable with `/theme` — default cream/parchment, Solarized Light, Solarized Dark — driven by CSS custom properties and persisted in `localStorage` so reloads don't flash the wrong palette.
- **Debug panel** (F12) — eight tabs (Overview, NPCs, World, Weather, Gossip, Conversations, Events, Inference) dockable to the side or bottom.
- **Save picker** (F5) with a DAG visualization of branches and inline fork form.
- **Keyboard shortcuts** — F5 saves, F12 debug, M map, Up/Down history, Tab autocomplete, Esc cancels travel.
- **Parish Designer** — integrated GUI editor at `/editor` for authoring NPCs, locations, schedules, and mod data without touching JSON directly; see the [Parish Designer](#parish-designer-gui-editor) section below.
- **Accessibility** — ARIA-labelled controls, visible focus rings, semantic HTML, WCAG-AA contrast across all theme variants.

### Web server

- **Axum backend** in `crates/parish-server` serves the same Svelte UI over HTTP + WebSocket, one isolated session per `parish_sid` cookie.
- **Auth** — Cloudflare Access JWT validation in production, optional Google OAuth, loopback bypass for local dev, fail-closed when misconfigured.
- **WebSocket events** for world updates, streaming tokens, theme changes, and map source switches.
- **Per-session save isolation** — game state lives under `saves/<session_id>/` and survives restarts.
- **Prometheus-style `/metrics`** for auth failures, session counts, and inference call stats.
- **Deploy artifacts** — multi-stage `Dockerfile` and Railway watchdog script in `deploy/`.

### Headless / CLI

- **Plain stdin/stdout REPL** for scripting, fixtures, and headless servers.
- **Interactive save picker** on startup with the same branch model as the GUI.
- **ANSI-coloured output** matching the GUI palette (NPC names, system messages, errors).
- **`--script <file>`** mode for deterministic JSON-in/JSON-out execution — the backbone of the test harness.
- **The full slash-command surface** works identically to the GUI.

### Modding & content

- **`mod.toml` manifest** declares world, NPCs, prompts, anachronisms, festivals, encounters, transport, pronunciations, UI overrides, and loading-screen text.
- **`world.json`** — locations with id, description templates, lat/lon, indoor/public flags, edge connections, mythological significance, and a `geo_kind` (real / manual / fictional).
- **`npcs.json`** — full NPC schema with personality, six-axis intelligence, home/workplace, mood, and per-season hourly schedules.
- **Editable prompt templates** — separate Tier 1 system, Tier 1 context, and Tier 2 system files plus a configurable historical-period preamble.
- **Anachronism registry** — JSON file of dated terms; modders can extend it for other periods.
- **Festivals, encounters, transport speeds, and Irish-word pronunciations** are all data-driven.
- **Backend-agnostic loading** — the same mod loads identically in Tauri, the web server, and the test harness.

### Parish Designer (GUI editor)

A GUI editor embedded in the SvelteKit UI at the `/editor` route, accessible from both the Tauri desktop app and the web server (`PARISH_ENABLE_EDITOR=1`). Follows the mode-parity rule — every editor command is implemented once in `parish-core` and wired to both backends.

- **Mod browser** — lists all mods under `mods/`, switch between them without restarting.
- **NPC editor** — edit identity, six-axis intelligence (tunable via sliders), home/workplace (location picker, no id-memorizing), knowledge items, gossip seeds, and relationships with automatic bidirectional bookkeeping.
- **Schedule timeline** — read-only 24-hour SVG band per season/day-type showing when each NPC is where.
- **Location editor** — description templates with live placeholder preview (`{time}`, `{weather}`, `{npcs_present}`), lat/lon, indoor/public flags, and connection editing with enforced bidirectional edges.
- **Cross-reference validator** — runs `WorldGraph::validate()` plus orphan NPC homes/workplaces, broken relationship targets, and schedule location refs; click any issue to jump to the field.
- **Save inspector** — browse `.db` save files, branches, and snapshots; view deserialized world state (clock, weather, NPCs, gossip network, conversation log); export a snapshot as a fixture JSON.
- **Deterministic JSON writer** — stable key ordering and 2-space indentation on every save so `git diff` stays clean even after a no-op round-trip.
- **Running-game isolation** — the editor operates on a fresh in-memory copy of mod files and never touches the live game session; a warning banner appears when the loaded mod matches the one being edited.

### Developer & modder tooling

- **`parish-geo-tool`** — Overpass-API CLI that pulls real Irish features into `world.json` by named area or bounding box, with cached responses, dry-run preview, hand-curated merge mode, and a `realign-coords` utility for snapping to historical map coordinates.
- **`parish-npc-tool`** — SQLite-backed NPC builder: bulk-generate parish or county populations with seedable randomness and 1820s demographic weights, query/filter by parish/occupation/tier, edit moods, promote tiers, batch-elaborate backstories with an LLM, validate referential integrity, and export/import JSON.
- **Script harness** — `.txt` fixtures in `testing/fixtures/` drive the engine through scripted scenes; structured `ScriptResult` JSON output enables deterministic regression checking. Run a single fixture (`just game-test-one <name>`), all of them (`just game-test-all`), or list available scripts (`just game-test-list`).
- **Eval rubrics & baselines** — snapshot `Vec<ScriptResult>` JSONs in `testing/evals/baselines/`, with structural rubrics that gate against empty look descriptions, frozen clocks, and anachronistic vocabulary.
- **Architecture fitness tests** — `crates/parish-core/tests/architecture_fitness.rs` mechanically enforces leaf-crate purity (no `tauri`/`axum`/`tower` in shared logic), CLI-vs-leaf duplication bans, and orphaned-module detection. Each failure prints a self-correcting hint.
- **`justfile`** with ~50 recipes grouping build, test, harness, lint, screenshots, deps, geo/NPC tooling, Ollama control, and local CI via `act`.
- **Witness-marker scan** — `just witness-scan` rejects AI completion stubs (`todo!()`, `// ...`) in changed files.
- **Doc-path validator** — `just check-doc-paths` ensures every backtick-cited file path in `docs/` actually exists.
- **Frontend test stack** — Vitest unit tests, Playwright E2E with mocked Tauri IPC, screenshot baselines (`just screenshots`).

### Documentation

- **`docs/index.md`** is the master hub — phase status, design overview, ADR index, plans, research, and agent guides.
- **24 ADRs** record the rationale behind graph-based worlds, cognitive LOD, SQLite write-ahead-log persistence, git-like branching, JSON-structured LLM output, real geography, per-category inference, and the geo-tool OSM pipeline.
- **Historical research archive** — religion, family, education, crafts, food, transportation, and Hiberno-English dialect notes informing NPC dialogue.
- **`docs/agent/`** — slim, indexed reference for AI coding agents (build, architecture, style, gotchas, harness, skills, git workflow), symlinked from `CLAUDE.md` and `AGENTS.md`.

## AI disclosure

Rundale/Parish is an experiment in building a world too detailed and too improvisational to author by hand. The premise is that AI can simulate a parish of hundreds of NPCs at varying fidelity, generate their dialogue and reactions on the fly, and remain coherent over long play sessions — and that the only way to find out is to build it.

To that end, the project is developed entirely by AI coding agents — mostly **Claude Code**, with **Codex** and **Gemini** on specific tasks. Quality control is a combination of agents reviewing each other's work and extensive automated checks — the architecture-fitness tests, gameplay harness, eval rubrics, and snapshot baselines described above are designed to keep AI-written code honest. Human play-testing is the final gate.

Static game content in `mods/rundale/` — NPC personalities, schedules, relationships; location descriptions, lore, pronunciations — is also AI-generated, but human-reviewed before it lands.

Character dialogue, mood, and behaviour are generated **in real time** by whichever LLM provider you've configured. Every NPC line, gossip rumour, and Tier 2/3 simulation tick comes from a live model call at play time; nothing is pre-baked. Each playthrough is genuinely different, and the dialogue's quality depends on the model you point the engine at.

## Quick Start

The workspace ships with a [`justfile`](justfile); run `just --list` for the full set of recipes.

**Requirements:** Rust (edition 2024), [Node.js](https://nodejs.org/) (v20+), [`just`](https://github.com/casey/just) (`cargo install just` or your package manager's equivalent), and an OpenAI-compatible LLM endpoint (e.g. [Ollama](https://ollama.ai/) on `localhost:11434`, LM Studio, OpenRouter, or a custom provider configured in `parish.toml`). There's no packaged release yet.

```sh
# One-time: install system deps, Rust, Node, and frontend packages
just setup
```

### GUI Mode (Tauri Desktop App)

The default experience is a Tauri 2 desktop app with a Svelte 5 frontend.

```sh
just run          # launches cargo tauri dev
```

## Repository Layout

```
crates/
  parish-types/        foundational shared types (zero internal deps)
  parish-config/       engine + LLM-provider config loader
  parish-palette/      backend-agnostic time/season/weather color interpolation
  parish-persistence/  SQLite save/load with WAL journal and branching saves
  parish-input/        player input parsing and command interpretation
  parish-inference/    LLM inference queue and provider clients
  parish-world/        world graph, movement, weather, environment state
  parish-npc/          NPC simulation, memory, schedules, reactions
  parish-core/         orchestration: game session, IPC, mod loading, prompts
  parish-cli/          CLI / headless / web binary (`parish`)
  parish-server/       Axum web backend (serves Svelte UI)
  parish-tauri/        Tauri 2 desktop backend bridge
  parish-geo-tool/     OSM extraction CLI for world authoring
  parish-npc-tool/     NPC world builder and inspection utility
apps/ui/               Svelte 5 + TypeScript frontend
testing/fixtures/      scripted gameplay fixtures
mods/rundale/          Rundale game content (world, NPCs, prompts, lore)
deploy/                Dockerfile + railway.toml
docs/                  design, ADRs, plans, research, agent guides
```

## Documentation

| Start here | What you'll find |
|------------|------------------|
| [docs/index.md](docs/index.md) | Master hub — phase status, links to everything |
| [docs/design/overview.md](docs/design/overview.md) | Architecture, tech stack, module tree, LLM providers |
| [docs/requirements/roadmap.md](docs/requirements/roadmap.md) | Per-item status tracking across all phases |
| [docs/adr/README.md](docs/adr/README.md) | Architecture decision records and rationale |
| [docs/known-issues.md](docs/known-issues.md) | Active bugs and UX issues |
| [AGENTS.md](AGENTS.md) / [CLAUDE.md](CLAUDE.md) | Agent guides — index into [docs/agent/](docs/agent/README.md) for build, style, and gotchas |

## Licence

Rundale on the Parish engine is © 2026 Dave Mooney and is licensed under the
[GNU General Public License v3.0](LICENSE) (`GPL-3.0-only`). Source code is
free to use, modify, and redistribute under the terms of that licence.

"Rundale" and "Parish" are unregistered trademarks of Dave Mooney. The
GPL covers source reuse but not the project names or logos: forks must
rename. (A formal trademark policy lives at `TRADEMARK.md` once published.)

## Credits

Parish is built on a stack of excellent open-source projects, including
[Rust](https://www.rust-lang.org/), [Tokio](https://tokio.rs/),
[Axum](https://github.com/tokio-rs/axum), [Tauri](https://tauri.app/),
[Svelte](https://svelte.dev/) / [SvelteKit](https://kit.svelte.dev/),
[MapLibre GL JS](https://maplibre.org/), [SQLite](https://www.sqlite.org/),
and [Phosphor Icons](https://phosphoricons.com/). Full attribution with
licence texts is in [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md); run
`just notices` to regenerate the exhaustive transitive list.

Map data © [OpenStreetMap](https://www.openstreetmap.org/copyright)
contributors, licensed under the
[Open Database Licence 1.0](https://opendatacommons.org/licenses/odbl/1-0/).
Historic 6″ Ordnance Survey Ireland tiles (1829–1842) courtesy of the
[National Library of Scotland](https://maps.nls.uk/), licensed under
[CC-BY-SA 3.0](https://creativecommons.org/licenses/by-sa/3.0/). UI glyphs
use [Noto Sans Symbols 2](https://github.com/notofonts/symbols2) under the
[SIL Open Font License 1.1](assets/fonts/NotoSansSymbols2-LICENSE.txt).
