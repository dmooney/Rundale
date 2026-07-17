# Rejected Production Candidate Revision v4

Date: 2026-07-12

This note records a rejected candidate boundary for issue #1628. It must not be
used as an approval source or promoted into the runtime pack. Candidate files
and review packets remain ignored authoring artifacts.

## Rejection

Full-cast review after this note was first written found identity collapse among
the younger and middle-aged women. Roisin's full-face calibration references
had become a stronger identity prior than the old free-text appearance data, so
multiple unrelated women inherited the same oval face, eye placement, bun or
headscarf construction, and shawl/apron silhouette. Pair-local portrait-to-marker
checks did not expose that cast-level failure.

The v4 review packets remain pending historical artifacts and must never be
approved or promoted. Revision v5 replaces the appearance contract with
structured facial geometry, rejects near-duplicate cohort profiles, removes all
full-face references from provider requests, and adds a mandatory complete-cast
distinctiveness check.

## Canonical Inputs

- Pipeline revision: `notebook-person-pairs-v4`
- Provider: OpenAI Images edits adapter
- Model snapshot: `gpt-image-2-2026-04-21`
- Request shape: one `2048x1024` portrait-and-marker pair per subject
- Generation config SHA-256: `dc3bdff4c2d2d05b9c5509fdc11f915f7b7a2e32e5a891355a445f860dddd5fc`
- NPC art-input dataset SHA-256: `0b2852a0161ff5339cc67329e4e0001465e4c5666fe650e84905f041d6b8867f`
- Named roster: numeric NPC IDs 1 through 23
- Fallback subject: `Unknown parish neighbour`

The committed art-input dataset is reproducible from the root-relative command
in `../README.md`. No credential is stored in the config, inputs, candidate
receipts, review packets, or approved release.

## Provider and Migration Lineage

The named batch used 22 bounded provider calls and resumed the previously paid,
user-approved Roisin pair. Fourteen new responses passed the initial contracts.
Eight complete but oversized responses were normalized from their preserved raws
under later deterministic revisions; cropped edge contact remained a hard
failure. The v4 canonical-path revision then reprocessed all 23 successful named
receipts locally in run `named-cast-v4-canonical-20260712`, with zero provider
calls.

Two early fallback attempts were rejected because their identity overlapped the
Roisin reference. The fallback art direction was changed to an anonymous
middle-aged man in a low felt cap and gray-indigo long coat. One bounded provider
call produced job
`54b9d41b31fce40cf21d104f56db5b7fb3d87f869366d5133c26ed51a78c6988`.
Independent visual QA found the pair complete, correctly split between sparse
uncolored portrait ink and restrained marker watercolor, and distinct from every
named NPC. The canonical v4 fallback receipt
`7c4c15cf66189a449bae85961fcf3ae0a4df83fd6f33cf3f6550828a9d55eda2`
was reprocessed locally from that paid raw with zero provider calls.

## Final Plan Evidence

Running the generator in plan mode with fallback enabled reports:

```text
pipeline revision: notebook-person-pairs-v4
unsharded jobs: 24
selected jobs: 24
resumable existing: 24
pending provider requests: 0
```

The 24-pair review grid passed objective framing, transparency, period,
surface-split, tiny-size readability, and pair-local identity checks. That was
insufficient: the batch failed cast-level facial separability and is rejected in
full. No exact v4 receipt is eligible for production approval.
