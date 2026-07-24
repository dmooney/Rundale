# parish-npc-tool — agent scope

SQLite-backed NPC world builder and inspection utility for Parish/Rundale (#433). Dev-time binary only — generates and inspects NPC populations at design time ahead of shipping a mod. Not part of the runtime game loop or mode parity. See root [`AGENTS.md`](../../../AGENTS.md).

## Scoped commands

```sh
cargo run  -p parish-npc-tool -- generate-world --counties roscommon,galway
cargo run  -p parish-npc-tool -- generate-parish Kiltoom --pop 2000
cargo run  -p parish-npc-tool -- validate --all
cargo run  -p parish-npc-tool -- export --parish Kiltoom
cargo run --manifest-path parish/Cargo.toml -p parish-npc-tool -- art-inputs --npcs mods/rundale/npcs.json --world mods/rundale/world.json --art-direction parish/apps/ui/art/notebook-person-art/npc-art-direction-v1.json --output parish/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json
cargo test -p parish-npc-tool                              # unit
```

## Local gotchas

- **Dev-time only — no mode parity (rule #2).** Not wired into `parish-engine`, `parish-server`, or `parish-tauri`.
- **Binary-only crate.** All logic in `src/main.rs`; no library surface. Consume output JSON or invoke as a subprocess.
- **Depends on `parish-npc` for typed NPC schema** (`NpcFile`/`NpcFileEntry`) — gives deterministic field ordering that `serde_json::Value` cannot (TD-001). Does not depend on `parish-core`; `rusqlite` and generation deps stay out of the engine.
- **Owns its own SQLite schema.** Parish/household/NPC tables (#434) diverge from `parish-persistence`'s branch-keyed save format; migrations are independent.
- **Output requires human validation.** Generated NPC JSON is authoritative only after author review and `validate` pass; commit into the mod's `npcs.json` (or future `parish-world.db`) after that.
- **`elaborate` subcommand reaches out to an LLM at invocation time.** The crate itself does not depend on `parish-inference`.

## Module map

`main.rs` — all logic (single binary, clap-driven subcommands).
