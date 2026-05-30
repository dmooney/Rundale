# parish-npc-tool — agent scope

SQLite-backed NPC world builder and inspection utility for Parish/Rundale (#433). Standalone build-time binary — generates and inspects NPC populations at design time, ahead of shipping a mod. Does **not** participate in the runtime game loop or mode parity. See root [`AGENTS.md`](../../../AGENTS.md) for non-negotiable rules.

## Scoped commands

```sh
cargo run  -p parish-npc-tool -- generate-world --counties roscommon,galway
cargo run  -p parish-npc-tool -- generate-parish Kiltoom --pop 2000
cargo run  -p parish-npc-tool -- validate --all
cargo run  -p parish-npc-tool -- export --parish Kiltoom
cargo test -p parish-npc-tool                              # unit
```

## Local gotchas

- **Standalone tool — no mode parity.** `parish-npc-tool` is a dev-time binary, not part of the game loop. It does not participate in mode parity (rule #2) and is not wired into `parish-engine`, `parish-server`, or `parish-tauri`.
- **Binary-only crate (no lib).** All logic lives in `src/main.rs`. There is no library surface for embedders — consume the output JSON or use as a subprocess.
- **No dependency on `parish-core` or other Parish crates.** This crate is intentionally isolated from the runtime workspace to avoid pulling `rusqlite` and generation-time deps into the engine. It shares JSON schema conventions with `parish-npc` but no Rust code.
- **Owns its own SQLite schema.** The DB schema (parish/household/NPC tables, see #434) diverges from `parish-persistence`'s branch-keyed game-snapshot format. Schema migrations are managed independently — no migration framework shared with the runtime.
- **Output must be hand-massaged into the mod.** The tool produces NPC JSON that authors review and commit into the mod's `npcs.json` (or future `parish-world.db`). Generated output is authoritative only after human validation — use `validate` before committing.
- **No inference dependency at build time.** The `elaborate` subcommand reaches out to an LLM at invocation time, but the crate itself does not depend on `parish-inference` or wire into the engine's inference worker.

## Module map

`main.rs` — all logic (single binary file, clap-driven subcommands).
