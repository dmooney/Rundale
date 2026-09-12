# limerick-npc-tool

SQLite-backed NPC world builder and inspection utility for Limerick/Rundale (#433).

A standalone command-line dev utility that generates and inspects large NPC populations at design time. Authors run it ahead of shipping a mod; the running game does not invoke it.

## Why decoupled from `limerick-core`

`limerick-npc-tool` is a build-time / authoring tool, not part of the runtime. It owns its own SQLite schema (limerick/household/NPC rows, see #434) which diverges from `limerick-persistence`'s branch-keyed game-snapshot format. Keeping it as a sibling crate isolates `rusqlite` and generation-time deps from the runtime engine and lets the tool evolve independently of game-state persistence.

It does **not** depend on the runtime `limerick-npc` library crate. The runtime crate handles in-memory NPC simulation; this tool produces the JSON the runtime later loads.

## Commands

```sh
limerick-npc-tool generate-world --counties roscommon,galway  # build the world DB
limerick-npc-tool generate-parish Kiltoom --pop 2000          # seed one parish
limerick-npc-tool list --parish Kiltoom --occupation Farmer
limerick-npc-tool show 12345
limerick-npc-tool search "Darcy"
limerick-npc-tool edit 12345 --mood cheerful
limerick-npc-tool promote 12345                               # Sketched -> Elaborated
limerick-npc-tool elaborate --parish Kiltoom --batch 50       # batch LLM elaboration
limerick-npc-tool validate --parish Kiltoom
limerick-npc-tool validate --all
limerick-npc-tool stats
limerick-npc-tool export --parish Kiltoom > kiltoom.json
limerick-npc-tool import < kiltoom.json
limerick-npc-tool family-tree 12345
limerick-npc-tool relationships 12345
cargo run --manifest-path limerick/Cargo.toml -p limerick-npc-tool -- art-inputs \
  --npcs mods/rundale/npcs.json \
  --world mods/rundale/world.json \
  --art-direction limerick/apps/ui/art/notebook-person-art/npc-art-direction-v1.json \
  --output limerick/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json
```

See [`docs/design/scalable-npc-data-design.md`](../../../docs/design/scalable-npc-data-design.md) for the full design.

## Audience

Mod authors and content designers — not end-users of the game. Output is hand-massaged into the mod's `npcs.json` (or future `limerick-world.db`) before commit.

## Relationship to runtime crate `limerick-npc`

`limerick-npc` (the runtime library) consumes NPC data files at game-load time. `limerick-npc-tool` (this crate, the dev binary) produces those files. They share JSON schema conventions but no Rust code.

The `art-inputs` command is file-only and does not touch the SQLite world-builder
database. It validates that every NPC has a reviewed art-direction entry before
exporting provider-ready notebook person-art prompts.
