# mods/rundale — agent scope

Game content for Rundale: the canonical tiny world of Kilteevan, 1820 (product spec §14), three locations and three NPCs. The earlier large world is test data at `limerick/testing/fixtures/mods/rundale-legacy/`. Loaded by `limerick-core::loading`. See root [`AGENTS.md`](../../AGENTS.md) and use the `/rundale-geo-tool` skill for any coord work.

## Scoped commands

```sh
just game-test          # tiny-world walkthrough (testing/fixtures/rundale/tiny_world.txt)
cargo test -p limerick-engine --test world_sheet   # world sheet oracle
just screenshots        # regenerate visual baselines
cargo test -p limerick-core --test mod_loading   # schema validation
```

## Local gotchas

- **`mod.toml` schema is fragile** — additive changes only without a migration. Existing saves load against the schema in their save header.
- **`world.json` coords must follow geo-tool rules.** Use the `/rundale-geo-tool` skill — never hand-edit lat/lon. Real-world locations pin to historical OS maps, not modern Nominatim. Subordinate village clusters via `relative_to`.
- **`world-sheet.txt` is a test oracle.** `limerick-engine/tests/world_sheet.rs` plays a new game and fails on any contradiction with it; update the sheet with every intentional world change.
- **Keep JSON in the editor's on-disk format** (4-space indent). Editor round-trip tests re-save this mod and require identical bytes.
- **`anachronisms.json` is consumed by `limerick-npc::anachronism`.** Adding a banned word/phrase requires checking the dialogue corpus doesn't already use it (would generate spurious flags).
- **`prompts/` templates** are loaded by `limerick-core::prompts`. Variable names are case-sensitive — `{player_name}` not `{playerName}`.
- **`endpoints/` holds Limerick Endpoint definitions**, declared in `mod.toml` `[endpoints]` and named `<slug>.v<version>.json`. Until release, change the `.v1.json` file in place and republish with `pnpm definitions replace`; after release, published versions are immutable and a change ships as a new version file. `rundale-intent`'s `instructions` must equal the engine intent prompt (test-enforced). See `mobile/endpoint/README.md`.
- **`festivals.json` + `encounters.json`** trigger by date/location — coordinate names must exist in `world.json`.

## Files

`mod.toml` manifest, `world-sheet.txt` canonical world sheet, `endpoints/` Endpoint definitions, `world.json` geography, `npcs.json` NPC catalog, `prompts/` templates, `loading.toml` boot config, `anachronisms.json`+`festivals.json`+`encounters.json`+`pronunciations.json`+`transport.toml`+`ui.toml` content.
