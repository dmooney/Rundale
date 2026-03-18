# Parish

An Irish Living World Text Adventure built in Rust.

The player explores a small parish near Roscommon, County Roscommon, interacting with NPCs driven by local LLM inference (Ollama). A cognitive level-of-detail system simulates hundreds of NPCs at varying fidelity based on distance from the player.

See [DESIGN.md](DESIGN.md) for the full design document.

## Requirements

- Rust (edition 2024)
- [Ollama](https://ollama.ai/) running on `localhost:11434`

## Quick Start

```sh
cargo build
cargo run
```
