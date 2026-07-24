# Issue 1628 Deliverables

Source issue: `https://github.com/dmooney/Rundale/issues/1628`

This file splits the issue into concrete production deliverables so each step can
be reviewed independently.

| #   | Deliverable                                                                                             | Status | Notes                                                                                                                                                                                                                                                                                         |
| --- | ------------------------------------------------------------------------------------------------------- | ------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Complete current NPC art-input dataset from canonical NPC/world metadata plus reviewed visual metadata. | Done   | Internal schema v4 covers all 23 NPCs plus fallback with stable facial/hair identity and character-only marker data: silhouette, stance, an enumerated empty-hand pose, and typed intrinsic readability cues. Facial, hairstyle, and marker-shape concerns are separate.                      |
| 2   | GitHub-friendly reusable image-generation prompt file for notebook person portraits and markers.        | Done   | `.github/prompts/rundale-notebook-person-art.prompt.md` consumes the structured identity record, uses only the authoritative full concept, emits one `2048x1024` pair prompt, preserves exact hair topology, and forbids props, other people, and scenery in the marker cell.                 |
| 3   | Automated provider-generation stage from NPC art inputs to candidate portrait/marker images.            | Done   | The bounded runner calls the pinned provider directly, persists every paid response before validation, supports content-addressed resume/retry, caps actual HTTP attempts, and stops bounded queues on account-wide failures. The final runs produced or resumed all 24 character-only pairs. |
| 4   | Provider/model configuration and reproducible generation settings.                                      | Done   | `generation-config-v1.json` pins `gpt-image-2-2026-04-21`, the sole authoritative concept reference, paired request, rate/retry policy, keying, content checks, complete-low-margin normalization, and provenance revisions.                                                                  |
| 5   | Candidate artifact storage with receipts/provenance.                                                    | Done   | Content-addressed storage retains every paid raw before validation, provider request ID/usage, split raw, transparent candidate, validation/migration lineage, and immutable failure attempts. Superseded v4-v7 artifacts remain history, never production art.                               |
| 6   | Human review/approval gate.                                                                             | Done   | All 24 pair decisions bind exact receipt/raw/child hashes, hair topology, and marker identity. Reviewer `dmooney` approved the displayed complete cast; whole-cast review `8def65018ffc8133600bf154` separately binds face and hairstyle distinctiveness.                                     |
| 7   | Initial approved named-NPC portrait set.                                                                | Done   | The immutable `approved/v1` release contains all 23 named sparse, uncolored notebook portraits with hash-bound pair and whole-cast approvals.                                                                                                                                                 |
| 8   | Initial approved named-NPC marker/sprite set.                                                           | Done   | The release contains all 23 named restrained-watercolor character markers with empty hands and no props, scenery, ground planes, or shadows.                                                                                                                                                  |
| 9   | Production-quality unknown NPC fallback portrait and marker.                                            | Done   | The approved unknown-neighbour pair follows the same production contracts, remains distinct from every named NPC, and is the deterministic runtime fallback.                                                                                                                                  |
| 10  | Runtime integration from NPC identity to approved portrait/marker assets.                               | Done   | Runtime lookup is numeric-ID-first with ambiguity rejection and deterministic fallback. The browser sentinel proved NPC 19 resolves by ID even when its compatibility name is deliberately stale.                                                                                             |
| 11  | Runtime manifest/provenance coverage.                                                                   | Done   | Release `41ddb06811e2bcda004421314e01560423b0986f990477c65592ac2b19576049` and the regenerated runtime manifest cover NPC IDs 1-23 plus fallback, 48 unique assets, release freshness, and the complete source/approval hash chain.                                                           |
| 12  | Desktop and mobile proof screenshots from the real illustrated notebook UI.                             | Done   | The final Playwright proof captured `.proofs/issue-1628-person-art/desktop.png` and `mobile.png` from a fresh Parish server and verified painted, unclipped runtime regions.                                                                                                                  |
| 13  | Visible contact sheet or equivalent final review artifact plus proof-gate evidence.                     | Done   | The runtime HTML/PNG contact sheet contains 24 labeled pairs. Browser proof decoded all 48 images, and the final proof bundle maps the release, tests, and screenshots to the acceptance criteria.                                                                                            |

## Completion Evidence

The definitive character-only run produced all 23 named NPC pairs plus the
unknown-neighbour fallback. Two provider failures were retained immutably and
retried without overwriting paid attempts. Automated marker, portrait/pair, and
whole-cast audits passed, and the user approved the displayed final 24-pair
sheet. Promotion then re-exported canonical NPC inputs and refused any drift
before writing immutable release `approved/v1`.

The release-only builder generated the complete runtime pack, manifest,
provenance, and contact sheets. The production frontend freshness gate passed.
The four-part browser proof passed numeric-ID selection, desktop rendering,
mobile rendering, and the 24-entry/48-image contact sheet against a fresh Parish
server. Exact commands, release IDs, screenshot artifacts, and acceptance
mapping are recorded in `.proofs/issue-1628-person-art/evidence.md`.
