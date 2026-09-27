# limerick-npc-tool — agent scope

SQLite-backed NPC world builder and inspection utility for Limerick/Rundale (#433). Dev-time binary only — generates and inspects NPC populations at design time ahead of shipping a mod. Not part of the runtime game loop or mode parity. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo run  -p limerick-npc-tool -- generate-world --counties roscommon,galway
cargo run  -p limerick-npc-tool -- generate-parish Kiltoom --pop 2000
cargo run  -p limerick-npc-tool -- validate --all
cargo run  -p limerick-npc-tool -- export --parish Kiltoom
cargo run --manifest-path limerick/Cargo.toml -p limerick-npc-tool -- art-inputs --npcs mods/rundale/npcs.json --world mods/rundale/world.json --art-direction limerick/apps/ui/art/notebook-person-art/npc-art-direction-v1.json --output limerick/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json
cargo test -p limerick-npc-tool                              # unit
```

## Local gotchas

- **Dev-time only — no mode parity (rule #2).** Not wired into `limerick-engine`, `limerick-server`, or `limerick-tauri`.
- **Binary-only crate.** All logic in `src/main.rs`; no library surface. Consume output JSON or invoke as a subprocess.
- **Depends on `limerick-npc` for typed NPC schema** (`NpcFile`/`NpcFileEntry`) — gives deterministic field ordering that `serde_json::Value` cannot (TD-001). Does not depend on `limerick-core`; `rusqlite` and generation deps stay out of the engine.
- **Owns its own SQLite schema.** Parish/household/NPC tables (#434) diverge from `limerick-persistence`'s branch-keyed save format; migrations are independent.
- **Output requires human validation.** Generated NPC JSON is authoritative only after author review and `validate` pass; commit into the mod's `npcs.json` (or future `limerick-world.db`) after that.
- **`elaborate` subcommand reaches out to an LLM at invocation time.** The crate itself does not depend on `limerick-inference`.

## Module map

`main.rs` — all logic (single binary, clap-driven subcommands).
