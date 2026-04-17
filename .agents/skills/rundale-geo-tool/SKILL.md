---
name: rundale-geo-tool
description: Use whenever touching Rundale world geography — `realign_rundale_coords`, `mods/rundale/world.json` coordinates, pinning real-world locations to historical maps, subordinating village clusters via `relative_to`, choosing between `geo_kind: real`/`manual`/`fictional`, or deciding when to use modern geocoders vs historical OS maps. Covers the `geo-tool` CLI suite, the coordinate resolver (absolute + relative + graph-delta fallback), how to compute historical offsets from earlier commits, and why Nominatim alone is the wrong primary source for a 1820s Irish world. Trigger eagerly — any task involving lat/lon in Rundale, the Parish Designer editor's geographic fields, "pin X to coord Y", "move the Kilteevan cluster", "the fictional establishments didn't follow the village", or similar.
---

How to work with Rundale's geographic coordinate system.

The game is set in 1820s rural Ireland. `mods/rundale/world.json` stores lat/lon for ~22 locations. Some are real places (geocoded from modern OSM), some are author-pinned to historical map features (OS 6-inch First Edition, ~1837), and most are fictional villages/farms/churches/pubs whose positions need to stay spatially coherent with the anchors they're near. Two CLI binaries handle this, plus a runtime resolver in the game itself.

## The binaries

**`geo-tool`** (`crates/geo-tool/src/main.rs`) — a multi-subcommand CLI. Two flows:

- *Legacy top-level flags* (`geo-tool -- --area …`) run the **OSM Overpass pipeline**: queries OpenStreetMap, extracts modern game-relevant features within a bounding box, and emits a candidate `world.json`. This is the modern-world seeding side; rerun when bootstrapping a new mod or expanding the footprint. All existing justfile recipes (`just geo-tool`, `just geo-tool-merge`) still drive this.
- *`historic-discover` subcommand* (`geo-tool historic-discover --bbox … --zoom 16`) runs the **AI-assisted OS 6-inch pipeline**: fetches tiled raster imagery from a historic tile source (NLS Roscommon sheet by default), feeds 2×2 tile chunks to a vision-capable LLM (OpenRouter → Claude/GPT-4o or a local Ollama vision model) to identify every man-made or named feature on the engraved map, and emits a `parish.json` of candidate 1820s locations already placed on the historical map. Labelled features come out as `geo_kind: Manual` (authoritative 1820s pins); unlabelled features come out as `geo_kind: Fictional` with batched LLM-generated plausible 1820s Irish names. Output is a drop-in `--merge` source for the legacy Overpass flow. See the "Seed a new area from OS 6-inch" recipe.

**`realign_rundale_coords`** (`crates/geo-tool/src/bin/realign_rundale_coords.rs`) — the day-to-day tool. Reads `mods/rundale/world.json`, geocodes `Real` locations via Nominatim, resolves `Manual` pins and `relative_to` references, then graph-delta-realigns any remaining `Fictional` locations based on how nearby anchors moved. Writes the result back with 4-space indent. Justfile wrapper: `just realign-coords`. Only geocodes `Real`, so `Manual` entries emitted by `historic-discover` stay pinned.

## The coordinate model

Every location in `world.json` has these coordinate-related fields:

| Field | Meaning |
|---|---|
| `lat`, `lon` | The *resolved* absolute WGS-84 coords. Always present. For `relative_to` locations, this is a cache the resolver rewrites. |
| `geo_kind` | One of `real`, `manual`, `fictional`. Controls how `lat`/`lon` is determined. |
| `relative_to` (optional) | `{ anchor: <id>, dnorth_m: <m>, deast_m: <m> }`. When present, `lat`/`lon` are derived as `anchor.lat/lon + offset` in meters ENU. Authorial intent lives here. |
| `geo_source` (optional) | Provenance string for Manual pins — e.g. `"OS 6-inch First Edition, Roscommon sheet, ca. 1837"`. Ignored at runtime; metadata only. |

Three `geo_kind` variants:

