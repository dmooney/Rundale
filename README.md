# Parish

An Irish Living World Text Adventure built in Rust, set in 1820.

The player arrives as a newcomer to **Kilteevan Village** in the parish of Kiltoom, near Roscommon, County Roscommon. NPCs are driven by local LLM inference (via any OpenAI-compatible provider — Ollama, LM Studio, OpenRouter, or custom). A cognitive level-of-detail (LOD) system simulates hundreds of NPCs at varying fidelity based on proximity to the player.

> Any resemblance to real persons, living or dead, or actual businesses is purely coincidental. All characters and commercial establishments in this game are fictional.

## Current Status

**Phase 2 — World Graph** (in progress). Phase 1 (Core Loop) is complete.

See the [Roadmap](docs/requirements/roadmap.md) for per-item status tracking.

## Quick Start

```sh
cargo build
cargo run
```

**Requirements:** Rust (edition 2024), [Ollama](https://ollama.ai/) on `localhost:11434` (auto-installed if missing).

**Windows users:** See the [Windows Setup Guide](docs/windows-setup.md).

## Documentation

Start at [docs/index.md](docs/index.md) for the full documentation hub. Key entry points:

| Document | What you'll find |
|----------|-----------------|
| [Documentation Index](docs/index.md) | Master hub — start here to find anything |
| [Architecture Overview](docs/design/overview.md) | Tech stack, core loop, module tree, LLM provider support |
| [Roadmap](docs/requirements/roadmap.md) | 6-phase plan with per-item status checkboxes |
| [ADR Index](docs/adr/README.md) | 11 architecture decision records with rationale |
| [Implementation Plans](docs/plans/) | Detailed phase-by-phase implementation plans |
| [Development Journal](docs/journal.md) | Cross-session notes, observations, recommendations |

## For AI Agents

See [CLAUDE.md](CLAUDE.md) for build commands, code style, engineering standards, and gotchas. It links to deeper documentation as needed. The documentation hierarchy is:

```
README.md (you are here)
├── CLAUDE.md              — Build, test, style, standards (agent quick-ref)
└── docs/index.md          — Full documentation hub
    ├── docs/design/       — Architecture & subsystem design
    │   └── overview.md    — Start here, links to all subsystem docs
    ├── docs/adr/          — Architecture decision records
    ├── docs/requirements/ — Roadmap with status tracking
    └── docs/plans/        — Phase-by-phase implementation plans
```
