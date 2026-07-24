# Roisin Identity-Locked API Pair v2

Status: approved by atomic human review; promotion eligible, not yet promoted.

## Purpose

Prove the production path from Roisin Connolly's canonical NPC/world metadata
and reviewed visual supplement to one provider request that returns both her
player-notebook portrait and painted-world marker with a shared face identity.

## Provider Request

- Model: `gpt-image-2-2026-04-21`
- Endpoint: `/v1/images/edits`
- Size: `2048x1024`
- Quality: `high`
- Provider request ID: `req_0e2f6349b3e94fd489d84ea1e11f342d`
- Full raw SHA-256:
  `cb603412c8440e8a963857e4d178ba01d8fa6ef00e554bbd4afc51e744aa0d38`
- Receipt:
  `candidates/objects/82/8293eab62e1e5f582d2548fc66ef5c576a2af38b6cbd1b7f8397a6a6ea28eab8/receipt.json`

The request used one metadata-derived `pair_prompt`, the fixed paired layout,
both full child contracts, the accepted sparse portrait style derivative, and
the accepted painted-world marker style derivative. The paid response was
persisted before validation and split at the fixed 1024-pixel boundary.

## Validation Result

Portrait:

- raw/candidate SHA-256:
  `9a325fa2e281cee37caa8413cf9df4ed298206f900c81964e9d0080c2c4b81b1` /
  `6ec7c25440573e53585a768863e0fe93d35f1316aaa74a8eb3817b16e29e66d0`
- subject coverage: `6.96%`
- inked drawing height: `63.48%`
- dark ink coverage: `0.90%`
- solid colored fill: `0.10%`
- transparent candidate area: `93.35%`
- retained strokes normalized to `#36362e`

Marker:

- raw/candidate SHA-256:
  `cdc7a27deff1ed627ed62c5189094d250f1c1d275ecf5e2ec180b2a2d1823e1e` /
  `552c0ce531d1b88e39dd38ff823289ba65765463043d3f36605ca8b5b687cde6`
- subject coverage: `7.73%`
- subject bounds: `20.41%` width by `55.37%` height
- minimum edge margin: `22.27%`
- transparent candidate area: `92.28%`
- residual magenta spill among visible pixels: `0%`

## Validator Calibration

The first validation pass rejected the portrait because `2.9%` of pixels were
classified as chromatic subject pixels against a `2.0%` ceiling. Visual and
pixel inspection showed those pixels were thin sepia/key antialiasing around
otherwise open linework, not painted fill. The validator now separately
measures solid chromatic interiors using a one-pixel erosion. The production
threshold permits up to `4.0%` thin chromatic edges but only `0.3%` solid
colored fill. A regression fixture proves that thin chromatic ink passes while
a solid colored patch fails. The preserved provider response was then
reprocessed locally; no replacement provider request was made.

The first transparent marker derivative exposed a thin magenta edge in the
checker preview. Root cause analysis found that fully opaque dark-magenta blend
pixels fell outside the alpha feather and therefore bypassed the old
partial-alpha-only despill. Postprocess revision `notebook-person-key-v4`
neutralizes magenta-balanced retained pixels regardless of alpha and rejects a
candidate if more than `0.2%` of its visible pixels still match the key-spill
signature. The corrected marker reports `0%` residual spill and reuses the same
provider response.

## Human Gate

The self-contained review packet is
`review-packets/roisin-identity-pair-v2-despilled/review.html`. One decision
covers the portrait, marker, cross-asset identity, UI/world surface split, and
the policy that any failed child requires a new joint render. The user approved
the pair on 2026-07-10. The immutable review record is
`candidates/objects/82/8293eab62e1e5f582d2548fc66ef5c576a2af38b6cbd1b7f8397a6a6ea28eab8/reviews/a055f9af871a26ff71efc05e.json`,
bound to pair digest
`d713ee7b003f97c2a3df313f94930fd00ffc59df745402070a094a800bf7078f`.
Approval makes both children promotion eligible but does not itself copy them
into runtime assets.

## Progression Composite

`roisin-art-progression.png` traces 13 milestones from the earlier Conversation
Lens and Illustrated Parish Notebook concepts through portrait cycles A-E,
chat calibration, API failures and corrections, and the approved production
pair. `build-roisin-art-progression.sh` reproduces the composite from the exact
source artifacts without repainting them.