- **`real`** — Backed by a modern OSM feature. `realign_rundale_coords` geocodes the name via Nominatim, updates `lat`/`lon`, contributes a delta that graph-realigns nearby fictionals.
- **`manual`** — Author-pinned to an authoritative coord (typically a historical map feature). Never geocoded. Still contributes as an anchor for realignment and for `relative_to` descendants.
- **`fictional`** — Invented place. Position is either: (a) authored absolutely and graph-delta-realigned when anchors move, or (b) derived via `relative_to` and tracks its anchor exactly.

Resolution order inside `realign_rundale_coords`:

1. Load `world.json`.
2. Apply any `--set-coord` / `--set-source` CLI overrides (these flip `geo_kind` to `Manual` and record deltas).
3. For each `Real` location, geocode via Nominatim (with suffix-stripping fallback — see "Gotchas"). Graceful degradation: on zero hits, warn and keep existing coord.
4. Topologically resolve `relative_to` refs — any cycle or unknown anchor is a hard error. Writes new `lat`/`lon` for each relative location.
5. For each `Fictional` location *without* `relative_to`, apply a weighted delta from the BFS-reachable anchor set.
6. Serialize back to disk with 4-space indent.

## Decision tree: which mode for a new coordinate task?

```
Am I bootstrapping an entire new area from scratch, with no curated Rundale data?
├─ YES → Run `geo-tool historic-discover --bbox ...` to get a first-pass
│        world.json already placed on the OS 6" First Edition. Then iterate
│        with --merge against curated data. (See "Seed a new area" recipe.)
│
└─ NO → Is this a single location you already know about?
    │
    Is this a real-world place that still exists today and modern geocoders find correctly?
    ├─ YES → geo_kind = real. Set name to match OSM. Let Nominatim handle it.
    │
    └─ NO → Is it a historical feature (on OS 6-inch / 25-inch / Down Survey) but not today?
        ├─ YES → geo_kind = manual. Get coord from GeoHive (map.geohive.ie), set
        │        geo_source to cite the map sheet. Use --set-coord for the pin.
        │
        └─ NO → It's fictional. Does it need to track another location rigidly
                (e.g. "the forge sits 50 m east of the church")?
            ├─ YES → geo_kind = fictional + relative_to = { anchor, dnorth_m, deast_m }.
            │        It will follow its anchor exactly through any future pin or move.
            │
            └─ NO → geo_kind = fictional, absolute lat/lon. The graph-delta realign
                    will nudge it via weighted average of nearby anchor shifts.
                    (Works for small shifts; fails to preserve clusters on big moves.)
```

## Canonical recipes

### Pin a real-world location to a historical coord

```bash
cargo run -p geo-tool --bin realign_rundale_coords -- \
  --world mods/rundale/world.json --in-place \
  --set-coord "Kilteevan Village=53.6320798910683,-8.102070946274374" \
  --set-source "Kilteevan Village=OS 6-inch First Edition, Roscommon sheet, ca. 1837"
```

`--set-coord` flips `geo_kind` to `Manual`, writes the coord, clears any `relative_to`, and records a delta so connected fictionals realign. Both flags repeatable.

### Subordinate a cluster to an anchor (two steps)

If you're pinning an anchor (e.g. The Crossroads) and want its village cluster to move with it, you must first convert the cluster members to `relative_to` the anchor. Steps:

1. Identify the cluster — the fictional locations that should always sit near the anchor (not all of them, just the ones that belong to the cluster).
2. Compute historical offsets using the helper:
   ```bash
   python3 .skills/rundale-geo-tool/scripts/compute_historical_offsets.py \
     --anchor-id 1 --cluster 2,3,4,6,9,13 --baseline-commit 91c996c
   ```
   The baseline commit is "the last commit where the cluster was spatially coherent." For Rundale, that's typically `91c996c` (before any realign pipeline ran) or `cc3d85f` (before the OS-6" pinning work).
3. Apply the offsets with the helper:
   ```bash
   python3 .skills/rundale-geo-tool/scripts/add_relative_to.py \
     --anchor-id 1 \
     --offsets '{"2":{"dnorth_m":445,"deast_m":462}, ...}'
   ```
   Or hand-edit `world.json` (both work; the script is just a shortcut).
4. Run realign to resolve:
   ```bash
   just realign-coords
   ```
5. Now future `--set-coord` on the anchor automatically carries the whole cluster.

