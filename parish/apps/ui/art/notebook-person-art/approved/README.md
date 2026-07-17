# Approved Notebook Person Art

This directory defines the release boundary between reviewed local candidates
and immutable source masters that downstream builders may consume.

Production release `v1` is checked in here. It contains all 23 numeric Rundale
NPCs plus the unknown-neighbour fallback and is identified by release ID
`41ddb06811e2bcda004421314e01560423b0986f990477c65592ac2b19576049`.
Reviewer `dmooney` approved the exact 24-pair whole-cast binding after visual
review; the release retains every pair decision, the whole-cast decision, and
the complete provider-to-master provenance chain.

Review packets, candidate receipts, or calibration approvals outside this
directory still do not authorize a partial or replacement release. A future
release must pass the same promotion contract and use a new immutable version
directory.

Run the promoter with the reviewed packet manifests and the immutable whole-cast
decision. It resolves each candidate's immutable review pointer, while the cast
decision proves those exact 23 named pairs and fallback were compared together:

```sh
npm --prefix parish/apps/ui run notebook:art-promote -- \
  --packet path/to/review-packet/manifest.json \
  --packet path/to/another-review-packet/manifest.json \
  --cast-review path/to/whole-cast-review-decision.json
```

Explicit repeated `--receipt` and `--decision` pairs remain available for
one-off recovery and fixture work.

Production mode is the default. It requires exactly the 23 numeric NPC records
in the bound input dataset plus one fallback. `--fixture` relaxes only that
release-wide completeness check for synthetic, unpaid tests; it does not relax
receipt, decision, pointer, provenance, or artifact hash validation.

A successful production run creates a new approved version directory atomically.
It refuses an existing output directory. Release `v1/` includes:

```text
v1/
  release-manifest.json
  generation-config.json
  npc-art-inputs.json
  whole-cast-review.json
  references/<sha256>.png
  people/<npc-id-or-fallback>/
    portrait.png
    marker.png
    provider-raw.png
    portrait-raw.png
    marker-raw.png
    prompt.txt
    input-record.json
    candidate-receipt.json
    review-decision.json
```

Every copied file is re-hashed after copying. The manifest is deterministic and
records provider/model/request metadata, configuration and dataset hashes,
reference provenance, prompt and input hashes, provider and split raw hashes,
promoted art hashes, reprocessing provenance, exact pair decisions, and the
immutable whole-cast approval record. Its contract is documented by
`release-manifest.schema.json`.

Downstream runtime builds read only this release bundle. Candidate-store paths
remain recorded as lineage, but a clean checkout does not need those ignored
local files to regenerate the runtime assets, manifest, provenance, or contact
sheet.
