# Production Candidate Revision v5

Date: 2026-07-12

**Status: rejected on 2026-07-14. Do not approve or promote this cohort.**

This note records the exact issue #1628 candidate set displayed for human
approval. It is not an approval record. Promotion remains forbidden until every
hash-bound review decision is explicitly approved.

## Canonical Inputs

- Pipeline revision: `notebook-person-pairs-v5`
- Provider: OpenAI Images edits adapter
- Model snapshot: `gpt-image-2-2026-04-21`
- Request shape: one `2048x1024` portrait-and-marker pair per subject
- Sole provider reference: `docs/graphics-v2/illustrated-parish-notebook.png`
- Generation config SHA-256: `467aa86d7823ea878aeacb584592a293bbed8b32fdba560d59ff181c2ac100e6`
- NPC art-input dataset SHA-256: `bc9dae77b2c067133b5f69808e107ad52fb7271b530a02431fd01ff455c3c869`
- Named roster: numeric NPC IDs 1 through 23
- Fallback subject: `Unknown parish neighbour`

Schema v2 provides a stable identity seed, apparent age, nine explicit facial
geometry dimensions, distinguishing features, hair, wardrobe, pose, and marker
cues for every subject. The exporter rejects duplicate fingerprints and
same-cohort profiles that differ in fewer than four geometry or hair dimensions.

## Provider and Migration Lineage

The bounded `identity-v5-full-cast-20260712` run made 24 provider requests. The
runner persisted every full paid response before validation. Fifteen passed the
then-current contracts; nine visually valid responses were retained as failures
at scale, portrait-fill, or key-spill boundaries.

No rerender was needed. Validation was corrected to normalize complete
low-margin figures, neutralize resized key spill, and compute subject bounds
outside the configured key-feather radius so near-key border drift cannot mimic
a cropped figure. True subject pixels touching a cell boundary remain a hard
failure. Bumping the deterministic postprocess to v7 invalidated old receipts.
Run `identity-v5-validation-v7-20260712` then migrated all 24 saved raws into
current content-addressed receipts with zero provider calls and zero failures.

The final plan reports:

```text
pipeline revision: notebook-person-pairs-v5
unsharded jobs: 24
selected jobs: 24
resumable existing: 24
pending provider requests: 0
```

## Visual Gate

Three independent complete-set reviews passed the candidates for:

- cast-level facial distinctiveness, including a focused comparison of all ten
  women and nearest-neighbor review;
- portrait-to-marker identity, apparent age, role silhouette, and readable
  props;
- sparse uncolored notebook portrait ink, painted-world marker palette,
  framing, period fit, transparent surfaces, and visible artifacts.

No blocking duplicate was found. The nearest women were Siobhan Murphy and Nora
Duffy, separated by jaw weight, brow, forehead, chin, age structure, and hairline.
The focused reviewer also confirmed distinct geometry for Roisin/Maire,
Aoife/Niamh, and Brigid/Una.

These are preflight checks only. The exact 23 named pairs and fallback remain
pending until the user approves the displayed set and immutable review records
bind that decision to every receipt and child-artifact hash.

## Rejection Finding

The user correctly identified a cast-level hairstyle collision missed by all
three preflight reviews: nearly every woman had the same centre-parted low
bun/coil silhouette. The faces were materially distinct, but the source sidecar
itself requested centre or near-centre parts for seven of ten women and low or
covered-low rear arrangements for nearly all ten. The provider amplified that
repetition rather than inventing it.

The v5 women are rejected as a production cohort. This exposed a validator flaw:
one free-text `hair` sentence counted as a single differing identity value, so
minor wording changes such as "low knot" versus "compact low coil" concealed a
shared topology. Schema v3 now validates machine-comparable front, rear,
covering, and silhouette families independently of the nine facial dimensions.
The unchanged men/boys/fallback remain eligible as source candidates, but no v5
pair is approved and the v5 whole-cast review must never be used for promotion.
