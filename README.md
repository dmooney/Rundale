# Parish

An Irish Living World Text Adventure built in Rust.

The player explores a small parish near Roscommon, County Roscommon, interacting with NPCs driven by local LLM inference (Ollama). A cognitive level-of-detail system simulates hundreds of NPCs at varying fidelity based on distance from the player.

## Current Status

**Phase 1 — Core Loop.** See the [roadmap](docs/requirements/roadmap.md) for detailed progress.

## Documentation

- [Documentation Index](docs/index.md) — hub for all project docs
- [Architecture Overview](docs/design/overview.md) — tech stack, core loop, module tree
- [Roadmap](docs/requirements/roadmap.md) — 6 phases with status tracking
- [ADR Index](docs/adr/README.md) — architecture decision records
- [Implementation Plans](docs/plans/) — detailed plans for each phase
- [DESIGN.md](DESIGN.md) — original monolithic design document (archival reference)

> Any resemblance to real persons, living or dead, or actual businesses is purely coincidental. All characters and commercial establishments in this game are fictional.

## Requirements

- Rust (edition 2024)
- [Ollama](https://ollama.ai/) running on `localhost:11434`

## Quick Start

```sh
cargo build
cargo run
```
