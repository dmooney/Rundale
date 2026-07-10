# Issue 1628 Deliverables

Source issue: `https://github.com/dmooney/Rundale/issues/1628`

This file splits the issue into concrete production deliverables so each step can
be reviewed independently.

| # | Deliverable | Status | Notes |
|---|-------------|--------|-------|
| 1 | Complete current NPC art-input dataset from canonical NPC/world metadata plus reviewed visual metadata. | Done | `npc-art-direction-v1.json`, `npc-art-inputs-v1.json`, and `npc-art-data-audit-v1.md` cover all 23 current NPCs. |
| 2 | GitHub-friendly reusable image-generation prompt file for notebook person portraits and markers. | Done | `.github/prompts/rundale-notebook-person-art.prompt.md` consumes NPC art-input records and has accepted Roisin prompt evidence in `parish/apps/ui/art/notebook-person-art/experiments/`. |
| 3 | Automated provider-generation stage from NPC art inputs to candidate portrait/marker images. | Pending | Later API-backed stage; current prompt experiments use the built-in image-generation tool only. |
| 4 | Provider/model configuration and reproducible generation settings. | Pending | Must be explicit and swappable before the API pipeline is production-ready. |
| 5 | Candidate artifact storage with receipts/provenance. | Pending | Store prompt, source record, provider/model, generation time, output path, and review status. |
| 6 | Human review/approval gate. | Pending | Generated candidates must not become approved assets silently. |
| 7 | Initial approved named-NPC portrait set. | Pending | Each named NPC needs an identity-specific tiny notebook portrait. |
| 8 | Initial approved named-NPC marker/sprite set. | Pending | Each named NPC needs a readable production marker matching the same art direction. |
| 9 | Production-quality unknown NPC fallback portrait and marker. | Pending | Must be documented as fallback-only and visually distinct from named NPCs. |
| 10 | Runtime integration from NPC identity to approved portrait/marker assets. | Pending | Lookup must resolve approved assets and fall back predictably. |
| 11 | Runtime manifest/provenance coverage. | Pending | Manifest/provenance must cover every portrait, marker, fallback, source prompt/config, and approval status. |
| 12 | Desktop and mobile proof screenshots from the real illustrated notebook UI. | Pending | Screenshots must show final assets in the running UI. |
| 13 | Visible contact sheet or equivalent final review artifact plus proof-gate evidence. | Pending | Contact sheet must load actual approved images; agent-check proof must remain insufficient until all art, pipeline, provenance, tests, and screenshots exist. |

## Current Boundary

Deliverable 1 is complete as an authoring-data slice. It does not claim that
approved generated images exist yet.

Deliverable 2 is about perfecting and saving the reusable prompt contract. It is
not the API implementation; the API-backed provider pipeline starts at
deliverable 3.
