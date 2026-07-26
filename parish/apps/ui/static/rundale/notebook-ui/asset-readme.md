# Rundale Approved Portrait and Marker Assets

This directory preserves the approved portrait/marker release and provenance.
The chat-first shell uses portraits as responsive DOM images. Markers and other
notebook-era assets are archival unless a current component explicitly imports
them; no Pixi renderer consumes this directory.

Approved portraits and complete in-world markers are built from the deterministic
approved-release manifest at `parish/apps/ui/art/notebook-person-art/approved/v1/release-manifest.json`. The builder verifies the release,
master, source candidate, raw source, generation, and approval hashes plus PNG
dimensions and content. Portraits and markers are contain-scaled on transparent
canvases, so complete figures are never cropped.

`asset-manifest.json` records NPC IDs and per-asset provenance.
`person-art-provenance.md` records the approved release and master hashes.
`person-art-contact-sheet.png` and `person-art-contact-sheet.html` show all
named pairs and the approved fallback in a dynamic four-column grid.
