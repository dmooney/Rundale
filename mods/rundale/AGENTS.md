# mods/rundale — agent scope

Game content for Rundale (Irish living world, 1820). Loaded by `parish-core::loading`. See root [`AGENTS.md`](../../AGENTS.md) and use the `/rundale-geo-tool` skill for any coord work.

## Scoped commands

```sh
just game-test          # harness walkthrough against this mod
just screenshots        # regenerate visual baselines
cargo test -p parish-core --test mod_loading   # schema validation
```

## Local gotchas

- **`mod.toml` schema is fragile** — additive changes only without a migration. Existing saves load against the schema in their save header.
- **`world.json` coords must follow geo-tool rules.** Use the `/rundale-geo-tool` skill — never hand-edit lat/lon. Real-world locations pin to historical OS maps, not modern Nominatim. Subordinate village clusters via `relative_to`.
- **`npcs.json` is 174 KB.** Edit with care — ID renames cascade to schedules, encounters, dialogue history; large reflows make review impossible. One NPC per commit.
- **`anachronisms.json` is consumed by `parish-npc::anachronism`.** Adding a banned word/phrase requires checking the dialogue corpus doesn't already use it (would generate spurious flags).
- **`prompts/` templates** are loaded by `parish-core::prompts`. Variable names are case-sensitive — `{player_name}` not `{playerName}`.
- **`festivals.json` + `encounters.json`** trigger by date/location — coordinate names must exist in `world.json`.

## Files

`mod.toml` manifest, `world.json` geography, `npcs.json` NPC catalog, `prompts/` templates, `loading.toml` boot config, `anachronisms.json`+`festivals.json`+`encounters.json`+`pronunciations.json`+`transport.toml`+`ui.toml` content.
