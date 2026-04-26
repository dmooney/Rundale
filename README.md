# Rundale

An Irish Living World Text Adventure built in Rust, set in 1820. Powered by the **Parish** engine.

The player arrives as a newcomer to **Kilteevan Village** in the parish of Kiltoom, near Roscommon, County Roscommon. NPCs are driven by local LLM inference (via any OpenAI-compatible provider — Ollama, LM Studio, OpenRouter, or custom). A cognitive level-of-detail (LOD) system simulates hundreds of NPCs at varying fidelity based on proximity to the player.

> Any resemblance to real persons, living or dead, or actual businesses is purely coincidental. All characters and commercial establishments in this game are fictional.

![Rundale GUI — Morning](docs/screenshots/gui-morning.png)

*Tauri GUI showing the chat panel, interactive map, NPC sidebar, and time-of-day color theming (morning palette).*

## Current Status

**Phases 1–4 complete** (Core Loop, World Graph, NPCs & Simulation, Persistence). **Phase 5 — Full LOD & Scale** is in progress: sub-phases 5A–5E are done (event bus, weather state machine, long-term memory & gossip, Tier 3 batch inference, Tier 4 rules engine & seasonal effects), and 5F (world graph expansion to Roscommon/Athlone/Dublin) is the next open item. **Phase 8 — Tauri GUI** is landed (one screenshot-capture polish item outstanding).

See the [Roadmap](docs/requirements/roadmap.md) for per-item status tracking.

## Quick Start

The workspace ships with a [`justfile`](justfile); run `just --list` for the full set of recipes.

**Requirements:** Rust (edition 2024), [Node.js](https://nodejs.org/) (v20+), [`just`](https://github.com/casey/just) (`cargo install just` or your package manager's equivalent), and an OpenAI-compatible LLM endpoint (e.g. [Ollama](https://ollama.ai/) on `localhost:11434`, LM Studio, OpenRouter, or a custom provider configured in `parish.toml`).

```sh
# One-time: install system deps, Rust, Node, and frontend packages
just setup
```

### GUI Mode (Tauri Desktop App)

The default experience is a Tauri 2 desktop app with a Svelte 5 frontend.

```sh
just run          # launches cargo tauri dev
```

### Headless Mode (Terminal REPL)

Plain stdin/stdout REPL — useful for scripting, fixtures, and servers without a display:

```sh
just run-headless
```

On startup, a save picker shows existing save files (in `saves/`) with their timeline branches, or lets you start a new game. In-game, use `/load` to switch saves, `/save` to snapshot, `/fork <name>` to branch timelines.

### Web Server

An Axum backend in `crates/parish-server` serves the Svelte UI over WebSockets (see [OAuth setup](docs/oauth-setup.md) and the `deploy/` artifacts for Dockerfile + Railway config).

**Platform guides:** [macOS](docs/macos-setup.md) | [Linux](docs/linux-setup.md) | [Windows](docs/windows-setup.md)

## Documentation

The documentation is organized hierarchically — start at a summary level and drill down as needed.

```
README.md (you are here — project overview, quick start)
├── CLAUDE.md / AGENTS.md      — Slim agent indexes → docs/agent/
└── docs/index.md              — Full documentation hub (start here for everything)
    ├── docs/agent/            — Agent-facing build/test/style/gotchas docs
    ├── docs/requirements/
    │   └── roadmap.md         — Per-item status tracking across all phases
    ├── docs/design/
    │   └── overview.md        — Architecture overview → subsystem docs
    ├── docs/adr/
    │   └── README.md          — Architecture decision records with rationale
    ├── docs/plans/            — Detailed implementation plan per phase
    ├── docs/research/         — Historical research informing design
    ├── docs/archive/          — Historical / superseded docs (DESIGN.md)
    ├── docs/journal.md        — Cross-session development notes
    └── docs/known-issues.md   — Active bugs and UX issues
```

| Start here | What you'll find |
|------------|-----------------|
| [docs/index.md](docs/index.md) | **Master hub** — phase status, links to everything |
| [docs/requirements/roadmap.md](docs/requirements/roadmap.md) | Per-item checkboxes for all phases |
| [docs/design/overview.md](docs/design/overview.md) | Architecture, tech stack, module tree, LLM providers |
| [docs/adr/README.md](docs/adr/README.md) | Architecture decision records (ADRs) |

## Repository Layout

```
crates/
  parish-core/      Parish engine — pure game logic library
  parish-cli/       Parish engine — headless / web / CLI binary (`parish`)
  parish-server/    Parish engine — Axum web backend
  parish-tauri/     Parish engine — Tauri 2 desktop backend
  parish-geo-tool/  OSM extraction CLI
apps/ui/            Svelte 5 + TypeScript frontend
testing/fixtures/   scripted gameplay fixtures
mods/rundale/       Rundale game content (world, NPCs, prompts, lore)
deploy/             Dockerfile + railway.toml
docs/               design, ADRs, plans, research, agent guides
```

## For AI Agents

See [CLAUDE.md](CLAUDE.md) and [AGENTS.md](AGENTS.md) — both index into [docs/agent/](docs/agent/README.md) for build commands, architecture, code style, engineering standards, and gotchas.
