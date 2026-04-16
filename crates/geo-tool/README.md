# geo-tool utilities

This crate contains command-line tools for geographic workflows in Parish.

## Binaries

### `geo-tool`
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
cargo build -p geo-tool --bin realign_rundale_coords
```

### In-place geocode + realignment

```bash
cargo run -p geo-tool --bin realign_rundale_coords -- \
  --world mods/rundale/world.json \
  --in-place
```

### Offline/baseline workflow (no network)

```bash
cargo run -p geo-tool --bin realign_rundale_coords -- \
  --world mods/rundale/world.json \
  --baseline-world /tmp/world_before.json \
  --no-geocode \
  --in-place
```

### Custom context string

```bash
cargo run -p geo-tool --bin realign_rundale_coords -- \
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
