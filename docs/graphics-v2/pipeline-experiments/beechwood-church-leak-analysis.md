# Beechwood Church Leak Analysis

This note analyzes why `idea-g-raw-map-control-02.png` produced a church /
churchyard even though the Beechwood map crop was intended to be interpreted
without per-location hints.

## Artifacts

- Source map crop: `map-crop-control-02.png`
- Generated plate: `idea-g-raw-map-control-02.png`
- Prompt: `idea-g-raw-map-control-02.prompt.md`
- Style reference: `../illustrated-parish-notebook.png`

## Likely Cause

The church most likely came from semantic leakage from the full illustrated
notebook style reference, amplified by ambiguous Beechwood map geometry.

The style reference contains a highly salient church/churchyard scene:

- a white church with a bellcote/tower,
- a walled graveyard with headstones,
- a road junction beside the church,
- visible chapel-themed UI/location text in the image.

The Beechwood crop contains several visual cues that can be mapped onto that
reference even though they are not a church:

- a road running beside a building group,
- a long rectangular hatched building/structure near the road,
- a large walled rectangular planted enclosure,
- dense tree/woodland symbols around the site,
- large printed letters and labels that add visual noise,
- ambiguous boundary/field lines and a strong right-edge dotted boundary.

Given a generic "historical parish" prompt and a full-scene style reference,
the model appears to have used the church scene as a semantic template rather
than only as a rendering style. It transformed the road-adjacent building group
and walled enclosure into a chapel with graveyard, then added unsupported water
at the right edge.

## Why Cycle A Worked Better

Cycle A used the same raw-map-first method, but the Grove target crop was more
cooperative:

- the site read strongly as a farm/orchard cluster,
- the central buildings and planted enclosure were less church-like,
- there was less large text competing with the physical map marks,
- the crop had fewer features that resembled the style reference's
  church/churchyard composition.

So Cycle A's success was not caused by a hidden pipeline improvement. It was a
good combination of clean context, a simple raw-map prompt, and a map crop whose
geometry did not invite the style reference's church semantics.

## Prompt/Pipeline Implication

For the next test, keep the raw map as the primary layout evidence, but remove
the full-scene style reference. Use cropped style references only:

- road/yard texture,
- stone wall/hedge/tree rendering,
- roof/facade rendering,
- watercolor/ink treatment.

Avoid style-reference crops that contain:

- churches,
- graveyards/headstones,
- bridges/water,
- road signs or labels,
- full-scene landmark composition.

The prompt should also explicitly say:

```text
Do not import objects, buildings, landmarks, road signs, graveyards, churches,
bridges, rivers, people, UI, or named places from the style reference. Use the
style reference only for rendering medium, brush/ink texture, palette, camera
feel, and material handling.
```

This is still generic and not location-specific. It bans semantic transfer from
the style reference rather than describing Beechwood.