See commits `7d05463` (Kilteevan cluster) and `e1f3aa0` (Crossroads cluster) for worked examples.

### Seed a new area from OS 6-inch (AI-assisted first pass)

When bootstrapping a brand-new bbox where no curated Rundale data exists, let a vision-capable LLM read the OS 6" sheet and emit a candidate `parish.json`:

```bash
export OPENROUTER_API_KEY=sk-or-...
cargo run -p geo-tool -- historic-discover \
  --bbox 53.600,-8.130,53.650,-8.080 \
  --zoom 16 \
  --tile-source nls-roscommon \
  --provider-url https://openrouter.ai/api \
  --model anthropic/claude-opus-4-7 \
  --api-key-env OPENROUTER_API_KEY \
  --cache-dir data/cache/historic \
  --output /tmp/rundale-historic.json
```

What it does:
- Fetches NLS tile PNGs covering the bbox at `z=16` (cache at `data/cache/historic/nls-roscommon/{z}/{x}/{y}.png`).
- Stitches 2×2 tile groups into 512×512 images, sends each to the vision model with a 1830s-OS-6" system prompt asking for every man-made/named feature as JSON (`px`, `py`, label, feature_kind, connected road segments).
- Filters at `--confidence-floor` (default 0.5), projects pixel → WGS-84, dedupes within 30 m.
- Batches a second text-only call to the same model to name any unlabelled features with plausible 1820s Irish names.
- Builds bidirectional connections: vision-reported road segment endpoints snap to the nearest feature within 60 m; isolated features get a nearest-neighbour fallback at ≤ 2 000 m.
- Writes `parish.json` + `.meta.json` + `.audit.json` (confidence drops, dedup counts, attribution, tile source id).

Output contract:
- Labelled features → `geo_kind: Manual`, `geo_source: "OS 6-inch First Edition, Roscommon sheet, ca. 1829–42 (NLS)"`.
- Unlabelled features → `geo_kind: Fictional`, `geo_source` ending `"(unlabelled feature)"`.

`Manual` means `realign_rundale_coords` will leave them pinned on subsequent runs — that's the whole point. Merge into an existing mod:

```bash
cargo run -p geo-tool -- \
  --bbox 53.600,-8.130,53.650,-8.080 \
  --merge /tmp/rundale-historic.json \
  --output mods/rundale/world.json
just realign-coords   # Manual entries untouched; any Real entries get Nominatim
```

Tile sources beyond NLS Roscommon are pluggable via the `HistoricTileSource` trait in `crates/geo-tool/src/historic/tile_source.rs`; only Roscommon ships today. If `--tile-source` is omitted, the first registered source whose coverage overlaps the bbox wins. Kerry/Cork/etc. will return "no registered tile source covers bbox" until more sources are added.

**When to use this vs hand-pinning from GeoHive:** use `historic-discover` when you're populating dozens of features across a fresh bbox (e.g. a new county or parish) — it's hours of GeoHive clicks in one command. Use hand-pinning via GeoHive + `--set-coord` for individual refinements, for anchors that need pixel-perfect placement, or when you don't want to burn LLM tokens for a single location.

### Offline realignment from a baseline

When you've edited `world.json` coords by hand and want the graph-delta to apply without hitting Nominatim:

```bash
cp mods/rundale/world.json /tmp/world_baseline.json
# ... hand-edit the anchor coords ...
cargo run -p geo-tool --bin realign_rundale_coords -- \
  --world mods/rundale/world.json \
  --baseline-world /tmp/world_baseline.json \
  --no-geocode --in-place
```

`--baseline-world` computes deltas by diffing the two files; `--no-geocode` skips the Nominatim pass entirely.

### Quick inspect

```bash
python3 -c "
import json
w = {l['id']: l for l in json.load(open('mods/rundale/world.json'))['locations']}
loc = w[15]  # Kilteevan Village
print({k: loc[k] for k in ['name','lat','lon','geo_kind','relative_to','geo_source'] if k in loc})
"
```

## Data sources

