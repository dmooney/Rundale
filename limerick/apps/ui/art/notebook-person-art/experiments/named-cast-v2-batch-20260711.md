# Named Cast Provider Batch: 2026-07-11

Issue: #1628  
Provider: OpenAI Images edits API  
Model: `gpt-image-2-2026-04-21`  
Asset mode: one identity-locked `2048x1024` portrait-and-marker pair per request

## Paid Run

The named-cast plan contained 23 jobs. The approved-calibration Roisin provider
request resumed from its content-addressed receipt, leaving an explicit ceiling
of 22 new requests. The live run completed all 22 calls:

- 14 pairs passed the original validators.
- 8 pairs were preserved as failures.
- Every failure was a framing ceiling, not provider, moderation, identity-prompt,
  color-contract, key-removal, or missing-content failure.
- The eight failures were Aoife Brennan, Colm Gallagher, Brendan Duffy, Nora
  Duffy, Maire Gallagher, Brigid Ni Fhatharta, Sean Ruadh Kelly, and Siobhan
  Murphy.

Source run: `candidates/runs/named-cast-20260711/`.

## Root Cause And Recovery

The provider treats requested subject scale as a soft composition cue. The
pipeline correctly enforced fixed runtime ceilings but initially had no
deterministic correction for a complete, uncropped figure. Five of the eight
marker/portrait overages were small; Nora Duffy's marker was the largest at
78.3 percent against a 65 percent ceiling.

`notebook-person-pairs-v2` now keys the figure first, downsizes only complete
oversized subjects with premultiplied bilinear sampling, recenters them, and
records before/after bounds and scale in the receipt. Genuine edge contact still
fails. Nora also exposed one near-key pixel at the bottom-right canvas corner;
the crop guard now requires two same-axis subject pixels, ignoring one-pixel key
noise while preserving the multi-pixel crop test.

The final whole-manifest migration reused every paid raw and made zero provider
requests:

- Run: `candidates/runs/named-cast-v2-final-20260711/`
- Source entries: 23
- Reprocessed: 23
- Failed: 0
- Provider requests: 0
- Config SHA-256: `a332b478c501e0f040096fcfa766e023f86f12c87fe4c69f190a37aa1d67c1f6`

A fresh plan under that config reports 23 resumable jobs and zero pending jobs.

## Review Boundary

The final receipts are split into three self-contained packets under
`candidates/review-packets/named-cast-v2-batch-1-20260711/` through
`named-cast-v2-batch-3-20260711/`. They show transparent candidates, preserved
paired raws, selected/tiny previews, hashes, provider request IDs, and the full
atomic checklist.

All final-revision decisions remain pending. No pair has been promoted into the
runtime asset pack, and this batch does not complete deliverables 7-13 by
itself.
