# Idea U Beechwood Tight Thatched Door Clean Style

Generated with the built-in `image_gen` tool from the exact prompt saved in `idea-u-beechwood-tight-thatched-door-clean-style.prompt.md`. Output image: `idea-u-beechwood-tight-thatched-door-clean-style.png` (`1672 x 941`, effectively 16:9).

## Verdict

Promising, but not topology-clean enough to treat as a final pipeline answer. It is a strong close-scale style/camera experiment and a useful Cycle U datapoint for Beechwood, but it loosens the Cycle M building footprint relationships.

## Strict Audit

- Topology preservation vs Cycle M/R/S: partial pass. The local ingredients are right: road exits, walled yard, garden enclosure to the north/east, gates, and a building cluster around a muddy court. However, the Cycle M/R/S main connected courtyard mass has been reinterpreted as a more separated farmstead cluster. That preserves Beechwood feel but weakens exact footprint continuity.
- Camera/scale vs Cycle R/S and notebook: partial pass. The crop is much tighter than Cycle R/S, with readable doors, wall thickness, muddy ruts, and facade texture. It is still higher and more roof-dominant than the original notebook and the low-camera crops; closer to a lowered isometric plate than a true 18-25 degree orthographic pitch.
- Thatch/no-chimney requirement: pass. At least two main rural buildings use rough aged thatch, and I do not see chimneys, smoke holes, chimney-like roof projections, or smoke.
- Door/threshold requirement: pass. Every visible yard/road-facing facade has a readable dark doorway or threshold. Side facades without doors are either not the primary approach face or partly masked/cropped by the composition.
- Semantic/layout leaks: mostly pass. No UI, labels, text, people, animals, carts, shops, churches, chapels, graveyards, bridges, water, smoke, fog, or copied notebook landmarks are visible.

## Problems And Risks

- The main topology risk is structural: the model turns the authoritative U/courtyard footprint into detached or semi-detached buildings. That is acceptable for a mood plate, not for a topology-preserving map-to-plate step.
- Garden and wall language is still a little tidy. The cabbages/rows and stone wall courses are less sterile than Cycle R/S, but some grid regularity and bead-like wall rhythm remain.
- One slate-roof building remains with a fairly regular tile pattern. It is less polished than Cycle R, but still cleaner than the rough slate target.
- The image hits the "close playable patch" goal well, but because it drops more of the larger Beechwood structure, it may hide topology mistakes rather than solve them.

## Generalization Beyond Grove

This does generalize Cycle U beyond Grove in the narrow sense: the same low-camera, tight-crop, door-readable, thatch-forward prompt stack produced a plausible Beechwood rural plate without major semantic leaks. It does not yet prove generalization for topology-sensitive production use, because Beechwood exposed a footprint-preservation failure that Grove could mask more easily. Best next step is to keep this style/camera/scale direction but strengthen the structure control before batch use.
