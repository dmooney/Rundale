# Parish

An Irish Living World Text Adventure built in Rust, set in 1820.

The player arrives as a newcomer to **Kilteevan Village** in the parish of Kiltoom, near Roscommon, County Roscommon. NPCs are driven by local LLM inference (via any OpenAI-compatible provider — Ollama, LM Studio, OpenRouter, or custom). A cognitive level-of-detail (LOD) system simulates hundreds of NPCs at varying fidelity based on proximity to the player.

> Any resemblance to real persons, living or dead, or actual businesses is purely coincidental. All characters and commercial establishments in this game are fictional.

![Parish GUI — Morning](docs/screenshots/gui-morning.png)

*GUI mode showing the chat panel, interactive map, Irish word sidebar, and time-of-day color theming (morning palette). GUI is the default mode — just run `cargo run`.*

## Current Status

**Phases 1–3 complete** (Core Loop, World Graph, NPCs & Simulation). **Phase 4 — Persistence** is next.

See the [Roadmap](docs/requirements/roadmap.md) for per-item status tracking.

## Quick Start

```sh
cargo build
cargo run
```

**Requirements:** Rust (edition 2024), [Ollama](https://ollama.ai/) on `localhost:11434` (auto-installed if missing).

**Windows users:** See the [Windows Setup Guide](docs/windows-setup.md).

## Documentation

The documentation is organized hierarchically — start at a summary level and drill down as needed.

```
README.md (you are here — project overview, quick start)
├── CLAUDE.md                  — Agent quick-ref: build, test, style, standards
└── docs/index.md              — Full documentation hub (start here for everything)
    ├── docs/requirements/
    │   └── roadmap.md         — Per-item status tracking across all 6 phases
    ├── docs/design/
    │   └── overview.md        — Architecture overview → links to 14 subsystem docs
    ├── docs/adr/
    │   └── README.md          — 12 architecture decision records with rationale
    ├── docs/plans/            — Detailed implementation plan per phase
    ├── docs/research/         — Historical research informing design
    ├── docs/journal.md        — Cross-session development notes
    └── docs/known-issues.md   — Active bugs and UX issues
```

| Start here | What you'll find |
|------------|-----------------|
| [docs/index.md](docs/index.md) | **Master hub** — phase status, links to everything |
| [docs/requirements/roadmap.md](docs/requirements/roadmap.md) | Per-item checkboxes for all 6 phases |
| [docs/design/overview.md](docs/design/overview.md) | Architecture, tech stack, module tree, LLM providers |
| [docs/adr/README.md](docs/adr/README.md) | Architecture decision records (ADRs) |

## For AI Agents

See [CLAUDE.md](CLAUDE.md) for build commands, code style, engineering standards, and gotchas. For deeper context, follow links to [docs/index.md](docs/index.md) and the specific subsystem design docs.
