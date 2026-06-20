# Visual Client M2

This milestone turns the separate graphics client into a real scene renderer
contract consumer. The player sees the backend-authored plate image on the
canvas, with hotspot labels and NPC slots layered over it. Canvas clicks are
translated into game commands for the same browser session, proving the visual
client can be an alternate app surface instead of a passive debug preview.

## Affected Subsystems

- `parish/apps/visual`: loads scene plate images, renders them on Canvas 2D,
  tracks hovered/selected hotspots, derives hotspot commands, and submits them
  through `/api/command`.
- `parish/crates/parish-server`: consumed as-is through `/api/scene-state`,
  `/api/scene-asset/*`, and `/api/command`.
- `parish/apps/ui`: intentionally untouched for this milestone.

## Data Model

No new backend data model is required. The visual client extends its client-side
display model with:

- stage-space hotspot bounds used for hit-testing;
- a derived command per hotspot (`go to <label>` for travel, inspect text for
  inspect, and a future-safe `talk to <label>` for talk);
- transient view state for loaded plate image, hover, and selection.

The renderer still uses browser-native Canvas 2D. A future phase can swap in a
rendering engine once interactions and image assets are proven against the real
server contract.

## Observable Signal

The harness signal remains `/scene`: The Crossroads and Darcy's Pub must expose
their plate URLs and hotspot lists. The browser signal is the visual client
showing the plate image underneath overlays, then changing scene title after a
canvas hotspot click drives `/api/command`.

## Feature Flag

No new flag. Backend scene-state remains gated by the existing `diorama` flag.
