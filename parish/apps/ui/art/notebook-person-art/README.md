# Notebook Person Art Pipeline

This folder contains the reviewed source inputs for the illustrated-notebook
person art slice (#1628).

## Metadata Export

The upstream art-input dataset is generated from canonical NPC/world data plus
the reviewed art-direction supplement:

```sh
cargo run --manifest-path parish/Cargo.toml -p parish-npc-tool -- art-inputs \
  --npcs mods/rundale/npcs.json \
  --world mods/rundale/world.json \
  --art-direction parish/apps/ui/art/notebook-person-art/npc-art-direction-v1.json \
  --output parish/apps/ui/art/notebook-person-art/npc-art-inputs-v1.json
```

The export covers all 23 current Rundale NPCs. The audit is
`npc-art-data-audit-v1.md`; the authoring rules are
`docs/graphics-v2/npc-portraits/art-metadata-guidelines.md`.

## Provider Candidate Generation

`generation-config-v1.json` pins the provider adapter, model snapshot, image
settings, paired-cell layout, asset-specific visual references, keyed-output
rules, validation thresholds, rate limit, retry policy, storage layout, and
initial review status. The current provider is OpenAI's
`gpt-image-2-2026-04-21` through `/v1/images/edits`. Each request attaches the
issue-approved portrait and marker style derivatives and generates both assets
from one shared identity prompt.

Plan the full current batch without credentials or provider calls:

```sh
npm --prefix parish/apps/ui run notebook:art-candidates
```

The current dataset plans 24 requests: one identity-locked portrait/marker pair
for each of 23 named NPCs plus one unknown-neighbour fallback pair. Narrow plans
are useful for prompt and pipeline checks:

```sh
npm --prefix parish/apps/ui run notebook:art-candidates -- \
  --npc-id 1 --asset pair --exclude-fallback
```

Live generation is opt-in and requires both `OPENAI_API_KEY` and an explicit
request ceiling. The ceiling applies after resume checks, so completed
content-addressed jobs do not consume the allowance:

```sh
npm --prefix parish/apps/ui run notebook:art-candidates -- \
  --env-file /path/to/Rundale/.env \
  --npc-id 1 --asset pair --exclude-fallback \
  --execute --max-requests 1
```

Git worktrees do not copy ignored `.env` files. In that case, pass
`--env-file` with the primary worktree's file; the runner loads it in process
without copying the key into candidate artifacts or receipts.

The runner sends one exact metadata-derived `pair_prompt`, the fixed paired
rendering contract, both full child contracts, and both role-scoped style
references. One `2048x1024` response is persisted before validation, then split
deterministically into a left portrait and right marker. Each child is validated
against its own visual contract. Portrait checks cover inked drawing height,
total subject fill, dark-ink coverage, ink density, light fill, chromatic edge
coverage, and solid colored fill; marker checks cover full-body scale, width,
and safe margins. The runner removes the key and stores both transparent child
candidates. It retries only transient provider failures, honors a global request
rate, records request ID and usage, and never records credentials.

Because antialiased dark strokes inherit magenta from the opaque provider key,
portrait postprocessing also normalizes every retained stroke to the configured
graphite ink color while preserving the generated alpha and geometry. This
postprocess revision is part of the content-addressed job identity. Marker
postprocessing preserves the watercolor palette while neutralizing
palette-forbidden magenta-balanced edge spill at any alpha; a residual-spill
gate rejects contaminated transparent candidates.

The paired request uses the issue-approved sparse Roisin sketch as a left-cell
style-transfer anchor and the issue-approved marker derivative as a right-cell
style-transfer anchor. Both are style references only; NPC identity comes from
metadata. Their role separation prevents the painted-world marker treatment
from overriding the UI portrait's sparse pen-and-ink language. Reference
provenance and deterministic derivatives are documented in
`references/README.md`.

One provider call materially improves face consistency, but stochastic image
generation is not a mathematical identity guarantee. The production guarantee
is procedural: one shared request, explicit cross-cell identity invariants,
fixed splitting, per-child validation, atomic human review, and mandatory joint
rerendering when either child or their identity match fails.

OpenAI documents that `gpt-image-2` does not currently return transparent
backgrounds. The provider raw output therefore uses flat `#ff00ff` for both
asset types; local deterministic key removal produces the transparent portrait
and marker candidates. The final portrait remains uncolored pen-and-ink and has
no baked parchment. See the official
[image-generation guide](https://developers.openai.com/api/docs/guides/image-generation)
and [model page](https://developers.openai.com/api/docs/models/gpt-image-2).

### Candidate Storage

Candidates are local authoring artifacts and are not release inputs:

```text
candidates/
  objects/<hash-prefix>/<job-sha256>/
    prompt.txt
    input-record.json
    receipt.json
    attempts/<attempt-id>/
      raw.png
      portrait-raw.png
      portrait-candidate.png
      marker-raw.png
      marker-candidate.png
      failure.json (failed attempts only)
  runs/<run-id>/
    run.json
    manifest.jsonl
```

The job hash covers the NPC/fallback record, shared pair prompt, provider/model
settings, fixed cell layout, postprocess revision, candidate index, and every
reference-image hash. A pair receipt binds the paid full sheet, both split raws,
both transparent candidates, and their hashes to one provider request. A valid
matching receipt makes reruns resumable. `--shard-count N --shard-index I`
partitions those stable job IDs for parallel workers without changing them.
Every generated receipt starts as `candidate` / `pending`, sets promotion
eligibility to false, and cannot be consumed by the approved runtime builder.

Rejected raw responses are immutable and can be reprocessed after a validator
improvement without making another provider request:

```sh
npm --prefix parish/apps/ui run notebook:art-candidates -- \
  --reprocess-failure path/to/failure.json
```

At millions of NPCs, the same job identity and receipt contracts should move to
an object store plus queue/database index instead of one local JSON input and
filesystem tree. The current command already supplies deterministic sharding,
bounded concurrency, request caps, and idempotent resume semantics needed at
that boundary; it does not pretend a monolithic JSON file is an eight-million
record transport.

## Human Review Gate

Generation never sets approval. Prepare a self-contained review packet from one
or more pending candidate receipts:

```sh
npm --prefix parish/apps/ui run notebook:art-review -- prepare \
  --receipt path/to/receipt.json \
  --output path/to/review-packet \
  --packet-id review-batch-name
```

For a pair receipt, the packet embeds the full raw sheet, both split raws, both
transparent candidates, selected and tiny runtime previews, provider identity,
request ID, and every bound hash. Its one atomic checklist combines the
portrait and marker checks with cross-asset identity, correct UI/world surface
separation, and acknowledgement that a failed child requires a joint rerender.
The reviewer fills every checklist value with `true` or `false`, sets
`decision` to `approved` or `rejected`, identifies themselves, and records notes
for a rejection. One decision applies to both children; they cannot be approved
independently.

Submit and query the decision:

```sh
npm --prefix parish/apps/ui run notebook:art-review -- decide \
  --template path/to/completed-review.json

npm --prefix parish/apps/ui run notebook:art-review -- status \
  --receipt path/to/receipt.json
```

The decision command re-hashes the receipt, full sheet, and all four child
artifacts, rejects changed or incomplete packets, requires every check to pass
for approval, and writes an immutable review record plus a hash-linked lookup
pointer. A second decision is refused. Approval only makes both children
eligible for the later promotion stage; it does not alter
`approved-cast-v1.json` or runtime assets.

## Approved Asset Build

```sh
pnpm --dir parish/apps/ui run notebook:people
```

This downstream assembly command reads `approved-cast-v1.json`, validates that
every source sheet and person entry is explicitly `approved`, crops the reviewed
source sheets, chroma-keys the marker sprites, writes stable runtime PNGs under
`static/rundale/notebook-ui/people/`, updates `asset-manifest.json`, and writes
`static/rundale/notebook-ui/person-art-contact-sheet.png`.

It is not the provider-generation stage. The provider-generation stage must
consume `npc-art-inputs-v1.json`, call the configured image provider/model,
store candidates and receipts, and promote only reviewed/approved assets.

## Review Gate

Generated candidates are not treated as approved by default. A source sheet,
fallback, or person entry with any `approval_status` other than `approved`
causes the pipeline to fail. The config stores the source prompt, source sheet,
runtime asset paths, cell coordinates, and per-entry review notes.

The source visual authority for this issue is
`docs/graphics-v2/illustrated-parish-notebook.png`. The accepted Roisin chat
portrait is an issue-produced, user-approved calibration derivative used only
to isolate that concept's sparse portrait line language. Existing unrelated
portrait experiments, marker concept sheets, old procedural busts, and
placeholder markers are not source artwork for this approved set.

## Approved Initial Set

The first approved set covers the live starting Kilteevan cast plus the
early/common notebook people used by the selected-person UI:

- Brigid Ni Fhatharta
- Sean Ruadh Kelly
- Peig Hannigan
- Roisin Connolly
- Aoife Brennan
- Mick Flanagan
- Niamh Darcy
- Unknown parish neighbour fallback
