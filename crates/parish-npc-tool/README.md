# parish-npc-tool

SQLite-backed NPC world builder and inspection utility for Parish/Rundale (#433).

A standalone command-line dev utility that generates and inspects large NPC populations at design time. Authors run it ahead of shipping a mod; the running game does not invoke it.

## Why decoupled from `parish-core`

`parish-npc-tool` is a build-time / authoring tool, not part of the runtime. It owns its own SQLite schema (parish/household/NPC rows, see #434) which diverges from `parish-persistence`'s branch-keyed game-snapshot format. Keeping it as a sibling crate isolates `rusqlite` and generation-time deps from the runtime engine and lets the tool evolve independently of game-state persistence.

It does **not** depend on the runtime `parish-npc` library crate. The runtime crate handles in-memory NPC simulation; this tool produces the JSON the runtime later loads.

## Commands

```sh
parish-npc-tool generate-world --counties roscommon,galway  # build the world DB
parish-npc-tool generate-parish Kiltoom --pop 2000          # seed one parish
parish-npc-tool list --parish Kiltoom --occupation Farmer
parish-npc-tool show 12345
parish-npc-tool search "Darcy"
parish-npc-tool edit 12345 --mood cheerful
parish-npc-tool promote 12345                               # Sketched -> Elaborated
parish-npc-tool elaborate --parish Kiltoom --batch 50       # batch LLM elaboration
parish-npc-tool validate --parish Kiltoom
parish-npc-tool validate --all
parish-npc-tool stats
parish-npc-tool export --parish Kiltoom > kiltoom.json
parish-npc-tool import < kiltoom.json
parish-npc-tool family-tree 12345
parish-npc-tool relationships 12345
```

See [`docs/design/scalable-npc-data-design.md`](../../docs/design/scalable-npc-data-design.md) for the full design.

## Audience

Mod authors and content designers — not end-users of the game. Output is hand-massaged into the mod's `npcs.json` (or future `parish-world.db`) before commit.

## Relationship to runtime crate `parish-npc`

`parish-npc` (the runtime library) consumes NPC data files at game-load time. `parish-npc-tool` (this crate, the dev binary) produces those files. They share JSON schema conventions but no Rust code.
