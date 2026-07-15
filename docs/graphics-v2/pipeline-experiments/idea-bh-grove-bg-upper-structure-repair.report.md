# Audit Report

Generated with one built-in `image_gen` edit call using the seven attached images in order. The selected generated PNG was copied from `/Users/dmooney/.codex/generated_images/019f0f5b-b947-7290-a9ef-ddcf40b88dea/ig_0a8d1866ac043446016a415f77b640819492d8947cc416861f.png`.

- Upper garden-wall structure: restored. The former gate/wall-like detail now reads as a tiny low roofed outbuilding sitting on or just inside the upper garden wall, aligned with the reference position from Images 2-3.
- Rest of frame: visually preserved in crop, roads, fields, wall network, garden beds, tree placement, main buildings, lower cottages, and palette. It is not guaranteed pixel-identical because the built-in image edit re-rendered the full plate, but I do not see moved roads, new paths, new buildings, or a global restyle.
- Doors on openings: existing visible cottage and main-building doors remain fitted with thresholds. The added shed is very small; it has a dark front/side detail that reads as a small fitted plank door or closed opening rather than a person-sized empty doorway.
- Roof, chimney, and nub discipline: the added shed has a low dark roof and no visible chimney, vent, pipe, capstone, roof nub, smoke hole, or smoke. Existing roofs also appear free of new chimneys or chimney-like protrusions.
- Semantic leaks: no visible people, animals, carts, barrels, crates, water, church, shop sign, labels, UI, or extra props were introduced. The only intentional semantic addition is the requested small shed/outbuilding.
- Remaining risk: because this is an image-model repair without an explicit pixel mask, subtle texture-level repainting may exist outside the intended repair area. Use visual review or a local diff/mask workflow before treating it as a strict pixel-preserving asset.