| Source | URL | Used? | For what |
|---|---|---|---|
| **Nominatim** | `nominatim.openstreetmap.org/search` | Yes, runtime | Modern geocoding of `Real` locations in `realign_rundale_coords`. Rate-limited (~1 req/sec); not suitable at island scale. |
| **Overpass** | `overpass-api.de/api/interpreter` | Yes, runtime | Bulk OSM feature extraction in the main `geo-tool` binary. Run rarely. |
| **OSM raster tiles** | `tile.openstreetmap.org` | Yes, UI | Map background layer in the frontend. |
| **OS 6-inch First Edition** (ca. 1829–42) | `map.geohive.ie` (viewer) + NLS Roscommon tile set (`mapseries-tilesets.s3.amazonaws.com/os/roscommon1/{z}/{x}/{y}.png`) | Yes, **both manual and automated** | Authoritative source for 1820s Irish settlements. For single pins, get coords by clicking labels in the GeoHive viewer and `--set-coord` them. For bulk seeding of a new area, use `geo-tool historic-discover --bbox …`, which calls a vision LLM over the NLS raster. Pluggable `HistoricTileSource` trait in `crates/geo-tool/src/historic/tile_source.rs` for adding GeoHive / NLS-Ireland / etc. |
| **OS 25-inch** (ca. 1887–1913) | `map.geohive.ie` | Occasionally | Higher-resolution historical map for later-era details. |
| **Tailte Éireann MapGenie / WMTS** | `tailte.ie/services/mapgenie/` | No (referenced in `parish.example.toml:145`, commented out) | Planned future source for tiled historical maps. |
| **logainm.ie** | `logainm.ie` | No (only cited in `docs/research/irish-language.md`) | Authoritative for Irish placename etymology and admin hierarchy. Does **not** have "village" as a category — it describes administrative identity (townland/civil parish/etc.), not physical settlements. Good for name disambiguation, not for village-center coords. |
| **townlands.ie** | `townlands.ie` | No (referenced in design docs) | Townland/civil parish polygons. Planned for Stage B. |
| **Geofabrik Ireland extract** | `download.geofabrik.de` | No | Planned for offline OSM bulk processing at island scale. |
| **Wikipedia/Wikidata** | — | No | Modern village centers; useful sanity checks, wrong for 1820s settlements. |

**Key lesson:** for a 1820s world, the OS 6-inch First Edition is the authoritative map, not any modern geocoder. The physical village cluster often sat hundreds of metres away from what today's Nominatim or Google Maps calls "Kilteevan" — see the Kilteevan example: modern village center is ~1.3 km NW of the OS 6" labeled feature.

## Gotchas (in rough order of how likely you are to hit them)

