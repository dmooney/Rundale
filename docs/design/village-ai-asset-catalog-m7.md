# Village AI Asset Catalog M7

## Purpose

M6 made terrain chunk compositing visible, but the chunks are still procedural proof sprites. M7 defines the production-facing AI asset catalog: deterministic prompt specs, metadata, masks, anchors, and output paths for the terrain chunks, object families, cottage variants, and NPC atoms needed to replace proof sprites with generated pixel art.

## Player Experience

Players should eventually see villages that preserve the same layout coherence while varying far beyond a small hand-authored sprite kit. Terrain chunks should share style and connect physically; cottages and props should stay culturally plausible for rural Ireland; NPCs should be assembled from atoms so many unique people can appear without a human art team drawing each one.

## Affected Subsystems

- `mods/rundale/scene-recipes/outdoor-village-layouts.json`: source of art direction, style lock, family lists, grid, and terrain profiles.
- `parish/apps/visual/scripts/generate-village-layouts.mjs`: add catalog generation, CLI output, validation, and summary metrics.
- `parish/apps/visual/scripts/generate-village-layouts.test.mjs`: cover deterministic catalog generation, terrain prompt coverage, NPC atom requirements, and negative validation cases.
- `.proofs/village-ai-asset-catalog-m7/`: generated pack, summary, chunk map, asset catalog JSON, screenshots, transcript, evidence, and judge.
- `parish/testing/fixtures/play_village-ai-asset-catalog-m7.txt`: live fallback proof.

## Data Model

The catalog should be proof-only JSON, not committed runtime scene content:

```json
{
  "schema_version": 1,
  "style_lock": "...",
  "terrain_requests": [
    {
      "id": "terrain.path.path-straight.n-s.v0",
      "class": "path",
      "template": "path-straight",
      "ports": ["n", "s"],
      "target": { "width": 78, "height": 54, "transparent": true },
      "anchor": [50, 50],
      "mask": { "walkable": true, "water": false, "blocks_objects": false },
      "prompt": "...",
      "negative_prompt": "...",
      "output_path": "assets/generated/terrain/path/path-straight-n-s-v0.png"
    }
  ],
  "npc_atom_requests": [],
  "npc_assemblies": []
}
```

## Validation

Validation should fail if a catalog request is ambiguous, untraceable, or unsafe for compositing. Required fields include id, kind, prompt, negative prompt, style tags, target size, transparent requirement for sprites, anchor, output path, and mask/compatibility metadata. NPC assemblies must include a body/base, head, lower clothing, footwear, and at least one outer/clothing or held-item layer.

## AI Direction

The manifest should describe assets in a stable art direction: high 3/4 pixel art, damp 1820s rural Westmeath, muted wet-earth palette, consistent pixel density, transparent background, no text baked into sprites, no modern objects, no mismatched camera angle, and clean alpha edges.

This deliberately separates generation from placement. The layout solver decides where things go; AI image generation produces compatible atoms for those validated slots.

## Feature Flag

No runtime feature flag is needed while M7 remains proof tooling. If AI-generated catalog assets become committed live scene content, gate the rollout behind a default-on flag such as `visual-ai-asset-catalog`.
