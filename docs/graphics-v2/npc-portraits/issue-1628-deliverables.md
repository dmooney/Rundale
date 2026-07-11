# Issue 1628 Deliverables

Source issue: `https://github.com/dmooney/Rundale/issues/1628`

This file splits the issue into concrete production deliverables so each step can
be reviewed independently.

| #   | Deliverable                                                                                             | Status  | Notes                                                                                                                                                                                                                                                            |
| --- | ------------------------------------------------------------------------------------------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Complete current NPC art-input dataset from canonical NPC/world metadata plus reviewed visual metadata. | Done    | `npc-art-direction-v1.json`, `npc-art-inputs-v1.json`, and `npc-art-data-audit-v1.md` cover all 23 current NPCs.                                                                                                                                                 |
| 2   | GitHub-friendly reusable image-generation prompt file for notebook person portraits and markers.        | Done    | `.github/prompts/rundale-notebook-person-art.prompt.md` consumes one NPC art-input record and emits one identity-locked `2048x1024` pair prompt plus an atomic review checklist.                                                                                 |
| 3   | Automated provider-generation stage from NPC art inputs to candidate portrait/marker images.            | Done    | The live 23-NPC batch used 22 new `gpt-image-2-2026-04-21` calls plus the preserved Roisin call. Fourteen passed immediately; eight complete framing outliers were normalized from preserved raws. Final audit: 23 resumable, 0 pending.                         |
| 4   | Provider/model configuration and reproducible generation settings.                                      | Done    | `generation-config-v1.json` pins the OpenAI adapter/model snapshot, paired request, references, rate/retry policy, validation, and deterministic premultiplied-alpha scale normalization with provenance-bearing revision and bounds.                            |
| 5   | Candidate artifact storage with receipts/provenance.                                                    | Done    | All 23 final named pairs have content-addressed receipts covering sheets, children, prompts, inputs, model/settings, request IDs, usage, references, hashes, validation, normalization, and source reprocessing lineage.                                         |
| 6   | Human review/approval gate.                                                                             | Done    | `notebook:art-review` enforces one hash-bound atomic decision per pair. Three self-contained review batches cover the final 23-candidate revision. Generation and postprocess migration leave every final pair pending; prior approval never transfers silently. |
| 7   | Initial approved named-NPC portrait set.                                                                | Pending | All 23 identity-specific final-revision portrait candidates exist and are review-ready; explicit human approval is still required.                                                                                                                               |
| 8   | Initial approved named-NPC marker/sprite set.                                                           | Pending | All 23 identity-locked final-revision marker candidates exist and are review-ready; explicit human approval is still required.                                                                                                                                   |
| 9   | Production-quality unknown NPC fallback portrait and marker.                                            | Pending | Must be documented as fallback-only and visually distinct from named NPCs.                                                                                                                                                                                       |
| 10  | Runtime integration from NPC identity to approved portrait/marker assets.                               | Pending | Lookup must resolve approved assets and fall back predictably.                                                                                                                                                                                                   |
| 11  | Runtime manifest/provenance coverage.                                                                   | Pending | Manifest/provenance must cover every portrait, marker, fallback, source prompt/config, and approval status.                                                                                                                                                      |
| 12  | Desktop and mobile proof screenshots from the real illustrated notebook UI.                             | Pending | Screenshots must show final assets in the running UI.                                                                                                                                                                                                            |
| 13  | Visible contact sheet or equivalent final review artifact plus proof-gate evidence.                     | Pending | Contact sheet must load actual approved images; agent-check proof must remain insufficient until all art, pipeline, provenance, tests, and screenshots exist.                                                                                                    |

## Current Boundary

Deliverables 1-6 now cover metadata, reusable paired prompting, the complete
live named-cast provider batch, pinned deterministic postprocessing, immutable
full-sheet and child provenance, and the atomic human review gate. All 23 named
pairs are validated and review-ready under one final config hash. They remain
pending because approval cannot transfer from an earlier postprocess receipt.

Deliverables 7-8 now require review decisions rather than more generation.
Deliverable 9 still requires the fallback pair. Runtime promotion, manifest
coverage, screenshots, and final proof remain separate work in deliverables
10-13.
