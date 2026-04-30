# parish-geo-tool

This crate contains command-line tools for geographic workflows in Parish.

## Binaries

### `parish-geo-tool`
Primary extractor/transformer for OpenStreetMap-to-Parish world generation.

### `realign_rundale_coords`
Classifies and corrects map coordinates for mixed real/fictional worlds.

It is designed for Rundale's `mods/rundale/world.json` where:
- `geo_kind: "real"` locations are geocoded (true anchor points), and
- `geo_kind: "fictional"` locations are moved by inferred deltas so local layout is preserved.

## What `realign_rundale_coords` does

1. Loads a world JSON file (`{"locations": [...]}`).
2. Resolves coordinate deltas for real locations by either:
   - live geocoding (`Nominatim`) from name + context, or
   - comparing the current file against `--baseline-world`.
3. Applies weighted graph-distance deltas to connected fictional locations.
4. Writes the updated world file (in-place or to a new output file).

## Usage

### Build

```bash
cargo build -p parish-geo-tool --bin realign_rundale_coords
```

### In-place geocode + realignment

```bash
cargo run -p parish-geo-tool --bin realign_rundale_coords -- \
  --world mods/rundale/world.json \
  --in-place
```

### Offline/baseline workflow (no network)

```bash
cargo run -p parish-geo-tool --bin realign_rundale_coords -- \
  --world mods/rundale/world.json \
  --baseline-world /tmp/world_before.json \
  --no-geocode \
  --in-place
```

### Custom context string

```bash
cargo run -p parish-geo-tool --bin realign_rundale_coords -- \
  --world mods/rundale/world.json \
  --context "County Roscommon, Ireland" \
  --output mods/rundale/world.realigned.json
```

## Justfile shortcuts

From repository root:

```bash
just realign-coords-build
just realign-coords
just realign-coords-run --world mods/rundale/world.json --baseline-world /tmp/world_before.json --no-geocode --in-place
```

## Geocoder behavior and limitations (Stage A stopgap)

`realign_rundale_coords` uses Nominatim as its only online geocoder. When a real location's `name` ends in a type word (`Village`, `Town`, `Parish`, `Hamlet`, `Townland`, `Cross`, `Crossroads`), the tool retries with the suffix stripped — so `Kilteevan Village` falls back to `Kilteevan`, which matches the OSM townland.

When Nominatim returns no hits for any query variant, the tool logs a warning to stderr and keeps the location's existing `lat`/`lon`. The pipeline still runs; only the skipped location's delta is omitted from the fictional realignment. If *every* real location is skipped, the tool errors out with a message pointing at `--no-geocode` / `--baseline-world`.

**This is a stopgap.** At island scale (thousands of places), Nominatim's rate limits (~1 req/sec) and usage policy make it unsuitable as the production geocoder. The planned replacement is a local canonical placename registry built from logainm.ie and townlands.ie bulk exports plus a Geofabrik Ireland OSM extract, with village centers derived from civic-POI clusters. Tracked as Stage B–D follow-ups.

## Manual pins and relative positioning

Not every game-world location can be geocoded from a modern source — historical settlements marked on the OS 6-inch First Edition (ca. 1830s) often sit hundreds of metres from their modern namesakes, or no longer exist today. Two escape hatches let you author authoritative coordinates outside the geocoder.

### `Manual` locations

Set `"geo_kind": "manual"` on a location to tell the realign tool to never geocode it. The coord in `lat`/`lon` is treated as authoritative, and the location still contributes to the anchor set that realigns fictional neighbours. Use this for:

- Historical features you've georeferenced off the OS 6" / 25" maps
- Any real-world place Nominatim misplaces or can't find

You can pin a location from the command line instead of editing JSON:

```bash
cargo run -p parish-geo-tool --bin realign_rundale_coords -- \
  --world mods/rundale/world.json --in-place \
  --set-coord "Kilteevan Village=53.6320798910683,-8.102070946274374" \
  --set-source "Kilteevan Village=OS 6-inch First Edition, Roscommon sheet, ca. 1837"
```

`--set-coord "Name=lat,lon"` (repeatable) flips the location's `geo_kind` to `manual`, writes the coordinate, clears any `relative_to`, and records a delta so connected fictional locations realign. `--set-source "Name=note"` (repeatable) attaches a provenance string — visible in `world.json` as `geo_source` and ignored at runtime.

### Relative positioning

Any location can declare its position as an offset from another location's position, in meters north and east. Add a `relative_to` field:

```json
{
  "id": 28,
  "name": "Kilteevan Church",
  "geo_kind": "fictional",
  "lat": 0.0,
  "lon": 0.0,
  "relative_to": { "anchor": 15, "dnorth_m": 20.0, "deast_m": 5.0 }
}
```

The tool resolves `relative_to` chains in topological order (errors on cycles or unknown anchors) and writes the resulting absolute `lat`/`lon` back — the pair stored in JSON is a cache of the last resolution. Fictional locations with `relative_to` track their anchor exactly and are *not* graph-delta'd, so they stay pinned relative to the anchor no matter how it moves.

Typical use cases:
- A fictional cottage located 50 m south of a historic church: `relative_to: { anchor: <church_id>, dnorth_m: -50, deast_m: 0 }`
- A fairy fort co-located with a real archaeological site: `relative_to: { anchor: <site_id>, dnorth_m: 0, deast_m: 0 }`
