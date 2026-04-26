# Map Panel Evolution — Brainstorm

> Parent: [GUI Design](gui-design.md) | [Docs Index](../index.md)
>
> Status: **Brainstorm / RFC** — not yet committed to any specific approach.

## Problem

The current map renders all locations as a static geo-projected node-link diagram filling the right sidebar. Two issues:

1. **Label overlap** — Locations that are geographically close (e.g. Kiltoom Village / Kiltoom Church / The Crossroads) produce overlapping name labels, making them unreadable.
2. **No sense of scale or exploration** — The entire parish is always visible at a fixed zoom. There's no spatial progression as the player moves; the map feels like a static diagram rather than a living world.

## Ideas

### 1. GTA-Style Minimap (Player-Centered Radar)

A small, fixed-zoom circular or rounded-rect minimap anchored in the corner, always centered on the player's current location.

- **Fixed radius**: Show only locations within ~500m (or 2–3 graph hops) of the player.
- **Rotation**: Optionally rotate so "forward" (last movement direction) is always up, or keep north-up.
- **Player icon**: A directional chevron or dot at center.
- **Edge-of-radar indicators**: Locations beyond the visible radius shown as small arrows on the perimeter pointing toward them, like off-screen indicators in open-world games.
- **Fog of war**: Unvisited locations shown as `?` markers or greyed out until the player has been there.
- **Smooth panning**: When the player moves, the map smoothly scrolls to re-center rather than snapping.

**Pros**: Solves overlap (fewer visible nodes), feels like a real game map, encourages exploration.
**Cons**: Loses the "big picture" parish overview. Needs a complementary full-map view.

### 2. Full Map via `/map` Command or Hotkey

A modal overlay or dedicated panel showing the complete parish, triggered by:

- `/map` text command
- `M` hotkey (when input field is not focused)
- A small "expand" icon on the minimap corner

Options for the full map view:

- **a) OSM tile background**: Fetch and cache OSM raster tiles for the Kiltoom/Kilteevan area. Overlay game locations on top of real cartography. Gives instant geographic grounding — players see the actual lough, roads, boreens.
- **b) Stylized hand-drawn map**: A pre-rendered artistic map image (think Tolkien or old Ordnance Survey style) with locations pinned on top. Could generate with the parish-geo-tool data as a base.
- **c) Zoomable node graph**: The current geo-projected graph but with mouse-wheel zoom and click-drag pan. At high zoom the labels spread out naturally.
- **d) egui `ScrollArea` with virtual canvas**: Render the graph onto a large virtual canvas (e.g., 2000x2000 logical pixels) inside a scrollable/zoomable area. The sidebar shows a viewport onto this canvas.

### 3. Label Collision Avoidance (Quick Win)

Regardless of which map mode we pick, we should fix label overlap:

- **Force-directed nudge**: After computing geo positions, run a quick iterative repulsion pass on label bounding boxes (5–10 iterations is enough). Labels push apart until no overlap remains.
- **Leader lines**: If a label must move far from its node, draw a thin line connecting label to node (like a callout).
- **Adaptive font size**: Reduce font size when nodes are dense, or show abbreviated names (e.g., "Village" instead of "Kiltoom Village") when space is tight, with full name on hover.
- **Label placement priority**: Current location and adjacent locations always get labels. Distant locations can be icon-only, with name shown on hover tooltip.
- **Ellipsis/truncation**: For the minimap, only show names of nearby locations; distant ones are just dots.

### 4. Progressive Disclosure / Exploration Map

Tie the map to game progression:

- **Initially blank**: Player starts with only their current location visible on the map.
- **Reveal on visit**: Locations appear on the map the first time the player visits (or hears about them from an NPC).
- **NPC rumors**: An NPC mentioning "the fairy fort beyond Hodson Bay" could add a `?` marker at the approximate location before the player visits.
- **Map as inventory item**: The player finds or buys a parish map from a character, which reveals all locations at once. Before that, they rely on the minimap and memory.

This makes the map feel earned and supports the exploration theme.

### 5. Location Detail on Hover/Click

Enrich the map with contextual info:

- **Hover tooltip**: Show location name, brief description, time-to-walk from current position, NPCs currently there, and whether it's indoor/outdoor.
- **Click for details**: Clicking any visible location (not just adjacent) opens a small info card. Adjacent locations get a "Go here" button; distant ones show a suggested route.
- **NPC indicators**: Instead of uniform dots, show tiny NPC portraits or initials. Color-code by relationship (green = friendly, red = hostile, grey = stranger).
- **Activity indicators**: Pulsing glow or animation at locations where something is happening (NPC conversation, event, festival).

### 6. Time-of-Day Map Atmosphere

The map should feel different at different times, like the rest of the game:

- **Ambient lighting**: At night, the map darkens. Only locations with "lights" (pub, church, houses) glow. The countryside between locations is dark.
- **Weather overlay**: Rain could add a subtle texture or blue tint. Fog could reduce visibility radius on the minimap.
- **Shadow direction**: Cast subtle shadows from nodes based on sun position (morning = shadows west, evening = shadows east). Pure flavor.

### 7. Animated Travel

When the player moves between locations, show it on the map:

- **Moving dot**: A small player icon travels along the edge connecting origin and destination over the traversal time.
- **Path highlight**: The route lights up as the player walks it.
- **Encounter interruption**: If an encounter fires mid-journey, the moving dot stops at the encounter point and a small `!` appears.
- **Footprints**: Previously traveled paths get a subtle "worn path" visual (slightly thicker or lighter line), showing the player's preferred routes over time.

