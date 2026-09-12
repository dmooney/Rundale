# Hair Topology Candidate Revision v6

Date: 2026-07-14

**Status: rejected. Do not approve or promote this cohort.**

Revision v6 was a bounded ten-woman provider experiment after the user rejected
v5's repeated centre-parted low-bun silhouette. It tested the new schema-v3
front/rear/covering/silhouette metadata while preserving the 14 unaffected
men, boys, and fallback candidates for incremental reuse.

## Run Record

- Run: `identity-v6-hair-topology-20260714`
- Provider: OpenAI Images edits adapter
- Model snapshot: `gpt-image-2-2026-04-21`
- Request shape: ten atomic `2048x1024` portrait-marker pairs
- Generation config SHA-256: `467aa86d7823ea878aeacb584592a293bbed8b32fdba560d59ff181c2ac100e6`
- NPC art-input dataset SHA-256: `0b71a97cd8be0c921cb9b85d15d5072bcea34b7ad03d8f54971b0c8636fe200b`
- Result: 10 generated, 0 resumed, 0 mechanical failures

The run persisted all ten paid raw responses and candidate receipts before
visual review. It is useful provenance and prompt evidence, not approved art.

## Review Finding

The structured metadata corrected the original cast-level repetition: the ten
women no longer shared one low-bun silhouette. The exact batch still failed the
production visual contract:

- Siobhan, Roisin, Aoife, Niamh, Maire, Nora, Kathleen, and Brigid did not
  visibly preserve every declared front/rear/covering/silhouette component.
- Siobhan, Roisin, Niamh, Nora, Brigid, Una, and Peig were too densely rendered
  as engraving/editorial illustrations instead of sparse player-notebook ink.
- Una had a clear portrait-to-marker face/apparent-age mismatch; Nora and Maire
  also drifted in apparent age across the atomic pair.
- Only Una and Peig passed the strict topology comparison, and Una still failed
  cross-asset identity.

The whole v6 ten-woman batch is rejected. A successful topology distinction is
not enough when the image does not follow the individual topology or when the
portrait and marker cease to be the same person.

## Correction

Revision v7 strengthens each provider-facing hair contract with literal visible
front, rear, covering, and silhouette requirements; explicitly bans the generic
substitute shapes seen in v6; locks face and apparent age across both cells; and
leads portrait style with sparse open contour line economy, minimal shading, and
no dense engraving or black masses.
