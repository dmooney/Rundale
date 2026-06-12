# Scene DSL pipeline — text → SVG → control image → master → variants

> Idea capture (2026-06-12, owner + agent brainstorm during M5 art work).
> Status: idea, not scheduled. Parent: [parish-diorama.md](parish-diorama.md)
> (RFC #1428); proven mechanics live in `parish/scripts/diorama/` and
> [the art handover](../../plans/parish-diorama-art-handover.md).

## The idea

A loose, LLM-interpreted scene-description DSL that compiles to diorama
masters:

```
text scene description
  → LLM emits layout as SVG (semantic classes/colors)
  → rasterize (deterministic)
  → geometry lint (machine-checkable topology) — fail → LLM retries
  → diffusion render (gpt-5.5 + image_generation: schematic as control
    image + one global style reference)
  → master plate
  → season × lighting variants (proven edit-off-master batch)
```

The Kilteevan river fix already proved the core mechanism: the image model
paints correctly inside a plan it didn't have to invent. This generalizes
the plan layer from hardcoded Python constants (`gen_master.py`) to
LLM-interpreted text, so each new location is a description file, not code.

## Why a LOOSE DSL

The consumer is an LLM, so a strict grammar buys nothing. The DSL's job is
**constraint capture**, not syntax:

- topology — "river: ONE continuous stream, enters E, exits W, crossed
  exactly once by the south lane, bridge at that crossing"
- relations — "well in the open common, NW of the crossroads"
- counts — "4 cottages, one per quadrant; at most one chimney in the scene"

Nouns are open vocabulary (cottage, forge, anvil, church, hearth, bar…):
the same LLM that draws the schematic also writes the **legend paragraph**
used in the diffusion prompt, so any noun survives the handoff. The legend
(color → meaning table) is the stable contract between the SVG conventions
and the render prompt — effectively the DSL runtime.

## The geometry lint is the secret weapon

Because the intermediate is structured SVG, layout bugs are machine-checkable
BEFORE spending render money or human review:

- river path is a single polyline, continuous, touches two frame edges
- exactly one bridge marker, within ε of the river ∩ lane intersection
- river locally straight through the crossing (a bend at the bridge makes
  the model misalign the banks — cand-01 defect)
- no wall segment intersects a lane band or the river
- well sits inside the common/plaza polygon, not on a lane band

Every one of these corresponds to a real defect a human had to catch by eye
during the Kilteevan run (wall across the road, well on the lane, bank
misalignment at the bridge). Lint failure feeds the error text back to the
LLM for a retry — a cheap text loop replacing an expensive
render-plus-eyeball loop.

**Render-side motivation:** even with a clean schematic, renders break
geometry often enough that an automated post-render gate (N samples →
vision check → human pick) is mandatory, not optional. See the empirical
findings below.

## Empirical findings (Kilteevan, 12 renders, 2026-06-12)

1. **A full-scene style reference leaks its COMPOSITION into the output.**
   Every batch that passed the old (broken-river) master as style ref
   produced rivers broken the same way — 0/4 continuity in the worst batch,
   even with refine-mode framing, layout narration and continuity rules in
   the prompt. Replacing the full master with a **style swatch sheet**
   (isolated texture crops — cottage, well, river bank, grass/wall — tiled
   on a dark ground, built by `gen_master.py swatches`) took river
   continuity from 0/4 to 4/4 in one step. Swatches carry palette and
   technique but physically cannot carry layout.
2. **Text narration + continuity rules are necessary but not sufficient.**
   They did not save the full-master-ref batch; combined with swatches they
   produced 2/4 full passes (the other 2 failed only on bank alignment at
   the bridge — the hardest registration detail).
3. **The vision gate works and human/agent eyeballs don't.** A full-frame
   glance misjudged a broken candidate as good (cand-02); the gpt-5.5
   check (downscaled full frame + full-res bridge crop, strict-JSON
   verdict) correctly failed all 8 bad renders and passed the 2 that
   survived zoomed manual verification.
4. **Exit semantics need a classifier, not a list of lanes.** Connections
   split into edge exits (painted lanes), in-scene doors (the weaver's
   cottage IS one of the four depicted), and generic links that share a
   painted exit's hotspot. And world.json **bearings can contradict the
   narrative** (Kilteevan's "road north" sits at bearing 295): the
   player-facing path_description wins for art; bearings only fill gaps.

## Bonus: hotspots from the same source

The scene description can emit **hotspot rects** alongside the schematic —
art geometry and clickable regions derive from one source, killing
art-vs-hotspot drift, and `scenes.json` becomes generated output rather than
hand-maintained. Exits/lanes already ground in `world.json` connections.

## Practical notes

- Emitted SVGs are small text — keep them in git (reproducible-ish regen);
  PNGs stay in iCloud per the no-art-in-git decision.
- One global style reference (the accepted Kilteevan master) for all
  locations so the palette stays uniform.
- Indoor scenes (Darcy's Pub) need a different vocabulary (hearth, bar,
  door) — open nouns + per-scene legend handle this naturally.
- Natural home: this IS the deferred `parish-art-tool` port (item 3 of the
  handover's deferred list) — the DSL pipeline becomes the art-tool's
  generate path rather than a separate tool.