### 8. TUI Map Mode

Not just GUI — bring a map to the terminal too:

- **ASCII minimap**: A small Braille-character or box-drawing map in a Ratatui pane, showing the same player-centered radar concept in text.
- **`/map` command in headless**: Print an ASCII representation of the parish to stdout. Useful for debugging and accessibility.

### 9. OSM Integration Depth

If we go the OSM tile route for the full map:

- **Offline tile cache**: Bundle a small tileset (zoom 14–17 for the parish area, ~5MB) so it works without network. The `parish-geo-tool` binary could generate this.
- **Custom tile style**: Use a muted, sepia, or hand-drawn tile style (e.g., Stamen Watercolor or a custom Mapbox style) to match the game's aesthetic rather than standard OSM tiles.
- **Clickable real features**: Show real geographic features from OSM (the lough, roads, townland boundaries) as non-interactive background, with game locations as interactive foreground.
- **Coordinate query**: Clicking anywhere on the real map could generate a description ("You see a field of grazing sheep and a stone wall running east-west…") using the LLM with geographic context.

### 10. Map as Narrative Device

The map could be more than navigation:

- **Story annotations**: Important events leave markers on the map ("Here you found the lost ring" / "Father Brennan told you about the fairy tree here").
- **NPC trails**: Optionally show where NPCs have been today as faint dotted paths, reflecting their daily schedules. This would visualize the "living world" simulation.
- **Seasonal changes**: The map's color palette shifts with seasons — green fields in summer, brown/gold in autumn, bare trees in winter.
- **Sound zones**: Subtle audio indicators on the map (bird icons near the forest, wave icons near the lough) that hint at the soundscape — even if we never add audio, the visual indicators add flavor.

## Recommended Phasing

| Phase | Scope | Effort |
|-------|-------|--------|
| ~~**Quick win**~~ | ~~Label collision avoidance (#3) — fix overlap with force-directed nudge~~ | **Done** |
| ~~**Phase A**~~ | ~~GTA minimap (#1) + `/map` hotkey for full view (#2c — zoomable graph)~~ | **Done** |
| ~~**Phase B**~~ | ~~Fog of war / progressive disclosure (#4) + hover tooltips (#5)~~ | **Done** |
| ~~**Phase C**~~ | ~~Animated travel (#7) + time-of-day atmosphere (#6)~~ | **Done** |
| ~~**Phase D (map)**~~ | ~~OSM tile background for full map (#9), migrated to MapLibre GL JS for polished label placement (variable anchors, zoom-aware decluttering, symbol-sort priority)~~ | **Done** |
| ~~**Phase D.1 (tiles)**~~ | ~~`/tiles` slash command + configurable tile-source registry (OSM + Ireland Historic 6" 1829–1842 via NLS), gated behind `period-map-tiles` feature flag~~ | **Done** |
| **Phase D (TUI)** | TUI ASCII map (#8) | Medium |
| **Phase E** | Narrative annotations (#10) + NPC trails | Large |

## Open Questions

- Should the minimap be circular (GTA style) or rectangular (matching the panel shape)?
- For OSM tiles: bundle offline or fetch on demand? Offline avoids network dependency but adds to binary/data size.
- Should fog of war persist across save/load? (Probably yes — it's part of game state.)
- How does the `/map` overlay interact with the input field? Does it capture keyboard focus?
- Do we want the minimap in the TUI as well, or only GUI mode?

## Phase D.1 — Period tiles (configurable tile-source registry)

The OSM background shipped in Phase D is anachronistic for Rundale's 1820
setting — it renders modern motorways, housing estates, wind farms, etc.
Phase D.1 adds a **registry of named tile sources** data-driven from
`parish.toml`'s `[engine.map]` section and a `/tiles <id>` slash command
to switch between them at runtime.

Two sources ship baked-in:

- **`osm`** — the current OSM raster (working default).
- **`historic-6inch`** — Ordnance Survey of Ireland First Edition 6-inch
  (surveyed 1829–1842), the most period-accurate cartography for
  Kiltoom. Ships wired to the [National Library of Scotland's free public
  S3-hosted tile service][nls-ireland] — no signup, CORS-open. Operators
  who want higher-fidelity tiles can override the URL in `parish.toml`
  with a Tailte Éireann MapGenie endpoint (gated behind the National
  Mapping Agreement) or a captured GeoHive tile URL.

[nls-ireland]: https://maps.nls.uk/geo/explore/

If an operator registers a new source without filling in a URL, the
frontend falls back to the flat panel background with a one-shot console
warning instead of a broken map.

Adding further sources (custom sepia-styled tiles, scanned grand-jury
maps georeferenced via MapWarper, etc.) is a pure TOML add; no code
changes needed.

The feature is gated behind the **`period-map-tiles`** flag
(default-enabled, per CLAUDE.md rule #6). Kill-switch:
`/flag disable period-map-tiles`.

**Scope limits (defer to later phases):**
- Only XYZ raster URL templates (`{z}/{x}/{y}.png`, optional TMS y-flip)
  — no WMS/WMTS adapters yet.
- No custom MapLibre style (still the Phase D sepia-via-desaturation look).
- No offline tile bundling — still on-demand fetch.
- Minimap stays flat-bg only.

## Related

- [GUI Design](gui-design.md) — Current map panel implementation
- [World Geography](world-geography.md) — Location data model with lat/lon
- [parish-geo-tool](geo-tool.md) — OSM data extraction tool
- [Time System](time-system.md) — Day/night cycle for map atmosphere
- [NPC System](npc-system.md) — Daily schedules for NPC trail visualization
