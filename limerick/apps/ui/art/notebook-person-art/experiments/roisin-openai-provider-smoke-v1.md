# Roisin OpenAI Provider Smoke V1

Date: 2026-07-10

This is the first live API proof for issue #1628's metadata-to-candidate stage.
It is generation evidence, not human art approval.

## Source And Provider

- NPC: Roisin Connolly (`npc_id: 4`)
- Asset: portrait candidate 1
- Input: `npc-art-inputs-v1.json`
- Visual reference: `docs/graphics-v2/illustrated-parish-notebook.png`
- Adapter: `openai-images-edits-v1`
- Model: `gpt-image-2-2026-04-21`
- Size/quality: `1024x1024`, high, PNG
- Provider request ID: `req_5696df17ac744818b2851af92b28299b`
- Usage: 2,580 input tokens, 7,024 output image tokens, 9,604 total

## Artifact Evidence

- Raw SHA-256: `2af2995eb3dcf2b57d650c4d9ff1ba20bc0766ee408b6ee975783eef30e03fa6`
- Candidate review status: `pending`
- Promotion eligible: `false`
- Raw key coverage: `0.591758`
- Raw subject coverage: `0.408242`
- Candidate transparent coverage: `0.592438`
- Candidate visible coverage: `0.406912`
- All four candidate border edges have no pixel above alpha 5.

The first validator used its hard key distance for exact corner acceptance and
rejected one corner whose magenta distance was `41.96` against a threshold of
`40`. The paid raw output was preserved on the second live attempt. The corner
rule now uses the configured feather distance, and the same raw response was
reprocessed locally into a transparent candidate without another API request.

The content-addressed raw image, candidate, failure receipt, and successful
pending receipt live under the ignored local `candidates/` authoring store. They
do not enter the runtime pack until the separate human approval deliverable.

A self-contained review packet was prepared at
`candidates/review-packets/roisin-provider-v1/review.html`. It embeds four valid
PNG previews and an eleven-item portrait checklist. Its decision and reviewer
fields remain null, and `notebook:art-review status` reports `pending`.