1. **Nominatim doesn't know "Kilteevan Village."** OSM tags it `place=townland name=Kilteevan`, not `Kilteevan Village`. The tool auto-retries with trailing type words stripped (`Village`, `Town`, `Parish`, `Hamlet`, `Townland`, `Cross`, `Crossroads`), so `Kilteevan Village` falls back to `Kilteevan` and returns the townland centroid. When that's still wrong (townland centroid ≠ village center), use `Manual` with the OS 6" coord.
2. **Graph-delta realignment is a weighted average, not a rigid translation.** When a `Real` or `Manual` anchor moves, fictionals without `relative_to` get nudged by the BFS-weighted mean of nearby anchor deltas. For a cluster that *must* stay rigid relative to the anchor (village buildings around the crossroads), use `relative_to` — not graph-delta. The Kilteevan pin in commit `1f6efdd` drifted the whole village cluster ~10 km off before we set up `relative_to` in `7d05463`.
3. **`--set-coord` alone does not subordinate the cluster.** It only records a delta for the pinned location. Absolute-positioned fictionals around it get the graph-delta treatment. If you want clean cluster propagation, wire up `relative_to` first (see the recipe above) and then `--set-coord`.
4. **4-space indent.** All files in `mods/rundale/*.json` use 4-space indent, and the editor's byte-identity test enforces it. `realign_rundale_coords` writes with `PrettyFormatter::with_indent(b"    ")`. If you hand-edit with Python, use `json.dump(w, f, indent=4)` and append `'\n'` at the end. 2-space output will silently break the editor round-trip test.
5. **`geo_source`, `relative_to`, and `lat`/`lon` all coexist.** `relative_to` overrides `lat`/`lon` on resolve; but `lat`/`lon` is still written back as a cache. `geo_source` is purely informational.
6. **Weather test brittleness.** If you shift a lot of coords, `test_full_world_state_roundtrip` in `crates/parish-cli/tests/persistence_integration.rs` can trip because the stochastic weather engine transitions at different game-times. Fix at the test (move the weather-set to just before `/save`), not at the data.
7. **Travel-time bound.** `test_parish_computed_travel_times_reasonable` asserts every edge fits in `1..=300` minutes at 1.25 m/s walking. Currently the longest edge is Kilteevan ↔ Curraghboy Road at ~16 km / 220 min. If a future coord shift pushes an edge past 300 min, consider adding intermediate nodes rather than bumping the bound further.
8. **Nominatim usage policy.** Rate-limited at ~1 req/sec and production use requires a self-hosted instance. The current tool has 4 real locations so fine; at island scale (thousands of places) the plan is to switch to a local canonical registry built from logainm + townlands.ie + Geofabrik bulk exports.
9. **`logainm` vs `OS 6"` for historical geocoding.** logainm is the Irish state placename authority but it only covers administrative identity (townland, civil parish, ED). It has no "village" category and no 1830s point features. The OS 6-inch First Edition is what you want for "where was the physical settlement in 1830."
10. **`historic-discover` is a first-pass seeder, not a replacement for review.** The vision LLM confidently hallucinates features on noisy or low-zoom tiles and transcribes labels from period script imperfectly (long-s as "f", worn engraving misread, etc.). Always review the `.audit.json` sidecar (confidence floor drops, dedup counts) and spot-check the output in the editor with the OS-6" overlay before committing. Zoom 16 is the sweet spot — 15 is too coarse to read labels, 17 multiplies tile count by 4 without much quality gain.
11. **`historic-discover` coverage is currently Roscommon-only.** The `TileSourceRegistry` ships only `NlsRoscommonSource`; a bbox outside the Roscommon sheet returns "no registered tile source covers bbox". Add more implementations of `HistoricTileSource` (`tile_source.rs`) as island-wide sources come online; the discover pipeline speaks the XYZ convention, so TMS sources must y-flip internally.

## Canonical commits to reference

- `91c996c` — last commit where the Rundale cluster was hand-authored and spatially coherent (Kilteevan Village at `(53.606, -8.110)`, tight village within 1.5 km). Use as the baseline for computing historical offsets.
- `cc3d85f` — the realign-coords runbook + just recipes were added. World had been through one automated realign but pre-dated all the Stage A/B work.
- `1f6efdd` — Stage A shipped: Manual + RelativeRef + CLI `--set-coord`. Kilteevan first pinned to OS 6" coord.
- `7d05463` — Kilteevan's cluster (Forge, Well, Mill, Weaver's) subordinated via `relative_to`.
- `e1f3aa0` — The Crossroads' cluster (Darcy's, St. Brigid's, Letter Office, Hedge School, Murphy's Farm, Connolly's Shop) subordinated.

## Testing after a coord change

```bash
cargo fmt --all && \
cargo clippy --workspace --bins --tests -- -D warnings && \
cargo test --workspace
```

Specific tests worth re-running after big coord shifts:

- `cargo test -p parish --test world_graph_integration test_parish_computed_travel_times_reasonable` — asserts all edges are walkable in a day
- `cargo test -p parish --test persistence_integration test_full_world_state_roundtrip` — save/load fidelity
- `cargo test -p parish-core --lib editor::persist::tests::save_mod_byte_identical_to_source` — editor round-trip byte-identity (depends on 4-space indent)

## Bundled scripts and references

See:

- `scripts/compute_historical_offsets.py` — given an anchor id, a list of cluster ids, and a baseline commit SHA, prints the `(dnorth_m, deast_m)` offsets from the anchor to each cluster member as they existed in that commit. Use this whenever you're about to set `relative_to` on a cluster — it saves you from computing haversine deltas by hand.
- `scripts/add_relative_to.py` — edits `world.json` in place to add `relative_to` references to a set of location ids. Matches the 4-space indent convention. Run `just realign-coords` after to materialize.

Both scripts assume `cwd` is the repo root.
