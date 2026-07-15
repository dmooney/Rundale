# idea-ag-beechwood-af-door-threshold-repair

Built-in `image_gen` prompt used verbatim:

```text
Use case: precise-object-edit
Asset type: game environment background plate repair, 16:9 desktop plate
Edit target: the attached Beechwood AF background plate
Reference style: preserve the existing illustrated parish-notebook look: hand-inked linework, watercolor wash, rough stone, muddy road texture, quiet Irish rural palette, no UI.
Primary request: make the plate pass a door/threshold audit without changing the map-derived layout.
Required edit: the lower-right foreground thatched cottage currently has a blank visible wall. Add one clear, believable dark doorway with a small threshold to that cottage on its visible front-facing wall. The doorway must be readable at game scale.
Audit rule: every visible standalone building or visible building wing must have at least one readable door, doorway, gate opening, or threshold on a visible wall unless it is clearly only a ruin or wall segment. Do not leave any foreground/background cottage as a blank sealed box.
Preserve strictly: north-up composition, isomorphic/orthographic camera, all roads, walls, garden plots, gates, building footprints, building count, roof shapes, enclosure shapes, vegetation masses, crop rows, watercolor texture, and overall lighting.
Do not add: new buildings, churches, chapels, water, bridges, smoke, fog, people, animals, UI, labels, text, signs, random props, new paths, decorative boundary hedges, or invented landmarks.
Chimney rule: do not add chimneys or smoke. Do not turn wall stones, ridge caps, or roof repairs into chimney-like nubs.
Repair style: tiny surgical paint-over only; the image should look like the same plate after a careful artist added missing entrances.
Output: one repaired 16:9 image.
```

Input roles:

- Edit target: `docs/graphics-v2/pipeline-experiments/idea-af-beechwood-ae-roof-nub-cleanup.png`
- Style reference: `docs/graphics-v2/style-crops/illustrated-style-low-camera-slate-single-house-door-clean.png`
- Style reference: `docs/graphics-v2/style-crops/illustrated-style-low-camera-thatched-single-house-door-clean.png`
